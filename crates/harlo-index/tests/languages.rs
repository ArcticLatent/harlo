//! Each language's symbols, imports, calls and hashes, through the public API.

// Rust guideline compliant 2026-10-09

use harlo_index::{FileIndex, Lang, Symbol, lang, scan};

/// One line per symbol: its footnote word, name, lines and flags.
fn rows(index: &FileIndex) -> Vec<String> {
    index
        .symbols
        .iter()
        .map(|s| format!("{} {} {}-{}{}{}", s.kind, s.name, s.start, s.end, if s.public { " pub" } else { "" }, if s.test { " test" } else { "" }))
        .collect()
}

fn symbol<'a>(index: &'a FileIndex, name: &str) -> &'a Symbol {
    index.symbols.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no {name} in {:?}", index.symbols))
}

/// Each call as `receiver → name`.
fn calls(index: &FileIndex) -> Vec<String> {
    index.calls.iter().map(|c| format!("{}{}", c.receiver.as_deref().map(|r| format!("{r} → ")).unwrap_or_default(), c.name)).collect()
}

fn imports(index: &FileIndex) -> Vec<&str> {
    index.imports.iter().map(|i| i.text.as_str()).collect()
}

/// The fixtures: one file per language, each with the forms its code
/// commonly takes (members, nesting, visibility, tests), and its symbols.
const FIXTURES: &[(&str, &str, &[&str])] = &[
    (
        "Card.qml",
        include_str!("fixtures/Card.qml"),
        &[
            "property run 5-5 pub",
            "property keys 6-6 pub",
            "signal opened 7-7 pub",
            "fn stageColor 9-11 pub",
            "id openAsking 17-22",
            "fn openAsking.press 20-20",
            "id openAsking.hint 21-21",
            "component Badge 24-27 pub",
            "property Badge.count 26-26 pub",
        ],
    ),
    (
        "Cart.cs",
        include_str!("fixtures/Cart.cs"),
        &[
            "mod Shop 4-29 pub",
            "type Shop.Cart 7-17 pub",
            "property Shop.Cart.Count 9-9 pub",
            "fn Shop.Cart.Total 11-14 pub",
            "fn Shop.Cart.Sync 16-16",
            "type Shop.IStore 19-22",
            "fn Shop.IStore.Load 21-21",
            "type Shop.CartTests 24-28 pub",
            "fn Shop.CartTests.Totals 26-27 pub test",
        ],
    ),
    (
        "Cart.kt",
        include_str!("fixtures/Cart.kt"),
        &[
            "type Cart 6-16 pub",
            "fn Cart.total 7-7 pub",
            "fn Cart.sync 9-11",
            "type Cart.Companion 13-15 pub",
            "fn Cart.Companion.empty 14-14 pub",
            "type Registry 18-20 pub",
            "fn Registry.find 19-19 pub",
            "type Store 22-22 pub",
            "fn Store.load 22-22 pub",
            "type Id 24-24 pub",
            "type CartTest 26-29 pub",
            "fn CartTest.totals 27-28 pub test",
        ],
    ),
    (
        "Cart.php",
        include_str!("fixtures/Cart.php"),
        &[
            "type Cart 8-20 pub",
            "fn Cart::total 10-13 pub",
            "fn Cart::sync 15-19",
            "fn helper 22-22 pub",
            "type Store 24-24 pub",
            "fn Store::load 24-24 pub",
            "type Countable 25-25 pub",
        ],
    ),
    (
        "Cart.scala",
        include_str!("fixtures/Cart.scala"),
        &[
            "type Cart 6-12 pub",
            "fn Cart.total 7-7 pub",
            "fn Cart.sync 9-11",
            "type Cart 14-16 pub",
            "fn Cart.empty 15-15 pub",
            "type Store 18-20 pub",
            "fn Store.load 19-19 pub",
            "type Id 22-22 pub",
        ],
    ),
    (
        "Sheet.swift",
        include_str!("fixtures/Sheet.swift"),
        &[
            "type WorkflowSheet 4-14 pub",
            "property WorkflowSheet.body 7-9 pub",
            "fn WorkflowSheet.open 10-12",
            "fn WorkflowSheet.init 13-13 pub",
            "impl Run 16-18 pub",
            "fn Run.make 17-17 pub",
            "type Store 20-22 pub",
            "fn Store.load 21-21 pub",
            "type Id 24-24 pub",
        ],
    ),
    (
        "Shop.java",
        include_str!("fixtures/Shop.java"),
        &[
            "type Cart 6-21 pub",
            "fn Cart.Cart 9-11 pub",
            "fn Cart.total 14-16 pub",
            "fn Cart.sync 18-20",
            "type Store 23-25",
            "fn Store.load 24-24",
            "type Mode 27-27",
            "type Point 29-29",
            "type CartTest 31-34",
            "fn CartTest.totals 32-33 test",
        ],
    ),
    ("build.sh", include_str!("fixtures/build.sh"), &["fn build 5-8 pub", "fn helper 10-12 pub"]),
    ("cart.lua", include_str!("fixtures/cart.lua"), &["fn M.total 6-8 pub", "fn M.sync 10-12 pub", "fn helper 14-16"]),
    (
        "cart.rb",
        include_str!("fixtures/cart.rb"),
        &["mod Shop 5-22 pub", "type Shop.Cart 6-21 pub", "fn Shop.Cart.total 8-10 pub", "fn Shop.Cart.build 12-14 pub", "fn Shop.Cart.sync 18-20"],
    ),
    (
        "ci.yml",
        include_str!("fixtures/ci.yml"),
        &["table name 2-2 pub", "table on 3-5 pub", "table on.push 4-5 pub", "table jobs 7-13 pub", "table jobs.build 8-11 pub", "table jobs.test 12-13 pub"],
    ),
    (
        "list.c",
        include_str!("fixtures/list.c"),
        &["const MAX 4-4 pub", "macro SQUARE 5-5 pub", "type point 8-10 pub", "type pair 12-14 pub", "fn add 16-19", "fn make 21-24 pub"],
    ),
    ("schema.sql", include_str!("fixtures/schema.sql"), &["table users 2-5 pub", "fn total 7-11 pub", "table active 13-13 pub"]),
    (
        "shapes.hpp",
        include_str!("fixtures/shapes.hpp"),
        &[
            "mod geo 3-23 pub",
            "type geo::Shape 6-12 pub",
            "fn geo::Shape::area 8-8 pub",
            "fn geo::Shape::draw 9-9 pub",
            "fn geo::Shape::id 11-11",
            "type geo::Box 14-16 pub",
            "fn geo::Box::area 15-15 pub",
            "fn geo::twice 19-19 pub",
            "type geo::Id 21-21 pub",
            "fn geo::Shape::scale 25-25 pub",
        ],
    ),
    (
        "shop.jl",
        include_str!("fixtures/shop.jl"),
        &[
            "mod Shop 1-30 pub",
            "type Shop.Cart 9-11 pub",
            "type Shop.Store 13-13 pub",
            "fn Shop.total 16-18 pub",
            "fn Shop.area 20-20 pub",
            "macro Shop.twice 22-24 pub",
            "fn Shop.Base.show 26-28 pub",
        ],
    ),
];

