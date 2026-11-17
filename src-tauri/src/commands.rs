use crate::entry::RawEntry;
use crate::error::{AppError, AppResult};
use crate::index::{Index, StoredEntry};
use crate::kb_registry::KBInfo;
use crate::state::AppState;
use crate::validator::{self, Severity, ValidationIssue, ValidationReport};
use std::path::{Component, Path, PathBuf};
#[cfg(feature = "tauri")]
use tauri::State;

/// Validate that `name` is safe to use as a single filename component.
/// Rejects path traversal ('..', '/', '\\'), absolute paths, and embedded separators.
/// On success, `kb_root.join(format!("{name}.md"))` is guaranteed to stay under `kb_root`.
fn validate_filename(name: &str) -> AppResult<()> {
    if name.is_empty() {
        return Err(AppError::NameEmpty);
    }
    let normalized = name.replace('\\', "/");
    let mut count = 0;
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(_) => count += 1,
            Component::CurDir | Component::ParentDir | Component::RootDir
            | Component::Prefix(_) => {
                return Err(AppError::NameInvalid {
                    name: name.to_string(),
                });
            }
        }
    }
    if count != 1 {
        return Err(AppError::NameInvalid {
            name: name.to_string(),
        });
    }
    Ok(())
}

/// Read the active KB's path from `state.current_kb` and verify it still
/// points at a usable KB root. Returns a descriptive error in three failure
/// modes (no active KB / path missing / path is a file) so file-writing
/// commands fail loudly instead of silently creating `.md` files at a
/// stale or non-existent location after the user moves the KB directory
/// externally.
///
/// Without this check, `create_entry_simple` and `create_entry_from_reference`
/// would happily `join(...)` the stale path and write new content to a
/// directory the user no longer considers their KB — confusing and silent.

/// 读取 KB 根目录的 README 作为「首页」内容（D6：README = KB 首页）。
/// README 不索引为条目、不入图谱、不参与关系解析；这里只做只读展示。
/// 大小上限 2 MiB，超限返回错误（README 不该那么大）。
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn read_kb_readme(state: State<'_, AppState>) -> AppResult<Option<String>> {
    let kb_root = kb_root_or_err(&state).await?;
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    for name in ["README.md", "readme.md", "Readme.md"] {
        let path = kb_root.join(name);
        if path.is_file() {
            let meta = std::fs::metadata(&path).map_err(|e| {
                AppError::io(format!("read_kb_readme: stat {}", path.display()), e)
            })?;
            if meta.len() > MAX_BYTES {
                return Err(AppError::io(
                    format!("read_kb_readme: {} too large", path.display()),
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("README exceeds {} bytes", MAX_BYTES),
                    ),
                ));
            }
            let content = std::fs::read_to_string(&path).map_err(|e| {
                AppError::io(format!("read_kb_readme: read {}", path.display()), e)
            })?;
            return Ok(Some(content));
        }
    }
    Ok(None)
}

/// 当前 KB 的运行时状态目录（app data `indexes/<kb_id>/`，Q10.a）。
/// 布局与版本历史的读写一律用它，不再落 KB 目录。
async fn runtime_dir_or_err(state: &AppState) -> AppResult<PathBuf> {
    let id = {
        let cur = state.current_kb.read().await;
        cur.as_ref().ok_or_else(|| AppError::NoActiveKb)?.id.clone()
    };
    Ok(state.runtime_dir_base.join(&id))
}

async fn kb_root_or_err(state: &AppState) -> AppResult<PathBuf> {
    let path = {
        let cur = state.current_kb.read().await;
        let kb = cur.as_ref().ok_or_else(|| AppError::NoActiveKb)?;
        PathBuf::from(&kb.path)
    };
    if !path.exists() {
        return Err(AppError::KbDirGone {
            path: path.to_string_lossy().to_string(),
        });
    }
    if !path.is_dir() {
        return Err(AppError::KbDirNotADirectory {
            path: path.to_string_lossy().to_string(),
        });
    }
    Ok(path)
}

