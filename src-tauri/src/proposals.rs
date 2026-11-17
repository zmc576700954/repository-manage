//! 语义提案机制目录（批次 3 定格式，批次 4 落 GUI 审阅闭环）。
//!
//! `_proposals/` 是 `_` 前缀机制目录（Q14）：双侧校验器豁免规范但仍索引
//! 展示。提案文件由 MCP `propose_change` 写入，格式：
//!
//! ```text
//! ---
//! proposal: <id>
//! target: <条目 id 或路径>
//! status: pending | reviewed | applied | rejected
//! created: <RFC3339>
//! origin: agent
//! rationale: <动机单行>
//! ---
//!
//! <!-- synapse:proposal-content-start -->
//! (提案的目标全文，与目标条目同构)
//! <!-- synapse:proposal-content-end -->
//! ```
//!
//! 审阅报告 `<id>.review.md` 由 ReviewerAgent 经 MCP `review_proposal`
//! 写入（批次 4），GUI apply/dismiss 只翻转提案 `status`（fm_writer
//! 保序改写，其余字节不动）。本模块 tauri-free：commands 与 mcp 共用。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::parser::fm_writer::{apply_edits, scalar_value, FrontmatterEdit, FmValue};

pub const PROPOSALS_DIR: &str = "_proposals";
pub const CONTENT_START: &str = "<!-- synapse:proposal-content-start -->";
pub const CONTENT_END: &str = "<!-- synapse:proposal-content-end -->";

/// 提案生命周期：pending（Agent 新建）→ reviewed（ReviewerAgent 出报告）
/// → applied（GUI 批准并落盘）| rejected（GUI 驳回）。
pub const STATUS_PENDING: &str = "pending";
pub const STATUS_REVIEWED: &str = "reviewed";
pub const STATUS_APPLIED: &str = "applied";
pub const STATUS_REJECTED: &str = "rejected";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalMeta {
    /// 文件 stem（同提案 id）。
    pub id: String,
    pub target: String,
    pub status: String,
    pub created: Option<String>,
    pub origin: Option<String>,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    #[serde(flatten)]
    pub meta: ProposalMeta,
    /// 文件全文（审阅视图留档用）。
    pub raw: String,
    /// `proposal-content` 标记之间的目标全文；纯说明提案为 None。
    pub content: Option<String>,
    /// ReviewerAgent 的审阅报告（`<id>.review.md` 全文）；未审阅为 None。
    pub review: Option<String>,
}

fn proposals_dir(kb_root: &Path) -> PathBuf {
    kb_root.join(PROPOSALS_DIR)
}

fn proposal_path(kb_root: &Path, id: &str) -> PathBuf {
    proposals_dir(kb_root).join(format!("{id}.md"))
}

/// 提案 id → 文件。id 只允许出现在我们生成的形态里（字母数字 `-_.`），
/// 其余一律拒绝，杜绝从 id 拼路径的逃逸。
fn validate_proposal_id(id: &str) -> AppResult<()> {
    let ok = !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !id.ends_with(".review");
    if ok {
        Ok(())
    } else {
        Err(AppError::Other {
            message: format!("invalid proposal id: {id}"),
        })
    }
}

fn parse_meta(id: &str, raw: &str) -> ProposalMeta {
    ProposalMeta {
        id: id.to_string(),
        target: scalar_value(raw, "target").unwrap_or_default(),
        status: scalar_value(raw, "status").unwrap_or_else(|| STATUS_PENDING.to_string()),
        created: scalar_value(raw, "created"),
        origin: scalar_value(raw, "origin"),
        rationale: scalar_value(raw, "rationale"),
    }
}

/// `proposal-content` 标记之间的目标全文；标记缺失/不闭合返回 None。
pub fn split_content(raw: &str) -> Option<String> {
    let start = raw.find(CONTENT_START)? + CONTENT_START.len();
    let rest = &raw[start..];
    let end = rest.find(CONTENT_END)?;
    let body = &rest[..end];
    let trimmed = body.strip_prefix('\n').unwrap_or(body);
    Some(trimmed.to_string())
}

