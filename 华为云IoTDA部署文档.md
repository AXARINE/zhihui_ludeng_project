# 智慧路灯 — 华为云 IoTDA 部署文档

> 三步把系统跑起来:① 华为云控制台建实例/产品/设备/凭证(一次性,§3),② 固件烧录到 BearPi-HM Nano(§4),③ 后端部署(§5)。
> 接口清单见 `backend/README.md`,功能愿景见 `智慧路灯_基本功能清单.md`。

## 0. 快速开始(华为云已配好,5 分钟)

```bash
tar xzf streetlight-deploy-*.tar.gz && cd streetlight-deploy-*/
./deploy.sh          # 首次运行生成 config.json;填好 5 个必填项(见 §5.1)后再跑一次
```

- 验证:`curl http://127.0.0.1/api/health` → 200;全链路验收见 §6。
- 本机访问用 `http://127.0.0.1/`(用本机局域网 IP 自访会超时,WSL2 镜像模式怪癖);其他设备访问 `http://<主机IP>/`,打不开就放行防火墙 80/443。
- 手机热点/家用宽带无公网入口,局域网演示不需要 §3.5 数据转发(轮询已全功能)。

## 1. 部署架构

```
Hi3861 --Wi-Fi/MQTT(1883)--> 华为云 IoTDA <--北向 API(HTTPS, AK/SK V11 衍生签名)-- Rust 后端 --> PostgreSQL
                                                                                        ↑
                                                                              前端(Caddy 托管) --/api--> 后端
```

| 组件 | 说明 |
|---|---|
| 设备固件 | 50ms 采样 + 本地光照联动(断网可用),每 5s 上报属性,接收命令与阈值/调光下发 |
| IoTDA | 设备接入(MQTT)+ 影子 + 在线状态 + 命令/属性转发 |
| Rust 后端 | 轮询影子入库、在线/离线监控、REST API(含账号/RBAC)、命令经北向转发 |
| PostgreSQL | 数据持久化,迁移随后端启动自动执行 |
| 前端 | Vue3 静态产物,由部署包的 Caddy 托管 |

环境要求:WSL2 Ubuntu + Docker(编译/烧录/部署全用);2.4GHz Wi-Fi(Hi3861 不支持 5G);华为云账号已实名并开通 IoTDA。

## 2. 凭据清单(部署前先备齐)

| 凭据 | 填到哪 | 获取位置 |
|---|---|---|
| Wi-Fi SSID / 密码 | 固件 `app_config.h` | 自备(2.4G) |
| 设备 ID / 设备密钥 | 固件 `app_config.h` | 控制台 → 注册设备(§3.3) |
| 实例**设备侧域名** | 固件 `e53_sc1_example.cpp` 顶部 `CONFIG_APP_SERVERIP` | 控制台 → 实例 → 接入信息(§3.1) |
| 实例**应用侧域名** | `config.json` 的 `iotda_endpoint` | 同上 |
| 项目 ID | `config.json` 的 `huawei_project_id` | 我的凭证 → 项目列表(对应区域行) |
| AK / SK | `config.json` 的 `huawei_ak` / `huawei_sk` | 我的凭证 → 访问密钥(§3.4) |

## 3. 华为云 IoTDA 侧配置(控制台操作,一次性)

### 3.1 创建实例并记录接入信息

1. 设备接入 IoTDA → 开通/创建**标准版实例**(区域如 **cn-south-1**,单设备演示免费额度足够)。
2. 实例详情 → **接入信息**,记下**设备侧域名**(`xxx.st1.iotda-device.{region}.myhuaweicloud.com` → 填固件)与**应用侧域名**(`xxx.st1.iotda-app...` → 填 config.json)。
3. ⚠️ 标准版/企业版**没有区域共享域名**(`iotda.{region}.myhuaweicloud.com` 不存在),必须用实例级域名。

### 3.2 创建产品与模型

