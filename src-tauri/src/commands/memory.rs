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

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::memory::notes::{
    Actor, Error, Note, NoteId, RealMoveIo, Scope, Status, discard, move_note_file_with_io,
    promote, scan_notes, stop_using,
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
    /// Fizyczny korzeń pliku; z `id` tworzy pełną tożsamość widoczną dla okna.
    pub place: NotePlace,
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

impl From<&Note> for NoteWire {
    fn from(note: &Note) -> Self {
        Self::at(note, NotePlace::Library)
    }
}

impl NoteWire {
    fn at(note: &Note, place: NotePlace) -> Self {
        Self {
            place,
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
) -> Result<Vec<NoteWire>, Error> {
    let mut catalog: Vec<NoteWire> = scan_notes(library_root)?
        .iter()
        .map(|note| NoteWire::at(note, NotePlace::Library))
        .collect();
    catalog.extend(
        scan_notes(project_root)?
            .iter()
            .map(|note| NoteWire::at(note, NotePlace::Project)),
    );
    // 2026-08-26 (T-128): kolejność pełnego adresu jest stabilna także wtedy, gdy oba korzenie
    // zawierają ten sam id. Sortowanie po samym id zostawiałoby kolejność bliźniaków decyzji
    // systemu plików, a katalog jest odpowiedzią, którą magazyn podmienia w całości.
    catalog.sort_by(|left, right| (&left.place, &left.id).cmp(&(&right.place, &right.id)));
    Ok(catalog)
}

/// Adresowane „Use this”; po mutacji wraca ponownie wyliczony cały katalog.
pub fn put_note_to_use_at_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
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
) -> Result<Vec<NoteWire>, NoteRefusal> {
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
) -> Result<Vec<NoteWire>, NoteRefusal> {
    let (root, id) = ordinary_action(library_root, project_root, address)?;
    discard(root, &id, Actor::You { at: at.to_owned() })?;
    Ok(list_note_catalog_inner(library_root, project_root)?)
}

/// Kopiuje wcześniejszą notatkę projektową z biblioteki do projektu bez nadpisania celu.
pub fn move_legacy_note_to_project_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
) -> Result<Vec<NoteWire>, NoteRefusal> {
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

    let source = library_root.join("notes").join(format!("{id}.md"));
    let target = project_root.join("notes").join(format!("{id}.md"));
    let mut io = RealMoveIo;
    match move_note_file_with_io(&source, &target, &mut io) {
        Ok(()) => Ok(list_note_catalog_inner(library_root, project_root)?),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(NoteRefusal::Said(
                "This project already has a note with that name, so neither copy changed."
                    .to_owned(),
            ))
        }
        Err(error) => Err(error.into()),
    }
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
    catalog_folder: &Path,
) -> Result<Vec<NoteWire>, Error> {
    list_note_catalog_inner(library_root, &project_notes_root(catalog_folder))
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

/// Adapter adresowanej akcji dla wołających, którzy mają już rozwiązany korzeń projektu.
pub fn put_note_to_use_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    put_note_to_use_at_inner(library_root, project_root, address, at)
}

/// Publiczna nazwa adresowanej akcji używana przez katalog i jego acceptance oracle.
pub fn put_addressed_note_to_use_inner(
    library_root: &Path,
    catalog_folder: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    put_note_to_use_addressed_inner(
        library_root,
        &project_notes_root(catalog_folder),
        address,
        at,
    )
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

/// Adapter adresowanej akcji dla wołających, którzy mają już rozwiązany korzeń projektu.
pub fn discard_note_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    discard_note_at_inner(library_root, project_root, address, at)
}

/// Publiczna nazwa adresowanej akcji używana przez katalog i jego acceptance oracle.
pub fn discard_addressed_note_inner(
    library_root: &Path,
    catalog_folder: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    discard_note_addressed_inner(
        library_root,
        &project_notes_root(catalog_folder),
        address,
        at,
    )
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

/// Adapter adresowanej akcji dla wołających, którzy mają już rozwiązany korzeń projektu.
pub fn stop_using_note_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    stop_using_note_at_inner(library_root, project_root, address, at)
}

/// Publiczna nazwa adresowanej akcji używana przez katalog i jego acceptance oracle.
pub fn stop_using_addressed_note_inner(
    library_root: &Path,
    catalog_folder: &Path,
    address: &NoteAddress,
    at: &str,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    stop_using_note_addressed_inner(
        library_root,
        &project_notes_root(catalog_folder),
        address,
        at,
    )
}

/// Przenosi wcześniejszą notatkę projektową z biblioteki do projektu i oddaje pełny katalog.
pub fn move_note_to_project_inner(
    library_root: &Path,
    catalog_folder: &Path,
    address: &NoteAddress,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    move_note_to_project_addressed_inner(library_root, &project_notes_root(catalog_folder), address)
}

/// Adapter Move dla wołających, którzy mają już rozwiązany korzeń projektu.
pub fn move_note_to_project_addressed_inner(
    library_root: &Path,
    project_root: &Path,
    address: &NoteAddress,
) -> Result<Vec<NoteWire>, NoteRefusal> {
    move_legacy_note_to_project_inner(library_root, project_root, address)
}
