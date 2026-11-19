---
type: Architecture
title: Mobile notifications, widgets and Live Activities
description: How the app and its extensions share encrypted push content and cached snapshots without a permanent background socket.
covers:
  - ios/Harlo/HarloNotifications/
  - ios/Harlo/HarloWidgets/
  - ios/Harlo/Shared/
  - ios/HarloKit/Sources/HarloKit/Push.swift
  - ios/HarloKit/Sources/HarloKit/WidgetSnapshot.swift
  - ios/HarloKit/Sources/HarloKit/LimitsSnapshot.swift
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: project
    resource: "repo://ios/Harlo/project.yml"
  - id: delegate
    resource: "repo://ios/Harlo/Harlo/AppDelegate.swift"
  - id: app
    resource: "repo://ios/Harlo/Harlo/HarloApp.swift"
  - id: notifications
    resource: "repo://ios/Harlo/Harlo/Notifications.swift"
  - id: model
    resource: "repo://ios/HarloKit/Sources/HarloKit/Model.swift"
  - id: push
    resource: "repo://ios/HarloKit/Sources/HarloKit/Push.swift"
  - id: crypto
    resource: "repo://ios/HarloKit/Sources/HarloKit/Crypto.swift"
  - id: service
    resource: "repo://ios/Harlo/HarloNotifications/NotificationService.swift"
  - id: sessions
    resource: "repo://ios/HarloKit/Sources/HarloKit/WidgetSnapshot.swift"
  - id: limits
    resource: "repo://ios/HarloKit/Sources/HarloKit/LimitsSnapshot.swift"
  - id: session-provider
    resource: "repo://ios/Harlo/HarloWidgets/SessionsProvider.swift"
  - id: limit-provider
    resource: "repo://ios/Harlo/HarloWidgets/LimitsProvider.swift"
  - id: activities
    resource: "repo://ios/Harlo/Harlo/LiveActivities.swift"
  - id: attributes
    resource: "repo://ios/Harlo/Shared/SessionActivity.swift"
  - id: live
    resource: "repo://ios/Harlo/HarloWidgets/Live.swift"
  - id: activity-widget
    resource: "repo://ios/Harlo/HarloWidgets/SessionLiveActivity.swift"
  - id: shared-limits
    resource: "repo://ios/Harlo/HarloWidgets/LimitsWidget.swift"
  - resource: "repo://ios/Harlo/HarloNotifications/"
  - resource: "repo://ios/Harlo/HarloWidgets/"
  - resource: "repo://ios/Harlo/Shared/"
---
# Mobile notifications, widgets and Live Activities

Background presentation is not a second continuously connected client. The foreground app writes compact snapshots, the notification extension can patch the session snapshot, and WidgetKit reads those snapshots. Live Activities instead receive APNS updates to activity-specific tokens. All three targets share the app's Keychain access group; widget and notification extension code derive its team prefix from `HarloTeam` in their generated Info.plist.[^project][^delegate][^service][^live]

## Registration and actions

After pairing, and on foreground entry when paired, the app requests notification permission if undetermined and registers when authorized or provisional. The device token is encoded as lowercase hex, stored with sandbox and asks/done/limits preferences, and sent through the encrypted `push` request on every connection. Decoding an older registration defaults absent preference fields to enabled. Debug builds report sandbox tokens; release builds report production tokens.[^app][^notifications][^model][^push]

`AppDelegate` registers `approval` and `question` categories. Allow and Deny both require authentication; Deny is also marked destructive. Most foreground notifications are suppressed because the app already shows the state, while `test` notifications retain banner and sound. Foreground entry removes delivered notifications and sets the badge to the model's waiting count; later waiting-count changes update it.[^delegate][^notifications][^app]

A notification's `h` is opened with the saved phone identity and computer public key. `PushContent` reads kind, session, title, body and optional request; it derives display lines and whether an approval/confirmation can be answered directly. This is a standalone sealed JSON box, not the connection's `{c,s,m}` replay-sequenced envelope.[^push][^crypto]

The service extension preserves the original content if it cannot copy, find or decrypt `h`. Otherwise it sets title/subtitle/body, session thread and approval/question category, and attaches the decoded JSON as `harlo` for the app. The app prefers that decoded value but can decrypt `h` itself if needed. A tap opens the session; a limit/reset notification instead selects the dashboard's limits card.[^service][^notifications][^delegate]

