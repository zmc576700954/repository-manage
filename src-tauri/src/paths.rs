use std::path::PathBuf;

/// 读取 SYNAPSE_KB_ROOT 环境变量，返回知识库根目录。
/// 未设置时回退到平台默认应用数据目录下的 kb 子目录。
pub fn kb_root() -> PathBuf {
    if let Ok(v) = std::env::var("SYNAPSE_KB_ROOT") {
        return PathBuf::from(v);
    }
    app_data_dir().join("kb")
}

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
/// 优先使用 SYNAPSE_INDEX_DB 环境变量；未设置时回退到 kb_root()/index.db。
pub fn index_db_path() -> PathBuf {
    if let Ok(v) = std::env::var("SYNAPSE_INDEX_DB") {
        return PathBuf::from(v);
    }
    kb_root().join("index.db")
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

    #[test]
    fn kb_root_env_override() {
        // 测试需要串行执行，临时设置/清理
        std::env::set_var("SYNAPSE_KB_ROOT", "/tmp/synapse-test-kb");
        assert_eq!(kb_root(), PathBuf::from("/tmp/synapse-test-kb"));
        std::env::remove_var("SYNAPSE_KB_ROOT");
    }

    #[test]
    fn index_db_env_override() {
        std::env::set_var("SYNAPSE_INDEX_DB", "/tmp/custom-index.db");
        assert_eq!(index_db_path(), PathBuf::from("/tmp/custom-index.db"));
        std::env::remove_var("SYNAPSE_INDEX_DB");
    }
}
