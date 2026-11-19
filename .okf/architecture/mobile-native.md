---
type: Architecture
title: Native mobile app and HarloKit
description: How the iPhone UI, observable model, protocol client, and native rendering share responsibility.
covers:
  - ios/
generated: { by: harlo/gpt-6-luna, at: "2026-11-19T18:56:17Z" }
sources:
  - id: package
    resource: "repo://ios/HarloKit/Package.swift"
  - id: fixture
    resource: "repo://ios/fixtures/recording.jsonl"
  - id: project
    resource: "repo://ios/Harlo/project.yml"
  - id: export
    resource: "repo://ios/Harlo/ExportOptions.plist"
  - id: app
    resource: "repo://ios/Harlo/Harlo/HarloApp.swift"
  - id: delegate
    resource: "repo://ios/Harlo/Harlo/AppDelegate.swift"
  - id: root
    resource: "repo://ios/Harlo/Harlo/RootView.swift"
  - id: tabs
    resource: "repo://ios/Harlo/Harlo/MainTabs.swift"
  - id: session
    resource: "repo://ios/Harlo/Harlo/SessionView.swift"
  - id: content
    resource: "repo://ios/Harlo/Harlo/SessionContent.swift"
  - id: model
    resource: "repo://ios/HarloKit/Sources/HarloKit/Model.swift"
  - id: client
    resource: "repo://ios/HarloKit/Sources/HarloKit/Client.swift"
  - id: messages
    resource: "repo://ios/HarloKit/Sources/HarloKit/Messages.swift"
  - id: mode
    resource: "repo://ios/HarloKit/Sources/HarloKit/Model.swift"
    title: fn HarloModel.mode
  - id: store
    resource: "repo://ios/HarloKit/Sources/HarloKit/Store.swift"
  - id: types
    resource: "repo://ios/HarloKit/Sources/HarloKit/"
  - id: rich
    resource: "repo://ios/HarloKit/Sources/HarloKit/RichText.swift"
  - id: cache
    resource: "repo://ios/Harlo/Harlo/RichCache.swift"
  - id: wiki
    resource: "repo://ios/Harlo/Harlo/WikiPageView.swift"
  - id: wiki-graph
    resource: "repo://ios/HarloKit/Sources/HarloKit/Wiki.swift"
    title: fn Wiki.graph
  - id: photos
    resource: "repo://ios/Harlo/Harlo/Photos.swift"
  - id: dictation
    resource: "repo://ios/Harlo/Harlo/Dictation.swift"
  - resource: "repo://ios/"
---
# Native mobile app and HarloKit

The iPhone app is a native controller for a computer running Harlo, not an on-device agent runner. SwiftUI views consume `HarloModel`; that model owns pairing, connection recovery and the currently open session; the `HarloClient` actor transports requests and events. Keeping the model and wire types in the Foundation-based HarloKit package also lets the command-line probe exercise the same behavior without SwiftUI.[^model][^client][^package]

This page is the directory-level map for `ios/`: `Harlo/` contains the app, shared UI, extensions, assets and Xcode project configuration; `HarloKit/` contains portable protocol/model code, the C crypto adapter, probe and unit tests; `fixtures/` supplies recorded desktop responses. The companion pages below explain the protocol, extensions and test workflow in detail.[^project][^package][^fixture]

## Targets and ownership

`ios/Harlo/project.yml` is the XcodeGen source of truth. It declares an iPhone app, notification-service extension, widget extension and UI-test target, with iOS 18 deployment and Swift 6 language settings. The local package supplies HarloKit and its C NaCl dependency. `Shared/` is compiled into both app and widget targets; it contains ActivityKit attributes, common widget views and shared artwork. App assets supply icons, provider logos, penguins and launch imagery; fonts and logo licenses are bundled from the desktop tree. Export options select automatic App Store Connect signing and leave version/build-number management to the caller.[^project][^export]

`AppDelegate` lazily constructs the production model with `KeychainStore` and the device name. `HarloApp` injects it into the SwiftUI environment, applies the desktop-derived theme, and drives `resume`/`suspend` from scene phase. It installs `NetworkWatch` once and translates changes into model checks. Tour and replay models bypass real connection/notification lifecycle handling.[^app][^delegate]

`RootView` chooses pairing or the tab shell from whether a host exists. Dashboard, New Session, Approvals and Settings are separate tabs. An opened session stays in the dashboard or new-session stack that opened it; external opens, including notifications, land on the dashboard. Popping the session destination clears the model's selection. `SessionView` keys `SessionContent` by the `OpenSession` object's identity, so a different session starts with fresh view-local state.[^root][^tabs][^session]

## State and command flow

The `@MainActor @Observable` model owns list-level sessions, features, limits, theme, push registration and one `OpenSession`. The latter owns transcript rows, requests, queues, draft, plan, helpers and workflow state. Neither object is a durable copy of the computer's session database: the store retains identity, host, preferences and widget snapshots, while reconnect refreshes remote state.[^model][^store]

