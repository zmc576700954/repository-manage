#!/usr/bin/env bash
# 同时推送到 Gitee（origin）和 GitHub（github）。
# Gitee 是国内主仓，GitHub 作为镜像备份。
#
# 用法：
#   ./scripts/push-all.sh                  # 推送当前分支
#   ./scripts/push-all.sh feature-x        # 推送指定分支
#   ./scripts/push-all.sh --tags           # 推送 tags

set -euo pipefail

BRANCH="${1:-$(git symbolic-ref --short HEAD 2>/dev/null || echo main)}"
PUSH_TAGS=0

for arg in "$@"; do
  case "$arg" in
    --tags) PUSH_TAGS=1 ;;
  esac
done

# 颜色
if [[ -t 1 ]]; then
  GREEN=$'\033[32m'; YELLOW=$'\033[33m'; RED=$'\033[31m'; RESET=$'\033[0m'
else
  GREEN=''; YELLOW=''; RED=''; RESET=''
fi
log()  { echo "${GREEN}[$(date +%H:%M:%S)]${RESET} $*"; }
warn() { echo "${YELLOW}[$(date +%H:%M:%S)] WARN${RESET} $*" >&2; }
fail() { echo "${RED}[$(date +%H:%M:%S)] FAIL${RESET} $*" >&2; exit 1; }

cd "$(git rev-parse --show-toplevel)"

# 检查两个 remote 是否都存在
if ! git remote get-url gitee >/dev/null 2>&1; then
  fail "remote 'gitee' not configured. Add: git remote add gitee https://gitee.com/zhu_ming_chen/repository-manage.git"
fi
if ! git remote get-url github >/dev/null 2>&1; then
  warn "remote 'github' not configured (will skip)"
  HAS_GITHUB=0
else
  HAS_GITHUB=1
fi

# 检查 working tree 干净
if ! git diff --quiet HEAD 2>/dev/null; then
  warn "you have unstaged changes; commit first"
  git status --short
  exit 1
fi

log "pushing branch '$BRANCH' to Gitee..."
git push gitee "$BRANCH"

if [[ $PUSH_TAGS -eq 1 ]]; then
  log "pushing tags to Gitee..."
  git push gitee --tags
fi

if [[ $HAS_GITHUB -eq 1 ]]; then
  log "pushing branch '$BRANCH' to GitHub..."
  if ! git push github "$BRANCH" 2>&1; then
    warn "GitHub push failed (check credentials: gh auth login or SSH key)"
    warn "Gitee push succeeded; GitHub is out of sync"
    exit 0
  fi

  if [[ $PUSH_TAGS -eq 1 ]]; then
    log "pushing tags to GitHub..."
    git push github --tags || warn "GitHub tags push failed"
  fi
fi

log "${GREEN}✓ all remotes synced${RESET}"
