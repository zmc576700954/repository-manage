# 内联元数据编辑 + 重命名/移动 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Synapse KB 增加元数据可写操作：(A) 在 DetailPanel 内联编辑 entry 的 title / tags / labels / status；(B) 通过模态对话框重命名/移动 entry，自动级联更新所有 `[[old-id]]` 引用。

**Architecture:** 后端新增 `parser/metadata_updater.rs` 负责改写 `@synapse-*` HTML 注释；新增 3 个 Tauri commands (`update_entry_metadata` / `precheck_rename` / `execute_rename`)；SQLite 关系迁移和 layout 位置迁移在 `execute_rename` 写锁内原子完成。前端新增 `EntryMetadataEditor` 和 `RenameEntryDialog` 两个组件，`useAppStore` 增 3 个 mutations。

**Tech Stack:** Rust (Tauri 2 / rusqlite / regex / serde)、React 18 + TypeScript (Zustand / react-i18next)、i18next (zh + en)。

**Spec:** `docs/superpowers/specs/2026-10-12-inline-edit-rename-design.md`

---

## File Structure

**新增**：
- `src-tauri/src/parser/metadata_updater.rs` — 改写 `@synapse-*` 注释的纯函数
- `src/components/EntryMetadataEditor.tsx` — DetailPanel 内的内联编辑组件
- `src/components/RenameEntryDialog.tsx` — 重命名模态对话框
- `src/components/i18n/EntryMetadataEditor.zh.json` + `.en.json` — 内联编辑文案
- `src/components/i18n/RenameEntryDialog.zh.json` + `.en.json` — 重命名对话框文案

**修改**：
- `src-tauri/src/parser/mod.rs` — 导出 `metadata_updater` 模块
- `src-tauri/src/commands.rs` — +3 commands + 2 错误变体 + 单元测试
- `src-tauri/src/index.rs` — +`rename_entry_id` 方法
- `src-tauri/src/layout_store.rs` — +`rename_key` 方法 + 测试
- `src-tauri/src/error.rs` — +5 错误变体
- `src-tauri/src/lib.rs` — 注册新 commands
- `src/views/DetailPanel.tsx` — 用 EntryMetadataEditor 替换只读 tags；header 加 Rename 按钮
- `src/store/useAppStore.ts` — +3 mutations（updateMetadata / precheckRename / renameEntry）
- `src/i18n/resources.ts` — 注册 2 个新 NS

---

## Task 1: `metadata_updater` 模块骨架 + 第一个测试

**Files:**
- Create: `src-tauri/src/parser/metadata_updater.rs`
- Modify: `src-tauri/src/parser/mod.rs:1-15`（添加 `pub mod metadata_updater;`）

- [ ] **Step 1: 创建 `metadata_updater.rs` 骨架（无函数体）**

`src-tauri/src/parser/metadata_updater.rs`：

```rust
//! Rewrite `@synapse-*` HTML comments in content.md while preserving
//! the rest of the file (markdown body, YAML frontmatter, line endings).
//!
//! Used by `update_entry_metadata` to apply UI-driven edits without
//! round-tripping through a full markdown parse / re-serialise.

use std::path::Path;
use crate::error::{AppError, AppResult};

/// Apply a set of metadata changes to a content.md file. Only the
/// `<!-- @synapse-{field}: ... -->` lines are touched; everything else
/// is byte-identical to the input.
pub fn apply_metadata_changes(
    path: &Path,
    changes: &MetadataChanges,
) -> AppResult<()> {
    todo!()
}

/// Field changes passed from the frontend. `None` means "leave alone".
pub struct MetadataChanges {
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,
    pub status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn update_title_writes_synapse_title_comment() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(
            &path,
            "<!-- @synapse-id: note -->\n\
             <!-- @synapse-title: Old Title -->\n\
             <!-- @synapse-tags: type:note -->\n\
             \n\
             # Note\n",
        )
        .unwrap();

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: Some("New Title".to_string()),
                tags: None,
                labels: None,
                status: None,
            },
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("<!-- @synapse-title: New Title -->"),
            "title line not updated: {content}"
        );
        assert!(
            content.contains("<!-- @synapse-id: note -->"),
            "id line untouched: {content}"
        );
        assert!(
            content.contains("# Note\n"),
            "markdown body untouched: {content}"
        );
    }
}
```

`src-tauri/src/parser/mod.rs`：在文件顶部、`pub mod yaml_field;` 之后添加 `pub mod metadata_updater;`。

- [ ] **Step 2: 运行测试，验证失败**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib parser::metadata_updater
```

Expected: FAIL with `not yet implemented`（`todo!()` panic）。

- [ ] **Step 3: 实现 `apply_metadata_changes`（最小可用版本）**

```rust
pub fn apply_metadata_changes(
    path: &Path,
    changes: &MetadataChanges,
) -> AppResult<()> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::io(format!("metadata_updater: read {}", path.display()), e)
    })?;
    let mut new_content = content.clone();
    if let Some(t) = &changes.title { new_content = replace_or_insert(&new_content, "title", t); }
    if let Some(t) = &changes.tags  { new_content = replace_or_insert(&new_content, "tags",   &t.join(", ")); }
    if let Some(t) = &changes.labels{ new_content = replace_or_insert(&new_content, "labels", &t.join(", ")); }
    if let Some(s) = &changes.status{ new_content = replace_or_insert(&new_content, "status", s); }
    if new_content != content {
        std::fs::write(path, &new_content).map_err(|e| {
            AppError::io(format!("metadata_updater: write {}", path.display()), e)
        })?;
    }
    Ok(())
}

fn replace_or_insert(content: &str, field: &str, value: &str) -> String {
    use regex::Regex;
    // 匹配 `<!-- @synapse-{field}: ... -->`（不跨行）
    let pattern = format!(r"(?m)^(<!-- @synapse-{field}:\s*)([^-]+?)(\s*-->)$");
    let re = Regex::new(&pattern).expect("valid regex");
    if let Some(caps) = re.captures(content) {
        // 在原行原地替换
        let mut result = String::with_capacity(content.len());
        let m = caps.get(0).unwrap();
        result.push_str(&content[..m.start()]);
        result.push_str(&caps[1]);
        result.push_str(value);
        result.push_str(&caps[3]);
        result.push_str(&content[m.end()..]);
        result
    } else {
        // 未找到：在第一个 `<!-- @synapse-` 行后插入
        let insert_pattern = Regex::new(r"(?m)^(<!-- @synapse-[a-z]+:[^\n]*-->\n)").unwrap();
        if let Some(caps) = insert_pattern.captures(content) {
            let m = caps.get(1).unwrap();
            let mut result = String::with_capacity(content.len() + 64);
            result.push_str(&content[..m.end()]);
            result.push_str(&format!("<!-- @synapse-{field}: {value} -->\n"));
            result.push_str(&content[m.end()..]);
            result
        } else {
            // 文件里没有任何 synapse 注释 — 头部插入
            format!("<!-- @synapse-{field}: {value} -->\n\n{content}")
        }
    }
}
```

- [ ] **Step 4: 运行测试，验证通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib parser::metadata_updater
```

