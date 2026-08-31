use futures::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::AppState;
use crate::api::LampAction;
use crate::notify;

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

/// 影子查询结果:Light 服务上报属性 + 平台事件时间(在线心跳源)
pub struct ShadowReport {
    pub props: ShadowProps,
    /// `IoTDA` 平台接收上报时打的时间戳(`yyyyMMdd'T'HHmmss'Z'`),解析失败为 None
    pub event_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(serde::Deserialize)]
struct Reported {
    properties: ShadowProps,
    #[serde(default)]
    event_time: Option<String>,
}

/// 解析 `IoTDA` 事件时间(格式 `yyyyMMdd'T'HHmmss'Z'`,如 `20260826T103000Z`),
/// 非法串返回 None(调用方按不去重、不刷新心跳处理)
pub fn parse_event_time(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|dt| dt.and_utc())
}

#[derive(serde::Deserialize)]
struct DeviceInfo {
    status: OnlineStatus,
}

/// 灵活搜索设备列表(`POST /search/query-devices`)的响应视图:
/// `devices` = 本页条目,`count` = 满足条件的总条数(华为云 SDK `SearchDevicesResponse`)
#[derive(serde::Deserialize)]
struct SearchDevicesResponse {
    #[serde(default)]
    devices: Vec<DeviceListItem>,
    #[serde(default)]
    count: i64,
}

#[derive(serde::Deserialize)]
struct DeviceListItem {
    device_id: String,
    #[serde(default)]
    device_name: Option<String>,
}

/// SQL 检索单页行数(华为云 API 上限 50)
const SEARCH_PAGE_ROWS: u32 = 50;
/// SQL 检索 offset 硬上限(华为云 API 限制 500)
const MAX_SQL_OFFSET: u32 = 500;

/// 设备自动同步默认间隔:30 分钟
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 1800;

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

/// 布尔型环境变量:`1`/`true`/`TRUE`/`yes`/`on` 为真,其余(含未设置)为假
fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "on")
    )
}

