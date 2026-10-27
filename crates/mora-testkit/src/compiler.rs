use mora_diag::{Diagnostic, Files};
use mora_span::FileId;

/// What the harnesses need from a compiler. The driver implements it once it
/// can compile; the self-tests implement it with a fake.
///
/// A panic in any method fails only the case being run.
pub trait Compiler: Sync {
    /// Compiles `text` as the file `path` and returns what it reported.
    fn check(&self, path: &str, text: &str) -> Checked;

    /// The backends [`Compiler::run`] supports, empty until code generation
    /// exists. A run test goes through each of them.
    fn backends(&self) -> &[Backend];

    /// Builds the program and runs it with `backend`, or returns the
    /// diagnostics that stopped the build.
    fn run(&self, path: &str, text: &str, backend: Backend) -> Result<Output, Checked>;
}

/// The diagnostics of one compilation and the files they point into.
#[derive(Debug)]
pub struct Checked {
    pub files: Files,
    /// The file compiled; its diagnostics are matched against annotations.
    pub file: FileId,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Jit,
    Aot,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Jit => "jit",
            Self::Aot => "aot",
        }
    }
}

/// What a program printed and how it exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub stdout: String,
    pub exit: i32,
}
