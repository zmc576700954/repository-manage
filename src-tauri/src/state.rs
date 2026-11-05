use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::index::Index;
use crate::kb_registry::{KbRegistry, KBInfo};
use crate::validator::ValidationReport;

/// Holds the OS-level file watcher for the currently active KB root. When this
/// is dropped, notify automatically unregisters the underlying watch. The
/// `root` is recorded so callers can tell whether an existing watcher already
/// covers a given path (and avoid creating duplicates).
pub struct WatcherHandle {
    pub _debouncer: notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
    pub root: PathBuf,
}

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<RwLock<Index>>,
    pub kbs: Arc<RwLock<KbRegistry>>,
    pub current_kb: Arc<RwLock<Option<KBInfo>>>,
    /// Cached latest validation report for the active KB.
    /// `None` until first validation run; updated by `validate_kb` command.
    pub validation_report: Arc<RwLock<Option<ValidationReport>>>,
    /// Serializes every command that mutates the on-disk KB or its SQLite index.
    /// Mutating commands (`create_entry_simple`, `create_relation`, `rebuild_*`,
    /// KB switch / add / remove) acquire this lock for the duration of all
    /// `fs::write` calls AND the DB writes that mirror them. Read-only commands
    /// (`get_entries`, `get_entry_content`, `get_active_kb`, `list_kbs`)
    /// deliberately do NOT take it — they're lock-free.
    ///
    /// Without this lock two concurrent writers can race on a `.md` file's
    /// read-modify-write (e.g. `append_reference_line` reading `from.md`,
    /// appending `[[new_id]]`, writing back) and the loser's append silently
    /// disappears when the winner's write hits the disk first. The same race
    /// makes a `rebuild()` running concurrently observe half-written state.
    pub write_lock: Arc<Mutex<()>>,
    /// Active file-system watcher for the active KB root. Replaced (the old
    /// one is dropped, which unregisters the OS watch) by `load_active_kb`
    /// and `clear_active_kb` so the watcher always tracks whichever KB is
    /// currently active. Wrapped in a sync `Mutex` (not tokio's) because the
    /// debouncer is sync-only — there's no need to hold it across `await`s.
    pub watcher: Arc<std::sync::Mutex<Option<WatcherHandle>>>,
    /// Sender half of the watch-event channel. The notify callback sends `()`
    /// on each debounced batch; the rebuild-loop thread drains the receiver.
    /// Held here so `load_active_kb` can clone it when constructing a new
    /// debouncer for a different KB root.
    pub watch_tx: Arc<std::sync::Mutex<Option<mpsc::Sender<()>>>>,
    /// 启动闸门：启动时的全量 rebuild 持有写锁，首屏读命令（`get_entries`）
    /// 持读锁。这样 `get_entries` 不会在 rebuild 进行到一半时读到空表或
    /// 半张表，从而避免“打开应用先看到空图谱、随后自己跳出来”的竞态。
    /// 启动 rebuild 结束（含无活动 KB / 路径不存在的提前返回）后写锁释放，
    /// 之后的读命令恢复为无等待。
    pub startup_gate: Arc<RwLock<()>>,
}
