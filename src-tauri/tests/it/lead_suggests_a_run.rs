//! Propozycja biegu powstaje z prozy LIDERA i tylko z niej — pierwsze kryterium T-61.
//!
//! # Po co to istnieje
//!
//! Rozstrzygnięcie właściciela 2026-08-20, wariant A: lider **podaje gotową komendę**, a nie
//! startuje bieg sam. Wartość jest konkretna — lider patrzy na projekt, więc umie powiedzieć
//! „to jest robota dla Easy, z takim zadaniem", a człowiek nie musi pamiętać nazw plików
//! workflow ani przepisywać zdania. Jedno kliknięcie zamiast przepisywania jest całą różnicą
//! między rozmową a formularzem.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(line.text().contains("/run"))`. Przechodzi dla implementacji, która zostawia prozę
//! wierszem `note` i nie rozpoznaje niczego — czyli dla stanu sprzed tego zadania, w którym
//! proza z komendą w środku jest nieodróżnialna od każdej innej. Rozstrzygają dwie rzeczy naraz:
//! przedmiotem asercji jest RODZAJ wiersza (a nie napis w jego treści), a fikstura z dwoma
//! zdaniami wymaga DWÓCH różnych rodzajów — inaczej to samo zielone dostałaby implementacja,
//! dla której propozycją jest wszystko.
//!
//! # Czego ten plik NIE umie sprawdzić, i mówię to wprost
//!
//! Że rozpoznania nie woła nikt poza rozmową. Sprawdzalna jest ta połowa, na której cała obrona
//! stoi: droga BIEGU to sam [`Curator`], więc gdyby rozpoznanie kiedykolwiek weszło do kuracji,
//! `prose_from_a_step_inside_a_run_is_never_a_proposal` robi się czerwone i zostaje takie. Druga
//! połowa — „`suggested` ma dokładnie jednego wołającego" — jest własnością wołających i widać
//! ją gerpem po `src-tauri/src`, nie z wnętrza testu. Cena tego braku jest zapisana w nagłówku
//! `Line::Suggested`: krok w środku biegu z prozą `/run …` dostałby przycisk startujący DRUGI
//! bieg, a silnik prowadzi dziś jeden.

use loadout_lib::commands::chat::LEAD;
use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen, suggested};

/// Krok w środku biegu. Nazwa kroku, nie lidera — na tym stoi przypadek (c).
const STEP: &str = "Build";

/// Proza lidera, która JEST propozycją: komenda w pierwszej linii, powód pod nią.
///
/// Dwie linie, bo tak pisze model i bo dokładnie tędy biegnie granica, której wiersz nie ma
/// prawa zgubić: pierwsza linia jest poleceniem, reszta jest tym, po co człowiek to czyta.
const PROPOSES: &str = "\
/run easy Make the flaky login test pass
The cookie name is wrong in two places, so Easy will find it in one pass.";

/// Komenda z tej prozy, znak w znak. Wartość oczekiwana, nie wyliczona z wyniku.
const COMMAND: &str = "/run easy Make the flaky login test pass";

/// Kawałek drugiej linii — po nim widać, że wiersz zachował POWÓD, nie samą komendę.
const BECAUSE: &str = "The cookie name is wrong in two places";

/// Ta sama komenda w ŚRODKU zdania: opis, nie polecenie.
///
/// Przycisk pod opisem startuje bieg, o który nikt nie prosił — a zdanie „zrobiłbym to przez
/// /run easy" jest właśnie opisem. To jest jedyny przypadek w tym pliku, w którym implementacja
/// szukająca napisu `/run` gdziekolwiek w prozie robi się czerwona.
const MENTIONS: &str = "I would reach for /run easy here, but let us read the test first.";

/// Zwykła proza lidera. Ani śladu komendy.
const PLAIN: &str = "The login test fails because the cookie name is wrong in two places.";

/// Wiersze, jakie z tej prozy robi ROZMOWA: kuracja, a po niej rozpoznanie propozycji.
///
/// Ta para i w tej kolejności, bo dokładnie tak stoi w `commands::chat::read_along` — test,
/// który wołałby samo rozpoznanie, mierzyłby funkcję, a nie drogę, którą wiersz przebywa.
fn conversation(agent: &str, said: &[&str]) -> Vec<Line> {
    let mut curator = Curator::new();
    let mut history = Vec::new();
    for text in said {
        let event = AgentEvent::Said {
            text: (*text).to_owned(),
        };
        // Czas jest stały i to nie jest uproszczenie: okno sklejania (reguła 4) dotyczy czytania,
        // szukania i zmian, a proza nie skleja się z niczym.
        let seen = Seen {
            agent,
            at_ms: 0,
            event: &event,
            tool: None,
        };
        history.extend(
            curator
                .observe(seen)
                .into_iter()
                .map(|line| suggested(line, &event)),
        );
    }
    history.extend(curator.flush());
    history
}

