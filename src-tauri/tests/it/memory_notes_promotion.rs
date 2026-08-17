//! AC-2 dla T-17: do „in use" prowadzi wyłącznie działanie człowieka.
//!
//! [ARCHITECTURE §2 pyt. 5] rozstrzyga to za cały produkt: dwa stany, promuje **wyłącznie
//! człowiek**, bez człowieka notatka zostaje sugerowana i nigdy nie trafia do promptu.
//! To unieważnia auto-promocję przy drugim wystąpieniu z [T6 §5.3] — powtórzenie podbija
//! `occurrences` i na tym kończy swoją władzę. Powód nie jest estetyczny: arXiv 2608.11095
//! na 1867 repozytoriach pokazuje, że nieobsługiwana akrecja instrukcji **jest** chorobą,
//! a agent nie jest lepszym kuratorem niż ludzie, którzy te pliki utrzymywali [T6 §5.3].
//!
//! **Słabą wersją tego kryterium jest `assert!(promote(.., Actor::Agent(..)).is_err())`.**
//! Przechodzi na implementacji, która zapisuje plik, a dopiero potem zwraca błąd — czyli na
//! takiej, po której na dysku leży notatka w użyciu, której nikt nie zatwierdził. Rozróżnia
//! **ponowny odczyt pliku z dysku** po każdym wywołaniu, i dlatego status czytamy tutaj
//! bajtami, a nie przez `scan_notes`: pytanie brzmi „co leży na dysku", a nie „co o tym sądzi
//! ta sama implementacja, którą sądzimy".
//!
//! Drugą rzeczą, której żadna implementacja z auto-promocją nie przejdzie, jest przypadek
//! „dwa biegi": ta sama kandydatka zgłoszona dwa razy ma `occurrences == 2` i status dalej
//! `suggested`.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use loadout_lib::memory::notes::{
    Actor, Error, Kind, NoteDraft, NoteId, Scope, Status, promote, record_candidate,
};

/// Tytuł zasadzonej notatki i nazwa jej pliku. Nazwa jest funkcją tytułu (`slugify` z T-16),
/// więc obie stoją tu wypisane wprost — kryterium nie ma prawa liczyć jej tą samą funkcją,
/// której używa implementacja (niezmiennik 20).
const TITLE: &str = "The tenant is resolved before the guard";
const SLUG: &str = "the-tenant-is-resolved-before-the-guard";

const RULE: &str = "Login goes through the tenant middleware before the guard, so an \
                    unresolved tenant surfaces as 401, not 400.";
const BECAUSE: &str = "run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88";

/// Chwila, w której zasadzona notatka ostatnio się zmieniła.
const PLANTED_AT: &str = "2026-08-15T10:31:02Z";
/// Chwila, w której człowiek klika „Use this".
const CLICKED_AT: &str = "2026-08-16T14:02:11Z";

/// Zasadza jedną sugerowaną notatkę i oddaje jej identyfikator.
fn plant(root: &Path) -> NoteId {
    let notes = root.join("notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(
        notes.join(format!("{SLUG}.md")),
        format!(
            "---\n\
             scope: this-project\n\
             kind: fact\n\
             title: {TITLE}\n\
             rule: {RULE}\n\
             because: {BECAUSE}\n\
             status: suggested\n\
             occurrences: 1\n\
             modified: {PLANTED_AT}\n\
             last_used_at: null\n\
             ---\n\
             \n\
             How to apply: read the middleware before blaming the guard.\n"
        ),
    )
    .unwrap();
    NoteId(SLUG.to_owned())
}

/// Wartość klucza odczytana **z pliku**, bez udziału skanera.
///
/// Dwa i więcej wierszy z tym samym kluczem to też porażka: implementacja, która dopisuje
/// `status: in-use` zamiast przepisać istniejący wiersz, zostawia plik, który mówi obie
/// rzeczy naraz — a czytelnik zobaczy tę, którą jego parser weźmie pierwszą.
fn field_on_disk(root: &Path, id: &NoteId, key: &str) -> String {
    let path = root.join("notes").join(format!("{id}.md"));
    let text = fs::read_to_string(&path).expect("the note file is gone or unreadable");
    let head = text.split("\n---").next().unwrap_or_default().to_owned();

    let mut found: Vec<String> = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_owned())
        .collect();

    assert_eq!(
        found.len(),
        1,
        "the front-matter has to carry exactly one `{key}:` line, and it carries {}. \
         A second line is not an extra fact, it is two answers to one question, and which one \
         wins depends on whose parser reads the file. The head reads:\n{head}",
        found.len()
    );
    found.remove(0)
}

