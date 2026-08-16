//! Format pliku workflow — typy z T3 §3.1 i ani jednego pola więcej.
//!
//! To jest jedyna rzecz w Loadoucie, którą użytkownik może **stracić**: plik da się zmergować
//! gitem, poprawić ręcznie w edytorze i otworzyć raz nowszym buildem, raz starszym. Stąd dwie
//! decyzje, które w tym module wyglądają na drobiazgi, a są całą jego treścią:
//!
//! 1. **Nigdzie `deny_unknown_fields`** (T3 §8.4). Plik agenta pisze człowiek i literówka ma
//!    zaboleć od razu — tam `deny_unknown_fields` jest wymagane (T-11). Plik workflow pisze
//!    maszyna i ma przeżyć wersję, której nie zna, więc tutaj ta sama flaga byłaby błędem.
//! 2. **`#[serde(flatten)] extra` na każdym kroku.** Bez tego starszy build wczytuje plik
//!    z polem, którego nie zna, zapisuje go z powrotem i **kasuje pracę nowszego builda bez
//!    jednego komunikatu**. T3 §3.2 uruchomił to na tej maszynie: wewnętrznie tagowany enum
//!    z `flatten` przepuszcza nieznane klucze bez straty, razem z typem liczbowym.
//!
//! Czego tu nie ma i nie będzie: portów, typu krawędzi, warunku, węzła-grupy, pętli. Strzałka
//! znaczy „po" i nic więcej (T3 §6.2). Trzeci rodzaj kafelka jest tym, co zabiło poprzedniego prototypu.
//!
//! # Stan tego katalogu: SZKIELET (2026-08-16)
//!
//! Typy są kompletne — to one sprawiają, że kryteria się **kompilują** — a ciała funkcji
//! w `file.rs` i `check.rs` są `todo!()`. To jest wymagany kształt fazy kontraktu: test ma się
//! skompilować i paść **w czasie wykonania, na braku ZACHOWANIA**, bo test, który się nie
//! kompiluje, niczego nie uruchomił (`AGENTS.md` §2a p. 5, który wprost każe zacząć od
//! `todo!()`).
//!
//! Cena jest jedna i trzeba ją znać: `clippy::todo` jest `deny`
//! w `[workspace.lints.clippy]`, więc `./verify.sh quick` jest w tej fazie czerwony na tych
//! ciałach — i taki ma być, dopóki żadne z nich nie zostało napisane. `./verify.sh before`
//! uruchamia same kryteria i ta czerwień go nie dotyczy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod check;
pub mod file;

/// Skok siatki płótna w pikselach [T3 §8.2 reguła 1].
///
/// Pozycje zapisujemy jako całkowite wielokrotności tej liczby, bo `240.00000001` brudzi diff
/// przy każdym najechaniu myszą, a `240` nie brudzi go nigdy.
pub const GRID: f64 = 24.0;

/// Cały plik: `~/.loadout/workflows/<slug>.json`.
///
/// `format` jest **pierwszym** kluczem i czyta się go przed deserializacją całej reszty —
/// odmowa-w-przód z [`file::load`] musi zadziałać także wtedy, gdy nowszy build zmienił
/// kształt kroku tak, że ten build nie umie go wczytać.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowFile {
    /// Wersja formatu. Podnoszona **wyłącznie** przy zmianie łamiącej (T3 §8.4).
    pub format: u32,
    /// Stabilny identyfikator, nigdy nie zmieniany przy zmianie nazwy.
    pub id: String,
    /// To, co użytkownik wpisał: „Ship a feature".
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Kolejność **wstawiania**, nigdy przesortowana [T3 §8.2 reguła 2]: sortowanie
    /// topologiczne czyta się ładniej i przy wstawieniu kroku u góry przepisuje cały plik.
    pub steps: Vec<Step>,
    pub links: Vec<Link>,
    /// Klucze, których ta wersja nie zna — patrz `extra` na kroku.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Dwa rodzaje kafelka. To jest cała lista i ma taka zostać (D6, ARCHITECTURE §6b).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    Agent(AgentStep),
    Checkpoint(CheckpointStep),
}

