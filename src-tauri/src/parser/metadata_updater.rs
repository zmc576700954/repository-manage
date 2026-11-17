//! 元数据写入：frontmatter 保序写入器（fm_writer）的应用入口。
//!
//! Q2.a / Q6.a：YAML frontmatter 是元数据唯一真源，`@synapse-*` HTML 注释
//! 降为只读兼容。本模块把 UI 驱动的元数据编辑写入 frontmatter
//! （保未知键 / 键序 / 首行位置），不再插入 HTML 注释。

use std::path::Path;
use crate::error::{AppError, AppResult};
use crate::parser::fm_writer::{apply_edits, has_key, FmValue, FrontmatterEdit};

/// 批次 3（MCP dry_run）：与 [`apply_metadata_changes`] 同一变换，但
/// 纯读——返回改写后的完整文件内容，不落任何字节。
pub fn preview_metadata_changes(
    path: &Path,
    changes: &MetadataChanges,
) -> AppResult<String> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::io(format!("metadata_updater: read {}", path.display()), e)
    })?;
    Ok(render_metadata_edits(&content, changes))
}

/// `changes` → frontmatter 编辑 → 应用到 `content` 的纯函数。
/// `apply_metadata_changes` 与 `preview_metadata_changes` 共用，
/// 保证预览与落盘字节级一致。
fn render_metadata_edits(content: &str, changes: &MetadataChanges) -> String {
    let mut edit = FrontmatterEdit::default();
    if let Some(t) = &changes.title {
        edit.set.push(("title".into(), FmValue::Str(t.clone())));
    }
    if let Some(t) = &changes.tags {
        edit.set.push(("tags".into(), FmValue::List(t.clone())));
    }
    if let Some(t) = &changes.labels {
        edit.set.push(("labels".into(), FmValue::List(t.clone())));
    }
    if let Some(s) = &changes.status {
        // 方言路由（D3）：kaiwu 式 frontmatter KB 的状态键是中文 `状态`
        // （M3a 枚举校验的对象）；rm 原生 KB 用 ASCII `status`。
        // 判据：文件 frontmatter 里已有顶层 `状态` 键。
        let status_key = if has_key(content, "状态") { "状态" } else { "status" };
        edit.set.push((status_key.into(), FmValue::Str(s.clone())));
    }
    if edit.set.is_empty() {
        return content.to_string();
    }
    apply_edits(content, &edit)
}

/// Apply a set of metadata changes to a content.md file. Only the
/// frontmatter block is touched; everything else is byte-identical
/// to the input.
pub fn apply_metadata_changes(
    path: &Path,
    changes: &MetadataChanges,
) -> AppResult<()> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::io(format!("metadata_updater: read {}", path.display()), e)
    })?;
    let new_content = render_metadata_edits(&content, changes);
    if new_content != content {
        std::fs::write(path, &new_content).map_err(|e| {
            AppError::io(format!("metadata_updater: write {}", path.display()), e)
        })?;
    }
    Ok(())
}

/// Field changes passed from the frontend. `None` means "leave alone".
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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

    fn write_file(path: &std::path::Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    #[test]
    fn update_title_writes_frontmatter_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_file(
            &path,
            "---\nid: note\ntitle: Old Title\ntags: [type:note]\n---\n\n# Note\n",
        );

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
        assert!(content.contains("title: New Title"), "title key updated: {content}");
        assert!(content.contains("id: note"), "id key untouched: {content}");
        assert!(content.contains("tags: [type:note]"), "tags key untouched: {content}");
        assert!(content.contains("# Note\n"), "markdown body untouched: {content}");
    }

    /// dash-in-value case: the write must survive values with dashes and
    /// colons, replacing (not appending a second) title key.
    #[test]
    fn update_title_with_dash_in_value() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_file(&path, "---\nid: note\ntitle: Old-Dash-Title\n---\n");

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: Some("React - Hooks 深入".to_string()),
                tags: None,
                labels: None,
                status: None,
            },
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("React - Hooks 深入"),
            "new title written: {content}"
        );
        assert!(
            !content.contains("Old-Dash-Title"),
            "old title replaced: {content}"
        );
        let count = content.matches("title:").count();
        assert_eq!(count, 1, "exactly one title key, got {count}:\n{content}");
    }

    #[test]
    fn preserves_unrelated_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let body = "---\nid: note\ntitle: T\n---\n\n# Note\n\nSome **bold** and `code`.\n\n- [[other]]\n- [web](https://x.com)\n";
        write_file(&path, body);

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
        assert!(after.contains("Some **bold** and `code`."), "body preserved");
        assert!(after.contains("- [[other]]"), "wikilink preserved");
        assert!(after.contains("[web](https://x.com)"), "md link preserved");
        assert!(after.ends_with("- [web](https://x.com)\n"), "trailing newline preserved");
    }

    /// kaiwu 式文件（中文四键、无 title 键）：新键追加到区块末尾，四键原样。
    #[test]
    fn appends_key_to_kaiwu_frontmatter_without_touching_four_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("k.md");
        write_file(
            &path,
            "---\n优先级: 🟡 P1\n状态: 稳定\n最后更新: 2026-11-03\n下次 review: 2026-12-03\n---\n\n正文\n",
        );

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: None,
                tags: None,
                labels: None,
                status: Some("已归档".to_string()),
            },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        for key in ["优先级: 🟡 P1", "最后更新: 2026-11-03", "下次 review: 2026-12-03"] {
            assert!(after.contains(key), "{key} preserved: {after}");
        }
        // D3 方言路由：文件已有中文 `状态` 键 → 就地替换（值 = canonical 枚举），
        // 不新增并行的 ASCII status 键。
        assert!(after.contains("状态: 已归档"), "status routed to 状态: {after}");
        assert!(!after.contains("status:"), "no parallel ASCII status key: {after}");
        assert!(after.starts_with("---\n优先级:"), "frontmatter stays first line: {after}");
    }

    /// 无 frontmatter 的老文件：在文件顶部创建区块，而不是插 HTML 注释（F3 事故根因）。
    #[test]
    fn creates_frontmatter_block_at_first_line_for_legacy_comment_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.md");
        write_file(&path, "<!-- @synapse-id: note -->\n# Note\n");

        apply_metadata_changes(
            &path,
            &MetadataChanges {
                title: None,
                tags: Some(vec!["type:note".into()]),
                labels: None,
                status: Some("draft".to_string()),
            },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        assert!(
            after.starts_with("---\n"),
            "frontmatter must be FIRST LINE, no comment injection: {after}"
        );
        assert!(after.contains("status: draft"), "{after}");
        assert!(after.contains("tags: type:note"), "{after}");
        assert!(after.contains("<!-- @synapse-id: note -->"), "legacy comment kept: {after}");
    }

    #[test]
    fn tags_replaces_full_list_not_appends() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_file(&path, "---\nid: note\ntags: [a, b, c]\n---\n");

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
        assert!(after.contains("tags: x, y"), "tags replaced: {after}");
        assert!(!after.contains("a, b, c"), "old tags gone: {after}");
    }

    #[test]
    fn empty_changes_writes_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        let body = "---\nid: note\n# Note\n";
        write_file(&path, body);

        apply_metadata_changes(
            &path,
            &MetadataChanges { title: None, tags: None, labels: None, status: None },
        )
        .unwrap();

        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, body, "no changes should leave file byte-identical");
    }
}
