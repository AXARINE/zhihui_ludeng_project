#!/usr/bin/env bash
# ============================================================
# 智慧路灯"发新版"一键流水线(本机 WSL,面向 deploy/ 发布栈)
#
#   ./release.sh                  质量门禁 → 构建 → 部署 → 冒烟验证
#   ./release.sh --tag v0.2.0     上述全流程 + 打 tag 并推送(触发 CI 出 Release 包)
#   ./release.sh --no-deploy      只构建 + 校验产物,不动线上容器
#   ./release.sh --skip-tests     跳过测试门禁(应急用,不建议)
#   ./release.sh help             本帮助
#
# 流程与 .github/workflows/release.yml 保持一致(同样的 npm build + docker build),
# 差别有两处:本脚本额外跑测试门禁,并把产物真正部署到本机 deploy/ 栈做冒烟验证。
#
# 手工等价步骤(本脚本就是把它们串起来):
#   cd backend && cargo test && docker build -t streetlight-backend:latest .
#   cd frontend_vue && npm run build && cp -a dist/. ../deploy/site/
#   cd deploy && ./deploy.sh && curl http://127.0.0.1/api/health
# ============================================================
set -euo pipefail
cd "$(dirname "$0")"

ROOT="$PWD"
DO_DEPLOY=1
DO_TESTS=1
TAG=""

usage() {
  sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --tag) TAG="${2:-}"; [ -n "$TAG" ] || { echo "==> --tag 需要版本号,如 --tag v0.2.0" >&2; exit 1; }; shift 2 ;;
    --no-deploy) DO_DEPLOY=0; shift ;;
    --skip-tests) DO_TESTS=0; shift ;;
    help|-h|--help) usage ;;
    *) echo "==> 未知参数:$1(./release.sh help 看用法)" >&2; exit 1 ;;
  esac
done

step() { echo; echo "==> [$1/$TOTAL] $2"; }
TOTAL=6
[ -n "$TAG" ] && TOTAL=7

# ---------- 0. 前置检查:缺什么早报错,别构建到一半才失败 ----------
for bin in docker npm cargo; do
  command -v "$bin" >/dev/null || { echo "==> 缺少 $bin,无法发布" >&2; exit 1; }
done
if [ "$DO_DEPLOY" = 1 ] && [ ! -f deploy/config.json ]; then
  echo "==> deploy/config.json 不存在;先 cd deploy && ./deploy.sh 生成并填写" >&2
  exit 1
fi
# 打 tag 要求工作区干净,否则 tag 指向的提交与实际发布产物对不上
if [ -n "$TAG" ] && [ -n "$(git status --porcelain)" ]; then
  echo "==> 工作区有未提交改动,--tag 会让 tag 与产物不一致;先提交再发版" >&2
  git status --short >&2
  exit 1
fi

# ---------- 1. 质量门禁 ----------
step 1 "质量门禁:cargo test"
if [ "$DO_TESTS" = 1 ]; then
  (cd backend && cargo test --quiet)
  echo "    测试通过"
else
  echo "    已跳过(--skip-tests)"
fi

# ---------- 2. 前端构建 ----------
step 2 "构建前端 dist"
cd "$ROOT/frontend_vue"
[ -d node_modules ] || npm ci
npm run build >/dev/null
echo "    产物:frontend_vue/dist ($(find dist -type f | wc -l) 个文件)"

# ---------- 3. 后端瘦镜像 ----------
step 3 "构建后端瘦镜像(静态 musl → scratch)"
cd "$ROOT/backend"
docker build -q -t streetlight-backend:latest . >/dev/null
IMG_SIZE=$(docker image inspect streetlight-backend:latest --format '{{.Size}}')
echo "    镜像:streetlight-backend:latest ($((IMG_SIZE / 1024 / 1024)) MB)"

# ---------- 4. 同步前端产物到部署包 ----------
step 4 "同步 dist → deploy/site"
cd "$ROOT"
rm -rf deploy/site/assets
cp -a frontend_vue/dist/. deploy/site/
echo "    已同步"

# ---------- 5. 部署 ----------
step 5 "部署到本机发布栈"
if [ "$DO_DEPLOY" = 1 ]; then
  (cd deploy && ./deploy.sh >/dev/null)
  echo "    容器已更新"
else
  echo "    已跳过(--no-deploy)"
fi

# ---------- 6. 冒烟验证 ----------
step 6 "冒烟验证"
if [ "$DO_DEPLOY" = 1 ]; then
  ok=0
  for _ in $(seq 1 20); do
    if curl -fsS http://127.0.0.1/api/health >/dev/null 2>&1; then ok=1; break; fi
    sleep 1
  done
  [ "$ok" = 1 ] || { echo "    /api/health 20s 内未就绪" >&2; docker logs --tail 30 streetlight-deploy-backend >&2; exit 1; }
  for path in /api/health / /docs; do
    code=$(curl -s -o /dev/null -w '%{http_code}' -L "http://127.0.0.1$path")
    case "$code" in
      2*) echo "    $path → $code" ;;
      *) echo "    $path → $code(异常)" >&2; exit 1 ;;
    esac
  done
else
  echo "    已跳过(--no-deploy)"
fi

# ---------- 7. 打 tag 发版(触发 CI 产出 Release 部署包) ----------
if [ -n "$TAG" ]; then
  step 7 "打 tag 并推送:$TAG"
  git tag -a "$TAG" -m "Release $TAG"
  git push origin "$TAG"
  echo "    已推送;CI 将构建部署包并挂到 GitHub Release"
fi

echo
echo "==> 发布完成"
[ "$DO_DEPLOY" = 1 ] && echo "    入口:http://127.0.0.1/   Swagger:http://127.0.0.1/docs"
exit 0
