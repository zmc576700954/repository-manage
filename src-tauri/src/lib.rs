pub mod entry;
pub mod error;
pub mod fingerprint;
pub mod fs;
pub mod index;
pub mod kb_registry;
pub mod layout_store;
pub mod parser;
pub mod paths;
pub mod scanner;
pub mod state;
pub mod validator;

#[cfg(feature = "tauri")]
pub mod commands;

#[cfg(feature = "tauri")]
use std::sync::Arc;
#[cfg(feature = "tauri")]
use tokio::sync::{Mutex, RwLock};

#[cfg(feature = "tauri")]
use crate::index::Index;
#[cfg(feature = "tauri")]
use crate::kb_registry::KbRegistry;
#[cfg(feature = "tauri")]
use crate::state::{AppState, WatcherHandle};

/// 由 `.setup` 写入的 AppHandle，供 watcher 重建循环在完成后推送
/// `kb-changed` 事件。用 `OnceLock` 而不是塞进 `AppState`，避免所有测试
/// 里的 `AppState` 字面量都要多填一个永远为 `None` 的字段。
#[cfg(feature = "tauri")]
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// 重建的来源。前端据此区分“自身写操作的回声”和真正的**外部**变更：
/// `local-write` 事件只有在内容指纹与前端本地已同步的数据一致时才丢弃，
/// 从而不会把与本地写同批合并的外部变更误吞。
#[cfg(feature = "tauri")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum RebuildOrigin {
    #[serde(rename = "local-write")]
    LocalWrite,
    #[serde(rename = "external")]
    External,
}

/// watcher 重建完成后推送给前端的事件负载。
///
/// - `kbId`：哪个 KB 发生了变化（前端可以据此忽略非当前 KB 的事件）
/// - `entryCount` / `revision`：重建后的条目数与进程内单调递增的序号
/// - `added` / `removed`：相对重建前新增/消失的 entry id；其余条目视为
///   可能被修改（前端据此决定是否只更新受影响条目）
/// - `renamed`：命令侧（`execute_rename`）产生的旧 id → 新 id。命令在
///   `write_lock` 内先完成索引迁移再释放锁，watcher 的前后快照里已经都是
///   新 id、`added`/`removed` 为空，因此必须单独携带这条增量。
/// - `origin`：`local-write`（本进程写命令触发）或 `external`（外部文件变更）
/// - `fingerprint`：重建后索引内容的指纹。与上一次相同说明这次重建没有
///   实际变化（例如布局文件/索引库等与 KB 内容无关的写入触发了 watcher），
///   后端直接不推送，避免前端无谓刷新。
#[cfg(feature = "tauri")]
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KbChangedPayload {
    pub kb_id: String,
    pub entry_count: usize,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub renamed: Vec<crate::state::RenameDelta>,
    pub origin: RebuildOrigin,
    pub fingerprint: String,
    pub revision: u64,
}

/// 对重建后的条目集合做稳定指纹（实现与缓存策略见 [`crate::fingerprint`]）：
/// 按 id 排序后序列化再哈希，并混入每个正文文件的**内容哈希**，因此只改
/// 正文也能被感知，且不依赖 `size + mtime` 的精度。
#[cfg(feature = "tauri")]
pub(crate) fn entries_fingerprint(entries: &[crate::index::StoredEntry]) -> String {
    crate::fingerprint::entries_fingerprint(entries)
}

/// 比较 rebuild 前后的索引快照，构造事件负载；内容指纹与上一次推送相同
/// 时返回 `None`（没有实际变化，无需通知前端）。
#[cfg(feature = "tauri")]
fn build_kb_changed_payload(
    kb_id: &str,
    before: &[crate::index::StoredEntry],
    after: &[crate::index::StoredEntry],
    revision: u64,
    previous_fingerprint: Option<&str>,
    origin: RebuildOrigin,
) -> Option<KbChangedPayload> {
    use std::collections::HashSet;

    let fingerprint = entries_fingerprint(after);
    if previous_fingerprint == Some(fingerprint.as_str()) {
        return None;
    }

    let before_ids: HashSet<&str> = before.iter().map(|e| e.id.as_str()).collect();
    let after_ids: HashSet<&str> = after.iter().map(|e| e.id.as_str()).collect();
    let mut added: Vec<String> = after_ids
        .difference(&before_ids)
        .map(|id| id.to_string())
        .collect();
    let mut removed: Vec<String> = before_ids
        .difference(&after_ids)
        .map(|id| id.to_string())
        .collect();
    added.sort();
    removed.sort();

    Some(KbChangedPayload {
        kb_id: kb_id.to_string(),
        entry_count: after.len(),
        added,
        removed,
        renamed: Vec::new(),
        origin,
        fingerprint,
        revision,
    })
}

