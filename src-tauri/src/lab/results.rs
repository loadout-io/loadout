//! Wynik przebiegu, liczony z dwóch plików, które i tak istnieją.
//!
//! # Skąd bierze się wynik, skoro nie ma pliku z wynikiem
//!
//! Z planu i z `run.json`. Plan mówi, **który krok należy do której komórki** (identyfikator
//! kroku składa [`super::plan::key_for`]), a `run.json` mówi, **co się z tym krokiem stało**.
//! Trzeci plik trzymałby tę samą odpowiedź po raz drugi, pisany po zakończeniu biegu — czyli
//! dokładnie wtedy, kiedy aplikacja może zginąć, a po jej powrocie nikt nie umiałby powiedzieć,
//! która z dwóch odpowiedzi jest prawdziwa (niezmienniki 4 i 21).
//!
//! # Trzy rzeczy muszą się zgodzić, żeby komórka przeszła
//!
//! 1. **Praca się udała** — krok agenta skończył się `succeeded`. Brak wymaganego pola już
//!    tutaj przewraca krok, po stronie biegu (`commands::run::missing_a_required_field`), więc
//!    obecności pól ta funkcja nie sprawdza drugi raz.
//! 2. **Pola mówią to, czego oczekiwano** — [`super::Expect::contains`], sprawdzane tu i tylko
//!    tu, bo do promptu ta wartość nie wchodzi nigdy (powód przy `plan::handover_for`).
//! 3. **Komenda przeszła** — krok „sprawdź" skończył się `succeeded`, a jego werdykt to
//!    koniunkcja kodu wyjścia i wzorca dowodu (`engine::drivers::command::passed`,
//!    niezmiennik 19). Przypadek bez komendy nie ma tego kroku i nie ma go za co sądzić.
//!
//! Koniunkcja, nigdy alternatywa. Każdy z trzech warunków osobno przepuszcza komórkę, która
//! nie zrobiła tego, o co proszono — a zielona komórka nad niezrobioną pracą jest tą samą wadą,
//! dla której powstało całe to repo.

use std::collections::BTreeMap;

use crate::memory::handoff::fields_said_in;

use super::plan::{Half, key_for};
use super::{Case, EvalSet, Variant};

/// Słowo z drutu, którym `run.json` nazywa krok, który się udał.
const SUCCEEDED: &str = "succeeded";

/// Słowa, po których wiadomo, że tego kroku **nie osądzono** — nie że nie przeszedł.
///
/// Rozróżnienie jest treścią, nie odcieniem. Bieg zatrzymany przez człowieka w połowie
/// macierzy zostawia komórki, których nikt nie zmierzył; policzone jako porażki obniżyłyby
/// wynik zestawu o pracę, której nikt nie zamówił, a wtedy „7/9" po zatrzymaniu znaczy coś
/// innego niż „7/9" po pełnym przebiegu.
const NEVER_JUDGED: [&str; 5] = ["pending", "ready", "running", "cancelled", "skipped"];

/// Jeden krok skończonego biegu, w kształcie, którego potrzebuje ta warstwa.
///
/// **Nie `PastStepWire`**, i to jest granica, nie kaprys: tamten typ mieszka w `commands/`,
/// a `lab/` nie ma prawa od `commands/` zależeć — zależność w tę stronę zamknęłaby koło, bo
/// `commands/` czyta `lab/`. Złożenie tego kształtu z tego, co leży na dysku, należy do
/// warstwy komend i jest tam czterema wierszami.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    /// Klucz kroku z pliku planu — ten sam, który złożył [`key_for`].
    pub tile: String,
    /// Słowo z drutu: `succeeded`, `failed`, `cancelled`, `skipped`, `running`…
    pub state: String,
    /// Ile ten krok kosztował. `None` znaczy „nie podał", nie zero.
    pub cost_usd: Option<f64>,
    /// Powód, jeśli coś poszło nie tak.
    pub error: String,
    /// Co ten krok oddał: **całe ciało przekazania**, kiedy jakieś zostawił.
    ///
    /// Całe, a nie podsumowanie: `run.json` trzyma jedno zdanie przycięte do limitu, a pole
    /// oczekiwane przez przypadek bywa dziesiątym wierszem odpowiedzi. Sprawdzanie treści na
    /// przyciętym zdaniu odpowiadałoby „nie ma" o wartości, która jest.
    pub said: String,
}

/// Jak skończyła się jedna komórka.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Praca się udała, pola mówią swoje, komenda przeszła.
    Passed,
    /// Zmierzone i nie przeszło.
    DidNotPass,
    /// Nie zmierzone: bieg tu nie dotarł albo człowiek go zatrzymał.
    NotJudged,
}

impl Outcome {
    /// Słowo z drutu dla okna. Tłumaczenie na zdanie należy do okna (niezmiennik 14).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::DidNotPass => "did-not-pass",
            Self::NotJudged => "not-judged",
        }
    }
}

