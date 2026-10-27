use mora_testkit::{Backend, Checked, Compiler, Output};

/// The compiler the suites use until the driver can compile a file (M1).
/// Until then the suites are empty, so it is never called.
pub struct Pending;

impl Compiler for Pending {
    fn check(&self, _: &str, _: &str) -> Checked {
        unimplemented!("the driver cannot compile yet");
    }

    fn backends(&self) -> &[Backend] {
        &[]
    }

    fn run(&self, _: &str, _: &str, _: Backend) -> Result<Output, Checked> {
        unimplemented!("the driver cannot run programs yet");
    }
}

pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