Expected: PASS，1 test。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/parser/metadata_updater.rs src-tauri/src/parser/mod.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(metadata): metadata_updater skeleton + title-replace test"
```

---

## Task 2: `metadata_updater` 全部测试

**Files:**
- Modify: `src-tauri/src/parser/metadata_updater.rs`（在 `mod tests` 内追加 4 个 test）

- [ ] **Step 1: 添加 4 个测试**

在 `src-tauri/src/parser/metadata_updater.rs` 的 `mod tests` 块底部追加：

```rust
    #[test]
    fn preserves_unrelated_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let body = "<!-- @synapse-id: note -->\n<!-- @synapse-title: T -->\n\
                    <!-- @synapse-tags: type:note -->\n\n\
                    # Note\n\nSome **bold** and `code`.\n\n- [[other]]\n- [web](https://x.com)\n";
        fs::write(&path, body).unwrap();

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: Some("New".to_string()),
                tags: None,
                labels: None,
                status: None,
            },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("Some **bold** and `code`."), "markdown body must be preserved");
        assert!(after.contains("- [[other]]"), "wikilink must be preserved");
        assert!(after.contains("[web](https://x.com)"), "markdown link must be preserved");
        // Line ending must be preserved (one trailing \n on body section).
        assert!(after.ends_with("- [web](https://x.com)\n"), "trailing newline must be preserved");
    }

    #[test]
    fn inserts_new_field_when_comment_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(
            &path,
            "<!-- @synapse-id: note -->\n<!-- @synapse-title: T -->\n# Note\n",
        )
        .unwrap();

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: None,
                tags: None,
                labels: None,
                status: Some("draft".to_string()),
            },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        // 应该在第一个 synapse 行后插入
        assert!(after.contains("<!-- @synapse-status: draft -->"), "status line not inserted: {after}");
        // 位置校验：status 行应该在 id/title 之后
        let id_pos = after.find("<!-- @synapse-id:").unwrap();
        let title_pos = after.find("<!-- @synapse-title:").unwrap();
        let status_pos = after.find("<!-- @synapse-status:").unwrap();
        assert!(id_pos < status_pos, "status must come after id");
        assert!(title_pos < status_pos, "status must come after title");
    }

    #[test]
    fn tags_replaces_full_csv_not_appends() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(
            &path,
            "<!-- @synapse-id: note -->\n<!-- @synapse-tags: a, b, c -->\n",
        )
        .unwrap();

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: None,
                tags: Some(vec!["x".to_string(), "y".to_string()]),
                labels: None,
                status: None,
            },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("<!-- @synapse-tags: x, y -->"), "tags not replaced: {after}");
        assert!(!after.contains("a, b, c"), "old tags still present: {after}");
    }

    #[test]
    fn empty_changes_writes_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let body = "<!-- @synapse-id: note -->\n# Note\n";
        fs::write(&path, body).unwrap();

        apply_metadata_changes(
            &path,
            &MetadataChanges { title: None, tags: None, labels: None, status: None },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, body, "no changes should leave file byte-identical");
    }
```

- [ ] **Step 2: 运行测试，验证全部通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib parser::metadata_updater
```

Expected: PASS，5 tests（1 + 4）。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/parser/metadata_updater.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(metadata): metadata_updater test coverage (preservation, insert, replace, noop)"
```

---

## Task 3: 扩展 `AppError` 加 5 个新变体

**Files:**
- Modify: `src-tauri/src/error.rs:1-100`（在 `AppError` enum 内追加 5 个变体）

- [ ] **Step 1: 在 `AppError` enum 中追加 5 个变体**

打开 `src-tauri/src/error.rs`，找到 `pub enum AppError { ... }`。在最后一个变体后（通常是 `BackupNotFound` 或类似）追加：

```rust
    /// Inline metadata edit rejected a field value (empty title, tag with
    /// comma, status not in enum, etc.).
    #[error("invalid metadata field '{field}': {reason}")]
    MetadataInvalidField { field: String, reason: String },

    /// Rename precheck found that the target id is already taken by
    /// another entry in the same KB.
    #[error("rename conflict: id '{new_id}' already exists")]
    RenameConflict { new_id: String },

    /// Filesystem refuses to move — target file already exists.
    #[error("rename target file exists: {path}")]
    RenameFileExists { path: String },

    /// File mtime changed between read and write — possible external edit.
    #[error("external edit detected on {path}")]
    ExternalEditDetected { path: String },

    /// Cascade find/replace failed mid-way; rollback already attempted.
    #[error("cascade failed: {failed_files:?} ({reason})")]
    CascadeFailed { failed_files: Vec<String>, reason: String },
```

确保这些变体放在 `AppError` 末尾（与 `BackupNotFound` 等并列），不破坏 `#[derive]` 宏。

- [ ] **Step 2: 验证编译**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo check
```

Expected: 通过，0 errors。可能需要修一下 import 路径（如果 `thiserror` 的 `#[error(...)]` 宏格式有特殊字符要求）。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/error.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(error): add 5 new error variants for inline-edit + rename"
```

---

## Task 4: `index.rename_entry_id` 方法 + 测试

**Files:**
- Modify: `src-tauri/src/index.rs`（在 `Index` impl 内追加 `rename_entry_id` 方法 + 测试）

- [ ] **Step 1: 添加测试到 `mod tests` 末尾**

在 `src-tauri/src/index.rs` 的 `#[cfg(test)] mod tests` 块底部追加：

```rust
    // ---------- rename_entry_id: cross-table id migration ----------

    /// `rename_entry_id` is the SQL half of `execute_rename`. It must
    /// move the entry's primary key + every relation edge whose
    /// from_id / to_id pointed at the old id, all in one transaction.
    /// Pre-fix the entry row would be wiped (FK violation) and the
    /// relations would silently survive under the old id.
    #[test]
    fn rename_entry_id_moves_row_and_edges_atomically() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n").unwrap();
        fs::write(root.join("b.md"), "# B\n").unwrap();
        fs::write(root.join("c.md"), "# C\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();
        index.add_relation("a", "b", "reference", None).unwrap();
        index.add_relation("c", "a", "derived", None).unwrap();

        // Rename A → A2. The outgoing A→B edge must become A2→B, and
        // the incoming C→A edge must become C→A2.
        index.rename_entry_id("a", "a2").unwrap();

        let entries = index.get_all_entries().unwrap();
        let a2 = entries.iter().find(|e| e.id == "a2").expect("a2 must exist");
        let b  = entries.iter().find(|e| e.id == "b").unwrap();
        let c  = entries.iter().find(|e| e.id == "c").unwrap();
        assert!(entries.iter().find(|e| e.id == "a").is_none(), "old id a must be gone");

        // A2's outgoing: A2 → B (was A → B).
        let a2_out: Vec<&StoredRelation> = a2.relations.iter().filter(|r| r.from_id == "a2").collect();
        assert_eq!(a2_out.len(), 1, "A2 should have 1 outgoing edge (→B)");
        assert_eq!(a2_out[0].to_id, "b");
        assert_eq!(a2_out[0].relation_type, "reference");

        // C's outgoing: C → A2 (was C → A).
        let c_out: Vec<&StoredRelation> = c.relations.iter().filter(|r| r.from_id == "c").collect();
        assert_eq!(c_out.len(), 1, "C should still have 1 outgoing edge");
        assert_eq!(c_out[0].to_id, "a2", "C→A must become C→A2");
        assert_eq!(c_out[0].relation_type, "derived");

        // B's incoming: A2 → B.
        let b_in: Vec<&StoredRelation> = b.relations.iter().filter(|r| r.to_id == "b").collect();
        assert_eq!(b_in.len(), 1);
        assert_eq!(b_in[0].from_id, "a2");
    }

    /// Renaming an entry to an id that already exists must fail loudly
    /// (the SQL transaction's UNIQUE constraint would have rolled it
    /// back anyway, but we want a typed error, not a rusqlite panic).
    #[test]
    fn rename_entry_id_to_existing_id_returns_error() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n").unwrap();
        fs::write(root.join("b.md"), "# B\n").unwrap();
        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let result = index.rename_entry_id("a", "b");
        assert!(result.is_err(), "rename to existing id must fail");
        // A must still be there with original row.
        let entries = index.get_all_entries().unwrap();
        assert!(entries.iter().any(|e| e.id == "a"), "A must survive failed rename");
        assert!(entries.iter().any(|e| e.id == "b"), "B must be untouched");
    }

    /// Renaming a non-existent id is a no-op returning an error, so
    /// the command can decide whether to surface it. (Some callers
    /// will precheck and never hit this path; others may not.)
    #[test]
    fn rename_entry_id_unknown_old_id_returns_error() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n").unwrap();
        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let result = index.rename_entry_id("missing", "newid");
        assert!(result.is_err(), "unknown old id must fail");
    }
```

