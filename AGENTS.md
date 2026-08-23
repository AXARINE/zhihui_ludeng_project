# AGENTS.md — BearPi-HM Nano 智慧路灯项目指南

本文件是 AI 代理操作本项目的工作手册,假定读者对本项目一无所知。项目根 = `~/bearpi`(WSL2 Ubuntu 内,Windows 侧访问路径为 `\\wsl.localhost\Ubuntu\home\alkari\bearpi`)。用户交流语言:中文。

## 项目是什么

小熊派 **BearPi-HM Nano** 开发板(海思 **Hi3861**,RISC-V 32 位,352KB SRAM / 2MB Flash,运行 OpenHarmony 轻量系统 + LiteOS-M 内核)的智慧路灯项目与开发环境。基于 E53_SC1 传感器扩展板(BH1750 光照传感器 + 补光灯),用 **WSL2 + Docker** 代替官方 VMware 镜像做编译环境。

需求文档 `04_智慧路灯_基本功能清单.md` 描述的是完整愿景(光照上报、后端、前端 ECharts 展示、阈值管理、心跳/离线告警、RAG 维护问答等)。实施计划见 `05_华为云IoTDA实施计划.md`。**当前架构(2026-08 起)**:设备端接**华为云 IoTDA**(oc_mqtt,参考官方 D9 样例);本地 Rust 后端(axum)通过 **IoTDA 北向 API**(AK/SK V11-HMAC-SHA256 衍生签名)轮询设备影子入库 PostgreSQL 并提供 REST API。前端不在本仓库范围内(由他人负责)。

## 两个 git 仓库与代码权威来源

| 仓库 | 路径 | 说明 |
|---|---|---|
| 环境仓库 | `~/bearpi` 本身 | 构建/烧录脚本、需求文档、工具。无远程;`.gitignore` 忽略 `bearpi-hm_nano/`、`tools/`、`raw_notes/`、`compile_commands.json`、`.clangd`、`AGENTS.md`、`.agent`、`.cache` |
| 智慧路灯仓库 | `~/bearpi/smart-street-light/` | **当前主战场,独立 git 仓库(已有初始提交、无远程)**。固件源码的**权威副本**在 `smart-street-light/C3_e53_sc1_pls/`,改固件代码改这里 |
| 源码树仓库 | `~/bearpi/bearpi-hm_nano/` | 官方 gitee 仓库 `bearpi/bearpi-hm_nano`(master)的检出,OpenHarmony 全量源码。**不要直接改** `sample/C3_e53_sc1_pls/` —— 它是 smart-street-light 仓库的同步副本,直接改会在下次 `./build.sh` 时被覆盖 |

同步是单向的:`smart-street-light/build.sh` 第 16 行 `cp -r` 把本仓库样例覆盖进源码树后再编译。反向不会自动发生。

## 目录结构

| 路径 | 内容 |
|---|---|
| `smart-street-light/` | 智慧路灯项目本体:`C3_e53_sc1_pls/`(固件源码:`e53_sc1_example.c` + `src/E53_SC1.c` + `src/wifi_connect.c` + `include/` + `BUILD.gn`)、`server/`(后端,见下文)、`build.sh` / `flash.sh` / `gen-compdb.sh`(支持 `BEARPI_ROOT` 环境变量)、`bearpi-serial.ps1`、`tools/hiburn_windows/`、`README.md`、需求文档与实施计划 |
| `bearpi-hm_nano/` | OpenHarmony 源码树。顶层:`applications/`(应用)、`base/` `foundation/`(系统服务)、`kernel/`、`drivers/`、`build/`(构建脚本)、`vendor/`、`out/`(编译产物) |
| `bearpi-hm_nano/applications/BearPi/BearPi-HM_Nano/sample/` | 官方样例:A 系列(内核)、B 系列(基础外设)、C 系列(E53 传感器)、D 系列(物联网/云)、Z 系列(开发者贡献)。每个样例一个目录,内含 `BUILD.gn` + 源码 |
| `build.sh` / `flash.sh` / `gen-compdb.sh`(根目录) | 对**源码树当前内容**直接编译/烧录(不做 smart-street-light 同步);日常迭代用 smart-street-light 里的同名脚本 |
| `compile_commands.json` + `.clangd` | Zed/clangd 的编译数据库与配置(由 build.sh 维护,git 已忽略) |
| `tools/hiburn_windows/` | HiBurn 烧录工具(Windows 版,与 smart-street-light 内副本一致) |
| `raw_notes/` | 嵌入式教程笔记原始稿(纯文本,与构建无关,git 已忽略) |
| `04_智慧路灯_基本功能清单.md` | 智慧路灯需求文档(与 smart-street-light 内副本一致) |
| `.agent/` / `.cache/` | 空目录 / clangd 缓存,均已被 git 忽略 |

