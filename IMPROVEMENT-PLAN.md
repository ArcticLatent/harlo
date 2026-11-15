# Harlo improvement plan (from the review in OPENAI_AGENT_IDEAS.MD)

Checked against the source at 188bb90a, 2026-11-12. Each step below is a
finding I confirmed in the code and agree with, in the order I'd do them.
What I'd skip or change, and why, is at the end.

Rules for every step: a test that fails before and passes after; `verify.sh
changed` green; anything that changes what the model sees is benchmarked
(paired runs, recorded in HISTORY.md) and kept only if the pass rate holds
with the same or fewer tokens. Real-model benchmark runs spend plan tokens,
so I ask before each.

## Stage 1: what the agent counts and keeps (harlo-agent)

### 1. Size the next request, not the last one (F01)
Done 2026-11-12 in 62ed77c3 (local commit).
Confirmed:
- `context_tokens()` is the previous request's input plus output. New tool
  results and the new prompt aren't counted before `full()` decides.
- A resumed session has no last usage, so `full()` returns false until a
  request has been sent.
- Bug: `compact::estimate(&json!(self.instructions))` counts the
  instructions as 4 tokens. `estimate` expects a message, not a string
  (agent.rs:1111, 1205).
- Tool schemas and the extra system text aren't counted either.

Do:
- Fix the instructions estimate.
- Estimate the next request as the last measured size plus the estimate of
  everything appended since.
- When nothing has been measured, estimate the whole request: instructions,
  schemas and history.
- Check again before sending to a fallback model with a smaller window.

Tests: unit tests with the stand-in provider:
- a resumed nearly-full session;
- several large tool results in one round;
- a fallback to a small window;
- a server that sends no usage.

### 2. A profile's instructions survive compaction (F13, first half)
Done 2026-11-12 in 62ed77c3 (local commit).
Confirmed: a subagent's profile is appended to `self.instructions`, and
compaction rebuilds `self.instructions` from the project files. A profiled
helper forgets its profile after compacting.

Do: keep the profile as its own part of the prompt that compaction keeps.

Test: compact a profiled subagent, then check its next request still
carries the profile.

### 3. Shell output survives a timeout and long output (F04)
Done 2026-11-12 in 62ed77c3 (local commit).
Confirmed:
- On timeout, everything the command printed is thrown away and replaced
  by a generic line (bash.rs:121–134). A test run that times out leaves
  the model with nothing, so it reruns it.
- Past 16 MiB, later output is dropped, so the "last lines" shown aren't
  the real end.

Do:
- Write output to the spill file as it arrives, keeping the start and a
  ring buffer of the end in memory.
- On success, failure, timeout or stop, return the start, the end and the
  file.

Tests:
- 20 MiB of output with a marker at the very end;
- a marker printed, then a timeout.

### 4. Count every request's tokens (F02, without a new subsystem)
Done 2026-11-12 in 9ae558bc (local commit).
Confirmed:
- `/btw` (`aside`) drops its usage.
- `session::spent()` counts only assistant messages, so a restart loses
  compaction and web-search usage.
- Parallel subagents read the budget before spending, so four can pass the
  check at once.
- No request sets an output ceiling.

Do:
- A small typed `Usage` that every request path returns: main agent,
  compaction, side question, web search and subagents.
- `spent()` reads every entry that carries usage.
- Subagents reserve against the shared budget before a request and settle
  after it.
- Don't add a separate ledger file. The session log already holds the
  entries; it just needs to record and read them all.

Test: the same totals before and after a restart with compaction, `/btw`,
a web search and four parallel helpers.

### 5. Cost says what it is (F03)
Done 2026-11-12 in 9ae558bc (local commit).
Confirmed: both providers write 0 into every cost field, and
`get_session_stats` returns 0.0. A dollar limit therefore never stops
harlo's own agent.

Do:
- Report cost as one of:
  - included in a plan (Codex sign-in);
  - local (no charge);
  - priced from the provider's own model list where it gives rates;
  - unknown.
- Never report a false 0.
- A per-model rate setting covers providers that give no prices.
- Dollar limits apply only where the cost is known, and the panel says
  when it isn't.

Test: a stand-in priced provider shows the same nonzero cost in the
transcript, the session stats, after a resume, and in the spending limit.

## Stage 2: failures that waste a run

### 6. Provider waits have limits (F08)
Done 2026-11-12 in 821d25d7 (local commit).
Confirmed:
- The provider client is `reqwest::Client::new()`: no connect timeout, and
  no limit on a stream that goes quiet. A stalled connection waits until
  you press Stop.
- SSE buffers grow without a cap.
- A `Retry-After` over 60 s is dropped, so harlo retries sooner than the
  server asked.

Do:
- A connect timeout.
- An idle-stream deadline, generous enough for slow local reasoning models
  and adjustable.
- A cap on an SSE event's size.
- For a long `Retry-After`: wait for it when it's within a limit, else stop
  and say when to try again.

Tests:
- a stand-in server that stalls after headers;
- one that sends an endless event;
- one that answers 429 with Retry-After 600.

