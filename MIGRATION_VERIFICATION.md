# Python pre-commit to Rust SNP Migration Verification Checklist

This checklist provides a systematic approach to verify that logic from the Python pre-commit project has been correctly migrated to the Rust SNP implementation.

## Quick Reference Paths

- **Python pre-commit**: `/home/dromix/projects/snp/pre-commit`
- **Rust SNP**: `/home/dromix/projects/snp/snp`

## Phase 1: Foundation Verification ✅ (Mostly Complete)

### CLI Argument Parsing
- [ ] Compare `--help` output between systems
  ```bash
  # Python
  cd /home/dromix/projects/snp/pre-commit && python -m pre_commit --help

  # Rust
  cd /home/dromix/projects/snp/snp && cargo run -- --help
  ```
- [ ] Verify subcommand structure matches
- [ ] Test conflicting argument detection (e.g., `--verbose` + `--quiet`)
- [ ] Verify exit codes for invalid arguments

### Configuration Parsing
- [ ] Test identical `.pre-commit-config.yaml` files in both systems
- [ ] Compare parsed data structures field-by-field
- [ ] Verify stage name transformation (`commit` → `pre-commit`)
- [ ] Test invalid YAML error messages
- [ ] Check regex pattern validation for `files` and `exclude`

### Error Handling
- [ ] Compare error message quality and context
- [ ] Verify exit codes match pre-commit behavior
- [ ] Test error suggestions and help text
- [ ] Ensure structured errors provide actionable information

## Phase 2: Core Logic Verification ❌ (Not Yet Implemented)

### Hook Execution Engine
**Python Reference**: `pre_commit/commands/run.py`, `pre_commit/hook.py`
**Rust Status**: ❌ Not implemented

Critical verification points when implemented:
- [ ] File discovery matches git staged files
- [ ] Hook filtering by stage and file type
- [ ] Subprocess execution and output handling
- [ ] Exit code propagation from hooks
- [ ] Parallel execution behavior

### Repository Management
**Python Reference**: `pre_commit/repository.py`, `pre_commit/store.py`
**Rust Status**: ❌ Not implemented

Verification checklist when implemented:
- [ ] Repository cloning and caching behavior
- [ ] Environment isolation per repository
- [ ] Version resolution and checkout
- [ ] Local repository handling
- [ ] Repository update mechanisms

### File Type Classification
**Python Reference**: `pre_commit/identify_integration.py`, file type detection
**Rust Status**: ❌ Missing entirely

Critical missing functionality:
- [ ] File type detection (python, javascript, etc.)
- [ ] Integration with file type definitions
- [ ] Hook filtering by file types
- [ ] Exclude types functionality

## Phase 3: Language Plugin Verification ❌ (Not Started)

### System Language Plugin
**Python Reference**: `pre_commit/languages/system.py`
**Rust Status**: ❌ Not implemented

### Python Language Plugin
**Python Reference**: `pre_commit/languages/python.py`
**Rust Status**: ❌ Not implemented

Verification points:
- [ ] Virtual environment creation
- [ ] pip package installation
- [ ] Python version management
- [ ] Entry point resolution

### Node.js Language Plugin
**Python Reference**: `pre_commit/languages/node.py`
**Rust Status**: ❌ Not implemented

### Additional Languages
- [ ] Rust (`languages/rust.py`) - 0% implemented
- [ ] Go (`languages/go.py`) - 0% implemented
- [ ] Docker (`languages/docker.py`) - 0% implemented
- [ ] 15+ other languages - 0% implemented

## Configuration Schema Verification

### Hook Field Validation
Compare validation between:
- **Python**: `pre_commit/clientlib.py:192-501` (cfgv schema)
- **Rust**: `src/config.rs:28-47` (serde structs)

**Test Cases**:
```yaml
# Test empty required fields
repos:
- repo: https://github.com/test/test
  rev: v1.0.0
  hooks:
  - id: ""  # Should fail
    entry: test
    language: system

# Test invalid regex patterns
- id: test-regex
  entry: test
  language: system
  files: "[invalid"  # Should fail

# Test invalid stages
- id: test-stage
  entry: test
  language: system
  stages: [invalid-stage]  # Should fail
```

