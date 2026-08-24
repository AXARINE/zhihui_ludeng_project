use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac_raw(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// 华为云 `IoTDA` 北向 API 客户端(AK/SK 签名认证,算法 V11-HMAC-SHA256 衍生签名;标准版/企业版实例必须)
///
/// 端点为实例级域名(cn-south-1 等区域无共享域名),
/// 通过 `HUAWEI_IOTDA_ENDPOINT` 配置,形如 xxx.st1.iotda-app.cn-south-1.myhuaweicloud.com
pub struct IothubClient {
    endpoint: String,
    region: String,
    project_id: String,
    ak: String,
    sk: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ShadowProps {
    pub light_status: Option<String>,
    pub luminance: Option<i64>,
}

impl IothubClient {
    pub fn from_env() -> anyhow::Result<Option<Arc<Self>>> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let (Some(ak), Some(sk), Some(project_id), Some(endpoint)) = (
            get("HUAWEI_AK"),
            get("HUAWEI_SK"),
            get("HUAWEI_PROJECT_ID"),
            get("HUAWEI_IOTDA_ENDPOINT"),
        ) else {
            tracing::warn!("HUAWEI_* 环境变量未配置,IoTDA 北向功能停用");
            return Ok(None);
        };
        let endpoint = endpoint
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string();
        // V11 衍生签名需要区域名:优先 HUAWEI_IOTDA_REGION,否则从域名推断(如 cn-south-1)
        let region = std::env::var("HUAWEI_IOTDA_REGION")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| {
                endpoint
                    .split('.')
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find(|w| w[1] == "myhuaweicloud.com")
                    .map(|w| w[0].to_string())
            })
            .ok_or_else(|| anyhow::anyhow!("无法确定 IoTDA 区域,请设置 HUAWEI_IOTDA_REGION"))?;
        Ok(Some(Arc::new(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
            endpoint,
            region,
            project_id,
            ak,
            sk,
        })))
    }

    fn host(&self) -> &str {
        &self.endpoint
    }

    fn path_of(&self, path: &str) -> String {
        format!("/v5/iot/{}{}", self.project_id, path)
    }

    /// 按华为云 V11-HMAC-SHA256 衍生签名算法生成 Authorization 头
    ///
    /// `IoTDA` 标准版/企业版实例必须使用衍生签名(官方 SDK: `WithDerivedPredicate`);
    /// 基础版共享域名才用旧版 SDK-HMAC-SHA256。与官方 SDK `derived_signer.go` 对齐:
    /// 1. info = {YYYYMMDD}/{region}/iotdm,service 固定 "iotdm"
    /// 2. 派生密钥 = HKDF(SHA-256, ikm=SK, salt=AK, info=info) 的 32 字节,再 hex 编码后作为 HMAC key
    /// 3. 规范 URI 以 '/' 结尾、规范头部块与 `SignedHeaders` 之间多一个空行(与旧算法相同)
    fn sign(&self, method: &str, uri: &str, sdk_date: &str, body: &str) -> String {
        let signed_headers = "content-type;host;x-sdk-date";
        let canonical_headers = format!(
            "content-type:application/json\nhost:{}\nx-sdk-date:{}\n",
            self.host(),
            sdk_date
        );
        let canonical_request = format!(
            "{}\n{}/\n\n{}\n{}\n{}",
            method,
            uri,
            canonical_headers,
            signed_headers,
            sha256_hex(body.as_bytes())
        );
        let info = format!("{}/{}/iotdm", &sdk_date[..8], self.region);
        // HKDF-Extract: PRK = HMAC(salt=AK, data=SK); HKDF-Expand: T1 = HMAC(key=PRK, data=info||0x01)
        let prk = hmac_raw(self.ak.as_bytes(), self.sk.as_bytes());
        let mut expand = info.as_bytes().to_vec();
        expand.push(0x01);
        let derived_key_hex = hex::encode(hmac_raw(&prk, &expand));
        let string_to_sign = format!(
            "V11-HMAC-SHA256\n{}\n{}\n{}",
            sdk_date,
            info,
            sha256_hex(canonical_request.as_bytes())
        );
        let mut mac = HmacSha256::new_from_slice(derived_key_hex.as_bytes())
            .expect("hmac accepts any key length");
        mac.update(string_to_sign.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        format!(
            "V11-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.ak, info, signed_headers, signature
        )
    }

    fn signed_headers(&self, method: &str, path: &str, body: &str) -> [(&'static str, String); 4] {
        let sdk_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let uri = self.path_of(path);
        let auth = self.sign(method, &uri, &sdk_date, body);
        [
            ("content-type", "application/json".to_string()),
            ("host", self.host().to_string()),
            ("x-sdk-date", sdk_date),
            ("authorization", auth),
        ]
    }

    async fn get(&self, path: &str) -> anyhow::Result<reqwest::Response> {
        let [ct, host, date, auth] = self.signed_headers("GET", path, "");
        let url = format!("https://{}{}", self.host(), self.path_of(path));
        let resp = self
            .http
            .get(url)
            .header(ct.0, ct.1)
            .header(host.0, host.1)
            .header(date.0, date.1)
            .header(auth.0, auth.1)
            .send()
            .await?;
        Ok(resp)
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: serde_json::Value,
    ) -> anyhow::Result<reqwest::Response> {
        let raw = body.to_string();
        let [ct, host, date, auth] = self.signed_headers(method.as_str(), path, &raw);
        let url = format!("https://{}{}", self.host(), self.path_of(path));
        let resp = self
            .http
            .request(method, url)
            .header(ct.0, ct.1)
            .header(host.0, host.1)
            .header(date.0, date.1)
            .header(auth.0, auth.1)
            .body(raw)
            .send()
            .await?;
        Ok(resp)
    }

    /// 查询设备影子,返回 Light 服务上报的属性
    pub async fn shadow(&self, device_id: &str) -> anyhow::Result<Option<ShadowProps>> {
        let resp = self.get(&format!("/devices/{device_id}/shadow")).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("shadow query failed: {status}: {}", body.trim());
        }
        let v: serde_json::Value = resp.json().await?;
        let props = v["shadow"]
            .as_array()
            .and_then(|arr| arr.iter().find(|s| s["service_id"] == "Light"))
            .map(|s| &s["reported"]["properties"]);
        Ok(props.map(|p| ShadowProps {
            luminance: p["Luminance"].as_i64(),
            light_status: p["LightStatus"].as_str().map(String::from),
        }))
    }

    /// 查询设备在线状态,返回 "ONLINE" / "OFFLINE" 等
    pub async fn device_status(&self, device_id: &str) -> anyhow::Result<String> {
        let resp = self.get(&format!("/devices/{device_id}")).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("device query failed: {status}: {}", body.trim());
        }
        let v: serde_json::Value = resp.json().await?;
        Ok(v["status"].as_str().unwrap_or("UNKNOWN").to_string())
    }

    /// 下发 `Light_Control_Led` 命令(Led: ON/OFF/AUTO)
    pub async fn control_led(&self, device_id: &str, led: &str) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "service_id": "Light",
            "command_name": "Light_Control_Led",
            "paras": { "Led": led }
        });
        let resp = self
            .request(
                reqwest::Method::POST,
                &format!("/devices/{device_id}/commands"),
                body,
            )
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("command failed: {status}: {}", body.trim());
        }
        Ok(())
    }

    /// 设置 Threshold 可写属性
    pub async fn set_threshold(&self, device_id: &str, threshold: i32) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "services": [{
                "service_id": "Light",
                "properties": { "Threshold": threshold }
            }]
        });
        let resp = self
            .request(
                reqwest::Method::PUT,
                &format!("/devices/{device_id}/properties"),
                body,
            )
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("set properties failed: {status}: {}", body.trim());
        }
        Ok(())
    }
}

