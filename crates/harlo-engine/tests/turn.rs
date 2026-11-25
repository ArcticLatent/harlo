//! One whole turn against the scripted model: the model asks for an edit,
//! the rules ask a person, the edit runs, and the model answers.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use harlo_engine::question::Asked;
use harlo_engine::{
    Answer, Ask, Created, Effect, Event, Gate, Message, ModelRef, ModelSpec, Outcome, Part,
    ProviderSpec, Rule, Sink, ToolState, Turn, agent_rules, run,
};
use tokio_util::sync::CancellationToken;

/// The scripted model, stopped when dropped.
struct Model {
    child: Child,
    url: String,
}

impl Model {
    fn start(dir: &Path) -> Self {
        use std::io::BufRead as _;

        // The model takes a free port itself and says which: one found
        // free here first could be handed to another test meanwhile.
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fake-model.py");
        let mut child = Command::new("python3")
            .arg("-I")
            .arg(script)
            .arg("0")
            .arg(dir.join("model.jsonl"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the scripted model");
        let said = child.stdout.take().expect("the model's output");
        let mut line = String::new();
        std::io::BufReader::new(said)
            .read_line(&mut line)
            .expect("the model's port");
        let port: u16 = line.trim().parse().expect("the scripted model's port");
        Self {
            child,
            url: format!("http://127.0.0.1:{port}/v1"),
        }
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct Events(Mutex<Vec<Event>>);

impl Sink for Events {
    fn emit(&self, event: Event) {
        self.0.lock().expect("events").push(event);
    }
}

/// Answers every ask with `answer`, keeping the asks.
struct Person {
    answer: Answer,
    asked: Mutex<Vec<Ask>>,
}

impl Gate for Person {
    fn ask(&self, ask: Ask) -> impl Future<Output = Answer> + Send {
        self.asked.lock().expect("asks").push(ask);
        std::future::ready(self.answer.clone())
    }

    /// Picks each question's first option; with no options, dismisses them.
    fn question(&self, asked: Asked) -> impl Future<Output = Option<Vec<Vec<String>>>> + Send {
        let picks: Option<Vec<Vec<String>>> = asked
            .questions
            .iter()
            .map(|q| q.options.first().map(|o| vec![o.label.clone()]))
            .collect();
        std::future::ready(picks)
    }
}

fn turn(model: &Model, cwd: &Path, prompt: &str, careful: bool) -> Turn {
    let mut rules = vec![("agent".to_owned(), agent_rules())];
    if careful {
        rules.push(("session".to_owned(), vec![Rule::any("edit", Effect::Ask)]));
    }
    Turn {
        cwd: cwd.to_owned(),
        agent: "build".into(),
        model: ModelRef {
            provider: "fake".into(),
            model: "fake-coder".into(),
            variant: None,
        },
        provider: ProviderSpec {
            id: "fake".into(),
            name: "Fake".into(),
            base_url: model.url.clone(),
            api_key: Some("fake-key".into()),
            models: vec![ModelSpec {
                id: "fake-coder".into(),
                name: "Fake Coder".into(),
                ..ModelSpec::default()
            }],
            wire: harlo_engine::catalog::Wire::Chat,
            system_first: false,
        },
        system: "You are a coding agent.".into(),
        rules,
        shadow: None,
        memory: None,
        steering: None,
        compact_at: None,
        sealed: Vec::new(),
        skills: Vec::new(),
        todo: false,
        files: None,
        jobs: None,
        code: None,
        search: None,
        subagents: None,
        subagent_agents: Vec::new(),
        history: vec![Message::User {
            id: "msg_0".into(),
            time: Created { created: 0 },
            text: prompt.into(),
            skills: Vec::new(),
            files: Vec::new(),
        }],
    }
}

const EDIT: &str = r#"tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} ;; say Changed alpha to omega."#;

#[tokio::test]
async fn an_approved_edit_runs_and_the_model_answers() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let model = Model::start(dir.path());
    let events = Events::default();
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };

    let outcome = run(
        turn(&model, &project, EDIT, true),
        &events,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "omega\n"
    );

    let asked = person.asked.lock().expect("asks");
    assert_eq!(asked.len(), 1);
    assert_eq!(
        (asked[0].action.as_str(), asked[0].resources.as_slice()),
        ("edit", ["a.txt".to_owned()].as_slice())
    );
    assert!(
        asked[0].metadata["files"][0]["patch"]
            .as_str()
            .is_some_and(|p| p.contains("+omega"))
    );
    assert!(asked[0].why.contains("session rule"), "{}", asked[0].why);

    let events = events.0.lock().expect("events");
    let steps: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::StepEnded { step } => Some(step),
            _ => None,
        })
        .collect();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].finish.as_deref(), Some("tool-calls"));
    assert!(matches!(
        &steps[0].content[0],
        Part::Tool { name, state: ToolState::Completed { content, .. }, .. }
            if name == "edit" && content[0]["text"] == "Edited a.txt (1 replacement)"
    ));
    assert_eq!(
        steps[0].snapshot.as_ref().map(|s| s.files.clone()),
        Some(vec!["a.txt".to_owned()])
    );
    assert_eq!(
        steps[1].content,
        [Part::Text {
            text: "Changed alpha to omega.".into()
        }]
    );
    let first = events
        .iter()
        .position(|e| matches!(e, Event::ToolCalled { .. }));
    let ended = events
        .iter()
        .position(|e| matches!(e, Event::ToolEnded { .. }));
    assert!(first < ended, "called before it ended");
}

