//! A randomized fault-injection tester for the workflow engine.
//!
//! Each simulated run goes through a random workflow (an implementation,
//! then checks and reviews with their own tries) in a world where things go
//! wrong: agents that fail, go quiet or error; checks that fail; results
//! that come late, twice or out of order; harlo crashing at any point (the
//! run saved as JSON and loaded again, then recovered); machines that don't
//! answer; and a user who pauses, resumes and stops. After every event the
//! run is checked against the engine's promises:
//!
//! - a run is done only when, on its last attempt, the implementation
//!   reported and every later stage passed, in order;
//! - a result from an earlier dispatch or attempt changes nothing;
//! - once known, the implementation session never changes, and every fix
//!   goes to it;
//! - a running run always has something under way (it never sits idle);
//! - a paused run says why, and starts nothing new;
//! - what it dispatches is what it waits for, and a saved run loads the same;
//! - once done or stopped, nothing moves it.
//!
//! `WORKFLOW_SIM_RUNS` (default 3000) and `WORKFLOW_SIM_SEED` (default
//! random per process, printed) set the work; a failure prints the seed of
//! the run that broke, and `WORKFLOW_SIM_SEED=<it> WORKFLOW_SIM_RUNS=1`
//! replays it with its trace. `WORKFLOW_SIM_REPORT=<file>` writes the
//! summary as JSON (scripts/workflow-lab.sh reads it).

// Rust guideline compliant 2026-10-09

use std::collections::BTreeMap;
use std::path::PathBuf;

use harlo_core::workflow::{self, Action, Agent, Definition, Event, Next, Outcome, Place, Request, Run, Stage, StageKind, State, Worktree};

/// A small, seedable random number generator (splitmix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// True with probability `p`.
    fn chance(&mut self, p: f64) -> bool {
        #[expect(clippy::cast_precision_loss, reason = "a probability needs no more than 53 bits")]
        let x = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        x < p
    }

    /// A number in `0..n` (`n` > 0).
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }
}

/// How often things go wrong in the simulated world.
#[derive(Clone, Copy)]
struct Faults {
    crash: f64,
    duplicate: f64,
    stale: f64,
    pause: f64,
    stop: f64,
    worktree_fails: f64,
    start_fails: f64,
    quiet: f64,
    error: f64,
    wrong_attempt: f64,
    host_gone: f64,
    survives_crash: f64,
    session_file_written: f64,
}

const FAULTS: Faults = Faults {
    crash: 0.03,
    duplicate: 0.08,
    stale: 0.05,
    pause: 0.02,
    stop: 0.002,
    worktree_fails: 0.05,
    start_fails: 0.04,
    quiet: 0.08,
    error: 0.02,
    wrong_attempt: 0.04,
    host_gone: 0.2,
    survives_crash: 0.4,
    session_file_written: 0.6,
};

/// Something the world does or tells harlo, in the order it comes.
#[derive(Clone, Debug)]
enum Item {
    /// harlo carries out an action (a crash before this: it never happened).
    Do(Action),
    /// An event reaches the run.
    Tell(Event),
}

/// The world around one run: its pending items, the sessions that exist,
/// and what the engine took.
struct World {
    rng: Rng,
    queue: Vec<Item>,
    /// Events delivered so far (stale copies come from here).
    told: Vec<Event>,
    /// Sessions by their first message's marker (the latest last).
    sessions: BTreeMap<String, Vec<Place>>,
    /// The engine took these results: (attempt, stage index, passed).
    taken: Vec<(u32, usize, bool)>,
    /// A worktree is being made.
    making_worktree: bool,
    next_session: u32,
    trace: Vec<String>,
    stats: Stats,
    /// On a machine (its sessions and worktree there).
    host: String,
    now: i64,
}

#[derive(Default, Clone, Copy)]
struct Stats {
    runs: u64,
    done: u64,
    stopped: u64,
    unfinished: u64,
    events: u64,
    crashes: u64,
    recoveries_reconnected: u64,
    recoveries_paused: u64,
    duplicates: u64,
    stale: u64,
    ignored: u64,
    pauses: u64,
    resumes: u64,
    hosts_gone: u64,
    max_attempt: u32,
}

