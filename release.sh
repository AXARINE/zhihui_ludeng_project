#!/usr/bin/env bash
# ============================================================
# 智慧路灯"本机发新版"流水线(面向 deploy/ 发布栈)
#
#   ./release.sh                  质量门禁 → 构建 → 部署 → 冒烟验证
#   ./release.sh --no-deploy      只构建 + 校验产物,不动线上容器
#   ./release.sh --skip-tests     跳过测试门禁(应急用,不建议)
#   ./release.sh help             本帮助
#
# 与 CI 的分工:
#   - 本脚本:把改动落到**本机** deploy/ 栈(构建镜像 + 前端 + 部署 + 冒烟)。
#   - CI(.github/workflows/release.yml):push 到 master 后自动定版、打 tag、
#     产出 Release 部署包供他人下载。版本号由 CI 递增,本脚本不打 tag。
#   两者步骤一致(同样的 cargo test + npm build + docker build),本机验过再 push,
#   CI 基本不会翻车。
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
TOTAL=6

usage() {
  # 打印文件头注释块(到闭合的 ==== 行为止),改动 header 无需同步行号
  sed -n '2,/^# ===/p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-deploy) DO_DEPLOY=0; shift ;;
    --skip-tests) DO_TESTS=0; shift ;;
    help|-h|--help) usage ;;
    *) echo "==> 未知参数:$1(./release.sh help 看用法)" >&2; exit 1 ;;
  esac
done

step() { echo; echo "==> [$1/$TOTAL] $2"; }

# ---------- 0. 前置检查:缺什么早报错,别构建到一半才失败 ----------
for bin in docker npm cargo; do
  command -v "$bin" >/dev/null || { echo "==> 缺少 $bin,无法发布" >&2; exit 1; }
done
if [ "$DO_DEPLOY" = 1 ] && [ ! -f deploy/config.json ]; then
  echo "==> deploy/config.json 不存在;先 cd deploy && ./deploy.sh 生成并填写" >&2
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

echo
echo "==> 本机发布完成"
[ "$DO_DEPLOY" = 1 ] && echo "    入口:http://127.0.0.1/   Swagger:http://127.0.0.1/docs"
echo "    push 到 master 后,CI 会自动定版打 tag 并产出 Release 部署包"
exit 0
