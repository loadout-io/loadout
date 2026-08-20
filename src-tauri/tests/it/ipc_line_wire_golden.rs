//! AC-6 dla T-07: pompa wysyła dokładnie to, co stoi w złotym pliku.
//!
//! `src/ipc/line-wire.golden.json` to jeden plik i dwie strony granicy: ten test trzyma stronę
//! rustową, `src/ipc/types.test.ts` trzyma stronę frontu. Dryf jednej z nich jest
//! czerwony u niej.
//!
//! # Dlaczego przedmiotem asercji jest to, co oddała POMPA
//!
//! `Line` mieszka w `engine/line.rs`, który należy do T-05. Test serializujący `Line` wprost
//! albo przechodziłby od pierwszej minuty (bo T-05 nadał serde poprawnie), albo padałby za
//! cudzy błąd, którego to zadanie nie ma prawa naprawić — w obu przypadkach kryterium nie
//! mierzyłoby **niczego, co tu powstaje**, a warstwa `before` nie miałaby jak być czerwona
//! z właściwego powodu (`AGENTS.md` §2a p. 5). Ścieżka wysyłki z `ipc.rs` jest tym, co to
//! zadanie buduje — i to ona stoi pod asercją.
//!
//! **Słaba wersja tego kryterium: porównać kilka wybranych pól albo
//! `assert!(json.to_string().contains("agentId"))`.** Przechodzi, kiedy trzynasty wariant
//! dostał `snake_case`, bo nikt go nie wpisał do listy. Druga, groźniejsza: **ominięcie pompy**
//! i serializacja `Line` wprost — przechodzi, zanim ktokolwiek napisze `spawn_pump`.
//!
//! Rozróżniają je trzy rzeczy: **pętla po 14 wariantach** wygenerowana z jednego wektora
//! wartości (a nie z ręcznej listy nazw pól), wartości przepuszczone **przez kanał**, oraz
//! rekurencyjny skan kluczy. Wariant dodany jutro albo trafia do złotego pliku, albo jest
//! czerwony: [`sample`] jest wyczerpującym `match`em po [`LineKind`], więc piętnasty rodzaj
//! nie skompiluje się bez wpisu tutaj, a wpis bez wiersza w złotym pliku przewraca długość.
//!
//! Skan kluczy jest osobną asercją, bo jego brak ma zapisaną cenę: w meetnotes brakujący
//! `rename_all_fields` posłał `started_at` do frontu i położył cały widok, a sześć poprawek
//! poszło najpierw w złą warstwę — objaw był w widoku, przyczyna w `derive`
//! [00-SYNTHESIS §3].

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use loadout_lib::engine::line::{Line, LineKind};
use loadout_lib::ipc::{Sent, line_channel, spawn_pump};
use serde_json::Value;
use tauri::ipc::{Channel, InvokeResponseBody};

/// Wszystkie czternaście rodzajów wiersza [T2 §7.2], w kolejności deklaracji.
const KINDS: [LineKind; 17] = [
    LineKind::Run,
    LineKind::Step,
    LineKind::Agent,
    LineKind::Thinking,
    // 2026-08-18 — pietnasty rodzaj. Stan kroku nie wchodzi do historii (trasa `now`, tak jak
    // `Thinking`), ale JEST na drucie, wiec ma tu stac: wariant bez wiersza w tej tablicy jest
    // wariantem, ktorego nikt nigdy nie zobaczyl na drucie.
    LineKind::StepState,
    LineKind::Read,
    LineKind::Search,
    LineKind::Edit,
    LineKind::Ran,
    LineKind::Note,
    // 2026-08-19 — szesnasty rodzaj: tura CZLOWIEKA. Powod w calosci przy `Line::Told` —
    // do tego dnia zdanie wpisane w wiersz wejscia nie mialo nosnika na drucie i znikalo.
    LineKind::Told,
    // 2026-08-20 — siedemnasty rodzaj: lider proponuje bieg (T-61). Wpis wchodzi razem
    // z wierszem w zlotym pliku i z lustrem po stronie okna, bo dopisany osobno albo przewraca
    // dlugosc, albo opisuje rodzaj, ktorego okno nie przyjmie.
    LineKind::Suggested,
    LineKind::Asked,
    LineKind::Handoff,
    LineKind::Memory,
    LineKind::Problem,
    LineKind::Done,
];