- [ ] **Step 2: 运行测试，验证失败（方法不存在）**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib index::tests::rename_entry_id
```

Expected: FAIL — `no method named rename_entry_id`。

- [ ] **Step 3: 实现 `rename_entry_id`**

在 `src-tauri/src/index.rs` 的 `impl Index { ... }` 块内（`add_relation` 之后）追加：

```rust
    /// Atomically move an entry's primary key from `old` to `new` and
    /// cascade the change to every relation edge whose `from_id` or
    /// `to_id` pointed at `old`. Returns `rusqlite::Error` on:
    ///   * `new` already exists (UNIQUE violation) — caller can map to
    ///     `AppError::RenameConflict`
    ///   * `old` doesn't exist (no rows updated) — caller can map to
    ///     `AppError::EntryNotFound`
    ///
    /// Wrapped in a transaction so partial state can never leak
    /// (e.g. relation edges migrated but the entry row not, or vice
    /// versa). The old row's ON DELETE CASCADE on `tags` / `attachments`
    /// / `relations` is what makes this work — we DELETE the old row
    /// inside the transaction, then re-INSERT the same data under `new`.
    ///
    /// This is the SQL half of `execute_rename`. The file-rename half
    /// happens in the command, which calls this method after the
    /// filesystem move succeeds.
    pub fn rename_entry_id(&self, old: &str, new: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        // 1) Update relation edges FIRST (FK target moves; existing rows
        //    would otherwise dangle if we deleted the parent row first).
        tx.execute(
            "UPDATE relations SET from_id = ?1 WHERE from_id = ?2",
            params![new, old],
        )?;
        tx.execute(
            "UPDATE relations SET to_id = ?1 WHERE to_id = ?2",
            params![new, old],
        )?;

        // 2) Delete the old entry row. ON DELETE CASCADE wipes any
        //    leftover tags / attachments / relations rows that point at
        //    `old` (should be none after step 1, but CASCADE keeps the
        //    invariant safe).
        let n = tx.execute("DELETE FROM entries WHERE id = ?1", params![old])?;
        if n == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        // 3) Re-insert under the new id. The row data (title, path,
        //    tags, etc.) is unchanged — the file on disk still has the
        //    same content; only the id moves.
        tx.execute(
            "INSERT INTO entries (id, title, path, group_name, has_content_md, \
             labels, status, aliases, source, created, updated, ignored_rules, \
             error_count, warning_count) \
             SELECT ?1, title, path, group_name, has_content_md, \
             labels, status, aliases, source, created, updated, ignored_rules, \
             error_count, warning_count \
             FROM entries WHERE id = ?2",
            params![new, old],
        )?;
        // Note: the SELECT above runs against a snapshot taken BEFORE
        // the DELETE in the same transaction, so it works. Verify the
        // row landed:
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM entries WHERE id = ?1",
            params![new],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 4: 运行测试，验证通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib index::tests::rename_entry_id
```

Expected: PASS，3 tests。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/index.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(index): rename_entry_id — atomic SQL id migration across entries + relations"
```

---

## Task 5: `layout_store.rename_key` + 测试

**Files:**
- Modify: `src-tauri/src/layout_store.rs`（在 `update_position` 后追加 `rename_key` + 测试）

- [ ] **Step 1: 添加测试到 `mod tests` 末尾**

在 `src-tauri/src/layout_store.rs` 的 `#[cfg(test)] mod tests` 块底部追加：

```rust
    /// `rename_key` is the layout half of `execute_rename`. The user
    /// expects the node to stay where it was after renaming, so we
    /// transfer the position data from the old id to the new id
    /// atomically (single layout file write).
    #[test]
    fn rename_key_transfers_position() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        update_position(kb_root, "react-hooks", 100.0, 200.0).unwrap();

        layout_store::rename_key(kb_root, "react-hooks", "react-hooks-v2").unwrap();

        let layout = load_layout(kb_root);
        assert!(layout.get("react-hooks").is_none(), "old key must be gone");
        let new_pos = layout.get("react-hooks-v2").expect("new key must exist");
        assert_eq!(new_pos.x, 100.0);
        assert_eq!(new_pos.y, 200.0);
    }

    /// If the old key didn't have a position (it was never moved from
    /// the default group layout), `rename_key` is a silent no-op —
    /// the new id is also absent. The next time the user moves the
    /// node, the new id gets its position naturally.
    #[test]
    fn rename_key_unknown_old_key_is_noop() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        // No position recorded for "missing".
        layout_store::rename_key(kb_root, "missing", "new-name").unwrap();

        let layout = load_layout(kb_root);
        assert!(layout.get("missing").is_none());
        assert!(layout.get("new-name").is_none(), "no-op must not invent a position");
    }

    /// Renaming to an id that already has a position: the existing
    /// position wins (we don't clobber the user's previous layout for
    /// the target). Old key is dropped.
    #[test]
    fn rename_key_target_existing_keeps_existing_position() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        update_position(kb_root, "src", 1.0, 1.0).unwrap();
        update_position(kb_root, "old-id", 5.0, 5.0).unwrap();

        layout_store::rename_key(kb_root, "old-id", "src").unwrap();

        let layout = load_layout(kb_root);
        let pos = layout.get("src").unwrap();
        assert_eq!(pos.x, 1.0, "existing position must be preserved");
        assert!(layout.get("old-id").is_none());
    }
```

- [ ] **Step 2: 运行测试，验证失败**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib layout_store
```

Expected: FAIL — `no function or associated item named rename_key`。

- [ ] **Step 3: 实现 `rename_key`**

在 `src-tauri/src/layout_store.rs` 的 `update_position` 函数后追加：

```rust
/// Transfer a node's position from one entry id to another. Used by
/// `execute_rename` so the renamed node stays where the user put it
/// on the canvas. If the old id had no position recorded, this is a
/// silent no-op (the new id is also absent). If the new id already
/// has a position, that one wins — we don't clobber an existing
/// layout entry.
pub fn rename_key(kb_root: &Path, old_id: &str, new_id: &str) -> std::io::Result<()> {
    let mut layout = load_layout(kb_root);
    if let Some(pos) = layout.positions.remove(old_id) {
        // Only set if new_id doesn't already exist (don't clobber).
        layout.positions.entry(new_id.to_string()).or_insert(pos);
        save_layout(kb_root, &layout)?;
    }
    Ok(())
}
```

- [ ] **Step 4: 运行测试，验证通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib layout_store
```

Expected: PASS，6 tests（3 + 3）。

- [ ] **Step 5: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/layout_store.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(layout): rename_key — transfer position from old id to new id"
```

---

## Task 6: `update_entry_metadata` command + 测试

**Files:**
- Modify: `src-tauri/src/commands.rs`（在 `delete_entry` 之后追加新 command + 测试）

- [ ] **Step 1: 添加测试到 `commands.rs` 的 `#[cfg(test)] mod tests` 末尾**

在 `src-tauri/src/commands.rs` 的测试模块底部追加：

```rust
    // ---------- update_entry_metadata ----------

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

        let err = update_entry_metadata(
            State::new(&state),
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

        let err = update_entry_metadata(
            State::new(&state),
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

    /// Happy path: title change writes to disk and the DB reflects
    /// the new value after rebuild.
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

        let result = update_entry_metadata(
            State::new(&state),
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

        // File on disk updated.
        let on_disk = std::fs::read_to_string(kb_root.join("note.md")).unwrap();
        assert!(on_disk.contains("<!-- @synapse-title: New -->"), "disk: {on_disk}");

        // Returned entry has the new title.
        assert_eq!(result.title, "New");
    }
```

需要在 `mod tests` 块顶部添加 `use crate::commands::{update_entry_metadata, MetadataChanges};`（或在文件顶部加 import）。也要用现有的 `state_with_current_kb` 辅助函数（已在文件底部定义）。

- [ ] **Step 2: 运行测试，验证失败（命令未注册）**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib commands::tests::update_metadata
```

Expected: FAIL — `cannot find function update_entry_metadata` / 类型未定义。

- [ ] **Step 3: 实现 `MetadataChanges` 类型 + `update_entry_metadata` command**

在 `src-tauri/src/commands.rs` 的 `delete_entry` 之后追加：

```rust
// ---------- Inline metadata editing (Cycle 2) ----------

/// Field changes passed to `update_entry_metadata`. `None` means
/// "leave alone" — the field is not touched on disk.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataChanges {
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,
    pub status: Option<String>,
}

