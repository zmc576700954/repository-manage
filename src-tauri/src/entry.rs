use std::path::{Path, PathBuf};

use crate::parser::{
    attachments::classify_attachment, attachments::extract_attachments, comments::extract_labels,
    comments::parse_metadata_comments, links::extract_double_bracket_links,
    links::extract_markdown_links, links::extract_relations, links::MarkdownLink, links::Relation,
    yaml_field::extract_yaml_frontmatter, yaml_field::parse_yaml_fields, yaml_field::YamlFields,
};

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    /// 展示用分组：扁平布局下是顶层目录名；目录布局下是 `@synapse-group`。
    /// 任何拼磁盘路径的逻辑都必须用 `dir`，不能用 `group`。
    pub group: Option<String>,
    /// 条目真实磁盘归属的相对子路径（相对 KB 根，`/` 分隔）。
    /// 扁平布局是文件的父目录；目录布局优先取 `@synapse-group`（保持旧行为），
    /// 缺失时回退到 entry 目录的物理父目录。
    pub dir: Option<String>,
    pub path: PathBuf,
    #[allow(dead_code)]
    pub content_path: PathBuf,
    pub attachments: Vec<RawAttachment>,
    pub linked_targets: Vec<String>,
    pub markdown_links: Vec<MarkdownLink>,
    /// 行级显式关系声明（`@relation: type [[id]]`），rebuild 时用于恢复关系类型。
    pub typed_relations: Vec<Relation>,
    #[allow(dead_code)]
    pub yaml: Option<YamlFields>,
    pub has_content_md: bool,
    pub labels: Vec<String>,
    pub status: Option<String>,
    pub aliases: Vec<String>,
    pub source: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub ignored_rules: Vec<String>,
    pub validation: Vec<crate::validator::ValidationIssue>,
}

#[derive(Debug, Clone)]
pub struct RawAttachment {
    pub path: String,
    pub caption: Option<String>,
    pub attachment_type: String,
}

/// 正文解析结果：扁平 `.md` 与目录 `content.md` 共用这一套逻辑。
struct ParsedBody {
    metadata: std::collections::HashMap<String, String>,
    first_heading: Option<String>,
    tags: Vec<String>,
    attachments: Vec<RawAttachment>,
    linked_targets: Vec<String>,
    markdown_links: Vec<MarkdownLink>,
    typed_relations: Vec<Relation>,
    yaml: Option<YamlFields>,
    labels: Vec<String>,
    status: Option<String>,
    aliases: Vec<String>,
    source: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    ignored_rules: Vec<String>,
}

fn parse_body(content: &str) -> ParsedBody {
    let metadata = parse_metadata_comments(content);
    let first_heading = content
        .lines()
        .find(|l| l.trim_start().starts_with("# "))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty());

    let tags = metadata
        .get("tags")
        .map(|s| extract_labels(s))
        .unwrap_or_default();
    let attachments: Vec<RawAttachment> = extract_attachments(content)
        .into_iter()
        .map(|(path, caption)| RawAttachment {
            attachment_type: classify_attachment(&path).to_string(),
            path,
            caption,
        })
        .collect();
    let yaml = extract_yaml_frontmatter(content)
        .map(|yaml| {
            // 去掉首尾的 --- 标记，避免 serde_yaml 解析为多文档
            yaml.lines()
                .filter(|l| l.trim() != "---")
                .collect::<Vec<_>>()
                .join("\n")
        })
        .and_then(|yaml| parse_yaml_fields(&yaml).ok());

    let typed_relations = extract_relations(content)
        .into_iter()
        .map(|mut rel| {
            // `[[entry#section]]` 只保留 entry 部分
            if let Some(idx) = rel.target.find('#') {
                rel.target.truncate(idx);
            }
            rel
        })
        .filter(|rel| !rel.target.is_empty())
        .collect();

    ParsedBody {
        first_heading,
        tags,
        labels: metadata
            .get("labels")
            .map(|s| extract_labels(s))
            .unwrap_or_default(),
        status: metadata.get("status").cloned(),
        aliases: metadata
            .get("aliases")
            .map(|s| extract_labels(s))
            .unwrap_or_default(),
        source: metadata.get("source").cloned(),
        created: metadata.get("created").cloned(),
        updated: metadata.get("updated").cloned(),
        ignored_rules: metadata
            .get("ignore")
            .map(|s| extract_labels(s))
            .unwrap_or_default(),
        attachments,
        linked_targets: extract_double_bracket_links(content),
        markdown_links: extract_markdown_links(content),
        typed_relations,
        yaml,
        metadata,
    }
}

