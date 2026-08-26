#!/usr/bin/env bash
# ============================================================
# 智慧路灯后端一键更新脚本
#
# 流程:git pull 拉取最新代码 -> 重建 backend 镜像/容器 ->
#       等待健康检查 -> 打印状态与日志。数据库等其他服务不动。
#
# 可选环境变量:
#   BACKEND_HEALTH_URL      健康检查地址(默认 http://127.0.0.1:8080/api/health)
#   BACKEND_HEALTH_TIMEOUT  启动等待超时秒数(默认 60)
# ============================================================
set -euo pipefail

cd "$(dirname "$0")"

for cmd in git docker; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "错误: 未找到 $cmd, 请先安装" >&2
    exit 1
  fi
done
if ! docker compose version >/dev/null 2>&1; then
  echo "错误: 未找到 docker compose(需要 Docker Compose v2)" >&2
  exit 1
fi

# ---------- 1. 拉取最新代码 ----------
echo "==> git pull 拉取最新代码..."
git pull --ff-only

echo "==> 当前版本: $(git describe --tags --always 2>/dev/null || git rev-parse --short HEAD)"

# ---------- 2. 构建并替换后端容器 ----------
echo "==> 构建 backend 镜像..."
docker compose build backend

echo "==> 重建 backend 容器(不重启数据库等其他服务)..."
docker compose up -d --no-deps backend

# ---------- 3. 等待健康检查 ----------
HEALTH_URL="${BACKEND_HEALTH_URL:-http://127.0.0.1:8080/api/health}"
HEALTH_TIMEOUT="${BACKEND_HEALTH_TIMEOUT:-60}"

if command -v curl >/dev/null 2>&1; then
  echo "==> 等待健康检查通过(最多 ${HEALTH_TIMEOUT}s): $HEALTH_URL"
  ok=0
  for _ in $(seq 1 "$HEALTH_TIMEOUT"); do
    if curl -fsS --max-time 2 "$HEALTH_URL" >/dev/null 2>&1; then
      ok=1
      break
    fi
    sleep 1
  done
  if [[ "$ok" -ne 1 ]]; then
    echo "!! 健康检查超时, 最近日志如下:" >&2
    docker compose logs --tail 50 backend
    exit 1
  fi
  echo "==> 健康检查通过"
else
  echo "!! 未安装 curl, 跳过健康检查(可手动执行: curl $HEALTH_URL)" >&2
fi

# ---------- 4. 输出状态与日志 ----------
echo "==> 服务状态:"
docker compose ps
echo "==> 最近日志:"
docker compose logs --tail 30 backend
echo "==> 后端更新完成"
