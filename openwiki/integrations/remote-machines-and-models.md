---
type: integration
title: Remote machines and model servers
description: SSH-backed project paths, per-machine helper deployment and state, model discovery, and bidirectional forwarding for managed inference servers.
tags: [ssh, remote, models, engines, configuration]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-14T05:16:59.652Z
sources:
  - id: openwiki-source-f1322101a0183d196da191a3
    resource: repo://crates/harlo-agent/src/providers.rs
  - id: openwiki-source-f3b7ddced6bd0e383fa959d1
    resource: repo://crates/harlo-app/qml/ModelServerDialog.qml
  - id: openwiki-source-9cb86229670695fb5654cb21
    resource: repo://crates/harlo-app/qml/SettingsDialog.qml
  - id: openwiki-source-6f4a8152df109bb50475dc85
    resource: repo://crates/harlo-app/src/accounts.rs
  - id: openwiki-source-be6e7bfc466a23d5de0a4a6d
    resource: repo://crates/harlo-core/src/agent_host.rs
  - id: openwiki-source-2c80b958b3431bd2cf32f669
    resource: repo://crates/harlo-core/src/bin/harlo-engines.rs
  - id: openwiki-source-f6f3d9d6055ad1850dd87043
    resource: repo://crates/harlo-core/src/engine_setup.rs
  - id: openwiki-source-2e13cea144aac4277e57e2b1
    resource: repo://crates/harlo-core/src/model_servers.rs
  - id: openwiki-source-3967b37bf3499aaa56517a6d
    resource: repo://crates/harlo-core/src/remote_helper.rs
  - id: openwiki-source-91f70adb7dede434bd1dac37
    resource: repo://crates/harlo-core/src/remote.rs
  - id: openwiki-source-5d94b2fabb592e050ff61e0a
    resource: repo://crates/harlo-core/src/servers.rs
generated: { by: "harlo", at: "2026-11-14T05:16:59.652Z" }
---

# Remote machines and model servers

## One path model, two execution locations

Remote projects, working directories, and session files are represented as `ssh://<host>/<absolute path>`. Core's `remote` module splits that identity into a configured `Host` and a real host path at disk/process boundaries. Code that simply manipulates project identity may keep the URI; code that opens files or launches processes must route through the host layer rather than treating it as a local filesystem path.

OpenSSH provides non-interactive authentication and shared connections. Default options include `ControlMaster=auto`, `ControlPersist=600`, `BatchMode=yes`, a 10-second connection timeout, and keepalives. New host keys are accepted by default and changed keys refused; configured host options precede defaults, so OpenSSH's first-value behavior matters. Credentials are SSH keys or an authentication agent, not an interactive password prompt in each file operation.

Explicitly disconnected hosts are tracked so background work does not immediately reconnect them. Remote non-interactive commands also receive a PATH prefix covering common agent installation directories.

## Deploying Harlo's own executables

`remote_helper::Helper` packages a matching x86_64 or aarch64 executable, strips it when possible, compresses it, and uploads a build-keyed copy to `~/.cache/harlo/<name>-<build>`. That copy is reused; older builds are removed. There are two principal consumers:

| Helper | Work performed on the destination |
| --- | --- |
| `harlo-agent` | Runs Harlo sessions using that machine's sign-ins, settings, and session logs |
| `harlo-engines` | Discovers, installs, starts, stops, and inspects model servers using that machine's engine/model folders |

Remote Harlo requires Linux and a supported architecture, plus a matching shipped static agent binary. Missing binaries produce a deployment error rather than assuming the desktop executable will run there. Claude instead resolves its program on the remote PATH. Session launches expose callbacks in `harlo-harness` so core can provide remote programs and forward model servers without making the harness depend on core.

This is machine-local state: signing in on the desktop does not mean the remote agent is signed in. Diagnose remote file/history/provider problems on the owning machine first.

## Model discovery is not server installation

`model_servers.rs` handles an existing OpenAI-compatible endpoint: it asks `/models`, offers the results, and writes selected models into Harlo's `$XDG_CONFIG_HOME/harlo-agent/providers.json`. Entries include base URL, API type, key, model IDs, and context metadata. Existing unrelated JSON is preserved, including ordering; a conflicting provider name is not silently replaced.

