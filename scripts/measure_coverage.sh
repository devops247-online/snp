#!/bin/bash

# SNP Test Coverage Measurement Script
# This script runs cargo-llvm-cov to measure test coverage and generate reports

set -e

echo "=== SNP Test Coverage Measurement ==="

# Clean previous coverage data
cargo llvm-cov clean

echo "Running coverage analysis..."

# Run coverage with detailed output
cargo llvm-cov \
    --all-features \
    --workspace \
    --ignore-filename-regex '(tests/.*\.rs|benches/.*\.rs)' \
    --lcov --output-path coverage.lcov \
    --html --output-dir coverage-html \
    --text --output-path coverage.txt

echo ""
echo "=== Coverage Summary ==="
cat coverage.txt

echo ""
echo "=== Coverage Reports Generated ==="
echo "- LCOV format: coverage.lcov"
echo "- HTML report: coverage-html/index.html"
echo "- Text summary: coverage.txt"

echo ""
echo "=== Opening HTML Coverage Report ==="
if command -v xdg-open &> /dev/null; then
    xdg-open coverage-html/index.html
elif command -v open &> /dev/null; then
    open coverage-html/index.html
else
    echo "HTML report available at: file://$(pwd)/coverage-html/index.html"
fi

echo ""
echo "=== Quick Coverage Analysis ==="

# Extract coverage percentage from the text output
if [ -f coverage.txt ]; then
    COVERAGE_PERCENT=$(grep -o '[0-9.]*%' coverage.txt | tail -1 | sed 's/%//')
    echo "Current coverage: ${COVERAGE_PERCENT}%"

    # Set target coverage
    TARGET_COVERAGE=95.0

    if (( $(echo "$COVERAGE_PERCENT >= $TARGET_COVERAGE" | bc -l) )); then
        echo "✅ Coverage target of ${TARGET_COVERAGE}% achieved!"
    else
        echo "❌ Coverage of ${COVERAGE_PERCENT}% is below target of ${TARGET_COVERAGE}%"
        echo "   Gap: $(echo "$TARGET_COVERAGE - $COVERAGE_PERCENT" | bc -l)%"

        # Suggest next steps
        echo ""
        echo "=== Coverage Improvement Suggestions ==="
        echo "1. Run 'cargo llvm-cov --html' to see detailed coverage report"
        echo "2. Focus on uncovered functions and branches in core modules"
        echo "3. Add integration tests for end-to-end workflows"
        echo "4. Add edge case tests for error handling paths"
        echo "5. Add property-based tests for complex algorithms"
    fi
else
    echo "Warning: Could not parse coverage percentage from coverage.txt"
fi

echo ""
echo "=== Coverage measurement complete ==="
