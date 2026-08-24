"""智慧路灯后端服务

功能：
- 账号管理：市政人员 / 路灯管理员 的增删查（密码 bcrypt 哈希）
- 数据采集：光照强度 / 心跳(设备状态) / 告警 上报接口（HTTP REST）
- 查询接口：供管理页面展示设备、告警、光照、角色

运行：  uvicorn main:app --host 127.0.0.1 --port 8000
文档：  http://127.0.0.1:8000/docs
管理页：http://127.0.0.1:8000/
"""
import os
import re
from contextlib import contextmanager
from datetime import datetime, timedelta

import bcrypt
import pymysql
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field

# ---------------- 数据库配置 ----------------
# 默认值适配团队共同开发环境（root/123456）。
# 队友的 MySQL 密码或地址不同时，无需改代码，用环境变量覆盖即可，例如：
#   set DB_PASSWORD=你的密码        (Windows cmd)
#   $env:DB_PASSWORD="你的密码"     (PowerShell)
DB_CONFIG = {
    "host": os.getenv("DB_HOST", "127.0.0.1"),
    "port": int(os.getenv("DB_PORT", "3306")),
    "user": os.getenv("DB_USER", "root"),
    "password": os.getenv("DB_PASSWORD", "123456"),
    "database": os.getenv("DB_NAME", "smart_street_light"),
    "charset": "utf8mb4",
    "cursorclass": pymysql.cursors.DictCursor,
    "autocommit": True,
}

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
STATIC_DIR = os.path.join(BASE_DIR, "static")

# 光照联动防抖：连续 N 次采样低于/高于阈值才动作，避免传感器单次毛刺误触发
DEBOUNCE_SAMPLES = 3

