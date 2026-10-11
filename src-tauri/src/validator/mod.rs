//! 校验引擎：检查 KB 是否符合规范。
//! 规则分 E（error）/ W（warning）/ I（info）三级。

pub mod rules;
pub mod system_labels;
pub mod migrator;

use std::collections::HashSet;
use std::path::Path;

use crate::entry::RawEntry;

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
    fn count_by_severity(&self, severity: Severity) -> usize {
        self.issues
            .iter()
            .filter(|i| matches!(i.severity, severity) && !i.ignored)
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

    let entries = match scan_entries(kb_root) {
        Ok(e) => e,
        Err(_) => return report,
    };

    let all_ids: HashSet<String> = entries.iter().map(|e| e.id.clone()).collect();

    for entry in &entries {
        let issues = rules::check_all(entry, kb_root, &all_ids);
        report.issues.extend(issues);
    }

    report.issues.extend(system_labels::compute_all(&entries));

    report
}

/// 扫描 KB 根目录，返回所有 RawEntry。
/// 优先尝试 entry 目录模式（每个子目录含 content.md），
/// 如果没有找到，回退到平铺模式（每个 .md 文件就是一个 entry）。
fn scan_entries(kb_root: &Path) -> std::io::Result<Vec<RawEntry>> {
    let entry_dirs = crate::scanner::find_entry_dirs(kb_root);
    if !entry_dirs.is_empty() {
        return Ok(entry_dirs
            .into_iter()
            .map(|dir| crate::entry::read_entry(&dir))
            .collect());
    }

    let md_files = crate::scanner::find_md_files(kb_root);
    Ok(md_files
        .into_iter()
        .map(|file| crate::entry::read_md_file(&file, kb_root))
        .collect())
}
