//! The call graph on small repositories, one language at a time.

// Rust guideline compliant 2026-10-09

use harlo_index::{FileIndex, Graph, scan};
use std::collections::BTreeMap;

fn repo(files: &[(&str, &str)]) -> BTreeMap<String, FileIndex> {
    files.iter().map(|(p, t)| ((*p).to_owned(), scan(p, t).expect(p))).collect()
}

/// Each call in `from` (a full name) as `name → target names (sure|possible)`.
fn calls_of(files: &BTreeMap<String, FileIndex>, from: &str) -> Vec<String> {
    let g = Graph::new(files);
    let at = g.find(from, None);
    assert_eq!(at.len(), 1, "{from}: {at:?}");
    let mut out: Vec<String> = Vec::new();
    let edges = g.callees(at[0]);
    let mut by_site: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for e in &edges {
        by_site.entry(e.site).or_default().push(e);
    }
    for (site, es) in by_site {
        let (_, call) = g.site(site);
        let mut names: Vec<String> = es.iter().map(|e| format!("{}:{}", g.path(e.to), g.symbol(e.to).name)).collect();
        names.sort();
        out.push(format!("{} → {} ({})", call.name, names.join(" "), if es[0].sure { "sure" } else { "possible" }));
    }
    out
}

#[test]
fn rust() {
    let files = repo(&[
        (
            "src/ledger.rs",
            "pub struct Ledger;\nimpl Ledger {\n    pub fn new() -> Ledger { Ledger }\n    pub fn post(&self) { self.check(); }\n    fn check(&self) {}\n}\npub fn open() -> Ledger { Ledger::new() }\npub fn audit() {}\n",
        ),
        ("src/report.rs", "pub struct Report;\nimpl Report {\n    pub fn post(&self) {}\n}\n"),
        (
            "src/orders.rs",
            "use crate::ledger::open;\npub fn place(ledger: &crate::ledger::Ledger) {\n    let report_log = open();\n    report_log.post();\n    let audit = || 1;\n    audit();\n    ledger.post();\n    crate::ledger::open();\n    helper();\n    println!(\"x\");\n    let v = Vec::new();\n}\nfn helper() {}\n",
        ),
    ]);
    assert_eq!(calls_of(&files, "Ledger::post"), ["check → src/ledger.rs:Ledger::check (sure)"]);
    assert_eq!(calls_of(&files, "open"), ["new → src/ledger.rs:Ledger::new (sure)"]);
    assert_eq!(
        calls_of(&files, "place"),
        [
            "open → src/ledger.rs:open (sure)",
            // `report_log`'s type isn't written: a `post` of a type named like it.
            // (`audit()` calls the closure, not `ledger::audit`: not recorded.)
            "post → src/report.rs:Report::post (possible)",
            // `ledger` reads as `Ledger`.
            "post → src/ledger.rs:Ledger::post (sure)",
            "open → src/ledger.rs:open (sure)",
            "helper → src/orders.rs:helper (sure)",
        ]
    );
    // `Vec::new` and `println!` are the library's: not in the graph.
    let g = Graph::new(&files);
    let new = g.find("Ledger::new", None)[0];
    assert_eq!(g.callers(new).len(), 1);
}

#[test]
fn python() {
    let files = repo(&[
        (
            "shop/ledger.py",
            "class Ledger:\n    def post(self, e):\n        self.check(e)\n\n    def check(self, e):\n        pass\n\n\ndef total(entries):\n    return sum(entries)\n",
        ),
        (
            "shop/orders.py",
            "from shop import ledger\nfrom shop.ledger import total\n\n\ndef place(order):\n    ledger_book = ledger.Ledger()\n    ledger_book.post(order)\n    ledger.total([])\n    total([])\n",
        ),
    ]);
    assert_eq!(calls_of(&files, "Ledger.post"), ["check → shop/ledger.py:Ledger.check (sure)"]);
    assert_eq!(
        calls_of(&files, "place"),
        [
            "Ledger → shop/ledger.py:Ledger (sure)",
            // `ledger_book`: no type written; a `post` of a type named like it.
            "post → shop/ledger.py:Ledger.post (possible)",
            "total → shop/ledger.py:total (sure)",
            "total → shop/ledger.py:total (sure)",
        ]
    );
}