#[tokio::test]
async fn without_a_careful_rule_edits_run_unasked_and_a_refusal_stops_the_turn() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let model = Model::start(dir.path());

    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    let outcome = run(
        turn(&model, &project, EDIT, false),
        &Events::default(),
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert!(
        person.asked.lock().expect("asks").is_empty(),
        "the agent's rules allow edits"
    );

    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let refuses = Person {
        answer: Answer::Reject(None),
        asked: Mutex::default(),
    };
    let events = Events::default();
    let outcome = run(
        turn(&model, &project, EDIT, true),
        &events,
        &refuses,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Interrupted), "{outcome:?}");
    assert_eq!(
        std::fs::read_to_string(project.join("a.txt")).expect("read"),
        "alpha\n",
        "nothing written"
    );
    let ends = events
        .0
        .lock()
        .expect("events")
        .iter()
        .filter(|e| matches!(e, Event::StepEnded { .. }))
        .count();
    assert_eq!(ends, 1, "no second model step after a refusal");
}

#[tokio::test]
async fn a_file_outside_the_project_asks_and_always_covers_its_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    let model = Model::start(dir.path());
    let target = elsewhere.join("note.txt");
    let prompt = format!(
        r#"tool write {{"path":"{}","content":"hello\n"}} ;; say Wrote it."#,
        target.display()
    );

    let refuses = Person {
        answer: Answer::Reject(Some("stay in the project".into())),
        asked: Mutex::default(),
    };
    let outcome = run(
        turn(&model, &project, &prompt, false),
        &Events::default(),
        &refuses,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert!(!target.exists(), "a refused write changes nothing");
    let asked = refuses.asked.lock().expect("asks").clone();
    let real = std::fs::canonicalize(&elsewhere).expect("real folder");
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].action, harlo_engine::OUTSIDE);
    assert_eq!(
        asked[0].resources,
        [real.join("note.txt").display().to_string()]
    );
    assert_eq!(asked[0].save, [format!("{}/*", real.display())]);
    assert!(
        asked[0].metadata.contains_key("files"),
        "the person sees what would be written"
    );

    let allows = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    let outcome = run(
        turn(&model, &project, &prompt, false),
        &Events::default(),
        &allows,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert_eq!(std::fs::read_to_string(&target).expect("read"), "hello\n");
}

