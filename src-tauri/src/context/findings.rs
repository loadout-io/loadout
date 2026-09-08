//! Kontrakt odpowiedzi agenta i renderowanie zaakceptowanych ustaleń.
//!
//! Model oddaje wyłącznie DANE. Nie wybiera identyfikatora zestawu, wersji ani ścieżki. Nawet
//! identyfikator ustalenia wybija aplikacja z dokładnej treści i źródeł, więc ponowiona partia
//! nie potrafi podmienić cudzego wpisu samą nazwą.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::limits::SHORT_INDEX_BYTES;
use super::{ContextFinding, ContextTopic, FindingKind, Origin, SourceReference};

/// Odpowiedź większa od tej wartości nie jest listą ustaleń jednej ograniczonej partii.
const ANSWER_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AnsweredKind {
    Requirement,
    Fact,
    VisualReference,
    Assumption,
    Question,
    Conflict,
}

impl From<AnsweredKind> for FindingKind {
    fn from(kind: AnsweredKind) -> Self {
        match kind {
            AnsweredKind::Requirement => Self::Requirement,
            AnsweredKind::Fact => Self::Fact,
            AnsweredKind::VisualReference => Self::VisualReference,
            AnsweredKind::Assumption => Self::Assumption,
            AnsweredKind::Question => Self::Question,
            AnsweredKind::Conflict => Self::Conflict,
        }
    }
}

/// Temat dokładnie w kształcie, który wolno zwrócić agentowi.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnsweredTopic {
    id: String,
    title: String,
}

/// Ustalenie dokładnie w kształcie odpowiedzi agenta.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnsweredFinding {
    kind: AnsweredKind,
    text: String,
    condition: String,
    sources: Vec<SourceReference>,
    topic: String,
    conflicts_with: Vec<String>,
}

/// Jedyny format odpowiedzi agenta. `deny_unknown_fields` odmawia pól sterujących hostem.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Answered {
    topics: Vec<AnsweredTopic>,
    findings: Vec<AnsweredFinding>,
    questions: Vec<String>,
}

impl Answered {
    /// Przykład serializowany do promptu z tego samego typu, który czyta parser.
    #[must_use]
    pub fn example() -> Self {
        Self {
            topics: vec![AnsweredTopic {
                id: "checkout".to_owned(),
                title: "Checkout".to_owned(),
            }],
            findings: vec![AnsweredFinding {
                kind: AnsweredKind::Requirement,
                text: "Keep the exact total visible.".to_owned(),
                condition: "while the cart is being edited".to_owned(),
                sources: vec![SourceReference {
                    source_id: "source-id".to_owned(),
                    part: "page 2".to_owned(),
                }],
                topic: "checkout".to_owned(),
                conflicts_with: Vec::new(),
            }],
            questions: vec!["Which total wins when the two sources disagree?".to_owned()],
        }
    }
}

/// Zaakceptowana część odpowiedzi i jawna lista tego, czego nie dało się przyjąć.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Findings {
    pub topics: Vec<ContextTopic>,
    pub findings: Vec<ContextFinding>,
    pub questions: Vec<String>,
    pub missing: Vec<String>,
}

/// Dlaczego odpowiedź nie jest nawet częściowo czytelnym kontraktem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotFindings {
    TooLong,
    NotTheContract(String),
}

impl NotFindings {
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Self::TooLong => {
                "The answer was too large to be the findings for one batch.".to_owned()
            }
            Self::NotTheContract(why) => format!(
                "The answer was not the required findings JSON ({}).",
                one_line(why)
            ),
        }
    }
}

impl fmt::Display for NotFindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.said())
    }
}

impl std::error::Error for NotFindings {}

/// Czyta odpowiedź, zachowując poprawne wpisy i nazywając każdy odrzucony.
pub fn read_findings(
    answered: &[u8],
    allowed: &BTreeSet<SourceReference>,
) -> Result<Findings, NotFindings> {
    if answered.len() > ANSWER_BYTES {
        return Err(NotFindings::TooLong);
    }
    let answered: Answered = serde_json::from_slice(answered)
        .map_err(|error| NotFindings::NotTheContract(error.to_string()))?;

    let mut missing = Vec::new();
    let mut topics = Vec::new();
    let mut topic_ids = BTreeSet::new();
    for topic in answered.topics {
        let id = topic.id.trim();
        let title = topic.title.trim();
        if !safe_id(id) || title.is_empty() || !topic_ids.insert(id.to_owned()) {
            missing.push(format!(
                "A topic named {id:?} was empty, repeated, or unsafe."
            ));
            continue;
        }
        topics.push(ContextTopic {
            id: id.to_owned(),
            title: title.to_owned(),
        });
    }

    let mut findings = Vec::new();
    for (at, finding) in answered.findings.into_iter().enumerate() {
        let text = finding.text.trim();
        let topic = finding.topic.trim();
        let mut sources = finding.sources;
        sources.sort();
        sources.dedup();
        let sources_are_exact =
            !sources.is_empty() && sources.iter().all(|one| allowed.contains(one));
        if text.is_empty() || !topic_ids.contains(topic) || !sources_are_exact {
            missing.push(format!(
                "Finding {} needs text, a known topic, and exact references from this batch.",
                at + 1
            ));
            continue;
        }
        let kind = FindingKind::from(finding.kind);
        let condition = finding.condition.trim().to_owned();
        let id = finding_id(kind, text, &condition, &sources);
        findings.push(ContextFinding {
            id,
            kind,
            text: text.to_owned(),
            condition,
            sources,
            topic: topic.to_owned(),
            conflicts_with: finding
                .conflicts_with
                .into_iter()
                .filter_map(|one| nonempty(&one))
                .collect(),
            origin: Origin::Generated,
        });
    }

    let findings = deduplicated(findings);
    let questions = answered
        .questions
        .into_iter()
        .filter_map(|one| nonempty(&one))
        .collect();
    Ok(Findings {
        topics,
        findings,
        questions,
        missing,
    })
}

