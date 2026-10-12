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
    let pattern = format!(r"(?m)^(<!-- @synapse-{field}:\s*)([^\n]*?)(\s*-->)$");
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

    /// Pin the dash-in-value case: the regex must NOT exclude dashes
    /// from the captured value, otherwise titles like "React - Hooks"
    /// or tags like "high-priority" silently fail to update and the
    /// file ends up with DUPLICATE metadata lines.
    #[test]
    fn update_title_with_dash_in_value() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(
            &path,
            "<!-- @synapse-id: note -->\n\
             <!-- @synapse-title: Old-Dash-Title -->\n",
        )
        .unwrap();

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

        // 1. The new line is present.
        assert!(
            content.contains("<!-- @synapse-title: React - Hooks 深入 -->"),
            "new title not written: {content}"
        );

        // 2. The old line is gone (replaced, not appended).
        assert!(
            !content.contains("Old-Dash-Title"),
            "old title still present after update: {content}"
        );

        // 3. Exactly one @synapse-title: line (no duplicate metadata).
        let title_count = content.matches("<!-- @synapse-title:").count();
        assert_eq!(
            title_count, 1,
            "expected exactly one @synapse-title line, got {title_count}:\n{content}"
        );
    }

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
        // 应该插在第一个 synapse 行之后（即 id 之后、title 之前）
        assert!(after.contains("<!-- @synapse-status: draft -->"), "status line not inserted: {after}");
        // 位置校验：status 行应该在 id 之后、title 之前
        let id_pos = after.find("<!-- @synapse-id:").unwrap();
        let title_pos = after.find("<!-- @synapse-title:").unwrap();
        let status_pos = after.find("<!-- @synapse-status:").unwrap();
        assert!(id_pos < status_pos, "status must come after id (first synapse line)");
        assert!(status_pos < title_pos, "status must come BEFORE title (inserted after first line, not at end)");
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
}