/// Złoty plik. Ten sam, który czyta lustro po stronie frontu.
const GOLDEN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../src/ipc/line-wire.golden.json"
));

/// Jeden przykład danego rodzaju wiersza.
///
/// **Wyczerpujący `match` bez gałęzi domyślnej i to jest jego jedyne zadanie**: piętnasty
/// wariant `Line` nie skompiluje tego pliku, dopóki ktoś nie powie, jak wygląda na drucie.
/// Lista nazw pól pisana ręcznie nie ma tej własności — milczy dokładnie o tym wariancie,
/// o którym autor zapomniał.
fn sample(kind: LineKind) -> Line {
    match kind {
        LineKind::Run => Line::Run {
            agent: "lead".to_owned(),
            text: "Fix the login bug · Research → Plan → Build".to_owned(),
        },
        LineKind::Step => Line::Step {
            agent: "lead".to_owned(),
            text: "── Planning".to_owned(),
        },
        LineKind::Agent => Line::Agent {
            agent: "researcher-2".to_owned(),
            text: "Researcher 2 joined".to_owned(),
        },
        LineKind::Thinking => Line::Thinking {
            agent: "builder".to_owned(),
        },
        LineKind::StepState => Line::StepState {
            agent: "builder".to_owned(),
            step_id: "s_2".to_owned(),
            state: "running".to_owned(),
        },
        LineKind::Read => Line::Read {
            agent: "builder".to_owned(),
            text: "Read 3 files".to_owned(),
            count: 3,
            paths: vec![
                "src/auth.rs".to_owned(),
                "src/login.rs".to_owned(),
                "src/routes.rs".to_owned(),
            ],
            detail_id: None,
        },
        LineKind::Search => Line::Search {
            agent: "builder".to_owned(),
            text: "Searched for \"login\" — 12 matches".to_owned(),
            count: 12,
            paths: Vec::new(),
            detail_id: Some(4),
        },
        LineKind::Edit => Line::Edit {
            agent: "builder".to_owned(),
            text: "Edited src/auth.rs".to_owned(),
            count: 1,
            paths: vec!["src/auth.rs".to_owned()],
            added: 12,
            removed: 4,
            detail_id: Some(5),
        },
        LineKind::Ran => Line::Ran {
            agent: "builder".to_owned(),
            text: "Ran npm test — didn't work".to_owned(),
            ok: false,
            preview: "FAIL src/auth.test.ts".to_owned(),
            detail: vec![
                "FAIL src/auth.test.ts".to_owned(),
                "1 failed, 41 passed".to_owned(),
            ],
            detail_id: Some(6),
        },
        LineKind::Note => Line::Note {
            agent: "researcher-2".to_owned(),
            text: "The bug is in the cookie name, not in the check.".to_owned(),
        },
        LineKind::Told => Line::Told {
            agent: "builder".to_owned(),
            text: "also add a dark mode toggle".to_owned(),
        },
        // 2026-08-20 — SIEDEMNASTY RODZAJ: lider proponuje bieg (T-61). Próbka stoi tu, bo
        // `sample` jest wyczerpującym `match`em — bez niej ten plik przestaje się kompilować,
        // czyli KAŻDE kryterium rustowe pada na budowie i żadne z nich nic nie mierzy.
        //
        // Próbka weszła w fazie kontraktu SAMA, bez wpisu w `KINDS` i bez wiersza w złotym
        // pliku, i to nie było przeoczenie: te dwie rzeczy razem z lustrem po stronie okna są
        // dokładnie tym, czego wymaga drugie kryterium tamtego zadania, więc dopisane wtedy
        // zazieleniłyby je, zanim cokolwiek powstało. Weszły razem w fazie implementacji —
        // wpis, wiersz i lustro — więc od tej chwili tablica opisuje siedemnaście rodzajów
        // i tyle samo wierszy widzi w pliku.
        LineKind::Suggested => Line::Suggested {
            agent: "lead".to_owned(),
            text: "/run easy Make the flaky login test pass — the cookie name is wrong in two \
                   places, so Easy will find it in one pass."
                .to_owned(),
            command: "/run easy Make the flaky login test pass".to_owned(),
        },
        LineKind::Asked => Line::Asked {
            agent: "lead".to_owned(),
            text: "Which database should this use?".to_owned(),
            options: vec!["SQLite".to_owned(), "Postgres".to_owned()],
        },
        LineKind::Handoff => Line::Handoff {
            agent: "lead".to_owned(),
            text: "Planner → Implementer".to_owned(),
        },
        LineKind::Memory => Line::Memory {
            agent: "lead".to_owned(),
            text: "Saved a note — api-conventions.md".to_owned(),
            path: "notes/api-conventions.md".to_owned(),
        },
        LineKind::Problem => Line::Problem {
            agent: "builder".to_owned(),
            text: "Hit the usage limit — waiting for it to reset".to_owned(),
            resets_at: Some(1_767_225_600),
        },
        LineKind::Done => Line::Done {
            agent: "builder".to_owned(),
            text: "Done · 2 turns · 6.2s · $0.15".to_owned(),
            turns: 2,
            duration_ms: 6_200,
            cost_usd: Some(0.15),
        },
    }
}

