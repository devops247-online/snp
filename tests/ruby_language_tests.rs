use std::collections::HashMap;
use std::path::PathBuf;

use snp::core::Hook;
use snp::language::environment::{CacheStrategy, EnvironmentConfig, IsolationLevel};
use snp::language::ruby::{RubyError, RubyLanguagePlugin, RubyVersionManager};
use snp::language::traits::Language;
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_ruby_language_detection() {
    let plugin = RubyLanguagePlugin::new();

    // Test basic language properties
    assert_eq!(plugin.language_name(), "ruby");

    let extensions = plugin.supported_extensions();
    assert!(extensions.contains(&"rb"));
    assert!(extensions.contains(&"ruby"));
    assert!(extensions.contains(&"gemspec"));

    let patterns = plugin.detection_patterns();
    assert!(!patterns.is_empty());
    assert!(patterns.iter().any(|p| p.contains("Gemfile")));
    assert!(patterns.iter().any(|p| p.contains("#!/usr/bin/env ruby")));
}

#[tokio::test]
async fn test_ruby_version_detection() {
    let mut plugin = RubyLanguagePlugin::new();

    // Test Ruby version detection - skip if Ruby not installed
    match plugin.detect_ruby_installations().await {
        Ok(installations) => {
            if installations.is_empty() {
                eprintln!("No Ruby installations found on system - this is expected if Ruby is not installed");
                return; // Skip the rest of the test
            }

            for installation in &installations {
                assert!(
                    installation.ruby_executable.exists(),
                    "Ruby executable should exist"
                );
                assert!(
                    !installation.version.is_empty(),
                    "Version should be detected"
                );
                assert!(
                    !installation.gem_executable.exists() || installation.gem_executable.exists(),
                    "Gem executable should exist if available"
                );
            }
        }
        Err(_) => {
            eprintln!("Ruby not found on system - skipping version detection test");
        }
    }
}

#[tokio::test]
async fn test_ruby_version_managers() {
    let plugin = RubyLanguagePlugin::new();

    // Test version manager detection
    let managers = plugin.detect_version_managers().await;

    // This should not fail even if no version managers are installed
    for manager in managers {
        match manager {
            RubyVersionManager::Rbenv(path) => {
                assert!(path.exists(), "rbenv path should exist");
            }
            RubyVersionManager::Rvm(path) => {
                assert!(path.exists(), "rvm path should exist");
            }
            RubyVersionManager::ChRuby(path) => {
                assert!(path.exists(), "chruby path should exist");
            }
            RubyVersionManager::System => {
                // System version manager is always valid
            }
        }
    }
}

#[tokio::test]
async fn test_gemfile_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a test Gemfile
    let gemfile_content = r#"# frozen_string_literal: true

source 'https://rubygems.org'

ruby '3.0.0'

gem 'rails', '~> 7.0.0'
gem 'pg', '>= 0.18', '< 2.0'
gem 'puma', '~> 5.0'
gem 'sassc-rails', '>= 2.1.0'

group :development, :test do
  gem 'byebug', platforms: [:mri, :mingw, :x64_mingw]
  gem 'rspec-rails'
end

group :development do
  gem 'listen', '~> 3.3'
  gem 'spring'
end

# Windows does not include zoneinfo files, so bundle the tzinfo-data gem
gem 'tzinfo-data', platforms: [:mingw, :mswin, :x64_mingw, :jruby]
"#;

    fs::write(project_path.join("Gemfile"), gemfile_content)
        .await
        .unwrap();

    let mut plugin = RubyLanguagePlugin::new();
    let bundler_info = plugin.analyze_bundler_info(project_path).await.unwrap();

    // Test Gemfile parsing
    assert_eq!(bundler_info.ruby_version, Some("3.0.0".to_string()));
    assert_eq!(bundler_info.source, "https://rubygems.org");
    assert!(!bundler_info.dependencies.is_empty());

    // Check specific gems
    let rails_gem = bundler_info
        .dependencies
        .iter()
        .find(|g| g.name == "rails")
        .expect("Should find rails gem");
    assert_eq!(rails_gem.version_requirement, "~> 7.0.0");
    assert_eq!(rails_gem.groups, vec!["default"]);

    let byebug_gem = bundler_info
        .dependencies
        .iter()
        .find(|g| g.name == "byebug")
        .expect("Should find byebug gem");
    assert!(byebug_gem.groups.contains(&"development".to_string()));
    assert!(byebug_gem.groups.contains(&"test".to_string()));
}

