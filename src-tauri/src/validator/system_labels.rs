//! 系统计算标签（I 类）：基于文件 mtime 与链接关系。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::{Duration, SystemTime};

use crate::entry::RawEntry;
use crate::validator::{Severity, ValidationIssue};

const SEVEN_DAYS: Duration = Duration::from_secs(7 * 86400);
const ONE_HUNDRED_EIGHTY_DAYS: Duration = Duration::from_secs(180 * 86400);

fn issue(entry_id: &str, rule_id: &str, message: String) -> ValidationIssue {
    ValidationIssue {
        entry_id: entry_id.to_string(),
        rule_id: rule_id.into(),
        severity: Severity::Info,
        message,
        file_line: None,
        ignored: false,
    }
}

/// 跑所有系统标签计算，返回 I 类 ValidationIssue 列表。
pub fn compute_all(entries: &[RawEntry]) -> Vec<ValidationIssue> {
    let mut issues = vec![];

    // 计算入度（被谁引用）
    let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
    let all_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();

    for entry in entries {
        for target in &entry.linked_targets {
            incoming
                .entry(target.clone())
                .or_default()
                .push(entry.id.clone());
        }
    }

    for entry in entries {
        let mtime_result = fs::metadata(&entry.path).and_then(|m| m.modified());

        let now = SystemTime::now();
        let age = mtime_result
            .as_ref()
            .ok()
            .and_then(|mt| now.duration_since(*mt).ok());

        // I1: recently-modified (only if we have valid mtime)
        // I3: stale (only if we have valid mtime)
        if let Some(age) = age {
            if age < SEVEN_DAYS {
                issues.push(issue(
                    &entry.id,
                    "I1",
                    format!("Modified {} days ago", age.as_secs() / 86400),
                ));
            }

            if age > ONE_HUNDRED_EIGHTY_DAYS {
                issues.push(issue(
                    &entry.id,
                    "I3",
                    format!("Stale ({} days since modification)", age.as_secs() / 86400),
                ));
            }
        }

        // I2: orphan (无任何 link 边) — doesn't depend on mtime
        let outgoing_empty = entry.linked_targets.is_empty();
        let incoming_empty = incoming.get(&entry.id).map_or(true, |v| v.is_empty());
        if outgoing_empty && incoming_empty {
            issues.push(issue(
                &entry.id,
                "I2",
                "Orphan entry (no links)".into(),
            ));
        }

        // I4: broken-link with dedup
        let mut seen = HashSet::new();
        for target in &entry.linked_targets {
            if !all_ids.contains(target) && seen.insert(target.clone()) {
                issues.push(issue(
                    &entry.id,
                    "I4",
                    format!("[[{}]] target does not exist", target),
                ));
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

    #[test]
    fn recently_modified_fires_for_fresh_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh.md");
        std::fs::write(&path, "# fresh").unwrap();
        let e = entry_with_path("fresh", path.to_str().unwrap());
        let issues = compute_all(&[e]);
        assert!(issues.iter().any(|i| i.rule_id == "I1"));
    }

    #[test]
    fn stale_fires_for_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.md");
        std::fs::write(&path, "# old").unwrap();
        let old_time = std::time::SystemTime::now()
            - std::time::Duration::from_secs(200 * 86400);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old_time))
            .unwrap();
        let e = entry_with_path("old", path.to_str().unwrap());
        let issues = compute_all(&[e]);
        assert!(issues.iter().any(|i| i.rule_id == "I3"));
    }

    #[test]
    fn broken_link_fires() {
        let mut e = entry_with_path("foo", "/nonexistent-foo.md");
        e.linked_targets = vec!["nope".into()];
        let issues = compute_all(&[e]);
        assert!(issues.iter().any(|i| i.rule_id == "I4"));
    }

    #[test]
    fn broken_link_dedup() {
        let mut e = entry_with_path("foo", "/nonexistent-foo.md");
        e.linked_targets = vec!["nope".into(), "nope".into(), "also-no".into()];
        let issues = compute_all(&[e]);
        let broken_count = issues.iter().filter(|i| i.rule_id == "I4").count();
        assert_eq!(broken_count, 2, "Duplicate [[nope]] should be deduped to 1");
    }

    #[test]
    fn metadata_failure_skips_age_labels() {
        // Path that doesn't exist → metadata fails
        let e = entry_with_path("missing", "/this/path/does/not/exist.md");
        let issues = compute_all(&[e]);
        // I1 and I3 should NOT fire (metadata unavailable)
        assert!(!issues.iter().any(|i| i.rule_id == "I1"));
        assert!(!issues.iter().any(|i| i.rule_id == "I3"));
        // I2 (orphan) may still fire since it doesn't depend on mtime
    }
}