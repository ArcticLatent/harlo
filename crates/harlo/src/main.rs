//! Harlo: a desktop app for coding agents run by OpenCode.

// Rust guideline compliant 2026-10-09

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> iced::Result {
    harlo::run()
}
