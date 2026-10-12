//! E9：从 @synapse-labels 删除 system 维护的 label 行

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::fs::backup;
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

const SYSTEM_LABELS: &[&str] = &["recently-modified", "orphan", "stale", "broken-link"];

pub fn descriptor_and_apply(
    entry: &RawEntry,
    _entry_dir: &Path,
    confirm: bool,
) -> AppResult<FixDescriptor> {
    if !confirm {
        return Ok(FixDescriptor {
            rule_id: "E9".into(),
            kind: FixKind::Confirm,
            risk: FixRisk::Medium,
            label_key: "Fix.E9.apply".into(),
            preview_key: Some("Fix.E9.preview".into()),
            confirm_key: Some("Fix.E9.confirmTitle".into()),
            docs_key: "Validation.rule.E9.fixGuide".into(),
        });
    }

    let raw = std::fs::read_to_string(&entry.content_path)?;
    let new_content = strip_system_labels(&raw);

    backup::backup_then_write(&entry.content_path, &new_content, "E9")?;

    Ok(FixDescriptor {
        rule_id: "E9".into(),
        kind: FixKind::Confirm,
        risk: FixRisk::Medium,
        label_key: "Fix.E9.apply".into(),
        preview_key: Some("Fix.E9.preview".into()),
        confirm_key: Some("Fix.E9.confirmTitle".into()),
        docs_key: "Validation.rule.E9.fixGuide".into(),
    })
}

/// 过滤掉完全由 system label 组成的 `@synapse-labels:` 行。
/// 包含任意 system label 的整行也会被丢掉（保守策略）。
fn strip_system_labels(raw: &str) -> String {
    let trailing_nl = raw.ends_with('\n');
    let kept: Vec<&str> = raw
        .lines()
        .filter(|l| {
            if let Some(rest) = l.strip_prefix("<!-- @synapse-labels:") {
                let rest = rest.trim_end_matches("-->").trim();
                // 包含任意 system label → 整行丢掉
                !SYSTEM_LABELS
                    .iter()
                    .any(|s| rest.split(',').any(|t| t.trim() == *s))
            } else {
                true
            }
        })
        .collect();
    let mut out = kept.join("\n");
    if trailing_nl && !out.ends_with('\n') {
        out.push('\n');
    }
    out
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
            path: parent,
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

    /// 未 confirm 时返回的 descriptor 形状正确，源文件未触动
    #[test]
    fn descriptor_when_not_confirmed() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("my-entry");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        let raw_in = "<!-- @synapse-labels: recently-modified, orphan -->\n# Body\n";
        std::fs::write(&content_path, raw_in).unwrap();
        let entry = make_entry(content_path.clone());

        let desc = descriptor_and_apply(&entry, &dir, false).unwrap();
        assert_eq!(desc.rule_id, "E9");
        assert!(matches!(desc.kind, FixKind::Confirm));
        assert!(matches!(desc.risk, FixRisk::Medium));
        assert_eq!(desc.label_key, "Fix.E9.apply");
        assert_eq!(desc.preview_key, Some("Fix.E9.preview".into()));
        assert_eq!(desc.confirm_key, Some("Fix.E9.confirmTitle".into()));
        assert_eq!(desc.docs_key, "Validation.rule.E9.fixGuide");

        // 源文件未被触动
        assert_eq!(std::fs::read_to_string(&content_path).unwrap(), raw_in);
    }

    /// 只含 system label 的整行被剥离
    #[test]
    fn strips_all_system_label_line() {
        let raw_in = "<!-- @synapse-labels: recently-modified, orphan -->\n# Body\n";
        let out = strip_system_labels(raw_in);
        assert_eq!(out, "# Body\n");
    }

    /// 包含单个 system label 的行也被剥离
    #[test]
    fn strips_single_system_label_line() {
        let raw_in = "<!-- @synapse-labels: recently-modified -->\n# Body\n";
        let out = strip_system_labels(raw_in);
        assert_eq!(out, "# Body\n");
    }

    /// 不含 system label 的 `@synapse-labels:` 行原样保留
    #[test]
    fn keeps_user_only_label_line() {
        let raw_in = "<!-- @synapse-labels: custom-tag, manual -->\n# Body\n";
        let out = strip_system_labels(raw_in);
        assert_eq!(out, raw_in);
    }

    /// 多个 system label 组合也被剥离
    #[test]
    fn strips_combo_of_system_labels() {
        let raw_in =
            "<!-- @synapse-labels: stale, broken-link -->\n<!-- @synapse-labels: user -->\n# Body\n";
        let out = strip_system_labels(raw_in);
        assert_eq!(out, "<!-- @synapse-labels: user -->\n# Body\n");
    }

    /// 没有 `@synapse-labels:` 行时原样返回
    #[test]
    fn no_op_when_no_labels_line() {
        let raw_in = "# Body\nplain content\n";
        let out = strip_system_labels(raw_in);
        assert_eq!(out, raw_in);
    }

    /// trailing newline 保留
    #[test]
    fn preserves_trailing_newline() {
        let raw_in = "# Body\n";
        let out = strip_system_labels(raw_in);
        assert_eq!(out, raw_in);
        assert!(out.ends_with('\n'));
    }

    /// 端到端：descriptor_and_apply 写入剥离后的内容
    #[test]
    fn descriptor_and_apply_writes_stripped_content() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("my-entry");
        std::fs::create_dir_all(&dir).unwrap();
        let content_path = dir.join("content.md");
        let raw_in = "<!-- @synapse-labels: recently-modified, orphan -->\n# Body\nplain\n";
        std::fs::write(&content_path, raw_in).unwrap();
        let entry = make_entry(content_path.clone());

        let desc = descriptor_and_apply(&entry, &dir, true).unwrap();
        assert_eq!(desc.rule_id, "E9");

        let result = std::fs::read_to_string(&content_path).unwrap();
        assert!(!result.contains("recently-modified"));
        assert!(!result.contains("orphan"));
        assert!(result.contains("# Body"));
        assert!(result.contains("plain"));
    }
}
