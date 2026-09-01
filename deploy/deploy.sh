#!/usr/bin/env bash
# 智慧路灯一键部署:config.json → 生成 .env + Caddyfile → 加载后端瘦镜像 → compose up -d
#
# 用法:tar xzf streetlight-deploy-*.tar.gz && cd streetlight-deploy-*/ && ./deploy.sh
# 首次运行:从 config.example.json 生成 config.json 并退出,填好必填项后再次运行。
# .env 与 Caddyfile 均由本脚本生成,请勿手改(会被下次部署覆盖)。
set -euo pipefail
cd "$(dirname "$0")"

# ---------- 1. config.json ----------
if [ ! -f config.json ]; then
  if [ -f config.example.json ]; then
    cp config.example.json config.json
  else
    echo "==> 错误:找不到 config.example.json,请从 GitHub Release 下载完整部署包" >&2
    exit 1
  fi
  echo "==> 已生成 config.json,请填写 huawei_ak/huawei_sk/huawei_project_id/iotda_endpoint/jwt_secret 后重新运行本脚本"
  exit 1
fi

# 扁平 JSON 取值(一键一行、值为字符串;值内不要包含双引号)
json_get() {
  sed -nE 's/^[[:space:]]*"'"$1"'"[[:space:]]*:[[:space:]]*"([^"]*)".*$/\1/p' config.json | tail -1
}

DOMAIN="$(json_get domain)"
AK="$(json_get huawei_ak)"
SK="$(json_get huawei_sk)"
PROJECT_ID="$(json_get huawei_project_id)"
ENDPOINT="$(json_get iotda_endpoint)"
REGION="$(json_get iotda_region)"
JWT_SECRET="$(json_get jwt_secret)"
SA_PWD="$(json_get bootstrap_super_admin_password)"
ADMIN_PWD="$(json_get bootstrap_admin_password)"
WEBHOOK_TOKEN="$(json_get iotda_webhook_token)"
POLL_SECS="$(json_get iotda_poll_interval_secs)"
AUTO_SYNC="$(json_get iotda_auto_sync_devices)"
SYNC_SECS="$(json_get iotda_sync_interval_secs)"
PG_PWD="$(json_get postgres_password)"
PG_VOLUME="$(json_get pgdata_volume)"
ORIGINS="$(json_get allowed_origins)"
AI_KEY="$(json_get ai_api_key)"
AI_BASE_URL="$(json_get ai_base_url)"
AI_MODEL="$(json_get ai_model)"

# 必填项检查:空值一律拦下,避免带默认密钥上线
missing=()
[ -z "$AK" ] && missing+=(huawei_ak)
[ -z "$SK" ] && missing+=(huawei_sk)
[ -z "$PROJECT_ID" ] && missing+=(huawei_project_id)
[ -z "$ENDPOINT" ] && missing+=(iotda_endpoint)
[ -z "$JWT_SECRET" ] && missing+=(jwt_secret)
if [ "${#missing[@]}" -gt 0 ]; then
  echo "==> config.json 必填项未填写:${missing[*]}" >&2
  exit 1
fi

# ---------- 2. 生成 .env ----------
{
  echo "# 由 deploy.sh 根据 config.json 生成,请勿手改"
  echo "HUAWEI_AK=$AK"
  echo "HUAWEI_SK=$SK"
  echo "HUAWEI_PROJECT_ID=$PROJECT_ID"
  echo "HUAWEI_IOTDA_ENDPOINT=$ENDPOINT"
  [ -n "$REGION" ] && echo "HUAWEI_IOTDA_REGION=$REGION"
  echo "JWT_SECRET=$JWT_SECRET"
  [ -n "$SA_PWD" ] && echo "BOOTSTRAP_SUPER_ADMIN_PASSWORD=$SA_PWD"
  [ -n "$ADMIN_PWD" ] && echo "BOOTSTRAP_ADMIN_PASSWORD=$ADMIN_PWD"
  [ -n "$WEBHOOK_TOKEN" ] && echo "IOTDA_WEBHOOK_TOKEN=$WEBHOOK_TOKEN"
  [ -n "$POLL_SECS" ] && echo "IOTDA_POLL_INTERVAL_SECS=$POLL_SECS"
  [ -n "$AUTO_SYNC" ] && echo "IOTDA_AUTO_SYNC_DEVICES=$AUTO_SYNC"
  [ -n "$SYNC_SECS" ] && echo "IOTDA_SYNC_INTERVAL_SECS=$SYNC_SECS"
  [ -n "$PG_PWD" ] && echo "POSTGRES_PASSWORD=$PG_PWD"
  [ -n "$PG_VOLUME" ] && echo "PGDATA_VOLUME=$PG_VOLUME"
  [ -n "$ORIGINS" ] && echo "ALLOWED_ORIGINS=$ORIGINS"
  if [ -n "$AI_KEY" ]; then
    echo "AI_API_KEY=$AI_KEY"
    [ -n "$AI_BASE_URL" ] && echo "AI_BASE_URL=$AI_BASE_URL"
    [ -n "$AI_MODEL" ] && echo "AI_MODEL=$AI_MODEL"
  fi
} > .env

# ---------- 3. 生成 Caddyfile(domain 留空 = :80;填域名自动申请 HTTPS) ----------
cat > Caddyfile <<EOF
${DOMAIN:-:80} {
	encode zstd gzip

	root * /srv
	handle /api/* {
		reverse_proxy backend:8080
	}
	handle /docs* {
		reverse_proxy backend:8080
	}
	handle {
		try_files {path} /index.html
		file_server
	}
}
EOF

# ---------- 3.5 前端产物兜底 ----------
# site/ 是构建产物目录(不入库)。缺 index.html 时先摆占位页,免得 Caddy 对
# 首页返回 404 / 白屏,让人以为部署失败——后端此时其实已经可用。
mkdir -p site
if [ ! -f site/index.html ]; then
  if [ -f site-placeholder.html ]; then
    cp site-placeholder.html site/index.html
    echo "==> 注意:site/ 无前端产物,已摆占位页;构建前端请在仓库根跑 ./release.sh"
  else
    echo "==> 注意:site/ 无前端产物,首页将不可用(API 与 /docs 不受影响)"
  fi
fi

# ---------- 4. 后端瘦镜像 ----------
if ! docker image inspect streetlight-backend:latest >/dev/null 2>&1; then
  if [ -f images/streetlight-backend.tar ]; then
    echo "==> 加载后端镜像 images/streetlight-backend.tar ..."
    docker load -i images/streetlight-backend.tar
  else
    echo "==> 错误:本地没有 streetlight-backend:latest 镜像,也没有 images/streetlight-backend.tar" >&2
    echo "    从 Release 下载部署包,或本仓库内手动构建:" >&2
    echo "    cd ../backend && docker build -t streetlight-backend:latest . \\" >&2
    echo "      && docker save streetlight-backend:latest -o ../deploy/images/streetlight-backend.tar" >&2
    exit 1
  fi
fi

# ---------- 5. 启动 ----------
docker compose up -d
echo
echo "==> 部署完成:"
echo "    统一入口(前端 + API): http${DOMAIN:+s}://${DOMAIN:-<本机IP>}/"
echo "    后端直连(仅本机):      http://127.0.0.1:8080/   Swagger: http://127.0.0.1:8080/docs"
docker compose ps
