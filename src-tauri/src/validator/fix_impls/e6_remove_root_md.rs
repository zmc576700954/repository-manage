//! E6: 移除根目录的 .md（应改为子目录 + content.md）。stub。

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::validator::fix::FixDescriptor;

pub fn descriptor_and_apply(
    _entry: &RawEntry,
    _entry_dir: &Path,
    _confirm: bool,
) -> AppResult<FixDescriptor> {
    unimplemented!("E6 remove-root-md fix — implemented in T12")
}
