#!/usr/bin/env bash
# Synapse-KB 部署脚本（接收已构建的 artifact，不编译）
# 适用场景：把已构建好的 server 二进制 + dist 上传到服务器，配置 systemd 启动。
#
# 用法：
#   cd /path/to/your/deploy/dir   # 这个目录必须有 server 和 dist/
#   sudo bash deploy-artifact.sh
#
# 或者指定路径：
#   APP_HOME=/path/to/dir sudo bash deploy-artifact.sh
#
# 环境变量（可选）：
#   APP_HOME          - 部署根目录（默认当前目录）
#   SYNAPSE_KB_ROOT   - 知识库根（默认 $APP_HOME/kb）
#   SYNAPSE_INDEX_DB  - 索引数据库（默认 $APP_HOME/index.db）
#   SYNAPSE_HTTP_BIND - 监听地址（默认 127.0.0.1:19181）
#   SERVICE_NAME      - systemd 服务名（默认 synapse-kb）
#   SERVICE_USER      - 运行用户（默认 www）

set -euo pipefail

# ---------- 默认值 ----------
APP_HOME="${APP_HOME:-$(pwd)}"
SERVICE_NAME="${SERVICE_NAME:-synapse-kb}"
SYNAPSE_KB_ROOT="${SYNAPSE_KB_ROOT:-$APP_HOME/kb}"
SYNAPSE_INDEX_DB="${SYNAPSE_INDEX_DB:-$APP_HOME/index.db}"
SYNAPSE_HTTP_BIND="${SYNAPSE_HTTP_BIND:-127.0.0.1:19181}"
SERVICE_USER="${SERVICE_USER:-www}"

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

# 解析 APP_HOME 成绝对路径
APP_HOME="$(cd "$APP_HOME" && pwd)"
SERVER_BIN="$APP_HOME/server"
STATIC_DIR="$APP_HOME/dist"

# 验证必要文件
[[ -x "$SERVER_BIN" ]] || fail "binary not found or not executable: $SERVER_BIN
  → Make sure you uploaded server to this directory, then: chmod +x $SERVER_BIN"
[[ -d "$STATIC_DIR" ]] || fail "static dir not found: $STATIC_DIR
  → Make sure the tarball's dist/ folder is here"

log "APP_HOME:   $APP_HOME"
log "binary:     $SERVER_BIN ($(du -h "$SERVER_BIN" | cut -f1))"
log "static:     $STATIC_DIR ($(du -sh "$STATIC_DIR" | cut -f1))"
log "kb_root:    $SYNAPSE_KB_ROOT"
log "bind:       $SYNAPSE_HTTP_BIND"
log "service:    $SERVICE_NAME.service"

# 验证 binary 架构
ARCH=$(file "$SERVER_BIN" | grep -oE 'x86-64|aarch64|arm' | head -1 || echo "unknown")
log "binary arch: $ARCH"
[[ "$ARCH" == "x86-64" || "$ARCH" == "aarch64" ]] || \
  fail "binary is not x86-64 or aarch64 (got: $ARCH)"

# ---------- 准备目录 ----------
log "preparing directories..."
mkdir -p "$SYNAPSE_KB_ROOT" "$(dirname "$SYNAPSE_INDEX_DB")"

# ---------- service user ----------
if ! id "$SERVICE_USER" >/dev/null 2>&1; then
  warn "user '$SERVICE_USER' not found, creating..."
  useradd -r -M -s /usr/sbin/nologin "$SERVICE_USER" 2>/dev/null || true
fi

# 权限（service user 需要读写 KB root 和 index.db）
chown -R "$SERVICE_USER:$SERVICE_USER" "$SYNAPSE_KB_ROOT" "$(dirname "$SYNAPSE_INDEX_DB")" 2>/dev/null || true
chmod 0755 "$SERVER_BIN"

# ---------- systemd ----------
SYSTEMD_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
log "writing systemd unit -> $SYSTEMD_FILE"

cat > "$SYSTEMD_FILE" <<EOF
[Unit]
Description=Synapse-KB Web Server
After=network.target

[Service]
Type=simple
User=$SERVICE_USER
WorkingDirectory=$APP_HOME
Environment=SYNAPSE_KB_ROOT=$SYNAPSE_KB_ROOT
Environment=SYNAPSE_INDEX_DB=$SYNAPSE_INDEX_DB
Environment=SYNAPSE_HTTP_BIND=$SYNAPSE_HTTP_BIND
Environment=SYNAPSE_STATIC_DIR=$STATIC_DIR
Environment=RUST_LOG=info
ExecStart=$SERVER_BIN
Restart=on-failure
RestartSec=5

LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "${SERVICE_NAME}.service" 2>/dev/null || true

log "restarting service..."
systemctl restart "${SERVICE_NAME}.service"

sleep 1
if systemctl is-active --quiet "${SERVICE_NAME}.service"; then
  log "${GREEN}✓ service active${RESET}"
else
  fail "service failed to start. Check: journalctl -u $SERVICE_NAME -n 50"
fi

# smoke test
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