/// 读取正整数环境变量,缺失/非法/为 0 时回落默认值
fn env_u64_secs(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&s| s > 0)
        .unwrap_or(default)
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
            dry_run: env_flag("IOTHUB_DRY_RUN"),
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

    /// 查询设备影子,返回 Light 服务上报的属性与平台事件时间(心跳源)
    pub async fn shadow(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<ShadowReport>> {
        if self.dry_run {
            tracing::debug!("dry-run: shadow({device_id}) stubbed");
            // 心跳保鲜:维持 dry-run "设备固定 Online、不产生离线告警"的约定
            return Ok(Some(ShadowReport {
                props: ShadowProps {
                    luminance: None,
                    light_status: None,
                },
                event_time: Some(chrono::Utc::now()),
            }));
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
            .map(|s| ShadowReport {
                props: s.reported.properties,
                event_time: s
                    .reported
                    .event_time
                    .as_deref()
                    .and_then(parse_event_time),
            }))
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

    /// 分页拉取项目下设备列表(复用 `request` 的 3 次重试退避)。
    /// `product_id`:Some 时只查该产品,None 查项目全部。
    ///
    /// 用 `POST /search/query-devices` 的类 SQL 检索而非 `GET /devices?limit&offset`:
    /// 签名器假设规范 URI 无 query string(路径末尾无条件补 `/`),改签名会牵动
    /// KAT 锁死的 `sign_derived`;SQL 检索参数全在 body 里,完全绕开。
    /// 代价:offset 上限 500、单页 50,超过约 550 台会报错(本项目远达不到)。
    async fn list_devices(
        &self,
        product_id: Option<&str>,
    ) -> anyhow::Result<Vec<DeviceListItem>> {
        // product_id 仅允许字母/数字/下划线/连接符且 ≤36 字符(华为云字段规范),
        // 校验后拼进类 SQL,拒绝一切注入式字符
        let where_clause = match product_id {
            Some(pid) if !pid.is_empty() => {
                let valid = pid.len() <= 36
                    && pid
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
                if !valid {
                    anyhow::bail!(
                        "IOTDA_SYNC_PRODUCT_ID 非法:仅允许字母/数字/下划线/连接符,≤36 字符"
                    );
                }
                format!(" where product_id = '{pid}'")
            }
            _ => String::new(),
        };
        let mut all = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let sql = format!(
                "select * from device{where_clause} limit {offset},{SEARCH_PAGE_ROWS}"
            );
            let resp = self
                .request(
                    reqwest::Method::POST,
                    "/search/query-devices",
                    Some(serde_json::json!({ "sql": sql })),
                )
                .await?;
            let page: SearchDevicesResponse = resp.json().await?;
            let page_len = page.devices.len();
            all.extend(page.devices);
            offset += SEARCH_PAGE_ROWS;
            // count = 满足条件的总条数;offset 取尽或本页不足一页即收尾
            if offset >= page.count as u32 || page_len < SEARCH_PAGE_ROWS as usize {
                break;
            }
            if offset > MAX_SQL_OFFSET {
                anyhow::bail!(
                    "设备数超过 SQL 检索上限(offset 最大 {MAX_SQL_OFFSET}),请改用其他列表方式"
                );
            }
        }
        Ok(all)
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

    /// 设置调光可写属性(Brightness 手动亮度 / `DimCurve` 照度曲线,只放下出现的键)
    pub async fn set_dimming(
        &self,
        device_id: &str,
        brightness: Option<i32>,
        dim_curve: Option<&str>,
    ) -> anyhow::Result<()> {
        if self.dry_run {
            tracing::debug!(
                "dry-run: set_dimming({device_id}, {brightness:?}, {dim_curve:?}) stubbed"
            );
            return Ok(());
        }
        let mut properties = serde_json::Map::new();
        if let Some(b) = brightness {
            properties.insert("Brightness".to_string(), b.into());
        }
        if let Some(c) = dim_curve {
            properties.insert("DimCurve".to_string(), c.into());
        }
        let body = serde_json::json!({
            "services": [{
                "service_id": "Light",
                "properties": serde_json::Value::Object(properties)
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

/// 失联判定窗口(秒):心跳(`last_seen_at` = 最后一条上报的平台事件时间)
/// 超过该窗口未前进,即使 `IoTDA` 状态 API 仍报 ONLINE 也本地标记离线。
/// 本地失联检测与"转在线门控"(见 `apply_online_status`)共用此常量。
pub const OFFLINE_TIMEOUT_SECS: i64 = 90;

/// 本地失联检测(与 `IoTDA` 状态 API 解耦):心跳超过 `OFFLINE_TIMEOUT_SECS`
/// 未前进的在线设备直接标记离线并产生告警(去重),不等待 MQTT 超时判定。
/// 返回本次被标记离线的设备 id。
async fn mark_stale_devices_offline(db: &sqlx::PgPool) -> Vec<String> {
    let newly_offline: Vec<(String,)> = match sqlx::query_as(
        "UPDATE device SET status='offline' \
         WHERE status='online' \
         AND last_seen_at < now() - $1 * interval '1 second' \
         RETURNING id",
    )
    .bind(OFFLINE_TIMEOUT_SECS)
    .fetch_all(db)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("local timeout check failed: {e}");
            return Vec::new();
        }
    };
    for (device_id,) in &newly_offline {
        tracing::warn!("device {device_id} offline (local timeout)");
        // 产生离线告警(避免重复)
        if let Err(e) = sqlx::query(
            "INSERT INTO alarm (device_id, type, message) \
             SELECT $1, 'offline', '设备离线' \
             WHERE NOT EXISTS ( \
                 SELECT 1 FROM alarm \
                 WHERE device_id=$1 AND type='offline' AND resolved_at IS NULL)",
        )
        .bind(device_id)
        .execute(db)
        .await
        {
            tracing::error!("insert offline alarm for {device_id} failed: {e}");
        }
    }
    newly_offline.into_iter().map(|(id,)| id).collect()
}

/// 轮询任务:周期性并发拉取各设备影子/状态入库
pub async fn run(state: AppState, iothub: Arc<IothubClient>) {
    // 轮询间隔可用 IOTDA_POLL_INTERVAL_SECS 覆盖,未设置/非法/为 0 时默认 8 秒
    let interval_secs = env_u64_secs("IOTDA_POLL_INTERVAL_SECS", 8);
    tracing::info!("iothub poll interval: {interval_secs}s");
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    // 单轮轮询超时(如设备多/网络慢)时顺延而不是补打,避免请求叠加
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // 设备自动同步(可选):把华为云新增设备自动注册进本地 device 表 + 通知提醒。
    // 间隔与轮询 tick 解耦(默认 30 分钟),启动即先跑一次,兼作配置自检。
    let auto_sync = env_flag("IOTDA_AUTO_SYNC_DEVICES");
    let sync_interval = Duration::from_secs(env_u64_secs(
        "IOTDA_SYNC_INTERVAL_SECS",
        DEFAULT_SYNC_INTERVAL_SECS,
    ));
    let sync_product = std::env::var("IOTDA_SYNC_PRODUCT_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if auto_sync {
        tracing::info!(
            "device auto-sync enabled (interval: {sync_interval:?}, product filter: {sync_product:?})"
        );
    }
    let mut last_sync: Option<tokio::time::Instant> = None;
    loop {
        ticker.tick().await;

        // 首次 tick 立即同步(自检),之后按固定间隔;失败只记日志,不影响轮询
        if auto_sync && last_sync.is_none_or(|t| t.elapsed() >= sync_interval) {
            last_sync = Some(tokio::time::Instant::now());
            sync_devices(&state, &iothub, sync_product.as_deref()).await;
        }

        mark_stale_devices_offline(&state.db).await;

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
/// 状态翻转时更新 `device.status` 并产生/消解离线告警。
///
/// 心跳约定:`last_seen_at` 只由数据上报刷新(`apply_shadow_props`),
/// 不在这里刷新——状态观测(`device_status`)在设备断连后的 MQTT 宽限期
/// (60-120s)内仍报 ONLINE,若用它刷新心跳,本地失联检测将永远抢不过它。
///
/// 转在线受心跳新鲜度门控:最近 `OFFLINE_TIMEOUT_SECS` 内无数据上报的设备
/// 不翻回在线,避免本地标记离线后被宽限期的 ONLINE 观测反复翻回造成抖动。
pub async fn apply_online_status(
    db: &sqlx::PgPool,
    device_id: &str,
    online: bool,
) -> anyhow::Result<()> {
    // 在线状态变化 → 告警产生/消解
    let changed: Option<(String,)> = if online {
        sqlx::query_as(
            "UPDATE device SET status='online' \
             WHERE id=$1 AND status!='online' \
             AND last_seen_at > now() - $2 * interval '1 second' \
             RETURNING id",
        )
        .bind(device_id)
        .bind(OFFLINE_TIMEOUT_SECS)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query_as(
            "UPDATE device SET status='offline' \
             WHERE id=$1 AND status!='offline' \
             RETURNING id",
        )
        .bind(device_id)
        .fetch_optional(db)
        .await?
    };
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
    Ok(())
}

/// 影子属性入库(轮询与后续 webhook 推送共用):光照写入历史库,灯态更新到设备表。
/// `data_time` 为 None 时 `created_at` 用数据库默认 now();为 Some 时显式写入,
/// 并按 (`device_id`, `created_at`) 去重(webhook 重复推送不产生重复行)。
///
/// 心跳:`data_time`(`IoTDA` 平台事件时间)同时刷新 `last_seen_at`,且只前进不回拨
/// ——设备断连后影子保留旧值、事件时间冻结,乱序/迟到的旧事件也不会误刷心跳,
/// 这是本地失联检测(90s)能在 MQTT 超时之前生效的前提。
pub async fn apply_shadow_props(
    db: &sqlx::PgPool,
    device_id: &str,
    props: &ShadowProps,
    data_time: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<()> {
    if let Some(ts) = data_time {
        sqlx::query(
            "UPDATE device SET last_seen_at=$2 \
             WHERE id=$1 AND (last_seen_at IS NULL OR last_seen_at < $2)",
        )
        .bind(device_id)
        .bind(ts)
        .execute(db)
        .await?;
    }
    if let Some(lux) = props.luminance {
        if let Some(ts) = data_time {
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

/// 设备自动同步的结果视图(仅本模块内部使用)
#[derive(Default)]
struct SyncReport {
    /// 本次新入库的设备(云端有、本地无)
    pub added: Vec<String>,
    /// 本地有、云端无的设备(可能在云端被删或注册 ID 有误)
    pub missing_in_cloud: Vec<String>,
}

/// 华为云设备列表 → 本地 `device` 表(只增、不删、不改):
/// - 云端有、本地没有 → 插入,name 取云端 device_name(缺省用 device_id)
/// - 已存在 → 不动(手工注册的 name/location/经纬度不被覆盖)
/// - 本地有、云端没有 → 只收集进 `missing_in_cloud` 供提醒,不删除
///
/// 幂等:先拉全量再写库;任何一轮失败/重复执行都不会产生重复行或覆盖资料。
async fn sync_devices_from_cloud(
    db: &sqlx::PgPool,
    iothub: &IothubClient,
    product_id: Option<&str>,
) -> anyhow::Result<SyncReport> {
    let cloud = iothub.list_devices(product_id).await?;
    let mut report = SyncReport::default();
    for item in &cloud {
        let name = item
            .device_name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&item.device_id);
        let inserted = sqlx::query(
            "INSERT INTO device (id, name) VALUES ($1, $2) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&item.device_id)
        .bind(name)
        .execute(db)
        .await?;
        if inserted.rows_affected() > 0 {
            report.added.push(item.device_id.clone());
        }
    }
    let cloud_ids: HashSet<&str> =
        cloud.iter().map(|d| d.device_id.as_str()).collect();
    let local: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM device").fetch_all(db).await?;
    report.missing_in_cloud = local
        .into_iter()
        .map(|(id,)| id)
        .filter(|id| !cloud_ids.contains(id.as_str()))
        .collect();
    Ok(report)
}

/// 一次自动同步的执行体:同步入库 + 新增/漂移通知。
/// 通知去重靠 `notify::insert_sync_notification` 的"同设备同标题未读已存在则跳过",
/// 漂移设备每轮都会命中,但提醒只保留到管理员读掉为止,不会 30 分钟刷一条。
async fn sync_devices(
    state: &AppState,
    iothub: &IothubClient,
    product_id: Option<&str>,
) {
    if iothub.dry_run {
        tracing::debug!("dry-run: skip device auto-sync");
        return;
    }
    let report = match sync_devices_from_cloud(&state.db, iothub, product_id).await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("device auto-sync failed: {e:#}");
            return;
        }
    };
    for id in &report.added {
        tracing::info!("device auto-sync: 新设备 {id} 已自动入库");
        if let Err(e) = notify::insert_sync_notification(
            &state.db,
            "华为云新增设备已自动注册",
            &format!(
                "设备 {id} 在华为云已存在而本地未注册,已自动入库;如需补充位置与坐标请编辑设备资料。"
            ),
            Some(id),
        )
        .await
        {
            tracing::error!("sync notification for {id} failed: {e}");
        }
    }
    for id in &report.missing_in_cloud {
        tracing::warn!("device auto-sync: 本地设备 {id} 在华为云未找到");
        if let Err(e) = notify::insert_sync_notification(
            &state.db,
            "本地设备在华为云未找到",
            &format!(
                "本地已注册设备 {id} 未出现在华为云设备列表中,可能已在云端删除或注册 ID 有误,请检查。"
            ),
            Some(id),
        )
        .await
        {
            tracing::error!("sync notification for {id} failed: {e}");
        }
    }
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

    // 影子属性 → 历史库 + 灯态 + 心跳(平台事件时间)
    if let Some(report) = iothub.shadow(device_id).await? {
        apply_shadow_props(
            &state.db,
            device_id,
            &report.props,
            report.event_time,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 影子响应的 Light 服务带 `event_time:属性与平台事件时间(心跳源)都能解析`
    #[test]
    fn shadow_response_with_event_time() {
        let json = serde_json::json!({
            "device_id": "dev001",
            "shadow": [{
                "service_id": "Light",
                "reported": {
                    "properties": { "Luminance": 350, "LightStatus": "on" },
                    "event_time": "20260826T103000Z"
                }
            }]
        });
        let resp: ShadowResponse = serde_json::from_value(json).unwrap();
        let svc = resp
            .shadow
            .into_iter()
            .find(|s| s.service_id == "Light")
            .unwrap();
        assert_eq!(svc.reported.properties.luminance, Some(350));
        assert_eq!(
            svc.reported
                .event_time
                .as_deref()
                .and_then(parse_event_time),
            Some(
                chrono::DateTime::parse_from_rfc3339("2026-08-26T10:30:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc)
            )
        );
    }

    /// 影子响应缺 `event_time:属性照常解析,心跳时间为` None(不刷新 `last_seen_at`)
    #[test]
    fn shadow_response_without_event_time() {
        let json = serde_json::json!({
            "shadow": [{
                "service_id": "Light",
                "reported": { "properties": { "Luminance": 350 } }
            }]
        });
        let resp: ShadowResponse = serde_json::from_value(json).unwrap();
        let svc = resp
            .shadow
            .into_iter()
            .find(|s| s.service_id == "Light")
            .unwrap();
        assert!(svc.reported.event_time.is_none());
    }

    /// 灵活搜索响应:devices + count 正常解析,device_name 可缺省
    #[test]
    fn search_devices_response_parses() {
        let json = serde_json::json!({
            "devices": [
                {"device_id": "d1", "device_name": "路灯1"},
                {"device_id": "d2"}
            ],
            "count": 2
        });
        let resp: SearchDevicesResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.devices.len(), 2);
        assert_eq!(resp.devices[0].device_name.as_deref(), Some("路灯1"));
        assert_eq!(resp.devices[1].device_name.as_deref(), None);
        assert_eq!(resp.count, 2);
    }

    /// 灵活搜索响应:count 与整个 devices 数组缺省时也能解析(按 0/空处理)
    #[test]
    fn search_devices_response_defaults() {
        let json = serde_json::json!({
            "devices": [{"device_id": "d1"}]
        });
        let resp: SearchDevicesResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.devices.len(), 1);
        assert_eq!(resp.count, 0);

        let json = serde_json::json!({});
        let resp: SearchDevicesResponse = serde_json::from_value(json).unwrap();
        assert!(resp.devices.is_empty());
        assert_eq!(resp.count, 0);
    }
}
