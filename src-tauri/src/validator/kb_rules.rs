//! KB 级校验规则（kaiwu × rm 合并批次 2）。
//!
//! 与 check_kb.py 的 L7/L8/L9 同构（映射表见 docs/kb-spec.md 附录 A）：
//! - W9 = L7：同 KB 内文件 stem 重复（链接解析 stem 兜底会二义）。
//! - W10 = L8：README `## 依赖矩阵` 的「依赖」列 ⊆ 该文档 `## 相关文档` 目标。
//! - W11 = L9：依赖/被依赖两列互惠。
//!
//! 条件规则哲学（规范 §附盲区声明）：`## 依赖矩阵` 小节不存在或表格式
//! 不合法时整体沉默——格式严格才能享受校验（与 L-5 同设计）。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::entry::RawEntry;
use crate::index::{build_link_lookups, doc_code_from_parts, resolve_target};
use crate::parser::is_external_url;
use crate::validator::{Severity, ValidationIssue};

/// 剥掉围栏代码块后的文本（围栏内行替换为空串，行号保持不变）。
fn strip_fenced(text: &str) -> Vec<&str> {
    let mut out = Vec::with_capacity(text.lines().count());
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            out.push("");
            continue;
        }
        out.push(if in_fence { "" } else { line.as_ref() });
    }
    out
}

/// 找到 `## <heading>` 小节的行区间 [start, end)（含空行，止于下一个
/// 一级/二级标题）。找不到小节 → None；找到但**只可能是精确标题文本**。
fn section_slice<'a>(lines: &[&'a str], heading: &str) -> Option<(usize, usize)> {
    let target = format!("## {heading}");
    let mut start = None;
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == target {
            start = Some(i + 1);
        } else if start.is_some() && (trimmed.starts_with("# ") || trimmed.starts_with("## ")) {
            end = i;
            break;
        }
    }
    start.map(|s| (s, end))
}

/// 依赖矩阵表头列名要求。
const MATRIX_HEADING: &str = "依赖矩阵";
const RELATED_HEADING: &str = "相关文档";

/// 解析 README 的依赖矩阵表。返回 [(代号, 依赖代号列表, 被依赖代号列表)]。
/// 无小节 / 无合法表头 → None（规则整体沉默）。
fn parse_matrix(lines: &[&str]) -> Option<Vec<(String, Vec<String>, Vec<String>)>> {
    let (start, end) = section_slice(lines, MATRIX_HEADING)?;
    let mut doc_col = None;
    let mut dep_col = None;
    let mut rev_col = None;
    let mut rows = Vec::new();
    for line in &lines[start..end] {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<String> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        if cells.len() < 3 {
            continue;
        }
        if cells.iter().all(|c| {
            let t = c.replace(['-', ' ', ':'], "");
            t.is_empty()
        }) {
            continue; // 分隔行
        }
        if doc_col.is_none() {
            for (j, c) in cells.iter().enumerate() {
                if c.contains("被依赖") {
                    rev_col = Some(j);
                } else if c.contains("依赖") {
                    dep_col = Some(j);
                } else if c.contains("文档") {
                    doc_col = Some(j);
                }
            }
            match (doc_col, dep_col, rev_col) {
                (Some(_), Some(_), Some(_)) => continue,
                _ => return None, // 表头不合法 → 沉默
            }
        }
        let split_codes = |cell: &str| -> Vec<String> {
            cell.split(['、', ',', ';', '，', '；'])
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .collect()
        };
        let dc = doc_col.unwrap_or(0);
        let pc = dep_col.unwrap_or(1);
        let rc = rev_col.unwrap_or(2);
        if dc >= cells.len() || pc >= cells.len() || rc >= cells.len() {
            continue;
        }
        rows.push((cells[dc].clone(), split_codes(&cells[pc]), split_codes(&cells[rc])));
    }
    if doc_col.is_none() {
        return None;
    }
    Some(rows)
}

/// 条目的文档代号：frontmatter `文档代号` 优先，路径派生兜底（与
/// index::build_link_lookups 的 by_doc_code 同口径）。
fn entry_doc_code(e: &RawEntry, kb_root: &Path) -> Option<String> {
    if let Some(c) = &e.doc_code {
        return Some(c.clone());
    }
    let dir_seg = e.dir.as_deref()?.split('/').next()?.to_string();
    let stem = e.path.file_stem()?.to_string_lossy().to_string();
    doc_code_from_parts(&dir_seg, &stem)
}

/// 该文档 `## 相关文档` 小节里链接目标条目的代号集合。
fn related_codes(
    entry: &RawEntry,
    content: &str,
    lookups: &crate::index::LinkLookups,
    code_by_id: &HashMap<String, String>,
) -> HashSet<String> {
    let mut codes = HashSet::new();
    let lines = strip_fenced(content);
    let Some((start, end)) = section_slice(&lines, RELATED_HEADING) else {
        return codes;
    };
    // 相关文档小节里的相对链接：从原文本对应区间取（strip 后行号一致，
    // 但内容被替换过——链接解析用原文本行）。
    let raw_lines: Vec<&str> = content.lines().collect();
    for (i, stripped) in lines.iter().enumerate().take(end).skip(start) {
        if stripped.trim().is_empty() {
            continue;
        }
        let raw = raw_lines.get(i).copied().unwrap_or("");
        let entry_of_line = crate::parser::links::extract_markdown_links(raw);
        for link in entry_of_line {
            if is_external_url(&link.url) {
                continue;
            }
            let cleaned = link.url.split('#').next().unwrap_or(&link.url);
            let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
            if crate::index::is_attachment_url(cleaned) {
                continue;
            }
            if let Some(target_id) = resolve_target(cleaned, entry.dir.as_deref(), lookups) {
                if let Some(code) = code_by_id.get(&target_id) {
                    codes.insert(code.clone());
                }
            }
        }
    }
    codes
}