## 核心工作流(改固件代码,按顺序)

1. **改代码**:编辑 `smart-street-light/C3_e53_sc1_pls/` 下的源码。
2. **编译**(WSL 内):`cd ~/bearpi/smart-street-light && ./build.sh` → 先把样例同步进源码树(并校验 `sample/BUILD.gn` 已启用 `"C3_e53_sc1_pls:e53_sc1_example"`,当前已启用)→ Docker 镜像 `openharmony/openharmony-docker:0.0.3` 内执行 `python build.py BearPi-HM_Nano` → 产物 `bearpi-hm_nano/out/BearPi-HM_Nano/Hi3861_wifiiot_app_allinone.bin` → 自动更新 `compile_commands.json`。
3. **烧录**(WSL 内):`./flash.sh 4`(当前板子是 COM4)→ HiBurn 窗口打开后**提示用户按开发板 RESET 键**开始烧录 → `FLASH OK (HiBurn exit 0)`。
4. **启动新固件**:烧录完成后**再按一次 RESET** 才会运行新程序(芯片烧完停在下载模式)。
5. **看日志**:115200 波特率,见下文。

切换其他官方样例时:编辑 `bearpi-hm_nano/applications/BearPi/BearPi-HM_Nano/sample/BUILD.gn`,在 `features` 里启用目标(格式 `"目录名:目标名"`)、屏蔽其他,然后用根目录 `build.sh` 编译。

## 构建系统与代码组织

- 构建基于 OpenHarmony 轻量系统的 **GN + Ninja**:`build.py` 是入口,组件用 `lite_component` 声明,`features` 按 `"目录:目标"` 引用各样例目录里的 `BUILD.gn` 目标(样例自身是 `static_library`)。
- 应用代码写法:CMSIS-RTOS2 API(`cmsis_os2.h`,`osThreadNew` 创建任务),用 `ohos_init.h` 的 `APP_FEATURE_INIT()` 宏注册入口;外设操作走 Hi3861 SDK 的 `wifiiot_gpio.h` / `wifiiot_i2c.h` / `wifiiot_gpio_ex.h` 等头文件。
- 代码风格:跟随官方样例 —— C 语言、Apache 2.0 许可证头、中文函数头注释块(函数名称/说明/参数/返回值)。改样例时保持原文件风格。
- **没有单元测试框架**;验证方式 = 编译通过 + 烧录后看串口输出是否符合预期。

## 当前固件状态(C3_e53_sc1_pls IoTDA 版)

涉及文件:`smart-street-light/C3_e53_sc1_pls/e53_sc1_example.c`、`src/E53_SC1.c`、`src/wifi_connect.c`(复制自 D5/D9 样例)、`include/`(源码树内为同步副本,当前一致)。

- **两个任务**:`task_main_entry`(10KB 栈,Wi-Fi → oc_mqtt 连 IoTDA,队列处理下行命令/属性设置/上报请求)、`task_sensor_entry`(4KB 栈,50ms 采样 + 本地灯控 + 每 5s 推上报消息)。
- **联网配置**:`e53_sc1_example.c` 顶部 `CONFIG_WIFI_SSID/PWD`、`CONFIG_APP_SERVERIP`(IoTDA 接入域名,当前 cn-south-1)、`CONFIG_APP_DEVICEID/DEVICEPWD` 已填真实值。**注意这些值(尤其密钥)在固件源码里是明文,提交/分享前留意**。
- **产品模型**(IoTDA 服务 `Light`):属性 `Luminance`(int)+ `LightStatus`(string)每 5s 上报;命令 `Light_Control_Led`(Led=ON/OFF/AUTO);可写属性 `Threshold`(属性设置回调更新运行时阈值,默认 40)。
- **控制模式**:auto(默认,`Lux < 阈值` 本地开关灯)/ manual(收到 ON/OFF 命令后进入,光照逻辑暂停;收到 AUTO 恢复)。
- BH1750 连续低分辨率模式(0x13,16ms);换算系数 `result * 7`;补光灯 GPIO7 高电平点亮。

## 后端服务(smart-street-light/server/)

