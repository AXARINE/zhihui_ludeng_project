# AGENTS.md — BearPi-HM Nano 智慧路灯项目指南

本文件是 AI 代理操作本项目的工作手册,假定读者对本项目一无所知。项目根 = `~/bearpi`(WSL2 Ubuntu 内,Windows 侧访问路径为 `\\wsl.localhost\Ubuntu\home\alkari\bearpi`)。用户交流语言:中文。

## 项目是什么

小熊派 **BearPi-HM Nano** 开发板(海思 **Hi3861**,RISC-V 32 位,352KB SRAM / 2MB Flash,运行 OpenHarmony 轻量系统 + LiteOS-M 内核)的智慧路灯项目与开发环境。基于 E53_SC1 传感器扩展板(BH1750 光照传感器 + 补光灯),用 **WSL2 + Docker** 代替官方 VMware 镜像做编译环境。

需求文档 `04_智慧路灯_基本功能清单.md` 是完整愿景;实施计划 `05_华为云IoTDA实施计划.md`。**当前架构(2026-08 起,已全链路验收)**:

```
Hi3861 --Wi-Fi/MQTT(oc_mqtt)--> 华为云 IoTDA(标准版实例, cn-south-1)
                                     ↑ 北向 API(HTTPS, AK/SK V11 衍生签名)
本地 WSL:Rust 后端(axum, 8080) --> PostgreSQL(WSL 原生 docker)
```

已实现并验证:实时光照、历史数据、光照联动(auto 本地阈值)、手动控灯(ON/OFF/AUTO)、阈值下发、在线状态、离线告警(断连→告警、恢复→自动消解)、设备管理。前端不在本仓库范围;RAG 问答未做。

## 三个 git 仓库与代码权威来源

| 仓库 | 路径 | 说明 |
|---|---|---|
| 环境仓库 | `~/bearpi` 本身 | 构建/烧录脚本、需求文档、工具。无远程 |
| 智慧路灯仓库 | `~/bearpi/smart-street-light/` | **当前主战场**,远程 = `https://github.com/AXARINE/zhihui_ludeng_project.git`。固件源码的**权威副本**在 `smart-street-light/C3_e53_sc1_pls/`,改固件改这里 |
| 源码树仓库 | `~/bearpi/bearpi-hm_nano/` | 官方 gitee 仓库 `bearpi/bearpi-hm_nano`(master)的检出。**不要直接改** `sample/C3_e53_sc1_pls/` —— 它是 smart-street-light 的同步副本,直接改会在下次 `./build.sh` 时被覆盖 |

同步是单向的:`smart-street-light/build.sh` 把本仓库样例 `cp -r` 覆盖进源码树后再编译。

## 目录结构

| 路径 | 内容 |
|---|---|
| `smart-street-light/C3_e53_sc1_pls/` | 固件源码:`e53_sc1_example.c`(主逻辑)、`src/E53_SC1.c`(传感器/灯)、`src/wifi_connect.c`(Wi-Fi 连接,复制自官方 D5/D9 样例)、`include/`、`BUILD.gn` |
| `smart-street-light/server/` | 后端:`backend/`(Rust)、`infra-up.sh`(起数据库)、`docker-compose.yml` + `mosquitto/`(v1 本地直连方案遗留,主链路不用) |
| `smart-street-light/` 其余 | `build.sh` / `flash.sh` / `gen-compdb.sh`、`bearpi-serial.ps1`、`tools/hiburn_windows/`、需求文档与实施计划 |
| `bearpi-hm_nano/` | OpenHarmony 源码树(`applications/`、`kernel/`、`vendor/`、`out/` 等) |
| `~/bearpi` 根目录 | 根 `build.sh`/`flash.sh`/`gen-compdb.sh`(不同步 smart-street-light)、`compile_commands.json` + `.clangd`、`tools/`、`raw_notes/` |

## 核心工作流(改固件代码,按顺序)

