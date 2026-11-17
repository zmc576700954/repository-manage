//! stdio MCP server（kaiwu × rm 合并批次 3，Q14 / Q11 / D7）。
//!
//! 把 rm 的写路径暴露给外部 Agent 进程：换行分隔 JSON-RPC 2.0 over stdio，
//! 与 dev-kb 的只读 MCP（`agent_app/mcp_server.py`）同协议，方法集
//! initialize / ping / tools/list / tools/call（+ exit 断连）。单进程顺序
//! 处理：一行请求 → 一行响应；协议之外的一切日志走 stderr。
//!
//! 三层工具面（Q14）：
//! - **read 全开**：`list_entries` / `read_entry` / `get_graph` / `validate`
//!   / `get_history`
//! - **结构写自动**（强制 `dryRun` 参数，false 才落盘）：`save_content`
//!   （整文件，origin=agent）/ `patch_metadata` / `apply_fix`
//! - **语义写走提案**：`propose_change` → KB 内 `_proposals/`（`_` 前缀 =
//!   机制目录，双侧校验器豁免规范但仍索引展示）
//!
//! 行为宪章（D7）：Agent 编辑**不碰** `最后更新` / `下次 review`——
//! `save_content` 逐值比对这些键，任何改动直接拒绝（`AgentFrozenField`）；
//! `patch_metadata` 的参数结构里根本没有这两个键。出处由版本历史的
//! `origin=agent` 记录（`SaveOrigin::Agent`）。
//!
//! 进程模型：本 server 与 GUI 是两个进程，各自持有**私有** SQLite 索引
//! （`indexes/<kb_id>.agent.db`，索引是可重建缓存，KB 目录才是真源）；
//! 写路径复用 commands 的 `*_impl` 纯函数层，进程内以 `write_lock` 串行、
//! 文件级原子替换落盘。GUI 侧 watcher 会把本进程写入视作 external 变更，
//! 但 agent 版本记录（毫秒级）先于 watcher 防抖（500ms）落盘，且
//! `record_external_change` 按内容哈希去重——不会出现双版本。
//!
//! 启动：`synapse-mcp --kb <path>`（缺省用注册表的活动 KB）。注册表
//! **只读不写**：`--kb` 不在注册表时按规范化路径派生稳定 id
//! （`agent-<fnv1a64>`），历史与索引仍落 app data 运行时目录。

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::commands::SaveEntryResult;
use crate::error::{AppError, AppResult};
use crate::history::VersionMeta;
use crate::index::{Index, StoredEntry};
use crate::kb_registry::{KbRegistry, KBInfo};
use crate::parser::metadata_updater::MetadataChanges;
use crate::state::AppState;

/// 与 dev-kb MCP server 保持一致的协议版本协商集合。
const PROTOCOL_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "synapse-kb";
const SERVER_VERSION: &str = "0.1.0";

/// D7：Agent 不许代 bump 的人工复审元数据键。批次 4 起 pub：
/// GUI `apply_proposal` 应用 Agent 起草的全文时用同一张冻结清单。
pub const AGENT_FROZEN_KEYS: [&str; 2] = ["最后更新", "下次 review"];

/// 提案正文的定界标记（批次 4 的 GUI 审阅 / EditAgent 应用按此切分）。
pub const PROPOSAL_CONTENT_START: &str = "<!-- synapse:proposal-content-start -->";
pub const PROPOSAL_CONTENT_END: &str = "<!-- synapse:proposal-content-end -->";

// ---------------------------------------------------------------------------
// 工具面定义（tools/list 负载）
// ---------------------------------------------------------------------------

