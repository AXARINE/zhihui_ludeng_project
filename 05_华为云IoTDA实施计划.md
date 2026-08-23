# 智慧路灯 — 华为云 IoTDA 实施计划(v2)

> v2 变更说明:课程要求走华为云路线,设备接入改为**华为云 IoTDA**(参考官方 D9_iot_cloud_oc_light 样例)。
> 应用侧数据通道采用**北向 API 轮询**(账号已实名,无需公网服务器),本地 Rust 后端 + PostgreSQL 保留。
> v1(本地 Mosquitto 直连)作废;`server/docker-compose.yml` 中的 Mosquitto 不再进主链路。

## 目标架构

```
Hi3861 --MQTT(oc_mqtt,属性/命令)--> 华为云 IoTDA
                                        ↑ 北向 API(HTTPS,IAM Token)
                                   Rust 后端(本地 WSL,axum)
                                        ↓
                                   PostgreSQL(本地 Docker)
```

- 设备端:连 IoTDA,上报 `Luminance` / `LightStatus` 属性;接收平台命令(开关灯/切自动)与属性设置(阈值);**保留本地 50ms 光照联动**,断网可用。
- 后端:定时轮询设备影子(光照/灯态)入库做历史;查设备状态做在线/离线监控;REST API 对内统一出口,命令经北向 API 转发到设备。
- 前端:不在本项目范围。

## IoTDA 侧准备(用户控制台操作)

区域为 **cn-south-1(华南-广州)**,项目 ID `79048fdb3e6142079ddb5ed99367629d`。

1. **创建产品**:设备接入 IoTDA → 产品 → 创建产品
   - 协议 MQTT,数据格式 JSON,设备类型自定(如 StreetLight)
2. **定义模型**(在产品详情 → 模型定义):
   - 服务 ID:`Light`
   - 属性:`Luminance`(int,只读)、`LightStatus`(string,只读)、`Threshold`(int,读写)
   - 命令:`Light_Control_Led`,参数 `Led`(string,枚举 ON/OFF/AUTO)
3. **注册设备**:设备 → 所有设备 → 注册设备,记下 **设备 ID** 和 **设备密钥**(secret)
4. **北向 API 凭证**:统一身份认证 IAM → 创建 IAM 用户(勾编程访问)或使用 **AK/SK**(我的凭证 → 访问密钥);记录 **项目 ID**(我的凭证 → 项目列表,**cn-south-1** 行)
   - ⚠️ 权限:给该 IAM 用户(或其用户组)绑定策略 **IoTDA FullAccess**,作用范围选区域级项目 **cn-south-1**(本项目已通过自定义策略 policy7ue2o2 授予 IoTDA:*:* )。
   - ⚠️ 签名算法:标准版/企业版实例的北向 API **必须使用 V11-HMAC-SHA256 衍生签名**(service 固定 iotdm,区域从 endpoint 推断);若误用旧版 SDK-HMAC-SHA256,接口返回 401 IOTDA.000002 Authentication failed,与 IAM 权限无关。后端 iothub.rs 已实现衍生签名。

## 实施步骤

### 1. 固件改造(`C3_e53_sc1_pls/`,权威副本)

以 D9 样例为蓝本移植 oc_mqtt 接入,与现有本地逻辑融合:

- `BUILD.gn`:include_dirs 补 `iot_link`(oc_mqtt_al / oc_mqtt_profile_v5 / inc / queue),deps 加 `//third_party/iot_link:iot_link`(照 D9)。
- `e53_sc1_example.c`:
  - 新增连接任务:`WifiConnect(SSID, 密码)` → `device_info_init` → `oc_mqtt_init` → `oc_mqtt_profile_connect`(server=`117.78.5.125` 标准版,port 1883,DTLS 关闭,与 D9 一致)
  - 属性上报:每 5s `oc_mqtt_profile_propertyreport`(service `Light`,属性 `Luminance` + `LightStatus`)
  - 命令处理:`oc_set_cmd_rsp_cb` 回调入队列,主任务解析 `Light_Control_Led`(ON/OFF/AUTO),回 `oc_mqtt_profile_cmdresp`
  - 属性设置回调:处理 `Threshold` 写属性,覆盖运行时阈值(默认 40)
  - 保留:50ms 采样循环、auto 模式本地阈值开关灯
- 需用户提供:Wi-Fi SSID/密码、设备 ID、设备密钥(填入 `CONFIG_*` 宏)

### 2. 后端改造(`server/backend/`)

- 新增 `src/iothub.rs`:
  - AK/SK 签名(**标准版用 V11-HMAC-SHA256 衍生签名**,旧 SDK-HMAC-SHA256 会 401;基础版才用旧算法)
  - 轮询任务(每 5~10s):`GET /v5/iot/{project_id}/devices/{id}/shadow` → 解析 `Luminance`/`LightStatus` → 写 `lux_record`、更新 `device`
  - 设备状态:`GET /v5/iot/{project_id}/devices/{id}` → `status` 字段(ONLINE/OFFLINE)→ 离线写 `alarm`
  - 命令转发:`POST /v5/iot/{project_id}/devices/{id}/commands`(Light_Control_Led)
  - 属性设置:`POST /v5/iot/{project_id}/devices/{id}/properties`(Threshold)
- REST API 保持已实现的清单不变(lamp/threshold 接口改为调北向 API;删除 rumqttc 依赖与 Mosquitto 直连)
- 新增依赖:`reqwest`(rustls)
- 离线告警:以 IoTDA 设备状态为准,保留本地 `alarm` 表与检测逻辑

### 3. 联调验收

- 烧录(用户按 RESET),串口确认 `oc_mqtt_profile_connect succed`
- IoTDA 控制台:设备在线、属性每 5s 刷新
- `curl localhost:8080/api/devices/sl-001/lux/latest` 有值;history 持续增长
- `POST .../lamp {"action":"on"}` → 灯亮(≤ 1 个轮询周期内状态回显)
- `PUT .../threshold` → 设备按新阈值动作
- 拔电 → 后端产生 offline 告警;上电恢复自动 resolved

### 4. 收尾

- 更新 `AGENTS.md`(IoTDA 架构、新工作流)、`README.md`

## 需要用户提供

- Wi-Fi 2.4G 名称/密码
- IoTDA:设备 ID、设备密钥、项目 ID、区域(默认 cn-north-4)
- 北向 API 凭证:IAM 用户名/密码 或 AK/SK(仅存本地,建议写 `.env`,不进 git)

## 风险

- IoTDA 免费额度/计费:标准版有免费额度(连接数/消息量),单设备演示足够;注意及时删除不用的产品。
- 轮询延迟:状态回显有 5~10s 延迟,演示可接受;如需实时可后续升级数据流转(需公网)。
- 北向 API 限流:轮询间隔 ≥5s,勿过快。
