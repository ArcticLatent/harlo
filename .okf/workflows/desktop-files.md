---
type: Workflow
title: Desktop file editing and repository inspection
description: Open and save files safely, understand disk-change notices, and trace Git, terminal, and visual inspection paths.
covers:
  - crates/harlo-app/src/text_file.rs
  - crates/harlo-app/src/file_browser.rs
  - crates/harlo-app/src/git_panel.rs
  - crates/harlo-app/src/ci.rs
  - crates/harlo-app/src/terminal_session.rs
  - crates/harlo-app/src/image_view.rs
  - crates/harlo-app/src/diagrams.rs
  - crates/harlo-app/src/visuals.rs
  - crates/harlo-app/qml/FileEditor.qml
  - crates/harlo-app/qml/DiffView.qml
  - crates/harlo-app/qml/FilesPanel.qml
  - crates/harlo-app/qml/ChangesPanel.qml
  - crates/harlo-app/qml/TerminalView.qml
  - crates/harlo-app/qml/ImageViewer.qml
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: browser
    resource: "repo://crates/harlo-app/src/file_browser.rs"
  - id: chat
    resource: "repo://crates/harlo-app/qml/ChatView.qml"
  - id: editor
    resource: "repo://crates/harlo-app/qml/FileEditor.qml"
  - id: io
    resource: "repo://crates/harlo-app/src/text_file.rs"
  - id: adapter
    resource: "repo://crates/harlo-app/src/code_view.rs"
  - id: git
    resource: "repo://crates/harlo-app/src/git_panel.rs"
  - id: ci
    resource: "repo://crates/harlo-app/src/ci.rs"
  - id: terminal
    resource: "repo://crates/harlo-app/src/terminal_session.rs"
    title: "fn TerminalSession::start"
  - id: diagrams
    resource: "repo://crates/harlo-app/src/diagrams.rs"
  - id: visuals
    resource: "repo://crates/harlo-app/src/visuals.rs"
  - id: tests
    resource: "repo://crates/harlo-app/qml/SelfTests.qml"
  - resource: "repo://crates/harlo-app/src/image_view.rs"
  - resource: "repo://crates/harlo-app/qml/DiffView.qml"
  - resource: "repo://crates/harlo-app/qml/FilesPanel.qml"
  - resource: "repo://crates/harlo-app/qml/ChangesPanel.qml"
  - resource: "repo://crates/harlo-app/qml/TerminalView.qml"
  - resource: "repo://crates/harlo-app/qml/ImageViewer.qml"
---
# Desktop file editing and repository inspection

The file browser presents a flat model of expanded tree rows; opening a file selects a file tab rather than replacing the underlying chat session. `TextFile` owns disk I/O and status, `CodeView` owns editable document state, and `FileEditor.qml` coordinates their lifetimes. Keeping those roles separate prevents a tab switch from being mistaken for a save.[^browser][^chat][^editor]

## Open, switch, save

1. On a path change, `FileEditor` keeps a modified document if its draft is still tracked, clears the old editor loader, and chooses text or image handling.
2. `TextFile::load` increments a generation and reads in a blocking task. It stamps the file **before** reading so a concurrent write remains detectable. A stale generation cannot install its result into the new tab.
3. Results distinguish text, binary, too-large, and error states. Only a current text result activates the editor.
4. Saving passes the editor's current text to `TextFile`. Local writes complete synchronously; remote writes run off-thread. The remote method returning `true` means the save was accepted, not that SSH persistence has completed.
5. The `savedTo` signal marks the document/draft saved and informs surrounding views. Do not clear the dirty marker just because `save()` was called.[^editor][^io]

Draft state includes document/undo state retained by the code-view side, while the QML draft map identifies dirty paths. This is not described by the implementation as a durable crash-recovery journal; do not promise users that unsaved drafts survive process death.[^editor][^adapter]

## When an agent edits the same file

Visible text files are polled every 1.5 seconds while the window is shown. An external change reloads an unmodified document while preserving view position; with local edits, it instead raises a changed/deleted notice. “Keep mine” acknowledges the disk stamp. It does **not** merge the two versions.[^editor]

Remote stamps are fetched asynchronously and `diskState()` initially returns the last known result. Stamps use modification time and size, not content hashes. The save path shown here is not an atomic compare-and-swap against the loaded stamp, so a notice is conflict awareness rather than a guarantee against every concurrent overwrite.[^io]

## Git inspection and mutation

`GitPanel::refresh` runs repository discovery/status/diff work off-thread and checks its generation before applying results. A session directory can be a repository or a folder containing several repositories; in the latter case the chosen repository, otherwise one with changes, becomes the displayed root. Absolute requested file paths are normalized relative to that root.[^git]

Stage/unstage/revert operations delegate to core Git functions, with hunk operations using the currently loaded diff. Reverting an untracked file is deliberately different: there is no committed version to restore, so it goes to the trash. Do not describe that action as checking out a previous version.[^git]

Pull-request CI inspection uses hosting CLIs. Their nonzero exit is not automatically a transport failure: the CI helper retains stdout because `gh` can exit nonzero while checks are pending or failing. Keep the distinction between command failure and failed checks when changing status handling.[^ci]

## Terminal and visual branches

The terminal adapter owns the core terminal handle and translates events into Qt updates, coalescing burst wakeups with an atomic pending flag. An available remote host produces an SSH shell; otherwise the implementation chooses a local valid directory or home. Check host resolution and the displayed working directory before assuming an unresolved `ssh://` path guarantees remote execution. The terminal painter itself only draws the screen; see [code rendering](/architecture/desktop-code.md).[^terminal]

Images are viewed rather than text-edited. Local image tabs watch stamps; remote image tabs use a local copy and explicitly do not poll that remote image for changes. Diagrams normally render natively to cached PNGs, with a browser fallback using bundled Mermaid. HTML mockups are screenshots from a headless browser with a separate cache profile, not the user's browser profile; their cache identity includes file time and size.[^editor][^diagrams][^visuals]

## Checks worth preserving

Exercise rapid tab switching during loads, Unicode cursor/selection edits, a failed remote save, disk changes both with and without unsaved edits, and staged versus unstaged diff selection. The implementation has separate generation, draft, and repository-selection guards; a happy-path local save tests none of their races. The app's click-driven self-test harness can exercise real Qt input, while `harlo-code` tests cover document behavior independently.[^io][^editor][^git][^tests]

[^browser]: crates/harlo-app/src/file_browser.rs
[^chat]: crates/harlo-app/qml/ChatView.qml
[^editor]: crates/harlo-app/qml/FileEditor.qml
[^io]: crates/harlo-app/src/text_file.rs
[^adapter]: crates/harlo-app/src/code_view.rs
[^git]: crates/harlo-app/src/git_panel.rs
[^ci]: crates/harlo-app/src/ci.rs
[^terminal]: crates/harlo-app/src/terminal_session.rs, fn TerminalSession::start
[^diagrams]: crates/harlo-app/src/diagrams.rs
[^visuals]: crates/harlo-app/src/visuals.rs
[^tests]: crates/harlo-app/qml/SelfTests.qml