app = FastAPI(
    title="智慧路灯后端",
    description="智慧路灯平台接口：账号管理（市政人员 / 路灯管理员增删）与数据采集（光照 / 心跳 / 告警上报）。",
    version="1.0.0",
    swagger_ui_parameters={"defaultModelsExpandDepth": -1},
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


@contextmanager
def get_conn():
    conn = pymysql.connect(**DB_CONFIG)
    try:
        yield conn
    finally:
        conn.close()


# ---------------- 密码哈希 ----------------
def hash_password(password: str) -> str:
    # bcrypt 只取前 72 字节
    return bcrypt.hashpw(password.encode("utf-8")[:72], bcrypt.gensalt()).decode("utf-8")


def verify_password(password: str, hashed: str) -> bool:
    try:
        return bcrypt.checkpw(password.encode("utf-8")[:72], hashed.encode("utf-8"))
    except ValueError:
        return False


# ---------------- 请求体模型 ----------------
class UserCreate(BaseModel):
    username: str = Field(..., min_length=1, max_length=64, description="登录用户名")
    password: str = Field(..., min_length=6, max_length=64, description="密码（至少 6 位）")
    real_name: str = Field("", max_length=64, description="姓名（可选）")
    role_id: int = Field(..., description="角色 ID：1 市政人员 / 2 路灯管理员")


class LuminanceIn(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")
    luminance: float = Field(..., description="光照强度（lux）")


class HeartbeatIn(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")
    online_status: bool = Field(..., description="在线状态：true 在线 / false 离线")


class AlarmIn(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")
    alarm_type: str = Field("offline", max_length=32, description="告警类型，如 offline")
    message: str = Field("", max_length=255, description="告警内容")


class DeviceCreate(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")
    name: str = Field("", max_length=128, description="设备名称")
    location: str = Field("", max_length=255, description="安装位置/路段")


class ThresholdIn(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")
    low_threshold: float = Field(100.0, description="开灯阈值下限(lux)")
    high_threshold: float = Field(300.0, description="关灯阈值上限(lux)")


class ControlEvalIn(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")


class ManualControlIn(BaseModel):
    device_id: str = Field(..., min_length=1, max_length=64, description="设备唯一标识")
    action: str = Field(..., description="开关灯：on-开灯 off-关灯")


def _ensure_device(conn, device_id: str):
    """上报数据时若设备不存在则自动注册"""
    with conn.cursor() as cur:
        cur.execute("SELECT id FROM device WHERE device_id=%s", (device_id,))
        if cur.fetchone() is None:
            cur.execute(
                "INSERT INTO device (device_id, name, location, online_status, last_heartbeat) "
                "VALUES (%s, %s, '', 0, NULL)",
                (device_id, device_id),
            )


def _auto_control(conn, device_id: str, luminance: float):
    """光照联动算法（滞回控制 + 连续确认防抖）：
    - 关灯时：连续 DEBOUNCE_SAMPLES 次光照都低于下限 low → 自动开灯
    - 开灯时：连续 DEBOUNCE_SAMPLES 次光照都高于上限 high → 自动关灯
    - 其余情况维持现状，既避免阈值附近抖光，也避免单次毛刺误触发
    阈值取 threshold_config 配置，未配置时用默认 100 / 300 lux。
    动作会同步更新 device.lamp_status 并写入 command_record（source=auto）。
    返回 (action, reason)，action 为 'on' / 'off' / None。"""
    n = DEBOUNCE_SAMPLES
    with conn.cursor() as cur:
        # 1) 阈值配置
        cur.execute(
            "SELECT low_threshold, high_threshold FROM threshold_config WHERE device_id=%s",
            (device_id,),
        )
        cfg = cur.fetchone()
        low = cfg["low_threshold"] if cfg else 100.0
        high = cfg["high_threshold"] if cfg else 300.0

        # 2) 当前灯状态 = device.lamp_status（0-关 1-开）
        cur.execute("SELECT lamp_status FROM device WHERE device_id=%s", (device_id,))
        dev = cur.fetchone()
        is_on = dev is not None and dev["lamp_status"] == 1

        # 3) 最近 n 次采样（本次上报已在调用前入库，故包含当前值）
        cur.execute(
            "SELECT luminance FROM luminance_data WHERE device_id=%s ORDER BY id DESC LIMIT %s",
            (device_id, n),
        )
        samples = [r["luminance"] for r in cur.fetchall()]

        # 样本不足时不动作，保证防抖有效
        if len(samples) < n:
            return None, f"样本不足（{len(samples)}/{n}），等待更多上报后再联动"

        # 4) 滞回 + 连续确认判断
        if not is_on and all(v < low for v in samples):
            action, reason = "on", f"连续 {n} 次光照均低于下限 {low}，自动开灯"
        elif is_on and all(v > high for v in samples):
            action, reason = "off", f"连续 {n} 次光照均高于上限 {high}，自动关灯"
        else:
            action, reason = None, f"光照 {luminance} lux，滞回区间内（当前{'开' if is_on else '关'}灯），维持现状"

        if action is not None:
            cur.execute(
                "INSERT INTO command_record (device_id, command_type, source, status) "
                "VALUES (%s, %s, 'auto', 'sent')",
                (device_id, action),
            )
            cur.execute(
                "UPDATE device SET lamp_status=%s WHERE device_id=%s",
                (1 if action == "on" else 0, device_id),
            )
        return action, reason


# ---------------- 健康检查 / 角色 ----------------
@app.get("/api/health", summary="健康检查", tags=["系统"])
def health():
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT 1")
            cur.fetchone()
    return {"status": "ok", "database": "connected"}


@app.get("/api/roles", summary="角色列表", tags=["账号管理"])
def list_roles():
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT id, role_code, role_name, description FROM role ORDER BY id")
            return cur.fetchall()


# ---------------- 用户增删查 ----------------
@app.get("/api/users", summary="账号列表", tags=["账号管理"])
def list_users():
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT u.id, u.username, u.real_name, u.role_id,
                       r.role_code, r.role_name, u.status, u.created_at
                FROM user u JOIN role r ON r.id = u.role_id
                ORDER BY u.id
                """
            )
            return cur.fetchall()


@app.post("/api/users", status_code=201, summary="新增账号", tags=["账号管理"])
def create_user(body: UserCreate):
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT id FROM role WHERE id=%s", (body.role_id,))
            if cur.fetchone() is None:
                raise HTTPException(status_code=400, detail="角色不存在")
            cur.execute("SELECT id FROM user WHERE username=%s", (body.username,))
            if cur.fetchone() is not None:
                raise HTTPException(status_code=409, detail="用户名已存在")
            cur.execute(
                "INSERT INTO user (username, password_hash, real_name, role_id, status) "
                "VALUES (%s, %s, %s, %s, 1)",
                (body.username, hash_password(body.password), body.real_name, body.role_id),
            )
            new_id = cur.lastrowid
    return {"id": new_id, "username": body.username, "role_id": body.role_id}


@app.delete("/api/users/{user_id}", summary="删除账号", tags=["账号管理"])
def delete_user(user_id: int):
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT id FROM user WHERE id=%s", (user_id,))
            if cur.fetchone() is None:
                raise HTTPException(status_code=404, detail="用户不存在")
            cur.execute("DELETE FROM user WHERE id=%s", (user_id,))
    return {"deleted": user_id}


# ---------------- 数据采集接口 ----------------
@app.post("/api/data/luminance", status_code=201, summary="上报光照强度", tags=["数据采集"])
def report_luminance(body: LuminanceIn):
    with get_conn() as conn:
        _ensure_device(conn, body.device_id)
        with conn.cursor() as cur:
            cur.execute(
                "INSERT INTO luminance_data (device_id, luminance) VALUES (%s, %s)",
                (body.device_id, body.luminance),
            )
            new_id = cur.lastrowid
            cur.execute(
                "SELECT id, device_id, luminance, created_at FROM luminance_data WHERE id=%s",
                (new_id,),
            )
            row = cur.fetchone()
        # 实时联动：根据阈值自动判断开关灯
        action, reason = _auto_control(conn, body.device_id, body.luminance)
        row["auto_action"] = action
        row["auto_reason"] = reason
    return row


@app.post("/api/data/heartbeat", summary="上报设备心跳/状态", tags=["数据采集"])
def report_heartbeat(body: HeartbeatIn):
    new_status = 1 if body.online_status else 0
    with get_conn() as conn:
        _ensure_device(conn, body.device_id)
        with conn.cursor() as cur:
            cur.execute("SELECT online_status FROM device WHERE device_id=%s", (body.device_id,))
            prev = cur.fetchone()
            was_online = prev is not None and prev["online_status"] == 1
            cur.execute(
                "UPDATE device SET online_status=%s, last_heartbeat=NOW() WHERE device_id=%s",
                (new_status, body.device_id),
            )
            # 在线 → 离线切换时，自动生成一条离线告警
            if was_online and new_status == 0:
                cur.execute(
                    "INSERT INTO alarm_record (device_id, alarm_type, message, status) "
                    "VALUES (%s, 'offline', '设备心跳离线', 0)",
                    (body.device_id,),
                )
            cur.execute(
                "SELECT id, device_id, online_status, lamp_status, last_heartbeat FROM device WHERE device_id=%s",
                (body.device_id,),
            )
            row = cur.fetchone()
    return row


@app.post("/api/data/alarm", status_code=201, summary="上报设备告警", tags=["数据采集"])
def report_alarm(body: AlarmIn):
    with get_conn() as conn:
        _ensure_device(conn, body.device_id)
        with conn.cursor() as cur:
            cur.execute(
                "INSERT INTO alarm_record (device_id, alarm_type, message, status) "
                "VALUES (%s, %s, %s, 0)",
                (body.device_id, body.alarm_type, body.message),
            )
            new_id = cur.lastrowid
            cur.execute(
                "SELECT id, device_id, alarm_type, message, status, created_at "
                "FROM alarm_record WHERE id=%s",
                (new_id,),
            )
            row = cur.fetchone()
    return row


# ---------------- 设备控制（光照联动） ----------------
@app.post("/api/thresholds", summary="设置光照阈值", tags=["设备控制"])
def set_threshold(body: ThresholdIn):
    """为设备配置光照联动阈值（不存在则插入，存在则更新）。"""
    if body.low_threshold >= body.high_threshold:
        raise HTTPException(status_code=400, detail="下限必须小于上限（low_threshold < high_threshold）")
    with get_conn() as conn:
        _ensure_device(conn, body.device_id)
        with conn.cursor() as cur:
            cur.execute(
                "INSERT INTO threshold_config (device_id, low_threshold, high_threshold) "
                "VALUES (%s, %s, %s) "
                "ON DUPLICATE KEY UPDATE low_threshold=%s, high_threshold=%s",
                (body.device_id, body.low_threshold, body.high_threshold,
                 body.low_threshold, body.high_threshold),
            )
    return {
        "device_id": body.device_id,
        "low_threshold": body.low_threshold,
        "high_threshold": body.high_threshold,
    }


@app.post("/api/control/auto", summary="光照联动（手动触发一次评估）", tags=["设备控制"])
def auto_control(body: ControlEvalIn):
    """读取该设备最新一条光照，运行滞回控制算法，返回本次动作与原因。"""
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute(
                "SELECT luminance FROM luminance_data WHERE device_id=%s ORDER BY id DESC LIMIT 1",
                (body.device_id,),
            )
            latest = cur.fetchone()
        if latest is None:
            raise HTTPException(status_code=404, detail="该设备暂无光照数据")
        action, reason = _auto_control(conn, body.device_id, latest["luminance"])
    return {
        "device_id": body.device_id,
        "luminance": latest["luminance"],
        "action": action,
        "reason": reason,
    }


@app.post("/api/control/manual", summary="手动开关灯", tags=["设备控制"])
def manual_control(body: ManualControlIn):
    if body.action not in ("on", "off"):
        raise HTTPException(status_code=400, detail="action 只能是 on 或 off")
    with get_conn() as conn:
        _ensure_device(conn, body.device_id)
        with conn.cursor() as cur:
            cur.execute(
                "UPDATE device SET lamp_status=%s WHERE device_id=%s",
                (1 if body.action == "on" else 0, body.device_id),
            )
            cur.execute(
                "INSERT INTO command_record (device_id, command_type, source, status) "
                "VALUES (%s, %s, 'manual', 'sent')",
                (body.device_id, body.action),
            )
    return {"device_id": body.device_id, "action": body.action,
            "lamp_status": 1 if body.action == "on" else 0}


@app.post("/api/alarms/{alarm_id}/resolve", summary="处理告警（标记已处理）", tags=["告警管理"])
def resolve_alarm(alarm_id: int):
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT id FROM alarm_record WHERE id=%s", (alarm_id,))
            if cur.fetchone() is None:
                raise HTTPException(status_code=404, detail="告警不存在")
            cur.execute(
                "UPDATE alarm_record SET status=1, resolved_at=NOW() WHERE id=%s",
                (alarm_id,),
            )
    return {"resolved": alarm_id, "status": 1}


# ---------------- 设备管理（增删查） ----------------
@app.post("/api/devices", status_code=201, summary="新增路灯设备", tags=["设备管理"])
def create_device(body: DeviceCreate):
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT id FROM device WHERE device_id=%s", (body.device_id,))
            if cur.fetchone() is not None:
                raise HTTPException(status_code=409, detail="设备ID已存在")
            cur.execute(
                "INSERT INTO device (device_id, name, location, online_status) VALUES (%s, %s, %s, 0)",
                (body.device_id, body.name, body.location),
            )
            new_id = cur.lastrowid
            cur.execute(
                "SELECT id, device_id, name, location, online_status, lamp_status, last_heartbeat, created_at "
                "FROM device WHERE id=%s",
                (new_id,),
            )
            row = cur.fetchone()
    return row


@app.delete("/api/devices/{device_id}", summary="删除/解绑路灯设备", tags=["设备管理"])
def delete_device(device_id: int):
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute("SELECT id, device_id FROM device WHERE id=%s", (device_id,))
            row = cur.fetchone()
            if row is None:
                raise HTTPException(status_code=404, detail="设备不存在")
            did = row["device_id"]
            # 级联清除该设备的关联数据
            cur.execute("DELETE FROM luminance_data WHERE device_id=%s", (did,))
            cur.execute("DELETE FROM threshold_config WHERE device_id=%s", (did,))
            cur.execute("DELETE FROM alarm_record WHERE device_id=%s", (did,))
            cur.execute("DELETE FROM command_record WHERE device_id=%s", (did,))
            cur.execute("DELETE FROM device WHERE id=%s", (device_id,))
    return {"deleted": device_id, "device_id": did}


# ---------------- 查询接口（供管理页展示） ----------------
@app.get("/api/devices", summary="设备列表", tags=["设备管理"])
def list_devices():
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute(
                "SELECT id, device_id, name, location, online_status, lamp_status, last_heartbeat, created_at "
                "FROM device ORDER BY id DESC"
            )
            return cur.fetchall()


@app.get("/api/alarms", summary="告警列表", tags=["查询"])
def list_alarms(limit: int = 50):
    limit = max(1, min(int(limit), 1000))
    with get_conn() as conn:
        with conn.cursor() as cur:
            cur.execute(
                f"SELECT id, device_id, alarm_type, message, status, created_at "
                f"FROM alarm_record ORDER BY id DESC LIMIT {limit}"
            )
            return cur.fetchall()


@app.get("/api/luminance", summary="光照数据", tags=["查询"])
def list_luminance(device_id: str | None = None, limit: int = 50):
    limit = max(1, min(int(limit), 1000))
    with get_conn() as conn:
        with conn.cursor() as cur:
            if device_id:
                cur.execute(
                    f"SELECT id, device_id, luminance, created_at FROM luminance_data "
                    f"WHERE device_id=%s ORDER BY id DESC LIMIT {limit}",
                    (device_id,),
                )
            else:
                cur.execute(
                    f"SELECT id, device_id, luminance, created_at FROM luminance_data "
                    f"ORDER BY id DESC LIMIT {limit}"
                )
            return cur.fetchall()


@app.get("/api/commands", summary="控制指令记录", tags=["查询"])
def list_commands(device_id: str | None = None, limit: int = 50):
    limit = max(1, min(int(limit), 1000))
    with get_conn() as conn:
        with conn.cursor() as cur:
            if device_id:
                cur.execute(
                    f"SELECT id, device_id, command_type, source, status, message, created_at "
                    f"FROM command_record WHERE device_id=%s ORDER BY id DESC LIMIT {limit}",
                    (device_id,),
                )
            else:
                cur.execute(
                    f"SELECT id, device_id, command_type, source, status, message, created_at "
                    f"FROM command_record ORDER BY id DESC LIMIT {limit}"
                )
            return cur.fetchall()


# ---------------- 维护智能问答（本地检索增强，无需外部大模型） ----------------
# 流程：意图识别（关键词加权）→ 实体抽取（设备 / 时间窗）→ 真查业务表 → 模板生成回答。
# "生成"用话术模板而非大模型，但"检索"是真实的：命中业务数据 + 维护知识库 maintenance_knowledge。

class MaintenanceAskIn(BaseModel):
    question: str = Field(..., min_length=1, max_length=200, description="用户提问")

# 意图词典：命中关键词累加长度作得分（长词权重高），取最高分为意图
_INTENTS = [
    ("query_alarm",     ["告警", "报警", "离线", "故障", "异常"]),
    ("query_threshold", ["阈值", "光照阈值", "参数", "配置", "下限", "上限"]),
    ("query_luminance", ["光照", "亮度", "照度", "光照强度", "lux"]),
    ("query_device",    ["设备", "在线", "状态", "路灯", "灯"]),
    ("query_command",   ["指令", "开关", "控制记录", "操作记录", "记录"]),
    ("advice",          ["怎么", "如何", "为什么", "原因", "建议", "维修", "维护", "处理", "解决", "排查", "频繁", "温度", "抖"]),
]

_KB_INTRO = "知识库覆盖：离线、光照异常、频繁开关、通信超时、灯不亮、温度过高。"


def _classify_intent(question: str) -> str:
    q = question.lower()
    best, best_score = "fallback", 0
    for intent, kws in _INTENTS:
        score = sum(len(kw) for kw in kws if kw in q)
        if score > best_score:
            best, best_score = intent, score
    return best


def _parse_window(question: str, default_days: int = 7):
    m = re.search(r"最近\s*(\d+)\s*(天|日|小时|分钟|周)", question)
    if not m:
        return datetime.now() - timedelta(days=default_days), f"最近{default_days}天"
    n, unit = int(m.group(1)), m.group(2)
    step = {"天": timedelta(days=n), "日": timedelta(days=n), "小时": timedelta(hours=n),
            "分钟": timedelta(minutes=n), "周": timedelta(weeks=n)}[unit]
    return datetime.now() - step, f"最近{n}{unit}"


def _resolve_device(cur, question: str):
    """从提问抽取设备：优先匹配 device_id/name 子串，其次 '灯N号'/'N号灯'。返回 None 表示全部设备。"""
    cur.execute("SELECT device_id, name FROM device ORDER BY id")
    devices = cur.fetchall()
    for d in devices:
        if d["device_id"] and d["device_id"] in question:
            return d["device_id"]
        if d["name"] and d["name"] in question:
            return d["device_id"]
    m = (re.search(r"灯\s*(\d+)\s*号", question)
         or re.search(r"(\d+)\s*号\s*灯", question)
         or re.search(r"灯\s*(\d+)", question))
    if m:
        num = m.group(1)
        for d in devices:
            if num in (d["device_id"] or "") or num in (d["name"] or ""):
                return d["device_id"]
    return None


def _advice_for_alarms(cur, rows):
    """根据告警类型/内容关键词，从知识库取维护建议。"""
    texts = {r["alarm_type"] for r in rows} | {r["message"] or "" for r in rows}
    cur.execute("SELECT keyword, cause, suggestion FROM maintenance_knowledge")
    for e in cur.fetchall():
        if any(e["keyword"] in t for t in texts):
            return f"【{e['keyword']}】原因：{e['cause']}；建议：{e['suggestion']}"
    return ""


def _advice_for_question(cur, question: str):
    cur.execute("SELECT keyword, cause, suggestion FROM maintenance_knowledge")
    for e in cur.fetchall():
        if e["keyword"] in question:
            return f"【{e['keyword']}】原因：{e['cause']}；建议：{e['suggestion']}"
    return None


def _answer(conn, question: str) -> str:
    intent = _classify_intent(question)
    with conn.cursor() as cur:
        device_id = _resolve_device(cur, question)
        dev_clause = "AND device_id=%s" if device_id else ""
        dev_params = (device_id,) if device_id else ()
        scope = f"设备 {device_id}" if device_id else "全部设备"

        if intent == "query_alarm":
            start, desc = _parse_window(question)
            cur.execute(
                f"SELECT device_id, alarm_type, message, status, created_at FROM alarm_record "
                f"WHERE created_at>=%s {dev_clause} ORDER BY created_at DESC LIMIT 20",
                (start, *dev_params),
            )
            rows = cur.fetchall()
            if not rows:
                return f"{desc}，{scope}没有告警记录。"
            unhandled = sum(1 for r in rows if r["status"] == 0)
            lines = [f"{desc}，{scope}共 {len(rows)} 条告警，未处理 {unhandled} 条："]
            for r in rows[:5]:
                tag = "未处理" if r["status"] == 0 else "已处理"
                lines.append(
                    f"· {r['device_id']} {r['alarm_type']}（{tag}）"
                    f"{r['created_at'].strftime('%m-%d %H:%M')} {r['message']}"
                )
            adv = _advice_for_alarms(cur, rows)
            if adv:
                lines.append("维护建议：" + adv)
            return "\n".join(lines)

        if intent == "query_luminance":
            start, desc = _parse_window(question, default_days=1)
            cur.execute(
                f"SELECT COUNT(*) c, MIN(luminance) mn, MAX(luminance) mx, AVG(luminance) av "
                f"FROM luminance_data WHERE created_at>=%s {dev_clause}",
                (start, *dev_params),
            )
            s = cur.fetchone()
            if s["c"] == 0:
                return f"{desc}，{scope}没有光照数据。"
            cur.execute(
                f"SELECT luminance, created_at FROM luminance_data "
                f"WHERE created_at>=%s {dev_clause} ORDER BY id DESC LIMIT 1",
                (start, *dev_params),
            )
            latest = cur.fetchone()
            return (
                f"{desc}，{scope}共上报 {s['c']} 条光照数据：当前 {latest['luminance']} lux，"
                f"最低 {s['mn']}，最高 {s['mx']}，平均 {s['av']:.1f}。"
            )

        if intent == "query_device":
            cur.execute(
                "SELECT device_id, name, location, online_status, lamp_status, last_heartbeat "
                "FROM device ORDER BY id"
            )
            rows = cur.fetchall()
            if not rows:
                return "目前还没有接入任何路灯设备。"
            on = sum(1 for r in rows if r["online_status"] == 1)
            lines = [f"共 {len(rows)} 台设备，在线 {on} 台："]
            for r in rows:
                hb = r["last_heartbeat"].strftime("%m-%d %H:%M") if r["last_heartbeat"] else "从未心跳"
                lines.append(
                    f"· {r['device_id']}（{r['location'] or '未标注位置'}）："
                    f"{'在线' if r['online_status'] else '离线'}，灯{'开' if r['lamp_status'] else '关'}，最后心跳 {hb}"
                )
            return "\n".join(lines)

        if intent == "query_threshold":
            if device_id:
                cur.execute(
                    "SELECT device_id, low_threshold, high_threshold FROM threshold_config WHERE device_id=%s",
                    (device_id,),
                )
            else:
                cur.execute("SELECT device_id, low_threshold, high_threshold FROM threshold_config")
            rows = cur.fetchall()
            if not rows:
                return "还没有为设备配置光照阈值（默认下限 100 / 上限 300 lux）。"
            return "\n".join(
                f"· {r['device_id']}：下限 {r['low_threshold']} lux / 上限 {r['high_threshold']} lux"
                for r in rows
            )

        if intent == "query_command":
            start, desc = _parse_window(question)
            cur.execute(
                f"SELECT device_id, command_type, source, status, created_at FROM command_record "
                f"WHERE created_at>=%s {dev_clause} ORDER BY created_at DESC LIMIT 20",
                (start, *dev_params),
            )
            rows = cur.fetchall()
            if not rows:
                return f"{desc}，{scope}没有控制指令记录。"
            auto = sum(1 for r in rows if r["source"] == "auto")
            lines = [f"{desc}，{scope}共 {len(rows)} 条指令（自动 {auto} / 手动 {len(rows) - auto}）："]
            for r in rows[:5]:
                act = "开灯" if r["command_type"] == "on" else "关灯"
                lines.append(f"· {r['device_id']} {act}（{r['source']}）{r['created_at'].strftime('%m-%d %H:%M')}")
            return "\n".join(lines)

        if intent == "advice":
            adv = _advice_for_question(cur, question)
            if adv:
                return adv
            cur.execute("SELECT alarm_type, message FROM alarm_record ORDER BY id DESC LIMIT 20")
            recent = cur.fetchall()
            adv2 = _advice_for_alarms(cur, recent) if recent else ""
            if adv2:
                return "结合最近的告警：" + adv2
            return f"请告诉我具体故障现象。{_KB_INTRO}"

        return (
            "我还不太明白你的问题。你可以这样问我：\n"
            "· 最近7天有哪些告警？\n"
            "· 设备现在在线还是离线？\n"
            "· 光照阈值是多少？\n"
            "· 最近的光照数据怎么样？\n"
            "· 路灯频繁开关怎么办？"
        )


@app.post("/api/assistant/ask", summary="维护智能问答", tags=["智能问答"])
def assistant_ask(body: MaintenanceAskIn):
    with get_conn() as conn:
        answer = _answer(conn, body.question)
    return {"question": body.question, "answer": answer}


# ---------------- 静态页面 ----------------
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")


@app.get("/", include_in_schema=False)
def index():
    return FileResponse(os.path.join(STATIC_DIR, "index.html"))
