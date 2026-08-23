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

use serde::Serialize;

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

impl From<&Note> for NoteWire {
    fn from(note: &Note) -> Self {
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
