//! Scans every file `git ls-files` lists in a repository and reports what
//! the scanner found: `cargo run -p harlo-okf --example scan -- <repo> [file]`.
//! With a file, prints its units; with `--work`, times the wiki's work list.

// Rust guideline compliant 2026-10-09

use harlo_okf::units;
use std::collections::BTreeMap;
use std::process::Command;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let repo = args.next().unwrap_or_else(|| ".".into());
    if let Some(file) = args.next() {
        if file == "--work" {
            let started = Instant::now();
            let bundle = harlo_okf::bundle::load(std::path::Path::new(&repo)).expect("the wiki reads");
            let ledger = harlo_okf::ledger::read(std::path::Path::new(&repo)).expect("the ledger reads");
            let work = harlo_okf::work::work(&bundle, &ledger);
            println!("{} things to do ({} unowned) in {:?}\n", work.len(), work.unowned.len(), started.elapsed());
            print!("{}", work.report());
            return;
        }
        let text = std::fs::read_to_string(format!("{repo}/{file}")).expect("the file reads");
        for u in units::scan(&file, &text) {
            println!("{:>5}-{:<5} {:9} {}{}{}", u.start, u.end, u.kind.word(), u.name, if u.public { " pub" } else { "" }, if u.test { " test" } else { "" });
        }
        return;
    }
    let out = Command::new("git").args(["-C", &repo, "ls-files", "-z"]).output().expect("git runs");
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout).split('\0').filter(|f| !f.is_empty()).map(str::to_owned).collect();
    let started = Instant::now();
    let mut by_lang: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut empty = Vec::new();
    let mut lines_total = 0;
    for f in &files {
        let Some(lang) = units::lang(f) else { continue };
        let Ok(text) = std::fs::read_to_string(format!("{repo}/{f}")) else { continue };
        lines_total += text.lines().count();
        let found = units::scan(f, &text);
        let e = by_lang.entry(format!("{lang:?}")).or_default();
        e.0 += 1;
        e.1 += found.len();
        e.2 += found.iter().filter(|u| u.public && !u.test).count();
        if found.is_empty() && text.lines().count() > 30 {
            e.3 += 1;
            empty.push(f.clone());
        }
        // A unit running past the file's end, or ending before it starts: a misread.
        for u in &found {
            assert!(u.start <= u.end && u.end <= text.lines().count() + 1, "{f}: {u:?}");
        }
    }
    println!("{} lines scanned in {:?}", lines_total, started.elapsed());
    for (lang, (n, units, public, none)) in by_lang {
        println!("{lang:10} files {n:5} units {units:6} public {public:6} empty(>30 lines) {none}");
    }
    for f in empty.iter().take(30) {
        println!("  no units: {f}");
    }
    // The ledger's snapshot of every file in scope, and its size.
    let started = Instant::now();
    let scoped = harlo_okf::scope::files(std::path::Path::new(&repo));
    let mut ledger = harlo_okf::ledger::Ledger::new();
    let mut notable = 0;
    for f in &scoped {
        let Ok(text) = std::fs::read_to_string(format!("{repo}/{f}")) else { continue };
        let found = units::scan(f, &text);
        notable += found.iter().filter(|u| harlo_okf::ledger::notable(u)).count();
        ledger.files.insert(f.clone(), harlo_okf::ledger::Seen::new(&text, &found));
    }
    let json = serde_json::to_string_pretty(&ledger).expect("serializes");
    let owners = scoped.iter().filter(|f| harlo_okf::scope::needs_owner(f)).count();
    println!("scope: {} files ({owners} need a page), {notable} notable declarations; snapshot {} bytes, {} lines, in {:?}", scoped.len(), json.len(), json.lines().count(), started.elapsed());
}
