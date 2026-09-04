//! Z-38, druga połowa: to, co bieg zapisał o prywatnej turze, DOCHODZI do okna z typem.
//!
//! # Po co to jest osobnym modułem
//!
//! Bo `z38_reflection_says_it_ran_out` obok ma się kompilować na drzewie SPRZED tego zadania
//! (AGENTS.md §2a pkt 4: test, który się nie skompilował, niczego nie dowiódł), więc nie zna ani
//! jednej nazwy, która powstała razem z poprawką — czyta `run.json` jako surowy JSON. To zostawia
//! jedno pytanie bez odpowiedzi: czy te klucze mają jeszcze DROGĘ do okna. Kod z drutu, którego
//! czytelnik nie zna, jest po stronie okna niczym: `serde` odkłada go w `#[serde(other)] Unknown`
//! i historia mówi wtedy „Loadout did not look back at this run" o biegu, za którego turę ktoś
//! zapłacił — czyli dokładnie ta wada, dla której zadanie powstało, tylko o jedną warstwę dalej.
//!
//! Ten moduł powstał więc PO nośnikach i pyta wyłącznie o nie, tą samą komendą, którą panel
//! historii otwiera bieg (`read_run_inner`).
//!
//! # Dlaczego pliki są wypisane LITERAŁEM, a nie zapisane przez bieg
//!
//! Odczyt, który czyta wyłącznie to, co sam przed chwilą zapisał, nie odpowiada na pytanie
//! o niezmiennik 4 ani trochę — a `run.json` niesie klucze MIESZANE (`camelCase` i `snake_case`
//! w jednym pliku, powód w całości przy `commands::history::ReflectionWire`). Ta sama zasada, co
//! w `reflection_receipt_reaches_the_history.rs` obok; ten moduł jest jego ciągiem dalszym dla
//! trzech kodów, które doszły w Z-38.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use loadout_lib::commands::history::{NotAsked, list_runs_inner, read_run_inner};
use loadout_lib::commands::run::REFLECTION_BUDGET_CAP_USD;

/// Bieg, którego tura zeszła na cenie — z kwotą, którą pokazuje potem panel historii.
const RAN_OUT_OF_MONEY: &str = "20260904-101511__0198a1f2-3b4c-7d5e-8f60-000000000112";

/// Ten sam bieg, tylko zatrzymany zegarem.
const RAN_OUT_OF_TIME: &str = "20260904-101512__0198a1f2-3b4c-7d5e-8f60-000000000113";

/// I bieg, dla którego nie było czym zapytać.
const NO_AGENT_APP: &str = "20260904-101510__0198a1f2-3b4c-7d5e-8f60-000000000111";

/// Bieg z kodem, którego ten czytelnik nie zna — z jutrzejszego Loadouta.
const FROM_TOMORROW: &str = "20260904-101513__0198a1f2-3b4c-7d5e-8f60-000000000114";

/// Sufit, który obowiązywał bieg z audytu 2026-09-04: jeden procent z 57,52 USD.
const CEILING_USD: f64 = 0.58;

/// `run.json` biegu, którego tura zeszła na cenie — wypisany DOKŁADNIE tak, jak zapisuje go
/// `commands::run`: `discardedAgain` w `camelCase`, `budget_usd` i `dropped_without_reason`
/// w `snake_case`.
const RAN_OUT_OF_MONEY_FILE: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000112",
  "workflow_id": "ship-a-feature.json",
  "title": "Ship a feature",
  "status": "succeeded",
  "spent_usd": 58.1,
  "reflection": {
    "ran": false,
    "kept": 0,
    "discardedAgain": 0,
    "dropped_without_reason": 0,
    "cost_usd": 0.58,
    "budget_usd": 0.58,
    "why": "ran-out-of-budget"
  },
  "steps": []
}"#;

const RAN_OUT_OF_TIME_FILE: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000113",
  "workflow_id": "ship-a-feature.json",
  "title": "Ship a feature",
  "status": "succeeded",
  "reflection": {
    "ran": false,
    "kept": 0,
    "discardedAgain": 0,
    "dropped_without_reason": 0,
    "budget_usd": 0.58,
    "why": "ran-out-of-time"
  },
  "steps": []
}"#;