- `server/docker-compose.yml`:PostgreSQL 16(Mosquitto 为 v1 本地方案遗留,主链路不再使用)。
- **启动数据库**:Docker Desktop 读不了 `\\wsl.localhost` bind mount(同 flash.sh 的坑)→ 用 `server/infra-up.sh`(自动复制到 `%TEMP%\streetlight-server` 后 `docker.exe compose up -d`)。
- `server/backend/`:Rust(axum + sqlx + reqwest)。**运行前**:复制 `.env.example` 为 `.env` 填华为云 AK/SK、项目 ID、区域;`cd server/backend && set -a && . ./.env && set +a && cargo run`。端口 8080。
- 结构:`main.rs`(装配)、`iothub.rs`(IoTDA 北向客户端,AK/SK 的 V11-HMAC-SHA256 衍生签名 —— 标准版实例必须,旧版 SDK-HMAC-SHA256 会 401 IOTDA.000002 + 8s 轮询影子/设备状态入库)、`api.rs`(REST)、`migrations/0001_init.sql`(device/lux_record/config/alarm 四表)。
- REST API:`GET/POST/DELETE /api/devices[/:id]`、`GET /api/devices/:id/lux/latest|history`、`POST /api/devices/:id/lamp` `{"action":"on|off|auto"}`、`GET/PUT /api/devices/:id/threshold`、`GET /api/alarms`。
- 设备需先 `POST /api/devices` 注册(用 IoTDA 的设备 ID)才会被轮询;离线告警以 IoTDA 设备状态(ONLINE/OFFLINE)为准。

## 看设备输出(printf 日志)

链路:printf → UART0(GPIO3/GPIO4)→ 板载 CH340E → Type-C USB → Windows COM4 → 串口终端。

| 方式 | 命令/工具 |
|---|---|
| Windows PowerShell | `pwsh -File C:\Users\Alkari\Desktop\bearpi-serial.ps1`(默认 COM4,可 `-Com 5`;`smart-street-light/bearpi-serial.ps1` 是同一脚本) |
| WSL 终端内借道 | `powershell.exe -ExecutionPolicy Bypass -File 'C:\Users\Alkari\Desktop\bearpi-serial.ps1'`(路径必须 Windows 格式 + 单引号) |
| 图形工具 | MobaXterm → Serial → COM4 / 115200 |

注意:串口独占,HiBurn 开着时看不了日志;波特率错了会乱码;想重播启动日志按 RESET;WSL2 看不到 COM 口,不要找 `/dev/ttyS*`;板子 USB 拔掉后 COM4 消失,先让用户插线。

## 烧录的已知坑(flash.sh 已内置修复,勿回退)

- HiBurn.exe 在 WSL 文件系统里缺执行权限 → 脚本自动 `chmod +x`。
- HiBurn 读不了 `\\wsl.localhost` 的 UNC 路径(退出码 52/17)→ 脚本自动把 HiBurn + 固件暂存到 Windows 本地临时目录 `%TEMP%\bearpi-flash` 再烧。
- COM 参数必须用**数字格式** `-com:4`,`-com:COM4` 会秒退。

## Zed/clangd 环境(已修好,勿破坏)

- Zed 的 clangd 插件在 WSL 里运行,项目根 = `/home/alkari/bearpi`。
- `compile_commands.json` 由 `build.sh` 编译成功后自动生成/更新(ninja compdb + 路径重写,写到项目根);手动重生成用 `gen-compdb.sh`(两个目录里的都行)。
- `.clangd` 做两件事:①`CompileFlags.Remove` 剔除 GCC 独有参数(`-mtune=size`、`-Werror` 等 6 个,其中 `-mtune=size` 会直接让 clangd 崩溃);②`Add` 两个 `-isystem` 指向工具链头文件。
- 交叉工具链已从 Docker 提取到 `~/tools/gcc_riscv32`(GCC **7.3.0**,华为定制 hcc_riscv32)。**不要升级工具链版本**:官方生态锁死 7.3.0,scons 脚本/预编译库/`-Werror` 全按它调校。
- 修改 `.clangd` 后需要用户在 Zed 里重启语言服务器(命令面板 → restart language servers)。

## 操作本项目时的注意事项

- 所有文件在 WSL 9P 挂载上:Windows 侧的 write/edit 工具可能报 `GetFileSecurityW EIO` 或 `ENOTSUP` → 改用 pwsh 的 `Set-Content` 写入;WSL 内直接编辑无此问题。
- git 操作在 WSL 内执行(Windows git 对 WSL 仓库报 dubious ownership)。根仓库和 `bearpi-hm_nano/`、`smart-street-light/` 是三个独立仓库。
- Docker 容器以 root 运行,`bearpi-hm_nano/out/` 产物属 root;在源码树仓库里 `git status` 会对这些文件报 `Permission denied`,属已知现象,不要试图 git 操作 `out/`。
- 烧录和看日志需要**用户物理操作**(按 RESET、插拔线),代理替代不了,流程中要明确提示用户。
- 改动固件代码后的验收 = `./build.sh` 编译通过 + 烧录后串口日志正确。
