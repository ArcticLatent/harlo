//! A folder's files as a tree, and how a file opens.

// Rust guideline compliant 2026-10-09

use std::collections::HashSet;

use harlo_core::files::{Kind, Listing, MAX_OPEN, Tree, classify, parent_of, syntax_of};
use harlo_core::server::FileMatch;

fn entry(path: &str, directory: bool) -> FileMatch {
    FileMatch {
        path: path.into(),
        directory,
    }
}

fn visible(tree: &Tree) -> Vec<String> {
    tree.rows()
        .into_iter()
        .map(|row| {
            let mark = match (row.node.directory, row.open) {
                (true, true) => "v ",
                (true, false) => "> ",
                (false, _) => "",
            };
            format!("{}{mark}{}", "  ".repeat(row.depth), row.node.name)
        })
        .collect()
}

#[test]
fn folders_come_first_and_open_to_their_own_entries() {
    let mut tree = Tree::new();
    assert_eq!(tree.root(), None);
    let root = tree.load("");
    assert_eq!(root, "");
    assert_eq!(tree.root(), Some(&Listing::Loading));
    let ignored: HashSet<String> = ["target".to_owned()].into();
    tree.listed(
        "",
        Ok(vec![
            entry("b.txt", false),
            entry(".git", true),
            entry("src", true),
            entry("README.md", false),
            entry("target", true),
            entry("a.txt", false),
        ]),
        &ignored,
    );
    assert_eq!(
        visible(&tree),
        ["> src", "> target", "a.txt", "b.txt", "README.md"],
        "Git's own folder is left out"
    );
    assert!(tree.rows()[1].node.ignored);

    assert_eq!(tree.toggle("src"), Some("src".to_owned()), "list it first");
    assert!(tree.rows()[0].loading);
    tree.listed(
        "src",
        Ok(vec![entry("src/main.rs", false), entry("src/ui", true)]),
        &HashSet::new(),
    );
    assert_eq!(
        visible(&tree),
        [
            "v src",
            "  > ui",
            "  main.rs",
            "> target",
            "a.txt",
            "b.txt",
            "README.md"
        ]
    );
    assert_eq!(tree.toggle("src"), None, "closing needs nothing");
    assert_eq!(visible(&tree)[0], "> src");
    assert_eq!(tree.toggle("src"), None, "already listed");
    assert_eq!(tree.listed_folders(), ["", "src"]);

    // Listing again keeps what's shown until the new list arrives.
    let shown = tree.rows();
    let _ = tree.load("src");
    assert_eq!(tree.rows(), shown);
}

#[test]
fn a_change_affects_the_folders_it_touches() {
    let mut tree = Tree::new();
    tree.load("");
    tree.listed("", Ok(vec![entry("src", true)]), &HashSet::new());
    let _ = tree.toggle("src");
    assert_eq!(tree.affected("src/main.rs"), ["src"]);
    assert_eq!(tree.affected("new.txt"), [""]);
    assert_eq!(tree.affected("src"), ["", "src"]);
    assert!(tree.affected("docs/a.md").is_empty(), "not listed");
    assert_eq!(parent_of("a/b/c.rs"), "a/b");

    tree.listed("src", Err("gone".into()), &HashSet::new());
    let _ = tree.toggle("src");
    assert_eq!(
        tree.toggle("src"),
        Some("src".to_owned()),
        "a failed list is tried again"
    );
}

#[test]
fn files_open_as_text_images_or_not_at_all() {
    assert_eq!(
        classify("src/main.rs", b"fn main() {}\n".to_vec()),
        Kind::Text("fn main() {}\n".into())
    );
    assert_eq!(classify("logo.PNG", vec![0x89, b'P']), Kind::Image);
    assert_eq!(classify("icon.svg", b"<svg/>".to_vec()), Kind::Svg);
    assert_eq!(classify("a.bin", vec![1, 0, 2]), Kind::Binary);
    assert_eq!(classify("latin1.txt", vec![0xE9, b'a']), Kind::Binary);
    assert_eq!(
        classify("huge.log", vec![b'a'; MAX_OPEN + 1]),
        Kind::TooLarge(MAX_OPEN + 1)
    );

    assert_eq!(syntax_of("src/main.RS"), "rs");
    assert_eq!(syntax_of("Makefile"), "makefile");
    assert_eq!(
        syntax_of(".gitignore"),
        "txt",
        "a dot file has no extension"
    );
    assert_eq!(syntax_of("LICENSE"), "txt");
}