1. 产品 → 创建产品:协议 **MQTT**、数据格式 **JSON**。
2. 产品详情 → 模型定义,服务 ID 为 **`Light`**:

| 类型 | 标识 | 数据类型 | 读写 | 说明 |
|---|---|---|---|---|
| 属性 | `Luminance` | int | 只读 | 光照值,设备每 5s 上报 |
| 属性 | `LightStatus` | string | 只读 | 灯态(ON/OFF) |
| 属性 | `Brightness` | int | **可读可写** | 输出亮度 0~100;云端设值即手动调光(0=关灯) |
| 属性 | `DimCurve` | string(长度 ≥64) | **可读可写** | auto 模式照度-亮度曲线 `lux:pct,...`(≤4 点严格递增);空串=回退阈值开关 |
| 属性 | `Threshold` | int | **可读可写** | 开关灯阈值 |
| 命令 | `Light_Control_Led` | 参数 `Led`:string,枚举 ON/OFF/AUTO | — | 控灯/恢复自动 |

> ⚠️ 可写属性不勾"可写"会导致下发报 IOTDA.000029;模型缺 `Brightness` 字段会导致上报整条被拒。**先建模型,再烧固件**。

### 3.3 注册设备

设备 → 所有设备 → 注册设备;记下**设备 ID** 与**设备密钥**(填 `app_config.h`)。

### 3.4 北向 API 凭证(IAM AK/SK)

1. IAM → 创建用户(编程访问)或"我的凭证 → 访问密钥"直接建 AK/SK。
2. 用户所属**用户组**绑定 IoTDA 权限策略(如 `{"Action": ["IoTDA:*:*"]}`);⚠️ 授权有数分钟传播延迟。
3. ⚠️ 北向必须用 **V11-HMAC-SHA256 衍生签名**(后端已实现;`iotda_region` 留空自动推断)。误用旧版 SDK-HMAC-SHA256 会 401 IOTDA.000002。

### 3.5 数据转发(HTTP 推送,公网推荐)

推送为主、轮询兜底:上报与状态变化由 IoTDA 主动 POST 给后端,不再依赖轮询。

1. 实例详情 → **数据转发** → 创建规则:勾选**设备属性变化**与**设备状态变化**,目标 **HTTP 推送**,URL 填 `https://<域名>/api/iotda/callback`。
2. **自定义 Header(公网必配)**:`Authorization: Bearer <随机长串>`,与 config.json 的 `iotda_webhook_token` 同值(`openssl rand -hex 32`);不配则回调无鉴权。
3. 同时把 `iotda_poll_interval_secs` 设为 `60`(轮询只兜底校准)。

> 无公网入口(家用宽带/热点)不用建此规则——推不进来;保持轮询 + 自动同步即可,功能无缺失。

## 4. 设备端固件部署

```bash
git clone --recursive https://github.com/AXARINE/zhihui_ludeng_project.git
cd zhihui_ludeng_project
# 配置凭据:复制模板并填写(Wi-Fi + 设备 ID/密钥;该文件被 .gitignore 忽略)
cp C3_e53_sc1_pls/include/app_config.example.h C3_e53_sc1_pls/include/app_config.h
# 编辑 C3_e53_sc1_pls/e53_sc1_example.cpp 顶部 CONFIG_APP_SERVERIP = 实例设备侧域名

./build.sh      # Docker 一键编译
./flash.sh 4    # 烧录(4 换成板子 COM 号);HiBurn 弹出后按一下开发板 RESET,烧完再按一次 RESET 运行
```

- ⚠️ 端口保持 **1883**,不要改 8883 MQTTS(Hi3861 上 iot_link/mbedtls TLS 不稳定:内核异常、订阅超时、重启循环)。
- 验证:串口 115200 看到 `oc_mqtt_profile_connect succed` 及每 5s 上报;无串口时手捂光敏,补光灯亮 = 固件在跑;IoTDA 控制台设备"在线"。

