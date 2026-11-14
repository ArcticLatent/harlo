---
type: integration
title: Phone pairing, encrypted transport, and relay
description: Device authorization, replay-resistant NaCl channels, session subscriptions, relay metadata exposure, and encrypted APNs notifications across desktop and iPhone.
tags: [phone, ios, encryption, relay, apns]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-14T05:16:59.652Z
sources:
  - id: openwiki-source-3e5d9e2412052022c04ac0ec
    resource: repo://crates/harlo-apns/src/lib.rs
  - id: openwiki-source-1d40907d1133795813cede65
    resource: repo://crates/harlo-app/src/main.rs
  - id: openwiki-source-9f29fe751f9cc19b273be74f
    resource: repo://crates/harlo-app/src/phone_activity.rs
  - id: openwiki-source-5a140dd0fb2fd61495f082c8
    resource: repo://crates/harlo-app/src/phone_bridge.rs
  - id: openwiki-source-df3006c1d4f506624c91dfd2
    resource: repo://crates/harlo-app/src/phone_push.rs
  - id: openwiki-source-71daf39798f4072df6e58b8d
    resource: repo://crates/harlo-app/src/phone_server.rs
  - id: openwiki-source-fb73cab443b13627be975e74
    resource: repo://crates/harlo-app/src/phone.rs
  - id: openwiki-source-33d58143535fd8586f3e5352
    resource: repo://crates/harlo-core/src/phone.rs
  - id: openwiki-source-4db2ffefa327cd504d9b5a66
    resource: repo://crates/harlo-core/src/session.rs
  - id: openwiki-source-a9697f23b0ad71a5ad25a2a8
    resource: repo://crates/harlo-core/src/transcript.rs
  - id: openwiki-source-f6c0ec21414aa65ae7ca1f61
    resource: repo://crates/harlo-relay/src/lib.rs
  - id: openwiki-source-2a9076f4b9c96d8d098111b9
    resource: repo://ios/Harlo/Harlo/SessionView.swift
  - id: openwiki-source-d106fff79d94df591057ff07
    resource: repo://ios/Harlo/Harlo/TranscriptViews.swift
  - id: openwiki-source-7d63dd608fb68281fc2c1857
    resource: repo://ios/Harlo/Harlo/WikiViews.swift
  - id: openwiki-source-8c23df65e9f6a257e8c67c14
    resource: repo://ios/HarloKit/Sources/HarloKit/Client.swift
  - id: openwiki-source-d4c9b9a27de9af9dcb466113
    resource: repo://ios/HarloKit/Sources/HarloKit/Messages.swift
  - id: openwiki-source-17a3c203c4ef7ce0399eb7de
    resource: repo://ios/HarloKit/Sources/HarloKit/Model.swift
  - id: openwiki-source-c36579598b4c49e452d83358
    resource: repo://ios/HarloKit/Sources/HarloKit/RichText.swift
  - id: openwiki-source-8da0f5a3c9cfbd4d66280fb2
    resource: repo://ios/HarloKit/Sources/HarloKit/Visuals.swift
  - id: openwiki-source-e36aee686debea4d83dc020e
    resource: repo://ios/HarloKit/Tests/HarloKitTests/HarloKitTests.swift
generated: { by: "harlo", at: "2026-11-14T05:16:59.652Z" }
---

# Phone pairing, encrypted transport, and relay

## Responsibility map

- Core `phone.rs` owns identity, devices, pairing offers, crypto channels, and frame replay checks.
- Desktop `phone_server.rs` serves the phone web app and `/ws`, handles requests, and subscribes to live sessions. `phone_bridge.rs` carries workspace-owned operations to the Qt side.
- `ios/HarloKit` implements the client protocol independently of SwiftUI. `HarloClient` is an actor managing one connection; reconnecting is its caller's responsibility.
- `harlo-relay` forwards encrypted frames and optionally delivers push notifications through `harlo-apns`.

This boundary allows LAN, Tailscale, and relay connections to share application encryption rather than relying on the relay to authorize or interpret session commands.

## Pairing and connection lifecycle

