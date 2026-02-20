use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Update error: {0}")]
    Update(String),
}

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
    }
}