#[test]
fn fixture_symbols() {
    for (name, text, want) in FIXTURES {
        let index = scan(name, text).unwrap_or_else(|| panic!("{name} wasn't read"));
        assert_eq!(rows(&index), *want, "{name}");
        assert_eq!(Some(index.lang), lang(name));
        // Every symbol's hash is filled in, and its lines are inside the file.
        let lines = text.lines().count();
        assert!(index.symbols.iter().all(|s| s.hash.len() == 16 && s.start <= s.end && s.end <= lines), "{name}");
    }
    // Every language but the inline ones below has a fixture.
    let covered: Vec<Lang> = FIXTURES.iter().filter_map(|(n, _, _)| lang(n)).collect();
    let inline = [Lang::Rust, Lang::Python, Lang::Go, Lang::JavaScript, Lang::TypeScript, Lang::Tsx];
    assert!(Lang::ALL.iter().all(|l| covered.contains(l) || inline.contains(l)), "{covered:?}");
}

#[test]
fn fixture_details() {
    let java = scan("Shop.java", include_str!("fixtures/Shop.java")).unwrap();
    assert_eq!(symbol(&java, "Cart.total").signature, "public int total(int tax)");
    assert_eq!(symbol(&java, "Cart.total").doc.as_deref(), Some("The total, with tax."));
    assert_eq!(symbol(&java, "CartTest.totals").signature, "void totals()", "annotations are left out");
    assert_eq!(imports(&java), ["import java.util.List"]);
    assert_eq!(calls(&java), ["items.stream() → count", "items → stream", "Math → max", "new Ledger() → post", "Ledger"]);

    let c = scan("list.c", include_str!("fixtures/list.c")).unwrap();
    assert_eq!(symbol(&c, "add").signature, "static int add(int a, int b)");
    assert_eq!(symbol(&c, "pair").signature, "typedef struct pair");
    assert_eq!(symbol(&c, "point").doc.as_deref(), Some("A point."));
    assert_eq!(imports(&c), ["#include <stdio.h>", "#include \"list.h\""]);
    assert_eq!(calls(&c), ["printf", "add"]);

    let cpp = scan("shapes.hpp", include_str!("fixtures/shapes.hpp")).unwrap();
    assert_eq!(symbol(&cpp, "geo::Shape").doc.as_deref(), Some("A shape."));
    assert_eq!(symbol(&cpp, "geo::Shape::scale").signature, "double geo::Shape::scale(double f)");

    let cs = scan("Cart.cs", include_str!("fixtures/Cart.cs")).unwrap();
    assert_eq!(symbol(&cs, "Shop.Cart").doc.as_deref(), Some("A cart."), "XML tags are left out");
    assert_eq!(symbol(&cs, "Shop.CartTests.Totals").signature, "public void Totals()");
    assert_eq!(calls(&cs), ["Items → Sum", "Math → Max", "new Ledger() → Post", "Ledger"]);

    let rb = scan("cart.rb", include_str!("fixtures/cart.rb")).unwrap();
    assert_eq!(symbol(&rb, "Shop").doc.as_deref(), Some("A cart of items."));
    assert_eq!(imports(&rb), ["require \"json\"", "require_relative \"item\""]);
    assert_eq!(calls(&rb), ["items → sum", "helper", "Ledger.new → post", "Ledger → new"]);

    let php = scan("Cart.php", include_str!("fixtures/Cart.php")).unwrap();
    assert_eq!(symbol(&php, "Cart::total").signature, "public function total(int $tax): int");
    assert_eq!(imports(&php), ["use App\\Ledger", "require_once 'helpers.php'"]);
    assert_eq!(calls(&php), ["$this->items → sum", "max", "Ledger → post", "Ledger"]);

    let swift = scan("Sheet.swift", include_str!("fixtures/Sheet.swift")).unwrap();
    assert_eq!(symbol(&swift, "WorkflowSheet").doc.as_deref(), Some("A sheet."));
    assert_eq!(symbol(&swift, "WorkflowSheet.body").signature, "var body: some View");
    assert_eq!(calls(&swift), ["Text", "store → load", "Run"]);

    let kt = scan("Cart.kt", include_str!("fixtures/Cart.kt")).unwrap();
    assert_eq!(symbol(&kt, "Cart.total").signature, "fun total(tax: Int): Int");
    assert_eq!(symbol(&kt, "Cart").doc.as_deref(), Some("A cart."));

    let scala = scan("Cart.scala", include_str!("fixtures/Cart.scala")).unwrap();
    assert_eq!(symbol(&scala, "Cart.total").signature, "def total(tax: Int): Int");
    assert_eq!(calls(&scala), ["max", "Ledger → post"]);

    let jl = scan("shop.jl", include_str!("fixtures/shop.jl")).unwrap();
    assert_eq!(symbol(&jl, "Shop.Cart").doc.as_deref(), Some("A cart of items."));
    assert_eq!(symbol(&jl, "Shop.total").doc.as_deref(), Some("The total, with tax."));
    assert_eq!(symbol(&jl, "Shop.area").signature, "area(r)");
    assert_eq!(calls(&jl), ["sum", "max", "print"], "a definition's own signature isn't a call");
    assert_eq!(imports(&jl), ["using LinearAlgebra", "import Base: show"]);

    let sh = scan("build.sh", include_str!("fixtures/build.sh")).unwrap();
    assert_eq!(symbol(&sh, "build").doc.as_deref(), Some("Builds the app."));
    assert_eq!(imports(&sh), ["source ./lib.sh"]);

    let lua = scan("cart.lua", include_str!("fixtures/cart.lua")).unwrap();
    assert_eq!(symbol(&lua, "M.total").doc.as_deref(), Some("The total."));
    assert_eq!(calls(&lua), ["sum", "math → max", "self.ledger → post", "print"]);
    assert_eq!(imports(&lua), ["require(\"json\")"]);

    let qml = scan("Card.qml", include_str!("fixtures/Card.qml")).unwrap();
    assert_eq!(symbol(&qml, "openAsking").signature, "FlatButton");
    assert_eq!(calls(&qml), ["Theme → colour", "root → opened"]);

    let sql = scan("schema.sql", include_str!("fixtures/schema.sql")).unwrap();
    assert_eq!(symbol(&sql, "users").doc.as_deref(), Some("Users who sign in."));
    assert_eq!(symbol(&sql, "total").signature, "CREATE OR REPLACE FUNCTION total(a int)");
}

