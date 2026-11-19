---
type: Architecture
title: Agents, model servers, and engine setup
description: The distinct agent and inference-engine lifecycles, shared local/remote setup logic, and configuration translation boundaries.
covers:
  - crates/harlo-core/src/agents.rs
  - crates/harlo-core/src/providers.rs
  - crates/harlo-core/src/signin.rs
  - crates/harlo-core/src/quota.rs
  - crates/harlo-core/src/updates.rs
  - crates/harlo-core/src/engines.rs
  - crates/harlo-core/src/engine_setup.rs
  - crates/harlo-core/src/engine_host.rs
  - crates/harlo-core/src/servers.rs
  - crates/harlo-core/src/model_servers.rs
  - crates/harlo-core/src/agent_host.rs
  - crates/harlo-core/src/remote_helper.rs
  - crates/harlo-core/src/bin/harlo-engines.rs
  - crates/harlo-core/src/mcp.rs
  - crates/harlo-core/src/mcp_catalog.rs
  - crates/harlo-core/src/mcp_probe.rs
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:51:01Z" }
sources:
  - id: providers
    resource: "repo://crates/harlo-core/src/providers.rs"
  - id: engines
    resource: "repo://crates/harlo-core/src/engines.rs"
  - id: setup
    resource: "repo://crates/harlo-core/src/engine_setup.rs"
  - id: servers
    resource: "repo://crates/harlo-core/src/servers.rs"
  - id: models
    resource: "repo://crates/harlo-core/src/model_servers.rs"
  - id: args
    resource: "repo://crates/harlo-core/src/agents.rs"
  - id: signin
    resource: "repo://crates/harlo-core/src/signin.rs"
  - id: quota
    resource: "repo://crates/harlo-core/src/quota.rs"
  - id: updates
    resource: "repo://crates/harlo-core/src/updates.rs"
  - id: helper
    resource: "repo://crates/harlo-core/src/bin/harlo-engines.rs"
  - id: wait
    resource: "repo://crates/harlo-core/src/servers.rs"
    title: fn wait_up
  - id: needs
    resource: "repo://crates/harlo-core/src/servers.rs"
    title: fn needs_start
  - id: launch
    resource: "repo://crates/harlo-core/src/servers.rs"
    title: fn launch
  - id: add
    resource: "repo://crates/harlo-core/src/model_servers.rs"
    title: fn with_server
  - id: free
    resource: "repo://crates/harlo-core/src/model_servers.rs"
    title: fn free_by_default
  - id: forward
    resource: "repo://crates/harlo-core/src/model_servers.rs"
  - id: remote-helper
    resource: "repo://crates/harlo-core/src/remote_helper.rs"
  - id: engine-host
    resource: "repo://crates/harlo-core/src/engine_host.rs"
  - id: agent-host
    resource: "repo://crates/harlo-core/src/agent_host.rs"
  - id: mcp
    resource: "repo://crates/harlo-core/src/mcp.rs"
  - id: catalog
    resource: "repo://crates/harlo-core/src/mcp_catalog.rs"
  - id: probe
    resource: "repo://crates/harlo-core/src/mcp_probe.rs"
  - id: remove
    resource: "repo://crates/harlo-core/src/servers.rs"
    title: fn remove
---
# Agents, model servers, and engine setup

An **agent** executes a conversation through a harness; an **inference engine** serves models through a model-server endpoint. They have separate installation, configuration, and process lifecycles. `providers` checks agent availability/models, `engines` discovers hardware and engine installations, `engine_setup` performs setup/update jobs, `servers` owns saved launch/stop commands, and `model_servers` translates chosen endpoints/models into the agent's provider configuration.[^providers][^engines][^setup][^servers][^models]

## Agent boundary

Harlo launch arguments encode its own tools and sandbox/network choices; other agents do not receive Harlo-specific flags. Provider checks use a curated Claude model list and a short ephemeral Harlo RPC run that sends no prompt. A discovery probe therefore differs from starting a persisted user session.[^args][^providers]

Sign-in handling parses the agents' own command output into links, codes, and prompts. Agents retain their credentials; the core presents the interaction rather than becoming the credential authority. Quota queries likewise use the agent's own paths: Harlo's usage command/live reports and Claude's own usage operation. The updates module checks Claude's published npm version and delegates installation to the agent's update command.[^signin][^quota][^updates]

## Normalize setup reports, not engine implementations

The setup layer emits `Event::Job` state and `Event::Output` text through `Report`. It can report steps, progress, details, and server log output; setup/update operations accept an atomic cancellation flag. The desktop invokes this logic locally, while `harlo-engines` invokes it remotely and encodes reports as JSON lines. This shares job semantics without requiring the remote machine to run a UI.[^setup][^helper]

The engine catalog distinguishes Ollama, llama.cpp, and Strata. Their installers and launch commands are engine-specific: Ollama release archives are checked against its checksum file, llama.cpp builds use GitHub's SHA-256, and Strata uses its pinned installer. Harlo's engine versions live under its data directory; Strata uses its own usual installation paths. Setup is user-level rather than a root installer.[^engines]

