---
type: Architecture
title: Mobile pairing, encryption and reconnect
description: The phone's trust bootstrap, sealed message format, endpoint selection, and foreground recovery behavior.
covers:
  - ios/HarloKit/Sources/CTweetNaCl/
  - ios/HarloKit/Sources/HarloKit/Pairing.swift
  - ios/HarloKit/Sources/HarloKit/Crypto.swift
  - ios/HarloKit/Sources/HarloKit/Client.swift
  - ios/HarloKit/Sources/HarloKit/Race.swift
  - ios/HarloKit/Sources/HarloKit/NetworkWatch.swift
  - ios/HarloKit/Sources/HarloKit/Store.swift
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: pairing
    resource: "repo://ios/HarloKit/Sources/HarloKit/Pairing.swift"
  - id: model
    resource: "repo://ios/HarloKit/Sources/HarloKit/Model.swift"
  - id: crypto
    resource: "repo://ios/HarloKit/Sources/HarloKit/Crypto.swift"
  - id: store
    resource: "repo://ios/HarloKit/Sources/HarloKit/Store.swift"
  - id: client
    resource: "repo://ios/HarloKit/Sources/HarloKit/Client.swift"
  - id: adapter
    resource: "repo://ios/HarloKit/Sources/CTweetNaCl/harlo_nacl.c"
  - id: race
    resource: "repo://ios/HarloKit/Sources/HarloKit/Race.swift"
  - id: watch
    resource: "repo://ios/HarloKit/Sources/HarloKit/NetworkWatch.swift"
  - resource: "repo://ios/HarloKit/Sources/CTweetNaCl/"
---
# Mobile pairing, encryption and reconnect

There are two independent concerns: reaching the computer and authenticating encrypted application traffic. A direct WebSocket, Funnel endpoint or relay can carry the same phone protocol. The phone persists a static identity and the paired computer's public key; changing routes does not create a new pairing.[^pairing][^model][^crypto]

## Bootstrap and saved trust

A scanned/pasted offer is either base64url JSON or an HTTP(S) URL whose fragment contains `p=`. Version 1 fields are `k` (32-byte public key), `c` (pairing code), `n` (computer name), `r`/`i` (relay base and ID), and `e` (alternate endpoints). The URL's origin is retained, not its path. If a relay is specified it is the initial endpoint, otherwise the origin becomes `/ws`, with HTTP mapped to WS and HTTPS to WSS. Relay URLs become `/ws?id=…`.[^pairing]

Pairing initially tries that one selected endpoint, not an endpoint race. It sends the code and device name; after success it merges offer addresses and announced addresses into `PairedHarlo`. Scanning another offer with the same public key augments the existing host rather than clearing its session selection. Only an explicit offline error is retried during pairing, once a second for up to the configured 45-second window: this accommodates iOS's Local Network permission prompt without treating every refusal as transient.[^model]

The store generates a 32-byte phone secret if none is readable, derives its public key, and saves the secret as `identity`. `host` stores the computer key, name, endpoints, last successful endpoint and pairing date. On Apple platforms the Keychain uses `AfterFirstUnlockThisDeviceOnly`, allowing background access after the first unlock without backup migration. Local Forget removes the host association but retains the phone identity and does not remove the computer's record of the phone.[^store][^model][^pairing]

## Handshake and sealed frames

The client sends an unencrypted JSON `hello` containing its public key and supported display features (`limits`, `settled`); a nonempty pairing code adds `code` and `name`. The initial reply is also parsed as plaintext. `ready` supplies `conn`, computer/device names, endpoints and features; `denied` terminates the attempt. **Do not claim that the handshake itself is sealed:** the encrypted channel is constructed only after `ready`, using the computer key already obtained from the offer.[^client]

The C adapter wraps TweetNaCl's Curve25519/XSalsa20-Poly1305 box, translating its padded buffers into a compact wire representation. Swift's `Box` emits standard base64 of a 24-byte nonce followed by the 16-byte authenticator and ciphertext. Its decoder also accepts URL-safe and unpadded base64. Keys are static; this implementation does not perform an ephemeral-key exchange on each connection.[^crypto][^adapter]

Each connection seals JSON `{c, s, m}`: connection ID, sequence number and application message. Send and receive counters are independent. Opening rejects the wrong connection ID or a sequence no greater than the last received; it accepts increasing numbers, not only consecutive ones. This is separate from request IDs inside `m`. Outgoing sends are chained in seal order so concurrently issued calls cannot arrive out of order and trigger replay rejection.[^crypto][^client]

Authenticated-decryption/frame failures end the stream and close the socket with a protocol error. Shutdown fails pending request continuations and records `closeReason`. The client sends application pings every 30 seconds but does not itself own reconnect; callers can use a bounded `alive` ping to detect a socket that looks connected after a network change.[^client]

## Endpoint selection and recovery

`PairedHarlo.tryOrder` ranks private/link-local/localhost/`.local` WS addresses first, other WS addresses second, WSS third, and a URL with `id=` in its query last. This is a syntactic heuristic, not reachability discovery or proof that a route is Tailscale/Funnel. The last successful endpoint wins ties. Announced endpoints remove obsolete WS addresses except the last successful one; previously known non-WS addresses remain.[^pairing]

For reconnect, `Race.first` starts the first ordered candidate immediately and all others after a 0.3-second head start. Direct WS attempts have a default 4-second timeout and other attempts 10 seconds. First success wins, late successes close, a denial ends the race immediately, and total failure reports the earliest candidate's error. Failed connection rounds back off 1, 2, 4, 8, then at most 15 seconds; an established stream ending pauses 500 ms before another round.[^race][^model]

On success the model re-registers push preferences, reloads sessions and supported limits, reopens the selected session when foregrounded, and refreshes theme. A denial or `revoked` event forgets the host and asks for pairing again. Foreground resume starts recovery; background suspend deliberately hangs up rather than promising an always-on iOS socket.[^model]

`NetworkWatch` ignores its initial report and reacts to status/interface-name changes. The model tests an existing connection with a 3-second ping, closes a dead one, or looks for a better route if it still answers. It also checks every 60 seconds for strictly better-ranked endpoints. Migration connects the replacement first, verifies that the old connection is still current and the app active, then starts the loop with the new client and closes the old one. The new loop refreshes state and reopens the session; establishing the replacement does not mean that refresh has already completed.[^watch][^model]

For relay-level routing and close codes see [deployment behavior](/workflows/mobile-relay-apns.md). Push content uses the same static-key box without the live connection envelope; see [notifications](/architecture/mobile-notifications.md). The [probe](/workflows/mobile-probe.md) exercises spent-code refusal, concurrent requests and reconnect behavior.

[^pairing]: ios/HarloKit/Sources/HarloKit/Pairing.swift
[^model]: ios/HarloKit/Sources/HarloKit/Model.swift
[^crypto]: ios/HarloKit/Sources/HarloKit/Crypto.swift
[^store]: ios/HarloKit/Sources/HarloKit/Store.swift
[^client]: ios/HarloKit/Sources/HarloKit/Client.swift
[^adapter]: ios/HarloKit/Sources/CTweetNaCl/harlo_nacl.c
[^race]: ios/HarloKit/Sources/HarloKit/Race.swift
[^watch]: ios/HarloKit/Sources/HarloKit/NetworkWatch.swift
