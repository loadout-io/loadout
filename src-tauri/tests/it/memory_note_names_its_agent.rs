//! AC-1 dla T-80: notatka umie powiedzieć, **czyja jest**.
//!
//! `Scope::ThisAgent` stoi w `memory/notes.rs` od T-17 razem z własnym sufitem 800 jednostek
//! i przez cały ten czas nie miał po czym filtrować: `Note` nie ma pola wskazującego agenta,
//! a lista `KNOWN` zna dziewięć kluczy i żaden nim nie jest. Notatka o zakresie `this-agent`
//! nie umie dziś powiedzieć, do kogo należy — więc trzeci blok nie miał jak powstać, a sufit
//! nikogo nigdy nie ograniczył. To jest pierwsza rzecz do zrobienia i cała reszta tego zadania
//! na niej stoi.
//!
//! **Słabą wersją tego kryterium jest `note.extra.get("agent")`.** Przechodzi DZISIAJ, bez ani
//! jednej linii implementacji: nieznany klucz front-mattera wraca przez `Note::extra`
//! (niezmiennik 5), więc test pytający o `extra` mierzy mechanizm, który już istnieje, i mówi
//! „zielone" o rzeczy, której nie ma. Dlatego każde zdanie niżej pyta o **pole notatki**,
//! a `extra` jest sprawdzane wyłącznie jako to, w czym tego klucza już być NIE MA.
//!
//! **Drugą słabą wersją jest sam odczyt.** Implementacja, która klucz czyta, a przy zapisie go
//! nie odtwarza, gubi właściciela przy pierwszym drugim zgłoszeniu tej samej kandydatki —
//! notatka zostaje na dysku, dalej ma zakres `this-agent` i od tej chwili nie jest niczyja.
//! Rozróżnia to test o drugim zgłoszeniu, który czyta **surowy plik** po zapisie.
//!
//! **Trzecią jest cicha degradacja.** Notatka `this-agent` bez nazwy agenta ma być odmową
//! zapisu, a nie notatką projektu: „pojechało do wszystkich kroków w tym projekcie" wygląda
//! na ekranie identycznie jak „zapisano", a różni się tym, do ilu promptów wchodzi. Ten sam
//! kierunek błędu, co przy `scope_from`, gdzie wartość nieczytelna schodzi do węższego zakresu,
//! nigdy do szerszego.
//!
//! Pliki są wypisane co do bajtu i żaden nie powstał przez zapis Loadouta: skan, który czyta
//! wyłącznie to, co sam zapisał, nie odpowiada na to pytanie ani trochę.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `panic` z tego samego powodu: `unwrap_or_else(|| panic!(…))` i `else { panic!(…) }` NIOSĄ tu
// zdanie, po którym poznaje się, czego zabrakło. Bez nich zostaje bezimienne „unwrap on a None",
// czyli komunikat, który nie mówi ani czego szukano, ani gdzie.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::memory::notes::{
    Kind, Note, NoteDraft, NoteId, Scope, Status, record_candidate_for, scan_notes,
};

/// Agent, do którego należy zasiana notatka. Tak, jak zapisałby to człowiek w pliku.
const OWNER: &str = "backend-dev";

/// Klucz, którego ta wersja Loadouta nie zna. Ma przeżyć każdy zapis (niezmiennik 5).
const STRANGER_KEY: &str = "x-future";

/// Notatka jednego agenta i notatka niczyja — dwie odpowiedzi, które muszą się różnić.
const MINE: &str = "ZEBU-BELONGS-TO-BACKEND";
const NOBODYS: &str = "ZEBU-BELONGS-TO-NOBODY";

/// Tytuł zasianej notatki. Nazwa jej pliku jest jego znormalizowaną postacią, więc drugie
/// zgłoszenie tego samego tytułu trafia w ten sam plik.
const PLANTED_TITLE: &str = "The tenant is resolved before the guard";
const PLANTED_ID: &str = "the-tenant-is-resolved-before-the-guard";

/// Kandydatka, której na dysku jeszcze nie ma.
const FRESH_TITLE: &str = "Migrations are additive";
const FRESH_ID: &str = "migrations-are-additive";
const FRESH_RULE: &str = "ZEBU-FRESH a migration that drops a column is never additive.";

const AT: &str = "2026-08-22T09:15:00Z";

