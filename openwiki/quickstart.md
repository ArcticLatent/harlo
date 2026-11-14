---
type: overview
title: Quickstart and task routing
description: Start building Harlo and find the right wiki guide for agent execution, native sessions, plans, SSH models, phone transport, OpenWiki, or release verification.
tags: [quickstart, navigation, build, architecture]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-14T13:04:09.841Z
sources:
  - id: openwiki-source-651d1fb6c9e49916a916ab51
    resource: repo://Cargo.toml
  - id: openwiki-source-f317ee207e1653d2033c81a4
    resource: repo://CONTRIBUTING.md
  - id: openwiki-source-be6e7bfc466a23d5de0a4a6d
    resource: repo://crates/harlo-core/src/agent_host.rs
  - id: openwiki-source-aa0d9221c1e01ca9decebdae
    resource: repo://crates/harlo-core/src/db.rs
  - id: openwiki-source-4af2dd3e4a0acae243d3b65c
    resource: repo://crates/harlo-core/src/openwiki.rs
  - id: openwiki-source-3ef6360b45fa7169c94b35d3
    resource: repo://crates/harlo-harness/src/lib.rs
  - id: openwiki-source-07a69ae82a0e133d3de748f1
    resource: repo://crates/harlo-harness/src/store.rs
  - id: openwiki-source-325d02e6188e2718d48b8686
    resource: repo://crates/harlo-wiki/src/lib.rs
generated: { by: "harlo", at: "2026-11-14T13:04:09.841Z" }
---

# Quickstart and task routing

Harlo is a native Rust/Qt desktop for its own coding agent and Claude Code, with SSH-backed machines, managed model servers, and encrypted phone access. This wiki explains the boundaries behind those features. It is not a replacement for current source, release notes, or the user-facing README.

## First useful run

The repository is a Cargo workspace over `crates/*`, using Rust edition 2024. The contributor guide documents Rust 1.85+, Qt 6.9+ (base, declarative and multimedia), cmake and clang; README additionally calls out `notify-send`. Use current stable Rust when working with the current lockfile. These are documented prerequisites, not a build verified during wiki generation.

```sh
cargo run -p harlo-app --release
# Logic-only iteration without the optional whisper.cpp dictation build:
cargo test -p harlo-core --no-default-features
# Full verification once native dependencies are installed:
cargo test --workspace
```

Select **Demo** in the sidebar to exercise streamed chat without installing/signing in an agent or spending model credits. For real sessions, use the setup guide (`harlo --setup`) and configure the desired agent/provider. Test with a scratch HOME or XDG data/config directories to avoid touching personal sessions and settings.

For the phone client library, `swift test --package-path ios/HarloKit` exercises the Swift package; the SwiftUI app is generated with XcodeGen from `ios/Harlo/project.yml`. Its release process is separate from the Linux app.

## Find the right guide

| Your task or symptom | Read first | Primary source boundary |
| --- | --- | --- |
| Chat state, lost rows, session switching, persistence, Qt threading | [Sessions and desktop](architecture/sessions-and-desktop.md) | Harness events → core session/reducer → app model |
| Finding earlier work before launching helpers | [Prior-session discovery](workflows/plans-and-project-changes.md#finding-prior-work-before-starting-a-helper) | Background history search → workspace-scoped results |
| Tool execution, approvals, sandbox access, resume, subagents | [Agent execution and safety](concepts/agent-execution-and-safety.md) | `harlo-agent` loop/RPC/tools/sandbox |
| Token budgets, broken-stream spend, unknown or partly known costs | [Agent accounting](concepts/agent-execution-and-safety.md#usage-and-provider-capabilities) | Shared request usage and provider pricing |
| Shared plan edits, helper worktrees, step commits, bring-over clashes | [Plans and project changes](workflows/plans-and-project-changes.md) | Core plan/git plus desktop orchestration |
| Remote files/sessions, helper deployment, unavailable local models | [Remote machines and model servers](integrations/remote-machines-and-models.md) | SSH host routing, engine helper, provider configuration |
| Local proxy billing, free-server flags, missing prices | [Free versus unknown cost](integrations/remote-machines-and-models.md#free-servers-versus-unknown-costs) | Provider configuration → runtime accounting |
| Pairing, encrypted events, reconnects, push or relay deployment | [Phone and relay](integrations/phone-and-relay.md) | Core crypto, desktop server, HarloKit, relay/APNs |
| Long code blocks, phone clipping, copied code and blank lines | [Phone transcript delivery](integrations/phone-and-relay.md#from-phone-requests-to-shared-sessions) | Transcript pieces → bounded phone frames → Swift rendering |
| Wiki sessions, generation/resume, update offers, remote wiki calls | [OpenWiki integration](integrations/openwiki.md) | Core native tool dispatch → local engine or SSH agent |
| Ranked wiki search, linked workspaces, evidence drift, durable claim state | [Native wiki engine](architecture/wiki-engine.md) | In-memory retrieval → repository evidence → persisted page queue |
| Build dependencies, targeted checks, packages, signed self-updates | [Testing and releases](operations/testing-and-releases.md) | Contributor guide, CI, packaging, update verification |

## Architecture in one minute

- **Harness:** starts agents and converts their protocols into shared events. Claude stream-json and Harlo RPC do not leak into each UI component.
- **Core:** UI-independent session state, transcript reduction, application metadata, Git, remotes, models, and phone crypto.
- **Desktop:** cxx-qt/QML presentation and workspace coordination. The code view has a separate Qt-independent Rust drawing crate.
- **Agent:** an independent executable with its own model loop, tools, session log, and safety policy.
- **Wiki:** `harlo-wiki` implements OpenWiki-format retrieval and authoring natively. Core calls it directly locally or through `harlo-agent wiki` over SSH; no separate Node runtime is needed.
- **Phone/relay:** Swift and browser clients consume encrypted session APIs. The relay routes ciphertext; it is not the owner of conversations.

Put non-Qt logic in core/harness (or the relevant standalone component) and keep desktop glue at the Qt boundary. When adding a feature that crosses machines or clients, trace the same state owner through desktop, remote, and phone entrypoints rather than implementing independent copies.

## Data ownership and debugging

Harlo application metadata normally lives under `~/.local/share/harlo/` and configuration under `~/.config/harlo/`. Conversations remain in each agent's session files; Harlo agent logs normally live under `~/.local/share/harlo-agent/sessions/`. A remote session uses the remote machine's own files and sign-ins. Check the owner and machine before assuming an empty sidebar means history was deleted.

A useful debugging sequence is: identify the owning process → reproduce with a narrow test or Demo → inspect the boundary event/state → add a focused regression test → exercise the real integration if the bug involves SSH, OS sandboxing, Qt, or APNs. Synthetic UI success alone does not validate those external boundaries.

## Reading and maintaining this wiki

Pages are grouped by responsibility and workflow, not by source directory. Use their focused tests as starting points and their provenance as pointers to source. Retrieval can search a question and read only matching sections; a wiki read does not itself regenerate documentation.

The [OpenWiki guide](integrations/openwiki.md) explains session integration and host-owned root selection; the [native engine guide](architecture/wiki-engine.md) explains retrieval scope, evidence resolution, and the durable generation queue. Source changes should be reconciled through an update run, not by hand-editing generated claims, indexes, or run metadata. No application code changes or commits are required to create this wiki.
