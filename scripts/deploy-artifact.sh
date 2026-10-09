#!/usr/bin/env bash
# Synapse-KB 部署脚本（接收已构建的 artifact，不编译）
# 适用场景：在本地（Mac/Linux）用 Docker 构建出 server 二进制后，
#          上传到服务器跑这个脚本部署。
#
# 用法：
#   # 1. 本地打包
#   tar czf synapse-kb.tar.gz server dist
#   scp synapse-kb.tar.gz root@server:/tmp/
#
#   # 2. 服务器上执行
#   tar xzf /tmp/synapse-kb.tar.gz -C /opt/synapse-kb
#   sudo bash scripts/deploy-artifact.sh
#
# 环境变量（可选）：
#   APP_HOME          - 应用根目录（默认 /opt/synapse-kb）
#   SYNAPSE_KB_ROOT   - 知识库根（默认 /var/lib/synapse-kb/kb）
#   SYNAPSE_INDEX_DB  - 索引数据库（默认 /var/lib/synapse-kb/index.db）
#   SYNAPSE_HTTP_BIND - 监听地址（默认 127.0.0.1:19181）
#   SERVICE_NAME      - systemd 服务名（默认 synapse-kb）

set -euo pipefail

# ---------- 配置 ----------
SERVICE_NAME="${SERVICE_NAME:-synapse-kb}"
APP_HOME="${APP_HOME:-/opt/synapse-kb}"
SYNAPSE_KB_ROOT="${SYNAPSE_KB_ROOT:-/var/lib/synapse-kb/kb}"
SYNAPSE_INDEX_DB="${SYNAPSE_INDEX_DB:-/var/lib/synapse-kb/index.db}"
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

# 验证 artifact 已解压到 $APP_HOME
if [[ ! -x "$APP_HOME/server" ]]; then
  fail "$APP_HOME/server not found. Extract tarball first:
  tar xzf /tmp/synapse-kb.tar.gz -C $APP_HOME"
fi
if [[ ! -d "$APP_HOME/dist" ]]; then
  fail "$APP_HOME/dist/ not found. Make sure tarball includes dist/."
fi

log "using binary: $APP_HOME/server ($(du -h "$APP_HOME/server" | cut -f1))"
log "using static: $APP_HOME/dist/ ($(du -sh "$APP_HOME/dist" | cut -f1))"

# 验证 binary 架构（避免上传错平台的 binary）
ARCH=$(file "$APP_HOME/server" | grep -oE 'x86-64|aarch64|arm' | head -1 || echo "unknown")
log "binary arch: $ARCH"
if [[ "$ARCH" != "x86-64" && "$ARCH" != "aarch64" ]]; then
  fail "binary is not x86-64 or aarch64. Did you build for the right platform?"
fi

# ---------- 准备目录 ----------
log "preparing directories..."
mkdir -p "$APP_HOME" "$SYNAPSE_KB_ROOT"
mkdir -p "$(dirname "$SYNAPSE_INDEX_DB")"

# ---------- 权限 ----------
# 确保 service user 存在
if ! id "$SERVICE_USER" >/dev/null 2>&1; then
  warn "user '$SERVICE_USER' not found, creating..."
  useradd -r -M -s /usr/sbin/nologin "$SERVICE_USER" 2>/dev/null || true
fi

# 修正权限（www 用户需要读写 KB root 和 index.db）
chown -R "$SERVICE_USER:$SERVICE_USER" "$SYNAPSE_KB_ROOT" "$(dirname "$SYNAPSE_INDEX_DB")" 2>/dev/null || true
chmod 0755 "$APP_HOME/server"

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
Environment=SYNAPSE_STATIC_DIR=$APP_HOME/dist
Environment=RUST_LOG=info
ExecStart=$APP_HOME/server
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

# 等待并检查
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
