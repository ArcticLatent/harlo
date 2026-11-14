//! Dev check: the YAML writer over a corpus (JSON lines `{value, out}` on
//! stdin), printing each case that differs.
use std::io::BufRead;

fn main() {
    let (mut n, mut bad) = (0, 0);
    for line in std::io::stdin().lock().lines() {
        let v: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let ours = harlo_wiki::okf::yaml(&v["value"]);
        n += 1;
        if ours != v["out"].as_str().unwrap() {
            bad += 1;
            println!("{}\n  yaml {:?}\n  ours {:?}", v["value"], v["out"].as_str().unwrap(), ours);
        }
    }
    println!("{n} cases, {bad} differ");
}
