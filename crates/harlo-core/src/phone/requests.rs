//! The server's approvals and questions as the phone asks them, and the
//! phone's answers back as the server's.
//!
//! An approval is a `select` with Allow once, Allow always and Deny. A
//! question with one field is an `input`, a `select` or a `confirm`; one with
//! several is an `ask`, each field a question with its options.

// Rust guideline compliant 2026-10-09

use harlo_client::{Answer, Field, FieldKind, PermissionReply};
use serde_json::{Value, json};

use crate::workspace::{Form, Permission};

/// The approval option that allows this request only.
pub const ALLOW_ONCE: &str = "Allow once";

/// The approval option that allows it and saves the rule.
pub const ALLOW_ALWAYS: &str = "Allow always";

/// The approval option that refuses it.
pub const DENY: &str = "Deny";

/// Returns an approval request as the phone reads it (the phone's `UIRequest`).
#[must_use]
pub fn permission(p: &Permission) -> Value {
    let mut title = format!("Allow tool: {}", p.action);
    for resource in p.resources.iter().take(3) {
        title.push('\n');
        title.push_str(resource);
    }
    // What it acts on (the command, the files) first; why it asks under it.
    let message = match (p.resources.join("\n"), &p.message) {
        (acts_on, Some(why)) if !acts_on.is_empty() => format!("{acts_on}\n\n{why}"),
        (_, Some(why)) => why.clone(),
        (acts_on, None) => acts_on,
    };
    let mut options = vec![ALLOW_ONCE];
    if !p.save.is_empty() {
        options.push(ALLOW_ALWAYS);
    }
    options.push(DENY);
    json!({ "id": p.id, "method": "select", "title": title, "message": message,
            "options": options, "approval": true })
}

/// Returns the reply a phone's answer to an approval gives.
///
/// The phone answers with an option (`{ value }`), a confirmation
/// (`{ confirmed }`), or by dismissing it (`{ cancelled }`), which refuses.
#[must_use]
pub fn permission_reply(answer: &Value) -> PermissionReply {
    match answer["value"].as_str() {
        Some(ALLOW_ONCE) => PermissionReply::Once,
        Some(ALLOW_ALWAYS) => PermissionReply::Always,
        None if answer["confirmed"] == true => PermissionReply::Once,
        Some(_) | None => PermissionReply::Reject,
    }
}

/// The fields a phone is asked: the ones not kept out of sight.
fn shown(form: &Form) -> Vec<&Field> {
    form.fields.iter().filter(|f| !f.hidden).collect()
}

/// The question a field asks, in words.
fn wording(field: &Field) -> String {
    match (&field.title, &field.description) {
        (Some(title), Some(more)) => format!("{title}\n{more}"),
        (Some(text), None) | (None, Some(text)) => text.clone(),
        (None, None) => field.key.clone(),
    }
}

/// Returns a question as the phone reads it (the phone's `UIRequest`).
#[must_use]
pub fn form(form: &Form) -> Value {
    let fields = shown(form);
    let base = |method: &str| json!({ "id": form.id, "method": method, "title": form.title });
    if let [field] = fields.as_slice() {
        let mut request = match &field.kind {
            FieldKind::Text {
                options,
                custom: false,
                ..
            } if !options.is_empty() => {
                let mut r = base("select");
                r["options"] = options.iter().map(|o| o.label.clone()).collect();
                r
            }
            FieldKind::Text {
                placeholder,
                default,
                ..
            } => {
                let mut r = base("input");
                r["placeholder"] = json!(placeholder.clone().unwrap_or_default());
                r["prefill"] = json!(default.clone().unwrap_or_default());
                r
            }
            FieldKind::Boolean { .. } => base("confirm"),
            _ => return ask(form, &fields),
        };
        request["message"] = json!(wording(field));
        return request;
    }
    ask(form, &fields)
}

/// Several questions, or one the simple forms don't fit.
fn ask(form: &Form, fields: &[&Field]) -> Value {
    let questions: Vec<Value> = fields
        .iter()
        .map(|f| {
            let (options, multi): (Vec<String>, bool) = match &f.kind {
                FieldKind::Text { options, .. } => {
                    (options.iter().map(|o| o.label.clone()).collect(), false)
                }
                FieldKind::Multiselect { options, .. } => {
                    (options.iter().map(|o| o.label.clone()).collect(), true)
                }
                FieldKind::Boolean { .. } => (vec!["Yes".to_owned(), "No".to_owned()], false),
                FieldKind::Number { .. } => (Vec::new(), false),
                FieldKind::External { url } => (vec![url.clone()], false),
            };
            json!({ "id": f.key, "question": wording(f), "header": f.title.clone().unwrap_or_default(),
                    "options": options, "multi": multi })
        })
        .collect();
    json!({ "id": form.id, "method": "ask", "title": form.title, "message": form.title,
            "questions": questions })
}

