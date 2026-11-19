---
type: Overview
title: Harlo codebase
description: A contributor map of Harlo's desktop, coding agents, mobile companion, and supporting services.
covers:
  - Cargo.toml
generated: { by: harlo/gpt-6-astra, at: "2026-11-19T00:54:36Z" }
sources:
  - id: workspace
    resource: "repo://Cargo.toml"
  - id: desktop
    resource: "repo://crates/harlo-app/src/main.rs"
  - id: core
    resource: "repo://crates/harlo-core/src/lib.rs"
  - id: mobile
    resource: "repo://ios/HarloKit/Package.swift"
---
# Harlo codebase

Harlo is a native desktop front end for coding agents: its own Harlo runtime and Claude Code. The desktop is Qt/QML with Rust application bridges; UI-independent session, transcript, persistence and workspace logic lives in `harlo-core`. This separation lets front ends consume the same session behavior without making the core depend on Qt.[^desktop][^core]

The repository is a Rust workspace whose crates ship as an application rather than published crates. The native Swift companion and its client library live alongside that workspace. Start at the subsystem boundary relevant to your change, then follow the workflow pages for ordering and failure behavior.[^workspace][^mobile]

## Architecture map

| Subsystem | Pages |
| --- | --- |
| Core | [Boundaries](/architecture/core.md), [persistence](/architecture/core-persistence.md), [engine installation](/architecture/core-engines.md) |
| Harness | [Process protocols](/architecture/harness.md) |
| Agent | [Runtime](/architecture/agent.md), [context and replay](/architecture/agent-context.md), [tools and sandbox](/architecture/agent-tools.md) |
| Desktop | [Application](/architecture/desktop.md), [UI state ownership](/architecture/desktop-ui.md), [code editor](/architecture/desktop-code.md) |
| Mobile | [Native app](/architecture/mobile-native.md), [phone protocol](/architecture/mobile-protocol.md), [notifications and widgets](/architecture/mobile-notifications.md) |
| Knowledge | [Code index and wiki](/architecture/knowledge.md) |
| Distribution | [Release trust and packaging](/workflows/releases.md) |

## Workflow map

| Work area | Pages |
| --- | --- |
| Sessions | [Lifecycle](/workflows/sessions.md), [Git and remote execution](/workflows/git-and-remote.md) |
| Agent | [Orchestration](/workflows/agent-orchestration.md) |
| Desktop | [Conversations](/workflows/desktop-sessions.md), [files and terminals](/workflows/desktop-files.md), [connectivity](/workflows/desktop-connectivity.md) |
| Mobile | [Relay and APNs operations](/workflows/mobile-relay-apns.md), [probe and recordings](/workflows/mobile-probe.md) |

## Where to put a change

Keep UI-independent behavior in core rather than duplicating it in QML. Process-specific normalization belongs in the harness; model/tool execution belongs in the agent. Desktop code owns Qt integration and its shared Tokio runtime, not the portable definition of a transcript. These are practical boundaries reflected by the module dependencies, not interchangeable places to implement a feature.[^core][^desktop][^workspace]

Footnotes on each page are its evidence trail. Follow them before changing an invariant; broad `covers` entries assign ownership of supporting implementation files, while focused pages explain the important cross-file contracts.

[^workspace]: Cargo.toml
[^desktop]: crates/harlo-app/src/main.rs
[^core]: crates/harlo-core/src/lib.rs
[^mobile]: ios/HarloKit/Package.swift
