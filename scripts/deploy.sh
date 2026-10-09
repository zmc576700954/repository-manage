#!/usr/bin/env bash
# Synapse-KB 一键部署脚本
# 用法：
#   ./deploy.sh                          # 默认部署（git pull + build + restart）
#   ./deploy.sh --init                   # 首次部署（git clone + build + restart）
#   ./deploy.sh --init --repo <URL>      # 指定仓库 URL
#   ./deploy.sh --skip-build             # 只重启服务，不重新编译
#   ./deploy.sh --reset                  # 强制全量重新编译（清空 target）
#   SYNAPSE_REPO=/path/to/local ./deploy.sh   # 使用本地代码而非 git pull
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
if [[ $INIT_MODE -eq 1 ]]; then
  # 首次部署：克隆仓库
  [[ -n "$REPO_URL" ]] || fail "--init requires --repo <URL> or REPO_URL env"
  REPO_DIR="$APP_HOME/repo"
  if [[ -d "$REPO_DIR/.git" ]]; then
    warn "repo already exists at $REPO_DIR; pulling instead"
    cd "$REPO_DIR"
    git pull --ff-only
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
          # 把远程地址改回原始地址，便于后续 pull
          cd "$REPO_DIR"
          git remote set-url origin "$REPO_URL"
        else
          warn "ghproxy failed, trying original URL"
          git clone --depth 1 "$REPO_URL" "$REPO_DIR"
        fi
        ;;
      *)
        git clone --depth 1 "$REPO_URL" "$REPO_DIR"
        ;;
    esac
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
  git pull --ff-only || warn "git pull failed (continuing with current HEAD)"
fi

# ---------- Rust 工具链 ----------
ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then
    log "cargo found: $(cargo --version)"
    return
  fi
  warn "cargo not found, installing Rust toolchain..."

  # 重要：必须先写 ~/.cargo/config.toml，rustup-init 才会用国内镜像
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

  # 通过环境变量让 rustup 用国内镜像下载 rustup-init 本身
  export RUSTUP_DIST_SERVER="https://rsproxy.cn/rustup"
  export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

  log "downloading rustup-init from rsproxy.cn..."
  if ! curl --proto '=https' --tlsv1.2 -sSf \
        "https://rsproxy.cn/rustup-init.sh" -o /tmp/rustup-init.sh; then
    warn "rsproxy.cn failed, falling back to mirrors.ustc.edu.cn"
    curl --proto '=https' --tlsv1.2 -sSf \
      "https://mirrors.ustc.edu.cn/rustup/rustup-init.sh" -o /tmp/rustup-init.sh \
      || fail "all rustup mirrors failed"
  fi

  log "running rustup-init (this may take a few minutes)..."
  bash /tmp/rustup-init.sh -y \
    --default-toolchain stable \
    --profile minimal \
    --no-modify-path

  # 立即让后续命令可用
  source /root/.cargo/env

  # 验证安装
  if ! command -v cargo >/dev/null 2>&1; then
    fail "rust install completed but cargo not found. Check /tmp/rustup-init.log"
  fi
  log "installed: $(rustc --version)"
  log "cargo at: $(which cargo)"
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
