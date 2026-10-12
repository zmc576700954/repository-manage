//! E1: 从路径派生 id 写入 @synapse-id

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    let derived = entry_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("entry")
        .to_string();

    let raw = std::fs::read_to_string(&entry.content_path)?;
    let new_content = inject_synapse_id(&raw, &derived);

    backup::backup_then_write(&entry.content_path, &new_content, "E1")?;

    Ok(FixDescriptor {
        rule_id: "E1".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E1.apply".into(),
        preview_key: Some("Fix.E1.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E1.fixGuide".into(),
    })
}

fn inject_synapse_id(raw: &str, derived: &str) -> String {
    if raw.lines().any(|l| l.contains("@synapse-id:")) {
        return raw.to_string();
    }
    let inject = format!("<!-- @synapse-id: {} -->\n", derived);
    if let Some(pos) = raw.find("---\n") {
        let mut out = String::with_capacity(raw.len() + inject.len());
        out.push_str(&raw[..pos]);
        out.push_str(&inject);
        out.push_str(&raw[pos..]);
        return out;
    }
    format!("{}{}", inject, raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::links::MarkdownLink;
    use crate::validator::ValidationIssue;
    use tempfile::TempDir;

    fn make_entry(path: std::path::PathBuf, content_path: std::path::PathBuf, raw: &str) -> RawEntry {
        RawEntry {
            id: String::new(),
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
    fn injects_id_when_missing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("react-hooks");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        std::fs::write(&content_path, "# Hooks\n").unwrap();
        let entry = make_entry(dir.clone(), content_path.clone(), "# Hooks\n");
        descriptor_and_apply(&entry, &dir).unwrap();
        let result = std::fs::read_to_string(&content_path).unwrap();
        assert!(result.contains("@synapse-id: react-hooks"));
    }

    #[test]
    fn injects_before_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("yfm");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        let body = "---\ncolor: red\n---\n\n# Body\n";
        std::fs::write(&content_path, body).unwrap();
        let entry = make_entry(dir.clone(), content_path.clone(), body);
        descriptor_and_apply(&entry, &dir).unwrap();
        let result = std::fs::read_to_string(&content_path).unwrap();
        let id_pos = result.find("@synapse-id:").unwrap();
        let fm_pos = result.find("---\n").unwrap();
        assert!(id_pos < fm_pos, "id comment should precede frontmatter");
    }

    #[test]
    fn no_op_when_already_has_id() {
        let raw_in = "<!-- @synapse-id: foo -->\n# Title\n";
        let out = inject_synapse_id(raw_in, "bar");
        assert_eq!(out, raw_in);
    }
}