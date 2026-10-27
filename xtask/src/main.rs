//! Repository tooling, run as `cargo xtask <command>`. Commands arrive with
//! the M0 deliverables that need them.

use std::process::ExitCode;

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    eprintln!(
        "unknown xtask command: {}",
        command.as_deref().unwrap_or("<none>")
    );
    ExitCode::from(2)
}
