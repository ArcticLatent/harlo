//! Each language's rules that a node's kind and text settle: its grammar,
//! its comments, imports and calls, and which files are tests.

// Rust guideline compliant 2026-10-09

use crate::Lang;
use std::borrow::Cow;
use tree_sitter::{Language, Node};

/// The language's tree-sitter grammar; `None` for those read as plain text.
pub(crate) fn grammar(lang: Lang) -> Option<Language> {
    Some(match lang {
        Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
        Lang::Python => tree_sitter_python::LANGUAGE.into(),
        Lang::Go => tree_sitter_go::LANGUAGE.into(),
        Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Lang::Java => tree_sitter_java::LANGUAGE.into(),
        Lang::C => tree_sitter_c::LANGUAGE.into(),
        Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Lang::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        Lang::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Lang::Swift => tree_sitter_swift::LANGUAGE.into(),
        Lang::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        Lang::Scala => tree_sitter_scala::LANGUAGE.into(),
        Lang::Julia => tree_sitter_julia::LANGUAGE.into(),
        Lang::Bash => tree_sitter_bash::LANGUAGE.into(),
        Lang::Lua => tree_sitter_lua::LANGUAGE.into(),
        Lang::Qml => tree_sitter_qmljs::LANGUAGE.into(),
        Lang::Yaml | Lang::Sql => return None,
    })
}

/// The text a grammar reads for `text`: the same bytes, with what it
/// can't read blanked out.
///
/// C and C++ code is full of macros the grammar can't expand: `Q_OBJECT`
/// alone on a line in a class reads as the start of a field and breaks the
/// rest of the class, and Qt's `signals:` and `public slots:` aren't C++.
/// A line that's only a macro (`Q_OBJECT`, `QML_ELEMENT`,
/// `Q_PROPERTY(int x READ x)`) is blanked, and Qt's section labels read as
/// `public:`. Lengths stay the same, so lines and offsets still match the file.
pub(crate) fn prepare(lang: Lang, text: &str) -> Cow<'_, str> {
    if !matches!(lang, Lang::C | Lang::Cpp) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let t = body.trim();
        let label = matches!(t, "signals:" | "Q_SIGNALS:" | "public slots:" | "public Q_SLOTS:" | "protected slots:" | "private slots:" | "Q_SLOTS:");
        if label {
            let indent = body.len() - body.trim_start().len();
            let word = if t.starts_with("private") {
                "private:"
            } else if t.starts_with("protected") {
                "protected:"
            } else {
                "public:"
            };
            out.push_str(&body[..indent]);
            out.push_str(word);
            out.push_str(&" ".repeat(body.len() - indent - word.len()));
            out.push_str(&line[body.len()..]);
            changed = true;
        } else if lone_macro(t) {
            out.push_str(&" ".repeat(body.len()));
            out.push_str(&line[body.len()..]);
            changed = true;
        } else {
            out.push_str(line);
        }
    }
    if changed { Cow::Owned(out) } else { Cow::Borrowed(text) }
}

/// Whether a line is only a macro: an all-caps name, perhaps with
/// arguments, and no `;` (`Q_OBJECT`, `Q_PROPERTY(int x READ x)`).
fn lone_macro(line: &str) -> bool {
    let name_end = line.find('(').unwrap_or(line.len());
    let name = &line[..name_end];
    let args_ok = name_end == line.len() || line.ends_with(')');
    name.len() > 2
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && name.contains('_')
        && args_ok
}

/// Whether the whole file at `path` is test code, by its name.
pub(crate) fn test_file(lang: Lang, path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name);
    let in_dir = |d: &str| path.starts_with(&format!("{d}/")) || path.contains(&format!("/{d}/"));
    match lang {
        Lang::Rust => in_dir("tests"),
        Lang::Go => name.ends_with("_test.go"),
        Lang::Python => name.starts_with("test_") || stem.ends_with("_test") || name == "conftest.py" || in_dir("tests"),
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => name.contains(".test.") || name.contains(".spec.") || in_dir("__tests__"),
        Lang::Java | Lang::Kotlin | Lang::Scala => in_dir("src/test") || ["Test", "Tests", "Spec", "Suite"].iter().any(|s| stem.ends_with(s)),
        Lang::CSharp => stem.ends_with("Tests") || stem.ends_with("Test") || path.split('/').any(|d| d.ends_with(".Tests") || d == "Tests"),
        Lang::Swift => stem.ends_with("Tests") || path.split('/').any(|d| d.ends_with("Tests")),
        Lang::Ruby => stem.ends_with("_spec") || stem.ends_with("_test") || stem.starts_with("test_") || in_dir("spec") || in_dir("test"),
        Lang::Php => stem.ends_with("Test") || in_dir("tests"),
        Lang::C | Lang::Cpp => stem.ends_with("_test") || stem.starts_with("test_") || in_dir("tests") || in_dir("test"),
        Lang::Julia | Lang::Bash | Lang::Lua => in_dir("test") || in_dir("tests") || stem.ends_with("_spec") || stem.ends_with("_test"),
        Lang::Qml => stem.starts_with("tst_"),
        Lang::Yaml | Lang::Sql => false,
    }
}

