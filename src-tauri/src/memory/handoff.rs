//! Pliki przekazań: zapis, odczyt i skanowanie katalogu biegu.
//!
//! Jedyne miejsce w repo, które składa front-matter przekazania. Reguła, z której bierze się
//! całe to zadanie, stoi w `docs/ARCHITECTURE.md` §8 i w [T6 §10.2]: **front-matter pisze
//! Loadout, agent daje tylko treść.** Agent, który wymyśla własne metadane, zmyśli je.
//!
//! Z tego wynika kolejność, która nie jest kosmetyczna: metadane są **nadpisywane**, nigdy
//! scalane z tym, co przyszło w ciele. Scalanie wygląda identycznie w diffie i w UI, a różni
//! się tym, że `status`, `reads` i `id` zaczynają pochodzić od modelu. Sfałszowany blok
//! **zostaje w ciele** — kasowanie go ukryłoby próbę przed człowiekiem, który jako jedyny
//! może na nią zareagować.
//!
//! Czego tu nie ma:
//! - `Connection` — ten moduł zwraca strukturę, wiersz wkłada `store::writer` (niezmiennik 2);
//! - `#[cfg(unix)]` — ścieżki składamy `PathBuf`em, uprawnień nie tykamy (niezmiennik 3);
//! - ścieżki ani treści w argv — ciało jedzie do następnego kroku przez stdin (niezmiennik 9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::Result;

/// Twardy limit ciała **po normalizacji**, w bajtach [T6 §10.2, §4 „Context bloat"].
///
/// 8 KB ≈ 2 000 jednostek długości, czyli ~1% okna 200k. Liczba jest tu z powodem, a nie
/// dlatego, że ładnie wygląda: Anthropic mierzy 15× więcej długości w systemach
/// wieloagentowych niż w czacie [T6 §3.3], a cap na granicy agenta jest jedyną obroną,
/// która działa bez współpracy modelu. Ryzyko odwrotne — „cap ucina to jedno zdanie, dla
/// którego przekazanie powstało" — jest nazwane w [T6 §11.2] i dlatego cięcie idzie po
/// granicy sekcji, a pełny tekst zawsze ląduje w `attachments/`.
pub const BODY_CAP: usize = 8192;

/// Trzy sekcje o stałych nazwach i stałej kolejności [T6 §10.2].
///
/// `Answer` to jest to, czego potrzebuje następny agent; `Evidence` to `plik:linia` albo URL,
/// bo twierdzenie bez dowodu jest twierdzeniem; `Open` to nierozstrzygnięte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Answer,
    Evidence,
    Open,
}

/// Zamknięty zbiór siedmiu wartości plus wariant „coś nowego albo cudzego".
///
/// [`Kind::Other`] jest niezmiennikiem 5 zapisanym w typie: starszy albo nowszy Loadout,
/// ręczna edycja pliku, wpis z gałęzi, której jeszcze nie ma. Skan katalogu biegu nie ma
/// prawa się na tym przewrócić — jeden nieczytelny plik zamieniłby listę w UI w pustkę.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Brief,
    Findings,
    Plan,
    PatchSummary,
    Question,
    Answer,
    Review,
    Other(String),
}

/// Przekazania są niezmienne. Korekta to nowy plik, nie edycja starego [T6 §9].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Current,
    Superseded,
}

/// Co podaje wołający. Siedem pól — reszta front-mattera jest wyliczana przez Loadout
/// i wołający nie ma jak jej podać, właśnie o to chodzi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDraft {
    pub run: String,
    pub step: u32,
    pub from: String,
    pub to: Vec<String>,
    pub kind: Kind,
    pub title: String,
    /// Lista tego, co Loadout **faktycznie wstrzyknął** w prompt tego kroku — nie to, co agent
    /// twierdzi, że przeczytał. Pochodzenie, o którym nie da się skłamać [T6 §10.2].
    pub reads: Vec<String>,
}