const NO_AGENT_APP_FILE: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000111",
  "workflow_id": "ship-a-feature.json",
  "title": "Ship a feature",
  "status": "succeeded",
  "reflection": {
    "ran": false,
    "kept": 0,
    "discardedAgain": 0,
    "dropped_without_reason": 0,
    "why": "no-agent-app"
  },
  "steps": []
}"#;

/// Kod, którego dzisiejszy czytelnik nie zna — i który nie ma prawa unieważnić całego biegu.
const FROM_TOMORROW_FILE: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000114",
  "workflow_id": "ship-a-feature.json",
  "title": "Ship a feature",
  "status": "succeeded",
  "reflection": {
    "ran": false,
    "kept": 0,
    "discardedAgain": 0,
    "dropped_without_reason": 0,
    "why": "ran-out-of-patience"
  },
  "steps": []
}"#;

/// Projekt z czterema biegami, każdy z innym powodem zejścia prywatnej tury.
fn a_project_with_all_four(root: &Path) -> PathBuf {
    let project = root.join("ledger-ui");
    let runs = project.join(".loadout").join("runs");
    for (folder, description) in [
        (RAN_OUT_OF_MONEY, RAN_OUT_OF_MONEY_FILE),
        (RAN_OUT_OF_TIME, RAN_OUT_OF_TIME_FILE),
        (NO_AGENT_APP, NO_AGENT_APP_FILE),
        (FROM_TOMORROW, FROM_TOMORROW_FILE),
    ] {
        std::fs::create_dir_all(runs.join(folder)).unwrap();
        std::fs::write(runs.join(folder).join("run.json"), description).unwrap();
    }
    project
}

#[test]
fn the_reason_and_the_ceiling_reach_the_window_with_their_own_types() {
    let root = tempfile::tempdir().unwrap();
    let project = a_project_with_all_four(root.path());

    let opened =
        read_run_inner(&project, RAN_OUT_OF_MONEY).expect("that run is right there on disk");
    let did = opened
        .reflection
        .expect("the run's record carries a receipt for the private turn and nothing read it");

    assert_eq!(
        did.why,
        Some(NotAsked::RanOutOfBudget),
        "the code `ran-out-of-budget` did not survive the trip to the window. An unknown code \
         lands in `Unknown`, the panel falls back to \"Loadout did not look back at this run\", \
         and the person is told nothing happened about a turn they paid for — the same defect \
         this task removed one layer earlier"
    );
    assert_eq!(
        did.budget_usd,
        Some(CEILING_USD),
        "the ceiling this turn had did not reach the window. It is the last word of the sentence \
         the history panel builds (`said.ts`), and it cannot be rebuilt from a constant: since \
         this task it scales with what the run cost"
    );
    assert!(
        !did.ran,
        "a turn that never answered came out as one that ran, and every sentence the screen \
         builds from here starts with that word"
    );

    // ── Pozostałe dwa kody z tego zadania, po jednym biegu na każdy ────────────────────────
    let by_the_clock = read_run_inner(&project, RAN_OUT_OF_TIME)
        .expect("that run is right there on disk")
        .reflection
        .expect("that run's record carries a receipt");
    assert_eq!(
        by_the_clock.why,
        Some(NotAsked::RanOutOfTime),
        "a turn stopped by the clock reads as something else, and the two have different answers \
         for the person: one costs more money, the other costs more minutes"
    );

    let no_app = read_run_inner(&project, NO_AGENT_APP)
        .expect("that run is right there on disk")
        .reflection
        .expect("that run's record carries a receipt");
    assert_eq!(
        no_app.why,
        Some(NotAsked::NoAgentApp),
        "the one code that is about THIS MACHINE rather than about the run did not survive the \
         trip, so the screen tells the person to look in a run directory where there is nothing \
         to find"
    );
}

