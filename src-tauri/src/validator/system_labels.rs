//! 系统计算标签（I 类）：基于文件 mtime 与链接关系。

use std::collections::HashMap;
use std::fs;
use std::time::{Duration, SystemTime};

use crate::entry::RawEntry;
use crate::validator::{Severity, ValidationIssue};

const SEVEN_DAYS: Duration = Duration::from_secs(7 * 86400);
const ONE_HUNDRED_EIGHTY_DAYS: Duration = Duration::from_secs(180 * 86400);

/// 跑所有系统标签计算，返回 I 类 ValidationIssue 列表。
pub fn compute_all(entries: &[RawEntry]) -> Vec<ValidationIssue> {
    let mut issues = vec![];

    // 计算入度（被谁引用）
    let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
    let all_ids: std::collections::HashSet<String> =
        entries.iter().map(|e| e.id.clone()).collect();

    for entry in entries {
        for target in &entry.linked_targets {
            incoming
                .entry(target.clone())
                .or_default()
                .push(entry.id.clone());
        }
    }

    for entry in entries {
        let mtime = fs::metadata(&entry.path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let now = SystemTime::now();
        let age = now.duration_since(mtime).unwrap_or_default();

        // I1: recently-modified
        if age < SEVEN_DAYS {
            issues.push(ValidationIssue {
                entry_id: entry.id.clone(),
                rule_id: "I1".into(),
                severity: Severity::Info,
                message: format!("Modified {} days ago", age.as_secs() / 86400),
                file_line: None,
                ignored: false,
            });
        }

        // I3: stale
        if age > ONE_HUNDRED_EIGHTY_DAYS {
            issues.push(ValidationIssue {
                entry_id: entry.id.clone(),
                rule_id: "I3".into(),
                severity: Severity::Info,
                message: format!(
                    "Stale ({} days since modification)",
                    age.as_secs() / 86400
                ),
                file_line: None,
                ignored: false,
            });
        }

        // I2: orphan (无任何 link 边)
        let outgoing_empty = entry.linked_targets.is_empty();
        let incoming_empty = incoming.get(&entry.id).map_or(true, |v| v.is_empty());
        if outgoing_empty && incoming_empty {
            issues.push(ValidationIssue {
                entry_id: entry.id.clone(),
                rule_id: "I2".into(),
                severity: Severity::Info,
                message: "Orphan entry (no links)".into(),
                file_line: None,
                ignored: false,
            });
        }

        // I4: broken-link
        for target in &entry.linked_targets {
            if !all_ids.contains(target) {
                issues.push(ValidationIssue {
                    entry_id: entry.id.clone(),
                    rule_id: "I4".into(),
                    severity: Severity::Info,
                    message: format!("[[{}]] target does not exist", target),
                    file_line: None,
                    ignored: false,
                });
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RawEntry;
    use std::path::PathBuf;

    fn entry_with_path(id: &str, path: &str) -> RawEntry {
        RawEntry {
            id: id.to_string(),
            title: id.to_string(),
            tags: vec!["type:note".into()],
            group: None,
            path: PathBuf::from(path),
            content_path: PathBuf::from(path),
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
    fn empty_entries_yields_no_issues() {
        let issues = compute_all(&[]);
        assert!(issues.is_empty());
    }

    #[test]
    fn orphan_detected() {
        let e = entry_with_path("a", "/nonexistent-a.md");
        let issues = compute_all(&[e]);
        assert!(issues.iter().any(|i| i.rule_id == "I2"));
    }
}