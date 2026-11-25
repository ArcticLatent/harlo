//! A source file's outline: its functions and types, and the lines each
//! spans.
//!
//! A model that wants one function of a long file doesn't need the file:
//! the outline costs a line for each declaration, and tells it which lines
//! to read. [`of`] reads Rust, Python, JavaScript, TypeScript, Go, Swift,
//! C, C++ and Java with the grammars their editors use, so nesting,
//! strings and comments are followed as the language has them.
//!
//! What's listed is what a file declares at its top and inside its types,
//! modules and namespaces: functions and methods, types, and in Rust
//! constants. The insides of functions aren't looked at. Each line is the
//! declaration up to its body, after the lines it spans and a tab, indented
//! by how deep it sits.

// Rust guideline compliant 2026-10-09

use std::fmt::Write as _;
use std::path::Path;

use tree_sitter::{Language, Node, Parser};

/// The largest file outlined, in bytes: past any source file written by
/// hand, and quick to read.
pub(super) const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// The most declarations listed.
const MAX_LISTED: usize = 500;
/// The longest a declaration is shown.
const MAX_SHOWN: usize = 160;
/// How deep declarations inside declarations are followed.
const MAX_DEPTH: usize = 4;
/// How many nodes that aren't declarations are looked through to reach
/// one: an `export`, a decorator, a block.
const MAX_BETWEEN: usize = 4;

/// The languages outlined, for the message that says so.
pub(super) const LANGUAGES: &str =
    "Rust, Python, JavaScript, TypeScript, Go, Swift, C, C++ and Java";

/// How one language's declarations are told apart.
struct Kinds {
    /// The kinds of node listed.
    listed: &'static [&'static str],
    /// The listed kinds whose insides are listed too.
    holders: &'static [&'static str],
    /// The listed kinds shown only when they take more than a line:
    /// variables, of which only those holding a function matter.
    long_only: &'static [&'static str],
}

/// Rust's.
const RUST: Kinds = Kinds {
    listed: &[
        "function_item",
        "function_signature_item",
        "struct_item",
        "enum_item",
        "union_item",
        "trait_item",
        "impl_item",
        "mod_item",
        "type_item",
        "const_item",
        "static_item",
        "macro_definition",
    ],
    holders: &["impl_item", "trait_item", "mod_item"],
    long_only: &[],
};

/// Python's.
const PYTHON: Kinds = Kinds {
    listed: &["function_definition", "class_definition"],
    holders: &["class_definition"],
    long_only: &[],
};

/// JavaScript's and TypeScript's.
const SCRIPT: Kinds = Kinds {
    listed: &[
        "function_declaration",
        "generator_function_declaration",
        "function_signature",
        "class_declaration",
        "abstract_class_declaration",
        "method_definition",
        "method_signature",
        "abstract_method_signature",
        "interface_declaration",
        "type_alias_declaration",
        "enum_declaration",
        "internal_module",
        "module",
        "lexical_declaration",
        "variable_declaration",
    ],
    holders: &[
        "class_declaration",
        "abstract_class_declaration",
        "interface_declaration",
        "internal_module",
        "module",
    ],
    long_only: &["lexical_declaration", "variable_declaration"],
};

/// Go's.
const GO: Kinds = Kinds {
    listed: &[
        "function_declaration",
        "method_declaration",
        "type_spec",
        "type_alias",
    ],
    holders: &[],
    long_only: &[],
};

/// Swift's.
const SWIFT: Kinds = Kinds {
    listed: &[
        "class_declaration",
        "protocol_declaration",
        "function_declaration",
        "protocol_function_declaration",
        "init_declaration",
        "deinit_declaration",
        "subscript_declaration",
        "typealias_declaration",
    ],
    holders: &["class_declaration", "protocol_declaration"],
    long_only: &[],
};

