//! AC-4 dla T-17: przepełniony budżet daje wymuszony wybór, nie ciche przycięcie.
//!
//! [T6 §5.3] jest tu jednoznaczne: „The budget is the real anti-bloat mechanism. Each scope
//! has a hard cap on the *active* set" — 1000 / 1500 / 800 — oraz „When a promotion would
//! exceed the cap, Loadout does not silently trim — it shows a forced choice". Ciche
//! przycięcie wygląda w interfejsie identycznie jak sukces i różni się tylko tym, że notatka,
//! którą człowiek zatwierdził, przestaje docierać do modelu.
//!
//! **Słabą wersją tego kryterium jest `assert!(matches!(err, MemoryFull { .. }))`.** Przechodzi
//! na implementacji, która przy okazji „na wszelki wypadek" wyrzuciła najdawniej używaną
//! notatkę z użycia albo skróciła blok. Rozróżniają trzy rzeczy, wszystkie w tym pliku:
//! kolejność w `retire` (najdawniej użyte pierwsze), suma długości tej listy (musi pokryć
//! deficyt) i **bajtowa równość** bloku sprzed i po odmowie.
//!
//! Jednostka długości: [T6 §10.2] mówi ~4 bajty na jednostkę i tak liczy `memory::est_tokens`
//! z T-16. Kosztem notatki jest długość jej `rule`, bo `rule` jest jedyną częścią notatki,
//! która trafia do promptu — nagłówek bloku i myślniki są ramą o stałej długości. Reguły
//! w tym teście są dopychane do **dokładnej** wielokrotności czterech bajtów, więc arytmetyka
//! niżej jest równością, nie oszacowaniem.
//!
//! Trzy zakresy leżą w **jednym** drzewie i to jest część kryterium: sumowanie „wszystkiego,
//! co w użyciu" bez patrzenia na zakres daje 2900 zamiast 1400 i przewraca każdą liczbę tutaj.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use loadout_lib::memory::est_tokens;
use loadout_lib::memory::notes::{
    Actor, Budget, Error, Note, NoteId, Scope, Status, promote, scan_notes, what_you_know,
};

/// Zasadzone drzewo: `(id, scope, status, jednostki, last_used_at)`.
///
/// Sumy w użyciu: `this-project` 1400 przy limicie 1500, `this-agent` 600 przy 800,
/// `everywhere` 900 przy 1000. Każdy zakres jest o krok od pełna i każdy o inny krok —
/// jedna liczba zaszyta na sztywno zamiast trzech przewraca co najmniej dwa testy niżej.
const PLANTED: [(&str, &str, &str, usize, &str); 12] = [
    // Kolejność ostatniego użycia jest CELOWO inna niż alfabetyczna i inna niż po koszcie.
    // Lista, którą dostaje człowiek, ma być posortowana po tym, czego model najdawniej
    // potrzebował — nie po tym, co akurat było pierwsze w katalogu.
    (
        "alpha-project",
        "this-project",
        "in-use",
        500,
        "2026-08-14T09:00:00Z",
    ),
    (
        "bravo-project",
        "this-project",
        "in-use",
        400,
        "2026-08-11T09:00:00Z",
    ),
    (
        "charlie-project",
        "this-project",
        "in-use",
        300,
        "2026-08-15T09:00:00Z",
    ),
    (
        "delta-project",
        "this-project",
        "in-use",
        200,
        "2026-08-12T09:00:00Z",
    ),
    ("echo-project", "this-project", "suggested", 300, "null"),
    (
        "alpha-agent",
        "this-agent",
        "in-use",
        400,
        "2026-08-13T09:00:00Z",
    ),
    (
        "bravo-agent",
        "this-agent",
        "in-use",
        200,
        "2026-08-10T09:00:00Z",
    ),
    ("charlie-agent", "this-agent", "suggested", 200, "null"),
    ("delta-agent", "this-agent", "suggested", 300, "null"),
    (
        "alpha-everywhere",
        "everywhere",
        "in-use",
        500,
        "2026-08-09T09:00:00Z",
    ),
    (
        "bravo-everywhere",
        "everywhere",
        "in-use",
        400,
        "2026-08-08T09:00:00Z",
    ),
    ("charlie-everywhere", "everywhere", "suggested", 200, "null"),
];

/// Notatki w użyciu z zakresu `this-project`, uporządkowane tak, jak człowiek ma je zobaczyć:
/// najdawniej użyte pierwsze. Wypisane wprost, nie policzone — kryterium, które sortuje sobie
/// oczekiwanie tą samą regułą co implementacja, sprawdza samo siebie (niezmiennik 20).
const LEAST_RECENTLY_USED_FIRST: [&str; 4] = [
    "bravo-project",
    "delta-project",
    "alpha-project",
    "charlie-project",
];

