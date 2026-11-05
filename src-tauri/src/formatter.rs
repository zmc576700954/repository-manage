//! 确定性的“无损排版”实现。
//!
//! 设计约束（产品侧已确认）：
//! - **不改语义**：不压缩空行、不增删行、不改标题级别、不改列表层级；
//! - 永远跳过 YAML front matter、围栏代码块、表格与 `@synapse-*` 元数据注释；
//! - 结果必须可复现：同一输入恒得同一输出，且对输出再跑一次为空操作。
//!
//! 允许的修改仅限排版层面：
//! 1. 去掉行尾空白（受保护区域除外）；
//! 2. 标题 `#` 与文本之间统一成一个空格（仅当原本就有空白，避免把
//!    `#hashtag` 这种非标题行误改成标题）；
//! 3. 列表标记与内容之间统一成一个空格，缩进统一为每层 2 个空格
//!    （层级由原缩进宽度推导，不改变嵌套关系）；
//! 4. 中文与英文字母/数字相邻时补一个空格（跳过行内代码、`[[wiki]]`、
//!    链接目标、HTML 标签等受保护片段）。
//!
//! 未做多空行压缩：那会改变空行数量，属于产品明确禁止的语义修改。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOutcome {
    pub formatted: String,
    pub changed: bool,
    /// 发生变化的行数（用于预览界面提示，不是 diff 行数）。
    pub changed_lines: usize,
}

/// 对完整文档做一次排版，保留原始换行符（含 CRLF/LF 混合）。
pub fn format_content(input: &str) -> FormatOutcome {
    let mut out = String::with_capacity(input.len());
    let mut changed_lines = 0usize;
    let mut line_index = 0usize;

    let mut in_frontmatter = false;
    let mut code_fence: Option<&'static str> = None;

    for raw in input.split_inclusive('\n') {
        let (body, ending) = split_line_ending(raw);

        let trimmed_start = body.trim_start();

        // YAML front matter 只认文件第一行的 `---`；内部内容原样保留。
        if line_index == 0 && trimmed_start == "---" {
            in_frontmatter = true;
            out.push_str(body);
            out.push_str(ending);
            line_index += 1;
            continue;
        }
        if in_frontmatter {
            if trimmed_start == "---" || trimmed_start == "..." {
                in_frontmatter = false;
            }
            out.push_str(body);
            out.push_str(ending);
            line_index += 1;
            continue;
        }

        // 围栏代码块内部（含围栏行）原样保留。
        if let Some(fence) = code_fence {
            if trimmed_start.starts_with(fence) {
                code_fence = None;
            }
            out.push_str(body);
            out.push_str(ending);
            line_index += 1;
            continue;
        }
        if trimmed_start.starts_with("```") {
            code_fence = Some("```");
            out.push_str(body);
            out.push_str(ending);
            line_index += 1;
            continue;
        }
        if trimmed_start.starts_with("~~~") {
            code_fence = Some("~~~");
            out.push_str(body);
            out.push_str(ending);
            line_index += 1;
            continue;
        }

        // 表格行与 `@synapse-*` 注释行整行跳过。
        if trimmed_start.starts_with('|')
            || trimmed_start.starts_with("<!-- @synapse-")
        {
            out.push_str(body);
            out.push_str(ending);
            line_index += 1;
            continue;
        }

        let formatted = format_plain_line(body);
        if formatted != body {
            changed_lines += 1;
        }
        out.push_str(&formatted);
        out.push_str(ending);
        line_index += 1;
    }

    let changed = out != input;
    FormatOutcome {
        formatted: out,
        changed,
        changed_lines,
    }
}

fn split_line_ending(raw: &str) -> (&str, &str) {
    if let Some(body) = raw.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = raw.strip_suffix('\n') {
        (body, "\n")
    } else {
        (raw, "")
    }
}

fn format_plain_line(line: &str) -> String {
    if let Some(heading) = normalize_heading(line) {
        return heading;
    }
    if let Some(list_item) = normalize_list_item(line) {
        return list_item;
    }
    let body = line.trim_end();
    space_cjk_latin(body)
}

