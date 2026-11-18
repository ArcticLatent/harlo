//! What each language's nodes declare: a symbol's kind, its name, whether
//! it's public and whether it's a test.
//!
//! The names follow harlo's wiki footnotes (see the crate docs). Where a
//! grammar keeps a declaration's name in a `name` field, that's the name;
//! the rest are found here (a C function's declarator, a Julia signature,
//! a QML object's `id`).

// Rust guideline compliant 2026-10-09

use crate::Kind;
use crate::Lang;
use crate::scan::{Def, Walk};
use tree_sitter::Node;

/// The kinds of node that hold a declaration's body, where the grammar has
/// no `body` field for it.
const BODIES: [&str; 12] = [
    "class_body",
    "function_body",
    "enum_class_body",
    "protocol_body",
    "enum_body",
    "declaration_list",
    "field_declaration_list",
    "interface_body",
    "template_body",
    "compound_statement",
    "ui_object_initializer",
    "block",
];

impl Walk<'_> {
    /// The declaration `node` makes, if any.
    pub(crate) fn def<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        match self.lang {
            Lang::Rust => self.rust(node),
            Lang::Python => self.python(node),
            Lang::Go => self.go(node),
            Lang::JavaScript | Lang::TypeScript | Lang::Tsx => self.script(node),
            Lang::Java => self.java(node),
            Lang::C | Lang::Cpp => self.c_like(node),
            Lang::CSharp => self.csharp(node),
            Lang::Ruby => self.ruby(node),
            Lang::Php => self.php(node),
            Lang::Swift => self.swift(node),
            Lang::Kotlin => self.kotlin(node),
            Lang::Scala => self.scala(node),
            Lang::Julia => self.julia(node),
            Lang::Bash => (node.kind() == "function_definition").then(|| self.named(node, Kind::Function, true)).flatten(),
            Lang::Lua => self.lua(node),
            Lang::Qml => self.qml(node),
            Lang::Yaml | Lang::Sql => None,
        }
    }

    /// A declaration named by its `name` field.
    fn named<'t>(&self, node: Node<'t>, kind: Kind, public: bool) -> Option<Def<'t>> {
        let name = self.text(node.child_by_field_name("name")?).trim();
        def_as(node, kind, name, public)
    }

    /// The text of `node`'s first child of one of `kinds`.
    fn child_text(&self, node: Node<'_>, kinds: &[&str]) -> Option<&str> {
        node.children(&mut node.walk()).find(|c| kinds.contains(&c.kind())).map(|c| self.text(c))
    }

    fn rust<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let public = has_child(node, "visibility_modifier");
        let mut def = match node.kind() {
            "function_item" | "function_signature_item" => self.named(node, Kind::Function, public),
            "struct_item" | "enum_item" | "union_item" | "trait_item" | "type_item" => self.named(node, Kind::Type, public),
            // `mod a;` only says where a module is: its file is the module.
            "mod_item" if node.child_by_field_name("body").is_some() => self.named(node, Kind::Module, public),
            "macro_definition" => self.named(node, Kind::Macro, public),
            "const_item" | "static_item" => self.named(node, Kind::Constant, public).filter(|d| !d.name.starts_with('_')),
            "impl_item" => def_as(node, Kind::Impl, type_name(self.text(node.child_by_field_name("type")?)), false),
            _ => None,
        }?;
        def.test = (def.kind == Kind::Module && def.name == "tests") || self.rust_test_attr(node);
        Some(def)
    }

    /// Whether a Rust item has a test attribute (`#[test]`, `#[cfg(test)]`, `#[tokio::test]`).
    fn rust_test_attr(&self, node: Node<'_>) -> bool {
        let mut prev = node.prev_sibling();
        while let Some(p) = prev {
            match p.kind() {
                "attribute_item" if words(self.text(p)).any(|w| w == "test" || w == "rstest") => return true,
                "attribute_item" | "line_comment" | "block_comment" => {}
                _ => return false,
            }
            prev = p.prev_sibling();
        }
        false
    }

    fn python<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let kind = match node.kind() {
            "function_definition" => Kind::Function,
            "class_definition" => Kind::Type,
            _ => return None,
        };
        let mut def = self.named(node, kind, true)?;
        def.public = !def.name.starts_with('_');
        def.test = (kind == Kind::Function && def.name.starts_with("test")) || (kind == Kind::Type && def.name.starts_with("Test"));
        Some(def)
    }

    fn go<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let kind = match node.kind() {
            "function_declaration" | "method_declaration" => Kind::Function,
            "type_spec" | "type_alias" => Kind::Type,
            "const_spec" => Kind::Constant,
            _ => return None,
        };
        let mut def = self.named(node, kind, false)?;
        def.public = def.name.starts_with(|c: char| c.is_uppercase());
        if node.kind() == "method_declaration" {
            let receiver = self.text(node.child_by_field_name("receiver")?).trim_matches(['(', ')']);
            let owner = type_name(receiver.split_whitespace().last()?);
            def.name = format!("{owner}.{}", def.name);
        }
        Some(def)
    }

    fn script<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let kind = match node.kind() {
            "function_declaration" | "generator_function_declaration" | "method_definition" | "abstract_method_signature" => Kind::Function,
            "class_declaration" | "abstract_class_declaration" | "interface_declaration" | "type_alias_declaration" | "enum_declaration" => Kind::Type,
            "internal_module" | "module" => Kind::Module,
            // `const handler = async (req) => …`, and a class field holding one.
            "variable_declarator" | "public_field_definition" | "field_definition" => {
                let value = node.child_by_field_name("value")?;
                if !matches!(value.kind(), "arrow_function" | "function_expression" | "function" | "generator_function") {
                    return None;
                }
                let name = node.child_by_field_name(if node.kind() == "field_definition" { "property" } else { "name" })?;
                if !matches!(name.kind(), "identifier" | "property_identifier" | "private_property_identifier") {
                    return None;
                }
                // The statement holding it: `export const a = …` starts at `export`.
                let statement = if node.kind() == "variable_declarator" { node.parent().unwrap_or(node) } else { node };
                let outer = exported(statement).unwrap_or(statement);
                let public = self.member_public(node, self.text(name)) && (node.kind() != "variable_declarator" || outer.kind() == "export_statement");
                let mut def = def_as(node, Kind::Function, self.text(name), public)?;
                def.outer = outer;
                def.body = value.child_by_field_name("body");
                return Some(def);
            }
            _ => return None,
        };
        let mut def = self.named(node, kind, false)?;
        def.outer = exported(node).unwrap_or(node);
        def.public = if matches!(node.kind(), "method_definition" | "abstract_method_signature") {
            self.member_public(node, &def.name)
        } else {
            def.outer.kind() == "export_statement"
        };
        Some(def)
    }

    /// A class member's own visibility: not `private` or `protected`, not `#name`.
    fn member_public(&self, node: Node<'_>, name: &str) -> bool {
        let hidden = node.children(&mut node.walk()).any(|c| c.kind() == "accessibility_modifier" && self.text(c) != "public");
        !hidden && !name.starts_with('#')
    }

    fn java<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let kind = match node.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration" | "annotation_type_declaration" => Kind::Type,
            "method_declaration" | "constructor_declaration" | "compact_constructor_declaration" => Kind::Function,
            _ => return None,
        };
        let mut def = self.named(node, kind, false)?;
        let mods = self.child_text(node, &["modifiers"]).unwrap_or("");
        let in_interface = node.parent().is_some_and(|p| matches!(p.kind(), "interface_body" | "annotation_type_body"));
        def.public = words(mods).any(|w| w == "public") || (in_interface && !words(mods).any(|w| w == "private"));
        def.test = ["@Test", "@ParameterizedTest", "@RepeatedTest"].iter().any(|a| mods.contains(a));
        Some(def)
    }

    fn csharp<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let kind = match node.kind() {
            "class_declaration" | "struct_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration" | "delegate_declaration" => {
                Kind::Type
            }
            "method_declaration" | "constructor_declaration" => Kind::Function,
            "property_declaration" => Kind::Property,
            "namespace_declaration" => Kind::Module,
            _ => return None,
        };
        let mut def = self.named(node, kind, false)?;
        let mut cursor = node.walk();
        let mods: Vec<&str> = node.children(&mut cursor).filter(|c| c.kind() == "modifier").map(|c| self.text(c)).collect();
        let in_interface = node.parent().and_then(|p| p.parent()).is_some_and(|p| p.kind() == "interface_declaration");
        def.public = kind == Kind::Module || mods.contains(&"public") || (in_interface && !mods.contains(&"private"));
        def.test = node
            .children(&mut node.walk())
            .filter(|c| c.kind() == "attribute_list")
            .any(|a| words(self.text(a)).any(|w| matches!(w, "Test" | "TestMethod" | "Fact" | "Theory" | "TestCase")));
        Some(def)
    }

    fn c_like<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let cpp = self.lang == Lang::Cpp;
        let field = |f: &str| node.child_by_field_name(f);
        let member = if cpp { self.cpp_access(node) } else { None };
        let not_static = !node.children(&mut node.walk()).any(|c| c.kind() == "storage_class_specifier" && self.text(c) == "static");
        match node.kind() {
            "function_definition" => {
                let name = self.declarator_name(field("declarator")?)?;
                def_as(node, Kind::Function, &name, member.unwrap_or(not_static))
            }
            // A definition, not a mention: `struct point { … }`, not `struct point p;`.
            "struct_specifier" | "union_specifier" | "enum_specifier" | "class_specifier" if field("body").is_some() => {
                self.named(node, Kind::Type, member.unwrap_or(true))
            }
            "type_definition" => {
                let name = self.declarator_name(field("declarator")?)?;
                let mut def = def_as(node, Kind::Type, &name, true)?;
                // `typedef struct { … } pair;` reads `typedef struct pair`.
                let ty = self.text(field("type")?);
                def.signature = Some(format!("typedef {} {name}", ty.split('{').next().unwrap_or(ty).trim()));
                Some(def)
            }
            "preproc_function_def" => self.named(node, Kind::Macro, true),
            "preproc_def" => self.named(node, Kind::Constant, true),
            "namespace_definition" if cpp => self.named(node, Kind::Module, true),
            "alias_declaration" | "concept_definition" if cpp => self.named(node, Kind::Type, member.unwrap_or(true)),
            _ => None,
        }
    }

    /// The name a C declarator declares, through its pointers and parameters.
    fn declarator_name(&self, mut d: Node<'_>) -> Option<String> {
        // Declarators nest a level per `*`, `&`, `()` or `[]`: a few deep at most.
        for _ in 0..16 {
            match d.kind() {
                "identifier" | "field_identifier" | "qualified_identifier" | "destructor_name" | "operator_name" | "type_identifier" => {
                    return Some(self.text(d).to_owned());
                }
                _ => d = d.child_by_field_name("declarator").or_else(|| d.named_child(0))?,
            }
        }
        None
    }

    /// Whether a C++ class member is public (`None`: not a member).
    fn cpp_access(&self, node: Node<'_>) -> Option<bool> {
        let at = node.parent().filter(|p| p.kind() == "template_declaration").map_or(node, |_| node.parent().unwrap_or(node));
        let list = at.parent().filter(|p| p.kind() == "field_declaration_list")?;
        let mut prev = at.prev_sibling();
        while let Some(p) = prev {
            if p.kind() == "access_specifier" {
                return Some(self.text(p).starts_with("public"));
            }
            prev = p.prev_sibling();
        }
        // A struct's members are public until it says otherwise; a class's aren't.
        Some(list.parent().is_none_or(|s| s.kind() != "class_specifier"))
    }

    fn ruby<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let kind = match node.kind() {
            "class" => Kind::Type,
            "module" => Kind::Module,
            "method" | "singleton_method" => Kind::Function,
            _ => return None,
        };
        let mut def = self.named(node, kind, true)?;
        if kind == Kind::Function {
            // `private` on its own line hides the methods after it.
            let mut prev = node.prev_named_sibling();
            while let Some(p) = prev {
                if p.kind() == "identifier" && matches!(self.text(p), "private" | "protected" | "public") {
                    def.public = self.text(p) == "public";
                    break;
                }
                prev = p.prev_named_sibling();
            }
            def.test = def.name.starts_with("test_");
        }
        Some(def)
    }

    fn php<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let mut def = match node.kind() {
            "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration" => self.named(node, Kind::Type, true),
            "function_definition" | "method_declaration" => self.named(node, Kind::Function, true),
            "namespace_definition" if node.child_by_field_name("body").is_some() => self.named(node, Kind::Module, true),
            _ => None,
        }?;
        if node.kind() == "method_declaration" {
            def.public = self.child_text(node, &["visibility_modifier"]).is_none_or(|v| v == "public");
            def.test = def.name.starts_with("test");
        }
        Some(def)
    }

    fn swift<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let mods = self.child_text(node, &["modifiers"]).unwrap_or("");
        let public = !words(mods).any(|w| w == "private" || w == "fileprivate");
        let mut def = match node.kind() {
            "class_declaration" => {
                let extension = node.child_by_field_name("declaration_kind").is_some_and(|k| self.text(k) == "extension");
                if extension {
                    def_as(node, Kind::Impl, type_name(self.text(node.child_by_field_name("name")?)), public)
                } else {
                    self.named(node, Kind::Type, public)
                }
            }
            "protocol_declaration" | "typealias_declaration" => self.named(node, Kind::Type, public),
            "function_declaration" | "protocol_function_declaration" => self.named(node, Kind::Function, public),
            "init_declaration" => def_as(node, Kind::Function, "init", public),
            // A computed property, which is code: `var body: some View { … }`.
            "property_declaration" if node.child_by_field_name("computed_value").is_some() || has_child(node, "computed_property") => {
                let name = self.text(node.child_by_field_name("name")?);
                def_as(node, Kind::Property, name, public)
            }
            _ => None,
        }?;
        def.test = test_name(&def);
        Some(def)
    }

    fn kotlin<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let mods = self.child_text(node, &["modifiers"]).unwrap_or("");
        let public = !words(mods).any(|w| matches!(w, "private" | "internal" | "protected"));
        let mut def = match node.kind() {
            "class_declaration" | "object_declaration" => self.named(node, Kind::Type, public),
            "companion_object" => self.named(node, Kind::Type, public).or_else(|| def_as(node, Kind::Type, "Companion", public)),
            "function_declaration" => self.named(node, Kind::Function, public),
            "type_alias" => def_as(node, Kind::Type, self.child_text(node, &["identifier", "type_identifier", "simple_identifier"])?, public),
            _ => None,
        }?;
        def.test = mods.contains("@Test");
        Some(def)
    }

    fn scala<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let mods = self.child_text(node, &["modifiers", "access_modifier"]).unwrap_or("");
        let public = !words(mods).any(|w| w == "private" || w == "protected");
        match node.kind() {
            "class_definition" | "trait_definition" | "object_definition" | "enum_definition" | "type_definition" => self.named(node, Kind::Type, public),
            "function_definition" | "function_declaration" => self.named(node, Kind::Function, public),
            "package_object" => self.named(node, Kind::Module, public),
            _ => None,
        }
    }

    fn julia<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        let (kind, name) = match node.kind() {
            "module_definition" => (Kind::Module, self.text(node.child_by_field_name("name")?)),
            "struct_definition" | "abstract_definition" | "primitive_definition" => (Kind::Type, self.text(first_of(node.named_child(0)?, &["identifier"])?)),
            "function_definition" | "macro_definition" => {
                let kind = if node.kind() == "macro_definition" { Kind::Macro } else { Kind::Function };
                let signature = node.children(&mut node.walk()).find(|c| c.kind() == "signature")?;
                (kind, self.julia_callee(signature)?)
            }
            // `area(r) = π * r^2`, at the top of a file or module.
            "assignment" if node.parent().is_some_and(|p| matches!(p.kind(), "source_file" | "block" | "module_definition")) => {
                let left = node.named_child(0)?;
                let left = if left.kind() == "where_expression" { left.named_child(0)? } else { left };
                if left.kind() != "call_expression" {
                    return None;
                }
                (Kind::Function, self.text(left.named_child(0)?))
            }
            _ => return None,
        };
        let mut def = def_as(node, kind, name, !name.starts_with('_'))?;
        if node.kind() == "assignment" {
            def.body = node.named_child(1);
        }
        Some(def)
    }

    /// The function a Julia signature defines (`f(x::Int)::Int where T` → `f`).
    fn julia_callee(&self, signature: Node<'_>) -> Option<&str> {
        let call = first_of(signature, &["call_expression"])?;
        let name = call.named_child(0)?;
        Some(self.text(name))
    }

    fn lua<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        if node.kind() != "function_declaration" {
            return None;
        }
        let name = self.text(node.child_by_field_name("name")?).replace(':', ".");
        def_as(node, Kind::Function, &name, !has_child(node, "local"))
    }

    fn qml<'t>(&self, node: Node<'t>) -> Option<Def<'t>> {
        match node.kind() {
            "ui_property" => self.named(node, Kind::Property, true),
            "ui_signal" => self.named(node, Kind::Signal, true),
            "function_declaration" => self.named(node, Kind::Function, true),
            "ui_inline_component" => self.named(node, Kind::Component, true),
            // An object with an `id`, below the file's or a component's root object.
            "ui_object_definition" if node.parent().is_some_and(|p| !matches!(p.kind(), "program" | "ui_inline_component")) => {
                let init = node.child_by_field_name("initializer")?;
                let id = init
                    .named_children(&mut init.walk())
                    .find(|b| b.kind() == "ui_binding" && b.child_by_field_name("name").is_some_and(|n| self.text(n) == "id"))?;
                let id = self.text(id.child_by_field_name("value")?).trim().trim_end_matches(';').trim_end();
                let mut def = def_as(node, Kind::Object, id, false)?;
                def.body = Some(init);
                Some(def)
            }
            _ => None,
        }
    }
}

