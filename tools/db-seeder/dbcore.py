"""智慧路灯测试数据工具 —— 数据库核心层(无 GUI 依赖,可单独测试)。

直连本地 PostgreSQL,绕过后端 API;通过 information_schema 自动内省表结构,
提供手动单行插入、批量生成插入与场景预设(光照曲线 / 设备上下线)。
"""

from __future__ import annotations

import math
import random
import re
from dataclasses import dataclass
from datetime import datetime, timedelta

import psycopg

#: 列在 INSERT 中使用数据库默认值(该列不进 SQL)
USE_DEFAULT = object()

_INT_TYPES = {"smallint", "integer", "bigint"}
_FLOAT_TYPES = {"real", "double precision", "numeric"}
_TS_TYPES = {"timestamp with time zone", "timestamp without time zone", "date"}


class Cancelled(Exception):
    """批量插入被用户取消(已回滚)。"""


# ---------------------------------------------------------------- 内省

@dataclass
class Column:
    name: str
    data_type: str  # information_schema 的 data_type,如 integer / text / timestamp with time zone
    nullable: bool
    default: str | None
    is_pk: bool

    @property
    def is_serial(self) -> bool:
        return bool(self.default and "nextval(" in self.default)

    @property
    def has_default(self) -> bool:
        return self.default is not None or self.is_serial

    @property
    def short_type(self) -> str:
        return {
            "timestamp with time zone": "timestamptz",
            "timestamp without time zone": "timestamp",
        }.get(self.data_type, self.data_type)


@dataclass
class Table:
    name: str
    columns: list[Column]


def quote_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def connect(host: str, port: int, dbname: str, user: str, password: str) -> psycopg.Connection:
    return psycopg.connect(
        host=host, port=port, dbname=dbname, user=user, password=password,
        connect_timeout=5,
    )


def list_tables(conn) -> list[str]:
    rows = conn.execute(
        "SELECT table_name FROM information_schema.tables "
        "WHERE table_schema='public' AND table_type='BASE TABLE' "
        "AND table_name != '_sqlx_migrations' ORDER BY table_name"
    ).fetchall()
    return [r[0] for r in rows]


def describe_table(conn, name: str) -> Table:
    rows = conn.execute(
        "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default, "
        "EXISTS ("
        "  SELECT 1 FROM information_schema.table_constraints tc "
        "  JOIN information_schema.key_column_usage kcu "
        "    ON tc.constraint_name = kcu.constraint_name "
        "   AND tc.table_schema = kcu.table_schema "
        "  WHERE tc.constraint_type = 'PRIMARY KEY' "
        "    AND tc.table_schema = c.table_schema "
        "    AND tc.table_name = c.table_name "
        "    AND kcu.column_name = c.column_name"
        ") "
        "FROM information_schema.columns c "
        "WHERE c.table_schema='public' AND c.table_name=%s "
        "ORDER BY c.ordinal_position",
        (name,),
    ).fetchall()
    return Table(name, [
        Column(r[0], r[1], r[2] == "YES", r[3], r[4]) for r in rows
    ])


def distinct_values(conn, table: str, column: str, limit: int = 50) -> list[str]:
    """text 列现有取值(给手动表单的下拉提示用)。"""
    rows = conn.execute(
        f"SELECT DISTINCT {quote_ident(column)} FROM {quote_ident(table)} "
        f"WHERE {quote_ident(column)} IS NOT NULL LIMIT %s",
        (limit,),
    ).fetchall()
    return [str(r[0]) for r in rows]


def device_ids(conn) -> list[str]:
    return [r[0] for r in conn.execute("SELECT id FROM device ORDER BY id").fetchall()]


# ---------------------------------------------------------------- 值解析

def parse_timestamp(raw: str) -> datetime:
    raw = raw.strip()
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"):
        try:
            dt = datetime.strptime(raw, fmt)
            return dt.astimezone()  # naive 按本地时区
        except ValueError:
            continue
    raise ValueError(f"时间格式无效:{raw!r}(支持 'YYYY-MM-DD HH:MM:SS' 或 'YYYY-MM-DD')")


def parse_value(col: Column, raw: str):
    """把表单字符串按列类型转成 Python 值(交给 psycopg 参数化)。"""
    t = col.data_type
    if t in _INT_TYPES:
        return int(raw)
    if t in _FLOAT_TYPES:
        return float(raw)
    if t == "boolean":
        v = raw.strip().lower()
        if v in ("true", "1", "t", "yes", "on"):
            return True
        if v in ("false", "0", "f", "no", "off"):
            return False
        raise ValueError(f"布尔值无效:{raw!r}")
    if t in _TS_TYPES:
        return parse_timestamp(raw)
    return raw


# ---------------------------------------------------------------- 单行插入

