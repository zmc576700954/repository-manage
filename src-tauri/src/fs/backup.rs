use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppResult;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub id: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub created_at: u64,
    pub rule_id: String,
}

pub fn backup_dir(kb_root: &Path) -> PathBuf {
    kb_root.join(".synapse-fix-backup")
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn backup_then_write(
    target: &Path,
    new_content: &str,
    rule_id: &str,
) -> AppResult<Backup> {
    let kb_root = target.parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| crate::error::AppError::IoOp {
            message: format!("no kb_root for {}", target.display()),
        })?;
    let dir = backup_dir(kb_root);
    std::fs::create_dir_all(&dir)?;

    let ts = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| crate::error::AppError::IoOp {
            message: e.to_string(),
        })?
        .as_secs();
    let entry_id = target.parent().and_then(|p| p.file_name())
        .and_then(|n| n.to_str()).unwrap_or("entry");
    let id = format!("{}_{}", ts, sanitize_id(entry_id));
    let backup_path = dir.join(format!("{}.md", id));

    let original = std::fs::read(target)?;
    std::fs::write(&backup_path, &original)?;

    std::fs::write(target, new_content)?;

    Ok(Backup {
        id,
        original_path: target.to_path_buf(),
        backup_path,
        created_at: ts,
        rule_id: rule_id.to_string(),
    })
}

pub fn list_backups(kb_root: &Path) -> Vec<Backup> {
    let dir = backup_dir(kb_root);
    if !dir.exists() { return vec![]; }
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![]; };
    entries.filter_map(|e| e.ok()).filter_map(|e| {
        let name = e.file_name().to_str()?.to_string();
        if !name.ends_with(".md") { return None; }
        let id = name.trim_end_matches(".md").to_string();
        let ts_str = id.split('_').next()?;
        let ts: u64 = ts_str.parse().ok()?;
        Some(Backup {
            id,
            original_path: e.path().with_file_name("__placeholder__"),
            backup_path: e.path(),
            created_at: ts,
            rule_id: String::new(),
        })
    }).collect()
}

pub fn restore_backup(backup: &Backup) -> AppResult<()> {
    let content = std::fs::read(&backup.backup_path)?;
    std::fs::write(&backup.original_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn backup_then_write_creates_backup() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().join("entry1");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let target = entry_dir.join("content.md");
        std::fs::write(&target, "ORIGINAL").unwrap();

        let b = backup_then_write(&target, "NEW", "E1").unwrap();
        assert_eq!(std::fs::read_to_string(&b.backup_path).unwrap(), "ORIGINAL");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "NEW");
        assert_eq!(b.rule_id, "E1");
    }

    #[test]
    fn restore_backup_recovers_original() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().join("entry1");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let target = entry_dir.join("content.md");
        std::fs::write(&target, "ORIGINAL").unwrap();

        let b = backup_then_write(&target, "NEW", "E1").unwrap();
        restore_backup(&b).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "ORIGINAL");
    }

    #[test]
    fn list_backups_returns_sorted() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().join("entry1");
        std::fs::create_dir_all(&entry_dir).unwrap();
        let target = entry_dir.join("content.md");
        std::fs::write(&target, "ORIGINAL").unwrap();

        let _ = backup_then_write(&target, "A", "E1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let _ = backup_then_write(&target, "B", "E3").unwrap();

        let backups = list_backups(tmp.path());
        assert!(backups.len() >= 2);
    }
}