/// 三层工具面。schema 与 dev-kb 同风格：object + additionalProperties=false。
fn tool_definitions() -> Value {
    json!([
    {
        "name": "list_entries",
        "description": "列出当前 KB 全部条目（id/标题/分组/状态/标签/关系/文档代号）。索引在启动时已重建。",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
    },
    {
        "name": "read_entry",
        "description": "按条目 id 读取元数据与完整 Markdown 正文。",
        "inputSchema": {
            "type": "object",
            "properties": {"id": {"type": "string", "minLength": 1}},
            "required": ["id"],
            "additionalProperties": false,
        },
    },
    {
        "name": "get_graph",
        "description": "返回图谱视图：节点（条目元数据）与边（typed relation + 链接解析出的 reference）。",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
    },
    {
        "name": "validate",
        "description": "对当前 KB 跑全部规范规则（与 `synapse validate --json` 同一实现），返回 findings 与计数。",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
    },
    {
        "name": "get_history",
        "description": "读取某条目的版本历史（含 origin=editor/autosave/format/restore/external/trim/agent 标记）与阈值警告。",
        "inputSchema": {
            "type": "object",
            "properties": {"entryId": {"type": "string", "minLength": 1}},
            "required": ["entryId"],
            "additionalProperties": false,
        },
    },
    {
        "name": "save_content",
        "description": "结构性写：整文件替换某条目的 Markdown 正文（frontmatter + 正文）。dryRun=true 只返回预览；dryRun=false 落盘并记录 origin=agent 版本。行为宪章 D7：不得改动 `最后更新` / `下次 review` 的值，违者拒绝。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "minLength": 1},
                "content": {"type": "string"},
                "dryRun": {"type": "boolean"},
            },
            "required": ["id", "content", "dryRun"],
            "additionalProperties": false,
        },
    },
    {
        "name": "patch_metadata",
        "description": "结构性写：只改 frontmatter 已建模键（title/tags/labels/status；status 接受 canonical 中文枚举或 rm 旧英文值，kaiwu 文件自动路由到 `状态` 键）。dryRun=true 返回改写后全文预览；dryRun=false 落盘并记录 origin=agent 版本。参数结构不含 `最后更新` / `下次 review`（D7）。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "minLength": 1},
                "changes": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "labels": {"type": "array", "items": {"type": "string"}},
                        "status": {"type": "string"},
                    },
                    "additionalProperties": false,
                },
                "dryRun": {"type": "boolean"},
            },
            "required": ["id", "changes", "dryRun"],
            "additionalProperties": false,
        },
    },
    {
        "name": "apply_fix",
        "description": "结构性写：按规则 id（E1–E9）对某条目应用校验修复器。dryRun=true 返回修复器描述（descriptor 的 *_key 是 GUI i18n 键）；dryRun=false 真正执行并记录 origin=agent 版本。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "minLength": 1},
                "ruleId": {"type": "string", "minLength": 1},
                "dryRun": {"type": "boolean"},
            },
            "required": ["id", "ruleId", "dryRun"],
            "additionalProperties": false,
        },
    },
    {
        "name": "propose_change",
        "description": "语义写：把内容改动作为提案写入 KB 的 `_proposals/` 机制目录（不直接改目标条目，等人审）。rationale 必填说明动机；content 可选，为提案的目标全文（用 synapse:proposal-content 标记包裹）。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "target": {"type": "string", "minLength": 1},
                "rationale": {"type": "string", "minLength": 1},
                "content": {"type": "string"},
            },
            "required": ["target", "rationale"],
            "additionalProperties": false,
        },
    },
    {
        "name": "review_proposal",
        "description": "语义写（批次 4）：ReviewerAgent 把审阅结论写入 `_proposals/<id>.review.md` 并把提案 status 翻为 reviewed。verdict 只接受 pass / needs-attention；notes 是 D8 三条（数字可追溯/被推翻表述显式化/依赖矩阵一致性）的逐条结论。",
        "inputSchema": {
            "type": "object",
            "properties": {
                "proposalId": {"type": "string", "minLength": 1},
                "verdict": {"type": "string", "enum": ["pass", "needs-attention"]},
                "notes": {"type": "string", "minLength": 1},
                "dryRun": {"type": "boolean"},
            },
            "required": ["proposalId", "verdict", "notes", "dryRun"],
            "additionalProperties": false,
        },
    },
    ])
}

// ---------------------------------------------------------------------------
// 服务器
// ---------------------------------------------------------------------------