/// Editing a plain comment changes no hash; editing code changes its own.
#[test]
fn comments_and_code_in_hashes() {
    let cases = [
        ("a.java", "class A {\n  int f() { return 1; }\n}\n", "class A {\n  int f() { return 1; } // one\n}\n", "class A {\n  int f() { return 2; }\n}\n"),
        ("a.c", "int f(void) {\n  return 1;\n}\n", "int f(void) {\n  /* one */ return 1;\n}\n", "int f(void) {\n  return 2;\n}\n"),
        ("a.rb", "def f\n  1\nend\n", "def f\n  1 # one\nend\n", "def f\n  2\nend\n"),
        ("a.lua", "function f()\n  return 1\nend\n", "function f()\n  return 1 -- one\nend\n", "function f()\n  return 2\nend\n"),
        ("a.sql", "CREATE TABLE t (\n  a int\n);\n", "CREATE TABLE t (\n  a int -- one\n);\n", "CREATE TABLE t (\n  b int\n);\n"),
        ("a.yml", "a:\n  b: 1\n", "a:\n  b: 1 # one\n", "a:\n  b: 2\n"),
    ];
    for (name, before, commented, changed) in cases {
        let hash = |t: &str| scan(name, t).unwrap().symbols.last().unwrap().hash.clone();
        assert_eq!(hash(before), hash(commented), "{name}: a plain comment");
        assert_ne!(hash(before), hash(changed), "{name}: code");
    }
}

