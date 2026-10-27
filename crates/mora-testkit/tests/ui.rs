//! The `tests/ui` suite: `cargo test -p mora-testkit --test ui -- [--bless] [filter...]`.

use std::process::ExitCode;

use mora_testkit::harness::{Options, ui_suite};

mod pending;

fn main() -> ExitCode {
    let options = Options::parse(std::env::args().skip(1));
    match ui_suite(&pending::Pending, &pending::root(), "tests/ui", &options) {
        Ok(report) => report.finish("ui"),
        Err(error) => {
            eprintln!("cannot read tests/ui: {error}");
            ExitCode::FAILURE
        }
    }
}
