/// 从 Markdown 提取 ![[path|caption]] 形式的附件引用。
/// 返回 (path, caption) 列表。
pub fn extract_attachments(content: &str) -> Vec<(String, Option<String>)> {
    let mut attachments = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 2 < bytes.len()
            && bytes[i] == b'!'
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
        {
            let mut j = i + 3;
            while j + 1 < bytes.len() && !(bytes[j] == b']' && bytes[j + 1] == b']') {
                j += 1;
            }

            if j + 1 < bytes.len() {
                let inner = &content[i + 3..j];
                if let Some((path, caption)) = inner.split_once('|') {
                    attachments.push((path.trim().to_string(), Some(caption.trim().to_string())));
                } else {
                    attachments.push((inner.trim().to_string(), None));
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }

    attachments
}

/// 根据文件扩展名判断附件类型
pub fn classify_attachment(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();

    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => "image",
        "pdf" | "doc" | "docx" | "txt" | "md" | "epub" => "document",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_attachment_with_caption() {
        let content = "Image: ![[./diagrams/flow.png|流程图]]";
        let atts = extract_attachments(content);
        assert_eq!(atts, vec![("./diagrams/flow.png".to_string(), Some("流程图".to_string()))]);
    }

    #[test]
    fn extracts_attachment_without_caption() {
        let content = "Plain: ![[image.png]]";
        let atts = extract_attachments(content);
        assert_eq!(atts, vec![("image.png".to_string(), None)]);
    }

    #[test]
    fn classifies_image_extensions() {
        assert_eq!(classify_attachment("photo.png"), "image");
        assert_eq!(classify_attachment("photo.JPG"), "image");
        assert_eq!(classify_attachment("photo.svg"), "image");
    }

    #[test]
    fn classifies_document_extensions() {
        assert_eq!(classify_attachment("doc.pdf"), "document");
        assert_eq!(classify_attachment("notes.md"), "document");
    }

    #[test]
    fn classifies_unknown_extensions() {
        assert_eq!(classify_attachment("file.xyz"), "other");
        assert_eq!(classify_attachment("noext"), "other");
    }

    #[test]
    fn ignores_non_attachment_double_brackets() {
        let content = "Regular link [[entry]] here.";
        let atts = extract_attachments(content);
        assert!(atts.is_empty());
    }
}
