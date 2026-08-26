// 维护智能问答（本地检索增强，无需外部大模型）
//
// 流程：意图识别（关键词加权）→ 实体抽取（设备 / 时间窗）→ 真查业务表 → 模板生成回答。
// 逻辑移植自 smart-street-light/backend/main.py（Python 版），表名适配本仓库 Rust 后端：
//   alarm / lux_record / config / device / command_record / maintenance_knowledge
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use sqlx::postgres::PgArguments;
use sqlx::query::QueryAs;
use sqlx::{PgPool, Postgres};
use std::sync::LazyLock;

const KB_INTRO: &str = "知识库覆盖：离线、光照异常、频繁开关、通信超时、灯不亮、温度过高。可问我：告警情况、光照趋势、阈值设置、设备状态、控制指令或维护建议。";

// 查询行结构(FromRow 按列名映射,不再依赖"列顺序"注释)
#[derive(sqlx::FromRow)]
struct AlarmRow {
    device_id: String,
    r#type: String,
    message: String,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct DeviceRow {
    id: String,
    name: String,
    location: String,
    status: String,
    lamp: String,
    last_seen_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct CommandRow {
    device_id: String,
    action: String,
    source: String,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LuxAggRow {
    count: i64,
    min: Option<i32>,
    max: Option<i32>,
    avg: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct ThresholdRow {
    device_id: String,
    threshold: i32,
}

#[derive(sqlx::FromRow)]
struct KnowledgeRow {
    keyword: String,
    cause: String,
    suggestion: String,
}

// 正则编译一次,进程内复用(编译期常量,非法时首次使用即暴露)
static RE_WINDOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"最近\s*(\d+)\s*(天|日|小时|分钟|周)").expect("valid regex")
});
static RE_DEVICE_NUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"灯\s*(\d+)\s*号|(\d+)\s*号\s*灯|灯\s*(\d+)")
        .expect("valid regex")
});

// 意图词典：命中关键词累加长度作得分（长词权重高），取最高分为意图
const INTENTS: &[(&str, &[&str])] = &[
    ("query_alarm", &["告警", "报警", "离线", "故障", "异常"]),
    ("query_threshold", &["阈值", "参数", "配置", "下限", "上限"]),
    (
        "query_luminance",
        &["光照", "亮度", "照度", "光照强度", "lux"],
    ),
    ("query_device", &["设备", "在线", "状态", "路灯", "灯"]),
    (
        "query_command",
        &["指令", "开关", "控制记录", "操作记录", "记录"],
    ),
    (
        "advice",
        &[
            "怎么",
            "如何",
            "为什么",
            "原因",
            "建议",
            "维修",
            "维护",
            "处理",
            "解决",
            "排查",
            "频繁",
            "温度",
            "抖",
        ],
    ),
];

pub fn classify_intent(question: &str) -> &'static str {
    let q = question.to_lowercase();
    // 声明式 fold:命中关键词长度累加为得分,严格大于才替换(平局保留先声明的意图)
    INTENTS
        .iter()
        .fold(
            ("fallback", 0usize),
            |best @ (_, best_score), &(intent, kws)| {
                let score: usize = kws
                    .iter()
                    .filter(|kw| q.contains(**kw))
                    .map(|kw| kw.chars().count())
                    .sum();
                if score > best_score {
                    (intent, score)
                } else {
                    best
                }
            },
        )
        .0
}

/// 解析"最近N天/小时/分钟/周"，返回 (起始时间, 描述)
pub fn parse_window(
    question: &str,
    default_days: i64,
) -> (DateTime<Utc>, String) {
    RE_WINDOW.captures(question).map_or_else(
        || {
            (
                Utc::now() - Duration::days(default_days),
                format!("最近{default_days}天"),
            )
        },
        |caps| {
            let n: i64 = caps[1].parse().unwrap_or(default_days);
            let unit = &caps[2];
            let (dur, label) = match unit {
                "小时" => (Duration::hours(n), "小时"),
                "分钟" => (Duration::minutes(n), "分钟"),
                "周" => (Duration::weeks(n), "周"),
                _ => (Duration::days(n), "天"),
            };
            (Utc::now() - dur, format!("最近{n}{label}"))
        },
    )
}