def build_insert(table: str, assignments: dict) -> tuple[str, list]:
    """assignments: {列名: 值};值为 USE_DEFAULT 的列被省略(走数据库默认值)。"""
    cols = [c for c, v in assignments.items() if v is not USE_DEFAULT]
    params = [assignments[c] for c in cols]
    if not cols:
        sql = f"INSERT INTO {quote_ident(table)} DEFAULT VALUES"
    else:
        col_sql = ", ".join(quote_ident(c) for c in cols)
        ph = ", ".join(["%s"] * len(cols))
        sql = f"INSERT INTO {quote_ident(table)} ({col_sql}) VALUES ({ph})"
    return sql, params


def insert_row(conn, table: str, assignments: dict) -> None:
    sql, params = build_insert(table, assignments)
    try:
        conn.execute(sql, params)
        conn.commit()
    except Exception:
        conn.rollback()
        raise


# ---------------------------------------------------------------- 批量插入

def insert_rows(conn, table: str, col_names: list[str], rows,
                total: int | None = None, chunk: int = 500,
                progress=None, cancel=None) -> int:
    """分块 executemany,单事务提交;cancel(threading.Event)置位时回滚并抛 Cancelled。"""
    col_sql = ", ".join(quote_ident(c) for c in col_names)
    ph = ", ".join(["%s"] * len(col_names))
    sql = f"INSERT INTO {quote_ident(table)} ({col_sql}) VALUES ({ph})"
    done, buf = 0, []
    try:
        with conn.cursor() as cur:
            for row in rows:
                if cancel is not None and cancel.is_set():
                    raise Cancelled()
                buf.append(row)
                if len(buf) >= chunk:
                    cur.executemany(sql, buf)
                    done += len(buf)
                    buf.clear()
                    if progress:
                        progress(done, total)
            if buf:
                cur.executemany(sql, buf)
                done += len(buf)
                if progress:
                    progress(done, total)
        conn.commit()
        return done
    except Exception:
        conn.rollback()
        raise


# ---------------------------------------------------------------- 值生成器(批量)

STRATEGIES = ["默认", "固定值", "随机整数", "随机选择", "时间序列", "自增", "随机设备ID", "NULL"]

STRATEGY_HINT = (
    "参数格式:固定值=原样输入 | 随机整数=小-大 | 随机选择=a,b,c | "
    "时间序列=起始时间,步长秒 | 自增=起始,步长(文本列可用模板如 seed-lamp-{i}) | "
    "随机设备ID/默认/NULL=无需参数"
)


def default_strategy(col: Column) -> tuple[str, str]:
    """按列特征给一个合理的默认批量策略。"""
    if col.is_serial:
        return ("默认", "")
    if col.name == "device_id" and col.data_type == "text":
        return ("随机设备ID", "")
    if col.data_type in _INT_TYPES:
        return ("随机整数", "0-60000" if col.name == "lux" else "0-100")
    if col.data_type in _FLOAT_TYPES:
        return ("固定值", "0")
    if col.data_type in _TS_TYPES:
        if col.name == "created_at":
            start = (datetime.now() - timedelta(days=7)).strftime("%Y-%m-%d 00:00:00")
            return ("时间序列", f"{start},300")
        if col.has_default:
            return ("默认", "")
        return ("固定值", datetime.now().strftime("%Y-%m-%d %H:%M:%S"))
    if col.data_type == "boolean":
        return ("随机选择", "true,false")
    if col.is_pk and col.data_type == "text" and not col.has_default:
        return ("自增", f"seed-{col.name}-{{i}}")
    if col.has_default:
        return ("默认", "")
    return ("固定值", "test")


def make_generator(strategy: str, param: str, col: Column, ctx: dict):
    """返回 f(row_index)->值;strategy='默认'/'NULL' 由调用方特殊处理。"""
    t = col.data_type
    if strategy == "固定值":
        if not param:
            if not col.nullable:
                raise ValueError(f"列 {col.name}: 固定值不能为空")
            return lambda i: None
        value = parse_value(col, param)
        return lambda i: value
    if strategy == "随机整数":
        m = re.fullmatch(r"\s*(-?\d+)\s*[-,~]\s*(-?\d+)\s*", param)
        if not m:
            raise ValueError(f"列 {col.name}: 随机整数参数应为 '小-大',收到 {param!r}")
        a, b = int(m.group(1)), int(m.group(2))
        if a > b:
            raise ValueError(f"列 {col.name}: 随机整数下界大于上界")
        return lambda i: random.randint(a, b)
    if strategy == "随机选择":
        opts = [p.strip() for p in param.split(",") if p.strip()]
        if not opts:
            raise ValueError(f"列 {col.name}: 随机选择需要至少一个候选值")
        parsed = [parse_value(col, o) for o in opts]
        return lambda i: random.choice(parsed)
    if strategy == "时间序列":
        if t not in _TS_TYPES:
            raise ValueError(f"列 {col.name}: 时间序列只适用于时间类型列")
        start_s, _, step_s = param.rpartition(",")
        start = parse_timestamp(start_s)
        try:
            step = float(step_s.strip())
        except ValueError:
            raise ValueError(f"列 {col.name}: 时间序列步长(秒)无效:{step_s!r}")
        return lambda i: start + timedelta(seconds=step * i)
    if strategy == "自增":
        if t in _INT_TYPES:
            start_s, _, step_s = param.partition(",")
            try:
                start, step = int(start_s.strip()), int(step_s.strip() or "1")
            except ValueError:
                raise ValueError(f"列 {col.name}: 自增参数应为 '起始,步长',收到 {param!r}")
            return lambda i: start + step * i
        if "{i}" not in param:
            raise ValueError(f"列 {col.name}: 文本列自增参数需含 {{i}} 模板,如 seed-lamp-{{i}}")
        return lambda i: param.replace("{i}", str(i))
    if strategy == "随机设备ID":
        ids = ctx.get("device_ids") or []
        if not ids:
            raise ValueError("device 表为空,无法随机取设备 ID")
        return lambda i: random.choice(ids)
    raise ValueError(f"未知策略:{strategy}")


