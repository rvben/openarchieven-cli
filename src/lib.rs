//! openarchieven — CLI for the Open Archives genealogical API.
//!
//! Public entry point is [`run`], which reads `std::env::args` and writes to
//! stdout/stderr.

use std::process::ExitCode;

pub fn run() -> ExitCode {
    eprintln!("openarchieven: not yet implemented");
    ExitCode::from(2)
}
