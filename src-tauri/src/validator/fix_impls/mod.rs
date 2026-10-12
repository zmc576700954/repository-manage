//! 按 rule_id 派发到具体规则实现

pub mod e1_add_id;
pub mod e2_sync_id;
pub mod e3_add_title;
pub mod e4_default_type;
pub mod e5_move_deep;
pub mod e6_remove_root_md;
pub mod e7_rename_reserved;
pub mod e9_strip_system;

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::{AppError, AppResult};
use crate::validator::fix::FixDescriptor;

/// 派发：执行修复并返回 FixDescriptor。
/// `confirm=true` 是 Confirm 类规则必传。
pub fn descriptor_and_apply(
    rule_id: &str,
    entry: &RawEntry,
    entry_dir: &Path,
    confirm: bool,
) -> AppResult<FixDescriptor> {
    match rule_id {
        "E1" => e1_add_id::descriptor_and_apply(entry, entry_dir),
        "E2" => e2_sync_id::descriptor_and_apply(entry, entry_dir, confirm),
        "E3" => e3_add_title::descriptor_and_apply(entry, entry_dir),
        "E4" => e4_default_type::descriptor_and_apply(entry, entry_dir),
        "E5" => Ok(e5_move_deep::descriptor_only()),
        "E6" => e6_remove_root_md::descriptor_and_apply(entry, entry_dir, confirm),
        "E7" => Ok(e7_rename_reserved::descriptor_only()),
        "E9" => e9_strip_system::descriptor_and_apply(entry, entry_dir, confirm),
        _ => Err(AppError::UnknownRule {
            rule_id: rule_id.to_string(),
        }),
    }
}
