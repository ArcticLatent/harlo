//! `harlo-helper`: what harlo runs beside pi, here and on other machines.
//!
//! - `sandbox describe|danger|read|write|search|run …`: harlo's sandbox and
//!   its check for commands that can't be undone ([`guard`]), for harlo's
//!   extension for pi;
//! - `wiki --okf` (a request on standard input), `--check <dir>`,
//!   `--validate [<dir>] [--strict] [--json]`: the `.okf/` wiki of a
//!   repository on this machine, and its read-only check for CI;
//! - `--version`.

// Rust guideline compliant 2026-11-19

mod danger;
mod guard;
mod paths;
mod sandbox;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sandbox") => match guard::command(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("harlo-helper: {e:#}");
                ExitCode::FAILURE
            }
        },
        Some("wiki") => wiki(&args[1..]),
        Some("--version" | "-V") => {
            println!("harlo-helper {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("harlo-helper sandbox … | wiki --okf | --check <dir> | --validate [<dir>] [--strict] [--json] | --version");
            ExitCode::from(2)
        }
    }
}

/// `wiki …`: harlo's requests of a wiki on this machine, or its check.
fn wiki(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--validate") {
        return ExitCode::from(validate(args));
    }
    let request = if args.iter().any(|a| a == "--okf") {
        let mut input = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
        serde_json::from_str(&input).unwrap_or_default()
    } else if let Some(dir) = args.iter().skip_while(|a| *a != "--check").nth(1) {
        let root = repository_root(Path::new(dir));
        serde_json::json!({ "op": "check", "root": root.to_string_lossy(), "options": { "now": utc_now() } })
    } else {
        eprintln!("harlo-helper wiki --okf (a request on stdin) | --check <dir> | --validate [<dir>] [--strict] [--json]");
        return ExitCode::from(2);
    };
    println!("{}", harlo_okf::view::answer(&request));
    ExitCode::SUCCESS
}

/// `wiki --validate [DIR] [--strict] [--json]`: the wiki of the repository
/// DIR (default: here) is in, checked for CI with nothing written.
///
/// Prints the report; 0 when it passes, 1 when it fails (errors; with
/// `--strict`, anything the wiki owes the code too), 2 when there's no wiki
/// to check.
fn validate(args: &[String]) -> u8 {
    let strict = args.iter().any(|a| a == "--strict");
    let as_json = args.iter().any(|a| a == "--json");
    let dir = args.iter().skip_while(|a| *a != "--validate").skip(1).find(|a| !a.starts_with("--")).map_or(".", String::as_str);
    let root = repository_root(&std::path::absolute(dir).unwrap_or_else(|_| PathBuf::from(dir)));
    match harlo_okf::validate::validate(&root) {
        Ok(report) => {
            if as_json {
                println!("{}", report.json(strict));
            } else {
                print!("{}", report.text(strict));
            }
            u8::from(!report.passes(strict))
        }
        Err(why) => {
            eprintln!("harlo-helper wiki --validate: {why}");
            2
        }
    }
}

/// Now in UTC, as ISO 8601 (`2026-11-18T04:05:06Z`).
fn utc_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    iso(secs)
}

/// Seconds since 1970 as ISO 8601 in UTC (the civil date from the day
/// count, proleptic Gregorian).
fn iso(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// The repository `dir` is in (the folder holding `.git`), else `dir`.
fn repository_root(dir: &Path) -> PathBuf {
    dir.ancestors().find(|d| d.join(".git").exists()).unwrap_or(dir).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_utc_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso(1_794_974_706), "2026-11-18T04:05:06Z");
    }
}
