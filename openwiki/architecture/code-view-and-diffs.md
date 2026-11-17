---
type: architecture
title: Code viewing, editing, and diff rendering
description: How Harlo's Qt desktop shares a Rust code-rendering core between its file editor and Git diff panes, while transcript tool output uses its own code-block presentation path.
tags: [desktop, code-view, editor, diff, rendering]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-17T17:39:11.286Z
sources:
  - id: openwiki-source-f2c78795e60fd06da08e22f2
    resource: repo://crates/harlo-app/qml/DiffView.qml
  - id: openwiki-source-3d15a2a46e8ff623d0c7498e
    resource: repo://crates/harlo-app/qml/FileEditor.qml
  - id: openwiki-source-42e447a8def7fd12f9ad7f00
    resource: repo://crates/harlo-app/qml/TranscriptView.qml
  - id: openwiki-source-e2396ed1ac2421c5f3bd00a3
    resource: repo://crates/harlo-app/src/code_view.rs
  - id: openwiki-source-040701e66055333fb6afb7ba
    resource: repo://crates/harlo-code/src/view.rs
generated: { by: "harlo", at: "2026-11-17T17:39:11.286Z" }
---

# Code viewing, editing, and diff rendering

## Two presentation paths

The desktop has two related but distinct ways to show code. The standalone `harlo-code` crate owns document editing primitives, syntax colouring, font layout, geometry, and raster drawing without depending on Qt. The app's `CodeView` bridges that renderer into Qt: it forwards input and geometry operations and blits the rendered image. The shared Qt item is used by the editable file editor and by each read-only diff pane. Chat transcript code blocks instead use `CodeText` with prepared HTML; they are not the same editor widget.

This split keeps the text and drawing engine reusable and testable without Qt while letting QML own workspace layout, file lifecycle, diff controls, and chat transcript interaction. Qt sees the code-view surface through properties and functions shaped like `TextEdit`, including UTF-16 positions; Rust performs document operations and paints the view.

## Editing files and preserving drafts

`FileEditor.qml` loads a text file into a `CodeView`; images use the separate image viewer. It creates a fresh editor item when switching files because reusing an item with queued syntax-highlighting work for a previous document can crash Qt. The editor's modified state is tracked by path, and switching tabs asks the Rust view to keep the modified document (including undo history) so reopening the path can restore it. Saving marks the document clean; discarding reloads the disk text and removes the retained draft.

The Rust bridge deliberately keeps an unsaved `Doc` outside the visual item: `keep` stores it by path only for a modified, writable document, and `restore` removes and installs it into the new view. A separate `CodeDrafts` QObject lets the owner mark a path saved or discarded. This is in-memory view-lifecycle preservation, not durable autosave: the file save operation remains the persistence boundary. The editor also layers search/replace, go-to-line, indentation, comments, bracket matching, and disk-change notices around the shared widget.

## Diff panes reuse the renderer

`DiffView.qml` presents a change either in synchronized before/after panes or as a unified one-column view when two columns would be too narrow. Each pane uses the same Rust `CodeView` in read-only mode. The QML layer supplies diff rows as a flat sequence of five values per rendered row (`kind`, side, line number, and changed range); Rust uses that metadata for line-number/sign gutters, added/removed tints, changed-substring emphasis, and hatched rows where the other side has no line. The surrounding QML remains responsible for folds, aligned scrolling, hunk stage/unstage/discard controls, and review comments.

Because both file editing and diffs use the same document geometry and drawing code, selection, positioning, text layout, and syntax colours share an implementation. Their ownership differs: the file editor is writable and retains drafts by path, whereas diff panes are read-only projections of the current comparison and refresh when the file changes.

## Transcript tool output is separate

Transcript tool results may contain file listings or code output. The session model converts supported listing output into structured line data for a tool card; transcript QML uses `CodeText` for the code display, while ordinary assistant fenced blocks use `CodeText` with rendered HTML. This path gives tool output appropriate line labels and preview limits, but it should not be described as the editable `CodeView` or as the diff renderer.

## Verification boundaries

The Qt-independent `harlo-code` tests exercise raster output and position mapping, parsing diff-row metadata, scrolling to a distant line, and document editing/undo behavior. QML self-tests cover how code and file views behave in the desktop, but those are not run by the Rust crate tests. For a change to the renderer, begin with `cargo test -p harlo-code`; for widget wiring, file lifecycle, or diff interaction, also exercise the desktop's Qt/QML checks described in [testing and releases](../operations/testing-and-releases.md).

For transcript state and the surrounding desktop workspace lifecycle, see [sessions and desktop](sessions-and-desktop.md). For coordinated Git changes and plans, see [plans and project changes](../workflows/plans-and-project-changes.md).