/// C's and C++'s.
const C_FAMILY: Kinds = Kinds {
    listed: &[
        "function_definition",
        "declaration",
        "field_declaration",
        "struct_specifier",
        "class_specifier",
        "union_specifier",
        "enum_specifier",
        "namespace_definition",
        "type_definition",
        "alias_declaration",
        "concept_definition",
    ],
    holders: &[
        "struct_specifier",
        "class_specifier",
        "namespace_definition",
    ],
    long_only: &[],
};

/// Java's.
const JAVA: Kinds = Kinds {
    listed: &[
        "class_declaration",
        "interface_declaration",
        "enum_declaration",
        "record_declaration",
        "annotation_type_declaration",
        "method_declaration",
        "constructor_declaration",
    ],
    holders: &[
        "class_declaration",
        "interface_declaration",
        "enum_declaration",
        "record_declaration",
    ],
    long_only: &[],
};

/// The language of the file at `path`, by its extension, and how its
/// declarations are told apart.
fn grammar(path: &Path) -> Option<(Language, &'static Kinds)> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "rs" => (tree_sitter_rust::LANGUAGE.into(), &RUST),
        "py" | "pyi" => (tree_sitter_python::LANGUAGE.into(), &PYTHON),
        "js" | "mjs" | "cjs" | "jsx" => (tree_sitter_javascript::LANGUAGE.into(), &SCRIPT),
        "ts" | "mts" | "cts" => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), &SCRIPT),
        "tsx" => (tree_sitter_typescript::LANGUAGE_TSX.into(), &SCRIPT),
        "go" => (tree_sitter_go::LANGUAGE.into(), &GO),
        "swift" => (tree_sitter_swift::LANGUAGE.into(), &SWIFT),
        "c" => (tree_sitter_c::LANGUAGE.into(), &C_FAMILY),
        // (A `.h` may be either; C++ reads both.)
        "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => {
            (tree_sitter_cpp::LANGUAGE.into(), &C_FAMILY)
        }
        "java" => (tree_sitter_java::LANGUAGE.into(), &JAVA),
        _ => return None,
    })
}

/// Whether files named like `path` can be outlined.
pub(super) fn reads(path: &Path) -> bool {
    grammar(path).is_some()
}

/// The outline of `source`, the text of the file at `path`: a line for
/// each declaration; `None` when the language isn't one read here or the
/// file declares nothing.
pub(super) fn of(path: &Path, source: &str) -> Option<String> {
    let (language, kinds) = grammar(path)?;
    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    // (A file with mistakes in it still has an outline: the grammar reads
    // around them.)
    let tree = parser.parse(source, None)?;
    let text = source.as_bytes();
    let mut out = String::new();
    let mut listed = 0usize;
    let mut more = 0usize;
    // Walked with a list, not by recursion, each node with how deep it
    // sits and how many nodes since a declaration: generated code nests
    // as deep as it likes.
    let mut waiting: Vec<(Node<'_>, usize, usize)> = vec![(tree.root_node(), 0, 0)];
    while let Some((node, depth, between)) = waiting.pop() {
        let (inner_depth, inner_between) = if shown(kinds, node) {
            if listed == MAX_LISTED {
                more += 1;
                continue;
            }
            listed += 1;
            let (first, last) = (node.start_position().row + 1, node.end_position().row + 1);
            if first == last {
                let _ = write!(out, "{first}");
            } else {
                let _ = write!(out, "{first}-{last}");
            }
            let _ = writeln!(out, "\t{}{}", "  ".repeat(depth), signature(node, text));
            if !kinds.holders.contains(&node.kind()) || depth + 1 >= MAX_DEPTH {
                continue;
            }
            (depth + 1, 0)
        } else if node.parent().is_none() {
            (depth, 0)
        } else if between < MAX_BETWEEN && !kinds.listed.contains(&node.kind()) {
            (depth, between + 1)
        } else {
            continue;
        };
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
        waiting.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, inner_depth, inner_between)),
        );
    }
    if listed == 0 {
        return None;
    }
    if more > 0 {
        let _ = writeln!(out, "({more} more not listed)");
    }
    Some(out)
}

