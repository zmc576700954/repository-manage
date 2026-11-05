//! 正文读写的底层原语。
//!
//! 编辑器保存必须用「同目录临时文件 + rename」替换目标文件：
//! 写到一半崩溃/断电时旧内容仍然完整，不会留下半截 `.md`。
//! 临时文件以 `.synapse-tmp-` 开头，scanner 会把它当作产物跳过。

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// 原子写入 `path`，返回本次使用的临时文件路径。
///
/// 返回的临时路径在成功时已不存在：调用方（写命令）可以把它登记进
/// `note_local_write`，watcher 事件里出现该路径时会被识别为本进程写入，
/// 不会把一次保存误判成外部变更。
pub fn write_atomic(path: &Path, content: &str) -> AppResult<PathBuf> {
    let parent = path.parent().ok_or_else(|| AppError::IoOp {
        message: format!("invalid content path: {}", path.display()),
    })?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::IoOp {
            message: format!("invalid content path: {}", path.display()),
        })?;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = parent.join(format!(".synapse-tmp-{file_name}-{nanos}"));

    std::fs::write(&tmp, content).map_err(|e| {
        AppError::io(format!("content_io: write temp {}", tmp.display()), e)
    })?;

    if let Err(e) = replace(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::io(
            format!(
                "content_io: replace {} -> {}",
                tmp.display(),
                path.display()
            ),
            e,
        ));
    }
    Ok(tmp)
}

/// `std::fs::rename` 在 POSIX 上是原子替换；Windows 上目标已存在会失败，
/// 因此退化为「先删目标再改名」。只在 Windows 需要这个分支——其它平台
/// 直接走 rename，不会出现目标短暂缺失的窗口。
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
    #[cfg(windows)]
    {
        match std::fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(e) if to.exists() => {
                std::fs::remove_file(to)?;
                std::fs::rename(from, to).map_err(|_| e)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "old").unwrap();

        let tmp = write_atomic(&path, "new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(!tmp.exists(), "临时文件必须在 rename 后消失");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "目录里只应留下目标文件"
        );
    }

    #[test]
    fn atomic_write_failure_does_not_corrupt_original() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "old").unwrap();

        // 让临时文件路径不可写：把父目录换成只读不合适（macOS root 仍可写），
        // 这里直接用一个非法目标验证失败分支：目标是目录。
        let target_dir = dir.path().join("dir-target");
        std::fs::create_dir(&target_dir).unwrap();
        assert!(write_atomic(&target_dir, "new").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    }
}
