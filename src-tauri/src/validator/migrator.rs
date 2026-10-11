//! `@synapse-group` → `@synapse-labels` 自动迁移。

use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct MigrationOptions {
    pub dry_run: bool,
    pub backup: bool,
}

/// Result of a migration run.
///
/// - `scanned`: total content.md files examined
/// - `would_modify`: files that would be changed (always set; in dry-run equals what would be migrated)
/// - `migrated`: files actually written (0 in dry-run)
/// - `kept_with_warning`: files where `@synapse-group` was retained (heuristic chose Keep)
/// - `backup_dir`: path to backup directory if `backup: true` and not dry-run
/// - `log_path`: path to `.synapse-migrate.log` (only written when not dry-run)
#[derive(Debug, Clone, Default)]
pub struct MigrationResult {
    pub scanned: usize,
    pub migrated: usize,
    pub kept_with_warning: usize,
    pub would_modify: usize,
    pub backup_dir: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
}

/// 迁移入口。
pub fn migrate_labels(
    kb_root: &Path,
    opts: MigrationOptions,
) -> Result<MigrationResult, String> {
    let mut result = MigrationResult::default();

    // 1. 备份
    if opts.backup && !opts.dry_run {
        let ts = chrono_now_compact();
        let backup_dir = kb_root.join(format!(".synapse-migrate-backup-{ts}"));
        copy_dir(kb_root, &backup_dir, &["content.md"]).map_err(|e| e.to_string())?;
        result.backup_dir = Some(backup_dir);
    }

    // 2. 扫所有 content.md (skip backup/log artifacts to avoid re-migrating backups)
    let entries: Vec<PathBuf> = WalkDir::new(kb_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            // Skip entries inside backup/log dirs (any ancestor is a backup artifact)
            for ancestor in e.path().ancestors() {
                if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with(".synapse-migrate-backup-") || name == ".synapse-migrate.log" {
                        return false;
                    }
                }
            }
            e.file_name() == "content.md"
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    result.scanned = entries.len();

    for entry_path in &entries {
        let content = fs::read_to_string(entry_path).map_err(|e| e.to_string())?;
        let (new_content, decision) = migrate_content(&content);

        if let Some(Decision::Migrated) = decision {
            result.would_modify += 1;
            if !opts.dry_run {
                fs::write(entry_path, &new_content).map_err(|e| e.to_string())?;
                result.migrated += 1;
            }
        } else if matches!(decision, Some(Decision::Kept)) {
            result.kept_with_warning += 1;
        }
    }

    // 3. 写 log
    if !opts.dry_run {
        let log_path = kb_root.join(".synapse-migrate.log");
        let log = format!(
            "scanned={}\nmigrated={}\nkept_with_warning={}\n",
            result.scanned, result.migrated, result.kept_with_warning
        );
        fs::write(&log_path, log).map_err(|e| e.to_string())?;
        result.log_path = Some(log_path);
    }

    Ok(result)
}

enum Decision {
    Migrated,
    Kept,
}

fn migrate_content(content: &str) -> (String, Option<Decision>) {
    let mut new_lines: Vec<String> = vec![];
    let mut kept = false;
    let mut new_label_line: Option<String> = None;

    for line in content.lines() {
        if let Some(value) = line.trim().strip_prefix("<!-- @synapse-group:") {
            let value = value.trim().trim_end_matches("-->").trim();
            let classification = classify_group_value(value);
            match classification {
                Class::Convert => {
                    if new_label_line.is_none() {
                        new_label_line = Some(format!("<!-- @synapse-labels: {} -->", value));
                    }
                    continue; // 移除旧行
                }
                Class::Keep => {
                    kept = true;
                    new_lines.push(line.to_string());
                }
            }
        } else {
            new_lines.push(line.to_string());
        }
    }

    if let Some(label_line) = new_label_line {
        // 插入到第一行（id 行之后）
        let mut inserted = false;
        let mut result_line: Vec<String> = vec![];
        for line in &new_lines {
            result_line.push(line.clone());
            if !inserted && line.trim_start().starts_with("<!-- @synapse-id:") {
                result_line.push(label_line.clone());
                inserted = true;
            }
        }
        if !inserted {
            result_line.insert(0, label_line);
        }
        (result_line.join("\n"), Some(Decision::Migrated))
    } else if kept {
        (new_lines.join("\n"), Some(Decision::Kept))
    } else {
        (content.to_string(), None)
    }
}

enum Class {
    Convert,
    Keep,
}

/// Classify a `@synapse-group` value to decide Convert vs Keep.
///
/// Rules (in order):
/// 1. Path-like (contains `/`) → Keep (denotes a category tree, not a label)
/// 2. Pure version (`1.2.3`) → Keep (not a semantic label)
/// 3. Kebab-case ASCII (`important`, `wip`) → Convert (single label)
/// 4. Unicode non-ASCII → Convert (single label, e.g., Chinese)
/// 5. Other ASCII (uppercase, mixed case) → Keep (ambiguous, user must decide)
fn classify_group_value(value: &str) -> Class {
    // 规则 1: 含 `/` → 保留
    if value.contains('/') {
        return Class::Keep;
    }
    // 规则 4: 纯版本号 → 保留
    if value
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.')
        && value.contains('.')
    {
        return Class::Keep;
    }
    // 规则 2: 全 kebab-case ASCII → 转换
    if value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.is_empty()
    {
        return Class::Convert;
    }
    // 规则 3: unicode 非 ASCII → 转换
    if value.chars().any(|c| !c.is_ascii()) {
        return Class::Convert;
    }
    Class::Keep
}

