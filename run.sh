#!/bin/bash
# 智慧路灯系统一键启动（Windows Git Bash 运行: ./run.sh）
# 1) 确保 PostgreSQL 容器在跑(Docker Desktop 需先启动)
# 2) 加载 .env 并启动后端(8080)
set -e
cd "$(dirname "$0")/backend"

# 数据库
if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx streetlight-postgres; then
  echo "[OK] PostgreSQL 已在运行"
elif docker ps -a --format '{{.Names}}' 2>/dev/null | grep -qx streetlight-postgres; then
  echo "[..] 启动 PostgreSQL 容器..."
  docker start streetlight-postgres
else
  echo "[!!] 未找到 streetlight-postgres 容器，请先启动 Docker Desktop 并创建数据库容器"
  echo "     docker run -d --name streetlight-postgres -e POSTGRES_DB=streetlight \\"
  echo "       -e POSTGRES_USER=streetlight -e POSTGRES_PASSWORD=streetlight \\"
  echo "       -p 5432:5432 -v streetlight-pgdata:/var/lib/postgresql/data \\"
  echo "       --restart unless-stopped postgres:16"
  exit 1
fi

# 环境变量（小组后端不自动读 .env）
set -a; source .env; set +a

# 启动后端（优先用已编译产物，改过代码则 cargo run）
echo "[..] 启动后端 http://127.0.0.1:8080 (Ctrl+C 停止)"
if [ -f target/debug/streetlight-backend.exe ]; then
  exec ./target/debug/streetlight-backend.exe
else
  exec cargo run
fi
