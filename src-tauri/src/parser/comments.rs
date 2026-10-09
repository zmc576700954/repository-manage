use std::collections::HashMap;

/// 解析 content.md 文件开头的 HTML 注释行，提取 @synapse-* 元数据。
/// 返回值为 metadata 映射，未识别的注释会被忽略。
pub fn parse_metadata_comments(content: &str) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("<!--") || !line.ends_with("-->") {
            continue;
        }

        let inner = &line[4..line.len() - 3].trim();
        let Some((key, value)) = inner.split_once(':') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        if key.starts_with("@synapse-") {
            let field = key.trim_start_matches("@synapse-");
            metadata.insert(field.to_string(), value.to_string());
        }
    }

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_id_and_title() {
        let content = r#"<!-- @synapse-id: react-hooks -->
<!-- @synapse-title: React Hooks 深入理解 -->

# Content here
"#;

        let metadata = parse_metadata_comments(content);

        assert_eq!(metadata.get("id").map(|s| s.as_str()), Some("react-hooks"));
        assert_eq!(
            metadata.get("title").map(|s| s.as_str()),
            Some("React Hooks 深入理解")
        );
    }

    #[test]
    fn parses_tags_with_comma_separator() {
        let content = r#"<!-- @synapse-id: foo -->
<!-- @synapse-title: Foo -->
<!-- @synapse-tags: react, frontend, web -->
"#;

        let metadata = parse_metadata_comments(content);

        assert_eq!(
            metadata.get("tags").map(|s| s.as_str()),
            Some("react, frontend, web")
        );
    }

    #[test]
    fn ignores_non_synapse_comments() {
        let content = r#"<!-- regular HTML comment -->
<!-- @synapse-id: bar -->

body
"#;

        let metadata = parse_metadata_comments(content);

        assert_eq!(metadata.len(), 1);
        assert!(metadata.contains_key("id"));
    }

    #[test]
    fn handles_comments_with_no_value() {
        let content = "<!-- @synapse-id: -->";

        let metadata = parse_metadata_comments(content);

        assert_eq!(metadata.get("id").map(|s| s.as_str()), Some(""));
    }
}