/// 列举全部提案（跳过审阅报告），新 id（字典序≈时间序）在前。
/// 目录不存在 → 空列表（KB 还没有提案是常态）。
pub fn list_proposals(kb_root: &Path) -> AppResult<Vec<ProposalMeta>> {
    let dir = proposals_dir(kb_root);
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| AppError::io(format!("proposals: read {}", dir.display()), e))?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") || name.ends_with(".review.md") {
                return None;
            }
            Some(name.trim_end_matches(".md").to_string())
        })
        .collect();
    ids.sort();
    ids.reverse();
    for id in ids {
        let raw = std::fs::read_to_string(proposal_path(kb_root, &id)).map_err(|e| {
            AppError::io(
                format!("proposals: read {}", proposal_path(kb_root, &id).display()),
                e,
            )
        })?;
        out.push(parse_meta(&id, &raw));
    }
    Ok(out)
}

/// 读单个提案（含内容块与审阅报告）。
pub fn read_proposal(kb_root: &Path, id: &str) -> AppResult<Proposal> {
    validate_proposal_id(id)?;
    let path = proposal_path(kb_root, id);
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        AppError::io(format!("proposals: read {}", path.display()), e)
    })?;
    let review_path = proposals_dir(kb_root).join(format!("{id}.review.md"));
    let review = std::fs::read_to_string(&review_path).ok();
    Ok(Proposal {
        meta: parse_meta(id, &raw),
        content: split_content(&raw),
        review,
        raw,
    })
}

/// 翻转提案 `status`（fm_writer 保序改写：只动 status 键行，其余字节保留）。
pub fn set_status(kb_root: &Path, id: &str, status: &str) -> AppResult<()> {
    validate_proposal_id(id)?;
    let path = proposal_path(kb_root, id);
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        AppError::io(format!("proposals: read {}", path.display()), e)
    })?;
    let edit = FrontmatterEdit::set("status", FmValue::Str(status.to_string()));
    let next = apply_edits(&raw, &edit);
    if next != raw {
        std::fs::write(&path, next).map_err(|e| {
            AppError::io(format!("proposals: write {}", path.display()), e)
        })?;
    }
    Ok(())
}

/// ReviewerAgent 的审阅落点（经 MCP `review_proposal` 调用）：
/// 写 `<id>.review.md`（覆盖旧报告）并把提案 status 翻成 reviewed。
/// 返回报告文件路径。
pub fn write_review(
    kb_root: &Path,
    id: &str,
    verdict: &str,
    notes: &str,
) -> AppResult<PathBuf> {
    validate_proposal_id(id)?;
    // 先确认提案存在（对不存在的 id 写报告是 BUG）。
    let _ = read_proposal(kb_root, id)?;
    let review_path = proposals_dir(kb_root).join(format!("{id}.review.md"));
    let mut file = String::new();
    file.push_str("---\n");
    file.push_str(&format!("proposal: {id}\n"));
    file.push_str(&format!("verdict: {verdict}\n"));
    file.push_str("reviewer: ReviewerAgent\n");
    file.push_str(&format!("created: {}\n", utc_iso()));
    file.push_str("---\n\n");
    file.push_str(notes);
    if !notes.ends_with('\n') {
        file.push('\n');
    }
    if let Some(parent) = review_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("proposals: mkdir {}", parent.display()), e))?;
    }
    std::fs::write(&review_path, file).map_err(|e| {
        AppError::io(format!("proposals: write {}", review_path.display()), e)
    })?;
    set_status(kb_root, id, STATUS_REVIEWED)?;
    Ok(review_path)
}

/// UTC RFC3339（与 mcp::utc_iso 同算法；此处独立实现避免模块反向依赖）。
fn utc_iso() -> String {
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
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}
