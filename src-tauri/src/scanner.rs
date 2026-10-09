use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 扫描根目录，找出所有条目文件夹。
/// 条目文件夹 = 含 content.md 的子目录。
pub fn find_entry_dirs(root: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();

    if !root.exists() {
        return entries;
    }

    for entry in WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() && path.join("content.md").exists() {
            entries.push(path.to_path_buf());
        }
    }

    entries
}

/// 提取条目的 ID（相对于 root 的目录名）。
pub fn entry_id_from_path(root: &Path, entry_path: &Path) -> Option<String> {
    entry_path
        .strip_prefix(root)
        .ok()
        .and_then(|p| p.components().next())
        .map(|c| c.as_os_str().to_string_lossy().to_string())
}

/// 收集目录下所有文件路径（递归）。
pub fn collect_files(dir: &Path) -> HashSet<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn finds_direct_entry_folders() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join("topic-a")).unwrap();
        fs::write(root.join("topic-a").join("content.md"), "# A").unwrap();

        fs::create_dir(root.join("topic-b")).unwrap();
        fs::write(root.join("topic-b").join("content.md"), "# B").unwrap();

        let entries = find_entry_dirs(root);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn ignores_folders_without_content_md() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join("valid")).unwrap();
        fs::write(root.join("valid").join("content.md"), "# V").unwrap();

        fs::create_dir(root.join("invalid")).unwrap();
        fs::write(root.join("invalid").join("notes.md"), "# N").unwrap();

        let entries = find_entry_dirs(root);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("valid"));
    }

    #[test]
    fn returns_empty_when_root_missing() {
        let entries = find_entry_dirs(Path::new("/nonexistent/path"));
        assert!(entries.is_empty());
    }

    #[test]
    fn extracts_entry_id() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let entry_path = root.join("react-hooks");
        fs::create_dir(&entry_path).unwrap();

        let id = entry_id_from_path(root, &entry_path).unwrap();
        assert_eq!(id, "react-hooks");
    }
}
