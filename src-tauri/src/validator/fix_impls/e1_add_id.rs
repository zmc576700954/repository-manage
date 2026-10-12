//! E1: 给缺 id 的 entry 加一个。stub，后续任务实现。

use std::path::Path;

use crate::entry::RawEntry;
use crate::error::AppResult;
use crate::validator::fix::FixDescriptor;

pub fn descriptor_and_apply(_entry: &RawEntry, _entry_dir: &Path) -> AppResult<FixDescriptor> {
    unimplemented!("E1 add-id fix — implemented in T7")
}