impl World {
    fn log(&mut self, line: String) {
        if self.trace.len() < 4000 {
            self.trace.push(line);
        }
    }

    /// A session of this run's machine, new.
    fn new_session(&mut self) -> Place {
        self.next_session += 1;
        Place { host: self.host.clone(), path: PathBuf::from(format!("/sessions/s{}.jsonl", self.next_session)) }
    }

    /// The marker of the dispatch `seq` (from the run's wait).
    fn marker_of(run: &Run, seq: u64) -> String {
        run.wait.as_ref().filter(|w| w.seq == seq).map(|w| w.marker.clone()).unwrap_or_default()
    }

    /// An agent's answer to a dispatch, in the queue: a report, no report,
    /// or an error. A report for the wrong attempt (refused when it's made)
    /// is followed by its turn's end without one.
    fn answer(&mut self, seq: u64, attempt: u32, kind: StageKind) {
        if self.rng.chance(FAULTS.error) {
            self.queue.push(Item::Tell(Event::NoReport { seq, error: Some("the model's server answered 500".into()) }));
            return;
        }
        if self.rng.chance(FAULTS.quiet) {
            self.queue.push(Item::Tell(Event::NoReport { seq, error: None }));
            return;
        }
        if attempt > 1 && self.rng.chance(FAULTS.wrong_attempt) {
            self.queue.push(Item::Tell(Event::Report { seq, attempt: attempt - 1, ok: true, summary: "Done.".into() }));
            self.queue.push(Item::Tell(Event::NoReport { seq, error: None }));
            return;
        }
        // (Later attempts pass more often, so most runs finish.)
        let pass = match kind {
            StageKind::Implement => 0.92,
            _ => 0.45 + 0.15 * f64::from(attempt.min(4)),
        };
        let ok = self.rng.chance(pass);
        self.queue.push(Item::Tell(Event::Report { seq, attempt, ok, summary: if ok { "Done.".into() } else { "Line 3 is wrong.".into() } }));
    }

    /// Carries out an action: what comes of it goes in the queue.
    fn act(&mut self, run: &Run, action: &Action) {
        match action {
            Action::MakeWorktree { branch } => {
                self.making_worktree = true;
                let event = if self.rng.chance(FAULTS.worktree_fails) {
                    Event::WorktreeFailed("the disk is full".into())
                } else {
                    Event::WorktreeReady(Worktree {
                        place: Place { host: self.host.clone(), path: PathBuf::from(format!("/worktrees/{branch}")) },
                        branch: branch.clone(),
                        base: "main".into(),
                        base_commit: "abc123".into(),
                    })
                };
                self.queue.push(Item::Tell(event));
            }
            Action::StartImplementation { seq, .. } | Action::StartReview { seq, .. } => {
                let kind = if matches!(action, Action::StartImplementation { .. }) { StageKind::Implement } else { StageKind::Review };
                if self.rng.chance(FAULTS.start_fails) {
                    self.queue.push(Item::Tell(Event::StartFailed { seq: *seq, why: "the agent isn't installed".into() }));
                    return;
                }
                let marker = Self::marker_of(run, *seq);
                let attempt = run.wait.as_ref().map_or(run.attempt, |w| w.attempt);
                let session = self.new_session();
                self.sessions.entry(marker).or_default().push(session.clone());
                self.queue.push(Item::Tell(Event::Started { seq: *seq }));
                self.queue.push(Item::Tell(Event::SessionKnown { seq: *seq, session }));
                self.answer(*seq, attempt, kind);
            }
            Action::Prompt { seq, kind, .. } => {
                let attempt = run.wait.as_ref().filter(|w| w.seq == *seq).map_or(run.attempt, |w| w.attempt);
                self.answer(*seq, attempt, *kind);
            }
            Action::RunCheck { seq, attempt, .. } => {
                let pass = 0.5 + 0.15 * f64::from((*attempt).min(3));
                let outcome = if self.rng.chance(0.03) {
                    Outcome { exit: None, tail: String::new(), seconds: 1, error: Some("ssh: connection closed".into()), name: String::new() }
                } else if self.rng.chance(pass) {
                    Outcome { exit: Some(0), tail: "ok".into(), seconds: 2, error: None, name: String::new() }
                } else {
                    Outcome { exit: Some(1), tail: "1 failed".into(), seconds: 3, error: None, name: String::new() }
                };
                self.queue.push(Item::Tell(Event::Started { seq: *seq }));
                self.queue.push(Item::Tell(Event::Checked { seq: *seq, attempt: *attempt, outcome }));
            }
            Action::Tell { .. } => {}
        }
    }
}

