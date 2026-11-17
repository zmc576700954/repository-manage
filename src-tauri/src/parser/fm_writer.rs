//! YAML frontmatter 保序写入器（kaiwu × rm 合并批次 1，Q2.a / Q6.a）。
//!
//! frontmatter 是元数据的唯一真源；`@synapse-*` HTML 注释降为只读兼容。
//! 本模块对文件做「读 → 改 → 写」，除 frontmatter 区块内的目标键外，
//! 其余字节（未知键、键序、frontmatter 内注释、正文、行尾）一律原样保留。
//!
//! 规则（与解析器 `extract_yaml_frontmatter` 的宽容口径一致）：
//! - 文件里存在第一个 `---` … `---` 区块 → 就地编辑该区块（不移动位置，
//!   因此 kaiwu 的「frontmatter 必须在首行」约束不会被破坏）；
//! - 没有 frontmatter 区块 → 在文件最顶部插入新区块（首行位置 = 文件第 1 行）；
//! - 已知键就地替换（含多行值整体替换），缺失键追加到区块末尾（关闭 `---` 之前）；
//! - 值序列化：标量交给 serde_yaml 决定引号风格；列表写成 CSV 标量（M1 安全，单键单行）。

/// 要写入 frontmatter 的值。标量与字符串列表覆盖 rm 与 kaiwu 的全部用键。
#[derive(Debug, Clone, PartialEq)]
pub enum FmValue {
    Str(String),
    Bool(bool),
    Int(i64),
    List(Vec<String>),
}

/// 一次 frontmatter 编辑：`set`（保序写入，同键后者覆盖前者）与 `remove`。
#[derive(Debug, Clone, Default)]
pub struct FrontmatterEdit {
    pub set: Vec<(String, FmValue)>,
    pub remove: Vec<String>,
}

impl FrontmatterEdit {
    pub fn set(key: impl Into<String>, value: FmValue) -> Self {
        Self {
            set: vec![(key.into(), value)],
            remove: vec![],
        }
    }

    pub fn remove(key: impl Into<String>) -> Self {
        Self {
            set: vec![],
            remove: vec![key.into()],
        }
    }
}

/// 区块定位：frontmatter 在 `lines` 里的行区间（含首尾 `---` 行）。
fn locate_block(lines: &[&str]) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.trim() == "---")?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + start + 1)?;
    Some((start, end))
}

/// 顶层键判定：区块内非缩进、非注释、非空行，且形如 `key:` 或 `key: value`。
fn top_level_key(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    if trimmed.starts_with(' ') || trimmed.starts_with('\t') || trimmed.starts_with('#') {
        return None;
    }
    let colon = trimmed.find(':')?;
    let key = trimmed[..colon].trim();
    if key.is_empty() || key.contains(' ') || key.contains('#') {
        return None;
    }
    Some(key.to_string())
}

/// 单个 YAML 标量的文本形式（引号交给 serde_yaml 决定）。
fn scalar_text(s: &str) -> String {
    serde_yaml::to_string(s)
        .unwrap_or_else(|_| format!("{s:?}"))
        .trim_end()
        .to_string()
}

/// 把 FmValue 渲染成 frontmatter 行（可能一行，列表也保持一行 flow 风格）。
fn render_lines(key: &str, value: &FmValue) -> Vec<String> {
    match value {
        FmValue::Str(s) => vec![format!("{key}: {}", scalar_text(s))],
        FmValue::Bool(b) => vec![format!("{key}: {b}")],
        FmValue::Int(i) => vec![format!("{key}: {i}")],
        // M1 安全（check_kb：frontmatter 每行恰好一个扁平「键: 值」，不允许列表行）：
        // 列表写成 CSV 标量，解析侧 fm_scalar_string/extract_labels 兼容两种形态。
        FmValue::List(items) => {
            let rendered: Vec<String> = items.iter().map(|s| scalar_text(s)).collect();
            vec![format!("{key}: {}", rendered.join(", "))]
        }
    }
}

