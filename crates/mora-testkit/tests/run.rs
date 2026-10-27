//! The `tests/run` suite: `cargo test -p mora-testkit --test run -- [filter...]`.

use std::process::ExitCode;

use mora_testkit::harness::{Options, run_suite};

mod pending;

fn main() -> ExitCode {
    let options = Options::parse(std::env::args().skip(1));
    if options.bless {
        eprintln!("run tests have no snapshots; --bless applies to the ui suite");
        return ExitCode::FAILURE;
    }
    match run_suite(&pending::Pending, &pending::root(), "tests/run", &options) {
        Ok(report) => report.finish("run"),
        Err(error) => {
            eprintln!("cannot read tests/run: {error}");
            ExitCode::FAILURE
        }
    }
}