/// `synapse-mcp` 的启动参数。
#[derive(Debug, Clone, Default)]
pub struct McpOptions {
    /// KB 根目录。None = 用注册表的活动 KB。
    pub kb_path: Option<PathBuf>,
    /// 运行时目录基座（索引/历史所在层）。None = 生产值 `paths::indexes_dir()`；
    /// 测试注入 tempdir，避免污染真实 app data。
    pub runtime_dir_base: Option<PathBuf>,
}

pub struct McpServer {
    state: AppState,
    kb_root: PathBuf,
    runtime_dir: PathBuf,
    kb_id: String,
    kb_name: String,
}

/// 解析目标 KB：`--kb` 精确/规范化匹配注册表条目；未注册则派生稳定 id。
/// 注册表只读——MCP server 不增删注册项、不改活动 KB。
fn resolve_kb(options: &McpOptions) -> AppResult<KBInfo> {
    let registry = if crate::paths::kbs_json_path().exists() {
        KbRegistry::load_or_default()
    } else {
        KbRegistry::empty_at(crate::paths::kbs_json_path())
    };

    if let Some(want) = &options.kb_path {
        let want_canon = want.canonicalize().unwrap_or_else(|_| want.clone());
        for kb in registry.list() {
            let kb_path = PathBuf::from(&kb.path);
            if &kb_path == want || kb_path.canonicalize().unwrap_or(kb_path.clone()) == want_canon
            {
                return Ok(kb);
            }
        }
        // 未注册：派生稳定 id（同一路径跨进程/重启一致），不写注册表。
        let canon = want_canon.to_string_lossy().to_string();
        let id = format!("agent-{:016x}", crate::fingerprint::fnv1a64(canon.as_bytes()));
        let name = want
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| canon.clone());
        return Ok(KBInfo {
            id,
            name,
            path: want.to_string_lossy().to_string(),
        });
    }

    registry.active().cloned().ok_or(AppError::NoActiveKb)
}

impl McpServer {
    /// 打开私有索引、构造 AppState、启动重建（迁移旧运行时产物 → rebuild →
    /// 校验缓存）。与 GUI `run_startup_rebuild` 同序，但索引文件独立。
    pub fn boot(options: &McpOptions) -> AppResult<Self> {
        let kb = resolve_kb(options)?;
        let kb_root = PathBuf::from(&kb.path);
        if !kb_root.exists() {
            return Err(AppError::KbDirGone {
                path: kb.path.clone(),
            });
        }
        if !kb_root.is_dir() {
            return Err(AppError::KbDirNotADirectory {
                path: kb.path.clone(),
            });
        }

        let runtime_dir_base = options
            .runtime_dir_base
            .clone()
            .unwrap_or_else(crate::paths::indexes_dir);
        let runtime_dir = runtime_dir_base.join(&kb.id);
        let db_path = runtime_dir_base.join(format!("{}.agent.db", kb.id));
        let index = Index::open(&db_path).map_err(|e| AppError::DatabaseOp {
            message: format!("mcp: open agent index {}: {e}", db_path.display()),
        })?;

        let state = AppState {
            index: std::sync::Arc::new(tokio::sync::RwLock::new(index)),
            kbs: std::sync::Arc::new(tokio::sync::RwLock::new(KbRegistry::empty_at(
                // 只读占位：MCP server 永不 save 注册表。
                std::env::temp_dir().join("synapse-mcp-registry-readonly.json"),
            ))),
            current_kb: std::sync::Arc::new(tokio::sync::RwLock::new(Some(kb.clone()))),
            runtime_dir_base: runtime_dir_base.clone(),
            validation_report: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            write_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            watcher: std::sync::Arc::new(std::sync::Mutex::new(None)),
            watch_tx: std::sync::Arc::new(std::sync::Mutex::new(None)),
            startup_gate: std::sync::Arc::new(tokio::sync::RwLock::new(())),
            local_write_paths: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            pending_renames: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        };

        Ok(Self {
            state,
            kb_root,
            runtime_dir,
            kb_id: kb.id,
            kb_name: kb.name,
        })
    }

