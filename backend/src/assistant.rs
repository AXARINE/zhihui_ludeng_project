// 维护智能问答（本地检索增强，无需外部大模型）
//
// 流程：意图识别（关键词加权）→ 实体抽取（设备 / 时间窗）→ 真查业务表 → 知识库匹配调修建议。
// 回答除文本外还带 related_devices：告警涉及的设备 id 列表，前端渲染成可点击标签，
// 管理员点击即跳转设备详情页，实现"查找锁定"。
// 逻辑源自 smart-street-light/backend/main.py（Python 版），表名适配本仓库 Rust 后端：
//   alarm / lux_record / config / device / command_record / maintenance_knowledge
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde_json::json;
use sqlx::postgres::PgArguments;
use sqlx::query::QueryAs;
use sqlx::{PgPool, Postgres};
use std::sync::LazyLock;

const KB_INTRO: &str = "知识库覆盖：离线、光照异常、频繁开关、通信超时、灯不亮、调光、亮度、读数不变、上报、常亮、误报、温度过高。可问我：告警情况及处理建议、光照趋势、阈值设置、设备状态、控制指令。";

// ===== AI 生成层（可选）=====
// OpenAI 兼容接口：DeepSeek / 智谱 GLM / Kimi / 通义均适用，换 base+model 即可。
// backend/.env 配置 AI_API_KEY 即启用；不配则纯本地关键词问答。
struct AiCfg {
    key: String,
    base: String,
    model: String,
}

