//! V-02: obowiązkowe zachowania, których podsumowanie nie ma jak zgubić.
//!
//! # Po co ten moduł istnieje
//!
//! Zmierzone 2026-09-06 (incydent I-05, bieg `20260906-165702`): krok QA opisał w prozie brak
//! obowiązkowych zachowań i **w tej samej odpowiedzi** napisał `outcome: pass`. Nic w tym nie
//! było sprzeczne z instrukcją, którą dostał: uniwersalne `OUTCOME_ASKED_FOR` prosi o `pass`,
//! kiedy praca jest „good enough to build on" — a praca z brakującym wymaganiem naprawdę może
//! być dobrą podstawą do dalszej pracy. Pytanie było złe, nie odpowiedź.
//!
//! Rozstrzygnięcie: kiedy człowiek zatwierdził listę wymagań, wynik NIE jest jednym słowem
//! z ostatniego wiersza. Powstaje z **kompletności i wyników** zatwierdzonych kryteriów.
//! Przeszukiwanie prozy po słowach („non-blocking", „minor") nie rozwiązuje semantyki — to jest
//! ta sama klasa, co czytanie `subtype` zamiast `is_error`.
//!
//! # Trzy wyniki, nie dwa
//!
//! `Failed` i `NotTested` są różnymi rzeczami dla człowieka i prowadzą do różnych czynności:
//! pierwsze mówi o produkcie, drugie o środowisku pomiaru. Niesprawdzone wymagane kryterium
//! nie jest ani zaliczeniem, ani dowodem wady — i dlatego nie ma prawa wybrać żadnej gałęzi
//! grafu bez decyzji człowieka.

use serde::{Deserialize, Serialize};

/// Czym wolno potwierdzić jedno kryterium.
///
/// Rozróżnienie jest po to, żeby weryfikator nie mógł obniżyć metody: kryterium wymagające
/// pełnego runtime'u nie jest spełnione przez ekran z podstawionym wynikiem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// Test automatyczny w suicie projektu.
    #[default]
    AutomatedTest,
    /// Interfejs uruchomiony z podstawionym backendem.
    MockedUi,
    /// Pełna aplikacja z prawdziwym backendem.
    FullRuntime,
    /// Potwierdzenie człowieka.
    HumanConfirmed,
    #[serde(other)]
    Unknown,
}

impl Method {
    /// Czy `reported` wystarcza tam, gdzie zatwierdzono `self`.
    ///
    /// Pełny runtime **nie** jest zastępowalny mockiem ani testem jednostkowym. W drugą
    /// stronę wolno: kto potwierdził kryterium na żywej aplikacji, potwierdził je mocniej,
    /// niż wymagano.
    #[must_use]
    pub fn is_met_by(self, reported: Self) -> bool {
        if self == reported {
            return true;
        }
        matches!(
            (self, reported),
            (Self::AutomatedTest | Self::MockedUi, Self::FullRuntime)
        )
    }

    /// Jak ta metoda nazywa się w zdaniu dla człowieka.
    #[must_use]
    pub const fn said(self) -> &'static str {
        match self {
            Self::AutomatedTest => "an automated test",
            Self::MockedUi => "the interface with stand-in data",
            Self::FullRuntime => "the running application with its real backend",
            Self::HumanConfirmed => "a person confirming it",
            Self::Unknown => "a way this version does not know",
        }
    }
}

/// Jedno zatwierdzone wymaganie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Criterion {
    /// Stabilny identyfikator. To po nim weryfikator melduje wynik.
    pub id: String,
    /// Zachowanie widoczne dla CZŁOWIEKA, nie nazwa funkcji.
    #[serde(default)]
    pub behaviour: String,
    /// Czy bez tego wynik nie może być zaliczeniem. Domyślnie tak: lista wymagań, której
    /// pozycje są opcjonalne, jest listą sugestii.
    #[serde(default = "yes")]
    pub required: bool,
    /// Czym wolno to potwierdzić.
    #[serde(default)]
    pub method: Method,
}