Discovery also imports listed prices: OpenRouter-style per-token `prompt`, `completion`, and optional `input_cache_read` become dollars per million tokens in model `cost` entries. Negative or invalid rates are not treated as known prices; missing cache rates default to input rates. Setting a server price updates every configured model (converting bare IDs into objects); clearing it removes explicit prices. `prices_as_listed_or_given` verifies discovery, round-trip persistence, overrides, removal, and unknown-server errors. Runtime cost classification remains the agent's responsibility.

`engine_setup.rs` owns the managed setup flow: obtain/use an engine, download a model, start it, and test a message. Local desktop setup and remote `harlo-engines` use the same core implementation. Remote setup reports JSON job state and output lines back to the desktop; it does not require a remote Qt window.

`servers.rs` retains managed server commands and log locations. `ensure_running` returns if healthy; otherwise it starts the server and waits up to 15 minutes for readiness. A detected faulted engine can be stopped even if its endpoint answers before being relaunched. Post-start callbacks can refresh model context configuration; problems in those callbacks are logged as warnings.

A server started by the remote helper survives that helper's exit. This is intentional: the helper returns once the long-lived server is available.

## Free servers versus unknown costs

A local address does not prove that inference is free: a loopback or LAN proxy may forward to a paid provider. Provider entries therefore carry an explicit `free` flag. Harlo's agent classifies an unpriced provider as no-extra-cost only when that flag is true; configured model prices, then server-listed prices, take precedence. Otherwise the cost is unknown, not a known zero. Plan-backed providers retain their separate plan classification.

Core defaults newly added servers to free only when the address is local/network-local and either no real API key is supplied (empty or `none`) or Harlo has a start command for that endpoint. The desktop exposes **Free: it runs on hardware you own** during addition and in Harlo's local-server settings, so the user can override that assumption. A price can also be supplied for a local paid proxy; the price editor is not restricted to online addresses.

Loading the desktop server list migrates Harlo provider entries missing `free` using the same core defaults. Entries already containing the field are left alone; repeated migration is a no-op. Migration write failures are logged as warnings. `servers_from_before_say_whether_they_are_free` checks keyless local servers, keyed local proxies, online endpoints, and preservation of an explicit false value.

Spending limits depend on reported costs: an unpriced, non-free server cannot be reliably counted toward them. Even with a price, a response that omits token usage has unknown cost; see [agent accounting](../concepts/agent-execution-and-safety.md). Reachability, placement, and billing classification are independent states.

## Sharing servers across machines

There are two directions of SSH forwarding:

- A desktop-local server can be forwarded to a remote machine for that machine's agents.
- A remote server can be given a local loopback address for desktop agents. Harlo records which host/remote URL that local address represents.

Before using an unreachable locally forwarded remote URL, `model_servers::reach` restores the forward, checks again, and if necessary requests `harlo-engines start` on the owning host. That remote start call has a 20-minute bound. Forward failures and helper-reported errors propagate to the caller rather than proving that an endpoint is usable just because a provider entry exists.

Keep the three states separate when debugging: **configured model**, **working SSH forward**, and **ready model process**. A good `/models` response at setup time does not guarantee later reachability.

## Extension seams and tests

New model engines belong in engine/setup/server machinery, while provider-file compatibility belongs in `model_servers` and the [agent provider layer](../concepts/agent-execution-and-safety.md). Keep machine-specific side effects behind the helper API. The [OpenWiki integration](openwiki.md) uses the same remote-routing principle but deploys a Node runtime instead of these Rust executables.

Focused tests cover provider discovery, preserving existing provider configuration, and supported remote agent platforms. SSH forwarding tests (`forwards_a_port_there`, `reaches_a_server_there`) are ignored by default and require `HARLO_TEST_SSH`; do not count ordinary unit tests as exercising a real tunnel.

```sh
cargo test -p harlo-core --no-default-features model_servers
cargo test -p harlo-core --no-default-features agent_host
cargo test -p harlo-agent what_a_model_costs
```

For a broader view of where the remote session feeds into the UI, see [sessions and desktop](../architecture/sessions-and-desktop.md).
