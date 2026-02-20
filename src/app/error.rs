use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Update error: {0}")]
    Update(String),

    #[error("Settings error: {0}")]
    Settings(String),

    #[error("Session error: {0}")]
    Session(String),
}

/// Convenience type alias for Results with AppError
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err, AppError::Io(_)));
        assert!(app_err.to_string().contains("file not found"));
    }

    #[test]
    fn test_error_display() {
        let err = AppError::Update("version check failed".to_string());
        assert_eq!(err.to_string(), "Update error: version check failed");

        let err = AppError::Settings("invalid font size".to_string());
        assert_eq!(err.to_string(), "Settings error: invalid font size");

        let err = AppError::Session("corrupt session file".to_string());
        assert_eq!(err.to_string(), "Session error: corrupt session file");
    }
}