/// 扁平格式的 id 派生：相对路径去扩展名、去掉开头的保留前缀目录，再用 `-` 连接。
pub fn flat_id_from_rel(rel: &Path) -> String {
    let without_ext = rel.with_extension("");
    let raw = without_ext.to_string_lossy().replace('\\', "/");
    let parts: Vec<&str> = raw.split('/').filter(|s| !s.is_empty()).collect();
    let kept: Vec<&str> = parts.into_iter().skip_while(|p| p.starts_with('_')).collect();
    if kept.is_empty() {
        return without_ext
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    kept.join("-")
}

/// 相对 KB 根的物理父目录（`/` 分隔）。根目录下返回 `None`。
fn physical_parent_rel(path: &Path, kb_root: &Path) -> Option<String> {
    path.strip_prefix(kb_root)
        .ok()?
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|s| !s.is_empty() && s != ".")
}

/// 展示分组 = `dir` 的第一个非保留前缀（`_` 开头）目录段。
/// 同一顶层目录下的所有层级归到同一组；根目录条目无分组。
/// 只依赖路径，因此是确定性的，与遍历顺序无关。
fn top_level_group(dir: Option<&str>) -> Option<String> {
    dir?
        .split('/')
        .filter(|s| !s.is_empty())
        .skip_while(|s| s.starts_with('_'))
        .next()
        .map(|s| s.to_string())
}

