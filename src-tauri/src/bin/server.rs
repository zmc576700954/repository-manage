//! 独立 Web 服务器二进制：托管前端 dist/ + 暴露 axum API。
//!
//! 设计目标：单二进制即可部署，无需 Tauri 桌面运行时。
//! 默认监听 127.0.0.1:19181（应由 Nginx 反代访问，不直接暴露公网）。
//!
//! 用法：
//!   SYNAPSE_KB_ROOT=/path/to/kb \
//!   SYNAPSE_INDEX_DB=/path/to/index.db \
//!   SYNAPSE_HTTP_BIND=0.0.0.0:19181 \
//!   SYNAPSE_STATIC_DIR=../dist \
//!   ./server

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::response::{IntoResponse, Response};
use synapse_kb_lib::index::Index;
use synapse_kb_lib::local_server::{router as api_router, AppState as ServerState};
use tokio::sync::RwLock;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cfg = Config::from_env()?;
    tracing::info!(
        kb_root = %cfg.kb_root.display(),
        index_db = %cfg.index_db.display(),
        http_bind = %cfg.http_bind,
        static_dir = %cfg
            .static_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into()),
        "starting synapse-kb server"
    );

    // 打开 SQLite 索引
    let index = Index::open(&cfg.index_db)
        .with_context(|| format!("failed to open index at {}", cfg.index_db.display()))?;

    // 首次启动时自动 rebuild 索引（如果 KB root 存在）
    if cfg.kb_root.exists() {
        let count = index
            .rebuild(&cfg.kb_root)
            .with_context(|| format!("failed to rebuild index from {}", cfg.kb_root.display()))?;
        tracing::info!(count, "initial index rebuild complete");
    } else {
        tracing::warn!(
            path = %cfg.kb_root.display(),
            "kb root does not exist; index will be empty until path is configured"
        );
    }

    let state = ServerState {
        index: Arc::new(RwLock::new(index)),
    };

    // 构建路由：API + 静态文件 fallback
    let static_dir = cfg.static_dir.clone();
    let app = api_router(state).fallback(move |req: axum::extract::Request| {
        let static_dir = static_dir.clone();
        async move { serve_static(req, static_dir).await }
    });

    // TraceLayer 包外层以观测所有请求
    let app = app.layer(TraceLayer::new_for_http());

    // 监听
    let listener = tokio::net::TcpListener::bind(&cfg.http_bind)
        .await
        .with_context(|| format!("failed to bind {}", cfg.http_bind))?;
    let local_addr = listener
        .local_addr()
        .with_context(|| "failed to read local addr")?;
    tracing::info!(addr = %local_addr, "server listening");

    axum::serve(listener, app)
        .await
        .context("axum server exited with error")?;

    Ok(())
}

async fn serve_static(req: axum::extract::Request, static_dir: Option<PathBuf>) -> Response {
    let Some(dir) = static_dir else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Static files not configured. Set SYNAPSE_STATIC_DIR to the built frontend dist/ directory.\n",
        )
            .into_response();
    };

    if !dir.is_dir() {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "SYNAPSE_STATIC_DIR={} is not a directory\n",
                dir.display()
            ),
        )
            .into_response();
    }

    // ServeDir + not_found_service 让 SPA 路由 fallback 到 index.html
    let index_path = dir.join("index.html");
    let serve = ServeDir::new(&dir).not_found_service(ServeFile::new(index_path));

    match serve.oneshot(req).await {
        Ok(resp) => {
            // ServeDir 返回的 body 是自定义类型，转为 axum Body
            let (parts, body) = resp.into_parts();
            Response::from_parts(parts, axum::body::Body::new(body))
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("static file error: {e}"),
        )
            .into_response(),
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,synapse_kb_lib=info")),
        )
        .with_target(false)
        .compact()
        .init();
}

#[derive(Debug)]
struct Config {
    kb_root: PathBuf,
    index_db: PathBuf,
    http_bind: SocketAddr,
    static_dir: Option<PathBuf>,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let kb_root = std::env::var("SYNAPSE_KB_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| synapse_kb_lib::paths::kb_root());

        let index_db = std::env::var("SYNAPSE_INDEX_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| synapse_kb_lib::paths::index_db_path());

        let http_bind: SocketAddr = std::env::var("SYNAPSE_HTTP_BIND")
            .unwrap_or_else(|_| "127.0.0.1:19181".to_string())
            .parse()
            .context("SYNAPSE_HTTP_BIND must be host:port")?;

        let static_dir = std::env::var("SYNAPSE_STATIC_DIR").ok().map(PathBuf::from);

        Ok(Self {
            kb_root,
            index_db,
            http_bind,
            static_dir,
        })
    }
}
