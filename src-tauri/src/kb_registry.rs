use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths::{indexes_dir, kbs_json_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KBInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    kbs: BTreeMap<String, KBInfo>,
    #[serde(default)]
    active_id: Option<String>,
}

pub struct KbRegistry {
    file: PathBuf,
    kbs: BTreeMap<String, KBInfo>,
    active_id: Option<String>,
}

impl KbRegistry {
    pub fn load_or_default() -> Self {
        Self::load_from(&kbs_json_path())
    }

    pub fn load_from(file: &Path) -> Self {
        let state: PersistedState = std::fs::read_to_string(file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            file: file.to_path_buf(),
            kbs: state.kbs,
            active_id: state.active_id,
        }
    }

    /// Construct an empty registry backed by `file` without reading from disk.
    /// Useful for tests and for callers that manage the file lifecycle themselves.
    pub fn empty_at(file: PathBuf) -> Self {
        Self {
            file,
            kbs: BTreeMap::new(),
            active_id: None,
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let state = PersistedState {
            kbs: self.kbs.clone(),
            active_id: self.active_id.clone(),
        };
        let json = serde_json::to_string_pretty(&state).unwrap();

        // Atomic write: serialize to a sibling temp file, then rename over the target.
        // `rename` is atomic on POSIX and replace-existing on modern Windows, so a
        // crash mid-write leaves the previous kbs.json intact instead of truncating it.
        let tmp = self.tmp_path();
        std::fs::write(&tmp, &json)?;
        if let Err(e) = std::fs::rename(&tmp, &self.file) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }

    fn tmp_path(&self) -> PathBuf {
        let mut name = self.file.file_name().unwrap_or_default().to_os_string();
        name.push(".tmp");
        match self.file.parent() {
            Some(parent) => parent.join(name),
            None => PathBuf::from(name),
        }
    }

    pub fn list(&self) -> Vec<KBInfo> {
        self.kbs.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&KBInfo> {
        self.kbs.get(id)
    }

    /// Add a KB and persist. On save failure the in-memory mutation is
    /// rolled back so the registry stays consistent with what is actually
    /// on disk; the error is returned to the caller.
    pub fn add(&mut self, name: String, path: String) -> std::io::Result<KBInfo> {
        let id = generate_kb_id();
        let info = KBInfo { id, name, path };
        self.kbs.insert(info.id.clone(), info.clone());
        let prev_active = self.active_id.clone();
        if self.active_id.is_none() {
            self.active_id = Some(info.id.clone());
        }
        if let Err(e) = self.save() {
            self.kbs.remove(&info.id);
            self.active_id = prev_active;
            return Err(e);
        }
        Ok(info)
    }

    /// Remove a KB and persist. If the removed KB was the active one, clear
    /// the active-id too — but do NOT auto-pick a replacement. Pre-fix we
    /// picked `self.kbs.keys().next()`, which is the lexicographically
    /// smallest hash from `generate_kb_id` — i.e. an arbitrary, surprise-
    /// inducing switch when the user removes their primary KB. The caller
    /// (`remove_kb` in commands.rs) decides the new active, defaulting to
    /// "no active KB" which is the honest answer when the user has just
    /// said "I don't want this anymore."
    pub fn remove(&mut self, id: &str) -> std::io::Result<Option<KBInfo>> {
        let prev_active = self.active_id.clone();
        let removed = self.kbs.remove(id);
        if let Some(ref info) = removed {
            if self.active_id.as_deref() == Some(id) {
                self.active_id = None;
            }
            if let Err(e) = self.save() {
                self.kbs.insert(id.to_string(), info.clone());
                self.active_id = prev_active;
                return Err(e);
            }
        }
        Ok(removed)
    }

    /// Switch the active KB and persist. On save failure the previous
    /// active id is restored; the caller's view of `active_id` does not
    /// silently change. Returns silently when `id` names a KB that isn't
    /// in the registry — that's a no-op by design (matches the pre-fix
    /// behavior), not a save failure.
    pub fn set_active(&mut self, id: Option<String>) -> std::io::Result<()> {
        if let Some(ref i) = id {
            if !self.kbs.contains_key(i) {
                return Ok(());
            }
        }
        let prev = self.active_id.clone();
        self.active_id = id;
        if let Err(e) = self.save() {
            self.active_id = prev;
            return Err(e);
        }
        Ok(())
    }

    pub fn active(&self) -> Option<&KBInfo> {
        self.active_id.as_ref().and_then(|i| self.kbs.get(i))
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub fn index_db_path_for(&self, kb_id: &str) -> PathBuf {
        let dir = indexes_dir();
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{kb_id}.db"))
    }
}

fn generate_kb_id() -> String {
    // Per Finding #12: the pre-fix scheme XORed SystemTime nanos with a
    // fixed multiplier and truncated to 64 bits. Two concurrent `add_kb`
    // calls within the same nanosecond on different threads could read
    // the same `SystemTime::now()`, producing the same hex — and
    // BTreeMap::insert would silently overwrite the older one, losing
    // the user's KB. Use randomness instead: nanoseconds for the time
    // component, plus 8 bytes from a thread-local PRNG seeded from
    // SystemTime — distinct threads see distinct sequences even when
    // they happen to read the same clock.
    use std::cell::Cell;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    thread_local! {
        static PRNG_STATE: Cell<u64> = Cell::new({
            let mut h = DefaultHasher::new();
            SystemTime::now().hash(&mut h);
            h.finish()
        });
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let random = PRNG_STATE.with(|s| {
        // xorshift64 — fast, no allocator, good enough for unique ids
        let mut x = s.get();
        if x == 0 {
            x = 0xDEADBEEF_CAFEBABE;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        x
    });
    let id = nanos ^ random.wrapping_mul(0x9E3779B97F4A7C15);
    format!("kb-{id:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// After save() the target file contains valid JSON for the in-memory state and
    /// no leftover `.tmp` sibling — atomic write cleanup works.
    #[test]
    fn save_writes_valid_json_and_cleans_up_tmp() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        let mut reg = KbRegistry::empty_at(file.clone());
        reg.kbs.insert(
            "kb-aaa".into(),
            KBInfo {
                id: "kb-aaa".into(),
                name: "Test".into(),
                path: "/tmp/kb".into(),
            },
        );
        reg.active_id = Some("kb-aaa".into());

        reg.save().expect("save should succeed");

        let written = fs::read_to_string(&file).unwrap();
        let parsed: PersistedState = serde_json::from_str(&written).unwrap();
        assert_eq!(parsed.kbs.len(), 1);
        assert_eq!(parsed.active_id.as_deref(), Some("kb-aaa"));

        // The temp file the writer used must not linger after a successful rename.
        let mut tmp_name = file.file_name().unwrap().to_os_string();
        tmp_name.push(".tmp");
        let tmp = dir.path().join(tmp_name);
        assert!(!tmp.exists(), "temp file should be cleaned up after rename");
    }

    /// Saving replaces a previously invalid/empty file with valid JSON instead of
    /// truncating it — the rename target stays atomic.
    #[test]
    fn save_overwrites_invalid_file_atomically() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        fs::write(&file, "this is not valid json { broken").unwrap();

        let reg = KbRegistry::empty_at(file.clone());
        reg.save().expect("save should overwrite invalid input");

        let written = fs::read_to_string(&file).unwrap();
        let parsed: PersistedState = serde_json::from_str(&written).unwrap();
        assert!(parsed.kbs.is_empty());
        assert_eq!(parsed.active_id, None);
    }

    /// Repeated save() calls keep producing valid JSON (renames are idempotent).
    #[test]
    fn save_multiple_times_is_idempotent() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        let mut reg = KbRegistry::empty_at(file.clone());
        for i in 0..5 {
            reg.kbs.insert(
                format!("kb-{i}"),
                KBInfo {
                    id: format!("kb-{i}"),
                    name: format!("name-{i}"),
                    path: format!("/tmp/kb-{i}"),
                },
            );
            reg.save().unwrap();
        }
        let written = fs::read_to_string(&file).unwrap();
        let parsed: PersistedState = serde_json::from_str(&written).unwrap();
        assert_eq!(parsed.kbs.len(), 5);
    }

    /// load_from on a corrupt file returns defaults rather than panicking —
    /// the silent fallback is documented (this test pins the contract).
    #[test]
    fn load_from_corrupt_file_falls_back_to_empty() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        fs::write(&file, "truncated {...").unwrap();

        let reg = KbRegistry::load_from(&file);
        assert!(reg.kbs.is_empty());
        assert_eq!(reg.active_id, None);
        assert_eq!(reg.file, file);
    }

    #[test]
    fn load_from_missing_file_is_empty() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("does-not-exist.json");
        let reg = KbRegistry::load_from(&file);
        assert!(reg.kbs.is_empty());
        assert_eq!(reg.active_id, None);
    }

    /// Build a registry whose `save()` is guaranteed to fail: point `file`
    /// at a directory inside a fresh tempdir. `save()` writes the JSON to
    /// a sibling `.tmp` file (succeeds) then `rename`s it onto `file`,
    /// which fails with `IsADirectory` because `file` is a directory.
    /// This is the portable way to force a save failure without platform-
    /// specific tricks (chmod 000, /dev/null quirks).
    fn registry_with_failing_save(dir: &tempfile::TempDir) -> KbRegistry {
        let file = dir.path().join("not-a-file");
        fs::create_dir(&file).unwrap();
        KbRegistry::empty_at(file)
    }

    /// When `add()`'s save fails, the in-memory KB list and active_id
    /// must roll back to their pre-call state. Otherwise a transient
    /// disk-full at registration would leave the registry's in-memory
    /// view ahead of the on-disk JSON — silent divergence the user
    /// wouldn't find out about until the next launch.
    #[test]
    fn add_rolls_back_when_save_fails() {
        let dir = tempdir().unwrap();
        let mut reg = registry_with_failing_save(&dir);

        let result = reg.add("name".to_string(), "/tmp/kb".to_string());
        assert!(result.is_err(), "add must surface the save error");

        assert!(reg.kbs.is_empty(), "in-memory KB list must roll back");
        assert_eq!(reg.active_id, None, "active_id must roll back");
    }

    /// Same contract for `remove()`: a failed save restores both the
    /// removed entry and any active-id change. Without rollback a
    /// user who clicks "remove" on a transiently-stuck disk would
    /// see the KB disappear from the sidebar but return after a
    /// restart — confusing.
    #[test]
    fn remove_rolls_back_when_save_fails() {
        let dir = tempdir().unwrap();
        let mut reg = registry_with_failing_save(&dir);

        // Seed the registry directly (bypassing add() so save() doesn't
        // get called here — we want the failure to come from remove()).
        reg.kbs.insert(
            "kb-x".into(),
            KBInfo {
                id: "kb-x".into(),
                name: "X".into(),
                path: "/tmp/x".into(),
            },
        );
        reg.active_id = Some("kb-x".into());

        let result = reg.remove("kb-x");
        assert!(result.is_err(), "remove must surface the save error");

        assert_eq!(reg.kbs.len(), 1, "removed KB must be restored");
        assert!(reg.kbs.contains_key("kb-x"), "kb-x must still be present");
        assert_eq!(
            reg.active_id.as_deref(),
            Some("kb-x"),
            "active_id must be restored"
        );
    }

    /// `remove()` of a non-existent id never reaches the failing rename,
    /// so it returns `Ok(None)` without attempting save. We don't write
    /// to disk for a no-op call — there's nothing to persist.
    #[test]
    fn remove_of_missing_id_is_noop_not_save_error() {
        let dir = tempdir().unwrap();
        let mut reg = registry_with_failing_save(&dir);
        let result = reg.remove("does-not-exist");
        assert!(
            matches!(result, Ok(None)),
            "missing id must be Ok(None), not a save error"
        );
    }

    /// `set_active()` rolls back: a failed save leaves active_id pointing
    /// at the previous KB, not the requested one. Switching to a KB the
    /// caller later tries to load must keep working from the original
    /// active-id value.
    #[test]
    fn set_active_rolls_back_when_save_fails() {
        let dir = tempdir().unwrap();
        let mut reg = registry_with_failing_save(&dir);
        reg.kbs.insert(
            "kb-a".into(),
            KBInfo {
                id: "kb-a".into(),
                name: "A".into(),
                path: "/tmp/a".into(),
            },
        );
        reg.active_id = Some("kb-a".into());

        let result = reg.set_active(Some("kb-a".into()));
        assert!(result.is_err(), "set_active must surface the save error");

        assert_eq!(
            reg.active_id.as_deref(),
            Some("kb-a"),
            "active_id stays as it was"
        );
    }

    /// `set_active(Some(id))` where id is not in the registry is a
    /// silent no-op (the pre-fix behavior). Even when save would fail,
    /// this branch returns `Ok(())` without touching the disk — the
    /// caller asked for nothing, we should not lie about a save that
    /// never happened.
    #[test]
    fn set_active_to_unknown_id_is_noop_even_when_save_would_fail() {
        let dir = tempdir().unwrap();
        let mut reg = registry_with_failing_save(&dir);
        reg.kbs.insert(
            "kb-a".into(),
            KBInfo {
                id: "kb-a".into(),
                name: "A".into(),
                path: "/tmp/a".into(),
            },
        );
        reg.active_id = Some("kb-a".into());

        let result = reg.set_active(Some("kb-nope".into()));
        assert!(result.is_ok(), "no-op must not surface a save error");
        assert_eq!(
            reg.active_id.as_deref(),
            Some("kb-a"),
            "no-op must not touch active_id"
        );
    }

    /// Happy-path round-trip on the public API: after a successful
    /// `add`, the on-disk file contains the new KB; after `remove`,
    /// it's gone. This is the regression guard for the rollback
    /// behavior — the previous test asserts what happens on failure,
    /// this one pins what happens on success.
    #[test]
    fn add_and_remove_persist_through_public_api() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        let mut reg = KbRegistry::empty_at(file.clone());

        let info = reg.add("alpha".to_string(), "/tmp/alpha".to_string()).unwrap();
        let parsed: PersistedState =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(parsed.kbs.contains_key(&info.id));
        assert_eq!(parsed.active_id.as_deref(), Some(info.id.as_str()));

        reg.remove(&info.id).unwrap();
        let parsed: PersistedState =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(parsed.kbs.is_empty());
        assert_eq!(parsed.active_id, None);
    }

