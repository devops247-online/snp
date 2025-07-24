// Hook implementation template generation command
// Generates hook implementation templates for users

use crate::error::{Result, SnpError};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct HookImplConfig {
    pub language: String,
    pub hook_type: String,
    pub output_directory: PathBuf,
    pub hook_name: String,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct HookImplResult {
    pub template_generated: bool,
    pub template_language: String,
    pub files_created: Vec<String>,
    pub validation_success: bool,
}

pub async fn execute_hook_impl_command(config: &HookImplConfig) -> Result<HookImplResult> {
    info!(
        "Generating hook implementation template for language: {}",
        config.language
    );

    // Validate language support
    if !is_supported_language(&config.language) {
        return Err(SnpError::Config(Box::new(
            crate::error::ConfigError::ValidationFailed {
                message: format!("Unsupported language: {}", config.language),
                file_path: None,
                errors: vec![format!("Supported languages: python, rust, go, node")],
            },
        )));
    }

    // Create output directory
    fs::create_dir_all(&config.output_directory)?;

    // Generate template based on language
    let mut files_created = Vec::new();

    match config.language.as_str() {
        "python" => {
            let generator = PythonTemplateGenerator::new(config);
            files_created.extend(generator.generate_project_files().await?);
            files_created.extend(generator.generate_hook_manifest().await?);
            files_created.extend(generator.generate_test_files().await?);
            files_created.extend(generator.generate_documentation().await?);
        }
        "rust" => {
            let generator = RustTemplateGenerator::new(config);
            files_created.extend(generator.generate_project_files().await?);
            files_created.extend(generator.generate_hook_manifest().await?);
            files_created.extend(generator.generate_test_files().await?);
            files_created.extend(generator.generate_documentation().await?);
        }
        "go" => {
            let generator = GoTemplateGenerator::new(config);
            files_created.extend(generator.generate_project_files().await?);
            files_created.extend(generator.generate_hook_manifest().await?);
            files_created.extend(generator.generate_test_files().await?);
            files_created.extend(generator.generate_documentation().await?);
        }
        "node" | "javascript" | "typescript" => {
            let generator = NodeTemplateGenerator::new(config);
            files_created.extend(generator.generate_project_files().await?);
            files_created.extend(generator.generate_hook_manifest().await?);
            files_created.extend(generator.generate_test_files().await?);
            files_created.extend(generator.generate_documentation().await?);
        }
        _ => {
            return Err(SnpError::Config(Box::new(
                crate::error::ConfigError::ValidationFailed {
                    message: format!("Unsupported language: {}", config.language),
                    file_path: None,
                    errors: vec![],
                },
            )));
        }
    }

    // Validate generated template
    let validation_success = validate_generated_template(&config.output_directory, &files_created);

    Ok(HookImplResult {
        template_generated: !files_created.is_empty(),
        template_language: config.language.clone(),
        files_created,
        validation_success,
    })
}

fn is_supported_language(language: &str) -> bool {
    matches!(
        language,
        "python" | "rust" | "go" | "node" | "javascript" | "typescript"
    )
}

trait TemplateGenerator {
    async fn generate_project_files(&self) -> Result<Vec<String>>;
    async fn generate_hook_manifest(&self) -> Result<Vec<String>>;
    async fn generate_test_files(&self) -> Result<Vec<String>>;
    async fn generate_documentation(&self) -> Result<Vec<String>>;
}

struct PythonTemplateGenerator<'a> {
    config: &'a HookImplConfig,
}

impl<'a> PythonTemplateGenerator<'a> {
    fn new(config: &'a HookImplConfig) -> Self {
        Self { config }
    }

    fn get_script_name(&self) -> String {
        self.config.hook_name.replace('-', "_") + ".py"
    }

    fn get_module_name(&self) -> String {
        self.config.hook_name.replace('-', "_")
    }
}

