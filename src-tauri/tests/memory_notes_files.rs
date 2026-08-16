//! AC-5 dla T-17: skasowanie indeksu nie zmienia tego, co wchodzi do promptu.
//!
//! To jest niezmiennik 4 postawiony przed sądem: „pliki są prawdą, `loadout.db` jest indeksem
//! — kasujesz bazę i nic nie ginie" [ARCHITECTURE §2 pyt. 2]. Cicha porażka jest tu bardzo
//! konkretna: `status` trzymany wyłącznie w kolumnie `SQLite`, „bo szybciej filtrować".
//! Kasujesz `loadout.db` i prawdy o tym, co zatwierdziłeś, już nie ma — przy odbudowie indeksu
//! wszystko wraca jako `suggested` albo, gorzej, jako `in use`.
//!
//! Dlatego ten test **wypisuje pliki notatek jako literalne stringi** i nie tworzy żadnej bazy.
//! Skan, który czyta tylko to, co sam zapisał, nie odpowiada na to pytanie ani trochę.
//!
//! **Słabą wersją tego kryterium jest sam pierwszy skan.** Przechodzi także na implementacji,
//! która czyta status z pliku raz, a potem trzyma go w pamięci albo w indeksie. Rozróżnia drugi
//! skan **po ręcznej edycji pliku**, w tym samym procesie, bez czyszczenia czegokolwiek — to
//! jest dokładnie zachowanie „pliki są prawdą".
//!
//! Pod spodem leży też niezmiennik 5. Katalog notatek potrafi zawierać plik od nowszego
//! Loadouta i plik po ręcznej edycji. Strict parser przewraca skan na **jednym** takim pliku,
//! a użytkownik widzi pustą sekcję Pamięć zamiast błędu — i notatka, którą zatwierdził,
//! po cichu przestaje docierać do modelu.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use loadout_lib::memory::notes::{
    Block, Budget, Kind, NoteId, Scope, Status, scan_notes, what_you_know,
};

const IN_1: &str = "OKAPI-IN-1";
const IN_2: &str = "OKAPI-IN-2";
const SUG_1: &str = "OKAPI-SUG-1";
const SUG_2: &str = "OKAPI-SUG-2";

/// Notatka od nowszego Loadouta: klucz, którego ta wersja nie zna, i `kind` spoza trójki.
const UNKNOWN_KEY: &str = "x-future";
const UNKNOWN_KIND: &str = "rumour";
/// Jej plik — ten sam, który drugi skan zastanie po ręcznej edycji.
const NEWER: &str = "the-index-is-disposable";

/// Cztery pliki, wypisane co do bajtu. Żaden nie powstał przez `record_candidate`.
const FILES: [(&str, &str); 4] = [
    (
        "tenant-before-guard",
        "---\n\
         scope: this-project\n\
         kind: fact\n\
         title: The tenant is resolved before the guard\n\
         rule: OKAPI-IN-1 an unresolved tenant surfaces as 401, not 400.\n\
         because: run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88\n\
         status: in-use\n\
         occurrences: 2\n\
         modified: 2026-08-15T10:31:02Z\n\
         last_used_at: 2026-08-16T08:00:00Z\n\
         ---\n\
         \n\
         How to apply: read the middleware before blaming the guard.\n",
    ),
    (
        NEWER,
        "---\n\
         scope: this-project\n\
         kind: rumour\n\
         x-future: 1\n\
         title: The index is disposable\n\
         rule: OKAPI-IN-2 delete loadout.db whenever it is in the way; the files carry the truth.\n\
         because: it was rebuilt from 5000 notes in about 200 ms on this machine\n\
         status: in-use\n\
         occurrences: 1\n\
         modified: 2026-08-15T11:02:00Z\n\
         last_used_at: 2026-08-16T09:00:00Z\n\
         ---\n\
         \n\
         How to apply: never write a field you cannot rebuild from the files.\n",
    ),
    (
        "retry-the-flaky-suite",
        "---\n\
         scope: this-project\n\
         kind: pitfall\n\
         title: Retry the flaky suite\n\
         rule: OKAPI-SUG-1 when the suite is red twice in a row, run it a third time.\n\
         because: an agent wrote this down after one red run and nobody checked it\n\
         status: suggested\n\
         occurrences: 1\n\
         modified: 2026-08-16T10:00:00Z\n\
         last_used_at: null\n\
         ---\n\
         \n\
         How to apply: it is not applied; nobody approved it.\n",
    ),
    (
        "always-take-the-fast-path",
        "---\n\
         scope: this-project\n\
         kind: rule\n\
         title: Always take the fast path\n\
         rule: OKAPI-SUG-2 skip the checks when the change is small.\n\
         because: one run was quicker that way, once\n\
         status: suggested\n\
         occurrences: 3\n\
         modified: 2026-08-16T12:00:00Z\n\
         last_used_at: null\n\
         ---\n\
         \n\
         How to apply: it is not applied; nobody approved it.\n",
    ),
];

