---
type: operations
title: Testing, packaging, and signed updates
description: Contributor build prerequisites, focused and integration verification, Linux release production, and the signature/checksum chain protecting self-updates.
tags: [testing, build, packaging, releases, security]
verified:
  - by: openwiki/0.9.0
    at: 2026-11-17T18:09:09.933Z
sources:
  - id: openwiki-source-4d1d392666be6dfdd7a91a2e
    resource: repo://.github/workflows/release.yml
  - id: openwiki-source-aa11f7ccdcbf0dd150735c18
    resource: repo://.github/workflows/tests.yml
  - id: openwiki-source-f317ee207e1653d2033c81a4
    resource: repo://CONTRIBUTING.md
  - id: openwiki-source-0b93973e66ce9fb1db2bd77c
    resource: repo://crates/harlo-agent/src/agent.rs
  - id: openwiki-source-f9cde46e266df585b9f891eb
    resource: repo://crates/harlo-agent/src/chat.rs
  - id: openwiki-source-d59b76b96eb4918e43784eab
    resource: repo://crates/harlo-agent/src/skills.rs
  - id: openwiki-source-1ab0216479222bff9a772d8b
    resource: repo://crates/harlo-app/src/updates.rs
  - id: openwiki-source-5af0c90ff6d5f75199de24a2
    resource: repo://crates/harlo-core/src/export.rs
  - id: openwiki-source-4af2dd3e4a0acae243d3b65c
    resource: repo://crates/harlo-core/src/openwiki.rs
  - id: openwiki-source-4db2ffefa327cd504d9b5a66
    resource: repo://crates/harlo-core/src/session.rs
  - id: openwiki-source-7f90659c3ba3e62659b4a786
    resource: repo://crates/harlo-core/src/updates.rs
  - id: openwiki-source-4ffdea106fc0bda2812d95f6
    resource: repo://crates/harlo-core/tests/memory.rs
  - id: openwiki-source-322c82102553742bfacf73ab
    resource: repo://crates/harlo-harness/src/claude_run.rs
  - id: openwiki-source-c0d5ffb66451b26dd04ddd63
    resource: repo://crates/harlo-harness/src/process.rs
  - id: openwiki-source-4dadc7b205a2f63287cbe0a8
    resource: repo://crates/harlo-update-signing/src/lib.rs
  - id: openwiki-source-056ac262dd327c0a8e1ed162
    resource: repo://crates/harlo-wiki/src/marked.rs
  - id: openwiki-source-c3f26d7f96c5e44628c83808
    resource: repo://crates/harlo-wiki/src/retrieval.rs
  - id: openwiki-source-eb7f694aac7969d92da614ab
    resource: repo://crates/harlo-wiki/src/run.rs
  - id: openwiki-source-578be1dc5f53b796e8783a2c
    resource: repo://docs/releasing.md
  - id: openwiki-source-f9c77b8035af27d8ca2a61f1
    resource: repo://packaging/linux/bundle.sh
  - id: openwiki-source-2e6fe030dac0fa4ae34a9b22
    resource: repo://site/README.md
  - id: openwiki-source-f1c29df965300759d3c1ff56
    resource: repo://wrangler.jsonc
generated: { by: "harlo", at: "2026-11-17T18:09:09.933Z" }
---

# Testing, packaging, and signed updates

## Build and isolate development

The contributor guide documents Rust 1.85+, Qt 6.9+ (base, declarative, multimedia), cmake, and clang. Use the current stable Rust toolchain and the repository lockfile when diagnosing dependency/toolchain compatibility; documented minima are not a guarantee that every later locked dependency builds with an older compiler. Dictation includes whisper.cpp, so even the Rust workspace has native build requirements.

```sh
cargo run -p harlo-app --release
cargo test --workspace
cargo clippy --workspace --all-targets
# Core logic without whisper.cpp's optional dictation feature:
cargo test -p harlo-core --no-default-features
# Headless model-server helper:
cargo build -p harlo-core --release --bin harlo-engines --no-default-features
```

Use a scratch HOME or XDG data/config directories for interactive testing. Demo sessions stream synthetic events without an installed agent or model credits. Demo verifies presentation and session plumbing, not a real provider, sandbox, or authentication flow.

## Choose tests by boundary

