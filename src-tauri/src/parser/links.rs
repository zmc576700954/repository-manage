/// Markdown 内联链接：[text](url) 形式。
/// 跳过 image 形式 `![text](url)` 和 reference-style `[text]: url`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    pub text: String,
    pub url: String,
}

pub fn extract_markdown_links(content: &str) -> Vec<MarkdownLink> {
    let mut out = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // 跳过 image 形式 ![
        if i > 0 && bytes[i] == b'[' && bytes[i - 1] == b'!' {
            i += 1;
            continue;
        }
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        // 找 ']' 结束 text
        let mut text_end = i + 1;
        let mut bracket_depth = 1;
        while text_end < bytes.len() && bracket_depth > 0 {
            match bytes[text_end] {
                b'[' => bracket_depth += 1,
                b']' => bracket_depth -= 1,
                _ => {}
            }
            if bracket_depth > 0 {
                text_end += 1;
            }
        }
        if bracket_depth != 0 || text_end >= bytes.len() {
            i += 1;
            continue;
        }
        // 期待 '](' 紧跟
        if text_end + 1 >= bytes.len() || bytes[text_end + 1] != b'(' {
            i = text_end + 1;
            continue;
        }
        // 找 ')' 结束 url
        let url_start = text_end + 2;
        let mut url_end = url_start;
        let mut paren_depth = 1;
        while url_end < bytes.len() && paren_depth > 0 {
            match bytes[url_end] {
                b'(' => paren_depth += 1,
                b')' => paren_depth -= 1,
                _ => {}
            }
            if paren_depth > 0 {
                url_end += 1;
            }
        }
        if paren_depth != 0 || url_end >= bytes.len() {
            i = text_end + 1;
            continue;
        }
        let text = std::str::from_utf8(&bytes[i + 1..text_end])
            .unwrap_or("")
            .trim()
            .to_string();
        let url = std::str::from_utf8(&bytes[url_start..url_end])
            .unwrap_or("")
            .trim()
            .to_string();
        if !text.is_empty() && !url.is_empty() {
            out.push(MarkdownLink { text, url });
        }
        i = url_end + 1;
    }
    out
}

pub fn is_external_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("ftp://")
        || u.starts_with("mailto:")
        || u.starts_with("//")
}

/// 从 Markdown 正文提取 [[path]] 形式的双向链接。
/// 返回链接目标路径列表（不含路径前缀装饰）。
pub fn extract_double_bracket_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // 跳过 ![[ 开头的（那是附件引用）
            let is_attachment = i > 0 && bytes[i - 1] == b'!';

            // 找到 ]] 结束位置
            let mut j = i + 2;
            while j + 1 < bytes.len() && !(bytes[j] == b']' && bytes[j + 1] == b']') {
                j += 1;
            }

            if j + 1 < bytes.len() {
                let inner = &content[i + 2..j];
                if !is_attachment {
                    let target = strip_block_ref(inner);
                    if !target.is_empty() {
                        links.push(target.to_string());
                    }
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }

    links
}

/// 显式关系声明（行级 @relation: type [[id]]）
#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    pub relation_type: String,
    pub target: String,
}

/// 从 markdown 内容中提取 @relation: type [[id]] 行级声明。
/// 仅识别 5 种默认关系类型。
pub fn extract_relations(content: &str) -> Vec<Relation> {
    const VALID_RELATION_TYPES: &[&str] = &[
        "reference",
        "derived",
        "contradicts",
        "supersedes",
        "extends",
    ];

    let mut relations = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("@relation:") else {
            continue;
        };
        let rest = rest.trim();
        let Some((rtype, target_part)) = rest.split_once(' ') else {
            continue;
        };
        let rtype = rtype.trim();
        let target_part = target_part.trim();
        let target = match target_part
            .strip_prefix("[[")
            .and_then(|s| s.strip_suffix("]]"))
        {
            Some(t) => t.to_string(),
            None => continue,
        };
        if rtype.is_empty() || !VALID_RELATION_TYPES.contains(&rtype) {
            continue;
        }
        relations.push(Relation {
            relation_type: rtype.to_string(),
            target,
        });
    }
    relations
}

/// 移除 #block-id 后缀
fn strip_block_ref(target: &str) -> &str {
    if let Some(idx) = target.find('#') {
        &target[..idx]
    } else {
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_link() {
        let content = "See [[other-entry]] for details.";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["other-entry"]);
    }

    #[test]
    fn extracts_multiple_links() {
        let content = "Links: [[a]], [[b/c]], and [[d]].";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["a", "b/c", "d"]);
    }

    #[test]
    fn strips_block_reference() {
        let content = "See [[entry#section-1]] and [[other#block]].";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["entry", "other"]);
    }

    #[test]
    fn ignores_attachment_links() {
        let content = "Image: ![[image.png|caption]] and link [[real-link]].";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["real-link"]);
    }

    #[test]
    fn handles_empty_content() {
        assert!(extract_double_bracket_links("").is_empty());
    }

    #[test]
    fn handles_unclosed_brackets() {
        let content = "Unclosed [[link here";
        let links = extract_double_bracket_links(content);
        assert!(links.is_empty());
    }

    #[test]
    fn handles_utf8_content() {
        let content = "参考 [[条目-中文]] 和 [[another]]";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["条目-中文", "another"]);
    }

    #[test]
    fn handles_links_in_code_fence() {
        // 简单实现不区分代码块，原样提取
        let content = "```\n[[code-link]]\n```\n[[real-link]]";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["code-link", "real-link"]);
    }

    #[test]
    fn strips_block_reference_with_empty_block() {
        // [[entry#]] 形式：空块引用
        let content = "See [[entry#]]";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["entry"]);
    }

    #[test]
    fn empty_link_target_is_skipped() {
        // [[]] 应该跳过，因为 target 为空
        let content = "Empty [[]] and [[real]]";
        let links = extract_double_bracket_links(content);
        assert_eq!(links, vec!["real"]);
    }

    #[test]
    fn extracts_relation_declarations() {
        let content = r#"参考文档。

@relation: derived [[source-entry]]
@relation: contradicts [[old-entry]]

正文。
"#;
        let rels = extract_relations(content);
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].relation_type, "derived");
        assert_eq!(rels[0].target, "source-entry");
        assert_eq!(rels[1].relation_type, "contradicts");
        assert_eq!(rels[1].target, "old-entry");
    }

    #[test]
    fn ignores_non_relation_declarations() {
        let content = "普通文本 @relation: derived [[x]] 但不在行首\n";
        let rels = extract_relations(content);
        assert!(rels.is_empty());
    }

    #[test]
    fn handles_invalid_declarations() {
        let content = "@relation: unknown [[x]]\n@relation: derived yyy\n";
        let rels = extract_relations(content);
        // unknown 类型不识别；无 [[]] 的视为无效
        assert_eq!(rels.len(), 0);
    }

    #[test]
    fn extracts_relation_with_block_id() {
        let content = "@relation: extends [[entry-x#section-1]]\n";
        let rels = extract_relations(content);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].target, "entry-x#section-1");
    }
}