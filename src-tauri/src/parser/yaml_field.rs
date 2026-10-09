use serde::{Deserialize, Serialize};

/// YAML frontmatter 中的高级配置字段
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YamlFields {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub order: Option<i32>,
}

/// 从 Markdown 文件开头提取 --- 包裹的 YAML 块，返回解析结果。
/// 没有 YAML 块时返回 None。
/// 跳过开头的非 --- 行（如 HTML 注释），找到第一个 --- 作为起始。
pub fn extract_yaml_frontmatter(content: &str) -> Option<&str> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // 找到第一个 --- 的位置
    let start_idx = lines.iter().position(|l| l.trim() == "---")?;

    // 从 start_idx+1 开始找结束 ---
    let end_idx = lines[start_idx + 1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + start_idx + 1)?;

    // 计算 start_idx 对应的字节偏移：前 start_idx 行的总字节数 + 间隔的换行符
    let mut byte_start = 0;
    for line in &lines[..start_idx] {
        byte_start += line.len() + 1; // +1 for \n
    }

    // 计算到 end_idx 行末尾的字节偏移（含行间换行符）
    let mut byte_end = byte_start;
    for i in start_idx..=end_idx {
        byte_end += lines[i].len();
        if i < end_idx {
            byte_end += 1; // 行间换行符
        }
    }

    Some(&content[byte_start..byte_end])
}

/// 解析 YAML 字符串为 YamlFields
pub fn parse_yaml_fields(yaml: &str) -> Result<YamlFields, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_yaml_frontmatter() {
        let content = r##"---
color: "#5B8DEF"
shape: rect
---

# Content
"##;
        let yaml = extract_yaml_frontmatter(content).unwrap();
        assert!(yaml.starts_with("---"));
        assert!(yaml.contains("color"));
    }

    #[test]
    fn returns_none_when_no_frontmatter() {
        let content = "# Just a heading\n\nSome content.";
        assert!(extract_yaml_frontmatter(content).is_none());
    }

    #[test]
    fn parses_simple_yaml() {
        let yaml = "color: \"#FF0000\"\nshape: rect\npinned: true\norder: 5\n";
        let parsed = parse_yaml_fields(yaml).unwrap();

        assert_eq!(parsed.color, Some("#FF0000".to_string()));
        assert_eq!(parsed.shape, Some("rect".to_string()));
        assert_eq!(parsed.pinned, Some(true));
        assert_eq!(parsed.order, Some(5));
    }

    #[test]
    fn parses_partial_yaml() {
        let yaml = "color: \"#FF0000\"\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.color, Some("#FF0000".to_string()));
        assert_eq!(parsed.shape, None);
    }

    #[test]
    fn returns_error_on_invalid_yaml() {
        let yaml = "color: : invalid";
        assert!(parse_yaml_fields(yaml).is_err());
    }

    #[test]
    fn extracts_frontmatter_after_html_comments() {
        // 真实场景：HTML 注释在前，YAML 在后
        let content = r##"<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks -->

---
color: "#5B8DEF"
shape: rect
---

# Content
"##;
        let yaml = extract_yaml_frontmatter(content).unwrap();
        assert!(yaml.contains("color"));
        assert!(yaml.contains("shape"));
    }

    #[test]
    fn parses_negative_order() {
        let yaml = "order: -3\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.order, Some(-3));
    }

    #[test]
    fn parses_pinned_false() {
        let yaml = "pinned: false\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.pinned, Some(false));
    }

    #[test]
    fn returns_none_for_unclosed_frontmatter() {
        let content = "---\ncolor: red\n";
        assert!(extract_yaml_frontmatter(content).is_none());
    }
}
