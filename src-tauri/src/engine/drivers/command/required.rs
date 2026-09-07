//! V-01: KTÓRE testy przeszły, nie ile ich było.
//!
//! # Po co ten moduł istnieje
//!
//! Niezmiennik 19 pilnuje licznika przejść: `exit 0` bez ani jednego zameldowanego przejścia
//! jest czerwone. To zamyka jedną klasę kłamstwa i nie tyka drugiej. Zmierzone 2026-09-06
//! (incydenty I-03 i I-04, bieg `20260906-165702`): krok Backend miał dopisać trzynaście
//! testów, jego `cd src-tauri` nie powiodło się, a filtry, którymi „potwierdzał" pracę,
//! uruchamiały zero albo **dwa niezwiązane** testy. Licznik był dodatni, kod wyjścia zerowy,
//! wzorzec dowodu trafiał — i krok był zielony. Zdanie końcowe agenta nie ujawniło niczego,
//! bo agent opisywał, co zamierzał, a nie co się wykonało.
//!
//! Adaptery mapują dwa kształty wyjścia na jeden kontrakt (niezmiennik 23: polityka w rdzeniu,
//! adapter ma pięć linii):
//!
//! - **libtest** — `cargo test`: `test <id> ... ok` / `... FAILED` / `... ignored`;
//! - **TAP** — runner frontendowy uruchomiony z reporterem TAP: `ok <n> - <id>`,
//!   `not ok <n> - <id>`.
//!
//! Domyślny reporter vitesta nie wypisuje pojedynczych przypadków, więc jego wyjście nie ma
//! tożsamości testu, którą dałoby się sprawdzić. To nie jest brak adaptera, tylko brak danych:
//! sprawdzenie z listą wymaganych testów musi poprosić runner o TAP.

use serde::Serialize;

/// Co zrobił jeden test, niezależnie od runnera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ran {
    Passed,
    Failed,
    /// Wykonanie pominięte przez sam runner (`ignored`, `# SKIP`). Pominięty test nie
    /// potwierdza niczego, ale też nie jest porażką produktu.
    Skipped,
}

/// Jeden test rozpoznany w wyjściu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub id: String,
    pub ran: Ran,
}

/// Co się stało z wymaganymi testami tego sprawdzenia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredTests {
    /// Wymagane i potwierdzone: wykonane oraz zdane.
    pub confirmed: Vec<String>,
    /// Wymagane, wykonane, niezdane.
    pub did_not_pass: Vec<String>,
    /// Wymagane i nieobecne w wyjściu: nie ma dowodu, że w ogóle się wykonały.
    pub did_not_run: Vec<String>,
}

/// Ile nazw wchodzi do jednego zdania, zanim zostaje sam licznik.
const NAMES_IN_A_SENTENCE: usize = 3;

impl RequiredTests {
    /// Czy wymagania tego sprawdzenia są spełnione.
    ///
    /// Testy DODATKOWE nie wchodzą do tej odpowiedzi w ogóle i to jest cała treść V-01:
    /// suita może urosnąć o pięć nowych przejść i nadal nie potwierdzać tego, co miała.
    #[must_use]
    pub fn all_confirmed(&self) -> bool {
        self.did_not_pass.is_empty() && self.did_not_run.is_empty()
    }

    /// Zdanie dla człowieka. Puste, kiedy nie ma czego zgłosić.
    ///
    /// Dwa fakty stoją osobno, bo naprawia się je inaczej: test, który padł, mówi o produkcie;
    /// test, którego nie było, mówi o komendzie albo o katalogu, w którym ją uruchomiono.
    #[must_use]
    pub fn said(&self) -> String {
        let total = self.confirmed.len() + self.did_not_pass.len() + self.did_not_run.len();
        let mut parts = Vec::new();
        if !self.did_not_run.is_empty() {
            parts.push(format!(
                "did not run {} of the {total} tests it must confirm ({})",
                self.did_not_run.len(),
                names(&self.did_not_run)
            ));
        }
        if !self.did_not_pass.is_empty() {
            parts.push(format!(
                "{} of them did not pass ({})",
                self.did_not_pass.len(),
                names(&self.did_not_pass)
            ));
        }
        if parts.is_empty() {
            return String::new();
        }
        format!("This check {}.", parts.join(", and "))
    }
}

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

/// Sufit niedokończonej linii. Runner, który pisze megabajt bez znaku nowej linii, nie pisze
/// wyniku testu — i nie ma prawa zjeść pamięci procesu, który go czyta.
const LONGEST_LINE: usize = 16 * 1024;

