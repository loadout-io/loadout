//! AC-3 dla T-92: kandydatkę da się odrzucić, notatki w użyciu — nie tą drogą.
//!
//! Pamięć ma od T-17 dokładnie **jedno** wejście dla decyzji człowieka i jest nim „tak"
//! (`promote`). Makieta rysuje przy kandydatce dwie akcje (`docs/mockup/index.html:757`),
//! `src/sections/memory/index.tsx` i `mounted.test.tsx` zgłaszają brak drugiej od 2026-08-16,
//! a `MemoryState` zna `use`, `stopUsing` i `cancel` — czyli człowiek, któremu agent
//! zaproponował zdanie nieprawdziwe, nie ma ani jednej drogi, żeby to powiedzieć. Lista, z
//! której nic nie schodzi, przestaje być czytana, a wtedy bramka promocji jest rytuałem
//! [T6 §5.1]. Od tego zadania kandydatek przybywa **po każdym biegu** (AC-1), więc to
//! przestaje być brakiem wygody i staje się brakiem, który rośnie sam.
//!
//! # Cztery słabe wersje tego kryterium
//!
//! **Pierwsza: `assert!(discard(...).is_ok())`.** Przechodzi na implementacji, która woła
//! `fs::remove_file`. Nic w pamięci nie jest twardo usuwane [T6 §5.3]: zdanie skasowane
//! z dysku jest zdaniem, którego nikt nie umie ani odzyskać, ani wytłumaczyć następnemu
//! agentowi, który zaproponuje je drugi raz. Rozróżnia to asercja o pliku, który **leży**
//! w `discarded/`.
//!
//! **Druga: sprawdzenie samego `is_err()` przy notatce w użyciu.** Przechodzi na implementacji,
//! która przenosi plik i **potem** zwraca błąd — czyli zostawia człowieka bez notatki, o której
//! powiedziano mu, że jej nie ruszono. Rozróżnia to asercja, że plik po odmowie stoi tam, gdzie
//! stał, i dalej mówi `in-use`. Ta sama kolejność, którą `promote` ma opisaną w kontrakcie.
//!
//! **Trzecia: asercja na wariancie błędu zamiast na zdaniu.** `matches!(err, Error::StillInUse)`
//! przechodzi dla odmowy, której człowiek nigdy nie zobaczy, bo nie ma jej czym pokazać
//! (niezmiennik 29 — mechanizm istnieje, ekran o nim mówi, odbiorcy nie ma). Dlatego zdanie
//! sądzimy tam, gdzie wychodzi na drut: w [`discard_note_inner`], czyli w tym, co dostaje okno.
//!
//! **Czwarta: pominięcie długu z nagłówka `commands/memory.rs`.** Ten plik mówi od T-17 wprost,
//! że `stop_using_note_inner` „przy pierwszej okazji ma się przenieść do `memory::notes` obok
//! `promote`" (niezmiennik 23). [`discard`] jest trzecim wejściem, które musi wiedzieć, co
//! znaczy „ta notatka nie wchodzi do promptu" — a trzecia kopia tej wiedzy to już nie kopia,
//! tylko drugi zestaw reguł. Ostatni test pyta więc o `notes::stop_using` z nazwy.
//!
//! # Czego ten plik świadomie NIE uruchamia
//!
//! Tauri. Rejestracja komendy jest sądzona na **źródle**, dokładnie jak w
//! `ipc_commands_registered.rs` i z tego samego powodu: `Failed to launch` stoi na liście
//! `NOT_A_REAL_RED` w bramce, więc kryterium wymagające żywego okna nie umie być czerwone
//! z właściwego powodu. Zachowanie samej komendy jest sprawdzone o warstwę niżej, na prawdziwym
//! katalogu.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::memory::{NoteRefusal, discard_note_inner};
use loadout_lib::memory::notes::{
    Actor, DISCARDED_DIR, Error, NoteId, Status, discard, promote, scan_notes, stop_using,
};

/// Ten sam plik i to samo zdanie, co czyta `ipc_commands_registered.rs` po drugiej stronie.
const IPC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));

/// Jedyna lista nazw komend Loadouta. Czytają ją oba lustra — rustowe i okna.
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/commands.golden.txt"));

/// Nazwa komendy, którą woła przycisk „Discard".
const COMMAND: &str = "discard_note";

/// Chwila, w której człowiek kliknął. Podaje ją wołający — `memory::notes` nie ma zegara.
const CLICKED: &str = "2026-08-23T11:02:30Z";

/// Znacznik w tytule, żeby nazwa pliku w `discarded/` dała się rozpoznać bez zgadywania.
const CANDIDATE: &str = "the-tenant-is-resolved-before-the-guard";
const IN_USE: &str = "prompts-travel-on-stdin";

