//! 后端单元测试(纯逻辑,不依赖数据库与网络,`cargo test` 直接运行)。
//!
//! 覆盖:V11-HMAC-SHA256 衍生签名 KAT(锁死已对真实 `IoTDA` 验收的签名行为)、
//! 密码哈希、公开路径白名单、灯控/在线状态/影子属性的 serde 约定、
//! 错误状态码映射、时间参数解析与智能问答的意图/时间窗识别、
//! `IoTDA` 数据转发 webhook 的推送体解析。

use crate::api::{self, Error, LampAction};
use crate::assistant;
use crate::auth;
use crate::iothub::{self, OnlineStatus, ShadowProps};
use crate::webhook;
use axum::http::{Method, StatusCode};
use axum::response::IntoResponse;
use chrono::{DateTime, Duration, Utc};

// ---------------- IoTDA 北向签名与哈希 ----------------

#[test]
fn sha256_known_vectors() {
    assert_eq!(
        iothub::sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        iothub::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn hmac_sha256_rfc4231_case1() {
    // RFC 4231 Test Case 1:key = 0x0b * 20,data = "Hi There"
    assert_eq!(
        hex::encode(iothub::hmac_raw(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn v11_derived_sign_known_answer() {
    // 期望值由独立 Python 实现(标准库 hmac/hashlib,按 AGENTS.md 记录的
    // V11 衍生签名规范)生成,锁定"URI 补 '/' / 派生密钥 hex 字符串作 key /
    // 头部块与 SignedHeaders 之间多一个空行"等踩坑点,防回归。
    let creds = iothub::Credentials::new(
        "TESTAK123",
        "TESTSK456",
        "cn-south-1",
        "xxxx.st1.iotda-app.cn-south-1.myhuaweicloud.com",
    );
    let auth = iothub::sign_derived(
        &creds,
        "GET",
        "/v5/iot/test-project-id/devices/dev001/shadow",
        "20260825T101530Z",
        "",
    );
    assert_eq!(
        auth,
        "V11-HMAC-SHA256 Credential=TESTAK123/20260825/cn-south-1/iotdm, \
         SignedHeaders=content-type;host;x-sdk-date, \
         Signature=4a3e542368cc05be344f433613fd6aecdcb3d5fdc5b5df157bda33125569b990"
    );
}

#[test]
fn v11_derived_sign_put_with_body_known_answer() {
    // 修改属性(PUT + 非空 body)路径的 KAT:验证请求体参与签名
    let creds = iothub::Credentials::new(
        "TESTAK123",
        "TESTSK456",
        "cn-south-1",
        "xxxx.st1.iotda-app.cn-south-1.myhuaweicloud.com",
    );
    let auth = iothub::sign_derived(
        &creds,
        "PUT",
        "/v5/iot/test-project-id/devices/dev001/properties",
        "20260825T101530Z",
        r#"{"services":[{"service_id":"Light","properties":{"Threshold":40}}]}"#,
    );
    assert_eq!(
        auth,
        "V11-HMAC-SHA256 Credential=TESTAK123/20260825/cn-south-1/iotdm, \
         SignedHeaders=content-type;host;x-sdk-date, \
         Signature=74b411ff493db06059eebf45599b68693d8f19f7b584c556af353fd755a7f461"
    );
}

#[test]
fn v11_derived_sign_deterministic_and_format() {
    let creds =
        iothub::Credentials::new("AK", "SK", "cn-south-1", "host.example.com");
    let sign = |body: &str| {
        iothub::sign_derived(
            &creds,
            "POST",
            "/v5/iot/p/devices/d/commands",
            "20260825T000000Z",
            body,
        )
    };
    let a = sign(r#"{"paras":{"Led":"ON"}}"#);
    assert_eq!(a, sign(r#"{"paras":{"Led":"ON"}}"#)); // 同输入 → 同签名
    assert_ne!(a, sign(r#"{"paras":{"Led":"OFF"}}"#)); // body 参与签名
    assert!(a.starts_with(
        "V11-HMAC-SHA256 Credential=AK/20260825/cn-south-1/iotdm, \
         SignedHeaders=content-type;host;x-sdk-date, Signature="
    ));
    let sig = a.rsplit("Signature=").next().expect("signature 存在");
    assert_eq!(sig.len(), 64);
    assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(sig, sig.to_lowercase());
}

#[test]
fn is_retryable_only_for_network_error_or_5xx() {
    // None = 网络错误(无响应);5xx = 网关/服务端抖动,均可重试
    assert!(iothub::is_retryable(None));
    assert!(iothub::is_retryable(Some(StatusCode::INTERNAL_SERVER_ERROR)));
    assert!(iothub::is_retryable(Some(StatusCode::BAD_GATEWAY)));
    // 2xx 已成功;4xx 是请求/鉴权问题,重试无意义
    assert!(!iothub::is_retryable(Some(StatusCode::OK)));
    assert!(!iothub::is_retryable(Some(StatusCode::BAD_REQUEST)));
    assert!(!iothub::is_retryable(Some(StatusCode::UNAUTHORIZED)));
}

// ---------------- 设备 / 影子 / 灯控 serde 约定 ----------------

#[test]
fn online_status_deserialization() {
    use crate::iothub::OnlineStatus::{Offline, Online, Unknown};
    let parse = |s: &str| serde_json::from_str::<OnlineStatus>(s).unwrap();
    assert!(matches!(parse(r#""ONLINE""#), Online));
    assert!(matches!(parse(r#""OFFLINE""#), Offline));
    // 大小写敏感;IoTDA 的其他取值统一归 Unknown
    assert!(matches!(parse(r#""online""#), Unknown));
    assert!(matches!(parse(r#""UNKNOWN""#), Unknown));
    assert!(Online.is_online());
    assert!(!Offline.is_online());
    assert!(!Unknown.is_online());
}

#[test]
fn shadow_props_pascal_case_deserialize() {
    let p: ShadowProps =
        serde_json::from_str(r#"{"Luminance": 123, "LightStatus": "ON"}"#)
            .unwrap();
    assert_eq!(p.luminance, Some(123));
    assert_eq!(p.light_status.as_deref(), Some("ON"));

    // 字段缺省 / 显式 null → None
    let p: ShadowProps =
        serde_json::from_str(r#"{"Luminance": null}"#).unwrap();
    assert_eq!(p.luminance, None);
    assert_eq!(p.light_status, None);

    // 未知字段忽略(影子 JSON 可能带更多服务字段)
    let p: ShadowProps =
        serde_json::from_str(r#"{"Luminance": 7, "Extra": "ignored"}"#)
            .unwrap();
    assert_eq!(p.luminance, Some(7));
}

#[test]
fn lamp_action_lowercase_serde_and_iotda_mapping() {
    use crate::api::LampAction::{Auto, Off, On};
    let parse = |s: &str| serde_json::from_str::<LampAction>(s).unwrap();
    assert!(matches!(parse(r#""on""#), On));
    assert!(matches!(parse(r#""off""#), Off));
    assert!(matches!(parse(r#""auto""#), Auto));
    // API 只收小写;大写/非法值由 axum 拒收(与固件命令大写取值是相反方向)
    assert!(serde_json::from_str::<LampAction>(r#""ON""#).is_err());
    assert!(serde_json::from_str::<LampAction>(r#""blink""#).is_err());
    // 下发给 IoTDA 命令参数的大写取值
    assert_eq!(On.as_iotda_str(), "ON");
    assert_eq!(Off.as_iotda_str(), "OFF");
    assert_eq!(Auto.as_iotda_str(), "AUTO");
    assert_eq!(On.to_string(), "on");
    assert_eq!(Off.to_string(), "off");
    assert_eq!(Auto.to_string(), "auto");
}

// ---------------- 错误与参数处理 ----------------

#[test]
fn error_maps_to_http_status() {
    let status = |e: Error| e.into_response().status();
    assert_eq!(
        status(Error::Db(sqlx::Error::PoolTimedOut)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        status(Error::Iothub(anyhow::anyhow!("boom"))),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        status(Error::IothubUnavailable),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        status(Error::BadRequest("x".into())),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(Error::Unauthorized("x".into())),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(status(Error::Forbidden("x".into())), StatusCode::FORBIDDEN);
    assert_eq!(status(Error::NotFound("x".into())), StatusCode::NOT_FOUND);
    assert_eq!(status(Error::Conflict("x".into())), StatusCode::CONFLICT);
    assert_eq!(
        status(Error::Internal("x".into())),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn parse_ts_normalizes_to_utc() {
    let expect: DateTime<Utc> =
        DateTime::parse_from_rfc3339("2026-08-24T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
    let ok = api::parse_ts("from", "2026-08-24T10:00:00Z").unwrap();
    assert_eq!(ok, expect);
    // 带 +08:00 偏移 → 归一化为同一 UTC 时刻
    let ok = api::parse_ts("to", "2026-08-24T18:00:00+08:00").unwrap();
    assert_eq!(ok, expect);
    // 非法时间 → BadRequest,错误信息带参数名
    let err = api::parse_ts("to", "yesterday").unwrap_err();
    assert!(matches!(&err, Error::BadRequest(m) if m.contains("to")));
}

#[test]
fn clamp_limit_defaults_and_bounds() {
    assert_eq!(api::clamp_limit(None, 50, 500), 50);
    assert_eq!(api::clamp_limit(Some(0), 50, 500), 1);
    assert_eq!(api::clamp_limit(Some(-3), 50, 500), 1);
    assert_eq!(api::clamp_limit(Some(1000), 50, 500), 500);
    assert_eq!(api::clamp_limit(Some(7), 50, 500), 7);
    assert_eq!(api::clamp_limit(Some(500), 50, 500), 500);
}

// ---------------- 认证:密码哈希与公开路径 ----------------

#[test]
fn password_hash_roundtrip_and_salt() {
    let hash = auth::hash_password("s3cret-pw").unwrap();
    assert!(hash.starts_with("$argon2"));
    assert!(auth::verify_password("s3cret-pw", &hash));
    assert!(!auth::verify_password("wrong-pw", &hash));
    // 随机盐:同一明文两次哈希结果不同
    assert_ne!(hash, auth::hash_password("s3cret-pw").unwrap());
    // 非法哈希串:判定失败而不是 panic
    assert!(!auth::verify_password("s3cret-pw", "not-a-hash"));
}

#[test]
fn is_public_whitelist() {
    let get = Method::GET;
    let post = Method::POST;
    for p in [
        "/api/health",
        "/api/auth/login",
        "/docs",
        "/docs/",
        "/docs/swagger-ui",
        "/api/openapi.json",
        "/api/iotda/callback",
    ] {
        assert!(auth::is_public(p, &get), "{p} 应为公开路径");
    }
    // CORS 预检一律放行
    assert!(auth::is_public("/api/devices", &Method::OPTIONS));
    for p in [
        "/",
        "/api/devices",
        "/api/devices/dev1/lux/latest",
        "/api/alarms",
        "/api/dashboard",
        "/api/users",
        "/api/auth/me",
        "/docs2",
    ] {
        assert!(!auth::is_public(p, &get), "{p} 应要求认证");
    }
    // 公开路径的放行只看路径不看方法(锁定现有行为)
    assert!(auth::is_public("/api/health", &post));
}

// ---------------- 智能问答:意图 / 时间窗 / 时间格式 ----------------

#[test]
fn classify_intent_keyword_weighting() {
    assert_eq!(assistant::classify_intent("最近有哪些告警?"), "query_alarm");
    assert_eq!(
        assistant::classify_intent("3号灯是不是离线了"),
        "query_alarm"
    );
    assert_eq!(
        assistant::classify_intent("现在的光照强度是多少 lux"),
        "query_luminance"
    );
    assert_eq!(
        assistant::classify_intent("联动阈值设置是多少"),
        "query_threshold"
    );
    assert_eq!(
        assistant::classify_intent("1号灯现在是什么状态"),
        "query_device"
    );
    assert_eq!(
        assistant::classify_intent("查一下最近的控制记录"),
        "query_command"
    );
    assert_eq!(assistant::classify_intent("灯不亮应该怎么处理"), "advice");
    assert_eq!(assistant::classify_intent("现在几点"), "fallback");
}

#[test]
fn parse_window_recent_units_and_default() {
    // start 必落在 [调用前-7天, 调用后-7天] 内
    let before = Utc::now();
    let (start, desc) = assistant::parse_window("最近7天有没有告警", 7);
    let after = Utc::now();
    assert_eq!(desc, "最近7天");
    assert!(start >= before - Duration::days(7));
    assert!(start <= after - Duration::days(7));

    let (_, desc) = assistant::parse_window("最近3小时的光照", 1);
    assert_eq!(desc, "最近3小时");
    let (_, desc) = assistant::parse_window("最近30分钟内的指令", 7);
    assert_eq!(desc, "最近30分钟");
    let (_, desc) = assistant::parse_window("最近2周的指令", 7);
    assert_eq!(desc, "最近2周");

    // 无数字/单位 → 默认窗口
    let (_, desc) = assistant::parse_window("上个月的情况", 7);
    assert_eq!(desc, "最近7天");
}

#[test]
fn fmt_time_uses_month_day_clock() {
    let dt: DateTime<Utc> =
        DateTime::parse_from_rfc3339("2026-08-25T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
    assert_eq!(assistant::fmt_time(dt), "08-25 10:05");
    let dt: DateTime<Utc> =
        DateTime::parse_from_rfc3339("2026-12-01T00:05:59Z")
            .unwrap()
            .with_timezone(&Utc);
    assert_eq!(assistant::fmt_time(dt), "12-01 00:05");
}

// ---------------- IoTDA 数据转发 webhook(纯解析逻辑,不碰 DB) ----------------

#[test]
fn webhook_event_time_parse() {
    let expect: DateTime<Utc> =
        DateTime::parse_from_rfc3339("2026-08-26T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
    assert_eq!(
        webhook::parse_event_time("20260826T103000Z"),
        Some(expect)
    );
    // 非法串/格式不符 → None(调用方按不去重处理)
    assert_eq!(webhook::parse_event_time("garbage"), None);
    assert_eq!(webhook::parse_event_time("2026-08-26T10:30:00Z"), None);
    assert_eq!(webhook::parse_event_time(""), None);
}

#[test]
fn webhook_property_report_parse() {
    let body = serde_json::json!({
        "resource": "device.property",
        "event": "report",
        "event_time": "20260826T103000Z",
        "notify_data": {
            "header": {"app_id": "a1", "device_id": "header-fallback"},
            "body": {
                "device_id": "dev001",
                "services": [
                    {"service_id": "Other", "properties": {"X": 1}},
                    {
                        "service_id": "Light",
                        "properties": {"Luminance": 123, "LightStatus": "ON"},
                        "event_time": "20260826T103000Z"
                    }
                ]
            }
        }
    });
    let Some(webhook::NotifyEvent::Property {
        device_id,
        props,
        event_time,
    }) = webhook::parse_notification(&body)
    else {
        panic!("应解析为属性上报事件");
    };
    assert_eq!(device_id, "dev001");
    assert_eq!(props.luminance, Some(123));
    assert_eq!(props.light_status.as_deref(), Some("ON"));
    let expect: DateTime<Utc> =
        DateTime::parse_from_rfc3339("2026-08-26T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
    assert_eq!(event_time, Some(expect));

    // event_time 非法:属性照常解析,仅时间去重字段为 None
    let mut bad = body.clone();
    bad["notify_data"]["body"]["services"][1]["event_time"] =
        serde_json::json!("not-a-time");
    let Some(webhook::NotifyEvent::Property {
        props, event_time, ..
    }) = webhook::parse_notification(&bad)
    else {
        panic!("event_time 非法不应丢弃整条属性上报");
    };
    assert_eq!(props.luminance, Some(123));
    assert_eq!(event_time, None);

    // device_id 缺失时退回 header.device_id
    let mut header_only = body;
    header_only["notify_data"]["body"]
        .as_object_mut()
        .unwrap()
        .remove("device_id");
    let Some(webhook::NotifyEvent::Property { device_id, .. }) =
        webhook::parse_notification(&header_only)
    else {
        panic!("应退回 header.device_id");
    };
    assert_eq!(device_id, "header-fallback");
}

#[test]
fn webhook_status_notification_parse() {
    let make = |status: &str| {
        serde_json::json!({
            "resource": "device.status",
            "event": "update",
            "notify_data": {"body": {"device_id": "dev001", "status": status}}
        })
    };
    let Some(webhook::NotifyEvent::Status { device_id, online }) =
        webhook::parse_notification(&make("ONLINE"))
    else {
        panic!("应解析为状态事件");
    };
    assert_eq!(device_id, "dev001");
    assert!(online);

    let Some(webhook::NotifyEvent::Status { online, .. }) =
        webhook::parse_notification(&make("OFFLINE"))
    else {
        panic!("应解析为状态事件");
    };
    assert!(!online);

    // 未知状态值宽容忽略
    assert!(webhook::parse_notification(&make("FLYING")).is_none());
    // 缺 status / 缺 device_id → 忽略
    let mut no_status = make("ONLINE");
    no_status["notify_data"]["body"]
        .as_object_mut()
        .unwrap()
        .remove("status");
    assert!(webhook::parse_notification(&no_status).is_none());
    assert!(
        webhook::parse_notification(&serde_json::json!({
            "resource": "device.status",
            "notify_data": {"body": {"status": "ONLINE"}}
        }))
        .is_none()
    );
}

#[test]
fn webhook_unknown_resource_and_missing_fields_ignored() {
    // 不关心的 resource 一律忽略
    assert!(
        webhook::parse_notification(&serde_json::json!({
            "resource": "device.lifecycle",
            "event": "create",
            "notify_data": {"body": {"device_id": "dev001"}}
        }))
        .is_none()
    );
    // device.property 但 event 不是 report
    assert!(
        webhook::parse_notification(&serde_json::json!({
            "resource": "device.property",
            "event": "delete",
            "notify_data": {"body": {"device_id": "dev001", "services": []}}
        }))
        .is_none()
    );
    // 缺 notify_data / 缺 services / services 中无 Light 服务
    assert!(
        webhook::parse_notification(&serde_json::json!({
            "resource": "device.property", "event": "report"
        }))
        .is_none()
    );
    assert!(
        webhook::parse_notification(&serde_json::json!({
            "resource": "device.property",
            "event": "report",
            "notify_data": {"body": {"device_id": "dev001"}}
        }))
        .is_none()
    );
    assert!(
        webhook::parse_notification(&serde_json::json!({
            "resource": "device.property",
            "event": "report",
            "notify_data": {"body": {
                "device_id": "dev001",
                "services": [{"service_id": "Other", "properties": {}}]
            }}
        }))
        .is_none()
    );
    // 空 JSON / 非预期类型均不 panic
    assert!(webhook::parse_notification(&serde_json::json!({})).is_none());
    assert!(webhook::parse_notification(&serde_json::json!([])).is_none());
    assert!(webhook::parse_notification(&serde_json::json!("x")).is_none());
}