/// Ile plików leży w `notes/`. Nazwy, nie liczba: przy porażce chcemy wiedzieć, co powstało.
fn listing(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("notes"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn draft(title: &str, at: &str, declared: Status) -> NoteDraft {
    NoteDraft {
        title: title.to_owned(),
        rule: "Migrations are additive and idempotent; DROP is never written.".to_owned(),
        because: "the store rewrote rows once and the run before it stopped being reproducible"
            .to_owned(),
        scope: Scope::ThisProject,
        kind: Kind::Rule,
        status: declared,
        at: at.to_owned(),
    }
}

#[test]
fn an_agent_and_loadout_are_both_refused_and_the_file_does_not_move() {
    let root = tempfile::tempdir().unwrap();
    let id = plant(root.path());

    for by in [Actor::Agent("research-auth".to_owned()), Actor::Loadout] {
        let refused = promote(root.path(), &id, by.clone());

        assert!(
            matches!(refused, Err(Error::OnlyYouCanDoThat)),
            "{by:?} asked for this note to be put to use and something other than a refusal \
             came back: {refused:?}. Only a person promotes (ARCHITECTURE §2 q. 5)"
        );
        assert_eq!(
            field_on_disk(root.path(), &id, "status"),
            "suggested",
            "the refusal is worth nothing if the file moved anyway. An implementation that \
             writes first and refuses afterwards passes every `is_err()` and leaves a note in \
             use that nobody approved — this is the line that tells the two apart"
        );
        assert_eq!(
            field_on_disk(root.path(), &id, "modified"),
            PLANTED_AT,
            "and nothing touched the file at all, not even to stamp it"
        );
    }
}

#[test]
fn a_person_puts_the_note_to_use_and_the_file_says_so() {
    let root = tempfile::tempdir().unwrap();
    let id = plant(root.path());

    let promoted = promote(
        root.path(),
        &id,
        Actor::You {
            at: CLICKED_AT.to_owned(),
        },
    )
    .expect("a person clicked, and this is the one actor that may do this");

    assert_eq!(
        promoted.status,
        Status::InUse,
        "the value that comes back has to agree with what just happened"
    );
    assert_eq!(
        field_on_disk(root.path(), &id, "status"),
        "in-use",
        "and the FILE has to say it, because the file is the truth (invariant 4). A status that \
         lives only in a row of loadout.db is a status that comes back as `suggested` the first \
         time somebody deletes the index"
    );
    assert_eq!(
        field_on_disk(root.path(), &id, "modified"),
        CLICKED_AT,
        "the moment moves to the moment of the click. It arrives with the actor, so this \
         module never reads a clock and two runs of the same test give the same bytes"
    );

    assert_eq!(
        field_on_disk(root.path(), &id, "because"),
        BECAUSE,
        "and nothing else in the file changed. Rendering the front-matter afresh from a \
         half-filled struct rewrites fields this action never asked about"
    );
    assert_eq!(
        field_on_disk(root.path(), &id, "rule"),
        RULE,
        "same for rule"
    );
    assert_eq!(
        field_on_disk(root.path(), &id, "occurrences"),
        "1",
        "same for occurrences"
    );
}

#[test]
fn a_draft_that_declares_itself_in_use_lands_as_suggested_anyway() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let recorded = record_candidate(
        root.path(),
        draft("Migrations are additive", PLANTED_AT, Status::InUse),
    )
    .expect("a candidate with a reason behind it is recordable");

    assert_eq!(
        recorded.status,
        Status::Suggested,
        "the draft declared `in use` and the declaration is IGNORED, not honoured. Anything \
         else and the two-state model is decoration: whoever writes the draft decides what the \
         next prompt says"
    );
    assert_eq!(
        field_on_disk(root.path(), &recorded.id, "status"),
        "suggested",
        "and the file says the same, so deleting the index cannot promote it either"
    );
}

#[test]
fn the_same_candidate_from_two_runs_is_counted_twice_and_promoted_never() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());
    let before = listing(root.path());

    let first = record_candidate(
        root.path(),
        draft("Migrations are additive", PLANTED_AT, Status::Suggested),
    )
    .expect("first run proposes it");

    // Ten sam tytuł po znormalizowaniu: inna wielkość liter, inne odstępy, wykrzyknik.
    // Dopasowanie po surowym łańcuchu przepuściłoby to jako drugą, nową notatkę.
    let second = record_candidate(
        root.path(),
        draft(
            "  migrations   are ADDITIVE!  ",
            CLICKED_AT,
            Status::Suggested,
        ),
    )
    .expect("second run proposes the same thing");

    assert_eq!(
        second.id, first.id,
        "the same candidate normalises to the same identity, so it is the same note. Two files \
         here means the count below can never reach two, and the repetition signal — the whole \
         reason the field exists — is lost"
    );
    assert_eq!(
        listing(root.path()).len(),
        before.len() + 1,
        "exactly one new file. `notes/` holds {:?}",
        listing(root.path())
    );

    assert_eq!(
        field_on_disk(root.path(), &second.id, "occurrences"),
        "2",
        "seen in two separate runs, and the file counts it"
    );
    assert_eq!(
        second.occurrences, 2,
        "and the value that comes back agrees"
    );

    assert_eq!(
        second.status,
        Status::Suggested,
        "T6 §5.3 proposes auto-promotion on the second sighting and ARCHITECTURE §2 q. 5 \
         overrules it: repetition is a signal FOR the person, never a decision instead of \
         them. No implementation that auto-promotes can pass this line"
    );
    assert_eq!(
        field_on_disk(root.path(), &second.id, "status"),
        "suggested",
        "and the file agrees with that too"
    );
    assert_eq!(
        field_on_disk(root.path(), &second.id, "modified"),
        CLICKED_AT,
        "the second sighting is a change to the note, so the moment moves"
    );
}
