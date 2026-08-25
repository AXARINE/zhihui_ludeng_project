// 维护智能问答（本地检索增强，无需外部大模型）
//
// 流程：意图识别（关键词加权）→ 实体抽取（设备 / 时间窗）→ 真查业务表 → 模板生成回答。
// 逻辑移植自 smart-street-light/backend/main.py（Python 版），表名适配本仓库 Rust 后端：
//   alarm / lux_record / config / device / command_record / maintenance_knowledge
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use sqlx::PgPool;

const KB_INTRO: &str = "知识库覆盖：离线、光照异常、频繁开关、通信超时、灯不亮、温度过高。可问我：告警情况、光照趋势、阈值设置、设备状态、控制指令或维护建议。";

// 复杂查询行的类型别名（消 clippy::type_complexity），元素按 SQL 列顺序
// 告警行：device_id / type / message / created_at / resolved_at
type AlarmRow = (String, String, String, DateTime<Utc>, Option<DateTime<Utc>>);
// 设备行：id / name / location / status / lamp / last_seen_at
type DeviceRow = (
    String,
    String,
    String,
    String,
    String,
    Option<DateTime<Utc>>,
);

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
    let mut best = "fallback";
    let mut best_score = 0usize;
    for (intent, kws) in INTENTS {
        let score = kws
            .iter()
            .filter(|kw| q.contains(**kw))
            .map(|kw| kw.chars().count())
            .sum();
        if score > best_score {
            best = intent;
            best_score = score;
        }
    }
    best
}

/// 解析"最近N天/小时/分钟/周"，返回 (起始时间, 描述)
///
/// # Panics
/// 内置正则常量非法时 panic(编译期常量,实际不会触发)。
pub fn parse_window(
    question: &str,
    default_days: i64,
) -> (DateTime<Utc>, String) {
    let re = Regex::new(r"最近\s*(\d+)\s*(天|日|小时|分钟|周)")
        .expect("valid regex");
    re.captures(question).map_or_else(
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
    for (id, name) in &rows {
        if question.contains(id) || question.contains(name) {
            return Ok(Some(id.clone()));
        }
    }
    let re = Regex::new(r"灯\s*(\d+)\s*号|(\d+)\s*号\s*灯|灯\s*(\d+)")
        .expect("valid regex");
    if let Some(caps) = re.captures(question)
        && let Some(num) =
            caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3))
    {
        let num = num.as_str();
        for (id, name) in &rows {
            if id.contains(num) || name.contains(num) {
                return Ok(Some(id.clone()));
            }
        }
    }
    Ok(None)
}

/// 从告警文本匹配知识库建议
async fn advice_for_alarms(
    pool: &PgPool,
    texts: &[String],
) -> Result<String, sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT keyword, cause, suggestion FROM maintenance_knowledge",
    )
    .fetch_all(pool)
    .await?;
    for (kw, cause, suggestion) in &rows {
        if texts.iter().any(|t| t.contains(kw)) {
            return Ok(format!("【{kw}】原因：{cause}；建议：{suggestion}"));
        }
    }
    Ok(String::new())
}

/// 从提问匹配知识库建议
async fn advice_for_question(
    pool: &PgPool,
    question: &str,
) -> Result<Option<String>, sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT keyword, cause, suggestion FROM maintenance_knowledge",
    )
    .fetch_all(pool)
    .await?;
    for (kw, cause, suggestion) in &rows {
        if question.contains(kw) {
            return Ok(Some(format!(
                "【{kw}】原因：{cause}；建议：{suggestion}"
            )));
        }
    }
    Ok(None)
}

pub fn fmt_time(dt: DateTime<Utc>) -> String {
    dt.format("%m-%d %H:%M").to_string()
}

