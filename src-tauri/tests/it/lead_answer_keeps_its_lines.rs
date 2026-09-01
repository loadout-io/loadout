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
//! # 2026-08-31 — DRUGA POŁOWA TEJ SAMEJ REGUŁY
//!
//! „Bieg skleja do jednej linii" było prawdą i było niewystarczające. Zrzut właściciela z biegu
//! `20260830-191440`: odpowiedź kroku na **78 wierszy** stała w jednym wierszu strumienia
//! i zasłaniała komplet dziewięciu kroków. Skarga: „nie podoba mi się ta ściana tekstu".
//!
//! Sklejenie nie było wadą — wadą było to, że proza nie miała DOKĄD pójść. Reguła 1 mówi „treść
//! siedzi ZA wierszem, nigdy w nim", a `Line::Note` nie miało pola na tę treść, więc całość szła
//! do `text`. Teraz ma (`body`), i te kryteria sądzą obie strony podziału.
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
    said(curator, agent, ANSWER).map(|(text, _)| text)
}

/// Wiersz prozy w całości — nagłówek i ciało — dla dowolnego tekstu.
fn said(curator: &mut Curator, agent: &str, text: &str) -> Option<(String, Vec<String>)> {
    let event = AgentEvent::Said {
        text: text.to_owned(),
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
            Line::Note { text, body, .. } => Some((text, body)),
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

/// Odpowiedź, która się w wierszu nie mieści — pierwszy wiersz jest podsumowaniem, tak jak piszą
/// modele naprawdę.
const LONG: &str = "Implementation is complete and all gates are green.\n\n## Answer\nTasks and \
                    Reminders are now global-rail destinations.\n\n## Evidence\n- \
                    src/app/app-shell.component.html:7\n- src/styles.css:456";

#[test]
fn prose_that_does_not_fit_hands_the_row_a_headline_and_keeps_the_rest_behind_it() {
    let mut curator = Curator::new();
    let (text, body) = said(&mut curator, "Frontend", LONG).expect("a Said event makes a row");

    assert_eq!(
        text, "Implementation is complete and all gates are green.",
        "the row gets the FIRST LINE, because that is where a model puts its summary. Cutting \
         after N characters would put \"…green. ## Answer Tasks and Re\" on screen — a summary \
         with the start of a heading glued to it"
    );
    assert!(
        body.len() > 1,
        "the rest has to go somewhere. Seventy-eight lines in one row is what the owner saw on \
         2026-08-30 and called a wall of text; a row with no body at all would mean the same \
         text is simply gone from the screen. Got: {body:?}"
    );
    assert_eq!(
        body.join("\n"),
        LONG,
        "the body carries the WHOLE answer, headline included. A body starting at the second \
         sentence reads like text with its opening cut off, and makes a person assemble one \
         answer from two places on the screen"
    );
}

#[test]
fn prose_that_fits_stays_in_the_row_and_gets_no_body() {
    let mut curator = Curator::new();
    let (text, body) = said(&mut curator, "Frontend", "All green.").expect("a row");

    assert_eq!(text, "All green.");
    assert!(
        body.is_empty(),
        "an expand control on a two-word note is a step to take for nothing. The body exists for \
         prose that does not fit, and for nothing else. Got: {body:?}"
    );
}

#[test]
fn a_lead_answer_never_gets_a_body_because_a_conversation_is_read_in_place() {
    let mut curator = Curator::talking();
    let (text, body) = said(&mut curator, "Lead", LONG).expect("a row");

    assert!(
        body.is_empty(),
        "the lead's turn is the thing the person came for. Putting it behind a click hides the \
         answer to their own question, and it is the one row in this view that must never need \
         one. Got: {body:?}"
    );
    assert!(
        text.contains('\n'),
        "and it keeps the shape the model wrote it in, which is the whole point of the talking \
         curator. Got: {text:?}"
    );
}
