//! AC-5 dla T-18: „Claude to widzi" jest odczytem ze zdarzenia init, nie domysłem.
//!
//! **Słabą wersją tego kryterium jest `init_line.contains(name)`.** Przechodzi przypadki (a)
//! i (b) — i **kłamie** na (c), gdzie nazwa umiejętności występuje w `cwd` i w nazwie serwera
//! narzędzi, a w żadnej z dwóch tablic jej nie ma. To jest dokładnie ten fałszywy zielony
//! ptaszek, o który chodzi w całym tym zadaniu: plik leży o poziom obok ścieżki, w którą
//! vendor zagląda, użytkownik widzi „Installed for 6 tools" i dowiaduje się o niczym, bo
//! „agent nie wie o umiejętności" nie odróżnia się od „model nie uznał, że warto jej użyć".
//!
//! Drugi rozróżniający przypadek to (d): zdarzenie bez obu kluczy. Implementacja, która
//! tłumaczy „brak klucza" na „nie widzi", wywoła fałszywy alarm przy pierwszej zmianie
//! kształtu zdarzenia — a vendorzy dokładają klucze co tydzień, po cichu (niezmiennik 5).
//!
//! Linie init stoją tu literalnie. Kryterium jest offline: uruchomienie prawdziwego `claude`
//! nigdy nie jest kryterium, bo brak CLI nie może być czerwony [T5 §6.3].

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use loadout_lib::skills::place::{self, Discovery};

/// (a) Zdarzenie z tablicą `skills`, a w niej nasza nazwa.
const WITH_SKILLS: &str = r#"{"type":"system","subtype":"init","cwd":"/home/u/work","model":"claude-sonnet-5","tools":["Read","Write"],"mcp_servers":[],"slash_commands":["notatki"],"skills":["notatki","pdf"]}"#;

/// (b) Bez `skills`; nazwa stoi w `slash_commands`. Tak umiejętność z `~/.claude/skills`
/// objawia się w CLI v2.1.233.
const WITH_SLASH_COMMANDS: &str = r#"{"type":"system","subtype":"init","cwd":"/home/u/work","model":"claude-sonnet-5","tools":["Read","Write"],"mcp_servers":[],"slash_commands":["notatki","pdf"]}"#;

/// (c) Nazwa występuje w `cwd` i w `mcp_servers[].name`, a w żadnej z dwóch tablic jej nie ma.
const ONLY_IN_THE_PATH: &str = r#"{"type":"system","subtype":"init","cwd":"/home/u/review-pull-requests/x","model":"claude-sonnet-5","tools":["Read"],"mcp_servers":[{"name":"review-pull-requests","status":"connected"}],"slash_commands":["notatki"],"skills":["notatki"]}"#;

/// (d) Zdarzenie bez obu kluczy — kształt, którego jeszcze nie widzieliśmy.
const NEITHER_KEY: &str = r#"{"type":"system","subtype":"init","cwd":"/home/u/work","model":"claude-sonnet-5","tools":["Read"],"mcp_servers":[]}"#;

/// (f) `skills` jest, ale bez naszej nazwy — za to `slash_commands` ją ma.
const SKILLS_WINS: &str = r#"{"type":"system","subtype":"init","cwd":"/home/u/work","model":"claude-sonnet-5","tools":["Read"],"mcp_servers":[],"slash_commands":["notatki","pdf"],"skills":["notatki"]}"#;

fn wrote() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/home/u/.claude/skills/pdf"),
        PathBuf::from("/home/u/.agents/skills/pdf"),
    ]
}

#[test]
fn the_skills_array_answers_the_question_when_it_is_there() {
    assert_eq!(
        place::discovery_from_init("pdf", WITH_SKILLS, &wrote()),
        Discovery::Seen,
        "the event listed `pdf` among its skills and the verdict was not Seen"
    );
}

#[test]
fn slash_commands_answers_it_when_the_skills_array_is_absent() {
    assert_eq!(
        place::discovery_from_init("pdf", WITH_SLASH_COMMANDS, &wrote()),
        Discovery::Seen,
        "CLI v2.1.233 reports a skill from ~/.claude/skills only through `slash_commands`. \
         Reading nothing else means every install looks unproven on the version people run"
    );
}

#[test]
fn the_skills_array_is_the_only_one_that_counts_once_it_exists() {
    assert_eq!(
        place::discovery_from_init("pdf", SKILLS_WINS, &wrote()),
        Discovery::NotSeen { looked_in: wrote() },
        "the event carried a `skills` array without `pdf` in it. Falling back to \
         `slash_commands` there turns the authoritative list into a suggestion, and a skill \
         the CLI stopped loading keeps reporting as seen"
    );
}

#[test]
fn a_name_that_only_shows_up_in_the_path_is_not_a_sighting() {
    let verdict = place::discovery_from_init("review-pull-requests", ONLY_IN_THE_PATH, &wrote());

    assert_eq!(
        verdict,
        Discovery::NotSeen { looked_in: wrote() },
        "`review-pull-requests` appears twice in this event — in `cwd` and as the name of a \
         tool server — and in neither of the two arrays. Anything that searches the whole line \
         says Seen here, which is the false green check this task exists to prevent"
    );
}

#[test]
fn an_event_with_neither_key_is_unknown_and_never_a_denial() {
    let verdict = place::discovery_from_init("pdf", NEITHER_KEY, &wrote());

    assert!(
        matches!(verdict, Discovery::Unknown(_)),
        "an event carrying neither `skills` nor `slash_commands` says nothing about `pdf`, and \
         the verdict was {verdict:?}. Reading a missing key as `does not see it` raises a false \
         alarm the first time a vendor changes the shape of the event — and they change it \
         weekly, quietly (invariant 5)"
    );
    assert_ne!(
        verdict,
        Discovery::Unknown("not installed"),
        "`the event said nothing` and `the CLI is not installed` are different situations with \
         different next steps, so they cannot share one sentence"
    );
}

#[test]
fn a_vendor_that_never_started_is_unknown_not_missing() {
    assert_eq!(
        place::discovery_from_init("pdf", "", &wrote()),
        Discovery::Unknown("not installed"),
        "no CLI means no init event, and a vendor the user does not have installed is not a \
         failed install [T5 §6.3]"
    );
}