/// Dwa pliki, wypisane co do bajtu. Pierwszy należy do agenta i niesie klucz od nowszego
/// Loadouta; drugi nie należy do nikogo i nie ma prawa dostać właściciela.
const FILES: [(&str, &str); 2] = [
    (
        PLANTED_ID,
        "---\n\
         scope: this-agent\n\
         agent: backend-dev\n\
         kind: rule\n\
         x-future: 1\n\
         title: The tenant is resolved before the guard\n\
         rule: ZEBU-BELONGS-TO-BACKEND an unresolved tenant surfaces as 401, not 400.\n\
         because: run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88\n\
         status: in-use\n\
         occurrences: 1\n\
         modified: 2026-08-20T10:31:02Z\n\
         last_used_at: null\n\
         ---\n\
         \n\
         How to apply: read the middleware before blaming the guard.\n",
    ),
    (
        "the-index-is-disposable",
        "---\n\
         scope: this-project\n\
         kind: fact\n\
         title: The index is disposable\n\
         rule: ZEBU-BELONGS-TO-NOBODY delete the index whenever it is in the way.\n\
         because: it was rebuilt from 5000 notes in about 200 ms on this machine\n\
         status: in-use\n\
         occurrences: 1\n\
         modified: 2026-08-20T11:02:00Z\n\
         last_used_at: null\n\
         ---\n\
         \n\
         How to apply: never write a field you cannot rebuild from the files.\n",
    ),
];

fn plant(root: &Path) {
    let notes = root.join("notes");
    fs::create_dir_all(&notes).unwrap();
    for (name, body) in FILES {
        fs::write(notes.join(format!("{name}.md")), body).unwrap();
    }
}

/// Notatka o tym identyfikatorze, przeczytana z DYSKU przez zwykły skan.
fn read_back(root: &Path, id: &str) -> Note {
    scan_notes(root)
        .expect("the scan fell over on a directory it did not write (invariant 5)")
        .into_iter()
        .find(|note| note.id == NoteId(id.to_owned()))
        .unwrap_or_else(|| panic!("no note called {id} came back from the scan"))
}

fn draft(title: &str, rule: &str, scope: Scope) -> NoteDraft {
    NoteDraft {
        title: title.to_owned(),
        rule: rule.to_owned(),
        because: "the same run reproduced it twice, and the second time somebody was watching"
            .to_owned(),
        scope,
        kind: Kind::Rule,
        status: Status::Suggested,
        at: AT.to_owned(),
    }
}

/// Nazwy plików leżących w katalogu notatek, posortowane. Odmowa ma nie zostawiać po sobie
/// niczego, a listing jest jedyną rzeczą, która to widzi.
fn listing(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("notes"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn raw(root: &Path, id: &str) -> String {
    let path: PathBuf = root.join("notes").join(format!("{id}.md"));
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} could not be read: {error}", path.display()))
}

#[test]
fn the_scan_reads_the_name_of_the_agent_a_note_belongs_to() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let mine = read_back(root.path(), PLANTED_ID);

    assert_eq!(
        mine.agent.as_deref(),
        Some(OWNER),
        "the file says this note belongs to {OWNER} and the scan came back with {:?}. Until a \
         note can say WHOSE it is, the third scope has nothing to filter by: `this-agent` has \
         had a ceiling of its own since T-17 and it has never limited anybody, because no step \
         could tell which notes were its own. Its rule reads: {}",
        mine.agent,
        mine.rule
    );
    assert!(
        mine.rule.contains(MINE),
        "the fixture lost the note this test is about; the scan came back with: {}",
        mine.rule
    );
    assert!(
        !mine.extra.contains_key("agent"),
        "the agent is part of the contract now, so it is no longer a key nobody knows. Leaving \
         it in `extra` means the answer to \"whose note is this\" lives in the bag of things we \
         carry without understanding — and every reader has to know that. `extra` holds {:?}",
        mine.extra
    );
    assert_eq!(
        mine.extra.get(STRANGER_KEY).map(String::as_str),
        Some("1"),
        "and the key nobody knows is still a stranger, carried as itself. That one has not \
         become part of the contract, and dropping it silently erases a field somebody else's \
         Loadout depends on (invariant 5). `extra` holds {:?}",
        mine.extra
    );

    let nobodys = read_back(root.path(), "the-index-is-disposable");
    assert_eq!(
        nobodys.agent, None,
        "this note has no owner in its file, and a note that belongs to the whole project must \
         not be given one. An implementation that answers this question with a constant, or with \
         the last name it saw, passes the assertion above and fails here — and after it, every \
         project note reaches exactly one agent. It came back owned by {:?}, and its rule reads: \
         {}",
        nobodys.agent, nobodys.rule
    );
    assert!(
        nobodys.rule.contains(NOBODYS),
        "the fixture lost the second note; the scan came back with: {}",
        nobodys.rule
    );
}

