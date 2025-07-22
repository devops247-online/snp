// Error handling for SNP
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SnpError>;

#[derive(Debug, Error)]
pub enum SnpError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Git operation failed: {0}")]
    Git(String),

    #[error("Hook execution failed: {0}")]
    HookExecution(String),

    #[error("IO operation failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("CLI argument error: {0}")]
    Cli(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = SnpError::Config("test error".to_string());
        assert_eq!(error.to_string(), "Configuration error: test error");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let snp_error = SnpError::from(io_error);
        assert!(snp_error.to_string().contains("IO operation failed"));
    }
}
