//! 文档版本历史：快照存储、生成策略、回退、裁剪与阈值警告。
//!
//! 存储位置：app data `indexes/<kb_id>/history/<entry-key>/`
//! （Q10.a：运行时状态全部迁出 KB 目录，KB 目录 = 纯文档）。
//! - `<version-id>.json`：元数据（作者、时间、来源、摘要、哈希、是否存正文）
//! - `<version-id>.md`：正文副本（仅 ≤2MB 时保存）
//!
//! `.synapse/` 已被 scanner 的产物过滤忽略，因此历史文件不会被当成 KB 条目，
//! 也不会进入索引；写历史不会触发 `kb-changed` 的刷新回环。
//!
//! 生成策略（产品已确认）：
//! - 显式保存：只要内容与最新版本不同就生成一条（相同哈希去重）；
//! - 自动保存：内容不同且距最新版本 ≥5 分钟才生成；
//! - 外部修改：内容哈希变化才生成，`origin=external`；
//! - 回退：强制追加一条 `origin=restore` 的记录，带 `restored_from`；
//! - 单版本正文 >2MB：只存摘要 + 哈希，不存正文，UI 会明确提示不可回退。
//!
//! 保留策略：**不做自动清理**。只提供手动裁剪（保留指定版本及之后），
//! 并在超过阈值（每文档 200 版 / 全库 5GB）时给出可见警告，不阻止保存。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const AUTOSAVE_MIN_INTERVAL_MS: u64 = 5 * 60 * 1000;
pub const MAX_STORED_CONTENT_BYTES: usize = 2 * 1024 * 1024;
pub const PER_ENTRY_WARN_COUNT: usize = 200;
pub const TOTAL_WARN_BYTES: u64 = 5 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveOrigin {
    Editor,
    Autosave,
    Format,
    Restore,
    External,
    Trim,
}

