# 智慧路灯(BearPi-HM Nano)

基于小熊派 **BearPi-HM Nano** 开发板(海思 Hi3861,RISC-V 32 位,OpenHarmony 轻量系统 + LiteOS-M)和 E53_SC1 传感器扩展板的智慧路灯样例。需求见 `04_智慧路灯_基本功能清单.md`。

当前固件功能:BH1750 光照传感器连续采样(50ms 周期),**Lux < 40 自动开灯**,否则关灯;光照值通过串口打印。

## 目录说明

| 路径 | 内容 |
|---|---|
| `C3_e53_sc1_pls/` | 固件源码(路灯样例,基于官方 E53_SC1 样例修改) |
| `04_智慧路灯_基本功能清单.md` | 需求文档(用户故事/业务流程) |
| `build.sh` | Docker 一键编译(自动把样例同步进源码树) |
| `flash.sh` | 一键烧录(内置 WSL2 环境修复) |
| `gen-compdb.sh` | 重新生成 clangd 用的 compile_commands.json(一般不用手动跑) |
| `bearpi-serial.ps1` | 串口日志查看脚本(Windows PowerShell) |
| `tools/hiburn_windows/` | HiBurn 烧录工具(Windows 版) |

## 前置条件

- **WSL2 Ubuntu** + Docker,拉取镜像 `openharmony/openharmony-docker:0.0.3`
- **OpenHarmony 源码树**:gitee 仓库 `bearpi/bearpi-hm_nano`(master)克隆到 `~/bearpi/bearpi-hm_nano`(其他位置用 `BEARPI_ROOT=/path/to/bearpi` 环境变量指定)
- 编辑源码树的 `applications/BearPi/BearPi-HM_Nano/sample/BUILD.gn`,在 `features` 里启用 `"C3_e53_sc1_pls:e53_sc1_example"`、屏蔽其他样例
- 开发板 USB 连接 Windows(串口为 COM4 或按实际),Hi3861 串口驱动(CH340)已安装

## 构建 → 烧录 → 看日志 全流程

```bash
# 1. 编译(WSL 内,仓库根目录)
./build.sh
#    产物: $BEARPI_ROOT/bearpi-hm_nano/out/BearPi-HM_Nano/Hi3861_wifiiot_app_allinone.bin

# 2. 烧录(WSL 内,参数为 COM 号数字)
./flash.sh 4
#    HiBurn 窗口打开后,按一下开发板 RESET 键开始烧录,看到 FLASH OK 即成功

# 3. 烧录完成后再按一次 RESET,才会运行新固件
```

```powershell
# 4. 看日志(Windows PowerShell,115200 波特率)
pwsh -File bearpi-serial.ps1          # 默认 COM4,可 -Com 5 换口
```

## 硬件连接

- BH1750 光照传感器:I2C1(GPIO0=SDA / GPIO1=SCL,400kHz),从机地址 0x23,连续低分辨率模式(0x13)
- 补光灯:GPIO7,高电平点亮
- 日志:printf → UART0(GPIO3/GPIO4)→ 板载 CH340E → USB → Windows COM 口

## 已知坑(脚本已内置修复,勿回退)

- HiBurn 读不了 `\\wsl.localhost` UNC 路径 → flash.sh 会把 HiBurn + 固件暂存到 Windows `%TEMP%\bearpi-flash` 再烧
- HiBurn 的 COM 参数必须用数字格式 `-com:4`,`-com:COM4` 会秒退
- 串口独占:HiBurn 烧录时看不了日志,关掉 HiBurn 再开串口
- WSL2 看不到 COM 口,不要找 `/dev/ttyS*`;串口查看只能在 Windows 侧进行

## 修改代码的迭代流程

直接改本仓库的 `C3_e53_sc1_pls/` 里的源码,然后重跑 `./build.sh`(会先同步进源码树再编译)→ `./flash.sh 4` → 按 RESET → 看串口输出。没有单元测试,验收 = 编译通过 + 串口日志符合预期。