fn ai_cfg() -> Option<AiCfg> {
    let key = std::env::var("AI_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())?;
    Some(AiCfg {
        key,
        base: std::env::var("AI_BASE_URL")
            .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4".into()),
        model: std::env::var("AI_MODEL").unwrap_or_else(|_| "glm-4-flash".into()),
    })
}

// 进程内复用一个 HTTP 客户端（内部带连接池，别每次请求新建）
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// 问答结果：text 是给人看的回答，devices 供前端生成"锁定设备"跳转标签
pub struct Answer {
    pub text: String,
    pub devices: Vec<String>,
}

// 查询行结构(FromRow 按列名映射,不再依赖"列顺序"注释)
#[derive(sqlx::FromRow)]
struct AlarmRow {
    device_id: String,
    r#type: String,
    message: String,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    location: Option<String>, // LEFT JOIN device 带出安装位置，用于"查找锁定"
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

// alarm.type 存英文(现在只有 offline)，知识库 keyword 是中文——
// 匹配建议时按此表补一个中文词参与匹配；新增告警类型时在此补一行即可
const ALARM_TYPE_KW: &[(&str, &str)] = &[("offline", "离线")];

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

/// 去重收集设备 id：优先未处理告警的设备，没有未处理则取全部
fn collect_ids(rows: &[AlarmRow]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in rows {
        if r.resolved_at.is_some() || out.contains(&r.device_id) {
            continue;
        }
        out.push(r.device_id.clone());
    }
    if !out.is_empty() {
        return out;
    }
    for r in rows {
        if !out.contains(&r.device_id) {
            out.push(r.device_id.clone());
        }
    }
    out
}

/// 主流程：配了 AI_API_KEY 走大模型生成，否则纯本地关键词问答。
/// AI 失败自动回退本地回答（AI 层永不报错）；数据库错误仍正常上抛。
pub async fn answer(
    pool: &PgPool,
    question: &str,
) -> Result<Answer, sqlx::Error> {
    let local = answer_local(pool, question).await?;
    let Some(cfg) = ai_cfg() else {
        return Ok(local);
    };
    // 上下文包：设备清单 + 近7天告警（已含知识库建议）+ 本地意图回答。
    // 数据量小（几盏灯/十几条告警），固定全带上，规避意图识别漏检。
    let (ctx_alarms, alarm_devices) =
        answer_alarm(pool, None, "最近7天有哪些告警", "全部设备").await?;
    let (ctx_devices, _) = answer_devices(pool, None, "全部设备").await?;
    let Answer {
        text: local_text,
        devices: local_devices,
    } = local;
    let devices =
        if local_devices.is_empty() { alarm_devices } else { local_devices };
    match ai_answer(&cfg, question, &ctx_devices, &ctx_alarms, &local_text)
        .await
    {
        Ok(text) => Ok(Answer { text, devices }),
        Err(e) => {
            tracing::warn!("AI 回答失败，回退本地关键词回答: {e}");
            Ok(Answer { text: local_text, devices })
        }
    }
}

/// 调 OpenAI 兼容 /chat/completions 生成回答；失败由调用方回退本地
async fn ai_answer(
    cfg: &AiCfg,
    question: &str,
    devices_txt: &str,
    alarms_txt: &str,
    local_txt: &str,
) -> anyhow::Result<String> {
    let sys = "你是智慧路灯管理平台的维护助手，面向管理员。请依据给出的平台真实数据回答，\
               不要编造数据里没有的设备或告警；用中文纯文本（不要 Markdown 符号），\
               简洁分点；涉及故障时给出调修建议，并点名相关设备编号方便管理员定位。";
    let user = format!(
        "【设备清单】\n{devices_txt}\n\n【近7天告警（含维护建议）】\n{alarms_txt}\n\n\
         【本地意图分析结果】\n{local_txt}\n\n【管理员提问】\n{question}"
    );
    let body = json!({
        "model": cfg.model,
        "messages": [
            {"role": "system", "content": sys},
            {"role": "user", "content": user}
        ],
        "temperature": 0.3,
        "max_tokens": 600
    });
    let url = format!("{}/chat/completions", cfg.base.trim_end_matches('/'));
    let resp = HTTP
        .post(url)
        .bearer_auth(&cfg.key)
        .timeout(std::time::Duration::from_secs(60))
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let v: serde_json::Value = resp.json().await?;
    let text = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if text.is_empty() {
        anyhow::bail!("AI 返回为空");
    }
    Ok(text)
}

/// 本地流程：识别意图与设备后,分发到各意图的处理函数。
/// 每个处理函数返回 (回答文本, 涉及设备列表)
async fn answer_local(
    pool: &PgPool,
    question: &str,
) -> Result<Answer, sqlx::Error> {
    let intent = classify_intent(question);
    let device_id = resolve_device(pool, question).await?;
    // Option 即迭代器(idiom 1.10):Some(id) → [id],None → []
    let locked: Vec<String> = device_id.clone().into_iter().collect();
    let scope = device_id
        .as_deref()
        .map_or_else(|| "全部设备".to_string(), |d| format!("设备 {d}"));
    let dev = device_id.as_deref();

    let (text, devices) = match intent {
        "query_alarm" => answer_alarm(pool, dev, question, &scope).await?,
        "query_luminance" => {
            (answer_luminance(pool, dev, question, &scope).await?, locked)
        }
        "query_threshold" => {
            (answer_threshold(pool, dev, &scope).await?, locked)
        }
        "query_device" => answer_devices(pool, dev, &scope).await?,
        "query_command" => {
            (answer_commands(pool, dev, question, &scope).await?, locked)
        }
        _ => (
            find_advice(pool, std::slice::from_ref(&question))
                .await?
                .unwrap_or_else(|| {
                    format!("没太理解您的问题。{KB_INTRO}")
                }),
            locked,
        ),
    };
    Ok(Answer { text, devices })
}

async fn answer_alarm(
    pool: &PgPool,
    device_id: Option<&str>,
    question: &str,
    scope: &str,
) -> Result<(String, Vec<String>), sqlx::Error> {
    let (start, desc) = parse_window(question, 7);
    // 未处理排前，方便管理员先看要修的；带出 device.location 用于定位
    let sql = if device_id.is_some() {
        "SELECT a.device_id, a.type, a.message, a.created_at, a.resolved_at, d.location \
         FROM alarm a LEFT JOIN device d ON d.id = a.device_id \
         WHERE a.created_at >= $1 AND a.device_id = $2 \
         ORDER BY (a.resolved_at IS NULL) DESC, a.created_at DESC LIMIT 20"
    } else {
        "SELECT a.device_id, a.type, a.message, a.created_at, a.resolved_at, d.location \
         FROM alarm a LEFT JOIN device d ON d.id = a.device_id \
         WHERE a.created_at >= $1 \
         ORDER BY (a.resolved_at IS NULL) DESC, a.created_at DESC LIMIT 20"
    };
    let rows: Vec<AlarmRow> =
        bind_opt_device(sqlx::query_as(sql).bind(start), device_id)
            .fetch_all(pool)
            .await?;
    if rows.is_empty() {
        return Ok((format!("{desc}，{scope}没有告警记录。"), Vec::new()));
    }
    let unhandled = rows.iter().filter(|r| r.resolved_at.is_none()).count();
    let mut lines = vec![format!(
        "{desc}，{scope}共 {} 条告警，未处理 {unhandled} 条（未处理排前）：",
        rows.len()
    )];
    for r in rows.iter().take(8) {
        let tag = if r.resolved_at.is_none() {
            "未处理"
        } else {
            "已处理"
        };
        let loc = r
            .location
            .as_deref()
            .filter(|l| !l.is_empty())
            .map_or_else(|| "位置-".to_string(), |l| format!("位置{l}"));
        lines.push(format!(
            "· {}（{loc}）{}（{tag}）{} {}",
            r.device_id,
            r.r#type,
            fmt_time(r.created_at),
            r.message
        ));
    }
    // 调修建议：告警类型/消息 + 提问原文一起匹配知识库（英文类型经 ALARM_TYPE_KW 翻译）
    let mut texts: Vec<&str> = vec![question];
    for r in &rows {
        texts.push(&r.r#type);
        texts.push(&r.message);
        if let Some((_, kw)) =
            ALARM_TYPE_KW.iter().find(|(t, _)| *t == r.r#type)
        {
            texts.push(kw);
        }
    }
    if let Some(adv) = find_advice(pool, &texts).await? {
        lines.push(format!("维护建议：{adv}"));
    }
    Ok((lines.join("\n"), collect_ids(&rows)))
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
) -> Result<(String, Vec<String>), sqlx::Error> {
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
        return Ok(("当前没有路灯设备。".to_string(), Vec::new()));
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
    // 全部返回，管理员可直接点标签跳到离线那盏
    let devices = rows.iter().map(|r| r.id.clone()).collect();
    Ok((lines.join("\n"), devices))
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
