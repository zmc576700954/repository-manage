use std::path::{Path, PathBuf};

use crate::parser::{
    attachments::classify_attachment, attachments::extract_attachments, comments::parse_metadata_comments,
    links::extract_double_bracket_links, links::extract_markdown_links, links::MarkdownLink,
    yaml_field::extract_yaml_frontmatter, yaml_field::parse_yaml_fields, yaml_field::YamlFields,
};

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub group: Option<String>,
    pub path: PathBuf,
    #[allow(dead_code)]
    pub content_path: PathBuf,
    pub attachments: Vec<RawAttachment>,
    pub linked_targets: Vec<String>,
    pub markdown_links: Vec<MarkdownLink>,
    #[allow(dead_code)]
    pub yaml: Option<YamlFields>,
    pub has_content_md: bool,
}

#[derive(Debug, Clone)]
pub struct RawAttachment {
    pub path: String,
    pub caption: Option<String>,
    pub attachment_type: String,
}

/// 从一个 `.md` 文件直接构建 entry（适用于"每个 .md 一个 entry"的 KB 格式）。
/// - id: 相对于 kb_root 的路径，转成用 `-` 分隔（不含 .md 扩展名）
/// - group: 直接父目录相对 kb_root 的完整子路径（用 `/` 拼接），如 `compliance/gdpr`
///           KB 根直接下的 .md group = None
/// - title: 文件首个 `# heading`；缺失时退到文件名（去后缀）
/// - linked_targets: 内容中的 `[[xxx]]` 链接
/// - markdown_links: 内容中的 `[text](url)` 链接（内/外都保留）
pub fn read_md_file(file: &Path, kb_root: &Path) -> RawEntry {
    let rel = file.strip_prefix(kb_root).unwrap_or(file);

    let group = rel
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|s| !s.is_empty() && s != ".");

    let file_stem = file
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content = std::fs::read_to_string(file).unwrap_or_default();

    let title = content
        .lines()
        .find(|l| l.trim_start().starts_with("# "))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(file_stem.clone());

    let linked_targets = extract_double_bracket_links(&content);
    let markdown_links = extract_markdown_links(&content);

    let id = rel
        .with_extension("")
        .to_string_lossy()
        .replace(['\\', '/'], "-");

    RawEntry {
        id,
        title,
        tags: vec![],
        group,
        path: file.to_path_buf(),
        content_path: file.to_path_buf(),
        attachments: vec![],
        linked_targets,
        markdown_links,
        yaml: None,
        has_content_md: true,
    }
}