/// Paczki, które **naprawdę wyszły kanałem**.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    /// Kanał, który pompa dostanie zamiast okna.
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi.
            if let Ok(mut seen) = seen.lock() {
                seen.push(body);
            }
            Ok(())
        })
    }

    /// Wszystko, co wyszło, sklejone w jedną listę wierszy w kolejności wyjścia.
    fn wire(&self) -> Result<Vec<Value>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| anyhow!("the recorder was poisoned: {error}"))?;
        let mut out = Vec::new();
        for body in seen.iter().cloned() {
            out.extend(body.deserialize::<Vec<Value>>()?);
        }
        Ok(out)
    }
}

/// Wszystkie klucze wszystkich poziomów, także tych wewnątrz list.
fn keys(value: &Value, into: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                into.insert(key.clone());
                keys(nested, into);
            }
        }
        Value::Array(items) => {
            for item in items {
                keys(item, into);
            }
        }
        _ => {}
    }
}

#[tokio::test(start_paused = true)]
async fn the_pump_puts_on_the_wire_exactly_what_the_golden_file_says() -> Result<()> {
    let golden: Vec<Value> = serde_json::from_str(GOLDEN)?;
    assert_eq!(
        golden.len(),
        KINDS.len(),
        "the golden file holds one example of every line kind there is. A variant added \
         without a row here is a variant nobody ever looked at on the wire"
    );

    let delivered = Delivered::default();
    let (sink, source) = line_channel(64);
    let pump = spawn_pump(source, delivered.channel());

    let queued = KINDS
        .iter()
        .filter(|kind| sink.send(sample(**kind)) == Sent::Queued)
        .count();
    assert_eq!(queued, KINDS.len(), "every sample fits in the queue");

    drop(sink);
    let stats = pump.await?;
    let wire = delivered.wire()?;

    assert_eq!(
        wire.len(),
        KINDS.len(),
        "as many lines came out THROUGH THE CHANNEL as went in. Serialising a \
         `Line` by hand instead would pass before anybody writes `spawn_pump`, and would \
         measure T-05's derive rather than this task's send path"
    );
    assert_eq!(
        u64::try_from(wire.len())?,
        stats.delivered,
        "and the pump's balance agrees with what the channel carried"
    );

    for (index, (sent, want)) in wire.iter().zip(golden.iter()).enumerate() {
        assert_eq!(
            sent, want,
            "variant {index} left the pump in a shape the golden file does not describe. \
             The comparison is `Value` against `Value`, so formatting is free — names, \
             nesting and values are not"
        );
    }

    let kinds: BTreeSet<&str> = golden
        .iter()
        .filter_map(|entry| entry.get("kind").and_then(Value::as_str))
        .collect();
    assert_eq!(
        kinds.len(),
        KINDS.len(),
        "and the rows describe that many DIFFERENT kinds — two rows for one kind \
         would leave another kind untested while the length still looked right"
    );

    let mut seen = BTreeSet::new();
    for value in &wire {
        keys(value, &mut seen);
    }
    let snake: Vec<&String> = seen.iter().filter(|key| key.contains('_')).collect();
    assert!(
        snake.is_empty(),
        "every key on every level is camelCase; these are not: {snake:?}. A missing \
         `rename_all_fields` sent `started_at` to the front in meetnotes and took the whole \
         view down with it, and the first six fixes went into the wrong layer because the \
         symptom was in the view and the cause was in a derive [00-SYNTHESIS §3]"
    );
    Ok(())
}
