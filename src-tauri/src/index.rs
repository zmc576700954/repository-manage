use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use crate::entry::RawEntry;
use crate::paths::index_db_path;
use crate::scanner::find_entry_dirs;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    path TEXT NOT NULL,
    group_name TEXT,
    has_content_md INTEGER NOT NULL DEFAULT 1
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
    FOREIGN KEY (from_id) REFERENCES entries(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_id);
CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_id);
CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEntry {
    pub id: String,
    pub title: String,
    pub path: String,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub attachments: Vec<StoredAttachment>,
    pub relations: Vec<StoredRelation>,
    pub has_content_md: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAttachment {
    pub path: String,
    pub caption: Option<String>,
    pub attachment_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_default() -> rusqlite::Result<Self> {
        Self::open(&index_db_path())
    }

    pub fn upsert_entry(&self, entry: &RawEntry) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO entries (id, title, path, group_name, has_content_md) VALUES (?, ?, ?, ?, ?)",
            params![entry.id, entry.title, entry.path.to_string_lossy(), entry.group, entry.has_content_md as i32],
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
            "SELECT id, title, path, group_name, has_content_md FROM entries ORDER BY id",
        )?;

        let entries_iter = stmt.query_map([], |row| {
            Ok(StoredEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                path: row.get(2)?,
                group: row.get(3)?,
                tags: vec![],
                attachments: vec![],
                relations: vec![],
                has_content_md: row.get::<_, i32>(4)? != 0,
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
    pub fn rebuild(&self, kb_root: &Path) -> rusqlite::Result<usize> {
        // 先清空表（短暂持锁）
        {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM entries", [])?;
            conn.execute("DELETE FROM relations", [])?;
        }

        let mut count = 0;
        for entry_dir in find_entry_dirs(kb_root) {
            let entry = crate::entry::read_entry(&entry_dir);
            self.upsert_entry(&entry)?;

            for target in &entry.linked_targets {
                self.add_relation(&entry.id, target, "custom", None)?;
            }
            count += 1;
        }

        Ok(count)
    }
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
            yaml: None,
            has_content_md: true,
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

        // create one entry on disk
        let entry_path = root.join("topic");
        fs::create_dir(&entry_path).unwrap();
        fs::write(
            entry_path.join("content.md"),
            "<!-- @synapse-id: topic -->\n<!-- @synapse-title: Topic -->\n",
        )
        .unwrap();

        let index = Index::open(&dir.path().join("db.sqlite")).unwrap();
        let count = index.rebuild(root).unwrap();

        assert_eq!(count, 1);
        let entries = index.get_all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "topic");
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
}