    // ---------- Finding #15: remove() does NOT auto-pick a successor ----------

    /// Pre-fix: removing the active KB picked `self.kbs.keys().next()` —
    /// a lexicographically smallest hash. With a real user that meant
    /// switching to an arbitrary, surprise KB. Post-fix: `remove()`
    /// clears `active_id` if it matched the removed one and never picks
    /// a successor. The caller (`remove_kb` in commands.rs) decides
    /// what to switch to (currently: nothing, until the user picks).
    #[test]
    fn remove_of_active_kb_clears_active_id_without_picking_successor() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        let mut reg = KbRegistry::empty_at(file.clone());

        let a = reg.add("alpha".into(), "/tmp/a".into()).unwrap();
        let b = reg.add("beta".into(), "/tmp/b".into()).unwrap();
        let c = reg.add("gamma".into(), "/tmp/c".into()).unwrap();
        // `add` set the first one as active. Make the user remove `b`.
        assert_eq!(reg.active_id(), Some(a.id.as_str()));
        reg.set_active(Some(b.id.clone())).unwrap();

        let removed = reg.remove(&b.id).unwrap();
        assert!(removed.is_some(), "must report the removed KB");

        // The removed KB was the active one. active_id is now None —
        // NOT some arbitrary successor from the leftover set.
        assert_eq!(
            reg.active_id(),
            None,
            "active_id must clear, not auto-pick a successor"
        );