/// Krok, który uruchamia agenta.
///
/// Vendora ani modelu tu nie ma: krok nazywa **agenta**, a vendor, model, narzędzia i tryb
/// uprawnień mieszkają w jego definicji (T3 §3.1). Zmiana modelu dzieje się raz, nie w sześciu
/// kafelkach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub id: String,
    /// Nazwa widoczna na kafelku — i **to samo zdanie**, którym uwaga z `check()` nazywa winnego.
    pub name: String,
    /// Id zapisanego agenta (`library::agents`).
    pub agent: String,
    /// Patch RFC 7396 nad definicją agenta: brak klucza znaczy „dziedzicz" [T4 §5.1].
    /// `{}` dla kroku nietkniętego.
    ///
    /// 2026-08-16 — surowa mapa, nie typ `Overrides`: typ, `resolve()` i `capture()` należą do
    /// T-11 (`library::agents`), którego w tym drzewie jeszcze nie ma. Ten moduł patcha
    /// **nie scala** — przenosi go z pliku do T-11 i z powrotem. Przy scalaniu T-11 to pole
    /// dostaje jego typ; drugiej implementacji scalania nie piszemy (TASK.md, rozstrzygnięcie 1).
    #[serde(default)]
    pub overrides: Map<String, Value>,
    /// Przelotka na opcje vendora: `"claude" -> {flaga: wartość}` (ARCHITECTURE §6b, D6).
    /// Loadout nie interpretuje zawartości — sprawdza tylko, czy nie podnosi tego, co ustawia
    /// sam. `BTreeMap`, nie `Value`: kolejność ma być deterministyczna, żeby zapis nie
    /// produkował fałszywych różnic w gicie.
    ///
    /// Jedyne pole spoza `Option`, które przy zapisie **znika, gdy jest puste**. `overrides: {}`
    /// niesie informację („ten krok nie jest nadpisany", T4 §5.1); pusta przelotka nie niesie
    /// żadnej i byłaby wierszem szumu w każdym kroku każdego pliku.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vendor_options: BTreeMap<String, BTreeMap<String, String>>,
    /// Ile identycznych sesji naraz, 1–8 [T3 §4.4]. Osiem jednoczesnych sesji na prawdziwej
    /// maszynie to już dużo.
    #[serde(default = "one_copy")]
    pub copies: u8,
    /// Prompt, zwykły tekst. `{{copy}}` i `{{copies}}` podstawia silnik [T3 §4.3].
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub skills: Skills,
    #[serde(default)]
    pub folder: Folder,
    /// Zostaje w schemacie bez kontrolki w UI: czyta je T-16, a edytor pól formularza jest
    /// odłożony (T3 §7.1).
    #[serde(default)]
    pub handover: Handover,
    /// Brak klucza znaczy `{"x":0,"y":0}`: plik poprawiony ręcznie ma się wczytać, a nie odmówić
    /// z powodu pozycji, którą płótno i tak umie ustawić.
    #[serde(default)]
    pub at: Point,
    /// Klucze, których **ta** wersja nie zna.
    ///
    /// 2026-08-16 — powód jest jednozdaniowy: starszy build nie kasuje pola nowszego. Bez tego
    /// jedno otwarcie w starszym Loadoucie zjada konfigurację, której nowszy build nie umie
    /// odtworzyć, i nie zostawia po tym ani jednego komunikatu [T3 §3.2, uruchomione].
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Domyślna liczba kopii. Funkcja, bo `#[serde(default)]` dla `u8` dałoby zero, a zero kopii
/// to krok, który nigdy nie biegnie.
fn one_copy() -> u8 {
    1
}

/// Krok, który zatrzymuje bieg i pyta człowieka [T3 §6.1 punkt 5].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointStep {
    pub id: String,
    pub name: String,
    /// „Does the plan look right?"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// Jak `AgentStep::at`.
    #[serde(default)]
    pub at: Point,
    /// Jak `AgentStep::extra`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Gdzie krok pracuje.
///
/// `fresh-copy` to obietnica izolacji z ARCHITECTURE §2 punkt 4 („każdy krok dostaje własną
/// kopię twoich plików"). Dlatego dwa kroki, które **mogą biec równocześnie** i celują w ten
/// sam folder, są odmową przy zapisie, a nie podpowiedzią (niezmiennik 12).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "use", rename_all = "kebab-case")]
pub enum Folder {
    /// Folder projektu, w którym biegnie workflow.
    #[default]
    Project,
    /// Własna kopia tylko dla tego kroku.
    FreshCopy,
    /// Wskazany ręcznie.
    Pick { path: String },
}

/// `"all"` albo lista nazw [T3 §3.1].
///
/// Znacznik jest osobnym typem, bo w enumie `untagged` wariant jednostkowy serializuje się jako
/// `null`, a format wymaga w tym miejscu stringa `"all"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EverySkill {
    #[default]
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Skills {
    Every(EverySkill),
    Only(Vec<String>),
}

impl Default for Skills {
    fn default() -> Self {
        Self::Every(EverySkill::All)
    }
}

/// `"notes"` — zwykła proza. Osobny typ z tego samego powodu, co [`EverySkill`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlainNotes {
    #[default]
    Notes,
}

/// Co krok przekazuje dalej [T3 §3.1].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Handover {
    Plain(PlainNotes),
    Form { fields: Vec<HandoverField> },
}

impl Default for Handover {
    fn default() -> Self {
        Self::Plain(PlainNotes::Notes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverField {
    pub name: String,
    pub describe: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Strzałka. Bez portów, bez danych, bez warunku — znaczy „po" (T3 §3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub from: String,
    pub to: String,
}

/// Pozycja kafelka na płótnie.
///
/// Pole jest `f64`, bo plik można poprawić ręcznie i przyjdzie stamtąd `241.4`. Zapisany tekst
/// ma jednak nieść **całkowitą wielokrotność [`GRID`]** — przyciąganie robi [`file::save`],
/// także wtedy, gdy zrobił je już frontend [T3 §8.2].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
