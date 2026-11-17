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

    // 2. 用统一解析口径取当前 tags（frontmatter 优先、注释回退），
    //    缺 type: 前缀时注入 type:note 写回 frontmatter（Q6.a，不再插注释）。
    let parsed = crate::entry::read_md_file(
        &entry.content_path,
        entry.content_path.parent().unwrap_or(Path::new(".")),
    );
    let new_content = inject_type(&raw, &parsed.tags);

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

fn inject_type(raw: &str, current_tags: &[String]) -> String {
    // 已包含 type: 前缀则 no-op
    if current_tags.iter().any(|t| t.starts_with("type:")) {
        return raw.to_string();
    }
    let mut new_tags = vec!["type:note".to_string()];
    new_tags.extend(current_tags.iter().cloned());
    // Q6.a：写 frontmatter tags（CSV 标量，M1 安全），无 frontmatter 时
    // 由 fm_writer 在文件顶部创建区块（F3 事故的根因即「注释插到 frontmatter 之前」，
    // 该路径已不存在）。
    crate::parser::fm_writer::apply_edits(
        raw,
        &crate::parser::fm_writer::FrontmatterEdit::set(
            "tags",
            crate::parser::fm_writer::FmValue::List(new_tags),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(content_path: std::path::PathBuf) -> RawEntry {
        let parent = content_path.parent().unwrap().to_path_buf();
        RawEntry {
            id: "my-entry".into(),
            title: String::new(),
            tags: vec![],
            group: None,
            dir: None,
            path: parent.clone(),
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

    #[test]
    fn injects_type_into_frontmatter_tags() {
        let raw_in = "---\n状态: 稳定\n---\n\n# Body\n";
        let out = inject_type(raw_in, &["foo".to_string(), "bar".to_string()]);
        assert!(out.contains("tags: type:note, foo, bar"), "frontmatter tags written: {out}");
        assert!(out.contains("状态: 稳定"), "existing keys preserved: {out}");
        assert!(out.starts_with("---\n"), "frontmatter stays first: {out}");
    }

    #[test]
    fn no_op_when_type_already_present() {
        let raw_in = "---\ntags: type:book, foo\n---\n\n# Body\n";
        let out = inject_type(raw_in, &["type:book".to_string(), "foo".to_string()]);
        assert_eq!(out, raw_in);
    }

    #[test]
    fn injects_into_existing_comment_tags_via_frontmatter() {
        let raw_in = "<!-- @synapse-tags: foo, bar -->\n# Body\n";
        let out = inject_type(raw_in, &["foo".to_string(), "bar".to_string()]);
        assert!(out.starts_with("---\n"), "no comment injection; frontmatter first: {out}");
        assert!(out.contains("tags: type:note, foo, bar"), "{out}");
        assert!(out.contains("<!-- @synapse-tags: foo, bar -->"), "legacy comment kept: {out}");
    }

    #[test]
    fn injects_into_kaiwu_style_frontmatter() {
        let raw_in = "---\n优先级: 🟡 P1\n状态: 稳定\n---\n\n# Body\n";
        let out = inject_type(raw_in, &[]);
        assert!(out.contains("tags: type:note"), "{out}");
        assert!(out.contains("优先级: 🟡 P1"), "四键保留: {out}");
    }
}