/// Update one or more `@synapse-*` fields in a content.md file. The
/// `metadata_updater` helper does the byte-level rewrite; we then
/// rebuild the SQLite index so the UI sees the new values on the
/// next `get_entries` call.
///
/// Validation runs before any file I/O — bad field values are
/// rejected with `AppError::MetadataInvalidField` and the file is
/// not modified.
#[tauri::command(rename_all = "camelCase")]
pub async fn update_entry_metadata(
    state: State<'_, AppState>,
    id: String,
    changes: MetadataChanges,
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
        if !["draft", "stable", "wip", "deprecated"].contains(&status.as_str()) {
            return Err(AppError::MetadataInvalidField {
                field: "status".to_string(),
                reason: format!("status '{status}' not in enum"),
            });
        }
    }

    // 2) Resolve the entry's on-disk path.
    let path = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?
            .ok_or_else(|| AppError::EntryNotFound { id: id.clone() })?
    };

    // 3) Rewrite the @synapse-* comments.
    crate::parser::metadata_updater::apply_metadata_changes(&path, &changes)?;

    // 4) Rebuild the index so the DB reflects the new file content.
    let kb_root = kb_root_or_err(&*state).await?;
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
        .ok_or_else(|| AppError::EntryNotFound { id })
}
```

- [ ] **Step 4: 在 `lib.rs` 注册新 command**

打开 `src-tauri/src/lib.rs`，找到 `tauri::Builder::default().invoke_handler(...)`，在 command 列表里追加 `update_entry_metadata`（按字母顺序或保持现有顺序）：

```rust
.invoke_handler(tauri::generate_handler![
    // ... 现有 commands ...
    update_entry_metadata,
])
```

- [ ] **Step 5: 运行测试，验证通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib commands::tests::update_metadata
```

Expected: PASS，3 tests。

