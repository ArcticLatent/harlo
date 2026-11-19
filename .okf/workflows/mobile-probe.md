---
type: Workflow
title: Mobile probe, recordings and test boundaries
description: Exercise HarloKit against a prepared desktop, replay captured sessions, and separate portable tests from iOS delivery and UI checks.
covers:
  - ios/HarloKit/Sources/harlo-probe/
  - ios/HarloKit/Tests/
  - ios/Harlo/HarloUITests/
  - ios/fixtures/
  - ios/HarloKit/Sources/HarloKit/Replay.swift
  - ios/HarloKit/Sources/HarloKit/Demo.swift
  - ios/HarloKit/Sources/HarloKit/Sample.swift
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: package
    resource: "repo://ios/HarloKit/Package.swift"
  - id: probe
    resource: "repo://ios/HarloKit/Sources/harlo-probe/main.swift"
  - id: tests
    resource: "repo://ios/HarloKit/Tests/HarloKitTests/HarloKitTests.swift"
  - id: relay-tests
    resource: "repo://crates/harlo-relay/src/lib.rs"
  - id: apns-tests
    resource: "repo://crates/harlo-apns/src/lib.rs"
  - id: replay
    resource: "repo://ios/HarloKit/Sources/HarloKit/Replay.swift"
  - id: replaying
    resource: "repo://ios/Harlo/Harlo/Replaying.swift"
  - id: fixture
    resource: "repo://ios/fixtures/recording.jsonl"
  - id: client
    resource: "repo://ios/HarloKit/Sources/HarloKit/Client.swift"
  - id: model
    resource: "repo://ios/HarloKit/Sources/HarloKit/Model.swift"
  - id: demo
    resource: "repo://ios/HarloKit/Sources/HarloKit/Demo.swift"
  - id: tour
    resource: "repo://ios/Harlo/Harlo/Tour.swift"
  - id: project
    resource: "repo://ios/Harlo/project.yml"
  - id: ui
    resource: "repo://ios/Harlo/HarloUITests/HarloUITests.swift"
  - resource: "repo://ios/HarloKit/Sources/harlo-probe/"
  - resource: "repo://ios/HarloKit/Tests/"
  - resource: "repo://ios/Harlo/HarloUITests/"
  - resource: "repo://ios/fixtures/"
  - resource: "repo://ios/HarloKit/Sources/HarloKit/Sample.swift"
---
# Mobile probe, recordings and test boundaries

There are three useful test boundaries: portable HarloKit tests, a real-protocol command-line phone, and simulator UI tests against a recording. They answer different questions. A replay can verify native presentation without proving connectivity; a push-file probe can verify encrypted payloads and notification actions without proving that Apple delivered a notification.[^package][^probe][^ui]

## Start with portable checks

From the repository root, the package and crate tests can be invoked independently of the iOS app:

```sh
swift test --package-path ios/HarloKit
cargo test -p harlo-relay -p harlo-apns
```

HarloKit's test target covers box round trips, wrong keys/tampering, replay and connection-ID rejection, offer parsing, endpoint ordering, row/event decoding, rich text, request answers, themes, widget snapshots, limits, workflows and wiki rendering data. Replay tests also load `ios/fixtures/recording.jsonl` relative to the test source, so keep the fixture tree alongside the package when running those tests.[^package][^tests]

Relay tests start local listeners and check routing, close behavior, connection limits, forwarded addresses and idle pings. Push tests use an HTTP/2 stand-in for Apple and generated credentials, testing topic/priority mapping, validation, rate limiting and gone tokens. APNS tests verify signed JWTs, malformed credentials, redacted Debug output and the delayed second attempt after a connection failure. They do not need production Apple credentials.[^relay-tests][^apns-tests]

## Run the real-protocol probe on disposable state

`harlo-probe` is a Swift executable target linked to the same HarloKit model as the app. It accepts a fresh pairing link, prints `IOS-E2E PASS`/`FAIL` checks, and exits 0 or 1 according to its accumulated failures (missing arguments exit 2). Quote the link so its fragment and query survive the shell.[^package][^probe]

```sh
swift run --package-path ios/HarloKit harlo-probe "$PAIRING_LINK"
```

**This is a scripted scenario, not a harmless generic health check.** It sends messages, answers approvals, creates and renames sessions, changes models/permissions, switches branches, and can restore files through undo. Use a disposable prepared desktop/project. The implementation expects a demo session, a project with `main` and `feature` branches, particular changed files such as `notes.txt`/`added.txt`, and a known session-mention fixture. A fresh code is needed because the scenario later deliberately tests that it has been spent.[^probe]

The probe's environment is part of that fixture contract. `HARLO_TEST_PUSH_FILE` must name the capture file the test desktop writes instead of submitting pushes to Apple; its absence records a failure. `HARLO_E2E_REPO`, when provided, enables the test that changes `notes.txt`, checks the undo plan and restores it. Merely pointing the probe at an ordinary working desktop does not establish these prerequisites.[^probe]