/// A random workflow: an implementation, then one to four checks and
/// reviews.
fn definition(rng: &mut Rng) -> Definition {
    let mut stages = vec![Stage::implement()];
    for i in 0..=rng.below(4) {
        let tries = u32::try_from(rng.below(4)).unwrap_or(0);
        if rng.chance(0.5) {
            stages.push(Stage::check(&format!("Check {i}"), "", tries));
        } else {
            stages.push(Stage::review(&format!("Review {i}"), Agent::default(), tries));
        }
    }
    Definition { id: "wfd-sim".into(), name: "Simulated".into(), stages }
}

/// What the run is, for comparing before and after an event (its log
/// aside).
fn shape(run: &Run) -> (State, usize, u32, Option<workflow::Wait>, Option<Place>, u64, Option<Worktree>) {
    (run.state.clone(), run.stage, run.attempt, run.wait.clone(), run.implementation.clone(), run.next_seq, run.worktree.clone())
}

/// A broken promise.
struct Broken(String);

/// Checks what every step must keep: the actions it gave and the run after.
fn check_after(run: &Run, actions: &[Action], world: &World, implementation: Option<&Place>) -> Result<(), Broken> {
    // Once known, the implementation session never changes.
    if let (Some(before), Some(now)) = (implementation, run.implementation.as_ref())
        && before != now
    {
        return Err(Broken(format!("the implementation session changed from {before:?} to {now:?}")));
    }
    if let Some(w) = &run.wait
        && w.seq >= run.next_seq
    {
        return Err(Broken(format!("it waits on dispatch {} but its next is {}", w.seq, run.next_seq)));
    }
    let paused = matches!(run.state, State::Paused { .. });
    for a in actions {
        match a {
            Action::StartImplementation { seq, .. } | Action::StartReview { seq, .. } | Action::RunCheck { seq, .. } => {
                if paused {
                    return Err(Broken(format!("it started new work while paused: {a:?}")));
                }
                if run.wait.as_ref().map(|w| w.seq) != Some(*seq) {
                    return Err(Broken(format!("it dispatched {seq} but waits on {:?}", run.wait.as_ref().map(|w| w.seq))));
                }
            }
            Action::Prompt { seq, kind, session, .. } => {
                if run.wait.as_ref().map(|w| w.seq) != Some(*seq) {
                    return Err(Broken(format!("it prompted for {seq} but waits on {:?}", run.wait.as_ref().map(|w| w.seq))));
                }
                if *kind == StageKind::Implement && Some(session) != run.implementation.as_ref() {
                    return Err(Broken(format!("a fix went to {session:?}, not the implementation session {:?}", run.implementation)));
                }
            }
            Action::MakeWorktree { .. } if paused => return Err(Broken("it made a worktree while paused".into())),
            Action::MakeWorktree { .. } | Action::Tell { .. } => {}
        }
        if let Action::StartImplementation { .. } = a
            && run.implementation.is_some()
        {
            return Err(Broken("it started a second implementation session".into()));
        }
    }
    match &run.state {
        State::Paused { why, .. } if why.trim().is_empty() => return Err(Broken("it paused without saying why".into())),
        State::Done => {
            // Done: on the last attempt, the implementation reported and
            // every later stage passed, in order.
            let n = run.definition.stages.len();
            let tail: Vec<(u32, usize, bool)> = world.taken.iter().rev().take(n).rev().copied().collect();
            let want: Vec<(u32, usize, bool)> = (0..n).map(|i| (run.attempt, i, true)).collect();
            if tail != want {
                return Err(Broken(format!("done on attempt {} but the last results were {tail:?}", run.attempt)));
            }
        }
        _ => {}
    }
    // Saved and loaded, it's the same run.
    let json = serde_json::to_string(run).map_err(|e| Broken(format!("it doesn't save: {e}")))?;
    let back: Run = serde_json::from_str(&json).map_err(|e| Broken(format!("it doesn't load: {e}")))?;
    if &back != run {
        return Err(Broken("saved and loaded, it's a different run".into()));
    }
    Ok(())
}

