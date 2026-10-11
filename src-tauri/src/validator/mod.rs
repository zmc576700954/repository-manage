// Placeholder validator module.
// Full validation logic will be implemented in Task 2.1+.

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub rule_id: String,
    pub message: String,
    pub severity: String,
}