    /// 启动闸门内完成首轮 rebuild（含旧运行时产物迁移与全量校验缓存）。
    pub async fn startup_rebuild(&self) -> AppResult<usize> {
        let _gate = self.state.startup_gate.write().await;
        let _guard = self.state.write_lock.lock().await;
        if let Err(e) =
            crate::state::migrate_legacy_runtime_state(&self.kb_root, &self.runtime_dir)
        {
            eprintln!("[mcp] legacy runtime state migration failed: {e}");
        }
        let idx = self.state.index.read().await;
        let n = idx.rebuild(&self.kb_root).map_err(|e| AppError::DatabaseOp {
            message: format!("mcp: startup rebuild failed: {e}"),
        })?;
        let report = crate::validator::validate_kb(&self.kb_root);
        *self.state.validation_report.write().await = Some(report);
        Ok(n)
    }

    /// 主循环：stdin 每行一个 JSON-RPC 请求，stdout 回一行响应。
    /// `exit` / EOF 结束。单次工具异常不影响进程存活（契约 ADR-0021 同款）。
    pub async fn serve(&self) -> AppResult<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        eprintln!(
            "[mcp] serving kb {} ({}) at {}",
            self.kb_name,
            self.kb_id,
            self.kb_root.display()
        );
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[mcp] stdin read error: {e}");
                    break;
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    let resp = json!({
                        "jsonrpc": "2.0", "id": null,
                        "error": {"code": -32700, "message": format!("Parse error: {e}")},
                    });
                    let _ = writeln!(stdout, "{resp}");
                    let _ = stdout.flush();
                    continue;
                }
            };
            if request.get("method").and_then(|m| m.as_str()) == Some("exit") {
                break;
            }
            if let Some(response) = self.handle(&request).await {
                let _ = writeln!(stdout, "{response}");
                let _ = stdout.flush();
            }
        }
        Ok(())
    }

    /// 单个 JSON-RPC 请求 → 响应。通知（无 id）不回应。
    async fn handle(&self, request: &Value) -> Option<Value> {
        let id = request.get("id")?.clone();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            "initialize" => {
                let requested = params.get("protocolVersion").and_then(|v| v.as_str());
                let version = requested
                    .filter(|v| PROTOCOL_VERSIONS.contains(&v))
                    .unwrap_or(DEFAULT_PROTOCOL_VERSION);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                    "instructions": "Synapse KB 知识库读写服务。read 工具全开；结构写工具强制 dryRun 预览；语义改动用 propose_change 出提案，不要绕过提案直接改正文结论。",
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.call_tool(name, &args).await {
                    Ok(value) => Ok(json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string(&value)
                                .unwrap_or_else(|e| format!("{{\"error\":\"serialize: {e}\"}}")),
                        }],
                        "structuredContent": value,
                        "isError": false,
                    })),
                    Err(message) => Ok(json!({
                        "content": [{"type": "text", "text": message}],
                        "isError": true,
                    })),
                }
            }
            "shutdown" => Ok(Value::Null),
            other => Err(json!({
                "code": -32601,
                "message": format!("Method not found: {other}"),
            })),
        };

        Some(match result {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        })
    }

    // -- 工具分发 -----------------------------------------------------------

    async fn call_tool(&self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "list_entries" => self.tool_list_entries().await,
            "read_entry" => self.tool_read_entry(args).await,
            "get_graph" => self.tool_get_graph().await,
            "validate" => self.tool_validate(),
            "get_history" => self.tool_get_history(args).await,
            "save_content" => self.tool_save_content(args).await,
            "patch_metadata" => self.tool_patch_metadata(args).await,
            "apply_fix" => self.tool_apply_fix(args).await,
            "propose_change" => self.tool_propose_change(args),
            "review_proposal" => self.tool_review_proposal(args),
            other => Err(format!("未知工具: {other}")),
        }
    }

    fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("参数错误: 缺少必填字符串参数 {key}"))
    }

    /// 结构写三件套共用的强制 dryRun 校验。
    fn require_dry_run(args: &Value) -> Result<bool, String> {
        args.get("dryRun")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| "参数错误: 结构写工具必须显式传 dryRun (boolean)".to_string())
    }

    async fn entries(&self) -> Result<Vec<StoredEntry>, String> {
        let idx = self.state.index.read().await;
        idx.get_all_entries().map_err(|e| format!("database: {e}"))
    }

    async fn content_path(&self, id: &str) -> Result<PathBuf, String> {
        let idx = self.state.index.read().await;
        idx.get_entry_content_path(id)
            .map_err(|e| format!("database: {e}"))?
            .ok_or_else(|| format!("entry-not-found: {id}"))
    }

    // -- read 层 ------------------------------------------------------------

    async fn tool_list_entries(&self) -> Result<Value, String> {
        let entries = self.entries().await?;
        let count = entries.len();
        Ok(serde_json::to_value(entries)
            .map(|entries| json!({"count": count, "entries": entries}))
            .map_err(|e| format!("serialize: {e}"))?)
    }

    async fn tool_read_entry(&self, args: &Value) -> Result<Value, String> {
        let id = Self::require_str(args, "id")?;
        let entries = self.entries().await?;
        let entry = entries
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("entry-not-found: {id}"))?;
        let path = self.content_path(id).await?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("io: read {}: {e}", path.display()))?;
        Ok(json!({"entry": entry, "content": content}))
    }

    async fn tool_get_graph(&self) -> Result<Value, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct GraphNode {
            id: String,
            title: String,
            group: Option<String>,
            dir: Option<String>,
            status: Option<String>,
            tags: Vec<String>,
        }
        #[derive(Serialize)]
        struct GraphEdge {
            from: String,
            to: String,
            #[serde(rename = "type")]
            relation_type: String,
        }
        let entries = self.entries().await?;
        let nodes: Vec<GraphNode> = entries
            .iter()
            .map(|e| GraphNode {
                id: e.id.clone(),
                title: e.title.clone(),
                group: e.group.clone(),
                dir: e.dir.clone(),
                status: e.status.clone(),
                tags: e.tags.clone(),
            })
            .collect();
        let mut edges: Vec<GraphEdge> = Vec::new();
        for e in &entries {
            for r in &e.relations {
                edges.push(GraphEdge {
                    from: r.from_id.clone(),
                    to: r.to_id.clone(),
                    relation_type: r.relation_type.clone(),
                });
            }
        }
        let node_count = nodes.len();
        let edge_count = edges.len();
        Ok(json!({
            "nodes": nodes,
            "edges": edges,
            "counts": {"nodes": node_count, "edges": edge_count},
        }))
    }

    fn tool_validate(&self) -> Result<Value, String> {
        let report = crate::validator::validate_kb(&self.kb_root);
        let summary = json!({
            "errors": report.error_count(),
            "warnings": report.warning_count(),
            "info": report.info_count(),
        });
        serde_json::to_value(&report)
            .map(|mut v| {
                v["summary"] = summary;
                v
            })
            .map_err(|e| format!("serialize: {e}"))
    }

    async fn tool_get_history(&self, args: &Value) -> Result<Value, String> {
        let id = Self::require_str(args, "entryId")?;
        // 条目必须存在，避免把任意字符串当 key 读历史目录。
        let _ = self.content_path(id).await?;
        let versions: Vec<VersionMeta> =
            crate::history::list_versions(&self.runtime_dir, id)
                .map_err(|e| e.to_string())?;
        let warnings = crate::history::warnings(&self.runtime_dir, id)
            .map_err(|e| e.to_string())?;
        Ok(json!({"entryId": id, "versions": versions, "warnings": warnings}))
    }

    // -- 结构写层（强制 dryRun） --------------------------------------------

    async fn tool_save_content(&self, args: &Value) -> Result<Value, String> {
        let id = Self::require_str(args, "id")?.to_string();
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "参数错误: 缺少字符串参数 content".to_string())?;
        let dry_run = Self::require_dry_run(args)?;

        let path = self.content_path(&id).await?;
        let previous = std::fs::read_to_string(&path)
            .map_err(|e| format!("io: read {}: {e}", path.display()))?;

        // D7：人工复审元数据是 Agent 禁区。逐值比对，任何改动（含新增/删除
        // 这两个键）一律拒绝——日期由人复核后自己 bump。
        let frozen = frozen_field_violations(&previous, content);
        if !frozen.is_empty() {
            return Err(AppError::AgentFrozenField {
                keys: frozen.join("、"),
            }
            .to_string());
        }

        let hints = save_hints(content);
        if dry_run {
            let summary = crate::history::diff_summary(&previous, content);
            let changed = previous != content;
            return Ok(json!({
                "dryRun": true,
                "changed": changed,
                "addedLines": summary.added_lines,
                "removedLines": summary.removed_lines,
                "next": content,
                "hints": hints,
            }));
        }

        let result: SaveEntryResult = crate::commands::write_entry_content(
            &self.state,
            &id,
            content,
            crate::history::SaveOrigin::Agent,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
        let version = latest_agent_version(&self.runtime_dir, &id);
        Ok(json!({
            "dryRun": false,
            "changed": result.changed,
            "bytes": result.bytes,
            "savedAtMs": result.saved_at_ms,
            "version": version,
            "hints": hints,
        }))
    }

    async fn tool_patch_metadata(&self, args: &Value) -> Result<Value, String> {
        let id = Self::require_str(args, "id")?.to_string();
        let changes: MetadataChanges = serde_json::from_value(
            args.get("changes").cloned().unwrap_or(json!({})),
        )
        .map_err(|e| format!("参数错误: changes: {e}"))?;
        let dry_run = Self::require_dry_run(args)?;

        let path = self.content_path(&id).await?;
        if dry_run {
            // 与落盘共用同一渲染函数，保证预览字节级一致（含 D3 状态键路由）。
            let original = std::fs::read_to_string(&path)
                .map_err(|e| format!("io: read {}: {e}", path.display()))?;
            let next = crate::parser::metadata_updater::preview_metadata_changes(
                &path, &changes,
            )
            .map_err(|e| e.to_string())?;
            let changed = original != next;
            return Ok(json!({
                "dryRun": true,
                "changed": changed,
                "original": original,
                "next": next,
            }));
        }

        // 记录 agent 版本（GUI 元数据编辑不记版本；Agent 写必须留出处）。
        let previous = std::fs::read_to_string(&path)
            .map_err(|e| format!("io: read {}: {e}", path.display()))?;
        let entry = crate::commands::update_entry_metadata_impl(
            &self.state,
            id.clone(),
            changes,
        )
        .await
        .map_err(|e| e.to_string())?;
        let next = std::fs::read_to_string(&path)
            .map_err(|e| format!("io: read {}: {e}", path.display()))?;
        let version = crate::history::record_version(
            &self.runtime_dir,
            &id,
            &previous,
            &next,
            crate::history::SaveOrigin::Agent,
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(json!({"dryRun": false, "entry": entry, "version": version}))
    }

    async fn tool_apply_fix(&self, args: &Value) -> Result<Value, String> {
        let id = Self::require_str(args, "id")?.to_string();
        let rule_id = Self::require_str(args, "ruleId")?.to_string();
        let dry_run = Self::require_dry_run(args)?;

        if dry_run {
            // 预览 = fix 体系的 confirm=false 分支（不动盘）。
            let result = crate::commands::apply_fix_impl(
                &self.state,
                id,
                rule_id,
                false,
            )
            .await
            .map_err(|e| e.to_string())?;
            return Ok(json!({"dryRun": true, "descriptor": result.descriptor}));
        }

        let path = self.content_path(&id).await?;
        let previous = std::fs::read_to_string(&path)
            .map_err(|e| format!("io: read {}: {e}", path.display()))?;
        let result = crate::commands::apply_fix_impl(&self.state, id.clone(), rule_id, true)
            .await
            .map_err(|e| e.to_string())?;
        let next = std::fs::read_to_string(&path)
            .map_err(|e| format!("io: read {}: {e}", path.display()))?;
        let version = if next != previous {
            crate::history::record_version(
                &self.runtime_dir,
                &id,
                &previous,
                &next,
                crate::history::SaveOrigin::Agent,
                None,
            )
            .map_err(|e| e.to_string())?
        } else {
            None
        };
        Ok(json!({"dryRun": false, "result": result, "version": version}))
    }

    // -- 语义写层（提案） ---------------------------------------------------

    fn tool_propose_change(&self, args: &Value) -> Result<Value, String> {
        let target = Self::require_str(args, "target")?;
        let rationale = Self::require_str(args, "rationale")?;
        let content = args.get("content").and_then(|v| v.as_str());

        let proposals_dir = self.kb_root.join("_proposals");
        std::fs::create_dir_all(&proposals_dir)
            .map_err(|e| format!("io: create {}: {e}", proposals_dir.display()))?;

        let proposal_id = format!("{}-{}", utc_timestamp_compact(), sanitize_proposal_stem(target));
        let mut path = proposals_dir.join(format!("{proposal_id}.md"));
        let mut final_id = proposal_id.clone();
        let mut seq = 2;
        while path.exists() {
            final_id = format!("{proposal_id}-{seq}");
            path = proposals_dir.join(format!("{final_id}.md"));
            seq += 1;
        }

        let mut file = String::new();
        file.push_str("---\n");
        file.push_str(&format!("proposal: {final_id}\n"));
        file.push_str(&format!("target: {}\n", single_line(target)));
        file.push_str("status: pending\n");
        file.push_str(&format!("created: {}\n", utc_iso()));
        file.push_str("origin: agent\n");
        file.push_str(&format!(
            "rationale: {}\n",
            serde_yaml::to_string(rationale)
                .map_err(|e| format!("serialize rationale: {e}"))?
                .trim_end()
        ));
        file.push_str("---\n\n");
        if let Some(body) = content {
            file.push_str(PROPOSAL_CONTENT_START);
            file.push('\n');
            file.push_str(body);
            if !body.ends_with('\n') {
                file.push('\n');
            }
            file.push_str(PROPOSAL_CONTENT_END);
            file.push('\n');
        } else {
            file.push_str("（无内容变更——本提案仅说明意图，等后续补充。）\n");
        }

        std::fs::write(&path, file)
            .map_err(|e| format!("io: write {}: {e}", path.display()))?;
        eprintln!("[mcp] proposal written: {}", path.display());
        Ok(json!({
            "proposalId": final_id,
            "path": path.to_string_lossy(),
            "status": "pending",
        }))
    }

    /// ReviewerAgent 的审阅落点（批次 4）：写 `<id>.review.md` + 翻提案
    /// status。审阅也走 MCP——「Agent 不直接碰盘」对审阅同样成立。
    fn tool_review_proposal(&self, args: &Value) -> Result<Value, String> {
        let proposal_id = Self::require_str(args, "proposalId")?.to_string();
        let verdict = Self::require_str(args, "verdict")?.to_string();
        if verdict != "pass" && verdict != "needs-attention" {
            return Err(format!(
                "参数错误: verdict 只接受 pass / needs-attention，得到 {verdict}"
            ));
        }
        let notes = Self::require_str(args, "notes")?.to_string();
        let dry_run = Self::require_dry_run(args)?;

        // 先读提案：不存在即报错；顺带给预览附上当前 status。
        let proposal = crate::proposals::read_proposal(&self.kb_root, &proposal_id)
            .map_err(|e| e.to_string())?;
        if dry_run {
            return Ok(json!({
                "dryRun": true,
                "proposalId": proposal_id,
                "currentStatus": proposal.meta.status,
                "reviewPath": format!("{}/{}.review.md", crate::proposals::PROPOSALS_DIR, proposal_id),
                "verdict": verdict,
            }));
        }
        let review_path = crate::proposals::write_review(
            &self.kb_root,
            &proposal_id,
            &verdict,
            &notes,
        )
        .map_err(|e| e.to_string())?;
        eprintln!("[mcp] review written: {}", review_path.display());
        Ok(json!({
            "dryRun": false,
            "proposalId": proposal_id,
            "reviewPath": review_path.to_string_lossy(),
            "status": crate::proposals::STATUS_REVIEWED,
        }))
    }
}

