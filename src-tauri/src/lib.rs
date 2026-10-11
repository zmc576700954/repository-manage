pub mod entry;
pub mod error;
pub mod index;
pub mod kb_registry;
pub mod layout_store;
pub mod parser;
pub mod paths;
pub mod scanner;
pub mod state;

#[cfg(feature = "tauri")]
pub mod commands;

#[cfg(feature = "tauri")]
use std::sync::Arc;
#[cfg(feature = "tauri")]
use tokio::sync::RwLock;

#[cfg(feature = "tauri")]
use crate::index::Index;
#[cfg(feature = "tauri")]
use crate::kb_registry::KbRegistry;
#[cfg(feature = "tauri")]
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[cfg(feature = "tauri")]
pub fn run() {
    use std::path::PathBuf;

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

    // 启动时强制 rebuild 当前 KB — 避免旧 DB schema/group 与代码不一致
    if let Some(kb) = active.as_ref() {
        let kb_path = PathBuf::from(&kb.path);
        if kb_path.exists() {
            if let Err(e) = initial_index.rebuild(&kb_path) {
                eprintln!("[startup] rebuild active KB failed: {e}");
            }
        }
    }

    let index_arc = Arc::new(RwLock::new(initial_index));
    let kbs_arc = Arc::new(RwLock::new(registry));
    let current_kb_arc = Arc::new(RwLock::new(active));

    let app_state = AppState {
        index: index_arc.clone(),
        kbs: kbs_arc.clone(),
        current_kb: current_kb_arc.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_entries,
            commands::create_relation,
            commands::delete_relation,
            commands::create_entry_simple,
            commands::list_kbs,
            commands::add_kb,
            commands::remove_kb,
            commands::set_active_kb,
            commands::get_active_kb,
            commands::rebuild_active_kb,
            commands::active_kb_entry_count,
            commands::get_entry_content,
            commands::create_entry_from_reference,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