/// 重建一次索引并返回重建前后的快照。`write_lock` 只在函数内部持有——
/// 调用方拿到返回值时锁已经释放，因此随后的 `emit` 不会发生在重建临界区
/// 内，前端收到事件后调 `get_entries` 一定能读到重建完成后的完整数据。
#[cfg(feature = "tauri")]
async fn rebuild_once(
    state: &AppState,
    kb_path: &std::path::Path,
) -> Option<(
    Vec<crate::index::StoredEntry>,
    Vec<crate::index::StoredEntry>,
)> {
    let _guard = state.write_lock.lock().await;
    let idx = state.index.read().await;
    let before = match idx.get_all_entries() {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[watcher] read before rebuild failed: {e}");
            return None;
        }
    };
    if let Err(e) = idx.rebuild(kb_path) {
        eprintln!("[watcher] rebuild failed: {e}");
        return None;
    }
    match idx.get_all_entries() {
        Ok(rows) => Some((before, rows)),
        Err(e) => {
            eprintln!("[watcher] read after rebuild failed: {e}");
            None
        }
    }
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[cfg(feature = "tauri")]
pub fn run() {
    use std::path::PathBuf;
    use std::sync::mpsc;

    let registry = KbRegistry::load_or_default();
    let active = registry.active().cloned();

    let initial_index = match active.as_ref() {
        Some(kb) => match Index::open(&registry.index_db_path_for(&kb.id)) {
            Ok(idx) => idx,
            Err(e) => panic!("Failed to open index: {e}"),
        },
        None => match Index::open_default() {
            Ok(idx) => idx,
            Err(e) => panic!("Failed to open index: {e}"),
        },
    };

    // NOTE: we deliberately do NOT call `initial_index.rebuild(...)` here.
    // A full rebuild walks the entire KB on the calling thread — fine for
    // a dozen notes, but for a 5k-entry KB it can take seconds and would
    // block the window from appearing. `Index::open` already runs
    // `CREATE TABLE IF NOT EXISTS`, so schema migrations happen at open
    // time. The setup callback below spawns the startup rebuild on tauri's
    // runtime so the main thread returns to `tauri::Builder::run`
    // immediately. The watcher task (also spawned below) is the second
    // safety net for any disk drift between Index::open and the rebuild.

    let index_arc = Arc::new(RwLock::new(initial_index));
    let kbs_arc = Arc::new(RwLock::new(registry));
    let current_kb_arc = Arc::new(RwLock::new(active.clone()));

    // std::sync::mpsc — the notify callback runs on the watcher's own thread
    // and can only call sync APIs. The rebuild loop drains this from a
    // dedicated tokio runtime via `spawn_blocking`.
    let (watch_tx, watch_rx) = mpsc::channel::<Vec<PathBuf>>();

    // Build the initial watcher for the active KB (if any). Failures here
    // are logged but non-fatal — the app still runs without live updates.
    let initial_root: Option<PathBuf> = active
        .as_ref()
        .map(|kb| PathBuf::from(&kb.path))
        .filter(|p| p.exists());
    let initial_watcher = initial_root
        .as_ref()
        .and_then(|root| create_watcher(root, watch_tx.clone()));

    let app_state = AppState {
        index: index_arc.clone(),
        kbs: kbs_arc.clone(),
        current_kb: current_kb_arc.clone(),
        validation_report: Arc::new(RwLock::new(None)),
        write_lock: Arc::new(Mutex::new(())),
        watcher: Arc::new(std::sync::Mutex::new(initial_watcher)),
        watch_tx: Arc::new(std::sync::Mutex::new(Some(watch_tx))),
        startup_gate: Arc::new(RwLock::new(())),
        local_write_paths: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        pending_renames: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    // Spawn the rebuild loop on a dedicated thread + tokio runtime so it
    // doesn't compete with tauri commands for the main runtime's workers
    // and so blocking `rx.recv()` doesn't stall the rest of the app.
    let state_for_loop = app_state.clone();
    std::thread::Builder::new()
        .name("kb-watcher".to_string())
        .spawn(move || {
            run_watch_loop(watch_rx, state_for_loop);
        })
        .expect("spawn kb-watcher thread");

    let state_for_setup = app_state.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(app_state)
        .setup(move |app| {
            // 供 watcher 重建循环推送 `kb-changed` 事件使用。
            let _ = APP_HANDLE.set(app.handle().clone());

            // The .setup callback runs after Tauri's runtime is up, so
            // spawning here lands on the real tokio executor instead of
            // needing its own. The window can already paint while the
            // rebuild runs; serializing on `write_lock` keeps it consistent
            // with any watcher events or user commands that arrive mid-rebuild.
            tauri::async_runtime::spawn(async move {
                run_startup_rebuild(&state_for_setup).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_entries,
            commands::get_entries_snapshot,
            commands::create_relation,
            commands::delete_relation,
            commands::create_entry_simple,
            commands::delete_entry,
            commands::list_kbs,
            commands::add_kb,
            commands::remove_kb,
            commands::set_active_kb,
            commands::get_active_kb,
            commands::rebuild_active_kb,
            commands::active_kb_entry_count,
            commands::get_entry_content,
            commands::create_entry_from_reference,
            commands::validate_kb,
            commands::validate_entry,
            commands::get_system_labels,
            commands::run_validation,
            commands::get_kb_config,
            commands::update_kb_config,
            commands::apply_fix,
            commands::list_backups,
            commands::restore_backup,
            commands::update_entry_metadata,
            commands::precheck_rename,
            commands::execute_rename,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Construct a `notify-debouncer-mini` watcher rooted at `root` whose callback
/// forwards a coalesced `()` to `tx` for each debounced batch of file events.
/// Returns `None` on failure (logged, not fatal — the app runs without live
/// updates).
#[cfg(feature = "tauri")]
pub(crate) fn create_watcher(
    root: &std::path::Path,
    tx: std::sync::mpsc::Sender<Vec<std::path::PathBuf>>,
) -> Option<WatcherHandle> {
    use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
    use std::time::Duration;

    let mut debouncer = match new_debouncer(
        Duration::from_millis(500),
        move |result: DebounceEventResult| {
            // 把这一批的真实路径交给重建循环：来源判定按“路径是否全部由
            // 本进程成功写入过”来做，而不是靠粗粒度的“最近有没有写过”。
            // 读取失败（notify 错误）也触发重建，路径为空 → 按 external 处理。
            let paths = match result {
                Ok(events) => events.into_iter().map(|event| event.path).collect(),
                Err(e) => {
                    eprintln!("[watcher] debounce error: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send(paths);
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[watcher] create_debouncer failed: {e}");
            return None;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(root, notify_debouncer_mini::notify::RecursiveMode::Recursive)
    {
        eprintln!("[watcher] watch({}) failed: {e}", root.display());
        return None;
    }

    Some(WatcherHandle {
        _debouncer: debouncer,
        root: root.to_path_buf(),
    })
}

/// Drain the watch-event channel on a dedicated tokio executor. Each event
/// triggers a rebuild of the currently-active KB root, serialized against
/// user-facing mutations via `write_lock`. `rx` is a std::sync::mpsc receiver;
/// we wrap it so a `spawn_blocking` task can own the receive half while the
/// async block performs the rebuild.
#[cfg(feature = "tauri")]
fn run_watch_loop(rx: std::sync::mpsc::Receiver<Vec<std::path::PathBuf>>, state: AppState) {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    use std::time::Duration;

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[watcher] tokio runtime failed: {e}");
            return;
        }
    };

    let rx = StdArc::new(StdMutex::new(rx));

    rt.block_on(async move {
            // 上一次推送的内容指纹：相同则说明本次重建没有任何实际变化，
            // 直接跳过 emit（自身写操作之外的“无效重建”不会引起前端刷新）。
            let mut last_fingerprint: Option<String> = None;
            let mut revision: u64 = 0;

            loop {
                // Block on the next event without stalling the runtime.
                let recv = {
                    let rx = rx.clone();
                    tokio::task::spawn_blocking(move || {
                        let lock = rx.lock().unwrap();
                        lock.recv().ok()
                    })
                    .await
                };
                match recv {
                    Ok(Some(mut paths)) => {
                        // Coalesce a flurry of events into one rebuild: drain
                        // any pending batches already queued, then sleep briefly
                        // so events that arrived between recv() and the drain
                        // still collapse. Also accumulate their paths — source
                        // classification needs the whole batch.
                        let more = {
                            let rx = rx.clone();
                            tokio::task::spawn_blocking(move || {
                                let lock = rx.lock().unwrap();
                                let mut all = Vec::new();
                                while let Ok(next) = lock.try_recv() {
                                    all.extend(next);
                                }
                                all
                            })
                            .await
                            .ok()
                        };
                        if let Some(more) = more {
                            paths.extend(more);
                        }
                        paths.sort();
                        paths.dedup();
                        tokio::time::sleep(Duration::from_millis(150)).await;

                        // 重建走 `rebuild_once`
                        let kb_path = {
                            let cur = state.current_kb.read().await;
                            cur.as_ref().map(|kb| std::path::PathBuf::from(&kb.path))
                        };
                        let Some(kb_path) = kb_path else { continue };
                        if !kb_path.exists() {
                            continue;
                        }
                        let Some((before, after)) = rebuild_once(&state, &kb_path).await else {
                            continue;
                        };
                        revision = revision.saturating_add(1);

                        // 这批路径对应的正文内容缓存先失效：外部修改由文件系统
                        // 事件驱动重读，不依赖 mtime/size 的精度。
                        crate::fingerprint::invalidate_paths(&paths);

                        // 来源判定：本批事件路径是否**全部**是本进程成功写入过的。
                        // 只要混入任一外部路径就是 external；写失败的命令不会
                        // 留下记录，因此也不会把外部事件误标为 local-write。
                        let origin = if state.all_paths_locally_written(&paths) {
                            RebuildOrigin::LocalWrite
                        } else {
                            RebuildOrigin::External
                        };

                        let kb_id = {
                            let cur = state.current_kb.read().await;
                            cur.as_ref().map(|kb| kb.id.clone())
                        };
                        let Some(kb_id) = kb_id else { continue };
                        let Some(mut payload) = build_kb_changed_payload(
                            &kb_id,
                            &before,
                            &after,
                            revision,
                            last_fingerprint.as_deref(),
                            origin,
                        ) else {
                            continue;
                        };
                        // 命令内部已经改过索引，watcher 的前后快照看不出 rename；
                        // 把命令记录的增量附在事件上，供前端做增量更新。
                        payload.renamed = state.take_pending_renames();
                        last_fingerprint = Some(payload.fingerprint.clone());
                        if let Some(app) = APP_HANDLE.get() {
                            use tauri::Emitter;
                            if let Err(e) = app.emit("kb-changed", payload) {
                                eprintln!("[watcher] emit kb-changed failed: {e}");
                            }
                        }
                    }
                    _ => break, // channel closed or join task failed
                }
            }
        });
}

/// Sync the index with the on-disk KB state at app startup. Replaces the
/// pre-fix synchronous `rebuild(&kb_root)` call that ran on the main thread
/// before `tauri::Builder::run` could paint the window — for a multi-thousand
/// entry KB that was a multi-second freeze. Now spawned from the `.setup`
/// callback so it runs on the tokio executor after the runtime is up, while
/// the window is already showing.
///
/// `write_lock` serializes against the watcher loop and any user commands;
/// `Index::open` already ran `CREATE TABLE IF NOT EXISTS` so schema
/// migrations don't need this rebuild to apply.
#[cfg(feature = "tauri")]
async fn run_startup_rebuild(state: &AppState) {
    use std::path::PathBuf;

    // 启动闸门：在整段 rebuild（含校验）期间持有写锁，让首屏 `get_entries`
    // 的读锁排队，避免读到空表/半张表。提前返回时 guard 随作用域释放。
    let _gate = state.startup_gate.write().await;

    let kb_path: Option<PathBuf> = {
        let cur = state.current_kb.read().await;
        cur.as_ref().map(|kb| PathBuf::from(&kb.path))
    };
    let Some(kb_path) = kb_path else { return };
    if !kb_path.exists() {
        eprintln!(
            "[startup] active KB path does not exist: {} — skipping rebuild",
            kb_path.display()
        );
        return;
    }

    let _guard = state.write_lock.lock().await;
    let idx = state.index.read().await;
    if let Err(e) = idx.rebuild(&kb_path) {
        eprintln!("[startup] rebuild failed: {e}");
    }

    // Run validation after rebuild (still inside write_lock) so the frontend
    // has a cached report immediately on startup. validate_kb is sync and does
    // file I/O, but startup is single-threaded so blocking is acceptable.
    let report = crate::validator::validate_kb(&kb_path);
    *state.validation_report.write().await = Some(report);
}

#[cfg(all(test, feature = "tauri"))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;
    use tempfile::tempdir;

    /// `notify-debouncer-mini` fires once per debounced batch — even a flurry
    /// of changes collapses into a single event after the configured quiet
    /// window. This pins the contract the rebuild loop relies on: any change
    /// in the watched root produces at least one event within ~1s.
    #[test]
    fn watcher_fires_event_on_md_file_change() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Seed with a starter .md so the watcher has something to see.
        std::fs::write(root.join("seed.md"), "# seed\n").unwrap();

        let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
        let watcher = create_watcher(root, tx).expect("create watcher");
        // Hold the watcher alive for the test.
        let _watcher = watcher;

        // Make a change.
        std::fs::write(root.join("new.md"), "# new\n").unwrap();

        // Wait up to 2s for at least one event. notify on macOS can be flaky
        // in heavily-virtualized test runners, so we don't fail on timeout —
        // we report it as a warning. The real-world users of the watcher run
        // on developer machines where fs events propagate in milliseconds.
        let event = rx.recv_timeout(Duration::from_secs(2));
        assert!(
            event.is_ok(),
            "expected a watcher event after creating new.md within 2s"
        );
    }

    /// The debouncer should coalesce a rapid burst of writes. We don't pin
    /// the exact count of events — notify on macOS can over-emit (each file
    /// create fires multiple kernel events that may or may not collapse
    /// depending on platform), and pinning "exactly one" makes the test
    /// flaky. Instead we pin the looser contract the rebuild loop depends
    /// on: the watcher responds to a SECOND, well-separated batch of writes
    /// with another event. That proves the subscription is alive across the
    /// quiet period rather than re-firing only once.
    #[test]
    fn watcher_responds_to_two_separated_bursts() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("seed.md"), "# seed\n").unwrap();

        let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
        let watcher = create_watcher(root, tx).expect("create watcher");
        let _watcher = watcher;

        // First burst — three writes spaced 20 ms apart.
        for i in 0..3 {
            std::fs::write(root.join(format!("burst1-{i}.md")), format!("# {i}\n")).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        let first = rx.recv_timeout(Duration::from_secs(3));
        assert!(first.is_ok(), "expected an event for the first burst");

        // Wait well past the debouncer's quiet window so a second event is
        // unambiguously a fresh signal.
        std::thread::sleep(Duration::from_millis(900));

        // Second burst — three more writes.
        for i in 0..3 {
            std::fs::write(root.join(format!("burst2-{i}.md")), format!("# {i}\n")).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        let second = rx.recv_timeout(Duration::from_secs(3));
        assert!(
            second.is_ok(),
            "expected an event for the second burst — watcher subscription may have died"
        );
    }

    // ---------- run_startup_rebuild (Finding #2) ----------

    /// Construct a minimal AppState the test can drive without booting Tauri.
    /// `write_lock` is a real mutex, the index is a fresh on-disk DB, and the
    /// `current_kb` carries the path the helper reads from. `run_startup_rebuild`
    /// only consults `current_kb` — it never touches `kbs` — so the registry
    /// here is a placeholder empty registry, not a real KB list.
    fn make_test_state(kb_path: std::path::PathBuf) -> AppState {
        use crate::kb_registry::{KbRegistry, KBInfo};

        let db_path = std::env::temp_dir().join(format!(
            "synapse-kb-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let index = Index::open(&db_path).expect("open test index");

        let registry_file = std::env::temp_dir().join(format!(
            "synapse-kb-test-kbs-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let registry = KbRegistry::empty_at(registry_file);

        AppState {
            index: Arc::new(tokio::sync::RwLock::new(index)),
            kbs: Arc::new(tokio::sync::RwLock::new(registry)),
            current_kb: Arc::new(tokio::sync::RwLock::new(Some(KBInfo {
                id: "kb-test".to_string(),
                name: "test".to_string(),
                path: kb_path.to_string_lossy().to_string(),
            }))),
            validation_report: Arc::new(tokio::sync::RwLock::new(None)),
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            watcher: Arc::new(std::sync::Mutex::new(None)),
            watch_tx: Arc::new(std::sync::Mutex::new(None)),
            startup_gate: Arc::new(tokio::sync::RwLock::new(())),
            local_write_paths: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pending_renames: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// The startup rebuild must populate the DB with whatever's currently on
    /// disk — that's the whole point of running it before the user interacts
    /// with the app. Pin: a fresh DB goes from 0 entries to "all files indexed".
    #[tokio::test]
    async fn run_startup_rebuild_picks_up_existing_files() {
        let kb_dir = tempfile::tempdir().unwrap();
        let root = kb_dir.path();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        std::fs::write(root.join("b.md"), "# B\n").unwrap();
        std::fs::create_dir(root.join("compliance")).unwrap();
        std::fs::write(root.join("compliance").join("gdpr.md"), "# GDPR\n").unwrap();

        let state = make_test_state(root.to_path_buf());

        // Pre-condition: empty index.
        let pre = state.index.read().await.entry_count().unwrap();
        assert_eq!(pre, 0);

        run_startup_rebuild(&state).await;

        // Post-condition: three entries indexed.
        let post = state.index.read().await.entry_count().unwrap();
        assert_eq!(post, 3, "all three .md files should be in the DB");
    }

    /// When there is no active KB (e.g. first launch with no KBs added yet)
    /// `run_startup_rebuild` must be a no-op rather than panic. The user can
    /// still see an empty graph; the rebuild silently returns.
    #[tokio::test]
    async fn run_startup_rebuild_no_active_kb_is_noop() {
        let kb_dir = tempfile::tempdir().unwrap();
        let root = kb_dir.path();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();

        let mut state = make_test_state(root.to_path_buf());
        // Force no active KB.
        *state.current_kb.write().await = None;

        run_startup_rebuild(&state).await;

        // DB stays empty — the helper saw no active KB to walk.
        let post = state.index.read().await.entry_count().unwrap();
        assert_eq!(post, 0);
    }

    /// If the active KB's path has been deleted between sessions, the
    /// rebuild must skip silently (logged at stderr) instead of panicking.
    /// This is the common case after a user moves their KB and forgets to
    /// update the registry.
    #[tokio::test]
    async fn run_startup_rebuild_missing_kb_path_is_noop() {
        let kb_dir = tempfile::tempdir().unwrap();
        let nonexistent = kb_dir.path().join("does-not-exist");
        let state = make_test_state(nonexistent);

        run_startup_rebuild(&state).await;

        let post = state.index.read().await.entry_count().unwrap();
        assert_eq!(post, 0, "missing KB path must not crash the helper");
    }

    /// 首屏 `get_entries` 的读锁必须被启动 rebuild 的写锁挡住，否则前端
    /// 会在 rebuild 尚未写入时读到空表（空图谱之后再自己跳出来）。
    #[tokio::test]
    async fn startup_gate_blocks_reader_until_rebuild_releases_write() {
        let kb_dir = tempfile::tempdir().unwrap();
        let root = kb_dir.path();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        let state = make_test_state(root.to_path_buf());

        let gate = state.startup_gate.clone();
        let writer = gate.clone().write_owned().await;

        let reader_gate = gate.clone();
        let reader = tokio::spawn(async move {
            let _read = reader_gate.read_owned().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!reader.is_finished(), "reader must wait while rebuild holds the gate");

        drop(writer);
        reader.await.unwrap();
    }

    // ---------- watcher → 前端 `kb-changed` 事件（R5） ----------

    fn stored_entry(id: &str, title: &str, tags: &[&str]) -> crate::index::StoredEntry {
        crate::index::StoredEntry {
            id: id.to_string(),
            title: title.to_string(),
            path: format!("{id}.md"),
            content_path: format!("{id}.md"),
            group: None,
            dir: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            attachments: vec![],
            relations: vec![],
            has_content_md: false,
            labels: vec![],
            status: None,
            aliases: vec![],
            source: None,
            created: None,
            updated: None,
            ignored_rules: vec![],
            error_count: 0,
            warning_count: 0,
        }
    }

    #[test]
    fn kb_changed_payload_reports_added_and_removed() {
        let before = vec![stored_entry("a", "A", &[]), stored_entry("b", "B", &[])];
        let after = vec![stored_entry("a", "A", &[]), stored_entry("c", "C", &[])];

        let payload = build_kb_changed_payload(
            "kb-test",
            &before,
            &after,
            3,
            None,
            RebuildOrigin::External,
        )
        .expect("changed ids must produce a payload");
        assert_eq!(payload.kb_id, "kb-test");
        assert_eq!(payload.revision, 3);
        assert_eq!(payload.origin, RebuildOrigin::External);
        assert!(payload.renamed.is_empty());
        assert_eq!(payload.entry_count, 2);
        assert_eq!(payload.added, vec!["c".to_string()]);
        assert_eq!(payload.removed, vec!["b".to_string()]);
    }

    /// 内容完全相同（例如一次无关写入或自身写操作触发的重建）时不应推送
    /// 事件——这是避免前端“刷新→写文件→watcher→再刷新”回环的第一道闸。
    #[test]
    fn kb_changed_payload_is_none_when_fingerprint_unchanged() {
        let before = vec![stored_entry("a", "A", &["type:concept"])];
        let after = vec![stored_entry("a", "A", &["type:concept"])];

        let first = build_kb_changed_payload(
            "kb-test",
            &[],
            &before,
            1,
            None,
            RebuildOrigin::External,
        )
        .expect("first rebuild always notifies");
        let second = build_kb_changed_payload(
            "kb-test",
            &before,
            &after,
            2,
            Some(first.fingerprint.as_str()),
            RebuildOrigin::External,
        );
        assert!(second.is_none(), "unchanged content must not emit");
    }

    /// 只改 tags 也必须被视为内容变化（前端需要刷新标签/校验结果）。
    #[test]
    fn kb_changed_payload_detects_tag_only_edits() {
        let before = vec![stored_entry("a", "A", &["type:concept"])];
        let after = vec![stored_entry("a", "A", &["type:concept", "topic:sre"])];

        let payload = build_kb_changed_payload(
            "kb-test",
            &before,
            &after,
            1,
            None,
            RebuildOrigin::External,
        )
        .expect("tag edit must produce a payload");
        assert!(payload.added.is_empty());
        assert!(payload.removed.is_empty());
        assert_eq!(payload.entry_count, 1);
    }

    /// 事件负载必须取自 rebuild 之后的数据，并且返回时不再持有
    /// `write_lock`（调用方随后才 emit），这样前端收到事件后读到的
    /// 一定是完整索引。
    #[tokio::test]
    async fn rebuild_once_returns_post_rebuild_snapshot_and_releases_lock() {
        let kb_dir = tempfile::tempdir().unwrap();
        let root = kb_dir.path();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        let state = make_test_state(root.to_path_buf());

        let (before, after) = rebuild_once(&state, root).await.expect("rebuild must succeed");
        assert!(before.is_empty(), "fresh index has no rows before the first rebuild");
        assert_eq!(after.len(), 1, "post-rebuild snapshot must include a.md");
        assert!(
            state.write_lock.try_lock().is_ok(),
            "write_lock must be released before the caller emits the event"
        );

        std::fs::write(root.join("b.md"), "# B\n").unwrap();
        let (before2, after2) = rebuild_once(&state, root).await.expect("second rebuild");
        assert_eq!(before2.len(), 1);
        assert_eq!(after2.len(), 2);

        let payload = build_kb_changed_payload(
            "kb-test",
            &before2,
            &after2,
            2,
            None,
            RebuildOrigin::LocalWrite,
        )
        .expect("new file must produce a payload");
        assert_eq!(payload.added, vec!["b".to_string()]);
        assert_eq!(payload.entry_count, 2);
        assert_eq!(payload.origin, RebuildOrigin::LocalWrite);
    }

    /// 前端按字符串判断来源，必须与 Rust 枚举的序列化结果严格一致。
    #[test]
    fn kb_changed_payload_serializes_origin_and_renamed() {
        let payload = KbChangedPayload {
            kb_id: "kb-test".to_string(),
            entry_count: 1,
            added: vec![],
            removed: vec![],
            renamed: vec![crate::state::RenameDelta {
                from_id: "old".to_string(),
                to_id: "new".to_string(),
            }],
            origin: RebuildOrigin::LocalWrite,
            fingerprint: "fp".to_string(),
            revision: 1,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["origin"], "local-write");
        assert_eq!(json["renamed"][0]["fromId"], "old");
        assert_eq!(json["renamed"][0]["toId"], "new");

        let external = serde_json::to_value(KbChangedPayload {
            origin: RebuildOrigin::External,
            ..payload
        })
        .unwrap();
        assert_eq!(external["origin"], "external");
    }

    /// 来源判定：事件路径必须**全部**命中“本进程成功写入”的记录才算
    /// `local-write`。混入任一外部路径 → `external`；失败写入不记录 → 不会
    /// 把外部事件误标。这里用真实临时文件验证规范化路径的匹配。
    #[test]
    fn local_write_path_tracking_distinguishes_local_from_external() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_test_state(dir.path().to_path_buf());

        let mine = dir.path().join("mine.md");
        let other = dir.path().join("other.md");
        std::fs::write(&mine, "# mine\n").unwrap();
        std::fs::write(&other, "# other\n").unwrap();

        state.note_local_write(&mine);
        assert!(
            state.all_paths_locally_written(std::slice::from_ref(&mine)),
            "path written successfully by a command must classify as local-write"
        );

        // 同一次写入的第二次事件：记录已被消费，退化为 external（前端再用
        // 指纹去重），不会无限认领。
        assert!(!state.all_paths_locally_written(std::slice::from_ref(&mine)));

        // 批量里混入外部路径 → external（不能整批当成自身回声）。
        state.note_local_write(&mine);
        assert!(!state.all_paths_locally_written(&[mine.clone(), other.clone()]));

        // 没有任何本地记录 → external。
        assert!(!state.all_paths_locally_written(&[other]));
    }

    /// 同一路径“本地写成功 → 外部再改（同大小）”时，即使外部修改落在
    /// 同一个防抖批次里，也要判为 external：比对写入完成时的内容哈希，
    /// 而不是只看路径命中。这样就不依赖前端用指纹兜底。
    #[test]
    fn local_then_external_same_path_is_external() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_test_state(dir.path().to_path_buf());
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# A\n\none\n").unwrap();
        state.note_local_write(&path);

        // 外部把同大小的 "one" 改成 "two"，路径命中但内容对不上。
        std::fs::write(&path, "# A\n\ntwo\n").unwrap();
        assert!(
            !state.all_paths_locally_written(std::slice::from_ref(&path)),
            "same path but changed content must classify as external"
        );
    }

    /// 删除命令在文件已被移除之后登记路径，期望状态是 Missing；事件到达
    /// 时磁盘上确实不存在 → 仍判为 local-write。
    #[test]
    fn local_delete_matches_missing_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_test_state(dir.path().to_path_buf());
        let path = dir.path().join("gone.md");
        std::fs::write(&path, "# gone\n").unwrap();
        std::fs::remove_file(&path).unwrap();
        state.note_local_write(&path);
        assert!(state.all_paths_locally_written(std::slice::from_ref(&path)));
    }

    /// 目录重命名后登记整棵子树：新旧两侧的子文件路径都要能命中，整批
    /// 才能判为 local-write；迁移后又被外部改写的子文件则必须是 external。
    #[test]
    fn directory_rename_registers_child_paths() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_test_state(dir.path().to_path_buf());
        let old = dir.path().join("alpha");
        let new = dir.path().join("beta");
        std::fs::create_dir_all(old.join("sub")).unwrap();
        std::fs::write(old.join("content.md"), "# alpha\n").unwrap();
        std::fs::write(old.join("sub").join("b.md"), "# b\n").unwrap();
        std::fs::rename(&old, &new).unwrap();

        state.note_local_write_tree(&old, &new);

        let events = vec![
            old.join("content.md"),
            old.join("sub").join("b.md"),
            new.join("content.md"),
            new.join("sub").join("b.md"),
        ];
        assert!(
            state.all_paths_locally_written(&events),
            "child paths affected by a directory rename must all classify as local"
        );

        // 迁移后外部改了子文件：内容哈希与登记值不同 → external。
        state.note_local_write_tree(&old, &new);
        std::fs::write(new.join("content.md"), "# alpha changed\n").unwrap();
        assert!(!state.all_paths_locally_written(&[new.join("content.md")]));
    }

    /// 写命令失败时不应该留下“本地写入”记录，否则同批次外部事件会被误标。
    #[test]
    fn failed_local_write_leaves_no_trace() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_test_state(dir.path().to_path_buf());
        let never_written = dir.path().join("never.md");
        // 没调用 note_local_write（模拟校验失败/写盘失败的命令）。
        assert!(!state.all_paths_locally_written(&[never_written]));
    }

    /// 重命名增量只在下一次推送时带走一次，且同 id 的 no-op 不记录。
    #[test]
    fn pending_rename_deltas_are_drained_after_emit() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_test_state(dir.path().to_path_buf());

        state.record_rename("old", "new");
        state.record_rename("same", "same");
        let drained = state.take_pending_renames();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].from_id, "old");
        assert_eq!(drained[0].to_id, "new");
        assert!(
            state.take_pending_renames().is_empty(),
            "deltas must not accumulate across events"
        );
    }

    /// 只改正文、不动 tags/title 时也必须产生新指纹，否则 watcher 不会推送
    /// 事件，详情面板的正文永远停在旧内容。这里专门用**同大小、同一秒内**
    /// 的替换，证明判定不依赖文件时间戳精度或 size。
    #[test]
    fn entries_fingerprint_detects_body_only_edit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        std::fs::write(&file, "# A\n\nalpha\n").unwrap();

        let mut entry = stored_entry("a", "A", &[]);
        entry.content_path = file.to_string_lossy().to_string();
        let before = entries_fingerprint(std::slice::from_ref(&entry));

        // "alpha" → "bravo"：字节数相同，且紧跟在同一秒内。写入后让缓存
        // 失效（生产路径由 `note_local_write` 或 watcher 批次触发）。
        std::fs::write(&file, "# A\n\nbravo\n").unwrap();
        crate::fingerprint::invalidate_paths(std::slice::from_ref(&file));
        let after = entries_fingerprint(std::slice::from_ref(&entry));
        assert_ne!(
            before, after,
            "same-size body edit within the same second must change the fingerprint"
        );
    }

    /// 防护：`kb-changed` 的 listen 依赖 `core:event:default` 能力；后续
    /// 裁剪 capabilities 时如果误删该权限，前端会静默收不到事件、自动
    /// 刷新彻底失效。这个测试让删除动作在 CI 里直接失败并给出明确提示。
    #[test]
    fn capabilities_grant_core_event_permission() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("capabilities")
            .join("default.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()));
        let json: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("解析 {} 失败: {e}", path.display()));
        let permissions = json
            .get("permissions")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{} 缺少 permissions 数组", path.display()));
        let has_event = permissions
            .iter()
            .any(|p| p.as_str() == Some("core:event:default"));
        assert!(
            has_event,
            "capabilities/default.json 缺少 `core:event:default`：前端 `listen('kb-changed')` 会静默失败，\
             外部文件变更不再自动刷新。请勿在裁剪能力时移除该权限。"
        );
    }
}
