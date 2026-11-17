//! 校验规则：E（error）/ W（warning）级。

use std::collections::HashSet;
use std::path::Path;

use crate::entry::RawEntry;
use crate::scanner::KbLayout;
use crate::validator::{Severity, ValidationIssue};

/// 跑所有 E/W 规则，返回发现的问题。
///
/// `tags_system_declared`：KB 级是否声明了 rm 的 tags 体系
/// （任一条目带 `@synapse-tags` 注释或 frontmatter `tags` 键）。
/// E4（`type:` 前缀要求）只对声明了体系的 KB 生效——kaiwu 式
/// frontmatter KB（YAML 四键、无 tags）不再被打成全量 E4 error。
pub fn check_all(
    entry: &RawEntry,
    kb_root: &Path,
    all_ids: &HashSet<String>,
    layout: KbLayout,
    tags_system_declared: bool,
) -> Vec<ValidationIssue> {
    let mut issues = vec![];
    issues.extend(check_required(entry, kb_root, all_ids, tags_system_declared));
    issues.extend(check_id_matches_path(entry, kb_root, all_ids));
    issues.extend(check_directory_depth(entry, kb_root, all_ids));
    issues.extend(check_root_md_files(entry, kb_root, all_ids, layout));
    issues.extend(check_reserved_subdir_names(entry, kb_root, all_ids));
    issues.extend(check_system_labels_written_by_user(entry, kb_root, all_ids));
    // D9 清理：check_yaml_parse 是空转死规则（解析失败在 parser 层已捕获），
    // 已删除；E8 编号缺位是历史遗留，编号不回收（见 docs/kb-spec.md 附录）。
    issues.extend(check_tag_count(entry, kb_root, all_ids));
    issues.extend(check_tag_chars(entry, kb_root, all_ids));
    issues.extend(check_broken_link_targets(entry, kb_root, all_ids));
    issues.extend(check_orphan_candidate(entry, kb_root, all_ids));
    // D9 清理：check_unused_attachments 是空转桩，已删除（声明见 docs/kb-spec.md 附录）。
    issues.extend(check_legacy_group_field(entry, kb_root, all_ids));
    issues.extend(check_review_overdue(entry));
    issues.extend(check_status_enum(entry));
    issues.extend(check_doc_code_format(entry));
    // 应用 per-entry ignore
    issues.retain(|i| !entry.ignored_rules.contains(&i.rule_id));
    issues
}

