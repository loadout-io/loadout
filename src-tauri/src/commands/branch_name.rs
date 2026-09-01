//! Nazwa gałęzi, na której ląduje wynik biegu.
//!
//! # Po co to istnieje
//!
//! Gałęzie kroków nazywają się `loadout/<bieg>/<krok>` i taka mają zostać: są księgowością,
//! muszą być jednoznaczne co do bajta i nie kolidować między biegami. Ale gałąź, którą człowiek
//! ogląda w pull requeście, jest CZYMŚ INNYM — i `loadout/01a05e4b-83b8-7d01-859b-62ed81f853a2`
//! nadaje się do tego zero.
//!
//! Właściciel 2026-09-01: „ja daje po prostu ID tam przy starcie a loadout dopasowuje juz nazwe
//! brancza od preferencji repo". Podział jest dokładnie taki: identyfikator zadania zna WYŁĄCZNIE
//! człowiek, a kształt nazwy da się ZMIERZYĆ z gałęzi, które repo już ma.
//!
//! # Dlaczego to jest pomiar, a nie zgadywanie
//!
//! Przedrostek bierze się z policzenia tego, co w repozytorium stoi — `task-` tam, gdzie jest ich
//! kilkadziesiąt, `feat/` tam, gdzie tak się pisze. Kiedy nic nie dominuje, **nie wymyślamy
//! niczego**: nazwą jest samo ID. To jest ta sama reguła, co przy podpowiedziach w polu — wolno
//! uzupełniać wyłącznie z zamkniętego zbioru, który naprawdę mamy.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// Ile razy przedrostek musi wystąpić, żeby uznać go za konwencję repo.
///
/// Dwa to za mało: dwie gałęzie `fix/` w repozytorium, które poza tym nie ma żadnej konwencji,
/// narzuciłyby ją wszystkim następnym. Trzy jest najmniejszą liczbą, przy której da się mówić
/// o zwyczaju, a nie o zbiegu okoliczności.
const ENOUGH: usize = 3;

/// Ile GAŁĘZI musi mieć przedrostek, żeby w ogóle było o czym mówić — jedna na tyle.
///
/// Bez tego repozytorium z dwustoma gałęziami i trzema `spike/` dostawałoby `spike/` jako swoją
/// konwencję, bo próg bezwzględny sam z siebie nie mówi nic o tle. Liczone całkowitoliczbowo:
/// droga przez `f64` na tym samym pytaniu potrafi obciąć wynik przy dużych repozytoriach.
const ONE_IN: usize = 5;

/// Przedrostek, którego to repozytorium naprawdę używa — albo nic.
///
/// Gałęzie Loadouta (`loadout/…`) są pomijane: to nasza własna księgowość, a nie zwyczaj
/// człowieka, i po kilku biegach zdominowałaby każde repozytorium, narzucając mu nazwę,
/// której nikt nie wybrał.
#[must_use]
pub fn convention(branches: &[String]) -> Option<String> {
    let mine: Vec<&String> = branches
        .iter()
        .filter(|one| !one.starts_with("loadout/"))
        .collect();
    if mine.is_empty() {
        return None;
    }

    let mut seen: HashMap<String, usize> = HashMap::new();
    for one in &mine {
        // Zarówno `feat/coś`, jak i `task-T-150`: separatorem jest pierwszy `/` albo `-`.
        if let Some(cut) = one.find(['/', '-']) {
            let prefix = one[..=cut].to_owned();
            *seen.entry(prefix).or_default() += 1;
        }
    }

    seen.into_iter()
        .filter(|(_, count)| *count >= ENOUGH && *count * ONE_IN >= mine.len())
        // Przy remisie wygrywa dłuższy przedrostek, a potem alfabet: cokolwiek, byle NIE kolejność
        // z `HashMap`, bo ta zmienia się między uruchomieniami i nazwa gałęzi przestałaby być
        // powtarzalna dla tego samego repozytorium.
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| left.0.len().cmp(&right.0.len()))
                .then_with(|| right.0.cmp(&left.0))
        })
        .map(|(prefix, _)| prefix)
}

/// Nazwa gałęzi dla podanego identyfikatora zadania.
///
/// Kiedy ID samo zaczyna się od wykrytego przedrostka, nie doklejamy drugiego: człowiek, który
/// wpisał `task-T-150` w repozytorium pisanym `task-`, chciał tej nazwy, a nie `task-task-T-150`.
#[must_use]
pub fn compose(branches: &[String], id: &str) -> String {
    let id = id.trim();
    match convention(branches) {
        Some(prefix) if !id.starts_with(&prefix) => format!("{prefix}{id}"),
        _ => id.to_owned(),
    }
}

/// Czy ta nazwa jest wolna. Pytane PRZY STARCIE, bo praca zrobiona i niemająca gdzie wylądować
/// jest gorsza niż bieg, który się nie zaczął.
#[must_use]
pub fn taken(branches: &[String], name: &str) -> bool {
    branches.iter().any(|one| one == name)
}

/// Nazwy gałęzi, które to repozytorium ma.
///
/// Pusto, kiedy folder nie jest repozytorium albo gita nie ma na `PATH`: brak konwencji jest
/// wtedy poprawną odpowiedzią, a nie awarią — pole przy Starcie ma dalej działać i przyjąć samo
/// ID. `for-each-ref` zamiast `branch`, bo `branch` maluje kolorami i gwiazdką bieżącej gałęzi,
/// a to jest wyjście dla człowieka, nie dla programu.
#[must_use]
pub fn branches_of(project: &Path) -> Vec<String> {
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|one| !one.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Co ekran Startu ma pokazać pod polem z identyfikatorem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposed {
    /// Nazwa, którą Loadout założy. Pusta, kiedy człowiek nie wpisał jeszcze ID.
    pub name: String,
    /// Przedrostek, który udało się zmierzyć — albo nic, i wtedy ekran mówi, że nie widzi konwencji.
    pub convention: Option<String>,
    /// Czy taka gałąź już jest. Pytane TERAZ, bo praca zrobiona i niemająca gdzie wylądować jest
    /// gorsza niż bieg, który się nie zaczął.
    pub taken: bool,
}

/// Propozycja nazwy dla identyfikatora, policzona z gałęzi tego repozytorium.
#[must_use]
pub fn proposed(project: &Path, id: &str) -> Proposed {
    let branches = branches_of(project);
    let name = compose(&branches, id);
    Proposed {
        convention: convention(&branches),
        taken: !name.is_empty() && taken(&branches, &name),
        name,
    }
}
