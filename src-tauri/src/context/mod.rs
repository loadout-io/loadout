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
//! CT-01 zapisywał wyłącznie materiał tekstowy. CT-02 dokłada do tego PLIKI: wklejone obrazy,
//! pliki z dysku i przygotowany PDF. Dokładanie jest addytywne — stary `draft.json` nie zna
//! ani jednego z nowych pól i ma się dalej czytać (niezmiennik 5), więc każde z nich niesie
//! `#[serde(default)]`.

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

pub mod access;
pub mod build;
pub mod files;
pub mod findings;
pub mod limits;
pub mod prompt;
pub mod sources;

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

/// Rodzaj materiału.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Materiał wpisany w polu albo wklejony jako tekst.
    #[default]
    Text,
    /// Obraz: wklejony screenshot albo `PNG`/`JPEG`/`WebP` z dysku.
    Image,
    /// PDF. Jedyny rodzaj, który przed użyciem wymaga osobnej fazy przygotowania.
    Pdf,
    /// Plik tekstowy albo Markdown wzięty z dysku — treść leży obok, w katalogu źródła.
    Document,
    /// Rodzaj, którego ten build nie zna (niezmiennik 5). Nieznana wartość z pliku nie ma prawa
    /// zabrać całego zestawu — nowszy Loadout dopisze rodzaj, a starszy ma go po prostu minąć.
    #[serde(other)]
    Unknown,
}

/// Plik zapisany w bibliotece — KOPIA, nigdy dowiązanie do miejsca importu.
///
/// Ścieżki, z której plik przyszedł, tu nie ma i mieć nie ma: po skopiowaniu nie jest do niczego
/// potrzebna (PLAN §4), a zapisana zamieniałaby skasowanie pliku z `Downloads` w zepsute źródło.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredFile {
    /// Ścieżka WZGLĘDEM katalogu zestawu — `sources/<id>/<revision>/original.<ext>`.
    ///
    /// Względna, bo katalog biblioteki bierze się z `AppState.home`: ścieżka bezwzględna
    /// zapisana w pliku przestaje być prawdą, kiedy ktoś przeniesie profil albo dysk.
    pub path: String,
    /// Rewizja ŹRÓDŁA, nie pliku definicji. Oryginał jest niezmienny, a podmiana materiału
    /// tworzy nową rewizję obok — nie nadpisuje tej, na którą ktoś już się powołał (PLAN §4).
    pub revision: String,
    pub mime: String,
    pub bytes: u64,
    /// Odcisk bajtów oryginału. Deduplikacja w obrębie zestawu porównuje właśnie jego.
    pub fingerprint: String,
    /// Ile ważą razem pochodne tego źródła — miniatura, wariant dla agenta i strony.
    /// Osobny budżet od oryginałów (PLAN §9), więc osobna liczba.
    #[serde(default)]
    pub derived: u64,
    /// Ile stron ma dokument. `None` dla obrazu, tekstu i dla PDF-a, którego jeszcze nikt nie
    /// otworzył — nie „0", bo zero stron jest zdaniem o dokumencie, którego nikt nie policzył.
    ///
    /// Liczba stoi TUTAJ, obok `bytes` i `mime`, bo jest faktem O PLIKU. [`Preparation`] mówi
    /// obok, ile z nich jest już gotowych — to jest postęp, nie ten sam fakt (niezmiennik 13).
    #[serde(default)]
    pub pages: Option<u32>,
}

