use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::index::Index;

#[derive(Clone)]
pub struct AppState {
    pub kb_root: Arc<RwLock<Option<PathBuf>>>,
    pub index: Arc<RwLock<Index>>,
}