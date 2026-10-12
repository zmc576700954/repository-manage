//! E3: 写入默认 title（与 id 相同）

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    _entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    let title = entry.id.clone();
    let raw = std::fs::read_to_string(&entry.content_path)?;
    let new_content = inject_synapse_title(&raw, &title);

    backup::backup_then_write(&entry.content_path, &new_content, "E3")?;

    Ok(FixDescriptor {
        rule_id: "E3".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E3.apply".into(),
        preview_key: Some("Fix.E3.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E3.fixGuide".into(),
    })
}

fn inject_synapse_title(raw: &str, title: &str) -> String {
    if raw.lines().any(|l| l.contains("@synapse-title:")) {
        return raw.to_string();
    }
    let inject = format!("<!-- @synapse-title: {} -->\n", title);
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
    use tempfile::TempDir;

    fn make_entry(content_path: std::path::PathBuf) -> RawEntry {
        let parent = content_path.parent().unwrap().to_path_buf();
        RawEntry {
            id: "my-entry".into(),
            title: String::new(),
            tags: vec![],
            group: None,
            path: parent.clone(),
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
    fn injects_title_when_missing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("my-entry");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        std::fs::write(&content_path, "# Body\n").unwrap();
        let entry = make_entry(content_path.clone());
        descriptor_and_apply(&entry, &dir).unwrap();
        let result = std::fs::read_to_string(&content_path).unwrap();
        assert!(
            result.contains("@synapse-title: my-entry"),
            "should inject title comment, got: {}",
            result
        );
    }

    #[test]
    fn injects_before_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("yfm");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        let body = "---\ncolor: red\n---\n\n# Body\n";
        std::fs::write(&content_path, body).unwrap();
        let entry = make_entry(content_path.clone());
        descriptor_and_apply(&entry, &dir).unwrap();
        let result = std::fs::read_to_string(&content_path).unwrap();
        let title_pos = result.find("@synapse-title:").unwrap();
        let fm_pos = result.find("---\n").unwrap();
        assert!(
            title_pos < fm_pos,
            "title comment should precede frontmatter"
        );
    }

    #[test]
    fn no_op_when_already_has_title() {
        let raw_in = "<!-- @synapse-title: foo -->\n# Body\n";
        let out = inject_synapse_title(raw_in, "bar");
        assert_eq!(out, raw_in);
    }

    #[test]
    fn default_title_equals_id() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        std::fs::write(&content_path, "body\n").unwrap();
        let entry = make_entry(content_path.clone());
        descriptor_and_apply(&entry, &dir).unwrap();
        let result = std::fs::read_to_string(&content_path).unwrap();
        assert!(result.contains("@synapse-title: my-entry"));
    }
}
