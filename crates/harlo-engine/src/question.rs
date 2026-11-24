//! The `question` tool, with which the agent asks the person.
//!
//! Each question has options to pick from, and the person may answer in
//! their own words. The turn waits on its [`Gate`](crate::turn::Gate) and
//! goes on with the answers.

// Rust guideline compliant 2026-10-09

use serde_json::{Map, Value, json};

/// An option to pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// What's shown and answered.
    pub label: String,
    /// More about it.
    pub description: Option<String>,
}

/// One question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The question itself.
    pub question: String,
    /// A short heading for it.
    pub header: Option<String>,
    /// The options; a person may answer otherwise too.
    pub options: Vec<Choice>,
    /// Whether several options may be picked.
    pub multiple: bool,
}

/// Questions a turn is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// The form's ID, such as `frm_…`.
    pub id: String,
    /// The step's message ID.
    pub message: String,
    /// The tool call's ID.
    pub call: String,
    /// The questions, in order.
    pub questions: Vec<Question>,
}

/// The tool's definition, for the model.
#[must_use]
pub fn definition() -> Value {
    json!({
        "name": "question",
        "description": "Asks the person questions when you need their decision or information you \
            can't find yourself. Give options for each; the person can also answer in their own words. \
            Wait for the answers before going on.",
        "parameters": { "type": "object", "properties": {
            "questions": { "type": "array", "items": { "type": "object", "properties": {
                "question": { "type": "string", "description": "The question, complete." },
                "header": { "type": "string", "description": "A short heading, a word or two." },
                "options": { "type": "array", "items": { "type": "object", "properties": {
                    "label": { "type": "string" },
                    "description": { "type": "string" }
                }, "required": ["label"] } },
                "multiple": { "type": "boolean", "description": "Whether several options may be picked." }
            }, "required": ["question", "options"] } }
        }, "required": ["questions"] }
    })
}

/// The questions in a call's input.
///
/// # Errors
///
/// When there are none, or one lacks its text.
pub fn parse(input: &Map<String, Value>) -> Result<Vec<Question>, String> {
    let list = input
        .get("questions")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .ok_or_else(|| "question needs `questions`, a list of at least one.".to_owned())?;
    list.iter()
        .map(|q| {
            let question = q["question"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| "Each question needs `question`, as text.".to_owned())?;
            let options = q["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| {
                    let label = o["label"].as_str().filter(|l| !l.is_empty())?;
                    Some(Choice {
                        label: label.to_owned(),
                        description: o["description"].as_str().map(str::to_owned),
                    })
                })
                .collect();
            Ok(Question {
                question: question.to_owned(),
                header: q["header"].as_str().map(str::to_owned),
                options,
                multiple: q["multiple"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// What the model reads once the person answered: each question with its
/// answer, several picks joined by commas.
#[must_use]
pub fn answered(questions: &[Question], answers: &[Vec<String>]) -> String {
    let pairs: Vec<String> = questions
        .iter()
        .zip(answers)
        .map(|(q, a)| format!("\"{}\"=\"{}\"", q.question, a.join(", ")))
        .collect();
    format!(
        "User has answered your questions: {}. You can now continue with the user's answers in mind.",
        pairs.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_parse_and_answers_read_back() {
        let input = json!({ "questions": [
            { "question": "Which colour?", "header": "Colour", "options": [{ "label": "Red" }, { "label": "Blue", "description": "cool" }] },
            { "question": "Which sizes?", "multiple": true, "options": [{ "label": "S" }, { "label": "L" }] }
        ]});
        let questions = parse(input.as_object().expect("object")).expect("parsed");
        assert_eq!(questions[0].options[1].description.as_deref(), Some("cool"));
        assert!(questions[1].multiple);
        assert_eq!(
            answered(
                &questions,
                &[
                    vec!["Blue".to_owned()],
                    vec!["S".to_owned(), "L".to_owned()]
                ]
            ),
            "User has answered your questions: \"Which colour?\"=\"Blue\", \"Which sizes?\"=\"S, L\". \
             You can now continue with the user's answers in mind."
        );
        parse(json!({ "questions": [] }).as_object().expect("object")).expect_err("none");
    }
}
