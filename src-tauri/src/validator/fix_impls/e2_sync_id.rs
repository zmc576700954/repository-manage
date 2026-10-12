//! E2: 同步 @synapse-id 为 entry_dir 目录名（id 与目录不一致时）

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    entry_dir: &Path,
    _confirm: bool,
) -> AppResult<FixDescriptor> {
    let derived = entry_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("entry")
        .to_string();

    let raw = std::fs::read_to_string(&entry.content_path)?;
    let new_content = replace_synapse_id(&raw, &derived);

    backup::backup_then_write(&entry.content_path, &new_content, "E2")?;

    Ok(FixDescriptor {
        rule_id: "E2".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E2.apply".into(),
        preview_key: Some("Fix.E2.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E2.fixGuide".into(),
    })
}

/// 替换已存在的 `@synapse-id: <value>` 行中的 value。
/// 若文件不含 `@synapse-id:` 行则原样返回（应由 E1 处理）。
fn replace_synapse_id(raw: &str, new_id: &str) -> String {
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<!-- @synapse-id:") && trimmed.ends_with("-->") {
            let new_line = format!("<!-- @synapse-id: {} -->\n", new_id);
            return raw.replacen(line, &new_line, 1);
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_entry(path: std::path::PathBuf, content_path: std::path::PathBuf) -> RawEntry {
        RawEntry {
            id: "old-id".into(),
            title: String::new(),
            tags: vec![],
            group: None,
            path,
            content_path,
            attachments: vec![],
            linked_targets: vec![],
            markdown_links: vec![],
            yaml: None,
            has_content_md: true,
            labels: vec![],
            status: None,
            aliases: vec![],
            source: None,
            created: None,
            updated: None,
            ignored_rules: vec![],
            validation: vec![],
        }
    }

    #[test]
    fn rewrites_id_to_match_dir_name() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("react-hooks");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        std::fs::write(&content_path, "<!-- @synapse-id: old-id -->\n# Hooks\n").unwrap();
        let entry = make_entry(dir.clone(), content_path.clone());
        descriptor_and_apply(&entry, &dir, false).unwrap();
        let result = std::fs::read_to_string(&content_path).unwrap();
        assert!(
            result.contains("@synapse-id: react-hooks"),
            "id should be rewritten to match dir name, got: {}",
            result
        );
        assert!(
            !result.contains("@synapse-id: old-id"),
            "old id should be replaced, got: {}",
            result
        );
    }
}