/// Trzynaście pól kontraktu plus worek na to, czego kontrakt nie zna.
///
/// Wszystkie trzynaście musi dać się odczytać z samego pliku (niezmiennik 4). Pole, które
/// mieszka wyłącznie w wierszu `SQLite`, znika razem z `loadout.db` — a wtedy przekazanie
/// oznaczone jako zastąpione wraca do obiegu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub id: String,
    pub run: String,
    pub step: u32,
    pub from: String,
    pub to: Vec<String>,
    pub kind: Kind,
    pub title: String,
    pub status: Status,
    pub supersedes: Option<String>,
    pub reads: Vec<String>,
    pub created: String,
    /// Długość **zapisanego** ciała, tak jak stoi w pliku. Przy odczycie cudzego pliku bywa
    /// nieprawdą i wtedy jest to fakt do zaraportowania, nie do wygładzenia — patrz
    /// [`Handoff::bytes_mismatch`].
    pub bytes: usize,
    pub est_tokens: usize,
    /// Klucze spoza kontraktu, w kolejności z pliku. Niezmiennik 5 po stronie odczytu:
    /// `serde(deny_unknown_fields)` zamieniłby jeden ręcznie doklejony wiersz w pustą listę
    /// w UI. Klucz z ciała agenta tu **nie trafia** — ciało nigdy nie jest parsowane.
    pub extra: BTreeMap<String, String>,
}

/// Przekazanie odczytane z dysku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    pub path: PathBuf,
    pub meta: Meta,
    /// Ciało bez bloku front-mattera i bez wiersza separatora.
    pub body: String,
    /// Faktyczna długość ciała, policzona przy odczycie. Osobne pole, bo `meta.bytes` jest
    /// **deklaracją z pliku**, a te dwie liczby mają prawo się różnić.
    pub actual_bytes: usize,
}

impl Handoff {
    /// Czy plik kłamie o własnej długości.
    ///
    /// Cudzy plik (starszy Loadout, ręczna edycja, ucięty zapis) ma prawo tu trafić i nie jest
    /// błędem — ale przeliczenie `bytes` po cichu z zawartości zabrałoby jedyny sygnał, że coś
    /// się rozjechało.
    pub fn bytes_mismatch(&self) -> bool {
        todo!("T-16: meta.bytes != actual_bytes")
    }
}

/// Co powstało na dysku i co z tego wynika dla kroku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// `Some` **wyłącznie** wtedy, gdy w ciele stoi wskaźnik prowadzący do tego pliku
    /// (niezmiennik 21). Pełny tekst „na wszelki wypadek" przy ciele, którego nikt nie uciął,
    /// to artefakt, którego żaden skrypt nie czyta.
    pub attachment: Option<PathBuf>,
    /// Sekcje, których agent nie napisał, a Loadout je wstawił. Pusta lista znaczy, że ciało
    /// przyszło w umówionym kształcie — i to jest licznik, który warto oglądać [T6 §11.1].
    pub repaired: Vec<Section>,
    pub truncated: bool,
}

/// Składa front-matter, naprawia sekcje, pilnuje limitu i zapisuje plik w `run_dir/handoffs/`.
///
/// `agent_body` jest **danymi niezaufanymi**. Jedyne, co się z nim dzieje, to normalizacja
/// nowych linii, uzupełnienie brakujących nagłówków sekcji i ewentualne cięcie na granicy
/// sekcji. Nic z niego nie wpływa na ani jedno pole front-mattera.
pub fn write_handoff(_run_dir: &Path, _draft: MetaDraft, _agent_body: &str) -> Result<Written> {
    todo!("T-16: zapis przekazania")
}

/// Odczytuje jeden plik przekazania. Nieznany klucz i nieznany `kind` nie są błędem.
pub fn read_handoff(_path: &Path) -> Result<Handoff> {
    todo!("T-16: odczyt przekazania")
}

/// Czyta `run_dir/handoffs/` bez bazy i bez zaufania do tego, kto te pliki pisał.
///
/// Kolejność wynikowa jest kolejnością nazw plików, bo prefiks `NN` jest numerem kroku —
/// to jedyne uporządkowanie, które przeżywa skasowanie `loadout.db` (niezmiennik 4).
pub fn scan_run_dir(_run_dir: &Path) -> Result<Vec<Handoff>> {
    todo!("T-16: skan katalogu biegu")
}

/// Korekta: **nowy** plik z `supersedes: <old_id>`, a w starym zmienia się jedna linia.
///
/// Nadpisanie starego pliku w miejscu zabiera bieg historii i nikt tego nie zauważy, bo plik
/// dalej wygląda poprawnie [T6 §9]. Druga korekta tego samego `id` jest odmawiana
/// ([`super::Error::AlreadySuperseded`]) i nie zostawia po sobie ani jednego zapisu.
pub fn supersede(
    _run_dir: &Path,
    _old_id: &str,
    _draft: MetaDraft,
    _body: &str,
) -> Result<Written> {
    todo!("T-16: korekta przekazania")
}
