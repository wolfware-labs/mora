//! Repository tooling, run as `cargo xtask <command>`. Commands arrive with
//! the M0 deliverables that need them.

// Test and tooling code may unwrap (CONTRIBUTING.md, Code rules).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    eprintln!(
        "unknown xtask command: {}",
        command.as_deref().unwrap_or("<none>")
    );
    ExitCode::from(2)
}