/// W9 = L7：文件 stem 重复警告（README 本就不是条目，天然不在 entries 里）。
pub fn check_stem_uniqueness(entries: &[RawEntry]) -> Vec<ValidationIssue> {
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    for e in entries {
        let Some(stem) = e.path.file_stem().map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        match seen.get(&stem) {
            Some(first) => out.push(ValidationIssue {
                entry_id: e.id.clone(),
                rule_id: "W9".into(),
                severity: Severity::Warning,
                message: format!(
                    "文件 stem 重复：{stem}（与 {first} 同名，链接解析兜底会二义）"
                ),
                file_line: None,
                ignored: false,
            }),
            None => {
                seen.insert(stem, e.id.clone());
            }
        }
    }
    out
}

/// W10/W11 = L8/L9：依赖矩阵一致性（条件规则，README 无 `## 依赖矩阵`
/// 或表格式不合法 → 沉默）。
pub fn check_dependency_matrix(entries: &[RawEntry], kb_root: &Path) -> Vec<ValidationIssue> {
    let Ok(readme_text) = std::fs::read_to_string(kb_root.join("README.md")) else {
        return vec![];
    };
    let readme_lines = strip_fenced(&readme_text);
    let Some(rows) = parse_matrix(&readme_lines) else {
        return vec![];
    };

    let lookups = build_link_lookups(entries, kb_root);
    let mut code_by_id: HashMap<String, String> = HashMap::new();
    let mut id_by_code: HashMap<String, String> = HashMap::new();
    let mut content_by_id: HashMap<String, String> = HashMap::new();
    for e in entries {
        if let Some(code) = entry_doc_code(e, kb_root) {
            code_by_id.entry(e.id.clone()).or_insert_with(|| code.clone());
            id_by_code.entry(code).or_insert_with(|| e.id.clone());
        }
        let content = std::fs::read_to_string(&e.content_path).unwrap_or_default();
        content_by_id.insert(e.id.clone(), content);
    }

    let mut related_cache: HashMap<String, HashSet<String>> = HashMap::new();
    let mut related_of = |id: &str| -> HashSet<String> {
        if let Some(hit) = related_cache.get(id) {
            return hit.clone();
        }
        let codes = content_by_id
            .get(id)
            .map(|c| {
                entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| related_codes(e, c, &lookups, &code_by_id))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        related_cache.insert(id.to_string(), codes.clone());
        codes
    };

    let mut out = Vec::new();

    // W10：依赖 ⊆ 相关文档
    for (code, deps, _rev) in &rows {
        let Some(id) = id_by_code.get(code) else {
            out.push(ValidationIssue {
                entry_id: "README".into(),
                rule_id: "W10".into(),
                severity: Severity::Warning,
                message: format!("依赖矩阵行的代号 {code} 找不到对应文档"),
                file_line: None,
                ignored: false,
            });
            continue;
        };
        let related = related_of(id);
        for dep in deps {
            if !related.contains(dep) {
                out.push(ValidationIssue {
                    entry_id: id.clone(),
                    rule_id: "W10".into(),
                    severity: Severity::Warning,
                    message: format!(
                        "依赖矩阵声明依赖 {dep}，但相关文档小节未链接它（依赖 ⊆ 相关文档）"
                    ),
                    file_line: None,
                    ignored: false,
                });
            }
        }
    }

    // W11：依赖/被依赖互惠
    let mut deps_map: HashMap<String, HashSet<String>> = HashMap::new();
    let mut rev_map: HashMap<String, HashSet<String>> = HashMap::new();
    for (code, deps, rev) in &rows {
        deps_map.entry(code.clone()).or_default().extend(deps.iter().cloned());
        rev_map.entry(code.clone()).or_default().extend(rev.iter().cloned());
    }
    for (code, deps) in &deps_map {
        for dep in deps {
            if !rev_map.get(dep).is_some_and(|r| r.contains(code)) {
                out.push(ValidationIssue {
                    entry_id: "README".into(),
                    rule_id: "W11".into(),
                    severity: Severity::Warning,
                    message: format!(
                        "依赖矩阵不互惠：{code} 依赖 {dep}，但 {dep} 的被依赖列没有 {code}"
                    ),
                    file_line: None,
                    ignored: false,
                });
            }
        }
    }
    for (code, revs) in &rev_map {
        for r in revs {
            if !deps_map.get(r).is_some_and(|d| d.contains(code)) {
                out.push(ValidationIssue {
                    entry_id: "README".into(),
                    rule_id: "W11".into(),
                    severity: Severity::Warning,
                    message: format!(
                        "依赖矩阵不互惠：{code} 被依赖含 {r}，但 {r} 的依赖列没有 {code}"
                    ),
                    file_line: None,
                    ignored: false,
                });
            }
        }
    }
    out
}

/// 跑全部 KB 级规则。validate_kb 在 per-entry 规则之后调用。
pub fn check_all(entries: &[RawEntry], kb_root: &Path) -> Vec<ValidationIssue> {
    let mut issues = check_stem_uniqueness(entries);
    issues.extend(check_dependency_matrix(entries, kb_root));
    issues
}
