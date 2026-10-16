//! Prints usage stats for all sessions: `cargo run -p harlo-core --example usage_stats --release [days]`.

fn main() {
    let days: u32 = std::env::args().nth(1).and_then(|d| d.parse().ok()).unwrap_or(30);
    let t = std::time::Instant::now();
    let files = harlo_core::stats::all_sessions();
    let cold = t.elapsed();
    let t = std::time::Instant::now();
    let again = harlo_core::stats::all_sessions();
    let warm = t.elapsed();
    let r = harlo_core::stats::report(&again, days, &[]);
    println!("{} files: cold {cold:?}, cached {warm:?}", files.len());
    let mut r2 = r.clone();
    r2["daily"] = serde_json::json!(format!("{} days", r["daily"].as_array().map_or(0, |d| d.len())));
    r2["models"] = serde_json::json!(r["models"].as_array().map(|m| m.iter().take(4).cloned().collect::<Vec<_>>()));
    r2["tools"] = serde_json::json!(r["tools"].as_array().map(|m| m.iter().take(6).cloned().collect::<Vec<_>>()));
    println!("{}", serde_json::to_string_pretty(&r2).unwrap());
}
