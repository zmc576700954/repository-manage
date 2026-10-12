//! 校验规则：E（error）/ W（warning）级。

use std::collections::HashSet;
use std::path::Path;

use crate::entry::RawEntry;
use crate::validator::{Severity, ValidationIssue};

/// 跑所有 E/W 规则，返回发现的问题。
pub fn check_all(
    entry: &RawEntry,
    kb_root: &Path,
    all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let mut issues = vec![];
    issues.extend(check_required(entry, kb_root, all_ids));
    issues.extend(check_id_matches_path(entry, kb_root, all_ids));
    issues.extend(check_directory_depth(entry, kb_root, all_ids));
    issues.extend(check_root_md_files(entry, kb_root, all_ids));
    issues.extend(check_reserved_subdir_names(entry, kb_root, all_ids));
    issues.extend(check_system_labels_written_by_user(entry, kb_root, all_ids));
    issues.extend(check_yaml_parse(entry, kb_root, all_ids));
    issues.extend(check_tag_count(entry, kb_root, all_ids));
    issues.extend(check_tag_chars(entry, kb_root, all_ids));
    issues.extend(check_broken_link_targets(entry, kb_root, all_ids));
    issues.extend(check_orphan_candidate(entry, kb_root, all_ids));
    issues.extend(check_unused_attachments(entry, kb_root, all_ids));
    issues.extend(check_legacy_group_field(entry, kb_root, all_ids));
    // 应用 per-entry ignore
    issues.retain(|i| !entry.ignored_rules.contains(&i.rule_id));
    issues
}

pub fn check_required(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let mut issues = vec![];
    if entry.id.trim().is_empty() {
        issues.push(ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E1".into(),
            severity: Severity::Error,
            message: "@synapse-id is empty".into(),
            file_line: None,
            ignored: false,
        });
    }
    if entry.title.trim().is_empty() {
        issues.push(ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E3".into(),
            severity: Severity::Error,
            message: "@synapse-title is empty".into(),
            file_line: None,
            ignored: false,
        });
    }
    if !entry.tags.iter().any(|t| t.starts_with("type:")) {
        issues.push(ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E4".into(),
            severity: Severity::Error,
            message: "@synapse-tags must have at least one type: prefix".into(),
            file_line: None,
            ignored: false,
        });
    }
    issues
}