/// The result an event is about, when it's the one the run waits for:
/// (stage index, passed).
fn result_of(run: &Run, event: &Event) -> Option<(usize, bool)> {
    let wait = run.wait.as_ref()?;
    match event {
        Event::Report { seq, attempt, ok, .. } if *seq == wait.seq && *attempt == wait.attempt && wait.kind != StageKind::Check => Some((run.stage, *ok)),
        Event::Checked { seq, attempt, outcome } if *seq == wait.seq && *attempt == wait.attempt && wait.kind == StageKind::Check => Some((run.stage, outcome.passed())),
        _ => None,
    }
}

/// A result from an earlier dispatch: it must change nothing.
fn is_stale(run: &Run, event: &Event) -> bool {
    let seq = match event {
        Event::Started { seq }
        | Event::StartFailed { seq, .. }
        | Event::SessionKnown { seq, .. }
        | Event::Report { seq, .. }
        | Event::NoReport { seq, .. }
        | Event::Checked { seq, .. } => *seq,
        _ => return false,
    };
    run.wait.as_ref().is_none_or(|w| w.seq != seq)
        || matches!(event, Event::Report { attempt, .. } | Event::Checked { attempt, .. } if run.wait.as_ref().is_some_and(|w| w.attempt != *attempt))
}

/// One simulated run from its start: `Ok(true)` when it ended (done or
/// stopped), `Ok(false)` still going at the end of its time, or what broke.
#[expect(clippy::too_many_lines, reason = "one run's whole story reads best as one loop")]
fn simulate(seed: u64, stats: &mut Stats) -> Result<bool, (Broken, Vec<String>)> {
    let mut rng = Rng(seed);
    let host = if rng.chance(0.4) { "box".to_owned() } else { String::new() };
    let def = definition(&mut rng);
    let mut world = World {
        rng,
        queue: Vec::new(),
        told: Vec::new(),
        sessions: BTreeMap::new(),
        taken: Vec::new(),
        making_worktree: false,
        next_session: 0,
        trace: Vec::new(),
        stats: *stats,
        host: host.clone(),
        now: 1_000,
    };
    world.stats.runs += 1;
    let request = Request {
        project_id: 1,
        project: Place { host: host.clone(), path: PathBuf::from("/w/app") },
        definition: def.clone(),
        task: "Simulated task".into(),
        criteria: "- works".into(),
        test_command: "make test".into(),
        implementer: Agent { harness: "harlo".into(), model: String::new() },
        reviewer: Agent { harness: "claude".into(), model: String::new() },
        approval: String::new(),
    };
    let mut run = workflow::new_run("wf-sim", request, world.now);
    world.log(format!("stages: {:?}, host {host:?}", def.stages.iter().map(|s| (s.kind, s.tries)).collect::<Vec<_>>()));
    let fail = |world: &mut World, b: Broken| -> Result<bool, (Broken, Vec<String>)> { Err((b, std::mem::take(&mut world.trace))) };

    let actions = workflow::step(&mut run, Event::Begin, world.now);
    for a in &actions {
        world.act(&run, a);
    }
    let mut implementation: Option<Place> = None;
    // (A paused run is resumed by "you" after a while; a run waiting for
    // its machine, once it answers.)
    let mut paused_for = 0;
    for _ in 0..3000 {
        world.now += 1;
        if run.finished() {
            let before = shape(&run);
            // Nothing moves a finished run.
            for e in [Event::Resume, Event::Pause, Event::Begin] {
                let a = workflow::step(&mut run, e, world.now);
                if !a.is_empty() || shape(&run) != before {
                    return fail(&mut world, Broken("a finished run moved".into()));
                }
            }
            if run.state == State::Done {
                world.stats.done += 1;
            } else {
                world.stats.stopped += 1;
            }
            world.stats.max_attempt = world.stats.max_attempt.max(run.attempt);
            *stats = world.stats;
            return Ok(true);
        }
        // Harlo crashes: the run as saved, loaded again; what was under way
        // is lost (some results still come, late); then recovered.
        if world.rng.chance(FAULTS.crash) {
            world.stats.crashes += 1;
            let json = serde_json::to_string(&run).map_err(|e| (Broken(format!("save: {e}")), Vec::new()))?;
            run = serde_json::from_str(&json).map_err(|e| (Broken(format!("load: {e}")), Vec::new()))?;
            let survivors: Vec<Item> = world.queue.drain(..).filter(|i| matches!(i, Item::Tell(_))).collect();
            let mut kept = Vec::new();
            for item in survivors {
                // (A session started but harlo never heard of it: its file
                // may or may not be there.)
                if world.rng.chance(FAULTS.survives_crash) {
                    kept.push(item);
                }
            }
            world.queue = kept;
            if !world.rng.chance(FAULTS.session_file_written)
                && let Some(w) = run.wait.as_ref().filter(|w| w.session.is_none())
            {
                world.sessions.remove(&w.marker);
            }
            world.making_worktree = false;
            world.log(format!("CRASH at {:?} (wait {:?})", run.state, run.wait.as_ref().map(|w| (w.seq, w.kind, w.attempt, w.started))));
            if !host.is_empty() && run.state == State::Running && world.rng.chance(FAULTS.host_gone) {
                // Its machine doesn't answer: paused (as the app does) until
                // it does.
                world.stats.hosts_gone += 1;
                run.state = State::Paused { why: "harlo couldn't list the sessions on box".into(), next: Next::Recover };
                world.log("host gone: paused to recover later".into());
                continue;
            }
            let sessions = world.sessions.clone();
            let find = |marker: &str| sessions.get(marker).and_then(|v| v.last().cloned());
            let before = run.wait.clone();
            let was_running = run.state == State::Running;
            let actions = workflow::recover(&mut run, &find, world.now);
            match (&run.state, actions.is_empty()) {
                (State::Paused { .. }, _) if was_running => world.stats.recoveries_paused += 1,
                (State::Running, false) => world.stats.recoveries_reconnected += 1,
                _ => {}
            }
            world.log(format!("recover from {:?}: {actions:?} → {:?}", before.map(|w| w.seq), run.state));
            if let Err(b) = check_after(&run, &actions, &world, implementation.as_ref()) {
                return fail(&mut world, b);
            }
            if run.state == State::Running && run.wait.is_none() && run.worktree.is_none() && actions.is_empty() {
                // (A worktree lost in the crash: paused by recover.)
                return fail(&mut world, Broken("recovered without a worktree and without saying so".into()));
            }
            for a in &actions {
                world.act(&run, a);
            }
            implementation = run.implementation.clone().or(implementation);
            continue;
        }
        // You pause, resume or stop it now and then.
        if let State::Paused { next, .. } = run.state.clone() {
            paused_for += 1;
            if paused_for > 5 + world.rng.below(20) {
                paused_for = 0;
                world.stats.resumes += 1;
                let actions = if next == Next::Recover {
                    // (Its machine answers: the app picks it up.)
                    run.state = State::Running;
                    let sessions = world.sessions.clone();
                    let find = |marker: &str| sessions.get(marker).and_then(|v| v.last().cloned());
                    workflow::recover(&mut run, &find, world.now)
                } else {
                    workflow::step(&mut run, Event::Resume, world.now)
                };
                world.log(format!("resume ({next:?}) → {actions:?}"));
                if let Err(b) = check_after(&run, &actions, &world, implementation.as_ref()) {
                    return fail(&mut world, b);
                }
                for a in &actions {
                    world.act(&run, a);
                }
                continue;
            }
        } else if world.rng.chance(FAULTS.pause) {
            world.stats.pauses += 1;
            let actions = workflow::step(&mut run, Event::Pause, world.now);
            world.log(format!("pause → {:?}", run.state));
            if let Err(b) = check_after(&run, &actions, &world, implementation.as_ref()) {
                return fail(&mut world, b);
            }
            continue;
        }
        if world.rng.chance(FAULTS.stop) {
            let actions = workflow::step(&mut run, Event::Stop, world.now);
            world.log("stop".into());
            if let Err(b) = check_after(&run, &actions, &world, implementation.as_ref()) {
                return fail(&mut world, b);
            }
            continue;
        }
        // A stale result: something told before, again.
        if !world.told.is_empty() && world.rng.chance(FAULTS.stale) {
            let i = world.rng.below(world.told.len());
            let event = world.told[i].clone();
            if is_stale(&run, &event) {
                world.stats.stale += 1;
                world.queue.push(Item::Tell(event));
            }
        }
        if world.queue.is_empty() {
            // Nothing under way: only right when it's paused (and you'll
            // resume it) or making its worktree is lost.
            if run.state == State::Running {
                return fail(&mut world, Broken(format!("it's running with nothing under way (stage {}, wait {:?})", run.stage, run.wait)));
            }
            continue;
        }
        // The next item, not always the first (out of order), sometimes
        // twice.
        let i = if world.rng.chance(0.3) { world.rng.below(world.queue.len()) } else { 0 };
        // (Results come twice; harlo does each action once.)
        let item = if matches!(world.queue[i], Item::Tell(_)) && world.rng.chance(FAULTS.duplicate) {
            world.stats.duplicates += 1;
            world.queue[i].clone()
        } else {
            world.queue.remove(i)
        };
        match item {
            Item::Do(action) => world.act(&run, &action),
            Item::Tell(event) => {
                world.stats.events += 1;
                if matches!(event, Event::WorktreeReady(_) | Event::WorktreeFailed(_)) {
                    if !world.making_worktree {
                        // (A worktree from before a crash: the app drops it.)
                        continue;
                    }
                    world.making_worktree = false;
                }
                let stale = is_stale(&run, &event);
                let taken = result_of(&run, &event);
                let before = shape(&run);
                let actions = workflow::step(&mut run, event.clone(), world.now);
                world.log(format!("{event:?} → {actions:?} ({:?}, stage {}, attempt {})", run.state, run.stage, run.attempt));
                if stale {
                    world.stats.ignored += 1;
                    if shape(&run) != before || !actions.is_empty() {
                        return fail(&mut world, Broken(format!("a stale {event:?} moved the run")));
                    }
                }
                if let Some((stage, passed)) = taken {
                    if run.wait.as_ref().is_some_and(|w| Some(w.seq) == before.3.as_ref().map(|b| b.seq)) {
                        return fail(&mut world, Broken("the result it waited for was taken but it still waits for it".into()));
                    }
                    world.taken.push((before.2, stage, passed));
                }
                world.told.push(event);
                if let Err(b) = check_after(&run, &actions, &world, implementation.as_ref()) {
                    return fail(&mut world, b);
                }
                implementation = run.implementation.clone().or(implementation);
                for a in actions {
                    if matches!(a, Action::Tell { .. }) {
                        continue;
                    }
                    // (Carried out later: a crash in between loses it.)
                    world.queue.push(Item::Do(a));
                }
            }
        }
    }
    world.stats.unfinished += 1;
    world.log(format!("unfinished: {:?}, stage {}, attempt {}, {} log lines", run.state, run.stage, run.attempt, run.log.len()));
    if std::env::var("WORKFLOW_SIM_RUNS").as_deref() == Ok("1") {
        for line in run.log.iter().rev().take(30).rev() {
            eprintln!("  log #{} {}", line.attempt, line.text);
        }
    }
    *stats = world.stats;
    Ok(false)
}