1. Desktop has a long-lived key pair; a QR pairing offer contains its public key and a one-time code.
2. The phone generates its own key pair and sends a plaintext hello containing its public key, code, and device name on first connection.
3. Desktop registers the device and responds `ready` with a connection ID, or `denied`. Pairing codes have a 600-second lifetime and are consumed on use.
4. Later connections authenticate by the registered public key, without needing the pairing code again. Removing a device revokes authorization.
5. After the handshake, requests/replies/events are boxed using X25519 plus XSalsa20-Poly1305, compatible with TweetNaCl.

Core keeps phone identity/device files under the app data directory's `phone` subdirectory. Private writes use a temporary file, Unix mode 0600, and rename.

Each sealed connection message contains the connection ID, a monotonically increasing sequence number, and the application message. `Session::open` rejects another connection's ID and any sequence number no greater than the last accepted one. This is monotonic anti-replay validation, not a requirement that sequence numbers be contiguous. Static peer keys authenticate the channel; do not describe this design as an ephemeral-key forward-secrecy protocol.

## From phone requests to shared sessions

Phone requests carry `{id, op, …}` and receive correlated replies; asynchronous updates carry `{ev, …}`. Operations include listing/opening sessions, send/abort/answer, project/session creation, modes, models, files, diffs, and branches. A phone opening a session subscribes to the existing session handle, receiving reset/row/status/meta/request updates rather than running a parallel agent.

The workspace bridge owns actions requiring desktop application state, including making a session live without switching the visible tab, starting one, renaming/archiving, and sending with plan-mode context. Its watch channel publishes the session list only when it changes. See [sessions and desktop](../architecture/sessions-and-desktop.md) for snapshot/delta semantics.

If Stop arrives while a new turn is still waiting for its undo snapshot, an `unsent` event returns the held messages' original words and inline photos. Desktop-only file attachments are not converted into phone attachments. HarloKit routes the event by session key, decodes valid base64 photos, prepends returned text above any newer draft, and queues photos for the composer. The SwiftUI composer puts returned photos before those selected meanwhile and clears the queue; a photo-only return leaves draft text unchanged.

A phone subscription advertises that it has a composer. It can therefore receive messages returned after a failed agent start, as well as cancelled preparation. If recovery happened with no composer attached, the session retains the messages in memory and gives them to the first desktop or phone composer that subscribes; passive workspace watchers do not consume them.

Long desktop code blocks are split into continuation rows by the shared transcript. Phone serialization marks later code pieces as `codePart`, which the iOS view hides; the first code row carries both combined copyable code and combined highlighted HTML. The server joins matching consecutive pieces into one `<pre>` block, rather than displaying only the first desktop piece. Reset frames use this joined representation, and insert/update deltas resend an affected first row after the other changes when it is within the phone's window. Hidden continuation rows remain in the protocol list to preserve row positions.

The HTML join preserves each piece's own trailing blank lines and restores only the separator newline between pieces. It therefore preserves blank lines at piece boundaries and at the end, rather than trimming them away. HarloKit preserves those trailing newlines in Harlo's direct `<pre>` blocks; it removes one renderer-added newline only when the block contains a single `<code>` wrapper, as in nested Markdown code.

This does not remove transport bounds: reset starts with the latest 300 rows and frames are grouped at about 512 KiB. Ordinary row text and HTML use a 64 KiB clipping threshold with a continuation notice. Code blocks, whether whole or joined from pieces, instead use a separate 256 KiB HTML threshold: the server cuts at the last newline before that threshold when available, adds an omitted-line notice, and closes the `<pre>` wrapper. The notice and closing markup can extend the result slightly past the threshold. Copyable code bypasses the ordinary text limit and is reduced to the same retained line count when HTML is truncated; the omitted-line notice is not copied. The first row of a code block outside that window is not pulled into it merely because a later piece is visible.

## Wiki access from a phone session

The session menu opens **Wiki** in its own sheet, independently of the shared plan. A single navigation stack owns page links from rows, search hits, and graph boxes. The wiki view stays at the root even while loading or when no wiki exists, preserving the search field when returning from a page. Loading is requested once per sheet instance rather than on every page return.

