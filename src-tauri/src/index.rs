use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::entry::{read_md_file, RawEntry};
use crate::paths::index_db_path;
use crate::scanner::find_md_files;

/// Parse a JSON array string from SQLite back into a `Vec<String>`.
/// Returns an empty vec when the column is NULL or holds an empty string
/// (older rows or DEFAULT-initialised columns). On malformed JSON we
/// return an empty vec rather than erroring — the UI simply shows the
/// entry without those labels/aliases/ignored rules, and the next
/// rebuild overwrites the bad value.
fn parse_json_vec(s: &Option<String>) -> Vec<String> {
    let s = match s.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return Vec::new(),
    };
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

// Schema notes for KB Standards v2:
// The entries table is created via `CREATE TABLE IF NOT EXISTS`, so any
// existing on-disk DB created BEFORE these columns landed will not get
// them added automatically — `IF NOT EXISTS` is a no-op on the second
// open. For dev/test we just delete `~/.local/share/synapse/index.db`.
// Production migration (Task 2.4) adds ALTER TABLE fallbacks.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    path TEXT NOT NULL,
    group_name TEXT,
    has_content_md INTEGER NOT NULL DEFAULT 1,
    labels TEXT,
    status TEXT,
    aliases TEXT,
    source TEXT,
    created TEXT,
    updated TEXT,
    ignored_rules TEXT,
    error_count INTEGER NOT NULL DEFAULT 0,
    warning_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tags (
    entry_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (entry_id, tag),
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS attachments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id TEXT NOT NULL,
    path TEXT NOT NULL,
    caption TEXT,
    attachment_type TEXT NOT NULL,
    FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    note TEXT,
    UNIQUE(from_id, to_id, relation_type),
    -- CASCADE on BOTH sides: deleting an entry must wipe both outgoing
    -- (from_id match) and incoming (to_id match) relations, otherwise
    -- delete_entry leaves orphaned rows behind. Pre-existing databases
    -- created before this dual-FK landed will need a manual DB clear
    -- to migrate; startup rebuild only re-populates entries, not schema.
    FOREIGN KEY (from_id) REFERENCES entries(id) ON DELETE CASCADE,
    FOREIGN KEY (to_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_id);
CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_id);
CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEntry {
    pub id: String,
    pub title: String,
    pub path: String,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub attachments: Vec<StoredAttachment>,
    pub relations: Vec<StoredRelation>,
    pub has_content_md: bool,
    pub labels: Vec<String>,
    pub status: Option<String>,
    pub aliases: Vec<String>,
    pub source: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub ignored_rules: Vec<String>,
    pub error_count: u32,
    pub warning_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAttachment {
    pub path: String,
    pub caption: Option<String>,
    pub attachment_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRelation {
    pub from_id: String,
    pub to_id: String,
    pub relation_type: String,
    pub note: Option<String>,
}

pub struct Index {
    conn: Mutex<Connection>,
}

impl Index {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_default() -> rusqlite::Result<Self> {
        Self::open(&index_db_path())
    }

    /// 为已存在的 DB 补齐新加的列。`CREATE TABLE IF NOT EXISTS` 对旧库无效，
    /// 所以显式 ALTER TABLE 一下。列已存在会被 SQLite 报错，我们吞掉。
    fn migrate_schema(conn: &Connection) -> rusqlite::Result<()> {
        let add_columns = [
            "ALTER TABLE entries ADD COLUMN labels TEXT",
            "ALTER TABLE entries ADD COLUMN status TEXT",
            "ALTER TABLE entries ADD COLUMN aliases TEXT",
            "ALTER TABLE entries ADD COLUMN source TEXT",
            "ALTER TABLE entries ADD COLUMN created TEXT",
            "ALTER TABLE entries ADD COLUMN updated TEXT",
            "ALTER TABLE entries ADD COLUMN ignored_rules TEXT",
            "ALTER TABLE entries ADD COLUMN error_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE entries ADD COLUMN warning_count INTEGER NOT NULL DEFAULT 0",
        ];
        for sql in add_columns {
            // 重复 ALTER 同一列会失败；用 `duplicate column name` 作为「已存在」的信号。
            match conn.execute(sql, []) {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if !msg.contains("duplicate column name") {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn upsert_entry(&self, entry: &RawEntry) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        Self::upsert_entry_in_tx(&conn, entry)
    }

    /// Per-entry upsert body extracted from `upsert_entry` so `rebuild`
    /// can call it many times inside ONE lock+transaction, instead of
    /// paying the lock/transaction overhead per entry. Pre-fix: 1000
    /// entries = 1000 `conn.lock()` acquisitions and 3000-5000 separate
    /// `INSERT`/`DELETE` round-trips. Post-fix: rebuild acquires the
    /// mutex once and wraps all upserts in a single transaction.
    ///
    /// `labels`, `aliases`, and `ignored_rules` are serialized to JSON
    /// arrays before storage. `error_count` / `warning_count` are derived
    /// from the cached `validation` vec (errors vs. warnings) rather than
    /// storing the full issue list — the UI only needs the counts; the
    /// issue details are recomputed by the validator.
    fn upsert_entry_in_tx(conn: &Connection, entry: &RawEntry) -> rusqlite::Result<()> {
        let labels_json = serde_json::to_string(&entry.labels).unwrap_or_else(|_| "[]".to_string());
        let aliases_json = serde_json::to_string(&entry.aliases).unwrap_or_else(|_| "[]".to_string());
        let ignored_json = serde_json::to_string(&entry.ignored_rules).unwrap_or_else(|_| "[]".to_string());
        let error_count = entry
            .validation
            .iter()
            .filter(|v| matches!(v.severity, crate::validator::Severity::Error) && !v.ignored)
            .count() as i32;
        let warning_count = entry
            .validation
            .iter()
            .filter(|v| matches!(v.severity, crate::validator::Severity::Warning) && !v.ignored)
            .count() as i32;

        conn.execute(
            "INSERT OR REPLACE INTO entries (id, title, path, group_name, has_content_md, \
             labels, status, aliases, source, created, updated, ignored_rules, \
             error_count, warning_count) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                entry.id,
                entry.title,
                entry.path.to_string_lossy(),
                entry.group,
                entry.has_content_md as i32,
                labels_json,
                entry.status,
                aliases_json,
                entry.source,
                entry.created,
                entry.updated,
                ignored_json,
                error_count,
                warning_count,
            ],
        )?;

        conn.execute("DELETE FROM tags WHERE entry_id = ?", params![entry.id])?;
        for tag in &entry.tags {
            conn.execute(
                "INSERT INTO tags (entry_id, tag) VALUES (?, ?)",
                params![entry.id, tag],
            )?;
        }

        conn.execute("DELETE FROM attachments WHERE entry_id = ?", params![entry.id])?;
        for att in &entry.attachments {
            conn.execute(
                "INSERT INTO attachments (entry_id, path, caption, attachment_type) VALUES (?, ?, ?, ?)",
                params![entry.id, att.path, att.caption, att.attachment_type],
            )?;
        }

        Ok(())
    }

    pub fn add_relation(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: &str,
        note: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO relations (from_id, to_id, relation_type, note) VALUES (?, ?, ?, ?)",
            params![from_id, to_id, relation_type, note],
        )?;
        Ok(())
    }

    pub fn remove_relation(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM relations WHERE from_id = ? AND to_id = ? AND relation_type = ?",
            params![from_id, to_id, relation_type],
        )?;
        Ok(())
    }

    pub fn get_all_entries(&self) -> rusqlite::Result<Vec<StoredEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, path, group_name, has_content_md, \
                    labels, status, aliases, source, created, updated, ignored_rules, \
                    error_count, warning_count \
             FROM entries ORDER BY id",
        )?;

        let entries_iter = stmt.query_map([], |row| {
            let labels_json: Option<String> = row.get(5)?;
            let aliases_json: Option<String> = row.get(7)?;
            let ignored_json: Option<String> = row.get(11)?;
            Ok(StoredEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                path: row.get(2)?,
                group: row.get(3)?,
                tags: vec![],
                attachments: vec![],
                relations: vec![],
                has_content_md: row.get::<_, i32>(4)? != 0,
                labels: parse_json_vec(&labels_json),
                status: row.get(6)?,
                aliases: parse_json_vec(&aliases_json),
                source: row.get(8)?,
                created: row.get(9)?,
                updated: row.get(10)?,
                ignored_rules: parse_json_vec(&ignored_json),
                error_count: row.get::<_, i32>(12)?.max(0) as u32,
                warning_count: row.get::<_, i32>(13)?.max(0) as u32,
            })
        })?;

        let mut entries: Vec<StoredEntry> = entries_iter.collect::<rusqlite::Result<Vec<_>>>()?;

        for entry in &mut entries {
            entry.tags = Self::get_tags(&conn, &entry.id)?;
            entry.attachments = Self::get_attachments(&conn, &entry.id)?;
            entry.relations = Self::get_relations(&conn, &entry.id)?;
        }

        Ok(entries)
    }

    /// Look up the on-disk path for a single entry by id. Returns `Ok(None)` if the
    /// id is unknown. One-row SQL lookup, no joins — used by commands.rs to avoid
    /// loading every entry + tag + attachment + relation just to find one path.
    pub fn get_entry_path(&self, id: &str) -> rusqlite::Result<Option<PathBuf>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path FROM entries WHERE id = ?")?;
        let mut rows = stmt.query_map(params![id], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(p)) => Ok(Some(PathBuf::from(p))),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Number of entries currently indexed. SQL `COUNT(*)` — no row materialization.
    pub fn entry_count(&self) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Remove an entry row by id. `tags`, `attachments`, and `relations` rows
    /// are wiped automatically by their `ON DELETE CASCADE` foreign keys, so
    /// callers don't need to clean them up. Returns the number of rows
    /// actually deleted (0 if the id was unknown).
    pub fn remove_entry(&self, id: &str) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute("DELETE FROM entries WHERE id = ?", params![id])?;
        Ok(n)
    }

    /// Resolve a single entry's `[[xxx]]` double-links and internal `[text](path)`
    /// markdown links into "reference" relations and insert them. Equivalent to
    /// `rebuild`'s Pass 2 but for one entry, and crucially does NOT wipe the
    /// relations table — caller-inserted typed relations (e.g. from
    /// `create_entry_simple`'s steps 2/3 or `create_relation`) survive.
    ///
    /// `entry` should already be in the DB (`upsert_entry` called first).
    pub fn resolve_links_for_entry(
        &self,
        kb_root: &Path,
        entry: &RawEntry,
    ) -> rusqlite::Result<()> {
        use crate::parser::is_external_url;
        use std::collections::{HashMap, HashSet};

        if entry.linked_targets.is_empty() && entry.markdown_links.is_empty() {
            return Ok(());
        }

        // Build lookup tables from current DB state.
        let (id_set, by_stem, by_rel_path) = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id, path FROM entries")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut id_set: HashSet<String> = HashSet::new();
            let mut by_stem: HashMap<String, String> = HashMap::new();
            let mut by_rel_path: HashMap<String, String> = HashMap::new();
            for row in rows {
                let (id, p) = row?;
                id_set.insert(id.clone());
                let pp = std::path::Path::new(&p);
                if let Some(stem) = pp.file_stem() {
                    by_stem
                        .entry(stem.to_string_lossy().to_string())
                        .or_insert_with(|| id.clone());
                }
                if let Ok(rel) = pp.strip_prefix(kb_root) {
                    let s = rel.to_string_lossy().replace('\\', "/");
                    by_rel_path.entry(s).or_insert_with(|| id.clone());
                    let s2 = rel.with_extension("").to_string_lossy().replace('\\', "/");
                    by_rel_path.entry(s2).or_insert_with(|| id.clone());
                }
            }
            (id_set, by_stem, by_rel_path)
        };

        // [[xxx]] 双链
        for target in &entry.linked_targets {
            if let Some(target_id) =
                resolve_target(target, &id_set, &by_stem, &by_rel_path)
            {
                if target_id != entry.id {
                    self.add_relation(&entry.id, &target_id, "reference", None)?;
                }
            }
        }
        // [text](url) markdown 链接 — 只处理 KB 内部（不含外部 URL 与附件 URL）
        for link in &entry.markdown_links {
            if is_external_url(&link.url) {
                continue;
            }
            let cleaned = link.url.split('#').next().unwrap_or(&link.url);
            let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
            if is_attachment_url(cleaned) {
                continue;
            }
            if let Some(target_id) =
                resolve_target(cleaned, &id_set, &by_stem, &by_rel_path)
            {
                if target_id != entry.id {
                    self.add_relation(&entry.id, &target_id, "reference", None)?;
                }
            }
        }

        Ok(())
    }

    fn get_tags(conn: &Connection, entry_id: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT tag FROM tags WHERE entry_id = ?")?;
        let rows = stmt.query_map(params![entry_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    fn get_attachments(conn: &Connection, entry_id: &str) -> rusqlite::Result<Vec<StoredAttachment>> {
        let mut stmt = conn.prepare(
            "SELECT path, caption, attachment_type FROM attachments WHERE entry_id = ?",
        )?;
        let rows = stmt.query_map(params![entry_id], |row| {
            Ok(StoredAttachment {
                path: row.get(0)?,
                caption: row.get(1)?,
                attachment_type: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    fn get_relations(conn: &Connection, entry_id: &str) -> rusqlite::Result<Vec<StoredRelation>> {
        let mut stmt = conn.prepare(
            "SELECT from_id, to_id, relation_type, note FROM relations WHERE from_id = ? OR to_id = ?",
        )?;
        let rows = stmt.query_map(params![entry_id, entry_id], |row| {
            Ok(StoredRelation {
                from_id: row.get(0)?,
                to_id: row.get(1)?,
                relation_type: row.get(2)?,
                note: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// 全量重建索引（从 kb 根目录扫描）。
    /// 采用"每个 .md 一个 entry"的格式：扫描 kb 根下所有 .md 文件，
    /// 直接父目录作为 group（跳过 KB 根直接下的游离 .md，比如 README.md）。
    /// Pass 1: 收集所有 entry 并插入。
    /// Pass 2: 对每个 entry 的 [[xxx]] 双链和 `[text](path)` 内链做 resolve，
    ///         命中同 KB 的另一个 entry 时自动建立 reference 关系。
    pub fn rebuild(&self, kb_root: &Path) -> rusqlite::Result<usize> {
        use crate::parser::is_external_url;
        use std::collections::{HashMap, HashSet};

        // 先清空表（短暂持锁）
        {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM entries", [])?;
            conn.execute("DELETE FROM relations", [])?;
        }

        // Pass 1: 收集并插入 entries
        let mut entries: Vec<RawEntry> = Vec::new();
        for file in find_md_files(kb_root) {
            // KB 根直接下的 .md 也作为 entry（group = null），但 README 仍跳过
            // （用户可在 KB 根放 README.md 不希望被索引）
            if file.parent() == Some(kb_root) {
                let fname = file
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if fname.eq_ignore_ascii_case("readme.md") {
                    continue;
                }
            }
            entries.push(read_md_file(&file, kb_root));
        }

        // Dedup by derived id. Two distinct .md files can map to the same
        // id: e.g. `compliance-gdpr.md` at the root → id `compliance-gdpr`,
        // and `compliance/gdpr.md` nested → also `compliance-gdpr`. The
        // schema has `id PRIMARY KEY` so the second `INSERT OR REPLACE`
        // silently overwrites the first — but pre-fix the function returned
        // `entries.len()` (the file count), which lied to callers. We keep
        // the LAST occurrence here so the DB's final row matches what the
        // walker would have written anyway. Collisions are rare but real —
        // surface them to stderr so the user can rename one of the files.
        let mut seen: HashSet<String> = HashSet::with_capacity(entries.len());
        let mut unique_entries: Vec<RawEntry> = Vec::with_capacity(entries.len());
        let mut duplicates: Vec<std::path::PathBuf> = Vec::new();
        for entry in entries.into_iter().rev() {
            if seen.insert(entry.id.clone()) {
                unique_entries.push(entry);
            } else {
                duplicates.push(entry.path);
            }
        }
        unique_entries.reverse();
        if !duplicates.is_empty() {
            eprintln!(
                "[rebuild] {} file(s) dropped due to id collision (kept the last): {:?}",
                duplicates.len(),
                duplicates
            );
        }
        let entries = unique_entries;

        // 构建 lookup 表：id 集合、file_stem -> id、rel_path -> id
        let mut id_set: HashSet<String> = HashSet::with_capacity(entries.len());
        let mut by_stem: HashMap<String, String> = HashMap::with_capacity(entries.len());
        let mut by_rel_path: HashMap<String, String> = HashMap::with_capacity(entries.len());
        for e in &entries {
            id_set.insert(e.id.clone());
            let stem = e
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            by_stem.entry(stem).or_insert_with(|| e.id.clone());
            // rel path 形式："compliance/gdpr-and-data-protection.md"
            let rel_path = e
                .path
                .strip_prefix(kb_root)
                .unwrap_or(&e.path)
                .to_string_lossy()
                .replace('\\', "/");
            by_rel_path
                .entry(rel_path)
                .or_insert_with(|| e.id.clone());
        }

        // 为每个 entry 跑校验规则，让 RawEntry.validation 有数据。
        // 这样 `error_count` / `warning_count` 才能正确写入 DB。
        let entries: Vec<RawEntry> = entries
            .into_iter()
            .map(|mut e| {
                e.validation = crate::validator::rules::check_all(&e, kb_root, &id_set);
                e
            })
            .collect();

        // Pass 1: collect + insert entries in a single transaction. Pre-fix,
// `for e in &entries { self.upsert_entry(e)?; }` acquired the mutex per
// entry and ran each INSERT outside a transaction — 1000 entries × ~5
// round-trips = 5000 separate lock-and-execute cycles. Post-fix: one
// mutex acquisition, one transaction wrapping all upserts.
        {
            let conn = self.conn.lock().unwrap();
            let tx = conn.unchecked_transaction()?;
            for e in &entries {
                Self::upsert_entry_in_tx(&tx, e)?;
            }
            tx.commit()?;
        }

        // Pass 2: 解析链接并建立 reference 关系。
        // 先收集所有候选关系（无 SQL），再在一个事务里批量 INSERT。
        // 单次 Mutex 获取 + 单次 COMMIT，避免 N+M 次单独的 lock-and-insert。
        let mut relations: Vec<(String, String)> = Vec::new();
        for entry in &entries {
            // [[xxx]] 双链
            for target in &entry.linked_targets {
                if let Some(target_id) =
                    resolve_target(target, &id_set, &by_stem, &by_rel_path)
                {
                    if target_id != entry.id {
                        relations.push((entry.id.clone(), target_id));
                    }
                }
            }
            // [text](url) markdown 链接 — 只处理 KB 内部（不含外部 URL 与附件 URL）
            for link in &entry.markdown_links {
                if is_external_url(&link.url) {
                    continue;
                }
                let cleaned = link.url.split('#').next().unwrap_or(&link.url);
                let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
                if is_attachment_url(cleaned) {
                    // 图片/PDF/视频等是附件，不是 entry 关系目标
                    continue;
                }
                if let Some(target_id) =
                    resolve_target(cleaned, &id_set, &by_stem, &by_rel_path)
                {
                    if target_id != entry.id {
                        relations.push((entry.id.clone(), target_id));
                    }
                }
            }
        }

        if !relations.is_empty() {
            let conn = self.conn.lock().unwrap();
            let tx = conn.unchecked_transaction()?;
            for (from_id, to_id) in &relations {
                tx.execute(
                    "INSERT OR IGNORE INTO relations (from_id, to_id, relation_type, note) \
                     VALUES (?, ?, 'reference', NULL)",
                    params![from_id, to_id],
                )?;
            }
            tx.commit()?;
        }

        Ok(entries.len())
    }
}

/// 把各种形式的链接 target（双链 / 相对路径 / file stem）解析到 entry.id。
fn resolve_target(
    target: &str,
    id_set: &std::collections::HashSet<String>,
    by_stem: &std::collections::HashMap<String, String>,
    by_rel_path: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let t = target.trim().trim_start_matches("./");
    if t.is_empty() {
        return None;
    }
    // 1) 完整 id 匹配
    if id_set.contains(t) {
        return Some(t.to_string());
    }
    // 2) 相对路径匹配（去掉 .md 扩展名后 lookup）
    if let Some(id) = by_rel_path.get(t) {
        return Some(id.clone());
    }
    let without_ext = t.trim_end_matches(".md");
    if let Some(id) = by_rel_path.get(without_ext) {
        return Some(id.clone());
    }
    // 3) file stem 匹配（"gdpr-and-data-protection"）
    let stem = std::path::Path::new(t)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(id) = by_stem.get(&stem) {
        return Some(id.clone());
    }
    // 4) id 末尾匹配（target 是 entry.id 的最后一段，如 "gdpr-and-data-protection"
    //    对应 entry id "compliance-gdpr-and-data-protection"）
    for id in id_set {
        if id.ends_with(&format!("-{stem}")) {
            return Some(id.clone());
        }
    }
    None
}

/// Decide whether a markdown-link URL is pointing at an attachment rather than
/// another entry. URLs ending in `.md` (or with no extension) are entries; any
/// other extension is an asset on disk — image, PDF, video, archive — and
/// must not be turned into a "reference" relation when `[[wikilink]]`-style
/// attachments (`![[xxx]]`) are the convention for asset embedding.
///
/// Without this filter, `[photo](./photo.png)` in entry A.md would resolve via
/// `by_stem["photo"]` if any entry happens to share that stem and add a
/// spurious (A, photo, "reference") edge.
fn is_attachment_url(url: &str) -> bool {
    let ext = std::path::Path::new(url)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if ext.is_empty() {
        return false; // no extension — could be a stem-only reference
    }
    if ext == "md" {
        return false; // .md entries are not attachments
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RawAttachment;
    use std::fs;
    use tempfile::tempdir;

    fn make_entry(id: &str, title: &str, path: &Path) -> RawEntry {
        RawEntry {
            id: id.to_string(),
            title: title.to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            group: Some("group-a".to_string()),
            path: path.to_path_buf(),
            content_path: path.join("content.md"),
            attachments: vec![RawAttachment {
                path: "./img.png".to_string(),
                caption: Some("cap".to_string()),
                attachment_type: "image".to_string(),
            }],
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

    #[test]
    fn opens_and_creates_schema() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let index = Index::open(&db_path).unwrap();

        // schema should be created
        assert!(db_path.exists());
        drop(index);
    }

    #[test]
    fn upserts_and_retrieves_entry() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        let entry = make_entry("foo", "Foo Title", dir.path());
        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "foo");
        assert_eq!(entries[0].title, "Foo Title");
        assert_eq!(entries[0].group, Some("group-a".to_string()));
        assert_eq!(entries[0].tags, vec!["tag1", "tag2"]);
        assert_eq!(entries[0].attachments.len(), 1);
        assert_eq!(entries[0].attachments[0].path, "./img.png");
    }

    /// Round-trip test for the 8 KB Standards v2 columns added to the
    /// entries table: `labels`, `status`, `aliases`, `source`, `created`,
    /// `updated`, `ignored_rules`, plus the cached `error_count` /
    /// `warning_count` derived from `validation`. Verifies that the
    /// JSON-array serialisation for the Vec fields and the count
    /// derivation both survive an upsert + read.
    #[test]
    fn upserts_and_retrieves_kb_v2_fields() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        let mut entry = make_entry("v2", "V2 Entry", dir.path());
        entry.labels = vec!["important".to_string(), "wip".to_string()];
        entry.status = Some("draft".to_string());
        entry.aliases = vec!["旧名".to_string()];
        entry.source = Some("https://example.com".to_string());
        entry.created = Some("2026-01-01".to_string());
        entry.updated = Some("2026-10-12".to_string());
        entry.ignored_rules = vec!["W5".to_string(), "E12".to_string()];
        entry.validation = vec![
            crate::validator::ValidationIssue {
                entry_id: "v2".to_string(),
                rule_id: "E1".to_string(),
                severity: crate::validator::Severity::Error,
                message: "first error".to_string(),
                file_line: None,
                ignored: false,
            },
            crate::validator::ValidationIssue {
                entry_id: "v2".to_string(),
                rule_id: "W1".to_string(),
                severity: crate::validator::Severity::Warning,
                message: "first warn".to_string(),
                file_line: None,
                ignored: false,
            },
            crate::validator::ValidationIssue {
                entry_id: "v2".to_string(),
                rule_id: "W2".to_string(),
                severity: crate::validator::Severity::Warning,
                message: "second warn".to_string(),
                file_line: None,
                ignored: false,
            },
        ];

        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        let got = &entries[0];
        assert_eq!(got.labels, vec!["important", "wip"]);
        assert_eq!(got.status.as_deref(), Some("draft"));
        assert_eq!(got.aliases, vec!["旧名"]);
        assert_eq!(got.source.as_deref(), Some("https://example.com"));
        assert_eq!(got.created.as_deref(), Some("2026-01-01"));
        assert_eq!(got.updated.as_deref(), Some("2026-10-12"));
        assert_eq!(got.ignored_rules, vec!["W5", "E12"]);
        assert_eq!(got.error_count, 1);
        assert_eq!(got.warning_count, 2);
    }

    /// Defaults for the new columns when an entry doesn't set them:
    /// empty Vecs, None for scalars, zero counts. The `make_entry`
    /// helper exercises this path; we read it back to confirm the
    /// DEFAULT clauses don't trip the row's NOT NULL constraints.
    #[test]
    fn upsert_with_unset_v2_fields_round_trips_empty_defaults() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        let entry = make_entry("plain", "Plain", dir.path());
        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        let got = &entries[0];
        assert!(got.labels.is_empty());
        assert!(got.status.is_none());
        assert!(got.aliases.is_empty());
        assert!(got.source.is_none());
        assert!(got.created.is_none());
        assert!(got.updated.is_none());
        assert!(got.ignored_rules.is_empty());
        assert_eq!(got.error_count, 0);
        assert_eq!(got.warning_count, 0);
    }

    #[test]
    fn upsert_replaces_tags_and_attachments() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        let mut entry = make_entry("foo", "Foo", dir.path());
        entry.tags = vec!["a".to_string()];
        index.upsert_entry(&entry).unwrap();

        entry.tags = vec!["b".to_string(), "c".to_string()];
        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries[0].tags, vec!["b", "c"]);
    }

    #[test]
    fn adds_and_removes_relations() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        // upsert entries first to satisfy FK constraint
        index.upsert_entry(&make_entry("a", "A", dir.path())).unwrap();
        index.upsert_entry(&make_entry("b", "B", dir.path())).unwrap();
        index.upsert_entry(&make_entry("d", "D", dir.path())).unwrap();

        index
            .add_relation("a", "b", "reference", Some("see also"))
            .unwrap();
        index.add_relation("a", "d", "derived", None).unwrap();

        index.remove_relation("a", "b", "reference").unwrap();

        let entries = index.get_all_entries().unwrap();
        // entries table empty, relations persist only if entries exist
        // For this test, we just verify no panic and the relation can be added
        let _ = entries;
    }

    #[test]
    fn rebuild_scans_kb_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Flat .md per entry: topic.md at KB root produces id="topic", group=None.
        fs::write(root.join("topic.md"), "# Topic\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();

        assert_eq!(count, 1);
        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "topic");
        assert_eq!(entries[0].group, None);
        assert_eq!(entries[0].title, "Topic");
    }

    /// Pin the contract behind Finding #11's Pass-1 batching: a rebuild
    /// must round-trip every entry through the single shared transaction.
    /// Pre-fix, `for e in &entries { self.upsert_entry(e) }` ran each
    /// upsert outside a transaction — semantically equivalent here, but
    /// every entry paid a separate `conn.lock()` and three implicit
    /// commits. Post-fix the loop runs inside one transaction (visible
    /// as: no partial state on panic), and the data must still be there
    /// afterward.
    ///
    /// We exercise enough entries that any "skip the Nth entry" or "drop
    /// the LAST entry's row" regression in the batched code path would
    /// surface. We index by id so the test doesn't depend on walkdir
    /// ordering.
    #[test]
    fn rebuild_pass1_transaction_inserts_all_entries() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // 20 entries — enough that an off-by-one in the transaction
        // boundary would surface as a missing LAST row.
        for i in 0..20 {
            let body = format!("# Topic {i}\n");
            fs::write(root.join(format!("entry-{i:02}.md")), body).unwrap();
        }

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();
        assert_eq!(count, 20, "rebuild must report 20 DB rows");

        // Every entry is in the DB with the right title. Group is None
        // because all files are at the KB root. The test pins "no
        // entries silently dropped at the transaction boundary."
        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 20);

        let mut by_id: std::collections::HashMap<&str, &StoredEntry> =
            std::collections::HashMap::new();
        for e in &entries {
            by_id.insert(&e.id, e);
        }
        for i in 0..20 {
            let id = format!("entry-{i:02}");
            let entry = by_id.get(id.as_str()).expect("entry must be in DB");
            assert_eq!(entry.title, format!("Topic {i}"));
            assert_eq!(entry.group, None);
        }
    }

    #[test]
    fn rebuild_uses_parent_dir_as_group() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Nested .md → group is the parent dir, id is `<group>-<stem>`.
        fs::create_dir(root.join("compliance")).unwrap();
        fs::write(root.join("compliance").join("gdpr.md"), "# GDPR\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();

        assert_eq!(count, 1);
        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries[0].id, "compliance-gdpr");
        assert_eq!(entries[0].group.as_deref(), Some("compliance"));
    }

    #[test]
    fn rebuild_skips_root_level_readme() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // KB-root README.md must not be indexed (user-facing landing page,
        // not a knowledge entry). Nested README.md is a normal entry.
        fs::write(root.join("README.md"), "# Welcome\n").unwrap();
        fs::write(root.join("note.md"), "# Note\n").unwrap();
        fs::create_dir(root.join("guide")).unwrap();
        fs::write(root.join("guide").join("README.md"), "# Guide cover\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();

        assert_eq!(count, 2, "expected 2 entries (note + guide/README)");
        let entries = index.get_all_entries().unwrap();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"note"));
        assert!(ids.contains(&"guide-README"));
        assert!(!ids.iter().any(|i| i.contains("README") && !i.contains("guide-")));
    }

    #[test]
    fn relations_are_visible_via_get_all_entries() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        index.upsert_entry(&make_entry("alpha", "Alpha", dir.path())).unwrap();
        index.upsert_entry(&make_entry("beta", "Beta", dir.path())).unwrap();

        index
            .add_relation("alpha", "beta", "reference", Some("see"))
            .unwrap();

        let entries = index.get_all_entries().unwrap();
        let alpha = entries.iter().find(|e| e.id == "alpha").unwrap();
        let beta = entries.iter().find(|e| e.id == "beta").unwrap();
        assert_eq!(alpha.relations.len(), 1);
        assert_eq!(alpha.relations[0].from_id, "alpha");
        assert_eq!(alpha.relations[0].to_id, "beta");
        assert_eq!(alpha.relations[0].relation_type, "reference");
        assert_eq!(alpha.relations[0].note.as_deref(), Some("see"));
        // beta 也应能查到这条双向关联
        assert_eq!(beta.relations.len(), 1);
    }

    #[test]
    fn removing_relation_clears_both_sides() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        index.upsert_entry(&make_entry("a", "A", dir.path())).unwrap();
        index.upsert_entry(&make_entry("b", "B", dir.path())).unwrap();

        index.add_relation("a", "b", "derived", None).unwrap();
        assert!(index
            .get_all_entries()
            .unwrap()
            .iter()
            .any(|e| e.relations.iter().any(|r| r.relation_type == "derived")));

        index.remove_relation("a", "b", "derived").unwrap();
        let entries = index.get_all_entries().unwrap();
        for entry in &entries {
            assert!(
                entry.relations.is_empty(),
                "entry {} still has relations",
                entry.id
            );
        }
    }

    #[test]
    fn duplicate_relation_is_ignored() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        index.upsert_entry(&make_entry("x", "X", dir.path())).unwrap();
        index.upsert_entry(&make_entry("y", "Y", dir.path())).unwrap();

        index.add_relation("x", "y", "extends", None).unwrap();
        index.add_relation("x", "y", "extends", None).unwrap(); // 重复

        let entries = index.get_all_entries().unwrap();
        let x = entries.iter().find(|e| e.id == "x").unwrap();
        assert_eq!(x.relations.len(), 1);
    }

    #[test]
    fn upsert_replaces_group_name() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let index = Index::open(&db_path).unwrap();

        let mut entry = make_entry("g", "G", dir.path());
        entry.group = Some("old".to_string());
        index.upsert_entry(&entry).unwrap();

        entry.group = Some("new".to_string());
        index.upsert_entry(&entry).unwrap();

        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries[0].group.as_deref(), Some("new"));
    }

    // ---------- helpers: get_entry_path / entry_count ----------

    #[test]
    fn get_entry_path_returns_path_for_known_id() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("foo.md"), "# Foo\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let path = index.get_entry_path("foo").unwrap();
        assert!(path.is_some());
        assert_eq!(
            path.unwrap().file_name().and_then(|s| s.to_str()),
            Some("foo.md")
        );
    }

    #[test]
    fn get_entry_path_returns_none_for_unknown_id() {
        let dir = tempdir().unwrap();
        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        // No rebuild — DB is empty.
        assert!(index.get_entry_path("missing").unwrap().is_none());
    }

    #[test]
    fn entry_count_matches_rebuild_return() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n").unwrap();
        fs::write(root.join("b.md"), "# B\n").unwrap();
        fs::write(root.join("c.md"), "# C\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();
        assert_eq!(count, 3);
        assert_eq!(index.entry_count().unwrap(), 3);
    }

    // ---------- remove_entry: Finding #7 ----------

    #[test]
    fn remove_entry_deletes_row_and_returns_count() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("foo.md"), "# Foo\n").unwrap();
        fs::write(root.join("bar.md"), "# Bar\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let n = index.remove_entry("foo").unwrap();
        assert_eq!(n, 1, "exactly one row should have been deleted");
        assert_eq!(index.entry_count().unwrap(), 1);

        // Calling again is a no-op (id already gone) — returns 0.
        let n = index.remove_entry("foo").unwrap();
        assert_eq!(n, 0, "second remove is a no-op");
    }

    /// Removing an entry must cascade to its tags, attachments, and relations
    /// — both as `from_id` and `to_id`. Otherwise orphaned rows accumulate
    /// and the DB grows unbounded across delete cycles.
    #[test]
    fn remove_entry_cascades_to_relations() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n").unwrap();
        fs::write(root.join("b.md"), "# B\n").unwrap();
        fs::write(root.join("c.md"), "# C\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        // Mixed relations: A→B, C→A, plus a typed edge that should be gone.
        index.add_relation("a", "b", "reference", None).unwrap();
        index.add_relation("c", "a", "reference", None).unwrap();
        index.add_relation("a", "c", "derived", None).unwrap();

        // Pre-conditions. `get_relations` returns rows where the entry is EITHER
        // `from_id` OR `to_id`, so each logical relation appears in two
        // entries' lists. Three raw edges → six "relation slots" in the
        // iteration below.
        let pre = index.get_all_entries().unwrap();
        let pre_rels: Vec<_> = pre.iter().flat_map(|e| e.relations.iter()).collect();
        assert_eq!(pre_rels.len(), 6);

        // Remove A — its outgoing (A→B, A→C) and incoming (C→A) should all
        // be wiped by ON DELETE CASCADE on the relations table.
        index.remove_entry("a").unwrap();

        let post = index.get_all_entries().unwrap();
        let post_rels: Vec<_> = post.iter().flat_map(|e| e.relations.iter()).collect();
        assert!(
            post_rels.is_empty(),
            "expected no relations after removing A, got {post_rels:?}"
        );
        assert_eq!(index.entry_count().unwrap(), 2);
    }

    #[test]
    fn remove_entry_cascades_to_tags_and_attachments() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("foo.md"), "# Foo\n").unwrap();
        fs::write(root.join("bar.md"), "# Bar\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        // Tag and attach to foo.
        let conn = index.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tags (entry_id, tag) VALUES ('foo', 't1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO attachments (entry_id, path, caption, attachment_type) \
             VALUES ('foo', './pic.png', NULL, 'image')",
            [],
        )
        .unwrap();
        drop(conn);

        index.remove_entry("foo").unwrap();

        // Tags and attachments rows for foo must be gone (CASCADE).
        let conn = index.conn.lock().unwrap();
        let tag_n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE entry_id = 'foo'", [], |r| r.get(0))
            .unwrap();
        let att_n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE entry_id = 'foo'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tag_n, 0);
        assert_eq!(att_n, 0);
    }

    /// Flat-md format: each .md is one entry. Two .md files can derive the same
    /// entry id when one is at the root (`compliance-gdpr.md` → id `compliance-gdpr`)
    /// and one is nested (`compliance/gdpr.md` → also `compliance-gdpr`). The
    /// schema's `id PRIMARY KEY` + `INSERT OR REPLACE` then silently keeps the
    /// LAST occurrence in the DB — but pre-fix `rebuild` returned `entries.len()`
    /// (the file count), so callers saw `2` while the DB actually held `1` row.
    /// This test pins that bug: the return value must match the DB row count.
    #[test]
    fn rebuild_returns_db_row_count_not_file_count() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Two files whose derived ids collide.
        fs::write(root.join("compliance-gdpr.md"), "# Flat\n").unwrap();
        fs::create_dir(root.join("compliance")).unwrap();
        fs::write(root.join("compliance").join("gdpr.md"), "# Nested\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let returned = index.rebuild(root).unwrap();

        let actual = index.entry_count().unwrap();
        assert_eq!(
            returned, actual,
            "rebuild returned {returned} but DB has {actual} rows — \
             file-count != unique-id-count after id collision"
        );
        assert_eq!(actual, 1);
    }

    /// When three files share an id, the walker only emits ONE row in the DB.
    /// Which file's data is kept depends on walkdir's iteration order
    /// (OS-dependent, not sorted by default) — that's fine; what we care about
    /// is that there's exactly one row per id, not two. This pins that the
    /// dedup logic actually collapses collisions, not just renames them.
    #[test]
    fn rebuild_dedup_keeps_one_row_per_id() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join("compliance")).unwrap();
        fs::write(root.join("compliance").join("gdpr.md"), "# First\n").unwrap();
        fs::write(root.join("compliance").join("foo.md"), "# Foo\n").unwrap();
        fs::write(root.join("compliance-gdpr.md"), "# Last\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let returned = index.rebuild(root).unwrap();

        // Two unique ids: compliance-foo + compliance-gdpr (the third file
        // shares an id with compliance/gdpr.md).
        assert_eq!(returned, 2, "rebuild must report unique-id count");
        assert_eq!(index.entry_count().unwrap(), 2);

        // Verify dedup actually happened — exactly ONE row for the
        // colliding id, not two.
        let entries = index.get_all_entries().unwrap();
        let gdpr_count = entries
            .iter()
            .filter(|e| e.id == "compliance-gdpr")
            .count();
        assert_eq!(
            gdpr_count, 1,
            "id collision must collapse to one row, got {gdpr_count}"
        );
    }

    /// `remove_entry` on an unknown id is a no-op (returns 0), not an error.
    /// This keeps delete idempotent for the UI.
    #[test]
    fn remove_entry_unknown_id_is_noop() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.md"), "# A\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let n = index.remove_entry("does-not-exist").unwrap();
        assert_eq!(n, 0);
        assert_eq!(index.entry_count().unwrap(), 1, "unrelated entry untouched");
    }

    /// The `delete_entry` Tauri command is two halves: `fs::remove_file` on
    /// the .md, then `Index::remove_entry` on the row. Pin the combined
    /// shape here so a future refactor of the command doesn't silently
    /// drop one side (e.g. a CASCADE-only delete that leaves the .md
    /// behind, or a file-only delete that leaves the row).
    #[test]
    fn delete_entry_integration_removes_file_and_row() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let file = root.join("doomed.md");
        fs::write(&file, "# Doomed\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();
        assert_eq!(index.entry_count().unwrap(), 1);

        // Step 1: command removes the .md file.
        std::fs::remove_file(&file).unwrap();
        // Step 2: command removes the DB row.
        let n = index.remove_entry("doomed").unwrap();
        assert_eq!(n, 1);

        // Both halves observed: file is gone, row is gone.
        assert!(!file.exists(), ".md must be on disk");
        assert_eq!(index.entry_count().unwrap(), 0);
    }

    // ---------- batched rebuild: cross-linked KB ----------

    #[test]
    fn rebuild_batch_inserts_all_reference_edges() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Five root .md files, each [[xxx]]-linking to the others.
        let names = ["alpha", "bravo", "charlie", "delta", "echo"];
        for n in names {
            let body = format!(
                "# {n}\n\nLinks: {}\n",
                names
                    .iter()
                    .filter(|m| **m != n)
                    .map(|m| format!("[[{m}]]"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            fs::write(root.join(format!("{n}.md")), body).unwrap();
        }

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        // Five names × 4 targets = 20 directed edges. We count via the DB
        // directly because `get_relations` includes both incoming and outgoing
        // relations per entry, which would double-count.
        let conn = index.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 20, "expected 20 reference edges after rebuild");
    }

    // ---------- resolve_links_for_entry: Finding #4 regression ----------

    /// Finding #4: `create_entry_simple`'s step 4 used to call `rebuild()`, which
    /// DELETEs the entire relations table and re-inserts only "reference" edges
    /// from `[[wikilinks]]`. That silently dropped the typed relations the caller
    /// had added in steps 2/3 (`add_relation(from, new, rtype, note)` with
    /// `rtype = "derived"` etc.). After replacing step 4 with
    /// `resolve_links_for_entry`, the typed relation must survive the post-create
    /// index update.
    #[test]
    fn resolve_links_for_entry_preserves_typed_relation() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Two root .md files. A has arbitrary content; B references A.
        fs::write(root.join("a.md"), "# A\n").unwrap();
        fs::write(root.join("b.md"), "# B\n\nSee [[a]] for context.\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        // Simulate what `create_entry_simple`'s steps 2/3 do: insert a typed
        // relation (A → B, "derived", note="see §3"). After rebuild, the DB
        // currently holds the wikilink-derived (B → A, "reference", None) edge.
        index
            .add_relation("a", "b", "derived", Some("see §3"))
            .expect("typed relation insert should succeed");

        // Sanity check before the fix path runs.
        let pre = index.get_all_entries().unwrap();
        let typed_before = pre
            .iter()
            .flat_map(|e| e.relations.iter())
            .any(|r| r.relation_type == "derived" && r.note.as_deref() == Some("see §3"));
        assert!(typed_before, "typed relation must exist before resolve");

        // Run the new post-create step on B (simulating the fix). Pre-fix this
        // path was `rebuild(&kb_root)`, which DELETEs everything — the typed
        // relation would be wiped. With the fix, only B's outgoing wikilinks
        // become "reference" edges; the pre-existing typed row survives.
        let raw_b = read_md_file(&root.join("b.md"), root);
        index
            .resolve_links_for_entry(root, &raw_b)
            .expect("resolve_links_for_entry should succeed");

        // 1) The typed relation must still be there.
        let post = index.get_all_entries().unwrap();
        let typed_after = post
            .iter()
            .flat_map(|e| e.relations.iter())
            .any(|r| r.relation_type == "derived" && r.note.as_deref() == Some("see §3"));
        assert!(
            typed_after,
            "typed relation (derived, see §3) must survive resolve_links_for_entry"
        );

        // 2) B's outgoing wikilink to A must have produced a reference edge,
        //    proving the helper is still doing its job and not a no-op.
        let b_to_a = post.iter().flat_map(|e| e.relations.iter()).any(|r| {
            r.from_id == "b" && r.to_id == "a" && r.relation_type == "reference"
        });
        assert!(
            b_to_a,
            "wikilink in B's content must still resolve to a reference edge"
        );

        // 3) The DB must not have gained phantom typed-relations on the new
        //    helper's account. Total count = 1 typed + 1 reference = 2.
        let conn = index.conn.lock().unwrap();
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2, "expected 1 typed + 1 reference edge");
    }

    /// `resolve_links_for_entry` should be a no-op for entries with no links —
    /// the early return avoids the DB lookup when there's nothing to resolve.
    #[test]
    fn resolve_links_for_entry_no_links_is_noop() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("solo.md"), "# Solo\n\nNo links here.\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let raw = read_md_file(&root.join("solo.md"), root);
        index.resolve_links_for_entry(root, &raw).unwrap();

        // No outgoing links → no new relations. rebuild() indexed nothing.
        let conn = index.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "no edges for a linkless entry");
    }

    /// External `[text](https://...)` links must be skipped — only KB-internal
    /// markdown links become relations. Mirrors `rebuild`'s Pass 2 behaviour.
    #[test]
    fn resolve_links_for_entry_skips_external_links() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("hub.md"),
            "# Hub\n\n- [ext](https://example.com/page)\n- [[plain]]\n",
        )
        .unwrap();
        fs::write(root.join("plain.md"), "# Plain\n").unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        // Pre-fix the rebuild would have added (hub, plain, "reference") via
        // wikilink and zero edges via the external https link. With the helper
        // we should get the same outcome.
        let raw = read_md_file(&root.join("hub.md"), root);
        index.resolve_links_for_entry(root, &raw).unwrap();

        let conn = index.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "only the wikilink edge should be inserted");
    }

    // ---------- is_attachment_url / attachment vs relation ----------

    #[test]
    fn is_attachment_url_classifies_extensions() {
        // No extension — could be a stem-only reference, treat as not-attachment.
        assert!(!is_attachment_url("plain"));
        assert!(!is_attachment_url("./notes/section"));
        // .md entries are not attachments.
        assert!(!is_attachment_url("./other.md"));
        assert!(!is_attachment_url("OTHER.MD"));
        // All other known extensions are attachments.
        assert!(is_attachment_url("./photo.png"));
        assert!(is_attachment_url("./PHOTO.PNG"));
        assert!(is_attachment_url("./image.jpg"));
        assert!(is_attachment_url("./photo.jpeg"));
        assert!(is_attachment_url("./anim.gif"));
        assert!(is_attachment_url("./icon.svg"));
        assert!(is_attachment_url("./doc.pdf"));
        assert!(is_attachment_url("./notes.txt"));
        assert!(is_attachment_url("./slides.pptx"));
        assert!(is_attachment_url("./demo.mp4"));
        assert!(is_attachment_url("./audio.mp3"));
        assert!(is_attachment_url("./archive.zip"));
        assert!(is_attachment_url("./backup.tar.gz")); // extension is "gz"
    }

    /// `[[wikilink]]`-style attachments (`![[xxx]]`) are the canonical way to
    /// embed an asset, but a markdown link to a local image/PDF also happens
    /// in the wild. Such URLs must NOT become "reference" relations even when
    /// their file_stem happens to match another entry's id.
    #[test]
    fn resolve_links_for_entry_skips_attachment_markdown_links() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // `photo.md` exists as an entry; the markdown-link stem "photo" would
        // normally resolve via `by_stem`. But the URL has a .png extension —
        // it must be treated as an attachment, not a relation.
        fs::write(root.join("photo.md"), "# Photo Entry\n").unwrap();
        fs::write(
            root.join("hub.md"),
            "# Hub\n\n![pic](./photo.png) and [video](./demo.mp4)\n",
        )
        .unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        // Run resolve on hub. The .png and .mp4 URLs must NOT add edges.
        let raw = read_md_file(&root.join("hub.md"), root);
        index.resolve_links_for_entry(root, &raw).unwrap();

        let conn = index.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "no edges from attachment-shaped markdown links, got {count}"
        );
    }

    /// Mixed content: one entry has both an attachment link AND a real
    /// relation. Only the real one becomes an edge.
    #[test]
    fn rebuild_separates_attachments_from_real_relations() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Two entries. hub.md has both an attachment link to photo.png AND a
        // wikilink / markdown-link to other.md.
        fs::write(root.join("other.md"), "# Other\n").unwrap();
        fs::write(
            root.join("hub.md"),
            "# Hub\n\n![pic](./photo.png)\n\nSee [other](./other.md) for more.\n",
        )
        .unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let conn = index.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "only the .md-to-.md link should be a relation");

        let mut stmt = conn
            .prepare("SELECT from_id, to_id, relation_type FROM relations")
            .unwrap();
        let edges: Vec<(String, String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0], ("hub".to_string(), "other".to_string(), "reference".to_string()));
    }

    /// `[text](https://...)` was already handled by `is_external_url`. Combined
    /// with the new filter, a mixed body should produce exactly one real
    /// edge and zero false ones.
    #[test]
    fn rebuild_skips_external_and_attachment_markdown_links() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("target.md"), "# Target\n").unwrap();
        fs::write(
            root.join("hub.md"),
            "# Hub\n\n- [web](https://example.com)\n- ![pic](./photo.png)\n- [t](./target.md)\n",
        )
        .unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        index.rebuild(root).unwrap();

        let conn = index.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
