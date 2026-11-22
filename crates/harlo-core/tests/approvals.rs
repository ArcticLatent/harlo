//! The shield's modes, approval wording, and answering questions.

// Rust guideline compliant 2026-10-09

use harlo_core::approvals::{Draft, Shield, describe};
use opencode_client::{
    Answer, Choice, Condition, Effect, Field, FieldKind, Form, Permission, Rule,
};

fn field(key: &str, kind: FieldKind) -> Field {
    Field {
        key: key.into(),
        title: Some(key.into()),
        description: None,
        required: false,
        hidden: false,
        when: Vec::new(),
        kind,
    }
}

fn choices(labels: &[&str]) -> Vec<Choice> {
    labels
        .iter()
        .map(|label| Choice {
            value: (*label).into(),
            label: (*label).into(),
            description: None,
        })
        .collect()
}

fn form(fields: Vec<Field>) -> Form {
    Form {
        id: "frm_1".into(),
        session_id: "s1".into(),
        title: "Questions".into(),
        fields,
    }
}

#[test]
fn the_shield_reads_and_writes_a_sessions_rules() {
    for mode in Shield::CHOICES {
        let rules = mode.rules().expect("a mode the user can choose");
        assert_eq!(Shield::of(&rules), mode);
    }
    assert_eq!(Shield::of(&[]), Shield::Agent);
    assert_eq!(
        Shield::of(&[Rule::any("*", Effect::Allow)]),
        Shield::Trusted
    );
    // Rules set elsewhere are kept as they are until a mode is chosen.
    let elsewhere = [Rule::any("webfetch", Effect::Deny)];
    assert_eq!(Shield::of(&elsewhere), Shield::Custom);
    assert_eq!(Shield::Custom.rules(), None);
}

#[test]
fn approvals_say_what_the_agent_wants() {
    let asked = |action: &str, resources: &[&str]| Permission {
        id: "per_1".into(),
        session_id: "s1".into(),
        action: action.into(),
        resources: resources.iter().map(|r| (*r).to_owned()).collect(),
        save: Vec::new(),
        message: None,
        files: Vec::new(),
    };
    let said = describe(&asked("shell", &["cargo test"]));
    assert_eq!(
        (said.wants.as_str(), said.detail.as_str()),
        ("run a command", "cargo test")
    );
    assert_eq!(
        describe(&asked("edit", &["a.rs", "b.rs"])).wants,
        "edit 2 files"
    );
    assert_eq!(describe(&asked("edit", &["a.rs"])).detail, "a.rs");
    assert_eq!(describe(&asked("lsp_rename", &[])).wants, "use lsp rename");
}

#[test]
fn a_single_choice_takes_an_option_or_an_answer_of_ones_own() {
    let colour = field(
        "colour",
        FieldKind::Text {
            options: choices(&["Red", "Blue"]),
            custom: true,
            placeholder: None,
            default: Some("Red".into()),
        },
    );
    let form = form(vec![colour.clone()]);
    let mut draft = Draft::new(&form);
    assert!(draft.is_chosen("colour", "Red"), "starts at the default");

    draft.choose(&colour, "Blue");
    assert!(!draft.is_chosen("colour", "Red"));
    assert_eq!(
        draft.answers(&form),
        Ok(vec![("colour".into(), Answer::Text("Blue".into()))])
    );

    draft.type_text(&colour, "Green".into());
    assert!(
        !draft.is_chosen("colour", "Blue"),
        "one's own replaces the option"
    );
    assert_eq!(
        draft.answers(&form),
        Ok(vec![("colour".into(), Answer::Text("Green".into()))])
    );
}

#[test]
fn a_multiple_choice_toggles_and_keeps_to_its_limits() {
    let extras = Field {
        required: true,
        ..field(
            "extras",
            FieldKind::Multiselect {
                options: choices(&["Tests", "Docs", "Bench"]),
                custom: false,
                min_items: Some(2),
                max_items: Some(2),
                default: Vec::new(),
            },
        )
    };
    let form = form(vec![extras.clone()]);
    let mut draft = Draft::new(&form);
    let problem = |draft: &Draft| draft.answers(&form).expect_err("not yet")[0].text.clone();
    assert_eq!(problem(&draft), "Needs an answer");

    draft.choose(&extras, "Tests");
    assert_eq!(problem(&draft), "Choose at least 2");
    draft.choose(&extras, "Docs");
    draft.choose(&extras, "Bench");
    assert!(!draft.is_chosen("extras", "Bench"), "no more than two");
    assert_eq!(
        draft.answers(&form),
        Ok(vec![(
            "extras".into(),
            Answer::List(vec!["Tests".into(), "Docs".into()])
        )])
    );
    draft.choose(&extras, "Tests");
    assert!(
        !draft.is_chosen("extras", "Tests"),
        "choosing again unchooses"
    );
}

#[test]
fn numbers_are_checked_before_sending() {
    let count = field(
        "count",
        FieldKind::Number {
            integer: true,
            minimum: Some(1.0),
            maximum: Some(10.0),
            default: Some(3.0),
        },
    );
    let form = form(vec![count.clone()]);
    let mut draft = Draft::new(&form);
    assert_eq!(draft.typed("count"), "3");
    for (typed, problem) in [
        ("lots", "Enter a number"),
        ("2.5", "Enter a whole number"),
        ("0", "At least 1"),
        ("11", "At most 10"),
    ] {
        draft.type_text(&count, typed.into());
        let problems = draft.answers(&form).expect_err(typed);
        assert_eq!(problems[0].text, problem, "{typed}");
    }
    draft.type_text(&count, "7".into());
    assert_eq!(
        draft.answers(&form),
        Ok(vec![("count".into(), Answer::Number(7.0))])
    );
}

#[test]
fn fields_are_asked_only_when_their_conditions_hold() {
    let ship = field(
        "ship",
        FieldKind::Boolean {
            default: Some(false),
        },
    );
    let notes = Field {
        required: true,
        when: vec![Condition {
            key: "ship".into(),
            equal: true,
            value: Answer::Boolean(true),
        }],
        ..field(
            "notes",
            FieldKind::Text {
                options: Vec::new(),
                custom: true,
                placeholder: None,
                default: None,
            },
        )
    };
    let secret = Field {
        hidden: true,
        ..field(
            "secret",
            FieldKind::Text {
                options: Vec::new(),
                custom: true,
                placeholder: None,
                default: Some("kept".into()),
            },
        )
    };
    let form = form(vec![ship.clone(), notes.clone(), secret.clone()]);
    let mut draft = Draft::new(&form);
    assert!(!draft.asks(&form, &notes), "not while shipping is off");
    assert!(!draft.asks(&form, &secret), "hidden");
    assert_eq!(
        draft.answers(&form),
        Ok(vec![
            ("ship".into(), Answer::Boolean(false)),
            ("secret".into(), Answer::Text("kept".into())),
        ]),
        "a hidden field sends its default"
    );

    draft.set_flag(&ship, true);
    assert!(draft.asks(&form, &notes));
    assert_eq!(draft.answers(&form).expect_err("notes")[0].key, "notes");
    draft.type_text(&notes, "Friday".into());
    let _ = draft.answers(&form).expect("complete");
}
