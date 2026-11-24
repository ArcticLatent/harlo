//! Harlo: a desktop app for coding agents run by the server.

// Rust guideline compliant 2026-10-09

use std::process::ExitCode;

use harlo::options::{ArgsError, Options, PASSWORD_VAR, USAGE};
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let password = std::env::var(PASSWORD_VAR).ok();
    match Options::parse(std::env::args().skip(1), password) {
        Ok(options) => match harlo::run(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("harlo: {e}");
                ExitCode::FAILURE
            }
        },
        Err(ArgsError::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(ArgsError::Version) => {
            println!("harlo {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(ArgsError::Invalid(message)) => {
            eprintln!("harlo: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