const RUST: &str = "use std::fmt;\n\npub const LIMIT: usize = 3;\n\n/// A thing.\n#[derive(Debug)]\npub struct Ledger {\n    pub id: u32,\n}\n\nimpl Ledger {\n    pub fn post(&self) -> u32 {\n        let x = \"}\";\n        x\n    }\n\n    pub(crate) async fn sync(&mut self) {}\n    const fn zero() -> u32 { 0 }\n}\n\nimpl<T: Clone> fmt::Display for Ledger<T>\nwhere\n    T: Copy,\n{\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }\n}\n\nfn helper() {\n    const INNER: u32 = 2;\n    struct Due;\n    impl Drop for Due {\n        fn drop(&mut self) {}\n    }\n}\n\npub trait Store {\n    fn load(&self) -> u32;\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn works() {}\n}\n";

#[test]
fn rust() {
    let index = scan("src/lib.rs", RUST).unwrap();
    assert_eq!(
        rows(&index),
        [
            "const LIMIT 3-3 pub",
            "type Ledger 7-9 pub",
            "impl Ledger 11-19",
            "fn Ledger::post 12-15 pub",
            "fn Ledger::sync 17-17 pub",
            "fn Ledger::zero 18-18",
            "impl Ledger 21-26",
            "fn Ledger::fmt 25-25",
            "fn helper 28-34",
            "type Due 30-30",
            "impl Due 31-33",
            "fn Due::drop 32-32",
            "type Store 36-38 pub",
            "fn Store::load 37-37 pub",
            "mod tests 41-44 test",
            "fn tests::works 43-43 test",
        ]
    );
    assert_eq!(symbol(&index, "Ledger::post").signature, "pub fn post(&self) -> u32");
    assert_eq!(symbol(&index, "Ledger").doc.as_deref(), Some("A thing."));
    assert_eq!(symbol(&index, "Ledger").signature, "pub struct Ledger");
    assert_eq!(symbol(&index, "Ledger::fmt").signature, "fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result");
    assert_eq!(symbol(&index, "Store::load").signature, "fn load(&self) -> u32;");
    let long = "pub fn add(\n    // the first\n    a: u32,\n    b: u32,\n) -> u32 {\n    a + b\n}\n";
    assert_eq!(scan("a.rs", long).unwrap().symbols[0].signature, "pub fn add(a: u32, b: u32) -> u32");
    assert_eq!(imports(&index), ["use std::fmt"]);
    let from: Vec<_> = index.calls.iter().map(|c| (c.name.as_str(), c.line, c.from.map(|f| index.symbols[f].name.as_str()))).collect();
    assert_eq!(from, [("Ok", 25, Some("Ledger::fmt"))]);
    assert_eq!(rows(&scan("lib.rs", "mod a;\npub mod b {}\n").unwrap()), ["mod b 2-2 pub"]);

    let run = "fn run(&self) {\n    let n = self.ledger.post(1);\n    Ledger::new();\n    parse::<u32>(\"1\");\n    a.b().c();\n    (self.f)();\n}\n";
    let index = scan("a.rs", run).unwrap();
    assert_eq!(calls(&index), ["self.ledger → post", "Ledger → new", "parse", "a.b() → c", "a → b"]);
    assert!(index.calls.iter().all(|c| c.from == Some(0)));

    // Re-indenting, a blank line and a plain comment change nothing; `two` changed; its container didn't.
    let before = scan("a.rs", "impl A {\n    fn one() { 1 }\n    fn two() { 2 }\n}\n").unwrap().symbols;
    let after = scan("a.rs", "impl A {\n        fn one() { 1 } // one\n\n    fn two() { 3 }\n}\n").unwrap().symbols;
    assert_eq!(before[0].hash, after[0].hash);
    assert_eq!(before[1].hash, after[1].hash);
    assert_ne!(before[2].hash, after[2].hash);
    // A doc comment is part of the code it's in.
    let doc = scan("a.rs", "impl A {\n    /// One.\n    fn one() { 1 }\n    fn two() { 2 }\n}\n").unwrap().symbols;
    assert_ne!(doc[0].hash, before[0].hash);
}

