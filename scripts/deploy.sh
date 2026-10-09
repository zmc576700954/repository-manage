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
# 宝塔兼容：
#   自动检测 /www/wwwroot/*/repository-manage 目录
#   也可以通过 SYNAPSE_REPO 环境变量显式指定
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

# 宝塔兼容：自动检测 /www/wwwroot 下的项目目录
# 优先级：SYNAPSE_REPO 环境变量 > 宝塔自动检测 > APP_HOME/repo（默认）
# 匹配规则：路径下存在 deploy.sh 或 .git
if [[ -z "${SYNAPSE_REPO:-}" && -d "/www/wwwroot" ]]; then
  for site_dir in /www/wwwroot/*/repository-manage; do
    if [[ -d "$site_dir/.git" || -f "$site_dir/scripts/deploy.sh" ]]; then
      export SYNAPSE_REPO="$site_dir"
      log "auto-detected 宝塔 repo at: $SYNAPSE_REPO"
      break
    fi
  done
fi

# ---------- 预检 ----------
[[ $(id -u) -eq 0 ]] || fail "must run as root (use sudo)"

# ---------- 仓库准备 ----------
# 智能处理 gitee remote：兼容旧的 origin 命名
# 注意：避免 git -C（Git < 1.8 不支持），改用 subshell + cd
fix_remote_if_github() {
  local dir="$1"
  (
    cd "$dir"
    # 检查 origin（兼容命名）
    local current=$(git remote get-url origin 2>/dev/null || echo "")
    local gitee_url=$(git remote get-url gitee 2>/dev/null || echo "")
    local github_url=$(git remote get-url github 2>/dev/null || echo "")

    # 把任何指向 GitHub 的 remote 改名为 github
    if [[ "$current" == *"github.com"* ]]; then
      if [[ "$github_url" == "" ]]; then
        git remote rename origin github
        log "renamed remote 'origin' -> 'github'"
      fi
    fi

    # 确保有 gitee remote
    if ! git remote get-url gitee >/dev/null 2>&1; then
      git remote add gitee "https://gitee.com/zhu_ming_chen/repository-manage.git"
      log "added remote 'gitee'"
    fi
  )
}

if [[ $INIT_MODE -eq 1 ]]; then
  # 首次部署：克隆仓库
  [[ -n "$REPO_URL" ]] || fail "--init requires --repo <URL> or REPO_URL env"
  REPO_DIR="$APP_HOME/repo"
  if [[ -d "$REPO_DIR/.git" ]]; then
    warn "repo already exists at $REPO_DIR; pulling instead"
    cd "$REPO_DIR"
    fix_remote_if_github "$REPO_DIR"
    # git < 1.7.10 不支持 --ff-only，用 fetch + reset 实现等价语义
    git fetch gitee main 2>/dev/null && git reset --hard gitee/main || warn "git pull gitee failed (continuing)"
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
  # git < 1.7.10 不支持 --ff-only，用 fetch + reset 实现等价语义
  git fetch gitee main 2>/dev/null && git reset --hard gitee/main || warn "git pull gitee failed (continuing)"
fi

# ---------- Rust 工具链 ----------
# rsproxy.cn 镜像配置（关键：RUSTUP_DIST_SERVER 不能带 /rustup 后缀！）
#   RUSTUP_DIST_SERVER  - toolchain 实际下载位置（不带路径）
#   RUSTUP_UPDATE_ROOT  - rustup 元数据（带 /rustup）
RUST_DIST_SERVER="${RUSTUP_DIST_SERVER:-https://rsproxy.cn}"
RUST_UPDATE_ROOT="${RUSTUP_UPDATE_ROOT:-https://rsproxy.cn/rustup}"
RUST_MIRRORS=(
  "https://rsproxy.cn/rustup"      # rsproxy - 实际下载 rustup-init 用
  "https://mirrors.ustc.edu.cn/rustup"
  "https://mirrors.tuna.tsinghua.edu.cn/rustup"
)
# 固定 rustc 版本（避免 stable 滚动导致元数据不一致）
# 注意：rsproxy 镜像对各版本同步速度不同，按可用性从新到旧排序
# 脚本会按顺序尝试，第一个能下载成功的就用
RUST_VERSIONS=(
  "1.85.0"
  "1.84.0"
  "1.83.0"
  "1.82.0"
  "1.81.0"
  "1.80.0"
)
RUST_VERSION="${RUST_VERSION:-}"  # 空表示自动尝试

ensure_rust() {
  if command -v cargo >/dev/null 2>&1; then
    log "cargo found: $(cargo --version)"
    return
  fi
  warn "cargo not found, installing Rust toolchain..."

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
    # 关键修正：RUSTUP_DIST_SERVER 不能带 /rustup 后缀
    # RUSTUP_UPDATE_ROOT 才是 /rustup 路径
    export RUSTUP_DIST_SERVER="$RUST_DIST_SERVER"
    export RUSTUP_UPDATE_ROOT="$RUST_UPDATE_ROOT"

    # 尝试多个 rustc 版本（rsproxy 对不同版本同步速度不同）
    local versions_to_try=()
    if [[ -n "$RUST_VERSION" ]]; then
      versions_to_try=("$RUST_VERSION")
    else
      versions_to_try=("${RUST_VERSIONS[@]}")
    fi

    for ver in "${versions_to_try[@]}"; do
      log "trying rustc version $ver (3-5 minutes)..."
      if bash /tmp/rustup-init.sh -y \
            --default-toolchain "$ver" \
            --profile minimal \
            --no-modify-path 2>&1 | tail -10; then
        source /root/.cargo/env
        if command -v cargo >/dev/null 2>&1; then
          log "${GREEN}✓ rust installed${RESET}: $(rustc --version)"
          return
        fi
      fi
      warn "version $ver not available on mirror; trying next..."
    done
    warn "all versions failed via rustup; falling back to apt"
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

# ---------- Node.js / npm ----------
# NodeSource 国内镜像：https://npmmirror.com/mirrors/node/
ensure_node() {
  # 宝塔可能装多个版本：扫描 /www/server/nodejs/ 下所有版本目录
  if [[ -d "/www/server/nodejs" ]]; then
    # 优先取 current 软链
    if [[ -x "/www/server/nodejs/current/bin/npm" ]]; then
      local bt_path="/www/server/nodejs/current"
      log "found 宝塔 node (current) at $bt_path"
      export PATH="$bt_path/bin:$PATH"
      ln -sf "$bt_path/bin/node" /usr/local/bin/node 2>/dev/null
      ln -sf "$bt_path/bin/npm" /usr/local/bin/npm 2>/dev/null
      ln -sf "$bt_path/bin/npx" /usr/local/bin/npx 2>/dev/null
      log "✓ using 宝塔 node: $(node --version 2>&1 | head -1), npm $(npm --version 2>&1 | head -1)"
      return
    fi
    # 否则按版本号从高到低尝试
    for ver_dir in $(ls -1d /www/server/nodejs/v* 2>/dev/null | sort -rV); do
      if [[ -x "$ver_dir/bin/npm" ]]; then
        log "found 宝塔 node at $ver_dir"
        export PATH="$ver_dir/bin:$PATH"
        ln -sf "$ver_dir/bin/node" /usr/local/bin/node 2>/dev/null
        ln -sf "$ver_dir/bin/npm" /usr/local/bin/npm 2>/dev/null
        ln -sf "$ver_dir/bin/npx" /usr/local/bin/npx 2>/dev/null
        log "✓ using 宝塔 node: $(node --version 2>&1 | head -1), npm $(npm --version 2>&1 | head -1)"
        return
      fi
    done
  fi

  if command -v npm >/dev/null 2>&1; then
    log "npm found: $(npm --version 2>/dev/null || echo 'unknown')"
    return
  fi
  warn "npm not found, installing Node.js 16.x..."

  # 先尝试 apt（最简单）
  if command -v apt-get >/dev/null 2>&1; then
    log "trying apt-get install nodejs npm..."
    if apt-get install -y --no-install-recommends nodejs npm 2>/dev/null && command -v npm >/dev/null 2>&1; then
      log "✓ nodejs installed via apt: $(node --version), npm $(npm --version)"
      return
    fi
    warn "apt install failed, trying NodeSource mirror"
  fi

  # fallback：NodeSource 镜像
  log "downloading Node.js 16.20.2 from npmmirror.com (compatible with glibc 2.17)..."
  # 用 .tar.gz 而不是 .tar.xz（避免需要 xz 工具）
  # -L 强制跟随重定向（npmmirror 会 302 到 cdn.npmmirror.com）
  # 注意：Node 20 需要 glibc 2.28+，对 CentOS 7 / Debian 9 等老系统不兼容
  #       Node 16 支持 glibc 2.17，兼容绝大多数 Linux 发行版
  if curl -L --proto '=https' --tlsv1.2 -sSf \
        --connect-timeout 10 --max-time 300 \
        "https://cdn.npmmirror.com/binaries/node/v16.20.2/node-v16.20.2-linux-x64.tar.gz" \
        -o /tmp/node.tar.gz; then
    # 验证下载的是真 gzip（防止 HTML 错误页）
    if ! file /tmp/node.tar.gz | grep -q "gzip"; then
      fail "downloaded file is not gzip. First 100 bytes: $(head -c 100 /tmp/node.tar.gz)"
    fi
    mkdir -p /opt/node
    tar -xzf /tmp/node.tar.gz -C /opt/node --strip-components=1
    export PATH="/opt/node/bin:$PATH"
    ln -sf /opt/node/bin/node /usr/local/bin/node
    ln -sf /opt/node/bin/npm /usr/local/bin/npm
    ln -sf /opt/node/bin/npx /usr/local/bin/npx
    log "✓ node installed from npmmirror: $(node --version), npm $(npm --version)"
    return
  fi

  fail "could not install Node.js. Install manually first."
}

ensure_node

# 确保 cargo 和 node 在 PATH（即使前面 source 过）
# 宝塔 node 路径优先（扫描所有版本，取最新的）
NODE_BIN=""
if [[ -d "/www/server/nodejs" ]]; then
  for ver_dir in $(ls -1d /www/server/nodejs/v* 2>/dev/null | sort -rV); do
    if [[ -x "$ver_dir/bin/npm" ]]; then
      NODE_BIN="$ver_dir/bin"
      break
    fi
  done
  [[ -x "/www/server/nodejs/current/bin/npm" ]] && NODE_BIN="/www/server/nodejs/current/bin"
fi
[[ -x /opt/node/bin/npm ]] && NODE_BIN="/opt/node/bin"
export PATH="/root/.cargo/bin:${NODE_BIN}:$PATH"

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
