use serde::Serialize;
use thiserror::Error;

/// Application-level error. Every variant carries a stable kebab-case
/// `code` (serialized to the frontend) and the params needed for
/// translation. The frontend's i18next layer maps each code to a
/// message in the active locale — pre-fix these were hardcoded Chinese
/// strings in commands.rs that bypassed i18next entirely (Finding #14).
///
/// `Display` keeps a short developer-readable form (`code: args`) so
/// `eprintln!` logs and tests are still useful. The structured form is
/// what the user sees.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum AppError {
    /// Filesystem path the caller named does not exist.
    #[error("path-not-found: {path}")]
    PathNotFound { path: String },

    /// Filesystem path exists but isn't a directory.
    #[error("not-a-directory: {path}")]
    NotADirectory { path: String },

    /// KB directory is no longer reachable on disk (user moved/deleted it).
    #[error("kb-dir-gone: {path}")]
    KbDirGone { path: String },

    /// KB path exists but is not a directory (replaced with a file).
    #[error("kb-dir-not-directory: {path}")]
    KbDirNotADirectory { path: String },

    /// No KB is currently active.
    #[error("no-active-kb")]
    NoActiveKb,

    /// A KB id is unknown to the registry.
    #[error("kb-not-found: {kb_id}")]
    KbNotFound { kb_id: String },

    /// An entry id is unknown to the index.
    #[error("entry-not-found: {id}")]
    EntryNotFound { id: String },

    /// `name` argument was empty.
    #[error("name-empty")]
    NameEmpty,

    /// `name` argument contained path separators / traversal components.
    #[error("name-invalid: {name}")]
    NameInvalid { name: String },

    /// `name` argument ended with `.md` (must be a stem).
    #[error("name-has-md-suffix")]
    NameHasMdSuffix,

    /// `parent_group` argument tried to escape the KB root.
    #[error("parent-group-invalid: {parent_group}")]
    ParentGroupInvalid { parent_group: String },

    /// Two entries already exist at the same on-disk path.
    #[error("file-exists: {path}")]
    FileExists { path: String },

    /// Backend IO failure (failed mkdir, write, read, etc.).
    #[error("io: {message}")]
    IoOp { message: String },

    /// rusqlite failure.
    #[error("database: {message}")]
    DatabaseOp { message: String },

    /// Failed to persist the KB registry.
    #[error("save-kbs-failed: {message}")]
    SaveKbsFailed { message: String },

    /// Fallback for unclassified errors. Carries a developer-readable
    /// English message — the frontend should still display the code in
    /// its i18n table rather than the raw message, but if the code is
    /// missing from the bundle the message gives the user something
    /// to read.
    #[error("other: {message}")]
    Other { message: String },

    /// Fix dispatch asked for a rule id that has no implementation.
    #[error("unknown-rule: {rule_id}")]
    UnknownRule { rule_id: String },
}

impl AppError {
    /// Wrap a std::io::Error with a context message for the frontend.
    pub fn io(msg: impl Into<String>, _e: std::io::Error) -> Self {
        Self::IoOp {
            message: msg.into(),
        }
    }

    /// Wrap a rusqlite::Error with a context message.
    pub fn db(msg: impl Into<String>, _e: rusqlite::Error) -> Self {
        Self::DatabaseOp {
            message: msg.into(),
        }
    }
}

// Keep `?` working for the common conversions. `From<io::Error>` is
// lossy (no context message); callers in hot paths should use the
// `AppError::io` helper to attach context.
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::IoOp {
            message: e.to_string(),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::DatabaseOp {
            message: e.to_string(),
        }
    }
}

impl AppError {
    /// Wrap an `io::Error` from `kb_registry::save()` calls. Keeps the
    /// variant-specific code (`save-kbs-failed`) so the i18n bundle can
    /// show a registry-specific message instead of a generic IO message.
    pub fn from_save_kbs(e: std::io::Error) -> Self {
        Self::SaveKbsFailed {
            message: e.to_string(),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend relies on the kebab-case `code` field. Pin the
    /// serialization shape — renaming a variant must be a deliberate
    /// decision because the i18n bundle uses the code as the key.
    #[test]
    fn path_not_found_serializes_with_kebab_case_code() {
        let err = AppError::PathNotFound {
            path: "/tmp/missing".to_string(),
        };
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "path-not-found");
        assert_eq!(json["path"], "/tmp/missing");
    }

    #[test]
    fn no_active_kb_serializes_without_params() {
        let err = AppError::NoActiveKb;
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "no-active-kb");
    }

    #[test]
    fn io_from_propagates_message() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();
        let s = app_err.to_string();
        assert!(s.contains("io:"));
        assert!(s.contains("file not found"));
    }

    #[test]
    fn other_carries_message() {
        let err = AppError::Other {
            message: "something".to_string(),
        };
        assert!(err.to_string().contains("something"));
    }
}