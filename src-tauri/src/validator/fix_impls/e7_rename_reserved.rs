//! E7：Manual — 仅返回 descriptor（保留前缀目录，需手动重命名）

use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_only() -> FixDescriptor {
    FixDescriptor {
        rule_id: "E7".into(),
        kind: FixKind::Manual,
        risk: FixRisk::Low,
        label_key: "Fix.E7.apply".into(),
        preview_key: None,
        confirm_key: None,
        docs_key: "Validation.rule.E7.fixGuide".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E7 is a Manual descriptor — the user must rename the entry's
    /// directory to avoid the reserved prefix. Verify the descriptor
    /// shape only.
    #[test]
    fn descriptor_only_has_expected_shape() {
        let desc = descriptor_only();
        assert_eq!(desc.rule_id, "E7");
        assert!(matches!(desc.kind, FixKind::Manual));
        assert!(matches!(desc.risk, FixRisk::Low));
        assert_eq!(desc.label_key, "Fix.E7.apply");
        assert!(desc.preview_key.is_none());
        assert!(desc.confirm_key.is_none());
        assert_eq!(desc.docs_key, "Validation.rule.E7.fixGuide");
    }
}
