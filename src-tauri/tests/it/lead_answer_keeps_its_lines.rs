//! Odpowiedź lidera zachowuje wiersze, którymi ją napisał. Proza kroku biegu — nie.
//!
//! # Skarga
//!
//! Właściciel, 2026-08-23, o strumieniu: „ten tekst niech też będzie jakoś fajnie i ładnie
//! formatowany aby było to przyjemniejsze".
//!
//! # Dlaczego ta skarga NIE ZOSTAŁA wtedy załatwiona
//!
//! Poprawka weszła w CSS (`src/sections/run/feed/line.tsx`, `whitespace-pre-line`) i jest
//! poprawna. Spłaszczanie dzieje się jednak WARSTWĘ WCZEŚNIEJ, tutaj: `Curator::observe` woła
//! `one_line`, a ta skleja **każdy** biały znak w pojedynczą spację — więc do DOM-u nie
//! dojeżdżał ani jeden przełam, który CSS mógłby zachować.
//!
//! Kryterium frontowe (`an-answer-keeps-its-lines.test.tsx`) tego nie złapało, bo sądziło wiersz
//! rodzaju `step`, a taki pisze PLANISTA i nie przechodzi przez kuratora. Prawdziwa proza agenta
//! to rodzaj `note`. Zielone kryterium nad ścieżką, którą nikt nie chodzi — dokładnie klasa
//! z niezmiennika 29, tylko po stronie frontu.
//!
//! # Dlaczego DWA kryteria, a nie jedno
//!
//! Bo w jednym widoku stoją dwa różne produkty. Rozmowa jest DO CZYTANIA: akapit, lista i blok
//! kodu są w niej treścią. Strumień pracy jest DO PRZEGLĄDANIA: sześciu agentów piszących
//! akapitami zamienia go w ścianę, przed którą stoi cała reguła 1 [T2 §7.3]. Kryterium pytające
//! wyłącznie o pierwsze przepuściłoby zmianę psującą drugie.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Curator, Line, Seen};

/// Odpowiedź w trzech wierszach — dokładnie to, co modele piszą naprawdę.
const ANSWER: &str = "Three things stand out:\n- the parser\n- the writer";

/// Zdanie agenta przepuszczone przez podany kurator; `None`, gdy nie powstał wiersz prozy.
fn prose(curator: &mut Curator, agent: &str) -> Option<String> {
    let event = AgentEvent::Said {
        text: ANSWER.to_owned(),
    };
    let seen = Seen {
        agent,
        at_ms: 0,
        event: &event,
        tool: None,
    };
    curator
        .observe(seen)
        .into_iter()
        .find_map(|line| match line {
            Line::Note { text, .. } => Some(text),
            _ => None,
        })
}

#[test]
fn a_lead_answer_keeps_the_line_breaks_the_model_wrote() {
    let mut curator = Curator::talking();
    let said = prose(&mut curator, "Lead").expect("a Said event has to produce a prose row");

    assert_eq!(
        said, ANSWER,
        "the lead's answer has to reach the screen with the line breaks the model wrote. \
         Flattened, a list of three points reads as one wall of words, and the person asked for \
         the opposite on 2026-08-23"
    );
}

#[test]
fn a_run_step_still_says_it_in_one_line() {
    let mut curator = Curator::new();
    let said = prose(&mut curator, "Forge").expect("a Said event has to produce a prose row");

    assert!(
        !said.contains('\n'),
        "prose from a step inside a run stays on one line. Six agents writing paragraphs is the \
         wall of text the whole curated view exists to remove. Got: {said:?}"
    );
}
