use std::env;
use std::process::Command;

fn main() {
    // Set build date
    let build_date = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    println!("cargo:rustc-env=BUILD_DATE={build_date}");

    // Set Rust version
    let rustc_version = env::var("RUSTC_VERSION").unwrap_or_else(|_| {
        String::from_utf8(
            Command::new("rustc")
                .arg("--version")
                .output()
                .map(|output| output.stdout)
                .unwrap_or_default(),
        )
        .unwrap_or_default()
        .trim()
        .to_string()
    });
    println!("cargo:rustc-env=RUST_VERSION={rustc_version}");

    // Set git commit hash
    let git_commit = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .map(|output| output.stdout)
            .unwrap_or_default(),
    )
    .unwrap_or_else(|_| "unknown".to_string())
    .trim()
    .to_string();
    println!("cargo:rustc-env=GIT_COMMIT={git_commit}");

    // Set git branch
    let git_branch = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .map(|output| output.stdout)
            .unwrap_or_default(),
    )
    .unwrap_or_else(|_| "unknown".to_string())
    .trim()
    .to_string();
    println!("cargo:rustc-env=GIT_BRANCH={git_branch}");

    // Tell cargo to re-run this build script if any of these change
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
