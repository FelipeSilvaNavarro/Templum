//! `ask_user`'s questions (docs/specs/ask-user.md): parsed from either shape, and the answer's
//! text. No UI.

use serde_json::{Map, Value, json};

/// The most characters of a header shown; a longer one is cut with "…".
pub const HEADER_MAX: usize = 12;
/// The most characters of the question a notice names.
const NOTICE_MAX: usize = 60;

/// An `ask_user` call's questions.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub questions: Vec<Question>,
    /// Asked with `questions`: the answer is JSON. Today's `question` gets plain text.
    pub json: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub text: String,
    pub header: Option<String>,
    pub options: Vec<Choice>,
    pub multi: bool,
}

/// One of a question's options.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    pub description: Option<String>,
    pub preview: Option<String>,
}

/// `ask_user`'s arguments, in either shape; Err names the place that is wrong.
pub fn parse(args: &Value) -> Result<Ask, String> {
    match (&args["question"], &args["questions"]) {
        (Value::Null, Value::Null) => {
            Err("ask_user needs \"questions\" (or \"question\", a string)".into())
        }
        (Value::Null, questions) => questions_of(questions),
        (question, Value::Null) => {
            let text = question
                .as_str()
                .ok_or("ask_user needs \"question\", a string")?;
            let options = match &args["options"] {
                Value::Null => Vec::new(),
                Value::Array(items) => items
                    .iter()
                    .map(|o| o.as_str().map(label))
                    .collect::<Option<_>>()
                    .ok_or("\"options\" is a list of strings")?,
                _ => return Err("\"options\" is a list of strings".into()),
            };
            let question = Question {
                text: text.to_string(),
                header: None,
                options,
                multi: false,
            };
            Ok(Ask {
                questions: vec![question],
                json: false,
            })
        }
        _ => Err("Give question or questions, not both.".into()),
    }
}

fn questions_of(value: &Value) -> Result<Ask, String> {
    let items = value
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or("\"questions\" is a list of one or more questions")?;
    let mut questions: Vec<Question> = Vec::new();
    for (q, item) in items.iter().enumerate() {
        let place = format!("questions[{q}]");
        let text = item["question"]
            .as_str()
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| format!("{place} needs \"question\", a text that isn't blank"))?;
        if let Some(before) = questions.iter().position(|o| o.text == text) {
            return Err(format!(
                "questions[{before}] and {place} have the same text: the answers are keyed by it."
            ));
        }
        let options = match &item["options"] {
            Value::Null => Vec::new(),
            Value::Array(options) => {
                let mut choices: Vec<Choice> = Vec::new();
                for (i, option) in options.iter().enumerate() {
                    let at = format!("{place}.options[{i}]");
                    let choice = choice(option, &at)?;
                    if let Some(before) = choices
                        .iter()
                        .position(|c| c.label.trim() == choice.label.trim())
                    {
                        return Err(format!(
                            "{at} has the same label as {place}.options[{before}]."
                        ));
                    }
                    choices.push(choice);
                }
                choices
            }
            _ => return Err(format!("{place}.options is a list of options")),
        };
        let multi = match &item["multiSelect"] {
            Value::Null => false,
            value => value
                .as_bool()
                .ok_or_else(|| format!("{place}.multiSelect is true or false"))?,
        };
        questions.push(Question {
            text: text.to_string(),
            header: text_at(&item["header"], &format!("{place}.header"))?,
            options,
            multi,
        });
    }
    Ok(Ask {
        questions,
        json: true,
    })
}

/// An option: a string, its label, or `{ "label", "description", "preview" }`.
fn choice(option: &Value, at: &str) -> Result<Choice, String> {
    if let Some(text) = option.as_str() {
        return (!text.trim().is_empty())
            .then(|| label(text))
            .ok_or_else(|| format!("{at} needs a label"));
    }
    let text = option["label"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| format!("{at} needs a label"))?;
    Ok(Choice {
        label: text.to_string(),
        description: text_at(&option["description"], &format!("{at}.description"))?,
        preview: text_at(&option["preview"], &format!("{at}.preview"))?,
    })
}

