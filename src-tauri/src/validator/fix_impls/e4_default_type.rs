//! E4: 默认 type。stub。

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::validator::fix::FixDescriptor;

pub fn descriptor_and_apply(_entry: &RawEntry, _entry_dir: &Path) -> AppResult<FixDescriptor> {
    unimplemented!("E4 default-type fix — implemented in T10")
}
