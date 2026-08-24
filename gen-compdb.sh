#!/bin/bash
# 重新生成 Zed/clangd 用的 compile_commands.json
# 切换 sample/BUILD.gn 样例并重新编译后,重跑本脚本:
#   ./gen-compdb.sh
# 源码树默认用本仓库内 submodule(bearpi-hm_nano/);可用 BEARPI_ROOT=/path 覆盖(该目录下需有 bearpi-hm_nano/)
set -e
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BEARPI_ROOT="${BEARPI_ROOT:-$REPO_ROOT}"
docker run --rm -v "$BEARPI_ROOT:/home/openharmony" \
  -w /home/openharmony/bearpi-hm_nano/out/BearPi-HM_Nano \
  -e PATH=/home/tools/ninja:/home/tools/gcc_riscv32/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  openharmony/openharmony-docker:0.0.3 bash -c \
  "ninja -t compdb cc cxx | sed -e 's|/home/openharmony|$BEARPI_ROOT|g' -e 's|/home/tools|$HOME/tools|g' > /home/openharmony/compile_commands.json"
echo "compile_commands.json updated"
