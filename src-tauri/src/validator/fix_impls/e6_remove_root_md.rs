//! E6：将 root 级 .md 移入 entries/<basename>/ 并改名为 content.md

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::entry::RawEntry;
use crate::error::{AppError, AppResult};
use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

/// 在 `.synapse-fix-backup/` 下生成一个原始 .md 的备份（用于 move 类修复）
fn backup_original(content_path: &Path, kb_root: &Path, rule_id: &str) -> AppResult<()> {
    let dir = kb_root.join(".synapse-fix-backup");
    std::fs::create_dir_all(&dir)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::IoOp {
            message: e.to_string(),
        })?
        .as_secs();
    let stem = content_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("entry");
    let id = format!("{}_{}", ts, stem);
    let backup_path = dir.join(format!("{}.md", id));
    let meta_path = dir.join(format!("{}.meta", id));

    let original = std::fs::read(content_path)?;
    std::fs::write(&backup_path, &original)?;

    let meta_json = serde_json::to_vec(&serde_json::json!({
        "original_path": content_path.to_string_lossy(),
        "rule_id": rule_id,
    }))
    .map_err(|e| AppError::IoOp {
        message: e.to_string(),
    })?;
    std::fs::write(&meta_path, &meta_json)?;

    Ok(())
}

pub fn descriptor_and_apply(
    entry: &RawEntry,
    entry_dir: &Path,
    confirm: bool,
) -> AppResult<FixDescriptor> {
    if !confirm {
        return Ok(FixDescriptor {
            rule_id: "E6".into(),
            kind: FixKind::Confirm,
            risk: FixRisk::High,
            label_key: "Fix.E6.apply".into(),
            preview_key: Some("Fix.E6.preview".into()),
            confirm_key: Some("Fix.E6.confirmTitle".into()),
            docs_key: "Validation.rule.E6.fixGuide".into(),
        });
    }

    // 1. 提取 file stem 作为新目录名
    let file_stem = entry
        .content_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::IoOp {
            message: "no file stem".into(),
        })?
        .to_string();

    // 2. 计算目标路径 entry_dir/entries/<stem>/content.md
    let target_dir = entry_dir.join("entries").join(&file_stem);
    let dest = target_dir.join("content.md");

    // 3. 安全检查：目标位置已存在则报错（避免覆盖用户数据）
    if target_dir.exists() {
        return Err(AppError::FileExists {
            path: target_dir.to_string_lossy().into_owned(),
        });
    }

    // 4. 创建目标目录
    std::fs::create_dir_all(&target_dir)?;

    // 5. 先做原始文件备份（在删除源文件前），位于 entry_dir/.synapse-fix-backup/
    backup_original(&entry.content_path, entry_dir, "E6")?;

    // 6. move：先复制到 dest，再删除源（避免单步 rename 在跨设备时失败）
    std::fs::rename(&entry.content_path, &dest)?;

    Ok(FixDescriptor {
        rule_id: "E6".into(),
        kind: FixKind::Confirm,
        risk: FixRisk::High,
        label_key: "Fix.E6.apply".into(),
        preview_key: Some("Fix.E6.preview".into()),
        confirm_key: Some("Fix.E6.confirmTitle".into()),
        docs_key: "Validation.rule.E6.fixGuide".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_entry(path: std::path::PathBuf, content_path: std::path::PathBuf) -> RawEntry {
        RawEntry {
            id: String::new(),
            title: String::new(),
            tags: vec![],
            group: None,
            path,
            content_path,
            attachments: vec![],
            linked_targets: vec![],
            markdown_links: vec![],
            yaml: None,
            has_content_md: true,
            labels: vec![],
            status: None,
            aliases: vec![],
            source: None,
            created: None,
            updated: None,
            ignored_rules: vec![],
            validation: vec![],
        }
    }

    /// 未 confirm 时返回的 descriptor 形状正确，源文件未触动
    #[test]
    fn descriptor_when_not_confirmed() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().to_path_buf();
        let content_path = entry_dir.join("note.md");
        std::fs::write(&content_path, "# Note\n").unwrap();
        let entry = make_entry(entry_dir.clone(), content_path.clone());

        let desc = descriptor_and_apply(&entry, &entry_dir, false).unwrap();
        assert_eq!(desc.rule_id, "E6");
        assert!(matches!(desc.kind, FixKind::Confirm));
        assert!(matches!(desc.risk, FixRisk::High));
        assert_eq!(desc.label_key, "Fix.E6.apply");
        assert_eq!(desc.preview_key, Some("Fix.E6.preview".into()));
        assert_eq!(desc.confirm_key, Some("Fix.E6.confirmTitle".into()));
        assert_eq!(desc.docs_key, "Validation.rule.E6.fixGuide");

        // 源文件未被触动
        assert!(content_path.exists());
        assert_eq!(std::fs::read_to_string(&content_path).unwrap(), "# Note\n");
    }

    /// confirm=true 时把 root 级 .md 移入 entries/<stem>/content.md
    #[test]
    fn moves_root_md_into_entries_subdir() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().to_path_buf();
        let content_path = entry_dir.join("note.md");
        std::fs::write(&content_path, "# Note\nbody\n").unwrap();
        let entry = make_entry(entry_dir.clone(), content_path.clone());

        let desc = descriptor_and_apply(&entry, &entry_dir, true).unwrap();
        assert_eq!(desc.rule_id, "E6");

        // 源文件已删除
        assert!(!content_path.exists());

        // 目标文件已创建并内容一致
        let dest = entry_dir.join("entries").join("note").join("content.md");
        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "# Note\nbody\n");

        // 备份目录里应有原始内容
        let backup_dir = entry_dir.join(".synapse-fix-backup");
        assert!(backup_dir.exists());
        let has_backup = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".md"));
        assert!(has_backup, "should have created a backup of the original");
    }

    /// 目标已存在时拒绝（避免覆盖）
    #[test]
    fn rejects_when_target_dir_exists() {
        let tmp = TempDir::new().unwrap();
        let entry_dir = tmp.path().to_path_buf();
        let content_path = entry_dir.join("note.md");
        std::fs::write(&content_path, "# Note\n").unwrap();

        // 提前创建目标目录
        let target_dir = entry_dir.join("entries").join("note");
        std::fs::create_dir_all(&target_dir).unwrap();

        let entry = make_entry(entry_dir.clone(), content_path.clone());
        let result = descriptor_and_apply(&entry, &entry_dir, true);

        assert!(result.is_err());
        // 源文件未被删除
        assert!(content_path.exists());
        assert_eq!(std::fs::read_to_string(&content_path).unwrap(), "# Note\n");
    }
}
