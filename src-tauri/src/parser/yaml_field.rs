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
pub fn extract_yaml_frontmatter(content: &str) -> Option<&str> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }

    let end_idx = lines[1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + 1)?;

    Some(&content[..lines[..=end_idx].join("\n").len()])
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
}