impl<'a> TemplateGenerator for PythonTemplateGenerator<'a> {
    async fn generate_project_files(&self) -> Result<Vec<String>> {
        let mut files_created = Vec::new();

        // Generate setup.py
        let setup_py_content = format!(
            r#"#!/usr/bin/env python3

from setuptools import setup, find_packages

setup(
    name="{}",
    version="1.0.0",
    description="{}",
    py_modules=["{}"],
    install_requires=[],
    entry_points={{
        "console_scripts": [
            "{} = {}:main",
        ],
    }},
    python_requires=">=3.6",
)
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.get_module_name(),
            self.config.hook_name,
            self.get_module_name()
        );

        let setup_py_path = self.config.output_directory.join("setup.py");
        fs::write(&setup_py_path, setup_py_content)?;
        files_created.push("setup.py".to_string());

        // Generate main Python script
        let script_content = format!(
            r#"#!/usr/bin/env python3
"""
{} - {}
"""

import argparse
import sys
from typing import List, Optional, Sequence


def check_files(filenames: List[str]) -> int:
    """
    Main hook logic for {} files.

    Args:
        filenames: List of files to check

    Returns:
        0 if all files pass, 1 if any fail
    """
    retval = 0

    for filename in filenames:
        try:
            with open(filename, 'r', encoding='utf-8') as f:
                content = f.read()

            # TODO: Implement your {} logic here
            # Example: Check file content, format, lint, etc.
            print(f"Checking {{filename}}...")

            # Example validation
            if not content.strip():
                print(f"ERROR: {{filename}} is empty")
                retval = 1

        except Exception as e:
            print(f"ERROR: Failed to process {{filename}}: {{e}}")
            retval = 1

    return retval


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="{}"
    )
    parser.add_argument(
        'filenames',
        nargs='*',
        help='Filenames to check'
    )
    parser.add_argument(
        '--fix',
        action='store_true',
        help='Automatically fix issues where possible'
    )

    args = parser.parse_args(argv)

    if not args.filenames:
        print("No files provided")
        return 0

    return check_files(args.filenames)


if __name__ == '__main__':
    sys.exit(main())
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_type,
            self.config.hook_type,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name))
        );

        let script_path = self.config.output_directory.join(self.get_script_name());
        fs::write(&script_path, script_content)?;
        files_created.push(self.get_script_name());

        Ok(files_created)
    }

    async fn generate_hook_manifest(&self) -> Result<Vec<String>> {
        let hooks_yaml_content = format!(
            r#"-   id: {}
    name: {}
    description: {}
    entry: {}
    language: python
    files: {}
    stages: [pre-commit]
"#,
            self.config.hook_name,
            self.config.hook_name.replace('-', " ").to_title_case(),
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.get_script_name(),
            match self.config.hook_type.as_str() {
                "linter" => r"\.py$",
                "formatter" => r"\.py$",
                "checker" => r"\.(py|txt|md)$",
                _ => r".*",
            }
        );

        let hooks_yaml_path = self.config.output_directory.join(".pre-commit-hooks.yaml");
        fs::write(&hooks_yaml_path, hooks_yaml_content)?;

        Ok(vec![".pre-commit-hooks.yaml".to_string()])
    }

    async fn generate_test_files(&self) -> Result<Vec<String>> {
        let mut files_created = Vec::new();

        // Create tests directory
        let tests_dir = self.config.output_directory.join("tests");
        fs::create_dir_all(&tests_dir)?;

        // Generate test file
        let test_content = format!(
            r#"#!/usr/bin/env python3

import pytest
import tempfile
import os
from {} import main, check_files


def test_main_no_files():
    """Test main function with no files."""
    result = main([])
    assert result == 0


def test_main_with_files():
    """Test main function with test files."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
        f.write("print('hello')")
        f.flush()

        try:
            result = main([f.name])
            # Adjust assertion based on your hook logic
            assert result in [0, 1]
        finally:
            os.unlink(f.name)


def test_check_files_empty():
    """Test check_files with empty list."""
    result = check_files([])
    assert result == 0


def test_check_files_valid():
    '''Test check_files with valid files.'''
    with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
        f.write('# Valid Python file\nprint("hello")\n')
        f.flush()

        try:
            result = check_files([f.name])
            assert result == 0
        finally:
            os.unlink(f.name)


def test_check_files_invalid():
    """Test check_files with invalid files."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
        f.write("")  # Empty file should fail
        f.flush()

        try:
            result = check_files([f.name])
            assert result == 1
        finally:
            os.unlink(f.name)


if __name__ == '__main__':
    pytest.main([__file__])
"#,
            self.get_module_name()
        );

        let test_file_path = tests_dir.join(format!("test_{}.py", self.get_module_name()));
        fs::write(&test_file_path, test_content)?;
        files_created.push(format!("tests/test_{}.py", self.get_module_name()));

        Ok(files_created)
    }

    async fn generate_documentation(&self) -> Result<Vec<String>> {
        let readme_content = format!(
            r#"# {}

{}

## Installation

```bash
pip install -e .
```

## Usage

### As a pre-commit hook

Add this to your `.pre-commit-config.yaml`:

```yaml
repos:
-   repo: /path/to/this/repo
    rev: main
    hooks:
    -   id: {}
```

### Command line

```bash
{} file1.py file2.py
```

## Development

```bash
# Install development dependencies
pip install -e .[dev]

# Run tests
python -m pytest tests/

# Run the hook
python {} --help
```

## Configuration

TODO: Document any configuration options

## License

TODO: Add license information
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.config.hook_name,
            self.config.hook_name,
            self.get_script_name()
        );

        let readme_path = self.config.output_directory.join("README.md");
        fs::write(&readme_path, readme_content)?;

        Ok(vec!["README.md".to_string()])
    }
}