/// Plik notatki, wypisany co do bajtu — żaden nie powstał przez zapis Loadouta.
///
/// Pliki są prawdą (niezmiennik 4), a odrzucanie, które umie odrzucić wyłącznie to, co samo
/// przed chwilą zapisało, nie odpowiada na pytanie zadane przez tę sekcję.
fn note_file(status: &str, title: &str, rule: &str) -> String {
    format!(
        "---\n\
         scope: this-project\n\
         kind: rule\n\
         title: {title}\n\
         rule: {rule}\n\
         because: somebody watched this happen twice and wrote it down the second time\n\
         status: {status}\n\
         occurrences: 1\n\
         modified: 2026-08-22T09:00:00Z\n\
         last_used_at: null\n\
         ---\n"
    )
}

/// Korzeń pamięci z dwiema notatkami: jedną czekającą i jedną w użyciu.
fn a_memory_with_both() -> Result<tempfile::TempDir, Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("notes"))?;
    fs::write(
        root.path().join("notes").join(format!("{CANDIDATE}.md")),
        note_file(
            "suggested",
            "The tenant is resolved before the guard",
            "An unresolved tenant comes back as 401, not 400.",
        ),
    )?;
    fs::write(
        root.path().join("notes").join(format!("{IN_USE}.md")),
        note_file(
            "in-use",
            "Prompts travel on stdin",
            "The prompt never goes into argv, not even once.",
        ),
    )?;
    Ok(root)
}

/// Pliki leżące w tym katalogu, po nazwie i posortowane. Brak katalogu to pusta lista.
fn names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Identyfikatory notatek, które skan naprawdę oddaje.
fn scanned(root: &Path) -> Vec<String> {
    scan_notes(root)
        .expect("the notes root has to be readable")
        .into_iter()
        .map(|note| note.id.to_string())
        .collect()
}

#[test]
fn a_candidate_a_person_did_not_want_leaves_the_list_without_leaving_the_disk()
-> Result<(), Box<dyn StdError>> {
    let root = a_memory_with_both()?;
    let notes = root.path().join("notes");
    let gone = root.path().join(DISCARDED_DIR);

    let landed = discard(
        root.path(),
        &NoteId(CANDIDATE.to_owned()),
        Actor::You {
            at: CLICKED.to_owned(),
        },
    )?;

    // ── 1. Znika z listy ──────────────────────────────────────────────────────────────────
    assert_eq!(
        scanned(root.path()),
        vec![IN_USE.to_owned()],
        "the discarded candidate is still on the list the section reads. A `Discard` that leaves \
         the row where it was is a button that answers a click with nothing (invariant 16), and \
         the person clicks it again"
    );
    assert!(
        !notes.join(format!("{CANDIDATE}.md")).exists(),
        "the note file is still in notes/. Whatever else happened, the next scan will read it \
         back as a candidate: the list is the directory (invariant 4), not a column somewhere"
    );

    // ── 2. …i NIE znika z dysku ───────────────────────────────────────────────────────────
    //
    // To jest cała różnica między tym kryterium a `fs::remove_file`. Nic w pamięci nie jest
    // twardo usuwane [T6 §5.3]: zdanie skasowane jest zdaniem, którego nikt nie umie ani
    // odzyskać, ani wytłumaczyć następnemu agentowi, który zaproponuje je drugi raz.
    let left = names_in(&gone);
    assert_eq!(
        left.len(),
        1,
        "after discarding one candidate {} file(s) lie in {DISCARDED_DIR}/: {left:?}. Nothing \
         here is ever hard-deleted — a sentence removed from the disk cannot be brought back, \
         and cannot be shown to the next agent that proposes it a second time [T6 section 5.3]",
        left.len()
    );
    assert!(
        landed.is_file(),
        "discard() answered with {landed:?} and there is no file there. The path it returns is \
         the only thing a caller can put in front of a person who asks where their note went; a \
         path pointing at nothing is worse than no answer"
    );
    assert!(
        landed.starts_with(&gone),
        "the discarded note landed at {landed:?}, outside {gone:?}. It may not stay among the \
         notes: scan_notes reads that directory flat and would hand it back as a candidate on \
         the very next read"
    );

    // ── 3. Data w nazwie, żeby dwa odrzucenia tego samego tytułu się nie nadpisały ─────────
    //
    // Ten moduł nie ma zegara (nagłówek `notes.rs`), więc data jest tą, którą podał wołający.
    // Bez niej druga kandydatka o tym samym tytule kasuje pierwszą — czyli „nic nie jest
    // usuwane" przestaje być prawdą przy drugim kliknięciu, a nie przy pierwszym.
    let name = landed
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(
        name.contains("2026-08-23"),
        "the discarded note is called {name:?} and the day it was discarded is not in the name. \
         This module has no clock, so the moment comes from the caller ({CLICKED}); without it \
         the same title discarded twice overwrites itself and nothing is kept after all"
    );
    assert!(
        name.contains(CANDIDATE),
        "the discarded note is called {name:?} and does not carry the name it had. A person \
         looking for the sentence they threw away has the title and nothing else"
    );

    // ── 4. Treść dojechała w całości ──────────────────────────────────────────────────────
    let kept = fs::read_to_string(&landed)?;
    assert!(
        kept.contains("An unresolved tenant comes back as 401, not 400."),
        "the file in {DISCARDED_DIR}/ does not carry the sentence the note carried. Moving the \
         name and dropping the body keeps a receipt, not a note. It reads:\n{kept}"
    );

    Ok(())
}

