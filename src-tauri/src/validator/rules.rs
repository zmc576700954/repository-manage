//! 占位；后续任务填充 E/W 规则

use std::collections::HashSet;
use std::path::Path;

use crate::entry::RawEntry;
use crate::validator::{Severity, ValidationIssue};

/// 运行所有 E/W 规则，针对单个 entry。
/// 当前为占位实现，返回空列表。
pub fn check_all(
    _entry: &RawEntry,
    _kb_root: &Path,
    _all_ids: &HashSet<String>,
) -> Vec<ValidationIssue> {
    Vec::new()
}

/// 检查单个 entry 的所有规则，返回发现的 issues。
/// 这是一个辅助函数，规则实现会逐步添加。
#[allow(dead_code)]
pub(crate) fn make_issue(
    entry_id: &str,
    rule_id: &str,
    severity: Severity,
    message: &str,
    file_line: Option<u32>,
) -> ValidationIssue {
    ValidationIssue {
        entry_id: entry_id.to_string(),
        rule_id: rule_id.to_string(),
        severity,
        message: message.to_string(),
        file_line,
        ignored: false,
    }
}