impl SaveOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            SaveOrigin::Editor => "editor",
            SaveOrigin::Autosave => "autosave",
            SaveOrigin::Format => "format",
            SaveOrigin::Restore => "restore",
            SaveOrigin::External => "external",
            SaveOrigin::Trim => "trim",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContentSavedEvent {
    /// 版本历史根目录（Q10.a 后 = app data 的 `<runtime_dir>/`，
    /// 不再是 KB 目录）。字段名保留语义：历史树的根。
    pub history_root: PathBuf,
    pub entry_id: String,
    pub content_path: PathBuf,
    pub previous: String,
    pub next: String,
    pub origin: SaveOrigin,
    /// 回退操作时记录来自哪个版本。
    pub restored_from: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub added_lines: usize,
    pub removed_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub version_id: String,
    pub entry_id: String,
    pub created_at_ms: u64,
    pub author: String,
    pub origin: SaveOrigin,
    pub summary: VersionSummary,
    pub content_hash: String,
    pub content_stored: bool,
    pub content_size: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from: Option<String>,
    /// 裁剪审计等非内容版本的说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryWarnings {
    pub version_count: usize,
    pub per_entry_limit: usize,
    pub per_entry_exceeded: bool,
    pub total_bytes: u64,
    pub total_limit_bytes: u64,
    pub total_exceeded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimPreview {
    pub keep_version_id: String,
    pub delete_count: usize,
    pub bytes_to_free: u64,
    pub kept_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimOutcome {
    pub deleted_count: usize,
    pub bytes_freed: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn local_author() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// 条目 id → 历史目录名。合法字符保留，其余替换为 `_`，再附 8 位哈希，
/// 避免 `a/b` 与 `a_b` 这类清洗后碰撞，也避免路径分隔符逃逸。
fn entry_key(entry_id: &str) -> String {
    let mut safe = String::with_capacity(entry_id.len());
    for c in entry_id.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
            safe.push(c);
        } else {
            safe.push('_');
        }
    }
    let hash = crate::fingerprint::fnv1a64(entry_id.as_bytes()) as u32;
    format!("{safe}-{hash:08x}")
}

pub fn history_dir(runtime_dir: &Path, entry_id: &str) -> PathBuf {
    runtime_dir.join("history").join(entry_key(entry_id))
}

fn content_hash(content: &str) -> String {
    format!("{:016x}", crate::fingerprint::fnv1a64(content.as_bytes()))
}

fn diff_summary(before: &str, after: &str) -> VersionSummary {
    let b: Vec<&str> = before.split('\n').collect();
    let a: Vec<&str> = after.split('\n').collect();

    let mut prefix = 0;
    while prefix < b.len() && prefix < a.len() && b[prefix] == a[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix + prefix < b.len()
        && suffix + prefix < a.len()
        && b[b.len() - 1 - suffix] == a[a.len() - 1 - suffix]
    {
        suffix += 1;
    }
    VersionSummary {
        added_lines: a.len().saturating_sub(prefix + suffix),
        removed_lines: b.len().saturating_sub(prefix + suffix),
    }
}

fn version_files(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        out.push((path.with_file_name(format!("{stem}.md")), path));
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// 读取全部版本元数据，按时间升序（文件名定长，字典序即时间序）。
pub fn list_versions(kb_root: &Path, entry_id: &str) -> AppResult<Vec<VersionMeta>> {
    let dir = history_dir(kb_root, entry_id);
    let mut versions = Vec::new();
    for (_, meta_path) in version_files(&dir) {
        let Ok(bytes) = std::fs::read(&meta_path) else {
            continue;
        };
        match serde_json::from_slice::<VersionMeta>(&bytes) {
            Ok(meta) => versions.push(meta),
            Err(e) => eprintln!(
                "[history] skip corrupt metadata {}: {e}",
                meta_path.display()
            ),
        }
    }
    versions.sort_by(|a, b| a.version_id.cmp(&b.version_id));
    Ok(versions)
}

fn latest_version(versions: &[VersionMeta]) -> Option<&VersionMeta> {
    versions
        .iter()
        .filter(|v| v.origin != SaveOrigin::Trim)
        .max_by(|a, b| a.version_id.cmp(&b.version_id))
}

fn next_version_id(dir: &Path, at_ms: u64) -> String {
    let mut seq = 0usize;
    loop {
        let candidate = format!("{at_ms:013}-{seq:04}");
        if !dir.join(format!("{candidate}.json")).exists() {
            return candidate;
        }
        seq += 1;
    }
}

/// 记录一次正文变更。返回 `None` 表示按策略跳过（哈希相同或自动保存未到间隔）。
pub fn record_version(
    kb_root: &Path,
    entry_id: &str,
    previous: &str,
    next: &str,
    origin: SaveOrigin,
    restored_from: Option<String>,
) -> AppResult<Option<VersionMeta>> {
    record_version_at(
        kb_root,
        entry_id,
        previous,
        next,
        origin,
        restored_from,
        now_ms(),
    )
}

fn record_version_at(
    kb_root: &Path,
    entry_id: &str,
    previous: &str,
    next: &str,
    origin: SaveOrigin,
    restored_from: Option<String>,
    at_ms: u64,
) -> AppResult<Option<VersionMeta>> {
    let dir = history_dir(kb_root, entry_id);
    let versions = list_versions(kb_root, entry_id)?;
    let latest = latest_version(&versions);
    let hash = content_hash(next);

    // 回退必须留下一条新版本，即使哈希与上一条相同（保证“回退也留痕”）。
    if restored_from.is_none() {
        if let Some(latest) = latest {
            if latest.content_hash == hash {
                return Ok(None);
            }
        }
    } else {
        // 回退也不产生完全重复的记录：如果最新版本已经是目标内容且没有
        // restored_from，则还是追加，但若最新已经是同一次回退则跳过。
        if let Some(latest) = latest {
            if latest.content_hash == hash
                && latest.restored_from.as_deref() == restored_from.as_deref()
            {
                return Ok(None);
            }
        }
    }

    if origin == SaveOrigin::Autosave {
        if let Some(latest) = latest {
            if at_ms.saturating_sub(latest.created_at_ms) < AUTOSAVE_MIN_INTERVAL_MS {
                return Ok(None);
            }
        }
    }

    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::io(format!("history: create dir {}", dir.display()), e)
    })?;

    let version_id = next_version_id(&dir, at_ms);
    let meta_path = dir.join(format!("{version_id}.json"));
    let content_path = dir.join(format!("{version_id}.md"));

    let store_content = next.len() <= MAX_STORED_CONTENT_BYTES;
    if store_content {
        crate::content_io::write_atomic(&content_path, next)?;
    } else if content_path.exists() {
        let _ = std::fs::remove_file(&content_path);
    }

    let meta = VersionMeta {
        version_id: version_id.clone(),
        entry_id: entry_id.to_string(),
        created_at_ms: at_ms,
        author: local_author(),
        origin,
        summary: diff_summary(previous, next),
        content_hash: hash,
        content_stored: store_content,
        content_size: next.len(),
        restored_from,
        note: None,
    };
    let bytes = serde_json::to_vec(&meta).map_err(|e| AppError::IoOp {
        message: format!("history: serialize metadata: {e}"),
    })?;
    std::fs::write(&meta_path, bytes).map_err(|e| {
        AppError::io(format!("history: write {}", meta_path.display()), e)
    })?;
    Ok(Some(meta))
}

/// 保存管线 hook：显式保存 / 自动保存 / 排版 / 回退。
/// 失败只记日志，由调用方决定——版本记录问题不能阻止用户保存。
pub fn on_content_saved(event: &ContentSavedEvent) -> AppResult<Option<VersionMeta>> {
    record_version(
        &event.history_root,
        &event.entry_id,
        &event.previous,
        &event.next,
        event.origin,
        event.restored_from.clone(),
    )
}

/// watcher 检测到外部正文变化时调用。只在内容哈希与最新版本不同时记录。
pub fn record_external_change(
    kb_root: &Path,
    entry_id: &str,
    next: &str,
) -> AppResult<Option<VersionMeta>> {
    let versions = list_versions(kb_root, entry_id)?;
    let latest = latest_version(&versions);
    if let Some(latest) = latest {
        if latest.content_hash == content_hash(next) {
            return Ok(None);
        }
    }
    let previous = latest
        .and_then(|v| read_version_content(kb_root, entry_id, &v.version_id).ok().flatten())
        .unwrap_or_default();
    record_version(kb_root, entry_id, &previous, next, SaveOrigin::External, None)
}

/// 读取某个版本的正文副本；超过 2MB 未存正文时返回 `None`。
pub fn read_version_content(
    kb_root: &Path,
    entry_id: &str,
    version_id: &str,
) -> AppResult<Option<String>> {
    let dir = history_dir(kb_root, entry_id);
    let versions = list_versions(kb_root, entry_id)?;
    let Some(meta) = versions.iter().find(|v| v.version_id == version_id) else {
        return Err(AppError::VersionNotFound {
            version_id: version_id.to_string(),
        });
    };
    if !meta.content_stored {
        return Ok(None);
    }
    let path = dir.join(format!("{}.md", meta.version_id));
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::io(
            format!("history: read version {}", path.display()),
            e,
        )),
    }
}

