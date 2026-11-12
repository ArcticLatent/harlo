---
type: integration
title: Phone pairing, encrypted transport, and relay
description: Device authorization, replay-resistant NaCl channels, session subscriptions, relay metadata exposure, and encrypted APNs notifications across desktop and iPhone.
tags: [phone, ios, encryption, relay, apns]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-12T17:20:25.384Z
sources:
  - id: openwiki-source-3e5d9e2412052022c04ac0ec
    resource: repo://crates/harlo-apns/src/lib.rs
  - id: openwiki-source-5a140dd0fb2fd61495f082c8
    resource: repo://crates/harlo-app/src/phone_bridge.rs
  - id: openwiki-source-df3006c1d4f506624c91dfd2
    resource: repo://crates/harlo-app/src/phone_push.rs
  - id: openwiki-source-71daf39798f4072df6e58b8d
    resource: repo://crates/harlo-app/src/phone_server.rs
  - id: openwiki-source-33d58143535fd8586f3e5352
    resource: repo://crates/harlo-core/src/phone.rs
  - id: openwiki-source-f6c0ec21414aa65ae7ca1f61
    resource: repo://crates/harlo-relay/src/lib.rs
  - id: openwiki-source-8c23df65e9f6a257e8c67c14
    resource: repo://ios/HarloKit/Sources/HarloKit/Client.swift
generated: { by: "harlo", at: "2026-11-12T17:20:25.384Z" }
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

Core tests exercise identity persistence, two-way crypto, replay/wrong-connection/wrong-key rejection, device offers, and TweetNaCl interoperability.

```sh
cargo test -p harlo-core --no-default-features phone
swift test --package-path ios/HarloKit
```

See [testing and releases](../operations/testing-and-releases.md) for environment and packaging checks.