1. **改代码**:编辑 `smart-street-light/C3_e53_sc1_pls/` 下的源码。
2. **编译**(WSL 内):`cd ~/bearpi/smart-street-light && ./build.sh` → 同步样例进源码树 → Docker 镜像 `openharmony/openharmony-docker:0.0.3` 内编译 → 产物 `bearpi-hm_nano/out/BearPi-HM_Nano/Hi3861_wifiiot_app_allinone.bin` → 更新 `compile_commands.json`。
3. **烧录**(WSL 内):`./flash.sh 4`(板子 COM4)→ HiBurn 窗口打开后**提示用户按 RESET** → `FLASH OK (HiBurn exit 0)`。
4. **启动新固件**:烧完**再按一次 RESET** 才运行新程序。充电器供电时上电若不启动,同样按 RESET。
5. **看日志**:115200 波特率,见下文。

## 固件现状(C3_e53_sc1_pls IoTDA 版,已验收)

- **两个任务**:`task_main_entry`(10KB 栈,Wi-Fi → oc_mqtt 连 IoTDA,队列处理下行命令/属性设置/上报)、`task_sensor_entry`(4KB 栈,50ms 采样 + 本地灯控 + 每 5s 推上报)。
- **联网配置**:`e53_sc1_example.c` 顶部 `CONFIG_WIFI_SSID/PWD`、`CONFIG_APP_SERVERIP`(IoTDA **实例设备侧域名**,`xxx.st1.iotda-device.cn-south-1.myhuaweicloud.com`)、`CONFIG_APP_DEVICEID/DEVICEPWD`。**这些是明文真实凭据,已在 git 历史中——分享仓库前注意**。
- **产品模型**(服务 `Light`,建产品时手工建的):属性 `Luminance`(int)+ `LightStatus`(string)每 5s 上报;命令 `Light_Control_Led`(Led=ON/OFF/AUTO);可写属性 `Threshold`(int,**必须"可读可写"**,否则下发报 IOTDA.000029)。
- **控制模式**:auto(默认,`Lux < Threshold` 本地开关灯)/ manual(收到 ON/OFF 进入,收到 AUTO 恢复)。
- BH1750 连续低分辨率模式(0x13,16ms);换算系数 `result * 7`;补光灯 GPIO7 高电平点亮。
- Hi3861 只支持 **2.4GHz Wi-Fi**;实测 WPA2/WPA3 混合模式路由器可连。

## 后端服务(smart-street-light/server/)

- **启动数据库**:`server/infra-up.sh`(**WSL 原生 docker**,容器 `streetlight-postgres`,卷 `streetlight-pgdata`;不用 Docker Desktop——它读不了 `\\wsl.localhost` 的 bind mount)。
- **运行后端**:`cd server/backend && set -a && . ./.env && set +a && cargo run`(需 Rust stable + 系统 gcc;`.env` 从 `.env.example` 复制,填 AK/SK、项目 ID、实例应用侧域名、区域)。端口 8080。
- 结构:`main.rs`(装配)、`iothub.rs`(北向客户端 + 8s 轮询)、`api.rs`(REST)、`migrations/0001_init.sql`(device/lux_record/config/alarm 四表)。
- REST API:`GET/POST/DELETE /api/devices[/:id]`、`GET /api/devices/:id/lux/latest|history`、`POST /api/devices/:id/lamp` `{"action":"on|off|auto"}`、`GET/PUT /api/devices/:id/threshold`、`GET /api/alarms`。设备需先 `POST /api/devices` 注册才会被轮询。
- **影子只在设备 ONLINE 时入库**(离线时影子保留最后值,直接入库会每分钟灌 7 条假数据——踩过)。

## 华为云 IoTDA 的关键事实(踩坑记录)