const CLICKED_AT: &str = "2026-08-16T14:02:11Z";

/// Reguła o **dokładnie** `units` jednostkach długości.
fn rule_worth(units: usize, label: &str) -> String {
    let wanted = units * 4;
    let mut rule = format!("{label} carries a rule worth {units} units of length ");
    assert!(
        rule.len() < wanted,
        "the label alone is longer than the note is supposed to be"
    );
    while rule.len() + 1 < wanted {
        rule.push('x');
    }
    rule.push('.');

    assert_eq!(
        est_tokens(rule.len()),
        units,
        "the fixture has to hit the number exactly, or every sum below is an estimate and the \
         criterion asks nothing"
    );
    rule
}

fn plant(root: &Path) {
    let notes = root.join("notes");
    fs::create_dir_all(&notes).unwrap();

    for (id, scope, status, units, last_used) in PLANTED {
        fs::write(
            notes.join(format!("{id}.md")),
            format!(
                "---\n\
                 scope: {scope}\n\
                 kind: fact\n\
                 title: The note called {id}\n\
                 rule: {}\n\
                 because: it was measured on this machine and written down the same day\n\
                 status: {status}\n\
                 occurrences: 1\n\
                 modified: 2026-08-15T10:31:02Z\n\
                 last_used_at: {last_used}\n\
                 ---\n\
                 \n\
                 How to apply: read it before the next run.\n",
                rule_worth(units, id)
            ),
        )
        .unwrap();
    }
}

/// Skan plus sprawdzenie, że koszt każdej notatki jest tym, co zasadziliśmy. Cała arytmetyka
/// niżej stoi na tej równości, więc stoi tutaj, a nie w komentarzu.
fn scan(root: &Path) -> Vec<Note> {
    let notes = scan_notes(root).expect("the scan fell over on a tree this test wrote itself");

    for (id, _, _, units, _) in PLANTED {
        let found = notes.iter().find(|note| note.id == NoteId(id.to_owned()));
        assert_eq!(
            found.map(|note| note.est_tokens),
            Some(units),
            "{id} was planted with {units} units of length in its rule, and the scan says \
             otherwise. What a note costs is the length of the line that reaches the prompt"
        );
    }
    notes
}

fn field_on_disk(root: &Path, id: &str, key: &str) -> String {
    let text = fs::read_to_string(root.join("notes").join(format!("{id}.md")))
        .expect("the note file is gone or unreadable");
    let head = text.split("\n---").next().unwrap_or_default().to_owned();
    head.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_default()
}

fn ask_to_use(root: &Path, id: &str) -> Result<Note, Error> {
    promote(
        root,
        &NoteId(id.to_owned()),
        Actor::You {
            at: CLICKED_AT.to_owned(),
        },
    )
}

/// Rozbiera odmowę na `(over_by, retire)`. Cokolwiek innego niż [`Error::MemoryFull`] jest
/// porażką opisaną tutaj, a nie paniką bez nazwy dalej.
fn memory_full(refused: Result<Note, Error>) -> (usize, Vec<NoteId>) {
    let described = format!("{refused:?}");
    let full = match refused {
        Err(Error::MemoryFull { over_by, retire }) => Some((over_by, retire)),
        _ => None,
    };
    assert!(
        full.is_some(),
        "this promotion does not fit the cap of its scope, so the only correct answer is a \
         forced choice. Came back: {described}"
    );
    full.unwrap_or_default()
}

#[test]
fn a_promotion_over_the_cap_is_refused_with_the_size_of_the_overflow() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let (over_by, _) = memory_full(ask_to_use(root.path(), "echo-project"));

    assert_eq!(
        over_by, 200,
        "1400 units are in use in this project and the cap is 1500, so a note worth 300 is 200 \
         over. A single cap shared by every scope would answer something else here, and so \
         would a cap counted over notes from the other two scopes lying in the same tree"
    );

    assert_eq!(
        field_on_disk(root.path(), "echo-project", "status"),
        "suggested",
        "and the note did not move. A refusal that writes first is a note in use that nobody \
         approved (invariant 4: the file is the truth)"
    );
    for id in LEAST_RECENTLY_USED_FIRST {
        assert_eq!(
            field_on_disk(root.path(), id, "status"),
            "in-use",
            "{id} was in use before the refusal and nothing gave anyone permission to retire \
             it. Making room \"just in case\" is exactly the silent trim this criterion exists \
             to stop"
        );
    }
}