/// Łączy partie bez uśredniania warunków ani odwołań.
#[must_use]
pub fn merge(parts: Vec<Findings>) -> Findings {
    let mut topics = BTreeMap::<String, ContextTopic>::new();
    let mut findings = Vec::new();
    let mut questions = BTreeSet::new();
    let mut missing = Vec::new();
    for part in parts {
        for topic in part.topics {
            topics.entry(topic.id.clone()).or_insert(topic);
        }
        findings.extend(part.findings);
        questions.extend(part.questions);
        missing.extend(part.missing);
    }
    let findings = deduplicated(findings);
    Findings {
        topics: topics.into_values().collect(),
        findings,
        questions: questions.into_iter().collect(),
        missing,
    }
}

/// Krótki indeks. Ucięcie dotyka wyłącznie indeksu; pełne ustalenia są w tematach i JSON-ie.
#[must_use]
pub fn render_index(title: &str, findings: &Findings) -> String {
    let mut out = format!("# {}\n\n", title.trim());
    for topic in &findings.topics {
        let line = format!("- [{}](topics/{}.md)\n", topic.title, topic.id);
        if out.len().saturating_add(line.len()) > SHORT_INDEX_BYTES {
            break;
        }
        out.push_str(&line);
    }
    if !findings.questions.is_empty() {
        let heading = "\n## Questions\n\n";
        if out.len().saturating_add(heading.len()) <= SHORT_INDEX_BYTES {
            out.push_str(heading);
        }
        for question in &findings.questions {
            let line = format!("- {question}\n");
            if out.len().saturating_add(line.len()) > SHORT_INDEX_BYTES {
                break;
            }
            out.push_str(&line);
        }
    }
    out
}

/// Pełny temat, zawsze ze źródłami i warunkiem.
#[must_use]
pub fn render_topic(topic: &ContextTopic, findings: &[ContextFinding]) -> String {
    let mut out = format!("# {}\n", topic.title);
    for finding in findings.iter().filter(|one| one.topic == topic.id) {
        out.push_str("\n- ");
        out.push_str(&finding.text);
        if !finding.condition.is_empty() {
            out.push_str(" — ");
            out.push_str(&finding.condition);
        }
        if !finding.sources.is_empty() {
            out.push_str(" (sources: ");
            out.push_str(
                &finding
                    .sources
                    .iter()
                    .map(|one| format!("{} {}", one.source_id, one.part))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(')');
        }
        out.push('\n');
    }
    out
}

fn finding_id(
    kind: FindingKind,
    text: &str,
    condition: &str,
    sources: &[SourceReference],
) -> String {
    let identity = serde_json::to_vec(&(kind, text, condition, sources)).unwrap_or_default();
    format!("finding-{:x}", Sha256::digest(identity))
}

fn finding_identity(finding: &ContextFinding) -> Vec<u8> {
    serde_json::to_vec(&(
        finding.kind,
        finding.text.as_str(),
        finding.condition.as_str(),
        finding.sources.as_slice(),
    ))
    .unwrap_or_default()
}

fn deduplicated(findings: Vec<ContextFinding>) -> Vec<ContextFinding> {
    let mut positions = BTreeMap::<Vec<u8>, usize>::new();
    let mut out = Vec::<ContextFinding>::new();
    for mut finding in findings {
        finding.conflicts_with.sort();
        finding.conflicts_with.dedup();
        let identity = finding_identity(&finding);
        if let Some(at) = positions.get(&identity).copied() {
            // 2026-09-08 (CT-04) — dwa identyczne ustalenia są jednym wpisem, ale ich relacje
            // konfliktu są sumą informacji, nie własnością pierwszej odpowiedzi. Zwykłe
            // `retain` gubiło konflikt znaleziony przez późniejszą partię.
            out[at].conflicts_with.extend(finding.conflicts_with);
            out[at].conflicts_with.sort();
            out[at].conflicts_with.dedup();
        } else {
            positions.insert(identity, out.len());
            out.push(finding);
        }
    }
    out
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn nonempty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
