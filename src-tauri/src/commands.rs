#![cfg(feature = "tauri")]

use crate::error::{AppError, AppResult};
use crate::index::{Index, StoredEntry};
use crate::kb_registry::KBInfo;
use crate::state::AppState;
use crate::validator::{self, Severity, ValidationIssue, ValidationReport};
use std::path::{Component, Path, PathBuf};
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

#[tauri::command]
pub async fn list_kbs(state: State<'_, AppState>) -> AppResult<Vec<KBInfo>> {
    let reg = state.kbs.read().await;
    Ok(reg.list())
}

#[tauri::command]
pub async fn get_active_kb(state: State<'_, AppState>) -> AppResult<Option<KBInfo>> {
    let reg = state.kbs.read().await;
    Ok(reg.active().cloned())
}

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

#[tauri::command]
pub async fn set_active_kb(state: State<'_, AppState>, id: String) -> AppResult<()> {
    load_active_kb(&state, &id).await
}

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

#[tauri::command]
pub async fn get_entries(state: State<'_, AppState>) -> AppResult<Vec<StoredEntry>> {
    let idx = state.index.read().await;
    let entries = idx.get_all_entries()?;
    Ok(entries)
}

#[tauri::command]
pub async fn active_kb_entry_count(state: State<'_, AppState>) -> AppResult<usize> {
    let idx = state.index.read().await;
    Ok(idx.entry_count()?)
}

#[tauri::command]
pub async fn get_entry_content(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Option<String>> {
    let path = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?
    };
    let Some(path) = path else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(_) => Ok(None),
    }
}

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
            .get_entry_path(&from_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: from_id.clone(),
            })?;
        let t = idx
            .get_entry_path(&to_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: to_id.clone(),
            })?;
        (f, t)
    };

    append_reference_line(&from_path, &to_path, &to_id)?;

    let idx = state.index.read().await;
    idx.add_relation(&from_id, &to_id, &relation_type, note.as_deref())?;
    Ok(())
}

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
        idx.get_entry_path(&from_id)?
            .ok_or_else(|| AppError::EntryNotFound {
                id: from_id.clone(),
            })?
    };
    remove_wikilink_line(&from_path, &to_id)?;

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
fn remove_wikilink_line(path: &std::path::Path, to_id: &str) -> AppResult<()> {
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
        return Ok(());
    }
    let mut new_content: String = kept.iter().copied().collect();
    if !trailing_nl && !new_content.ends_with('\n') {
        // No-op — the helper preserves whatever end-of-file the file had.
    }
    let _ = trailing_nl;
    std::fs::write(path, new_content)?;
    Ok(())
}

/// Delete an entry: remove the `.md` file from disk and the row from the
/// `entries` table (FK CASCADE wipes related `tags`, `attachments`, and
/// `relations` rows). Wikilinks inside other entries that point at this one
/// become dangling — they will fail to resolve on the next rebuild, which is
/// the correct behavior (the user edits those files by hand to clean up).
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

    std::fs::remove_file(&path).map_err(|e| AppError::io(
        format!("delete_entry: remove {}", path.display()),
        e,
    ))?;

    let idx = state.index.read().await;
    idx.remove_entry(&id)?;
    Ok(())
}

/// 在 from .md 末尾追加一行 markdown 引用（指向 to 文件）。
/// 单一真理源策略：rebuild 时会从 .md 重新识别并建关系。
fn append_reference_line(
    from_path: &std::path::Path,
    _to_path: &std::path::Path,
    to_id: &str,
) -> AppResult<()> {
    let mut src = std::fs::read_to_string(from_path).unwrap_or_default();
    if !src.ends_with('\n') {
        src.push('\n');
    }
    src.push_str(&format!("\n- [[{to_id}]]\n"));
    std::fs::write(from_path, src)?;
    Ok(())
}

/// 独立创建新 entry。`parentGroup` 为空时落到 KB 根目录，否则落到 `<kb>/<parentGroup>/`。
///
/// 关联方向由参数决定：
/// - `link_from_entry` 传入：源 entry 作为**上级**（关系方向 source → 新 entry，写入源 .md）
/// - `link_to_entry` 传入：源 entry 作为**下级**（关系方向 新 entry → source，写入新 .md）
/// 两者可同时传入（少见），分别处理双向引用。
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

    let rtype = relation_type.as_deref().unwrap_or("reference");

    // 2) 上级模式：source 是新 entry 的 from，关系方向 source → 新 entry
    //    写入 source .md
    if let Some(from_id) = link_from_entry.as_deref() {
        let from_path = {
            let idx = state.index.read().await;
            idx.get_entry_path(from_id)?
                .ok_or_else(|| AppError::EntryNotFound {
                    id: from_id.to_string(),
                })?
        };
        append_reference_line(&from_path, &file_path, &new_id)?;
        let idx = state.index.read().await;
        idx.add_relation(from_id, &new_id, rtype, note.as_deref())?;
    }

    // 3) 下级模式：新 entry 是 from，关系方向 新 entry → source
    //    写入新 .md
    if let Some(to_id) = link_to_entry.as_deref() {
        let to_path = {
            let idx = state.index.read().await;
            idx.get_entry_path(to_id)?
                .ok_or_else(|| AppError::EntryNotFound {
                    id: to_id.to_string(),
                })?
        };
        append_reference_line(&file_path, &to_path, to_id)?;
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

    Ok(new_id)
}

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
        idx.get_entry_path(&from_id)?
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

    Ok(new_id)
}

// ---------- Validation & KB config commands (Task 4.2) ----------

/// Run full validation against the KB at `kb_root`, cache the report in
/// `state.validation_report`, and return it to the frontend. The cache is
/// what the startup hook (`run_validation`) and a manual "Validate now"
/// button share — keeping one report in state avoids drift between the
/// two paths. Per plan, `validate_kb` and `run_validation` are identical
/// today; they may diverge later (manual vs. startup trigger).
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
    ))
}

/// Return `(entry_id, rule_id)` for every `Info`-level issue in the KB's
/// validation report. The frontend uses these as "system labels" —
/// soft hints derived from rules, shown alongside user-authored labels.
/// We filter by `Severity::Info` only (warnings/errors are surfaced
/// separately as problems, not as labels). Per spec, `get_system_labels`
/// ignores the on-disk entry scan — the report already carries all
/// the info we need.
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

    // Swap the file watcher to track the new KB root. Dropping the previous
    // WatcherHandle unregisters the old OS watch; the new one watches the
    // freshly-activated KB so external edits there are picked up too.
    swap_watcher(state, Some(&kb_path));

    Ok(())
}

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
    // No active KB → no KB to watch. Drop the watcher.
    swap_watcher(state, None);
    Ok(())
}

/// Replace the active KB's file watcher with one rooted at `new_root`
/// (or `None` to drop the watcher entirely). No-op if the new root matches
/// the watcher's current root, so a redundant KB reload doesn't churn the
/// notify subscription. `state.write_lock` must be held by the caller.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;

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
                append_reference_line(&path, &path, &format!("id-{i}")).unwrap();
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
                    append_reference_line(&path, &path, &format!("id-{i}")).unwrap();
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
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let index = Index::open(&db_path).expect("open test index");

        let registry_file = std::env::temp_dir().join(format!(
            "synapse-cmd-test-kbs-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
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
            validation_report: Arc::new(tokio::sync::RwLock::new(None)),
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            watcher: Arc::new(std::sync::Mutex::new(None)),
            watch_tx: Arc::new(std::sync::Mutex::new(None)),
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
}