The sheet presents freshness and update/continue actions with the page browser, or creation guidance when the repository has no wiki. Sessions outside a Harlo project instead show that no project wiki is available. Opening the session writing the wiki dismisses the sheet and opens that session; generation still runs on the computer through the [OpenWiki lifecycle](openwiki.md), not on the phone.

The UI test `test09bWiki` covers menu access, page and graph navigation, provenance, return-to-search behavior, and search results.

## Relay trust and deployment

The desktop connects to `/host` with a host secret. Its routing ID is base64url of the first 16 bytes of SHA-256 of that secret. Phones connect to `/ws?id=<id>`; the relay multiplexes open/data/close frames with a connection number. The secret protects ownership of the relay routing slot; device authorization still belongs to desktop pairing.

The relay cannot read the sealed content, but sees IP addresses, timing, and frame sizes. It maintains routing state in memory rather than persisting conversations. Resource limits cover hosts, phones per host, connections per IP, frame sizes, bandwidth, idle connections, and notification rates.

```sh
cargo build --release -p harlo-relay
target/release/harlo-relay --listen 127.0.0.1:8080
```

Place HTTPS in front of the plain-HTTP relay. Use `--trust-forwarded` only behind a trusted proxy that is the only route in; otherwise forwarded headers can undermine address-based limits. The CLI also supports `HARLO_RELAY_LISTEN`, `--per-ip`, and `--max-hosts`. A desktop build can override the default `https://relay.harlo.run` using `HARLO_DEFAULT_RELAY`.

## Push is a separate delivery path

A relay with the app team's APNs credentials accepts `/push`; `GET /push` reports whether push is available. Without an APNs key, encrypted session transport still works. Configure `--apns-key`, `--apns-key-id` (or the Apple filename convention), and `--apns-team`; the default topic is `com.arcticlatent.harlo`.

Desktop seals notification content for the phone before forwarding it, leaving the relay and Apple a placeholder. The notification extension decrypts the content. Currently connected phones do not receive these ordinary pushes because the app is already open. Live Activity updates use their own APNs tokens and sealed content as well. `harlo-apns` signs provider tokens with ES256 and renews them after 40 minutes.

## Verification

Live Activity registrations are also recorded in the desktop phone data directory's `activities.json`. Startup and shutdown attempt to end leftover cards with encrypted “harlo was closed” content and immediate dismissal; shutdown waits at most three seconds. The persisted list is consumed before delivery, so this is best-effort cleanup, not a durable retry queue. `cards_left_over_are_ended` checks registration replacement/removal, encrypted end payloads, immediate dismissal, and one-time consumption. On returning to the iPhone app, ended cards are dismissed and active/stale cards are reconciled with session status.

Option-card inline text is parsed into styled spans in HarloKit (code, bold, italic, strike, and HTTP(S) links). Unclosed marks remain text, and the plain-text projection supports reading the same content aloud.

Core tests exercise identity persistence, two-way crypto, replay/wrong-connection/wrong-key rejection, device offers, and TweetNaCl interoperability.

`stopped_messages_reach_phones` checks ordered original words and pictures in the desktop event. HarloKit's `testStoppedMessagesComeBack` checks decoding, rejection of invalid base64 photos, preservation of newer draft text, and photo-only recovery.

`a_long_code_block_reaches_phones_whole` checks reset delivery at 149/150/151/320 lines and streamed growth through piece boundaries, applying the frames as a phone does and comparing shown lines, copy text, and retained row counts.

`a_long_code_blocks_blank_lines_reach_phones` checks blank lines at piece boundaries and the end, including escaped characters in plain and highlighted code. `a_huge_code_block_is_cut_after_a_line` checks the larger HTML threshold, balanced highlighting spans, retained initial lines, and omitted-line count.

`a_big_code_block_copies_what_it_shows` covers both split and unsplit code exceeding the ordinary row limit. The huge-block test also compares copied lines with displayed lines. HarloKit's `testACodeBlocksBlankLinesAreKept` distinguishes direct code-block trailing blanks from the newline added by a nested Markdown wrapper.

```sh
cargo test -p harlo-core --no-default-features phone
swift test --package-path ios/HarloKit
```

See [testing and releases](../operations/testing-and-releases.md) for environment and packaging checks.
