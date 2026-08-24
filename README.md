# 智慧路灯(BearPi-HM Nano)

基于小熊派 **BearPi-HM Nano**(海思 Hi3861,RISC-V 32 位,OpenHarmony 轻量系统 + LiteOS-M)、E53_SC1 扩展板(BH1750 光照传感器 + 补光灯),经**华为云 IoTDA** 接入的智慧路灯项目。

需求文档:`智慧路灯_基本功能清单.md`;实施计划与踩坑记录:`华为云IoTDA实施计划.md`。

## 架构

```
Hi3861 --Wi-Fi/MQTT(oc_mqtt, 1883)--> 华为云 IoTDA(标准版实例, cn-south-1)
                                              ↑ 北向 API(HTTPS, AK/SK V11 衍生签名)
本地:Rust 后端(axum, 8080) --> PostgreSQL(Docker)
```

- **设备端**:BH1750 每 50ms 采样,本地按阈值联动开关灯(断网可用),每 5s 上报影子属性;
- **云端**:IoTDA 保存设备影子与在线状态,负责下行命令/属性转发;
- **后端**:每 8s 轮询影子入库,对前端提供 REST API(前端不在本仓库);含账号登录 + RBAC 权限(见下文)。

## 已实现功能(全链路已验收)

- 实时光照监测、历史数据查询
- 光照联动开关灯(施密特触发 + 迟滞带,防开灯自照引起的频闪)
- 远程控灯(ON / OFF / AUTO 恢复联动)
- 光照阈值云端下发(可写属性 `Threshold`)
- 设备在线状态监控、离线告警(恢复自动消解)
- 控制指令留痕(动作/来源/北向受理结果,可审计)
- 设备管理(注册、位置、删除)
- 账号 / 登录 / RBAC(JWT + Argon2id,角色:市政人员 / 路灯管理员)
- Swagger UI 接口文档与在线调试(`/docs`)
- 告警人工处理(标记已处理 / 恢复未处理)
- 仪表盘聚合、全局光照 / 指令查询

## 目录说明

| 路径 | 内容 |
|---|---|
| `C3_e53_sc1_pls/` | 固件源码(权威副本,改固件只改这里;基于官方 E53_SC1 + D9 样例) |
| `bearpi-hm_nano/` | OpenHarmony 源码树(git submodule,gitee 官方仓库;build.sh 自动同步样例进去再编译) |
| `backend/` | Rust 后端(axum + sqlx + reqwest):`src/`、`migrations/`(PostgreSQL 建库脚本,启动自动执行)、`infra-up.sh`(起数据库) |
| `build.sh` / `flash.sh` | Docker 一键编译 / 一键烧录 |
| `gen-compdb.sh` | 重新生成 clangd 用的 compile_commands.json |
| `bearpi-serial.ps1` | 串口日志查看脚本(Windows PowerShell) |
| `tools/hiburn_windows/` | HiBurn 烧录工具(Windows 版) |

## 快速开始

### 0. 克隆

本仓库用 **git submodule** 携带 OpenHarmony 源码树,克隆时必须加 `--recursive`:

```bash
git clone --recursive <仓库地址>
# 已克隆的补上:git submodule update --init
```

### 1. 固件

前置:WSL2 Ubuntu + Docker(镜像 `openharmony/openharmony-docker:0.0.3`)。

复制 `C3_e53_sc1_pls/include/app_config.example.h` 为 `app_config.h`,填入你的 Wi-Fi SSID/密码和 IoTDA 设备 ID/密钥(该文件被 .gitignore 忽略,不会进仓库);IoTDA 实例设备侧域名改 `C3_e53_sc1_pls/e53_sc1_example.c` 顶部的 `CONFIG_APP_SERVERIP`。然后:

```bash
./build.sh        # 编译(自动同步样例、启用 BUILD.gn)
./flash.sh 4      # 烧录:HiBurn 弹出后按一下开发板 RESET,烧完再按一次 RESET 运行
```

串口日志(115200):`pwsh -File bearpi-serial.ps1`

### 2. 后端

数据库是 **PostgreSQL**;表结构由后端启动时自动创建(`migrations/`,sqlx 迁移),**无需手工导入任何 SQL**。

```bash
backend/infra-up.sh         # 启动 PostgreSQL(容器 streetlight-postgres)
cd backend
cp .env.example .env        # 填华为云 AK/SK、项目 ID、实例应用侧域名、区域(含 JWT_SECRET)
cargo run                   # 监听 8080,首次启动自动建表并创建引导管理员
```

首次启动时若账号表为空,后端按 `BOOTSTRAP_ADMIN_USERNAME` / `BOOTSTRAP_ADMIN_PASSWORD` 创建管理员
(默认 `admin` / `admin123`,仅开发用)。浏览器打开 `http://127.0.0.1:8080/docs` 即 Swagger UI:
先调 `POST /api/auth/login` 拿 token,点右上角 **Authorize** 填入 `Bearer <token>` 即可在线调试所有接口。

### 3. REST API(端口 8080)

除 `/api/health`、`/api/auth/login`、`/docs` 外,其余接口都需要请求头 `Authorization: Bearer <token>`。
权限码在登录响应的 `permissions` 里;路由需要的权限见下表(路灯管理员拥有全部权限)。