#[test]
fn python() {
    let py = "import os\nfrom .x import a, b\n\nclass Cart:\n    \"\"\"A cart.\n\n    More.\"\"\"\n\n    def total(self,\n              tax):\n        # plain\n        return self.items.sum(tax)\n\n    @property\n    def _sync(self):\n        pass\n\ndef main():\n    x = Cart()\n    def inner():\n        pass\n\ndef test_cart():\n    pass\n";
    let index = scan("shop.py", py).unwrap();
    assert_eq!(
        rows(&index),
        ["type Cart 4-16 pub", "fn Cart.total 9-12 pub", "fn Cart._sync 15-16", "fn main 18-21 pub", "fn inner 20-21", "fn test_cart 23-24 pub test"]
    );
    assert_eq!(symbol(&index, "Cart").doc.as_deref(), Some("A cart."));
    assert_eq!(symbol(&index, "Cart.total").signature, "def total(self, tax)");
    assert_eq!(imports(&index), ["import os", "from .x import a, b"]);
    let from: Vec<_> = index.calls.iter().map(|c| (c.name.as_str(), c.receiver.as_deref(), c.from.map(|f| index.symbols[f].name.as_str()))).collect();
    assert_eq!(from, [("sum", Some("self.items"), Some("Cart.total")), ("Cart", None, Some("main"))]);
    assert!(rows(&scan("tests/test_shop.py", "def helper():\n    pass\n").unwrap())[0].ends_with(" test"));
}

