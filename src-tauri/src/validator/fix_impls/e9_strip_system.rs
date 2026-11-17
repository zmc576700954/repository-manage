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
    let new_content = strip_system_labels(&raw, &entry.content_path);

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

/// Q6.a：有 frontmatter 的文件把净化后的 labels 写回 frontmatter（CSV 标量；
/// 全空则删除键）；无 frontmatter 的旧文件维持注释行过滤（保守策略：
/// 含任意 system label 的整行丢掉）。
fn strip_system_labels(raw: &str, content_path: &Path) -> String {
    if crate::parser::fm_writer::has_frontmatter(raw) {
        let parsed = crate::entry::read_md_file(
            content_path,
            content_path.parent().unwrap_or(Path::new(".")),
        );
        let kept: Vec<String> = parsed
            .labels
            .iter()
            .filter(|l| !SYSTEM_LABELS.contains(&l.as_str()))
            .cloned()
            .collect();
        let edit = if kept.is_empty() {
            crate::parser::fm_writer::FrontmatterEdit::remove("labels")
        } else {
            crate::parser::fm_writer::FrontmatterEdit::set(
                "labels",
                crate::parser::fm_writer::FmValue::List(kept),
            )
        };
        return crate::parser::fm_writer::apply_edits(raw, &edit);
    }
    strip_system_labels_comment(raw)
}

/// 旧文件（无 frontmatter）的注释行过滤：包含任意 system label 的整行丢掉。
fn strip_system_labels_comment(raw: &str) -> String {
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
            dir: None,
            path: parent,
            content_path,
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

    /// 旧文件（无 frontmatter）：只含 system label 的整行被剥离
    #[test]
    fn strips_all_system_label_line() {
        let raw_in = "<!-- @synapse-labels: recently-modified, orphan -->\n# Body\n";
        let out = strip_system_labels_comment(raw_in);
        assert_eq!(out, "# Body\n");
    }

    /// 旧文件：包含单个 system label 的行也被剥离
    #[test]
    fn strips_single_system_label_line() {
        let raw_in = "<!-- @synapse-labels: recently-modified -->\n# Body\n";
        let out = strip_system_labels_comment(raw_in);
        assert_eq!(out, "# Body\n");
    }

    /// 旧文件：不含 system label 的 `@synapse-labels:` 行原样保留
    #[test]
    fn keeps_user_only_label_line() {
        let raw_in = "<!-- @synapse-labels: custom-tag, manual -->\n# Body\n";
        let out = strip_system_labels_comment(raw_in);
        assert_eq!(out, raw_in);
    }

    /// 旧文件：多个 system label 组合也被剥离
    #[test]
    fn strips_combo_of_system_labels() {
        let raw_in =
            "<!-- @synapse-labels: stale, broken-link -->\n<!-- @synapse-labels: user -->\n# Body\n";
        let out = strip_system_labels_comment(raw_in);
        assert_eq!(out, "<!-- @synapse-labels: user -->\n# Body\n");
    }

    /// frontmatter 文件：labels 写回净化后的 CSV 标量（Q6.a）
    #[test]
    fn frontmatter_labels_rewritten_without_system_labels() {
        let raw_in = "---\nlabels: recently-modified, important\n状态: 稳定\n---\n\n# Body\n";
        let dir = tempfile::tempdir().unwrap();
        let content_path = dir.path().join("a.md");
        std::fs::write(&content_path, raw_in).unwrap();
        let out = strip_system_labels(raw_in, &content_path);
        assert!(out.contains("labels: important"), "{out}");
        assert!(!out.contains("recently-modified"), "{out}");
        assert!(out.contains("状态: 稳定"), "{out}");
    }

    /// frontmatter 文件：净化后为空 → 删除 labels 键
    #[test]
    fn frontmatter_labels_removed_when_empty() {
        let raw_in = "---\nlabels: orphan\n---\n\n# Body\n";
        let dir = tempfile::tempdir().unwrap();
        let content_path = dir.path().join("a.md");
        std::fs::write(&content_path, raw_in).unwrap();
        let out = strip_system_labels(raw_in, &content_path);
        assert!(!out.contains("labels"), "{out}");
        assert!(out.contains("---\n---\n") || out.lines().filter(|l| l.trim() == "---").count() == 2, "{out}");
    }

    /// 没有 labels 时原样返回
    #[test]
    fn no_op_when_no_labels_line() {
        let raw_in = "# Body\nplain content\n";
        let out = strip_system_labels(raw_in, std::path::Path::new("/nonexistent/a.md"));
        assert_eq!(out, raw_in);
    }

    /// trailing newline 保留
    #[test]
    fn preserves_trailing_newline() {
        let raw_in = "# Body\n";
        let out = strip_system_labels(raw_in, std::path::Path::new("/nonexistent/a.md"));
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