/// What the first tool call of a turn returned to the model, as text.
fn first_result(events: &Events) -> String {
    match tool_results(events).into_iter().next() {
        Some(ToolState::Completed { content, .. }) => content
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn a_search_shows_no_file_that_reading_would_ask_about() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("notes.txt"), "TOKEN is in prod.env\n").expect("notes");
    std::fs::write(project.join("prod.env"), "TOKEN=secret\n").expect("env");
    let model = Model::start(dir.path());
    let search =
        |path: &str| format!(r#"tool grep {{"pattern":"TOKEN","path":"{path}"}} ;; say Searched."#);

    // Over the folder, the file is left out, and the result says one was.
    let (events, person) = (
        Events::default(),
        Person {
            answer: Answer::Once,
            asked: Mutex::default(),
        },
    );
    let prompt = search(".");
    let outcome = run(
        turn(&model, &project, &prompt, false),
        &events,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert!(person.asked.lock().expect("asks").is_empty());
    let found = first_result(&events);
    assert!(found.contains("notes.txt:"), "{found}");
    assert!(!found.contains("secret"), "{found}");
    assert!(found.contains("(1 not searched"), "{found}");

    // By itself, it asks as reading it does; refused, nothing is shown.
    let (events, refuses) = (
        Events::default(),
        Person {
            answer: Answer::Reject(None),
            asked: Mutex::default(),
        },
    );
    let prompt = search("prod.env");
    let outcome = run(
        turn(&model, &project, &prompt, false),
        &events,
        &refuses,
        &CancellationToken::new(),
    )
    .await;
    assert!(!matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    let asked = refuses.asked.lock().expect("asks").clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(
        (asked[0].action.as_str(), asked[0].resources.as_slice()),
        ("read", ["prod.env".to_owned()].as_slice())
    );
    assert!(!first_result(&events).contains("secret"));

    // Allowed, the search runs.
    let (events, allows) = (
        Events::default(),
        Person {
            answer: Answer::Once,
            asked: Mutex::default(),
        },
    );
    let outcome = run(
        turn(&model, &project, &prompt, false),
        &events,
        &allows,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert!(first_result(&events).contains("TOKEN=secret"));
}

/// The tool results of the steps a turn ended.
fn tool_results(events: &Events) -> Vec<ToolState> {
    events
        .0
        .lock()
        .expect("events")
        .iter()
        .filter_map(|e| match e {
            Event::StepEnded { step } => Some(step.content.clone()),
            _ => None,
        })
        .flatten()
        .filter_map(|p| match p {
            Part::Tool { state, .. } => Some(state),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn questions_wait_for_answers_and_a_dismissal_stops_the_turn() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    let model = Model::start(dir.path());
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };

    let answered = Events::default();
    let ask = r#"tool question {"questions":[{"question":"Which colour?","options":[{"label":"Blue"}]}]} ;; say Blue it is."#;
    let outcome = run(
        turn(&model, &project, ask, false),
        &answered,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    assert!(matches!(
        &tool_results(&answered)[0],
        ToolState::Completed { content, metadata, .. }
            if content[0]["text"] == "User has answered your questions: \"Which colour?\"=\"Blue\". You can now continue with the user's answers in mind."
                && metadata["answers"] == serde_json::json!([["Blue"]])
    ));

    // With no options the test's person dismisses the questions.
    let dismissed = Events::default();
    let ask = r#"tool question {"questions":[{"question":"Anything else?","options":[]}]} ;; say Never seen."#;
    let outcome = run(
        turn(&model, &project, ask, false),
        &dismissed,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Interrupted), "{outcome:?}");
    assert!(matches!(
        &tool_results(&dismissed)[0],
        ToolState::Error { error, .. } if error.message == "The user dismissed this question"
    ));
}

/// The bodies of the requests the scripted model got, oldest first.
fn requests(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("model.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .map(|request| request["body"].clone())
        .collect()
}

#[tokio::test]
async fn only_a_step_that_may_change_files_records_them() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let model = Model::start(dir.path());
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    // (The scripted model counts results to find its step: one call a step.)
    let script = format!(r#"tool read {{"path":"a.txt"}} ;; {EDIT}"#);
    let mut reads_then_edits = turn(&model, &project, &script, false);
    let store = dir.path().join("snapshots");
    reads_then_edits.shadow = Some(harlo_engine::snapshot::Shadow::new(&store, &project));
    let events = Events::default();
    let outcome = run(
        reads_then_edits,
        &events,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    let events = events.0.lock().expect("events");
    let recorded: Vec<(bool, bool)> = events
        .iter()
        .filter_map(|e| match e {
            Event::StepEnded { step } => step.snapshot.as_ref(),
            _ => None,
        })
        .map(|s| (s.start.is_some(), s.end.is_some()))
        .collect();
    // Looking, then the edit, then the answer.
    assert_eq!(recorded, [(false, false), (true, true), (false, false)]);
    let edit = events
        .iter()
        .find_map(|e| match e {
            Event::StepEnded { step } => step.snapshot.as_ref().filter(|s| s.start.is_some()),
            _ => None,
        })
        .expect("the edit's step");
    assert_ne!(edit.start, edit.end, "the edit changed a file");
}

/// A web search that takes a while, counting how many run at once. The
/// first asked takes longest, so it ends last.
#[derive(Debug, Default)]
struct SlowSearch {
    running: AtomicUsize,
    most: AtomicUsize,
}

impl harlo_engine::web::Searcher for SlowSearch {
    fn search<'a>(
        &'a self,
        query: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Result<harlo_engine::tools::Output, String>> {
        Box::pin(async move {
            let running = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(running, Ordering::SeqCst);
            let wait = if query == "q0" { 120 } else { 40 };
            tokio::time::sleep(Duration::from_millis(wait)).await;
            self.running.fetch_sub(1, Ordering::SeqCst);
            Ok(harlo_engine::tools::Output {
                text: format!("found {query}"),
                metadata: serde_json::Map::new(),
                changed: Vec::new(),
                pictures: Vec::new(),
            })
        })
    }
}

/// What each of the turn's calls returned, as the model reads it.
fn texts(events: &Events) -> Vec<String> {
    tool_results(events)
        .iter()
        .map(|state| match state {
            ToolState::Completed { content, .. } => {
                content[0]["text"].as_str().unwrap_or_default().to_owned()
            }
            other => format!("{other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn calls_that_only_look_run_at_once_and_end_in_the_order_made() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    let model = Model::start(dir.path());
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    let script = (0..10)
        .map(|n| format!(r#"tool websearch {{"query":"q{n}"}}"#))
        .collect::<Vec<_>>()
        .join(" | ");
    let mut searches = turn(&model, &project, &script, false);
    let search = Arc::new(SlowSearch::default());
    let shared: Arc<dyn harlo_engine::web::Searcher> = Arc::<SlowSearch>::clone(&search);
    searches.search = Some(shared);
    let events = Events::default();
    let outcome = run(searches, &events, &person, &CancellationToken::new()).await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    // Eight at a time, however many the model asked for.
    assert_eq!(search.most.load(Ordering::SeqCst), 8);
    // The model reads them as it asked, though the first ended last…
    let expected: Vec<String> = (0..10).map(|n| format!("found q{n}")).collect();
    assert_eq!(texts(&events), expected);
    // …and they are reported in that order too.
    let reported: Vec<String> = events
        .0
        .lock()
        .expect("events")
        .iter()
        .filter_map(|e| match e {
            Event::ToolEnded {
                state: ToolState::Completed { content, .. },
                ..
            } => content[0]["text"].as_str().map(str::to_owned),
            _ => None,
        })
        .collect();
    assert_eq!(reported, expected);
}

#[tokio::test]
async fn a_call_that_changes_files_runs_between_the_reads_beside_it() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let model = Model::start(dir.path());
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    let script = r#"tool read {"path":"a.txt"} | tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} | tool read {"path":"a.txt"}"#;
    let events = Events::default();
    let outcome = run(
        turn(&model, &project, script, false),
        &events,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    let texts = texts(&events);
    assert_eq!(texts.len(), 3, "{texts:?}");
    assert!(texts[0].contains("alpha"), "before the edit: {texts:?}");
    assert!(texts[1].starts_with("Edited a.txt"), "{texts:?}");
    assert!(texts[2].contains("omega"), "after the edit: {texts:?}");
}

#[tokio::test]
async fn a_refusal_still_lets_the_reads_before_it_run() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let model = Model::start(dir.path());
    let person = Person {
        answer: Answer::Reject(None),
        asked: Mutex::default(),
    };
    let script = r#"tool read {"path":"a.txt"} | tool edit {"path":"a.txt","oldString":"alpha","newString":"omega"} | tool read {"path":"a.txt"}"#;
    let events = Events::default();
    let outcome = run(
        turn(&model, &project, script, true),
        &events,
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(!matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    let results = tool_results(&events);
    assert!(
        matches!(&results[0], ToolState::Completed { .. }),
        "{results:?}"
    );
    let kinds: Vec<&str> = results[1..]
        .iter()
        .map(|state| match state {
            ToolState::Error { error, .. } => error.kind.as_str(),
            _ => "ran",
        })
        .collect();
    assert_eq!(kinds, ["rejected", "skipped"]);
    let left = std::fs::read_to_string(project.join("a.txt")).expect("a.txt");
    assert_eq!(left, "alpha\n");
}

/// The names of the tools a request offered the model.
fn offered(request: &serde_json::Value) -> Vec<String> {
    request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn an_agent_is_offered_only_the_tools_its_rules_let_it_use() {
    let dir = tempfile::tempdir().expect("dir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    let model = Model::start(dir.path());
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };

    // The agent that finds things in the code: it reads and searches, and
    // runs commands when a person lets it.
    let explore = harlo_engine::agents::builtins(&dir.path().join("plans"))
        .into_iter()
        .find(|a| a.id == "explore")
        .expect("the explore agent");
    let mut looks = turn(&model, &project, "say Looked.", false);
    looks.rules = vec![(harlo_engine::AGENT.to_owned(), explore.rules)];
    let outcome = run(
        looks,
        &Events::default(),
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    // The default agent, and the same held back from the shell by the
    // configuration.
    let outcome = run(
        turn(&model, &project, "say Built.", false),
        &Events::default(),
        &person,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    let mut held = turn(&model, &project, "say Held.", false);
    held.rules
        .push(("config".to_owned(), vec![Rule::any("shell", Effect::Deny)]));
    let outcome = run(held, &Events::default(), &person, &CancellationToken::new()).await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    let sent = requests(dir.path());
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert_eq!(offered(&sent[0]), ["read", "glob", "grep", "shell"]);
    let all = offered(&sent[1]);
    for tool in ["read", "write", "edit", "shell", "question", "webfetch"] {
        assert!(all.iter().any(|t| t == tool), "{tool} in {all:?}");
    }
    let without: Vec<String> = all.iter().filter(|t| *t != "shell").cloned().collect();
    assert_eq!(offered(&sent[2]), without);
}

#[tokio::test]
async fn a_step_refused_as_too_long_is_summarized_and_asked_again() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Model::start(dir.path());
    let project = dir.path().join("project");
    std::fs::create_dir(&project).expect("project");
    let turn = turn(
        &model,
        &project,
        "flaky 1 400 prompt is too long: 215000 tokens > 200000 maximum | say Fits now.",
        false,
    );
    let events = Events::default();
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    let outcome = run(turn, &events, &person, &CancellationToken::new()).await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");
    let summaries: Vec<String> = events
        .0
        .lock()
        .expect("events")
        .iter()
        .filter_map(|e| match e {
            Event::CompactionEnded { compaction } => Some(compaction.status.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(summaries, ["completed"], "summarized once");

    // The request for the summary is the refused step's, with one message
    // more: the same tools, so the provider reads it from its cache.
    let sent = requests(dir.path());
    assert_eq!(
        sent.len(),
        3,
        "the refused step, the summary, the step again"
    );
    let last_text = |body: &serde_json::Value| {
        let messages = body["messages"].as_array().expect("messages");
        messages.last().expect("a message")["content"].to_string()
    };
    assert!(last_text(&sent[1]).contains("Summarize the conversation"));
    assert!(sent[0]["tools"].as_array().is_some_and(|t| !t.is_empty()));
    assert_eq!(sent[1]["tools"], sent[0]["tools"]);
    assert_eq!(
        sent[1]["messages"][0], sent[0]["messages"][0],
        "the same system prompt"
    );
    let again = sent[2]["messages"].to_string();
    assert!(
        again.contains("Summary of the conversation so far"),
        "{again}"
    );
}

#[tokio::test]
async fn a_full_context_is_summarized_before_the_next_step() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Model::start(dir.path());
    let project = dir.path().join("project");
    std::fs::create_dir(&project).expect("project");
    std::fs::write(project.join("a.txt"), "alpha\n").expect("a.txt");
    let mut turn = turn(
        &model,
        &project,
        r#"tool read {"path":"a.txt"} ;; say Read it."#,
        false,
    );
    turn.compact_at = Some(1);
    let events = Events::default();
    let person = Person {
        answer: Answer::Once,
        asked: Mutex::default(),
    };
    let outcome = run(turn, &events, &person, &CancellationToken::new()).await;
    assert!(matches!(outcome, Outcome::Succeeded), "{outcome:?}");

    let ended: Vec<_> = events
        .0
        .lock()
        .expect("events")
        .iter()
        .filter_map(|e| match e {
            Event::CompactionEnded { compaction } => Some(compaction.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ended.len(),
        1,
        "summarized once, though every step is over the limit"
    );
    assert_eq!(ended[0].status, "completed");
    assert_eq!(ended[0].reason, "auto");

    // The step after the summary reads the summary and the note to carry
    // on, not the history before it.
    let log = std::fs::read_to_string(dir.path().join("model.jsonl")).expect("model log");
    let last: serde_json::Value =
        serde_json::from_str(log.lines().last().expect("a request")).expect("json");
    let sent = last["body"]["messages"].to_string();
    assert!(
        sent.contains("Summary of the conversation so far"),
        "{sent}"
    );
    assert!(sent.contains("Carry on with the work"), "{sent}");
    assert!(!sent.contains("tool read"), "{sent}");
}