/// Whether a node of this kind is a comment.
pub(crate) fn is_comment(lang: Lang, kind: &str) -> bool {
    match lang {
        Lang::Rust | Lang::Java | Lang::Kotlin | Lang::Julia => kind == "line_comment" || kind == "block_comment",
        Lang::Swift => kind == "comment" || kind == "multiline_comment",
        Lang::Scala => kind == "comment" || kind == "block_comment",
        _ => kind == "comment",
    }
}

/// Whether a comment is a doc comment by its own text; `None` for languages
/// where any comment right above a declaration documents it.
pub(crate) fn doc_marker(lang: Lang, text: &str) -> Option<bool> {
    let block = text.starts_with("/**") && !text.starts_with("/***") && text != "/**/";
    Some(match lang {
        Lang::Rust => (text.starts_with("///") && !text.starts_with("////")) || text.starts_with("//!") || block || text.starts_with("/*!"),
        Lang::Swift | Lang::CSharp | Lang::C | Lang::Cpp => (text.starts_with("///") && !text.starts_with("////")) || block,
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx | Lang::Java | Lang::Php | Lang::Kotlin | Lang::Scala | Lang::Qml => block,
        // Python documents with docstrings, which are code.
        Lang::Python => false,
        Lang::Go | Lang::Ruby | Lang::Bash | Lang::Lua | Lang::Julia => return None,
        Lang::Yaml | Lang::Sql => false,
    })
}

/// Whether a language names the declarations inside its functions (Rust's
/// items, Python's nested functions). Elsewhere they're closures: details
/// of the function they're in, not the file's.
pub(crate) fn keeps_nested(lang: Lang) -> bool {
    matches!(lang, Lang::Rust | Lang::Python)
}

/// Whether `node` imports something; `src` is the file's text.
pub(crate) fn is_import(lang: Lang, node: Node<'_>, src: &[u8]) -> bool {
    let kind = node.kind();
    match lang {
        Lang::Rust => kind == "use_declaration" || kind == "extern_crate_declaration",
        Lang::Python => matches!(kind, "import_statement" | "import_from_statement" | "future_import_statement"),
        Lang::Go => kind == "import_declaration",
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => kind == "import_statement",
        Lang::Java | Lang::Scala | Lang::Swift => kind == "import_declaration",
        Lang::Kotlin => kind == "import",
        Lang::C | Lang::Cpp => kind == "preproc_include",
        Lang::CSharp => kind == "using_directive",
        Lang::Php => {
            matches!(kind, "namespace_use_declaration" | "require_expression" | "require_once_expression" | "include_expression" | "include_once_expression")
        }
        Lang::Julia => kind == "import_statement" || kind == "using_statement",
        Lang::Qml => kind == "ui_import",
        // `require "x"`, `source x.sh`, `require("x")`: calls that import.
        Lang::Ruby => {
            kind == "call"
                && node.child_by_field_name("receiver").is_none()
                && node.child_by_field_name("method").is_some_and(|m| matches!(text(m, src), "require" | "require_relative" | "load"))
        }
        Lang::Bash => kind == "command" && node.child_by_field_name("name").is_some_and(|n| matches!(text(n, src), "source" | ".")),
        Lang::Lua => kind == "function_call" && node.child_by_field_name("name").is_some_and(|n| text(n, src) == "require"),
        Lang::Yaml | Lang::Sql => false,
    }
}

