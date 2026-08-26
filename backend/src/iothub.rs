use futures::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::AppState;
use crate::api::LampAction;

type HmacSha256 = Hmac<Sha256>;

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// HMAC-SHA256 原始输出(接受任意长度 key)
///
/// # Panics
/// HMAC-SHA256 对任意长度 key 均可用,此函数实际不会 panic。
pub fn hmac_raw(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// 设备密钥(SK)的不透明包装:防止 Debug/Display 意外打印密钥
#[derive(Clone)]
pub struct SecretKey(String);

impl SecretKey {
    const fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(***)")
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// 北向签名凭据:AK/SK + 区域 + 应用侧域名,四者总是一起使用
#[derive(Clone)]
pub struct Credentials {
    pub ak: String,
    pub sk: SecretKey,
    pub region: String,
    pub host: String,
}

impl Credentials {
    pub fn new(
        ak: impl Into<String>,
        sk: impl Into<String>,
        region: impl Into<String>,
        host: impl Into<String>,
    ) -> Self {
        Self {
            ak: ak.into(),
            sk: SecretKey(sk.into()),
            region: region.into(),
            host: host.into(),
        }
    }
}

/// 华为云 `IoTDA` 北向 API 客户端(AK/SK 签名认证,算法 V11-HMAC-SHA256 衍生签名;标准版/企业版实例必须)
///
/// 端点为实例级域名(cn-south-1 等区域无共享域名),
/// 通过 `HUAWEI_IOTDA_ENDPOINT` 配置,形如 xxx.st1.iotda-app.cn-south-1.myhuaweicloud.com
pub struct IothubClient {
    creds: Credentials,
    project_id: String,
    http: reqwest::Client,
    /// 压测/联调模式(`IOTHUB_DRY_RUN=true`):所有北向操作本地短路,不发任何真实请求。
    /// 命令/阈值下发直接返回成功,影子返回空,设备状态固定 Online(不产生离线告警)。
    dry_run: bool,
}

/// Light 服务上报的属性(产品模型:`Luminance` int + `LightStatus` string)
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ShadowProps {
    pub luminance: Option<i32>,
    pub light_status: Option<String>,
}

/// 设备在线状态(`IoTDA` 返回 "ONLINE"/"OFFLINE",其余归 Unknown)
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OnlineStatus {
    Online,
    Offline,
    #[serde(other)]
    Unknown,
}

impl OnlineStatus {
    #[must_use]
    pub const fn is_online(self) -> bool {
        matches!(self, Self::Online)
    }
}

/// 设备影子响应的类型化视图(只关心 Light 服务的 reported properties)
#[derive(serde::Deserialize)]
struct ShadowResponse {
    shadow: Vec<ShadowService>,
}

#[derive(serde::Deserialize)]
struct ShadowService {
    service_id: String,
    reported: Reported,
}

#[derive(serde::Deserialize)]
struct Reported {
    properties: ShadowProps,
}

#[derive(serde::Deserialize)]
struct DeviceInfo {
    status: OnlineStatus,
}

/// 非 2xx 时把状态码与响应体(华为云错误信息在 body 里)带进错误
async fn ensure_success(
    resp: reqwest::Response,
) -> anyhow::Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("{status}: {}", body.trim());
}

/// 是否值得重试:None(网络错误,无响应)或 5xx(网关/服务端抖动)→ true;
/// 4xx 是请求/鉴权本身有问题,重试无意义
pub fn is_retryable(status: Option<reqwest::StatusCode>) -> bool {
    status.is_none_or(|s| s.is_server_error())
}

impl IothubClient {
    pub fn from_env() -> anyhow::Result<Option<Arc<Self>>> {
        fn env_var(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.is_empty())
        }
        // Option 的 collect 语义:任一变量缺失即整体短路为 None
        let Some(cfg) = [
            "HUAWEI_AK",
            "HUAWEI_SK",
            "HUAWEI_PROJECT_ID",
            "HUAWEI_IOTDA_ENDPOINT",
        ]
        .into_iter()
        .map(env_var)
        .collect::<Option<Vec<_>>>() else {
            tracing::warn!("HUAWEI_* 环境变量未配置,IoTDA 北向功能停用");
            return Ok(None);
        };
        let [ak, sk, project_id, endpoint] =
            <[String; 4]>::try_from(cfg).expect("statically 4 keys");
        let endpoint = endpoint
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string();
        // V11 衍生签名需要区域名:优先 HUAWEI_IOTDA_REGION,否则从域名推断(如 cn-south-1)
        let region = env_var("HUAWEI_IOTDA_REGION")
            .or_else(|| {
                endpoint
                    .split('.')
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find(|w| w[1] == "myhuaweicloud.com")
                    .map(|w| w[0].to_string())
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "无法确定 IoTDA 区域,请设置 HUAWEI_IOTDA_REGION"
                )
            })?;
        Ok(Some(Arc::new(Self {
            http: reqwest::Client::builder()
                // IoTDA 下发命令会同步等设备响应(默认约 20s),超时短于此会误判失败
                .timeout(Duration::from_secs(35))
                .build()?,
            creds: Credentials::new(ak, sk, region, endpoint),
            project_id,
            dry_run: matches!(
                env_var("IOTHUB_DRY_RUN").as_deref(),
                Some("1" | "true" | "TRUE" | "yes" | "on")
            ),
        })))
    }

    fn host(&self) -> &str {
        &self.creds.host
    }

    fn path_of(&self, path: &str) -> String {
        format!("/v5/iot/{}{}", self.project_id, path)
    }

    fn sign(
        &self,
        method: &str,
        uri: &str,
        sdk_date: &str,
        body: &str,
    ) -> String {
        sign_derived(&self.creds, method, uri, sdk_date, body)
    }

    fn signed_headers(
        &self,
        method: &str,
        path: &str,
        body: &str,
    ) -> HeaderMap {
        let sdk_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let uri = self.path_of(path);
        let auth = self.sign(method, &uri, &sdk_date, body);
        let value =
            |v: &str| HeaderValue::from_str(v).expect("valid header value");
        [
            (
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (reqwest::header::HOST, value(self.host())),
            (HeaderName::from_static("x-sdk-date"), value(&sdk_date)),
            (reqwest::header::AUTHORIZATION, value(&auth)),
        ]
        .into_iter()
        .collect()
    }

    /// 单次请求最多尝试 3 次:网络错误与 5xx 失败后分别退避 1s、2s 重试;
    /// 4xx 把响应体带进错误信息后立即返回,不重试
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> anyhow::Result<reqwest::Response> {
        const MAX_ATTEMPTS: u64 = 3;
        let raw = body.map(|b| b.to_string());
        let mut last_err = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(attempt)).await;
            }
            let headers = self.signed_headers(
                method.as_str(),
                path,
                raw.as_deref().unwrap_or_default(),
            );
            let url = format!("https://{}{}", self.host(), self.path_of(path));
            let req = self.http.request(method.clone(), url).headers(headers);
            let req = match &raw {
                Some(raw) => req.body(raw.clone()),
                None => req,
            };
            let (err, retryable) = match req.send().await {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) => {
                    let retryable = is_retryable(Some(resp.status()));
                    // 此分支已排除 2xx,ensure_success 必然返回 Err
                    let err = ensure_success(resp)
                        .await
                        .expect_err("2xx 已在上面返回");
                    (err, retryable)
                }
                Err(e) => {
                    let retryable = is_retryable(e.status());
                    (e.into(), retryable)
                }
            };
            if !retryable {
                return Err(err);
            }
            last_err = Some(err);
        }
        Err(last_err.expect("循环至少执行一次"))
    }

    /// 查询设备影子,返回 Light 服务上报的属性
    pub async fn shadow(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<ShadowProps>> {
        if self.dry_run {
            tracing::debug!("dry-run: shadow({device_id}) stubbed");
            return Ok(None);
        }
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/devices/{device_id}/shadow"),
                None,
            )
            .await?;
        Ok(resp
            .json::<ShadowResponse>()
            .await?
            .shadow
            .into_iter()
            .find(|s| s.service_id == "Light")
            .map(|s| s.reported.properties))
    }

    /// 查询设备在线状态
    pub async fn device_status(
        &self,
        device_id: &str,
    ) -> anyhow::Result<OnlineStatus> {
        if self.dry_run {
            tracing::debug!("dry-run: device_status({device_id}) stubbed");
            return Ok(OnlineStatus::Online);
        }
        let resp = self
            .request(
                reqwest::Method::GET,
                &format!("/devices/{device_id}"),
                None,
            )
            .await?;
        Ok(resp.json::<DeviceInfo>().await?.status)
    }

    /// 下发 `Light_Control_Led` 命令
    pub async fn control_led(
        &self,
        device_id: &str,
        action: LampAction,
    ) -> anyhow::Result<()> {
        if self.dry_run {
            tracing::debug!(
                "dry-run: control_led({device_id}, {}) stubbed",
                action.as_iotda_str()
            );
            return Ok(());
        }
        let body = serde_json::json!({
            "service_id": "Light",
            "command_name": "Light_Control_Led",
            "paras": { "Led": action.as_iotda_str() }
        });
        self.request(
            reqwest::Method::POST,
            &format!("/devices/{device_id}/commands"),
            Some(body),
        )
        .await?;
        Ok(())
    }

    /// 设置 Threshold 可写属性
    pub async fn set_threshold(
        &self,
        device_id: &str,
        threshold: i32,
    ) -> anyhow::Result<()> {
        if self.dry_run {
            tracing::debug!(
                "dry-run: set_threshold({device_id}, {threshold}) stubbed"
            );
            return Ok(());
        }
        let body = serde_json::json!({
            "services": [{
                "service_id": "Light",
                "properties": { "Threshold": threshold }
            }]
        });
        self.request(
            reqwest::Method::PUT,
            &format!("/devices/{device_id}/properties"),
            Some(body),
        )
        .await?;
        Ok(())
    }
}

