//! What a file's name says about editing it: its comment markers, whether
//! a colon opens a block, and the indentation it uses.

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or("").to_lowercase()
}

/// The comment markers for a file: `[line]`, `[open, close]` where the
/// language has only block comments, or none where it isn't known.
pub fn comment_for(path: &str) -> Vec<&'static str> {
    let name = name_of(path);
    let hash_names = ["makefile", "gnumakefile", "dockerfile", "containerfile", "cmakelists.txt", ".gitignore", ".dockerignore", ".bashrc", ".zshrc", ".profile", "pkgbuild", "justfile"];
    if hash_names.contains(&name.as_str()) || name.starts_with(".env") {
        return vec!["#"];
    }
    let ext = if name.contains('.') { name.rsplit('.').next().unwrap_or("") } else { "" };
    let by: [(&str, &str); 6] = [
        ("//", "c h cc cpp cxx hpp hh hxx ino m mm rs go js jsx mjs cjs ts tsx mts cts java kt kts swift cs fs dart scala sc php qml proto zig groovy gradle jsonc json5 glsl frag vert wgsl hlsl sol v vala d"),
        ("#", "py pyi pyw sh bash zsh fish ksh rb pl pm r yaml yml toml conf cfg ini nix cmake mk tf tfvars hcl ps1 psm1 jl ex exs nim cr coffee properties gitignore env desktop service timer mount socket rake gemspec podspec bzl star"),
        ("--", "lua sql hs lhs elm ada adb ads"),
        (";", "lisp lsp clj cljs cljc edn el scm ss rkt asm s"),
        ("%", "tex sty cls erl hrl"),
        ("\"", "vim"),
    ];
    for (mark, exts) in by {
        if !ext.is_empty() && exts.split(' ').any(|e| e == ext) {
            return vec![mark];
        }
    }
    if ["html", "htm", "xhtml", "xml", "svg", "md", "markdown", "ui", "qrc", "plist", "xsl", "vue", "svelte"].contains(&ext) {
        return vec!["<!--", "-->"];
    }
    match ext {
        "css" => vec!["/*", "*/"],
        "scss" | "less" => vec!["//"],
        _ => vec![],
    }
}

/// Python and YAML: a line ending in a colon opens a block.
pub fn colon_indents(path: &str) -> bool {
    let name = name_of(path);
    [".py", ".pyi", ".pyw", ".yaml", ".yml"].iter().any(|e| name.ends_with(e))
}

/// One level of indentation: a tab, or the spaces the text indents by (4
/// when it doesn't say).
pub fn detect_indent(path: &str, text: &str) -> String {
    let name = name_of(path);
    if name == "makefile" || name == "gnumakefile" || name.ends_with(".mk") {
        return "\t".into();
    }
    let mut end = text.len().min(400_000);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let (mut tabs, mut spaces, mut prev) = (0usize, 0usize, 0usize);
    let mut steps = [0usize; 9];
    for l in text[..end].split('\n') {
        if l.trim().is_empty() {
            continue;
        }
        if l.starts_with('\t') {
            tabs += 1;
            continue;
        }
        let n = l.bytes().take_while(|&b| b == b' ').count();
        if n > 0 {
            spaces += 1;
        }
        if n > prev && n - prev < steps.len() {
            steps[n - prev] += 1;
        }
        prev = n;
    }
    if tabs > spaces {
        return "\t".into();
    }
    let (mut best, mut seen) = (4, 0);
    for d in [4, 2, 8, 3] {
        if steps[d] > seen {
            best = d;
            seen = steps[d];
        }
    }
    " ".repeat(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_by_name() {
        assert_eq!(comment_for("/a/b.rs"), ["//"]);
        assert_eq!(comment_for("tool.py"), ["#"]);
        assert_eq!(comment_for("Makefile"), ["#"]);
        assert_eq!(comment_for("x.html"), ["<!--", "-->"]);
        assert_eq!(comment_for("x.css"), ["/*", "*/"]);
        assert!(comment_for("x.unknown").is_empty());
        assert!(colon_indents("a.yml") && !colon_indents("a.rs"));
    }

    #[test]
    fn indentation() {
        assert_eq!(detect_indent("a.js", "function f() {\n  x()\n  if (y) {\n    z()\n  }\n}\n"), "  ");
        assert_eq!(detect_indent("a.rs", "fn f() {\n\tx();\n}\n"), "\t");
        assert_eq!(detect_indent("a.rs", "x\n"), "    ");
        assert_eq!(detect_indent("Makefile", "a:\n    b\n"), "\t");
    }
}
