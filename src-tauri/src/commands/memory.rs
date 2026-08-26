//! Komendy pamięci: weź notatkę do użytku i przestań jej używać.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Cała polityka promocji mieszka w `memory::notes::promote` (T-17): wyłącznie człowiek,
//! „no because, no memory", budżet zakresu liczony z plików i wymuszony wybór zamiast cichego
//! przycięcia. Ta warstwa nie powtarza ani jednej z tych reguł.
//!
//! # Dług z 2026-08-16 spłacony 2026-08-23 (T-92)
//!
//! Do tego dnia stało tu ciało odstawienia notatki, bo `memory::notes` miało [`promote`] i nie
//! miało funkcji odwrotnej, a `src-tauri/src/memory/` leżało poza blokiem OWNS tamtego zadania
//! (`AGENTS.md` §7). Nagłówek nazywał to długiem wprost i wskazywał termin: „przy pierwszej
//! okazji ma się przenieść do `memory::notes` obok [`promote`], żeby oba kierunki jednego
//! przełącznika mieszkały w jednym pliku" (niezmiennik 23).
//!
//! Tą okazją jest `memory::notes::discard`: trzecie wejście, które musi wiedzieć, co znaczy
//! „ta notatka nie wchodzi do promptu". Cena rozdzielenia była wąska, dopóki kopia była jedna —
//! trzecia kopia tej wiedzy to już nie kopia, tylko drugi zestaw reguł. Od tej chwili wszystkie
//! trzy funkcje stoją w jednym pliku, a ta warstwa jest tym, czym miała być: skorupą, która
//! zamienia identyfikator na [`NoteId`], a błąd na zdanie dla okna.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::memory::notes::{
    Actor, Error, Note, NoteId, Scope, Status, discard, promote, scan_notes, stop_using,
};

/// Wartość `status:` dla notatki, która przestała wchodzić do promptu — **na drucie**.
///
/// Słowo z pliku, nie z ekranu: na dysku stoi `suggested`, a człowiek widzi `Suggested`
/// (`memory::notes::Status`). Stoi tu wyłącznie dlatego, że [`NoteWire`] jest lustrem dla okna
/// i musi nazwać stan tym samym napisem, którym nazywa go plik — zapisu notatki ta warstwa nie
/// zna od 2026-08-23 (patrz nagłówek modułu).
const SUGGESTED: &str = "suggested";

/// Notatka tak, jak widzi ją okno.
///
/// Lustro `Note` z `src/state/memory.ts`, pole w pole. Osobny typ od [`Note`], bo tamten nie
/// jest `Serialize` i nie ma nim być: `memory/notes.rs` nie należy do tego zadania, a
/// `#[derive(Serialize)]` dopisany tam zamroziłby jego pola jako kontrakt drutu przy okazji,
/// bez ani jednego kryterium, które by tego pilnowało.
///
/// Czego tu nie ma: `path` (okno nie dostaje ścieżek — katalog rozwiązuje Rust), `kind`,
/// `last_used_at` i `extra` (sekcja ich nie pokazuje, a pole, którego nikt nie czyta, jest
/// polem, które rozjedzie się pierwsze).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteWire {
    /// Nazwa pliku notatki bez rozszerzenia.
    pub id: String,
    pub title: String,
    /// Jedyna część notatki, która jedzie do promptu.
    pub rule: String,
    pub because: String,
    /// `suggested` albo `in-use` — to samo słowo, co w pliku.
    pub status: String,
    /// `everywhere`, `this-project` albo `this-agent`.
    pub scope: String,
    /// Czyja to wiedza. `null` znaczy „niczyja" i **jedzie na drut jako `null`**, a nie jako
    /// brak klucza (2026-08-22, T-80): zbiór kluczy tego typu jest porównywany z `Note`
    /// w `src/state/memory.ts` co do jednego (`ipc_read_paths`), a klucz, który raz jest,
    /// a raz go nie ma, znaczy tam, że okno i Rust zgadzają się tylko dla części notatek.
    pub agent: Option<String>,
    /// Z jakiego projektu ta notatka przyjechała; `null` znaczy „napisano ją tutaj".
    pub from: Option<String>,
    /// Ile ta notatka zabiera z budżetu zakresu. Na ekranie to słowo brzmi `length`
    /// (`DESIGN.md` §8), więc pole nazywa się tak samo — tłumaczenie w komponencie byłoby
    /// drugim miejscem, w którym mieszka nazwa tej liczby.
    pub length: usize,
    pub occurrences: u32,
    pub modified: String,
}

