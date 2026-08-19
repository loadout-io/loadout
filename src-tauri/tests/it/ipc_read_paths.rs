//! AC-5 dla T-38: notatki i umiejętności mają komendę odczytu, a plik z dysku wraca przez nią.
//!
//! Do 2026-08-18 nie istniała ani `list_notes`, ani `list_skills`. Skutek był zmierzony i miał
//! jedno zdanie: `install_skill` pisało na dysk, okno nigdy tego nie odczytywało, więc licznik
//! „N saved" pokazywał wyłącznie to, co dodano w TEJ sesji, a zainstalowana umiejętność znikała
//! po restarcie. To jest niezmiennik 4 złamany wprost — pliki są prawdą, a ekran mówił co innego.
//!
//! # Słaba wersja tego kryterium i co ją od niego odróżnia
//!
//! Słaba wersja brzmi `assert!(list_notes_inner(&root).is_ok())` albo, jeszcze słabiej, asercja,
//! że funkcja `list_notes_inner` w ogóle istnieje. `checks/quick-wired.sh` opisuje, dlaczego to
//! nie wystarcza: element `pub` używany wyłącznie z `tests/` nie jest dla clippy martwym kodem,
//! więc mechanizm bez wołającego przechodzi każdą bramkę, jaką mamy. Odróżniają je trzy rzeczy,
//! wszystkie niżej:
//!
//! - **plik pisze test, czyta produkcja.** Żaden przypadek nie zaczyna się od `record_candidate`
//!   ani od `review_skill_inner` — bajty wchodzą na dysk literałem, więc odczyt, który czyta
//!   tylko to, co sam zapisał, nie ma szansy przejść;
//! - **zbiór pól porównujemy z LUSTREM TS**, przeczytanym z `src/state/*.ts` w tym samym biegu.
//!   Wypisany tutaj literał zamrażałby to, co akurat zwraca Rust, i milczałby o polu, którego
//!   okno oczekuje, a Rust go nie oddaje — czyli o dokładnie tym defekcie, o który tu chodzi;
//! - **rejestrację sprawdzamy po OBU stronach szwu** (`generate_handler!` i `commands.golden.txt`),
//!   bo komenda opisana w jednym z tych dwóch miejsc jest niewywoływalna albo niewidoczna, a oba
//!   przypadki są ciche.
//!
//! Ten plik jest **modułem celu `it`**, nie własnym celem (`src-tauri/tests/it/main.rs`), więc
//! bez wiersza `mod ipc_read_paths;` tam nie uruchomiłby ani jednego testu i wyglądałby jak
//! zestaw, który przeszedł.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::memory::{NoteWire, list_notes_inner, notes_root};
use loadout_lib::commands::skills::{InstalledWire, Landing, install_skill_into, list_skills_in};

/// Plik, w którym stoi jedyna lista `generate_handler!`.
const IPC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));

/// Jedyna lista nazw komend. Ten sam plik czyta lustro po stronie okna.
const GOLDEN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/commands.golden.txt"));

/// Lustro `NoteWire` po stronie okna.
const MEMORY_TS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../src/state/memory.ts"
));

/// Lustro `InstalledWire` po stronie okna.
const SKILLS_TS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../src/state/skills.ts"
));

/// Dwie komendy, o które to kryterium pyta.
const READ_COMMANDS: [&str; 2] = ["list_notes", "list_skills"];

/// Notatka wypisana co do bajtu. Nie powstaje przez `record_candidate`: odczyt, który czyta
/// wyłącznie to, co sam zapisał, nie odpowiada na pytanie „czy plik wraca przez komendę".
const NOTE_FILE: &str = "\
---
scope: this-project
kind: fact
title: The window never read the disk
rule: OKAPI-READ a section that only remembers this session lies after every restart.
because: measured 2026-08-18 — install_skill wrote files nothing ever read back
status: in-use
occurrences: 2
modified: 2026-08-18T09:15:00Z
last_used_at: 2026-08-18T09:20:00Z
---

How to apply: give every write path a read path in the same task.
";

/// Nazwa pliku notatki bez rozszerzenia — ona jest jej identyfikatorem.
const NOTE_ID: &str = "the-window-never-read-the-disk";

/// Umiejętność, która przyszła z linku: leży jako kopia kanoniczna, zanim człowiek zatwierdzi.
const FROM_THE_LINK: &str = "pdf";
/// Umiejętność napisana ręcznie prosto w katalogu vendora — bez kopii kanonicznej.
const BY_HAND: &str = "release-notes";