/// DRUGA DROGA DO TEGO SAMEGO RACHUNKU — ta, którą chodzi sekcja Knowledge.
///
/// Kolejka decyzji w Knowledge zapełnia się notatkami z tury po biegu, więc kiedy ta tura zeszła
/// na cenie, kolejka jest pusta Z POWODU — a powód mieszka w `run.json` ostatniego biegu. Okno
/// pyta o niego WIERSZEM LISTY (`list_runs`), nie otwieraniem każdego biegu po kolei: sekcja,
/// która musiałaby otworzyć wszystkie, żeby dowiedzieć się czegoś o jednym, płaci za to
/// odczytem całej historii przy każdym wejściu.
///
/// Kolejność też jest tu treścią: front bierze wiersz PIERWSZY jako najnowszy bieg, a zdanie
/// o turze sprzed tygodnia stałoby nad kolejką, którą zapełnił wczorajszy.
#[test]
fn the_row_of_the_last_run_carries_the_same_receipt() {
    let root = tempfile::tempdir().unwrap();
    let project = a_project_with_all_four(root.path());

    let runs = list_runs_inner(&project);
    assert_eq!(
        runs.first().map(|row| row.folder.as_str()),
        Some(FROM_TOMORROW),
        "the newest run is not the first row, so the section reading `runs[0]` explains its \
         empty queue with a sentence about some other run"
    );

    let ran_out = runs
        .iter()
        .find(|row| row.folder == RAN_OUT_OF_MONEY)
        .expect("the run that ran out of money is one of the four on disk");
    let did = ran_out.reflection.expect(
        "the row of a run whose private turn ran out of money says nothing about that turn. \
         Knowledge then has no road to the fact at all: the notes it shows are written by that \
         turn, and an empty queue with no explanation reads as a run nobody could learn \
         anything from",
    );
    assert_eq!(
        did.why,
        Some(NotAsked::RanOutOfBudget),
        "the reason did not survive the trip into the row"
    );
    assert_eq!(
        did.budget_usd,
        Some(CEILING_USD),
        "and neither did the amount, which is the last word of the sentence that screen builds"
    );

    let unreadable = runs
        .iter()
        .find(|row| row.folder == RAN_OUT_OF_TIME)
        .expect("that run is on disk too");
    assert_eq!(
        unreadable.reflection.and_then(|did| did.why),
        Some(NotAsked::RanOutOfTime),
        "the row for a turn stopped by the clock says something else, and the two have different \
         answers for the person"
    );
}

/// Kod z jutrzejszego Loadouta nie ma prawa unieważnić historii (niezmiennik 5).
#[test]
fn a_reason_this_reader_does_not_know_still_opens_the_run() {
    let root = tempfile::tempdir().unwrap();
    let project = a_project_with_all_four(root.path());

    let opened = read_run_inner(&project, FROM_TOMORROW)
        .expect("a run with one unknown code is still a run, and it has to open");
    let did = opened
        .reflection
        .expect("the receipt is unreadable because of one unknown value inside it");
    assert_eq!(
        did.why,
        Some(NotAsked::Unknown),
        "an unknown code came out as something this reader claims to understand. `#[serde(other)]` \
         is what keeps a newer file readable, and a value guessed into a known variant would put \
         a wrong sentence on the screen instead of the honest fallback"
    );
}

/// Sufit tury jest liczbą PRODUKCYJNĄ, nie ustawieniem testu.
///
/// Bieg wyliczający sufit sądzi `z38_reflection_says_it_ran_out` na własnej ławce i celowo
/// przeciw literałowi (niezmiennik 20). Tutaj pytamy o coś innego i tylko o to: czy stała, do
/// której odwołuje się tamten literał, dalej mówi tę samą kwotę — bo jeśli ktoś podniesie ją
/// w kodzie, tamten test zrobi się czerwony i nie powie dlaczego.
#[test]
fn the_cap_on_one_private_turn_is_a_dollar() {
    assert!(
        (REFLECTION_BUDGET_CAP_USD - 1.00).abs() < f64::EPSILON,
        "the cap on one private turn is now ${REFLECTION_BUDGET_CAP_USD}. One percent with no \
         upper bound makes the cheapest turn of an expensive run its most expensive one, and \
         \"one cheap reflection after the run\" [T6 section 5.3] stops being true where nobody \
         looks — on the bill"
    );
}
