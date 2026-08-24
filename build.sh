#!/bin/bash
# 智慧路灯固件 Docker 一键编译（在 WSL Ubuntu 中执行: ./build.sh）
# 1) 把本仓库的 C3_e53_sc1_pls 样例同步进 OpenHarmony 源码树
# 2) 在 Docker 容器里编译 bearpi-hm_nano 工程
# 3) 编译成功后自动更新 Zed/clangd 用的 compile_commands.json
# 源码树 = 本仓库内的 submodule(bearpi-hm_nano/);也可用 BEARPI_ROOT=/path 指定别处(该目录下需有 bearpi-hm_nano/)
set -e

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BEARPI_ROOT="${BEARPI_ROOT:-$REPO_ROOT}"
SAMPLE_DIR="$BEARPI_ROOT/bearpi-hm_nano/applications/BearPi/BearPi-HM_Nano/sample"

[ -d "$SAMPLE_DIR" ] || { echo "源码树不存在: $SAMPLE_DIR(用 BEARPI_ROOT=路径 指定)"; exit 1; }

SAMPLE_DST="$SAMPLE_DIR/C3_e53_sc1_pls"
echo "sync sample -> $SAMPLE_DST"

# 暂存源码树里已有的凭据文件,再全量清旧(避免源仓库删文件后旧文件残留)
KEPT_CONF=""
if [ -f "$SAMPLE_DST/include/app_config.h" ]; then
  KEPT_CONF="$(mktemp)"
  mv "$SAMPLE_DST/include/app_config.h" "$KEPT_CONF"
fi
rm -rf "$SAMPLE_DST"

# 只同步本仓库 git 跟踪的文件:app_config.h 被 .gitignore 忽略,不会随同步进入源码树
( cd "$REPO_ROOT" && git ls-files -z -- C3_e53_sc1_pls | tar --null -T - -cf - ) \
  | tar -xf - -C "$SAMPLE_DIR"
[ -f "$SAMPLE_DST/e53_sc1_example.c" ] || { echo "error: 样例同步失败"; exit 1; }

# 凭据不随每次构建盲拷:内容没变就原样放回,缺失或有更新才写入
SRC_CONF="$REPO_ROOT/C3_e53_sc1_pls/include/app_config.h"
DST_CONF="$SAMPLE_DST/include/app_config.h"
if [ -n "$KEPT_CONF" ] && cmp -s "$KEPT_CONF" "$SRC_CONF" 2>/dev/null; then
  mv "$KEPT_CONF" "$DST_CONF"
else
  if [ -n "$KEPT_CONF" ]; then rm -f "$KEPT_CONF"; fi
  [ -f "$SRC_CONF" ] || { echo "error: 缺少 $SRC_CONF(照 include/app_config.example.h 填写)"; exit 1; }
  cp "$SRC_CONF" "$DST_CONF"
  echo "app_config.h 缺失或有更新,已写入源码树"
fi

# 自动启用本样例、屏蔽其他样例(submodule 是官方检出,features 默认全注释,这是树内的预期本地改动)
BUILD_GN="$SAMPLE_DIR/BUILD.gn"
if ! grep -Eq '^[[:space:]]*"C3_e53_sc1_pls:e53_sc1_example"' "$BUILD_GN"; then
  echo "sample/BUILD.gn 未启用本样例,自动启用并屏蔽其他样例"
  sed -i \
    -e 's|^\([[:space:]]*\)"\([A-Za-z0-9_]\+:[A-Za-z0-9_]\+\)",|\1#"\2",|' \
    -e 's|^\([[:space:]]*\)#\?"C3_e53_sc1_pls:e53_sc1_example",|\1"C3_e53_sc1_pls:e53_sc1_example",|' \
    "$BUILD_GN"
  grep -Eq '^[[:space:]]*"C3_e53_sc1_pls:e53_sc1_example"' "$BUILD_GN" \
    || { echo "error: 自动启用失败,请手动编辑 $BUILD_GN 的 features"; exit 1; }
fi

# submodule 的 git 卫生:凭据文件不进其 git 视野;out/ 是 root 属主产物,屏蔽其扫描;
# 树内只读使用,submodule 永久 dirty 属预期(样例同步+BUILD.gn 启用),不再在 status 里刷屏
GIT_DIR="$(git -C "$BEARPI_ROOT/bearpi-hm_nano" rev-parse --absolute-git-dir 2>/dev/null || true)"
if [ -n "$GIT_DIR" ]; then
  mkdir -p "$GIT_DIR/info"
  for pat in \
    "applications/BearPi/BearPi-HM_Nano/sample/C3_e53_sc1_pls/include/app_config.h" \
    "out/"; do
    grep -qxF "$pat" "$GIT_DIR/info/exclude" 2>/dev/null || echo "$pat" >> "$GIT_DIR/info/exclude"
  done
  git -C "$BEARPI_ROOT/bearpi-hm_nano" config status.showUntrackedFiles no
  git -C "$REPO_ROOT" config submodule.bearpi-hm_nano.ignore dirty
fi

docker run --rm   -v "$BEARPI_ROOT:/home/openharmony"   -w /home/openharmony/bearpi-hm_nano   -e PATH=/home/tools/gcc_riscv32/bin:/home/tools:/home/tools/ninja:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin   openharmony/openharmony-docker:0.0.3   bash -c "python build.py BearPi-HM_Nano "

"$REPO_ROOT/gen-compdb.sh" || echo "warning: compile_commands.json 更新失败(不影响固件)"
