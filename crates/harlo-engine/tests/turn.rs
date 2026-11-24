//! One whole turn against the scripted model: the model asks for an edit,
//! the rules ask a person, the edit runs, and the model answers.

// Rust guideline compliant 2026-10-09

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
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
    async fn start(dir: &Path) -> Self {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port")
            .port();
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fake-model.py");
        let child = Command::new("python3")
            .arg("-I")
            .arg(script)
            .arg(port.to_string())
            .arg(dir.join("model.jsonl"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the scripted model");
        for _ in 0..100 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
        },
        system: "You are a coding agent.".into(),
        rules,
        shadow: None,
        memory: None,
        steering: None,
        compact_at: None,
        skills: Vec::new(),
        history: vec![Message::User {
            id: "msg_0".into(),
            time: Created { created: 0 },
            text: prompt.into(),
            skills: Vec::new(),
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
    let model = Model::start(dir.path()).await;
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
    let model = Model::start(dir.path()).await;

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
    let model = Model::start(dir.path()).await;
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

#[tokio::test]
async fn a_full_context_is_summarized_before_the_next_step() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Model::start(dir.path()).await;
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
