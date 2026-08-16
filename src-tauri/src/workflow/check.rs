//! „Czy to da się uruchomić?" — raport, nie boolean [T3 §5.2].
//!
//! Frontend odpowiada na inne pytanie („czy da się narysować tę strzałkę?") i robi to przy
//! rysowaniu, jednym boolem. Rust jest tu autorytetem, bo plik na dysku bywa zmergowany gitem,
//! poprawiony ręcznie albo napisany przez inny build — **bieg nigdy nie ufa UI**.
//!
//! Reguła, która nie umie zaświecić, jest gorsza niż jej brak: zajmuje miejsce reguły, która by
//! zaświeciła. T3 §5.2 zmierzył dokładnie to — napisał wykrywanie „nieosiągalnych kroków",
//! uruchomił je i **nigdy nie wystrzeliło**, bo w grafie acyklicznym obchód z każdego wierzchołka
//! o stopniu wejściowym zero dociera zawsze wszędzie. Zamiast tego sprawdzamy **spójność**,
//! obchodem **ignorującym kierunek strzałek** — ten strzela.
//!
//! 2026-08-16 — cykli nie liczymy tu drugi raz. `engine::dag::Dag::new` odmawia cyklu przy
//! konstrukcji, na listach sąsiedztwa i bez `petgraph` (ARCHITECTURE §10), i zwraca węzły, które
//! na nim leżą. Implementacja `check()` ma zmapować id kroków na numery i zawołać tamto; drugi
//! obchód w tym pliku byłby dokładnie tym duplikatem, przed którym ostrzega TASK.md.

use serde::Serialize;

use super::WorkflowFile;

/// Flagi, które Loadout ustawia sam dla `claude` — przelotka nie ma prawa ich podać.
///
/// 2026-08-16 — **to jest druga kopia listy** i tak jej nie zostawiamy. ARCHITECTURE §6b mówi
/// „lista zarezerwowanych jest jedna, w jednym miejscu, obok budowniczego komendy", a budowniczy
/// to `engine::drivers::claude` (`TRANSPORT` + `LEAN_CONTEXT` + `--session-id`, dziś prywatne).
/// Ten plik nie ma tamtego w swoim bloku OWNS, więc scalenie list jest pytaniem do człowieka
/// (AGENTS.md §7), a nie cichym dopiskiem w cudzym pliku.
pub const RESERVED_CLAUDE: [&str; 7] = [
    "--session-id",
    "--output-format",
    "--input-format",
    "--verbose",
    "--permission-mode",
    "--strict-mcp-config",
    "--setting-sources",
];

/// To samo dla `codex`: `-C` (katalog roboczy), `-s` (piaskownica), `--json` (strumień zdarzeń).
pub const RESERVED_CODEX: [&str; 3] = ["-C", "-s", "--json"];

/// Wartości, których przelotka nie podnosi **niezależnie** od nazwy flagi.
///
/// Dial „co agent może zrobić z plikami" jest jedyną drogą do tych dwóch (ARCHITECTURE §6b
/// reguła 2, D6). Sama lista zarezerwowanych by nie wystarczyła: `--sandbox` nie jest na niej,
/// a `--sandbox danger-full-access` omija dial tak samo skutecznie jak `-s`.
pub const FORBIDDEN_ESCALATIONS: [&str; 2] = ["bypassPermissions", "danger-full-access"];

/// Waga uwagi. `Problem` blokuje Run i zapis, `Warning` nie blokuje niczego.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Problem,
    Warning,
}

/// Jedna uwaga o jednym defekcie.
///
/// `message` idzie **wprost na ekran** (T3 §5.3), więc jest gotowym angielskim zdaniem — bez
/// kodów, bez kluczy i18n i bez żargonu (niezmiennik 14). `cycle detected in DAG`, `orphan node`
/// i `in-degree` są tu zakazane tak samo, jak w komponencie Reacta.
///
/// `step_id` jest tym, na czym ląduje kropka na kafelku i co dostaje `fitView` po kliknięciu
/// uwagi — więc musi nazywać krok, **który istnieje**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub level: Level,
    pub step_id: Option<String>,
    pub message: String,
}

/// Wszystko, co da się powiedzieć o pliku bez uruchamiania go.
///
/// Wołane przy **zapisie** (niezmiennik 12: odmowa pada tam, nie w trakcie biegu) i drugi raz
/// przy Run — to drugie dowodzi T-15.
#[must_use]
pub fn check(_workflow: &WorkflowFile) -> Vec<Note> {
    todo!(
        "T-12: reguły z T3 §5.2 — pusty plik, powtórzone id, strzałka w nieistniejący krok, koło, wyspa, zakres copies, kolizja folderów, przelotka"
    )
}