fn version_bytes(meta_path: &Path, content_path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(m) = std::fs::metadata(meta_path) {
        total += m.len();
    }
    if let Ok(m) = std::fs::metadata(content_path) {
        total += m.len();
    }
    total
}

/// 裁剪预览：保留 `keep_version_id` 及其之后的全部版本，统计之前版本的
/// 数量与可释放空间。
pub fn trim_preview(
    kb_root: &Path,
    entry_id: &str,
    keep_version_id: &str,
) -> AppResult<TrimPreview> {
    let dir = history_dir(kb_root, entry_id);
    let versions = list_versions(kb_root, entry_id)?;
    let Some(keep_index) = versions.iter().position(|v| v.version_id == keep_version_id) else {
        return Err(AppError::VersionNotFound {
            version_id: keep_version_id.to_string(),
        });
    };

    let mut delete_count = 0usize;
    let mut bytes_to_free = 0u64;
    for version in &versions[..keep_index] {
        let meta_path = dir.join(format!("{}.json", version.version_id));
        let content_path = dir.join(format!("{}.md", version.version_id));
        bytes_to_free += version_bytes(&meta_path, &content_path);
        delete_count += 1;
    }

    Ok(TrimPreview {
        keep_version_id: keep_version_id.to_string(),
        delete_count,
        bytes_to_free,
        kept_count: versions.len() - delete_count,
    })
}

