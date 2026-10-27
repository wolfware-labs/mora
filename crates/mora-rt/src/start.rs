//! Process startup (§13.1).

use std::ffi::{c_char, c_int};
use std::io::Write;

unsafe extern "C" {
    /// Emitted by the compiler: runs the program and returns its exit code
    /// (§4.6).
    fn __mora_main() -> i32;
}

/// The process entry symbol. The C runtime of each platform calls `main`.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    // SAFETY: the program defines `__mora_main` with this signature.
    let code = unsafe { __mora_main() };
    // Output the program buffered must not be lost, and nothing is left to
    // report it to if stdout is gone.
    let _ = std::io::stdout().flush();
    code
}
