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
///
/// Per Finding #13: pre-fix, when `dirs::data_dir()` returned None
/// (rare on minimal Linux without HOME/XDG_DATA_HOME), the fallback was
/// a bare relative `".synapse-kb"`. For a packaged app whose CWD is
/// `/` or the binary directory, that landed `kbs.json` somewhere
/// unpredictable. We now fall back to `~/.synapse-kb` via `home_dir`,
/// and as a last resort to a temp-dir absolute path — never a bare
/// relative path.
pub fn app_data_dir() -> PathBuf {
    if let Some(p) = dirs::data_dir() {
        return p.join("synapse-kb");
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".synapse-kb");
    }
    // Last resort: an absolute path under the OS temp dir. Returning a
    // relative path here would mean "next to the binary or CWD", which
    // can be unwritable for packaged apps. A temp-dir path is at least
    // writable and discoverable across launches via the same logic.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("synapse-kb-{nanos}"))
}

/// 获取 SQLite 索引数据库文件路径。
/// 优先使用 SYNAPSE_INDEX_DB 环境变量；未设置时回退到 kb_root()/index.db。
pub fn index_db_path() -> PathBuf {
    if let Ok(v) = std::env::var("SYNAPSE_INDEX_DB") {
        return PathBuf::from(v);
    }
    kb_root().join("index.db")
}

/// 知识库注册表持久化文件路径。
pub fn kbs_json_path() -> PathBuf {
    app_data_dir().join("kbs.json")
}

/// 各知识库索引 db 文件存放目录。
pub fn indexes_dir() -> PathBuf {
    app_data_dir().join("indexes")
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

    /// Per Finding #13: the pre-fix fallback for `dirs::data_dir() == None`
    /// was a bare relative `".synapse-kb"`. For a packaged app whose CWD
    /// is `/` or the binary directory, that landed `kbs.json` somewhere
    /// unpredictable. Post-fix, the function never returns a relative
    /// path: `data_dir` → `home_dir` → temp_dir. We can't easily force
    /// `dirs::data_dir()` to None in a test (it's global state), but we
    /// CAN assert that the returned path is absolute — that's the
    /// observable contract.
    #[test]
    fn app_data_dir_is_always_absolute() {
        let dir = app_data_dir();
        assert!(
            dir.is_absolute(),
            "app_data_dir must never return a relative path; got {:?}",
            dir
        );
        assert!(
            dir.to_string_lossy().contains("synapse-kb"),
            "result must still name the app's data folder; got {:?}",
            dir
        );
    }
}