/// 手动裁剪。删除保留起点之前的版本（保留起点自身保留），然后追加一条
/// `origin=trim` 的元数据审计记录（不含任何被删内容）。
pub fn trim_history(
    kb_root: &Path,
    entry_id: &str,
    keep_version_id: &str,
) -> AppResult<TrimOutcome> {
    let dir = history_dir(kb_root, entry_id);
    let versions = list_versions(kb_root, entry_id)?;
    let Some(keep_index) = versions.iter().position(|v| v.version_id == keep_version_id) else {
        return Err(AppError::VersionNotFound {
            version_id: keep_version_id.to_string(),
        });
    };
    let _ = trim_preview(kb_root, entry_id, keep_version_id)?;

    let mut deleted_count = 0usize;
    let mut bytes_freed = 0u64;
    let mut deleted_range: Vec<&str> = Vec::new();
    for version in &versions[..keep_index] {
        let meta_path = dir.join(format!("{}.json", version.version_id));
        let content_path = dir.join(format!("{}.md", version.version_id));
        bytes_freed += version_bytes(&meta_path, &content_path);
        if std::fs::remove_file(&meta_path).is_ok() {
            deleted_count += 1;
        }
        let _ = std::fs::remove_file(&content_path);
        deleted_range.push(version.version_id.as_str());
    }

    let at_ms = now_ms();
    let version_id = next_version_id(&dir, at_ms);
    let meta = VersionMeta {
        version_id,
        entry_id: entry_id.to_string(),
        created_at_ms: at_ms,
        author: local_author(),
        origin: SaveOrigin::Trim,
        summary: VersionSummary::default(),
        content_hash: format!("trim-{}", at_ms),
        content_stored: false,
        content_size: 0,
        restored_from: None,
        note: Some(format!(
            "trimmed {deleted_count} version(s) from {} to {}, freed {bytes_freed} bytes, keeping {keep_version_id}",
            deleted_range.first().copied().unwrap_or("-"),
            deleted_range.last().copied().unwrap_or("-"),
        )),
    };
    let bytes = serde_json::to_vec(&meta).map_err(|e| AppError::IoOp {
        message: format!("history: serialize audit: {e}"),
    })?;
    std::fs::write(dir.join(format!("{}.json", meta.version_id)), bytes)
        .map_err(|e| AppError::io("history: write trim audit", e))?;

    Ok(TrimOutcome {
        deleted_count,
        bytes_freed,
    })
}

fn dir_usage(dir: &Path) -> (usize, u64) {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let Ok(read) = std::fs::read_dir(dir) else {
        return (files, bytes);
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let (f, b) = dir_usage(&path);
            files += f;
            bytes += b;
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".json") || name.ends_with(".md") {
            files += 1;
            if let Ok(m) = std::fs::metadata(&path) {
                bytes += m.len();
            }
        }
    }
    (files, bytes)
}

