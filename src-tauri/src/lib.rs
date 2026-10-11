pub mod entry;
pub mod error;
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
    let (watch_tx, watch_rx) = mpsc::channel::<()>();

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
        .setup(move |_app| {
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
    tx: std::sync::mpsc::Sender<()>,
) -> Option<WatcherHandle> {
    use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
    use std::time::Duration;

    let mut debouncer = match new_debouncer(
        Duration::from_millis(500),
        move |_result: DebounceEventResult| {
            // Forward any change event to the rebuild loop. Errors from
            // notify (e.g. path canonicalization failures) also trigger a
            // rebuild — better to over-rebuild than miss a user edit.
            let _ = tx.send(());
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
fn run_watch_loop(rx: std::sync::mpsc::Receiver<()>, state: AppState) {
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
                    Ok(Some(())) => {}
                    _ => break, // channel closed or join task failed
                }

                // Coalesce a flurry of events into one rebuild: drain any
                // pending events already queued, then sleep briefly so events
                // that arrived between recv() and the drain still collapse.
                let _ = {
                    let rx = rx.clone();
                    tokio::task::spawn_blocking(move || {
                        let lock = rx.lock().unwrap();
                        while lock.try_recv().is_ok() {}
                    })
                    .await
                };
                tokio::time::sleep(Duration::from_millis(150)).await;

                // Resolve the current KB path. If the user switched KBs or
                // there is no active KB, skip — the active-KB command path
                // already does its own rebuild and watcher swap.
                let kb_path = {
                    let cur = state.current_kb.read().await;
                    cur.as_ref().map(|kb| std::path::PathBuf::from(&kb.path))
                };
                let Some(kb_path) = kb_path else { continue };
                if !kb_path.exists() {
                    continue;
                }

                // Acquire write_lock so we don't race with `create_*` commands.
                // We hold index.read() (not write()) — rebuild's writes are
                // serialized internally by the SQLite mutex on `conn`.
                let _guard = state.write_lock.lock().await;
                let idx = state.index.read().await;
                if let Err(e) = idx.rebuild(&kb_path) {
                    eprintln!("[watcher] rebuild failed: {e}");
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
}

#[cfg(all(test, feature = "tauri"))]
mod tests {
    use super::*;
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

        let (tx, rx) = mpsc::channel::<()>();
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

        let (tx, rx) = mpsc::channel::<()>();
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
}