/// 从一个 `.md` 文件直接构建 entry（扁平格式）。
/// - id: `@synapse-id` 优先；否则由相对路径派生（去扩展名、去保留前缀、`/` → `-`）
/// - dir: 文件的物理父目录（完整相对路径），所有路径拼接都用它
/// - group: `dir` 的顶层目录，仅用于展示
/// - title: `@synapse-title` → 首个 `# heading` → 文件名
pub fn read_md_file(file: &Path, kb_root: &Path) -> RawEntry {
    let rel = file.strip_prefix(kb_root).unwrap_or(file);

    let dir = physical_parent_rel(file, kb_root);
    let group = top_level_group(dir.as_deref());

    let file_stem = file
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content = std::fs::read_to_string(file).unwrap_or_default();
    let body = parse_body(&content);

    let id = body
        .metadata
        .get("id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| flat_id_from_rel(rel));

    let title = body
        .metadata
        .get("title")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or(body.first_heading.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or(file_stem);

    RawEntry {
        id,
        title,
        tags: body.tags,
        group,
        dir,
        path: file.to_path_buf(),
        content_path: file.to_path_buf(),
        attachments: body.attachments,
        linked_targets: body.linked_targets,
        markdown_links: body.markdown_links,
        typed_relations: body.typed_relations,
        yaml: body.yaml,
        has_content_md: true,
        labels: body.labels,
        status: body.status,
        aliases: body.aliases,
        source: body.source,
        created: body.created,
        updated: body.updated,
        ignored_rules: body.ignored_rules,
        validation: vec![],
    }
}

/// 从 entry 目录的 `content.md` 读取完整 Entry 数据。
/// 如果目录没有 content.md，返回 has_content_md = false 的最小 Entry。
pub fn read_entry(entry_dir: &Path) -> RawEntry {
    let dir_id = entry_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let content_path = entry_dir.join("content.md");
    let has_content_md = content_path.exists();

    if !has_content_md {
        return RawEntry {
            id: dir_id.clone(),
            title: dir_id.clone(),
            tags: vec![],
            group: None,
            dir: None,
            path: entry_dir.to_path_buf(),
            content_path,
            attachments: vec![],
            linked_targets: vec![],
            markdown_links: vec![],
            typed_relations: vec![],
            yaml: None,
            has_content_md: false,
            labels: vec![],
            status: None,
            aliases: vec![],
            source: None,
            created: None,
            updated: None,
            ignored_rules: vec![],
            validation: vec![],
        };
    }

    let content = std::fs::read_to_string(&content_path).unwrap_or_default();
    let body = parse_body(&content);

    let id = body
        .metadata
        .get("id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| dir_id.clone());
    let title = body
        .metadata
        .get("title")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.clone());
    let group = body
        .metadata
        .get("group")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    RawEntry {
        id,
        title,
        tags: body.tags,
        group: group.clone(),
        // 目录布局沿用 `@synapse-group` 作为写入父目录（与解耦前的行为一致）；
        // 缺失时由 `read_entry_path` 回退到物理父目录。
        dir: group,
        path: entry_dir.to_path_buf(),
        content_path,
        attachments: body.attachments,
        linked_targets: body.linked_targets,
        markdown_links: body.markdown_links,
        typed_relations: body.typed_relations,
        yaml: body.yaml,
        has_content_md: true,
        labels: body.labels,
        status: body.status,
        aliases: body.aliases,
        source: body.source,
        created: body.created,
        updated: body.updated,
        ignored_rules: body.ignored_rules,
        validation: vec![],
    }
}

/// 统一入口：目录 → `content.md` 解析；文件 → 扁平 `.md` 解析。
/// 索引与校验都必须走这里，避免两套口径。
/// `dir` 缺失时按物理父目录回退，保证老数据/无 `@synapse-group` 的条目
/// 也有确定性的写入路径。
pub fn read_entry_path(path: &Path, kb_root: &Path) -> RawEntry {
    let mut entry = if path.is_dir() {
        read_entry(path)
    } else {
        read_md_file(path, kb_root)
    };
    if entry.dir.is_none() {
        entry.dir = physical_parent_rel(path, kb_root);
    }
    entry
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

    #[test]
    fn reads_labels_and_status() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let entry_dir = root.join("with-extras");
        fs::create_dir(&entry_dir).unwrap();
        let content = r#"<!-- @synapse-id: with-extras -->
<!-- @synapse-title: With Extras -->
<!-- @synapse-status: draft -->
<!-- @synapse-labels: important, wip -->
<!-- @synapse-aliases: 旧名 -->
<!-- @synapse-source: https://example.com -->
<!-- @synapse-created: 2026-01-01 -->
<!-- @synapse-updated: 2026-10-12 -->
<!-- @synapse-ignore: W5 -->

body
"#;
        fs::write(entry_dir.join("content.md"), content).unwrap();

        let entry = read_entry(&entry_dir);
        assert_eq!(entry.labels, vec!["important", "wip"]);
        assert_eq!(entry.status, Some("draft".to_string()));
        assert_eq!(entry.aliases, vec!["旧名"]);
        assert_eq!(entry.source, Some("https://example.com".to_string()));
        assert_eq!(entry.created, Some("2026-01-01".to_string()));
        assert_eq!(entry.updated, Some("2026-10-12".to_string()));
        assert_eq!(entry.ignored_rules, vec!["W5"]);
    }

    #[test]
    fn reads_group_field_silently_ignored() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let entry_dir = root.join("legacy");
        fs::create_dir(&entry_dir).unwrap();
        let content = r#"<!-- @synapse-id: legacy -->
<!-- @synapse-group: foo -->

body
"#;
        fs::write(entry_dir.join("content.md"), content).unwrap();

        let entry = read_entry(&entry_dir);
        // @synapse-group 应被静默忽略，labels 保持空
        assert!(entry.labels.is_empty());
    }

    /// flat-md format: `@synapse-title` should override `# heading` so
    /// inline title edits in the UI actually update the displayed
    /// title. Pre-fix this returned "正文" (from `# 正文`) instead of
    /// the metadata title.
    #[test]
    fn flat_md_prefers_synapse_title_over_heading() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("note.md"),
            r#"<!-- @synapse-id: note -->
<!-- @synapse-title: Display Title -->

# 正文
"#,
        )
        .unwrap();

        let entry = read_md_file(&root.join("note.md"), root);
        assert_eq!(entry.title, "Display Title");
    }

    /// flat-md format: when no `@synapse-title` is present, fall back
    /// to the first `# heading` (preserves the old behavior).
    #[test]
    fn flat_md_falls_back_to_heading_when_no_synapse_title() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("note.md"),
            "# Just heading\n",
        )
        .unwrap();

        let entry = read_md_file(&root.join("note.md"), root);
        assert_eq!(entry.title, "Just heading");
    }

    /// 统一解析：扁平 `.md` 也必须解析 tags / labels / YAML / 附件，
    /// 否则索引与校验会得出两套数据（E4 误报、元数据编辑读回为空）。
    #[test]
    fn flat_md_parses_all_metadata_fields() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("note.md"),
            r##"<!-- @synapse-id: note -->