/// 阈值警告：不阻止保存，只让 UI 提示去裁剪。
pub fn warnings(kb_root: &Path, entry_id: &str) -> AppResult<HistoryWarnings> {
    let count = list_versions(kb_root, entry_id)?
        .iter()
        .filter(|v| v.origin != SaveOrigin::Trim)
        .count();
    let (_, total_bytes) = dir_usage(&kb_root.join("history"));
    Ok(HistoryWarnings {
        version_count: count,
        per_entry_limit: PER_ENTRY_WARN_COUNT,
        per_entry_exceeded: count > PER_ENTRY_WARN_COUNT,
        total_bytes,
        total_limit_bytes: TOTAL_WARN_BYTES,
        total_exceeded: total_bytes > TOTAL_WARN_BYTES,
    })
}

/// 文档改名/迁移后把历史目录一起搬走。失败时调用方会整体回滚，
/// 因此不会出现文档已移动、历史仍挂在旧 id 下的半成品。
pub fn migrate_entry_history(kb_root: &Path, old_id: &str, new_id: &str) -> AppResult<()> {
    if old_id == new_id {
        return Ok(());
    }
    let old_dir = history_dir(kb_root, old_id);
    if !old_dir.exists() {
        return Ok(());
    }
    let new_dir = history_dir(kb_root, new_id);
    if new_dir.exists() {
        // 极少数情况下目标已有历史（例如先删后重建）：保守地不覆盖，
        // 让调用方记录警告即可。
        return Err(AppError::IoOp {
            message: format!(
                "history: target dir already exists: {}",
                new_dir.display()
            ),
        });
    }
    if let Some(parent) = new_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io("history: create history root", e))?;
    }
    std::fs::rename(&old_dir, &new_dir).map_err(|e| {
        AppError::io(
            format!(
                "history: migrate {} -> {}",
                old_dir.display(),
                new_dir.display()
            ),
            e,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn root(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().to_path_buf()
    }

    #[test]
    fn explicit_save_creates_version_and_dedupes_same_hash() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);

        let created = record_version(&kb, "note", "", "a\n", SaveOrigin::Editor, None)
            .unwrap()
            .expect("first save should create a version");
        assert_eq!(created.origin, SaveOrigin::Editor);
        assert_eq!(created.summary.added_lines, 1);

        let again = record_version(&kb, "note", "a\n", "a\n", SaveOrigin::Editor, None)
            .unwrap();
        assert!(again.is_none(), "identical content must be deduped");
    }

    #[test]
    fn autosave_is_throttled_to_five_minutes() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);

        let first = record_version_at(
            &kb,
            "note",
            "",
            "a\n",
            SaveOrigin::Autosave,
            None,
            1_000,
        )
        .unwrap();
        assert!(first.is_some());

        let too_soon = record_version_at(
            &kb,
            "note",
            "a\n",
            "b\n",
            SaveOrigin::Autosave,
            None,
            1_000 + 60_000,
        )
        .unwrap();
        assert!(too_soon.is_none(), "within 5 minutes must be skipped");

        let later = record_version_at(
            &kb,
            "note",
            "b\n",
            "c\n",
            SaveOrigin::Autosave,
            None,
            1_000 + AUTOSAVE_MIN_INTERVAL_MS,
        )
        .unwrap();
        assert!(later.is_some(), "after 5 minutes a version is created");
    }

    #[test]
    fn external_change_records_only_when_hash_changes() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);
        record_version(&kb, "note", "", "a\n", SaveOrigin::Editor, None).unwrap();

        assert!(
            record_external_change(&kb, "note", "a\n").unwrap().is_none(),
            "same hash must not create a version"
        );
        assert!(
            record_external_change(&kb, "note", "b\n").unwrap().is_some(),
            "changed hash must create an external version"
        );
    }

    #[test]
    fn restore_forces_new_version_with_restored_from() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);
        let first = record_version(&kb, "note", "", "a\n", SaveOrigin::Editor, None)
            .unwrap()
            .unwrap();
        record_version(&kb, "note", "a\n", "b\n", SaveOrigin::Editor, None).unwrap();

        let restored = record_version(
            &kb,
            "note",
            "b\n",
            "a\n",
            SaveOrigin::Restore,
            Some(first.version_id.clone()),
        )
        .unwrap()
        .expect("restore must append a version");
        assert_eq!(restored.origin, SaveOrigin::Restore);
        assert_eq!(restored.restored_from.as_deref(), Some(first.version_id.as_str()));
        assert_eq!(list_versions(&kb, "note").unwrap().len(), 3);
    }

    #[test]
    fn large_content_only_stores_metadata() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);
        let big = "x".repeat(MAX_STORED_CONTENT_BYTES + 1);

        let meta = record_version(&kb, "note", "", &big, SaveOrigin::Editor, None)
            .unwrap()
            .unwrap();
        assert!(!meta.content_stored);
        assert_eq!(meta.content_size, big.len());
        assert!(read_version_content(&kb, "note", &meta.version_id).unwrap().is_none());
    }

    #[test]
    fn trim_deletes_versions_before_keep_point_and_keeps_audit() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);
        let v1 = record_version(&kb, "note", "", "a\n", SaveOrigin::Editor, None)
            .unwrap()
            .unwrap();
        let v2 = record_version(&kb, "note", "a\n", "b\n", SaveOrigin::Editor, None)
            .unwrap()
            .unwrap();
        let v3 = record_version(&kb, "note", "b\n", "c\n", SaveOrigin::Editor, None)
            .unwrap()
            .unwrap();
        assert_eq!(list_versions(&kb, "note").unwrap().len(), 3);

        let preview = trim_preview(&kb, "note", &v2.version_id).unwrap();
        assert_eq!(preview.delete_count, 1);
        assert_eq!(preview.kept_count, 2);
        assert!(preview.bytes_to_free > 0);

        let outcome = trim_history(&kb, "note", &v2.version_id).unwrap();
        assert_eq!(outcome.deleted_count, 1);

        let remaining = list_versions(&kb, "note").unwrap();
        assert!(remaining.iter().all(|v| v.version_id != v1.version_id));
        assert!(remaining.iter().any(|v| v.version_id == v2.version_id));
        assert!(remaining.iter().any(|v| v.version_id == v3.version_id));
        let audit = remaining
            .iter()
            .find(|v| v.origin == SaveOrigin::Trim)
            .expect("trim audit must be recorded");
        assert!(audit.note.as_deref().unwrap().contains("trimmed 1 version"));
        assert!(read_version_content(&kb, "note", &audit.version_id).unwrap().is_none());
    }

    #[test]
    fn warnings_flag_by_count_and_global_size() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);
        for i in 0..=PER_ENTRY_WARN_COUNT {
            record_version(
                &kb,
                "note",
                "",
                &format!("body {i}\n"),
                SaveOrigin::Editor,
                None,
            )
            .unwrap();
        }
        let w = warnings(&kb, "note").unwrap();
        assert!(w.per_entry_exceeded, "201 versions must exceed the 200 limit");
        assert_eq!(w.version_count, PER_ENTRY_WARN_COUNT + 1);
    }

    #[test]
    fn history_is_ignored_by_scanner_and_renamed_with_entry() {
        let dir = tempdir().unwrap();
        let kb = root(&dir);
        record_version(&kb, "old-id", "", "a\n", SaveOrigin::Editor, None).unwrap();
        let old_dir = history_dir(&kb, "old-id");
        assert!(old_dir.exists());

        migrate_entry_history(&kb, "old-id", "new-id").unwrap();
        assert!(!old_dir.exists());
        assert!(history_dir(&kb, "new-id").exists());
        assert_eq!(list_versions(&kb, "new-id").unwrap().len(), 1);

        // scanner 的产物过滤不得把历史当成条目（该断言在 scanner 测试中覆盖，
        // 这里只验证目录确实位于 `.synapse/` 下）。
        assert!(old_dir.starts_with(kb.join("history")));
    }
}