The main sequence pairs, lists and opens a session, receives streaming rich text and folded tool runs, asks a side question, answers an approval and a multi-option question, and answers from the waiting list with the transcript closed. It then covers new sessions/photos, permissions, model/thinking switches, todos/subagents, changes/diffs, action logs, file confinement, mentions, branch changes, queue promotion/removal and unsupported branching errors.[^probe]

Recovery checks deliberately drop the connection, reopen the same session, and construct a new model over the same `MemoryStore` to reconnect without a code. Reachability checks exercise announced addresses, 40 concurrent pings, a dead preferred address, network changes and migration to a better route. The migration check explicitly skips when there is no rank-1/Tailscale-style address. A new identity must be refused both with the spent code and without a code.[^probe]

The push subscenario registers a synthetic token, suspends the model, reads the capture file, decrypts an approval, verifies placeholder content, answers in the background, and waits for completion. It also registers a synthetic Live Activity token and inspects sealed update/end state. This proves the protocol/model path, not iOS authorization, Keychain entitlements, extension execution or production APNS delivery. Source comments call out Linux Foundation WebSocket large-frame limitations; short probe transcripts should not be treated as iPhone throughput coverage.[^probe]

## Record a deterministic UI input

Record mode attaches `Recorder.shared` before pairing and writes JSON Lines to the supplied output path:

```sh
swift run --package-path ios/HarloKit harlo-probe record "$PAIRING_LINK" /tmp/mobile-recording.jsonl
```

The format records `ready`, `{call,args,reply}`, `{ev:…}` and `{scene:…}` records. It captures decoded application data, not just opaque ciphertext: treat recordings as potentially sensitive transcripts. Recording is another prepared scenario, using Harlo tour models, a Claude session and a project fixture; it is not interchangeable with the demo-based regular probe.[^probe][^replay]

The checked-in fixture's scenes are `session`, `ask`, `approval`, `working`, `aside`, `plan`, `helpers`, `end`, `pictures`, `workflow-asks`, `workflow-check`, and `workflow`. Optional recording inputs include `HARLO_RECORD_PHOTO` for image interactions and `HARLO_RECORD_WORKFLOW` for a trigger file consumed by the desktop scenario. `HARLO_RECORD_DIAGRAMS` writes diagram requests for external rendering rather than claiming every large PNG is reliably transported through Linux Foundation. Compare the fixture test's expected scene list before replacing the committed recording.[^probe][^tests][^fixture]

`Replay` keeps events only up to the selected scene, but retains calls from the whole recording. Answers prefer matching arguments before the scene, then the same operation before it, then a recorded fallback. Diagrams require matching source, with light/dark preference; a missing open reply can synthesize a live key. Opening emits that session's recorded events; answering emits a synthetic cleared event. These conveniences make replay useful for UI coverage, but not a strict wire-level mock or a fresh execution of side effects.[^replay][^client]

Timestamp-like integer values in replies/events/ready are shifted so the recording's last time becomes now; request arguments stay unchanged for lookup. The replay model uses memory storage and remains connected across suspend/network lifecycle calls. The separate demo/sample path is synthetic, while `-tour` captures sample screens into `Documents/tour`; neither should be confused with a recording of real desktop responses.[^replay][^model][^demo][^tour]

## Exercise the native UI separately

XcodeGen's project includes the recording only in Debug bundles and removes it for Release. UI tests launch `-replay <scene>` and optionally pass `HARLO_TEXT_SIZE` as a preferred content-size category. They navigate screens, assert labels/state, check transcript width, and retain numbered screenshot attachments. Accessibility audits are collected as `audit-…` attachments and intentionally do not fail the test, so passing UI tests are not an accessibility certification.[^project][^ui]

If a scene cannot load, first check the Debug resource and scene name: `Replaying.model` returns an alert-bearing memory model when either is missing.[^replaying] If the real probe cannot connect, follow [pairing and reconnect](/architecture/mobile-protocol.md); if delivery fails outside the capture-file test, follow [relay/APNS diagnostics](/workflows/mobile-relay-apns.md) and [extension behavior](/architecture/mobile-notifications.md).

[^package]: ios/HarloKit/Package.swift
[^probe]: ios/HarloKit/Sources/harlo-probe/main.swift
[^tests]: ios/HarloKit/Tests/HarloKitTests/HarloKitTests.swift
[^relay-tests]: crates/harlo-relay/src/lib.rs
[^apns-tests]: crates/harlo-apns/src/lib.rs
[^replay]: ios/HarloKit/Sources/HarloKit/Replay.swift
[^replaying]: ios/Harlo/Harlo/Replaying.swift
[^fixture]: ios/fixtures/recording.jsonl
[^client]: ios/HarloKit/Sources/HarloKit/Client.swift
[^model]: ios/HarloKit/Sources/HarloKit/Model.swift
[^demo]: ios/HarloKit/Sources/HarloKit/Demo.swift
[^tour]: ios/Harlo/Harlo/Tour.swift
[^project]: ios/Harlo/project.yml
[^ui]: ios/Harlo/HarloUITests/HarloUITests.swift