pub fn check_id_matches_path(
    entry: &RawEntry,
    kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let Ok(rel) = entry.path.strip_prefix(kb_root) else {
        return vec![];
    };
    let Some(file_name) = entry.path.file_name() else {
        return vec![];
    };
    let file_name_str = file_name.to_string_lossy();
    let parent = rel.parent().map(|p| p.to_string_lossy().replace('\\', "/"));
    let id_from_path = match parent.as_deref() {
        Some(p) if !p.is_empty() && p != "." => {
            format!("{}-{}", p.replace('/', "-"), file_name_str)
        }
        _ => file_name_str.to_string(),
    };
    if entry.id != id_from_path && !entry.id.is_empty() {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E2".into(),
            severity: Severity::Error,
            message: format!(
                "@synapse-id \"{}\" does not match path-derived \"{}\"",
                entry.id, id_from_path
            ),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

pub fn check_directory_depth(
    entry: &RawEntry,
    kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let Ok(rel) = entry.path.strip_prefix(kb_root) else {
        return vec![];
    };
    let depth = rel.components().count();
    if depth > 4 {
        // /kb/{d}/{sd}/{entry} = 4 components; > 4 means > 3 levels
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E5".into(),
            severity: Severity::Error,
            message: format!("Directory depth {} exceeds limit of 3", depth - 1),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

pub fn check_root_md_files(
    entry: &RawEntry,
    kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let Ok(rel) = entry.path.strip_prefix(kb_root) else {
        return vec![];
    };
    let depth = rel.components().count();
    if depth == 1 && entry.path.extension().map_or(false, |e| e == "md") {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E6".into(),
            severity: Severity::Error,
            message: ".md file directly under kb_root (must be in entry dir)".into(),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

pub fn check_reserved_subdir_names(
    entry: &RawEntry,
    kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let Ok(rel) = entry.path.strip_prefix(kb_root) else {
        return vec![];
    };
    let reserved = ["attachments", "notes", "content"];
    for ancestor in rel.ancestors().skip(1) {
        // skip the file itself
        if let Some(name) = ancestor.file_name() {
            if reserved.contains(&name.to_string_lossy().as_ref()) {
                return vec![ValidationIssue {
                    entry_id: entry.id.clone(),
                    rule_id: "E7".into(),
                    severity: Severity::Error,
                    message: format!(
                        "Entry path contains reserved subdirectory \"{}\"",
                        name.to_string_lossy()
                    ),
                    file_line: None,
                    ignored: false,
                }];
            }
        }
    }
    vec![]
}

pub fn check_system_labels_written_by_user(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    let system = ["recently-modified", "orphan", "stale", "broken-link"];
    let violations: Vec<_> = entry
        .labels
        .iter()
        .filter(|l| system.contains(&l.as_str()))
        .collect();
    if !violations.is_empty() {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "E9".into(),
            severity: Severity::Error,
            message: format!(
                "User-written system label(s): {:?} (system-managed, cannot be set)",
                violations
            ),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

pub fn check_yaml_parse(
    _entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    // YAML 解析失败已在 parser 层捕获；此处无操作
    vec![]
}

pub fn check_tag_count(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    if entry.tags.len() > 15 {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "W2".into(),
            severity: Severity::Warning,
            message: format!("Tag count {} exceeds limit 15", entry.tags.len()),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

pub fn check_tag_chars(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    for tag in &entry.tags {
        let val = tag.split(':').nth(1).unwrap_or("");
        if val.is_empty() {
            continue;
        }
        // 跳过 custom: 与 Unicode（统一放开）
        if tag.starts_with("custom:") {
            continue;
        }
        if !val.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return vec![ValidationIssue {
                entry_id: entry.id.clone(),
                rule_id: "W3".into(),
                severity: Severity::Warning,
                message: format!("Tag \"{}\" contains non-kebab-case chars", tag),
                file_line: None,
                ignored: false,
            }];
        }
    }
    vec![]
}

pub fn check_broken_link_targets(
    entry: &RawEntry,
    _kb_root: &Path,
    all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    entry
        .linked_targets
        .iter()
        .filter(|t| !all_ids.contains(*t))
        .map(|t| ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "W4".into(),
            severity: Severity::Warning,
            message: format!("[[{}]] target does not exist", t),
            file_line: None,
            ignored: false,
        })
        .collect()
}

pub fn check_orphan_candidate(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    if entry.linked_targets.is_empty() {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "W5".into(),
            severity: Severity::Warning,
            message: "Entry has no outgoing links (orphan candidate)".into(),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

pub fn check_unused_attachments(
    _entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    // 简化版：留待未来基于实际 attachments 目录扫描
    vec![]
}

pub fn check_legacy_group_field(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    if let Some(value) = &entry.group {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "W6".into(),
            severity: Severity::Warning,
            message: format!(
                "Legacy @synapse-group: {} (run `synapse migrate-labels` or convert manually)",
                value
            ),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RawEntry;
    use std::path::PathBuf;

    fn make_entry(id: &str, title: &str, tags: Vec<String>) -> RawEntry {
        RawEntry {
            id: id.to_string(),
            title: title.to_string(),
            tags,
            group: None,
            path: PathBuf::from(format!("/kb/{id}/content.md")),
            content_path: PathBuf::from(format!("/kb/{id}/content.md")),
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
    fn e1_missing_id() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.id = String::new();
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E1"));
    }

    #[test]
    fn e3_missing_title() {
        let mut e = make_entry("foo", "", vec!["type:concept".into()]);
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E3"));
    }

    #[test]
    fn e4_missing_type_prefix() {
        let e = make_entry("foo", "Foo", vec!["topic:react".into()]);
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E4"));
    }

    #[test]
    fn e4_clean_when_has_type() {
        let e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(!issues.iter().any(|i| i.rule_id == "E4"));
    }

    #[test]
    fn w5_orphan_when_no_links() {
        let e = make_entry("foo", "Foo", vec!["type:note".into()]);
        let issues = check_orphan_candidate(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "W5"));
    }

    #[test]
    fn e9_rejects_system_label() {
        let mut e = make_entry("foo", "Foo", vec!["type:note".into()]);
        e.labels = vec!["recently-modified".into()];
        let issues =
            check_system_labels_written_by_user(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E9"));
    }

    #[test]
    fn e2_id_mismatch_triggers() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.id = "wrong-id".into();
        let issues = check_id_matches_path(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E2"));
    }

    #[test]
    fn e5_deep_path_triggers() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        // depth > 3: /kb/a/b/c/d/foo/content.md = 6 components
        e.path = PathBuf::from("/kb/a/b/c/d/foo/content.md");
        e.content_path = e.path.clone();
        let issues = check_directory_depth(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E5"));
    }

    #[test]
    fn e6_root_md_triggers() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.path = PathBuf::from("/kb/foo.md");  // depth = 2 (kb + foo.md)
        e.content_path = e.path.clone();
        let issues = check_root_md_files(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E6"));
    }

    #[test]
    fn e7_reserved_subdir_triggers() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.path = PathBuf::from("/kb/attachments/foo/content.md");
        e.content_path = e.path.clone();
        let issues = check_reserved_subdir_names(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "E7"));
    }

    #[test]
    fn w2_too_many_tags_triggers() {
        let e = make_entry("foo", "Foo", (0..20).map(|i| format!("topic:i{}", i)).collect());
        let issues = check_tag_count(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "W2"));
    }

    #[test]
    fn w3_bad_chars_trigger() {
        let e = make_entry("foo", "Foo", vec!["type:Concept".into()]);  // capital C
        let issues = check_tag_chars(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "W3"));
    }

    #[test]
    fn w4_broken_link_triggers() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.linked_targets = vec!["nonexistent".into()];
        let mut all_ids = HashSet::new();
        all_ids.insert("foo".to_string());
        let issues = check_broken_link_targets(&e, std::path::Path::new("/kb"), &all_ids);
        assert!(issues.iter().any(|i| i.rule_id == "W4"));
    }
}