## 5. 后端部署

### 5.1 发布部署包(config.json 一键部署,推荐)

适合"发给别人 / 服务器从零部署"。**push 到 master 即自动发版**:CI 跑测试 → 构建 → 按最新 tag 补丁号 +1 自动定版 → 产出 `streetlight-deploy-<版本>.tar.gz` 连同 tag 一起挂到 GitHub Release(纯文档/固件/工具改动不触发)。**连续 push 只发最后一版**(旧 run 自动取消);需要给某个中间提交补包时,在 Actions 页手动运行工作流并填 `version`。

```bash
tar xzf streetlight-deploy-*.tar.gz && cd streetlight-deploy-*/
./deploy.sh    # 首次运行:生成 config.json 并退出;填完后再次运行 = 生成 .env/Caddyfile + 启动
```

部署包 = PostgreSQL + 后端瘦镜像 + Caddy(80/443 单入口,托管前端并反代 `/api`、`/docs`)。`deploy.sh` 从 `config.json` 生成 `.env` 与 `Caddyfile`(勿手改,会被下次部署覆盖),并拦截未填的必填项。

**config.json 键(全部字符串值)**:

| 键 | 必填 | 说明 |
|---|---|---|
| `huawei_ak` / `huawei_sk` | ✅ | 北向 API 访问密钥(§3.4) |
| `huawei_project_id` | ✅ | 实例所在区域的项目 ID |
| `iotda_endpoint` | ✅ | 实例**应用侧**域名(§3.1) |
| `jwt_secret` | ✅ | `openssl rand -hex 32` |
| `domain` | | 留空 = `:80` HTTP;填域名后 Caddy 自动申请 HTTPS(需 DNS 指向本机、放行 443) |
| `iotda_region` | | 留空自动从 endpoint 推断 |
| `bootstrap_super_admin_password` / `bootstrap_admin_password` | 建议 | 引导账号密码;默认值仅开发用,上线必改(删除默认 superadmin 前须先建新 super_admin) |
| `iotda_webhook_token` | 公网必填 | 数据转发推送鉴权 token(§3.5) |
| `iotda_poll_interval_secs` | | 影子轮询秒数,默认 8;启用推送后建议 60 |
| `iotda_auto_sync_devices` | | `true` = 华为云设备列表自动注册入库(只增不删,见 5.2) |
| `iotda_sync_interval_secs` | | 设备自动同步间隔秒数,默认 1800(30 分钟);演示/频繁增删设备时可设 60 |
| `postgres_password` | | 数据库密码,默认 `streetlight` |
| `pgdata_volume` | | 复用已有数据卷名(默认 `streetlight-deploy-pgdata`) |
| `allowed_origins` | | 前后端不同域时填前端域名(逗号分隔);同域 Caddy 托管无需配置 |
| `ai_api_key` / `ai_base_url` / `ai_model` | | 大模型问答(OpenAI 兼容);留空 = 本地关键词问答,功能不受影响 |

端口策略:对外只有 Caddy 80/443;后端 8080 只绑 127.0.0.1;数据库不映射宿主端口。**安全组只放行 80/443**。

更新:下载新版部署包解包覆盖 → `./deploy.sh`;数据卷按名保留,历史数据不丢。

### 5.2 源码方式(开发/二开)

```bash
cd backend
cp .env.example .env && vim .env   # 变量与 config.json 键一一对应,完整清单见 .env.example
./dev.sh db                        # 只起 PostgreSQL
./dev.sh run                       # 本地开发:加载 .env 启动,监听 8080
# 或全栈容器化:docker compose up -d --build(5432/8080 只绑 127.0.0.1)
```