/// Jedna komórka macierzy razem z tym, co o niej wiadomo.
#[derive(Debug, Clone, PartialEq)]
pub struct CellResult {
    /// Identyfikator przypadku (wiersz).
    pub case: String,
    /// Identyfikator wariantu (kolumna).
    pub variant: String,
    /// Jak się skończyła.
    pub outcome: Outcome,
    /// Zdanie dla człowieka: **dlaczego** tak, a nie inaczej. Puste przy przejściu.
    pub said: String,
    /// Ile ta komórka kosztowała — praca plus jej sprawdzenie.
    pub cost_usd: Option<f64>,
}

/// Cała macierz jednego przebiegu.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    /// Komórki w kolejności: wiersz po wierszu, kolumna po kolumnie.
    pub cells: Vec<CellResult>,
    /// Ile przeszło.
    pub passed: usize,
    /// Ile **zmierzono** — czyli wszystko poza [`Outcome::NotJudged`].
    pub judged: usize,
    /// Ile kosztował cały przebieg. `None`, kiedy żaden krok nie podał ceny.
    pub cost_usd: Option<f64>,
}

/// Liczy macierz jednego przebiegu.
///
/// Kolejność komórek jest kolejnością zestawu, nie kolejnością biegu: tabela ma się rysować
/// tak samo po każdym przebiegu, a bieg kończy kroki w kolejności, którą wybrała pula miejsc.
#[must_use]
pub fn score(set: &EvalSet, finished: &[Finished]) -> Scored {
    let by_tile: BTreeMap<&str, &Finished> = finished
        .iter()
        .map(|step| (step.tile.as_str(), step))
        .collect();

    let mut cells = Vec::new();
    let mut passed = 0;
    let mut judged = 0;
    let mut spent: Option<f64> = None;

    for case in set.running_cases() {
        for variant in &set.variants {
            let cell = judge(case, variant, &by_tile);
            match cell.outcome {
                Outcome::Passed => {
                    passed += 1;
                    judged += 1;
                }
                Outcome::DidNotPass => judged += 1,
                Outcome::NotJudged => {}
            }
            if let Some(cost) = cell.cost_usd {
                spent = Some(spent.unwrap_or(0.0) + cost);
            }
            cells.push(cell);
        }
    }

    Scored {
        cells,
        passed,
        judged,
        cost_usd: spent,
    }
}

/// Werdykt jednej komórki.
fn judge(case: &Case, variant: &Variant, by_tile: &BTreeMap<&str, &Finished>) -> CellResult {
    let work_key = key_for(&case.id, &variant.id, Half::Work);
    let checks_key = key_for(&case.id, &variant.id, Half::Checks);
    let work = by_tile.get(work_key.as_str()).copied();
    let checks = by_tile.get(checks_key.as_str()).copied();
    let cost = add(
        work.and_then(|step| step.cost_usd),
        checks.and_then(|step| step.cost_usd),
    );

    let cell = |outcome: Outcome, said: String| CellResult {
        case: case.id.clone(),
        variant: variant.id.clone(),
        outcome,
        said,
        cost_usd: cost,
    };

    let Some(work) = work else {
        // Krok, którego w tym biegu nie ma: zestaw urósł po przebiegu albo przebieg pochodzi
        // sprzed tej kolumny. Ani przejście, ani porażka — po prostu nikt tego nie zmierzył.
        return cell(
            Outcome::NotJudged,
            "This run is older than this row or column, so nothing here was measured.".to_owned(),
        );
    };

    if NEVER_JUDGED.contains(&work.state.as_str()) {
        return cell(Outcome::NotJudged, work_stopped_short(&work.state));
    }
    if work.state != SUCCEEDED {
        // Powód od biegu, kiedy jest; własne zdanie, kiedy go nie ma. Pusta komórka nad
        // czerwonym wynikiem wysyła człowieka do transkryptu po coś, co bieg już powiedział.
        let said = if work.error.trim().is_empty() {
            "The work did not finish.".to_owned()
        } else {
            work.error.trim().to_owned()
        };
        return cell(Outcome::DidNotPass, said);
    }

    if let Some(said) = a_field_that_does_not_say_it(case, &work.said) {
        return cell(Outcome::DidNotPass, said);
    }

    if case.command.trim().is_empty() {
        return cell(Outcome::Passed, String::new());
    }

    let Some(checks) = checks else {
        // Komenda jest w przypadku, a kroku nie ma w biegu: ten przebieg pochodzi sprzed
        // chwili, w której człowiek dopisał komendę. Milczenie o tym pokazałoby przejście
        // oparte wyłącznie na pracy — czyli na połowie umowy.
        return cell(
            Outcome::NotJudged,
            "This run is older than the command on this case, so nothing checked the work."
                .to_owned(),
        );
    };
    if NEVER_JUDGED.contains(&checks.state.as_str()) {
        return cell(Outcome::NotJudged, checks_stopped_short(&checks.state));
    }
    if checks.state != SUCCEEDED {
        let said = if checks.error.trim().is_empty() {
            "The checks did not pass.".to_owned()
        } else {
            checks.error.trim().to_owned()
        };
        return cell(Outcome::DidNotPass, said);
    }

    cell(Outcome::Passed, String::new())
}

