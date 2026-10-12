//! E2: 同步 id 到目录名（id 与目录不一致）。stub。

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::validator::fix::FixDescriptor;

pub fn descriptor_and_apply(
    _entry: &RawEntry,
    _entry_dir: &Path,
    _confirm: bool,
) -> AppResult<FixDescriptor> {
    unimplemented!("E2 sync-id fix — implemented in T8")
}
