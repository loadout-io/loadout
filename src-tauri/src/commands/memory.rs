//! Komendy pamięci: weź notatkę do użytku i przestań jej używać.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Cała polityka promocji mieszka w `memory::notes::promote` (T-17): wyłącznie człowiek,
//! „no because, no memory", budżet zakresu liczony z plików i wymuszony wybór zamiast cichego
//! przycięcia. Ta warstwa nie powtarza ani jednej z tych reguł.
//!
//! # Czego rdzeń nie ma i co z tego wynika
//!
//! 2026-08-16 — `memory::notes` ma [`promote`] i **nie ma funkcji odwrotnej**. Sekcja Pamięć
//! ma przycisk `Stop using` od T-17 (`src/sections/memory/note-row.tsx`), magazyn ma `stopUsing`,
//! a po stronie Rusta nie ma czego zawołać. `src-tauri/src/memory/` nie jest w bloku OWNS tego
//! zadania, więc dopisanie tam `demote` byłoby pytaniem do człowieka (`AGENTS.md` §7), nie cichą
//! poprawką w cudzym pliku — i dlatego odstawienie stoi TUTAJ, w [`stop_using_note_inner`].
//!
//! Ta funkcja jest jedynym miejscem w tym module, które zna słowo `suggested` i kształt zapisu
//! notatki. To jest dług, nie wzorzec: przy pierwszej okazji ma się przenieść do `memory::notes`
//! obok [`promote`], żeby oba kierunki jednego przełącznika mieszkały w jednym pliku
//! (niezmiennik 23). Powód, dla którego to nie boli dzisiaj, jest wąski: format pliku należy
//! w całości do [`FrontMatter`], a odczyt idzie wyłącznie przez [`scan_notes`] — więc kopią jest
//! JEDNO słowo i nic poza nim.

use std::path::Path;

use serde::Serialize;

use crate::memory::FrontMatter;
use crate::memory::notes::{Actor, Error, Note, NoteId, Scope, Status, promote, scan_notes};

/// Wartość `status:` dla notatki, która przestała wchodzić do promptu.
///
/// Słowo z pliku, nie z ekranu: na dysku stoi `suggested`, a człowiek widzi `Suggested`
/// (`memory::notes::Status`). Drugie miejsce w drzewie, w którym ten napis stoi wypisany —
/// pierwsze jest prywatne w `memory::notes`. Powód stoi w nagłówku modułu.
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

/// Notatka o tym identyfikatorze, odczytana z dysku.
///
/// Przez [`scan_notes`], nie przez własny odczyt: czytnik notatek jest jeden i ma zostać jeden.
/// Wraca też `path`, więc ta warstwa nie musi znać nazwy katalogu, w którym leżą notatki.
fn on_disk(root: &Path, id: &NoteId) -> Result<Note, Error> {
    scan_notes(root)?
        .into_iter()
        .find(|note| &note.id == id)
        .ok_or_else(|| Error::NoSuchNote(id.clone()))
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

/// „Stop using": notatka zostaje na liście i przestaje wchodzić do promptu.
///
/// Odstawienie nie ma budżetu do sprawdzenia i nie ma czego odmówić: zbiór w użyciu tylko
/// maleje. Nie ma tu też pytania o [`Actor`] — reguła „tylko człowiek" broni WEJŚCIA do promptu
/// (ARCHITECTURE §2 pyt. 5), a wyjście z niego nie jest uprawnieniem, którego trzeba pilnować.
///
/// Powód, dla którego ta funkcja stoi tutaj, a nie obok [`promote`], jest w nagłówku modułu.
pub fn stop_using_note_inner(root: &Path, id: &str, at: &str) -> Result<NoteWire, NoteRefusal> {
    let id = NoteId(id.to_owned());
    let note = on_disk(root, &id)?;

    // Notatka, która już nie jest w użyciu: plik zostaje NIETKNIĘTY. Stempel `modified` za
    // kliknięcie, które niczego nie zmieniło, jest kłamstwem o tym, kiedy ta notatka ostatnio
    // się zmieniła — a to pole czyta człowiek, żeby wiedzieć, co jest świeże. Ta sama decyzja
    // stoi po drugiej stronie przełącznika, w `promote`.
    if note.status == Status::Suggested {
        return Ok(NoteWire::from(&note));
    }

    let raw = std::fs::read_to_string(&note.path).map_err(Error::Io)?;
    let (mut front, body_at) = FrontMatter::split(&raw).map_err(Error::Memory)?;

    // Dwie linie w pliku i ani jedna więcej. Złożenie front-mattera od nowa z tego, co ta
    // funkcja wie, przepisałoby pola, o które nikt jej nie pytał — razem z kluczami, których ta
    // wersja Loadouta nie zna (niezmiennik 5).
    front.set("status", SUGGESTED);
    // Data w jednej linii: wartość front-mattera nie ma prawa nieść znaku końca wiersza, bo
    // rozcięłaby nagłówek na dwa. `promote` robi to samo swoim prywatnym `one_line`.
    front.set(
        "modified",
        &at.split_whitespace().collect::<Vec<_>>().join(" "),
    );

    let mut out = front.render();
    let body = &raw[body_at..];
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
    }
    std::fs::write(&note.path, out).map_err(Error::Io)?;

    // Wracamy z tym, co LEŻY NA DYSKU, a nie z tym, co przed chwilą złożyliśmy w pamięci —
    // wołający dostaje wtedy dokładnie to, co przeczyta następny skan, i nie ma jak zobaczyć
    // notatki, której zapis po cichu nie doszedł. Ta sama reguła stoi na końcu `promote`.
    Ok(NoteWire::from(&on_disk(root, &id)?))
}