<!-- @synapse-title: Note -->
<!-- @synapse-tags: type:concept, topic:rust -->
<!-- @synapse-labels: important -->
<!-- @synapse-status: draft -->

---
color: "#5B8DEF"
---

图片：![[./img.png|示意图]]
"##,
        )
        .unwrap();

        let entry = read_md_file(&root.join("note.md"), root);
        assert_eq!(entry.tags, vec!["type:concept", "topic:rust"]);
        assert_eq!(entry.labels, vec!["important"]);
        assert_eq!(entry.status, Some("draft".to_string()));
        assert_eq!(entry.attachments.len(), 1);
        assert_eq!(entry.attachments[0].path, "./img.png");
        assert_eq!(entry.attachments[0].attachment_type, "image");
        assert!(entry.yaml.is_some());
    }

    /// 保留前缀目录不参与 id 派生，与规则 E2 使用同一套算法。
    #[test]
    fn flat_md_id_strips_reserved_prefix() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("_inbox")).unwrap();
        fs::write(root.join("_inbox").join("note.md"), "# Note\n").unwrap();

        let entry = read_md_file(&root.join("_inbox").join("note.md"), root);
        assert_eq!(entry.id, "note");
    }

    #[test]
    fn flat_md_parses_typed_relations() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("note.md"),
            "# Note\n\n@relation: derived [[target]]\n",
        )
        .unwrap();

        let entry = read_md_file(&root.join("note.md"), root);
        assert_eq!(entry.typed_relations.len(), 1);
        assert_eq!(entry.typed_relations[0].relation_type, "derived");
        assert_eq!(entry.typed_relations[0].target, "target");
    }

    /// 展示分组取顶层目录，物理路径 `dir` 保留完整子路径——嵌套条目
    /// 显示在 `topic` 组里，但重命名/写入时仍落在 `topic/sub`。
    #[test]
    fn flat_md_keeps_full_dir_and_uses_top_level_group() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("topic").join("sub")).unwrap();
        fs::write(root.join("topic").join("sub").join("note.md"), "# Note\n").unwrap();
        fs::write(root.join("root-note.md"), "# Root\n").unwrap();
        fs::create_dir_all(root.join("_inbox").join("topic")).unwrap();
        fs::write(root.join("_inbox").join("topic").join("note.md"), "# Note\n").unwrap();

        let nested = read_md_file(&root.join("topic").join("sub").join("note.md"), root);
        assert_eq!(nested.group, Some("topic".to_string()));
        assert_eq!(nested.dir, Some("topic/sub".to_string()));

        let root_level = read_md_file(&root.join("root-note.md"), root);
        assert_eq!(root_level.group, None);
        assert_eq!(root_level.dir, None);

        // 保留前缀目录不参与展示分组，但仍保留在物理路径里。
        let reserved = read_md_file(&root.join("_inbox").join("topic").join("note.md"), root);
        assert_eq!(reserved.group, Some("topic".to_string()));
        assert_eq!(reserved.dir, Some("_inbox/topic".to_string()));
    }

    /// 目录布局：group / dir 都取自 `@synapse-group`（旧行为不变）；
    /// 没有 group 时 `read_entry_path` 用物理父目录回退 `dir`。
    #[test]
    fn directory_entry_falls_back_to_physical_parent_for_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let entry_dir = root.join("compliance").join("gdpr");
        fs::create_dir_all(&entry_dir).unwrap();
        fs::write(entry_dir.join("content.md"), "# GDPR\n").unwrap();

        let entry = read_entry_path(&entry_dir, root);
        assert_eq!(entry.group, None, "no @synapse-group → no display group");
        assert_eq!(
            entry.dir,
            Some("compliance".to_string()),
            "dir must fall back to the physical parent"
        );
    }
}