#[test]
fn go() {
    let go = "package shop\n\nimport (\n\t\"fmt\"\n\tf \"flag\"\n)\n\n// Cart holds items.\n// It's a cart.\ntype Cart struct {\n\tItems []Item\n}\n\n// Total sums the cart.\nfunc (c *Cart) Total() int {\n\treturn fmt.Sprint(c.Items)\n}\n\nfunc helper() {}\n\nconst Max = 3\n";
    let index = scan("cart.go", go).unwrap();
    assert_eq!(rows(&index), ["type Cart 10-12 pub", "fn Cart.Total 15-17 pub", "fn helper 19-19", "const Max 21-21 pub"]);
    assert_eq!(symbol(&index, "Cart").signature, "type Cart struct");
    assert_eq!(symbol(&index, "Cart").doc.as_deref(), Some("Cart holds items."));
    assert_eq!(symbol(&index, "Cart.Total").doc.as_deref(), Some("Total sums the cart."));
    assert_eq!(symbol(&index, "Cart.Total").signature, "func (c *Cart) Total() int");
    assert_eq!(imports(&index), ["import \"fmt\"", "import f \"flag\""]);
    assert_eq!(calls(&index), ["fmt → Sprint"]);
    assert!(rows(&scan("cart_test.go", "package shop\nfunc TestX() {}\n").unwrap())[0].ends_with(" test"));
}

#[test]
fn typescript_and_javascript() {
    let ts = "import x from 'y';\n\n/** Props of a view. */\nexport interface Props {\n  a: string;\n}\n\nexport const handler = async (req) => {\n  if (req) {\n    go(new Thing());\n  }\n};\n\nexport default class View {\n  render() {\n    return null;\n  }\n  private static async load(id: string) {\n  }\n  onClick = () => this.render();\n}\nfunction local() {}\nexport type Id = string;\nexport enum Mode { A, B }\n";
    let index = scan("view.tsx", ts).unwrap();
    assert_eq!(
        rows(&index),
        [
            "type Props 4-6 pub",
            "fn handler 8-12 pub",
            "type View 14-21 pub",
            "fn View.render 15-17 pub",
            "fn View.load 18-19",
            "fn View.onClick 20-20 pub",
            "fn local 22-22",
            "type Id 23-23 pub",
            "type Mode 24-24 pub",
        ]
    );
    assert_eq!(symbol(&index, "Props").doc.as_deref(), Some("Props of a view."));
    assert_eq!(symbol(&index, "handler").signature, "export const handler = async (req) =>");
    assert_eq!(symbol(&index, "View.load").signature, "private static async load(id: string)");
    assert_eq!(imports(&index), ["import x from 'y'"]);
    assert_eq!(calls(&index), ["go", "Thing", "this → render"]);
    let js = "export function a() { b(); }\nclass C { #hidden() {} }\n";
    assert_eq!(rows(&scan("a.test.js", js).unwrap()), ["fn a 1-1 pub test", "type C 2-2 test", "fn C.#hidden 2-2 test"]);
    assert_eq!(rows(&scan("a.ts", "namespace N { export function f() {} }\n").unwrap()), ["mod N 1-1", "fn N.f 1-1"]);
}

#[test]
fn unknown_or_broken_files() {
    assert!(scan("data.csv", "a,b\n").is_none());
    // A syntax error leaves the rest readable.
    let index = scan("a.rs", "fn ok() {}\nfn broken( {\nfn after() {}\n").unwrap();
    assert!(index.symbols.iter().any(|s| s.name == "ok"));
    // An empty file has nothing, in every language.
    for l in Lang::ALL {
        let name = format!(
            "empty.{}",
            match l {
                Lang::Rust => "rs",
                Lang::Python => "py",
                Lang::Go => "go",
                Lang::JavaScript => "js",
                Lang::TypeScript => "ts",
                Lang::Tsx => "tsx",
                Lang::Java => "java",
                Lang::C => "c",
                Lang::Cpp => "cpp",
                Lang::CSharp => "cs",
                Lang::Ruby => "rb",
                Lang::Php => "php",
                Lang::Swift => "swift",
                Lang::Kotlin => "kt",
                Lang::Scala => "scala",
                Lang::Julia => "jl",
                Lang::Bash => "sh",
                Lang::Lua => "lua",
                Lang::Qml => "qml",
                Lang::Yaml => "yml",
                Lang::Sql => "sql",
            }
        );
        assert_eq!(lang(&name), Some(l));
        assert!(scan(&name, "").unwrap().symbols.is_empty(), "{name}");
    }
}