/// A declaration named `name`.
fn def_as<'t>(node: Node<'t>, kind: Kind, name: &str, public: bool) -> Option<Def<'t>> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let body = node.child_by_field_name("body").or_else(|| node.children(&mut node.walk()).find(|c| BODIES.contains(&c.kind())));
    Some(Def { kind, name: name.to_owned(), outer: node, body, public, test: false, signature: None })
}

/// Whether a function is an XCTest-style test (`func testX()`).
fn test_name(def: &Def<'_>) -> bool {
    def.kind == Kind::Function && def.name.starts_with("test")
}

/// The declaration exported by an `export` statement holding `node`.
fn exported(node: Node<'_>) -> Option<Node<'_>> {
    node.parent().filter(|p| p.kind() == "export_statement")
}

fn has_child(node: Node<'_>, kind: &str) -> bool {
    node.children(&mut node.walk()).any(|c| c.kind() == kind)
}

/// The first node of one of `kinds` in `node` (itself included), depth first.
fn first_of<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut stack = vec![node];
    let mut cursor = node.walk();
    while let Some(n) = stack.pop() {
        if kinds.contains(&n.kind()) {
            return Some(n);
        }
        stack.extend(n.named_children(&mut cursor).collect::<Vec<_>>().into_iter().rev());
    }
    None
}

/// The words in `text` (runs of letters, digits and `_`).
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty())
}

/// The last path segment of a type written in `s` (`&mut fmt::Display<T>` → `Display`).
fn type_name(s: &str) -> &str {
    let s = s.trim().trim_start_matches(['&', '*']).trim_start();
    let s = s.strip_prefix("mut ").unwrap_or(s);
    let s = s.strip_prefix("dyn ").unwrap_or(s).trim_start();
    // Up to its type arguments, keeping its path (`a::b::C<T>` → `a::b::C`).
    let end = s.find(['<', '[', '{', '(', ' ', ';']).unwrap_or(s.len());
    let head = s[..end].trim_end_matches(':');
    let head = head.rsplit("::").next().unwrap_or(head);
    head.rsplit('.').next().unwrap_or(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names() {
        assert_eq!(type_name("Ledger<T>"), "Ledger");
        assert_eq!(type_name("qobject::CodeView"), "CodeView");
        assert_eq!(type_name("&mut fmt::Display"), "Display");
        assert_eq!(type_name("*Cart"), "Cart");
        assert_eq!(type_name("Store[T]"), "Store");
        assert_eq!(words("@Test public void").collect::<Vec<_>>(), ["Test", "public", "void"]);
    }
}