fn label(text: &str) -> Choice {
    Choice {
        label: text.to_string(),
        description: None,
        preview: None,
    }
}

/// An optional text: a blank one is none.
fn text_at(value: &Value, at: &str) -> Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone()).filter(|s| !s.trim().is_empty())),
        _ => Err(format!("{at} is a text")),
    }
}

/// `text`, at most `max` characters: a longer one is cut, "…" last.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

impl Ask {
    /// The first question's first line: the dot's tooltip, the notification's text.
    pub fn summary(&self) -> String {
        let text = self.questions.first().map_or("", |q| q.text.as_str());
        text.lines().next().unwrap_or_default().to_string()
    }

    /// One single-select question with options: a click on one answers at once.
    pub fn quick(&self) -> bool {
        matches!(&self.questions[..], [q] if !q.multi && !q.options.is_empty())
    }

    /// The result's text: `answers[q]` holds the labels picked in question `q`, in option order,
    /// then Other's text. JSON for `questions`, with `note` when written; else the answer alone.
    pub fn answer(&self, answers: &[Vec<String>], note: &str) -> String {
        if !self.json {
            return answers[0].concat();
        }
        let answered: Map<String, Value> = self
            .questions
            .iter()
            .zip(answers)
            .map(|(q, picked)| {
                let value = if q.multi {
                    json!(picked)
                } else {
                    json!(picked.concat())
                };
                (q.text.clone(), value)
            })
            .collect();
        let mut answer = json!({ "answers": answered });
        if !note.trim().is_empty() {
            answer["note"] = json!(note.trim());
        }
        answer.to_string()
    }

    /// The line typed into the agent's terminal when its client stopped waiting for `answer`.
    pub fn notice(&self, answer: &str) -> String {
        format!(
            "[answer to your question \"{}\"] {answer}",
            cut(&self.summary(), NOTICE_MAX)
        )
    }
}

