---
type: Workflow
title: Agent sessions, MCP and delegated work
description: How a long-lived session coordinates client commands, external tools and isolated subagents.
covers:
  - crates/harlo-agent/src/rpc.rs
  - crates/harlo-agent/src/mcp.rs
  - crates/harlo-agent/src/mcp_auth.rs
  - crates/harlo-agent/src/task.rs
  - crates/harlo-agent/src/isolate.rs
  - crates/harlo-agent/src/profiles.rs
generated: { by: harlo/gpt-6-luna, at: "2026-11-19T18:59:56Z" }
sources:
  - id: rpc
    resource: "repo://crates/harlo-agent/src/rpc.rs"
  - id: tasks
    resource: "repo://crates/harlo-agent/src/task.rs"
  - id: wait
    resource: "repo://crates/harlo-agent/src/task.rs"
    title: fn wait
  - id: background-on
    resource: "repo://crates/harlo-agent/src/task.rs"
    title: fn background_on
  - id: running
    resource: "repo://crates/harlo-agent/src/task.rs"
    title: struct Running
  - id: background
    resource: "repo://crates/harlo-agent/src/task.rs"
    title: struct Background
  - id: wait-spec
    resource: "repo://crates/harlo-agent/src/task.rs"
    title: fn wait_spec
  - id: dispatch
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: fn run_calls
  - id: turn
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::turn"
  - id: jobs
    resource: "repo://crates/harlo-agent/src/jobs.rs"
  - id: config
    resource: "repo://crates/harlo-agent/src/mcp.rs"
  - id: stdio
    resource: "repo://crates/harlo-agent/src/mcp.rs"
    title: "fn Stdio::start"
  - id: index
    resource: "repo://crates/harlo-agent/src/mcp.rs"
    title: "fn Mcp::index"
  - id: mcp
    resource: "repo://crates/harlo-agent/src/mcp.rs"
  - id: refresh
    resource: "repo://crates/harlo-agent/src/mcp.rs"
    title: "fn Mcp::refresh"
  - id: start
    resource: "repo://crates/harlo-agent/src/agent.rs"
    title: "fn Agent::start_mcp"
  - id: mcp-auth
    resource: "repo://crates/harlo-agent/src/mcp_auth.rs"
  - id: profiles
    resource: "repo://crates/harlo-agent/src/profiles.rs"
  - id: isolation
    resource: "repo://crates/harlo-agent/src/isolate.rs"
  - id: usage
    resource: "repo://crates/harlo-agent/src/session.rs"
---
# Agent sessions, MCP and delegated work

There are three different relationships here: Harlo drives an agent over RPC; an agent connects to MCP servers; and the agent can launch in-process `task` subagents. Harlo's own cross-session tools are forwarded over the parent RPC pipe, not implemented as recursive local tasks. Keeping these routes distinct preserves caller identity and gives local subagents a deliberately smaller capability set.[^rpc][^tasks][^dispatch]

For the request loop see [execution](/architecture/agent.md); for confinement and approvals see [tool safety](/architecture/agent-tools.md).

## Drive one active turn without blocking control messages

RPC reads one JSON object per stdin line and emits command responses and progress events on stdout. Only one main run is active at once: another prompt is refused, while steering and follow-up commands enqueue work. Steering is consumed between rounds; follow-ups are considered when the model would otherwise finish. The shared view supplies state/messages while the running task owns the agent.[^rpc][^turn]

Approvals use correlated UI requests and one-shot responses. Harlo-owned tool calls similarly emit `harlo_call` with an id and wait for `harlo_result`, with a bounded wait; the host knows the caller from the pipe rather than trusting a model-supplied session identity. Run cleanup withdraws outstanding approval cards. Stop cancels foreground work, but session-owned background jobs can later deliver notices or wake an idle run.[^rpc][^jobs]

## Establish MCP trust before expanding or starting configuration

MCP merges user configuration with project `.harlo/mcp.json`, with project entries winning by name. A project entry starts only if its current raw configuration hash matches the approval recorded for that project. This matters because configuration can name executables and expand environment variables or `!command` values: merely checking out a repository must not grant its configuration permission to execute.[^config]

**MCP approval is not bash confinement.** Stdio MCP starts its configured executable directly with its own process group and configured environment/cwd; it does not use the ordinary bubblewrap command constructor. HTTP servers are external services. Read-only hints and configured read-only overrides are trust declarations used by dispatch, not a proof that the server cannot mutate state.[^stdio][^index]

