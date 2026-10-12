#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const LAYOUT_FILENAME: &str = ".synapse-layout.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    pub positions: HashMap<String, NodePosition>,
}

impl Layout {
    pub fn get(&self, id: &str) -> Option<&NodePosition> {
        self.positions.get(id)
    }

    pub fn set(&mut self, id: impl Into<String>, x: f64, y: f64) {
        self.positions.insert(id.into(), NodePosition { x, y });
    }
}

/// 从 kb 根目录加载布局；如果不存在则返回空布局。
pub fn load_layout(kb_root: &Path) -> Layout {
    let layout_path = kb_root.join(LAYOUT_FILENAME);
    if !layout_path.exists() {
        return Layout::default();
    }

    std::fs::read_to_string(&layout_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 保存布局到 kb 根目录的 .synapse-layout.json。
pub fn save_layout(kb_root: &Path, layout: &Layout) -> std::io::Result<()> {
    let layout_path = kb_root.join(LAYOUT_FILENAME);
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(layout_path, json)
}

/// 更新单个节点的位置并保存。
pub fn update_position(kb_root: &Path, entry_id: &str, x: f64, y: f64) -> std::io::Result<()> {
    let mut layout = load_layout(kb_root);
    layout.set(entry_id, x, y);
    save_layout(kb_root, &layout)
}

/// 将节点的位置从一个 entry id 转移到另一个 entry id。由
/// `execute_rename` 使用——重命名节点后，节点仍在用户放置的位置。
/// 如果旧 id 没有位置记录，这是一个静默的 no-op（新 id 也缺席）。
/// 如果新 id 已经有位置，则保留现有的位置——不会覆盖现有的布局项。
pub fn rename_key(kb_root: &Path, old_id: &str, new_id: &str) -> std::io::Result<()> {
    let mut layout = load_layout(kb_root);
    if let Some(pos) = layout.positions.remove(old_id) {
        // 仅在新 id 不存在时设置（不覆盖）。
        layout.positions.entry(new_id.to_string()).or_insert(pos);
        save_layout(kb_root, &layout)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn returns_empty_when_no_file() {
        let dir = tempdir().unwrap();
        let layout = load_layout(dir.path());
        assert!(layout.positions.is_empty());
    }

    #[test]
    fn round_trips_positions() {
        let dir = tempdir().unwrap();

        let mut layout = Layout::default();
        layout.set("entry-a", 100.0, 200.0);
        layout.set("entry-b", -50.0, 300.0);

        save_layout(dir.path(), &layout).unwrap();

        let loaded = load_layout(dir.path());
        assert_eq!(loaded.get("entry-a").unwrap().x, 100.0);
        assert_eq!(loaded.get("entry-a").unwrap().y, 200.0);
        assert_eq!(loaded.get("entry-b").unwrap().x, -50.0);
        assert_eq!(loaded.get("entry-b").unwrap().y, 300.0);
    }

    #[test]
    fn updates_single_position() {
        let dir = tempdir().unwrap();

        update_position(dir.path(), "first", 1.0, 2.0).unwrap();
        update_position(dir.path(), "second", 3.0, 4.0).unwrap();

        let loaded = load_layout(dir.path());
        assert_eq!(loaded.get("first").unwrap().x, 1.0);
        assert_eq!(loaded.get("second").unwrap().y, 4.0);
    }

    #[test]
    fn falls_back_to_empty_on_corrupt_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(LAYOUT_FILENAME), "invalid json{").unwrap();

        let layout = load_layout(dir.path());
        assert!(layout.positions.is_empty());
    }

    /// `rename_key` is the layout half of `execute_rename`. The user
    /// expects the node to stay where it was after renaming, so we
    /// transfer the position data from the old id to the new id
    /// atomically (single layout file write).
    #[test]
    fn rename_key_transfers_position() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        update_position(kb_root, "react-hooks", 100.0, 200.0).unwrap();

        rename_key(kb_root, "react-hooks", "react-hooks-v2").unwrap();

        let layout = load_layout(kb_root);
        assert!(layout.get("react-hooks").is_none(), "old key must be gone");
        let new_pos = layout.get("react-hooks-v2").expect("new key must exist");
        assert_eq!(new_pos.x, 100.0);
        assert_eq!(new_pos.y, 200.0);
    }

    /// If the old key didn't have a position (it was never moved from
    /// the default group layout), `rename_key` is a silent no-op —
    /// the new id is also absent. The next time the user moves the
    /// node, the new id gets its position naturally.
    #[test]
    fn rename_key_unknown_old_key_is_noop() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        // No position recorded for "missing".
        rename_key(kb_root, "missing", "new-name").unwrap();

        let layout = load_layout(kb_root);
        assert!(layout.get("missing").is_none());
        assert!(layout.get("new-name").is_none(), "no-op must not invent a position");
    }

    /// Renaming to an id that already has a position: the existing
    /// position wins (we don't clobber the user's previous layout for
    /// the target). Old key is dropped.
    #[test]
    fn rename_key_target_existing_keeps_existing_position() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        update_position(kb_root, "src", 1.0, 1.0).unwrap();
        update_position(kb_root, "old-id", 5.0, 5.0).unwrap();

        rename_key(kb_root, "old-id", "src").unwrap();

        let layout = load_layout(kb_root);
        let pos = layout.get("src").unwrap();
        assert_eq!(pos.x, 1.0, "existing position must be preserved");
        assert!(layout.get("old-id").is_none());
    }
}