// ---------------------------------------------------------------------------
// D7 冻结键 / 提示 / 提案工具函数
// ---------------------------------------------------------------------------

/// frontmatter 区块内某顶层键的单行标量值（`key: value` 的 value 部分）。
/// `最后更新` / `下次 review` 都是单行日期，行级提取足够精确。
fn frontmatter_scalar(content: &str, key: &str) -> Option<String> {
    // 委托 fm_writer 的共享实现（批次 4 起 GUI apply_proposal 同口径）。
    crate::parser::fm_writer::scalar_value(content, key)
}

/// D7 闸门：`previous → next` 之间冻结键的值发生变化（含新增/删除）时，
/// 返回被改动的键名列表。批次 4 起 pub：GUI apply_proposal 共用。
pub fn frozen_field_violations(previous: &str, next: &str) -> Vec<String> {
    AGENT_FROZEN_KEYS
        .iter()
        .filter(|k| frontmatter_scalar(previous, k) != frontmatter_scalar(next, k))
        .map(|k| (*k).to_string())
        .collect()
}

/// M5 补键建议（batch-2 遗留 #1）：frontmatter 缺 `文档代号` 时，
/// 给出路径派生的建议值（35/35 可机械派生；派生不出则只报缺失）。
fn save_hints(content: &str) -> Value {
    if frontmatter_scalar(content, "文档代号").is_some() {
        return json!({});
    }
    // 建议值需要条目的 dir/stem——这里没有 RawEntry，从正文首行 `# ` 与
    // 路径信息拿不到稳定结果；只报缺失，建议值由 Agent 结合 list_entries
    // 的 dir 字段自行推导（同为 NN-MM 机械规则）。
    json!({"docCodeMissing": true})
}

