use crate::error::AppResult;
use crate::index::StoredEntry;
use crate::state::AppState;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn set_kb_root(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(crate::error::AppError::Other(format!(
            "Path does not exist: {}",
            path.display()
        )));
    }

    {
        let index = state.index.read().await;
        index.rebuild(&path)?;
    }

    *state.kb_root.write().await = Some(path);
    Ok(())
}

#[tauri::command]
pub async fn get_entries(state: State<'_, AppState>) -> AppResult<Vec<StoredEntry>> {
    let index = state.index.read().await;
    let entries = index.get_all_entries()?;
    Ok(entries)
}

#[tauri::command]
pub async fn rebuild_index(state: State<'_, AppState>) -> AppResult<usize> {
    let root = state
        .kb_root
        .read()
        .await
        .clone()
        .ok_or_else(|| crate::error::AppError::Other("KB root not set".to_string()))?;
    let index = state.index.read().await;
    let count = index.rebuild(&root)?;
    Ok(count)
}