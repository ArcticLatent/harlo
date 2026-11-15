Reviewed **`ac26cca3`**, including integration commit `f469a2a7`.

**The migration delivers substantial speed and memory improvements. I found three actionable issues, with parser robustness the highest priority.**

## Findings

### 1. High: nested Markdown can terminate Harlo
**`crates/harlo-wiki/src/marked.rs:507–538`**

The blockquote parser recurses without a depth limit.

- A **2,015-byte page containing 1,000 nested blockquotes** aborted an optimized library worker with a stack overflow.
- The shipped-profile agent also aborted at 5,000 levels.
- Upstream handled these inputs successfully or returned a tool error while remaining alive.

Local retrieval now executes inside Harlo (`crates/harlo-core/src/openwiki.rs:398`), so this can terminate the desktop process.

**Recommendation:** enforce parser depth/work limits and return a normal tool error. `catch_unwind` alone cannot contain this stack-overflow abort.

### 2. Medium: setup writes can escape the repository
**`crates/harlo-wiki/src/run.rs:545–564`**

I reproduced `openwiki_begin` modifying external files through:

- An `AGENTS.md` symlink.
- A `CLAUDE.md` symlink.
- A `.github` directory symlink, creating the workflow outside the repository.

This behavior is **inherited from upstream**, but remains a containment gap. Setup uses direct filesystem writes rather than the guarded wiki backend.

**Recommendation:** apply repository containment checks to setup files, including parent directories, while preserving any intentionally supported in-repository links.

### 3. Medium: concurrent searches discard the warm-cache advantage
**`crates/harlo-wiki/src/retrieval.rs:459–497`**

A search temporarily removes its index from the cache. Concurrent searches then see a miss and independently rebuild it.

On a warmed 500-page fixture:

- Sequential searches: **6.6–7.4 ms**.
- Four simultaneous searches: **11, 328, 330, and 339 ms**.

They subsequently insert duplicate indexes, consuming cache slots and potentially evicting other repositories.

**Recommendation:** retain one shared entry per scope and coordinate rebuilding so concurrent callers reuse it.

## Measured improvements

Fresh measurements using the optimized agent and pinned upstream runtime:

| Wiki | Upstream warm search | Native warm search | Process RSS, upstream → native |
|---|---:|---:|---:|
| Harlo’s wiki | 24.5 ms | **3.5 ms** | 325 → **21 MiB** |
| 500-page fixture | 988 ms | **8.4 ms** | 468 → **57 MiB** |
| 2,000-page fixture | 3,880 ms | **22.7 ms** | 757 → **141 MiB** |

Helper initialization fell from approximately **510–533 ms to 2 ms**. First-search parsing/indexing remains separate; for 2,000 pages it improved from **4.08 seconds to 0.99 seconds**.

**Token use should remain essentially unchanged for identical tool sequences.** Matching schemas and answers preserve existing retrieval efficiency; the Rust migration’s demonstrated gains are latency, memory, and deployment simplicity.

## Verification

- **38 relevant Rust tests passed.**
- **246 writer steps** matched the upstream recording across 11 scenarios.
- **1,622 reading answers and all ten tool definitions** matched upstream.
- Independent filesystem, parser, and concurrency reproductions completed.
- Debug and shipped-profile agent builds passed.

Desktop GUI and live SSH workflows were not freshly exercised. Application source was untouched; probes and results are under `/tmp/opencode/harlo-native-wiki-review/`.

**Bottom line:** the architecture change is paying off. Fix the parser abort first, then setup containment and concurrent cache reuse.
