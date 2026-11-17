//! 正文内容指纹与逐文件哈希缓存。
//!
//! 指纹必须能感知“只改正文、索引字段不变”的修改，所以每个正文文件的
//! 内容哈希都要参与。天真的做法是每次算指纹都把全部正文重读一遍——
//! 对一个 2000 篇、几十 MB 的 KB，这就是每次重建后固定多出来的几十毫秒。
//!
//! 这里的缓存**不以 mtime/size 作为失效条件**，因此不会重新引入“同秒内、
//! 同大小的正文替换漏检”的问题。失效只来自两类明确信号：
//!
//! 1. 本进程成功写入某个文件（`AppState::note_local_write` 会调用
//!    [`invalidate_paths`]）——写入后第一次算指纹时重读该文件；
//! 2. watcher 的防抖批次里出现某个路径（`run_watch_loop` 在重建后调用
//!    [`invalidate_paths`]）——外部修改由文件系统事件驱动重读，
//!    与时间戳精度无关。
//!
//! 缓存的键是正文路径（按 `entry.content_path` 原样保存，另用规范化路径
//! 建别名表以兼容 macOS `/tmp` ↔ `/private/tmp` 这类差异），值是内容
//! FNV-1a 64 位哈希。条目消失/改名时通过 `retain` 顺带裁剪，缓存不会
//! 无限增长。

use crate::index::StoredEntry;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::cell::RefCell;
#[cfg(not(test))]
use std::sync::Mutex;

#[derive(Default)]
struct Cache {
    /// 原文内容哈希。`None` 表示读取失败（文件缺失/无权限），同样缓存，
    /// 避免每次重建都对同一个坏路径做无谓的 IO。
    hashes: HashMap<PathBuf, Option<u64>>,
    /// 规范化路径 → 原文路径别名，用于把文件系统事件路径映射回缓存键。
    aliases: HashMap<PathBuf, PathBuf>,
}

#[cfg(not(test))]
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    /// 测试并行时不能共享进程级缓存：任何一个测试调用
    /// `entries_fingerprint` 都会裁剪掉其他测试的 key，`clear_cache`
    /// 也会把别人的缓存清空，导致断言/`unwrap` 偶发失败。
    /// 测试下退化为线程本地缓存，各测试之间彻底隔离。
    static TLS_CACHE: RefCell<Option<Cache>> = const { RefCell::new(None) };
}