/// Fizyczne miejsce notatki. Legacy pozostaje notatką biblioteczną.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotePlace {
    Library,
    Project,
}

/// Pełna publiczna tożsamość notatki.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteAddress {
    pub place: NotePlace,
    pub id: String,
}

/// Notatka katalogowa wraz z adresem, spłaszczona na drucie do `{ place, id, ... }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogNoteWire {
    #[serde(flatten)]
    pub address: NoteAddress,
    #[serde(flatten)]
    pub note: NoteWire,
}

impl From<&Note> for NoteWire {
    fn from(note: &Note) -> Self {
        Self::from_note(note)
    }
}

impl NoteWire {
    fn from_note(note: &Note) -> Self {
        Self {
            id: note.id.to_string(),
            title: note.title.clone(),
            rule: note.rule.clone(),
            because: note.because.clone(),
            status: match note.status {
                Status::Suggested => SUGGESTED,
                Status::InUse => "in-use",
            }
            .to_owned(),
            scope: match note.scope {
                Scope::Everywhere => "everywhere",
                Scope::ThisProject => "this-project",
                Scope::ThisAgent => "this-agent",
            }
            .to_owned(),
            agent: note.agent.clone(),
            from: note.from.clone(),
            length: note.est_tokens,
            occurrences: note.occurrences,
            modified: note.modified.clone(),
        }
    }
}

impl CatalogNoteWire {
    fn at(note: &Note, place: NotePlace) -> Self {
        Self {
            address: NoteAddress {
                place,
                id: note.id.to_string(),
            },
            note: NoteWire::from_note(note),
        }
    }
}

/// Odmowa pamięci na drucie.
///
/// `untagged`, bo magazyn sekcji rozpoznaje „zakres jest pełny" **po kształcie**, a nie po
/// klasie błędu: przez granicę IPC jedzie zwykły obiekt, więc `instanceof` odpowiedziałby „nie"
/// na każdą odmowę i wymuszony wybór nigdy by się nie otworzył (`src/state/memory.ts`,
/// `isMemoryFull`). Zdanie zapakowane w tekst przy tej jednej odmowie zabrałoby ze sobą listę
/// do odstawienia — a ta lista przychodzi Z ODMOWY i tylko stamtąd [T6 §5.3].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum NoteRefusal {
    /// Zakres jest pełny: o ile za dużo i co człowiek może odstawić.
    #[serde(rename_all = "camelCase")]
    Full {
        over_by: usize,
        /// Najdawniej użyte pierwsze — kolejność jest treścią tej listy.
        retire: Vec<String>,
        /// Zdanie rdzenia, żeby sekcja miała co pokazać obok wyboru.
        message: String,
    },
    /// Wszystko inne: jedno zdanie po angielsku, napisane tam, gdzie powstała odmowa.
    Said(String),
}

impl From<Error> for NoteRefusal {
    fn from(error: Error) -> Self {
        match error {
            // Zdanie bierzemy przez `to_string()` PRZED rozłożeniem wariantu, żeby tekst
            // wymuszonego wyboru pozostał tym, który napisał rdzeń (niezmiennik 13).
            Error::MemoryFull {
                over_by,
                ref retire,
            } => {
                let message = error.to_string();
                Self::Full {
                    over_by,
                    retire: retire.iter().map(NoteId::to_string).collect(),
                    message,
                }
            }
            other => Self::Said(other.to_string()),
        }
    }
}

impl From<std::io::Error> for NoteRefusal {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error).into()
    }
}

