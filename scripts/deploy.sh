#!/usr/bin/env bash
# Synapse-KB 一键部署脚本
# 用法：
#   ./deploy.sh                          # 默认部署（git pull + build + restart）
#   ./deploy.sh --init --repo <URL>      # 首次部署（git clone + build + restart）
#   ./deploy.sh --skip-build             # 只重启服务，不重新编译
#   ./deploy.sh --reset                  # 强制全量重新编译（清空 target）
#   SYNAPSE_REPO=/path/to/local ./deploy.sh   # 使用本地代码而非 git pull
#   RUST_VERSION=1.84.0 ./deploy.sh      # 指定 rustc 版本（默认 1.83.0）
#
# 环境变量（可选，会写入 systemd service 文件）：
#   SYNAPSE_KB_ROOT    - 知识库根目录（默认 /var/lib/synapse-kb/kb）
#   SYNAPSE_INDEX_DB   - 索引数据库路径
#   SYNAPSE_HTTP_BIND  - 监听地址（默认 127.0.0.1:19181）
#   SYNAPSE_STATIC_DIR - 前端 dist 目录（默认 /opt/synapse-kb/dist）
#   DEPLOY_BIN_PATH    - 部署二进制目标路径（默认 /opt/synapse-kb/server）
#   SERVICE_NAME       - systemd 服务名（默认 synapse-kb）
#   REPO_URL           - git 仓库地址（--init 模式时使用，默认空）

set -euo pipefail

# ---------- 配置 ----------
SERVICE_NAME="${SERVICE_NAME:-synapse-kb}"
APP_HOME="${APP_HOME:-/opt/synapse-kb}"
DEPLOY_BIN_PATH="${DEPLOY_BIN_PATH:-$APP_HOME/server}"
SYNAPSE_KB_ROOT="${SYNAPSE_KB_ROOT:-/var/lib/synapse-kb/kb}"
SYNAPSE_INDEX_DB="${SYNAPSE_INDEX_DB:-/var/lib/synapse-kb/index.db}"
SYNAPSE_HTTP_BIND="${SYNAPSE_HTTP_BIND:-127.0.0.1:19181}"
SYNAPSE_STATIC_DIR="${SYNAPSE_STATIC_DIR:-$APP_HOME/dist}"
REPO_URL="${REPO_URL:-}"

# ---------- 参数 ----------
SKIP_BUILD=0
RESET=0
INIT_MODE=0
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    --reset)      RESET=1 ;;
    --init)       INIT_MODE=1 ;;
    --repo)       shift; REPO_URL="${1:-}" ;;
    --repo=*)     REPO_URL="${arg#--repo=}" ;;
    -h|--help)
      grep '^#' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) echo "Unknown argument: $arg" >&2; exit 1 ;;
  esac
done

# ---------- 颜色 ----------
if [[ -t 1 ]]; then
  RED=$'\033[31m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; BOLD=$'\033[1m'; RESET=$'\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; BOLD=''; RESET=''
fi
log()  { echo "${GREEN}[$(date +%H:%M:%S)]${RESET} $*"; }
warn() { echo "${YELLOW}[$(date +%H:%M:%S)] WARN${RESET} $*" >&2; }
fail() { echo "${RED}[$(date +%H:%M:%S)] FAIL${RESET} $*" >&2; exit 1; }

# ---------- 预检 ----------
[[ $(id -u) -eq 0 ]] || fail "must run as root (use sudo)"

# ---------- 仓库准备 ----------
# 智能处理 gitee remote：兼容旧的 origin 命名
fix_remote_if_github() {
  local dir="$1"
  # 检查 origin（兼容命名）
  local current=$(git -C "$dir" remote get-url origin 2>/dev/null || echo "")
  local gitee_url=$(git -C "$dir" remote get-url gitee 2>/dev/null || echo "")
  local github_url=$(git -C "$dir" remote get-url github 2>/dev/null || echo "")

  # 把任何指向 GitHub 的 remote 改名为 github
  if [[ "$current" == *"github.com"* ]]; then
    if [[ "$github_url" == "" ]]; then
      git -C "$dir" remote rename origin github
      log "renamed remote 'origin' -> 'github'"
    fi
  fi

  # 确保有 gitee remote
  if ! git -C "$dir" remote get-url gitee >/dev/null 2>&1; then
    git -C "$dir" remote add gitee "https://gitee.com/zhu_ming_chen/repository-manage.git"
    log "added remote 'gitee'"
  fi
}

if [[ $INIT_MODE -eq 1 ]]; then
  # 首次部署：克隆仓库
  [[ -n "$REPO_URL" ]] || fail "--init requires --repo <URL> or REPO_URL env"
  REPO_DIR="$APP_HOME/repo"
  if [[ -d "$REPO_DIR/.git" ]]; then
    warn "repo already exists at $REPO_DIR; pulling instead"
    cd "$REPO_DIR"
    fix_remote_if_github "$REPO_DIR"
    git pull gitee main --ff-only || warn "git pull gitee failed (continuing)"
  else
    log "cloning $REPO_URL -> $REPO_DIR"
    mkdir -p "$APP_HOME"
    # 国内服务器优先使用镜像
    case "$REPO_URL" in
      https://github.com/*)
        MIRROR_URL="https://ghproxy.com/${REPO_URL}"
        log "trying ghproxy mirror first: $MIRROR_URL"
        if git clone --depth 1 "$MIRROR_URL" "$REPO_DIR" 2>/dev/null; then
          log "cloned via ghproxy"
          cd "$REPO_DIR"
          git remote rename origin github
          git remote add gitee "https://gitee.com/zhu_ming_chen/repository-manage.git"
        else
          warn "ghproxy failed, trying original URL"
          git clone --depth 1 "$REPO_URL" "$REPO_DIR"
        fi
        ;;
      *)
        git clone --depth 1 "$REPO_URL" "$REPO_DIR"
        ;;
    esac
    cd "$REPO_DIR"
    fix_remote_if_github "$REPO_DIR"
  fi