/// 从提问抽取设备：优先匹配 `device_id/name` 子串，其次"灯N号"/"N号灯"。None = 全部设备
async fn resolve_device(
    pool: &PgPool,
    question: &str,
) -> Result<Option<String>, sqlx::Error> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM device ORDER BY created_at")
            .fetch_all(pool)
            .await?;
    if let Some((id, _)) = rows
        .iter()
        .find(|(id, name)| question.contains(id) || question.contains(name))
    {
        return Ok(Some(id.clone()));
    }
    if let Some(caps) = RE_DEVICE_NUM.captures(question)
        && let Some(num) =
            caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3))
    {
        let num = num.as_str();
        return Ok(rows
            .iter()
            .find(|(id, name)| id.contains(num) || name.contains(num))
            .map(|(id, _)| id.clone()));
    }
    Ok(None)
}

/// 知识库检索：任一文本命中关键词即返回"原因+建议"
async fn find_advice(
    pool: &PgPool,
    texts: &[&str],
) -> Result<Option<String>, sqlx::Error> {
    let rows: Vec<KnowledgeRow> = sqlx::query_as(
        "SELECT keyword, cause, suggestion FROM maintenance_knowledge",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .find(|k| texts.iter().any(|t| t.contains(&k.keyword)))
        .map(|k| {
            format!(
                "【{}】原因：{}；建议：{}",
                k.keyword, k.cause, k.suggestion
            )
        }))
}

/// 可选设备过滤:Some 时向查询尾部追加 $n 绑定,None 时原样返回
fn bind_opt_device<'q, O>(
    query: QueryAs<'q, Postgres, O, PgArguments>,
    device_id: Option<&'q str>,
) -> QueryAs<'q, Postgres, O, PgArguments> {
    match device_id {
        Some(d) => query.bind(d),
        None => query,
    }
}

pub fn fmt_time(dt: DateTime<Utc>) -> String {
    dt.format("%m-%d %H:%M").to_string()
}

/// 主流程：识别意图与设备后,分发到各意图的处理函数
pub async fn answer(
    pool: &PgPool,
    question: &str,
) -> Result<String, sqlx::Error> {
    let intent = classify_intent(question);
    let device_id = resolve_device(pool, question).await?;
    let scope = device_id
        .as_deref()
        .map_or_else(|| "全部设备".to_string(), |d| format!("设备 {d}"));
    let dev = device_id.as_deref();

    match intent {
        "query_alarm" => answer_alarm(pool, dev, question, &scope).await,
        "query_luminance" => {
            answer_luminance(pool, dev, question, &scope).await
        }
        "query_threshold" => answer_threshold(pool, dev, &scope).await,
        "query_device" => answer_devices(pool, dev, &scope).await,
        "query_command" => answer_commands(pool, dev, question, &scope).await,
        _ => Ok(find_advice(pool, std::slice::from_ref(&question))
            .await?
            .unwrap_or_else(|| format!("没太理解您的问题。{KB_INTRO}"))),
    }
}

