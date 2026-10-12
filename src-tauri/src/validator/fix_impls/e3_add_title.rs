//! E3: 补 title。stub。

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::validator::fix::FixDescriptor;

pub fn descriptor_and_apply(_entry: &RawEntry, _entry_dir: &Path) -> AppResult<FixDescriptor> {
    unimplemented!("E3 add-title fix — implemented in T9")
}