| Change | Start with | Then verify |
| --- | --- | --- |
| Wire translation or framing | `cargo test -p harlo-harness` | Real/scripted agent protocol integration |
| Session reduction or metadata | Focused core tests | Demo subscriber lifecycle and UI behavior |
| Streaming saved histories or retained transcript memory | `cargo test -p harlo-core --test memory` and focused transcript/session tests | Representative long-session UI behavior and remote history loading |
| Sandbox or agent tools | `cargo test -p harlo-agent` | Actual OS sandbox behavior |
| Phone protocol | Core phone tests, `swift test --package-path ios/HarloKit` | Direct and relay transport behavior |
| OpenWiki integration | `cargo test -p harlo-wiki` and the core `openwiki` test filter | Native local engine behavior and remote `harlo-agent wiki` tool-server behavior |
| Packaging/update | Signing/core update tests | Installed package smoke tests and self-update scenario |

Keep external-service checks distinct from hermetic unit tests. See [agent safety](../concepts/agent-execution-and-safety.md) and [phone integration](../integrations/phone-and-relay.md) for focused invariants.

For stream-accounting regressions, start narrowly before running the broader agent suite:

```sh
cargo test --locked -p harlo-agent a_stream_that_breaks_keeps_what_it_said_it_used
cargo test --locked -p harlo-agent a_later_report_of_part_keeps_the_rest
cargo test --locked -p harlo-agent each_count_said_is_kept_on_its_own
cargo test --locked -p harlo-agent each_try_is_counted_as_it_ends_and_the_next_admitted
```

The partial-count check distinguishes output-only and input-only reports from measured zero, including budget-only estimation of missing input. The attempt-accounting check covers charging before retry admission and dropping an in-flight request after it has reported usage: the spend remains, the hold is released, and an empty snapshot does not add another charge. These unit boundaries complement the broken-stream and report-merging checks; they are not a claim that every provider's cancellation behavior has been tested.

