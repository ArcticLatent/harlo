---
type: Workflow
title: Desktop machines, providers, and phone access
description: Trace SSH setup and phone requests without confusing credential ownership, network transport, or Qt-thread authority.
covers:
  - crates/harlo-app/phone/
  - crates/harlo-app/src/phone.rs
  - crates/harlo-app/src/phone_server.rs
  - crates/harlo-app/src/phone_bridge.rs
  - crates/harlo-app/src/phone_relay.rs
  - crates/harlo-app/src/phone_funnel.rs
  - crates/harlo-app/src/phone_push.rs
  - crates/harlo-app/src/phone_activity.rs
  - crates/harlo-app/src/remotes.rs
  - crates/harlo-app/src/accounts.rs
  - crates/harlo-app/src/cloud_keys.rs
  - crates/harlo-app/src/setup.rs
  - crates/harlo-app/src/server_setup.rs
  - crates/harlo-app/src/updates.rs
  - crates/harlo-app/src/jobs.rs
  - crates/harlo-app/src/mcp.rs
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: remotes
    resource: "repo://crates/harlo-app/src/remotes.rs"
  - id: startup
    resource: "repo://crates/harlo-app/src/main.rs"
  - id: install
    resource: "repo://crates/harlo-app/src/remotes.rs"
    title: "fn Remotes::install_agent"
  - id: jobs
    resource: "repo://crates/harlo-app/src/jobs.rs"
  - id: setup
    resource: "repo://crates/harlo-app/src/setup.rs"
  - id: accounts
    resource: "repo://crates/harlo-app/src/accounts.rs"
  - id: server-setup
    resource: "repo://crates/harlo-app/src/server_setup.rs"
  - id: updates
    resource: "repo://crates/harlo-app/src/updates.rs"
  - id: mcp
    resource: "repo://crates/harlo-app/src/mcp.rs"
  - id: keys
    resource: "repo://crates/harlo-app/src/cloud_keys.rs"
  - id: server
    resource: "repo://crates/harlo-app/src/phone_server.rs"
  - id: greet
    resource: "repo://crates/harlo-app/src/phone_server.rs"
    title: fn greet
  - id: client
    resource: "repo://crates/harlo-app/phone/app.js"
  - id: relay
    resource: "repo://crates/harlo-app/src/phone_relay.rs"
  - id: funnel
    resource: "repo://crates/harlo-app/src/phone_funnel.rs"
  - id: ops
    resource: "repo://crates/harlo-app/src/phone_server.rs"
    title: fn session_op
  - id: bridge
    resource: "repo://crates/harlo-app/src/phone_bridge.rs"
  - id: slow
    resource: "repo://crates/harlo-app/src/phone_server.rs"
    title: fn slow_reply
  - id: push
    resource: "repo://crates/harlo-app/src/phone_push.rs"
  - resource: "repo://crates/harlo-app/phone/"
  - resource: "repo://crates/harlo-app/src/phone.rs"
  - resource: "repo://crates/harlo-app/src/phone_activity.rs"
---
# Desktop machines, providers, and phone access

The desktop is both a local UI and a host for remote control. SSH machines run agents/files remotely; phones control sessions through the desktop. Those are different boundaries: pairing a phone does not create an SSH host, and an SSH host connection does not authorize a phone.[^remotes][^server]

## Connect a machine and prepare an agent

Saved host definitions enter `harlo_core::remote` before sessions open. `Remotes` checks a host in a blocking task, publishes connecting/connected/error state on the Qt thread, and measures requests over the shared connection. Host paths are represented as `ssh://host/…`, with core helpers handling splitting, quoting, and execution.[^startup][^remotes]

Remote installs use a logged `Job`, reject duplicate in-progress installs for the same host/agent, wait up to 20 minutes, and reconnect afterward to discover the installed version. `jobs::wait` turns timeout/process failures into a concise message; “View output” follows the log while the process remains alive. Use that log when the last error line lacks context.[^install][^jobs]

Settings spread preparation across distinct adapters: `Setup` handles local agent installation and Git-host sign-in; `Accounts` handles agent/model-provider sign-in and server integration; `ServerSetup` installs an engine, obtains a model, starts and tests a server; `Updates` checks and updates agent versions; `Mcp` exposes MCP configuration to QML. They share core services, but one successful setup step does not imply the others succeeded.[^setup][^accounts][^server-setup][^updates][^mcp]

