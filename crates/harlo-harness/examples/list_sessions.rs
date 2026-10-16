//! Prints every pi/omp session found on disk: `cargo run -p harlo-harness --example list_sessions`.

use harlo_harness::{HarnessKind, store};

fn main() {
    for kind in [HarnessKind::Pi, HarnessKind::Omp] {
        let start = std::time::Instant::now();
        let files = store::list_files(kind);
        let infos: Vec<_> = files.iter().filter_map(|(p, _, _)| store::read_session(kind, p)).collect();
        println!("{}: {} files, {} sessions in {:?}", kind.as_str(), files.len(), infos.len(), start.elapsed());
        for i in infos {
            println!("  {:<40} {:<45} {}", i.cwd.display(), i.display_title().chars().take(45).collect::<String>(), i.size);
        }
    }
}