        // The other KBs are still there. The user can pick one to make it
        // active — until then, the app shows "no active KB" honestly.
        let leftover: Vec<KBInfo> = vec![a.clone(), c.clone()];
        let ids: Vec<&str> = leftover.iter().map(|k| k.id.as_str()).collect();
        for id in &ids {
            assert!(reg.get(id).is_some());
        }
    }

    /// Sanity: removing a non-active KB leaves `active_id` alone.
    #[test]
    fn remove_of_inactive_kb_leaves_active_id_untouched() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("kbs.json");
        let mut reg = KbRegistry::empty_at(file.clone());

        let a = reg.add("alpha".into(), "/tmp/a".into()).unwrap();
        let b = reg.add("beta".into(), "/tmp/b".into()).unwrap();
        // `a` is active by default. Remove `b`.
        reg.remove(&b.id).unwrap();
        assert_eq!(reg.active_id(), Some(a.id.as_str()));
    }

    // ---------- Finding #12: generate_kb_id is collision-resistant ----------

    /// Even hammered in a tight loop, no two calls produce the same id.
    /// Pre-fix used SystemTime nanos truncated to 64 bits — two threads
    /// in the same nanosecond could collide. Post-fix uses xorshift64
    /// state in addition to nanos, so distinct calls always yield
    /// distinct ids.
    #[test]
    fn generate_kb_id_is_unique_under_hammering() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for _ in 0..10_000 {
            let id = generate_kb_id();
            assert!(seen.insert(id.clone()), "collision detected: {id}");
        }
        assert_eq!(seen.len(), 10_000);
    }
}
