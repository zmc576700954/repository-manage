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
                    let trimmed_caption = caption.trim();
                    let caption = if trimmed_caption.is_empty() {
                        None
                    } else {
                        Some(trimmed_caption.to_string())
                    };
                    attachments.push((path.trim().to_string(), caption));
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

    #[test]
    fn extracts_multiple_attachments() {
        let content = r##"![[a.png|甲]] and ![[b.pdf]] and ![[c.gif|丙]]"##;
        let atts = extract_attachments(content);
        assert_eq!(atts.len(), 3);
        assert_eq!(atts[0], ("a.png".to_string(), Some("甲".to_string())));
        assert_eq!(atts[1], ("b.pdf".to_string(), None));
        assert_eq!(atts[2], ("c.gif".to_string(), Some("丙".to_string())));
    }

    #[test]
    fn trims_whitespace_in_path_and_caption() {
        let content = "![[  spaced.png  |  caption  ]]";
        let atts = extract_attachments(content);
        assert_eq!(atts, vec![("spaced.png".to_string(), Some("caption".to_string()))]);
    }

    #[test]
    fn empty_caption_becomes_none() {
        // ![[path|]] 应该视为无 caption
        let content = "![[image.png|]]";
        let atts = extract_attachments(content);
        assert_eq!(atts, vec![("image.png".to_string(), None)]);
    }

    #[test]
    fn classify_handles_uppercase_extensions() {
        assert_eq!(classify_attachment("PHOTO.PNG"), "image");
        assert_eq!(classify_attachment("Doc.PDF"), "document");
    }

    #[test]
    fn classify_handles_files_with_multiple_dots() {
        assert_eq!(classify_attachment("archive.tar.gz"), "other");
        assert_eq!(classify_attachment("my.photo.png"), "image");
    }
}
