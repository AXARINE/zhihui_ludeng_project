# 智慧路灯后端镜像(多阶段:静态 musl 编译 → scratch 运行)
# 构建:docker build -t streetlight-backend .
# 运行:走 docker-compose.yml(开发)或 ../deploy/docker-compose.yml(发布部署)
#
# 设计要点:
# - builder 用 rust:alpine:宿主环境即 musl,无需交叉工具链,产出不依赖 glibc 的静态二进制;
# - 运行阶段用 scratch:镜像里只有二进制 + CA 证书,无 shell / 包管理器,体积小、攻击面最小;
# - TLS 栈 = reqwest rustls(aws-lc-rs + rustls-platform-verifier),证书链从
#   /etc/ssl/certs/ca-certificates.crt 读取(从 builder 拷入,访问华为云 IoTDA 北向 HTTPS 必需);
# - sqlx 查询均为运行时执行(无 query! 宏),构建期不需要数据库。

# ---- 构建阶段 ----
FROM rust:1-alpine AS builder
WORKDIR /app
# 国内网络直连 static.crates.io 极慢,默认换 rsproxy.cn 稀疏镜像;
# CI(GitHub Actions 境外 runner)构建时传 --build-arg USE_RS_PROXY=0 走官方源
ARG USE_RS_PROXY=1
RUN if [ "$USE_RS_PROXY" = "1" ]; then printf '%s\n' \
    '[source.crates-io]' \
    'replace-with = "rsproxy-sparse"' \
    '[source.rsproxy-sparse]' \
    'registry = "sparse+https://rsproxy.cn/index/"' \
    > "$CARGO_HOME/config.toml"; fi
# build-base(gcc/musl-dev/make)+ cmake + perl:aws-lc-sys(rustls 加密后端)的 C 代码构建需要
RUN apk add --no-cache build-base cmake perl ca-certificates
# 先用空壳源码编译依赖层:Cargo.toml/lock 不变则缓存命中,改 src 不再全量重拉依赖
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src migrations && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked
# 再拷真实源码重编项目层(touch 让 cargo 认为 main.rs 比依赖层更新)
COPY migrations ./migrations
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---- 文件装配阶段(基于 builder,只为 scratch 准备系统文件,不打断依赖层缓存) ----
FROM builder AS files
# IPv4 优先:容器无 IPv6 出口,IoTDA 域名 AAAA 记录排前会导致首连失败(musl 1.2.4+ 读取 gai.conf)
RUN echo 'precedence ::ffff:0:0/96 100' > /etc/gai.conf

# ---- 运行阶段 ----
FROM scratch
# CA 证书供 rustls 平台验证器校验华为云 HTTPS 证书链
COPY --from=files /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=files /etc/gai.conf /etc/gai.conf
COPY --from=builder /app/target/release/streetlight-backend /streetlight-backend
# 非 root 运行;后端无任何文件读写(配置全靠环境变量,迁移内嵌二进制)
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["/streetlight-backend"]
