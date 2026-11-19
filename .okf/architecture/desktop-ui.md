---
type: Architecture
title: Desktop UI state and Qt model boundaries
description: Ownership, invalidation, and component lifetime rules connecting Workspace, ChatSession, and QML.
covers:
  - crates/harlo-app/qml/
  - crates/harlo-app/src/workspace.rs
  - crates/harlo-app/src/chat_session.rs
  - crates/harlo-app/src/registry.rs
  - crates/harlo-app/src/themes.rs
  - crates/harlo-app/src/qt_base.rs
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: registry
    resource: "repo://crates/harlo-app/src/registry.rs"
  - id: binding
    resource: "repo://crates/harlo-app/src/chat_session.rs"
  - id: workspace
    resource: "repo://crates/harlo-app/src/workspace.rs"
  - id: chat
    resource: "repo://crates/harlo-app/src/chat_session.rs"
  - id: chatview
    resource: "repo://crates/harlo-app/qml/ChatView.qml"
  - id: rows
    resource: "repo://crates/harlo-app/src/chat_session.rs"
    title: "fn ChatSession::apply_row_changes"
  - id: transcript
    resource: "repo://crates/harlo-app/qml/TranscriptView.qml"
  - id: main
    resource: "repo://crates/harlo-app/qml/Main.qml"
  - id: theme
    resource: "repo://crates/harlo-app/qml/Theme.qml"
  - id: palette
    resource: "repo://crates/harlo-app/src/code_view.rs"
    title: fn palette_from_json
  - id: build
    resource: "repo://crates/harlo-app/build.rs"
  - id: editor
    resource: "repo://crates/harlo-app/qml/FileEditor.qml"
  - resource: "repo://crates/harlo-app/qml/"
  - resource: "repo://crates/harlo-app/src/themes.rs"
  - resource: "repo://crates/harlo-app/src/qt_base.rs"
---
# Desktop UI state and Qt model boundaries

The key separation is **session lifetime versus view lifetime**. `Workspace` creates and closes live sessions; the registry shares an `Arc<SessionHandle>` plus harness and working-directory information. `ChatSession` is a Qt list-model subscriber to a live session, not the process owner. A hidden or replaced transcript therefore need not stop its agent.[^registry][^binding]

## Three kinds of state

- **Workspace state:** projects, saved and live sessions, active tab keys, application settings, and integration revisions. The sidebar is another projection of this coordinator.
- **ChatSession state:** transcript rows, request queues, attachment selection, context/model information, and the subscription for the currently bound key.
- **Component state:** scroll position, find bars, popup visibility, review comments, and other temporary interaction details. For example, `ChatView` stores review comments by repository, replacing its JavaScript map when comments change.[^workspace][^chat][^chatview]

Do not treat a JSON-returning invokable as automatically reactive. Workspace exposes explicit revisions for remote copies, plans, workflows, schedules, and other data; QML bindings must depend on the relevant revision/property before rereading a snapshot.[^workspace]

## Asynchronous invalidation

Rebinding a `ChatSession` unsubscribes its previous handle, increments `generation`, resets view state, and attaches a new subscription. `QtSink` queues updates onto the QObject's Qt thread and applies them only when the captured generation still matches. Background request/file-read callbacks use the same pattern in many paths. This is an important pattern to preserve, not a claim that every asynchronous callback already has the guard.[^binding]

The model publishes row insertions, removals, and changed roles rather than asking QML to rebuild the entire transcript on every event. Transcript rendering is virtualized by row kind. Consequently, row indices are presentation positions, not durable message identity; operations that survive updates should use row/session keys or resolve the saved entry explicitly.[^rows][^transcript]

## Navigation does not always rebind chat

`ChatView` distinguishes session keys from `file:`, `diff:`, and `view:` tabs. File and diff surfaces can cover the chat while the session remains bound beneath them so side panels still follow that session. The empty key and `view:new` show the draft screen. When debugging a side panel displaying the previous conversation while a file is open, check this deliberate behavior before changing bindings.[^chatview]

`Main.qml` owns top-level composition, window/tray behavior, and application-level routing. The rest of `qml/` splits into session/transcript/composer components; left/right rails and panels; file/diff/image/terminal views; settings/setup pages and dialogs; and reusable themed controls. Prefer locating the state owner before editing the visually nearest component.[^main][^chatview]

## Theme and resource contracts

`Theme.qml` derives light/dark/system selection and appearance settings from Workspace, then presents semantic color and sizing roles. Theme JSON reaches QML in Qt color order; code rendering separately parses `#rrggbb` and `#aarrggbb`. Palette conversion is a boundary worth testing, especially alpha colors, rather than blindly copying CSS hex strings.[^theme][^palette]

QML registration belongs to `build.rs`: the singleton pragma is accompanied by singleton registration there. Appearance assets and generated tables live outside QML; see [desktop boundaries](/architecture/desktop.md) before adding a new theme, font, or component.[^build]

## Lifetime and visibility pitfalls

A window hidden in the tray is not the same as an invisible item. `FileEditor` and `TranscriptView` explicitly inspect `Window.visibility` to gate work. Optional picture UI is loader-owned and released on close. Follow the existing lifecycle when adding polling or expensive content; otherwise background windows can continue doing foreground work.[^editor][^transcript]

Related workflows: [sending and running agents](/workflows/desktop-sessions.md), [editing and inspecting files](/workflows/desktop-files.md).

[^registry]: crates/harlo-app/src/registry.rs
[^binding]: crates/harlo-app/src/chat_session.rs
[^workspace]: crates/harlo-app/src/workspace.rs
[^chat]: crates/harlo-app/src/chat_session.rs
[^chatview]: crates/harlo-app/qml/ChatView.qml
[^rows]: crates/harlo-app/src/chat_session.rs, fn ChatSession::apply_row_changes
[^transcript]: crates/harlo-app/qml/TranscriptView.qml
[^main]: crates/harlo-app/qml/Main.qml
[^theme]: crates/harlo-app/qml/Theme.qml
[^palette]: crates/harlo-app/src/code_view.rs, fn palette_from_json
[^build]: crates/harlo-app/build.rs
[^editor]: crates/harlo-app/qml/FileEditor.qml