impl Question {
    /// The header as its chip shows it: HEADER_MAX characters at most.
    pub fn header_shown(&self) -> Option<String> {
        self.header.as_deref().map(|h| cut(h, HEADER_MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spec's example call.
    fn example() -> Value {
        json!({
            "questions": [
                {
                    "question": "Which database should the service use?",
                    "header": "Database",
                    "options": [
                        { "label": "Postgres", "description": "What production runs",
                          "preview": "DATABASE_URL=postgres://…\npool = 10" },
                        { "label": "SQLite", "description": "One file, no server" }
                    ],
                    "multiSelect": false
                },
                {
                    "question": "Which features go in the first release?",
                    "header": "Features",
                    "options": [{ "label": "Auth" }, { "label": "Search" }, { "label": "Export" }],
                    "multiSelect": true
                }
            ],
            "timeout": 600
        })
    }

    #[test]
    fn both_shapes_parse() {
        let today = parse(&json!({ "question": "Ship it?", "options": ["Yes", "No"] })).unwrap();
        assert!(!today.json);
        assert_eq!(today.questions.len(), 1);
        let q = &today.questions[0];
        assert_eq!(q.text, "Ship it?");
        assert!(!q.multi);
        assert_eq!(q.header, None);
        assert_eq!(
            q.options,
            ["Yes", "No"].map(|l| Choice {
                label: l.into(),
                description: None,
                preview: None
            })
        );
        let ask = parse(&example()).unwrap();
        assert!(ask.json);
        assert_eq!(ask.questions.len(), 2);
        assert!(!ask.questions[0].multi && ask.questions[1].multi);
        assert_eq!(ask.questions[0].header.as_deref(), Some("Database"));
        let first = &ask.questions[0].options[0];
        assert_eq!(first.description.as_deref(), Some("What production runs"));
        assert_eq!(
            first.preview.as_deref(),
            Some("DATABASE_URL=postgres://…\npool = 10")
        );
        assert_eq!(ask.questions[0].options[1].preview, None);
        assert_eq!(ask.questions[1].options.len(), 3);
    }

    #[test]
    fn options_may_be_strings_or_objects() {
        let ask = parse(&json!({ "questions": [
            { "question": "Which?", "options": ["A", { "label": "B", "description": " " }] }
        ] }))
        .unwrap();
        let options = &ask.questions[0].options;
        assert_eq!(options[0].label, "A");
        assert_eq!(options[1].label, "B");
        assert_eq!(options[1].description, None, "a blank description is none");
    }

    #[test]
    fn bad_questions_are_refused_by_place() {
        let q = |text: &str| json!({ "question": text, "options": ["A"] });
        let cases = [
            (json!({}), "question"),
            (
                json!({ "question": "Why?", "questions": [q("Why?")] }),
                "Give question or questions, not both.",
            ),
            (json!({ "questions": [] }), "questions"),
            (json!({ "questions": [q("Why?"), q("  ")] }), "questions[1]"),
            (
                json!({ "questions": [{ "question": "Why?", "options": [{ "description": "no label" }] }] }),
                "questions[0].options[0] needs a label",
            ),
            (
                json!({ "questions": [q("Why?"), q("Why?")] }),
                "questions[0] and questions[1] have the same text",
            ),
            (
                json!({ "questions": [{ "question": "Why?", "options": ["A", { "label": "A" }] }] }),
                "the same label",
            ),
            (
                json!({ "questions": [{ "question": "Why?", "multiSelect": "yes" }] }),
                "multiSelect",
            ),
        ];
        for (args, says) in cases {
            let err = parse(&args).unwrap_err();
            assert!(err.contains(says), "{args}: {err}");
        }
    }

    #[test]
    fn a_long_header_is_cut() {
        let header = |h: Option<&str>| Question {
            text: "Why?".into(),
            header: h.map(str::to_string),
            options: Vec::new(),
            multi: false,
        };
        let cut = header(Some("Infrastructure")).header_shown().unwrap();
        assert_eq!(cut, "Infrastruct…");
        assert_eq!(cut.chars().count(), HEADER_MAX);
        assert_eq!(
            header(Some("Database")).header_shown().as_deref(),
            Some("Database")
        );
        assert_eq!(header(None).header_shown(), None);
    }

    #[test]
    fn the_answer_is_json_for_questions() {
        let ask = parse(&example()).unwrap();
        let answers = vec![
            vec!["SQLite".to_string()],
            vec!["Auth".into(), "Export".into()],
        ];
        let answer: Value = serde_json::from_str(&ask.answer(&answers, " keep it small ")).unwrap();
        assert_eq!(
            answer,
            json!({
                "answers": {
                    "Which database should the service use?": "SQLite",
                    "Which features go in the first release?": ["Auth", "Export"],
                },
                "note": "keep it small",
            })
        );
        let one = vec![vec!["Postgres".to_string()], vec!["Search".to_string()]];
        let answer: Value = serde_json::from_str(&ask.answer(&one, "  ")).unwrap();
        assert_eq!(
            answer,
            json!({ "answers": {
                "Which database should the service use?": "Postgres",
                "Which features go in the first release?": ["Search"],
            } }),
            "no note when blank; a multiSelect with one pick is still a list"
        );
    }

    #[test]
    fn the_answer_is_text_for_question() {
        let ask = parse(&json!({ "question": "Ship it?", "options": ["Yes", "No"] })).unwrap();
        assert_eq!(ask.answer(&[vec!["Yes".into()]], ""), "Yes");
        assert_eq!(ask.answer(&[vec!["Yes".into()]], "a note"), "Yes");
        let when = parse(&json!({ "question": "When?" })).unwrap();
        assert_eq!(when.answer(&[vec!["Friday".into()]], ""), "Friday");
    }

    #[test]
    fn the_notice_names_the_question() {
        let ask = parse(&example()).unwrap();
        assert_eq!(ask.summary(), "Which database should the service use?");
        let notice = ask.notice("SQLite");
        assert!(
            notice.starts_with("[answer to your question \"Which database"),
            "{notice}"
        );
        assert!(notice.ends_with("\"] SQLite"), "{notice}");
        let long = "x".repeat(80);
        let ask = parse(&json!({ "question": format!("{long}\nsecond line") })).unwrap();
        assert_eq!(ask.summary(), long);
        assert_eq!(
            ask.notice("Yes"),
            format!("[answer to your question \"{}…\"] Yes", "x".repeat(59))
        );
    }
}