/// 标题：`#{1,6}` 后有空白时，把空白统一成一个空格并去掉行尾空白。
/// 级别本身不变；`#hashtag` 这种没有空白的行也不会被当成标题。
fn normalize_heading(line: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent_len..];
    if !rest.starts_with('#') {
        return None;
    }
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after_hashes = &rest[hashes..];
    if after_hashes.is_empty() {
        return Some(format!("{}{}", &line[..indent_len], "#".repeat(hashes)));
    }
    if !after_hashes.starts_with([' ', '\t']) {
        return None;
    }
    let indent = &line[..indent_len];
    let text = after_hashes.trim();
    if text.is_empty() {
        return Some(format!("{indent}{}", "#".repeat(hashes)));
    }
    let text = space_cjk_latin(text);
    Some(format!("{indent}{} {text}", "#".repeat(hashes)))
}

/// 列表项：`[-*+]` 或 `\d{1,9}[.)]` 后必须有空白。
/// 缩进按“原宽度 ÷ 2”折算层级，再统一写成每层 2 个空格；
/// 标记后的空白统一成一个空格，标记本身（含有序编号）保持不变。
fn normalize_list_item(line: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent_raw = &line[..indent_len];
    let rest = &line[indent_len..];

    let (marker_text, content) = split_list_marker(rest)?;
    let width = indent_raw
        .chars()
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum::<usize>();
    let level = width / 2;
    let indent = "  ".repeat(level);
    let text = space_cjk_latin(content.trim_end());
    Some(format!("{indent}{marker_text} {text}"))
}

fn split_list_marker(rest: &str) -> Option<(String, &str)> {
    let first = rest.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let after = &rest[first.len_utf8()..];
        if !after.starts_with([' ', '\t']) {
            return None;
        }
        return Some((first.to_string(), after.trim_start()));
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 9 {
        return None;
    }
    let after_digits = &rest[digits.len()..];
    let sep = after_digits.chars().next()?;
    if !matches!(sep, '.' | ')') {
        return None;
    }
    let after_sep = &after_digits[sep.len_utf8()..];
    if !after_sep.starts_with([' ', '\t']) {
        return None;
    }
    Some((format!("{digits}{sep}"), after_sep.trim_start()))
}

/// 中英文/数字之间补空格。受保护片段（行内代码、`[[wikilink]]`、
/// markdown 链接目标、HTML/autolink 尖括号内容）原样复制，绝不改动。
fn space_cjk_latin(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut prev: Option<char> = None;
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '`' {
            let end = take_backtick_span(&chars, i);
            push_verbatim(&chars, i, end, &mut out, &mut prev);
            i = end;
            continue;
        }
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == '[' {
            let end = take_until(&chars, i + 2, "]]");
            push_verbatim(&chars, i, end, &mut out, &mut prev);
            i = end;
            continue;
        }
        if chars[i] == ']' && i + 1 < chars.len() && chars[i + 1] == '(' {
            let end = take_until(&chars, i + 2, ")");
            push_verbatim(&chars, i, end, &mut out, &mut prev);
            i = end;
            continue;
        }
        if chars[i] == '<'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic()
                || chars[i + 1] == '/'
                || chars[i + 1] == '!')
        {
            let end = take_until(&chars, i + 1, ">");
            push_verbatim(&chars, i, end, &mut out, &mut prev);
            i = end;
            continue;
        }

        let c = chars[i];
        if let Some(p) = prev {
            if !p.is_whitespace()
                && !c.is_whitespace()
                && ((is_cjk(p) && c.is_ascii_alphanumeric())
                    || (p.is_ascii_alphanumeric() && is_cjk(c)))
            {
                out.push(' ');
            }
        }
        out.push(c);
        prev = Some(c);
        i += 1;
    }
    out
}

fn push_verbatim(
    chars: &[char],
    start: usize,
    end: usize,
    out: &mut String,
    prev: &mut Option<char>,
) {
    for c in &chars[start..end.min(chars.len())] {
        out.push(*c);
        *prev = Some(*c);
    }
}

/// 行内代码：返回配对反引号片段之后的下标。
fn take_backtick_span(chars: &[char], start: usize) -> usize {
    let run = chars[start..].iter().take_while(|c| **c == '`').count();
    let needle: String = "`".repeat(run);
    // 从 run 之后找同样长度的反引号串。
    let mut i = start + run;
    while i < chars.len() {
        if chars[i] == '`' {
            let close_run = chars[i..].iter().take_while(|c| **c == '`').count();
            if close_run == run {
                return i + run;
            }
            i += close_run;
        } else {
            i += 1;
        }
    }
    chars.len()
}