Allow/Deny temporarily connects if necessary, waits up to 20 seconds for a connection, sends the request's mapped answer, and suspends again if the app is not active. Failure to reach Harlo produces an alert rather than claiming the approval succeeded. This path does not need to reopen the visible transcript while backgrounded.[^model]

## Session and limit widgets

`WidgetSnapshot` retains at most eight items, plus host name, total, working/waiting counts and update time. Building from the full session list filters settled sessions unless working/waiting, then sorts waiting first, busy next, unread next, and recent items after those. The model skips writes for unchanged visible content and for demo/sample models; callbacks reload the `HarloSessions` timeline.[^sessions][^delegate]

For a readable push and an existing paired snapshot, the notification service applies `ask`, `done` or `failed` as input, unread or failed status and moves that session to the front. Other kinds do not update session state. This patch recomputes working/waiting counts from the retained items, unlike a full-list snapshot, so it is a best-effort cached view rather than a background authoritative census.[^service][^sessions]

`SessionsProvider` reads the shared Keychain, emits one entry and requests a refresh after 15 minutes. Preview/gallery paths can use samples; the actual timeline falls back to unpaired. Neither provider connects to the computer. A refresh schedule therefore does not imply fresh remote data.[^session-provider][^limit-provider]

`LimitsSnapshot` flattens machine accounts, deduplicating named accounts by provider/account with the first machine winning. The model writes it when visible numbers change and reloads `HarloLimits`. The limits provider emits now plus up to twelve reset entries in the next day, each five seconds after reset, and requests refresh after 30 minutes. `at(date)` locally clears an elapsed window's usage/reset time; this is a projection from cached data, not a new server measurement.[^limits][^limit-provider][^delegate]

## Live Activities and navigation

The app starts or reuses an activity for the session's live key when enabled, not in demo, and authorized by ActivityKit. The initial state says Working; the app watches token updates and hands each token to the computer. Reusing an active activity re-sends a known token. Activities start with a 25-minute stale deadline; foreground tidy removes ended activities and ends active/stale ones whose listed session is no longer busy, starting or input, allowing a 20-second startup grace period.[^activities]

The shared content state exposes phase, start/end times and todo counts alongside sealed detail. Widget rendering decrypts the words with `LiveDetail`, falls back to generic state text, and explicitly warns when a working/input activity is stale. Thus encryption protects the detailed words, not all metadata: phase, timing, counts and the app-created activity attributes are separate fields.[^attributes][^live][^push]

Lock-screen and Dynamic Island presentations link to `harlo://session?key=…`; the limits widget uses `harlo://limits`. The app avoids reopening the already selected session, otherwise finds a list summary or constructs a minimal one by key. These links navigate the native model rather than opening a web transcript.[^activity-widget][^app][^shared-limits]

See [relay/APNS deployment](/workflows/mobile-relay-apns.md) for delivery validation and failure statuses, [native architecture](/architecture/mobile-native.md) for foreground ownership, and [probe/replay](/workflows/mobile-probe.md) for what can be tested without Apple delivery.

[^project]: ios/Harlo/project.yml
[^delegate]: ios/Harlo/Harlo/AppDelegate.swift
[^app]: ios/Harlo/Harlo/HarloApp.swift
[^notifications]: ios/Harlo/Harlo/Notifications.swift
[^model]: ios/HarloKit/Sources/HarloKit/Model.swift
[^push]: ios/HarloKit/Sources/HarloKit/Push.swift
[^crypto]: ios/HarloKit/Sources/HarloKit/Crypto.swift
[^service]: ios/Harlo/HarloNotifications/NotificationService.swift
[^sessions]: ios/HarloKit/Sources/HarloKit/WidgetSnapshot.swift
[^limits]: ios/HarloKit/Sources/HarloKit/LimitsSnapshot.swift
[^session-provider]: ios/Harlo/HarloWidgets/SessionsProvider.swift
[^limit-provider]: ios/Harlo/HarloWidgets/LimitsProvider.swift
[^activities]: ios/Harlo/Harlo/LiveActivities.swift
[^attributes]: ios/Harlo/Shared/SessionActivity.swift
[^live]: ios/Harlo/HarloWidgets/Live.swift
[^activity-widget]: ios/Harlo/HarloWidgets/SessionLiveActivity.swift
[^shared-limits]: ios/Harlo/HarloWidgets/LimitsWidget.swift
