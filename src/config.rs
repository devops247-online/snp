// Configuration handling for SNP
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Result, SnpError};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub repos: Vec<Repository>,
    pub default_install_hook_types: Option<Vec<String>>,
    pub default_language_version: Option<HashMap<String, String>>,
    pub default_stages: Option<Vec<String>>,
    pub files: Option<String>,
    pub exclude: Option<String>,
    pub fail_fast: Option<bool>,
    pub minimum_pre_commit_version: Option<String>,
    pub ci: Option<serde_yaml::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Repository {
    pub repo: String,
    pub rev: Option<String>,
    pub hooks: Vec<Hook>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Hook {
    pub id: String,
    pub name: Option<String>,
    pub entry: String,
    pub language: String,
    pub files: Option<String>,
    pub exclude: Option<String>,
    pub types: Option<Vec<String>>,
    pub exclude_types: Option<Vec<String>>,
    pub additional_dependencies: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub always_run: Option<bool>,
    pub fail_fast: Option<bool>,
    pub pass_filenames: Option<bool>,
    pub stages: Option<Vec<String>>,
    pub verbose: Option<bool>,
}

impl Config {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SnpError::Config(format!("Failed to read config file: {e}")))?;

        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let config: Config = serde_yaml::from_str(yaml)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_config() {
        let yaml = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black
    language: python
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.repos.len(), 1);
        assert_eq!(config.repos[0].repo, "https://github.com/psf/black");
        assert_eq!(config.repos[0].hooks.len(), 1);
        assert_eq!(config.repos[0].hooks[0].id, "black");
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "invalid: yaml: content:";
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_defaults() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let hook = &config.repos[0].hooks[0];

        // Test that optional fields can be None
        assert!(hook.name.is_none());
        assert!(hook.always_run.is_none());
        assert!(hook.pass_filenames.is_none());
    }
}
