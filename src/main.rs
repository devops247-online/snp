// SNP (Shell Not Pass) - Main entry point
use clap::Parser;
use snp::cli::Cli;
use std::io::IsTerminal;
use std::process;

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.run() {
        Ok(code) => code,
        Err(e) => {
            // Use error formatter with color detection
            use snp::error::ErrorFormatter;
            use std::io::stderr;

            let use_colors = stderr().is_terminal()
                && std::env::var("NO_COLOR").is_err()
                && std::env::var("TERM").map_or(true, |term| term != "dumb");

            let formatter = ErrorFormatter::new(use_colors);
            eprintln!("{}", formatter.format_error(&e));

            e.exit_code()
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