/// 从 entry 目录读取并解析完整 Entry 数据。
/// 如果目录没有 content.md，返回 has_content_md = false 的最小 Entry。
pub fn read_entry(entry_dir: &Path) -> RawEntry {
    let id = entry_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content_path = entry_dir.join("content.md");
    let has_content_md = content_path.exists();

    if !has_content_md {
        return RawEntry {
            id: id.clone(),
            title: id.clone(),
            tags: vec![],
            group: None,
            path: entry_dir.to_path_buf(),
            content_path,
            attachments: vec![],
            linked_targets: vec![],
            markdown_links: vec![],
            yaml: None,
            has_content_md: false,
        };
    }

    let content = std::fs::read_to_string(&content_path).unwrap_or_default();

    let metadata = parse_metadata_comments(&content);
    let title = metadata
        .get("title")
        .cloned()
        .unwrap_or_else(|| id.clone());
    let tags = metadata
        .get("tags")
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();
    let group = metadata.get("group").cloned();

    let attachments: Vec<RawAttachment> = extract_attachments(&content)
        .into_iter()
        .map(|(path, caption)| RawAttachment {
            attachment_type: classify_attachment(&path).to_string(),
            path,
            caption,
        })
        .collect();

    let linked_targets = extract_double_bracket_links(&content);
    let markdown_links = extract_markdown_links(&content);

    let yaml = extract_yaml_frontmatter(&content)
        .map(|yaml| {
            // 去掉首尾的 --- 标记，避免 serde_yaml 解析为多文档
            yaml.lines()
                .filter(|l| l.trim() != "---")
                .collect::<Vec<_>>()
                .join("\n")
        })
        .and_then(|yaml| parse_yaml_fields(&yaml).ok());

    RawEntry {
        id,
        title,
        tags,
        group,
        path: entry_dir.to_path_buf(),
        content_path,
        attachments,
        linked_targets,
        markdown_links,
        yaml,
        has_content_md: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reads_complete_entry() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let entry_dir = root.join("react-hooks");
        fs::create_dir(&entry_dir).unwrap();

        let content = r##"<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks 深入理解 -->
<!-- @synapse-tags: react, frontend -->
<!-- @synapse-group: frontend -->

---
color: "#5B8DEF"
---

# 正文

参考 [[state-management]] 和 [[render-flow#block-1]]。
图片：![[./diagrams/flow.png|流程图]]
"##;
        fs::write(entry_dir.join("content.md"), content).unwrap();

        let entry = read_entry(&entry_dir);

        assert_eq!(entry.id, "react-hooks");
        assert_eq!(entry.title, "React Hooks 深入理解");
        assert_eq!(entry.tags, vec!["react", "frontend"]);
        assert_eq!(entry.group, Some("frontend".to_string()));
        assert_eq!(entry.linked_targets, vec!["state-management", "render-flow"]);
        assert_eq!(entry.attachments.len(), 1);
        assert_eq!(entry.attachments[0].path, "./diagrams/flow.png");
        assert_eq!(entry.attachments[0].caption, Some("流程图".to_string()));
        assert_eq!(entry.attachments[0].attachment_type, "image");
        assert!(entry.yaml.is_some());
        assert_eq!(entry.yaml.unwrap().color, Some("#5B8DEF".to_string()));
        assert!(entry.has_content_md);
    }

    #[test]
    fn handles_missing_content_md() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("broken");
        fs::create_dir(&entry_dir).unwrap();

        let entry = read_entry(&entry_dir);

        assert_eq!(entry.id, "broken");
        assert_eq!(entry.title, "broken");
        assert!(!entry.has_content_md);
        assert!(entry.attachments.is_empty());
        assert!(entry.linked_targets.is_empty());
    }

    #[test]
    fn uses_id_when_title_missing() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("no-title");
        fs::create_dir(&entry_dir).unwrap();
        fs::write(entry_dir.join("content.md"), "# Just content").unwrap();

        let entry = read_entry(&entry_dir);

        assert_eq!(entry.id, "no-title");
        assert_eq!(entry.title, "no-title");
    }

    #[test]
    fn parses_multiple_tags_from_csv() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("multi");
        fs::create_dir(&entry_dir).unwrap();
        fs::write(
            entry_dir.join("content.md"),
            r#"<!-- @synapse-id: multi -->
<!-- @synapse-tags: alpha, beta, gamma -->

content
"#,
        )
        .unwrap();

        let entry = read_entry(&entry_dir);
        assert_eq!(entry.tags, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn empty_tags_yields_empty_vec() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("notags");
        fs::create_dir(&entry_dir).unwrap();
        fs::write(
            entry_dir.join("content.md"),
            r#"<!-- @synapse-id: notags -->
<!-- @synapse-title: No Tags -->
"#,
        )
        .unwrap();

        let entry = read_entry(&entry_dir);
        assert!(entry.tags.is_empty());
    }

    #[test]
    fn falls_back_gracefully_on_invalid_yaml() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("badyaml");
        fs::create_dir(&entry_dir).unwrap();
        fs::write(
            entry_dir.join("content.md"),
            r##"<!-- @synapse-id: badyaml -->

---
color: : invalid
---
"##,
        )
        .unwrap();

        let entry = read_entry(&entry_dir);
        // YAML 解析失败时不应 panic，应得到 None
        assert!(entry.yaml.is_none());
        // 其他字段正常解析
        assert_eq!(entry.id, "badyaml");
    }

    #[test]
    fn detects_linked_targets() {
        let dir = tempdir().unwrap();
        let entry_dir = dir.path().join("with-links");
        fs::create_dir(&entry_dir).unwrap();
        fs::write(
            entry_dir.join("content.md"),
            r#"<!-- @synapse-id: with-links -->

参考 [[other-a]] 和 [[other-b#section]]。
"#,
        )
        .unwrap();

        let entry = read_entry(&entry_dir);
        assert_eq!(entry.linked_targets, vec!["other-a", "other-b"]);
    }
}