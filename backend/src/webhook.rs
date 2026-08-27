//! `IoTDA` 数据转发(HTTP 推送)回调入口,推送为主、轮询兜底校准。
//!
//! **本端点免 JWT、无认证(用户明确决策)**:知道路径即可伪造上报/状态数据。
//! 云部署后建议在 `IoTDA` 转发规则上配置自定义 Header,此处再加
//! `IOTDA_WEBHOOK_TOKEN` 校验(暂未实现,见部署文档)。
//!
//! 协议约定:任何情况下都尽快返回 200——`IoTDA` 超时未收到 200 会重推,
//! 解析失败/设备未注册/入库失败一律只记日志,不影响响应码。

use crate::AppState;
use crate::iothub::{self, OnlineStatus, ShadowProps, parse_event_time};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde_json::Value;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/iotda/callback", post(iotda_callback))
        .with_state(state)
}

/// 推送消息解析结果(宽容解析:任一关键字段缺失/非法即整体 None,调用方忽略)
#[derive(Debug)]
pub enum NotifyEvent {
    /// 属性上报(device.property / report):`event_time` 解析失败为 None,
    /// 表示入库不按 (`device_id`, `created_at`) 去重
    Property {
        device_id: String,
        props: ShadowProps,
        event_time: Option<DateTime<Utc>>,
    },
    /// 在线状态变化(device.status)
    Status { device_id: String, online: bool },
}

/// 按顶层 `resource` 字段分发解析推送体(纯函数,便于单测)
pub fn parse_notification(v: &Value) -> Option<NotifyEvent> {
    match v.get("resource")?.as_str()? {
        "device.property" => parse_property_report(v),
        "device.status" => parse_status_change(v),
        _ => None,
    }
}

#[utoipa::path(
    post,
    path = "/api/iotda/callback",
    request_body(content = serde_json::Value, description = "IoTDA 数据转发推送原始 JSON"),
    responses((status = 200, description = "已接收(解析失败/设备未注册同样返回 200,避免 IoTDA 重推)"))
)]
async fn iotda_callback(
    State(s): State<AppState>,
    Json(body): Json<Value>,
) -> StatusCode {
    let Some(event) = parse_notification(&body) else {
        tracing::warn!("IoTDA 回调忽略(无法解析或不关心的 resource): {body}");
        return StatusCode::OK;
    };
    if let Err(e) = handle_event(&s.db, event).await {
        tracing::error!("IoTDA 回调处理失败: {e:#}");
    }
    StatusCode::OK
}

/// 设备注册检查 + 分发到与轮询共用的入库/状态翻转逻辑
async fn handle_event(
    db: &sqlx::PgPool,
    event: NotifyEvent,
) -> anyhow::Result<()> {
    match event {
        NotifyEvent::Property {
            device_id,
            props,
            event_time,
        } => {
            if device_registered(db, &device_id).await? {
                iothub::apply_shadow_props(db, &device_id, &props, event_time)
                    .await?;
                // 真实上报即活性证据:直接翻回在线,不等下一个轮询 tick
                // (轮询间隔 60s 时恢复延迟可达 60s;门控保证心跳新鲜才生效)
                iothub::apply_online_status(db, &device_id, true).await
            } else {
                tracing::debug!("IoTDA 属性上报忽略:设备 {device_id} 未注册");
                Ok(())
            }
        }
        NotifyEvent::Status { device_id, online } => {
            if device_registered(db, &device_id).await? {
                iothub::apply_online_status(db, &device_id, online).await
            } else {
                tracing::debug!("IoTDA 状态推送忽略:设备 {device_id} 未注册");
                Ok(())
            }
        }
    }
}

async fn device_registered(
    db: &sqlx::PgPool,
    device_id: &str,
) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM device WHERE id=$1)",
    )
    .bind(device_id)
    .fetch_one(db)
    .await?)
}

/// 属性上报:`event` 必须是 "report";取 services 中 `service_id == "Light"` 的项
fn parse_property_report(v: &Value) -> Option<NotifyEvent> {
    if v.get("event")?.as_str()? != "report" {
        return None;
    }
    let notify = v.get("notify_data")?;
    let device_id = device_id_of(notify)?.to_string();
    let light = notify
        .get("body")?
        .get("services")?
        .as_array()?
        .iter()
        .find(|s| {
            s.get("service_id").and_then(Value::as_str) == Some("Light")
        })?;
    let props: ShadowProps =
        serde_json::from_value(light.get("properties")?.clone()).ok()?;
    let event_time = light
        .get("event_time")
        .and_then(Value::as_str)
        .and_then(parse_event_time);
    Some(NotifyEvent::Property {
        device_id,
        props,
        event_time,
    })
}

/// 在线状态推送:状态值宽容解析,未知值忽略
fn parse_status_change(v: &Value) -> Option<NotifyEvent> {
    let notify = v.get("notify_data")?;
    let device_id = device_id_of(notify)?.to_string();
    let status: OnlineStatus =
        serde_json::from_value(notify.get("body")?.get("status")?.clone())
            .ok()?;
    match status {
        OnlineStatus::Online => Some(NotifyEvent::Status {
            device_id,
            online: true,
        }),
        OnlineStatus::Offline => Some(NotifyEvent::Status {
            device_id,
            online: false,
        }),
        OnlineStatus::Unknown => None,
    }
}

/// 设备 ID 宽容取值:优先 `body.device_id`,退回 `header.device_id`
fn device_id_of(notify: &Value) -> Option<&str> {
    notify
        .get("body")
        .and_then(|b| b.get("device_id"))
        .or_else(|| notify.get("header").and_then(|h| h.get("device_id")))
        .and_then(Value::as_str)
}
