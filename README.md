# 智慧路灯 IoT 管理系统

基于 BearPi-HM Nano（Hi3861, RISC-V）+ 华为云 IoTDA 的智慧路灯系统：BH1750 光照监测、自动/手动控灯、阈值下发、离线告警、设备管理、账号 RBAC、审计流水与维护智能问答，已于 2026-08 完成全链路验收。

## 系统架构

[<img src="archify/vue-rust.webp" alt="智慧路灯系统架构图:Vue 前端 + Rust 后端" width="100%"/>](https://axarine.github.io/zhihui_ludeng_project/vue-rust-frontend-backend.html)

> 点击上图打开**交互式架构图**(浏览器直接渲染,支持缩放 / 聚焦 / 明暗主题与导出);规格见 [`archify/vue-rust.architecture.json`](archify/vue-rust.architecture.json)。

- **南向**：固件用 `oc_mqtt` 连 IoTDA **设备侧实例域名**（`xxx.st1.iotda-device.{region}.myhuaweicloud.com`，1883 明文；8883 MQTTS 在本工程实测不可用），每 5s 上报 `Light` 服务属性 `Luminance` / `LightStatus`。
- **北向**：后端以 AK/SK **V11-HMAC-SHA256 衍生签名**轮询 IoTDA **应用侧实例域名**（查询状态/影子、下发命令、设置可写属性），数据落库 PostgreSQL；同时支持 IoTDA **数据转发 HTTP 推送**（`POST /api/iotda/callback`）作为事件驱动主通道。
- 详细后端设计见 [`backend/README.md`](backend/README.md)，开发手册见 [`AGENTS.md`](AGENTS.md)。

## 功能清单

| 功能 | 说明 |
|------|------|
| 光照监测 | BH1750 实时采集（50ms 采样，L-res 模式），5 点滑动窗口混合滤波（中值+均值，抑制 ±1000lx 级跳变），`Luminance`/`LightStatus` 每 5s 上报，历史数据落库 |
| 自动控制 | **固件本地施密特触发**：迟滞带 + 扣除补光灯自照度 + 连续 1s 确认，防自反馈频闪；阈值由云端下发 |
| 手动控制 | 前端/API 下发 `Light_Control_Led`（ON/OFF/AUTO），命令经 IoTDA 下发并留痕 |
| PWM 调光 | GPIO7 复用 PWM0（10kHz 载波，γ=2.2 感知亮度校正）；云端设 `Brightness`(0~100，设值即手动）;auto 模式支持可配置照度-亮度曲线 `DimCurve`(≤4 锚点分段线性插值，空串回退阈值开关）；`GET/PUT /api/devices/{id}/dimming` |
| 阈值配置 | 云端可读可写属性 `Threshold`（int），下发后固件立即生效 |
| 在线检测 | IoTDA 状态 + **90s 本地失联检测**（心跳 = IoTDA 平台事件时间，单调前进） |
| 离线告警 | 断连自动产生 `offline` 告警（去重）、恢复自动消解；支持人工标记已处理/恢复 |
| 设备管理 | 设备注册/编辑/删除（级联清数据）、查看在线状态与灯态 |
| 仪表盘 | 首页聚合：设备总数/在线/开灯数、未处理告警、24h 光照均值、指令数 |
| 账号与 RBAC | JWT + Argon2id；`municipal`（市政）/ `admin`（路灯管理员）/ `super_admin`（系统管理员）三角色 + 15 个权限点，权限可在线调整 |
| 审计日志 | 用户/角色/阈值变更写 `audit_log`，控灯指令归因 `command_record.operator_id`，`GET /api/audit-logs` 查询 |
| 智能问答 | 本地检索式维护助手：意图识别 + 实体/时间窗抽取 + 查库 + 模板回答（不依赖外部大模型） |
| Swagger UI | `http://127.0.0.1:8080/docs` 在线调试全部接口 |

## 快速开始

### 环境要求

| 工具 | 版本 | 用途 |
|------|------|------|
| Git | 任意 | 克隆仓库 |
| Rust | stable（edition 2024，cargo 1.98+） | 后端编译 |
| Docker | 任意（含 compose v2 插件） | PostgreSQL / 一键部署 |
| Node.js | v18+ | 前端运行（可选） |
| WSL2 | 可选 | 固件编译（Docker 编译环境） |

### 1. 克隆仓库

```bash
git clone https://github.com/AXARINE/zhihui_ludeng_project.git
cd zhihui_ludeng_project
# 首次克隆需初始化固件源码树（git submodule）
git submodule update --init
```

### 2. 启动后端

后端统一入口是 `backend/dev.sh`，全部子命令见 `./dev.sh help`：

```bash
cd backend
cp .env.example .env          # 填写华为云 AK/SK、项目 ID、实例应用侧域名等

./dev.sh db                   # 只启动本地 PostgreSQL（docker compose）
./dev.sh run                  # 加载 .env 后 cargo run（本地开发，监听 :8080）
```

或一键容器化部署（本地 / 云服务器通用）：

```bash
./dev.sh up                   # docker compose up -d --build（postgres + backend + nocodb）
```

- 首次启动自动执行 `migrations/` 建表，并按角色**补建引导账号**（该角色已有账号则跳过）：
  - `superadmin` / `superadmin123`（系统管理员，可用 `BOOTSTRAP_SUPER_ADMIN_USERNAME/PASSWORD` 覆盖）
  - `admin` / `admin123`（路灯管理员，可用 `BOOTSTRAP_ADMIN_USERNAME/PASSWORD` 覆盖）
- 生产环境**必须**覆盖全部引导账号默认值与 `JWT_SECRET`；云部署建议删除 postgres/nocodb 的端口映射，安全组只放行 8080。
- Windows 侧替代入口：PowerShell 用 `backend/start.ps1`，Git Bash 用仓库根 `run.sh`。
- Swagger UI：`http://127.0.0.1:8080/docs`——先 `POST /api/auth/login` 拿 token，右上角 Authorize 填 `Bearer <token>` 即可在线调试。

### 3. 启动前端

```bash
cd frontend_vue
npm install
npm run dev
```

打开浏览器访问 http://localhost:5173（开发代理已配置到 8080）。

### 4. 固件编译与烧录（可选）

需要 WSL2 + Docker。在 WSL 中：

```bash
cd ~/bearpi/smart-street-light
./build.sh                     # 同步源码进 bearpi-hm_nano 源码树 → Docker 编译 → 更新 compile_commands.json
./flash.sh 4                   # 烧录到 COM4（自动处理 HiBurn 权限/临时目录/COM 数字格式等坑）
```

- 编译产物：`bearpi-hm_nano/out/BearPi-HM_Nano/Hi3861_wifiiot_app_allinone.bin`
- 烧录/启动流程：运行 `flash.sh` 后按提示**按 RESET 进入烧录模式** → `FLASH OK` 后再**按一次 RESET** 运行新固件。
- 看日志：串口 115200（`bearpi-serial.ps1` 或 MobaXterm → COM4）。

#### 固件配置

固件凭据在 `C3_e53_sc1_pls/include/app_config.h`（已被 `.gitignore` 忽略，不会提交），从模板复制并填写：

```bash
cd C3_e53_sc1_pls/include
cp app_config.example.h app_config.h
```

```c
#define CONFIG_WIFI_SSID "你的WiFi名称"      // 仅 2.4GHz
#define CONFIG_WIFI_PWD  "你的WiFi密码"
#define CONFIG_APP_DEVICEID  "..."           // IoTDA 设备 ID
#define CONFIG_APP_DEVICEPWD "..."           // 设备密钥
```

IoTDA 实例**设备侧**域名（`CONFIG_APP_SERVERIP`）在 `e53_sc1_example.cpp` 顶部，形如 `xxx.st1.iotda-device.cn-south-1.myhuaweicloud.com`。产品模型 `Light`：属性 `Luminance`(int) + `LightStatus`(string) + `Brightness`(int,0~100) 每 5s 上报；命令 `Light_Control_Led`（Led=ON/OFF/AUTO）；可写属性 `Threshold`(int)、`Brightness`(int 0~100)、`DimCurve`(string ≤64)（**全部必须"可读可写"**，否则下发报 IOTDA.000029)。

## 项目结构

```
├── C3_e53_sc1_pls/            # 固件源码（权威副本，改固件改这里；2026-08-31 起为 C++）
│   ├── e53_sc1_example.cpp    # 主逻辑：Wi-Fi → oc_mqtt 连 IoTDA、下行命令/属性、双任务、调光曲线
│   ├── src/E53_SC1.cpp        # BH1750 传感器 + 补光灯 GPIO7/PWM0 调光
│   ├── src/wifi_connect.cpp   # Wi-Fi 连接（复制自官方 D5/D9）
│   └── include/               # app_config.h（真实凭据，.gitignore 忽略）+ sdk_cxx.h（SDK 头封装）
├── bearpi-hm_nano/            # OpenHarmony 源码树（git submodule，编译目标；勿直接改 sample/）
├── backend/                   # Rust 后端
│   ├── src/                   # main/api/auth/iothub/assistant/openapi/webhook + tests
│   ├── migrations/            # 0001~0005：业务表/RBAC/知识库/super_admin/审计
│   ├── dev.sh                 # 统一入口：db/run/up/down/update/logs/status/help
│   ├── docker-compose.yml     # postgres + backend + nocodb 一键部署
│   └── README.md              # 后端详细文档（架构/API/RBAC/部署/规范）
├── frontend_vue/              # Vue3 前端（Element Plus + ECharts + Pinia）
├── build.sh                   # 固件编译：同步源码 + Docker 编译 + compile_commands.json
├── flash.sh                   # 烧录脚本（HiBurn 封装）
├── run.sh / start-frontend.bat# Windows 侧启动入口
├── AGENTS.md                  # AI 代理工作手册（本项目事实源）
├── 智慧路灯_基本功能清单.md      # 需求文档
└── 华为云IoTDA部署文档.md       # 华为云配置与部署步骤
```

## 环境变量

后端 `backend/.env`（模板 `.env.example`，凭据不入库/不进镜像）：

| 环境变量 | 说明 |
|---|---|
| `DATABASE_URL` | 本地默认 `postgres://streetlight:streetlight@127.0.0.1:5432/streetlight`；compose 内自动覆盖为服务名 `postgres` |
| `JWT_SECRET` | JWT 签名密钥，生产必改（建议 32+ 字节随机串） |
| `HUAWEI_AK` / `HUAWEI_SK` | 华为云访问密钥（SK 内部以 `SecretKey` 打码包装）；缺任一则北向功能停用 |
| `HUAWEI_PROJECT_ID` | 华为云项目 ID |
| `HUAWEI_IOTDA_ENDPOINT` | IoTDA 实例**应用侧**域名（`xxx.st1.iotda-app.{region}.myhuaweicloud.com`） |
| `HUAWEI_IOTDA_REGION` | V11 衍生签名所需区域；留空从 endpoint 域名自动推断 |
| `IOTDA_POLL_INTERVAL_SECS` | 影子轮询间隔秒数，默认 8；启用数据转发推送后建议 60 |
| `IOTDA_WEBHOOK_TOKEN` | 数据转发回调共享 token；配置后 `/api/iotda/callback` 要求 `Authorization: Bearer`（**公网部署必须配置**） |
| `ALLOWED_ORIGINS` | CORS 白名单（逗号分隔）；留空 = 开发模式全放开 |
| `BOOTSTRAP_SUPER_ADMIN_USERNAME/PASSWORD` | 引导系统管理员（默认 `superadmin` / `superadmin123`） |
| `BOOTSTRAP_ADMIN_USERNAME/PASSWORD` | 引导路灯管理员（默认 `admin` / `admin123`） |
| `DATABASE_POOL_SIZE` | 连接池上限，默认 20 |
| `ARGON2_MAX_CONCURRENCY` | Argon2 校验并发闸，默认 32（登录风暴防打爆内存） |
| `LOGIN_RATE_LIMIT_PER_MIN` | 登录限流：每 IP 每分钟次数，默认 30；0 = 不限流 |
| `IOTHUB_DRY_RUN` | 压测/联调：true 时北向调用本地短路（不发真实华为云请求） |

## 认证与角色

- 登录签发 **HS256 JWT**（24h）；全局中间件验签后按 `user_cache`（30s TTL）**复检账号活性**——禁用/删除/降权最迟 30s 对已签发 token 生效，角色取自数据库而非 token claims。
- 密码 **Argon2id** 哈希；策略 8~64 位且含字母+数字；登录对不存在/被禁用用户也跑 dummy 校验抹平时序（防用户名枚举）。
- 三角色：`municipal` 市政人员（监测/可视化/控制/参数/告警/指令）、`admin` 路灯管理员（全部业务权限，不含 `role:manage`）、`super_admin` 系统管理员（权限固定不可改）。
- 越权守卫：增删改 super_admin 账号/授予其角色仅限 super_admin 本人；禁用/删除/降级**最后一个启用的 super_admin** 会被拒（防锁死）；`perm_cache` 带 60s TTL，绕过 API 直接改库也最迟 60s 生效。
- 华为云北向必须用 **V11-HMAC-SHA256 衍生签名**（旧版 SDK-HMAC-SHA256 会 401）；标准版/企业版实例没有共享域名，设备侧/应用侧都要用**实例级域名**（详见部署文档）。

## 技术栈

| 层级 | 技术 |
|------|------|
| 硬件 | BearPi-HM Nano（Hi3861, RISC-V）+ E53_SC1（BH1750 + 补光灯） |
| 云平台 | 华为云 IoTDA（标准版实例，南向 MQTT 1883 + 北向 HTTPS V11 签名） |
| 后端 | Rust（axum + sqlx + reqwest），Swagger/OpenAPI 自动文档 |
| 数据库 | PostgreSQL 16（sqlx migrate 自动迁移） |
| 前端 | Vue3 + Vite + Element Plus + Pinia + ECharts |
| 认证 | JWT（HS256）+ Argon2id + 进程内 RBAC 权限缓存 |

## 常见问题

**Q: 后端启动报 "Address already in use"**
A: 8080 被占用，通常代理软件开了全局模式，改回规则模式或换端口。

**Q: 华为云北向调用 401（IOTDA.000002）**
A: 检查是否用了旧版 SDK-HMAC-SHA256——标准版/企业版必须 V11 衍生签名；再核对 `HUAWEI_AK/SK/PROJECT_ID` 与 `HUAWEI_IOTDA_ENDPOINT`（应用侧实例域名）、region 是否匹配。

**Q: 设备显示离线（板子明明连着）**
A: 后端有 **90s 本地失联检测**：`last_seen_at`（以 IoTDA 平台事件时间为心跳）超过 90s 未前进即标记离线。先检查 `app_config.h` 的设备 ID/密钥、WiFi 是否 2.4GHz、固件日志是否每 5s 上报；再确认设备已在 `POST /api/devices` 注册（未注册设备不被轮询）。

**Q: 命令下发超时（IOTDA.014111）/ 设备反复离线重启**
A: 确认固件用的是 1883 明文连接且 `CONFIG_APP_SERVERIP` 为设备侧实例域名；8883 MQTTS 在本工程不可用（证书解析会崩溃），不要启用。

**Q: 数据转发推送 401**
A: 配置了 `IOTDA_WEBHOOK_TOKEN` 后，IoTDA 控制台推送规则的自定义 Header 里必须加 `Authorization: Bearer <token>`（与 `IOTDA_WEBHOOK_TOKEN` 同值）；推送为主时建议 `IOTDA_POLL_INTERVAL_SECS=60`（轮询兜底校准）。

**Q: 阈值下发报 IOTDA.000029**
A: 产品模型 `Threshold` 必须勾选**可读可写**，只读属性不支持设置。

**Q: WSL 中 cargo 命令找不到**
A: 执行 `source ~/.cargo/env` 或重新打开 WSL 终端。

**Q: 固件编译报 symlink / 权限错误**
A: 在 WSL2 里跑 `./build.sh`（勿在 Windows 侧直接执行）；`bearpi-hm_nano` 是 submodule，首次需 `git submodule update --init`。

## 相关文档

- 后端详细文档（架构 / API 全说明 / RBAC / 部署 / 开发规范）：[`backend/README.md`](backend/README.md)
- AI 代理工作手册（仓库布局、固件工作流、踩坑记录、后端约定）：[`AGENTS.md`](AGENTS.md)
- 需求文档：[`智慧路灯_基本功能清单.md`](智慧路灯_基本功能清单.md)
- 华为云配置与部署：[`华为云IoTDA部署文档.md`](华为云IoTDA部署文档.md)
