# Synapse-KB Dockerfile
# 在 Linux 容器内构建独立 server 二进制（不依赖 tauri 桌面运行时）
# 适合在 macOS 主机上构建后上传到 Linux 服务器运行
#
# 用法：
#   # 1. 在 Mac 上构建（自动 cargo + npm build）
#   docker build -t synapse-kb-builder .
#
#   # 2. 提取二进制 + dist
#   mkdir -p dist-out
#   docker create --name extract synapse-kb-builder
#   docker cp extract:/opt/synapse-kb/server ./dist-out/server
#   docker cp extract:/opt/synapse-kb/dist/. ./dist-out/dist/
#   docker rm extract
#
#   # 3. 上传到服务器
#   scp -r dist-out/* root@server:/opt/synapse-kb/
#
#   # 4. 服务器上启动
#   SYNAPSE_KB_ROOT=/var/lib/synapse-kb/kb /opt/synapse-kb/server

# ---------- 阶段 1：构建（cargo + npm）----------
FROM rust:1.90-slim-bookworm AS builder

# 国内镜像：Debian apt 源
RUN sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list.d/debian.sources 2>/dev/null || \
    sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev ca-certificates nodejs npm \
      libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    && rm -rf /var/lib/apt/lists/*

# cargo 国内镜像
RUN mkdir -p ~/.cargo && \
    printf '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n\n[source.rsproxy-sparse]\nregistry = "sparse+https://rsproxy.cn/index/"\n\n[registries.rsproxy]\nindex = "https://rsproxy.cn/crates.io-index"\n\n[net]\ngit-fetch-with-cli = true\n' > ~/.cargo/config.toml

# npm 镜像
RUN npm config set registry https://registry.npmmirror.com

# ---------- 阶段 2：构建前端 ----------
WORKDIR /build
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY . .
RUN npm run build

# ---------- 阶段 3：构建后端 ----------
WORKDIR /build/src-tauri

# 先复制 manifest，让 cargo 解析依赖（命中缓存）
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./

# 复制真实源码（src/bin/server.rs 由 cargo 自动发现为 bin target）
COPY src-tauri/src ./src
# build.rs 必须保留：tauri_build::build() 调用 tauri::generate_context! 宏
# 需要生成 OUT_DIR 环境变量

# 编译
RUN cargo build --release --bin server \
    && strip target/release/server \
    && ls -lh target/release/server

# ---------- 阶段 4：整合产物 ----------
RUN mkdir -p /opt/synapse-kb/dist && \
    cp target/release/server /opt/synapse-kb/server && \
    cp -r /build/dist/. /opt/synapse-kb/dist/ && \
    chmod +x /opt/synapse-kb/server && \
    echo "build artifacts:" && \
    ls -lh /opt/synapse-kb/server && \
    ls /opt/synapse-kb/dist/ | head -10

# ---------- 阶段 5：运行时镜像（可独立运行）----------
FROM debian:bookworm-slim AS runtime

RUN sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list.d/debian.sources 2>/dev/null || \
    sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates tzdata curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 1000 -m -d /home/synapse -s /usr/sbin/nologin synapse

WORKDIR /opt/synapse-kb
COPY --from=builder /opt/synapse-kb /opt/synapse-kb

RUN mkdir -p /var/lib/synapse-kb && chown -R synapse:synapse /var/lib/synapse-kb

USER synapse

ENV SYNAPSE_KB_ROOT=/var/lib/synapse-kb/kb \
    SYNAPSE_INDEX_DB=/var/lib/synapse-kb/index.db \
    SYNAPSE_HTTP_BIND=0.0.0.0:19181 \
    SYNAPSE_STATIC_DIR=/opt/synapse-kb/dist \
    RUST_LOG=info

EXPOSE 19181

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:19181/api/entries || exit 1

ENTRYPOINT ["/opt/synapse-kb/server"]