/// What a call calls, if `node` is one: the text naming it, or its name and
/// what it's called on when the grammar keeps them apart.
pub(crate) enum Callee<'t> {
    /// `self.a.post`, `Ledger::new`: split at the last separator.
    Path(Node<'t>),
    /// A name and, apart, what it's called on.
    Parts(Node<'t>, Option<Node<'t>>),
    /// A macro's path (`tracing::event`), split like [`Callee::Path`].
    Macro(Node<'t>),
}

pub(crate) fn callee(lang: Lang, node: Node<'_>) -> Option<Callee<'_>> {
    let field = |f: &str| node.child_by_field_name(f);
    let kind = node.kind();
    let path = match lang {
        // A Rust macro's call (`tracing::event!(…)`): named with its `!`.
        Lang::Rust if kind == "macro_invocation" => return Some(Callee::Macro(field("macro")?)),
        Lang::Rust | Lang::Go | Lang::C | Lang::Cpp | Lang::Scala => (kind == "call_expression").then(|| field("function")).flatten()?,
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx | Lang::Qml => match kind {
            "call_expression" => field("function")?,
            "new_expression" => field("constructor")?,
            _ => return None,
        },
        Lang::Python => (kind == "call").then(|| field("function")).flatten()?,
        Lang::Java => match kind {
            "method_invocation" => return Some(Callee::Parts(field("name")?, field("object"))),
            "object_creation_expression" => field("type")?,
            _ => return None,
        },
        Lang::CSharp => match kind {
            "invocation_expression" => field("function")?,
            "object_creation_expression" => field("type")?,
            _ => return None,
        },
        Lang::Ruby => return (kind == "call").then(|| Some(Callee::Parts(field("method")?, field("receiver")))).flatten(),
        Lang::Php => match kind {
            "function_call_expression" => field("function")?,
            "member_call_expression" | "nullsafe_member_call_expression" => return Some(Callee::Parts(field("name")?, field("object"))),
            "scoped_call_expression" => return Some(Callee::Parts(field("name")?, field("scope"))),
            "object_creation_expression" => node.named_children(&mut node.walk()).find(|c| matches!(c.kind(), "name" | "qualified_name"))?,
            _ => return None,
        },
        // Their calls hold the callee as their first child, unnamed.
        Lang::Swift | Lang::Kotlin => (kind == "call_expression").then(|| node.named_child(0)).flatten()?,
        Lang::Julia => match kind {
            "call_expression" | "broadcast_call_expression" if !julia_declares(node) => node.named_child(0)?,
            _ => return None,
        },
        Lang::Bash => return (kind == "command").then(|| Some(Callee::Parts(field("name")?, None))).flatten(),
        Lang::Lua => (kind == "function_call").then(|| field("name")).flatten()?,
        Lang::Yaml | Lang::Sql => return None,
    };
    // `parse::<u32>()`: the function, not its type arguments.
    let path = match path.kind() {
        "generic_function" | "template_function" => path.child_by_field_name("function").or_else(|| path.child_by_field_name("name")).unwrap_or(path),
        _ => path,
    };
    Some(Callee::Path(path))
}

/// Whether a Julia call is what a definition defines: `f(x)` in
/// `function f(x)` or `f(x) = …`, not a call.
fn julia_declares(call: Node<'_>) -> bool {
    let mut at = call;
    while let Some(p) = at.parent() {
        match p.kind() {
            "typed_expression" | "where_expression" => at = p,
            "signature" => return true,
            "assignment" => return p.named_child(0).is_some_and(|l| l.id() == at.id()),
            _ => return false,
        }
    }
    false
}

/// A node's text in `src`.
pub(crate) fn text<'s>(node: Node<'_>, src: &'s [u8]) -> &'s str {
    node.utf8_text(src).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_macros_are_blanked() {
        let text = "class A {\n    Q_OBJECT\n    Q_PROPERTY(int x READ x)\npublic slots:\n    void f();\n    int MAX_SIZE = 3;\n};\n";
        let out = prepare(Lang::Cpp, text);
        assert_eq!(out.len(), text.len());
        assert_eq!(out, "class A {\n            \n                            \npublic:      \n    void f();\n    int MAX_SIZE = 3;\n};\n");
        assert!(matches!(prepare(Lang::Rust, text), Cow::Borrowed(_)));
        assert!(!lone_macro("FOO") && lone_macro("DECLARE_X(a)") && !lone_macro("Q_EMIT x();"));
    }
}
