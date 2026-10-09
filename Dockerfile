# Synapse-KB Dockerfile
# 多阶段构建：国内镜像源 + cargo 依赖缓存 + 精简运行时镜像
#
# 构建：
#   docker build -t synapse-kb:latest .
# 运行：
#   docker run -d --name synapse-kb \
#     -p 127.0.0.1:19181:19181 \
#     -v /var/lib/synapse-kb/kb:/var/lib/synapse-kb/kb:ro \
#     -v /var/lib/synapse-kb/index.db:/var/lib/synapse-kb/index.db \
#     -e SYNAPSE_KB_ROOT=/var/lib/synapse-kb/kb \
#     -e SYNAPSE_INDEX_DB=/var/lib/synapse-kb/index.db \
#     synapse-kb:latest

# ---------- 阶段 1：缓存 cargo 依赖 ----------
FROM rust:1.83-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /build

# ---------- 阶段 2：准备 recipe ----------
FROM chef AS planner
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./
COPY src-tauri/src ./src
COPY src-tauri/bin ./bin
# 移除 build.rs 引用（Tauri 桌面构建相关，避免污染 recipe）
RUN rm -f build.rs
RUN cargo chef prepare --recipe-path recipe.json

# ---------- 阶段 3：构建依赖（缓存层） ----------
FROM chef AS builder

# 国内镜像：USTC Debian + rsproxy cargo + npmmirror
RUN sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list.d/debian.sources 2>/dev/null || \
    sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev ca-certificates nodejs npm \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git
# 用 rsproxy.cn 替换默认 crates 源（国内加速）
RUN mkdir -p ~/.cargo && \
    printf '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n\n[source.rsproxy-sparse]\nregistry = "sparse+https://rsproxy.cn/index/"\n\n[registries.rsproxy]\nindex = "https://rsproxy.cn/crates.io-index"\n\n[net]\ngit-fetch-with-cli = true\n' > ~/.cargo/config.toml

# npm 镜像
RUN npm config set registry https://registry.npmmirror.com

# 先编译依赖（命中缓存时秒过）
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --bin server

# ---------- 阶段 4：编译源码 ----------
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./
COPY src-tauri/src ./src
COPY src-tauri/bin ./bin
RUN rm -f build.rs

# 复制前端 dist（先在主机构建更高效；这里给出 in-container 选项）
# 推荐方式：先用单独命令构建 dist 后通过 docker build 传入
# COPY dist /tmp/dist
# ENV SYNAPSE_STATIC_DIR=/opt/synapse-kb/dist
# RUN mkdir -p /opt/synapse-kb/dist && cp -r /tmp/dist/. /opt/synapse-kb/dist/

RUN cargo build --release --bin server \
    && strip target/release/server \
    && ls -lh target/release/server

# ---------- 阶段 5：运行时镜像 ----------
FROM debian:bookworm-slim AS runtime

# 镜像源切到 USTC
RUN sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list.d/debian.sources 2>/dev/null || \
    sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g; s|security.debian.org|mirrors.ustc.edu.cn|g' \
      /etc/apt/sources.list

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates tzdata curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 1000 -m -d /home/synapse -s /usr/sbin/nologin synapse

WORKDIR /opt/synapse-kb

# 复制二进制（来自构建阶段）
COPY --from=builder /build/target/release/server /usr/local/bin/synapse-kb-server

# 默认数据目录
RUN mkdir -p /var/lib/synapse-kb && chown -R synapse:synapse /var/lib/synapse-kb

USER synapse

ENV SYNAPSE_KB_ROOT=/var/lib/synapse-kb/kb \
    SYNAPSE_INDEX_DB=/var/lib/synapse-kb/index.db \
    SYNAPSE_HTTP_BIND=0.0.0.0:19181 \
    RUST_LOG=info

EXPOSE 19181

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:19181/api/entries || exit 1

ENTRYPOINT ["/usr/local/bin/synapse-kb-server"]