- **标准版/企业版实例没有共享域名**:cn-south-1 等区域的 `iotda.{region}.myhuaweicloud.com` 根本不存在(DNS NXDOMAIN)。用**实例级域名**:设备侧 `xxx.st1.iotda-device.{region}.myhuaweicloud.com`(MQTT 1883/8883),应用侧 `xxx.st1.iotda-app.{region}.myhuaweicloud.com`(HTTPS),在控制台"实例 → 接入信息"查看,设备连接密钥文件里也有。
- **北向 API 签名必须用 V11-HMAC-SHA256 衍生算法**(旧 SDK-HMAC-SHA256 会 401 IOTDA.000002):`info = yyyymmdd/{region}/iotdm`;`PRK = HMAC(key=AK, data=SK)`;`T1 = HMAC(key=PRK, data=info‖0x01)`;签名密钥 = **hex 编码的 T1**(注意是 hex 字符串当 key,不是原始字节)。
- 规范请求两个易错点:URI **必须以 `/` 结尾**(实际请求路径不带);头部块与 SignedHeaders 之间**多一个空行**。
- 修改设备属性是 **PUT** `/v5/iot/{project}/devices/{id}/properties`(不是 POST);下发命令是 POST `.../commands`。
- IAM 权限:AK/SK 所属 IAM 用户需在**用户组**里挂 IoTDA 权限策略(本项目用自定义策略 `{"Action": ["IoTDA:*:*"]}`,范围所有资源),授权有几分钟传播延迟。
- 排查北向问题可用官方 Python SDK 对照:`~/.cache/hwsdk`(pip --target 装的 huaweicloudsdkiotda,注意装 `huaweicloudsdkiam` 会清掉 iotda 包,需重装)。

## 看设备输出(printf 日志)

链路:printf → UART0(GPIO3/GPIO4)→ 板载 CH340E → Type-C USB → Windows COM4 → 串口终端。

| 方式 | 命令/工具 |
|---|---|
| Windows PowerShell | `pwsh -File C:\Users\Alkari\Desktop\bearpi-serial.ps1`(默认 COM4,可 `-Com 5`) |
| WSL 终端内借道 | `powershell.exe -ExecutionPolicy Bypass -File 'C:\Users\Alkari\Desktop\bearpi-serial.ps1'`(路径必须 Windows 格式 + 单引号) |
| 图形工具 | MobaXterm → Serial → COM4 / 115200 |

注意:串口独占(HiBurn 开着时看不了);想重播启动日志按 RESET;WSL2 看不到 COM 口;板子接充电器供电时**没有串口可看**,此时判断固件是否运行:手捂光敏传感器,补光灯亮 = 固件在跑(auto 联动正常)。

## 烧录的已知坑(flash.sh 已内置修复,勿回退)

- HiBurn.exe 在 WSL 文件系统里缺执行权限 → 脚本自动 `chmod +x`。
- HiBurn 读不了 `\\wsl.localhost` 的 UNC 路径(退出码 52/17)→ 脚本自动把 HiBurn + 固件暂存到 Windows 本地临时目录 `%TEMP%\bearpi-flash` 再烧。
- COM 参数必须用**数字格式** `-com:4`,`-com:COM4` 会秒退。
- HiBurn 退出码 17 = 烧录失败或窗口被关(板子没插线时最常见)。

## Zed/clangd 环境(已修好,勿破坏)

- Zed 的 clangd 插件在 WSL 里运行,项目根 = `/home/alkari/bearpi`。
- `compile_commands.json` 由 `build.sh` 编译成功后自动生成/更新;手动重生成用 `gen-compdb.sh`。
- `.clangd`:`CompileFlags.Remove` 剔除 GCC 独有参数(`-mtune=size` 会直接让 clangd 崩溃)+ `Add` 两个 `-isystem` 指向工具链头文件。
- 交叉工具链在 `~/tools/gcc_riscv32`(GCC **7.3.0**,华为定制)。**不要升级工具链**。

## 操作本项目时的注意事项

- 所有文件在 WSL 9P 挂载上:Windows 侧的 write/edit 工具可能报 `GetFileSecurityW EIO` → 改用 pwsh 的 `Set-Content` 或 WSL 内直接编辑。
- git 操作在 WSL 内执行(Windows git 对 WSL 仓库报 dubious ownership)。三个仓库独立。
- Docker 容器以 root 运行,`bearpi-hm_nano/out/` 产物属 root,源码树仓库 `git status` 报 Permission denied 属已知现象。
- 烧录和看日志需要**用户物理操作**(按 RESET、插拔线),代理替代不了,流程中要明确提示用户。
- 改动固件代码后的验收 = `./build.sh` 编译通过 + 烧录后串口日志正确。
- 改动后端后验收 = `cargo build` 通过 + curl REST API 验证(可伪造场景:离线告警靠拔电)。
