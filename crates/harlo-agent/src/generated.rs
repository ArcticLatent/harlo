//! Files a tool makes (generated code, lockfiles): an edit or overwrite of
//! one is refused the first time, since what's changed by hand is lost when
//! it's made again. The refusal says to change what it's made from; the
//! same change made again goes through (the user asked for that very file,
//! or nothing makes it any more).

use std::path::Path;

/// What a package manager keeps up to date, and the manifest it's made
/// from.
const LOCKFILES: &[(&str, &str)] = &[
    ("Cargo.lock", "Cargo.toml (then cargo update or cargo build)"),
    ("package-lock.json", "package.json (then npm install)"),
    ("npm-shrinkwrap.json", "package.json (then npm install)"),
    ("yarn.lock", "package.json (then yarn install)"),
    ("pnpm-lock.yaml", "package.json (then pnpm install)"),
    ("bun.lock", "package.json (then bun install)"),
    ("bun.lockb", "package.json (then bun install)"),
    ("poetry.lock", "pyproject.toml (then poetry lock)"),
    ("uv.lock", "pyproject.toml (then uv lock)"),
    ("Pipfile.lock", "Pipfile (then pipenv lock)"),
    ("Gemfile.lock", "Gemfile (then bundle install)"),
    ("composer.lock", "composer.json (then composer update)"),
    ("go.sum", "go.mod (then go mod tidy)"),
    ("flake.lock", "flake.nix (then nix flake update)"),
    ("mix.lock", "mix.exs (then mix deps.get)"),
    ("Package.resolved", "Package.swift (then swift package resolve)"),
    ("packages.lock.json", "the project file (then dotnet restore)"),
];

/// Names only generators give.
fn generated_name(name: &str) -> bool {
    const ENDS: &[&str] = &[
        ".pb.go", ".pb.cc", ".pb.h", ".pb.c", ".pb.swift", "_pb2.py", "_pb2.pyi", "_pb2_grpc.py", "_grpc.pb.go", ".pb.gw.go",
        ".g.dart", ".freezed.dart",
    ];
    name.starts_with("zz_generated.") || ENDS.iter().any(|e| name.len() > e.len() && name.ends_with(e))
}

/// Why `path` (whose start is `head`) is made by a tool, for the model:
/// None when it isn't.
pub fn refusal(path: &Path, head: &[u8]) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if let Some((_, from)) = LOCKFILES.iter().find(|(n, _)| *n == name) {
        return Some(format!(
            "{} is kept by the package manager: what's changed in it by hand is undone the next time it runs. Change {from} instead. {AGAIN}",
            path.display()
        ));
    }
    let why = if generated_name(name) {
        format!("its name, {name}, is a generator's")
    } else {
        let text = String::from_utf8_lossy(&head[..head.len().min(8 * 1024)]);
        format!("its header says {:?}", marker(&text)?)
    };
    Some(format!(
        "{} is generated ({why}): what's changed in it by hand is lost when it's made again. Change what it's made from (the generator's input or settings) and make it again. {AGAIN}",
        path.display()
    ))
}

const AGAIN: &str = "If this file itself must be changed (the user asked for it, or nothing makes it any more), make the same change again: it will be applied.";

/// The line of the file's opening comments that says it's generated.
fn marker(text: &str) -> Option<String> {
    let header = header(text);
    let lower: Vec<String> = header.iter().map(|l| l.to_lowercase()).collect();
    let all = lower.join(" ");
    let found = |pred: &dyn Fn(&str) -> bool| lower.iter().position(|l| pred(l)).map(|i| header[i].chars().take(120).collect::<String>());
    // `@generated` (Cargo, Poetry, Meta's tools), Go's "Code generated …
    // DO NOT EDIT", and "generated … do not edit" in any words.
    if let Some(m) = found(&|l| l.contains("@generated")) {
        return Some(m);
    }
    if all.contains("code generated") && DO_NOT.iter().any(|d| all.contains(d)) {
        return found(&|l| l.contains("code generated"));
    }
    if all.contains("generated") && DO_NOT.iter().any(|d| all.contains(d)) {
        return found(&|l| DO_NOT.iter().any(|d| l.contains(d))).or_else(|| found(&|l| l.contains("generated")));
    }
    None
}

const DO_NOT: &[&str] = &["do not edit", "don't edit", "do not modify", "don't modify", "not intended for manual editing", "will be overwritten", "will be lost"];

/// The comments a file starts with (after a shebang, a BOM and blank
/// lines), up to 40 lines: where generators say what they are.
fn header(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut open: Option<&str> = None;
    for (i, raw) in text.trim_start_matches('\u{feff}').lines().take(40).enumerate() {
        let l = raw.trim();
        if let Some(end) = open {
            out.push(l);
            if l.contains(end) {
                open = None;
            }
            continue;
        }
        if l.is_empty() || (i == 0 && l.starts_with("#!")) {
            continue;
        }
        let block = [("/*", "*/"), ("<!--", "-->"), ("\"\"\"", "\"\"\""), ("'''", "'''"), ("{-", "-}"), ("(*", "*)")]
            .into_iter()
            .find(|(start, _)| l.starts_with(start));
        if let Some((start, end)) = block {
            out.push(l);
            if !l[start.len()..].contains(end) {
                open = Some(end);
            }
            continue;
        }
        if ["//", "#", "--", ";", "%", "*", "!"].iter().any(|c| l.starts_with(c)) || l.to_ascii_lowercase().starts_with("rem ") {
            out.push(l);
            continue;
        }
        break;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(name: &str, text: &str) -> Option<String> {
        refusal(Path::new(name), text.as_bytes())
    }

    #[test]
    fn knows_generated_files() {
        let go = of("api.go", "// Code generated by protoc-gen-go. DO NOT EDIT.\n// source: api.proto\n\npackage api\n").unwrap();
        assert!(go.contains("its header says \"// Code generated by protoc-gen-go. DO NOT EDIT.\""), "{go}");
        assert!(of("Cargo.lock", "# This file is automatically @generated by Cargo.\n").unwrap().contains("Change Cargo.toml"));
        assert!(of("x/yarn.lock", "").unwrap().contains("kept by the package manager"));
        assert!(of("schema.rs", "// @generated automatically by Diesel CLI.\n\ndiesel::table! {}\n").is_some());
        assert!(of("types.ts", "/**\n * This file was automatically generated by json-schema-to-typescript.\n * DO NOT MODIFY IT BY HAND.\n */\nexport type A = 1;\n").is_some());
        assert!(of("m.py", "#!/usr/bin/env python\n\"\"\"Generated by the build.\n\nManual changes will be overwritten.\n\"\"\"\nimport x\n").is_some());
        assert!(of("thing_pb2.py", "import x\n").unwrap().contains("its name, thing_pb2.py, is a generator's"));
        assert!(of("zz_generated.deepcopy.go", "package v1\n").is_some());
    }

    #[test]
    fn leaves_hand_written_files_alone() {
        // Said only in the code, not its opening comments.
        assert!(of("gen.rs", "//! Writes the bindings.\n\nconst HEADER: &str = \"// @generated DO NOT EDIT\";\n").is_none());
        assert!(of("main.go", "package main\n\n// Code generated by hand. DO NOT EDIT.\n").is_none());
        // Generated once, meant to be edited.
        assert!(of("0001_initial.py", "# Generated by Django 4.2 on 2024-01-01\n\nfrom django.db import migrations\n").is_none());
        assert!(of("README.md", "# Generated docs\n\nEdit me.\n").is_none());
        assert!(of("pb.go", "package x\n").is_none());
        assert!(of("lock.json", "").is_none());
    }
}