/// 主流程：按意图查询数据库并生成回答
pub async fn answer(
    pool: &PgPool,
    question: &str,
) -> Result<String, sqlx::Error> {
    let intent = classify_intent(question);
    let device_id = resolve_device(pool, question).await?;
    let scope = device_id
        .as_ref()
        .map_or_else(|| "全部设备".to_string(), |d| format!("设备 {d}"));
    let has_dev = device_id.is_some();

    match intent {
        "query_alarm" => {
            let (start, desc) = parse_window(question, 7);
            let sql = if has_dev {
                "SELECT device_id, type, message, created_at, resolved_at FROM alarm \
                 WHERE created_at >= $1 AND device_id = $2 ORDER BY created_at DESC LIMIT 20"
            } else {
                "SELECT device_id, type, message, created_at, resolved_at FROM alarm \
                 WHERE created_at >= $1 ORDER BY created_at DESC LIMIT 20"
            };
            let rows: Vec<AlarmRow> = if let Some(d) = &device_id {
                sqlx::query_as(sql)
                    .bind(start)
                    .bind(d)
                    .fetch_all(pool)
                    .await?
            } else {
                sqlx::query_as(sql).bind(start).fetch_all(pool).await?
            };
            if rows.is_empty() {
                return Ok(format!("{desc}，{scope}没有告警记录。"));
            }
            let unhandled = rows.iter().filter(|r| r.4.is_none()).count();
            let mut lines = vec![format!(
                "{desc}，{scope}共 {} 条告警，未处理 {unhandled} 条：",
                rows.len()
            )];
            for r in rows.iter().take(5) {
                let tag = if r.4.is_none() {
                    "未处理"
                } else {
                    "已处理"
                };
                lines.push(format!(
                    "· {} {}（{tag}）{} {}",
                    r.0,
                    r.1,
                    fmt_time(r.3),
                    r.2
                ));
            }
            let texts: Vec<String> = rows
                .iter()
                .map(|r| r.1.clone())
                .chain(rows.iter().map(|r| r.2.clone()))
                .collect();
            let adv = advice_for_alarms(pool, &texts).await?;
            if !adv.is_empty() {
                lines.push(format!("维护建议：{adv}"));
            }
            Ok(lines.join("\n"))
        }
        "query_luminance" => {
            let (start, desc) = parse_window(question, 1);
            let sql = if has_dev {
                "SELECT COUNT(*), MIN(lux), MAX(lux), AVG(lux)::float8 FROM lux_record \
                 WHERE created_at >= $1 AND device_id = $2"
            } else {
                "SELECT COUNT(*), MIN(lux), MAX(lux), AVG(lux)::float8 FROM lux_record \
                 WHERE created_at >= $1"
            };
            let row: (i64, Option<i32>, Option<i32>, Option<f64>) =
                if let Some(d) = &device_id {
                    sqlx::query_as(sql)
                        .bind(start)
                        .bind(d)
                        .fetch_one(pool)
                        .await?
                } else {
                    sqlx::query_as(sql).bind(start).fetch_one(pool).await?
                };
            if row.0 == 0 {
                return Ok(format!("{desc}，{scope}没有光照数据。"));
            }
            Ok(format!(
                "{desc}，{scope}光照数据 {} 条：最低 {} lux，最高 {} lux，平均 {:.0} lux。",
                row.0,
                row.1.unwrap_or(0),
                row.2.unwrap_or(0),
                row.3.unwrap_or(0.0),
            ))
        }
        "query_threshold" => {
            let sql = if has_dev {
                "SELECT device_id, threshold FROM config WHERE device_id = $1"
            } else {
                "SELECT device_id, threshold FROM config"
            };
            let rows: Vec<(String, i32)> = if let Some(d) = &device_id {
                sqlx::query_as(sql).bind(d).fetch_all(pool).await?
            } else {
                sqlx::query_as(sql).fetch_all(pool).await?
            };
            if rows.is_empty() {
                return Ok(format!(
                    "{scope}暂未设置光照联动阈值（默认 40 lux）。"
                ));
            }
            let mut lines = vec![format!("{scope}光照联动阈值：")];
            for (d, t) in &rows {
                lines.push(format!("· {d}：{t} lux"));
            }
            Ok(lines.join("\n"))
        }
        "query_device" => {
            let sql = if has_dev {
                "SELECT id, name, location, status, lamp, last_seen_at FROM device \
                 WHERE id = $1 ORDER BY created_at"
            } else {
                "SELECT id, name, location, status, lamp, last_seen_at FROM device \
                 ORDER BY created_at"
            };
            let rows: Vec<DeviceRow> = if let Some(d) = &device_id {
                sqlx::query_as(sql).bind(d).fetch_all(pool).await?
            } else {
                sqlx::query_as(sql).fetch_all(pool).await?
            };
            if rows.is_empty() {
                return Ok("当前没有路灯设备。".to_string());
            }
            let mut lines = vec![format!("{scope}共 {} 台路灯：", rows.len())];
            for (id, name, location, status, lamp, last) in &rows {
                lines.push(format!(
                    "· {}（{}）位置{}，状态{}，灯{}，最近上报{}",
                    name,
                    id,
                    if location.is_empty() { "-" } else { location },
                    if status == "online" {
                        "在线"
                    } else {
                        "离线"
                    },
                    if lamp == "on" { "亮" } else { "灭" },
                    last.map_or_else(|| "-".to_string(), fmt_time),
                ));
            }
            Ok(lines.join("\n"))
        }
        "query_command" => {
            let (start, desc) = parse_window(question, 7);
            let sql = if has_dev {
                "SELECT device_id, action, source, status, created_at FROM command_record \
                 WHERE created_at >= $1 AND device_id = $2 ORDER BY created_at DESC LIMIT 10"
            } else {
                "SELECT device_id, action, source, status, created_at FROM command_record \
                 WHERE created_at >= $1 ORDER BY created_at DESC LIMIT 10"
            };
            let rows: Vec<(String, String, String, String, DateTime<Utc>)> =
                if let Some(d) = &device_id {
                    sqlx::query_as(sql)
                        .bind(start)
                        .bind(d)
                        .fetch_all(pool)
                        .await?
                } else {
                    sqlx::query_as(sql).bind(start).fetch_all(pool).await?
                };
            if rows.is_empty() {
                return Ok(format!("{desc}，{scope}没有控制指令记录。"));
            }
            let mut lines = vec![format!("{desc}，{scope}最近的指令记录：")];
            for (d, action, source, status, at) in &rows {
                lines.push(format!(
                    "· {d} {}（{}，{}）{}",
                    if action == "on" {
                        "开灯"
                    } else if action == "off" {
                        "关灯"
                    } else {
                        action
                    },
                    if source == "auto" {
                        "自动联动"
                    } else {
                        "手动"
                    },
                    if status == "sent" {
                        "已受理"
                    } else {
                        "失败"
                    },
                    fmt_time(*at),
                ));
            }
            Ok(lines.join("\n"))
        }
        _ => {
            let adv = advice_for_question(pool, question).await?;
            adv.map_or_else(|| Ok(format!("没太理解您的问题。{KB_INTRO}")), Ok)
        }
    }
}
