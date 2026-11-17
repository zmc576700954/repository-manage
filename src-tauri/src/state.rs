use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::mpsc;
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
    /// 运行时状态目录的基目录（Q10.a）：布局与版本历史放
    /// `<runtime_dir_base>/<kb_id>/`。生产 = `paths::indexes_dir()`；
    /// 测试注入 tempdir，避免污染真实 app data。
    pub runtime_dir_base: PathBuf,
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
    /// Sender half of the watch-event channel. The notify callback sends the
    /// debounced batch's file paths; the rebuild-loop thread drains the receiver
    /// and uses them to decide whether the batch came from local writes or
    /// external edits.
    /// Held here so `load_active_kb` can clone it when constructing a new
    /// debouncer for a different KB root.
    pub watch_tx: Arc<std::sync::Mutex<Option<mpsc::Sender<Vec<PathBuf>>>>>,
    /// 启动闸门：启动时的全量 rebuild 持有写锁，首屏读命令（`get_entries`）
    /// 持读锁。这样 `get_entries` 不会在 rebuild 进行到一半时读到空表或
    /// 半张表，从而避免“打开应用先看到空图谱、随后自己跳出来”的竞态。
    /// 启动 rebuild 结束（含无活动 KB / 路径不存在的提前返回）后写锁释放，
    /// 之后的读命令恢复为无等待。
    pub startup_gate: Arc<RwLock<()>>,
    /// 本进程**已成功写入**的路径集合（规范化后），带写入时间用于过期清理。
    ///
    /// 写命令只有在 `fs::write/rename/remove` 成功之后、释放 `write_lock`
    /// 之前调用 `note_local_write(path)`。watcher 拿到这一批事件的真实路径后
    /// 逐条比对：全部命中 → 这批重建由自身写操作触发（`local-write`）；
    /// 只要有一条没命中（例如外部同时改了别的文件）→ `external`。
    ///
    /// 与“写命令一开始就递增序号”相比：
    /// - 校验失败/中途失败的写命令不会留下任何记录，不会污染来源标记；
    /// - 批量事件里有外部路径时不会被整体误标成 `local-write`。
    ///
    /// 记录里同时保存“写入完成时该路径应处于的状态”（内容哈希/缺失/目录），
    /// 事件到达时比对当前状态：同一路径本地写完之后又被外部改掉，哈希
    /// 对不上 → 判为 `external`，不再依赖前端指纹兜底。
    pub local_write_paths: Arc<std::sync::Mutex<HashMap<PathBuf, LocalWriteRecord>>>,
    /// 命令内部已重建索引、但 watcher 尚未感知的重命名增量（旧 id → 新 id）。
    ///
    /// `execute_rename` 在 `write_lock` 内先移动文件并调用 `rename_entry_id`
    /// + `rebuild()`，所以 watcher 之后重建时“重建前快照”里已经都是新 id，
    /// `added`/`removed` 会为空。把命令产生的 rename 暂存在这里，watcher
    /// 推送事件时带上，前端才能做增量更新（至少能把选中的旧 id 映射到新 id）。
    pub pending_renames: Arc<std::sync::Mutex<Vec<RenameDelta>>>,
}

/// 一次重命名的增量信息（旧 id → 新 id），随 `kb-changed` 事件下发。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameDelta {
    pub from_id: String,
    pub to_id: String,
}

impl AppState {
    /// 记录一次**已经成功落盘**的写入路径。必须在释放 `write_lock` 之前调用：
    /// watcher 重建会等这把锁，因此事件到达时记录一定已可见。
    ///
    /// 同时快照当前的“期望状态”（内容哈希 / 缺失 / 目录），并让内容指纹
    /// 缓存对该路径失效。这样事件到达时可以直接比对，而不是只看路径命中。
    pub fn note_local_write(&self, path: impl AsRef<Path>) {
        let normalized = normalize_watch_path(path.as_ref());
        let expected = expected_state(&normalized);
        crate::fingerprint::invalidate_paths(std::slice::from_ref(&normalized));
        let mut paths = self.local_write_paths.lock().unwrap();
        paths.insert(
            normalized,
            LocalWriteRecord {
                at: Instant::now(),
                expected,
            },
        );
    }

    /// 目录重命名成功后登记整棵子树：新路径逐个登记内容哈希，旧路径逐个
    /// 登记为“缺失”。watcher 的事件批次通常包含这些子文件路径，逐一登记
    /// 后整批才能判为 `local-write`，不会退化成 `external` 多刷一次。
    pub fn note_local_write_tree(&self, old_root: &Path, new_root: &Path) {
        self.note_local_write(old_root);
        self.note_local_write(new_root);
        self.register_tree_children(old_root, new_root);
    }

    fn register_tree_children(&self, old_dir: &Path, new_dir: &Path) {
        let Ok(read_dir) = std::fs::read_dir(new_dir) else {
            return;
        };
        for entry in read_dir.flatten() {
            let new_child = entry.path();
            let Some(name) = new_child.file_name() else {
                continue;
            };
            let old_child = old_dir.join(name);
            if new_child.is_dir() {
                self.note_local_write(&old_child);
                self.note_local_write(&new_child);
                self.register_tree_children(&old_child, &new_child);
            } else {
                // 旧路径此时已不存在 → 期望 Missing；新路径按内容哈希登记。
                self.note_local_write(&old_child);
                self.note_local_write(&new_child);
            }
        }
    }