/// 找到 `needle` 结束的位置（含 needle）；找不到时返回文本末尾。
fn take_until(chars: &[char], from: usize, needle: &str) -> usize {
    let needle_chars: Vec<char> = needle.chars().collect();
    let mut i = from;
    while i + needle_chars.len() <= chars.len() {
        if chars[i..i + needle_chars.len()] == needle_chars[..] {
            return i + needle_chars.len();
        }
        i += 1;
    }
    chars.len()
}

fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4DBF   // CJK 扩展 A
        | 0x4E00..=0x9FFF // CJK 基本区
        | 0xF900..=0xFAFF // CJK 兼容
        | 0x3040..=0x30FF // 日文假名
        | 0xAC00..=0xD7AF // 韩文
        | 0x20000..=0x2FA1F // CJK 扩展 B+
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(input: &str) -> FormatOutcome {
        format_content(input)
    }

    #[test]
    fn strips_trailing_whitespace_outside_protected_regions() {
        let out = fmt("# Title  \n\n正文   \n");
        assert!(out.formatted.starts_with("# Title\n"));
        assert!(out.formatted.ends_with("正文\n"));
        assert!(out.changed);
    }

    #[test]
    fn leaves_frontmatter_untouched() {
        let input = "---\ntitle: 测试   \n---\n\n正文   \n";
        let out = fmt(input);
        assert_eq!(
            out.formatted,
            "---\ntitle: 测试   \n---\n\n正文\n",
            "front matter 的任何空白都不能动"
        );
    }

    #[test]
    fn leaves_code_blocks_untouched() {
        let input = "```rust\nlet x = 1;   \n中文English\n```\n\n中文English   \n";
        let out = fmt(input);
        assert!(out.formatted.contains("let x = 1;   "));
        assert!(out.formatted.contains("中文English\n```"));
        assert!(out.formatted.ends_with("中文 English\n"));
    }

    #[test]
    fn leaves_tables_untouched() {
        let input = "| 中文 | English |\n| --- | --- |\n| 值 | value |\n";
        let out = fmt(input);
        assert_eq!(out.formatted, input, "表格行必须完全跳过");
        assert!(!out.changed);
        assert_eq!(out.changed_lines, 0);
    }

    #[test]
    fn skips_synapse_metadata_comments() {
        let input = "<!-- @synapse-title: 中文Title -->\n\n正文\n";
        let out = fmt(input);
        assert!(out.formatted.contains("<!-- @synapse-title: 中文Title -->"));
    }

    #[test]
    fn normalizes_heading_spacing_but_not_level() {
        let out = fmt("##   标题  \n#hashtag\n");
        assert_eq!(out.formatted, "## 标题\n#hashtag\n");
    }

    #[test]
    fn normalizes_list_markers_and_indent() {
        let out = fmt("-   item\n    -   nested\n\t- tabbed\n1.   first\n");
        assert_eq!(
            out.formatted,
            "- item\n    - nested\n  - tabbed\n1. first\n"
        );
    }

    #[test]
    fn inserts_space_between_cjk_and_latin() {
        let out = fmt("使用React框架，版本2.\nReact中文\n");
        assert_eq!(out.formatted, "使用 React 框架，版本 2.\nReact 中文\n");
    }

    #[test]
    fn does_not_touch_wikilinks_inline_code_or_link_targets() {
        let input = "见[[note-中文]]和`printf中文`，参考[链接](https://example.com/a中文)。\n";
        let out = fmt(input);
        assert_eq!(
            out.formatted, input,
            "受保护片段内部（wikilink/行内代码/链接目标）必须逐字节保持"
        );
    }

    #[test]
    fn output_is_idempotent() {
        let input = "#  Title  \n\n-   item\n\n使用React框架   \n";
        let first = fmt(input);
        let second = fmt(&first.formatted);
        assert!(!second.changed, "排版结果必须稳定：第二次不应再有变化");
    }

    #[test]
    fn preserves_crlf_and_mixed_line_endings() {
        let input = "第一行  \r\n第二行\n第三行  \r\n";
        let out = fmt(input);
        assert_eq!(out.formatted, "第一行\r\n第二行\n第三行\r\n");
    }

    #[test]
    fn does_not_compress_blank_lines() {
        let input = "a\n\n\n\nb\n";
        let out = fmt(input);
        assert_eq!(out.formatted, input, "多空行压缩属于语义修改，明确不做");
    }
}
