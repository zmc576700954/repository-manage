//! E5：Manual — 仅返回 descriptor（目录深度过深，需手动调整）

use crate::validator::fix::{FixDescriptor, FixKind, FixRisk};

pub fn descriptor_only() -> FixDescriptor {
    FixDescriptor {
        rule_id: "E5".into(),
        kind: FixKind::Manual,
        risk: FixRisk::Low,
        label_key: "Fix.E5.apply".into(),
        preview_key: None,
        confirm_key: None,
        docs_key: "Validation.rule.E5.fixGuide".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E5 is a Manual descriptor — the user must reorganize the entry's
    /// directory depth by hand. Verify the descriptor shape only.
    #[test]
    fn descriptor_only_has_expected_shape() {
        let desc = descriptor_only();
        assert_eq!(desc.rule_id, "E5");
        assert!(matches!(desc.kind, FixKind::Manual));
        assert!(matches!(desc.risk, FixRisk::Low));
        assert_eq!(desc.label_key, "Fix.E5.apply");
        assert!(desc.preview_key.is_none());
        assert!(desc.confirm_key.is_none());
        assert_eq!(desc.docs_key, "Validation.rule.E5.fixGuide");
    }
}