/// 轮询任务:周期性拉设备影子/状态入库
pub async fn run(state: Arc<AppState>, iothub: Arc<IothubClient>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(8));
    loop {
        ticker.tick().await;
        let devices: Vec<(String,)> = match sqlx::query_as("SELECT id FROM device").fetch_all(&state.db).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("list devices failed: {e}");
                continue;
            }
        };
        for (device_id,) in devices {
            if let Err(e) = poll_device(&state, &iothub, &device_id).await {
                tracing::warn!("poll {device_id} failed: {e}");
            }
        }
    }
}

async fn poll_device(
    state: &AppState,
    iothub: &IothubClient,
    device_id: &str,
) -> anyhow::Result<()> {
    // 先查在线状态:离线时影子保留最后上报值,不能直接当实时数据入库
    let status = iothub.device_status(device_id).await?;
    let online = status == "ONLINE";

    // 在线状态变化 → 告警产生/消解
    let changed: Option<(String,)> = sqlx::query_as(
        "UPDATE device SET status=$2 WHERE id=$1 AND status!=$2 RETURNING id",
    )
    .bind(device_id)
    .bind(if online { "online" } else { "offline" })
    .fetch_optional(&state.db)
    .await?;
    if changed.is_some() {
        if online {
            sqlx::query(
                "UPDATE alarm SET resolved_at=now() \
                 WHERE device_id=$1 AND type='offline' AND resolved_at IS NULL",
            )
            .bind(device_id)
            .execute(&state.db)
            .await?;
        } else {
            tracing::warn!("device {device_id} offline");
            sqlx::query("INSERT INTO alarm (device_id, type, message) VALUES ($1, 'offline', '设备离线')")
                .bind(device_id)
                .execute(&state.db)
                .await?;
        }
    }

    if !online {
        return Ok(());
    }

    sqlx::query("UPDATE device SET last_seen_at=now() WHERE id=$1")
        .bind(device_id)
        .execute(&state.db)
        .await?;

    // 影子属性 → 历史库 + 灯态
    if let Some(props) = iothub.shadow(device_id).await? {
        if let Some(lux) = props.luminance {
            sqlx::query("INSERT INTO lux_record (device_id, lux) VALUES ($1, $2)")
                .bind(device_id)
                .bind(lux as i32)
                .execute(&state.db)
                .await?;
        }
        if let Some(lamp) = &props.light_status {
            sqlx::query("UPDATE device SET lamp=$2 WHERE id=$1")
                .bind(device_id)
                .bind(lamp.to_lowercase())
                .execute(&state.db)
                .await?;
        }
    }
    Ok(())
}