pub fn check_required(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
    tags_system_declared: bool,
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
    if tags_system_declared && !entry.tags.iter().any(|t| t.starts_with("type:")) {
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
    // 目录格式：id 默认等于 entry 目录名（或 @synapse-id 覆盖）。
    // 扁平格式：id 由相对路径去扩展名、去保留前缀后以 `-` 连接。
    let id_from_path = if entry.path.is_dir() {
        match entry.path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => return vec![],
        }
    } else {
        crate::entry::flat_id_from_rel(rel)
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

/// 去掉路径开头所有保留前缀（`_` 开头），让 id 只反映用户可见的部分。
#[allow(dead_code)]
fn strip_reserved_prefix(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let kept: Vec<&str> = parts
        .into_iter()
        .skip_while(|p| p.starts_with('_'))
        .collect();
    kept.join("/")
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
    layout: KbLayout,
) -> Vec<ValidationIssue> {
    // 扁平格式的根级 .md 是合法条目；E6 只适用于目录格式的 KB。
    if layout == KbLayout::Flat {
        return vec![];
    }
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

/// 普通 markdown 链接是否指向 KB 内的条目文件（`other.md`、`../dir/other.md`）。
/// 外链（任意 `scheme:`，如 `https://`、`mailto:`）与纯锚点 `#section` 都不算。
fn is_internal_md_link(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.starts_with('#') || url.starts_with("//") {
        return false;
    }
    // 带 scheme 的绝对地址不是 KB 内部引用；相对路径里出现冒号的情况极少，
    // 这里按 RFC 3986 的 scheme 字符集判断，避免把 `file.md` 误排除。
    if let Some(colon) = url.find(':') {
        let scheme = &url[..colon];
        if !scheme.is_empty()
            && !scheme.contains('/')
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            return false;
        }
    }
    let path = url.split(['#', '?']).next().unwrap_or("");
    path.to_ascii_lowercase().ends_with(".md")
}

pub fn check_orphan_candidate(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    // `[[wikilink]]` 与普通 markdown 链接都算有效引用；后者不能因为是外链
    // 或纯锚点就被当成孤立节点。
    let has_markdown_ref = entry
        .markdown_links
        .iter()
        .any(|link| is_internal_md_link(&link.url));
    if entry.linked_targets.is_empty() && !has_markdown_ref {
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

pub fn check_legacy_group_field(
    entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    // 扁平格式的 group 由父目录派生，不代表用户写了 @synapse-group；
    // 只有目录格式（group 来自 metadata）才可能是遗留字段。
    if entry.path.is_file() {
        return vec![];
    }
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


// ---------------------------------------------------------------- 批次 2 新增（kaiwu × rm 合并）
//
// W6/W7/W8 对应规范（docs/kb-spec.md）的 M4/M3a/M5；映射表见规范附录。
// 都是**条件规则**：键不存在即沉默，规范 KB 与通用 KB 互不干扰。

/// frontmatter 旁路键的字符串值（批次 0 的 extra_meta 是唯一来源）。
fn extra_str(entry: &RawEntry, key: &str) -> Option<String> {
    entry
        .extra_meta
        .iter()
        .find(|m| m.key == key)
        .map(|m| match &m.value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => String::new(),
        })
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
}

/// 校验当天（本地时区的近似：UTC 日期）。ISO 零填充格式下字符串比较即时间序。
/// `SYNAPSE_TODAY` 环境变量可钉住日期——dev-kb CI 的 fixture 基线必须确定性
/// （「同一份内容今天绿、三个月后黄」是运行时语义，fixture 冻结语义需要固定钟）。
fn today_iso() -> String {
    if let Ok(v) = std::env::var("SYNAPSE_TODAY") {
        if is_iso_date(v.trim()) {
            return v.trim().to_string();
        }
    }
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0) as i64;
    // civil-from-days（Howard Hinnant 算法）
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[7] == b'-'
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

/// W6 = 规范 M4（D4 新鲜度契约）：`下次 review` 早于校验当天 → 警告。
/// 刻意不取 mtime——「最后更新」是人工登记的复核日期（规范 §元数据）。
pub fn check_review_overdue(entry: &RawEntry) -> Vec<ValidationIssue> {
    let Some(review) = extra_str(entry, "下次 review") else {
        return vec![];
    };
    if !is_iso_date(&review) {
        // `按需` 等非日期取值合法（规范允许），静默。
        return vec![];
    }
    if review.as_str() < today_iso().as_str() {
        return vec![ValidationIssue {
            entry_id: entry.id.clone(),
            rule_id: "W6".into(),
            severity: Severity::Warning,
            message: format!("下次 review 已过复审期：{review}"),
            file_line: None,
            ignored: false,
        }];
    }
    vec![]
}

/// 规范 M3a 的状态基准值（括号注记剥离后比对）。
pub const STATUS_BASES: [&str; 6] = ["草案", "模板", "稳定", "需持续监控", "过期", "已归档"];

/// W7 = 规范 M3a（D3）：`状态`（或别名 status）存在时，基准值必须落在
/// canonical 枚举内；`（补充说明）` 剥离后比对。键不存在 → 沉默。
pub fn check_status_enum(entry: &RawEntry) -> Vec<ValidationIssue> {
    let Some(status) = entry.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![];
    };
    let base = status
        .split(['（', '('])
        .next()
        .unwrap_or(status)
        .trim();
    if STATUS_BASES.contains(&base) {
        return vec![];
    }
    vec![ValidationIssue {
        entry_id: entry.id.clone(),
        rule_id: "W7".into(),
        severity: Severity::Warning,
        message: format!(
            "状态基准值非法：{base}（应为 {}）",
            STATUS_BASES.join("、")
        ),
        file_line: None,
        ignored: false,
    }]
}

/// W8 = 规范 M5（Q12 文档代号第 5 键）：键存在时取值必须是 NN-MM 形态。
/// 键缺失不报——老库渐进补齐（规范附录盲区声明）。
pub fn check_doc_code_format(entry: &RawEntry) -> Vec<ValidationIssue> {
    let Some(code) = extra_str(entry, "文档代号") else {
        return vec![];
    };
    let ok = {
        let b = code.as_bytes();
        let dash = code.find('-');
        match dash {
            Some(i) if 1 <= i && i <= 4 => {
                let (a, rest) = code.split_at(i);
                let r = &rest[1..];
                !r.is_empty()
                    && r.len() <= 4
                    && a.chars().all(|c| c.is_ascii_digit())
                    && r.chars().all(|c| c.is_ascii_digit())
                    && !code.contains("--")
                    && b.len() == a.len() + 1 + r.len()
            }
            _ => false,
        }
    };
    if ok {
        return vec![];
    }
    vec![ValidationIssue {
        entry_id: entry.id.clone(),
        rule_id: "W8".into(),
        severity: Severity::Warning,
        message: format!("文档代号取值非法：{code:?}（应为 NN-MM 数字形态，如 02-01）"),
        file_line: None,
        ignored: false,
    }]
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
            dir: None,
            path: PathBuf::from(format!("/kb/{id}/content.md")),
            content_path: PathBuf::from(format!("/kb/{id}/content.md")),
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
    fn e1_missing_id() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.id = String::new();
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default(), true);
        assert!(issues.iter().any(|i| i.rule_id == "E1"));
    }

    #[test]
    fn e3_missing_title() {
        let mut e = make_entry("foo", "", vec!["type:concept".into()]);
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default(), true);
        assert!(issues.iter().any(|i| i.rule_id == "E3"));
    }

    #[test]
    fn e4_missing_type_prefix() {
        let e = make_entry("foo", "Foo", vec!["topic:react".into()]);
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default(), true);
        assert!(issues.iter().any(|i| i.rule_id == "E4"));
    }

    #[test]
    fn e4_clean_when_has_type() {
        let e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        let issues = check_required(&e, std::path::Path::new("/kb"), &Default::default(), true);
        assert!(!issues.iter().any(|i| i.rule_id == "E4"));
    }

    #[test]
    fn w5_orphan_when_no_links() {
        let e = make_entry("foo", "Foo", vec!["type:note".into()]);
        let issues = check_orphan_candidate(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(issues.iter().any(|i| i.rule_id == "W5"));
    }

    #[test]
    fn w5_clean_when_internal_markdown_link() {
        let mut e = make_entry("foo", "Foo", vec!["type:note".into()]);
        e.markdown_links = vec![crate::parser::links::MarkdownLink {
            text: "ComfyUI".into(),
            url: "../02-技术详解/01-ComfyUI.md".into(),
        }];
        let issues = check_orphan_candidate(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(
            !issues.iter().any(|i| i.rule_id == "W5"),
            "内部 markdown 链接应算有效引用: {issues:?}"
        );
    }

    #[test]
    fn w5_markdown_link_with_anchor_still_counts() {
        let mut e = make_entry("foo", "Foo", vec!["type:note".into()]);
        e.markdown_links = vec![crate::parser::links::MarkdownLink {
            text: "章节".into(),
            url: "01-ComfyUI.md#节点".into(),
        }];
        let issues = check_orphan_candidate(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(!issues.iter().any(|i| i.rule_id == "W5"));
    }

    #[test]
    fn w5_orphan_when_only_external_markdown_links() {
        let mut e = make_entry("foo", "Foo", vec!["type:note".into()]);
        e.markdown_links = vec![
            crate::parser::links::MarkdownLink {
                text: "外链".into(),
                url: "https://example.com/guide.md".into(),
            },
            crate::parser::links::MarkdownLink {
                text: "邮件".into(),
                url: "mailto:someone@example.com".into(),
            },
            crate::parser::links::MarkdownLink {
                text: "锚点".into(),
                url: "#section".into(),
            },
            crate::parser::links::MarkdownLink {
                text: "图片".into(),
                url: "assets/cover.png".into(),
            },
        ];
        let issues = check_orphan_candidate(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(
            issues.iter().any(|i| i.rule_id == "W5"),
            "外链/锚点/附件都不应算内部引用: {issues:?}"
        );
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
    fn e2_flat_path_id_matches_without_false_positive() {
        let mut e = make_entry("frontend-react", "Foo", vec!["type:concept".into()]);
        e.path = PathBuf::from("/kb/frontend/react.md");
        e.content_path = e.path.clone();
        let issues = check_id_matches_path(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(
            !issues.iter().any(|i| i.rule_id == "E2"),
            "flat id 由相对路径派生，正确的 id 不应报 E2: {:?}",
            issues
        );
    }

    #[test]
    fn e2_dir_path_id_matches_without_false_positive() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.path = PathBuf::from("/kb/foo");
        e.content_path = e.path.join("content.md");
        let issues = check_id_matches_path(&e, std::path::Path::new("/kb"), &Default::default());
        assert!(!issues.iter().any(|i| i.rule_id == "E2"));
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
        let issues = check_root_md_files(
            &e,
            std::path::Path::new("/kb"),
            &Default::default(),
            KbLayout::Directory,
        );
        assert!(issues.iter().any(|i| i.rule_id == "E6"));
    }

    #[test]
    fn e6_root_md_not_flagged_in_flat_layout() {
        let mut e = make_entry("foo", "Foo", vec!["type:concept".into()]);
        e.path = PathBuf::from("/kb/foo.md");
        e.content_path = e.path.clone();
        let issues = check_root_md_files(
            &e,
            std::path::Path::new("/kb"),
            &Default::default(),
            KbLayout::Flat,
        );
        assert!(!issues.iter().any(|i| i.rule_id == "E6"));
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