fn chrono_now_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:09}", dur.as_secs(), dur.subsec_nanos())
}

fn copy_dir(
    src: &Path,
    dst: &Path,
    extensions: &[&str],
) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let from = entry.path();

        if from == src {
            continue;
        }

        // Skip backup/log artifacts to prevent recursion and pollution
        if let Some(name) = from.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(".synapse-migrate-backup-") || name == ".synapse-migrate.log" {
                continue;
            }
        }

        let rel = from.strip_prefix(src).expect("walkdir entry under src");
        let to = dst.join(rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&to)?;
        } else if entry.file_type().is_file() {
            if let Some(name) = from.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".md") && extensions.iter().any(|e| name.ends_with(e)) {
                    fs::copy(from, &to)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_entry(dir: &Path, name: &str, content: &str) {
        let entry_dir = dir.join(name);
        fs::create_dir_all(&entry_dir).unwrap();
        fs::write(entry_dir.join("content.md"), content).unwrap();
    }

    #[test]
    fn dry_run_does_not_modify_files() {
        let dir = tempdir().unwrap();
        let kb = dir.path();
        write_entry(
            kb,
            "foo",
            "<!-- @synapse-id: foo -->\n<!-- @synapse-group: important -->\n\nbody\n",
        );

        let result = migrate_labels(
            kb,
            MigrationOptions {
                dry_run: true,
                backup: false,
            },
        )
        .unwrap();

        assert!(result.would_modify > 0);
        let content = fs::read_to_string(kb.join("foo/content.md")).unwrap();
        assert!(content.contains("@synapse-group"));
    }

    #[test]
    fn heuristic_simple_kebab_becomes_label() {
        let dir = tempdir().unwrap();
        let kb = dir.path();
        write_entry(
            kb,
            "foo",
            "<!-- @synapse-id: foo -->\n<!-- @synapse-group: important -->\n\nbody\n",
        );

        let result = migrate_labels(
            kb,
            MigrationOptions {
                dry_run: false,
                backup: false,
            },
        )
        .unwrap();

        assert_eq!(result.migrated, 1);
        let content = fs::read_to_string(kb.join("foo/content.md")).unwrap();
        assert!(content.contains("@synapse-labels: important"));
        assert!(!content.contains("@synapse-group"));
    }

    #[test]
    fn heuristic_path_like_kept() {
        let dir = tempdir().unwrap();
        let kb = dir.path();
        write_entry(
            kb,
            "foo",
            "<!-- @synapse-id: foo -->\n<!-- @synapse-group: backend/auth -->\n\nbody\n",
        );

        let result = migrate_labels(
            kb,
            MigrationOptions {
                dry_run: false,
                backup: false,
            },
        )
        .unwrap();

        assert_eq!(result.kept_with_warning, 1);
        let content = fs::read_to_string(kb.join("foo/content.md")).unwrap();
        assert!(content.contains("@synapse-group"));
    }

    #[test]
    fn idempotent_double_migration() {
        let dir = tempdir().unwrap();
        let kb = dir.path();
        write_entry(
            kb,
            "foo",
            "<!-- @synapse-id: foo -->\n<!-- @synapse-group: important -->\n\nbody\n",
        );

        let _ = migrate_labels(kb, MigrationOptions { dry_run: false, backup: false }).unwrap();
        let _ = migrate_labels(kb, MigrationOptions { dry_run: false, backup: false }).unwrap();

        let content = fs::read_to_string(kb.join("foo/content.md")).unwrap();
        let label_count = content.matches("@synapse-labels").count();
        assert_eq!(label_count, 1, "@synapse-labels must not be double-inserted");
    }

    #[test]
    fn backup_creates_directory_with_originals() {
        let dir = tempdir().unwrap();
        let kb = dir.path();
        write_entry(
            kb,
            "foo",
            "<!-- @synapse-id: foo -->\n<!-- @synapse-group: important -->\n\nbody\n",
        );

        let result = migrate_labels(kb, MigrationOptions { dry_run: false, backup: true }).unwrap();

        assert!(result.backup_dir.is_some());
        let backup = result.backup_dir.unwrap();
        assert!(backup.exists(), "backup directory should exist");
        let backup_file = backup.join("foo/content.md");
        assert!(backup_file.exists(), "backup should contain original file");
        let backup_content = fs::read_to_string(&backup_file).unwrap();
        assert!(backup_content.contains("@synapse-group"));
    }

    #[test]
    fn backup_skips_prior_backup_dir() {
        let dir = tempdir().unwrap();
        let kb = dir.path();
        // Create a fake prior backup directory in kb_root
        let prior_backup = kb.join(".synapse-migrate-backup-1234567890");
        fs::create_dir(&prior_backup).unwrap();
        fs::write(prior_backup.join("ignored.md"), "old").unwrap();

        let result = migrate_labels(kb, MigrationOptions { dry_run: false, backup: true }).unwrap();

        let new_backup = result.backup_dir.unwrap();
        let copied_prior = new_backup.join(".synapse-migrate-backup-1234567890");
        assert!(!copied_prior.exists(), "prior backup should not be copied into new backup");
    }
}