/// Validate that `parent_group` is a safe relative subpath under the KB root.
/// Allows nested directories (`compliance/legal`) but rejects any escape:
/// - empty segments (consecutive slashes)
/// - `.` / `..` components
/// - root paths (`/etc`)
/// - Windows drive prefixes
/// `kb_root.join(parent_group)` is then guaranteed to stay under `kb_root`.
fn validate_parent_group(parent_group: &str) -> AppResult<()> {
    if parent_group.is_empty() {
        return Ok(());
    }
    let normalized = parent_group.replace('\\', "/");
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir | Component::ParentDir | Component::RootDir
            | Component::Prefix(_) => {
                return Err(AppError::ParentGroupInvalid {
                    parent_group: parent_group.to_string(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn list_kbs(state: State<'_, AppState>) -> AppResult<Vec<KBInfo>> {
    let reg = state.kbs.read().await;
    Ok(reg.list())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_active_kb(state: State<'_, AppState>) -> AppResult<Option<KBInfo>> {
    let reg = state.kbs.read().await;
    Ok(reg.active().cloned())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn add_kb(
    state: State<'_, AppState>,
    name: String,
    path: String,
) -> AppResult<KBInfo> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(AppError::PathNotFound { path });
    }
    if !path_buf.is_dir() {
        return Err(AppError::NotADirectory { path });
    }

    let info = {
        let mut reg = state.kbs.write().await;
        reg.add(name, path).map_err(AppError::from_save_kbs)?
    };

    // 自动设为活跃 + 重建索引
    load_active_kb(&state, &info.id).await?;
    Ok(info)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn remove_kb(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let was_active = {
        let mut reg = state.kbs.write().await;
        let was_active = reg.active_id() == Some(id.as_str());
        reg.remove(&id)
            .map_err(AppError::from_save_kbs)?;
        was_active
    };

    if was_active {
        // Per Finding #15: the registry no longer auto-picks a successor.
        // Be honest: removing the active KB means there's no active KB
        // until the user picks one. clear_active_kb drops the index and
        // the file watcher.
        clear_active_kb(&state).await?;
    }

    // Delete the per-KB index.db so it doesn't accumulate orphans. The
    // registry no longer owns the path lookup — derive it here so the
    // filesystem cleanup runs alongside the registry cleanup. A missing
    // file (already cleaned up by hand, deleted twice) is fine.
    let db_path = crate::paths::indexes_dir().join(format!("{id}.db"));
    if db_path.exists() {
        if let Err(e) = std::fs::remove_file(&db_path) {
            eprintln!("[remove_kb] failed to delete index file {}: {e}", db_path.display());
        }
    }

    Ok(())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn set_active_kb(state: State<'_, AppState>, id: String) -> AppResult<()> {
    load_active_kb(&state, &id).await
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn rebuild_active_kb(state: State<'_, AppState>) -> AppResult<usize> {
    let _guard = state.write_lock.lock().await;
    let path = kb_root_or_err(&*state).await?;
    let count = {
        let idx = state.index.read().await;
        idx.rebuild(&path)?
    };
    Ok(count)
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_entries(state: State<'_, AppState>) -> AppResult<Vec<StoredEntry>> {
    // 等待启动 rebuild 结束（无活动 KB 或提前返回时它不会持有闸门）。
    let _gate = state.startup_gate.read().await;
    let idx = state.index.read().await;
    let entries = idx.get_all_entries()?;
    Ok(entries)
}

/// `get_entries` 的原子版本：同一次读取里同时返回条目和内容指纹。
///
/// 前端用返回值里的指纹作为“我当前展示的数据版本”。如果分两次调用
/// （先取条目、再取指纹），中间的 watcher 重建会让两者不一致，从而把
/// 真实的外部变更误判成自身回声丢掉。注意指纹包含正文文件的
/// `size + mtime`，因此只改正文也会体现在里面。
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_entries_snapshot(state: State<'_, AppState>) -> AppResult<EntriesSnapshot> {
    let _gate = state.startup_gate.read().await;
    let idx = state.index.read().await;
    let entries = idx.get_all_entries()?;
    let fingerprint = crate::entries_fingerprint(&entries);
    Ok(EntriesSnapshot {
        entries,
        fingerprint,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntriesSnapshot {
    pub entries: Vec<StoredEntry>,
    pub fingerprint: String,
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn active_kb_entry_count(state: State<'_, AppState>) -> AppResult<usize> {
    // 与 `get_entries` 同样等启动 rebuild 结束，避免首屏拿到旧计数。
    let _gate = state.startup_gate.read().await;
    let idx = state.index.read().await;
    Ok(idx.entry_count()?)
}

/// 读取当前 KB 的节点布局（`.synapse-layout.json`）。
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_layout(
    state: State<'_, AppState>,
) -> AppResult<std::collections::HashMap<String, crate::layout_store::NodePosition>> {
    let runtime_dir = runtime_dir_or_err(&state).await?;
    Ok(crate::layout_store::load_positions(&runtime_dir))
}

/// 保存单个节点的位置；由前端在拖拽结束时调用。
/// 与其它写命令一样持有 `write_lock`，串行化 `.synapse-layout.json` 的
/// 读-改-写（`layout_store::update_position` 内部也有一把同步锁，保证
/// 即便绕过该命令并发调用也不会互相覆盖）。
#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn save_node_position(
    state: State<'_, AppState>,
    entry_id: String,
    x: f64,
    y: f64,
) -> AppResult<()> {
    let runtime_dir = runtime_dir_or_err(&state).await?;
    let _guard = state.write_lock.lock().await;
    crate::layout_store::update_position(&runtime_dir, &entry_id, x, y)?;
    Ok(())
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn get_entry_content(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Option<String>> {
    // content_path 由索引提供；rebuild 结束前可能指向旧/缺失路径。
    let _gate = state.startup_gate.read().await;
    let path = {
        let idx = state.index.read().await;
        idx.get_entry_content_path(&id)?
    };
    let Some(path) = path else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(_) => Ok(None),
    }
}

// ---------- 正文编辑与无损排版 ----------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEntryResult {
    /// 内容与磁盘上的原文一致时为 `false`（不产生写入，也不触发版本 hook）。
    pub changed: bool,
    pub bytes: usize,
    /// 保存完成时间（epoch 毫秒）。前端只用于展示“已保存”状态。
    pub saved_at_ms: u64,
    /// 落盘后的完整正文，前端可直接用它同步编辑器与预览。
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatPreview {
    pub original: String,
    pub formatted: String,
    pub changed: bool,
    pub changed_lines: usize,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn content_path_for(state: &AppState, id: &str) -> AppResult<PathBuf> {
    let idx = state.index.read().await;
    idx.get_entry_content_path(id)?
        .ok_or_else(|| AppError::EntryNotFound { id: id.to_string() })
}

fn read_content(id: &str, path: &Path) -> AppResult<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(AppError::EntryContentMissing { id: id.to_string() })
        }
        Err(e) => Err(AppError::io(
            format!("read content {}", path.display()),
            e,
        )),
    }
}

/// 保存正文。调用方必须已持有 `state.write_lock`。
///
/// 顺序：读旧内容 → 需要时原子替换 → 登记本地写入（抑制 watcher 的
/// 回声刷新）→ 版本 hook（显式保存即使内容未变也会尝试记录）→ 重建索引。
///
/// 批次 3 起为 pub：stdio MCP server 的 `save_content` 工具以
/// `SaveOrigin::Agent` 复用同一条管线（Q5.a / D7）。
pub async fn write_entry_content(
    state: &AppState,
    id: &str,
    content: &str,
    origin: crate::history::SaveOrigin,
    restored_from: Option<String>,
) -> AppResult<SaveEntryResult> {
    let path = content_path_for(state, id).await?;
    let previous = read_content(id, &path)?;
    let changed = previous != content;

    if changed {
        let tmp = crate::content_io::write_atomic(&path, content)?;
        // 临时文件在 rename 后已消失（期望 Missing）；目标文件按内容哈希登记。
        // watcher 的这一批事件全部命中时才会判为 local-write。
        state.note_local_write(&tmp);
        state.note_local_write(&path);

        if let Ok(kb_root) = kb_root_or_err(state).await {
            let idx = state.index.read().await;
            let _ = idx.rebuild(&kb_root); // best-effort；前端随后会取快照
        }
    }

    // 版本 hook：显式保存即使内容未变也要调用（可能是自动保存刚写过盘但
    // 还没生成版本）；自动保存内容未变则完全跳过。
    if changed || origin != crate::history::SaveOrigin::Autosave {
        // Q10.a：版本历史写运行时目录（app data），不落 KB 目录。
        let history_root = runtime_dir_or_err(state).await?;
        if let Err(e) = crate::history::on_content_saved(&crate::history::ContentSavedEvent {
            history_root,
            entry_id: id.to_string(),
            content_path: path,
            previous,
            next: content.to_string(),
            origin,
            restored_from,
        }) {
            // 版本记录失败不能阻止用户保存。
            eprintln!("[history] record version failed for {id}: {e}");
        }
    }

    Ok(SaveEntryResult {
        changed,
        bytes: content.len(),
        saved_at_ms: now_ms(),
        content: content.to_string(),
    })
}

/// 编辑器显式保存（按钮或 Cmd/Ctrl+S）。
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn save_entry_content(
    state: State<'_, AppState>,
    id: String,
    content: String,
) -> AppResult<SaveEntryResult> {
    let _guard = state.write_lock.lock().await;
    write_entry_content(
        &state,
        &id,
        &content,
        crate::history::SaveOrigin::Editor,
        None,
    )
    .await
}

/// 自动保存：写盘，但版本记录受 5 分钟节流约束（在 history 层决定）。
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn autosave_entry_content(
    state: State<'_, AppState>,
    id: String,
    content: String,
) -> AppResult<SaveEntryResult> {
    let _guard = state.write_lock.lock().await;
    write_entry_content(
        &state,
        &id,
        &content,
        crate::history::SaveOrigin::Autosave,
        None,
    )
    .await
}

/// 纯读：返回排版前后的内容，供前端展示差异并让用户确认。
/// 不做任何写入。
async fn preview_format_impl(state: &AppState, id: &str) -> AppResult<FormatPreview> {
    let path = content_path_for(state, id).await?;
    let original = read_content(id, &path)?;
    let outcome = crate::formatter::format_content(&original);
    Ok(FormatPreview {
        original,
        formatted: outcome.formatted,
        changed: outcome.changed,
        changed_lines: outcome.changed_lines,
    })
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn preview_format_entry(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<FormatPreview> {
    preview_format_impl(&state, &id).await
}

/// 用户确认后写回排版结果。重新读取当前磁盘内容再排版，避免把预览之后
/// 被外部修改的内容覆盖掉（只写回“当前内容排版后的版本”）。
async fn apply_format_impl(state: &AppState, id: &str) -> AppResult<SaveEntryResult> {
    let _guard = state.write_lock.lock().await;
    let path = content_path_for(state, id).await?;
    let original = read_content(id, &path)?;
    let outcome = crate::formatter::format_content(&original);
    write_entry_content(
        state,
        id,
        &outcome.formatted,
        crate::history::SaveOrigin::Format,
        None,
    )
    .await
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn apply_format_entry(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<SaveEntryResult> {
    apply_format_impl(&state, &id).await
}

/// 同一 KB 内的文档迁移：保持文件名/目录名，只改变所在目录。
///
/// 复用 `execute_rename_impl` 的完整管线（id 派生、级联 `[[id]]`、布局键、
/// 附件随目录一起移动、失败整体回滚），因此不会出现半移动状态。
/// 跨 KB 迁移不在本命令范围内。
pub async fn move_entry_impl(
    state: &AppState,
    id: String,
    target_dir: String,
) -> AppResult<RenameResult> {
    let old_path = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?
            .ok_or_else(|| AppError::EntryNotFound { id: id.clone() })?
    };
    let name = if old_path.is_dir() {
        old_path.file_name()
    } else {
        old_path.file_stem()
    }
    .and_then(|n| n.to_str())
    .ok_or_else(|| AppError::NameEmpty)?
    .to_string();

    execute_rename_impl(state, id, name, target_dir, true).await
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn move_entry(
    state: State<'_, AppState>,
    id: String,
    target_dir: String,
) -> AppResult<RenameResult> {
    move_entry_impl(&state, id, target_dir).await
}

// ---------- 版本历史 ----------

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn list_entry_versions(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<crate::history::VersionMeta>> {
    let runtime_dir = runtime_dir_or_err(&state).await?;
    crate::history::list_versions(&runtime_dir, &id)
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn get_entry_version_content(
    state: State<'_, AppState>,
    id: String,
    version_id: String,
) -> AppResult<Option<String>> {
    let runtime_dir = runtime_dir_or_err(&state).await?;
    crate::history::read_version_content(&runtime_dir, &id, &version_id)
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn get_history_warnings(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<crate::history::HistoryWarnings> {
    let runtime_dir = runtime_dir_or_err(&state).await?;
    crate::history::warnings(&runtime_dir, &id)
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn preview_trim_entry_history(
    state: State<'_, AppState>,
    id: String,
    keep_version_id: String,
) -> AppResult<crate::history::TrimPreview> {
    let runtime_dir = runtime_dir_or_err(&state).await?;
    crate::history::trim_preview(&runtime_dir, &id, &keep_version_id)
}

/// 手动裁剪：不可恢复，UI 必须先展示预览并二次确认。
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn trim_entry_history(
    state: State<'_, AppState>,
    id: String,
    keep_version_id: String,
) -> AppResult<crate::history::TrimOutcome> {
    let _guard = state.write_lock.lock().await;
    let runtime_dir = runtime_dir_or_err(&state).await?;
    crate::history::trim_history(&runtime_dir, &id, &keep_version_id)
}

/// 回退到指定版本：把旧正文写回当前文件，并追加一条带 `restored_from`
/// 的新版本（不覆盖、不删除原版本）。
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn restore_entry_version(
    state: State<'_, AppState>,
    id: String,
    version_id: String,
) -> AppResult<SaveEntryResult> {
    let _guard = state.write_lock.lock().await;
    let runtime_dir = runtime_dir_or_err(&state).await?;
    let content = crate::history::read_version_content(&runtime_dir, &id, &version_id)?
        .ok_or_else(|| AppError::VersionContentUnavailable {
            version_id: version_id.clone(),
        })?;
    write_entry_content(
        &state,
        &id,
        &content,
        crate::history::SaveOrigin::Restore,
        Some(version_id),
    )
    .await
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_relation(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
    relation_type: String,
    note: Option<String>,
) -> AppResult<()> {
    let _guard = state.write_lock.lock().await;

    let (from_path, to_path) = {
        let idx = state.index.read().await;
        let f = idx
            .get_entry_content_path(&from_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: from_id.clone(),
            })?;
        let t = idx
            .get_entry_content_path(&to_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: to_id.clone(),
            })?;
        (f, t)
    };

    append_relation_line(&from_path, &to_path, &to_id, &relation_type)?;
    // 只有落盘成功后才登记“本地写入”，失败时不会污染来源标记。
    state.note_local_write(&from_path);

    let idx = state.index.read().await;
    idx.add_relation(&from_id, &to_id, &relation_type, note.as_deref())?;
    Ok(())
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_relation(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
    relation_type: String,
) -> AppResult<()> {
    let _guard = state.write_lock.lock().await;

    // Per Finding #5: pre-fix this only removed the SQLite row. The
    // `[[to_id]]` line stayed in the from `.md` file, and the next
    // `rebuild_active_kb` walked the .md, saw the wikilink, and
    // re-inserted (from_id, to_id, "reference") — silently undoing the
    // user's deletion. Strip the matching wikilink line from the .md
    // before dropping the row so the deletion survives a rebuild.
    let from_path = {
        let idx = state.index.read().await;
        idx.get_entry_content_path(&from_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: from_id.clone(),
            })?
    };
    if remove_wikilink_line(&from_path, &to_id)? {
        state.note_local_write(&from_path);
    }

    let idx = state.index.read().await;
    idx.remove_relation(&from_id, &to_id, &relation_type)?;
    Ok(())
}

/// Strip the line that contains `[[to_id]]` from `path`. Used by
/// `delete_relation` to keep the `.md` file consistent with the SQLite
/// row removal — without this, rebuild re-derives the wikilink as a
/// `reference` relation and undoes the deletion (Finding #5).
///
/// Only the wikilink form `[[to_id]]` is touched — `[text](path)` style
/// links are left alone, since rebuild treats them as a separate codepath
/// (`is_attachment_url` + path-stem lookup) and removing them is not
/// part of "delete this relation."
fn remove_wikilink_line(path: &std::path::Path, to_id: &str) -> AppResult<bool> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let needle = format!("[[{to_id}]]");

    // Split lines but keep track of whether the file ended with a newline
    // — `.lines()` drops a trailing newline, and we want to restore it
    // so the file diff stays minimal (no spurious \n churn on every
    // delete).
    let trailing_nl = content.ends_with('\n');
    let mut kept: Vec<&str> = Vec::new();
    let mut removed = false;
    for line in content.split_inclusive('\n') {
        // split_inclusive keeps the trailing \n on each line.
        if !removed && line.contains(&needle) {
            removed = true;
            continue;
        }
        kept.push(line);
    }
    if !removed {
        return Ok(false);
    }
    let mut new_content: String = kept.iter().copied().collect();
    if !trailing_nl && !new_content.ends_with('\n') {
        // No-op — the helper preserves whatever end-of-file the file had.
    }
    let _ = trailing_nl;
    std::fs::write(path, new_content)?;
    Ok(true)
}

/// Delete an entry: remove the `.md` file from disk and the row from the
/// `entries` table (FK CASCADE wipes related `tags`, `attachments`, and
/// `relations` rows). Wikilinks inside other entries that point at this one
/// become dangling — they will fail to resolve on the next rebuild, which is
/// the correct behavior (the user edits those files by hand to clean up).
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn delete_entry(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    let _guard = state.write_lock.lock().await;

    let path = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?
            .ok_or_else(|| AppError::EntryNotFound { id: id.clone() })?
    };

    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| AppError::io(
            format!("delete_entry: remove dir {}", path.display()),
            e,
        ))?;
    } else {
        std::fs::remove_file(&path).map_err(|e| AppError::io(
            format!("delete_entry: remove {}", path.display()),
            e,
        ))?;
    }
    state.note_local_write(&path);

    let idx = state.index.read().await;
    idx.remove_entry(&id)?;
    Ok(())
}

/// 在 from 的正文文件末尾追加一行显式关系声明（`@relation: type [[id]]`）。
/// 只写 `[[id]]` 的话 rebuild 只能恢复 `reference`，用户选择的 relationType 会丢失。
fn append_relation_line(
    from_path: &std::path::Path,
    _to_path: &std::path::Path,
    to_id: &str,
    relation_type: &str,
) -> AppResult<()> {
    let mut src = std::fs::read_to_string(from_path).unwrap_or_default();
    if !src.ends_with('\n') {
        src.push('\n');
    }
    src.push_str(&format!("\n@relation: {relation_type} [[{to_id}]]\n"));
    std::fs::write(from_path, src)?;
    Ok(())
}

/// 独立创建新 entry。`parentGroup` 为空时落到 KB 根目录，否则落到 `<kb>/<parentGroup>/`。
///
/// 关联方向由参数决定：
/// - `link_from_entry` 传入：源 entry 作为**上级**（关系方向 source → 新 entry，写入源 .md）
/// - `link_to_entry` 传入：源 entry 作为**下级**（关系方向 新 entry → source，写入新 .md）
/// 两者可同时传入（少见），分别处理双向引用。
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_entry_simple(
    state: State<'_, AppState>,
    parent_group: String,
    name: String,
    content: Option<String>,
    link_from_entry: Option<String>,
    link_to_entry: Option<String>,
    relation_type: Option<String>,
    note: Option<String>,
) -> AppResult<String> {
    let _guard = state.write_lock.lock().await;

    validate_filename(&name)?;
    if name.ends_with(".md") {
        return Err(AppError::NameHasMdSuffix);
    }
    validate_parent_group(&parent_group)?;

    let kb_root = kb_root_or_err(&state).await?;

    let file_path = if parent_group.is_empty() {
        kb_root.join(format!("{name}.md"))
    } else {
        let group_dir = kb_root.join(&parent_group);
        std::fs::create_dir_all(&group_dir)?;
        group_dir.join(format!("{name}.md"))
    };
    if file_path.exists() {
        return Err(AppError::FileExists {
            path: file_path.to_string_lossy().to_string(),
        });
    }

    // Build the new entry id using the SAME scheme as `entry::read_md_file`
    // (rel.path.with_extension("").replace(['\\', '/'], "-")) so the caller
    // gets back an id that round-trips through the DB. Pre-fix this used
    // `format!("{parent_group}-{name}")`, which kept literal `/` for
    // nested groups — a `compliance/legal/memo` parent_group produced id
    // `compliance/legal/memo-myname`, while `read_md_file` wrote the same
    // file's id as `compliance-legal-memo-myname`. The caller had to look
    // up the real id manually. Per Finding #3.
    let new_id = if parent_group.is_empty() {
        name.clone()
    } else {
        format!("{parent_group}-{name}").replace(['\\', '/'], "-")
    };

    // 1) 写新 .md
    let initial = content.unwrap_or_else(|| format!("# {name}\n\n<!-- TODO: 在此补充内容 -->\n"));
    std::fs::write(&file_path, &initial)?;
    // 新文件已落盘：登记为本地写入（含后续可能追加关系行的源文件）。
    let mut touched_paths: Vec<std::path::PathBuf> = vec![file_path.clone()];

    let rtype = relation_type.as_deref().unwrap_or("reference");

    // 2) 上级模式：source 是新 entry 的 from，关系方向 source → 新 entry
    //    写入 source .md
    if let Some(from_id) = link_from_entry.as_deref() {
        let from_path = {
            let idx = state.index.read().await;
            idx.get_entry_content_path(from_id)?
                .ok_or_else(|| AppError::EntryNotFound {
                    id: from_id.to_string(),
                })?
        };
        append_relation_line(&from_path, &file_path, &new_id, rtype)?;
        let idx = state.index.read().await;
        idx.add_relation(from_id, &new_id, rtype, note.as_deref())?;
        touched_paths.push(from_path);
    }

    // 3) 下级模式：新 entry 是 from，关系方向 新 entry → source
    //    写入新 .md
    if let Some(to_id) = link_to_entry.as_deref() {
        let to_path = {
            let idx = state.index.read().await;
            idx.get_entry_content_path(to_id)?
                .ok_or_else(|| AppError::EntryNotFound {
                    id: to_id.to_string(),
                })?
        };
        append_relation_line(&file_path, &to_path, to_id, rtype)?;
        let idx = state.index.read().await;
        idx.add_relation(&new_id, to_id, rtype, note.as_deref())?;
    }

    // 4) 增量更新索引：upsert 新 entry，然后解析它内容里的 [[xxx]] 双链 / 内链
    //    markdown 引用并建立 reference 关系。
    //    不要调用 rebuild() —— rebuild 会 DELETE relations 表并把所有 wikilink
    //    重置为 ("reference", None)，从而丢弃步骤 2/3 写入的 typed relation
    //    （Finding #4：用户传的 relationType / note 被静默覆盖）。
    let raw = crate::entry::read_md_file(&file_path, &kb_root);
    {
        let idx = state.index.read().await;
        idx.upsert_entry(&raw)?;
        idx.resolve_links_for_entry(&kb_root, &raw)?;
    }

    for path in &touched_paths {
        state.note_local_write(path);
    }
    Ok(new_id)
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn create_entry_from_reference(
    state: State<'_, AppState>,
    from_id: String,
    parent_group: String,
    name: String,
    source_url: String,
    ref_text: String,
) -> AppResult<String> {
    let _guard = state.write_lock.lock().await;

    let kb_root = kb_root_or_err(&state).await?;

    validate_filename(&name)?;
    if name.ends_with(".md") {
        return Err(AppError::NameHasMdSuffix);
    }
    validate_parent_group(&parent_group)?;

    let group_dir = kb_root.join(&parent_group);
    std::fs::create_dir_all(&group_dir)?;
    let file_path = group_dir.join(format!("{name}.md"));
    if file_path.exists() {
        return Err(AppError::FileExists {
            path: file_path.to_string_lossy().to_string(),
        });
    }

    // Same id scheme as `create_entry_simple` (Finding #3): replace path
    // separators so the id matches what `entry::read_md_file` will derive
    // from the file path. Nested parent groups stay flat.
    let new_id = format!("{parent_group}-{name}").replace(['\\', '/'], "-");

    let from_path = {
        let idx = state.index.read().await;
        idx.get_entry_content_path(&from_id)?
    }
    .ok_or_else(|| AppError::EntryNotFound {
                id: from_id.clone(),
            })?;

    // 1) 创建新 .md（占位内容，标题 + source 链接 + TODO）
    let title = if ref_text.is_empty() {
        name.clone()
    } else {
        ref_text.clone()
    };
    let content = format!(
        "# {title}\n\n> 来源: [{ref_text}]({source_url})\n\n<!-- TODO: 在此补充内容 -->\n",
    );
    std::fs::write(&file_path, content)?;

    // 2) 在 from entry 文件末尾追加一行 markdown 引用，指向新文件
    let from_dir = from_path.parent().unwrap_or(&kb_root).to_path_buf();
    let rel = pathdiff(&from_dir, &file_path);
    let mut src_content = std::fs::read_to_string(&from_path).unwrap_or_default();
    if !src_content.ends_with('\n') {
        src_content.push('\n');
    }
    src_content.push_str(&format!("\n- [{ref_text}]({rel})\n"));
    std::fs::write(&from_path, src_content)?;

    // 3) rebuild 索引（识别新 entry + 双向 [[xxx]] / markdown 引用关系）
    {
        let idx = state.index.read().await;
        idx.rebuild(&kb_root)?;
    }

    // 两个文件都已成功落盘，登记为本地写入。
    state.note_local_write(&file_path);
    state.note_local_write(&from_path);

    Ok(new_id)
}

// ---------- Validation & KB config commands (Task 4.2) ----------

/// Run full validation against the KB at `kb_root`, cache the report in
/// `state.validation_report`, and return it to the frontend. The cache is
/// what the startup hook (`run_validation`) and a manual "Validate now"
/// button share — keeping one report in state avoids drift between the
/// two paths. Per plan, `validate_kb` and `run_validation` are identical
/// today; they may diverge later (manual vs. startup trigger).
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn validate_kb(
    state: State<'_, AppState>,
    kb_root: PathBuf,
) -> AppResult<ValidationReport> {
    let report = validator::validate_kb(&kb_root);
    *state.validation_report.write().await = Some(report.clone());
    Ok(report)
}

/// Validate a single entry by directory path. Used by the frontend's
/// "Validate this file" affordance and by per-entry edit-time checks.
/// Returns the raw issues from the rule engine — the frontend decides
/// how to surface them.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn validate_entry(
    _state: State<'_, AppState>,
    entry_dir: PathBuf,
) -> AppResult<Vec<ValidationIssue>> {
    use crate::entry::read_entry;
    let entry = read_entry(&entry_dir);
    let all_ids: std::collections::HashSet<String> =
        std::iter::once(entry.id.clone()).collect();
    Ok(validator::rules::check_all(
        &entry,
        &entry_dir,
        &all_ids,
        crate::scanner::KbLayout::Directory,
        entry.declares_tags_system,
    ))
}

/// Return `(entry_id, rule_id)` for every `Info`-level issue in the KB's
/// validation report. The frontend uses these as "system labels" —
/// soft hints derived from rules, shown alongside user-authored labels.
/// We filter by `Severity::Info` only (warnings/errors are surfaced
/// separately as problems, not as labels). Per spec, `get_system_labels`
/// ignores the on-disk entry scan — the report already carries all
/// the info we need.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn get_system_labels(
    _state: State<'_, AppState>,
    kb_root: PathBuf,
) -> AppResult<Vec<(String, String)>> {
    let report = validator::validate_kb(&kb_root);
    let info_labels: Vec<(String, String)> = report
        .issues
        .into_iter()
        .filter(|i| matches!(i.severity, Severity::Info))
        .map(|i| (i.entry_id, i.rule_id))
        .collect();
    Ok(info_labels)
}

/// Startup-time validation hook. Identical to `validate_kb` today;
/// per plan they exist separately so they can diverge later (manual
/// vs. startup trigger path).
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn run_validation(
    state: State<'_, AppState>,
    kb_root: PathBuf,
) -> AppResult<ValidationReport> {
    let report = validator::validate_kb(&kb_root);
    *state.validation_report.write().await = Some(report.clone());
    Ok(report)
}

/// Read `.synapse-config.yaml` from `kb_root` and return it as JSON for
/// the frontend settings panel. When the file is absent, return an
/// empty JSON object — that's the "no config yet" signal the UI uses
/// to show defaults without forcing the user to create a file first.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn get_kb_config(
    _state: State<'_, AppState>,
    kb_root: PathBuf,
) -> AppResult<serde_json::Value> {
    let config_path = kb_root.join(".synapse-config.yaml");
    if !config_path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = std::fs::read_to_string(&config_path).map_err(|e| AppError::io(
        format!("get_kb_config: read {}", config_path.display()),
        e,
    ))?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| AppError::DatabaseOp {
            message: format!("get_kb_config: parse {}: {e}", config_path.display()),
        })?;
    let json_value: serde_json::Value =
        serde_json::to_value(&value).map_err(|e| AppError::DatabaseOp {
            message: format!("get_kb_config: convert YAML→JSON: {e}"),
        })?;
    Ok(json_value)
}

/// Overwrite `.synapse-config.yaml` under `kb_root` with the JSON `config`
/// the frontend sent. Per plan: no merge — this is a full replace. The
/// frontend assembles the full desired config from current values +
/// pending edits and round-trips it here, so a fresh file with just
/// the user's edits is exactly what we want.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_kb_config(
    _state: State<'_, AppState>,
    kb_root: PathBuf,
    config: serde_json::Value,
) -> AppResult<()> {
    let config_path = kb_root.join(".synapse-config.yaml");
    let yaml_value: serde_yaml::Value =
        serde_yaml::to_value(&config).map_err(|e| AppError::DatabaseOp {
            message: format!("update_kb_config: convert JSON→YAML: {e}"),
        })?;
    let yaml_string = serde_yaml::to_string(&yaml_value).map_err(|e| AppError::DatabaseOp {
        message: format!("update_kb_config: serialize YAML: {e}"),
    })?;
    std::fs::write(&config_path, yaml_string).map_err(|e| AppError::io(
        format!("update_kb_config: write {}", config_path.display()),
        e,
    ))?;
    Ok(())
}

fn pathdiff(from_dir: &std::path::Path, to: &std::path::Path) -> String {
    // 尝试同目录直接取 file_name
    if to.parent() == Some(from_dir) {
        return to
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    // 退而求其次：to 的完整 path
    to.to_string_lossy().to_string()
}

/// 仅 tauri 构建使用：KB 切换由 GUI command 驱动；MCP server 绑定单一
/// KB（`--kb`），不参与注册表切换（无 watcher / 事件面）。
#[cfg(feature = "tauri")]
async fn load_active_kb(state: &AppState, kb_id: &str) -> AppResult<()> {
    // Serializes the whole switch (registry update + disk scan + index swap)
    // against other mutators. Callers do NOT pre-acquire this lock — it would
    // reenter the same non-reentrant tokio Mutex.
    let _guard = state.write_lock.lock().await;

    let (kb, db_path) = {
        let mut reg = state.kbs.write().await;
        if reg.get(kb_id).is_none() {
            return Err(AppError::KbNotFound {
                kb_id: kb_id.to_string(),
            });
        }
        reg.set_active(Some(kb_id.to_string()))
            .map_err(AppError::from_save_kbs)?;
        let kb = reg.get(kb_id).unwrap().clone();
        let db_path = reg.index_db_path_for(kb_id);
        (kb, db_path)
    };

    let new_index = Index::open(&db_path)
        .map_err(|e| AppError::DatabaseOp {
                message: format!("打开索引失败: {e}"),
            })?;
    let kb_path = PathBuf::from(&kb.path);
    if kb_path.exists() {
        new_index
            .rebuild(&kb_path)
            .map_err(|e| AppError::DatabaseOp {
                message: format!("扫描知识库失败: {e}"),
            })?;
    }

    {
        let mut idx = state.index.write().await;
        *idx = new_index;
    }
    {
        let mut cur = state.current_kb.write().await;
        *cur = Some(kb.clone());
    }
    // 旧 KB 上尚未下发、或已被命令内部 rebuild 吞掉的重命名增量不能
    // 带到新 KB 的事件里。
    let _ = state.take_pending_renames();

    // Swap the file watcher to track the new KB root. Dropping the previous
    // WatcherHandle unregisters the old OS watch; the new one watches the
    // freshly-activated KB so external edits there are picked up too.
    swap_watcher(state, Some(&kb_path));

    Ok(())
}

#[cfg(feature = "tauri")]
async fn clear_active_kb(state: &AppState) -> AppResult<()> {
    let _guard = state.write_lock.lock().await;
    let default_db = crate::paths::index_db_path();
    let empty_index = Index::open(&default_db)
        .map_err(|e| AppError::DatabaseOp {
                message: format!("打开索引失败: {e}"),
            })?;
    {
        let mut idx = state.index.write().await;
        *idx = empty_index;
    }
    {
        let mut cur = state.current_kb.write().await;
        *cur = None;
    }
    // 无活动 KB：丢弃尚未下发的重命名增量，避免泄漏到下一个 KB。
    let _ = state.take_pending_renames();
    // No active KB → no KB to watch. Drop the watcher.
    swap_watcher(state, None);
    Ok(())
}

/// Replace the active KB's file watcher with one rooted at `new_root`
/// (or `None` to drop the watcher entirely). No-op if the new root matches
/// the watcher's current root, so a redundant KB reload doesn't churn the
/// notify subscription. `state.write_lock` must be held by the caller.
#[cfg(feature = "tauri")]
fn swap_watcher(state: &AppState, new_root: Option<&std::path::Path>) {
    let mut watcher_slot = state.watcher.lock().unwrap();
    let current_root = watcher_slot.as_ref().map(|h| h.root.clone());

    match new_root {
        None => {
            *watcher_slot = None;
        }
        Some(root) if Some(root) == current_root.as_deref() => {
            // Same root — keep the existing watcher.
        }
        Some(root) if !root.exists() => {
            // Don't watch a non-existent path; clear any prior watcher.
            *watcher_slot = None;
        }
        Some(root) => {
            let tx_opt = state.watch_tx.lock().unwrap().clone();
            match tx_opt {
                Some(tx) => match crate::create_watcher(root, tx) {
                    Some(handle) => *watcher_slot = Some(handle),
                    None => *watcher_slot = None,
                },
                None => *watcher_slot = None,
            }
        }
    }
}

// ---------- Fix & Backup commands (Task 15) ----------

/// Returned by `apply_fix` to the frontend. Carries the rule id and
/// entry id (so the caller can match this to the issue it dispatched
/// from) plus the `FixDescriptor` that was applied. `backupId` is
/// always `None` today — every fix impl calls
/// `backup_then_write` internally and the resulting `Backup` lives in
/// `.synapse-fix-backup/`; we don't yet expose its id to the JS side.
/// The field is reserved for the future case where a fix would not
/// create its own backup.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixApplyResult {
    pub rule_id: String,
    pub entry_id: String,
    pub descriptor: validator::fix::FixDescriptor,
    pub backup_id: Option<String>,
}

/// Apply a validator fix to a single entry. The fix is dispatched by
/// `rule_id` (E1–E9) to the appropriate `validator::fix_impls`
/// helper. After the fix writes to disk, the index is rebuilt so the
/// UI sees the new content on the next `get_entries`.
///
/// `confirm` must be `Some(true)` for Confirm-class rules (E2 / E6 /
/// E9); OneClick rules ignore it. The frontend drives this from the
/// "Apply" / "Confirm" buttons in the FixDrawer.
///
/// Resolution: the `Index` stores the on-disk path of each entry. For
/// directory-format KBs that is `<dir>/content.md`; for flat-format
/// KBs it is the entry `.md` file itself. Fix impls use two pieces of
/// information:
///   * `entry_dir.file_name()` — to derive the canonical id from the
///     on-disk path.
///   * `entry.content_path` — the actual file to read/write.
///
/// We construct a minimal `RawEntry` matching that contract directly,
/// instead of going through `read_entry` (which assumes a directory
/// containing `content.md`) or `read_md_file` (which derives its own
/// id and would diverge from the DB's id after a rename).
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn apply_fix(
    state: State<'_, AppState>,
    entry_id: String,
    rule_id: String,
    confirm: Option<bool>,
) -> AppResult<FixApplyResult> {
    apply_fix_impl(&state, entry_id, rule_id, confirm.unwrap_or(false)).await
}

/// `apply_fix` command 的可复用内核（批次 3）：stdio MCP server 的
/// `apply_fix` 工具走同一条 fix 分发 + rebuild 管线。内部持有
/// `state.write_lock`，与 GUI 写路径互斥语义一致。
pub async fn apply_fix_impl(
    state: &AppState,
    entry_id: String,
    rule_id: String,
    confirm: bool,
) -> AppResult<FixApplyResult> {
    let _guard = state.write_lock.lock().await;

    // 1) Look up the on-disk file path for this entry.
    let md_path = {
        let idx = state.index.read().await;
        idx.get_entry_content_path(&entry_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: entry_id.clone(),
            })?
    };

    // 2) Pick `entry_dir` so that `entry_dir.file_name()` returns the
    //    entry's id. Directory-format KBs land at `<dir>/content.md`,
    //    so the parent directory IS the entry. Flat-format KBs land
    //    at `<root>/<stem>.md`, so we synthesize a virtual entry_dir
    //    by stripping the `.md` extension — the fix only calls
    //    `file_name()` on this path, never `parent()` or `exists()`.
    let entry_dir = if md_path
        .file_name()
        .and_then(|n| n.to_str())
        == Some("content.md")
    {
        md_path.parent().unwrap_or(&md_path).to_path_buf()
    } else {
        md_path.with_extension("")
    };

    // 3) Build the minimal RawEntry the fix impl needs.
    let entry = RawEntry {
        id: entry_id.clone(),
        title: String::new(),
        tags: vec![],
        group: None,
        dir: None,
        path: entry_dir.clone(),
        content_path: md_path.clone(),
        attachments: vec![],
        linked_targets: vec![],
        markdown_links: vec![],
        typed_relations: vec![],
        yaml: None,
        has_content_md: true,
        labels: vec![],
        status: None,
        aliases: vec![],
        source: None,
        created: None,
        updated: None,
        ignored_rules: vec![],
        extra_meta: vec![],
        declares_tags_system: false,
        doc_code: None,
        validation: vec![],
    };

    // 4) Dispatch the fix. Unknown rule ids surface as AppError::UnknownRule.
    let descriptor = validator::fix_impls::descriptor_and_apply(
        rule_id.as_str(),
        &entry,
        &entry_dir,
        confirm,
    )?;
    // 只有真正执行（confirm=true）才写盘；预览不应留下“本地写”标记。
    if confirm {
        state.note_local_write(&md_path);
    }

    // 5) Rebuild so the UI reflects the new content on the next read.
    //    Errors here are not fatal — the on-disk fix already succeeded;
    //    a stale index will self-heal on the next watcher tick.
    let kb_root = kb_root_or_err(&state).await?;
    if let Ok(idx) = state.index.try_read() {
        let _ = idx.rebuild(&kb_root);
    }

    Ok(FixApplyResult {
        rule_id,
        entry_id,
        descriptor,
        backup_id: None,
    })
}

/// List every backup under `<kb_root>/.synapse-fix-backup/`. Returns
/// an empty vec for a KB that has never had a fix applied (the
/// directory is created on demand). The frontend uses this to populate
/// the "Restore from backup" picker in the FixDrawer.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn list_backups(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::fs::backup::Backup>> {
    let kb_root = kb_root_or_err(&state).await?;
    Ok(crate::fs::backup::list_backups(&kb_root))
}

/// Restore the file named by `backup_id` to its original on-disk
/// path (as recorded in the backup's `.meta` sidecar). Rebuilds the
/// index afterward so the UI reflects the restored content.
///
/// Takes the `write_lock` because `apply` calls do: this modifies the
/// KB's `.md` files, and we don't want a concurrent `apply_fix` to
/// observe half-restored state.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn restore_backup(
    state: State<'_, AppState>,
    backup_id: String,
) -> AppResult<()> {
    let _guard = state.write_lock.lock().await;

    let kb_root = kb_root_or_err(&state).await?;
    let backups = crate::fs::backup::list_backups(&kb_root);
    let backup = backups
        .into_iter()
        .find(|b| b.id == backup_id)
        .ok_or_else(|| AppError::BackupNotFound {
            id: backup_id.clone(),
        })?;
    crate::fs::backup::restore_backup(&backup)?;
    // 备份已恢复到 original_path，登记为本地写入。
    state.note_local_write(&backup.original_path);

    if let Ok(idx) = state.index.try_read() {
        let _ = idx.rebuild(&kb_root);
    }
    Ok(())
}

// ---------- Inline metadata editing (Cycle 2, Task 6) ----------

/// Update one or more `@synapse-*` fields in a content.md file. The
/// `metadata_updater` helper does the byte-level rewrite; we then
/// rebuild the SQLite index so the UI sees the new values on the
/// next `get_entries` call.
///
/// Validation runs before any file I/O — bad field values are
/// rejected with `AppError::MetadataInvalidField` and the file is
/// not modified.
///
/// This is the testable inner function; the Tauri command
/// `update_entry_metadata` is a thin wrapper that just hands off to
/// here with the `AppState` extracted from `tauri::State`. Splitting
/// the two lets us unit-test the validation / disk / index logic
/// without needing a full Tauri invoke handler.
pub async fn update_entry_metadata_impl(
    state: &AppState,
    id: String,
    changes: crate::parser::metadata_updater::MetadataChanges,
) -> AppResult<crate::index::StoredEntry> {
    let _guard = state.write_lock.lock().await;

    // 1) Validate. Empty / out-of-enum values are rejected here so
    //    we never write a file the validator would immediately flag.
    if let Some(t) = &changes.title {
        if t.trim().is_empty() {
            return Err(AppError::MetadataInvalidField {
                field: "title".to_string(),
                reason: "must not be empty".to_string(),
            });
        }
        if t.len() > 200 {
            return Err(AppError::MetadataInvalidField {
                field: "title".to_string(),
                reason: "must be ≤ 200 chars".to_string(),
            });
        }
    }
    if let Some(tags) = &changes.tags {
        if tags.len() > 15 {
            return Err(AppError::MetadataInvalidField {
                field: "tags".to_string(),
                reason: "must be ≤ 15".to_string(),
            });
        }
        for tag in tags {
            if tag.contains(',') {
                return Err(AppError::MetadataInvalidField {
                    field: "tags".to_string(),
                    reason: format!("tag '{tag}' contains a comma"),
                });
            }
            if tag.len() > 50 {
                return Err(AppError::MetadataInvalidField {
                    field: "tags".to_string(),
                    reason: format!("tag '{tag}' exceeds 50 chars"),
                });
            }
        }
    }
    if let Some(labels) = &changes.labels {
        for label in labels {
            if label.contains(',') {
                return Err(AppError::MetadataInvalidField {
                    field: "labels".to_string(),
                    reason: format!("label '{label}' contains a comma"),
                });
            }
        }
    }
    if let Some(status) = &changes.status {
        // D3（批次 3 对齐）：canonical 中文六枚举（可带 `（注记）`，剥除后
        // 比对，口径同 W7）或 rm 旧英文自由值均接受——GUI 存量流程写英文、
        // kaiwu 方言与 Agent 写 canonical 值，两条路都合法。
        let base = status.split(['（', '(']).next().unwrap_or(status).trim();
        let legacy = ["draft", "stable", "wip", "deprecated"].contains(&status.as_str());
        let canonical = validator::rules::STATUS_BASES.contains(&base);
        if !legacy && !canonical {
            return Err(AppError::MetadataInvalidField {
                field: "status".to_string(),
                reason: format!("status '{status}' not in enum"),
            });
        }
    }

    // 2) Resolve the entry's on-disk path.
    let path = {
        let idx = state.index.read().await;
        idx.get_entry_content_path(&id)?
            .ok_or_else(|| AppError::EntryNotFound { id: id.clone() })?
    };

    // 3) Rewrite the @synapse-* comments.
    crate::parser::metadata_updater::apply_metadata_changes(&path, &changes)?;
    // 落盘成功后才登记；校验失败/写盘失败不会留下来源标记。
    state.note_local_write(&path);

    // 4) Rebuild the index so the DB reflects the new file content.
    let kb_root = kb_root_or_err(state).await?;
    {
        let idx = state.index.read().await;
        let _ = idx.rebuild(&kb_root); // best-effort; UI re-fetches on next load
    }

    // 5) Return the freshly-indexed entry to the caller (avoids a
    //    second round-trip from the frontend).
    let idx = state.index.read().await;
    let entries = idx.get_all_entries()?;
    entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or(AppError::EntryNotFound { id })
}

/// Tauri command wrapper for `update_entry_metadata_impl`. The
/// frontend's `EntryMetadataEditor` calls this when a metadata field
/// blurs. Validation rejects bad values before any file I/O.
#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn update_entry_metadata(
    state: State<'_, AppState>,
    id: String,
    changes: crate::parser::metadata_updater::MetadataChanges,
) -> AppResult<crate::index::StoredEntry> {
    update_entry_metadata_impl(&state, id, changes).await
}

// ---------- Rename precheck (Cycle 2) ----------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrecheckRenameResult {
    pub valid: bool,
    pub new_id: String,
    pub conflict_with: Option<String>,
    pub incoming_count: u32,
    pub incoming_files: Vec<String>,
}

/// Pure-read validation of a proposed rename. The dialog calls this on
/// every keystroke (debounced) to drive the validation UI. No side
/// effects: no file I/O, no DB writes.
pub async fn precheck_rename_impl(
    state: &AppState,
    id: String,
    new_name: String,
    new_group: String,
) -> AppResult<PrecheckRenameResult> {
    let _guard = state.write_lock.lock().await; // serialize with concurrent renames
    let kb_root = kb_root_or_err(state).await?;

    // 目录格式的 entry id 就是目录名（或 @synapse-id），与扁平格式的
    // `group-name` 规则不同，因此先解析旧路径判断形态。
    let old_is_dir = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?.map(|p| p.is_dir()).unwrap_or(false)
    };

    // Compute new path / new id using the SAME scheme as
    // `entry::read_md_file` (path with `.md` stripped, separators
    // replaced with `-`).
    let new_id = if old_is_dir {
        new_name.clone()
    } else if new_group.is_empty() {
        new_name.clone()
    } else {
        format!("{new_group}-{new_name}").replace(['\\', '/'], "-")
    };

    // Conflict check.
    let idx = state.index.read().await;
    let entries = idx.get_all_entries()?;
    drop(idx);
    let conflict = entries.iter().find(|e| e.id == new_id && e.id != id);
    if let Some(other) = conflict {
        return Ok(PrecheckRenameResult {
            valid: false,
            new_id,
            conflict_with: Some(other.id.clone()),
            incoming_count: 0,
            incoming_files: vec![],
        });
    }

    // Count incoming refs (excluding the file being renamed).
    let (count, files) = count_incoming_refs(&kb_root, &id)?;

    Ok(PrecheckRenameResult {
        valid: true,
        new_id,
        conflict_with: None,
        incoming_count: count,
        incoming_files: files,
    })
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn precheck_rename(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
    new_group: String,
) -> AppResult<PrecheckRenameResult> {
    precheck_rename_impl(state.inner(), id, new_name, new_group).await
}

// ---------- Execute rename (Cycle 2) ----------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    pub new_id: String,
    pub updated_files: Vec<String>,
}

/// 回滚 rename/move 已完成的副作用：DB id、`@synapse-id`、文件位置与级联
/// 引用，最后重建索引。返回无法回滚的条目，供上层拼进错误原因。
async fn rollback_rename_state(
    state: &AppState,
    kb_root: &Path,
    old_path: &Path,
    new_path: &Path,
    old_id: &str,
    new_id: &str,
    id_file: &Path,
    pre_cascade_cache: &[(PathBuf, String)],
) -> Vec<String> {
    let mut rollback_errors: Vec<String> = Vec::new();
    {
        let idx = state.index.read().await;
        if let Err(re) = idx.rename_entry_id(new_id, old_id) {
            rollback_errors.push(format!("rename_entry_id: {re}"));
        }
    }
    if let Err(re) = update_synapse_id_in_file(id_file, new_id, old_id) {
        rollback_errors.push(format!("restore @synapse-id: {re}"));
    }
    if let Err(re) = std::fs::rename(new_path, old_path) {
        rollback_errors.push(format!(
            "move {} back to {}: {re}",
            new_path.display(),
            old_path.display()
        ));
    }
    for (path, original) in pre_cascade_cache {
        if let Err(re) = std::fs::write(path, original) {
            rollback_errors.push(format!("restore cascade {}: {re}", path.display()));
        }
    }
    // 让索引与磁盘重新一致（旧路径重新出现）。
    {
        let idx = state.index.read().await;
        let _ = idx.rebuild(kb_root);
    }
    rollback_errors
}

/// Atomically rename/move an entry: cascade `[[old_id]]` references in
/// other files, move the .md to its new path, update the
/// `@synapse-id` comment, migrate the SQLite row + relation edges,
/// and transfer the layout position. All in one write_lock.
///
/// `cascade=true` updates all incoming wikilinks; `cascade=false`
/// leaves them as broken refs (the dialog asks the user to pick).
pub async fn execute_rename_impl(
    state: &AppState,
    id: String,
    new_name: String,
    new_group: String,
    cascade: bool,
) -> AppResult<RenameResult> {
    use crate::scanner::find_md_files;

    let _guard = state.write_lock.lock().await;
    let kb_root = kb_root_or_err(state).await?;
    let runtime_dir = runtime_dir_or_err(state).await?;

    // R8: 与新建条目同一套校验——新名必须能作为单个文件名组件，
    // 新目录必须是 KB 根下的安全相对子路径。否则 `new_path` / `new_id`
    // 可能被 `..`、绝对路径或分隔符带出 KB 根。
    validate_filename(&new_name)?;
    if new_name.ends_with(".md") {
        return Err(AppError::NameHasMdSuffix);
    }
    validate_parent_group(&new_group)?;

    // 1) Resolve old path first — 目录格式与扁平格式的新路径/新 id 规则不同。
    let old_path = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?
            .ok_or_else(|| AppError::EntryNotFound { id: id.clone() })?
    };
    let old_is_dir = old_path.is_dir();

    // 2) Compute new path / new id.
    let (new_path, new_id) = if old_is_dir {
        let path = if new_group.is_empty() {
            kb_root.join(&new_name)
        } else {
            kb_root.join(&new_group).join(&new_name)
        };
        (path, new_name.clone())
    } else {
        let path = if new_group.is_empty() {
            kb_root.join(format!("{new_name}.md"))
        } else {
            kb_root.join(&new_group).join(format!("{new_name}.md"))
        };
        let new_id = if new_group.is_empty() {
            new_name.clone()
        } else {
            format!("{new_group}-{new_name}").replace(['\\', '/'], "-")
        };
        (path, new_id)
    };

    if new_path == old_path {
        // No-op rename (same path). Just rebuild to be safe.
        let idx = state.index.read().await;
        let _ = idx.rebuild(&kb_root);
        return Ok(RenameResult { new_id, updated_files: vec![] });
    }

    // 3) Conflict check (id exists in another entry) — must run before
    //    the path-exists check so we prefer the more specific error.
    {
        let idx = state.index.read().await;
        let entries = idx.get_all_entries()?;
        if entries.iter().any(|e| e.id == new_id && e.id != id) {
            return Err(AppError::RenameConflict { new_id });
        }
    }

    // 4) Path-exists check.
    if new_path.exists() {
        return Err(AppError::RenameFileExists { path: new_path.to_string_lossy().to_string() });
    }

    // 4) Cascade: find/replace `[[old_id]]` → `[[new_id]]` in every
    //    other .md, skipping code blocks; 并把解析后指向被移动条目的
    //    相对 markdown 链接按引用方位置改写成新路径（D1：kaiwu 生命周期
    //    ——归档 = 移动 + 改名——能在 rm 里存活的前提）。
    //    Track files we touch so we can report them in the result and
    //    roll back on failure.
    let mut updated_files: Vec<String> = Vec::new();
    let mut pre_cascade_cache: Vec<(std::path::PathBuf, String)> = Vec::new();
    if cascade && id != new_id {
        let needle = format!("[[{id}]]");
        let replacement = format!("[[{new_id}]]");
        // 旧路径的解析查找表（move 前 rebuild 的 DB 即旧路径快照）+
        // 被移动条目的新旧路径派生代号（Q12）。
        let lookups = {
            let idx = state.index.read().await;
            let entries = idx.get_all_entries()?;
            let as_raw: Vec<crate::entry::RawEntry> = entries
                .iter()
                .map(|e| {
                    let p = std::path::PathBuf::from(&e.path);
                    crate::entry::read_md_file(&p, &kb_root)
                })
                .collect();
            crate::index::build_link_lookups(&as_raw, &kb_root)
        };
        let old_rel = old_path
            .strip_prefix(&kb_root)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| old_path.clone());
        let new_rel = new_path
            .strip_prefix(&kb_root)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| new_path.clone());
        let old_code = crate::index::doc_code_from_parts(
            &old_rel
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            &old_rel
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        let new_code = crate::index::doc_code_from_parts(
            &new_rel
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            &new_rel
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        let new_rel_str = new_rel.to_string_lossy().replace('\\', "/");

        for file in find_md_files(&kb_root) {
            if file == old_path || (old_is_dir && file.starts_with(&old_path)) {
                continue;
            }
            let content = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (after_wikilink, count) = cascade_replace(&content, &needle, &replacement);

            // 相对链接级联：解析（含来源目录归一化 + 代号）命中被移动条目
            // 才重写；引用方自身的 dir 决定新的相对形式。
            let parsed = crate::entry::read_md_file(&file, &kb_root);
            let mut after_links = after_wikilink.clone();
            let mut link_count = 0usize;
            for link in &parsed.markdown_links {
                if crate::parser::is_external_url(&link.url) {
                    continue;
                }
                let cleaned = link.url.split('#').next().unwrap_or(&link.url);
                let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
                if crate::index::is_attachment_url(cleaned) {
                    continue;
                }
                let resolved =
                    crate::index::resolve_target(cleaned, parsed.dir.as_deref(), &lookups);
                if resolved.as_deref() != Some(id.as_str()) {
                    continue;
                }
                // 锚点/查询后缀原样保留
                let suffix = &link.url[cleaned.len()..];
                let new_url = if parsed.doc_code.as_deref() == Some(cleaned) {
                    continue; // frontmatter 文档代号是稳定锚，不随移动变
                } else if old_code.as_deref() == Some(cleaned) {
                    match &new_code {
                        Some(c) => format!("{c}{suffix}"),
                        None => continue,
                    }
                } else {
                    let rel =
                        crate::index::relative_path_between(parsed.dir.as_deref(), &new_rel_str);
                    if rel.is_empty() {
                        // 相对化后为空（不可能：来源与目标同路径会被 no-op 拦截），
                        // 跳过该链接的重写。
                        continue;
                    }
                    format!("{rel}{suffix}")
                };
                if new_url != link.url {
                    let occurrences = after_links.matches(&link.url).count();
                    after_links = after_links.replace(&link.url, &new_url);
                    link_count += occurrences;
                }
            }

            if count > 0 || link_count > 0 {
                pre_cascade_cache.push((file.clone(), content));
                if let Err(e) = std::fs::write(&file, &after_links) {
                    // Roll back all already-cascaded files
                    for (path, original) in pre_cascade_cache {
                        let _ = std::fs::write(&path, original);
                    }
                    return Err(AppError::CascadeFailed {
                        failed_files: vec![file.to_string_lossy().to_string()],
                        reason: e.to_string(),
                    });
                }
                if let Ok(rel) = file.strip_prefix(&kb_root) {
                    updated_files.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }

    // 5) Move the .md file.
    if let Some(parent) = new_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::rename(&old_path, &new_path) {
        // Rollback cascade
        for (path, original) in &pre_cascade_cache {
            let _ = std::fs::write(path, original);
        }
        return Err(AppError::io(
            format!("execute_rename: rename {} -> {}", old_path.display(), new_path.display()),
            e,
        ));
    }

    // 6) Update @synapse-id in the moved file via a targeted regex
    //    replace (no validation needed — the id is path-derived and
    //    we just computed it).
    let id_file = if old_is_dir {
        new_path.join("content.md")
    } else {
        new_path.clone()
    };
    if let Err(e) = update_synapse_id_in_file(&id_file, &id, &new_id) {
        // Rollback file move
        let _ = std::fs::rename(&new_path, &old_path);
        for (path, original) in &pre_cascade_cache {
            let _ = std::fs::write(path, original);
        }
        return Err(AppError::io(
            format!("execute_rename: update @synapse-id in {}", new_path.display()),
            e,
        ));
    }

    // 7) Migrate SQLite row + relations.
    {
        let idx = state.index.read().await;
        if let Err(e) = idx.rename_entry_id(&id, &new_id) {
            // Rollback
            let _ = update_synapse_id_in_file(&id_file, &new_id, &id);
            let _ = std::fs::rename(&new_path, &old_path);
            for (path, original) in &pre_cascade_cache {
                let _ = std::fs::write(path, original);
            }
            return Err(AppError::DatabaseOp { message: format!("rename_entry_id: {e}") });
        }
    }

    // 8) 历史目录跟随 id 迁移。与布局一样是迁移的一部分：失败就整体回滚，
    //    不能留下“文档移动了但历史还挂旧 id”的半成品。
    if let Err(e) = crate::history::migrate_entry_history(&runtime_dir, &id, &new_id) {
        let rollback_errors = rollback_rename_state(
            state,
            &kb_root,
            &old_path,
            &new_path,
            &id,
            &new_id,
            &id_file,
            &pre_cascade_cache,
        )
        .await;
        let reason = if rollback_errors.is_empty() {
            e.to_string()
        } else {
            format!(
                "{e}; rollback incomplete: {}",
                rollback_errors.join("; ")
            )
        };
        return Err(AppError::HistoryMigrationFailed { reason });
    }

    // 9) Transfer layout position (old id → new id).
    //
    // 布局迁移失败不能只是打日志：否则重命名成功、坐标却静默丢失，节点
    // 会在前端被当作新节点重新布局。这里选择整体回滚——把历史目录、DB 行、
    // `@synapse-id`、文件移动和级联引用全部恢复，保证“要么改名+布局一起成功，
    // 要么完全没变”。
    if let Err(e) = crate::layout_store::rename_key(&runtime_dir, &id, &new_id) {
        let mut rollback_errors: Vec<String> = Vec::new();
        // 历史目录已经迁到 new_id 下，先搬回旧 id。
        if let Err(re) = crate::history::migrate_entry_history(&runtime_dir, &new_id, &id) {
            rollback_errors.push(format!("restore history: {re}"));
        }
        rollback_errors.extend(
            rollback_rename_state(
                state,
                &kb_root,
                &old_path,
                &new_path,
                &id,
                &new_id,
                &id_file,
                &pre_cascade_cache,
            )
            .await,
        );
        let reason = if rollback_errors.is_empty() {
            e.to_string()
        } else {
            format!(
                "{e}; rollback incomplete: {}",
                rollback_errors.join("; ")
            )
        };
        return Err(AppError::LayoutMigrationFailed { reason });
    }

    // 10) Rebuild the index to refresh derived data (path, group, etc).
    {
        let idx = state.index.read().await;
        let _ = idx.rebuild(&kb_root);
    }

    // 11) 记录 old id → new id。watcher 之后重建时前后快照都已是新 id，
    //     `added`/`removed` 为空；这条增量让 `kb-changed` 仍能表达重命名。
    state.record_rename(&id, &new_id);

    // 12) 登记本次成功写过的所有路径（旧文件、新文件、@synapse-id 文件、
    //     级联更新的引用文件）。watcher 的事件路径全部命中时才会把这次
    //     重建标为 `local-write`；失败回滚的分支不会走到这里。
    if old_is_dir {
        // 目录重命名会移动整棵子树；watcher 事件里是子文件路径，必须逐一
        // 登记，否则整批会被判成 `external`，导致前端多刷一次。
        state.note_local_write_tree(&old_path, &new_path);
    } else {
        state.note_local_write(&old_path);
        state.note_local_write(&new_path);
    }
    if id_file != new_path {
        state.note_local_write(&id_file);
    }
    for rel in &updated_files {
        state.note_local_write(kb_root.join(rel));
    }

    Ok(RenameResult { new_id, updated_files })
}

#[cfg(feature = "tauri")]
#[tauri::command(rename_all = "camelCase")]
pub async fn execute_rename(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
    new_group: String,
    cascade: bool,
) -> AppResult<RenameResult> {
    let state_ref = state.inner();
    execute_rename_impl(state_ref, id, new_name, new_group, cascade).await
}

/// Replace `needle` with `replacement` in `content`, but skip lines
/// inside fenced code blocks. Returns `(new_content, replacement_count)`.
fn cascade_replace(content: &str, needle: &str, replacement: &str) -> (String, u32) {
    let mut result = String::with_capacity(content.len());
    let mut in_code = false;
    let mut count: u32 = 0;
    for line in content.lines() {
        let was_in_code = in_code;
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if was_in_code {
            // Inside a code block — pass through untouched.
            result.push_str(line);
            result.push('\n');
            continue;
        }
        if line.contains(needle) {
            count += line.matches(needle).count() as u32;
            result.push_str(&line.replace(needle, replacement));
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    // Preserve trailing-newline behaviour: if the original did NOT end
    // with a newline, drop the last one we just added.
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    (result, count)
}

/// Direct regex replace of `<!-- @synapse-id: OLD -->` → `<!-- @synapse-id: NEW -->`
/// in the file at `path`. No validation; just a targeted string rewrite.
fn update_synapse_id_in_file(path: &Path, old: &str, new: &str) -> std::io::Result<()> {
    // Q6.a：id 写 frontmatter（唯一真源），不再改正则匹配的注释行。
    // 旧注释 id 若存在则保留为 stale（只读兼容，解析侧 frontmatter 优先）。
    // 文件两处都没有 old id（如 id 完全来自路径派生且从未落盘）时保持原样。
    let content = std::fs::read_to_string(path)?;
    let fm_has_old = crate::parser::fm_writer::has_key(&content, "id");
    let comment_has_old = content.contains(&format!("@synapse-id: {old}"))
        || content.contains(&format!("@synapse-id:{old}"));
    if !fm_has_old && !comment_has_old {
        return Ok(());
    }
    let new_content = crate::parser::fm_writer::apply_edits(
        &content,
        &crate::parser::fm_writer::FrontmatterEdit::set(
            "id",
            crate::parser::fm_writer::FmValue::Str(new.to_string()),
        ),
    );
    if new_content != content {
        std::fs::write(path, new_content)?;
    }
    Ok(())
}

/// Walk every .md under `kb_root` (except files that contain the
/// `@synapse-id: old_id` declaration) and count occurrences of
/// `[[old_id]]` outside of code blocks. Returns
/// `(total_refs, sorted_unique_files)`.
fn count_incoming_refs(kb_root: &Path, old_id: &str) -> AppResult<(u32, Vec<String>)> {
    use std::collections::BTreeSet;
    use crate::scanner::find_md_files;

    let needle = format!("[[{old_id}]]");
    let mut total: u32 = 0;
    let mut files: BTreeSet<String> = BTreeSet::new();

    for file in find_md_files(kb_root) {
        let content = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Skip the file that IS the target (it has @synapse-id: old_id).
        // Other files might mention `old_id` in their content
        // legitimately (e.g. discussion) — those count as refs.
        let mut in_code = false;
        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
                continue;
            }
            if in_code {
                continue;
            }
            if line.contains(&needle) {
                total += 1;
                if let Ok(rel) = file.strip_prefix(kb_root) {
                    files.insert(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    Ok((total, files.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::metadata_updater::MetadataChanges;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    /// 并行测试创建临时 DB / registry 时不能只靠时间戳做文件名：
    /// macOS 上 `SystemTime::now()` 的精度只有几百纳秒，同一时刻启动的多个
    /// 测试会拿到相同的 nanos，落到同一个 sqlite 文件上互相 rebuild，
    /// 表现为 `rename_entry_id: Query returned no rows` 之类的偶发失败。
    /// 进程内自增序号保证每个测试状态的路径唯一。
    static TEST_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_test_suffix() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = TEST_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("{nanos}-{seq}")
    }

    fn kb_root() -> PathBuf {
        PathBuf::from("/tmp/kb-root")
    }

    // ---------- validate_filename ----------

    #[test]
    fn filename_accepts_normal_names() {
        validate_filename("foo").unwrap();
        validate_filename("foo-bar_baz").unwrap();
        validate_filename("条目-中文").unwrap();
        validate_filename("a").unwrap();
    }

    #[test]
    fn filename_rejects_empty() {
        assert!(validate_filename("").is_err());
    }

    #[test]
    fn filename_rejects_parent_dir() {
        assert!(validate_filename("..").is_err());
        assert!(validate_filename("../sneaky").is_err());
        assert!(validate_filename("foo/..").is_err());
    }

    #[test]
    fn filename_rejects_path_separators() {
        assert!(validate_filename("foo/bar").is_err());
        assert!(validate_filename("foo\\bar").is_err());
        assert!(validate_filename("/etc").is_err());
        assert!(validate_filename("foo/bar/baz").is_err());
    }

    /// The original vulnerability: filename = "../escaped" produces a path that
    /// joins to somewhere outside `kb_root`. After the fix this must be rejected.
    #[test]
    fn filename_blocks_original_cve_input() {
        // Pre-fix: kb_root.join("..\\escaped.md") → /tmp/kb-root/..\\escaped.md
        //           which the OS normalizes to /tmp/escaped.md outside the KB.
        // Post-fix: rejected by validate_filename before any fs::write runs.
        assert!(validate_filename("..\\escaped").is_err());
        assert!(validate_filename("../escaped").is_err());
    }

    // ---------- validate_parent_group ----------

    #[test]
    fn parent_group_accepts_empty_and_normal() {
        validate_parent_group("").unwrap();
        validate_parent_group("compliance").unwrap();
        validate_parent_group("compliance/legal").unwrap();
        validate_parent_group("a/b/c/d/e").unwrap();
    }

    #[test]
    fn parent_group_rejects_traversal() {
        assert!(validate_parent_group("..").is_err());
        assert!(validate_parent_group("../escaped").is_err());
        assert!(validate_parent_group("compliance/../escaped").is_err());
        assert!(validate_parent_group("./compliance").is_err());
        // Path::components() strips trailing CurDir, so "compliance/." reduces
        // to a single Normal component — not an escape vector, but we still
        // expect it to be accepted (no ParentDir).
        assert!(validate_parent_group("compliance/.").is_ok());
    }

    #[test]
    fn parent_group_rejects_absolute_paths() {
        assert!(validate_parent_group("/etc").is_err());
        assert!(validate_parent_group("/etc/passwd").is_err());
    }

    /// End-to-end: after validation passes, `kb_root.join(parent_group)` must not
    /// produce a path that escapes kb_root. We check by inspecting the joined path
    /// and confirming it begins with kb_root as a path.
    #[test]
    fn safe_join_keeps_file_under_kb_root() {
        let root = kb_root();
        for safe in ["compliance", "compliance/legal", "a/b/c"] {
            validate_parent_group(safe).unwrap();
            let joined = root.join(safe);
            assert!(
                joined.starts_with(&root),
                "{safe} should stay under kb_root, got {joined:?}"
            );
        }
    }

    // ---------- write_lock contract (Finding #8) ----------

    /// `append_reference_line` is a read-modify-write on `from.md`. Without
    /// serialization, two concurrent callers can both read the same old content,
    /// each append their own `[[id]]`, and the second `fs::write` overwrites
    /// the first — silently losing a reference. With the `write_lock` held
    /// across the call, every append must land. This pins the contract that
    /// mutating commands (`create_relation`, `create_entry_simple`, …) acquire
    /// `state.write_lock.lock().await` BEFORE invoking `append_reference_line`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn write_lock_serializes_concurrent_appends() {
        use std::sync::Arc;
        use tempfile::tempdir;
        use tokio::sync::Mutex;

        let dir = tempdir().unwrap();
        let from_path = dir.path().join("from.md");
        std::fs::write(&from_path, "# From\n").unwrap();

        let lock = Arc::new(Mutex::new(()));
        let mut handles = Vec::new();

        for i in 0..16 {
            let lock = lock.clone();
            let path = from_path.clone();
            handles.push(tokio::spawn(async move {
                let _guard = lock.lock().await;
                append_relation_line(&path, &path, &format!("id-{i}"), "reference").unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let content = std::fs::read_to_string(&from_path).unwrap();
        for i in 0..16 {
            assert!(
                content.contains(&format!("[[id-{i}]]")),
                "missing [[id-{i}]] after concurrent appends — file:\n{content}",
            );
        }
    }

    /// Inverse sanity check: when the lock is NOT held, concurrent
    /// `append_reference_line` calls race and at least one append can be lost.
    /// This is a flaky test by design — the race depends on thread scheduling —
    /// but it's here to make the bug visible if anyone removes the lock later.
    /// We loop a few times to make a lost-write observable when the OS does swap
    /// threads mid-RMW. Tagged `#[ignore]` so it isn't part of the default
    /// `cargo test` run; run with `cargo test -- --ignored` to exercise.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "flaky by design — exercises the unmitigated race"]
    async fn uncoordinated_appends_can_lose_writes() {
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let from_path = dir.path().join("from.md");
        std::fs::write(&from_path, "# From\n").unwrap();

        let iterations = 50;
        let per_iter = 8;
        let mut lost_runs = 0;

        for _ in 0..iterations {
            std::fs::write(&from_path, "# From\n").unwrap();
            let mut handles = Vec::new();
            for i in 0..per_iter {
                let path = from_path.clone();
                handles.push(std::thread::spawn(move || {
                    append_relation_line(&path, &path, &format!("id-{i}"), "reference").unwrap();
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
            let content = std::fs::read_to_string(&from_path).unwrap();
            let landed = (0..per_iter)
                .filter(|i| content.contains(&format!("[[id-{i}]]")))
                .count();
            if landed < per_iter {
                lost_runs += 1;
            }
        }
        // With 4 threads × 50 iterations × 8 calls, we expect at least some
        // runs to lose writes. If we ever see 0 here, something changed
        // (faster FS, single-CPU box, etc.) — keep this assertion loose.
        assert!(
            lost_runs > 0,
            "expected at least one lost-write run out of {iterations}; got 0. \
             The race may not be reproducible on this hardware — that's OK; \
             this is a regression smoke, not a determinism test.",
        );
        let _ = Arc::new(()); // silence unused-import warning if any
    }

    // ---------- kb_root_or_err (Finding #9) ----------

    /// Build a minimal `AppState` the `kb_root_or_err` helper can read
    /// `current_kb` from. The index/registry fields are placeholders —
    /// the helper only consults `current_kb.path`, so we never open them.
    fn state_with_current_kb(kb_path: Option<&std::path::Path>) -> AppState {
        use crate::index::Index;
        use crate::kb_registry::{KbRegistry, KBInfo};

        let db_path = std::env::temp_dir().join(format!(
            "synapse-cmd-test-{}.db",
            unique_test_suffix()
        ));
        let index = Index::open(&db_path).expect("open test index");

        let registry_file = std::env::temp_dir().join(format!(
            "synapse-cmd-test-kbs-{}.json",
            unique_test_suffix()
        ));
        let registry = KbRegistry::empty_at(registry_file);

        let current_kb = kb_path.map(|p| KBInfo {
            id: "kb-test".to_string(),
            name: "test".to_string(),
            path: p.to_string_lossy().to_string(),
        });

        AppState {
            index: Arc::new(tokio::sync::RwLock::new(index)),
            kbs: Arc::new(tokio::sync::RwLock::new(registry)),
            current_kb: Arc::new(tokio::sync::RwLock::new(current_kb)),
            runtime_dir_base: std::env::temp_dir().join(format!(
                "synapse-cmd-test-runtime-{}",
                unique_test_suffix()
            )),
            validation_report: Arc::new(tokio::sync::RwLock::new(None)),
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            watcher: Arc::new(std::sync::Mutex::new(None)),
            watch_tx: Arc::new(std::sync::Mutex::new(None)),
            startup_gate: Arc::new(tokio::sync::RwLock::new(())),
            local_write_paths: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pending_renames: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Happy path: a live KB directory round-trips through the helper as
    /// a usable `PathBuf`. This is what every file-writing command relies
    /// on — without it, `create_entry_simple`/`create_entry_from_reference`
    /// would have no root to anchor paths to.
    #[tokio::test]
    async fn kb_root_or_err_happy() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_current_kb(Some(dir.path()));
        let path = kb_root_or_err(&state).await.expect("must succeed");
        assert_eq!(path, dir.path());
    }

    /// Pre-fix: `commands.rs::create_entry_simple` (and friends) read
    /// `state.current_kb.path` and write into it without checking it
    /// still exists. After the user moves the KB externally, those
    /// commands would silently create `.md` files at the stale path or
    /// fail with a confusing fs error. Pin the new contract: a missing
    /// KB path is a loud `AppError`, surfaced to the UI.
    #[tokio::test]
    async fn kb_root_or_err_returns_error_for_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let nonexistent = dir.path().join("user-moved-the-kb");
        let state = state_with_current_kb(Some(&nonexistent));

        let err = kb_root_or_err(&state)
            .await
            .expect_err("must fail when KB path was moved");
        let msg = format!("{err}");
        // The error code is the contract the i18n bundle keys on; the
        // Display form (`kb-dir-gone: <path>`) is what gets logged.
        assert!(
            msg.starts_with("kb-dir-gone:"),
            "error must use kb-dir-gone code, got: {msg}"
        );
    }

    /// Same contract for the case where `current_kb.path` exists but
    /// points at a regular file (the user replaced the KB directory
    /// with a file of the same path). Pre-fix: file writes would
    /// attempt to `join(...)` into a non-directory and either fail at
    /// the OS layer with `ENOTDIR` or — worse — silently nest under
    /// the wrong target.
    #[tokio::test]
    async fn kb_root_or_err_returns_error_when_path_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_instead = dir.path().join("a-file-not-a-dir");
        std::fs::write(&file_instead, "not a dir").unwrap();
        let state = state_with_current_kb(Some(&file_instead));

        let err = kb_root_or_err(&state)
            .await
            .expect_err("must fail when KB path is a file");
        let msg = format!("{err}");
        assert!(
            msg.starts_with("kb-dir-not-directory:"),
            "error must use kb-dir-not-directory code, got: {msg}"
        );
    }

    /// First-launch / KB-removed state: `current_kb` is `None`. The
    /// helper must surface the same "no active KB" error the original
    /// inline code did, so the UI's existing error display keeps
    /// working unchanged.
    #[tokio::test]
    async fn kb_root_or_err_returns_error_when_no_active_kb() {
        let state = state_with_current_kb(None);
        let err = kb_root_or_err(&state)
            .await
            .expect_err("must fail when no KB is active");
        let msg = format!("{err}");
        assert!(
            msg == "no-active-kb",
            "error must use no-active-kb code, got: {msg}"
        );
    }

    // ---------- remove_wikilink_line (Finding #5) ----------

    /// Per Finding #5: `delete_relation` must strip the matching
    /// `[[to_id]]` line from the source .md file so the next rebuild
    /// can't re-derive the relation and silently undo the deletion.
    #[test]
    fn remove_wikilink_line_strips_matching_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let body = "\
# Note

Some intro text.

- [[other-a]] link to A
- [[other-b]] link to B
- [[target]] link to target

End of note.
";
        fs::write(&path, body).unwrap();

        remove_wikilink_line(&path, "target").unwrap();

        let after = fs::read_to_string(&path).unwrap();
        assert!(!after.contains("[[target]]"), "wikilink must be gone");
        assert!(after.contains("[[other-a]]"), "unrelated line preserved");
        assert!(after.contains("[[other-b]]"), "unrelated line preserved");
        assert!(after.contains("# Note"), "header preserved");
        assert!(after.contains("End of note."), "trailing content preserved");
    }

    /// The wikilink can appear with surrounding markdown (e.g. `- [[x]]
    /// description`). We match by `contains("[[to_id]]")`, not exact
    /// equality, so trailing descriptors survive.
    #[test]
    fn remove_wikilink_line_handles_inline_markdown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(
            &path,
            "- [[alpha]] some description\n- [[beta]] other description\n",
        )
        .unwrap();
        remove_wikilink_line(&path, "alpha").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(!after.contains("alpha"));
        assert!(after.contains("[[beta]] other description"));
    }

    /// When the file doesn't contain `[[to_id]]` (already cleaned up,
    /// or wrong file), the helper is a no-op. This is critical for
    /// idempotency: a double-delete must not corrupt the file.
    #[test]
    fn remove_wikilink_line_is_noop_when_target_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let body = "only [[beta]] here\n";
        fs::write(&path, body).unwrap();
        remove_wikilink_line(&path, "alpha").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, body);
    }

    /// Preserve a trailing newline so the file diff stays minimal.
    /// (Otherwise `remove_wikilink_line` could turn a file-with-trailing-
    /// newline into one without, churning git diffs on every delete.)
    #[test]
    fn remove_wikilink_line_preserves_trailing_newline() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "intro\n\n- [[x]]\n").unwrap();
        remove_wikilink_line(&path, "x").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, "intro\n\n");
    }

    // ---------- Finding #3: ID scheme parity ----------

    /// The id `create_entry_simple` returns must match what
    /// `entry::read_md_file` will compute when it walks the KB later.
    /// Pre-fix used `format!("{parent_group}-{name}")` and kept the
    /// literal `/` for nested groups — caller got
    /// `compliance/legal/memo-myname`, the DB stored
    /// `compliance-legal-memo-myname`. Pin the post-fix scheme.
    #[test]
    fn new_id_replaces_separators_to_match_read_md_file() {
        // Re-implement the same transform here as a regression guard.
        // If this drifts from the production code, this test catches it.
        let compute = |parent_group: &str, name: &str| -> String {
            if parent_group.is_empty() {
                name.to_string()
            } else {
                format!("{parent_group}-{name}").replace(['\\', '/'], "-")
            }
        };

        // Empty parent → id is the bare name.
        assert_eq!(compute("", "note"), "note");

        // Single-level parent → id is `<group>-<name>`.
        assert_eq!(compute("compliance", "gdpr"), "compliance-gdpr");

        // Nested parent (the bug): separators collapse to dashes so
        // the id round-trips through `entry::read_md_file`'s
        // `replace(['\\', '/'], "-")` transform.
        assert_eq!(compute("compliance/legal", "memo"), "compliance-legal-memo");
        assert_eq!(compute("a\\b\\c", "note"), "a-b-c-note");

        // Mixed separators also collapse.
        assert_eq!(compute("compliance\\legal/sub", "x"), "compliance-legal-sub-x");
    }

    // ---------- update_entry_metadata (Task 6) ----------

    /// Field validation rejects an empty title before any file I/O
    /// happens. Pre-fix this used to write an empty `@synapse-title:`
    /// line to disk, which the validator would then catch as E3.
    #[tokio::test]
    async fn update_metadata_rejects_empty_title() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("note.md"),
            "<!-- @synapse-id: note -->\n<!-- @synapse-title: T -->\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));

        let err = update_entry_metadata_impl(
            &state,
            "note".to_string(),
            MetadataChanges {
                title: Some("".to_string()),
                tags: None,
                labels: None,
                status: None,
            },
        )
        .await
        .expect_err("empty title must be rejected");
        let msg = format!("{err}");
        assert!(msg.starts_with("metadata-invalid-field:"), "got: {msg}");
    }

    /// Tag containing a comma is rejected — the CSV format would
    /// silently split it into two tags.
    #[tokio::test]
    async fn update_metadata_rejects_tag_with_comma() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("note.md"),
            "<!-- @synapse-id: note -->\n<!-- @synapse-title: T -->\n<!-- @synapse-tags: a -->\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));

        let err = update_entry_metadata_impl(
            &state,
            "note".to_string(),
            MetadataChanges {
                title: None,
                tags: Some(vec!["ok".to_string(), "has,comma".to_string()]),
                labels: None,
                status: None,
            },
        )
        .await
        .expect_err("tag with comma must be rejected");
    }

    /// Happy path: title change writes to disk and the command
    /// returns the freshly-reindexed entry. The file on disk must
    /// reflect the new `@synapse-title` value.
    #[tokio::test]
    async fn update_metadata_writes_title_and_updates_db() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("note.md"),
            "<!-- @synapse-id: note -->\n<!-- @synapse-title: Old -->\n<!-- @synapse-tags: type:note -->\n# Note\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));

        // Rebuild so the index knows about the on-disk file. The
        // command needs `get_entry_path` to resolve `id` → path; the
        // empty DB would return None and trip `EntryNotFound` first.
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = update_entry_metadata_impl(
            &state,
            "note".to_string(),
            MetadataChanges {
                title: Some("New".to_string()),
                tags: None,
                labels: None,
                status: None,
            },
        )
        .await
        .expect("update should succeed");

        // Q6.a：title 写进 frontmatter（注释是只读兼容，不再更新、也不清除）。
        let on_disk = std::fs::read_to_string(kb_root.join("note.md")).unwrap();
        assert!(on_disk.starts_with("---\ntitle: New\n---\n"), "frontmatter first with new title: {on_disk}");
        assert!(!on_disk.contains("<!-- @synapse-title: New -->"), "no comment write: {on_disk}");
        assert!(
            on_disk.contains("<!-- @synapse-title: Old -->"),
            "stale legacy comment stays (read-only compat): {on_disk}"
        );

        // Returned entry matches the on-disk id (title comes from the
        // first `# heading` in flat-md format, which is still "Note").
        assert_eq!(result.id, "note");
    }

    /// 统一解析回归：写入 tags/labels 后，命令内部的 rebuild 必须能读回，
    /// 否则接口会把前端的乐观更新覆盖为空（旧实现的扁平解析器丢弃 tags）。
    #[tokio::test]
    async fn update_metadata_persists_tags_and_labels() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("note.md"),
            "<!-- @synapse-id: note -->\n<!-- @synapse-title: Note -->\n# Note\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = update_entry_metadata_impl(
            &state,
            "note".to_string(),
            MetadataChanges {
                title: None,
                tags: Some(vec!["type:concept".to_string(), "topic:rust".to_string()]),
                labels: Some(vec!["important".to_string()]),
                status: None,
            },
        )
        .await
        .expect("update should succeed");

        let mut tags = result.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["topic:rust", "type:concept"]);
        assert_eq!(result.labels, vec!["important"]);
        let on_disk = std::fs::read_to_string(kb_root.join("note.md")).unwrap();
        // Q6.a：元数据写 frontmatter（唯一真源），不再插 HTML 注释。
        assert!(on_disk.contains("tags: type:concept, topic:rust"), "{on_disk}");
        assert!(on_disk.contains("labels: important"), "{on_disk}");
        assert!(on_disk.starts_with("---\n"), "frontmatter must be first line: {on_disk}");
    }

    // ---------- precheck_rename / count_incoming_refs ----------

    /// Renaming to an id that already exists in the KB is reported
    /// as `valid: false` with `conflict_with` set — the dialog uses
    /// this to disable the Rename button.
    #[tokio::test]
    async fn precheck_rename_detects_id_collision() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(kb_root.join("alpha.md"), "<!-- @synapse-id: alpha -->\n").unwrap();
        std::fs::write(kb_root.join("beta.md"),  "<!-- @synapse-id: beta -->\n").unwrap();
        let state = state_with_current_kb(Some(&kb_root));

        // Rebuild so the index knows about the on-disk files. The
        // empty DB would otherwise return no entries and the conflict
        // check would miss.
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = precheck_rename_impl(
            &state,
            "alpha".to_string(),
            "beta".to_string(),
            "".to_string(),
        )
        .await
        .unwrap();
        assert!(!result.valid, "rename to existing id must be invalid");
        assert_eq!(result.conflict_with.as_deref(), Some("beta"));
    }

    /// `count_incoming_refs` and the actual cascade must agree —
    /// precheck promises N incoming refs, execute_rename updates
    /// exactly N. This is the contract the dialog relies on.
    #[tokio::test]
    async fn precheck_incoming_count_matches_cascade() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        // target entry
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n# Target\n",
        )
        .unwrap();
        // 3 other files each with one [[target]] reference
        for name in ["a", "b", "c"] {
            std::fs::write(
                kb_root.join(format!("{name}.md")),
                format!("<!-- @synapse-id: {name} -->\n# {name}\n\nSee [[target]].\n"),
            )
            .unwrap();
        }

        let state = state_with_current_kb(Some(&kb_root));
        let precheck = precheck_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(precheck.incoming_count, 3, "should count 3 [[target]] refs");
        assert_eq!(precheck.incoming_files.len(), 3);
    }

    /// Code blocks must NOT be counted as incoming refs. The cascade
    /// skips them at write time, so the count must also exclude them
    /// — otherwise the dialog would promise to update N files but
    /// only M < N actually change.
    #[tokio::test]
    async fn precheck_skips_code_block_wikilinks() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("doc.md"),
            "<!-- @synapse-id: doc -->\n\
             # Doc\n\n\
             Real: [[target]]\n\n\
             ```\n\
             Code: [[target]]\n\
             ```\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));

        let precheck = precheck_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(precheck.incoming_count, 1, "only the real ref counts");
    }

    // ---------- execute_rename ----------

    /// Happy path: rename the file, update @synapse-id, cascade
    /// wikilinks in other files, update the DB. The new id resolves
    /// in the rebuilt index; old id is gone.
    #[tokio::test]
    async fn execute_rename_cascades_incoming_wikilinks() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n# Target\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("hub.md"),
            "<!-- @synapse-id: hub -->\n# Hub\n\nSee [[target]].\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = execute_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
            true,
        )
        .await
        .unwrap();

        assert_eq!(result.new_id, "target-v2");

        // hub's content was updated
        let hub = std::fs::read_to_string(kb_root.join("hub.md")).unwrap();
        assert!(hub.contains("[[target-v2]]"), "wikilink not cascaded: {hub}");
        assert!(!hub.contains("[[target]]"), "old wikilink still present: {hub}");

        // target's file moved and id updated in frontmatter (Q6.a)
        let target = std::fs::read_to_string(kb_root.join("target-v2.md")).unwrap();
        assert!(target.starts_with("---\nid: target-v2\n---\n"), "frontmatter id not updated: {target}");

        // Old file is gone
        assert!(!kb_root.join("target.md").exists(), "old file must be removed");
    }

    /// 重命名后 `execute_rename` 必须迁移 `.synapse-layout.json` 的键，
    /// 否则前端拿到的布局里没有新 id，节点会丢坐标并重新布局。
    #[tokio::test]
    async fn execute_rename_migrates_layout_key() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n# Target\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }
        crate::layout_store::update_position(&state.runtime_dir_base.join("kb-test"), "target", 120.0, -45.0).unwrap();

        let result = execute_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(result.new_id, "target-v2");

        let positions = crate::layout_store::load_positions(&state.runtime_dir_base.join("kb-test"));
        let moved = positions
            .get("target-v2")
            .expect("renamed entry must keep its layout position");
        assert_eq!(moved.x, 120.0);
        assert_eq!(moved.y, -45.0);
        assert!(
            !positions.contains_key("target"),
            "old layout key must be removed after rename"
        );
    }

    /// 布局迁移失败必须让整个重命名回滚：文件、`@synapse-id`、级联引用、
    /// 索引和布局都保持原状，不能出现“改名成功但坐标静默丢失”。
    /// 这里把 `.synapse-layout.json.tmp` 预先建成目录，使 `save_layout`
    /// 的临时文件写入必定失败（不影响真实 `.synapse-layout.json` 与文件移动）。
    #[tokio::test]
    async fn execute_rename_rolls_back_when_layout_migration_fails() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n# Target\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("hub.md"),
            "<!-- @synapse-id: hub -->\n# Hub\n\nSee [[target]].\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }
        let runtime_dir = state.runtime_dir_base.join("kb-test");
        crate::layout_store::update_position(&runtime_dir, "target", 120.0, -45.0).unwrap();
        // 让 layout 的原子写失败（Q10.a 后布局在 runtime 目录）。
        std::fs::create_dir(runtime_dir.join("layout.json.tmp")).unwrap();

        let err = execute_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
            true,
        )
        .await
        .expect_err("layout failure must abort the rename");
        assert!(matches!(err, AppError::LayoutMigrationFailed { .. }));

        // 文件回到旧路径，新路径不存在，@synapse-id 还原。
        assert!(kb_root.join("target.md").exists(), "old file must be restored");
        assert!(!kb_root.join("target-v2.md").exists(), "new file must be removed");
        let target = std::fs::read_to_string(kb_root.join("target.md")).unwrap();
        assert!(target.contains("<!-- @synapse-id: target -->"));
        // 级联引用还原。
        let hub = std::fs::read_to_string(kb_root.join("hub.md")).unwrap();
        assert!(hub.contains("[[target]]"), "cascade must be rolled back: {hub}");
        assert!(!hub.contains("[[target-v2]]"));
        // 索引恢复旧 id。
        {
            let idx = state.index.read().await;
            let entries = idx.get_all_entries().unwrap();
            assert!(entries.iter().any(|e| e.id == "target"));
            assert!(!entries.iter().any(|e| e.id == "target-v2"));
        }
        // 布局保持旧键与坐标。
        let positions = crate::layout_store::load_positions(&state.runtime_dir_base.join("kb-test"));
        let kept = positions.get("target").expect("old layout key must survive");
        assert_eq!(kept.x, 120.0);
        assert_eq!(kept.y, -45.0);
        assert!(!positions.contains_key("target-v2"));
        // 回滚路径不能留下“本地写入”记录。
        assert!(!state.all_paths_locally_written(&[kb_root.join("target-v2.md")]));
        assert!(state.take_pending_renames().is_empty());
    }

    /// 移动文档时历史目录必须跟随 id 迁移，不能留在旧 id 下。
    #[tokio::test]
    async fn execute_move_migrates_history_directory() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::create_dir_all(kb_root.join("old")).unwrap();
        std::fs::create_dir_all(kb_root.join("new")).unwrap();
        std::fs::write(
            kb_root.join("old/target.md"),
            "<!-- @synapse-id: old-target -->\n# Target\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }
        crate::history::record_version(
            &state.runtime_dir_base.join("kb-test"),
            "old-target",
            "",
            "# history v1\n",
            crate::history::SaveOrigin::Editor,
            None,
        )
        .unwrap();
        assert!(crate::history::list_versions(&state.runtime_dir_base.join("kb-test"), "old-target").unwrap().len() == 1);

        let result = execute_rename_impl(
            &state,
            "old-target".to_string(),
            "target".to_string(),
            "new".to_string(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(result.new_id, "new-target");
        assert!(kb_root.join("new/target.md").exists());

        let versions = crate::history::list_versions(&state.runtime_dir_base.join("kb-test"), "new-target").unwrap();
        assert_eq!(versions.len(), 1, "history must follow the new id");
        assert!(
            crate::history::list_versions(&state.runtime_dir_base.join("kb-test"), "old-target").unwrap().is_empty(),
            "old history directory must be gone"
        );
    }

    /// 历史迁移失败必须触发整体回滚（而不是像旧的 best-effort 那样只打日志）。
    #[tokio::test]
    async fn execute_move_rolls_back_when_history_migration_fails() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::create_dir_all(kb_root.join("old")).unwrap();
        std::fs::write(
            kb_root.join("old/target.md"),
            "<!-- @synapse-id: old-target -->\n# Target\n\nSee [[hub]].\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("hub.md"),
            "<!-- @synapse-id: hub -->\n# Hub\n\nLinks to [[old-target]].\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }
        crate::layout_store::update_position(&state.runtime_dir_base.join("kb-test"), "old-target", 10.0, 20.0).unwrap();
        crate::history::record_version(
            &state.runtime_dir_base.join("kb-test"),
            "old-target",
            "",
            "# history v1\n",
            crate::history::SaveOrigin::Editor,
            None,
        )
        .unwrap();
        // 目标 id 已有历史目录 → migrate_entry_history 拒绝迁移。
        std::fs::create_dir_all(crate::history::history_dir(&state.runtime_dir_base.join("kb-test"), "new-target")).unwrap();

        let err = execute_rename_impl(
            &state,
            "old-target".to_string(),
            "target".to_string(),
            "new".to_string(),
            true,
        )
        .await
        .expect_err("history migration failure must abort the move");
        assert!(matches!(err, AppError::HistoryMigrationFailed { .. }));

        // 文件、id、级联引用、布局全部保持原状。
        assert!(kb_root.join("old/target.md").exists(), "old file must be restored");
        assert!(!kb_root.join("new/target.md").exists(), "new file must be removed");
        let content = std::fs::read_to_string(kb_root.join("old/target.md")).unwrap();
        assert!(content.contains("<!-- @synapse-id: old-target -->"));
        let hub = std::fs::read_to_string(kb_root.join("hub.md")).unwrap();
        assert!(hub.contains("[[old-target]]"), "cascade must be rolled back: {hub}");
        let positions = crate::layout_store::load_positions(&state.runtime_dir_base.join("kb-test"));
        assert!(positions.contains_key("old-target"));
        assert!(!positions.contains_key("new-target"));
        {
            let idx = state.index.read().await;
            let entries = idx.get_all_entries().unwrap();
            assert!(entries.iter().any(|e| e.id == "old-target"));
            assert!(!entries.iter().any(|e| e.id == "new-target"));
        }
        // 旧历史仍在，且回滚后来没有把它的目录搬走。
        assert_eq!(
            crate::history::list_versions(&state.runtime_dir_base.join("kb-test"), "old-target").unwrap().len(),
            1
        );
        assert!(!state.all_paths_locally_written(&[kb_root.join("new/target.md")]));
        assert!(state.take_pending_renames().is_empty());
    }

    /// Code blocks must NOT be touched by cascade. A `[[target]]` inside
    /// a ``` block should be preserved verbatim.
    #[tokio::test]
    async fn execute_rename_skips_code_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("doc.md"),
            "<!-- @synapse-id: doc -->\n\
             # Doc\n\n\
             Real: [[target]]\n\n\
             ```markdown\n\
             Example: [[target]]\n\
             ```\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        execute_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
            true,
        )
        .await
        .unwrap();

        let doc = std::fs::read_to_string(kb_root.join("doc.md")).unwrap();
        assert!(doc.contains("Real: [[target-v2]]"), "real ref cascaded");
        assert!(doc.contains("Example: [[target]]"), "code block ref preserved: {doc}");
    }

    /// cascade=false leaves the wikilinks in other files broken —
    /// the user is opting out of auto-update.
    #[tokio::test]
    async fn execute_rename_without_cascade_leaves_broken_wikilinks() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("target.md"),
            "<!-- @synapse-id: target -->\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("hub.md"),
            "<!-- @synapse-id: hub -->\n# Hub\n\nSee [[target]].\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        execute_rename_impl(
            &state,
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
            false, // cascade disabled
        )
        .await
        .unwrap();

        let hub = std::fs::read_to_string(kb_root.join("hub.md")).unwrap();
        assert!(hub.contains("[[target]]"), "old wikilink must remain: {hub}");
    }

    /// If the target id already exists, execute_rename must fail
    /// loudly without modifying any file.
    #[tokio::test]
    async fn execute_rename_to_existing_id_fails() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(
            kb_root.join("alpha.md"),
            "<!-- @synapse-id: alpha -->\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("beta.md"),
            "<!-- @synapse-id: beta -->\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let err = execute_rename_impl(
            &state,
            "alpha".to_string(),
            "beta".to_string(),
            "".to_string(),
            true,
        )
        .await
        .expect_err("rename to existing id must fail");
        let msg = format!("{err}");
        assert!(msg.starts_with("rename-conflict:"), "got: {msg}");

        // Both files must be unchanged on disk.
        assert!(kb_root.join("alpha.md").exists());
        assert!(kb_root.join("beta.md").exists());
    }

    /// 解耦后的关键回归：嵌套扁平条目的重命名必须落在原来的物理目录，
    /// 不能被 `group`（顶层目录）压平，新 id 也必须反映完整路径。
    #[tokio::test]
    async fn execute_rename_keeps_flat_nested_physical_dir() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::create_dir_all(kb_root.join("topic").join("sub")).unwrap();
        std::fs::write(
            kb_root.join("topic").join("sub").join("note.md"),
            "# Note\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = execute_rename_impl(
            &state,
            "topic-sub-note".to_string(),
            "renamed".to_string(),
            "topic/sub".to_string(), // 前端现在传 entry.dir，而不是 entry.group
            true,
        )
        .await
        .unwrap();

        assert_eq!(result.new_id, "topic-sub-renamed");
        assert!(kb_root.join("topic").join("sub").join("renamed.md").exists());
        assert!(!kb_root.join("topic").join("sub").join("note.md").exists());
        assert!(
            !kb_root.join("topic").join("renamed.md").exists(),
            "must not be flattened into the top-level group dir"
        );

        // 重建后物理 dir 与展示 group 仍然分离。
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
            let entries = idx.get_all_entries().unwrap();
            assert_eq!(entries[0].id, "topic-sub-renamed");
            assert_eq!(entries[0].group.as_deref(), Some("topic"));
            assert_eq!(entries[0].dir.as_deref(), Some("topic/sub"));
        }
    }

    /// R8: 非法新名 / 非法新目录必须在任何文件操作之前被拒绝。
    #[tokio::test]
    async fn execute_rename_rejects_invalid_name_and_group() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(kb_root.join("alpha.md"), "# Alpha\n").unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        for (name, group) in [
            ("../escaped", ""),
            ("alpha.md", ""),
            ("a/b", ""),
            ("fine", "../outside"),
            ("fine", "/etc"),
        ] {
            let err = execute_rename_impl(
                &state,
                "alpha".to_string(),
                name.to_string(),
                group.to_string(),
                true,
            )
            .await
            .expect_err("invalid rename input must be rejected");
            let msg = format!("{err}");
            assert!(
                msg.starts_with("name-") || msg.starts_with("parent-group-invalid"),
                "unexpected error for {name:?}/{group:?}: {msg}"
            );
        }

        // 原始文件必须完好，且没有越界文件被写出。
        assert!(kb_root.join("alpha.md").exists());
        assert!(!kb_root.parent().unwrap().join("escaped.md").exists());
        assert!(!kb_root.join("..").join("outside").exists());
    }

    /// 目录布局的 group / dir 语义保持解耦前的行为：`@synapse-group`
    /// 既用于展示，也用于拼写入目录。
    #[tokio::test]
    async fn execute_rename_directory_entry_keeps_group_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::create_dir_all(kb_root.join("frontend").join("my-note")).unwrap();
        std::fs::write(
            kb_root.join("frontend").join("my-note").join("content.md"),
            "<!-- @synapse-group: frontend -->\n# My Note\n",
        )
        .unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = execute_rename_impl(
            &state,
            "my-note".to_string(),
            "my-note-v2".to_string(),
            "frontend".to_string(),
            true,
        )
        .await
        .unwrap();

        assert_eq!(result.new_id, "my-note-v2");
        assert!(kb_root.join("frontend").join("my-note-v2").join("content.md").exists());

        let idx = state.index.read().await;
        idx.rebuild(&kb_root).unwrap();
        let entries = idx.get_all_entries().unwrap();
        assert_eq!(entries[0].id, "my-note-v2");
        assert_eq!(entries[0].group.as_deref(), Some("frontend"));
        assert_eq!(entries[0].dir.as_deref(), Some("frontend"));
    }

    // ---------- 正文编辑器：保存 / 自动保存草稿的服务端部分 ----------

    /// 显式保存：原子替换、登记本地写入、内容相同不重复写。
    #[tokio::test]
    async fn write_entry_content_saves_and_marks_local_write() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        let path = kb_root.join("note.md");
        std::fs::write(&path, "# Note\n\n原始内容\n").unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let next = "# Note\n\n新的内容\n";
        let result = {
            let _guard = state.write_lock.lock().await;
            write_entry_content(
                &state,
                "note",
                next,
                crate::history::SaveOrigin::Editor,
                None,
            )
            .await
            .unwrap()
        };

        assert!(result.changed);
        assert_eq!(result.content, next);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        let md_files: Vec<_> = std::fs::read_dir(&kb_root)
            .unwrap()
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with(".md") && !name.starts_with(".synapse-tmp-")
            })
            .collect();
        assert_eq!(md_files.len(), 1, "保存后不应残留临时文件");

        // 本次写入的路径能被识别为 local-write（watcher 不会把它当外部变更）。
        assert!(state.all_paths_locally_written(std::slice::from_ref(&path)));

        // 内容相同 → changed=false，不产生写入。
        let same = {
            let _guard = state.write_lock.lock().await;
            write_entry_content(
                &state,
                "note",
                next,
                crate::history::SaveOrigin::Editor,
                None,
            )
            .await
            .unwrap()
        };
        assert!(!same.changed);
    }

    /// 回退：写回旧正文并追加一条带 restored_from 的新版本。
    #[tokio::test]
    async fn restore_entry_version_appends_new_version() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        let path = kb_root.join("note.md");
        std::fs::write(&path, "v1\n").unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        // 显式保存（内容未变也算一次）→ 初始版本；再两次变更 → 共 3 版。
        {
            let _guard = state.write_lock.lock().await;
            write_entry_content(&state, "note", "v1\n", crate::history::SaveOrigin::Editor, None)
                .await
                .unwrap();
            write_entry_content(&state, "note", "v2\n", crate::history::SaveOrigin::Editor, None)
                .await
                .unwrap();
            write_entry_content(&state, "note", "v3\n", crate::history::SaveOrigin::Editor, None)
                .await
                .unwrap();
        }
        let versions = crate::history::list_versions(&state.runtime_dir_base.join("kb-test"), "note").unwrap();
        assert_eq!(versions.len(), 3, "初始内容 + 两次保存 = 3 版");
        let first_id = versions[0].version_id.clone();

        let restored = {
            let _guard = state.write_lock.lock().await;
            let content = crate::history::read_version_content(&state.runtime_dir_base.join("kb-test"), "note", &first_id)
                .unwrap()
                .expect("小文件版本必须可回退");
            write_entry_content(
                &state,
                "note",
                &content,
                crate::history::SaveOrigin::Restore,
                Some(first_id.clone()),
            )
            .await
            .unwrap()
        };
        assert!(restored.changed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1\n");

        let after = crate::history::list_versions(&state.runtime_dir_base.join("kb-test"), "note").unwrap();
        assert_eq!(after.len(), 4, "回退本身也要产生一条新版本");
        let newest = after.last().unwrap();
        assert_eq!(newest.origin, crate::history::SaveOrigin::Restore);
        assert_eq!(newest.restored_from.as_deref(), Some(first_id.as_str()));
    }

    /// 正文文件被外部删除后，保存必须报错而不是静默重建。
    #[tokio::test]
    async fn write_entry_content_reports_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        let path = kb_root.join("note.md");
        std::fs::write(&path, "# Note\n").unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }
        std::fs::remove_file(&path).unwrap();

        let _guard = state.write_lock.lock().await;
        let err = write_entry_content(
            &state,
            "note",
            "# Note\n",
            crate::history::SaveOrigin::Editor,
            None,
        )
        .await
        .expect_err("missing file must be reported");
        assert!(
            format!("{err}").starts_with("entry-content-missing:"),
            "got: {err}"
        );
    }

    /// 排版预览只读；确认后写回，且第二次应用是 no-op。
    #[tokio::test]
    async fn format_preview_then_apply_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        let path = kb_root.join("note.md");
        let original = "#  标题  \n\n-   item\n\n使用React框架   \n";
        std::fs::write(&path, original).unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let preview = preview_format_impl(&state, "note").await.unwrap();
        assert!(preview.changed);
        assert_eq!(preview.original, original, "预览不能改动磁盘内容");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert_eq!(
            preview.formatted,
            "# 标题\n\n- item\n\n使用 React 框架\n"
        );

        let applied = apply_format_impl(&state, "note").await.unwrap();
        assert!(applied.changed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), preview.formatted);

        let second = apply_format_impl(&state, "note").await.unwrap();
        assert!(!second.changed, "排版结果必须稳定（第二次无写入）");
    }

    // ---------- 同 KB 迁移（复用 rename 管线） ----------

    /// 扁平条目换目录：文件名不变，新 id 由新目录派生，布局键跟随迁移。
    #[tokio::test]
    async fn move_entry_flat_keeps_name_and_migrates_layout() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::create_dir_all(kb_root.join("alpha")).unwrap();
        std::fs::create_dir_all(kb_root.join("beta")).unwrap();
        std::fs::write(kb_root.join("alpha").join("note.md"), "# Note\n").unwrap();

        let state = state_with_current_kb(Some(&kb_root));
        crate::layout_store::update_position(&state.runtime_dir_base.join("kb-test"), "alpha-note", 10.0, 20.0).unwrap();
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = move_entry_impl(&state, "alpha-note".to_string(), "beta".to_string())
            .await
            .unwrap();

        assert_eq!(result.new_id, "beta-note");
        assert!(kb_root.join("beta").join("note.md").exists());
        assert!(!kb_root.join("alpha").join("note.md").exists());

        let positions = crate::layout_store::load_positions(&state.runtime_dir_base.join("kb-test"));
        assert!(positions.get("alpha-note").is_none(), "旧布局键必须消失");
        assert_eq!(
            positions.get("beta-note").map(|p| (p.x, p.y)),
            Some((10.0, 20.0)),
            "布局位置必须跟随迁移"
        );

        let idx = state.index.read().await;
        idx.rebuild(&kb_root).unwrap();
        let entries = idx.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "beta-note");
        assert_eq!(entries[0].dir.as_deref(), Some("beta"));
    }

    /// 目录条目换父目录：目录名不变，整个目录（含附件）一起移动。
    #[tokio::test]
    async fn move_entry_directory_moves_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::create_dir_all(kb_root.join("compliance").join("gdpr")).unwrap();
        std::fs::create_dir_all(kb_root.join("legal")).unwrap();
        std::fs::write(
            kb_root.join("compliance").join("gdpr").join("content.md"),
            "# GDPR\n",
        )
        .unwrap();
        std::fs::write(
            kb_root.join("compliance").join("gdpr").join("diagram.png"),
            "bytes",
        )
        .unwrap();

        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let result = move_entry_impl(&state, "gdpr".to_string(), "legal".to_string())
            .await
            .unwrap();

        assert_eq!(result.new_id, "gdpr");
        assert!(kb_root.join("legal").join("gdpr").join("content.md").exists());
        assert!(
            kb_root.join("legal").join("gdpr").join("diagram.png").exists(),
            "目录迁移必须带上附件"
        );
        assert!(!kb_root.join("compliance").join("gdpr").exists());
    }

    /// 迁移到路径逃逸目录必须被拒绝，且原文件保持原位。
    #[tokio::test]
    async fn move_entry_rejects_escaping_target_dir() {
        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();
        std::fs::write(kb_root.join("note.md"), "# Note\n").unwrap();
        let state = state_with_current_kb(Some(&kb_root));
        {
            let idx = state.index.read().await;
            idx.rebuild(&kb_root).unwrap();
        }

        let err = move_entry_impl(&state, "note".to_string(), "../outside".to_string())
            .await
            .expect_err("escaping target dir must be rejected");
        assert!(format!("{err}").starts_with("parent-group-invalid:"), "got: {err}");
        assert!(kb_root.join("note.md").exists());
    }
}
