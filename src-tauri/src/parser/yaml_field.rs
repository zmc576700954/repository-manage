use serde::{de::DeserializeOwned, Deserialize, Serialize};

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
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// frontmatter 中 rm 未建模的键（保序），随条目索引进 DB 并透出给 UI。
/// kaiwu 式知识库的四键（优先级 / 状态 / 最后更新 / 下次 review）从这里原样透出；
/// 批次 1 的 frontmatter 保序写入器将以它为「不得丢失的已知集合」。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraMetaEntry {
    pub key: String,
    pub value: serde_json::Value,
}

/// rm 建模过的 frontmatter 键。这些键进 `YamlFields`，不进旁路表。
const KNOWN_YAML_KEYS: [&str; 7] = ["color", "shape", "pinned", "order", "icon", "preview", "hidden"];

/// 一次解析 frontmatter：已知键进 `YamlFields`，其余键原样进旁路表（保持键序）。
/// serde_yaml 0.9 的 `Mapping` 由 IndexMap 背书，插入序即文档键序。
/// 非映射形态（列表 / 标量）与非法 YAML 返回 Err —— 与旧 `parse_yaml_fields`
/// 的「整体失败即 None」口径一致。
///
/// 已知键类型不符时按缺省处理（不使整个 frontmatter 失效）：
/// 旧实现对任一键类型错误会丢弃全部字段，这里放宽为逐键容错。
pub fn parse_yaml_fields_with_extras(
    yaml: &str,
) -> Result<(YamlFields, Vec<ExtraMetaEntry>), serde_yaml::Error> {
    let mapping: serde_yaml::Mapping = serde_yaml::from_str(yaml)?;
    let mut fields = YamlFields::default();
    let mut extras: Vec<ExtraMetaEntry> = Vec::with_capacity(mapping.len());
    for (k, v) in &mapping {
        let key = k.as_str().map(String::from).unwrap_or_else(|| {
            // 非字符串 YAML 键（罕见）：序列化成文本作键，仅用于展示透传。
            serde_yaml::to_string(k)
                .unwrap_or_default()
                .trim()
                .to_string()
        });
        if KNOWN_YAML_KEYS.contains(&key.as_str()) {
            match key.as_str() {
                "color" => fields.color = from_value_ok(&v),
                "shape" => fields.shape = from_value_ok(&v),
                "pinned" => fields.pinned = from_value_ok(&v),
                "order" => fields.order = from_value_ok(&v),
                "icon" => fields.icon = from_value_ok(&v),
                "preview" => fields.preview = from_value_ok(&v),
                "hidden" => fields.hidden = from_value_ok(&v),
                _ => unreachable!("KNOWN_YAML_KEYS 与分支不一致"),
            }
        } else {
            extras.push(ExtraMetaEntry {
                key,
                value: serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            });
        }
    }
    Ok((fields, extras))
}

/// 单键反序列化失败返回 None（缺省），不让整个 frontmatter 报废。
fn from_value_ok<T: DeserializeOwned>(v: &serde_yaml::Value) -> Option<T> {
    serde_yaml::from_value(v.clone()).ok()
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

    #[test]
    fn parses_icon_field() {
        let yaml = "icon: \"📌\"\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.icon, Some("📌".to_string()));
    }

    #[test]
    fn parses_preview_field() {
        let yaml = "preview: full\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.preview, Some("full".to_string()));
    }

    #[test]
    fn parses_hidden_field() {
        let yaml = "hidden: true\n";
        let parsed = parse_yaml_fields(yaml).unwrap();
        assert_eq!(parsed.hidden, Some(true));
    }
}