- Swagger UI:`http://127.0.0.1:8080/docs`,login 拿 token → Authorize → 在线调试。
- 云服务器源码部署后更新:`./dev.sh update`(git pull → 重建 → 健康检查)。
- 设备注册:`.env` 设 `IOTDA_AUTO_SYNC_DEVICES=true` 自动同步云端设备(推荐),或手动 `POST /api/devices`(ID 与固件 `CONFIG_APP_DEVICEID` 一致)。

## 6. 部署验收清单

| 检查项 | 方法 | 预期 |
|---|---|---|
| 设备在线 | IoTDA 控制台 → 设备状态 | 在线 |
| 数据入库 | Swagger → `GET /api/dashboard` | `reports_24h` 持续增长 |
| 实时光照 | `GET /api/devices/{id}/lux/latest` | 返回当前照度 |
| 远程控灯 | `POST /api/devices/{id}/lamp` `{"action":"on"}` | 补光灯亮(≤1 个轮询周期回显) |
| 恢复自动 | `{"action":"auto"}` | 回到本地光照联动 |
| 阈值下发 | `PUT /api/devices/{id}/threshold` | 设备按新阈值动作 |
| 离线告警 | 拔电 → 重新上电 | 产生 offline 告警,恢复后自动消解 |
| 权限隔离 | 市政账号执行管理操作 | 返回 403 |

> 控灯/阈值透传 IoTDA 北向,设备离线时北向拒绝(502);在线状态另有 90s 本地失联检测,拔电到告警产生有数秒~数十秒延迟,验收预留观察窗口。

## 7. 日常运维

| 项 | 方法 |
|---|---|
| 后端日志 | `docker logs -f streetlight-deploy-backend`(源码栈为 `streetlight-backend`) |
| 设备日志 | 串口 115200(`bearpi-serial.ps1`);重播启动日志按 RESET |
| 备份 | `docker exec <pg容器> pg_dump -U streetlight streetlight > backup.sql` |
| 数据持久化 | 数据卷按名复用,`docker compose down`(不带 `-v`)不丢数据 |

常见问题:

| 症状 | 原因与处置 |
|---|---|
| 北向 401 IOTDA.000002 | 须 V11 衍生签名(后端已实现);或 IAM 授权传播延迟,等几分钟 |
| 下发报 IOTDA.000029 | 产品模型对应属性未勾"可读可写"(§3.2) |
| 命令超时 IOTDA.014111、设备反复离线 | 误开 8883 MQTTS,改回 1883(§4) |
| 设备连不上云 | 域名填错(无区域共享域名)、仅 2.4G Wi-Fi、设备 ID/密钥不匹配 |
| HiBurn 退出码 17 / 52 | 板子未连接或窗口被关;确认 COM 号 |
| 串口无输出 | 串口被 HiBurn 独占,关闭后再看;充电器供电按 RESET |

## 8. 安全说明与限制

- **凭据不进 git**:`app_config.h`、`backend/.env`、`deploy/config.json` 均被 `.gitignore` 忽略;镜像构建排除 `.env`。
- **轮询延迟**:影子入库与状态回显有数秒延迟(默认 8s 轮询;启用推送后更低),勿调过快避免触发华为云限流。
- **计费**:标准版单设备演示在免费额度内,演示完及时删除不用的产品/设备。

## 9. 迭代流程(部署后的开发)

- 固件:改 `C3_e53_sc1_pls/`(权威副本,勿改 submodule 树)→ `./build.sh` → `./flash.sh 4` → RESET ×2 → 串口验证;
- 后端:改 `backend/src/` → `cargo build` → curl 验证;新接口必须补 `#[utoipa::path]` 注解并登记进 `openapi.rs`;
- 前端:改 `frontend_vue/src/` → `npm run build` 自测;
- **发版**:本机整套走仓库根 `./release.sh`(测试 → 构建前后端 → 部署到本机 deploy/ 栈 → 冒烟验证),验过再 push;push 到 master 后 CI 自动定版出 Release 部署包(§5.1);
- 数据库 schema:**上线后必须新建递增迁移**,不再原地改旧文件。
