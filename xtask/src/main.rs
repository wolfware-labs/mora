//! Repository tooling, run as `cargo xtask <command>`. Commands arrive with
//! the M0 deliverables that need them.

// Test and tooling code may unwrap (CONTRIBUTING.md, Code rules).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod error_codes;
mod layers;
mod runtime;
mod spec_examples;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cargo_metadata::MetadataCommand;
use mora_diag::registry;

const USAGE: &str =
    "usage: cargo xtask <layers | error-codes | spec-examples | runtime [--test] [target...]>";

/// `cargo metadata --no-deps` for the workspace that contains this crate.
fn metadata_command() -> MetadataCommand {
    let mut command = MetadataCommand::new();
    command.manifest_path(root().join("Cargo.toml")).no_deps();
    command
}

fn run_layers() -> ExitCode {
    let metadata = match metadata_command().exec() {
        Ok(metadata) => metadata,
        Err(err) => {
            eprintln!("layers: cargo metadata failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    let violations = layers::check(&metadata);
    for violation in &violations {
        eprintln!("{violation}");
    }
    if violations.is_empty() {
        println!("layers: {} crates, ok", metadata.workspace_packages().len());
        ExitCode::SUCCESS
    } else {
        eprintln!("layers: {} violation(s)", violations.len());
        ExitCode::FAILURE
    }
}

/// The repository root: the parent of this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn run_error_codes() -> ExitCode {
    let repo = match error_codes::load(&root()) {
        Ok(repo) => repo,
        Err(err) => {
            eprintln!("error-codes: {err}");
            return ExitCode::FAILURE;
        }
    };
    let problems = error_codes::check(&repo, registry::RANGES, registry::CODES);
    for problem in &problems {
        eprintln!("{problem}");
    }
    if problems.is_empty() {
        let pages = repo
            .pages
            .iter()
            .filter(|(file, _)| file != "README.md")
            .count();
        println!(
            "error-codes: {} codes, {pages} pages, {} uses, ok",
            registry::CODES.len(),
            repo.uses.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("error-codes: {} problem(s)", problems.len());
        ExitCode::FAILURE
    }
}

fn run_spec_examples() -> ExitCode {
    let repo = match spec_examples::load(&root()) {
        Ok(repo) => repo,
        Err(err) => {
            eprintln!("spec-examples: {err}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = spec_examples::check(&repo, mora_testkit::compiler());
    for problem in &outcome.problems {
        eprintln!("{problem}");
    }
    if outcome.problems.is_empty() {
        println!(
            "spec-examples: {} examples, {} passed, {} pending, ok",
            repo.examples.len(),
            outcome.passed,
            outcome.pending
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("spec-examples: {} problem(s)", outcome.problems.len());
        ExitCode::FAILURE
    }
}

/// Builds both runtime variants for each target, the host when none is
/// given, and with `--test` checks each with the C test program.
fn run_runtime(args: &[&str]) -> ExitCode {
    let test = args.contains(&"--test");
    let mut targets: Vec<String> = args
        .iter()
        .filter(|arg| **arg != "--test")
        .map(|arg| (*arg).to_owned())
        .collect();
    if targets.is_empty() {
        match runtime::host() {
            Ok(host) => targets.push(host),
            Err(err) => {
                eprintln!("runtime: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
    let root = root();
    for target in &targets {
        for variant in runtime::VARIANTS {
            let result = runtime::build(&root, target, variant).and_then(|built| {
                if test {
                    runtime::test(&root, target, &built)?;
                }
                Ok(built)
            });
            match result {
                Ok(built) => println!(
                    "runtime: {target} {}{} (links {})",
                    variant.file,
                    if test { ", test passed" } else { "" },
                    built.native_libs.join(" ")
                ),
                Err(err) => {
                    eprintln!("runtime: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["layers"] => run_layers(),
        ["error-codes"] => run_error_codes(),
        ["spec-examples"] => run_spec_examples(),
        ["runtime", ref rest @ ..] => run_runtime(rest),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