Startup runs servers concurrently with per-server timeouts. Failures become notes so an unavailable optional server does not prevent ordinary coding work. Successful connections initialize, discover tools/resources and build namespaced tool names. A large schema inventory switches to tool discovery plus `mcp_call`, reducing the schema carried in every model request.[^mcp]

Tool schemas remain stable during a prompt. A server's list-changed notification marks it for refresh at the next prompt; cancellation or failure leaves refresh work pending. When a discovered tool is invoked through `mcp_call`, dispatch first resolves the underlying tool and arguments, then applies its actual read-only classification, approval and plan-mode rules. The discovery wrapper must not become a permission bypass.[^refresh][^start][^dispatch]

Remote MCP authentication discovers resource/authorization metadata, supports dynamic registration or configured client credentials, and uses PKCE with a loopback or pasted callback. Tokens are stored privately by server address, refreshed before expiry under a cross-process lock, and sent as bearer tokens. This allows helpers to share a sign-in without independently rotating the same refresh token.[^mcp-auth]

## Delegate with bounded capability and explicit merge behavior

A `task` call accepts up to eight tasks and runs at most four concurrently, returning final results in request order. Each gets an agent and its own log. No local task receives `task` or Harlo's orchestration tools, so it cannot recursively create more local tasks or helper sessions. Approvals route through the parent gate, cancellation reaches all children, and background bash is refused for subagents. Optional background task calls return a `bg-…` id; `wait` takes selected results, while any not explicitly waited for are collected before the parent turn ends.[^tasks][^running][^background][^dispatch][^wait][^wait-spec][^background-on]

A same-model task can fork the parent's request prefix to reuse context/cache. Tasks on another model/profile, and parallel writers isolated into worktrees, start fresh with instructions rather than pretending the inherited conversation describes their distinct environment. Profiles can narrow general-task tools and add instructions/model preference, but cannot grant capabilities outside that ceiling; unknown tool names are dropped.[^tasks][^profiles]

For multiple concurrent writers in a suitable Git project, isolation snapshots staged, unstaged and untracked nonignored files using a temporary index. The project's HEAD, index and files are not changed to create that starting commit. Each writer works in its own worktree, preserving the caller's subdirectory.[^isolation]

Results are brought back in task order. A patch that does not apply cleanly gets a file-level merge attempt; compatible insertions at the same place retain the earlier task's lines first. Remaining conflicts are not forced into the project: the worktree and patch are retained for deliberate reconciliation. Repositories with submodules or sparse checkout, and nonrepositories, fall back to shared-folder execution, so isolation must not be assumed from the mere presence of parallel tasks.[^isolation]

Progress frames distinguish child lifecycle from tool progress, and saved result details retain progress metadata. These local subagent logs also matter to [usage reconstruction](/architecture/agent-context.md): the parent's aggregate result must not be counted again on top of each child's individual request records.[^tasks][^usage]

[^rpc]: crates/harlo-agent/src/rpc.rs
[^tasks]: crates/harlo-agent/src/task.rs
[^wait]: crates/harlo-agent/src/task.rs, fn wait
[^background-on]: crates/harlo-agent/src/task.rs, fn background_on
[^running]: crates/harlo-agent/src/task.rs, struct Running
[^background]: crates/harlo-agent/src/task.rs, struct Background
[^wait-spec]: crates/harlo-agent/src/task.rs, fn wait_spec
[^dispatch]: crates/harlo-agent/src/agent.rs, fn run_calls
[^turn]: crates/harlo-agent/src/agent.rs, fn Agent::turn
[^jobs]: crates/harlo-agent/src/jobs.rs
[^config]: crates/harlo-agent/src/mcp.rs
[^stdio]: crates/harlo-agent/src/mcp.rs, fn Stdio::start
[^index]: crates/harlo-agent/src/mcp.rs, fn Mcp::index
[^mcp]: crates/harlo-agent/src/mcp.rs
[^refresh]: crates/harlo-agent/src/mcp.rs, fn Mcp::refresh
[^start]: crates/harlo-agent/src/agent.rs, fn Agent::start_mcp
[^mcp-auth]: crates/harlo-agent/src/mcp_auth.rs
[^profiles]: crates/harlo-agent/src/profiles.rs
[^isolation]: crates/harlo-agent/src/isolate.rs
[^usage]: crates/harlo-agent/src/session.rs