/// The value of the option with `label`, or the label itself as typed text.
fn value_of(options: &[harlo_client::Choice], label: &str) -> String {
    options
        .iter()
        .find(|o| o.label == label)
        .map_or_else(|| label.to_owned(), |o| o.value.clone())
}

/// One field's answer from what the phone picked and typed.
fn field_answer(field: &Field, picked: &[String], typed: &str) -> Option<Answer> {
    let first = picked.first().map_or(typed, String::as_str);
    match &field.kind {
        FieldKind::Text { options, .. } => {
            (!first.is_empty()).then(|| Answer::Text(value_of(options, first)))
        }
        FieldKind::Multiselect { options, .. } => {
            let mut values: Vec<String> = picked.iter().map(|p| value_of(options, p)).collect();
            if !typed.is_empty() {
                values.push(typed.to_owned());
            }
            Some(Answer::List(values))
        }
        FieldKind::Boolean { .. } => match first.to_ascii_lowercase().as_str() {
            "yes" | "true" => Some(Answer::Boolean(true)),
            "no" | "false" => Some(Answer::Boolean(false)),
            _ => None,
        },
        FieldKind::Number { .. } => first.trim().parse().ok().map(Answer::Number),
        FieldKind::External { .. } => None,
    }
}

/// A hidden field's answer: its default, as is.
fn default_answer(field: &Field) -> Option<Answer> {
    match &field.kind {
        FieldKind::Text { default, .. } => default.clone().map(Answer::Text),
        FieldKind::Number { default, .. } => default.map(Answer::Number),
        FieldKind::Boolean { default } => default.map(Answer::Boolean),
        FieldKind::Multiselect { default, .. } => Some(Answer::List(default.clone())),
        FieldKind::External { .. } => None,
    }
}

/// Returns the answers to a question from the phone's answer; `None` when
/// the phone dismissed it.
///
/// The phone answers a simple question with `{ value }` or `{ confirmed }`,
/// and an `ask` with `{ answers: [{ id, selectedOptions, customInput }] }`.
#[must_use]
pub fn form_answers(form: &Form, answer: &Value) -> Option<Vec<(String, Answer)>> {
    if answer["cancelled"] == true {
        return None;
    }
    let fields = shown(form);
    let mut out: Vec<(String, Answer)> = form
        .fields
        .iter()
        .filter(|f| f.hidden)
        .filter_map(|f| default_answer(f).map(|a| (f.key.clone(), a)))
        .collect();
    if let Some(answers) = answer["answers"].as_array() {
        for given in answers {
            let id = given["id"].as_str().unwrap_or_default();
            let Some(field) = fields.iter().find(|f| f.key == id) else {
                continue;
            };
            let picked: Vec<String> = given["selectedOptions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| o.as_str().map(str::to_owned))
                .collect();
            let typed = given["customInput"].as_str().unwrap_or_default().trim();
            if let Some(a) = field_answer(field, &picked, typed) {
                out.push((field.key.clone(), a));
            }
        }
    } else if let Some(field) = fields.first() {
        let given = match (answer["value"].as_str(), answer["confirmed"].as_bool()) {
            (Some(value), _) => value.to_owned(),
            (None, Some(true)) => "yes".to_owned(),
            (None, Some(false)) => "no".to_owned(),
            (None, None) => String::new(),
        };
        if let Some(a) = field_answer(field, &[], &given) {
            out.push((field.key.clone(), a));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking(resources: &[&str], message: Option<&str>) -> Permission {
        Permission {
            id: "per_1".into(),
            session_id: "ses_1".into(),
            action: "shell".into(),
            resources: resources.iter().map(|&r| r.to_owned()).collect(),
            save: vec!["git *".into()],
            message: message.map(str::to_owned),
            files: Vec::new(),
            call: None,
        }
    }

    #[test]
    fn an_approval_shows_what_it_runs_before_why_it_asks() {
        let why = "Asks because the session rule \"shell *\" says ask.";
        let request = permission(&asking(&["git clean -fdx"], Some(why)));
        assert_eq!(request["message"], format!("git clean -fdx\n\n{why}"));
        assert_eq!(request["options"], json!([ALLOW_ONCE, ALLOW_ALWAYS, DENY]));

        let request = permission(&asking(&["git clean -fdx"], None));
        assert_eq!(request["message"], "git clean -fdx");
        let request = permission(&asking(&[], Some(why)));
        assert_eq!(request["message"], why);
    }
}