const fn yes() -> bool {
    true
}

/// Co weryfikator powiedział o jednym kryterium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reported {
    Passed,
    Failed,
    /// Brak narzędzia, uprawnienia, zależności albo wiarygodnego pomiaru.
    NotTested,
}

/// Jeden wiersz raportu weryfikatora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub id: String,
    pub result: Reported,
    /// Czym to potwierdzono, jeśli weryfikator powiedział. Brak znaczy „metodą zatwierdzoną".
    pub method: Option<Method>,
    pub reason: String,
}

/// Rozstrzygnięcie całej listy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// Komplet wymaganych kryteriów potwierdzony.
    Passed,
    /// Co najmniej jedno wymagane kryterium zaobserwowano jako niespełnione.
    DidNotPass,
    /// Nie ma czego zaliczyć ani czego uznać za wadę: raport jest niekompletny, niespójny
    /// albo brakuje pomiaru.
    NotJudged,
}

/// Pełny wynik sądzenia listy — razem z tym, czego zabrakło.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    pub outcome: Outcome,
    /// Wymagane kryteria bez ani jednego meldunku.
    pub missing: Vec<String>,
    /// Zameldowane identyfikatory, których nikt nie zatwierdzał.
    pub unknown: Vec<String>,
    /// Zameldowane więcej niż raz.
    pub duplicated: Vec<String>,
    /// Wymagane i niespełnione.
    pub failed: Vec<String>,
    /// Wymagane i niezmierzone.
    pub not_tested: Vec<String>,
    /// Wymagane, potwierdzone metodą słabszą niż zatwierdzona.
    pub weaker_method: Vec<String>,
}

impl Judgement {
    /// Zdanie dla człowieka. Puste, kiedy komplet wymagań jest potwierdzony.
    #[must_use]
    pub fn said(&self) -> String {
        let mut parts = Vec::new();
        if !self.missing.is_empty() {
            parts.push(format!(
                "said nothing about {} of the requirements it had to answer ({})",
                self.missing.len(),
                names(&self.missing)
            ));
        }
        if !self.failed.is_empty() {
            parts.push(format!(
                "found {} of them not met ({})",
                self.failed.len(),
                names(&self.failed)
            ));
        }
        if !self.not_tested.is_empty() {
            parts.push(format!(
                "could not measure {} of them ({})",
                self.not_tested.len(),
                names(&self.not_tested)
            ));
        }
        if !self.weaker_method.is_empty() {
            parts.push(format!(
                "confirmed {} of them a weaker way than the one agreed ({})",
                self.weaker_method.len(),
                names(&self.weaker_method)
            ));
        }
        if !self.unknown.is_empty() {
            parts.push(format!(
                "answered about {} requirement(s) nobody approved ({})",
                self.unknown.len(),
                names(&self.unknown)
            ));
        }
        if !self.duplicated.is_empty() {
            parts.push(format!(
                "answered twice about {} of them ({})",
                self.duplicated.len(),
                names(&self.duplicated)
            ));
        }
        if parts.is_empty() {
            return String::new();
        }
        format!("The tester {}.", parts.join(", it "))
    }
}

const NAMES_IN_A_SENTENCE: usize = 3;

fn names(ids: &[String]) -> String {
    let shown = ids
        .iter()
        .take(NAMES_IN_A_SENTENCE)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    match ids.len().saturating_sub(NAMES_IN_A_SENTENCE) {
        0 => shown,
        rest => format!("{shown} and {rest} more"),
    }
}

/// Znacznik wiersza raportu. Ten sam idiom, co `outcome:` przy werdykcie pętli: CAŁY wiersz
/// o sztywnym kształcie, a nie szukanie słów w prozie.
const MARK: &str = "criterion";