/// 应用编辑，返回新文件内容。
///
/// 只重写 frontmatter 区块内的目标键行区间；区块外字节保持不变。
/// 行内统一使用 `\n`（rm 写路径的既有约定）；区块外的 CRLF 原样保留。
pub fn apply_edits(content: &str, edit: &FrontmatterEdit) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let Some((start, end)) = locate_block(&lines) else {
        // 没有 frontmatter：在文件顶部插入新区块。
        let mut block = String::from("---\n");
        for (key, value) in &edit.set {
            for line in render_lines(key, value) {
                block.push_str(&line);
                block.push('\n');
            }
        }
        block.push_str("---\n");
        return format!("{block}{content}");
    };

    // 区块内顶层键 → 行区间 [key_line, 区间末行)（到下一个顶层键或关闭 --- 前）。
    let mut key_ranges: Vec<(String, usize, usize)> = Vec::new();
    {
        let mut i = start + 1;
        while i < end {
            if let Some(key) = top_level_key(lines[i]) {
                let mut j = i + 1;
                while j < end && top_level_key(lines[j]).is_none() {
                    j += 1;
                }
                key_ranges.push((key, i, j));
                i = j;
            } else {
                i += 1;
            }
        }
    }

    // 目标集合：同键多次 set → 保留首次位置、最后值。
    let mut wanted: Vec<(String, &FmValue)> = Vec::new();
    for (key, value) in &edit.set {
        if let Some(slot) = wanted.iter_mut().find(|(k, _)| k == key) {
            slot.1 = value;
        } else {
            wanted.push((key.clone(), value));
        }
    }
    let remove_set: std::collections::HashSet<&str> = edit
        .remove
        .iter()
        .map(|s| s.as_str())
        .collect();

    // 生成新区块行：先复制旧行，再对涉及的键做行区间替换 / 删除 / 追加。
    let mut block_lines: Vec<String> = lines[start..=end].iter().map(|s| s.to_string()).collect();
    // 区块行索引 → 原文件行索引的换算：block_lines[i] 对应 lines[start + i]。
    let mut ops: Vec<(usize, usize, Vec<String>)> = Vec::new(); // (old_block_start, old_block_end, new_lines)
    let mut append_at_closing: Vec<Vec<String>> = Vec::new();

    for (key, value) in &wanted {
        let existing = key_ranges.iter().find(|(k, _, _)| k == key);
        let rendered = render_lines(key, value);
        match existing {
            Some((_, fs, fe)) => {
                let bs = fs - start;
                let be = fe - start;
                ops.push((bs, be, rendered));
            }
            None => append_at_closing.push(rendered),
        }
    }
    for key in &remove_set {
        if let Some((_, fs, fe)) = key_ranges.iter().find(|(k, _, _)| k == key) {
            let bs = fs - start;
            let be = fe - start;
            ops.push((bs, be, vec![]));
        }
        // remove 的键不在区块里 → no-op。
    }
    // 同一行的多个 op 不会发生（键唯一性由 wanted 去重保证）。
    ops.sort_by_key(|(bs, _, _)| *bs);
    for (bs, be, new_lines) in ops.iter().rev() {
        block_lines.splice(*bs..*be, new_lines.iter().cloned());
    }
    // 追加键：插到关闭 --- 之前。
    if !append_at_closing.is_empty() {
        let mut insert: Vec<String> = Vec::new();
        for lines_of_key in append_at_closing {
            insert.extend(lines_of_key);
        }
        let closing_at = end - start; // block_lines 里关闭 --- 的位置
        block_lines.splice(closing_at..closing_at, insert);
    }

    // 重组：区块外的字节原样保留（含 CRLF 与首尾空白），
    // 区块内以 \n 连接替换。区块字节范围 = 首个 --- 行行首 .. 关闭 --- 行行尾。
    let line_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(lines.len() + 1);
        let mut acc = 0;
        for line in &lines {
            offsets.push(acc);
            acc += line.len() + 1; // +1 for the split-out '\n'
        }
        offsets.push(acc);
        offsets
    };
    let block_start = line_offsets[start];
    let block_end = line_offsets[end] + lines[end].len();
    let new_block = block_lines.join("\n");
    format!(
        "{}{}{}",
        &content[..block_start],
        new_block,
        &content[block_end..]
    )
}

/// 文件（文本形式）是否已有 frontmatter 区块。
pub fn has_frontmatter(content: &str) -> bool {
    let lines: Vec<&str> = content.split('\n').collect();
    locate_block(&lines).is_some()
}

/// frontmatter 区块里是否已有某个顶层键（用于修复器的 no-op 判断）。
pub fn has_key(content: &str, key: &str) -> bool {
    let lines: Vec<&str> = content.split('\n').collect();
    let Some((start, end)) = locate_block(&lines) else {
        return false;
    };
    lines[start + 1..end]
        .iter()
        .any(|l| top_level_key(l).as_deref() == Some(key))
}

/// frontmatter 区块内某顶层键的单行标量值（`key: value` 的 value 部分，
/// 去首尾空白）。只认区块内顶层键行，嵌套/注释行不误读。
/// 批次 4 起 pub：D7 冻结键比对（mcp save_content 与 GUI apply_proposal
/// 共用同一口径）。
pub fn scalar_value(content: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = content.split('\n').collect();
    let (start, end) = locate_block(&lines)?;
    for line in &lines[start + 1..end] {
        let trimmed = line.trim_end();
        if trimmed.starts_with(' ') || trimmed.starts_with('\t') || trimmed.starts_with('#') {
            continue;
        }
        if let Some(colon) = trimmed.find(':') {
            if trimmed[..colon].trim() == key {
                return Some(trimmed[colon + 1..].trim().to_string());
            }
        }
    }
    None
}
