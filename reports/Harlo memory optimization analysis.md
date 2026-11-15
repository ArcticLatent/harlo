# Harlo memory optimization analysis

Date: November 14, 2026

**Yes—1.3 GiB is understandable for this exceptionally long session, but it is not an unavoidable requirement. There are worthwhile optimization opportunities.** The evidence does not establish a memory leak.

The investigation was read-only. No application files or running application state were changed. This report was subsequently saved at the user's request.

## What I measured

| Component | Initial proportional memory¹ |
|---|---:|
| Harlo desktop | **1.30 GiB** |
| Claude child process | **304 MiB** |
| Claude’s `rust-analyzer` | **3.27 GiB** |

¹Proportional memory accounts for shared pages more fairly than adding process RSS.

Your active session is unusually large:

- **1.84 GiB** raw session log.
- **170 MiB** normalized history.
- Approximately **58,700 messages**, including **27,800 tool results**.
- Approximately **77 MiB of embedded image data**.

Harlo already avoids loading the entire 1.84 GiB log as one string. However, it subsequently retains the complete normalized conversation and builds its transcript representations.

Later, the machine experienced memory pressure: approximately **943 MiB of Harlo’s memory moved into swap**. Its lower resident-memory reading at that point was not a genuine reduction in allocation.

## Main optimization opportunities

### 1. Make old history disk-backed — largest structural improvement

Currently, Harlo retains:

- Serialized complete messages in `KeptMessages`.
- Parsed transcript blocks, tool arguments, and complete tool outputs.
- Rendered rows for the entire conversation.

Relevant code: `crates/harlo-core/src/session.rs:427`, `crates/harlo-core/src/transcript.rs:602`.

The initial “latest rows first” loading improves responsiveness, but eventually builds all rows. **UI virtualization does not currently bound history memory.**

**Proposal:** extend the existing normalized disk cache with message offsets and lightweight metadata. Keep the active turn and a byte-bounded recent-history window in memory; load older sections asynchronously as needed.

To preserve smooth interaction:

- Prefetch around the scroll position.
- Keep stable row identifiers and scroll anchors.
- Load full tool output on expansion.
- Stream export/context processing instead of reparsing the entire history simultaneously.

This addresses both steady memory use and loading peaks.

### 2. Stop retaining historical image payloads in serialized messages — concrete, smaller win

The transcript already extracts inline pictures into cached files, but `KeptMessages` still retains their encoded image data.

Relevant code: `crates/harlo-core/src/pictures.rs:37`, `crates/harlo-core/src/session.rs:436`.

**Proposal:** retain a recoverable file/record reference for historical images and retrieve the original payload only when required.

For this session, **roughly 77 MiB of retained encoded payload is a specific optimization opportunity**. Actual RSS savings depend on allocator behavior. Exact export and recovery behavior should remain intact.

### 3. Tighten tool-run virtualization — likely benefits memory and responsiveness

The outer transcript is virtualized, but:

- A completed run uses a `Repeater` that creates **every inner card**, despite its scrollable box being capped in height.
- Collapsed/boxed outer rows still instantiate hidden `ToolCard` and `ThoughtLine` components.
- Zero-height rows can make pixel-based buffering encompass many more items than expected.

Relevant code: `crates/harlo-app/qml/TranscriptView.qml:1432`, `:2329`, `:2424`.

**Proposal:** use a virtualized inner model, create expensive contents through conditional loaders, and represent collapsed runs as summary rows rather than many zero-height delegates.

The allocation pattern is confirmed in source; its contribution to this particular process needs measurement.

### 4. Introduce coordinated, adaptive cache budgets

Configured image-cache budgets are substantial:

- Full decoded images: **768 MiB**.
- Thumbnails: **192 MiB**.

These are budgets, **not measured current occupancy**.

Relevant code: `crates/harlo-app/src/image_view.rs:409`, `:548`.

**Proposal:** use smaller idle budgets, preserve currently displayed images and nearby prefetch, and evict older decoded images when the viewer closes or memory pressure rises.

Markdown also has a nominal **32 MiB per-renderer cache**, while rendered rows retain their own strings. Share immutable rendered content and budget it alongside history. Merely reducing the Markdown cache would miss the larger retention issue.

### 5. Measure allocator retention after fixing ownership

Most initial desktop memory—approximately **1.14 GiB**—was anonymous memory. Some mappings resemble allocator arenas, but `/proc` cannot distinguish live objects from unused retained allocation space.

Harlo already calls `malloc_trim()` in several large-history paths.

**Proposal:** profile live allocations and retained allocator memory before changing allocator settings. Avoid frequent trimming during streaming; it can trade memory savings for latency.

## Recommended order

1. Add allocation/cache/delegate counters and establish a **same-session release-build baseline**.
2. Remove historical image-payload retention and unnecessary hidden tool-card construction.
3. Implement disk-backed history and bounded rendered-row retention.
4. Tune cache budgets, then allocator behavior.

Validate opening, scrolling, expanding large runs, exporting, and closing/reopening the same session. Track peak memory, resident **plus swapped** memory, and interaction latency.

**Bottom line:** meaningful savings—potentially hundreds of MiB—are plausible, but an exact reduction is not yet measured. The strongest goal is to make memory depend on the visible/recent working set rather than the session’s entire lifetime. Separately, the language server offers the larger system-wide savings opportunity.
