//! AC-7 dla T-05: wartości w wierszu pochodzą ze strumienia, a klucze na drucie są camelCase.
//!
//! **Słaba wersja tego kryterium to `assert!(line.text.contains("limit"))`.** Przechodzi ją
//! wiersz, który zmyśla godzinę resetu albo zaokrągla koszt do `0.15` i traci go bezpowrotnie —
//! `Line` jest jedyną rzeczą, którą dostaje widok, więc czego tu nie ma, tego nie ma nigdzie.
//!
//! Rozróżniają je trzy asercje: równość `resetsAt` i kosztu z wartościami z drutu **co do
//! bitu**, oraz skan **wszystkich** kluczy przeprowadzony na `serde_json::Value`, a nie na
//! ręcznie wypisanej liście pól. Lekcja z meetnotes: brakujący `rename_all_fields` wysłał
//! `started_at` do frontu i położył cały widok, a sześć poprawek poszło najpierw w złą warstwę
//! [00-SYNTHESIS §3]. Ręczna lista pól nie zauważyłaby ani jednej z nich, bo to zawsze jest
//! pole, o którym się zapomniało.

use std::path::Path;

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen};
use loadout_lib::engine::stream;
use serde_json::Value;
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Złoty plik: 16 zdarzeń prawdziwego biegu, zakończony linią `result`.
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Agent, którego strumień to jest.
const AGENT: &str = "builder";

/// Ile tur ogłosiła linia `result`.
const TURNS: u32 = 2;

/// Ile milisekund trwała tura, według vendora.
const DURATION_MS: u64 = 6_220;

/// Zdanie, które czyta człowiek na końcu tury. Liczby w nim są zaokrąglone **do wyświetlenia**,
/// a pola obok niosą wartości surowe — na tym polega różnica między formatowaniem a utratą.
const DONE_TEXT: &str = "Done · 2 turns · 6.2s · $0.15";

/// Kiedy wraca limit u dostawcy, w sekundach epoki uniksowej. Ta liczba jedzie z drutu i nie
/// wolno jej policzyć samemu: godzinę lokalną renderuje widok [T7 §7.2].
const RESETS_AT: i64 = 1_786_800_600;

/// Osiem rodzajów, które widać bez klikania.
fn structure_samples() -> Vec<Line> {
    vec![
        Line::Run {
            agent: AGENT.to_owned(),
            text: "Fix the login bug · Research → Plan → Build".to_owned(),
        },
        Line::Step {
            agent: AGENT.to_owned(),
            text: "Planning".to_owned(),
        },
        Line::Agent {
            agent: AGENT.to_owned(),
            text: "Researcher 2 joined".to_owned(),
        },
        Line::Note {
            agent: AGENT.to_owned(),
            text: "Greeting message stored in file.".to_owned(),
        },
        Line::Asked {
            agent: AGENT.to_owned(),
            text: "Needs your answer: which database?".to_owned(),
            options: vec!["Postgres".to_owned(), "SQLite".to_owned()],
        },
        Line::Handoff {
            agent: AGENT.to_owned(),
            text: "Planner → Implementer".to_owned(),
        },
        Line::Problem {
            agent: AGENT.to_owned(),
            text: "Claude is busy until the limit resets".to_owned(),
            resets_at: Some(RESETS_AT),
        },
        Line::Done {
            agent: AGENT.to_owned(),
            text: DONE_TEXT.to_owned(),
            turns: TURNS,
            duration_ms: DURATION_MS,
            cost_usd: Some(0.148_362_900_000_000_02),
        },
    ]
}

/// Sześć rodzajów, które są mechaniką — w tym `thinking`, który nigdy nie wchodzi do historii,
/// ale ma wariant serde tak samo jak reszta.
fn mechanic_samples() -> Vec<Line> {
    vec![
        Line::Thinking {
            agent: AGENT.to_owned(),
        },
        Line::Read {
            agent: AGENT.to_owned(),
            text: "Read 3 files".to_owned(),
            count: 3,
            paths: vec!["/w/a.rs".to_owned(), "/w/b.rs".to_owned()],
            detail_id: Some(1),
        },
        Line::Search {
            agent: AGENT.to_owned(),
            text: "Searched for \"auth token\" — 12 matches".to_owned(),
            count: 12,
            paths: vec!["/w/a.rs".to_owned()],
            detail_id: Some(2),
        },
        Line::Edit {
            agent: AGENT.to_owned(),
            text: "Edited /w/a.rs  +12 −4".to_owned(),
            count: 1,
            paths: vec!["/w/a.rs".to_owned()],
            added: 12,
            removed: 4,
            detail_id: Some(3),
        },
        Line::Ran {
            agent: AGENT.to_owned(),
            text: "Ran the tests — ok".to_owned(),
            ok: true,
            preview: "line 01 of the build output".to_owned(),
            detail: Vec::new(),
            detail_id: Some(4),
        },
        Line::Memory {
            agent: AGENT.to_owned(),
            text: "Saved a note — api-conventions.md".to_owned(),
            path: "api-conventions.md".to_owned(),
        },
    ]
}

/// Wszystkie czternaście wariantów.
fn samples() -> Vec<Line> {
    let mut all = structure_samples();
    all.extend(mechanic_samples());
    all
}

/// Zbiera nazwy kluczy z każdego poziomu wartości.
fn every_key(value: &Value, keys: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                keys.push(key.clone());
                every_key(child, keys);
            }
        }
        Value::Array(items) => {
            for item in items {
                every_key(item, keys);
            }
        }
        _ => {}
    }
}

