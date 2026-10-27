//! Records the commit `mora version` prints: `MORA_COMMIT` when set, as a
//! release build sets it, or else the checkout's `HEAD`.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-env-changed=MORA_COMMIT");
    let commit = std::env::var("MORA_COMMIT")
        .ok()
        .or_else(head)
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo::rustc-env=MORA_COMMIT={commit}");
}

/// The short hash of `HEAD`, rebuilt when `HEAD` or the branch it names moves.
fn head() -> Option<String> {
    let git = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.git");
    if git.is_dir() {
        println!("cargo::rerun-if-changed={}", git.join("HEAD").display());
        if let Ok(head) = std::fs::read_to_string(git.join("HEAD"))
            && let Some(branch) = head.trim().strip_prefix("ref: ")
        {
            println!("cargo::rerun-if-changed={}", git.join(branch).display());
            println!(
                "cargo::rerun-if-changed={}",
                git.join("packed-refs").display()
            );
        }
    }
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !hash.is_empty()).then_some(hash)
}