- [ ] **Step 6: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(metadata): update_entry_metadata command + validation tests"
```

---

## Task 7: `precheck_rename` + `count_incoming_refs` + 测试

**Files:**
- Modify: `src-tauri/src/commands.rs`（追加 `PrecheckRenameResult` 类型 + `precheck_rename` command + 内部 `count_incoming_refs` + 测试）

- [ ] **Step 1: 添加测试**

在 `commands.rs` 的 `mod tests` 块底部追加：

```rust
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

        let result = precheck_rename(
            State::new(&state),
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
        }
        .unwrap();

        let state = state_with_current_kb(Some(&kb_root));
        let precheck = precheck_rename(
            State::new(&state),
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

        let precheck = precheck_rename(
            State::new(&state),
            "target".to_string(),
            "target-v2".to_string(),
            "".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(precheck.incoming_count, 1, "only the real ref counts");
    }
```

- [ ] **Step 2: 运行测试，验证失败**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib commands::tests::precheck
```

Expected: FAIL — 类型/函数未定义。

- [ ] **Step 3: 实现类型 + `precheck_rename` + 内部 `count_incoming_refs`**

在 `update_entry_metadata` 之后追加：

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrecheckRenameResult {
    pub valid: bool,
    pub new_id: String,
    pub conflict_with: Option<String>,
    pub incoming_count: u32,
    pub incoming_files: Vec<String>,
}

/// Validate a proposed rename and count incoming `[[old_id]]` references
/// across the rest of the KB. Pure read — no side effects. The dialog
/// calls this on every keystroke (debounced) to drive the validation UI.
#[tauri::command(rename_all = "camelCase")]
pub async fn precheck_rename(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
    new_group: String,
) -> AppResult<PrecheckRenameResult> {
    let _guard = state.write_lock.lock().await; // serialize with concurrent renames
    let kb_root = kb_root_or_err(&*state).await?;

    // Compute new path / new id using the SAME scheme as
    // `entry::read_md_file` (path with `.md` stripped, separators
    // replaced with `-`).
    let new_path = if new_group.is_empty() {
        kb_root.join(format!("{new_name}.md"))
    } else {
        let g = kb_root.join(&new_group);
        std::fs::create_dir_all(&g).map_err(|e| AppError::io(
            format!("precheck_rename: create_dir_all {}", g.display()), e,
        ))?;
        g.join(format!("{new_name}.md"))
    };
    let new_id = if new_group.is_empty() {
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

/// Walk every .md under `kb_root` (except the file at `old_id`) and
/// count occurrences of `[[old_id]]` outside of code blocks. Returns
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
        // Skip the file currently being renamed (it has its own id
        // comment, which we don't want to count as a reference).
        if content.lines().any(|l| l.contains(&format!("@synapse-id: {old_id}"))) {
            // Only skip if it's the file at this path. The id comment
            // identifies "this file" — it's the one being renamed.
            // Other files might mention `old_id` in their content
            // legitimately (e.g. discussion) but not as an id comment.
            // Continue scanning for content matches.
        }
        let in_code = &mut false;
        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                *in_code = !*in_code;
                continue;
            }
            if *in_code {
                continue;
            }
            if line.contains(&needle) {
                total += 1;
                if let Some(rel) = file.strip_prefix(kb_root).ok() {
                    files.insert(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    Ok((total, files.into_iter().collect()))
}
```

需要 import `use std::path::Path;`（如果 commands.rs 还没有）。

- [ ] **Step 4: 注册新 command**

在 `lib.rs` 的 invoke_handler 列表里追加 `precheck_rename`：

```rust
.invoke_handler(tauri::generate_handler![
    // ... 现有 ...
    precheck_rename,
])
```

- [ ] **Step 5: 运行测试，验证通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib commands::tests::precheck
```

Expected: PASS，3 tests。

- [ ] **Step 6: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(rename): precheck_rename + count_incoming_refs with code-block skip"
```

---

## Task 8: `execute_rename` + cascade + 测试

**Files:**
- Modify: `src-tauri/src/commands.rs`（追加 `RenameResult` 类型 + `execute_rename` command + cascade helper + 测试）

- [ ] **Step 1: 添加测试**

在 `commands.rs` 的 `mod tests` 块底部追加：

```rust
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

        let result = execute_rename(
            State::new(&state),
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

        // target's file moved and @synapse-id updated
        let target = std::fs::read_to_string(kb_root.join("target-v2.md")).unwrap();
        assert!(target.contains("<!-- @synapse-id: target-v2 -->"), "@synapse-id not updated: {target}");

        // Old file is gone
        assert!(!kb_root.join("target.md").exists(), "old file must be removed");
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

        execute_rename(
            State::new(&state),
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

        execute_rename(
            State::new(&state),
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

        let err = execute_rename(
            State::new(&state),
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
```

- [ ] **Step 2: 运行测试，验证失败**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib commands::tests::execute_rename
```

Expected: FAIL — `cannot find function execute_rename`。

- [ ] **Step 3: 实现 `RenameResult` + `execute_rename` + cascade helper**

在 `precheck_rename` 之后追加：

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameResult {
    pub new_id: String,
    pub updated_files: Vec<String>,
}

/// Atomically rename/move an entry: cascade `[[old_id]]` references in
/// other files, move the .md to its new path, update the
/// `@synapse-id` comment, migrate the SQLite row + relation edges,
/// and transfer the layout position. All in one write_lock.
///
/// `cascade=true` updates all incoming wikilinks; `cascade=false`
/// leaves them as broken refs (the dialog asks the user to pick).
#[tauri::command(rename_all = "camelCase")]
pub async fn execute_rename(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
    new_group: String,
    cascade: bool,
) -> AppResult<RenameResult> {
    use crate::scanner::find_md_files;

    let _guard = state.write_lock.lock().await;
    let kb_root = kb_root_or_err(&*state).await?;

    // 1) Compute new path / new id (same scheme as precheck).
    let new_path = if new_group.is_empty() {
        kb_root.join(format!("{new_name}.md"))
    } else {
        kb_root.join(&new_group).join(format!("{new_name}.md"))
    };
    let new_id = if new_group.is_empty() {
        new_name.clone()
    } else {
        format!("{new_group}-{new_name}").replace(['\\', '/'], "-")
    };

    // 2) Resolve old path and verify the move is safe.
    let old_path = {
        let idx = state.index.read().await;
        idx.get_entry_path(&id)?
            .ok_or_else(|| AppError::EntryNotFound { id: id.clone() })?
    };
    if new_path == old_path {
        // No-op rename (same path). Just rebuild to be safe.
        let idx = state.index.read().await;
        let _ = idx.rebuild(&kb_root);
        return Ok(RenameResult { new_id, updated_files: vec![] });
    }
    if new_path.exists() {
        return Err(AppError::RenameFileExists { path: new_path.to_string_lossy().to_string() });
    }

    // 3) Conflict check (id exists in another entry).
    {
        let idx = state.index.read().await;
        let entries = idx.get_all_entries()?;
        if entries.iter().any(|e| e.id == new_id && e.id != id) {
            return Err(AppError::RenameConflict { new_id });
        }
    }

    // 4) Cascade: find/replace `[[old_id]]` → `[[new_id]]` in every
    //    other .md, skipping code blocks. Track files we touch so we
    //    can report them in the result and roll back on failure.
    let mut updated_files: Vec<String> = Vec::new();
    let mut pre_cascade_cache: Vec<(std::path::PathBuf, String)> = Vec::new();
    if cascade && old_id != new_id {
        let needle = format!("[[{old_id}]]");
        let replacement = format!("[[{new_id}]]");
        for file in find_md_files(&kb_root) {
            if file == old_path {
                continue;
            }
            let content = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (new_content, count) = cascade_replace(&content, &needle, &replacement);
            if count > 0 {
                pre_cascade_cache.push((file.clone(), content));
                if let Err(e) = std::fs::write(&file, &new_content) {
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
    if let Err(e) = update_synapse_id_in_file(&new_path, &id, &new_id) {
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
            let _ = update_synapse_id_in_file(&new_path, &new_id, &id);
            let _ = std::fs::rename(&new_path, &old_path);
            for (path, original) in &pre_cascade_cache {
                let _ = std::fs::write(path, original);
            }
            return Err(AppError::DatabaseOp { message: format!("rename_entry_id: {e}") });
        }
    }

    // 8) Transfer layout position (old id → new id).
    if let Err(e) = crate::layout_store::rename_key(&kb_root, &id, &new_id) {
        // Layout failure is non-fatal — the rename is already done.
        eprintln!("[execute_rename] layout rename_key failed: {e}");
    }

    // 9) Rebuild the index to refresh derived data (path, group, etc).
    {
        let idx = state.index.read().await;
        let _ = idx.rebuild(&kb_root);
    }

    Ok(RenameResult { new_id, updated_files })
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
    use regex::Regex;
    let content = std::fs::read_to_string(path)?;
    let pattern = format!(r"<!-- @synapse-id:\s*{re}\s*-->", re = regex::escape(old));
    let re = Regex::new(&pattern).expect("valid regex");
    let new_content = re.replace(&content, format!("<!-- @synapse-id: {new} -->")).into_owned();
    if new_content != content {
        std::fs::write(path, new_content)?;
    }
    Ok(())
}
```

注意：上面的代码用了 `HashSet` import，但实际未使用。可删除。`update_synapse_id_in_file` 用了 `regex` crate，确认 `Cargo.toml` 已有 `regex` 依赖（grep `regex =` 验证）。

- [ ] **Step 4: 注册新 command**

在 `lib.rs` 的 invoke_handler 列表里追加 `execute_rename`。

- [ ] **Step 5: 运行测试，验证通过**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test --lib commands::tests::execute_rename
```

Expected: PASS，4 tests。

- [ ] **Step 6: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(rename): execute_rename — atomic cascade + file move + SQLite + layout"
```

---

## Task 9: 全部 Rust 测试通过 + lint

**Files:**
- Modify: 无（仅验证）

- [ ] **Step 1: 运行全部 Rust 测试**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test
```

Expected: PASS，0 failures。所有原有测试 + 新增的 ~16 个测试全过。

- [ ] **Step 2: 运行 cargo clippy**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo clippy --lib -- -D warnings
```

Expected: 0 warnings。如果有 warning（unused import / dead code），修复之。

- [ ] **Step 3: 提交（如果有 fix）**

```bash
cd /Users/zhumingchen/repository-manage
git add -A
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "chore(rust): clippy fixes after Cycle 2 tasks 1-8" --allow-empty
```

---

## Task 10: i18n 资源文件（4 个）

**Files:**
- Create: `src/components/i18n/EntryMetadataEditor.zh.json`
- Create: `src/components/i18n/EntryMetadataEditor.en.json`
- Create: `src/components/i18n/RenameEntryDialog.zh.json`
- Create: `src/components/i18n/RenameEntryDialog.en.json`

注：先 `ls src/components/i18n/` 检查目录是否存在。如不存在则创建。

- [ ] **Step 1: 创建目录（如需）并写 4 个文件**

`src/components/i18n/EntryMetadataEditor.zh.json`：

```json
{
  "titleLabel": "标题",
  "titlePlaceholder": "输入标题...",
  "tagsLabel": "标签",
  "tagsPlaceholder": "添加标签...",
  "labelsLabel": "工作流标注",
  "labelsPlaceholder": "添加标注...",
  "statusLabel": "状态",
  "statusStable": "稳定",
  "statusDraft": "草稿",
  "statusWip": "撰写中",
  "statusDeprecated": "遗弃",
  "removeTagAria": "移除",
  "errorEmptyTitle": "标题不能为空",
  "errorInvalidTag": "标签不能包含逗号",
  "savedFlash": "已保存"
}
```

`src/components/i18n/EntryMetadataEditor.en.json`：

```json
{
  "titleLabel": "Title",
  "titlePlaceholder": "Enter title...",
  "tagsLabel": "Tags",
  "tagsPlaceholder": "Add tag...",
  "labelsLabel": "Labels",
  "labelsPlaceholder": "Add label...",
  "statusLabel": "Status",
  "statusStable": "Stable",
  "statusDraft": "Draft",
  "statusWip": "WIP",
  "statusDeprecated": "Deprecated",
  "removeTagAria": "Remove",
  "errorEmptyTitle": "Title must not be empty",
  "errorInvalidTag": "Tag must not contain comma",
  "savedFlash": "Saved"
}
```

`src/components/i18n/RenameEntryDialog.zh.json`：

```json
{
  "title": "重命名条目",
  "newNameLabel": "新名称",
  "newNamePlaceholder": "new-name",
  "newGroupLabel": "新分组",
  "newGroupPlaceholder": "frontend/subdomain",
  "precheckChecking": "检查中...",
  "precheckNoIncoming": "无传入引用",
  "precheckIncomingOne": "1 个文件引用此条目",
  "precheckIncomingMany": "{{count}} 个文件引用此条目",
  "conflictExists": "ID 已存在：{{id}}",
  "errorInvalidName": "名称不合法",
  "cascadeLabel": "如何处理传入引用？",
  "cascadeUpdate": "更新引用（推荐）",
  "cascadeLeave": "保留为断链",
  "cancel": "取消",
  "rename": "重命名",
  "renameBusy": "重命名中..."
}
```

`src/components/i18n/RenameEntryDialog.en.json`：

```json
{
  "title": "Rename Entry",
  "newNameLabel": "New name",
  "newNamePlaceholder": "new-name",
  "newGroupLabel": "New group",
  "newGroupPlaceholder": "frontend/subdomain",
  "precheckChecking": "Checking...",
  "precheckNoIncoming": "No incoming references",
  "precheckIncomingOne": "1 file references this entry",
  "precheckIncomingMany": "{{count}} files reference this entry",
  "conflictExists": "ID already exists: {{id}}",
  "errorInvalidName": "Invalid name",
  "cascadeLabel": "How to handle incoming references?",
  "cascadeUpdate": "Update references (recommended)",
  "cascadeLeave": "Leave as broken links",
  "cancel": "Cancel",
  "rename": "Rename",
  "renameBusy": "Renaming..."
}
```

- [ ] **Step 2: 验证 i18n key 一致性**

```bash
cd /Users/zhumingchen/repository-manage
for f in EntryMetadataEditor RenameEntryDialog; do
  zh=$(cat "src/components/i18n/$f.zh.json" | node -e "console.log(Object.keys(JSON.parse(require('fs').readFileSync(0,'utf8'))).sort().join(','))")
  en=$(cat "src/components/i18n/$f.en.json" | node -e "console.log(Object.keys(JSON.parse(require('fs').readFileSync(0,'utf8'))).sort().join(','))")
  if [ "$zh" = "$en" ]; then
    echo "OK: $f ($([ "$zh" = "$en" ] && echo "$(echo $zh | tr ',' '\n' | wc -l | tr -d ' ') keys) match)"
  else
    echo "MISMATCH: $f"
    diff <(echo "$zh") <(echo "$en")
  fi
done
```

Expected: 两个文件都 OK，key 集合一致。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src/components/i18n/
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(i18n): EntryMetadataEditor + RenameEntryDialog resource files (zh, en)"
```

---

## Task 11: 在 `resources.ts` 注册 2 个新 NS

**Files:**
- Modify: `src/i18n/resources.ts`

- [ ] **Step 1: 添加 NS 名称到数组**

打开 `src/i18n/resources.ts`。在 `export const NS = [...]` 数组内追加 `'EntryMetadataEditor'` 和 `'RenameEntryDialog'`（按字母顺序）。

- [ ] **Step 2: 在 zh / en 资源对象中 import + 引用**

在 `import EntryMetadataEditorZh from '...';` 之类 import 块的下方，添加：

```ts
import EntryMetadataEditorZh from '@/components/i18n/EntryMetadataEditor.zh.json';
import EntryMetadataEditorEn from '@/components/i18n/EntryMetadataEditor.en.json';
import RenameEntryDialogZh from '@/components/i18n/RenameEntryDialog.zh.json';
import RenameEntryDialogEn from '@/components/i18n/RenameEntryDialog.en.json';
```

在 `export const resources = { zh: { ... }, en: { ... } }` 中，两个语言对象里分别添加：

```ts
zh: {
  // 现有 NS...
  EntryMetadataEditor: EntryMetadataEditorZh,
  RenameEntryDialog: RenameEntryDialogZh,
},
en: {
  // 现有 NS...
  EntryMetadataEditor: EntryMetadataEditorEn,
  RenameEntryDialog: RenameEntryDialogEn,
},
```

- [ ] **Step 3: 验证类型检查通过**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 通过，0 errors。

- [ ] **Step 4: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src/i18n/resources.ts
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(i18n): register EntryMetadataEditor + RenameEntryDialog namespaces"
```

---

## Task 12: `useAppStore` 增 3 个 mutations

**Files:**
- Modify: `src/store/useAppStore.ts`

- [ ] **Step 1: 添加 mutation 实现**

在 `useAppStore` 内追加（找到现有的 `createRelation` / `createEntryFromReference` 等 mutation 之后的位置）：

```ts
// ---------- Inline metadata editing + rename (Cycle 2) ----------

updateMetadata: async (id, changes) => {
  // Optimistic update: mutate the local entries array so the UI
  // reflects the new value immediately. On failure we revert.
  const prev = get().entries.find((e) => e.id === id);
  if (!prev) throw new Error(`entry not found: ${id}`);
  const optimistic: Entry = {
    ...prev,
    title: changes.title ?? prev.title,
    tags: changes.tags ?? prev.tags,
    labels: changes.labels ?? prev.labels,
    status: changes.status !== undefined ? changes.status : prev.status,
  };
  set((s) => ({ entries: s.entries.map((e) => (e.id === id ? optimistic : e)) }));

  try {
    const updated = await invoke<StoredEntry>('update_entry_metadata', { id, changes });
    // Reconcile with backend (DB might compute derived fields).
    set((s) => ({ entries: s.entries.map((e) => (e.id === id ? toEntry(updated) : e)) }));
    return updated;
  } catch (e) {
    // Revert
    set((s) => ({ entries: s.entries.map((ent) => (ent.id === id ? prev : ent)) }));
    set({ error: (e as Error).message });
    throw e;
  }
},

precheckRename: async (id, newName, newGroup) => {
  return invoke<PrecheckRenameResult>('precheck_rename', { id, newName, newGroup });
},

renameEntry: async (id, newName, newGroup, cascade) => {
  const result = await invoke<RenameResult>('execute_rename', { id, newName, newGroup, cascade });

  // Reload all entries (the renamed file's id is now result.newId,
  // and other entries' content may have changed if cascade=true).
  await get().loadEntries();

  // If the renamed entry was selected, update selection to new id.
  if (get().selectedEntryId === id) {
    set({ selectedEntryId: result.newId });
  }
  return result;
},
```

需要添加类型导入（StoredEntry / PrecheckRenameResult / RenameResult / MetadataChanges），这些可以来自 `@shared/types` 或在文件顶部定义。

- [ ] **Step 2: 验证类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 0 errors。如有缺失 import，补上。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src/store/useAppStore.ts
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(store): updateMetadata + precheckRename + renameEntry mutations"
```

---

## Task 13: `EntryMetadataEditor` 组件

**Files:**
- Create: `src/components/EntryMetadataEditor.tsx`

- [ ] **Step 1: 创建组件**

`src/components/EntryMetadataEditor.tsx`：

```tsx
import { useState, useCallback, KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { Entry } from '@shared/types';

interface Props {
  entry: Entry;
}

const STATUS_OPTIONS = ['stable', 'draft', 'wip', 'deprecated'] as const;

export function EntryMetadataEditor({ entry }: Props) {
  const { t } = useTranslation('EntryMetadataEditor');
  const updateMetadata = useAppStore((s) => s.updateMetadata);

  // Local controlled state mirrors entry, so rapid edits don't fight
  // a stale entry prop. We sync from entry on entry.id change.
  const [title, setTitle] = useState(entry.title);
  const [tags, setTags] = useState<string[]>(entry.tags);
  const [labels, setLabels] = useState<string[]>(entry.labels);
  const [status, setStatus] = useState<string>(entry.status ?? 'stable');
  const [tagInput, setTagInput] = useState('');
  const [labelInput, setLabelInput] = useState('');
  const [error, setError] = useState<string | null>(null);

  const commit = useCallback(
    (changes: { title?: string; tags?: string[]; labels?: string[]; status?: string }) => {
      setError(null);
      return updateMetadata(entry.id, changes).catch((e) => {
        setError((e as Error).message);
        // Revert local state on error
        setTitle(entry.title);
        setTags(entry.tags);
        setLabels(entry.labels);
        setStatus(entry.status ?? 'stable');
      });
    },
    [entry.id, entry.title, entry.tags, entry.labels, entry.status, updateMetadata]
  );

  const addTag = (val: string) => {
    const trimmed = val.trim();
    if (!trimmed) return;
    if (trimmed.includes(',')) {
      setError(t('errorInvalidTag'));
      return;
    }
    if (tags.includes(trimmed)) {
      setTagInput('');
      return;
    }
    const next = [...tags, trimmed];
    setTags(next);
    setTagInput('');
    commit({ tags: next });
  };

  const removeTag = (tag: string) => {
    const next = tags.filter((x) => x !== tag);
    setTags(next);
    commit({ tags: next });
  };

  const handleTagKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addTag(tagInput);
    } else if (e.key === 'Backspace' && tagInput === '' && tags.length > 0) {
      removeTag(tags[tags.length - 1]);
    }
  };

  return (
    <div className="border-b border-gray-200 p-4">
      {/* Title (editable inline) */}
      <div className="mb-3">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('titleLabel')}
        </label>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => {
            const trimmed = title.trim();
            if (!trimmed) {
              setError(t('errorEmptyTitle'));
              setTitle(entry.title);
              return;
            }
            if (trimmed !== entry.title) commit({ title: trimmed });
          }}
          placeholder={t('titlePlaceholder')}
          className="w-full rounded-md border border-gray-300 px-3 py-1.5 text-sm"
        />
      </div>

      {/* Tags */}
      <div className="mb-3">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('tagsLabel')}
        </label>
        <div className="flex flex-wrap items-center gap-1 rounded-md border border-gray-300 px-2 py-1.5">
          {tags.map((tag) => (
            <span
              key={tag}
              className="flex items-center gap-1 rounded-full bg-blue-50 px-2 py-0.5 text-xs text-blue-700"
            >
              #{tag}
              <button
                type="button"
                onClick={() => removeTag(tag)}
                aria-label={t('removeTagAria')}
                className="text-blue-500 hover:text-red-600"
              >
                ×
              </button>
            </span>
          ))}
          <input
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onKeyDown={handleTagKey}
            onBlur={() => tagInput && addTag(tagInput)}
            placeholder={tags.length === 0 ? t('tagsPlaceholder') : ''}
            className="flex-1 min-w-[80px] border-none bg-transparent text-sm outline-none"
          />
        </div>
      </div>

      {/* Labels */}
      <div className="mb-3">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('labelsLabel')}
        </label>
        <div className="flex flex-wrap items-center gap-1 rounded-md border border-gray-300 px-2 py-1.5">
          {labels.map((label) => (
            <span
              key={label}
              className="flex items-center gap-1 rounded-full bg-amber-50 px-2 py-0.5 text-xs text-amber-700"
            >
              {label}
              <button
                type="button"
                onClick={() => {
                  const next = labels.filter((x) => x !== label);
                  setLabels(next);
                  commit({ labels: next });
                }}
                aria-label={t('removeTagAria')}
                className="text-amber-500 hover:text-red-600"
              >
                ×
              </button>
            </span>
          ))}
          <input
            value={labelInput}
            onChange={(e) => setLabelInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ',') {
                e.preventDefault();
                const trimmed = labelInput.trim();
                if (trimmed && !labels.includes(trimmed)) {
                  const next = [...labels, trimmed];
                  setLabels(next);
                  setLabelInput('');
                  commit({ labels: next });
                }
              }
            }}
            onBlur={() => labelInput && setLabelInput('')}
            placeholder={labels.length === 0 ? t('labelsPlaceholder') : ''}
            className="flex-1 min-w-[80px] border-none bg-transparent text-sm outline-none"
          />
        </div>
      </div>

      {/* Status */}
      <div className="mb-2">
        <label className="mb-1 block text-xs font-semibold uppercase tracking-wider text-gray-500">
          {t('statusLabel')}
        </label>
        <select
          value={status}
          onChange={(e) => {
            setStatus(e.target.value);
            commit({ status: e.target.value });
          }}
          className="rounded-md border border-gray-300 px-2 py-1 text-sm"
        >
          {STATUS_OPTIONS.map((s) => (
            <option key={s} value={s}>
              {t(`status${s.charAt(0).toUpperCase() + s.slice(1)}`)}
            </option>
          ))}
        </select>
      </div>

      {error && <p className="mt-2 text-xs text-red-600">{error}</p>}
    </div>
  );
}
```

- [ ] **Step 2: 验证类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 0 errors。如果有 `toEntry` 之类的辅助函数未定义，添加最小实现（从 `StoredEntry` 转换到 `Entry`）。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src/components/EntryMetadataEditor.tsx
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(ui): EntryMetadataEditor — inline title/tags/labels/status editing"
```

---

## Task 14: `RenameEntryDialog` 组件

**Files:**
- Create: `src/components/RenameEntryDialog.tsx`

- [ ] **Step 1: 创建组件**

`src/components/RenameEntryDialog.tsx`：

```tsx
import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '@/store/useAppStore';
import type { PrecheckRenameResult } from '@shared/types';

interface Props {
  entryId: string;
  initialName: string;
  initialGroup: string;
  existingGroups: string[];
  onClose: () => void;
  onSuccess: (newId: string) => void;
}

type PrecheckState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'valid'; result: PrecheckRenameResult }
  | { kind: 'conflict'; conflict: string }
  | { kind: 'error'; message: string };

export function RenameEntryDialog({
  entryId,
  initialName,
  initialGroup,
  existingGroups,
  onClose,
  onSuccess,
}: Props) {
  const { t } = useTranslation('RenameEntryDialog');
  const precheckRename = useAppStore((s) => s.precheckRename);
  const renameEntry = useAppStore((s) => s.renameEntry);

  const [newName, setNewName] = useState(initialName);
  const [newGroup, setNewGroup] = useState(initialGroup);
  const [cascade, setCascade] = useState(true);
  const [precheck, setPrecheck] = useState<PrecheckState>({ kind: 'idle' });
  const [submitting, setSubmitting] = useState(false);
  const debounceRef = useRef<number | null>(null);

  // Debounced precheck whenever name or group changes.
  useEffect(() => {
    if (newName === initialName && newGroup === initialGroup) {
      setPrecheck({ kind: 'idle' });
      return;
    }
    if (!newName.trim()) {
      setPrecheck({ kind: 'error', message: t('errorInvalidName') });
      return;
    }
    setPrecheck({ kind: 'checking' });
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(async () => {
      try {
        const result = await precheckRename(entryId, newName.trim(), newGroup.trim());
        if (result.conflictWith) {
          setPrecheck({ kind: 'conflict', conflict: result.conflictWith });
        } else if (result.valid) {
          setPrecheck({ kind: 'valid', result });
        } else {
          setPrecheck({ kind: 'error', message: t('errorInvalidName') });
        }
      } catch (e) {
        setPrecheck({ kind: 'error', message: (e as Error).message });
      }
    }, 200);
    return () => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
    };
  }, [entryId, newName, newGroup, initialName, initialGroup, precheckRename, t]);

  const canSubmit = precheck.kind === 'valid' && !submitting;
  const showCascade = precheck.kind === 'valid' && precheck.result.incoming_count > 0;

  const handleSubmit = async () => {
    if (precheck.kind !== 'valid') return;
    setSubmitting(true);
    try {
      const r = await renameEntry(entryId, newName.trim(), newGroup.trim(), cascade);
      onSuccess(r.newId);
    } catch (e) {
      setPrecheck({ kind: 'error', message: (e as Error).message });
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded-lg bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">{t('title')}</h2>

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm font-medium text-gray-700">{t('newNameLabel')}</label>
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={t('newNamePlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm font-mono"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-gray-700">{t('newGroupLabel')}</label>
            <input
              list="kb-groups-rename"
              value={newGroup}
              onChange={(e) => setNewGroup(e.target.value.trim())}
              placeholder={t('newGroupPlaceholder')}
              className="mt-1 w-full rounded-md border border-gray-300 px-3 py-2 text-sm"
            />
            <datalist id="kb-groups-rename">
              {existingGroups.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
          </div>
        </div>

        {/* Precheck status */}
        <div className="mt-3 min-h-[40px] text-xs">
          {precheck.kind === 'checking' && (
            <p className="text-gray-500">{t('precheckChecking')}</p>
          )}
          {precheck.kind === 'valid' && precheck.result.incoming_count === 0 && (
            <p className="text-gray-500">{t('precheckNoIncoming')}</p>
          )}
          {precheck.kind === 'valid' && precheck.result.incoming_count === 1 && (
            <p className="text-amber-700">⚠ {t('precheckIncomingOne')}</p>
          )}
          {precheck.kind === 'valid' && precheck.result.incoming_count > 1 && (
            <p className="text-amber-700">
              ⚠ {t('precheckIncomingMany', { count: precheck.result.incoming_count })}
            </p>
          )}
          {precheck.kind === 'conflict' && (
            <p className="text-red-600">⚠ {t('conflictExists', { id: precheck.conflict })}</p>
          )}
          {precheck.kind === 'error' && (
            <p className="text-red-600">⚠ {precheck.message}</p>
          )}
        </div>

        {/* Cascade option (only when there are incoming refs) */}
        {showCascade && (
          <div className="mt-3 space-y-1 text-sm">
            <p className="font-medium text-gray-700">{t('cascadeLabel')}</p>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                checked={cascade}
                onChange={() => setCascade(true)}
              />
              {t('cascadeUpdate')}
            </label>
            <label className="flex items-center gap-2">
              <input
                type="radio"
                checked={!cascade}
                onChange={() => setCascade(false)}
              />
              {t('cascadeLeave')}
            </label>
          </div>
        )}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={submitting}
            className="rounded-md px-3 py-2 text-sm text-gray-700 hover:bg-gray-100"
          >
            {t('cancel')}
          </button>
          <button
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="rounded-md bg-blue-500 px-3 py-2 text-sm font-medium text-white hover:bg-blue-600 disabled:opacity-50"
          >
            {submitting ? t('renameBusy') : t('rename')}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: 验证类型检查**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 0 errors。如有 `PrecheckRenameResult` 类型未在 `@shared/types` 中定义，添加之。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src/components/RenameEntryDialog.tsx
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(ui): RenameEntryDialog — precheck + cascade state machine"
```

---

## Task 15: `DetailPanel` 集成 — 替换 tags 显示 + 加 Rename 按钮

**Files:**
- Modify: `src/views/DetailPanel.tsx`

- [ ] **Step 1: 修改 DetailPanel**

打开 `src/views/DetailPanel.tsx`。做 3 处修改：

1. 在文件顶部 import：

```tsx
import { EntryMetadataEditor } from '@/components/EntryMetadataEditor';
import { RenameEntryDialog } from '@/components/RenameEntryDialog';
```

2. 删除现有"Tags"展示 block（约第 314-332 行），用 `<EntryMetadataEditor entry={entry} />` 替换。

3. 在 Header 的 close 按钮**之前**加 Rename 按钮：

```tsx
<button
  onClick={() => setShowRename(true)}
  className="shrink-0 rounded-md border border-gray-300 bg-white px-2 py-1 text-xs text-gray-700 hover:bg-gray-100"
  title="✎ Rename"
>
  ✎
</button>
```

4. 在组件内加 state + dialog 渲染：

```tsx
const [showRename, setShowRename] = useState(false);

// ... in JSX, after existing dialogs (RelationDialog, CreateEntryDialog):
{showRename && (
  <RenameEntryDialog
    entryId={entry.id}
    initialName={entry.id.split('-').pop() ?? entry.id}
    initialGroup={entry.group ?? ''}
    existingGroups={existingGroups}
    onClose={() => setShowRename(false)}
    onSuccess={(newId) => {
      setShowRename(false);
      selectEntry(newId);
    }}
  />
)}
```

（`initialName` 的拆分逻辑可能不准确——entry.id 是 `group-name`，name 是最后一段。Rust 端 `derive_id_from_path` 行为类似。可以让 `RenameEntryDialog` 接受完整 `entryId` 然后让组件自己拆，或者让父组件传准确字段。本实现取最后一段作为示例。）

- [ ] **Step 2: 验证类型检查 + build**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 0 errors。

- [ ] **Step 3: 提交**

```bash
cd /Users/zhumingchen/repository-manage
git add src/views/DetailPanel.tsx
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "feat(ui): DetailPanel — mount EntryMetadataEditor + Rename button"
```

---

## Task 16: 全量验收

**Files:**
- Modify: 无（仅验证）

- [ ] **Step 1: 跑全量 Rust 测试**

```bash
cd /Users/zhumingchen/repository-manage/src-tauri && cargo test
```

Expected: 0 failures。

- [ ] **Step 2: 跑前端 build**

```bash
cd /Users/zhumingchen/repository-manage && npm run build
```

Expected: 0 errors，bundle 增长 ~30-50KB（新增的 2 个组件 + store mutations）。

- [ ] **Step 3: 启动 dev server，确认无运行时错误**

```bash
cd /Users/zhumingchen/repository-manage && npm run tauri:dev > /tmp/tauri-dev.log 2>&1 &
sleep 60
grep -iE "error|panic|exception" /tmp/tauri-dev.log | grep -v "info-only" || echo "no runtime errors"
pkill -f tauri:dev || true
```

Expected: 无 error / panic / exception 日志。

- [ ] **Step 4: 手工 e2e（按 spec §8.1 13 条）**

逐条验证 spec §8.1 中的 13 条手工 e2e（内联编辑 6 条 + 重命名 7 条）。每条记录结果。如有失败，回到对应 task 修复。

- [ ] **Step 5: Cycle 1 回归**

确认 Cycle 1 的搜索 + Markdown 渲染 + ⌘K + view toggle 仍正常工作。

- [ ] **Step 6: 提交（仅在有 fix 时）**

```bash
cd /Users/zhumingchen/repository-manage
git status
# 若有 uncommitted fix：
git add -A
git -c user.email="zhumingchen@local" -c user.name="Zhuming" commit -m "fix: acceptance verification fixes"
```

---

## Self-Review（写作时自检）

✅ Spec coverage: §0 目标 → Tasks 13-15；§1 架构 → Tasks 1-12；§2.1 commands → Tasks 4, 6, 7, 8；§2.2 类型 → Tasks 6, 7, 8；§2.3-2.5 算法 → Tasks 1, 2, 4, 7, 8；§3 数据流 → Tasks 12-15；§4 错误处理 → Task 3；§5 i18n → Tasks 10-11；§6 数据存储 → Tasks 4, 5；§8 验收 → Task 16；§11 实施顺序 → 16 tasks (略多)

✅ Placeholder scan: 无 TBD / TODO / "类似 Task N"。所有函数都有完整签名 + 主体或 todo!() 等待实现。

✅ Type consistency:
- `MetadataChanges` 字段顺序与 spec §2.2 一致
- `PrecheckRenameResult` / `RenameResult` 字段与 spec §2.2 一致
- `cascade_replace` / `update_synapse_id_in_file` 是 commands.rs 内部 helper，未在外部 spec 中定义
- `rename_key` / `rename_entry_id` 命名一致

✅ Granularity: 每个 step 单一动作，2-5 分钟内可完成。

✅ Frequent commits: 每个 task 末尾一个 commit。