elif [[ "${SYNAPSE_REPO:-}" != "" && -d "${SYNAPSE_REPO}" ]]; then
  REPO_DIR="$SYNAPSE_REPO"
  log "using local repo: $REPO_DIR"
else
  REPO_DIR="$APP_HOME/repo"
  if [[ ! -d "$REPO_DIR/.git" ]]; then
    fail "repo not found at $REPO_DIR. Use --init --repo <URL> for first-time setup."
  fi
  log "updating repo: $REPO_DIR"
  cd "$REPO_DIR"
  fix_remote_if_github "$REPO_DIR"
  git pull gitee main --ff-only || warn "git pull gitee failed (continuing)"
fi

# ---------- Rust 工具链 ----------
# 镜像优先级（按国内可用性排序）：
#   1. rsproxy.cn  （rust 官方代理，最快）
#   2. mirrors.ustc.edu.cn  （中科大）
#   3. mirrors.tuna.tsinghua.edu.cn  （清华）
#   4. apt 系统包  （最后 fallback，版本可能较旧）
RUST_MIRRORS=(
  "https://rsproxy.cn/rustup"
  "https://mirrors.ustc.edu.cn/rustup"
  "https://mirrors.tuna.tsinghua.edu.cn/rustup"
)
# 固定 rustc 版本（避免 stable 滚动导致元数据不一致）
RUST_VERSION="${RUST_VERSION:-1.83.0}"

ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then
    log "cargo found: $(cargo --version)"
    return
  fi
  warn "cargo not found, installing Rust toolchain ${RUST_VERSION}..."

  # 写 cargo 配置（crates 镜像）
  mkdir -p /root/.cargo
  cat > /root/.cargo/config.toml <<'CARGO_EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"

[net]
git-fetch-with-cli = true
CARGO_EOF

  # 依次尝试每个镜像下载 rustup-init
  local downloaded=0
  for mirror in "${RUST_MIRRORS[@]}"; do
    log "trying mirror: $mirror"
    if curl --proto '=https' --tlsv1.2 -sSf \
          --connect-timeout 10 --max-time 60 \
          "${mirror}/rustup-init.sh" -o /tmp/rustup-init.sh; then
      downloaded=1
      log "downloaded from $mirror"
      break
    else
      warn "  $mirror failed"
    fi
  done

  if [[ $downloaded -eq 1 ]]; then
    # 让 rustup 用第一个镜像下载 toolchain
    export RUSTUP_DIST_SERVER="${RUST_MIRRORS[0]}"
    export RUSTUP_UPDATE_ROOT="${RUST_MIRRORS[0]}"

    log "running rustup-init (version ${RUST_VERSION}, this may take 3-5 minutes)..."
    if bash /tmp/rustup-init.sh -y \
          --default-toolchain "${RUST_VERSION}" \
          --profile minimal \
          --no-modify-path 2>&1 | tail -20; then
      source /root/.cargo/env
      if command -v cargo >/dev/null 2>&1; then
        log "${GREEN}✓ rust installed${RESET}: $(rustc --version)"
        return
      fi
    fi
    warn "rustup-init failed; falling back to apt"
  else
    warn "all rustup mirrors unreachable; falling back to apt"
  fi

  # 最后 fallback：apt 安装系统包
  # 注意：Debian 12 自带 rustc 1.63，低于本项目要求的 1.66；Ubutnu 22.04 是 1.75
  log "trying apt-get install rustc cargo..."
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update -qq && apt-get install -y --no-install-recommends rustc cargo
    if command -v cargo >/dev/null 2>&1; then
      local rust_ver=$(rustc --version | grep -oE '[0-9]+\.[0-9]+' | head -1)
      log "apt installed rustc ${rust_ver}"
      # 检查是否满足最低 1.66
      if awk -v v="$rust_ver" 'BEGIN{exit !(v >= 1.66)}'; then
        log "${GREEN}✓ rust installed via apt${RESET}: $(rustc --version)"
        return
      else
        warn "apt rustc ${rust_ver} is too old (< 1.66). Need rustup install."
      fi
    fi
  fi

  fail "could not install Rust toolchain via any method. Install rustup manually: https://rustup.rs/"
}