/// Gdzie stoi przygotowanie tego źródła.
///
/// Przygotowanie jest OSOBNĄ FAZĄ IMPORTU (PLAN §5), nie częścią otwarcia ekranu — dlatego jego
/// stan mieszka w szkicu, a nie w pamięci okna: człowiek, który zamknął okno w połowie, ma po
/// powrocie zobaczyć, ile stron jest gotowych, i dokończyć od pierwszej brakującej.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Preparation {
    /// Nic do przygotowania: tekst i obraz są gotowe w chwili importu.
    #[default]
    NotNeeded,
    /// Zostały strony. `pages_done` jest liczbą stron GOTOWYCH po kolei, więc następna do
    /// zrobienia to `pages_done + 1` — i to jest cała odpowiedź na „od której wznowić".
    /// Ile stron ma cały plik, mówi obok [`StoredFile::pages`].
    ///
    /// `rename_all` STOI NA WARIANCIE, nie tylko na enumie, i to nie jest ostrożność na zapas:
    /// atrybut kontenera na enumie przepisuje NAZWY WARIANTÓW, a pola wariantu zostają wtedy
    /// w `snake_case`. Okno czyta `pagesDone`, więc bez tej linii postęp przyjeżdża pod nazwą,
    /// której front nie zna — a widać to dopiero na prawdziwej granicy.
    #[serde(rename_all = "camelCase")]
    Needs {
        pages_done: u32,
    },
    Ready,
    /// Plik zaszyfrowany, uszkodzony albo taki, którego ten build nie umie przeczytać.
    /// Niesie WŁASNE zdanie, bo udawany pusty dokument jest gorszy niż nazwana porażka.
    Failed {
        said: String,
    },
    /// Stan, którego ten build nie zna (niezmiennik 5).
    #[serde(other)]
    Unknown,
}

/// Jedno źródło zestawu.
///
/// `Default` jest tu z powodu, nie z wygody: [`ContextDraft`] go wyprowadza, a każde pole
/// dołożone przez CT-02 ma `#[serde(default)]`, żeby `draft.json` zapisany przez CT-01 dalej
/// się czytał. Konstruktory w testach opierają się o `..ContextSource::default()`, więc kolejne
/// pole nie przepisuje ich wszystkich.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSource {
    pub id: String,
    pub kind: SourceKind,
    /// Nazwa, którą człowiek widzi na liście źródeł.
    pub name: String,
    /// Po co ten materiał tu leży — jedno zdanie człowieka, nie opis od modelu.
    pub description: String,
    /// Materiał tekstowy, słowo w słowo. Przy źródle plikowym pusty — treść leży w pliku.
    pub text: String,
    /// Plik w bibliotece. `None` dla materiału wpisanego w polu.
    #[serde(default)]
    pub file: Option<StoredFile>,
    #[serde(default)]
    pub preparation: Preparation,
    /// Szew tekstu i obrazu z JEDNEGO wklejenia: obraz wskazuje tekst, obok którego stanął.
    /// Bez niego mieszany paste zostawia dwa źródła, o których nikt już nie wie, że są jednym
    /// materiałem (PLAN §5: „zachowuje oba oraz ich powiązanie").
    #[serde(default)]
    pub companion_of: Option<String>,
    /// Widoczne ograniczenia tego źródła — zdania po angielsku, gotowe do pokazania.
    /// Źródło z uwagą JEST w zestawie; uwaga mówi, czego o nim nie wiemy.
    #[serde(default)]
    pub notes: Vec<String>,
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

/// Kto wprowadził ustalenie do gotowej wersji.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Human,
    Generated,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Rodzaj jednego ustalenia w opracowaniu.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FindingKind {
    Requirement,
    Fact,
    VisualReference,
    Assumption,
    Question,
    Conflict,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Dokładne miejsce w źródle, z którego pochodzi ustalenie.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub part: String,
}

/// Jedno ustalenie zaakceptowane przez aplikację.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextFinding {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: FindingKind,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub condition: String,
    #[serde(default)]
    pub sources: Vec<SourceReference>,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub origin: Origin,
}

/// Temat wersji, wybrany z zaakceptowanych ustaleń.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextTopic {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

/// Wynik jednego fragmentu materiału.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceOutcome {
    Processed,
    Excluded,
    Failed,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Postęp jednego fragmentu źródła.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceProgress {
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub outcome: SourceOutcome,
    #[serde(default)]
    pub said: String,
}