/// Wiersze, jakie z tej samej prozy robi BIEG: sam kurator i nic obok niego.
fn curated(agent: &str, said: &str) -> Vec<Line> {
    let event = AgentEvent::Said {
        text: said.to_owned(),
    };
    let mut curator = Curator::new();
    let mut history = curator.observe(Seen {
        agent,
        at_ms: 0,
        event: &event,
        tool: None,
    });
    history.extend(curator.flush());
    history
}

/// Komenda, którą niesie ten wiersz — albo `None`, kiedy to nie jest propozycja.
///
/// `match`, nie sięganie po pole wariantu na siłę: wiersz innego rodzaju jest tu ODPOWIEDZIĄ,
/// a nie awarią przypadku testowego. Dwa z czterech przypadków poniżej patrzą właśnie na to.
fn command_of(line: &Line) -> Option<&str> {
    match line {
        Line::Suggested { command, .. } => Some(command),
        _ => None,
    }
}

/// Rodzaje wierszy w kolejności, w jakiej powstały.
fn kinds(history: &[Line]) -> Vec<LineKind> {
    history.iter().map(Line::kind).collect()
}

#[test]
fn the_leads_command_becomes_a_row_that_carries_it_character_for_character() {
    let history = conversation(LEAD, &[PROPOSES]);

    assert_eq!(
        kinds(&history),
        [LineKind::Suggested],
        "prose whose first line is a command has to become a row of its own kind. Left as a \
         `note` it is a sentence with a slash in it: the window has no way to tell it from any \
         other paragraph, and the one thing this task exists to add — one click instead of \
         retyping — has nowhere to hang. The rows were {history:?}"
    );
    assert_eq!(
        history.first().and_then(command_of),
        Some(COMMAND),
        "the row has to carry the command CHARACTER FOR CHARACTER, in a field of its own. \
         Anything else means the window has to cut it back out of the prose — and a window \
         that reads `/run` out of an agent's paragraph is the CSS curation this whole design \
         refuses (invariant 15)"
    );
    assert!(
        history.first().map_or("", Line::text).contains(BECAUSE),
        "the row lost the reason the lead gave. A person is meant to read WHY before clicking; \
         a row carrying only the command is a one-field form, not a turn in a conversation. \
         The row said {:?}",
        history.first().map_or("", Line::text)
    );
    assert_ne!(
        history.first().map_or("", Line::text),
        COMMAND,
        "the text of the row is the whole prose, not the command over again — otherwise the \
         two fields say one thing and the second sentence of the lead is gone"
    );
    assert_eq!(
        history.first().map(Line::agent),
        Some(LEAD),
        "and it is still signed by whoever said it: `agent` answers 'whose tile is this' in \
         every row, and a proposal signed by nobody has no tile to belong to"
    );
}

#[test]
fn a_command_written_in_the_middle_of_a_sentence_is_a_description_not_an_order() {
    let history = conversation(LEAD, &[MENTIONS]);

    assert_eq!(
        kinds(&history),
        [LineKind::Note],
        "`/run` inside a sentence is the lead DESCRIBING a way to do something, and a button \
         under a description starts work nobody asked for. Recognising the word wherever it \
         appears is the cheap version of this whole file, and this is the case that says so. \
         The rows were {history:?}"
    );
    assert_eq!(
        history.first().and_then(command_of),
        None,
        "and nothing came out of it that a button could run"
    );
}

#[test]
fn prose_from_a_step_inside_a_run_is_never_a_proposal() {
    assert_eq!(
        kinds(&curated(STEP, PROPOSES)),
        [LineKind::Note],
        "the path a RUN takes is the curator alone, and it must never mint a proposal: a step \
         that writes `/run …` in its prose would grow a button starting a SECOND run, and this \
         engine drives one — `AppState::begin_run` swaps the handle, so the first would be \
         orphaned and keep burning the allowance (invariant 6)"
    );
    assert_eq!(
        kinds(&curated(LEAD, PROPOSES)),
        [LineKind::Note],
        "and that stays true for the lead's own prose: what makes a proposal is the path the \
         line takes, not the name in the `agent` field. A curator that looked at the name would \
         be a curator with an opinion about who is talking"
    );
    assert_eq!(
        kinds(&conversation(LEAD, &[PROPOSES])),
        [LineKind::Suggested],
        "the other half, and without it this case is green for an implementation that \
         recognises NOTHING anywhere: the same prose, taken through the conversation, IS a \
         proposal"
    );
}

#[test]
fn one_conversation_carrying_both_kinds_of_prose_gives_two_different_rows() {
    let history = conversation(LEAD, &[PLAIN, PROPOSES]);

    assert_eq!(
        kinds(&history),
        [LineKind::Note, LineKind::Suggested],
        "two sentences, two kinds of row. One kind for both is the empty pass this case exists \
         to stop: all-notes is the state before this task, and all-proposals is an \
         implementation for which every paragraph grows a button. The rows were {history:?}"
    );
    assert_eq!(
        history.first().and_then(command_of),
        None,
        "the plain sentence carries no command"
    );
    assert_eq!(
        history.get(1).and_then(command_of),
        Some(COMMAND),
        "and the one that is a proposal carries exactly the command the lead typed"
    );
}
