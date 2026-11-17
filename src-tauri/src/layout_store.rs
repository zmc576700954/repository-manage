// 布局持久化模块：get_layout / save_node_position 命令已接线。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// 布局文件名。Q10.a 后存放于 app data 的 `indexes/<kb_id>/layout.json`，
/// 不再落 KB 目录（旧 `.synapse-layout.json` 由迁移逻辑搬到新位置）。
const LAYOUT_FILENAME: &str = "layout.json";

/// `.synapse-layout.json` 的读-改-写是“读整文件 → 改一个 key → 写回整文件”，
/// 两个并发写会互相覆盖（后写者基于旧快照）。Tauri 命令层有 `write_lock`，
/// 但 `update_position` / `rename_key` 也可能被别处直接调用，所以模块内部
/// 再加一把进程级同步锁，保证单次更新原子。
static LAYOUT_WRITE_LOCK: Mutex<()> = Mutex::new(());

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

/// 从 KB 的运行时状态目录（app data `indexes/<kb_id>/`）加载布局；
/// 不存在则返回空布局。
pub fn load_layout(runtime_dir: &Path) -> Layout {
    let layout_path = runtime_dir.join(LAYOUT_FILENAME);
    if !layout_path.exists() {
        return Layout::default();
    }

    std::fs::read_to_string(&layout_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 保存布局到运行时状态目录的 layout.json（原子写）。
/// 先写临时文件再原子 rename，避免并发的 `load_layout` 读到写了一半的 JSON
/// （解析失败会回退成空布局，导致图谱位置瞬间回弹）。
pub fn save_layout(runtime_dir: &Path, layout: &Layout) -> std::io::Result<()> {
    // 运行时目录可能尚不存在（新 KB / 新装的 app data）——先建再写。
    std::fs::create_dir_all(runtime_dir)?;
    let layout_path = runtime_dir.join(LAYOUT_FILENAME);
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp_path = runtime_dir.join(format!("{LAYOUT_FILENAME}.tmp"));
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, layout_path)
}

/// 读取布局的节点位置表——`get_layout` 命令的唯一逻辑，便于单测。
pub fn load_positions(kb_root: &Path) -> HashMap<String, NodePosition> {
    load_layout(kb_root).positions
}

/// 更新单个节点的位置并保存。
pub fn update_position(runtime_dir: &Path, entry_id: &str, x: f64, y: f64) -> std::io::Result<()> {
    let _guard = LAYOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut layout = load_layout(runtime_dir);
    layout.set(entry_id, x, y);
    save_layout(runtime_dir, &layout)
}

/// 将节点的位置从一个 entry id 转移到另一个 entry id。由
/// `execute_rename` 使用——重命名节点后，节点仍在用户放置的位置。
/// 如果旧 id 没有位置记录，这是一个静默的 no-op（新 id 也缺席）。
/// 如果新 id 已经有位置，则保留现有的位置——不会覆盖现有的布局项。
pub fn rename_key(runtime_dir: &Path, old_id: &str, new_id: &str) -> std::io::Result<()> {
    let _guard = LAYOUT_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut layout = load_layout(runtime_dir);
    if let Some(pos) = layout.positions.remove(old_id) {
        // 仅在新 id 不存在时设置（不覆盖）。
        layout.positions.entry(new_id.to_string()).or_insert(pos);
        save_layout(runtime_dir, &layout)?;
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

    /// 命令层闭环：get_layout（`load_positions`）读空 → 拖拽（`update_position`）
    /// 落库 → 再次 get_layout 读到新位置 → rename 迁移 key 后再读。
    #[test]
    fn load_drag_rename_round_trip_through_public_api() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path();

        assert!(load_positions(kb_root).is_empty(), "fresh KB has no saved layout");

        update_position(kb_root, "react-hooks", 120.0, 80.0).unwrap();
        let after_drag = load_positions(kb_root);
        assert_eq!(after_drag.get("react-hooks").unwrap().x, 120.0);
        assert_eq!(after_drag.get("react-hooks").unwrap().y, 80.0);

        rename_key(kb_root, "react-hooks", "react-hooks-advanced").unwrap();
        let after_rename = load_positions(kb_root);
        let moved = after_rename
            .get("react-hooks-advanced")
            .expect("position must follow the renamed entry");
        assert_eq!(moved.x, 120.0);
        assert_eq!(moved.y, 80.0);
        assert!(after_rename.get("react-hooks").is_none());
    }

    /// 并发保存两个（或多个）节点位置时，任何一个写入都不能被另一个的
    /// 旧快照覆盖。`update_position` 是“读整文件 → 改 key → 写回”，
    /// 没有内部串行化时这里几乎必然丢位置。
    #[test]
    fn concurrent_updates_do_not_lose_positions() {
        let dir = tempdir().unwrap();
        let kb_root = dir.path().to_path_buf();

        let mut handles = vec![];
        for t in 0..8u32 {
            let root = kb_root.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..8u32 {
                    let id = format!("entry-{t}-{i}");
                    update_position(&root, &id, (t * 10 + i) as f64, (t * 100 + i) as f64)
                        .expect("update_position must succeed");
                }
            }));
        }
        for h in handles {
            h.join().expect("thread must not panic");
        }

        let positions = load_positions(&kb_root);
        assert_eq!(positions.len(), 64, "every concurrent write must survive");
        for t in 0..8u32 {
            for i in 0..8u32 {
                let id = format!("entry-{t}-{i}");
                let pos = positions.get(&id).expect("position must exist");
                assert_eq!(pos.x, (t * 10 + i) as f64);
                assert_eq!(pos.y, (t * 100 + i) as f64);
            }
        }
    }
}
