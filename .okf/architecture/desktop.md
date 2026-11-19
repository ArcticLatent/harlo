---
type: Architecture
title: Desktop application boundaries and lifecycle
description: How the Qt desktop composes session services, native rendering, bundled resources, and background integrations.
covers:
  - crates/harlo-app/
  - crates/harlo-code/
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: crates
    resource: "repo://crates/harlo-app/Cargo.toml"
  - id: code
    resource: "repo://crates/harlo-code/Cargo.toml"
  - id: workspace
    resource: "repo://crates/harlo-app/src/workspace.rs"
  - id: startup
    resource: "repo://crates/harlo-app/src/main.rs"
    title: fn main
  - id: graphics
    resource: "repo://crates/harlo-app/src/graphics.rs"
  - id: closing
    resource: "repo://crates/harlo-app/qml/Main.qml"
  - id: build
    resource: "repo://crates/harlo-app/build.rs"
  - id: assets
    resource: "repo://crates/harlo-app/src/diagrams.rs"
  - id: sounds
    resource: "repo://crates/harlo-app/sounds/"
  - id: phone
    resource: "repo://crates/harlo-app/src/phone_server.rs"
  - id: logs
    resource: "repo://crates/harlo-app/src/diagnostics.rs"
  - id: tests
    resource: "repo://crates/harlo-app/qml/SelfTests.qml"
  - resource: "repo://crates/harlo-app/"
  - resource: "repo://crates/harlo-code/"
---
# Desktop application boundaries and lifecycle

`harlo-app` is the composition root, not the agent implementation. Its `harlo` binary joins Qt/QML presentation to `harlo-core` session and persistence services and `harlo-harness` agent processes. `harlo-code` is the Qt-free document, layout, and rasterization library used by that presentation; it still depends on core syntax and terminal types.[^crates][^code]

## Ownership map

| Boundary | Responsibility | Where to continue |
| --- | --- | --- |
| Process and native platform | Runtime, single-instance handoff, Qt application, shutdown, graphics trial, diagnostics | This page |
| Qt models and QML | Workspace navigation, live-session projections, dialogs, settings, themed controls | [Desktop UI state](/architecture/desktop-ui.md) |
| Native document rendering | UTF-16 positions, edits, syntax, layout, pixels, terminal drawing | [Desktop code renderer](/architecture/desktop-code.md) |
| Agent work | Prompt preparation, session subscriptions, orchestration, persisted workflow execution | [Desktop sessions](/workflows/desktop-sessions.md) |
| Repository interaction | File drafts, disk changes, Git views, terminals, images | [Desktop files](/workflows/desktop-files.md) |
| External integration | SSH machines, provider setup, phone transport, notifications | [Desktop connectivity](/workflows/desktop-connectivity.md) |

The table divides responsibilities, not independent services: `Workspace` is a large coordinator spanning navigation, projects, session lifetime, workflow execution, and integration entry points. Moving a widget does not move ownership of its session.[^workspace]

## Startup order is part of the contract

1. Handle special process modes before opening the GUI: version, agent MCP bridge, and diagram utilities.
2. Forward an ordinary second launch to the existing instance before initializing logs. `--check-start` deliberately bypasses handoff.
3. Migrate the old piqt data only when piqt is not running. Apply graphics and default Controls style **before background threads start**; environment mutation later would race readers.
4. Register agent bridges and background jobs, load remote hosts before opening `ssh://` projects, create a widgets-capable Qt application, and load bundled fonts.
5. Load `qrc:/qt/qml/harlo/qml/Main.qml`, then enter Qt's event loop. The shared Tokio runtime has two worker threads; blocking work is separately dispatched.[^startup]

A widgets-capable application is intentional even though the interface is Qt Quick: native dialogs and desktop tray integration need it. The first rendered frame confirms a graphics trial; QML object-creation failure or a normal no-frame exit is inconclusive, not evidence that the selected graphics backend failed.[^startup][^graphics]

## Closing a window is not necessarily quitting

`Main.qml` can hide into the tray or retain the application for workflow runs. Actual process shutdown ends phone Live Activities, marks remote hosts disconnected, stops agent processes and model servers started by Harlo, disconnects remote hosts, and removes local sockets. It then drops the QML engine before shutting down the Qt application. Preserve that destruction order: the implementation explicitly avoids Qt's former crashing exit order.[^closing][^startup]

## Resources and build boundary

`build.rs` explicitly registers QML files and Rust QObject bridges in the `harlo` QML module. `Theme` and `Keymap` are registered singletons. New QML files are not discovered automatically. Provider SVGs and theme JSON are scanned and compiled into generated Rust tables; fonts, avatars, and app/provider icons become Qt resources. Qt Quick, QuickControls2, Widgets, Test, and Multimedia are linked here.[^build]

The remaining bundled directories are functional dependencies, not incidental artwork: `fonts/` supports UI/code shaping and generated file-icon glyphs; `icons/` supplies platform, provider, and avatar imagery; `sounds/` supplies notification audio; `themes/` supplies appearance variants; `phone/` is the served browser client. `assets/mermaid/` supplies the explicit browser fallback, while normal in-app diagrams use native `merman` rendering. Preserve neighboring licenses and notices when updating these assets.[^build][^assets][^sounds][^phone]

## Finding failures

Start with `harlo.log`, the retained previous log, panic reports, and optionally protocol JSONL under Harlo's data/logs directory. Forwarded second launches intentionally do not replace the running instance's log. Do not confuse a protocol log with a harmless UI screenshot: it records agent traffic.[^logs][^startup]

For implementation checks, `cargo test -p harlo-code` exercises the Qt-free library. App tests require the Qt build dependencies; the embedded `SelfTests.qml` provides UI scenarios rather than a standalone generic QML TestCase suite. Keep renderer tests, app bridge tests, and actual QML loading checks distinct: success in one does not establish the others.[^crates][^tests]

[^crates]: crates/harlo-app/Cargo.toml
[^code]: crates/harlo-code/Cargo.toml
[^workspace]: crates/harlo-app/src/workspace.rs
[^startup]: crates/harlo-app/src/main.rs, fn main
[^graphics]: crates/harlo-app/src/graphics.rs
[^closing]: crates/harlo-app/qml/Main.qml
[^build]: crates/harlo-app/build.rs
[^assets]: crates/harlo-app/src/diagrams.rs
[^sounds]: crates/harlo-app/sounds/
[^phone]: crates/harlo-app/src/phone_server.rs
[^logs]: crates/harlo-app/src/diagnostics.rs
[^tests]: crates/harlo-app/qml/SelfTests.qml
