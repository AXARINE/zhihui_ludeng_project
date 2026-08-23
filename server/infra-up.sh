#!/usr/bin/env bash
# 启动本地基础设施(PostgreSQL)
#
# 使用 WSL 原生 Docker(dockerd),不依赖 Docker Desktop。
# (Docker Desktop 无法从 \\wsl.localhost 读取 bind mount,且需要 Windows 侧常驻。)
set -euo pipefail

if docker ps --format '{{.Names}}' | grep -qx streetlight-postgres; then
    echo "streetlight-postgres 已在运行"
    exit 0
fi

docker start streetlight-postgres 2>/dev/null || docker run -d \
    --name streetlight-postgres \
    -e POSTGRES_DB=streetlight \
    -e POSTGRES_USER=streetlight \
    -e POSTGRES_PASSWORD=streetlight \
    -p 5432:5432 \
    -v streetlight-pgdata:/var/lib/postgresql/data \
    --restart unless-stopped \
    postgres:16

echo "OK: postgres(5432) 已启动"