### 7. Grep and find can be stopped (F10, the cheap part)
Done 2026-11-12 in 821d25d7 (local commit).
Confirmed: grep and find collect all their output and have no timeout or
`kill_on_drop`. A ranged read loads the whole file on the async thread.

Do:
- Search commands get a timeout and are killed on stop.
- Read and image fitting move to `spawn_blocking`.
- Streaming ranged reads wait for a measurement.

Test: stop during a grep over a large tree, and check it ends at once.

### 8. Subagents never write side by side by accident (F07)
Done 2026-11-12 in 6360ca3b (local commit).
Confirmed:
- When a writer's worktree can't be made, it silently writes in the shared
  folder.
- An "explore" helper has `bash` with the normal writable sandbox, so
  "changes no files" isn't enforced.

Do:
- With no worktree, writers run one at a time, and the parent is told why.
- Explore helpers' commands run with the project read-only, where the
  sandbox supports it, and the parent is told when it can't.

Tests:
- a forced worktree failure with two writers on one file;
- an explore helper's `touch` is refused.

### 9. A model switch after native compaction keeps the work (F05)
Done 2026-11-12 in 6360ca3b (local commit).
Confirmed: the Codex sign-in compacts natively. Any other model then gets
only the user's last ten requests (600 characters each) and a file list.
The file list also misses `apply_patch`.

Do:
- Add `apply_patch` (including moves and deletes) to the file tracker.
- A second native compaction's note lists only what it summarized, so the
  user's requests from the first note are lost. With the request sized
  first (step 1), two compactions in one turn happen more often. The
  portable summary has to start from the original log, not the last note.
- When a different model takes over after a native compaction, write a
  portable summary then, once, with the new model, from the original log
  within its window.
- This costs nothing unless you switch. An always-kept second summary
  would cost a request every compaction.

Test: compact natively, switch models, and check the next request carries
the decisions and the files changed.

## Stage 3: the desktop stays responsive

### 10. Search never waits on the indexer (F06, F15 first part)
Done 2026-11-12 in 7bfeff1d (local commit).
Confirmed:
- The search dialog calls `Workspace.searchMessages` synchronously. That
  locks the same `SearchIndex` the indexer holds while it reads session
  files, remote ones included. Typing in search can freeze the window.
- The indexer finds each file's stamp with a linear search through a
  vector, which is quadratic in the number of sessions.

Do:
- Run queries off the Qt thread on a read connection (WAL is on already),
  with a generation number so stale results are dropped.
- The indexer reads files before taking the lock and commits in batches.
- Stamps go in a map.

Test: a click test with an indexer slowed on purpose, typing in search.
The window stays responsive and the results arrive late.

### 11. Stop works while a turn's snapshot is taken (F12)
Done 2026-11-12 in 7bfeff1d (local commit).
Confirmed: before a prompt, the session loop awaits `snapshot_turn` and
can't see a Stop meanwhile. In a big or remote repository that's seconds.

Do: take the snapshot as a step the loop can still hear Stop during, and
send the prompt only if nothing stopped it.

Test: a slowed snapshot, with Stop pressed at once. The prompt is never
sent.

## Stage 4: measure, then decide

### 12. Prompt size by part (F09, measuring only)
Done 2026-11-12 in 62f02aab (local commit), run 20261112-205346: MCP tools ~1% of input, so no lazy MCP or on-demand schemas; harlo's own tools are 2.4k a request (33% of input in harlo), the part to look at next.
Have the benchmark report each prompt part's tokens:
- base instructions;
- project rules;
- skills;
- native tools;
- MCP tools;
- orchestration tools;
- history.

Then decide on:
- starting slow MCP servers lazily (first-response delay);
- loading MCP schemas on demand for big catalogs.

Both are kept only if the benchmark shows they win; the simple path stays
for small catalogs.

### 13. Long sessions (F11)
Done 2026-11-12 in 62f02aab (local commit): flush 5 ms at 51k rows (fine); the window was over budget (rows scanning every row, a rail tick per prompt), fixed with a row index and a windowed rail.
Profile a replay into 10k and 50k-row sessions: flush time and frame time.
Act only if a flush passes the frame budget. Candidates:
- update only the changed tail;
- a byte-bounded Markdown cache.

## What I'd skip or change

- **F14, version scripts/.** You keep scripts/ local-only by choice, so I'd
  leave it that way. Separate question, yours to decide: a Linux CI job
  running the Rust unit tests on pushes (it needs no scripts/). It costs
  Actions minutes on the private repo.
- **F13, second half (find nested AGENTS.md files as the agent enters
  folders).** It adds tokens to every request in those folders. Measure
  before adopting.
- **A separate request ledger (F02 as written).** Same result from the
  session log with typed usage (step 4), without a second store to keep in
  step.
- **Efficiency presets and child budgets (4.4, 4.5).** Only after steps 4–5
  make the numbers trustworthy.
- **Splitting Main.qml / workspace.rs (5.3).** Only the parts a step
  touches, as it touches them.
- **Their "avoid" list (section 8).** Agreed throughout. It matches what
  the benchmarks already showed.
