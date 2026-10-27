//! The `tests/ui` suite: `cargo test -p mora-testkit --test ui -- [--bless] [filter...]`.

use std::path::Path;
use std::process::ExitCode;

use mora_testkit::compiler;
use mora_testkit::harness::{Options, ui_suite};

fn main() -> ExitCode {
    let options = Options::parse(std::env::args().skip(1));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    match ui_suite(compiler(), &root, "tests/ui", &options) {
        Ok(report) => report.finish("ui"),
        Err(error) => {
            eprintln!("cannot read tests/ui: {error}");
            ExitCode::FAILURE
        }
    }
}