fn plant(root: &Path) {
    let notes = root.join("notes");
    fs::create_dir_all(&notes).unwrap();
    for (name, body) in FILES {
        fs::write(notes.join(format!("{name}.md")), body).unwrap();
    }
}

fn block(root: &Path) -> Block {
    let notes = scan_notes(root).expect(
        "the scan fell over on a directory it did not write. One file from a newer Loadout must \
         never take the whole listing with it (invariant 5)",
    );
    what_you_know(&notes, Budget::of(Scope::ThisProject))
}

/// Wszystkie pliki w drzewie, ścieżkami względnymi. Skan jest odczytem i nie ma prawa niczego
/// po sobie zostawić — ani indeksu, ani pliku tymczasowego.
fn every_file(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.path().is_dir() {
            every_file(&entry.path(), &format!("{name}/"), out);
        } else {
            out.push(name);
        }
    }
    out.sort();
}

#[test]
fn four_files_and_no_database_give_exactly_the_two_notes_a_person_approved() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let block = block(root.path());

    for sentinel in [IN_1, IN_2] {
        assert!(
            block.text.contains(sentinel),
            "{sentinel} says `status: in-use` in its own file and there is no database here at \
             all. If it is missing, the status was being read from something other than the \
             file. The block reads:\n{}",
            block.text
        );
    }
    for sentinel in [SUG_1, SUG_2] {
        assert!(
            !block.text.contains(sentinel),
            "{sentinel} says `status: suggested` in its own file and it reached the prompt \
             anyway. The block reads:\n{}",
            block.text
        );
    }
    assert_eq!(
        block.used.len(),
        2,
        "two notes are in use, so two are used: {:?}",
        block.used
    );

    let mut left_behind = Vec::new();
    every_file(root.path(), "", &mut left_behind);
    assert_eq!(
        left_behind,
        vec![
            "notes/always-take-the-fast-path.md".to_owned(),
            "notes/retry-the-flaky-suite.md".to_owned(),
            "notes/tenant-before-guard.md".to_owned(),
            format!("notes/{NEWER}.md"),
        ],
        "reading the notes wrote something. A scan that leaves an index, a cache or a temporary \
         file behind has a second place where the status can live — and the second place is the \
         one that survives `rm loadout.db` with the wrong answer"
    );
}

#[test]
fn a_key_and_a_kind_this_version_does_not_know_are_carried_not_refused() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let notes = scan_notes(root.path()).expect("one unreadable file must not empty the section");
    assert_eq!(
        notes.len(),
        4,
        "all four notes come back. A strict parser drops the one from a newer Loadout and the \
         person sees a shorter list, not an error (invariant 5). Came back: {:?}",
        notes
            .iter()
            .map(|note| note.id.to_string())
            .collect::<Vec<_>>()
    );

    let newer = notes
        .iter()
        .find(|note| note.id == NoteId(NEWER.to_owned()))
        .expect("the scan lost the note written by a newer Loadout entirely");

    assert_eq!(
        newer.kind,
        Kind::Other(UNKNOWN_KIND.to_owned()),
        "`kind: {UNKNOWN_KIND}` is not one of the three [T6 §10.3], and that is not an error — \
         it is a file from a newer Loadout or one somebody edited by hand. It is carried as \
         itself, so the value is still in the file after we write it back"
    );
    assert_eq!(
        newer.extra.get(UNKNOWN_KEY).map(String::as_str),
        Some("1"),
        "the key nobody knows keeps its value. Dropping it silently means the next write erases \
         a field somebody else's Loadout depends on. `extra` holds {:?}",
        newer.extra
    );
    assert_eq!(
        newer.status,
        Status::InUse,
        "and none of that stopped the status from being read, which is the field this whole \
         criterion is about"
    );
}

#[test]
fn editing_one_line_by_hand_changes_the_next_prompt_in_the_same_process() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let before = block(root.path());
    assert!(
        before.text.contains(IN_2),
        "the fixture depends on this note being in the block to begin with"
    );

    // Ręczna edycja: jedna linia, w edytorze, bez udziału Loadouta i bez czyszczenia
    // czegokolwiek. Dokładnie to robi człowiek, który rozmyślił się co do notatki.
    let path = root.path().join("notes").join(format!("{NEWER}.md"));
    let text = fs::read_to_string(&path).unwrap();
    let edited = text.replace("status: in-use", "status: suggested");
    assert_ne!(
        edited, text,
        "the test has to actually change the line it claims to change"
    );
    fs::write(&path, edited).unwrap();

    let after = block(root.path());

    assert!(
        !after.text.contains(IN_2),
        "the file now says `suggested` and the block still carries it. This is the whole \
         difference between a status read from the file and a status remembered from the first \
         scan — and it is the same difference as between surviving `rm loadout.db` and not. \
         The block reads:\n{}",
        after.text
    );
    assert!(
        after.text.contains(IN_1),
        "the note nobody touched is still there, so this is a filter reading the files, not a \
         cache that gave up on everything at once"
    );
    assert_eq!(
        after.used,
        vec![NoteId("tenant-before-guard".to_owned())],
        "one note in use, one note in the receipt"
    );
}