/// Koszt tak, jak stoi na drucie — czytany z fikstury, nie przepisany do testu. Liczba
/// przepisana ręcznie zgadzałaby się z implementacją, która ją zaokrągla, dokładnie tak długo,
/// jak długo ktoś nie przepisałby jej z tym samym zaokrągleniem.
fn wire_cost() -> anyhow::Result<f64> {
    let last = FIXTURE
        .lines()
        .last()
        .ok_or_else(|| anyhow::anyhow!("the fixture holds no lines at all"))?;
    let value: Value = serde_json::from_str(last)?;
    value
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow::anyhow!("the last line of the fixture carries no total_cost_usd"))
}

/// Puszcza całą fiksturę przez pompę i oddaje historię.
async fn history(dir: &Path) -> anyhow::Result<Vec<Line>> {
    let source = dir.join("stdout.jsonl");
    tokio::fs::write(&source, FIXTURE.as_bytes()).await?;
    let reader = BufReader::new(tokio::fs::File::open(&source).await?);

    let (tx, mut rx) = mpsc::channel(256);
    stream::pump(reader, &dir.join("agent-1.jsonl"), AGENT, tx).await?;

    let mut lines = Vec::new();
    while let Some(line) = rx.recv().await {
        lines.push(line);
    }
    Ok(lines)
}

/// Wiersze, które kurator zrobił z jednego zdarzenia limitu.
fn from_rate_limit(status: &str, pause_run: bool) -> Vec<Line> {
    let event = AgentEvent::RateLimit {
        status: status.to_owned(),
        resets_at: RESETS_AT,
        rate_limit_type: "five_hour".to_owned(),
        pause_run,
    };
    let mut curator = Curator::new();
    let mut lines = curator.observe(Seen {
        agent: AGENT,
        at_ms: 0,
        event: &event,
        tool: None,
    });
    lines.extend(curator.flush());
    lines
}

#[test]
fn no_key_on_any_level_of_any_variant_carries_an_underscore() -> anyhow::Result<()> {
    for line in samples() {
        let value = serde_json::to_value(&line)?;
        let mut keys = Vec::new();
        every_key(&value, &mut keys);

        assert!(
            !keys.is_empty(),
            "{:?} serialised to something with no keys at all, so this scan would pass for free",
            line.kind()
        );
        let offenders: Vec<&String> = keys.iter().filter(|key| key.contains('_')).collect();
        assert!(
            offenders.is_empty(),
            "{:?} sends {offenders:?} to the front end. A snake_case key is the meetnotes bug \
             exactly: started_at reached the front, the whole view fell over, and six fixes \
             went into the wrong layer first. The scan runs on the serialised value rather \
             than on a hand-written list of fields, because the field somebody forgets is \
             always the one that breaks it",
            line.kind()
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_closing_row_copies_the_numbers_the_result_line_reported() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let lines = history(dir.path()).await?;
    let done = lines
        .iter()
        .find(|line| line.kind() == LineKind::Done)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the fixture ends with a result line, so the history has to hold a closing row; \
                 it held {lines:?}"
            )
        })?;

    match done {
        Line::Done {
            turns,
            duration_ms,
            cost_usd,
            text,
            ..
        } => {
            assert_eq!(*turns, TURNS, "the turn count is copied, not counted here");
            assert_eq!(
                *duration_ms, DURATION_MS,
                "the duration is the vendor's duration_ms, copied. Timing it ourselves would \
                 measure our reader"
            );
            let wire = wire_cost()?;
            assert_eq!(
                cost_usd.map(f64::to_bits),
                Some(wire.to_bits()),
                "the cost is copied to the bit. This row is the only thing the view ever \
                 receives, so a cost rounded here is a cost gone for good — and the run \
                 total is then wrong by a little, for ever, in a way nobody can audit"
            );
            assert_eq!(
                text, DONE_TEXT,
                "the sentence a person reads rounds the numbers for the eye while the fields \
                 beside it keep them whole. That is the difference between formatting and \
                 losing"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "the closing row of a run is a done row; this one was {other:?}"
            ));
        }
    }

    Ok(())
}

#[test]
fn a_limit_that_stopped_nothing_is_not_a_row() {
    let lines = from_rate_limit("allowed", false);

    assert!(
        lines.is_empty(),
        "an allowed limit is the vendor saying everything is fine. A row for it is a banner \
         that cries wolf on every single run, and after the second one nobody reads any of \
         them. It produced {lines:?}"
    );
}

#[test]
fn a_limit_that_did_stop_something_is_one_row_carrying_the_reset_time() -> anyhow::Result<()> {
    let lines = from_rate_limit("rejected", true);

    assert_eq!(
        lines.len(),
        1,
        "a limit that rejected work is one row, and the decision that it exists at all is made \
         here — the view only renders the local time. It produced {lines:?}"
    );
    assert_eq!(
        lines[0].kind(),
        LineKind::Problem,
        "a run that cannot dispatch has a problem, and the row says so in the error colour"
    );
    match &lines[0] {
        Line::Problem { resets_at, .. } => {
            assert_eq!(
                *resets_at,
                Some(RESETS_AT),
                "the reset time is copied from the wire. A row that invents it tells the user \
                 to come back at an hour nothing will happen, and the automatic resume waits \
                 for the wrong moment"
            );
        }
        other => {
            return Err(anyhow::anyhow!(
                "the row a rejected limit produces is a problem row; this one was {other:?}"
            ));
        }
    }

    Ok(())
}