### Configuration File Scenarios
Test both systems with:
- [ ] Empty configuration file
- [ ] Missing required fields
- [ ] Invalid YAML syntax
- [ ] Complex nested configurations
- [ ] Local repositories
- [ ] Meta repositories
- [ ] Repository without rev field

## Command Implementation Verification

### Run Command
**Python**: `pre_commit/commands/run.py`
**Rust**: `src/cli.rs` (stub only)

When implemented, verify:
- [ ] File filtering logic matches
- [ ] Hook execution order
- [ ] Output formatting
- [ ] Exit code behavior
- [ ] `--all-files` vs staged files
- [ ] `--show-diff-on-failure` behavior

### Install/Uninstall Command
**Python**: `pre_commit/commands/install_uninstall.py`
**Rust**: `src/cli.rs` (stub only)

When implemented, verify:
- [ ] Git hook script generation
- [ ] Hook type support (pre-commit, pre-push, etc.)
- [ ] Permission handling
- [ ] Overwrite behavior
- [ ] Uninstall cleanup

### Autoupdate Command
**Python**: `pre_commit/commands/autoupdate.py`
**Rust**: ❌ Not implemented

## Git Integration Verification

### Staged File Detection
**Python**: Uses `git diff --cached --name-only --diff-filter=ACMR`
**Rust Status**: ❌ Not implemented

### Repository Operations
**Python**: `pre_commit/git.py` and repository management
**Rust Status**: ❌ Not implemented

Critical areas to verify:
- [ ] Git command execution
- [ ] Working directory management
- [ ] Branch/tag resolution
- [ ] Shallow clones for performance

## Testing Verification

### Test Coverage Comparison
- **Python**: Comprehensive test suite with 100+ test files
- **Rust**: Basic test coverage (107 tests total)

### Test Structure Verification
- [ ] Compare unit test approaches
- [ ] Verify integration test patterns
- [ ] Check error condition testing
- [ ] Validate edge case coverage

## Performance Verification

When core features are implemented:
- [ ] Startup time comparison
- [ ] Memory usage comparison
- [ ] File processing throughput
- [ ] Parallel execution efficiency

## Migration Progress Tracking

### Completed ✅
- [x] CLI structure with clap
- [x] Configuration parsing with serde
- [x] Comprehensive error handling
- [x] Core data structures
- [x] Basic validation logic

### In Progress 🔄
None currently active

### Critical Missing ❌
- [ ] Hook execution engine (P0)
- [ ] Language plugin system (P0)
- [ ] Git integration (P0)
- [ ] Repository management (P0)
- [ ] File type classification (P1)
- [ ] Environment caching (P1)
- [ ] Command implementations (P1)

## Implementation Priority

### Phase 1: Core Execution (P0 - Required for basic functionality)
1. Hook execution engine
2. System language plugin
3. Git staged file detection
4. Basic repository cloning

### Phase 2: Essential Features (P1 - Required for production use)
1. File type classification system
2. Environment management and caching
3. Python language plugin
4. Install/uninstall commands

### Phase 3: Full Compatibility (P2 - Feature parity)
1. All remaining language plugins
2. Autoupdate functionality
3. Advanced git operations
4. Performance optimizations

## Usage Examples for Verification

### Test Configuration Parsing
```bash
# Create test config
cat > test-config.yaml << 'EOF'
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    language: python
    entry: black --check --diff
    types: [python]
EOF

# Test Python pre-commit
cd /home/dromix/projects/snp/pre-commit
python -m pre_commit validate-config test-config.yaml

# Test Rust SNP
cd /home/dromix/projects/snp/snp
cargo run -- validate-config test-config.yaml
```

### Test Error Handling
```bash
# Test invalid config
echo "invalid: yaml: [" > invalid.yaml

# Compare error messages
python -m pre_commit validate-config invalid.yaml
cargo run -- validate-config invalid.yaml
```

This checklist should be updated as development progresses and new features are implemented in the Rust SNP project.