#[test]
fn the_forced_choice_starts_with_what_the_model_needed_longest_ago() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let (over_by, retire) = memory_full(ask_to_use(root.path(), "echo-project"));
    let named: Vec<String> = retire.iter().map(ToString::to_string).collect();
    let named_refs: Vec<&str> = named.iter().map(String::as_str).collect();

    assert!(
        !retire.is_empty(),
        "a forced choice with nothing to choose from is not a choice, it is a dead end"
    );
    assert!(
        LEAST_RECENTLY_USED_FIRST.starts_with(&named_refs),
        "the list has to run least-recently-used first, and it reads {named:?}. Sorting by \
         name, by length or by whatever the directory returned puts the note the model used an \
         hour ago at the top of the list a person is asked to give up"
    );

    let notes = scan(root.path());
    let freed: usize = retire
        .iter()
        .filter_map(|id| notes.iter().find(|note| &note.id == id))
        .map(|note| note.est_tokens)
        .sum();
    assert!(
        freed >= over_by,
        "retiring everything on this list frees {freed} units and the promotion is {over_by} \
         over. A list that cannot cover the deficit sends the person round the same wall twice"
    );

    assert!(
        !named.contains(&"echo-project".to_owned()),
        "and the note being put to use is not on the list of things to give up for it"
    );
    for id in &named {
        assert!(
            LEAST_RECENTLY_USED_FIRST.contains(&id.as_str()),
            "{id} is not a note in use in this scope, so it has no business in this list"
        );
    }
}

#[test]
fn what_the_prompt_gets_is_byte_for_byte_the_same_after_the_refusal() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let before = what_you_know(&scan(root.path()), Budget::of(Scope::ThisProject));
    assert_eq!(
        before.used.len(),
        4,
        "all four notes in use fit under the cap before anything is asked of them, so all four \
         are in the block. If they are not, the rest of this test compares two truncated \
         strings and passes"
    );
    assert!(
        before.dropped.is_empty(),
        "and nothing was dropped for length: {:?}",
        before.dropped
    );

    let refused = ask_to_use(root.path(), "echo-project");
    assert!(
        refused.is_err(),
        "the fixture depends on this being refused"
    );

    let after = what_you_know(&scan(root.path()), Budget::of(Scope::ThisProject));
    assert_eq!(
        after.text, before.text,
        "a refused promotion changes nothing about what the model is told — byte for byte. An \
         implementation that trims the block, or quietly retires the oldest note to make room, \
         answers `MemoryFull` just as correctly and still changes the prompt underneath"
    );
    assert_eq!(
        after.used, before.used,
        "and the same notes are named as used"
    );
}

#[test]
fn a_promotion_that_lands_exactly_on_the_cap_goes_through() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let promoted = ask_to_use(root.path(), "charlie-agent")
        .expect("600 units in use plus 200 is exactly 800, and exactly the cap is not over it");

    assert_eq!(promoted.status, Status::InUse, "it went through");
    assert_eq!(
        field_on_disk(root.path(), "charlie-agent", "status"),
        "in-use",
        "and the file says so"
    );

    let block = what_you_know(&scan(root.path()), Budget::of(Scope::ThisAgent));
    assert!(
        block.used.contains(&NoteId("charlie-agent".to_owned())),
        "and from now on it is in the prompt. A budget that refuses the note it just accepted \
         would leave the person looking at `In use` that means nothing"
    );
}

#[test]
fn each_scope_is_measured_against_its_own_cap() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    // Ten agent ma 600 jednostek w użyciu przy limicie 800. Notatka warta 300 zmieściłaby się
    // w limicie projektu (1500) i w limicie „wszędzie" (1000) — odmowa tutaj jest jedynym
    // zdaniem, które odróżnia trzy liczby od jednej.
    let (over_by, _) = memory_full(ask_to_use(root.path(), "delta-agent"));
    assert_eq!(
        over_by, 100,
        "`This agent` holds 800 units [T6 §5.3], 600 of them are in use, so a note worth 300 \
         is 100 over. A single shared cap of 1500 would have let this one straight through"
    );

    // Wszędzie: 900 w użyciu przy limicie 1000.
    let (over_by, _) = memory_full(ask_to_use(root.path(), "charlie-everywhere"));
    assert_eq!(
        over_by, 100,
        "`Everywhere` holds 1000 units and 900 of them are in use, so a note worth 200 is 100 \
         over. This is the narrowest budget of the three, because this text rides into every \
         prompt of every project"
    );
}
