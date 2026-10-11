//! 占位；后续任务填充 I 类系统标签

use crate::entry::RawEntry;
use crate::validator::ValidationIssue;

/// 计算所有系统标签（如 orphan、dead-end、hub 等），返回 I 级 issues。
/// 当前为占位实现，返回空列表。
pub fn compute_all(_entries: &[RawEntry]) -> Vec<ValidationIssue> {
    vec![]
}
