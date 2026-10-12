//! E9: 剥离系统标签。stub。

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::validator::fix::FixDescriptor;

pub fn descriptor_and_apply(
    _entry: &RawEntry,
    _entry_dir: &Path,
    _confirm: bool,
) -> AppResult<FixDescriptor> {
    unimplemented!("E9 strip-system fix — implemented in T14")
}
