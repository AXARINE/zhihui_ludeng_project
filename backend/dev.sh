#!/usr/bin/env bash
# ============================================================
# 智慧路灯后端统一入口(WSL/云,合并原 infra-up.sh / update.sh;
# Windows PowerShell 侧仍用 start.ps1,Git Bash 侧用仓库根 run.sh)
#
#   ./dev.sh db       只启动本地 PostgreSQL
#   ./dev.sh run      加载 .env 后 cargo run(本地开发)
#   ./dev.sh up       docker compose 一键起全栈
#   ./dev.sh down     停止全部容器(数据卷保留)
#   ./dev.sh update   云服务器更新:git pull + 重建 backend + 健康检查
#   ./dev.sh logs     跟踪日志(可加服务名,如: ./dev.sh logs backend)
#   ./dev.sh status   查看服务状态
#   ./dev.sh help     本帮助
# ============================================================
set -euo pipefail
cd "$(dirname "$0")"

usage() {
  cat <<'EOF'
用法: ./dev.sh <子命令> [参数...]

  db      只启动本地 PostgreSQL(docker compose up -d postgres)
  run     source .env 后 cargo run(本地开发,监听 8080)
  up      docker compose up -d --build(postgres + backend)
  down    docker compose down(停容器,数据卷 streetlight-pgdata 保留)
  update  git pull --ff-only + 重建 backend 容器 + 健康检查
  logs    docker compose logs -f --tail 100(如: ./dev.sh logs backend)
  status  docker compose ps
  help    本帮助

示例:
  ./dev.sh db && ./dev.sh run    # 本地开发
  ./dev.sh up                    # 本地 / 云上一键部署
  ./dev.sh update                # 云上拉代码并重建后端
EOF
}

require() {
  command -v "$1" >/dev/null 2>&1 || { echo "错误: 未找到 $1" >&2; exit 1; }
}

cmd_db() {
  require docker
  # 兼容旧版 infra-up.sh 用 docker run 创建的容器(无 compose 标签,同名会冲突):
  # 数据在命名卷 streetlight-pgdata 里,删容器不丢数据,由 compose 原样重建。
  if docker inspect streetlight-postgres >/dev/null 2>&1 \
    && [ -z "$(docker inspect --format '{{index .Config.Labels "com.docker.compose.project"}}' streetlight-postgres)" ]; then
    echo "==> 旧版 docker run 容器改由 compose 管理(数据卷不受影响)"
    docker rm -f streetlight-postgres
  fi
  if docker ps --format '{{.Names}}' | grep -qx streetlight-postgres; then
    echo "streetlight-postgres 已在运行"
    return 0
  fi
  docker compose up -d postgres
  echo "OK: postgres(5432) 已启动"
}

cmd_run() {
  require cargo
  [ -f .env ] || { echo "错误: 缺少 .env,先执行 cp .env.example .env 并填写" >&2; exit 1; }
  set -a
  . ./.env
  set +a
  echo "==> 已加载 .env,启动后端(cargo run)"
  exec cargo run "$@"
}

cmd_up() { require docker; docker compose up -d --build "$@"; }
cmd_down() { require docker; docker compose down "$@"; }
cmd_logs() { require docker; docker compose logs -f --tail 100 "$@"; }
cmd_status() { require docker; docker compose ps; }

cmd_update() {
  require git docker curl
  docker compose version >/dev/null 2>&1 || { echo "错误: 需要 docker compose v2" >&2; exit 1; }

  echo "==> git pull --ff-only"
  git pull --ff-only
  echo "==> 当前版本: $(git describe --tags --always 2>/dev/null || git rev-parse --short HEAD)"

  echo "==> 构建 backend 镜像"
  docker compose build backend
  echo "==> 重建 backend 容器(不动数据库等其他服务)"
  docker compose up -d --no-deps backend

  HEALTH_URL="${BACKEND_HEALTH_URL:-http://127.0.0.1:8080/api/health}"
  HEALTH_TIMEOUT="${BACKEND_HEALTH_TIMEOUT:-60}"
  echo "==> 等待健康检查(最多 ${HEALTH_TIMEOUT}s): $HEALTH_URL"
  for ((i = 0; i < HEALTH_TIMEOUT; i++)); do
    curl -fsS --max-time 2 "$HEALTH_URL" >/dev/null 2>&1 && break
    sleep 1
  done
  if curl -fsS --max-time 2 "$HEALTH_URL" >/dev/null 2>&1; then
    echo "==> 健康检查通过"
  else
    echo "!! 健康检查超时,最近日志如下:" >&2
    docker compose logs --tail 50 backend
    exit 1
  fi

  echo "==> 服务状态:"
  docker compose ps
  echo "==> 最近日志:"
  docker compose logs --tail 30 backend
  echo "==> 后端更新完成"
}

case "${1:-help}" in
  db) cmd_db ;;
  run) shift; cmd_run "$@" ;;
  up) shift; cmd_up "$@" ;;
  down) shift; cmd_down "$@" ;;
  update) cmd_update ;;
  logs) shift; cmd_logs "$@" ;;
  status) cmd_status ;;
  help | -h | --help) usage ;;
  *)
    echo "未知子命令: ${1:-}"
    echo
    usage
    exit 1
    ;;
esac
