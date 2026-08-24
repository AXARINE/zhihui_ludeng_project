# 智慧路灯(BearPi-HM Nano)

基于小熊派 **BearPi-HM Nano** 开发板(海思 Hi3861,RISC-V 32 位,OpenHarmony 轻量系统 + LiteOS-M)、E53_SC1 传感器扩展板(BH1750 光照传感器 + 补光灯)和**华为云 IoTDA** 的智慧路灯项目。

需求见 `04_智慧路灯_基本功能清单.md`,实施计划与踩坑记录见 `05_华为云IoTDA实施计划.md`,AI 代理工作手册见 `AGENTS.md`。

## 架构

```
Hi3861 --Wi-Fi/MQTT(oc_mqtt)--> 华为云 IoTDA(标准版实例, cn-south-1)
                                     ↑ 北向 API(HTTPS, AK/SK V11 衍生签名)
本地 WSL:Rust 后端(axum, 8080) --> PostgreSQL(Docker)
```

## 已实现功能(全链路已验收)

- 实时光照监测(BH1750,设备每 5s 上报,后端每 8s 轮询入库)
- 历史光照数据(PostgreSQL 存储,REST 查询)
- 光照联动开关灯(设备端本地判断,断网可用)
- 手动远程控灯(ON / OFF / AUTO 恢复联动)
- 光照阈值云端下发(可写属性 `Threshold`)
- 设备在线状态监控 + 离线告警(以 IoTDA 设备状态为准,恢复自动消解)
- 设备注册/管理 REST API

## 目录说明

| 路径 | 内容 |
|---|---|
| `C3_e53_sc1_pls/` | 固件源码(基于官方 E53_SC1 + D9_iot_cloud_oc_light 样例) |
| `backend/` | Rust 后端(axum + sqlx + reqwest,IoTDA 北向客户端)、`migrations/`(PostgreSQL 建库脚本,后端启动时自动执行)、`infra-up.sh`(启动 PostgreSQL,WSL 原生 docker) |
| `build.sh` / `flash.sh` | Docker 一键编译 / 一键烧录 |
| `gen-compdb.sh` | 重新生成 clangd 用的 compile_commands.json |
| `bearpi-hm_nano/` | OpenHarmony 源码树(git submodule,指向 gitee 官方仓库;build.sh 自动把固件样例同步进去再编译) |
| `bearpi-serial.ps1` | 串口日志查看脚本(Windows PowerShell) |
| `tools/hiburn_windows/` | HiBurn 烧录工具(Windows 版) |

## 快速开始

### 1. 固件

前置:WSL2 Ubuntu + Docker(镜像 `openharmony/openharmony-docker:0.0.3`)。本仓库用 **git submodule** 携带 OpenHarmony 源码树(`bearpi-hm_nano/`,gitee 官方仓库),克隆时加 `--recursive`(或克隆后 `git submodule update --init`);`sample/BUILD.gn` 的样例启用由 build.sh 自动处理。

复制 `C3_e53_sc1_pls/include/app_config.example.h` 为 `app_config.h`,填入你的 Wi-Fi SSID/密码和 IoTDA 设备 ID/密钥(该文件被 .gitignore 忽略);IoTDA 实例设备侧域名改 `C3_e53_sc1_pls/e53_sc1_example.c` 顶部的 `CONFIG_APP_SERVERIP`。然后:

```bash
./build.sh        # 编译
./flash.sh 4      # 烧录(HiBurn 弹出后按一下 RESET)
# 烧完再按一次 RESET 运行
```

串口日志(Windows PowerShell,115200):`pwsh -File bearpi-serial.ps1`

### 2. 后端

```bash
backend/infra-up.sh         # 启动 PostgreSQL
cd backend
cp .env.example .env        # 填华为云 AK/SK、项目 ID、实例应用侧域名、区域
cargo run                   # 监听 8080
```

REST API(端口 8080):

| 方法 | 路径 | 说明 |
|---|---|---|
| GET/POST/DELETE | `/api/devices[/:id]` | 设备列表/注册/删除 |
| GET | `/api/devices/:id/lux/latest` | 实时光照 |
| GET | `/api/devices/:id/lux/history?from=&to=` | 历史光照(RFC3339 时间) |
| POST | `/api/devices/:id/lamp` | 控灯 `{"action":"on\|off\|auto"}` |
| GET | `/api/devices/:id/commands` | 控制指令留痕(审计) |
| GET/PUT | `/api/devices/:id/threshold` | 阈值查询/下发 |
| GET | `/api/alarms?device_id=&resolved=` | 告警记录 |

## IoTDA 侧配置要点

1. 创建**标准版实例**,记下接入信息里的**设备侧域名**(填固件)和**应用侧域名**(填 `.env`)。
2. 创建产品,模型定义服务 `Light`:属性 `Luminance`(int)、`LightStatus`(string)、`Threshold`(int,**可读可写**);命令 `Light_Control_Led`(参数 `Led`:ON/OFF/AUTO)。
3. 注册设备,拿到设备 ID / 密钥。
4. IAM 创建用户(AK/SK),用户组挂 IoTDA 权限(如 `IoTDA:*:*`)。

## 硬件连接

- BH1750:I2C1(GPIO0=SDA / GPIO1=SCL,400kHz),地址 0x23,连续低分辨率模式
- 补光灯:GPIO7,高电平点亮
- 日志:printf → UART0 → 板载 CH340E → USB → Windows COM 口
- 注意:Hi3861 仅支持 2.4GHz Wi-Fi

## 已知坑(脚本已内置修复,勿回退)

- HiBurn 读不了 `\\wsl.localhost` UNC 路径 → flash.sh 暂存到 Windows `%TEMP%` 再烧;COM 参数必须数字格式 `-com:4`
- 串口独占:HiBurn 烧录时看不了日志
- Docker Desktop 读不了 WSL 路径 bind mount → 数据库用 WSL 原生 docker(`infra-up.sh`)
- 充电器供电时若上电不启动,按一下 RESET

## 迭代流程

固件:改 `C3_e53_sc1_pls/` → `./build.sh` → `./flash.sh 4` → RESET ×2 → 串口验证。
后端:改 `backend/` → `cargo build` → curl 验证 REST API。