#[test]
fn a_note_in_use_is_refused_in_the_words_a_person_reads() -> Result<(), Box<dyn StdError>> {
    let root = a_memory_with_both()?;
    let path = root.path().join("notes").join(format!("{IN_USE}.md"));
    let before = fs::read_to_string(&path)?;

    // Zdanie sądzimy tam, gdzie wychodzi NA DRUT — to jest jedyna droga, którą sekcja Pamięć
    // dowiaduje się o odmowie (`src/state/memory.ts`, pole `message`). Asercja na samym
    // wariancie błędu przechodzi dla odmowy, której człowiek nigdy nie zobaczy (niezmiennik 29).
    let refusal = discard_note_inner(root.path(), IN_USE, CLICKED)
        .expect_err("a note that goes into every prompt may not be thrown away by this road");

    let NoteRefusal::Said(sentence) = refusal else {
        panic!(
            "the refusal came back in the shape the section keeps for a full scope. \
             `Discard` on a note in use is an ordinary refusal with one sentence, not a forced \
             choice: the window would open a dialogue asking which notes to retire, which is \
             not the question the person asked"
        )
    };
    assert!(
        sentence.contains("Stop using it first"),
        "the sentence the window will show reads {sentence:?}. It has to say what to DO: \
         `Discard` and `Stop using` are two decisions and they stay two, so a person told only \
         that this cannot be done is left in front of a button that refuses and beside another \
         one that would have helped (invariant 14)"
    );

    // Odmowa PRZED pierwszym zapisem. Implementacja, która przenosi plik i dopiero potem wraca
    // błędem, przechodzi każde `is_err()` i zostawia człowieka bez notatki, o której powiedziano
    // mu, że jej nie ruszono.
    assert_eq!(
        fs::read_to_string(&path)?,
        before,
        "the refused note was rewritten anyway. Every refusal falls BEFORE the first write — \
         the same order `promote` has in its contract, and the only thing that tells the two \
         implementations apart"
    );
    assert!(
        names_in(&root.path().join(DISCARDED_DIR)).is_empty(),
        "a note in use was moved into {DISCARDED_DIR}/ despite the refusal. The refusal then \
         describes the world for exactly as long as nobody looks at the folder"
    );
    assert_eq!(
        scanned(root.path()),
        vec![CANDIDATE.to_owned(), IN_USE.to_owned()],
        "the list lost a note that was refused. Both are still there: the refusal is an answer, \
         not a half-done job"
    );

    Ok(())
}

#[test]
fn only_a_person_throws_a_note_away() -> Result<(), Box<dyn StdError>> {
    let root = a_memory_with_both()?;

    for by in [Actor::Agent("Scout".to_owned()), Actor::Loadout] {
        let refused = discard(root.path(), &NoteId(CANDIDATE.to_owned()), by.clone());
        assert!(
            matches!(refused, Err(Error::OnlyYouCanDoThat)),
            "{by:?} was allowed to throw a note away. The curator is one and it is the person — \
             the same rule that holds `promote`, read from the other side [ARCHITECTURE section \
             2 q. 5]. An agent that can delete somebody else's note can delete the one that \
             describes its own mistake. It answered {refused:?}"
        );
    }

    assert_eq!(
        scanned(root.path()),
        vec![CANDIDATE.to_owned(), IN_USE.to_owned()],
        "a refused discard took a note off the list anyway"
    );
    assert!(
        names_in(&root.path().join(DISCARDED_DIR)).is_empty(),
        "a refused discard moved the file anyway — the refusal fell after the first write"
    );

    Ok(())
}