fn latest_agent_version(runtime_dir: &Path, entry_id: &str) -> Option<VersionMeta> {
    let versions = crate::history::list_versions(runtime_dir, entry_id).ok()?;
    versions
        .into_iter()
        .rev()
        .find(|v| v.origin == crate::history::SaveOrigin::Agent)
}

/// 条目 id → 提案文件名的安全段：非法字符折叠为 `_`，剥掉尾部 `.md` 再
/// 截断并去掉尾部的点/连字符，防止路径逃逸、隐藏文件与 `.md.md` 双扩展名。
fn sanitize_proposal_stem(target: &str) -> String {
    let stripped = target.strip_suffix(".md").unwrap_or(target);
    let mut safe: String = stripped
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let len = safe.char_indices().nth(60).map(|(i, _)| i).unwrap_or(safe.len());
    safe.truncate(len);
    let trimmed = safe.trim_matches(|c| c == '.' || c == '-');
    if trimmed.is_empty() {
        "target".to_string()
    } else {
        trimmed.to_string()
    }
}

/// rationale 是 frontmatter 标量：压平换行，避免多行值破坏 M1 扁平形态
/// （提案文件本身豁免校验，但保持单行对工具链更友好）。
fn single_line(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(120).collect();
    if out.is_empty() {
        out.push_str("(empty)");
    }
    out
}

/// UTC 紧凑时间戳：`YYYYMMDD-HHMMSS`（提案 id 前缀，字典序即时间序）。
fn utc_timestamp_compact() -> String {
    let (y, m, d, h, mi, s) = utc_parts();
    format!("{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// UTC RFC3339：`YYYY-MM-DDTHH:MM:SSZ`。
fn utc_iso() -> String {
    let (y, m, d, h, mi, s) = utc_parts();
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// epoch 秒 → UTC 六元组（Howard Hinnant civil-from-days 算法，无外部依赖）。
fn utc_parts() -> (i64, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

// ---------------------------------------------------------------------------
// 临时验收测试（纪律：跑绿后删除，生产代码保留）
// ---------------------------------------------------------------------------