/// Śledzi wymagane testy **w locie**, kawałek po kawałku.
///
/// # Dlaczego strumieniowo, a nie z zebranego tekstu
///
/// Bo zebranego tekstu nie ma. Krok „sprawdź" zachowuje wyłącznie OSTATNIE 64 KiB wyjścia
/// (`KEEP_LAST`) — pełny strumień nigdy nie powstaje ani jako `Vec`, ani jako `String`,
/// i to jest rozstrzygnięcie sprzed tego zadania. Sądzenie po ogonie mówiłoby „ten test się
/// nie wykonał" o każdym teście prawdziwej suity, która wypisała więcej: dokładnie to samo
/// kłamstwo, które ten moduł ma zamykać, tylko od drugiej strony.
///
/// Ten sam kształt, co [`ProofScan`](super::ProofScan): stan żywy, wejście dowolnie pocięte.
#[derive(Debug)]
pub struct Scan {
    /// Wymagane testy w kolejności, w jakiej podał je człowiek, i co dotąd o nich wiadomo.
    wanted: Vec<(String, Option<Ran>)>,
    /// Ostatnia, jeszcze niedokończona linia.
    line: String,
    /// Czy bieżąca linia przekroczyła sufit i jest porzucana do najbliższego `\n`.
    too_long: bool,
}

impl Scan {
    #[must_use]
    pub fn new(required: &[String]) -> Self {
        Self {
            wanted: required
                .iter()
                .map(|one| (one.trim().to_owned(), None))
                .collect(),
            line: String::new(),
            too_long: false,
        }
    }

    /// Dowolny kawałek wyjścia. Linie sklejamy same, bo granica porcji z potoku nie ma
    /// nic wspólnego z granicą linii.
    pub fn take(&mut self, text: &str) {
        for part in text.split_inclusive('\n') {
            if let Some(rest) = part.strip_suffix('\n') {
                if self.too_long {
                    self.line.clear();
                    self.too_long = false;
                } else {
                    self.line.push_str(rest);
                    let line = std::mem::take(&mut self.line);
                    self.record(&line);
                }
            } else if self.line.len() + part.len() > LONGEST_LINE {
                self.too_long = true;
                self.line.clear();
            } else {
                self.line.push_str(part);
            }
        }
    }

    /// Dopasowanie jest DOKŁADNE. Nazwa podana przez człowieka jest tożsamością testu, a nie
    /// wzorcem: dopasowanie po fragmencie zamienia „wymagam `wanted::second`" w „wystarczy mi
    /// cokolwiek, co tak się zaczyna" — czyli w tę samą zgodę na przybliżenie, przez którą
    /// dwa niezwiązane testy uchodziły za trzynaście.
    fn record(&mut self, line: &str) {
        let Some(seen) = one_line(line.trim()) else {
            return;
        };
        if let Some((_, ran)) = self.wanted.iter_mut().find(|(id, _)| *id == seen.id) {
            *ran = Some(seen.ran);
        }
    }

    #[must_use]
    pub fn finish(mut self) -> RequiredTests {
        if !self.line.is_empty() && !self.too_long {
            let line = std::mem::take(&mut self.line);
            self.record(&line);
        }
        let mut result = RequiredTests {
            confirmed: Vec::new(),
            did_not_pass: Vec::new(),
            did_not_run: Vec::new(),
        };
        for (id, ran) in self.wanted {
            match ran {
                Some(Ran::Passed) => result.confirmed.push(id),
                Some(Ran::Failed) => result.did_not_pass.push(id),
                // Pominięty przez runner test nie wykonał się, więc niczego nie potwierdza.
                Some(Ran::Skipped) | None => result.did_not_run.push(id),
            }
        }
        result
    }
}

fn one_line(line: &str) -> Option<Seen> {
    libtest_line(line.trim()).or_else(|| tap_line(line.trim()))
}

/// `test some::path ... ok`, `... FAILED`, `... ignored`.
fn libtest_line(line: &str) -> Option<Seen> {
    let rest = line.strip_prefix("test ")?;
    let (id, verdict) = rest.rsplit_once(" ... ")?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    // libtest dokleja do werdyktu powód pominięcia (`ignored, needs a display`).
    let verdict = verdict.split(',').next().unwrap_or(verdict).trim();
    let ran = match verdict {
        "ok" => Ran::Passed,
        "FAILED" => Ran::Failed,
        "ignored" => Ran::Skipped,
        _ => return None,
    };
    Some(Seen {
        id: id.to_owned(),
        ran,
    })
}

/// `ok 1 - name`, `not ok 2 - name`, `ok 3 - name # SKIP`.
fn tap_line(line: &str) -> Option<Seen> {
    let (failed, rest) = match line.strip_prefix("not ok ") {
        Some(rest) => (true, rest),
        None => (false, line.strip_prefix("ok ")?),
    };
    // Numer przypadku należy do protokołu, nie do tożsamości testu.
    let rest = rest.trim_start_matches(|character: char| character.is_ascii_digit());
    let name = rest
        .trim_start()
        .strip_prefix("- ")
        .unwrap_or(rest.trim_start());
    let (name, directive) = match name.split_once(" # ") {
        Some((name, directive)) => (name, Some(directive)),
        None => (name, None),
    };
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let skipped = directive.is_some_and(|one| {
        let one = one.trim().to_ascii_uppercase();
        one.starts_with("SKIP") || one.starts_with("TODO")
    });
    Some(Seen {
        id: name.to_owned(),
        ran: if skipped {
            Ran::Skipped
        } else if failed {
            Ran::Failed
        } else {
            Ran::Passed
        },
    })
}