ensure_rust

# ---------- npm ----------
if ! command -v npm >/dev/null 2>&1; then
  fail "npm not found. Install Node.js first."
fi

# 确保 cargo 在 PATH（即使前面 source 过）
export PATH="/root/.cargo/bin:$PATH"

# ---------- 构建 ----------
cd "$REPO_DIR"

if [[ $RESET -eq 1 ]]; then
  warn "removing target/ for clean rebuild"
  rm -rf src-tauri/target
fi

if [[ $SKIP_BUILD -eq 1 ]]; then
  log "--skip-build: skipping rebuild"
else
  log "installing frontend deps..."
  # npm 镜像源
  npm config set registry https://registry.npmmirror.com >/dev/null
  npm ci --no-audit --no-fund

  log "building frontend..."
  npm run build

  log "building server binary (release)..."
  cd src-tauri
  cargo build --release --bin server
  cd ..
fi

# ---------- 部署 ----------
log "preparing deploy directories..."
mkdir -p "$APP_HOME" "$SYNAPSE_KB_ROOT"
mkdir -p "$(dirname "$SYNAPSE_INDEX_DB")"
mkdir -p "$(dirname "$DEPLOY_BIN_PATH")"

log "copying binary -> $DEPLOY_BIN_PATH"
install -m 0755 src-tauri/target/release/server "$DEPLOY_BIN_PATH"

log "syncing dist -> $SYNAPSE_STATIC_DIR"
rm -rf "$SYNAPSE_STATIC_DIR"
mkdir -p "$SYNAPSE_STATIC_DIR"
cp -r dist/. "$SYNAPSE_STATIC_DIR"/

# 修正权限
chown -R "$SERVICE_USER:${SERVICE_USER:-www}" "$APP_HOME" "$SYNAPSE_KB_ROOT" "$(dirname "$SYNAPSE_INDEX_DB")" 2>/dev/null || true

# ---------- systemd ----------
SYSTEMD_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
log "writing systemd unit -> $SYSTEMD_FILE"

cat > "$SYSTEMD_FILE" <<EOF
[Unit]
Description=Synapse-KB Web Server
After=network.target

[Service]
Type=simple
User=www
WorkingDirectory=$APP_HOME
Environment=SYNAPSE_KB_ROOT=$SYNAPSE_KB_ROOT
Environment=SYNAPSE_INDEX_DB=$SYNAPSE_INDEX_DB
Environment=SYNAPSE_HTTP_BIND=$SYNAPSE_HTTP_BIND
Environment=SYNAPSE_STATIC_DIR=$SYNAPSE_STATIC_DIR
Environment=RUST_LOG=info
ExecStart=$DEPLOY_BIN_PATH
Restart=on-failure
RestartSec=5

# 资源限制
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "${SERVICE_NAME}.service" 2>/dev/null || true

log "restarting service..."
systemctl restart "${SERVICE_NAME}.service"

# 等待并检查
sleep 1
if systemctl is-active --quiet "${SERVICE_NAME}.service"; then
  log "${GREEN}✓ service active${RESET}"
else
  fail "service failed to start. Check: journalctl -u $SERVICE_NAME -n 50"
fi

log "smoke test: GET /api/entries"
if curl -fsS --max-time 5 "http://${SYNAPSE_HTTP_BIND}/api/entries" >/dev/null; then
  log "${GREEN}✓ API responding${RESET}"
else
  warn "API not responding yet (may need a moment to bind)"
fi

log "${BOLD}deployment complete${RESET}"
echo ""
echo "  Service:  systemctl status $SERVICE_NAME"
echo "  Logs:     journalctl -u $SERVICE_NAME -f"
echo "  API URL:  http://${SYNAPSE_HTTP_BIND}"
echo "  KB root:  $SYNAPSE_KB_ROOT"
