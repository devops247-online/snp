// SNP (Shell Not Pass) - Main entry point
use clap::Parser;
use snp::cli::Cli;
use std::process;

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e}");
            1
        }
    };

    process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    // Integration tests are in tests/ directory
    // Unit tests for main are minimal since it's mostly a CLI entry point

    #[test]
    fn test_main_compiles() {
        // This test just ensures main function compiles
        // Actual functionality is tested via integration tests
        // No assertion needed - compilation success is the test
    }
}