/// Pierwsze pole, które nie mówi tego, czego od niego oczekiwano — albo `None`.
///
/// PIERWSZE, nie wszystkie: człowiek czyta to zdanie w komórce i naprawia jedną rzecz naraz.
/// Ten sam wybór stoi w `commands::run::missing_a_required_field`.
///
/// Oczekiwanie z pustym `contains` **nie jest tu sądzone**: ono mówi wyłącznie „to pole ma
/// być", a tego pilnuje już bieg. Sprawdzanie go drugi raz zamieniłoby jeden fakt w dwie
/// odpowiedzi, które kiedyś się rozjadą (niezmiennik 13).
fn a_field_that_does_not_say_it(case: &Case, said: &str) -> Option<String> {
    let fields = fields_said_in(said);
    case.expect
        .iter()
        .filter(|expect| !expect.contains.trim().is_empty())
        .find_map(|expect| {
            let name = expect.field.trim();
            let wanted = expect.contains.trim();
            match fields.get(name) {
                // Pole jest, ale nie mówi tego, czego od niego chciano. Porównanie bez
                // wielkości liter, bo model pisze „Yes" tam, gdzie człowiek napisał „yes",
                // a to nie jest różnica, o którą ktokolwiek pytał.
                Some(value) if value.to_lowercase().contains(&wanted.to_lowercase()) => None,
                Some(value) => Some(format!(
                    "\"{name}\" came back as \"{value}\", and this case asked it to mention \
                     \"{wanted}\"."
                )),
                None => Some(format!(
                    "\"{name}\" is missing from the answer, and this case asked for it."
                )),
            }
        })
}

/// Zdanie o pracy, której nikt nie zmierzył.
fn work_stopped_short(state: &str) -> String {
    match state {
        "cancelled" => "The run was stopped before this finished.".to_owned(),
        "skipped" => "This never started.".to_owned(),
        _ => "This is still going.".to_owned(),
    }
}

/// To samo o sprawdzeniu.
fn checks_stopped_short(state: &str) -> String {
    match state {
        "cancelled" => "The run was stopped before the checks ran.".to_owned(),
        // Krok „sprawdź" pominięty znaczy, że praca przed nim nie przeszła — a tamto zdanie
        // stoi już w tej samej komórce, więc to nie ma prawa go zastąpić.
        "skipped" => "The checks never ran.".to_owned(),
        _ => "The checks are still going.".to_owned(),
    }
}

/// Suma dwóch cen, w której `None` znaczy „nie podał", a nie zero.
///
/// Dwa `None` dają `None`: zero jest liczbą i sumuje się w rachunek, którego nikt nie zamawiał
/// (ten sam powód stoi przy `engine::drivers::Outcome::cost_usd`).
fn add(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (None, None) => None,
        (one, other) => Some(one.unwrap_or(0.0) + other.unwrap_or(0.0)),
    }
}

/// O ile ten przebieg jest lepszy albo gorszy od poprzedniego.
///
/// Liczone **po komórkach**, nie po sumach: zestaw, w którym jedna komórka zaczęła przechodzić,
/// a druga przestała, ma tę samą sumę i nie jest tym samym wynikiem. Człowiek pyta „czy moja
/// zmiana pomogła", a na to odpowiada wyłącznie różnica per komórka.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Movement {
    /// Ile komórek zaczęło przechodzić.
    pub gained: usize,
    /// Ile przestało.
    pub lost: usize,
}

/// Porównuje dwa przebiegi tego samego zestawu.
///
/// Komórki, których w którymkolwiek z dwóch przebiegów **nie zmierzono**, nie liczą się do
/// żadnej ze stron: przebieg zatrzymany w połowie wyglądałby inaczej jako strata i inaczej
/// jako brak, a tylko drugie jest prawdą.
#[must_use]
pub fn moved(now: &Scored, before: &Scored) -> Movement {
    let earlier: BTreeMap<(&str, &str), Outcome> = before
        .cells
        .iter()
        .map(|cell| ((cell.case.as_str(), cell.variant.as_str()), cell.outcome))
        .collect();

    let mut movement = Movement::default();
    for cell in &now.cells {
        let Some(was) = earlier.get(&(cell.case.as_str(), cell.variant.as_str())) else {
            continue;
        };
        if *was == Outcome::NotJudged || cell.outcome == Outcome::NotJudged {
            continue;
        }
        match (was, cell.outcome) {
            (Outcome::DidNotPass, Outcome::Passed) => movement.gained += 1,
            (Outcome::Passed, Outcome::DidNotPass) => movement.lost += 1,
            _ => {}
        }
    }
    movement
}