For phone code display/copy regressions, use the focused checks listed in [phone verification](../integrations/phone-and-relay.md#verification); core crypto tests alone are not a substitute for checking desktop serialization and the Swift renderer.

The `memory` integration-test target generates 64 MiB Harlo and Claude history files and measures additional allocations for current-branch reads, incremental Claude reads, branch outlines, and repeated transcript-row materialization. Its 8 MiB bound applies to those specific operations under the test harness, not the desktop's overall resident memory. Phone-server tests cover serialized transcript row limits, picture access, and code serialization; `phone_activity` tests check sealed Live Activity phases and leftover-card cleanup. Use the phone page's commands for the core phone and HarloKit suites, and do not infer APNs delivery from those local payload tests.

For saved-session loading races, `a_message_sent_while_a_long_session_loads_shows_at_once` checks that the desktop session preserves new events while it reads history, and `a_resumed_session_shows_what_you_send_before_its_history_is_read` checks Claude's adapter boundary and complete history-dependent responses. `its_agents_end_with_it` checks that the harness process-group cleanup also ends descendants that ignore `SIGTERM`; this is a focused process-lifecycle test, not a general proof that every external process shuts down cleanly.

For Markdown export and plugin regressions, the focused core tests `streamed_is_the_same_as_whole`, `what_waits_for_a_late_result_spills_the_same`, and `a_late_result_holds_little_of_what_came_between` cover streamed output equivalence and bounded waiting text behind late tool results. `plugin_commands_only_from_you` checks prompt ownership before the session intercepts a plugin command; `claude_codes_plugins_skills` checks plugin installation scope, enablement, and skill discovery in the agent. Image decoding/caching is app-side behavior and is not validated by the core session or phone serialization tests.

The wiki engine has its own crate-level test boundary as well as the core adapter tests. `harlo-wiki` exercises native retrieval, Markdown parsing limits, evidence handling, storage, and lifecycle behavior; core tests check that the desktop uses the engine locally and routes remote operations through the agent. A parser failure is expected to return a tool error without terminating Harlo; a focused engine test alone does not prove the UI or SSH path.

```sh
cargo test -p harlo-wiki
cargo test -p harlo-core --no-default-features openwiki
```

For the model-free update detector specifically, `cargo test -p harlo-wiki quiet_updates_miss_nothing` exercises declaration detection across several languages, significant edits, test-path exclusions, oddly quoted Git paths and diff settings, Git failures, and source changes arriving during the check. `quiet_updates` covers cited-line movement versus changed evidence, new files, manually edited pages, and the no-run recording path. `changed_files_are_listed_heaviest_first` verifies that larger source files precede smaller ones and that a license and image receive zero weight. `the_largest_code_no_page_cites_is_named_when_a_model_is_needed` verifies that uncovered code is surfaced only when another condition already needs a model-backed update; its fixture checks that older uncited code, small files, tests, vendored/minified code, and a picture do not appear. These focused cases validate the detector and local engine; they do not substitute for testing remote wiki calls.

Update planning has its own regression boundary: `an_update_plan_decides_on_every_uncovered_file_once` proves that omitting uncovered paths or listing only some in `leaveUncovered` is rejected, that seeding a feature folder decides its files together, and that decided paths are recorded so later updates do not repeatedly ask about them. `a_resumed_run_keeps_its_plan_when_the_code_changed` guards the complementary recovery rule: source drift must not silently replace an already-persisted plan.

## Headless Rust CI

`.github/workflows/tests.yml` runs the agent and core tests on pushes to `main`, pull requests, and manual dispatch. Push/PR changes confined to iOS, the website, wiki, or Markdown are ignored. Runs for the same ref cancel an older in-progress run.

The job uses Ubuntu 24.04, stable Rust, a Cargo cache, and a 45-minute timeout. It installs native compiler/build dependencies and bubblewrap, then runs:

```sh
cargo test --locked -p harlo-agent -p harlo-core
```

This is not a whole-workspace or Qt UI job. The workflow disables Ubuntu's AppArmor restriction on unprivileged user namespaces on its runner so sandbox tests can execute bubblewrap rather than only checking generated arguments. That runner setup is a prerequisite for exercising the real sandbox boundary, not advice to change a production machine's security policy. Desktop interaction and end-to-end checks remain separate from this job.

## Release production

`docs/releasing.md` describes the maintainer workflow: add the next version's CHANGELOG section, then invoke `packaging/release.sh` from an up-to-date main branch. The script checks repository state, updates Cargo versions, commits, tags and pushes, then follows the release workflow. These are publishing operations, not prerequisites for ordinary development; do not run them merely to test a change. `--dry-run` is the documented preview.

The release workflow builds remote ARM helpers, then Linux packages/AppImage with bundled Qt (currently Qt 6.12.0). The native wiki engine is part of the Rust workspace; the Linux bundle also includes its OpenWiki license notice. Package installation is exercised in Ubuntu 22.04/24.04, Debian 12, Fedora 42, openSUSE Tumbleweed and Arch containers. Publishing depends on both build and install jobs and is restricted to `v*` tags. It verifies that the tag version matches Cargo, extracts release notes, signs the update manifest, and creates the GitHub release. Manual runs and release-check branches can build/test without publishing.

The iPhone app has a separate release/version lifecycle. The website in `site/` is the static download/support surface, not the desktop UI runtime.

Cloudflare Workers static assets serves `site/` directly, with no site build step. `wrangler.jsonc` selects HTML path handling and the custom 404 page and disables workers.dev and preview addresses. The download page obtains current release files from GitHub's API; site deployment is distinct from producing or signing desktop packages.

## Signed self-update trust chain

```mermaid
flowchart LR
  P[Built packages] --> H[SHA-256 asset records]
  H --> M[Signed linux-release.json]
  M --> V[Verify Ed25519 and repository/tag]
  V --> D[Download selected package]
  D --> C[Compare SHA-256]
  C --> I[Install or replace AppImage]
```

`harlo-update-signing` defines a shared canonical payload: domain-separated bytes, version/tag/repository, and assets sorted by name, with NUL-delimited fields. The non-shipped `harlo-manifest-signer` holds the private-key signing path; installed Harlo carries the public verification key. The workflow supplies `HARLO_UPDATE_SIGNING_KEY` only to the signing step.

Core verifies the manifest signature before accepting it, then checks repository identity and `v<version>` consistency. The desktop hashes the downloaded asset and refuses installation on mismatch. SHA256SUMS alone is not this trust boundary: an attacker able to replace a package and plain checksum must still satisfy the signed manifest.

`HARLO_UPDATE_PUBLIC_KEY` supports throwaway test keys and is an explicit environment override. Protect the launch environment accordingly. Key rotation also needs planning: installed clients with the old built-in public key cannot accept releases signed only by a new key.

## What this wiki verifies

This wiki was researched from repository files; the suggested build and integration commands are not reported as executed tests. OpenWiki validates page submissions and finalizes its own indexes/provenance separately. Start at the [quickstart](../quickstart.md) to choose a subsystem before running expensive checks.
