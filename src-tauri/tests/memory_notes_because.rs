//! AC-3 dla T-17: notatka bez uzasadnienia nie powstaje.
//!
//! `because TEXT NOT NULL` jest najważniejszą linią schematu z [T6 §10.3] — „no because,
//! no memory". Stoi za tym pomiar, nie gust: arXiv 2608.11095 na 1867 repozytoriach i 247 694
//! życiorysach instrukcji pokazuje, że **uzasadnienie instrukcji rozpada się szybciej niż sama
//! instrukcja**, a kiedy „dlaczego" zniknie, bezpieczne skasowanie kosztuje `O(2^|D|)`, bo
//! trzeba od nowa wyprowadzić interakcje z każdą inną instrukcją [T6 §5.1]. Notatka bez
//! powodu jest nieusuwalna, a nieusuwalne notatki narastają, aż zatrują kontekst.
//!
//! **Słabą wersją tego kryterium jest `assert!(result.is_err())`.** Przechodzi na dwa sposoby,
//! które ten plik rozróżnia:
//! - walidacja uruchamiana **po** zapisie — plik powstaje i zostaje, a błąd i tak wraca.
//!   Rozróżnia porównanie zawartości katalogu `notes/` przed i po, razem z treścią plików;
//! - `because: "   "` przepuszczone, „bo pole jest `String` i jest niepuste". Rozróżnia
//!   osobny przypadek z samymi białymi znakami.
//!
//! Trzeci kierunek jest tak samo wiążący i dlatego jest tutaj: notatka, której `because`
//! ktoś wyczyścił ręcznie na dysku, nie daje się promować. Reguła obowiązuje plik, który już
//! leży, nie tylko zapis — inaczej wystarczy skasować jedną linię, żeby ją ominąć.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use loadout_lib::memory::notes::{
    Actor, Error, Kind, NoteDraft, NoteId, Scope, Status, promote, record_candidate,
};

const AT: &str = "2026-08-16T09:14:00Z";
const CLICKED_AT: &str = "2026-08-16T14:02:11Z";

/// Notatka, której `because` ktoś skasował ręcznie w edytorze — wiersz został, treść zniknęła.
const CLEARED_SLUG: &str = "the-guard-runs-after-the-tenant-is-resolved";

/// Zasadza dwie notatki: jedną zwyczajną i jedną z pustym `because`.
fn plant(root: &Path) {
    let notes = root.join("notes");
    fs::create_dir_all(&notes).unwrap();

    fs::write(
        notes.join("migrations-are-additive.md"),
        "---\n\
         scope: this-project\n\
         kind: rule\n\
         title: Migrations are additive\n\
         rule: Migrations only add; DROP and rewriting rows are never written.\n\
         because: rewriting rows once made a run impossible to reproduce\n\
         status: suggested\n\
         occurrences: 1\n\
         modified: 2026-08-15T10:31:02Z\n\
         last_used_at: null\n\
         ---\n\
         \n\
         How to apply: add a column, never change one.\n",
    )
    .unwrap();

    fs::write(
        notes.join(format!("{CLEARED_SLUG}.md")),
        "---\n\
         scope: this-project\n\
         kind: fact\n\
         title: The guard runs after the tenant is resolved\n\
         rule: The guard runs after the tenant is resolved, so a 401 is the tenant, not the user.\n\
         because:\n\
         status: suggested\n\
         occurrences: 1\n\
         modified: 2026-08-15T10:31:02Z\n\
         last_used_at: null\n\
         ---\n\
         \n\
         How to apply: read the middleware first.\n",
    )
    .unwrap();
}

/// Nazwy **i treść** plików w `notes/`. Same nazwy przepuściłyby implementację, która nie
/// tworzy nowego pliku, tylko dopisuje do istniejącego.
fn snapshot(root: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(root.join("notes"))
        .expect("notes/ has to exist before this test can say anything about it")
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(entry.path()).unwrap_or_default(),
            )
        })
        .collect();
    out.sort();
    out
}

fn field_on_disk(root: &Path, id: &NoteId, key: &str) -> String {
    let path = root.join("notes").join(format!("{id}.md"));
    let text = fs::read_to_string(&path).expect("the note file is gone or unreadable");
    let head = text.split("\n---").next().unwrap_or_default().to_owned();
    head.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_default()
}

