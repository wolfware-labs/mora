use mora_diag::codes::{MR0001};
use mora_diag::codes::*;

fn report() -> Code {
    Code::new(42)
}

// Not bypasses: these only end in the patterns.
fn other() -> ErrorCode {
    let _ = barcodes::{};
    ErrorCode::new(barcodes::MR0001)
}