| 方法 | 路径 | 权限码 | 说明 |
|---|---|---|---|
| GET | `/api/health` | 公开 | 健康检查 |
| POST | `/api/auth/login` | 公开 | 登录,返回 token / 用户 / 角色 / 权限码 |
| GET | `/api/auth/me` | 登录 | 当前登录用户 |
| GET | `/api/dashboard` | `device:status` | 首页聚合(设备/告警/24h 光照与指令统计) |
| GET/POST | `/api/devices` | `device:status` / `device:manage` | 设备列表 / 注册(可带 name、location) |
| PATCH/DELETE | `/api/devices/:id` | `device:manage` | 修改设备资料 / 删除设备及全部关联数据 |
| GET | `/api/devices/:id/lux/latest` | `luminance:monitor` | 实时光照 |
| GET | `/api/devices/:id/lux/history?from=&to=` | `luminance:history` | 历史光照(RFC3339,倒序,上限 5000) |
| GET | `/api/devices/:id/lux/stats?from=&to=` | `luminance:history` | 条数 / 最低 / 最高 / 平均 / 最新 |
| GET | `/api/lux/latest` | `luminance:monitor` | 所有设备最新光照 |
| POST | `/api/devices/:id/lamp` | `control:manual` | 控灯 `{"action":"on\|off\|auto"}` |
| GET | `/api/devices/:id/commands?from=&to=&limit=` | `command:log` | 单设备指令留痕 |
| GET | `/api/commands?device_id=&from=&to=&limit=` | `command:log` | 全局指令留痕 |
| GET/PUT | `/api/devices/:id/threshold` | `config:threshold` | 阈值查询/下发(0~10000) |
| GET | `/api/alarms?device_id=&resolved=&from=&to=&type=&limit=` | `alarm:log` | 告警记录 |
| PATCH | `/api/alarms/:id` | `alarm:log` | `{"resolved":true/false}` 处理 / 恢复告警 |
| GET/POST | `/api/users` | `user:manage` | 账号列表 / 新增(密码 6~64 位) |
| DELETE | `/api/users/:id` | `user:manage` | 删除账号(不能删自己) |
| GET | `/api/roles` | `user:manage` | 角色列表 |
| GET | `/api/permissions` | `user:manage` | 权限点列表 |
| PUT | `/api/roles/:id/permissions` | `user:manage` | 更新角色-权限映射 |

注意:控灯/阈值是透传 IoTDA 北向的,设备离线时北向拒绝(返回 502 带原因);`sent` 仅表示北向已受理,不代表灯已动作。

## IoTDA 侧配置要点

1. 创建**标准版实例**,在"实例 → 接入信息"记下**设备侧域名**(填固件,本项目使用 1883 明文 MQTT,不要用 8883,原因见"安全说明")和**应用侧域名**(填 `.env`);标准版没有区域共享域名。
2. 创建产品,模型定义服务 `Light`:属性 `Luminance`(int)、`LightStatus`(string)、`Threshold`(int,**必须可读可写**);命令 `Light_Control_Led`(参数 `Led`:ON/OFF/AUTO)。
3. 注册设备,拿到设备 ID / 密钥(填 `app_config.h`)。
4. IAM 创建用户(AK/SK),用户组挂 IoTDA 权限(如 `IoTDA:*:*`),授权有数分钟传播延迟。

## 硬件连接

- BH1750:I2C1(GPIO0=SDA / GPIO1=SCL,400kHz),地址 0x23,连续低分辨率模式
- 补光灯:GPIO7,高电平点亮
- 日志:printf → UART0 → 板载 CH340E → USB → Windows COM 口
- 注意:Hi3861 仅支持 2.4GHz Wi-Fi

## 安全说明

- 设备 → IoTDA 使用 1883 明文 MQTT。**不要启用 8883 MQTTS**:本工程 iot_link 内置的 mbedtls 在 Hi3861 上运行 TLS 存在稳定性问题(证书解析阶段触发内核异常,单条 SUBSCRIBE 最长 90s 后失败,断开清理阶段 panic,设备陷入重启循环,云端命令下发超时 IOTDA.014111;2026-08-24 实测)。根 CA 保留在 `include/iotda_ca.h` 备用,问题解决前不要启用。
- Wi-Fi 密码与设备密钥只存在于本地 `app_config.h`(被 .gitignore 忽略),不进任何 git 仓库。

## 已知坑(脚本已内置修复,勿回退)

- HiBurn 读不了 `\\wsl.localhost` UNC 路径 → flash.sh 暂存到 Windows `%TEMP%` 再烧;COM 参数必须数字格式 `-com:4`
- 串口独占:HiBurn 烧录时看不了日志
- Docker Desktop 读不了 WSL 路径 bind mount → 数据库用 WSL 原生 docker(`infra-up.sh`)
- 充电器供电时若上电不启动,按一下 RESET

## 迭代流程

- 固件:改 `C3_e53_sc1_pls/` → `./build.sh` → `./flash.sh 4` → RESET ×2 → 串口验证
- 后端:改 `backend/` → `cargo build` → curl 验证 REST API
- 数据库 schema:未上线前可直接改 `backend/migrations/0001_init.sql` 并清卷重建;上线后必须新建递增迁移