def batch_plan(table: Table, specs: dict[str, tuple[str, str]], ctx: dict):
    """把 {列名: (策略, 参数)} 编译成 (插入列列表, 行生成器工厂)。"""
    insert_cols, gens = [], []
    for col in table.columns:
        strategy, param = specs.get(col.name, ("默认", ""))
        if strategy == "默认":
            if not col.has_default and not col.nullable:
                raise ValueError(f"列 {col.name}: 无默认值且不可空,不能选'默认'")
            continue
        if strategy == "NULL":
            if not col.nullable:
                raise ValueError(f"列 {col.name}: 不可为 NULL")
            insert_cols.append(col.name)
            gens.append(lambda i: None)
            continue
        insert_cols.append(col.name)
        gens.append(make_generator(strategy, param, col, ctx))
    if not insert_cols:
        raise ValueError("所有列都是'默认',请至少为一列选择生成策略")

    def rows(n: int):
        for i in range(n):
            yield tuple(g(i) for g in gens)

    return insert_cols, rows


# ---------------------------------------------------------------- 场景预设

def lux_at(t: datetime, rng: random.Random) -> int:
    """模拟室外自然光照度:6:00-19:00 正弦日照曲线(峰值约 5.5 万 lux,云层扰动),夜间 2-25。"""
    h = t.hour + t.minute / 60 + t.second / 3600
    if 6.0 <= h <= 19.0:
        x = math.sin(math.pi * (h - 6.0) / 13.0)
        base = 55000 * (x ** 1.3) * rng.uniform(0.35, 1.0)
        base += rng.uniform(-300, 300)
    else:
        base = rng.uniform(2, 25)
    return max(0, int(base))


def seed_lux_curve(conn, device_id: str, days: int, interval_secs: int,
                   new_device: bool = False, name: str = "", location: str = "",
                   progress=None, cancel=None, seed: int | None = None) -> int:
    """为设备回填 days 天的光照历史(created_at 时间序列),并把设备置为 online 刷新心跳。"""
    rng = random.Random(seed)
    now = datetime.now().astimezone()
    start = now - timedelta(days=days)
    n = int(days * 86400 / interval_secs)

    if new_device:
        conn.execute(
            "INSERT INTO device (id, name, location) VALUES (%s, %s, %s) ON CONFLICT (id) DO NOTHING",
            (device_id, name or device_id, location),
        )
        conn.commit()

    def rows():
        for i in range(n):
            t = start + timedelta(seconds=interval_secs * i)
            yield (device_id, lux_at(t, rng), t)

    count = insert_rows(conn, "lux_record", ["device_id", "lux", "created_at"],
                        rows(), total=n, progress=progress, cancel=cancel)
    # 与后端 apply_shadow_props 等效的心跳/灯态维护
    conn.execute(
        "UPDATE device SET last_seen_at=now(), status='online' WHERE id=%s",
        (device_id,),
    )
    conn.commit()
    return count


def set_device_online(conn, device_id: str, online: bool) -> str:
    """按后端 apply_online_status 的语义翻转设备状态并产生/消解离线告警。"""
    if online:
        row = conn.execute(
            "UPDATE device SET status='online', last_seen_at=now() "
            "WHERE id=%s AND status!='online' RETURNING id",
            (device_id,),
        ).fetchone()
        if row:
            conn.execute(
                "UPDATE alarm SET resolved_at=now() "
                "WHERE device_id=%s AND type='offline' AND resolved_at IS NULL",
                (device_id,),
            )
            conn.commit()
            return f"{device_id} 已置为 online,未消解的离线告警已自动消解"
        conn.commit()
        return f"{device_id} 本来就在线,无变化"
    row = conn.execute(
        "UPDATE device SET status='offline' WHERE id=%s AND status!='offline' RETURNING id",
        (device_id,),
    ).fetchone()
    if row:
        conn.execute(
            "INSERT INTO alarm (device_id, type, message) VALUES (%s, 'offline', '设备离线')",
            (device_id,),
        )
        conn.commit()
        return f"{device_id} 已置为 offline,并产生一条离线告警"
    conn.commit()
    return f"{device_id} 本来就离线,无变化"
