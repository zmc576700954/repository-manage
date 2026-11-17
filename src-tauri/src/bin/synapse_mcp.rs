//! `synapse-mcp` — Synapse KB 的 stdio MCP server（批次 3，Q14）。
//!
//! 供 Agent 宿主（Claude Code / Claude Desktop / 批次 4 的 EditAgent）以
//! 子进程方式拉起，协议与 dev-kb 只读 MCP 相同：换行分隔 JSON-RPC 2.0。
//!
//! ```bash
//! cargo build --bin synapse-mcp
//! synapse-mcp --kb ~/kaiwu/kb/kaiwu
//! ```
//!
//! `--kb` 缺省时使用注册表的活动 KB；注册表只读不写。工具面：read 五个
//! 全开；结构写三个强制 dryRun；语义写 `propose_change` 落 `_proposals/`。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use synapse_kb_lib::mcp::{McpOptions, McpServer};

#[derive(Parser)]
#[command(
    name = "synapse-mcp",
    version,
    about = "Synapse KB stdio MCP server（read 全开 / 结构写 dryRun / 语义写提案）"
)]
struct Cli {
    /// KB 根目录（缺省 = 注册表的活动 KB；注册表只读不写）
    #[arg(long)]
    kb: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[mcp] tokio runtime failed: {e}");
            return ExitCode::from(2);
        }
    };

    runtime.block_on(async move {
        let server = match McpServer::boot(&McpOptions {
            kb_path: cli.kb,
            runtime_dir_base: None,
        }) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[mcp] boot failed: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = server.startup_rebuild().await {
            eprintln!("[mcp] startup rebuild failed: {e}");
            return ExitCode::from(2);
        }
        match server.serve().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("[mcp] serve failed: {e}");
                ExitCode::from(2)
            }
        }
    })
}
