//! Harlo: a desktop app for coding agents run by OpenCode.

// Rust guideline compliant 2026-10-09

use harlo::options::{ArgsError, Options, PASSWORD_VAR, USAGE};
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> iced::Result {
    let password = std::env::var(PASSWORD_VAR).ok();
    match Options::parse(std::env::args().skip(1), password) {
        Ok(options) => harlo::run(options),
        Err(ArgsError::Help) => {
            println!("{USAGE}");
            Ok(())
        }
        Err(ArgsError::Invalid(message)) => {
            eprintln!("harlo: {message}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}
