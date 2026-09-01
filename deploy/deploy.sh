#!/usr/bin/env bash
# 智慧路灯一键部署:准备 .env → 加载后端瘦镜像 → docker compose up -d
#
# 两种来源都支持:
#   a) GitHub Release 部署包(解压后自带 .env.example 与 images/streetlight-backend.tar);
#   b) 本仓库 deploy/ 目录(.env 模板取 ../backend/.env.example,镜像需自行构建,见下方提示)。
set -euo pipefail
cd "$(dirname "$0")"

# ---------- 1. .env ----------
if [ ! -f .env ]; then
  if [ -f .env.example ]; then
    cp .env.example .env
  elif [ -f ../backend/.env.example ]; then
    cp ../backend/.env.example .env
  else
    echo "==> 错误:找不到 .env 模板,请从 GitHub Release 下载部署包运行本脚本" >&2
    exit 1
  fi
  echo "==> 已生成 .env,请填写华为云 AK/SK、JWT_SECRET 等必填项后重新运行本脚本"
  exit 1
fi

# 必填项检查:缺失行 / 空值 / 模板占位值一律拦下,避免带默认密钥上线
for var in HUAWEI_AK HUAWEI_SK HUAWEI_IOTDA_ENDPOINT JWT_SECRET; do
  val="$(sed -nE "s/^${var}=(.*)$/\1/p" .env | tail -1)"
  case "$val" in
    "" | "访问密钥"* | "实例应用侧域名" | "change-me"*)
      echo "==> .env 的 ${var} 未填写,请补齐后重试(HUAWEI_AK/HUAWEI_SK/HUAWEI_IOTDA_ENDPOINT/JWT_SECRET 均为必填)" >&2
      exit 1
      ;;
  esac
done

# ---------- 2. 后端瘦镜像 ----------
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

# ---------- 3. 启动 ----------
docker compose up -d
echo
echo "==> 部署完成:"
echo "    统一入口(前端 + API): http://<本机IP或域名>/   (Caddy :80,局域网可访问)"
echo "    后端直连(仅本机):      http://127.0.0.1:8080/   Swagger: http://127.0.0.1:8080/docs"
docker compose ps