struct RustTemplateGenerator<'a> {
    config: &'a HookImplConfig,
}

impl<'a> RustTemplateGenerator<'a> {
    fn new(config: &'a HookImplConfig) -> Self {
        Self { config }
    }
}

impl<'a> TemplateGenerator for RustTemplateGenerator<'a> {
    async fn generate_project_files(&self) -> Result<Vec<String>> {
        let mut files_created = Vec::new();

        // Generate Cargo.toml
        let cargo_toml_content = format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"
description = "{}"

[[bin]]
name = "{}"
path = "src/main.rs"

[dependencies]
clap = {{ version = "4.0", features = ["derive"] }}
anyhow = "1.0"
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.config.hook_name
        );

        let cargo_toml_path = self.config.output_directory.join("Cargo.toml");
        fs::write(&cargo_toml_path, cargo_toml_content)?;
        files_created.push("Cargo.toml".to_string());

        // Create src directory
        let src_dir = self.config.output_directory.join("src");
        fs::create_dir_all(&src_dir)?;

        // Generate main.rs
        let main_rs_content = format!(
            r#"use clap::Parser;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "{}")]
#[command(about = "{}")]
struct Args {{
    /// Files to process
    files: Vec<PathBuf>,

    /// Fix issues automatically
    #[arg(long)]
    fix: bool,
}}

fn main() {{
    let args = Args::parse();

    if args.files.is_empty() {{
        println!("No files provided");
        return;
    }}

    let result = process_files(&args.files, args.fix);

    match result {{
        Ok(success) => {{
            if !success {{
                process::exit(1);
            }}
        }}
        Err(e) => {{
            eprintln!("Error: {{}}", e);
            process::exit(1);
        }}
    }}
}}

fn process_files(files: &[PathBuf], _fix: bool) -> anyhow::Result<bool> {{
    let mut success = true;

    for file in files {{
        println!("Processing {{:?}}", file);

        // TODO: Implement your {} logic here
        let content = std::fs::read_to_string(file)?;

        // Example validation
        if content.trim().is_empty() {{
            eprintln!("ERROR: {{:?}} is empty", file);
            success = false;
        }}
    }}

    Ok(success)
}}

#[cfg(test)]
mod tests {{
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_process_empty_files() {{
        let mut temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, "").unwrap();

        let result = process_files(&[temp_file.path().to_path_buf()], false);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should fail for empty file
    }}

    #[test]
    fn test_process_valid_files() {{
        let mut temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, "Hello, world!").unwrap();

        let result = process_files(&[temp_file.path().to_path_buf()], false);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Should succeed for non-empty file
    }}
}}
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_type
        );

        let main_rs_path = src_dir.join("main.rs");
        fs::write(&main_rs_path, main_rs_content)?;
        files_created.push("src/main.rs".to_string());

        Ok(files_created)
    }

    async fn generate_hook_manifest(&self) -> Result<Vec<String>> {
        let hooks_yaml_content = format!(
            r#"-   id: {}
    name: {}
    description: {}
    entry: {}
    language: rust
    files: {}
    stages: [pre-commit]
"#,
            self.config.hook_name,
            self.config.hook_name.replace('-', " ").to_title_case(),
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_name,
            match self.config.hook_type.as_str() {
                "linter" => r"\.rs$",
                "formatter" => r"\.rs$",
                "checker" => r"\.(rs|toml|md)$",
                _ => r".*",
            }
        );

        let hooks_yaml_path = self.config.output_directory.join(".pre-commit-hooks.yaml");
        fs::write(&hooks_yaml_path, hooks_yaml_content)?;

        Ok(vec![".pre-commit-hooks.yaml".to_string()])
    }

    async fn generate_test_files(&self) -> Result<Vec<String>> {
        // Rust tests are included in main.rs for simplicity
        Ok(vec![])
    }

    async fn generate_documentation(&self) -> Result<Vec<String>> {
        let readme_content = format!(
            r#"# {}

{}

## Installation

```bash
cargo build --release
```

## Usage

### As a pre-commit hook

Add this to your `.pre-commit-config.yaml`:

```yaml
repos:
-   repo: /path/to/this/repo
    rev: main
    hooks:
    -   id: {}
```

### Command line

```bash
cargo run -- file1.rs file2.rs
# or after building:
./target/release/{} file1.rs file2.rs
```

## Development

```bash
# Run tests
cargo test

# Run clippy
cargo clippy

# Format code
cargo fmt
```

## Configuration

TODO: Document any configuration options

## License

TODO: Add license information
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.config.hook_name,
            self.config.hook_name
        );

        let readme_path = self.config.output_directory.join("README.md");
        fs::write(&readme_path, readme_content)?;

        Ok(vec!["README.md".to_string()])
    }
}

