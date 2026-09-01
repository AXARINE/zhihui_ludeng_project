#!/bin/bash
# ============================================================
# 智慧路灯系统一键启动脚本
#
# 适用环境 : Windows Git Bash + Docker Desktop
#            (Windows 原生 cargo,后端产物为 .exe)
# 用法     : ./run.sh
# 流程     : 1) 确保 PostgreSQL 容器 streetlight-postgres 在运行
#            2) 加载 backend/.env 环境变量
#            3) 启动后端并监听 8080
#               (优先运行已编译的 target/debug/streetlight-backend.exe,
#                否则 cargo run)
#
# 注意     : 本脚本仅供 Windows 侧使用,WSL 内请走标准工作流:
#              cd ~/bearpi/smart-street-light/backend
#              ./dev.sh db    # 只起数据库(WSL 原生 docker)
#              ./dev.sh run   # 起后端
# ============================================================
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

# 启动后端（cargo run 增量编译：改过代码会重编，没改则秒起——别直接跑旧 exe，会用到过期程序）
echo "[..] 启动后端 http://127.0.0.1:8080 (Ctrl+C 停止)"
exec cargo run
