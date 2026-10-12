//! E4：在 @synapse-tags 注入 type:note（如果还没有 type: 前缀的 tag）

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_and_apply(
    entry: &RawEntry,
    _entry_dir: &Path,
) -> AppResult<FixDescriptor> {
    // 1. 读 content_path
    let raw = std::fs::read_to_string(&entry.content_path)?;

    // 2. 找到 @synapse-tags 行，注入 type:note
    let new_content = inject_type(&raw);

    // 3. backup + write
    backup::backup_then_write(&entry.content_path, &new_content, "E4")?;

    Ok(FixDescriptor {
        rule_id: "E4".into(),
        kind: FixKind::OneClick,
        risk: FixRisk::Low,
        label_key: "Fix.E4.apply".into(),
        preview_key: Some("Fix.E4.preview".into()),
        confirm_key: None,
        docs_key: "Validation.rule.E4.fixGuide".into(),
    })
}

fn inject_type(raw: &str) -> String {
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("<!-- @synapse-tags:") {
            let rest = rest.trim_end_matches("-->").trim();
            // 已包含 type: 前缀则 no-op
            if rest.split(',').any(|t| t.trim().starts_with("type:")) {
                return raw.to_string();
            }
            // 在 tags 前注入 type:note
            let new_line = format!("<!-- @synapse-tags: type:note, {} -->", rest);
            return raw.replacen(line, &new_line, 1);
        }
    }
    // 没有 @synapse-tags 行，注入新行（放在 frontmatter 前或文件顶）
    let inject = "<!-- @synapse-tags: type:note -->\n";
    if let Some(pos) = raw.find("---\n") {
        let mut out = String::with_capacity(raw.len() + inject.len());
        out.push_str(&raw[..pos]);
        out.push_str(inject);
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
    fn injects_type_into_existing_tags_line() {
        let raw_in = "<!-- @synapse-tags: foo, bar -->\n# Body\n";
        let out = inject_type(raw_in);
        assert!(
            out.contains("@synapse-tags: type:note, foo, bar"),
            "should prepend type:note, got: {}",
            out
        );
    }

    #[test]
    fn no_op_when_type_already_present() {
        let raw_in = "<!-- @synapse-tags: type:book, foo -->\n# Body\n";
        let out = inject_type(raw_in);
        assert_eq!(out, raw_in);
    }

    #[test]
    fn injects_new_tags_line_before_frontmatter() {
        let raw_in = "---\ncolor: red\n---\n\n# Body\n";
        let out = inject_type(raw_in);
        let tag_pos = out.find("@synapse-tags:").unwrap();
        let fm_pos = out.find("---\n").unwrap();
        assert!(tag_pos < fm_pos, "tags comment should precede frontmatter");
    }

    #[test]
    fn injects_new_tags_line_at_top_when_no_frontmatter() {
        let raw_in = "# Body\nplain content\n";
        let out = inject_type(raw_in);
        assert!(
            out.starts_with("<!-- @synapse-tags: type:note -->\n"),
            "tags line should be at top, got: {}",
            out
        );
    }

    #[test]
    fn descriptor_and_apply_writes_type_to_file() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("my-entry");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        std::fs::write(&content_path, "# Body\nplain\n").unwrap();
        let entry = make_entry(content_path.clone());
        let desc = descriptor_and_apply(&entry, &dir).unwrap();
        assert_eq!(desc.rule_id, "E4");
        assert!(matches!(desc.kind, FixKind::OneClick));
        assert!(matches!(desc.risk, FixRisk::Low));
        let result = std::fs::read_to_string(&content_path).unwrap();
        assert!(result.contains("@synapse-tags: type:note"));
    }
}