/// Korzeń pamięci wewnątrz biblioteki: `~/.loadout/memory/` (`docs/ARCHITECTURE.md` §8).
///
/// [`scan_notes`] szuka plików w `<korzeń>/notes/`, więc korzeniem jest katalog **nad** nimi.
/// Odpowiedź na pytanie „gdzie leżą notatki" stoi tutaj raz, a nie w każdej skorupie osobno.
///
/// 2026-08-16 — zawsze korzeń globalny, nigdy projektowy. `<repo>/.loadout/memory/` istnieje
/// w `docs/ARCHITECTURE.md` §8 i sięgnięcie po niego wymaga wiedzy o otwartym projekcie, czyli
/// stanu aplikacji — a `lib.rs` ma w tym zadaniu mandat na jeden wiersz i `.manage()` się w nim
/// nie mieści. Notatki `this-project` leżą więc dziś w korzeniu globalnym i to jest brak, nie
/// decyzja: zakres notatki mieszka w jej front-matterze, nie w katalogu (`notes::Scope`), więc
/// nic się nie gubi — ale rozdziału na projekty jeszcze nie ma.
#[must_use]
pub fn notes_root(library: &Path) -> std::path::PathBuf {
    library.join("memory")
}

/// Korzeń pamięci jednego zwalidowanego projektu.
#[must_use]
pub fn project_notes_root(project: &Path) -> std::path::PathBuf {
    project.join(".loadout").join("memory")
}

/// Pełny katalog biblioteki i jednego projektu.
///
/// Oba skany używają jednego parsera, a miejsce jest dopinane dopiero w adapterze katalogu.
pub fn list_note_catalog_inner(
    library_root: &Path,
    project_root: &Path,
) -> Result<Vec<CatalogNoteWire>, Error> {
    let mut catalog: Vec<CatalogNoteWire> = scan_notes(library_root)?
        .iter()
        .map(|note| CatalogNoteWire::at(note, NotePlace::Library))
        .collect();
    catalog.extend(
        scan_notes(project_root)?
            .iter()
            .map(|note| CatalogNoteWire::at(note, NotePlace::Project)),
    );
    // 2026-08-26 (T-128): kolejność pełnego adresu jest stabilna także wtedy, gdy oba korzenie
    // zawierają ten sam id. Sortowanie po samym id zostawiałoby kolejność bliźniaków decyzji
    // systemu plików, a katalog jest odpowiedzią, którą magazyn podmienia w całości.
    catalog.sort_by(|left, right| left.address.cmp(&right.address));
    Ok(catalog)
}

/// Adresowane „Use this”; po mutacji wraca ponownie wyliczony cały katalog.
pub fn put_note_to_use_at_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    let (root, id) = ordinary_action(library_root, project_root, address)?;
    promote(root, &id, Actor::You { at: at.to_owned() })?;
    Ok(list_note_catalog_inner(library_root, project_root)?)
}

/// Adresowane „Stop using”; po mutacji wraca ponownie wyliczony cały katalog.
pub fn stop_using_note_at_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    let (root, id) = ordinary_action(library_root, project_root, address)?;
    stop_using(root, &id, at)?;
    Ok(list_note_catalog_inner(library_root, project_root)?)
}

/// Adresowane „Discard”; po mutacji wraca ponownie wyliczony cały katalog.
pub fn discard_note_at_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    let (root, id) = ordinary_action(library_root, project_root, address)?;
    discard(root, &id, Actor::You { at: at.to_owned() })?;
    Ok(list_note_catalog_inner(library_root, project_root)?)
}

/// Kopiuje wcześniejszą notatkę projektową z biblioteki do projektu bez nadpisania celu.
pub fn move_legacy_note_to_project_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    move_note_to_project_with_remover_inner(library_root, project_root, address, |source| {
        fs::remove_file(source)
    })
}

fn valid_id(address: &NoteAddress) -> Result<NoteId, NoteRefusal> {
    let normalized = crate::memory::slugify(&address.id);
    if normalized.is_empty() || normalized != address.id {
        return Err(NoteRefusal::Said(
            "That note address is not valid, so no file changed.".to_owned(),
        ));
    }
    Ok(NoteId(address.id.clone()))
}

fn note_at(root: &Path, id: &NoteId) -> Result<Note, NoteRefusal> {
    scan_notes(root)?
        .into_iter()
        .find(|note| note.id == *id)
        .ok_or_else(|| Error::NoSuchNote(id.clone()).into())
}