async fn answer_alarm(
    pool: &PgPool,
    device_id: Option<&str>,
    question: &str,
    scope: &str,
) -> Result<String, sqlx::Error> {
    let (start, desc) = parse_window(question, 7);
    let sql = if device_id.is_some() {
        "SELECT device_id, type, message, created_at, resolved_at FROM alarm \
         WHERE created_at >= $1 AND device_id = $2 ORDER BY created_at DESC LIMIT 20"
    } else {
        "SELECT device_id, type, message, created_at, resolved_at FROM alarm \
         WHERE created_at >= $1 ORDER BY created_at DESC LIMIT 20"
    };
    let rows: Vec<AlarmRow> =
        bind_opt_device(sqlx::query_as(sql).bind(start), device_id)
            .fetch_all(pool)
            .await?;
    if rows.is_empty() {
        return Ok(format!("{desc}，{scope}没有告警记录。"));
    }
    let unhandled = rows.iter().filter(|r| r.resolved_at.is_none()).count();
    let mut lines = vec![format!(
        "{desc}，{scope}共 {} 条告警，未处理 {unhandled} 条：",
        rows.len()
    )];
    for r in rows.iter().take(5) {
        let tag = if r.resolved_at.is_none() {
            "未处理"
        } else {
            "已处理"
        };
        lines.push(format!(
            "· {} {}（{tag}）{} {}",
            r.device_id,
            r.r#type,
            fmt_time(r.created_at),
            r.message
        ));
    }
    let texts: Vec<&str> = rows
        .iter()
        .flat_map(|r| [r.r#type.as_str(), r.message.as_str()])
        .collect();
    if let Some(adv) = find_advice(pool, &texts).await? {
        lines.push(format!("维护建议：{adv}"));
    }
    Ok(lines.join("\n"))
}

async fn answer_luminance(
    pool: &PgPool,
    device_id: Option<&str>,
    question: &str,
    scope: &str,
) -> Result<String, sqlx::Error> {
    let (start, desc) = parse_window(question, 1);
    let sql = if device_id.is_some() {
        "SELECT COUNT(*) AS count, MIN(lux) AS min, MAX(lux) AS max, \
                AVG(lux)::float8 AS avg FROM lux_record \
         WHERE created_at >= $1 AND device_id = $2"
    } else {
        "SELECT COUNT(*) AS count, MIN(lux) AS min, MAX(lux) AS max, \
                AVG(lux)::float8 AS avg FROM lux_record \
         WHERE created_at >= $1"
    };
    let row: LuxAggRow =
        bind_opt_device(sqlx::query_as(sql).bind(start), device_id)
            .fetch_one(pool)
            .await?;
    if row.count == 0 {
        return Ok(format!("{desc}，{scope}没有光照数据。"));
    }
    Ok(format!(
        "{desc}，{scope}光照数据 {} 条：最低 {} lux，最高 {} lux，平均 {:.0} lux。",
        row.count,
        row.min.unwrap_or(0),
        row.max.unwrap_or(0),
        row.avg.unwrap_or(0.0),
    ))
}

async fn answer_threshold(
    pool: &PgPool,
    device_id: Option<&str>,
    scope: &str,
) -> Result<String, sqlx::Error> {
    let sql = if device_id.is_some() {
        "SELECT device_id, threshold FROM config WHERE device_id = $1"
    } else {
        "SELECT device_id, threshold FROM config"
    };
    let rows: Vec<ThresholdRow> =
        bind_opt_device(sqlx::query_as(sql), device_id)
            .fetch_all(pool)
            .await?;
    if rows.is_empty() {
        return Ok(format!("{scope}暂未设置光照联动阈值（默认 40 lux）。"));
    }
    let mut lines = vec![format!("{scope}光照联动阈值：")];
    for r in &rows {
        lines.push(format!("· {}：{} lux", r.device_id, r.threshold));
    }
    Ok(lines.join("\n"))
}

async fn answer_devices(
    pool: &PgPool,
    device_id: Option<&str>,
    scope: &str,
) -> Result<String, sqlx::Error> {
    let sql = if device_id.is_some() {
        "SELECT id, name, location, status, lamp, last_seen_at FROM device \
         WHERE id = $1 ORDER BY created_at"
    } else {
        "SELECT id, name, location, status, lamp, last_seen_at FROM device \
         ORDER BY created_at"
    };
    let rows: Vec<DeviceRow> = bind_opt_device(sqlx::query_as(sql), device_id)
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Ok("当前没有路灯设备。".to_string());
    }
    let mut lines = vec![format!("{scope}共 {} 台路灯：", rows.len())];
    for r in &rows {
        lines.push(format!(
            "· {}（{}）位置{}，状态{}，灯{}，最近上报{}",
            r.name,
            r.id,
            if r.location.is_empty() {
                "-"
            } else {
                &r.location
            },
            if r.status == "online" {
                "在线"
            } else {
                "离线"
            },
            if r.lamp == "on" { "亮" } else { "灭" },
            r.last_seen_at.map_or_else(|| "-".to_string(), fmt_time),
        ));
    }
    Ok(lines.join("\n"))
}

async fn answer_commands(
    pool: &PgPool,
    device_id: Option<&str>,
    question: &str,
    scope: &str,
) -> Result<String, sqlx::Error> {
    let (start, desc) = parse_window(question, 7);
    let sql = if device_id.is_some() {
        "SELECT device_id, action, source, status, created_at FROM command_record \
         WHERE created_at >= $1 AND device_id = $2 ORDER BY created_at DESC LIMIT 10"
    } else {
        "SELECT device_id, action, source, status, created_at FROM command_record \
         WHERE created_at >= $1 ORDER BY created_at DESC LIMIT 10"
    };
    let rows: Vec<CommandRow> =
        bind_opt_device(sqlx::query_as(sql).bind(start), device_id)
            .fetch_all(pool)
            .await?;
    if rows.is_empty() {
        return Ok(format!("{desc}，{scope}没有控制指令记录。"));
    }
    let mut lines = vec![format!("{desc}，{scope}最近的指令记录：")];
    for r in &rows {
        lines.push(format!(
            "· {} {}（{}，{}）{}",
            r.device_id,
            if r.action == "on" {
                "开灯"
            } else if r.action == "off" {
                "关灯"
            } else {
                r.action.as_str()
            },
            if r.source == "auto" {
                "自动联动"
            } else {
                "手动"
            },
            if r.status == "sent" {
                "已受理"
            } else {
                "失败"
            },
            fmt_time(r.created_at),
        ));
    }
    Ok(lines.join("\n"))
}
