//! OpenAPI/Swagger 文档:汇总各模块带 `#[utoipa::path]` 注解的 handler
//! 新增的api请放在这里
use crate::{api, auth, webhook};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "智慧路灯后端 API",
        version = "0.1.0",
        description = "设备 / 光照 / 控灯 / 阈值 / 告警 / 指令留痕 / 账号与 RBAC / 维护智能问答。\
                      受保护接口先调用 POST /api/auth/login 拿到 token,再点右上角 Authorize 填入。"
    ),
    paths(
        api::health,
        api::list_devices,
        api::create_device,
        api::update_device,
        api::delete_device,
        api::lux_latest,
        api::lux_history,
        api::lux_stats,
        api::global_lux_latest,
        api::map_devices,
        api::set_lamp,
        api::get_threshold,
        api::put_threshold,
        api::list_device_commands,
        api::list_global_commands,
        api::list_alarms,
        api::patch_alarm,
        api::list_audit_logs,
        api::dashboard,
        api::assistant_ask,
        auth::login,
        auth::me,
        auth::list_users,
        auth::create_user,
        auth::update_user,
        auth::delete_user,
        auth::list_roles,
        auth::list_permissions,
        auth::get_role_permissions,
        auth::update_role_permissions,
        webhook::iotda_callback
    )
)]
struct ApiDoc;

/// 生成 `OpenAPI` 文档并补上 Bearer 安全方案(供 Swagger UI 的 Authorize 使用)
pub fn openapi() -> utoipa::openapi::OpenApi {
    use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
    let mut doc = ApiDoc::openapi();
    doc.components
        .get_or_insert_with(Default::default)
        .add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
    doc
}