fn with_cache<R>(f: impl FnOnce(&mut Option<Cache>) -> R) -> R {
    #[cfg(not(test))]
    {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
    #[cfg(test)]
    {
        TLS_CACHE.with(|cell| f(&mut cell.borrow_mut()))
    }
}

/// 64-bit FNV-1a。比 SipHash 便宜，足够用于“内容是否变化”的判定；
/// 指纹最终还会和条目元数据一起进 `DefaultHasher`。
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn normalize(path: &Path) -> PathBuf {
    crate::state::normalize_watch_path(path)
}

fn file_hash(path: &Path) -> Option<u64> {
    std::fs::read(path).ok().map(|bytes| fnv1a64(&bytes))
}

/// 计算条目集合的内容指纹：条目元数据 + 每个正文文件的内容哈希。
///
/// 命中缓存时只做 `HashMap` 查找；未命中（启动首轮、本地写、外部事件
/// 之后）才真正读盘。`entries` 会先按 id 排序，保证结果与查询顺序无关。
pub fn entries_fingerprint(entries: &[StoredEntry]) -> String {
    let mut sorted: Vec<&StoredEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&sorted)
        .unwrap_or_default()
        .hash(&mut hasher);

    with_cache(|guard| {
        let cache = guard.get_or_insert_with(Cache::default);

        // 裁剪掉已经不在条目集合里的路径（删除、改名、切 KB）。
        let live: HashSet<&Path> = sorted
            .iter()
            .map(|e| Path::new(e.content_path.as_str()))
            .collect();
        cache.hashes.retain(|path, _| live.contains(path.as_path()));
        cache.aliases.retain(|_, raw| live.contains(raw.as_path()));

        for entry in &sorted {
            let raw = PathBuf::from(&entry.content_path);
            let hash = match cache.hashes.get(&raw) {
                Some(hit) => *hit,
                None => {
                    let computed = file_hash(&raw);
                    cache.hashes.insert(raw.clone(), computed);
                    if raw != normalize(&raw) {
                        cache.aliases.insert(normalize(&raw), raw.clone());
                    }
                    computed
                }
            };
            (entry.id.as_str(), hash).hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    })
}

/// 无缓存的参考实现：每次都把全部正文重读一遍。
///
/// 只用于基准测试/对比，生产路径请用 [`entries_fingerprint`]。
pub fn entries_fingerprint_uncached(entries: &[StoredEntry]) -> String {
    let mut sorted: Vec<&StoredEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&sorted)
        .unwrap_or_default()
        .hash(&mut hasher);
    for entry in &sorted {
        let hash = file_hash(Path::new(&entry.content_path));
        (entry.id.as_str(), hash).hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// 使这些路径对应的缓存失效；下一次算指纹时会重读它们。
pub fn invalidate_paths(paths: &[PathBuf]) {
    with_cache(|guard| {
        let Some(cache) = guard.as_mut() else {
            return;
        };
        for path in paths {
            cache.hashes.remove(path);
            let normalized = normalize(path);
            if let Some(raw) = cache.aliases.remove(&normalized) {
                cache.hashes.remove(&raw);
            }
            if normalized != *path {
                cache.hashes.remove(&normalized);
            }
        }
    });
}

/// 清空缓存。测试与基准用，生产不需要。
pub fn clear_cache() {
    with_cache(|guard| *guard = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, path: &Path) -> StoredEntry {
        StoredEntry {
            id: id.to_string(),
            title: id.to_string(),
            path: format!("{id}.md"),
            content_path: path.to_string_lossy().to_string(),
            group: None,
            dir: None,
            tags: vec![],
            attachments: vec![],
            relations: vec![],
            has_content_md: false,
            labels: vec![],
            status: None,
            aliases: vec![],
            source: None,
            created: None,
            updated: None,
            ignored_rules: vec![],
            error_count: 0,
            extra_meta: vec![],
            doc_code: None,
            warning_count: 0,
        }
    }

    /// 同一路径在缓存有效期内被外部改掉且未发事件时，缓存会返回旧哈希；
    /// 但只要事件到达并调用 invalidate_paths，就必须重读并感知变化。
    /// 这里刻意用同大小、且把 mtime 还原的替换，证明失效不依赖时间戳。
    #[test]
    fn invalidate_detects_same_size_same_mtime_change() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        std::fs::write(&file, "# A\n\nalpha\n").unwrap();

        clear_cache();
        let e = entry("a", &file);
        let before = entries_fingerprint(std::slice::from_ref(&e));

        let before_meta = std::fs::metadata(&file).unwrap();
        std::fs::write(&file, "# A\n\nbravo\n").unwrap();
        // 还原 mtime（同大小 + 同 mtime：只看时间戳的实现会漏检）。
        let times = std::fs::FileTimes::new().set_modified(before_meta.modified().unwrap());
        let f = std::fs::File::options().write(true).open(&file).unwrap();
        f.set_times(times).unwrap();
        drop(f);

        invalidate_paths(&[file.clone()]);
        let after = entries_fingerprint(std::slice::from_ref(&e));
        assert_ne!(before, after, "same-size/same-mtime edit must be detected");
        clear_cache();
    }

    /// 合成大 KB 基准：默认 ignored，不拖慢日常 `cargo test`。
    /// `cargo test --no-default-features --lib -- --ignored --nocapture fingerprint_bench`
    #[test]
    #[ignore]
    fn fingerprint_benchmark_2000_entries() {
        use std::time::{Duration, Instant};
        use crate::index::Index;

        fn median(mut values: Vec<Duration>) -> Duration {
            values.sort();
            values[values.len() / 2]
        }
        fn ms(d: Duration) -> f64 {
            d.as_secs_f64() * 1000.0
        }

        let dir = tempfile::tempdir().unwrap();
        let kb_root = dir.path().join("kb");
        std::fs::create_dir_all(&kb_root).unwrap();
        let mut files: Vec<PathBuf> = Vec::new();
        for i in 0..1800 {
            let path = kb_root.join(format!("note-{i:04}.md"));
            let body = format!(
                "<!-- @synapse-id: note-{i:04} -->\n# Note {i}\n\n{}",
                "small body line\n".repeat(48)
            );
            std::fs::write(&path, body).unwrap();
            files.push(path);
        }
        let big = "big body line for fingerprint benchmark\n".repeat(6000);
        for i in 0..200 {
            let path = kb_root.join(format!("big-{i:04}.md"));
            let body = format!("<!-- @synapse-id: big-{i:04} -->\n# Big {i}\n\n{big}");
            std::fs::write(&path, body).unwrap();
            files.push(path);
        }
        let total_bytes: u64 = files
            .iter()
            .map(|p| std::fs::metadata(p).unwrap().len())
            .sum();

        let index = Index::open(&dir.path().join("index.db")).unwrap();
        let mut rebuild_times = Vec::new();
        for _ in 0..3 {
            let started = Instant::now();
            index.rebuild(&kb_root).unwrap();
            rebuild_times.push(started.elapsed());
        }
        let entries = index.get_all_entries().unwrap();

        let mut uncached_times = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            entries_fingerprint_uncached(&entries);
            uncached_times.push(started.elapsed());
        }

        clear_cache();
        let cold_started = Instant::now();
        let cold = entries_fingerprint(&entries);
        let cold_elapsed = cold_started.elapsed();
        let mut cached_times = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            assert_eq!(entries_fingerprint(&entries), cold);
            cached_times.push(started.elapsed());
        }

        println!(
            "[bench] entries={} files={} total_bytes={:.1}MB",
            entries.len(),
            files.len(),
            total_bytes as f64 / 1_048_576.0
        );
        println!(
            "[bench] rebuild            median={:.1}ms worst={:.1}ms (n=3)",
            ms(median(rebuild_times.clone())),
            ms(*rebuild_times.iter().max().unwrap())
        );
        println!(
            "[bench] fingerprint full   median={:.1}ms worst={:.1}ms (n=5)",
            ms(median(uncached_times.clone())),
            ms(*uncached_times.iter().max().unwrap())
        );
        println!(
            "[bench] fingerprint cached median={:.1}ms worst={:.1}ms (n=5), cold={:.1}ms",
            ms(median(cached_times.clone())),
            ms(*cached_times.iter().max().unwrap()),
            ms(cold_elapsed)
        );
        println!(
            "[bench] warm speedup ≈ {:.1}x",
            ms(median(uncached_times.clone())) / ms(median(cached_times))
        );

        // 回归证明：同大小 + 同 mtime 的正文替换仍必须被识别。
        let target = files[0].clone();
        let before_meta = std::fs::metadata(&target).unwrap();
        let before_fp = entries_fingerprint(&entries);
        let text = std::fs::read_to_string(&target).unwrap();
        assert!(text.contains("small body line"));
        let replaced = text.replacen("small body line", "SMALL BODY LINE", 1);
        assert_eq!(replaced.len(), text.len(), "replacement must keep byte length");
        std::fs::write(&target, replaced).unwrap();
        filetime::set_file_mtime(
            &target,
            filetime::FileTime::from_last_modification_time(&before_meta),
        )
        .unwrap();
        invalidate_paths(std::slice::from_ref(&target));
        let detected = entries_fingerprint(&entries) != before_fp;
        println!("[bench] same-size/same-mtime edit detected: {detected}");
        assert!(detected);

        clear_cache();
    }

    #[test]
    fn prune_drops_entries_that_are_no_longer_present() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        std::fs::write(&a, "# A\n").unwrap();
        std::fs::write(&b, "# B\n").unwrap();

        clear_cache();
        let ea = entry("a", &a);
        let eb = entry("b", &b);
        entries_fingerprint(&[ea.clone(), eb.clone()]);
        let cache_len = with_cache(|guard| guard.as_ref().unwrap().hashes.len());
        assert_eq!(cache_len, 2);

        entries_fingerprint(std::slice::from_ref(&ea));
        let cache_len = with_cache(|guard| guard.as_ref().unwrap().hashes.len());
        assert_eq!(cache_len, 1, "removed entries must be pruned");
        clear_cache();
    }
}