#[test]
fn a_note_written_for_an_agent_carries_that_name_back_to_disk() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let written = record_candidate_for(
        root.path(),
        draft(FRESH_TITLE, FRESH_RULE, Scope::ThisAgent),
        Some(OWNER),
    )
    .expect("a note that names its agent has to be writable; this one was refused");

    assert_eq!(
        written.agent.as_deref(),
        Some(OWNER),
        "the note came back from the writer without the agent it was written for"
    );
    assert!(
        raw(root.path(), FRESH_ID).contains(&format!("agent: {OWNER}")),
        "the name has to be IN THE FILE, on its own line of the front-matter. A value that lives \
         only in the struct the writer returned is gone the moment anybody reads the directory \
         again — and the file is the truth here, not the index (invariant 4). The file reads:\n{}",
        raw(root.path(), FRESH_ID)
    );

    // Przez dysk, nie przez wartość zwróconą: wołający ma dostać dokładnie to, co przeczyta
    // następny skan, a nie to, co przed chwilą złożono w pamięci.
    let again = read_back(root.path(), FRESH_ID);
    assert_eq!(
        again.agent.as_deref(),
        Some(OWNER),
        "the scan read the file back and the owner was not there"
    );
    assert_eq!(
        again.scope,
        Scope::ThisAgent,
        "and the scope is still the one the draft asked for. A writer that quietly widens the \
         scope of a note it could not place is the failure this criterion exists to stop"
    );
    assert_eq!(
        again.status,
        Status::Suggested,
        "and it is a candidate, not something in use: only a person puts a note to use \
         (ARCHITECTURE §2 q. 5), and naming an agent is not that person"
    );
}

#[test]
fn the_second_sighting_keeps_the_agent_and_the_key_nobody_knows() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());

    let again = record_candidate_for(
        root.path(),
        draft(
            PLANTED_TITLE,
            "ZEBU-SECOND the same thing, seen twice.",
            Scope::ThisAgent,
        ),
        Some(OWNER),
    )
    .expect("the same candidate reported twice must land in the same file, not be refused");

    let file = raw(root.path(), PLANTED_ID);
    assert!(
        file.contains(&format!("agent: {OWNER}")),
        "the second sighting rewrote the file and the owner did not survive the write. From \
         that moment the note still has the scope of one agent and belongs to nobody, which is \
         the state this whole criterion exists to make impossible. The file reads:\n{file}"
    );
    assert!(
        file.contains(&format!("{STRANGER_KEY}: 1")),
        "and the key nobody knows survived too. A write that only reproduces the fields it \
         understands erases the ones it does not, and the loss shows up in somebody else's \
         Loadout (invariant 5). The file reads:\n{file}"
    );
    assert!(
        file.contains("occurrences: 2"),
        "and this really was the same file seen a second time, or the two assertions above are \
         true of a file nobody touched. The file reads:\n{file}"
    );
    assert!(
        file.contains("status: in-use"),
        "and reporting it again did not put it to use by itself: repetition is a signal for a \
         person, never a decision made for them [T6 §5.3]. The file reads:\n{file}"
    );
    assert_eq!(
        again.agent.as_deref(),
        Some(OWNER),
        "and what came back agrees with the file"
    );
}

#[test]
fn a_note_for_one_agent_that_names_no_agent_is_refused_and_nothing_is_written() {
    let root = tempfile::tempdir().unwrap();
    plant(root.path());
    let before = listing(root.path());

    let refusal = record_candidate_for(
        root.path(),
        draft(FRESH_TITLE, FRESH_RULE, Scope::ThisAgent),
        None,
    );

    let Err(error) = refusal else {
        panic!(
            "a note for one agent that names no agent was WRITTEN. There is no third answer \
             here: either it says whose it is, or it does not exist. Writing it as a project \
             note is the quiet version of the same failure — the sentence then reaches every \
             step in this project, and nothing on any screen says the scope was widened"
        );
    };

    let said = error.to_string();
    assert!(
        said.to_lowercase().contains("agent"),
        "the refusal has to name the thing that is missing, in the word the file uses for it. \
         A person who is told only that the note could not be saved cannot fix it, and a person \
         who is told nothing at all clicks again. It said: {said}"
    );
    assert_eq!(
        listing(root.path()),
        before,
        "and the refusal happened BEFORE the first write. A refusal that leaves a file behind \
         passes every `is_err()` in the world and leaves a note on disk that nobody asked for \
         — the directory listing is the only thing that tells the two apart"
    );

    let after = scan_notes(root.path()).expect("the scan fell over after a refusal");
    assert!(
        !after
            .iter()
            .any(|note| note.id == NoteId(FRESH_ID.to_owned())),
        "and nothing of it reached the scan under any scope. Degrading to `this-project` is not \
         a milder version of this refusal: it is the same note delivered to more prompts than \
         anybody agreed to. The scan came back with: {:?}",
        after.iter().map(|note| &note.id).collect::<Vec<_>>()
    );
}
