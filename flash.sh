#!/bin/bash
# 智慧路灯固件一键烧录（WSL2 环境修正版）
# 用法: ./flash.sh <COM号>    例: ./flash.sh 4
# 修正内容:
#   1) HiBurn.exe 在 WSL 文件系统里缺执行权限 -> 自动 chmod +x
#   2) HiBurn 无法读取 \\wsl.localhost 的 UNC 路径 -> 先复制到 Windows 本地临时目录再烧
#   3) COM 参数使用数字格式 -com:N（-com:COMx 格式会导致 HiBurn 秒退）
# 源码树位置默认 ~/bearpi,可用环境变量覆盖: BEARPI_ROOT=/path/to/bearpi ./flash.sh 4
set -e

COM=${1:?usage: flash.sh <COM number>}
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BEARPI_ROOT="${BEARPI_ROOT:-$HOME/bearpi}"

BIN="$BEARPI_ROOT/bearpi-hm_nano/out/BearPi-HM_Nano/Hi3861_wifiiot_app_allinone.bin"
[ -f "$BIN" ] || { echo "firmware not found, run ./build.sh first"; exit 1; }

SRC_DIR="$REPO_ROOT/tools/hiburn_windows/hiburn"
[ -f "$SRC_DIR/HiBurn.exe" ] || { echo "HiBurn.exe not found at $SRC_DIR"; exit 1; }
chmod +x "$SRC_DIR/HiBurn.exe" 2>/dev/null || true

# 取 Windows 本地 TEMP 路径（HiBurn 读不了 \\wsl.localhost UNC 路径）
WIN_TEMP="$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')"
if [ -z "$WIN_TEMP" ]; then
    # cmd.exe interop 不可用时的兜底（Windows 路径大小写不敏感）
    WIN_TEMP="C:\\Users\\$USER\\AppData\\Local\\Temp"
fi
STAGE_WIN="$WIN_TEMP\\bearpi-flash"
STAGE_UNIX="$(wslpath -u "$STAGE_WIN")"

echo "staging HiBurn + firmware to: $STAGE_WIN"
mkdir -p "$STAGE_UNIX"
cp -rf "$SRC_DIR"/. "$STAGE_UNIX/"
cp -f "$BIN" "$STAGE_UNIX/Hi3861_wifiiot_app_allinone.bin"
chmod +x "$STAGE_UNIX/HiBurn.exe"

cd "$STAGE_UNIX"
echo "press RESET button on the board to start burning..."
if ./HiBurn.exe -com:"$COM" -bin:"$STAGE_WIN\\Hi3861_wifiiot_app_allinone.bin" -signalbaud:921600 -2ms -show; then
    echo "FLASH OK (HiBurn exit 0)"
else
    rc=$?
    echo "HiBurn exited with code $rc (burn failed or window closed)"
    exit $rc
fi