#[test]
fn go() {
    let files = repo(&[
        (
            "cart/cart.go",
            "package cart\n\ntype Cart struct{}\n\nfunc (c *Cart) Total() int { return c.sum() }\n\nfunc (c *Cart) sum() int { return weigh() }\n",
        ),
        ("cart/weigh.go", "package cart\n\nfunc weigh() int { return 1 }\n"),
        ("main.go", "package main\n\nimport \"shop/cart\"\n\nfunc main() { cart.New() }\n"),
    ]);
    assert_eq!(calls_of(&files, "Cart.Total"), ["sum → cart/cart.go:Cart.sum (sure)"]);
    // Same package, another file.
    assert_eq!(calls_of(&files, "Cart.sum"), ["weigh → cart/weigh.go:weigh (sure)"]);
}

#[test]
fn java_implied_this() {
    let files = repo(&[(
        "src/Cart.java",
        "class Cart {\n    int total() { return sum() + this.tax(); }\n    int sum() { return 0; }\n    int tax() { return 0; }\n}\nclass Other {\n    int sum() { return 1; }\n}\n",
    )]);
    assert_eq!(calls_of(&files, "Cart.total"), ["sum → src/Cart.java:Cart.sum (sure)", "tax → src/Cart.java:Cart.tax (sure)"]);
}

#[test]
fn rust_macro_arguments() {
    let files = repo(&[(
        "src/lib.rs",
        "macro_rules! note { ($e:expr) => { $e }; }\npub fn total() -> u32 { 1 }\npub fn check() {\n    assert_eq!(total(), 1);\n    println!(\"{}\", crate::total());\n    note!(x.y);\n    let v = vec![total()];\n}\n",
    )]);
    assert_eq!(
        calls_of(&files, "check"),
        ["total → src/lib.rs:total (sure)", "total → src/lib.rs:total (sure)", "note! → src/lib.rs:note (sure)", "total → src/lib.rs:total (sure)"]
    );
}

#[test]
fn lookup_cards_and_map() {
    use harlo_index::{Query, cards, code_map, lookup};
    let files = repo(&[
        (
            "src/ledger.rs",
            "/// Books entries.\npub struct Ledger;\nimpl Ledger {\n    /// Posts an entry.\n    pub fn post(&self) { self.check(); }\n    fn check(&self) {}\n}\n",
        ),
        ("src/orders.rs", "use crate::ledger::Ledger;\npub fn place(ledger: &Ledger) {\n    ledger.post();\n}\npub fn ship(ledger: &Ledger) {\n    place(ledger);\n}\n"),
        ("src/main.rs", "fn main() {\n    let l = crate::ledger::Ledger;\n    crate::orders::ship(&l);\n    crate::orders::place(&l);\n}\n"),
        ("tests/ledger.rs", "#[test]\nfn posts(ledger: &Ledger) {\n    ledger.post();\n}\n"),
    ]);
    let g = Graph::new(&files);
    let card = lookup(&g, &Query { name: "Ledger.post", path: None, depth: 2 });
    assert_eq!(
        card,
        "fn Ledger::post  src/ledger.rs:5-5\n  pub fn post(&self)\n  Posts an entry.\n  calls: Ledger::check (src/ledger.rs:6)\n  called by:\n    place (src/orders.rs:3)\n      main (src/main.rs:4)\n      ship (src/orders.rs:6)\n  tests: maybe posts (tests/ledger.rs:3)"
    );
    // A folder without its `/`; a name it doesn't know lists names like it.
    assert!(lookup(&g, &Query { name: "place", path: Some("src"), depth: 1 }).starts_with("fn place  src/orders.rs:2-4"));
    assert!(lookup(&g, &Query { name: "place", path: Some("tests"), depth: 1 }).starts_with("Nothing named place in tests/"));
    assert_eq!(lookup(&g, &Query { name: "pos", path: None, depth: 1 }), "Nothing named pos; names like it:\n  fn Ledger::post  src/ledger.rs:5\n  fn posts  tests/ledger.rs:2");
    // The writer's brief: the entry point, the most called; a declaration's card.
    let map = code_map(&g, &[]);
    assert!(map.contains("Entry points:\n- main (src/main.rs:1)") && map.contains("- place (src/orders.rs:2), 2"), "{map}");
    let c = cards(&g, &[("src/ledger.rs".into(), "Ledger::post".into()), ("src/x.rs".into(), "gone".into())]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].callers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["place"], "tests left out");
    assert_eq!(c[0].callees[0].name, "Ledger::check");
}