struct GoTemplateGenerator<'a> {
    config: &'a HookImplConfig,
}

impl<'a> GoTemplateGenerator<'a> {
    fn new(config: &'a HookImplConfig) -> Self {
        Self { config }
    }
}

impl<'a> TemplateGenerator for GoTemplateGenerator<'a> {
    async fn generate_project_files(&self) -> Result<Vec<String>> {
        let mut files_created = Vec::new();

        // Generate go.mod
        let go_mod_content = format!(
            r#"module {}

go 1.19

require (
    github.com/spf13/cobra v1.7.0
)
"#,
            self.config.hook_name
        );

        let go_mod_path = self.config.output_directory.join("go.mod");
        fs::write(&go_mod_path, go_mod_content)?;
        files_created.push("go.mod".to_string());

        // Generate main.go
        let main_go_content = format!(
            r#"package main

import (
    "bufio"
    "fmt"
    "os"
    "strings"

    "github.com/spf13/cobra"
)

var (
    fix bool
)

var rootCmd = &cobra.Command{{
    Use:   "{}",
    Short: "{}",
    Args:  cobra.MinimumNArgs(0),
    Run:   run,
}}

func init() {{
    rootCmd.Flags().BoolVar(&fix, "fix", false, "Fix issues automatically")
}}

func main() {{
    if err := rootCmd.Execute(); err != nil {{
        fmt.Fprintf(os.Stderr, "Error: %v\n", err)
        os.Exit(1)
    }}
}}

func run(cmd *cobra.Command, args []string) {{
    if len(args) == 0 {{
        fmt.Println("No files provided")
        return
    }}

    success := true

    for _, filename := range args {{
        fmt.Printf("Processing %s\n", filename)

        if err := processFile(filename); err != nil {{
            fmt.Fprintf(os.Stderr, "ERROR: Failed to process %s: %v\n", filename, err)
            success = false
        }}
    }}

    if !success {{
        os.Exit(1)
    }}
}}

func processFile(filename string) error {{
    file, err := os.Open(filename)
    if err != nil {{
        return err
    }}
    defer file.Close()

    scanner := bufio.NewScanner(file)
    lineCount := 0

    for scanner.Scan() {{
        line := scanner.Text()
        lineCount++

        // TODO: Implement your {} logic here
        // Example validation
        if strings.TrimSpace(line) == "" && lineCount == 1 {{
            return fmt.Errorf("file starts with empty line")
        }}
    }}

    if err := scanner.Err(); err != nil {{
        return err
    }}

    if lineCount == 0 {{
        return fmt.Errorf("file is empty")
    }}

    return nil
}}
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_type
        );

        let main_go_path = self.config.output_directory.join("main.go");
        fs::write(&main_go_path, main_go_content)?;
        files_created.push("main.go".to_string());

        Ok(files_created)
    }

