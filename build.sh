#!/bin/bash
# 智慧路灯固件 Docker 一键编译（在 WSL Ubuntu 中执行: ./build.sh）
# 1) 把本仓库的 C3_e53_sc1_pls 样例同步进 OpenHarmony 源码树
# 2) 在 Docker 容器里编译 bearpi-hm_nano 工程
# 3) 编译成功后自动更新 Zed/clangd 用的 compile_commands.json
# 源码树位置默认 ~/bearpi,可用环境变量覆盖: BEARPI_ROOT=/path/to/bearpi ./build.sh
set -e

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BEARPI_ROOT="${BEARPI_ROOT:-$HOME/bearpi}"
SAMPLE_DIR="$BEARPI_ROOT/bearpi-hm_nano/applications/BearPi/BearPi-HM_Nano/sample"

[ -d "$SAMPLE_DIR" ] || { echo "源码树不存在: $SAMPLE_DIR(用 BEARPI_ROOT=路径 指定)"; exit 1; }

echo "sync sample -> $SAMPLE_DIR/C3_e53_sc1_pls"
cp -r "$REPO_ROOT/C3_e53_sc1_pls" "$SAMPLE_DIR/"

if ! grep -Eq '^[[:space:]]*"C3_e53_sc1_pls:e53_sc1_example"' "$SAMPLE_DIR/BUILD.gn"; then
    echo "error: $SAMPLE_DIR/BUILD.gn 的 features 里未启用 \"C3_e53_sc1_pls:e53_sc1_example\""
    echo "请去掉该行注释、屏蔽其他样例后重试"
    exit 1
fi

docker run --rm   -v "$BEARPI_ROOT:/home/openharmony"   -w /home/openharmony/bearpi-hm_nano   -e PATH=/home/tools/gcc_riscv32/bin:/home/tools:/home/tools/ninja:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin   openharmony/openharmony-docker:0.0.3   bash -c "python build.py BearPi-HM_Nano "

"$REPO_ROOT/gen-compdb.sh" || echo "warning: compile_commands.json 更新失败(不影响固件)"
