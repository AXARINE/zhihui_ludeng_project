# 智慧路灯后端镜像(多阶段构建)
# 构建:docker build -t streetlight-backend .
# 运行一般走 docker-compose.yml,单独跑需自己提供 DATABASE_URL 等环境变量

# ---- 构建阶段 ----
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
# 国内网络直连 static.crates.io 极慢,换 rsproxy.cn 稀疏镜像
RUN printf '%s\n' \
    '[source.crates-io]' \
    'replace-with = "rsproxy-sparse"' \
    '[source.rsproxy-sparse]' \
    'registry = "sparse+https://rsproxy.cn/index/"' \
    > "$CARGO_HOME/config.toml"
# sqlx 查询均为运行时执行(无 query! 宏),构建期不需要数据库
RUN cargo build --release --locked

# ---- 运行阶段 ----
FROM debian:bookworm-slim
# ca-certificates:访问华为云 IoTDA 北向 HTTPS 必需
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# 容器无 IPv6 出口,而 IoTDA 域名 DNS 会把 AAAA 记录排在前面,导致 reqwest 连
# IPv6 失败;gai.conf 提高 IPv4 优先级(getaddrinfo 排序),强制偏好 IPv4
RUN echo 'precedence ::ffff:0:0/96 100' > /etc/gai.conf
COPY --from=builder /app/target/release/streetlight-backend /usr/local/bin/streetlight-backend
EXPOSE 8080
CMD ["streetlight-backend"]
