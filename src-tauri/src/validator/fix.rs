//! 修复元数据：每个 E 规则的修复能力描述

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixDescriptor {
    pub rule_id: String,
    pub kind: FixKind,
    pub risk: FixRisk,
    pub label_key: String,
    pub preview_key: Option<String>,
    pub confirm_key: Option<String>,
    pub docs_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FixKind {
    OneClick,
    Confirm,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FixRisk {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FixDescriptor must serialize to camelCase with `kind` and `risk`
    /// matching the shapes the frontend will consume.
    #[test]
    fn fix_descriptor_serializes_camel_case() {
        let desc = FixDescriptor {
            rule_id: "E1".into(),
            kind: FixKind::OneClick,
            risk: FixRisk::Low,
            label_key: "fix.e1.label".into(),
            preview_key: Some("fix.e1.preview".into()),
            confirm_key: None,
            docs_key: "fix.e1.docs".into(),
        };
        let json = serde_json::to_value(&desc).unwrap();
        assert_eq!(json["ruleId"], "E1");
        assert_eq!(json["labelKey"], "fix.e1.label");
        assert_eq!(json["previewKey"], "fix.e1.preview");
        assert_eq!(json["confirmKey"], serde_json::Value::Null);
        assert_eq!(json["docsKey"], "fix.e1.docs");
    }

    /// FixKind is a tagged enum — `type` is the tag and the value is
    /// the camelCased variant name.
    #[test]
    fn fix_kind_serializes_with_type_tag() {
        let json = serde_json::to_value(FixKind::OneClick).unwrap();
        assert_eq!(json["type"], "oneClick");

        let json = serde_json::to_value(FixKind::Confirm).unwrap();
        assert_eq!(json["type"], "confirm");

        let json = serde_json::to_value(FixKind::Manual).unwrap();
        assert_eq!(json["type"], "manual");
    }

    /// FixRisk uses lowercase strings so the frontend can map them
    /// to a CSS color tier without further translation.
    #[test]
    fn fix_risk_serializes_lowercase() {
        assert_eq!(serde_json::to_value(FixRisk::Low).unwrap(), "low");
        assert_eq!(serde_json::to_value(FixRisk::Medium).unwrap(), "medium");
        assert_eq!(serde_json::to_value(FixRisk::High).unwrap(), "high");
    }
}
