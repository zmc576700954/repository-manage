#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 扫描根目录，找出所有条目文件夹。
/// 条目文件夹 = 含 content.md 的子目录。
/// 跳过 `.synapse-migrate-backup-*` / `.synapse-migrate.log` 等迁移产物。
pub fn find_entry_dirs(root: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();

    if !root.exists() {
        return entries;
    }

    for entry in WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() && path.join("content.md").exists() && !is_artifact(root, path) {
            entries.push(path.to_path_buf());
        }
    }

    entries
}

/// 扫描 KB 根目录下所有 .md 文件（不限层级，最多 5 层防过深）。
/// 跳过 `.synapse-migrate-backup-*` / `.synapse-migrate.log`。
pub fn find_md_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !root.exists() {
        return files;
    }
    for entry in WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if p.is_file()
            && p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
            && !is_artifact(root, p)
        {
            files.push(p.to_path_buf());
        }
    }
    files
}

/// KB 的布局形态：决定 id 派生与 E6 等规则的适用方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbLayout {
    /// 每个 `.md` 文件就是一个 entry。
    Flat,
    /// 每个含 `content.md` 的目录是一个 entry。
    Directory,
}

/// 统一扫描入口：索引与校验都必须使用它。
/// 返回「目录 entry」与「扁平 .md entry」的完整路径列表，以及布局形态。
/// - 含 `content.md` 的目录算一个 entry，其内部的 `.md` 不再单独成为 entry
/// - 其余 `.md` 文件按扁平 entry 处理
/// - KB 根目录的 README.md 跳过（项目说明，不是条目）
pub fn scan_entry_paths(root: &Path) -> (Vec<PathBuf>, KbLayout) {
    let entry_dirs = find_entry_dirs(root);
    let dir_set: HashSet<PathBuf> = entry_dirs.iter().cloned().collect();

    let mut paths = entry_dirs;
    for file in find_md_files(root) {
        if file.ancestors().skip(1).any(|a| dir_set.contains(a)) {
            continue;
        }
        if file.parent() == Some(root)
            && file
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case("README.md"))
                .unwrap_or(false)
        {
            continue;
        }
        paths.push(file);
    }

    paths.sort();
    let layout = if dir_set.is_empty() {
        KbLayout::Flat
    } else {
        KbLayout::Directory
    };
    (paths, layout)
}

/// 路径是否为产物（备份 / 日志 / 编辑器临时文件 / 隐藏目录），
/// 需要从 KB 扫描中排除。
///
/// 任何以 `.` 开头的路径段都跳过：`.synapse`（版本历史）、
/// `.synapse-migrate-*`（迁移备份/日志）、`.synapse-tmp-*`（原子写临时文件）
/// 以及用户/工具留下的 `.obsidian`、`.trash`、`.synapse-fix-backup` 等。
/// 以点开头的文件/目录不是 KB 条目，进索引只会在侧栏和图谱里出现幽灵条目。
fn is_artifact(root: &Path, path: &Path) -> bool {
    // 只检查 KB 根以下的路径段：临时目录本身可能就叫 `.tmpXXXX`
    // （tempfile 生成），不能把整个 KB 误判成隐藏目录。
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
    })
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
    fn scan_paths_handles_flat_kb_and_skips_readme() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("note.md"), "# Note\n").unwrap();
        fs::write(root.join("README.md"), "# KB\n").unwrap();

        let (paths, layout) = scan_entry_paths(root);
        assert_eq!(layout, KbLayout::Flat);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("note.md"));
    }

    /// 编辑器临时文件与未来版本历史目录都不能被当作 KB 条目：
    /// 保存管线会在 KB 内写 `.synapse-tmp-*`，版本历史计划放在 `.synapse/`。
    #[test]
    fn scan_paths_ignores_editor_temps_and_hidden_state_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("note.md"), "# Note\n").unwrap();
        fs::write(root.join(".synapse-tmp-note.md-123"), "# half-written\n").unwrap();
        fs::create_dir_all(root.join(".synapse").join("history").join("note")).unwrap();
        fs::write(
            root.join(".synapse").join("history").join("note").join("v1.md"),
            "# old version\n",
        )
        .unwrap();

        let (paths, _) = scan_entry_paths(root);
        assert_eq!(paths.len(), 1, "只应索引 note.md");
        assert!(paths[0].ends_with("note.md"));
    }

    /// 备份工具会在 KB 里留下 `.synapse-fix-backup/`、`.trash/` 等隐藏目录，
    /// 里面的 `.md` 是历史副本，不能出现在侧栏/图谱/搜索结果里。
    #[test]
    fn scan_paths_ignores_dot_prefixed_backup_dirs() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("note.md"), "# Note\n").unwrap();
        fs::create_dir_all(root.join(".synapse-fix-backup")).unwrap();
        fs::write(
            root.join(".synapse-fix-backup").join("1700000000_old.md"),
            "# old copy\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::write(root.join(".obsidian").join("workspace.md"), "# hidden\n").unwrap();
        fs::write(root.join(".draft.md"), "# hidden draft\n").unwrap();

        let (paths, _) = scan_entry_paths(root);
        assert_eq!(paths.len(), 1, "只应索引可见的 note.md: {paths:?}");
        assert!(paths[0].ends_with("note.md"));
    }

    #[test]
    fn scan_paths_mixed_layout_counts_dir_once() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("topic-a")).unwrap();
        fs::write(root.join("topic-a").join("content.md"), "# A\n").unwrap();
        fs::write(root.join("flat.md"), "# Flat\n").unwrap();
        fs::write(root.join("README.md"), "# KB\n").unwrap();

        let (paths, layout) = scan_entry_paths(root);
        assert_eq!(layout, KbLayout::Directory);
        assert_eq!(paths.len(), 2, "content.md 不应再作为独立 entry");
        assert!(paths.iter().any(|p| p.ends_with("topic-a")));
        assert!(paths.iter().any(|p| p.ends_with("flat.md")));
        assert!(!paths.iter().any(|p| p.ends_with("content.md")));
    }
}
