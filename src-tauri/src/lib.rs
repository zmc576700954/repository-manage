mod commands;
mod entry;
mod error;
mod index;
mod layout_store;
mod local_server;
mod parser;
mod paths;
mod scanner;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::index::Index;
use crate::local_server::{serve as serve_local, AppState as ServerState};
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let index = Index::open_default().expect("Failed to open index");

    let kb_root: Arc<RwLock<Option<PathBuf>>> = Arc::new(RwLock::new(None));
    let index_arc = Arc::new(RwLock::new(index));

    let app_state = AppState {
        kb_root: kb_root.clone(),
        index: index_arc.clone(),
    };

    let server_state = ServerState {
        index: index_arc.clone(),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::set_kb_root,
            commands::get_entries,
            commands::rebuild_index,
        ])
        .setup(move |_app| {
            tokio::spawn(async move {
                if let Err(e) = serve_local(server_state).await {
                    tracing::error!("Local server error: {}", e);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}