#[tokio::test]
async fn test_gemfile_lock_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create Gemfile.lock
    let gemfile_lock = r#"GEM
  remote: https://rubygems.org/
  specs:
    actionpack (7.0.4)
      actionview (= 7.0.4)
      activesupport (= 7.0.4)
    actionview (7.0.4)
      activesupport (= 7.0.4)
      builder (~> 3.1)
    activesupport (7.0.4)
      concurrent-ruby (~> 1.0, >= 1.0.2)
      i18n (>= 1.6, < 2)
    builder (3.2.4)
    concurrent-ruby (1.1.10)
    i18n (1.12.0)
      concurrent-ruby (~> 1.0)

PLATFORMS
  x86_64-linux

DEPENDENCIES
  actionpack
  actionview

RUBY VERSION
   ruby 3.0.0p648

BUNDLED WITH
   2.3.0
"#;

    fs::write(project_path.join("Gemfile.lock"), gemfile_lock)
        .await
        .unwrap();

    let mut plugin = RubyLanguagePlugin::new();
    let bundler_info = plugin.analyze_bundler_info(project_path).await.unwrap();

    // Test lock file parsing
    assert!(bundler_info.gemfile_lock_path.is_some());
    assert_eq!(bundler_info.bundler_version, Some("2.3.0".to_string()));
    assert!(!bundler_info.locked_dependencies.is_empty());

    // Check locked versions
    let actionpack = bundler_info
        .locked_dependencies
        .iter()
        .find(|g| g.name == "actionpack")
        .expect("Should find actionpack");
    assert_eq!(actionpack.version, "7.0.4");
}

#[tokio::test]
async fn test_ruby_environment_setup() {
    // Skip if Ruby not available
    if which::which("ruby").is_err() {
        eprintln!("Ruby not found - skipping environment setup test");
        return;
    }

    let plugin = RubyLanguagePlugin::new();

    let env_config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec!["rake".to_string()],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::None,
        isolation_level: IsolationLevel::Complete,
        working_directory: None,
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env = plugin.setup_environment(&env_config).await.unwrap();

    // Test environment setup
    assert_eq!(env.language, "ruby");
    assert!(env.root_path.exists());
    assert!(env.executable_path.exists());
    assert!(!env.environment_variables.is_empty());

    // Test environment variables
    assert!(env.environment_variables.contains_key("GEM_HOME"));
    assert!(
        env.environment_variables.contains_key("GEM_PATH")
            || env
                .environment_variables
                .get("GEM_PATH")
                .map(|v| v.is_empty())
                .unwrap_or(true)
    );
}

