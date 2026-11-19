---
type: Architecture
title: Desktop code and terminal rendering
description: The Qt-free document engine, incremental syntax pipeline, and native pixel/input bridge.
covers:
  - crates/harlo-code/
  - crates/harlo-app/src/code_view.rs
  - crates/harlo-app/src/fonts.rs
  - crates/harlo-app/cpp/harlo_codeview.h
  - crates/harlo-app/cpp/harlo_fonts.h
  - crates/harlo-app/qml/CodeText.qml
  - crates/harlo-app/qml/CodeHit.qml
  - crates/harlo-app/qml/CodeCopy.qml
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: lib
    resource: "repo://crates/harlo-code/src/lib.rs"
  - id: bridge
    resource: "repo://crates/harlo-app/cpp/harlo_codeview.h"
  - id: text
    resource: "repo://crates/harlo-code/src/text.rs"
  - id: doc
    resource: "repo://crates/harlo-code/src/doc.rs"
  - id: lang
    resource: "repo://crates/harlo-code/src/lang.rs"
  - id: syntax
    resource: "repo://crates/harlo-code/src/syntax.rs"
  - id: view
    resource: "repo://crates/harlo-code/src/view.rs"
  - id: fonts
    resource: "repo://crates/harlo-code/src/fonts.rs"
  - id: terminal
    resource: "repo://crates/harlo-code/src/terminal.rs"
  - id: sync
    resource: "repo://crates/harlo-code/src/view.rs"
    title: "fn View::sync"
  - id: budget
    resource: "repo://crates/harlo-app/src/code_view.rs"
  - resource: "repo://crates/harlo-code/"
  - resource: "repo://crates/harlo-app/src/fonts.rs"
  - resource: "repo://crates/harlo-app/cpp/harlo_fonts.h"
  - resource: "repo://crates/harlo-app/qml/CodeText.qml"
  - resource: "repo://crates/harlo-app/qml/CodeHit.qml"
  - resource: "repo://crates/harlo-app/qml/CodeCopy.qml"
---
# Desktop code and terminal rendering

`harlo-code` owns text, edits, layout, hit testing, and rasterized pixels without linking Qt. `harlo-app::CodeView` exposes that engine as a QObject-backed item. The C++ `CodeItem` base forwards native keys, pointer, wheel, focus, input-method, sizing, and timer events as ordinary values, then blits the image Rust produced.[^lib][^bridge]

## Data pipeline

| Layer | State and responsibility |
| --- | --- |
| `text` | Lines, UTF-16 lengths and positions, lazily cached line starts, edit splices |
| `doc` | Cursor/anchor, undo/redo, saved-state tracking, read-only edits, language editing rules |
| `lang` | Comment markers, indentation detection, colon-based indentation conventions |
| `syntax` | Token classes and parser/highlighter states, incremental convergence |
| `view` | Visible-line shaping, hit testing, scrolling, selection/find/diff decorations, pixels |
| `fonts` | Shared cosmic-text font system and rasterized-glyph cache |
| `terminal` | Terminal-grid painter, independently cached changed cells/rows |

These layers are implementation boundaries: a file save is outside the renderer, and a terminal process is outside the terminal painter.[^text][^doc][^lang][^syntax][^view][^fonts][^terminal]

## Coordinate invariants

**Public text positions count UTF-16 code units**, matching QML, with one unit for a newline. Rust strings and syntax-span boundaries use UTF-8 bytes. `Line::byte_at` and `Line::col_at` clamp and convert at character boundaries; never substitute a byte offset for a cursor or selection position. Grapheme-aware editing is a further concern in `Doc`, not permission to reinterpret the position unit.[^text][^doc]

**View geometry is logical pixels unless explicitly device pixels.** Rendering receives a device-pixel ratio; the C++ blit constructs premultiplied RGBA8888 pixels and sets the image's device ratio. Smoothing is disabled to avoid blurred text at fractional item positions. Input-method cursor geometry and scroll offsets must stay in the matching logical coordinate space.[^view][^bridge]

Diff rows have their own contract: old/new side, row kind, displayed number, and changed UTF-16 column range. Fold/comment rows can reserve space that QML draws over. Treat the row metadata as coupled to the text it decorates.[^view]

## Incremental work, not full-document repaint preparation

An edit emits line splices. `View::sync` passes invalidation through to coloring and layout. Coloring retains parser end states and stops when unchanged downstream states converge. Long lines above 10,000 bytes are not colored; far-away visible lines may temporarily use a fresh-state guess with 24 lines of context until the sequential pass catches up. A momentary color correction after jumping into a multiline string is therefore possible by design.[^sync][^syntax]

The Qt adapter budgets coloring across views on the same thread: a 4 ms tick and a shared 5 ms allowance per 16 ms window prevent two diff panes and an editor from each claiming a full frame. The comment calls this background coloring, but this budget is thread-local sliced work, not evidence of a separate syntax worker for every view.[^budget]

Fonts are preloaded using bundled bytes plus system fallback, and access is serialized through a shared mutex. Terminal painting uses the same code font infrastructure but a grid-specific cache, including custom box/block drawing so adjacent cells join.[^fonts][^terminal]

## Changing or debugging the engine

For incorrect edits, begin in `text`/`doc`; for incorrect token ranges, inspect `syntax`; for wrong cursor hits, inspect `view` coordinate conversion; for keyboard/IME or blurred display issues, inspect the app adapter and C++ base. File persistence and unsaved-tab lifetime are separate: see [desktop files](/workflows/desktop-files.md).[^text][^doc][^syntax][^view][^bridge]

Run `cargo test -p harlo-code` for the library's inline tests, including language and document behavior. Native input delivery and actual QML properties need app-level tests too; a correct raster buffer alone does not establish correct focus or shortcut routing.[^lang][^doc][^bridge]

[^lib]: crates/harlo-code/src/lib.rs
[^bridge]: crates/harlo-app/cpp/harlo_codeview.h
[^text]: crates/harlo-code/src/text.rs
[^doc]: crates/harlo-code/src/doc.rs
[^lang]: crates/harlo-code/src/lang.rs
[^syntax]: crates/harlo-code/src/syntax.rs
[^view]: crates/harlo-code/src/view.rs
[^fonts]: crates/harlo-code/src/fonts.rs
[^terminal]: crates/harlo-code/src/terminal.rs
[^sync]: crates/harlo-code/src/view.rs, fn View::sync
[^budget]: crates/harlo-app/src/code_view.rs
