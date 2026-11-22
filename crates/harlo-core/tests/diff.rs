//! Diffs as lines to show: numbering, gaps and the changed words.

// Rust guideline compliant 2026-10-09

use harlo_core::diff::{LineKind, Span, lines, written};

fn kinds(patch: &str) -> Vec<(LineKind, Option<u32>, String)> {
    lines(patch)
        .into_iter()
        .map(|l| (l.kind, l.number, l.text()))
        .collect()
}

#[test]
fn a_patch_becomes_numbered_lines_without_its_headers() {
    let patch = "Index: e.txt\n===\n--- e.txt\n+++ e.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+three\n\\ No newline at end of file\n@@ -9 +9 @@\n-nine\n+ten\n";
    assert_eq!(
        kinds(patch),
        [
            (LineKind::Context, Some(1), "one".to_owned()),
            (LineKind::Removed, Some(2), "two".to_owned()),
            (LineKind::Added, Some(2), "three".to_owned()),
            (LineKind::Gap, None, String::new()),
            (LineKind::Removed, Some(9), "nine".to_owned()),
            (LineKind::Added, Some(9), "ten".to_owned()),
        ]
    );
}

fn changed(spans: &[Span]) -> Vec<&str> {
    spans
        .iter()
        .filter(|s| s.changed)
        .map(|s| s.text.as_str())
        .collect()
}

#[test]
fn an_edited_line_marks_only_the_words_that_changed() {
    let patch = "@@ -1 +1 @@\n-Open Blender normally; its addon serves port 9876.\n+Open Blender normally; its addon serves port 9877.\n";
    let shown = lines(patch);
    assert_eq!(
        changed(&shown[0].spans),
        ["9876."],
        "words keep their punctuation"
    );
    assert_eq!(changed(&shown[1].spans), ["9877."]);
    assert_eq!(
        shown[1].text(),
        "Open Blender normally; its addon serves port 9877."
    );
}

#[test]
fn a_rewritten_line_is_not_marked_word_by_word() {
    let shown = lines("@@ -1 +1 @@\n-alpha beta gamma\n+something else entirely\n");
    assert!(shown.iter().all(|l| changed(&l.spans).is_empty()));
}

#[test]
fn a_written_file_shows_every_line_added() {
    let diff = written("new.txt", "fresh\nfile\n");
    assert_eq!((diff.additions, diff.deletions), (2, 0));
    assert_eq!(
        kinds(&diff.patch),
        [
            (LineKind::Added, Some(1), "fresh".to_owned()),
            (LineKind::Added, Some(2), "file".to_owned()),
        ]
    );
}
