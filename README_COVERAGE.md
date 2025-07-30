# SNP Test Coverage

This document describes the test coverage setup and measurement for the SNP project.

## Coverage Tools Setup

The project is configured to use `cargo-llvm-cov` for code coverage measurement. This tool provides accurate coverage data for Rust projects.

### Installation

```bash
cargo install cargo-llvm-cov
```

## Coverage Scripts

### Quick Coverage Check
For a quick coverage measurement during development:

```bash
./scripts/quick_coverage.sh
```

### Full Coverage Report
For detailed coverage analysis with HTML reports:

```bash
./scripts/measure_coverage.sh
```

### CI Coverage Check
For automated testing in CI/CD pipelines:

```bash
./scripts/coverage_ci.sh
```

## Coverage Commands

### Basic Coverage
```bash
cargo llvm-cov --all-features --lib --bin --text
```

### HTML Coverage Report
```bash
cargo llvm-cov --all-features --html --output-dir coverage-html
```

### LCOV Report (for CI integration)
```bash
cargo llvm-cov --all-features --lcov --output-path coverage.lcov
```

### Coverage with Test Exclusion
```bash
cargo llvm-cov --all-features --ignore-filename-regex 'tests/.*\.rs' --text
```

## Coverage Target

The project aims for **95% test coverage** as specified in the comprehensive testing plan.

## Test Structure

### Comprehensive Test Suite
The project includes extensive test coverage across multiple categories:

#### High-Priority Coverage Areas (Completed)
- ✅ Enhanced regex processor tests with multi-tier caching
- ✅ Error recovery system tests with reliability mechanisms
- ✅ File change detection system tests with filesystem events
- ✅ Cache integration tests with real workloads
- ✅ Event system tests with comprehensive edge cases
- ✅ Storage tests with concurrent access patterns
- ✅ Error recovery integration tests with component interaction
- ✅ Resource exhaustion tests under memory/disk/FD pressure
- ✅ Performance regression tests with benchmarking

#### Existing Test Coverage
- Core functionality tests (562 unit tests)
- Language plugin tests for Python, Node.js, Rust, Go, Ruby, Docker
- CLI command tests for all major commands
- Integration tests for end-to-end workflows
- Concurrency and work-stealing scheduler tests
- File locking and cross-platform tests
- Security and validation tests

## Coverage Analysis

### Running Coverage Analysis

1. **Clean previous coverage data:**
   ```bash
   cargo llvm-cov clean
   ```

2. **Run tests with coverage:**
   ```bash
   cargo llvm-cov --all-features --workspace --text
   ```

3. **Generate reports:**
   ```bash
   # HTML report
   cargo llvm-cov --html --output-dir coverage-html

   # LCOV report
   cargo llvm-cov --lcov --output-path coverage.lcov
   ```

### Viewing Coverage Reports

- **Text Summary:** Displayed in terminal output
- **HTML Report:** Open `coverage-html/index.html` in browser
- **LCOV Report:** Use with coverage visualization tools

### Coverage Exclusions

The coverage measurement excludes:
- Test files (`tests/**/*.rs`)
- Benchmark files (`benches/**/*.rs`)
- Generated code
- Example code

## Coverage Improvement

### Target Areas for Improvement

When coverage is below the 95% target:

1. **Core Module Coverage:** Focus on uncovered functions in `src/core.rs`
2. **Error Handling Paths:** Add tests for error scenarios and edge cases
3. **Integration Testing:** Add more end-to-end workflow tests
4. **Concurrency Testing:** Test concurrent access patterns thoroughly
5. **Edge Case Testing:** Cover boundary conditions and corner cases

### Adding New Tests

When adding new tests for coverage improvement:

1. **Identify Gaps:** Use HTML coverage report to find uncovered code
2. **Focus on Core Logic:** Prioritize business-critical code paths
3. **Test Error Paths:** Ensure error handling code is tested
4. **Add Integration Tests:** Test component interactions
5. **Verify Coverage Impact:** Re-run coverage to measure improvement

## CI Integration

### GitHub Actions Example

```yaml
name: Coverage
on: [push, pull_request]
jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Install cargo-llvm-cov
        run: cargo install cargo-llvm-cov
      - name: Run coverage
        run: ./scripts/coverage_ci.sh
      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          file: coverage.lcov
```

## Troubleshooting

### Common Issues

1. **Failing Tests:** Some tests may be flaky (e.g., file locking tests)
   - Use `--ignore-filename-regex` to exclude problematic tests
   - Fix flaky tests or mark them as `#[ignore]`

2. **Long Test Runtime:** Full test suite may take significant time
   - Use `--lib` and `--bin` flags to focus on specific targets
   - Run coverage on subsets during development

3. **Missing Coverage:** Some code may not be covered
   - Check for dead code or unreachable branches
   - Add targeted tests for specific functions
   - Review test completeness in core modules

### Performance Considerations

- Coverage measurement adds overhead to test execution
- Use quick coverage checks during development
- Full coverage analysis for CI and releases
- Parallel test execution may affect timing-dependent tests

## Future Enhancements

- Integration with coverage visualization tools
- Automated coverage regression detection
- Per-module coverage targets
- Coverage trend analysis over time