fn ordinary_action<'a>(
    library_root: &'a Path,
    project_root: &'a Path,
    address: &NoteAddress,
) -> Result<(&'a Path, NoteId), NoteRefusal> {
    let id = valid_id(address)?;
    let root = match address.place {
        NotePlace::Library => library_root,
        NotePlace::Project => project_root,
    };
    let note = note_at(root, &id)?;
    if address.place == NotePlace::Library && note.scope == Scope::ThisProject {
        return Err(NoteRefusal::Said(
            "Move this earlier note into the project before changing how it is used.".to_owned(),
        ));
    }
    Ok((root, id))
}

/// Wszystkie notatki z dysku, w kolejności, którą oddaje skaner.
///
/// 2026-08-18 — jedyna droga, którą sekcja Pamięć dowiaduje się, co leży w plikach. Do tego dnia
/// takiej drogi nie było wcale: magazyn startował pustą listą, a jedynym miejscem, w którym
/// notatka mogła się w nim pojawić, była odpowiedź na `put_note_to_use` — czyli na promocję
/// notatki, której ekran nigdy nie pokazał.
///
/// **Ani jednej linii skanowania tutaj** (niezmiennik 23). Czytnik notatek jest jeden i mieszka
/// w [`scan_notes`]: to on wie, że pliki leżą w `<korzeń>/notes/`, że biorą się wyłącznie te
/// z rozszerzeniem `.md`, że kolejność idzie po nazwach i że **korzeń bez katalogu notatek ma
/// zero notatek, a nie błąd**. Drugi czytnik w tej warstwie rozjechałby się z tamtym przy
/// pierwszej zmianie formatu i rozjazd byłby widoczny dopiero na ekranie użytkownika.
pub fn list_notes_inner(root: &Path) -> Result<Vec<NoteWire>, Error> {
    Ok(scan_notes(root)?.iter().map(NoteWire::from).collect())
}

/// Pełny katalog biblioteki i wskazanego projektu.
pub fn list_notes_for_project_inner(
    library_root: &Path,
    project_root: &Path,
) -> Result<Vec<CatalogNoteWire>, Error> {
    list_note_catalog_inner(library_root, project_root)
}

/// „Use this": od tej chwili notatka wchodzi do promptu.
///
/// `at` podaje wołający, bo `memory::notes` nie ma zegara i mieć nie będzie — to jest chwila,
/// w której **człowiek** kliknął, a nie moment, w którym plik dotarł na dysk.
pub fn put_note_to_use_inner(root: &Path, id: &str, at: &str) -> Result<NoteWire, NoteRefusal> {
    let note = promote(
        root,
        &NoteId(id.to_owned()),
        Actor::You { at: at.to_owned() },
    )?;
    Ok(NoteWire::from(&note))
}

/// Adresowana odmiana „Use this", zwracająca świeży pełny katalog tego samego projektu.
pub fn put_note_to_use_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    put_note_to_use_at_inner(library_root, project_root, address, at)
}

/// „Discard": kandydatka odchodzi do `<korzeń>/discarded/` i znika z listy.
///
/// 2026-08-23 (T-92) — do dziś pamięć miała **jedno** wejście dla decyzji człowieka i było nim
/// „tak". Makieta rysuje przy kandydatce dwie akcje (`docs/mockup/index.html:757`), sekcja
/// renderowała jedną, a `MemoryState` znało `use`, `stopUsing` i `cancel` — czyli człowiek,
/// któremu agent zaproponował zdanie nieprawdziwe, nie miał ani jednej drogi, żeby to
/// powiedzieć. Kandydatki zostawały na liście na zawsze, a lista, z której nic nie schodzi,
/// przestaje być czytana.
///
/// `at` podaje wołający, tak jak przy [`put_note_to_use_inner`]: to jest chwila, w której
/// **człowiek** kliknął, a `memory::notes` nie ma zegara i mieć nie będzie.
///
/// Cała polityka mieszka w [`discard`] (odmowa dla notatki w użyciu, przeniesienie zamiast
/// skasowania, wyłącznie [`Actor::You`]). Ta warstwa nie powtarza ani jednej z tych reguł.
pub fn discard_note_inner(root: &Path, id: &str, at: &str) -> Result<(), NoteRefusal> {
    discard(
        root,
        &NoteId(id.to_owned()),
        Actor::You { at: at.to_owned() },
    )?;
    Ok(())
}

