//! Biblioteka Context: nazwane zestawy materiałów, które człowiek kuruje poza biegiem.
//!
//! `Knowledge` (umiejętności i pamięć) zostaje osobną szufladą i nic tu z niej nie przychodzi —
//! `docs/context-library/PLAN.md` §3 rozstrzyga to wprost: notatka wchodzi do KAŻDEGO promptu,
//! a zestaw jest materiałem, który człowiek WYBIERA do konkretnej pracy.
//!
//! **`SQLite` tej funkcji nie dotyka i dotykać nie ma** (niezmiennik 4: pliki są prawdą).
//! Zestaw żyje w dwóch plikach pod `<home>/contexts/`, więc skasowanie `loadout.db` nie zabiera
//! ani jednego zestawu — dowodzi tego `context_library_survives_restart::`.
//!
//! Ten etap (CT-01) zapisuje WYŁĄCZNIE materiał tekstowy. Kształt bytów jest przygotowany na
//! pliki i obrazy z CT-02 — [`ContextSource`] ma rodzaj — ale kodu dla rodzajów, których ten
//! etap nie obsługuje, tu nie ma: martwa gałąź jest obietnicą bez pokrycia.

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

pub mod files;

/// Wersja kształtu obu plików zestawu. Jedna, dopóki nie zajdzie potrzeba drugiej
/// (niezmiennik 25).
pub const SCHEMA: u32 = 1;

/// Katalog biblioteki wewnątrz `AppState.home`.
pub const LIBRARY_DIR: &str = "contexts";

/// Manifest zestawu — `manifest.json`.
///
/// TOŻSAMOŚCIĄ JEST `id`, nigdy tytuł ani nazwa katalogu. Slug w nazwie katalogu jest
/// wyłącznie po to, żeby człowiek rozpoznał folder w Finderze; zmiana tytułu go nie rusza,
/// bo przeniesienie katalogu przy każdej poprawionej literze zrywałoby każdą ścieżkę, którą
/// ktoś zdążył zapisać (PLAN §4).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSet {
    pub schema: u32,
    pub id: String,
    pub title: String,
    pub description: String,
    pub archived: bool,
    /// Ile razy szkic tego zestawu został zapisany. Rośnie monotonicznie i służy CZŁOWIEKOWI —
    /// warunkowy zapis rozstrzyga [`files::save_draft`] po bajtach, nie po tej liczbie.
    pub draft_revision: u64,
    /// Wersja gotowego opracowania. W CT-01 ZAWSZE `None`: opracowanie buduje CT-04, a pole
    /// zapisane „na przyszłość" z wartością byłoby zdaniem o czymś, czego nikt nie policzył.
    pub latest_ready_revision: Option<String>,
    pub created_at: String,
    pub changed_at: String,
}

/// Rodzaj materiału. CT-02 dokłada tu pliki i obrazy — addytywnie, bez ruszania czytelników.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Materiał wpisany w polu. Jedyny rodzaj, który ten etap zapisuje.
    #[default]
    Text,
    /// Rodzaj, którego ten build nie zna (niezmiennik 5). Nieznana wartość z pliku nie ma prawa
    /// zabrać całego zestawu — nowszy Loadout dopisze rodzaj, a starszy ma go po prostu minąć.
    #[serde(other)]
    Unknown,
}

/// Jedno źródło zestawu.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSource {
    pub id: String,
    pub kind: SourceKind,
    /// Nazwa, którą człowiek widzi na liście źródeł.
    pub name: String,
    /// Po co ten materiał tu leży — jedno zdanie człowieka, nie opis od modelu.
    pub description: String,
    /// Materiał tekstowy, słowo w słowo. CT-02 kładzie treść pliku obok, w katalogu źródła.
    pub text: String,
}

/// Szkic zestawu — `draft.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextDraft {
    pub schema: u32,
    /// Kolejność źródeł JEST kolejnością tej listy; osobne pole z porządkiem byłoby drugim
    /// miejscem na jeden fakt (niezmiennik 13).
    pub sources: Vec<ContextSource>,
    /// Identyfikatory źródeł wyłączonych z opracowania. Wyłączenie zostaje decyzją człowieka,
    /// więc przeżywa zapis — a samo źródło zostaje w zestawie.
    pub excluded: Vec<String>,
    /// Odpowiedź na pytanie `How should this context be prepared?`.
    pub how_to_prepare: String,
    /// Własne wymagania człowieka, zachowane co do brzmienia (PLAN §4).
    pub requirements: Vec<String>,
}

/// Zestaw odczytany w całości: manifest, szkic i rewizja, którą okno ma oddać przy zapisie.
///
/// Jeden odczyt, nie dwa. Manifest i szkic pobrane osobno mogą pochodzić z dwóch różnych chwil,
/// a wtedy rewizja opisuje bajty inne niż te, które człowiek widzi na ekranie.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSetRead {
    pub set: ContextSet,
    pub draft: ContextDraft,
    /// Rewizja bajtów `draft.json` — dokładnie ta, którą trzeba oddać przy następnym zapisie.
    pub revision: String,
}

/// Dlaczego zestawu nie da się zapisać albo odczytać.
///
/// Każdy wariant jest osobnym zdaniem po angielsku (D5, niezmiennik 14), bo każdy naprawia się
/// inaczej — a front nie ma prawa wyciągać sensu z surowego błędu.
#[derive(Debug)]
pub enum Error {
    /// Pliku nie ma tam, gdzie okno go czytało. **Nic nie zostało nadpisane.**
    Changed,
    /// Nie ma takiego zestawu w bibliotece.
    NoSuchSet,
    /// Zestaw jest, ale jego pliki nie są tym, co ten build umie przeczytać.
    Malformed(serde_json::Error),
    /// Dysk odmówił.
    Unwritable(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Trzy fakty w jednym zdaniu, bo człowiek potrzebuje wszystkich trzech naraz: że jego
            // zmiana NIE weszła, dlaczego, i że nic cudzego nie zginęło. To samo brzmienie, co
            // przy workflow (`workflow::file::SaveError::Changed`) — jedna odmowa, jedno zdanie.
            Self::Changed => formatter.write_str(
                "This context set was not saved: it changed on disk after you opened it, so \
                 nothing was overwritten. Open it again to see the newer one.",
            ),
            Self::NoSuchSet => {
                formatter.write_str("Loadout has no context set saved under that name.")
            }
            Self::Malformed(error) => write!(
                formatter,
                "This context set is not one Loadout can read: {error}."
            ),
            Self::Unwritable(error) => {
                write!(formatter, "This context set could not be saved: {error}.")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Unwritable(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Malformed(error)
    }
}