#[test]
fn the_window_can_reach_this_by_name() -> Result<(), Box<dyn StdError>> {
    // Ta sama technika, co w `ipc_commands_registered.rs`: bez komentarzy, żeby zdanie
    // napisane w komentarzu nie liczyło się jak rejestracja (niezmiennik 20).
    let code: String = IPC
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(before, _)| before))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        code.contains("pub fn discard_note("),
        "there is no `discard_note` command in ipc.rs. The button in the section has nothing to \
         call, and a control without a handler does not enter the repo (invariant 16)"
    );

    let handler = code
        .split_once("generate_handler!")
        .map(|(_, after)| after.to_owned())
        .ok_or("ipc.rs carries no generate_handler! list")?;
    let registered = handler
        .split_once(']')
        .map_or(handler.as_str(), |(inside, _)| inside);
    assert!(
        registered.split(',').any(|name| name.trim() == COMMAND),
        "`{COMMAND}` exists in ipc.rs and is not in generate_handler!. That is the quietest \
         defect this seam has: the function is there, it is tested, `invoke` never reaches it, \
         and the window gets a refusal nobody can tie back to a missing line"
    );

    // Lustro komend jest JEDNO i czytają je obie strony granicy: `ipc_commands_registered.rs`
    // po stronie Rusta i `src/sections/commands-wired.test.ts` po stronie okna. Komenda
    // zarejestrowana i nieobecna na tej liście jest powierzchnią, o której front nie wie.
    let listed = GOLDEN.lines().map(str::trim).any(|line| line == COMMAND);
    assert!(
        listed,
        "`{COMMAND}` is not in commands.golden.txt. That file is the one list both sides read, \
         so a command missing from it is one the window has no legal way to name — and the \
         mirror on the other side refuses the row for it by design"
    );

    Ok(())
}

#[test]
fn both_directions_of_one_switch_live_in_one_file() -> Result<(), Box<dyn StdError>> {
    // Dług nazwany w nagłówku `commands/memory.rs` od T-17: „przy pierwszej okazji ma się
    // przenieść do `memory::notes` obok `promote`". [`discard`] jest trzecim wejściem, które
    // musi wiedzieć, co znaczy „ta notatka nie wchodzi do promptu" — a trzecia kopia tej wiedzy
    // to już nie kopia, tylko drugi zestaw reguł (niezmiennik 23).
    let root = a_memory_with_both()?;
    let id = NoteId(IN_USE.to_owned());

    let back = stop_using(root.path(), &id, CLICKED)?;
    assert_eq!(
        back.status,
        Status::Suggested,
        "notes::stop_using left the note in use. Both directions of one switch have to live in \
         one file: the word `suggested` written out in two places is how the second one drifts"
    );
    assert_eq!(
        back.modified, CLICKED,
        "the note came back stamped {:?} instead of the moment the person clicked. This module \
         has no clock and takes the moment from its caller",
        back.modified
    );

    // Odstawiona notatka JEST kandydatką, więc teraz da się ją odrzucić. To spina obie funkcje
    // w jedną drogę: „przestań używać", potem „odrzuć" — dwie decyzje, dwa kliknięcia.
    let landed = discard(
        root.path(),
        &id,
        Actor::You {
            at: CLICKED.to_owned(),
        },
    )?;
    assert!(
        landed.is_file(),
        "a note that was put back into waiting could not then be discarded. The refusal belongs \
         to notes that GO INTO a prompt, and this one no longer does"
    );

    // I kolejność jest częścią kontraktu: odstawienie notatki, która już nie jest w użyciu,
    // nie rusza pliku. Stempel `modified` za kliknięcie, które niczego nie zmieniło, jest
    // kłamstwem o tym, kiedy ta notatka ostatnio się zmieniła.
    let untouched: PathBuf = root.path().join("notes").join(format!("{CANDIDATE}.md"));
    let before = fs::read_to_string(&untouched)?;
    let same = stop_using(
        root.path(),
        &NoteId(CANDIDATE.to_owned()),
        "2026-08-24T08:00:00Z",
    )?;
    assert_eq!(
        same.status,
        Status::Suggested,
        "a note that was already waiting came back in another state"
    );
    assert_eq!(
        fs::read_to_string(&untouched)?,
        before,
        "stopping the use of a note that was not in use rewrote its file. `modified` is what a \
         person reads to tell what is fresh, and a stamp for a click that changed nothing is a \
         lie about exactly that"
    );

    // Kontrola: promocja dalej działa i dalej jest jedyną drogą do promptu. Bez tej linii
    // wszystko powyżej jest też prawdą o pliku, w którym `promote` przestało cokolwiek robić.
    let up = promote(
        root.path(),
        &id,
        Actor::You {
            at: CLICKED.to_owned(),
        },
    );
    assert!(
        up.is_err(),
        "the note that was just discarded came back into use. Its file is not in notes/ any \
         more, so there is nothing there to promote"
    );

    Ok(())
}
