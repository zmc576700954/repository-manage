use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Other: {0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_displays_message() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();
        let s = app_err.to_string();
        assert!(s.contains("IO error"));
        assert!(s.contains("file not found"));
    }

    #[test]
    fn other_error_displays_message() {
        let app_err = AppError::Other("custom failure".to_string());
        assert_eq!(app_err.to_string(), "Other: custom failure");
    }

    #[test]
    fn database_error_displays_message() {
        // 构造一个 rusqlite 错误（打开不存在的路径无效时易产生，这里用语法错误制造）
        let db_err = rusqlite::Error::InvalidQuery;
        let app_err: AppError = db_err.into();
        assert!(app_err.to_string().contains("Database error"));
    }

    #[test]
    fn error_serializes_to_string() {
        let app_err = AppError::Other("serializable".to_string());
        let json = serde_json::to_string(&app_err).unwrap();
        assert!(json.contains("serializable"));
    }
}