/// 按华为云 V11-HMAC-SHA256 衍生签名算法生成 Authorization 头(纯函数)
///
/// `IoTDA` 标准版/企业版实例必须使用衍生签名(官方 SDK: `WithDerivedPredicate`);
/// 基础版共享域名才用旧版 SDK-HMAC-SHA256。与官方 SDK `derived_signer.go` 对齐:
/// 1. info = {YYYYMMDD}/{region}/iotdm,service 固定 "iotdm"
/// 2. 派生密钥 = HKDF(SHA-256, ikm=SK, salt=AK, info=info) 的 32 字节,再 hex 编码后作为 HMAC key
/// 3. 规范 URI 以 '/' 结尾、规范头部块与 `SignedHeaders` 之间多一个空行(与旧算法相同)
///
/// # Panics
/// `sdk_date` 短于 8 字节(YYYYMMDD 前缀)时 panic;调用方保证传入合法日期串。
pub fn sign_derived(
    creds: &Credentials,
    method: &str,
    uri: &str,
    sdk_date: &str,
    body: &str,
) -> String {
    let signed_headers = "content-type;host;x-sdk-date";
    let canonical_headers = format!(
        "content-type:application/json\nhost:{}\nx-sdk-date:{sdk_date}\n",
        creds.host
    );
    let canonical_request = format!(
        "{}\n{}/\n\n{}\n{}\n{}",
        method,
        uri,
        canonical_headers,
        signed_headers,
        sha256_hex(body.as_bytes())
    );
    let info = format!("{}/{}/iotdm", &sdk_date[..8], creds.region);
    // HKDF-Extract: PRK = HMAC(salt=AK, data=SK); HKDF-Expand: T1 = HMAC(key=PRK, data=info||0x01)
    let prk = hmac_raw(creds.ak.as_bytes(), creds.sk.as_bytes());
    let mut expand = info.as_bytes().to_vec();
    expand.push(0x01);
    let derived_key_hex = hex::encode(hmac_raw(&prk, &expand));
    let string_to_sign = format!(
        "V11-HMAC-SHA256\n{sdk_date}\n{info}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let mut mac = HmacSha256::new_from_slice(derived_key_hex.as_bytes())
        .expect("hmac accepts any key length");
    mac.update(string_to_sign.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    format!(
        "V11-HMAC-SHA256 Credential={}/{info}, SignedHeaders={signed_headers}, Signature={signature}",
        creds.ak
    )
}

/// 轮询任务:周期性并发拉取各设备影子/状态入库
pub async fn run(state: AppState, iothub: Arc<IothubClient>) {
    // 轮询间隔可用 IOTDA_POLL_INTERVAL_SECS 覆盖,未设置/非法/为 0 时默认 8 秒
    let interval_secs = std::env::var("IOTDA_POLL_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(8);
    tracing::info!("iothub poll interval: {interval_secs}s");
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    // 单轮轮询超时(如设备多/网络慢)时顺延而不是补打,避免请求叠加
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let devices: Vec<(String,)> =
            match sqlx::query_as("SELECT id FROM device")
                .fetch_all(&state.db)
                .await
            {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("list devices failed: {e}");
                    continue;
                }
            };
        futures::stream::iter(devices)
            // 并发上限 8:设备多时避免同时打出几十个 HTTPS 请求
            .for_each_concurrent(8, |(device_id,)| {
                let state = &state;
                let iothub = &iothub;
                async move {
                    if let Err(e) = poll_device(state, iothub, &device_id).await
                    {
                        tracing::warn!("poll {device_id} failed: {e:#}");
                    }
                }
            })
            .await;
    }
}

/// 应用设备在线状态(轮询与后续 webhook 推送共用):
/// 状态翻转时更新 `device.status` 并产生/消解离线告警;在线时刷新 `last_seen_at`
pub async fn apply_online_status(
    db: &sqlx::PgPool,
    device_id: &str,
    online: bool,
) -> anyhow::Result<()> {
    // 在线状态变化 → 告警产生/消解
    let changed: Option<(String,)> = sqlx::query_as(
        "UPDATE device SET status=$2 WHERE id=$1 AND status!=$2 RETURNING id",
    )
    .bind(device_id)
    .bind(if online { "online" } else { "offline" })
    .fetch_optional(db)
    .await?;
    if changed.is_some() {
        if online {
            sqlx::query(
                "UPDATE alarm SET resolved_at=now() \
                 WHERE device_id=$1 AND type='offline' AND resolved_at IS NULL",
            )
            .bind(device_id)
            .execute(db)
            .await?;
        } else {
            tracing::warn!("device {device_id} offline");
            sqlx::query(
                "INSERT INTO alarm (device_id, type, message) VALUES ($1, 'offline', '设备离线')",
            )
            .bind(device_id)
            .execute(db)
            .await?;
        }
    }
    if online {
        sqlx::query("UPDATE device SET last_seen_at=now() WHERE id=$1")
            .bind(device_id)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// 影子属性入库(轮询与后续 webhook 推送共用):光照写入历史库,灯态更新到设备表。
/// `recorded_at` 为 None 时 `created_at` 用数据库默认 now();为 Some 时显式写入,
/// 并按 (`device_id`, `created_at`) 去重(webhook 重复推送不产生重复行)
pub async fn apply_shadow_props(
    db: &sqlx::PgPool,
    device_id: &str,
    props: &ShadowProps,
    recorded_at: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<()> {
    if let Some(lux) = props.luminance {
        if let Some(ts) = recorded_at {
            sqlx::query(
                "INSERT INTO lux_record (device_id, lux, created_at) SELECT $1, $2, $3 \
                 WHERE NOT EXISTS ( \
                     SELECT 1 FROM lux_record WHERE device_id=$1 AND created_at=$3)",
            )
            .bind(device_id)
            .bind(lux)
            .bind(ts)
            .execute(db)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO lux_record (device_id, lux) VALUES ($1, $2)",
            )
            .bind(device_id)
            .bind(lux)
            .execute(db)
            .await?;
        }
    }
    if let Some(lamp) = &props.light_status {
        sqlx::query("UPDATE device SET lamp=$2 WHERE id=$1")
            .bind(device_id)
            .bind(lamp.to_lowercase())
            .execute(db)
            .await?;
    }
    Ok(())
}

async fn poll_device(
    state: &AppState,
    iothub: &IothubClient,
    device_id: &str,
) -> anyhow::Result<()> {
    // 先查在线状态:离线时影子保留最后上报值,不能直接当实时数据入库
    let online = iothub.device_status(device_id).await?.is_online();

    apply_online_status(&state.db, device_id, online).await?;

    if !online {
        return Ok(());
    }

    // 影子属性 → 历史库 + 灯态
    if let Some(props) = iothub.shadow(device_id).await? {
        apply_shadow_props(&state.db, device_id, &props, None).await?;
    }
    Ok(())
}