/// Adresowana odmiana „Discard", zwracająca świeży pełny katalog tego samego projektu.
pub fn discard_note_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    discard_note_at_inner(library_root, project_root, address, at)
}

/// „Stop using": notatka zostaje na liście i przestaje wchodzić do promptu.
///
/// Odstawienie nie ma budżetu do sprawdzenia i nie ma czego odmówić: zbiór w użyciu tylko
/// maleje. Nie ma tu też pytania o [`Actor`] — reguła „tylko człowiek" broni WEJŚCIA do promptu
/// (ARCHITECTURE §2 pyt. 5), a wyjście z niego nie jest uprawnieniem, którego trzeba pilnować.
///
/// Cała polityka mieszka od 2026-08-23 w [`stop_using`], obok [`promote`] i [`discard`]
/// (nagłówek modułu). Ta warstwa zamienia identyfikator na [`NoteId`], a notatkę na drut.
pub fn stop_using_note_inner(root: &Path, id: &str, at: &str) -> Result<NoteWire, NoteRefusal> {
    let note = stop_using(root, &NoteId(id.to_owned()), at)?;
    Ok(NoteWire::from(&note))
}
/// Adresowana odmiana „Stop using", zwracająca świeży pełny katalog tego samego projektu.
pub fn stop_using_note_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    stop_using_note_at_inner(library_root, project_root, address, at)
}

/// Przenosi wcześniejszą notatkę projektową z biblioteki do projektu i oddaje pełny katalog.
pub fn move_note_to_project_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    move_legacy_note_to_project_inner(library_root, project_root, address)
}

/// Jawnie adresowana nazwa używana przez rdzeń i standalone oracle T-137.
pub fn move_note_to_project_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal> {
    move_note_to_project_inner(library_root, project_root, address)
}

/// Ten sam Move z wstrzykniętym ostatnim krokiem usuwania źródła.
///
/// Szew istnieje wyłącznie po to, by oracle mógł deterministycznie odmówić ostatniej operacji
/// i dowieść, że pełny cel jest już wtedy opublikowany. Polityka kopiowania i publikacji nadal
/// należy do tej funkcji; callback nie jest wyrocznią dla żadnego wcześniejszego kroku.
pub fn move_note_to_project_with_remover_inner<F>(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    remove_source: F,
) -> Result<Vec<CatalogNoteWire>, NoteRefusal>
where
    F: FnOnce(&Path) -> std::io::Result<()>,
{
    let id = valid_id(address)?;
    if address.place != NotePlace::Library {
        return Err(NoteRefusal::Said(
            "Only an earlier project note can be moved into this project.".to_owned(),
        ));
    }
    let legacy = note_at(library_root, &id)?;
    if legacy.scope != Scope::ThisProject {
        return Err(NoteRefusal::Said(
            "This note already belongs in the shared library, so it was not moved.".to_owned(),
        ));
    }

    let source_dir = library_root.join("notes");
    let source = source_dir.join(format!("{id}.md"));
    let target_dir = project_root.join("notes");
    let target = target_dir.join(format!("{id}.md"));
    match fs::symlink_metadata(&target) {
        Ok(_) => {
            return Err(NoteRefusal::Said(
                "This project already has a note with that name, so neither copy changed."
                    .to_owned(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Error::Io(error).into()),
    }

    let bytes = fs::read(&source)?;
    fs::create_dir_all(&target_dir)?;
    // 2026-08-26 (T-136): always-copy plus no-clobber makes the same promise on one volume
    // and across volumes; publishing the temporary file never overwrites a project note.
    let mut temporary = NamedTempFile::new_in(&target_dir)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&target).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            NoteRefusal::Said(
                "This project already has a note with that name, so neither copy changed."
                    .to_owned(),
            )
        } else {
            Error::Io(error.error).into()
        }
    })?;
    // The target is durable before the only destructive step. A failed unlink therefore
    // leaves two complete copies, while a successful unlink is made durable in its directory.
    fs::File::open(&target_dir)?.sync_all()?;
    remove_source(&source)?;
    fs::File::open(&source_dir)?.sync_all()?;
    Ok(list_note_catalog_inner(library_root, project_root)?)
}