/// Whether `node` is a declaration the outline lists.
fn shown(kinds: &Kinds, node: Node<'_>) -> bool {
    let kind = node.kind();
    if !kinds.listed.contains(&kind) {
        return false;
    }
    if kinds.long_only.contains(&kind) {
        return node.start_position().row != node.end_position().row;
    }
    match kind {
        // A C declaration is listed when it declares a function; a
        // variable isn't.
        "declaration" | "field_declaration" => declares_function(node),
        // `struct stat *` names a type; only `struct stat { … }` declares it.
        "struct_specifier" | "class_specifier" | "union_specifier" | "enum_specifier" => {
            node.child_by_field_name("body").is_some()
        }
        _ => true,
    }
}

/// Whether C declaration `node` declares a function, through any pointers
/// and references around its name.
fn declares_function(node: Node<'_>) -> bool {
    let mut declarator = node.child_by_field_name("declarator");
    while let Some(inner) = declarator {
        match inner.kind() {
            "function_declarator" => return true,
            "pointer_declarator" | "reference_declarator" | "parenthesized_declarator" => {
                declarator = inner
                    .child_by_field_name("declarator")
                    .or_else(|| inner.named_child(0));
            }
            _ => return false,
        }
    }
    false
}

/// Declaration `node` as the outline shows it: its text up to its body,
/// on one line.
fn signature(node: Node<'_>, source: &[u8]) -> String {
    let start = node.start_byte();
    // What follows the declaration's own words: its body, or the value a
    // constant is given.
    let end = ["body", "value"]
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .map_or_else(|| node.end_byte(), |inside| inside.start_byte());
    let whole = String::from_utf8_lossy(source.get(start..end).unwrap_or_default());
    let one_line = |text: &str| -> String {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        text.trim_end_matches(['{', ':', '=', ' ']).to_owned()
    };
    let mut text = one_line(whole.lines().next().unwrap_or_default());
    if whole.lines().nth(1).is_some() && node.child_by_field_name("body").is_some() {
        // A declaration written over several lines, all of it.
        text = one_line(&whole);
    } else if node.start_position().row != node.end_position().row
        && let Some(name) = ["declarator", "name"]
            .iter()
            .find_map(|field| node.child_by_field_name(field))
            .and_then(|name| name.utf8_text(source).ok())
        && !text.contains(name)
    {
        // `typedef struct {` names its type on its last line.
        text = format!("{text} … {name}");
    }
    // (Go writes `type` once for a group of types.)
    let prefix = if matches!(node.kind(), "type_spec" | "type_alias") {
        "type "
    } else {
        ""
    };
    let mut line: String = format!("{prefix}{text}").chars().take(MAX_SHOWN).collect();
    if text.chars().count() + prefix.len() > MAX_SHOWN {
        line.push('…');
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outline of `source` in a file called `name`.
    fn outline(name: &str, source: &str) -> String {
        of(Path::new(name), source).unwrap_or_default()
    }

    #[test]
    fn rust_lists_items_and_what_impls_and_modules_hold() {
        let source = "\
/// Docs.
pub struct Point {
    x: i32,
}

impl Point {
    pub fn new(
        x: i32,
    ) -> Self {
        let inner = |a: i32| a;
        Self { x: inner(x) }
    }
}

const LIMIT: usize = 4;

mod tests {
    fn checks() {}
}
";
        assert_eq!(
            outline("a.rs", source),
            "2-4\tpub struct Point\n\
             6-13\timpl Point\n\
             7-12\t  pub fn new( x: i32, ) -> Self\n\
             15\tconst LIMIT: usize\n\
             17-19\tmod tests\n\
             18\t  fn checks()\n"
        );
    }

    #[test]
    fn python_lists_classes_methods_and_decorated_functions() {
        let source = "\
import os

class Reader(Base):
    def read(self, n: int) -> bytes:
        def helper():
            pass
        return b''

@cached
def main():
    pass
";
        assert_eq!(
            outline("a.py", source),
            "3-7\tclass Reader(Base)\n\
             4-7\t  def read(self, n: int) -> bytes\n\
             10-11\tdef main()\n"
        );
    }

    #[test]
    fn typescript_lists_exports_and_functions_held_in_variables() {
        let source = "\
const one = 1;
export const load = async (id: string) => {
  return id;
};
export interface Shape {
  area(): number;
}
export class Box implements Shape {
  area(): number {
    return 1;
  }
}
type Id = string;
";
        assert_eq!(
            outline("a.ts", source),
            "2-4\tconst load = async (id: string) =>\n\
             5-7\tinterface Shape\n\
             6\t  area(): number\n\
             8-12\tclass Box implements Shape\n\
             9-11\t  area(): number\n\
             13\ttype Id\n"
        );
        assert!(outline("a.tsx", "function View() {\n  return <b>hi</b>;\n}\n").contains("View"));
        assert!(outline("a.js", "function go() {\n}\n").starts_with("1-2\tfunction go()"));
    }

    #[test]
    fn go_swift_c_and_java_list_their_declarations() {
        let go = "package a\n\ntype Point struct {\n\tX int\n}\n\nfunc (p Point) Len() int {\n\treturn 1\n}\n";
        assert_eq!(
            outline("a.go", go),
            "3-5\ttype Point struct\n7-9\tfunc (p Point) Len() int\n"
        );

        let swift =
            "struct Point {\n    var x: Int\n    func len() -> Int {\n        return 1\n    }\n}\n";
        assert_eq!(
            outline("a.swift", swift),
            "1-6\tstruct Point\n3-5\t  func len() -> Int\n"
        );

        let c = "\
#include <stdio.h>
int count;
typedef struct {
    int x;
} point;
static int len(const point *p);
static int len(const point *p) {
    return p->x;
}
";
        assert_eq!(
            outline("a.c", c),
            "3-5\ttypedef struct … point\n\
             6\tstatic int len(const point *p);\n\
             7-9\tstatic int len(const point *p)\n"
        );

        let cpp =
            "namespace geo {\nclass Point {\npublic:\n    int len() const;\n    int x;\n};\n}\n";
        assert_eq!(
            outline("a.hpp", cpp),
            "1-7\tnamespace geo\n2-6\t  class Point\n4\t    int len() const;\n"
        );

        let java = "class Point {\n    int x;\n    Point(int x) {\n        this.x = x;\n    }\n    int len() {\n        return x;\n    }\n}\n";
        assert_eq!(
            outline("A.java", java),
            "1-9\tclass Point\n3-5\t  Point(int x)\n6-8\t  int len()\n"
        );
    }

    #[test]
    fn other_files_have_no_outline_and_a_long_one_is_cut() {
        assert!(!reads(Path::new("notes.md")));
        assert_eq!(of(Path::new("notes.md"), "# Title\n"), None);
        // Nothing declared.
        assert_eq!(of(Path::new("a.py"), "x = 1\n"), None);
        // A file with a mistake in it is still outlined.
        assert!(outline("a.rs", "fn ok() {}\nfn broken( {\n").contains("fn ok()"));

        let many = "fn f() {}\n".repeat(MAX_LISTED + 7);
        let listed = outline("a.rs", &many);
        assert_eq!(listed.lines().count(), MAX_LISTED + 1);
        assert!(listed.ends_with("(7 more not listed)\n"), "{listed}");

        let wide = format!("fn f({}) {{}}\n", "a: u32, ".repeat(60));
        let shown = outline("a.rs", &wide);
        assert!(shown.trim_end().ends_with('…'), "{shown}");
        assert!(shown.chars().count() < MAX_SHOWN + 10, "{shown}");
    }
}