## Credential ownership

Agent sign-in uses the agent's own flow: Claude Code keeps its login token and Harlo's agent handles its own login. Cloud API keys are sent to the agent's key commands on **stdin**, including over SSH, not interpolated into process arguments. The app's settings page is not the canonical credential store. Preserve this distinction when adding retries, command logging, or remote setup.[^accounts][^keys]

## Pair and connect a phone

The Axum server embeds `phone/index.html`, JavaScript, styles, manifest, and TweetNaCl, and exposes `/ws`. The first message is a hello containing a public key and, for a new device, a one-time pairing code. `greet` allows 20 seconds for that message, rejects malformed keys, and requires an acceptable code for an unknown key before adding the device.[^server][^greet]

The browser client stores its device secret and paired host in local storage. A QR offer supplies the host key and optional relay identity; after reading it, the client removes the offer from browser history. After ready, application messages are NaCl-box encrypted. Their plaintext envelope includes connection identity and an increasing sequence number; the client rejects failed decryption, a different connection, or a non-increasing sequence.[^client]

**Security boundary:** encryption of application frames does not mean the entire connection is invisible. Hello/ready and relay routing exist outside the encrypted application payload. The host identity comes from pairing; transport reachability is not itself authorization. Browser storage holds a secret, so a browser profile is part of the device trust boundary.[^client][^greet][^relay]

## Direct and relayed transport share request semantics

`phone_relay` multiplexes phones over one desktop-to-relay WebSocket and gives each a `Link` consumed by the same `phone_server::connection` used for direct access. Relay reconnection backs off to 30 seconds and stops when its client handle is dropped. Funnel is a separate exposure option managed by the desktop, not a second session engine.[^relay][^funnel]

The phone server can answer live-session operations through the registry handle, such as abort, approval answers, model selection, and queue changes. Operations requiring Workspace authority cross `phone_bridge`: an unbounded request channel plus per-request oneshot reply, while session lists and limits use watch snapshots. A missing/closed Workspace returns a “harlo is closing” error rather than touching QObjects from the server task.[^ops][^bridge]

Slow work such as machine connection, model startup, diagrams, and wiki reads has separate async/blocking paths. A phone can make a session live without selecting its desktop tab; do not couple network requests to whatever conversation happens to be visible.[^slow][^bridge]

## Notifications are another channel

Push delivery uses device preferences and normally excludes phones already connected, unless explicitly asked to deliver to everyone. Pausing notifications suppresses ordinary pushes but not a test notification. A gone APNs token is removed; reconnecting/reopening the phone app can register another. Live Activities have separate lifecycle handling and are ended when the desktop quits.[^push][^startup]

For failures, distinguish: host SSH connectivity; local phone server availability; relay hosting; pairing/key acceptance; encrypted-frame validation; Workspace dispatch; and APNs delivery. A reachable relay does not prove a session exists, and a working phone socket does not prove push registration works. Related boundaries: [desktop lifecycle](/architecture/desktop.md), [session workflow](/workflows/desktop-sessions.md), [file operations](/workflows/desktop-files.md).[^relay][^greet][^bridge][^push]

[^remotes]: crates/harlo-app/src/remotes.rs
[^startup]: crates/harlo-app/src/main.rs
[^install]: crates/harlo-app/src/remotes.rs, fn Remotes::install_agent
[^jobs]: crates/harlo-app/src/jobs.rs
[^setup]: crates/harlo-app/src/setup.rs
[^accounts]: crates/harlo-app/src/accounts.rs
[^server-setup]: crates/harlo-app/src/server_setup.rs
[^updates]: crates/harlo-app/src/updates.rs
[^mcp]: crates/harlo-app/src/mcp.rs
[^keys]: crates/harlo-app/src/cloud_keys.rs
[^server]: crates/harlo-app/src/phone_server.rs
[^greet]: crates/harlo-app/src/phone_server.rs, fn greet
[^client]: crates/harlo-app/phone/app.js
[^relay]: crates/harlo-app/src/phone_relay.rs
[^funnel]: crates/harlo-app/src/phone_funnel.rs
[^ops]: crates/harlo-app/src/phone_server.rs, fn session_op
[^bridge]: crates/harlo-app/src/phone_bridge.rs
[^slow]: crates/harlo-app/src/phone_server.rs, fn slow_reply
[^push]: crates/harlo-app/src/phone_push.rs