fn draft(title: &str, because: &str) -> NoteDraft {
    NoteDraft {
        title: title.to_owned(),
        rule: "Prompts and secrets travel on stdin, never in argv.".to_owned(),
        because: because.to_owned(),
        scope: Scope::ThisProject,
        kind: Kind::Rule,
        status: Status::Suggested,
        at: AT.to_owned(),
    }
}

#[test]
fn a_draft_with_no_reason_writes_nothing_at_all() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());
    let before = snapshot(root.path());

    for (label, because) in [("empty", ""), ("whitespace", "   \t  ")] {
        let refused =
            record_candidate(root.path(), draft(&format!("A note with {label}"), because));

        assert!(
            matches!(refused, Err(Error::NoBecause)),
            "a reason made of {label} is not a reason, and this came back instead: {refused:?}. \
             A `String` that is technically non-empty is exactly how `\"   \"` gets through"
        );
        assert_eq!(
            snapshot(root.path()),
            before,
            "and NOTHING was written. Validation that runs after the write refuses just as \
             loudly and leaves the file lying there — the listing is the only thing that tells \
             the two apart"
        );
    }
}

#[test]
fn the_refusal_says_what_is_missing_in_a_word_a_person_knows() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let refused = record_candidate(root.path(), draft("A note with no reason", ""));
    assert!(
        refused.is_err(),
        "a note with no reason behind it must not be recordable at all: {refused:?}"
    );
    // `unwrap_or_default()` zamiast `panic!`: pusty łańcuch przewraca każdą asercję niżej
    // z jej własnym, nazwanym komunikatem, zamiast zamienić tę porażkę w bezimienną panikę.
    let said = refused
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();

    assert!(
        said.to_lowercase().contains("reason"),
        "the sentence has to name the missing thing in a word a person already knows. It says: \
         {said:?}"
    );
    assert!(
        said.split_whitespace().count() >= 4,
        "and it is a sentence, not a word. Refusing in silence looks exactly like a broken \
         button, and the person is the only one who can fix this. It says: {said:?}"
    );
    for jargon in ["because", "NoBecause", "TEXT NOT NULL"] {
        assert!(
            !said.contains(jargon),
            "{jargon:?} names the field, the variant or the column — the person reading this \
             learns what the code calls it, not what they have to do (invariant 14). It says: \
             {said:?}"
        );
    }
}

#[test]
fn a_draft_with_a_reason_lands_and_the_file_carries_it() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());
    let before = snapshot(root.path());

    let reason = "the supervisor logged argv once and a key ended up in the run log";
    let recorded = record_candidate(root.path(), draft("Secrets travel on stdin", reason))
        .expect("a draft that says why it is true is exactly the one that may be written");

    assert_eq!(
        snapshot(root.path()).len(),
        before.len() + 1,
        "one new file, and the ones that were already there are untouched"
    );
    assert_eq!(
        field_on_disk(root.path(), &recorded.id, "because"),
        reason,
        "the reason stands in the file, on its own line, word for word. A reason kept only in \
         a row of loadout.db disappears with the index, and then the note can never be safely \
         retired again (T6 §5.1)"
    );
    assert_eq!(
        recorded.because, reason,
        "and the value that comes back carries it too"
    );
}

#[test]
fn a_note_whose_reason_was_cleared_by_hand_cannot_be_put_to_use() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());
    let id = NoteId(CLEARED_SLUG.to_owned());

    assert_eq!(
        field_on_disk(root.path(), &id, "because"),
        "",
        "the fixture itself has to carry a note with an emptied reason, or the test below asks \
         nothing"
    );

    let refused = promote(
        root.path(),
        &id,
        Actor::You {
            at: CLICKED_AT.to_owned(),
        },
    );

    assert!(
        matches!(refused, Err(Error::NoBecause)),
        "a person clicked, and a person may promote — but there is nothing here to promote \
         yet. The rule holds for a file that already exists, not only for the write: otherwise \
         deleting one line is all it takes to get around it. Came back: {refused:?}"
    );
    assert_eq!(
        field_on_disk(root.path(), &id, "status"),
        "suggested",
        "and the file did not move"
    );
    assert_eq!(
        field_on_disk(root.path(), &id, "modified"),
        "2026-08-15T10:31:02Z",
        "not even the moment was stamped"
    );
}