    async fn generate_hook_manifest(&self) -> Result<Vec<String>> {
        let hooks_yaml_content = format!(
            r#"-   id: {}
    name: {}
    description: {}
    entry: {}
    language: golang
    files: {}
    stages: [pre-commit]
"#,
            self.config.hook_name,
            self.config.hook_name.replace('-', " ").to_title_case(),
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_name,
            match self.config.hook_type.as_str() {
                "linter" => r"\.go$",
                "formatter" => r"\.go$",
                "checker" => r"\.(go|mod|sum)$",
                _ => r".*",
            }
        );

        let hooks_yaml_path = self.config.output_directory.join(".pre-commit-hooks.yaml");
        fs::write(&hooks_yaml_path, hooks_yaml_content)?;

        Ok(vec![".pre-commit-hooks.yaml".to_string()])
    }

    async fn generate_test_files(&self) -> Result<Vec<String>> {
        // Go tests would be in main_test.go or separate test files
        Ok(vec![])
    }

    async fn generate_documentation(&self) -> Result<Vec<String>> {
        let readme_content = format!(
            r#"# {}

{}

## Installation

```bash
go build
```

## Usage

### As a pre-commit hook

Add this to your `.pre-commit-config.yaml`:

```yaml
repos:
-   repo: /path/to/this/repo
    rev: main
    hooks:
    -   id: {}
```

### Command line

```bash
go run main.go file1.go file2.go
# or after building:
./{} file1.go file2.go
```

## Development

```bash
# Run tests
go test

# Format code
go fmt

# Run linter
golangci-lint run
```

## Configuration

TODO: Document any configuration options

## License

TODO: Add license information
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.config.hook_name,
            self.config.hook_name
        );

        let readme_path = self.config.output_directory.join("README.md");
        fs::write(&readme_path, readme_content)?;

        Ok(vec!["README.md".to_string()])
    }
}

struct NodeTemplateGenerator<'a> {
    config: &'a HookImplConfig,
}

impl<'a> NodeTemplateGenerator<'a> {
    fn new(config: &'a HookImplConfig) -> Self {
        Self { config }
    }
}

