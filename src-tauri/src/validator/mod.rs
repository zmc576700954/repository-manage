//! 校验引擎：检查 KB 是否符合规范。
//! 规则分 E（error）/ W（warning）/ I（info）三级。

pub mod fix;
pub mod fix_impls;
pub mod kb_rules;
pub mod rules;
pub mod system_labels;
pub mod migrator;

use std::collections::HashSet;
use std::path::Path;

use crate::entry::RawEntry;
use crate::scanner::KbLayout;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub entry_id: String,
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file_line: Option<u32>,
    pub ignored: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ValidationReport {
    pub kb_root: String,
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    fn count_by_severity(&self, target: Severity) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == target && !i.ignored)
            .count()
    }

    pub fn error_count(&self) -> usize {
        self.count_by_severity(Severity::Error)
    }

    pub fn warning_count(&self) -> usize {
        self.count_by_severity(Severity::Warning)
    }

    pub fn info_count(&self) -> usize {
        self.count_by_severity(Severity::Info)
    }

    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }
}

/// 全 KB 校验入口。
pub fn validate_kb(kb_root: &Path) -> ValidationReport {
    let mut report = ValidationReport {
        kb_root: kb_root.to_string_lossy().to_string(),
        issues: vec![],
    };

    let (entries, layout) = match scan_entries(kb_root) {
        Ok(e) => e,
        Err(_) => return report,
    };

    let all_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();
    // E4 门控：KB 里任一条目声明了 tags 体系才启用 `type:` 前缀要求。
    let tags_system_declared = entries.iter().any(|e| e.declares_tags_system);

    for entry in &entries {
        let issues = rules::check_all(entry, kb_root, &all_ids, layout, tags_system_declared);
        report.issues.extend(issues);
    }

    report.issues.extend(system_labels::compute_all(&entries, kb_root));
    report.issues.extend(kb_rules::check_all(&entries, kb_root));

    report
}

/// 扫描 KB 根目录，返回所有 RawEntry 与布局形态。
/// 与索引共用 `scanner::scan_entry_paths` + `entry::read_entry_path`，
/// 保证 id / tags / YAML / 附件的解析口径完全一致。
fn scan_entries(kb_root: &Path) -> std::io::Result<(Vec<RawEntry>, KbLayout)> {
    let (paths, layout) = crate::scanner::scan_entry_paths(kb_root);
    let entries = paths
        .into_iter()
        .map(|path| crate::entry::read_entry_path(&path, kb_root))
        .collect();
    Ok((entries, layout))
}
