---
type: Workflow
title: Deploying and diagnosing the mobile relay and APNS
description: Run the frame relay behind HTTPS, configure push signing, and distinguish routing, capacity, and Apple delivery failures.
covers:
  - crates/harlo-relay/
  - crates/harlo-apns/
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: relay
    resource: "repo://crates/harlo-relay/src/lib.rs"
  - id: main
    resource: "repo://crates/harlo-relay/src/main.rs"
  - id: docker
    resource: "repo://crates/harlo-relay/Dockerfile"
  - id: apns
    resource: "repo://crates/harlo-apns/src/lib.rs"
  - id: client
    resource: "repo://ios/HarloKit/Sources/HarloKit/Client.swift"
  - resource: "repo://crates/harlo-relay/"
  - resource: "repo://crates/harlo-apns/"
---
# Deploying and diagnosing the mobile relay and APNS

`harlo-relay` combines an HTTP/WebSocket routing service with optional APNS forwarding. `harlo-apns` is the reusable library: `Client` signs requests to Apple, while `Forwarder` submits a push to a relay that holds the team's signing key. These are separate from the native phone's pairing and application encryption; the relay does not possess the phone/computer box keys.[^relay][^apns]

## Deploy the transport first

Build the container from the repository root, not the crate directory: the relay embeds phone web assets from `crates/harlo-app`. The Dockerfile builds with `cargo build --release --locked -p harlo-relay`, then copies the binary into a distroless Debian nonroot image. The service listens on `0.0.0.0:8080` by default and expects HTTPS termination in front of it.[^docker][^main]

```sh
docker build -f crates/harlo-relay/Dockerfile -t harlo-relay .
docker run -p 8080:8080 harlo-relay
```

The binary accepts `--listen` or `HARLO_RELAY_LISTEN`. Only enable `--trust-forwarded` or `HARLO_RELAY_TRUST_FORWARDED=1` when clients cannot bypass the proxy. Otherwise arbitrary callers can supply the address used for limits. Trusted extraction prefers `CF-Connecting-IP`, then the first `X-Forwarded-For` address; without it, limits use the TCP peer. `--per-ip` and `--max-hosts` require positive integers. SIGTERM or Ctrl-C initiates graceful server shutdown.[^main][^relay]

`GET /health` returns `ok`; it is a process/HTTP check, not proof that a particular computer is connected or Apple accepts the key. `/` and the JS/CSS/manifest/icon routes serve the browser phone client with no-cache headers. `/privacy` and `/support` permanently redirect to the website. `GET /push` reports whether a push client is configured, independently of WebSocket hosting.[^relay]

## Understand routing and reconnect effects

A computer opens `/host` and must send `{"type":"host","secret":…}` within 15 seconds; secrets shorter than 32 bytes are rejected. Its relay ID is unpadded base64url of the first 16 bytes of SHA-256 of that secret. Successful admission returns `hosting` and the ID. A newer connection with the same ID replaces the old host and its phone senders. Generation checks prevent cleanup from the old connection deleting its replacement.[^relay]

A phone opens `/ws?id=…`. The relay allocates a per-host connection number and sends the computer `open`; text frames are wrapped as `data` with `c` and `d`, and disconnects as `close`. A missing host closes the upgraded phone socket with **4404**; a full host uses **4429**. The iOS client gives 4404 a specific explanation. Routing tables, bounded 256-message channels, connection counts and rate counters are in memory: there is no durable message queue or session recovery in this service.[^relay][^client]

Default limits are 10,000 hosts, 16 phones per host, 64 simultaneous connections per IP, 4 MiB maximum messages, 1 MiB/s incoming text budget with a 16 MiB burst, and 120 seconds without incoming activity. Pings count as activity. The byte budget sleeps to slow a sender rather than immediately disconnecting it; per-IP admission failures return HTTP 429 before upgrade. Only host count and per-IP count are exposed by this CLI; other limits are library configuration.[^relay][^main]

The relay forwards opaque text rather than authenticating phone application messages. It sees routing information, IPs, timing and frame sizes, and also forwards the plaintext application handshake. The [phone channel](/architecture/mobile-protocol.md), not this routing layer, authenticates sealed traffic and rejects replay/out-of-connection frames.[^relay][^client]

## Enable Apple delivery

Mount the app team's `.p8` key read-only and ensure container user 65532 can read it. Configure `HARLO_RELAY_APNS_KEY` and `HARLO_RELAY_APNS_TEAM`; set `HARLO_RELAY_APNS_KEY_ID` or use an `AuthKey_<ID>.p8` filename. Equivalent flags are `--apns-key`, `--apns-team` and `--apns-key-id`. `--apns-topic` changes the default `com.arcticlatent.harlo`; there is no topic environment variable in this CLI. A configured but unreadable/invalid key or missing required IDs stops startup with exit 2 rather than silently disabling push.[^docker][^main]

Credentials parse a P-256 PKCS#8 PEM key and require trimmed team/key IDs of length ten. ES256 JWTs contain `kid`, `iss` and `iat`, with a cached token renewed after 40 minutes. HTTP requests time out after 15 seconds. Device registrations choose sandbox versus production Apple endpoints; alert requests use push type `alert`, priority 10 and the configured bundle topic. Live Activities append `.push-type.liveactivity`, use type `liveactivity`, and choose priority 10 when urgent or 5 otherwise. `HARLO_TEST_APNS_URL` overrides Apple's endpoint for a test server and should not be set in production.[^apns][^main]

The relay's `POST /push` validates a nonempty hexadecimal token of at most 200 characters, an object `aps`, and a serialized payload no larger than 4096 bytes. Activities additionally require event `update` or `end`, object `content-state`, and integer timestamp. Tokens are lowercased and collapse IDs truncated to 64 characters. The relay fixes the topic itself; callers cannot select arbitrary topics. It does **not** require an active host connection or validate a host credential on this HTTP route, and structural validation is not proof that a payload is encrypted.[^relay][^apns]

## Interpret failures before retrying

| Observation | Implemented meaning |
| --- | --- |
| `GET /push` says false; POST returns 404 | No signing client configured. |
| POST returns 400 | Forwarded token/payload/activity validation failed. |
| POST returns 429 | Per-token or per-IP push budget exhausted. |
| POST returns 410 | Apple considers the token gone/invalid for this destination. |
| POST returns 502 | Apple rejection or transport failure, not a successful delivery. |
| POST returns 200 | Apple request succeeded; not evidence the device displayed it. |

These mappings come from the relay's send result. Apple HTTP 410 or reasons `BadDeviceToken`, `DeviceTokenNotForTopic`, `Unregistered` become `Sent::Gone`; other rejections become `Failed`. Default push budgets are 30 per token and 300 per IP in a shared, elapsed-60-second counter window, not a durable rolling history.[^relay][^apns]

`Forwarder.send` retries once after three seconds **only** when connection establishment fails. It does not retry arbitrary HTTP errors or timeouts, which could duplicate an already accepted notification. Its availability check only accepts a successful response with `push: true`; 404/405 without a reason on submission yields an older-relay explanation. There is no durable retry worker in either crate.[^apns]

For device-side decryption and fallback see [notifications and widgets](/architecture/mobile-notifications.md). For local HTTP/2 stand-in tests and the distinction between simulated push and real delivery see [probe and replay](/workflows/mobile-probe.md).

[^relay]: crates/harlo-relay/src/lib.rs
[^main]: crates/harlo-relay/src/main.rs
[^docker]: crates/harlo-relay/Dockerfile
[^apns]: crates/harlo-apns/src/lib.rs
[^client]: ios/HarloKit/Sources/HarloKit/Client.swift