impl<'a> TemplateGenerator for NodeTemplateGenerator<'a> {
    async fn generate_project_files(&self) -> Result<Vec<String>> {
        let mut files_created = Vec::new();

        // Generate package.json
        let package_json_content = format!(
            r#"{{
  "name": "{}",
  "version": "1.0.0",
  "description": "{}",
  "main": "index.js",
  "bin": {{
    "{}": "./index.js"
  }},
  "scripts": {{
    "test": "jest",
    "lint": "eslint ."
  }},
  "dependencies": {{
    "commander": "^9.0.0"
  }},
  "devDependencies": {{
    "jest": "^29.0.0",
    "eslint": "^8.0.0"
  }},
  "keywords": ["pre-commit", "hook", "{}"],
  "author": "",
  "license": "MIT"
}}
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.config.hook_name,
            self.config.hook_type
        );

        let package_json_path = self.config.output_directory.join("package.json");
        fs::write(&package_json_path, package_json_content)?;
        files_created.push("package.json".to_string());

        // Generate index.js
        let index_js_content = format!(
            r#"#!/usr/bin/env node

const {{ program }} = require('commander');
const fs = require('fs');
const path = require('path');

program
  .name('{}')
  .description('{}')
  .argument('[files...]', 'files to process')
  .option('--fix', 'fix issues automatically')
  .action((files, options) => {{
    if (files.length === 0) {{
      console.log('No files provided');
      return;
    }}

    let success = true;

    for (const file of files) {{
      console.log(`Processing ${{file}}`);

      try {{
        const content = fs.readFileSync(file, 'utf8');

        // TODO: Implement your {} logic here
        // Example validation
        if (content.trim() === '') {{
          console.error(`ERROR: ${{file}} is empty`);
          success = false;
        }}

      }} catch (error) {{
        console.error(`ERROR: Failed to process ${{file}}: ${{error.message}}`);
        success = false;
      }}
    }}

    if (!success) {{
      process.exit(1);
    }}
  }});

program.parse();
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_type
        );

        let index_js_path = self.config.output_directory.join("index.js");
        fs::write(&index_js_path, index_js_content)?;
        files_created.push("index.js".to_string());

        Ok(files_created)
    }

    async fn generate_hook_manifest(&self) -> Result<Vec<String>> {
        let hooks_yaml_content = format!(
            r#"-   id: {}
    name: {}
    description: {}
    entry: {}
    language: node
    files: {}
    stages: [pre-commit]
"#,
            self.config.hook_name,
            self.config.hook_name.replace('-', " ").to_title_case(),
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook", self.config.hook_name)),
            self.config.hook_name,
            match self.config.hook_type.as_str() {
                "linter" => r"\.(js|jsx|ts|tsx)$",
                "formatter" => r"\.(js|jsx|ts|tsx|json|css)$",
                "checker" => r"\.(js|jsx|ts|tsx|json|md)$",
                _ => r".*",
            }
        );

        let hooks_yaml_path = self.config.output_directory.join(".pre-commit-hooks.yaml");
        fs::write(&hooks_yaml_path, hooks_yaml_content)?;

        Ok(vec![".pre-commit-hooks.yaml".to_string()])
    }

    async fn generate_test_files(&self) -> Result<Vec<String>> {
        // Node.js tests would be in separate test files
        Ok(vec![])
    }

    async fn generate_documentation(&self) -> Result<Vec<String>> {
        let readme_content = format!(
            r#"# {}

{}

## Installation

```bash
npm install
# or
yarn install
```

## Usage

### As a pre-commit hook

Add this to your `.pre-commit-config.yaml`:

```yaml
repos:
-   repo: /path/to/this/repo
    rev: main
    hooks:
    -   id: {}
```

### Command line

```bash
node index.js file1.js file2.js
# or
npm start -- file1.js file2.js
```

## Development

```bash
# Run tests
npm test

# Run linter
npm run lint

# Install dependencies
npm install
```

## Configuration

TODO: Document any configuration options

## License

MIT
"#,
            self.config.hook_name,
            self.config
                .description
                .as_ref()
                .unwrap_or(&format!("{} hook implementation", self.config.hook_name)),
            self.config.hook_name
        );

        let readme_path = self.config.output_directory.join("README.md");
        fs::write(&readme_path, readme_content)?;

        Ok(vec!["README.md".to_string()])
    }
}

fn validate_generated_template(output_dir: &Path, files_created: &[String]) -> bool {
    // Check that all expected files were created
    for file in files_created {
        let file_path = output_dir.join(file);
        if !file_path.exists() {
            debug!("Validation failed: {} does not exist", file);
            return false;
        }
    }

    // Check that .pre-commit-hooks.yaml is valid YAML
    let hooks_yaml_path = output_dir.join(".pre-commit-hooks.yaml");
    if hooks_yaml_path.exists() {
        if let Ok(content) = fs::read_to_string(&hooks_yaml_path) {
            if serde_yaml::from_str::<serde_yaml::Value>(&content).is_err() {
                debug!("Validation failed: .pre-commit-hooks.yaml is invalid YAML");
                return false;
            }
        }
    }

    true
}

trait ToTitleCase {
    fn to_title_case(&self) -> String;
}

impl ToTitleCase for str {
    fn to_title_case(&self) -> String {
        self.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}