#[test]
fn the_engine_keeps_its_promises_under_random_faults() {
    let runs: u64 = std::env::var("WORKFLOW_SIM_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3000);
    let seed: u64 = std::env::var("WORKFLOW_SIM_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or(1)
    });
    let started = std::time::Instant::now();
    let mut stats = Stats::default();
    let mut broken: Vec<(u64, String, Vec<String>)> = Vec::new();
    let mut unfinished: Vec<u64> = Vec::new();
    for i in 0..runs {
        let run_seed = seed.wrapping_add(i.wrapping_mul(0x2545_f491_4f6c_dd1d));
        match simulate(run_seed, &mut stats) {
            Ok(true) => {}
            Ok(false) => unfinished.push(run_seed),
            Err((b, trace)) => {
                broken.push((run_seed, b.0, trace));
                if broken.len() >= 5 {
                    break;
                }
            }
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    let summary = format!(
        "workflow-sim: seed {seed}, {} runs in {seconds:.1} s: {} done, {} stopped, {} unfinished; {} events ({} stale or late ignored, {} duplicates), {} crashes ({} reconnected, {} paused), {} hosts gone, {} pauses, {} resumes, attempts up to {}; {} broken",
        stats.runs,
        stats.done,
        stats.stopped,
        stats.unfinished,
        stats.events,
        stats.ignored,
        stats.duplicates,
        stats.crashes,
        stats.recoveries_reconnected,
        stats.recoveries_paused,
        stats.hosts_gone,
        stats.pauses,
        stats.resumes,
        stats.max_attempt,
        broken.len()
    );
    eprintln!("{summary}");
    if !unfinished.is_empty() {
        eprintln!("still going at the end of their time (WORKFLOW_SIM_SEED=<it> WORKFLOW_SIM_RUNS=1): {:?}", &unfinished[..unfinished.len().min(5)]);
    }
    if let Ok(path) = std::env::var("WORKFLOW_SIM_REPORT") {
        let report = serde_json::json!({
            "seed": seed, "seconds": seconds, "runs": stats.runs, "done": stats.done, "stopped": stats.stopped, "unfinished": stats.unfinished,
            "events": stats.events, "ignored": stats.ignored, "duplicates": stats.duplicates, "stale": stats.stale,
            "crashes": stats.crashes, "reconnected": stats.recoveries_reconnected, "pausedOnRecovery": stats.recoveries_paused,
            "hostsGone": stats.hosts_gone, "pauses": stats.pauses, "resumes": stats.resumes, "maxAttempt": stats.max_attempt,
            "unfinishedSeeds": &unfinished[..unfinished.len().min(20)],
            "broken": broken.iter().map(|(s, why, _)| serde_json::json!({ "seed": s, "why": why })).collect::<Vec<_>>(),
        });
        let _ = std::fs::write(&path, report.to_string());
    }
    for (s, why, trace) in &broken {
        eprintln!("\nbroken (WORKFLOW_SIM_SEED={s} WORKFLOW_SIM_RUNS=1): {why}");
        if runs == 1 || broken.len() == 1 {
            for line in trace.iter().rev().take(60).rev() {
                eprintln!("  {line}");
            }
        }
    }
    assert!(broken.is_empty(), "{} runs broke a promise (first: {})", broken.len(), broken[0].1);
    assert!(stats.done > stats.runs / 2, "most runs should finish ({} of {} done)", stats.done, stats.runs);
}