/// Pięć etapów budowania i stany odczytane po jego końcu.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BuildStage {
    Freezing,
    Splitting,
    Extracting,
    Grouping,
    Publishing,
    Ready,
    Interrupted,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Końcowa wartość budowania. Anulowanie nie jest błędem (niezmiennik 7).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BuildEnd {
    Running,
    Ready,
    Cancelled,
    Failed,
    StillRunning,
    Interrupted,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Niezmienna, gotowa wersja opracowania.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextRevision {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub set_id: String,
    #[serde(default)]
    pub draft_revision: u64,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub topics: Vec<ContextTopic>,
    #[serde(default)]
    pub findings: Vec<ContextFinding>,
    #[serde(default)]
    pub questions: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub sources: Vec<SourceProgress>,
    #[serde(default)]
    pub index_file: String,
    #[serde(default)]
    pub findings_file: String,
    #[serde(default)]
    pub topic_files: Vec<String>,
}

/// Trwały stan jednej operacji z `builds/<id>/state.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextBuild {
    #[serde(default)]
    pub schema: u32,
    #[serde(default)]
    pub operation_id: String,
    #[serde(default)]
    pub set_id: String,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub draft_revision: u64,
    #[serde(default)]
    pub stage: BuildStage,
    #[serde(default)]
    pub end: BuildEnd,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub batches_done: usize,
    #[serde(default)]
    pub batches_total: usize,
    #[serde(default)]
    pub sources: Vec<SourceProgress>,
    #[serde(default)]
    pub said: String,
    #[serde(default)]
    pub revision_id: Option<String>,
    #[serde(default)]
    pub input_fingerprint: String,
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub changed_at: String,
}

/// To, co ekran dostaje przy odczycie budowania.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextBuildRead {
    #[serde(default)]
    pub build: Option<ContextBuild>,
    #[serde(default)]
    pub revision: Option<ContextRevision>,
    #[serde(default)]
    pub build_with: String,
}

/// Zmiana człowieka publikowana jako następna, niezmienna wersja.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RevisionEdit {
    #[serde(default)]
    pub correction: String,
    #[serde(default)]
    pub finding_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
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
    /// Zestaw jest, ale nie ma w nim źródła o tym identyfikatorze. Podgląd i usuwanie celują
    /// ZATWIERDZONYM identyfikatorem, nie ścieżką od okna, więc to jest jedyna droga pudła.
    NoSuchSource,
    /// Nie ma wskazanej operacji budowania.
    NoSuchBuild,
    /// Nie ma gotowej wersji, którą można poprawić.
    NoSuchRevision,
    /// Nazwana porażka budowania, zachowana także w `state.json`.
    BuildFailed(String),
    /// Zestaw jest, ale jego pliki nie są tym, co ten build umie przeczytać.
    Malformed(serde_json::Error),
    /// Dysk odmówił.
    Unwritable(io::Error),
    /// Nazwana odmowa biblioteki — limit, zły rodzaj pliku, spóźniony wynik przygotowania.
    /// Osobno od [`Self::Unwritable`], bo po niej człowiek ma co zrobić, a nie tylko co przeczytać.
    Refused(limits::Refusal),
    /// Nazwana odmowa ograniczonego czytelnika — zły identyfikator, kursor albo zakres.
    Denied(access::Denied),
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
            Self::NoSuchSource => {
                formatter.write_str("This context set holds nothing under that name any more.")
            }
            Self::NoSuchBuild => {
                formatter.write_str("Loadout has no saved context build under that name.")
            }
            Self::NoSuchRevision => {
                formatter.write_str("Build this context before saving a correction.")
            }
            Self::BuildFailed(said) => formatter.write_str(said),
            Self::Malformed(error) => write!(
                formatter,
                "This context set is not one Loadout can read: {error}."
            ),
            Self::Unwritable(error) => {
                write!(formatter, "This context set could not be saved: {error}.")
            }
            // Odmowa biblioteki niesie już całe zdanie. Opakowana w „could not be saved" mówiłaby
            // człowiekowi o dysku wtedy, gdy problemem jest jego plik.
            Self::Refused(refusal) => write!(formatter, "{refusal}"),
            // Czytelnik także niesie gotowe zdanie. Druga otoczka zgubiłaby rozróżnienie między
            // cudzym zakresem, wygasłym dostępem i zwykłym brakiem pliku.
            Self::Denied(denied) => write!(formatter, "{denied}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<limits::Refusal> for Error {
    fn from(refusal: limits::Refusal) -> Self {
        Self::Refused(refusal)
    }
}

impl From<access::Denied> for Error {
    fn from(denied: access::Denied) -> Self {
        Self::Denied(denied)
    }
}

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