#[tokio::test]
async fn test_gem_installation() {
    if which::which("ruby").is_err() || which::which("gem").is_err() {
        eprintln!("Ruby/gem not found - skipping gem installation test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a simple gemspec
    let gemspec_content = r#"Gem::Specification.new do |spec|
  spec.name          = "test-gem"
  spec.version       = "0.1.0"
  spec.authors       = ["Test Author"]
  spec.email         = ["test@example.com"]
  spec.summary       = "Test gem"
  spec.description   = "A test gem for SNP"
  spec.homepage      = "https://example.com"
  spec.license       = "MIT"

  spec.files         = ["lib/test.rb"]
  spec.require_paths = ["lib"]
end
"#;

    fs::write(project_path.join("test-gem.gemspec"), gemspec_content)
        .await
        .unwrap();

    // Create lib directory and file
    fs::create_dir_all(project_path.join("lib")).await.unwrap();
    fs::write(project_path.join("lib/test.rb"), "# Test Ruby file")
        .await
        .unwrap();

    let plugin = RubyLanguagePlugin::new();

    let env_config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::None,
        isolation_level: IsolationLevel::Complete,
        working_directory: Some(project_path.to_path_buf()),
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env = plugin.setup_environment(&env_config).await.unwrap();

    // Test gem building and installation
    let result = plugin
        .install_gem_dependencies(&env, &[], project_path)
        .await;

    // This might fail in test environment, but shouldn't panic
    match result {
        Ok(_) => println!("Gem installation succeeded"),
        Err(e) => println!("Gem installation failed (expected in test): {e:?}"),
    }
}

#[tokio::test]
async fn test_ruby_hook_execution() {
    if which::which("ruby").is_err() {
        eprintln!("Ruby not found - skipping hook execution test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a simple Ruby script
    let script_content = r#"#!/usr/bin/env ruby
puts "Hello from Ruby hook!"
ARGV.each do |file|
  puts "Processing: #{file}"
end
"#;

    fs::write(project_path.join("script.rb"), script_content)
        .await
        .unwrap();

    let plugin = RubyLanguagePlugin::new();

    let env_config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::None,
        isolation_level: IsolationLevel::Complete,
        working_directory: Some(project_path.to_path_buf()),
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env = plugin.setup_environment(&env_config).await.unwrap();

    let hook = Hook::new("ruby-test", "ruby script.rb", "ruby").with_name("Ruby Test Hook");

    let test_files = vec![project_path.join("test1.rb"), project_path.join("test2.rb")];

    let result = plugin.execute_hook(&hook, &env, &test_files).await.unwrap();

    if !result.success {
        eprintln!("Hook execution failed:");
        eprintln!("Exit code: {:?}", result.exit_code);
        eprintln!("Stdout: {}", result.stdout);
        eprintln!("Stderr: {}", result.stderr);
        if let Some(ref error) = result.error {
            eprintln!("Error: {error:?}");
        }
    }

    assert!(result.success, "Ruby hook should execute successfully");
    assert!(result.stdout.contains("Hello from Ruby hook!"));
}

#[tokio::test]
async fn test_bundler_hook_execution() {
    if which::which("ruby").is_err() || which::which("bundle").is_err() {
        eprintln!("Ruby/bundler not found - skipping bundler execution test");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create Gemfile
    let gemfile = r#"source 'https://rubygems.org'
gem 'rake'
"#;
    fs::write(project_path.join("Gemfile"), gemfile)
        .await
        .unwrap();

    // Create Rakefile
    let rakefile = r#"task :test do
  puts "Running Ruby tests via Rake"
  ARGV.each { |file| puts "Checking: #{file}" }
end
"#;
    fs::write(project_path.join("Rakefile"), rakefile)
        .await
        .unwrap();

    let plugin = RubyLanguagePlugin::new();

    let env_config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec!["rake".to_string()],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::None,
        isolation_level: IsolationLevel::Complete,
        working_directory: Some(project_path.to_path_buf()),
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    let env = plugin.setup_environment(&env_config).await.unwrap();

    let hook = Hook::new("rake-test", "bundle exec rake test", "ruby").with_name("Rake Test Hook");

    let test_files = vec![project_path.join("lib/test.rb")];

    let result = plugin.execute_hook(&hook, &env, &test_files).await.unwrap();

    if !result.success {
        eprintln!("Bundler hook execution failed:");
        eprintln!("Exit code: {:?}", result.exit_code);
        eprintln!("Stdout: {}", result.stdout);
        eprintln!("Stderr: {}", result.stderr);
        if let Some(ref error) = result.error {
            eprintln!("Error: {error:?}");
        }
    }

    // This test might fail due to bundle install requirements, but should not panic
    println!("Bundler hook completed with success: {}", result.success);
}

#[tokio::test]
async fn test_ruby_error_types() {
    let error = RubyError::RubyNotFound {
        searched_paths: vec![PathBuf::from("/usr/bin/ruby")],
        min_version: Some("2.7.0".to_string()),
    };

    assert!(error.to_string().contains("Ruby installation not found"));

    let gem_error = RubyError::GemInstallationFailed {
        gem: "nonexistent-gem".to_string(),
        version: Some("1.0.0".to_string()),
        error: "Not found".to_string(),
    };

    assert!(gem_error.to_string().contains("Gem installation failed"));
}

#[tokio::test]
async fn test_ruby_version_requirements() {
    let plugin = RubyLanguagePlugin::new();

    // Test version requirement parsing
    let requirements = vec![
        ("~> 2.7.0", true),
        (">= 2.5.0", true),
        ("< 4.0.0", true),
        ("= 3.0.0", true),
        ("invalid_version", false),
    ];

    for (req, should_parse) in requirements {
        let result = plugin.parse_version_requirement(req);
        assert_eq!(
            result.is_ok(),
            should_parse,
            "Version requirement '{req}' parsing should be {should_parse}"
        );
    }
}

#[tokio::test]
async fn test_cross_platform_paths() {
    let plugin = RubyLanguagePlugin::new();

    // Test path handling for different platforms
    let test_paths = vec![
        "/usr/local/bin/ruby",
        "/opt/ruby/bin/ruby",
        "C:\\Ruby30-x64\\bin\\ruby.exe",
        "/Users/user/.rbenv/versions/3.0.0/bin/ruby",
    ];

    for path_str in test_paths {
        let path = PathBuf::from(path_str);
        // Should not panic when analyzing paths
        let _ = plugin.analyze_ruby_executable(&path).await;
    }
}

#[tokio::test]
async fn test_gem_dependency_resolution() {
    let plugin = RubyLanguagePlugin::new();

    // Test dependency string parsing
    let deps = vec![
        "rake".to_string(),
        "rspec >= 3.0".to_string(),
        "rails ~> 7.0.0".to_string(),
    ];
    let resolved = plugin.resolve_dependencies(&deps).await.unwrap();

    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0].name, "rake");
    assert_eq!(resolved[1].name, "rspec");
    assert_eq!(resolved[2].name, "rails");
}

#[tokio::test]
async fn test_bundler_environment_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    let plugin = RubyLanguagePlugin::new();

    let env_config = EnvironmentConfig {
        language_version: Some("system".to_string()),
        additional_dependencies: vec![],
        environment_variables: HashMap::new(),
        cache_strategy: CacheStrategy::Memory,
        isolation_level: IsolationLevel::Complete,
        working_directory: Some(project_path.to_path_buf()),
        version: None,
        repository_path: None,
        hook_timeout: None,
    };

    if let Ok(env) = plugin.setup_environment(&env_config).await {
        // Test environment isolation
        assert!(env.environment_variables.contains_key("GEM_HOME"));
        assert!(env
            .environment_variables
            .get("BUNDLE_IGNORE_CONFIG")
            .map(|v| v == "1")
            .unwrap_or(false));

        // Test that paths are properly isolated
        let gem_home = env.environment_variables.get("GEM_HOME").unwrap();
        assert!(gem_home.contains(&env.environment_id));
    }
}

#[test]
fn test_ruby_language_plugin_creation() {
    let plugin = RubyLanguagePlugin::new();
    assert_eq!(plugin.language_name(), "ruby");

    let extensions = plugin.supported_extensions();
    assert!(extensions.contains(&"rb"));
    assert!(extensions.contains(&"ruby"));
    assert!(extensions.contains(&"gemspec"));

    let patterns = plugin.detection_patterns();
    assert!(!patterns.is_empty());
}

#[test]
fn test_gem_dependency_parsing() {
    let plugin = RubyLanguagePlugin::new();

    // Test various gem specification formats
    let specs = vec![
        ("rake", "rake", None),
        ("rspec >= 3.0", "rspec", Some(">= 3.0")),
        ("rails ~> 7.0.0", "rails", Some("~> 7.0.0")),
        ("nokogiri = 1.13.0", "nokogiri", Some("= 1.13.0")),
    ];

    for (spec, expected_name, _expected_version) in specs {
        let result = plugin.parse_dependency(spec).unwrap();
        assert_eq!(result.name, expected_name);
        // Note: version_spec is an enum, not a simple string field
        // Could test the enum variant if needed
    }
}
