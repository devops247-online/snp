// SNP (Shell Not Pass) - Library module
// This file contains the core library functionality

pub mod cli;
pub mod config;
pub mod error;

// Re-export main types for easier access
pub use error::{Result, SnpError};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constant() {
        assert_eq!(VERSION, "0.1.0");
    }

    #[test]
    fn test_name_constant() {
        assert_eq!(NAME, "snp");
    }

    #[test]
    fn test_description_exists() {
        assert!(!DESCRIPTION.is_empty());
        assert!(DESCRIPTION.contains("Shell Not Pass"));
    }
}
