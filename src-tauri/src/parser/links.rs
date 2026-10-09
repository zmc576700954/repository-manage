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
}