/// Wynik z prozy weryfikatora.
///
/// Czytamy wyłącznie wiersze o dokładnym kształcie `criterion <id>: <wynik>[ — powód]`.
/// Wszystko inne jest prozą i nie ma wpływu na rozstrzygnięcie — także zdanie `outcome: pass`.
#[must_use]
pub fn reported_in(said: &str) -> Vec<Line> {
    said.lines().filter_map(one_line).collect()
}

fn one_line(line: &str) -> Option<Line> {
    let rest = line.trim();
    let rest = rest.strip_prefix(MARK).or_else(|| {
        // Wielkość liter nie ma znaczenia; miejsce w wierszu ma.
        rest.get(..MARK.len())
            .filter(|start| start.eq_ignore_ascii_case(MARK))
            .map(|_| &rest[MARK.len()..])
    })?;
    let rest = rest.strip_prefix(' ')?;
    let (id, rest) = rest.split_once(':')?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let (verdict, reason) = split_reason(rest.trim());
    let lowered = verdict.trim().to_ascii_lowercase();
    let (result, method) = read_verdict(&lowered)?;
    Some(Line {
        id: id.to_owned(),
        result,
        method,
        reason: reason.trim().to_owned(),
    })
}

fn split_reason(rest: &str) -> (&str, &str) {
    for separator in [" — ", " - ", " – "] {
        if let Some((verdict, reason)) = rest.split_once(separator) {
            return (verdict, reason);
        }
    }
    (rest, "")
}

/// `passed`, `failed`, `not tested` — plus opcjonalne `(full runtime)` po wyniku.
fn read_verdict(lowered: &str) -> Option<(Reported, Option<Method>)> {
    let (word, method) = match lowered.split_once(" via ") {
        Some((word, how)) => (word.trim(), method_named(how.trim())),
        None => (lowered, None),
    };
    let result = match word {
        "passed" | "pass" => Reported::Passed,
        "failed" | "fail" => Reported::Failed,
        "not tested" | "not-tested" | "nottested" => Reported::NotTested,
        _ => return None,
    };
    Some((result, method))
}

fn method_named(how: &str) -> Option<Method> {
    match how {
        "automated test" | "automated-test" | "test" => Some(Method::AutomatedTest),
        "mocked ui" | "mocked-ui" | "mock" => Some(Method::MockedUi),
        "full runtime" | "full-runtime" | "runtime" => Some(Method::FullRuntime),
        "human" | "human-confirmed" | "a person" => Some(Method::HumanConfirmed),
        _ => None,
    }
}

/// Sądzi zatwierdzoną listę po tym, co weryfikator naprawdę zameldował.
///
/// Weryfikator nie może usunąć wymagania ani zwolnić się z niego: kryterium bez meldunku jest
/// brakiem raportu, a nie zgodą. Nie może też obniżyć metody — potwierdzenie mockiem tam,
/// gdzie zatwierdzono pełny runtime, jest odpowiedzią na inne pytanie.
#[must_use]
pub fn judge(approved: &[Criterion], said: &str) -> Judgement {
    let lines = reported_in(said);
    let mut judgement = Judgement {
        outcome: Outcome::Passed,
        missing: Vec::new(),
        unknown: Vec::new(),
        duplicated: Vec::new(),
        failed: Vec::new(),
        not_tested: Vec::new(),
        weaker_method: Vec::new(),
    };
    for line in &lines {
        if !approved.iter().any(|one| one.id == line.id) {
            if !judgement.unknown.contains(&line.id) {
                judgement.unknown.push(line.id.clone());
            }
        } else if lines.iter().filter(|other| other.id == line.id).count() > 1
            && !judgement.duplicated.contains(&line.id)
        {
            judgement.duplicated.push(line.id.clone());
        }
    }
    for one in approved {
        let Some(line) = lines.iter().find(|line| line.id == one.id) else {
            if one.required {
                judgement.missing.push(one.id.clone());
            }
            continue;
        };
        if !one.required {
            continue;
        }
        match line.result {
            Reported::Failed => judgement.failed.push(one.id.clone()),
            Reported::NotTested => judgement.not_tested.push(one.id.clone()),
            Reported::Passed => {
                if let Some(used) = line.method
                    && !one.method.is_met_by(used)
                {
                    judgement.weaker_method.push(one.id.clone());
                }
            }
        }
    }
    judgement.outcome = if !judgement.failed.is_empty() {
        // Zaobserwowana niespełniona wymagana rzecz jest wadą produktu i tak się ją nazywa,
        // nawet kiedy w tym samym raporcie czegoś nie zmierzono.
        Outcome::DidNotPass
    } else if judgement.missing.is_empty()
        && judgement.unknown.is_empty()
        && judgement.duplicated.is_empty()
        && judgement.not_tested.is_empty()
        && judgement.weaker_method.is_empty()
    {
        Outcome::Passed
    } else {
        Outcome::NotJudged
    };
    judgement
}