fn skill_md(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads {name} files and says what is in them.\n---\n\n\
         # {name}\n\nRead the file before you answer.\n"
    )
}

// ── CZYTANIE WYROCZNI ──────────────────────────────────────────────────────────────────────

/// Źródło bez komentarzy liniowych.
///
/// Bez tego zdanie o `generate_handler!` napisane w komentarzu liczyłoby się jak rejestracja —
/// czyli dokładnie ten incydent, który `AGENTS.md` (niezmiennik 20) nazywa po imieniu.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        out.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        out.push('\n');
    }
    out
}

/// Nazwy komend naprawdę zarejestrowanych, ostatni człon ścieżki modułu.
fn registered() -> BTreeSet<String> {
    let code = without_comments(IPC);
    let Some(after) = code.split_once("generate_handler!").map(|(_, rest)| rest) else {
        return BTreeSet::new();
    };
    let Some(open_at) = after.find('[') else {
        return BTreeSet::new();
    };
    let Some(close_at) = after[open_at..].find(']').map(|at| at + open_at) else {
        return BTreeSet::new();
    };

    after[open_at + 1..close_at]
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.rsplit("::").next().unwrap_or(item).trim().to_owned())
        .collect()
}

/// Nazwy ze złotej listy.
fn on_the_golden_list() -> BTreeSet<String> {
    GOLDEN
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

/// Pola interfejsu `TypeScript` o tej nazwie, przeczytane ze źródła w tym samym biegu testu.
///
/// Wypisanie ich literałem tutaj zamrażałoby to, co akurat zwraca Rust — a pytanie brzmi
/// odwrotnie: czy Rust oddaje to, czego oczekuje okno. Wiersze komentarza odpadają po pierwszym
/// znaku, bo wewnątrz bloku `/** … */` każdy zaczyna się od gwiazdki albo ukośnika.
fn ts_fields(source: &str, interface: &str) -> BTreeSet<String> {
    let opening = format!("export interface {interface} {{");
    let Some(body) = source.split_once(&opening).map(|(_, rest)| rest) else {
        return BTreeSet::new();
    };
    // Interfejsy w tych dwóch plikach są płaskie, więc pierwsza klamra zamykająca w pierwszej
    // kolumnie kończy ciało. Szukanie po samym `}` ucięłoby je na klamrze z komentarza.
    let Some(end) = body.find("\n}") else {
        return BTreeSet::new();
    };

    body[..end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('*') && !line.starts_with('/'))
        .filter_map(|line| line.split_once(':').map(|(name, _)| name))
        .map(|name| name.trim_end_matches('?').trim().to_owned())
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .collect()
}

/// Klucze, którymi ta wartość pojedzie przez drut.
fn wire_keys<T: serde::Serialize>(value: &T) -> BTreeSet<String> {
    let json = serde_json::to_value(value).expect("the wire type has to be serialisable");
    json.as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

// ── PRZYGOTOWANIE DYSKU ────────────────────────────────────────────────────────────────────

/// Biblioteka Loadouta wewnątrz świeżego katalogu tymczasowego.
///
/// `~/.loadout` leży **w** katalogu domowym, więc rodzic biblioteki jest tym katalogiem — i to
/// z niego `list_skills_in` wyprowadza katalogi vendorów. Struktura tutaj jest więc taka
/// sama jak u użytkownika, tylko korzeń inny.
fn library(home: &Path) -> PathBuf {
    home.join(".loadout")
}

/// Katalogi, w które instalacja naprawdę pisze.
fn vendor_dirs(home: &Path) -> [PathBuf; 2] {
    loadout_lib::skills::place::destinations(loadout_lib::skills::Scope::Global, home, None)
}

/// Kopia kanoniczna, tak jak zostawia ją `review_skill_inner`.
fn plant_canonical(library: &Path, name: &str) {
    let dir = library.join("skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), skill_md(name)).unwrap();
}

// ── (a) OBIE STRONY SZWU ───────────────────────────────────────────────────────────────────

#[test]
fn both_read_commands_are_registered_and_on_the_golden_list() {
    let live = registered();
    let listed = on_the_golden_list();

    // Kontrola przeciw pustemu parserowi: bez niej dwa puste zbiory zgodziłyby się ze sobą,
    // a asercja niżej przeszłaby na `generate_handler!`, którego nikt nie sparsował.
    assert!(
        live.len() >= 10 && listed.len() >= 10,
        "the two sides of the seam were read as {} registered and {} listed names. Loadout has \
         had more than ten commands since T-27, so a number this small means the parser above \
         found nothing and the comparison below would agree with an empty handler",
        live.len(),
        listed.len()
    );

    for name in READ_COMMANDS {
        assert!(
            live.contains(name),
            "`{name}` is not in generate_handler! in src/ipc.rs, so `invoke('{name}', …)` from \
             the window is refused. The section then shows what this session added instead of \
             what is on disk, which is invariant 4 broken in the one place a person can see it. \
             Registered: {live:?}"
        );
        assert!(
            listed.contains(name),
            "`{name}` is missing from src-tauri/commands.golden.txt. That list is the one place \
             where both sides of the seam agree on a name, and a command absent from it is a \
             command the window was never told about. Listed: {listed:?}"
        );
    }
}

// ── (b) NOTATKA Z DYSKU WRACA PRZEZ KOMENDĘ ────────────────────────────────────────────────

#[test]
fn a_note_written_to_disk_comes_back_with_the_fields_the_window_expects() {
    let home = tempfile::tempdir().unwrap();
    let root = notes_root(&library(home.path()));
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes").join(format!("{NOTE_ID}.md")), NOTE_FILE).unwrap();

    let notes =
        list_notes_inner(&root).expect("a directory with one readable note is not a failure");

    assert_eq!(
        notes.len(),
        1,
        "one note file is on disk and the read path brought back {} of them. Zero is the defect \
         this criterion exists to end: the section starts empty and nothing in production fills \
         it, so a note approved yesterday is invisible today",
        notes.len()
    );
    let note: &NoteWire = notes.first().unwrap();

    assert_eq!(
        note.id, NOTE_ID,
        "the id of a note is its file name without the extension"
    );
    assert_eq!(
        note.title, "The window never read the disk",
        "the title came back as something other than the line in the file, so the read path is \
         not reading that file"
    );
    assert_eq!(
        note.status, "in-use",
        "the file says `status: in-use` and there is no database here at all. Any other answer \
         means the status was read from something that is not the file (invariant 4)"
    );
    assert_eq!(
        note.scope, "this-project",
        "the file says `scope: this-project`; the scope decides which budget the note spends"
    );
    assert_eq!(
        note.occurrences, 2,
        "the file says `occurrences: 2`; that number is a signal a person reads before approving"
    );

    // Zbiór pól czytamy z LUSTRA, a nie z literału: pytanie brzmi „czy Rust oddaje to, czego
    // oczekuje okno", a literał odpowiadałby wyłącznie na „czy Rust oddaje to, co oddawał".
    let expected = ts_fields(MEMORY_TS, "Note");
    assert!(
        expected.len() >= 5 && expected.contains("status"),
        "src/state/memory.ts was parsed into {expected:?} — that is not the `Note` interface. \
         Comparing two empty sets passes on nothing at all, so the mirror has to be read before \
         it can judge anything"
    );
    assert_eq!(
        wire_keys(note),
        expected,
        "list_notes hands the window a different set of fields than src/state/memory.ts declares \
         for `Note`. A field the window reads and Rust never sends arrives as `undefined` and \
         renders as an empty cell; a field Rust sends and the window never reads is a field \
         nobody keeps honest"
    );
}

// ── (c) UMIEJĘTNOŚĆ ZAPISANA PRZEZ INSTALACJĘ WRACA PO PONOWNYM OTWARCIU KATALOGU ───────────

#[test]
fn a_skill_that_install_wrote_comes_back_from_the_directory_and_not_from_memory() {
    let home = tempfile::tempdir().unwrap();
    let library = library(home.path());

    // 1. Nic nie zainstalowano: pusta lista, nie błąd.
    let before =
        list_skills_in(&library, None).expect("nothing installed yet is a state, not a failure");
    assert!(
        before.is_empty(),
        "a library where nobody installed anything answered {before:?}. A read path that invents \
         rows shows the person skills their agents cannot see"
    );

    // 2. Prawdziwa instalacja, tą samą funkcją, którą woła komenda `install_skill`.
    plant_canonical(&library, FROM_THE_LINK);
    install_skill_into(&library, FROM_THE_LINK, Landing::Everywhere, None)
        .expect("a reviewed skill installs");

    let after =
        list_skills_in(&library, None).expect("reading an installed skill is not a failure");
    assert_eq!(
        after
            .iter()
            .map(|one| one.name.as_str())
            .collect::<Vec<_>>(),
        vec![FROM_THE_LINK],
        "install_skill wrote '{FROM_THE_LINK}' to disk and the read path does not bring it back. \
         That is the whole defect: the window remembers what this session added, the files hold \
         the truth, and after a restart the two disagree"
    );
    assert!(
        after
            .first()
            .is_some_and(|one: &InstalledWire| one.from_the_internet),
        "'{FROM_THE_LINK}' came in through a link, so the marker that stands in for the \
         signatures v1 does not have has to survive the restart. It came back as {after:?}"
    );

    // 3. Umiejętność napisana ręcznie prosto w katalogu vendora: widoczna, bo agent ją widzi,
    //    i BEZ znacznika, bo nikt jej nie pobierał.
    let by_hand = vendor_dirs(home.path())[0].join(BY_HAND);
    fs::create_dir_all(&by_hand).unwrap();
    fs::write(by_hand.join("SKILL.md"), skill_md(BY_HAND)).unwrap();

    let both = list_skills_in(&library, None).expect("two skills on disk are two skills");
    assert_eq!(
        both.iter()
            .map(|one| (one.name.as_str(), one.from_the_internet))
            .collect::<Vec<_>>(),
        // Kolejność jest alfabetyczna i to jest część odpowiedzi: lista, której porządek zależy
        // od tego, co system plików akurat oddał pierwsze, tasuje się między dwoma wejściami
        // w sekcję i człowiek szuka wiersza tam, gdzie go widział ostatnio.
        vec![(FROM_THE_LINK, true), (BY_HAND, false)],
        "a skill written by hand straight into the agent directory is installed as far as the \
         agent is concerned, so it belongs on the list — and it never came from the internet, so \
         it must not carry that marker. Answer: {both:?}"
    );

    // 4. Nie ma tu żadnej pamięci: zdejmujemy katalogi z dysku i lista pustoszeje. Bez tego kroku
    //    kryterium przechodziłoby też na implementacji, która odpowiada z tego, co zapamiętała
    //    przy instalacji — czyli na dokładnie tej wadzie, którą to zadanie naprawia.
    for dir in vendor_dirs(home.path()) {
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
    }
    let gone = list_skills_in(&library, None).expect("an emptied directory is not a failure");
    assert!(
        gone.is_empty(),
        "the agent directories are empty and the read path still answered {gone:?}. It is \
         answering from something it remembered, so the screen would go on showing a skill that \
         no agent can load"
    );
}

#[test]
fn an_installed_skill_carries_the_fields_the_window_expects() {
    let home = tempfile::tempdir().unwrap();
    let library = library(home.path());
    plant_canonical(&library, FROM_THE_LINK);
    install_skill_into(&library, FROM_THE_LINK, Landing::Everywhere, None)
        .expect("a reviewed skill installs");

    let installed =
        list_skills_in(&library, None).expect("reading an installed skill is not a failure");
    let one = installed
        .first()
        .expect("the skill that was just installed has to be on the list");

    let expected = ts_fields(SKILLS_TS, "InstalledSkill");
    assert!(
        expected.len() >= 2 && expected.contains("name"),
        "src/state/skills.ts was parsed into {expected:?} — that is not the `InstalledSkill` \
         interface, and two empty sets agree about nothing"
    );
    assert_eq!(
        wire_keys(one),
        expected,
        "list_skills hands the window a different set of fields than src/state/skills.ts declares \
         for `InstalledSkill`. The marker `fromTheInternet` is the one that matters here: missing, \
         it arrives as `undefined`, which is falsy — so every skill pulled off a link would show \
         up as if a person had written it themselves"
    );
}

// ── (d) KATALOG, KTÓREGO NIE MA ────────────────────────────────────────────────────────────

#[test]
fn a_directory_that_is_not_there_is_an_empty_list_and_never_an_error() {
    let home = tempfile::tempdir().unwrap();
    // Nic nie tworzymy. Tak wygląda pierwsze uruchomienie Loadouta u człowieka, który jeszcze
    // niczego nie zapisał — i tak samo wygląda katalog biblioteki po `rm -rf`.
    let library = library(home.path());
    let root = notes_root(&library);

    assert!(
        !root.exists() && !library.exists(),
        "this case is only worth anything on a directory that really is not there"
    );

    let notes = list_notes_inner(&root).expect(
        "a missing notes directory has to be zero notes. An error here paints the Memory section \
         red on a fresh install, and a red bar a person sees on day one is a red bar they learn \
         to ignore",
    );
    assert!(
        notes.is_empty(),
        "a directory that does not exist answered with {notes:?} notes"
    );

    let skills = list_skills_in(&library, None).expect(
        "a missing agent directory has to be zero skills. Nothing installed yet is a state, not \
         a failure",
    );
    assert!(
        skills.is_empty(),
        "a directory that does not exist answered with {skills:?} skills"
    );
}
