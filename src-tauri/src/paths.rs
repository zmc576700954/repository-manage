use std::path::PathBuf;

/// 获取应用数据目录（跨平台）。
/// macOS: ~/Library/Application Support/synapse-kb
/// Linux: ~/.local/share/synapse-kb
/// Windows: %APPDATA%/synapse-kb
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|p| p.join("synapse-kb"))
        .unwrap_or_else(|| PathBuf::from(".synapse-kb"))
}

/// 获取 SQLite 索引数据库文件路径。
pub fn index_db_path() -> PathBuf {
    app_data_dir().join("index.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_in_app_data() {
        let dir = app_data_dir();
        assert!(dir.to_string_lossy().contains("synapse-kb"));
    }

    #[test]
    fn index_db_path_is_inside_app_data() {
        let db = index_db_path();
        let dir = app_data_dir();
        assert!(db.starts_with(&dir), "{:?} should start with {:?}", db, dir);
        assert_eq!(db.file_name().and_then(|s| s.to_str()), Some("index.db"));
    }
}
