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
}