/// Zdejmuje potwierdzenia, których TA maszyna nie mogła wykonać.
///
/// P-03a: kryterium wymagające pełnej aplikacji, zameldowane jako zdane na maszynie, która
/// nie ma zgody na sterowanie cudzym oknem, nie jest potwierdzeniem. Nie jest też wadą
/// produktu — nikt niczego nie zmierzył. Wynik przechodzi więc do „nie zmierzono", czyli
/// do człowieka.
///
/// Egzekwowane KODEM, nie prośbą w prompcie: instrukcja „napisz not tested, jeśli nie masz
/// narzędzia" jest miękka dokładnie tam, gdzie model najchętniej zgaduje (niezmiennik 28).
pub fn without_a_native_route(approved: &[Criterion], judged: &mut Judgement) {
    let native: Vec<String> = approved
        .iter()
        .filter(|one| one.required && one.method == Method::FullRuntime)
        .map(|one| one.id.clone())
        .collect();
    if native.is_empty() {
        return;
    }
    let mut moved = false;
    for id in native {
        let already_open = judged.failed.contains(&id)
            || judged.missing.contains(&id)
            || judged.not_tested.contains(&id)
            || judged.weaker_method.contains(&id);
        if !already_open {
            judged.not_tested.push(id);
            moved = true;
        }
    }
    if moved {
        judged.not_tested.sort();
        judged.not_tested.dedup();
        judged.outcome = if judged.failed.is_empty() {
            Outcome::NotJudged
        } else {
            Outcome::DidNotPass
        };
    }
}

/// Blok promptu dla weryfikatora, który dostał zatwierdzoną listę.
///
/// Wzmacnia instrukcję **tego kroku**, nie wszystkich użyć uniwersalnego bloku o wyniku:
/// powstaje wyłącznie wtedy, kiedy człowiek naprawdę zatwierdził listę wymagań.
#[must_use]
pub fn asked_for(approved: &[Criterion]) -> String {
    let mut asked = String::from(
        "Answer about every requirement below, one line each, exactly like this:\n\n\
         criterion <id>: passed | failed | not tested — one sentence saying what you observed\n\n\
         Add `via full runtime`, `via mocked ui`, `via automated test` or `via human` after the \
         result when you confirmed it a different way than the one asked for. Write `not tested` \
         when a tool, a permission, a dependency or a trustworthy measurement was missing: that \
         is neither a pass nor a defect, and it goes to a person. A requirement you say nothing \
         about is not confirmed, and no wording anywhere else in your answer changes that.\n\n\
         What this step must confirm:\n",
    );
    for one in approved {
        asked.push('\n');
        asked.push_str(&one.id);
        asked.push_str(": ");
        asked.push_str(one.behaviour.trim());
        asked.push_str(" (confirm with ");
        asked.push_str(one.method.said());
        if one.required {
            asked.push_str("; this one is required)");
        } else {
            asked.push_str("; optional)");
        }
    }
    asked
}
