//! Cztery czytające czasowniki Context, związane z jednym odbiorcą przez hosta.
//!
//! Tabela i rozdzielnik stoją obok siebie celowo: jeśli [`context_tools`] wymienia nazwę,
//! [`ContextDesk::answer`] ma ją obsłużyć przez TEN SAM [`ContextAccess`]. Pole odbiorcy,
//! katalog i przydział nie występują w schematach wejścia, więc model nie ma czym wskazać
//! innego kroku ani biblioteki.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Answer, Call, host::Answers, verbs::Verb};
use crate::context::access::{ContextAccess, ImageVariant};

/// Cztery nazwy należące do jednego ograniczonego czytelnika.
const NAMES: [&str; 4] = [
    "list_context",
    "search_context",
    "read_context",
    "view_context_image",
];

/// Deklaracje czasowników. Każdy jest naprawdę tylko do odczytu.
#[must_use]
pub fn context_tools() -> Vec<Verb> {
    vec![
        Verb {
            name: NAMES[0],
            describe: "List the context items given to this agent. The returned IDs already include any page selection; no folder or other recipient can be chosen.",
            schema: json!({"type":"object","properties":{},"additionalProperties":false}),
            read_only: true,
        },
        Verb {
            name: NAMES[1],
            describe: "Search only the text given to this agent. Results are ordered and bounded; pass the returned cursor unchanged for the next page.",
            schema: json!({"type":"object","properties":{"query":{"type":"string","minLength":1},"cursor":{"type":"string"}},"required":["query"],"additionalProperties":false}),
            read_only: true,
        },
        Verb {
            name: NAMES[2],
            describe: "Read at most 16 KiB from one listed context item. Pass the returned cursor unchanged to continue without losing or repeating text.",
            schema: json!({"type":"object","properties":{"id":{"type":"string"},"cursor":{"type":"string"}},"required":["id"],"additionalProperties":false}),
            read_only: true,
        },
        Verb {
            name: NAMES[3],
            describe: "View one picture from a listed context item. Each call returns one image of at most 5 MiB.",
            schema: json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}),
            read_only: true,
        },
    ]
}

/// Czy nazwa należy do tej tabeli. [`super::messages::StepDesk`] używa jej przed delegacją.
#[must_use]
pub fn is_context_tool(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Odpowiedzi jednego odbiorcy na jego przydzielonym czytelniku.
#[derive(Debug)]
pub struct ContextDesk {
    access: ContextAccess,
}

impl ContextDesk {
    #[must_use]
    pub fn new(access: ContextAccess) -> Self {
        Self { access }
    }

    /// Lista dokładnie tego biurka, zamrożona z tym samym przydziałem co rozdzielnik.
    #[must_use]
    pub fn tools(&self) -> Value {
        Value::Array(context_tools().iter().map(Verb::listed).collect())
    }

    fn dispatch(&self, call: &Call) -> Result<Answer, String> {
        match call.call.as_str() {
            "list_context" => {
                let _: Empty = serde_json::from_value(call.input.clone()).map_err(|_error| {
                    "List only the context given to this agent; do not supply another identity."
                        .to_owned()
                })?;
                value(self.access.list())
            }
            "search_context" => {
                let input: Search =
                    serde_json::from_value(call.input.clone()).map_err(|_error| {
                        "Give a search query and, for later pages, its exact reading cursor."
                            .to_owned()
                    })?;
                value(self.access.search(&input.query, input.cursor.as_deref()))
            }
            "read_context" => {
                let input: Read = serde_json::from_value(call.input.clone()).map_err(|_error| {
                    "Choose one listed context item and pass only its exact reading cursor."
                        .to_owned()
                })?;
                value(self.access.read(&input.id, input.cursor.as_deref()))
            }
            "view_context_image" => {
                let input: View = serde_json::from_value(call.input.clone()).map_err(|_error| {
                    "Choose one listed context item that says it has a picture.".to_owned()
                })?;
                let image = self
                    .access
                    .image(&input.id, ImageVariant::Agent)
                    .map_err(|error| error.to_string())?;
                Ok(Answer::Image {
                    data: image.data,
                    mime: image.mime,
                })
            }
            _ => Err("This context action is not available to this agent.".to_owned()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    query: String,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    id: String,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct View {
    id: String,
}

fn value<T>(result: Result<T, impl std::fmt::Display>) -> Result<Answer, String>
where
    T: serde::Serialize,
{
    let value = result.map_err(|error| error.to_string())?;
    serde_json::to_value(value)
        .map(Answer::Ok)
        .map_err(|_error| "Loadout could not shape this context answer safely.".to_owned())
}

#[async_trait]
impl Answers for ContextDesk {
    async fn answer(&self, call: Call) -> Answer {
        self.dispatch(&call).unwrap_or_else(Answer::Refused)
    }
}