    /// 判断这一批 watcher 事件路径是否**全部**来自本进程的成功写入。
    ///
    /// 命中记录还不够：还要比对当前磁盘状态与写入完成时的期望状态。同一
    /// 路径“本地写成功 → 外部再改”会被后一次写入的哈希变化识别出来，
    /// 判为 `external`（即使两次写入落在同一个防抖批次里）。
    /// 命中的记录会被消费；超过 [`LOCAL_WRITE_TTL`] 的记录会被清掉。
    pub fn all_paths_locally_written(&self, event_paths: &[PathBuf]) -> bool {
        if event_paths.is_empty() {
            return false;
        }
        let mut paths = self.local_write_paths.lock().unwrap();
        let now = Instant::now();
        paths.retain(|_, record| now.duration_since(record.at) < LOCAL_WRITE_TTL);

        let mut all_local = true;
        for path in event_paths {
            let normalized = normalize_watch_path(path);
            match paths.remove(&normalized) {
                Some(record) if record.expected.matches(&normalized) => {}
                _ => all_local = false,
            }
        }
        all_local
    }

    /// 记录一次成功的重命名，等 watcher 下一次推送时带走。
    pub fn record_rename(&self, from_id: &str, to_id: &str) {
        if from_id == to_id {
            return;
        }
        let mut pending = self.pending_renames.lock().unwrap();
        pending.push(RenameDelta {
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
        });
    }

    /// 取出并清空尚未下发的重命名增量。
    pub fn take_pending_renames(&self) -> Vec<RenameDelta> {
        std::mem::take(&mut *self.pending_renames.lock().unwrap())
    }
}

/// 本地写入记录的有效期。写命令与 watcher 的时差通常 <1s；留 60s 余量，
/// 既覆盖 notify 抖动，又避免残留记录把很久之后的外部变更误标为本地写。
const LOCAL_WRITE_TTL: Duration = Duration::from_secs(60);

/// 写入完成时路径应处于的状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedState {
    /// 文件应当不存在（删除/重命名的旧路径）。
    Missing,
    /// 文件内容应当是这串字节的 FNV-1a 哈希。
    File(u64),
    /// 目录（或读不到内容但存在）——无法用内容验证，只做存在性匹配。
    Any,
}

impl ExpectedState {
    fn matches(&self, path: &Path) -> bool {
        match self {
            Self::Any => true,
            _ => expected_state(path) == *self,
        }
    }
}

/// 一次本地写入的期望状态 + 发生时间。
#[derive(Clone, Copy, Debug)]
pub struct LocalWriteRecord {
    pub at: Instant,
    pub expected: ExpectedState,
}

fn expected_state(path: &Path) -> ExpectedState {
    match std::fs::metadata(path) {
        Ok(md) if md.is_file() => std::fs::read(path)
            .ok()
            .map(|bytes| ExpectedState::File(crate::fingerprint::fnv1a64(&bytes)))
            .unwrap_or(ExpectedState::Any),
        Ok(_) => ExpectedState::Any,
        Err(_) => ExpectedState::Missing,
    }
}

/// 统一路径形态，解决 macOS `/tmp` ↔ `/private/tmp` 之类的软链接差异：
/// 能 canonicalize 时解析父目录（文件本身可能已被删除/移动，父目录仍在），
/// 解析失败则退回原路径。
pub(crate) fn normalize_watch_path(path: &Path) -> PathBuf {
    let parent = path.parent().and_then(|p| std::fs::canonicalize(p).ok());
    match (parent, path.file_name()) {
        (Some(parent), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

/// Q10.a 迁移：把 KB 目录里的旧运行时产物搬到 app data 的运行时目录。
///
/// - `<kb_root>/.synapse-layout.json` → `<runtime_dir>/layout.json`
/// - `<kb_root>/.synapse/history/` → `<runtime_dir>/history/`
///
/// 幂等：源不存在即跳过；新旧并存时以新位置为准（旧文件删除）；
/// `.synapse` 空壳目录 best-effort 清理。任何一步失败不 panic——
/// 迁移是启动辅助，不应阻断 rebuild（错误返回给调用方记录）。
pub fn migrate_legacy_runtime_state(
    kb_root: &Path,
    runtime_dir: &Path,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(runtime_dir)?;

    let old_layout = kb_root.join(".synapse-layout.json");
    let new_layout = runtime_dir.join("layout.json");
    if old_layout.exists() {
        if new_layout.exists() {
            std::fs::remove_file(&old_layout)?;
        } else {
            std::fs::rename(&old_layout, &new_layout)?;
        }
    }

    let old_history = kb_root.join(".synapse").join("history");
    let new_history = runtime_dir.join("history");
    if old_history.is_dir() {
        if new_history.exists() {
            std::fs::remove_dir_all(&old_history)?;
        } else {
            std::fs::rename(&old_history, &new_history)?;
        }
    }

    // best-effort 清理空壳 `.synapse/`
    let synapse_dir = kb_root.join(".synapse");
    if synapse_dir.is_dir() {
        let is_empty = std::fs::read_dir(&synapse_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if is_empty {
            let _ = std::fs::remove_dir(&synapse_dir);
        }
    }
    Ok(())
}