A list key is not necessarily the live key returned by `open`. While that reply is pending, `OpenSession` buffers early events; after the live key arrives it applies matching events and discards other sessions' events. `reset` replaces rows, `rows` applies changes, and dedicated events update status, metadata, requests, queues, todos, subagents and side answers. Reopening clears old requests, todos and subagents so a background interval does not leave completed work looking active. The model chooses normal, steering, follow-up or stop-and-send mode from busy state and the user's selected action, rather than leaving this decision to each view.[^model][^messages][^mode]

The client assigns request IDs and resumes the corresponding continuation when a reply arrives. Unsolicited messages become an `AsyncStream<Event>` consumed by the model. Typed wrappers turn JSON into domain types and usually promote a reply's `error` into a thrown error; the generic `call` deliberately returns error objects unchanged. The model's operation helper converts thrown failures into an alert. Feature flags from `ready` gate optional capabilities such as plans, helpers, pictures, wiki and workflow operations rather than assuming every desktop version supports them.[^client][^model]

Sending chooses normal mode when idle, steering or follow-up when busy. An `unsent` event restores text ahead of any newer draft and returns photos for the composer. Plans/helpers, files/diffs/undo, model servers, limits and workflows use the same client/model boundary; their HarloKit value types keep decoding and presentation calculations outside the view tree.[^model][^types]

## Native presentation rather than a web view

Transcript runs group contiguous rows by their supplied group key. Active groups default open; completed groups default folded, with explicit user overrides retained. `SessionContent` uses a non-lazy vertical stack intentionally: streaming height changes and estimated offscreen heights previously made end-following unreliable. The UI displays an earlier-message count instead of implying the phone has the whole transcript.[^model][^content]

Desktop HTML is parsed into `RichBlock`/`Span` values for native paragraphs, lists, quotes, tables, images and highlighted code; unknown tags retain their text. `RichCache` caches by HTML and adds file links. Wiki pages have their own blocks, footnote/code navigation and page-link handling; diagrams and pictures are requested through HarloKit rather than executed as arbitrary embedded page content.[^rich][^cache][^wiki][^client]

For large wikis, the page graph narrows to the overview and its incoming/outgoing neighbors instead of drawing every page; smaller graphs retain the full page set. On full overview graphs, overview links are omitted except when that overview was the last page read, reducing edges that merely connect everything to the index.[^wiki-graph]

Camera/library images are resized to a maximum side of 1568 pixels and encoded as JPEG at quality 0.8 before sending. Dictation appends recognized text to the existing draft, requests microphone and speech permissions, and prefers on-device recognition only when supported; otherwise it can use Apple's recognition service. It must not be described as always offline.[^photos][^dictation]

## Where to follow a change

- Connection and trust changes: [pairing, encryption and reconnect](/architecture/mobile-protocol.md).
- Shared Keychain data, background presentation and deep links: [widgets and notifications](/architecture/mobile-notifications.md).
- Remote infrastructure: [relay and APNS deployment](/workflows/mobile-relay-apns.md).
- Portable model tests, recordings and simulator checks: [mobile probe and replay](/workflows/mobile-probe.md).

[^package]: ios/HarloKit/Package.swift
[^fixture]: ios/fixtures/recording.jsonl
[^project]: ios/Harlo/project.yml
[^export]: ios/Harlo/ExportOptions.plist
[^app]: ios/Harlo/Harlo/HarloApp.swift
[^delegate]: ios/Harlo/Harlo/AppDelegate.swift
[^root]: ios/Harlo/Harlo/RootView.swift
[^tabs]: ios/Harlo/Harlo/MainTabs.swift
[^session]: ios/Harlo/Harlo/SessionView.swift
[^content]: ios/Harlo/Harlo/SessionContent.swift
[^model]: ios/HarloKit/Sources/HarloKit/Model.swift
[^client]: ios/HarloKit/Sources/HarloKit/Client.swift
[^messages]: ios/HarloKit/Sources/HarloKit/Messages.swift
[^mode]: ios/HarloKit/Sources/HarloKit/Model.swift, fn HarloModel.mode
[^store]: ios/HarloKit/Sources/HarloKit/Store.swift
[^types]: ios/HarloKit/Sources/HarloKit/
[^rich]: ios/HarloKit/Sources/HarloKit/RichText.swift
[^cache]: ios/Harlo/Harlo/RichCache.swift
[^wiki]: ios/Harlo/Harlo/WikiPageView.swift
[^wiki-graph]: ios/HarloKit/Sources/HarloKit/Wiki.swift, fn Wiki.graph
[^photos]: ios/Harlo/Harlo/Photos.swift
[^dictation]: ios/Harlo/Harlo/Dictation.swift