A server is not ready merely because its launch process started. `wait_up` polls whether the endpoint answers and watches the held child, cancellation, and timeout. A successful launcher may exit while the real server keeps running, so the result also reports whether Harlo still holds that child. `needs_start` checks both endpoint availability and recognized engine faults.[^wait][^needs]

Local server launch uses a process group and a parent-death signal; detached mode creates a new session. `harlo-engines` explicitly selects detached mode so servers survive the SSH command that started them. This differs from the [harness process ownership](/architecture/harness.md) of an interactive agent session.[^launch][^helper]

## Provider configuration is an adapter

Discovery asks an OpenAI-compatible server for its models. The selected models are represented with IDs, context windows, and optional pricing, then written into Harlo's provider configuration with a base URL, API dialect, and key. Unrelated JSON content is retained; duplicate provider names are rejected, except that the same name and base URL can replace that server's entry during setup again.[^models][^add]

Prices are dollars per million tokens in this configuration. A local/private-network URL alone is not sufficient to classify a keyed gateway as free: the default additionally requires no meaningful key, or a server Harlo itself starts. A local gateway may forward requests to a paid service; network location and billing ownership must not be conflated.[^models][^free]

The same machine distinction applies to forwarding. `model_servers` can configure a host to reach this computer's server, or expose a host's server through a local URL; it records the correspondence and restores forwards when needed. Do not persist a temporary local forwarding URL as though it were the original remote server's identity.[^forward]

## Remote helper ownership

`remote_helper` selects Harlo's build for x86_64 or aarch64, strips/compresses it, and uploads it over SSH when that build is absent. Helpers live in the host's cache under a build-qualified name, with older builds cleaned up. `engine_host` uses this for `harlo-engines`; `agent_host` uses it for Harlo's agent and places the code index beside it when needed.[^remote-helper][^engine-host][^agent-host]

Remote agents retain their own sign-in, settings, and sessions on the host. Remote engines likewise operate on that machine's model files and server records. The helper protocol exposes catalog, updates, overview, setup/cancel, start/stop, logs, free-port selection, and removal; it is a control channel, not a local copy of the remote filesystem.[^agent-host][^engine-host][^helper]

## MCP normalization has limits too

The core offers a common MCP settings view while preserving each agent's dialect. Claude global entries are changed with `claude mcp add-json/remove`, because Claude rewrites its global configuration while running; project entries use `.mcp.json`. Harlo uses its own global/project files and project-entry approval. Harlo timeouts are seconds, while Claude has no matching folder/timeout/off-switch fields; unknown fields such as OAuth settings are retained.[^mcp]

The catalog downloads supported MCP servers from their releases with published SHA-256 verification, exposes a stable `current` link, and retains the previous version for running sessions. It resolves a sufficiently new Node installation explicitly instead of relying on a desktop-launched process inheriting a developer shell's PATH. `mcp_probe` checks actual initialization/tool listing over stdio, streamable HTTP, or HTTP+SSE because harness RPC does not report that status.[^catalog][^probe]

## Operational pitfalls

- Keep installation, endpoint readiness, provider registration, and a successful model request distinct when diagnosing setup; they occur in different modules/stages.[^setup][^wait][^models]
- A remote helper's process exit does not imply its detached model server stopped.[^helper][^launch]
- Reusing the common settings UI does not make Claude support every Harlo-specific provider or MCP field.[^add][^mcp]
- Treat server/model deletion as its own operation: `servers::remove` takes an explicit `delete_files` decision and returns errors plus a summary.[^remove]

See [Git and remote execution](/workflows/git-and-remote.md) for SSH routing and [core ownership](/architecture/core.md) for the wider application boundary.

[^providers]: crates/harlo-core/src/providers.rs
[^engines]: crates/harlo-core/src/engines.rs
[^setup]: crates/harlo-core/src/engine_setup.rs
[^servers]: crates/harlo-core/src/servers.rs
[^models]: crates/harlo-core/src/model_servers.rs
[^args]: crates/harlo-core/src/agents.rs
[^signin]: crates/harlo-core/src/signin.rs
[^quota]: crates/harlo-core/src/quota.rs
[^updates]: crates/harlo-core/src/updates.rs
[^helper]: crates/harlo-core/src/bin/harlo-engines.rs
[^wait]: crates/harlo-core/src/servers.rs, fn wait_up
[^needs]: crates/harlo-core/src/servers.rs, fn needs_start
[^launch]: crates/harlo-core/src/servers.rs, fn launch
[^add]: crates/harlo-core/src/model_servers.rs, fn with_server
[^free]: crates/harlo-core/src/model_servers.rs, fn free_by_default
[^forward]: crates/harlo-core/src/model_servers.rs
[^remote-helper]: crates/harlo-core/src/remote_helper.rs
[^engine-host]: crates/harlo-core/src/engine_host.rs
[^agent-host]: crates/harlo-core/src/agent_host.rs
[^mcp]: crates/harlo-core/src/mcp.rs
[^catalog]: crates/harlo-core/src/mcp_catalog.rs
[^probe]: crates/harlo-core/src/mcp_probe.rs
[^remove]: crates/harlo-core/src/servers.rs, fn remove
