//! Kandydatki na przypadki: o co pytamy agenta i jak czytamy jego odpowiedź.
//!
//! # Dlaczego pytanie mówi „przeczytaj PROJEKT", a nie „przeczytaj tego agenta"
//!
//! Bo przypadek napisany z tekstu, który ma testować, przechodzi — z niego pochodzi. Agent,
//! któremu każe się wymyślić sprawdziany dla `SKILL.md`, przeczyta ten plik i wypisze
//! dokładnie te sytuacje, które on obsługuje; wynik takiego zestawu jest zawsze wysoki i nie
//! znaczy nic. To jest ta sama choroba, przed którą stoi niezmiennik 22 („ewaluacja nie
//! mieszka wewnątrz systemu, który mierzy"), o jedną warstwę wyżej.
//!
//! Materiał przychodzi więc **z zewnątrz mierzonej rzeczy**: z prawdziwych plików projektu,
//! z jego komendy sprawdzającej, z jego zadań. Wyrocznia bierze się z tego samego miejsca — bo
//! projekt, który ma czym sądzić własny kod, ma czym osądzić także pracę agenta nad nim.
//!
//! # Dlaczego kandydatka bez `because` jest odrzucana
//!
//! Zmierzone przy notatkach i zapisane w `memory::notes`: reguła bez uzasadnienia jest regułą,
//! której człowiek nie umie ocenić, więc klika „accept" na wszystkim albo na niczym. Przy
//! przypadku jest gorzej niż przy notatce: przypadek bez pochodzenia to zwykle przypadek
//! wymyślony, a wymyślony przypadek mierzy wyobraźnię modelu.

use std::collections::BTreeSet;

use serde_json::Map;

use super::{Case, CaseStatus, Expect, Subject, slugify};

/// Nagłówek, którym zaczyna się każda kandydatka w odpowiedzi.
const OPENS: &str = "## Case";

/// O co prosimy agenta. Kształt pokazany wprost, bo model kopiuje ten, który zobaczy.
///
/// **Nie prosimy o JSON.** Odpowiedź agenta jest prozą, a proza z wierszami `klucz: wartość`
/// jest tym, co ten produkt czyta wszędzie indziej (`memory::handoff::fields_said_in`).
/// Poproszony o JSON model owija go w komentarz, ucina na limicie i wkleja w blok kodu
/// z językiem — a wtedy parser albo przewraca całą turę, albo cicho oddaje zero pozycji.
const HOW_TO_ANSWER: &str = "\
Write one block for each case, in this exact shape, and nothing else between the blocks:

## Case
name: a short label a person can read in a table row
task: what the agent under test will be asked to do, written as an instruction to it
because: the file, or file and line, in this project that made you write this case
command: a command from this project that says whether the work is right — leave the line out \
when there is none
proof: text that has to appear in that command's output for it to count as passed
expect: fieldName = something the answer has to mention

The `expect` line may repeat, once per field, and may be left out. Anything you cannot ground \
in a file you actually read, leave out: a case with no `because` line is thrown away unread.";

/// Pytanie o kandydatki, złożone dla jednego zestawu.
///
/// `how_many` jest **sufitem, nie zamówieniem**: model poproszony o dokładnie dziesięć dopisze
/// dziesiątą z niczego. Ta sama zasada, co przy notatkach — lepiej trzy ugruntowane niż dziesięć
/// wymyślonych.
#[must_use]
pub fn ask_for_cases(subject: &Subject, how_many: usize) -> String {
    let about = match subject {
        Subject::Agent { .. } => {
            "You are writing test cases for another agent that works in this project."
        }
        Subject::Skill { .. } => {
            "You are writing test cases for a skill that another agent will have in this project."
        }
    };
    format!(
        "{about}

Read this project — its code, its tests, the command it uses to check itself, its README — and \
write up to {how_many} cases that a person could hand to that agent as real work. Ground every \
one of them in something you actually opened.

Do not read, quote, or design around the agent or skill being tested. A case written from the \
thing it measures passes because it came from it, and measures nothing.

Prefer work this project can judge by itself: something its own test command, linter or build \
already has an opinion about. A case that nothing can judge is worth less than no case at all.

{HOW_TO_ANSWER}"
    )
}

/// Co wyszło z odczytu odpowiedzi.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposed {
    /// Kandydatki gotowe do pokazania człowiekowi, wszystkie ze statusem `suggested`.
    pub cases: Vec<Case>,
    /// Ile bloków odpadło, bo nie miały pochodzenia.
    ///
    /// Liczba, nie cisza: „przyszło sześć, zapisano cztery" jest zdaniem, które człowiek umie
    /// sprawdzić. Ciche odrzucenie uczy go, że model zwraca mniej, niż zwraca.
    pub without_a_reason: usize,
    /// Ile odpadło, bo nie miały nazwy albo zadania.
    pub unfinished: usize,
}

/// Czyta odpowiedź agenta na [`ask_for_cases`].
///
/// **Leniwie wobec kształtu, surowo wobec treści** (niezmiennik 5). Nieznany klucz jest
/// pomijany, blok bez nagłówka nie istnieje, tekst przed pierwszym nagłówkiem jest prozą, którą
/// model napisał do człowieka — a blok bez nazwy, zadania albo pochodzenia jest odrzucany
/// i policzony.
///
/// `taken` to identyfikatory, które w zestawie już są: kandydatka nie ma prawa przejąć adresu
/// przypadku, który człowiek zaakceptował wcześniej, bo wtedy jego wynik zniknąłby z tabeli
/// bez jednego zdania.
#[must_use]
pub fn read(said: &str, taken: &BTreeSet<String>) -> Proposed {
    let mut proposed = Proposed {
        cases: Vec::new(),
        without_a_reason: 0,
        unfinished: 0,
    };
    let mut used: BTreeSet<String> = taken.clone();

    for block in blocks(said) {
        let mut name = String::new();
        let mut task = String::new();
        let mut because = String::new();
        let mut command = String::new();
        let mut proof = String::new();
        let mut expect: Vec<Expect> = Vec::new();

        for line in block.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.trim().to_lowercase().as_str() {
                "name" => value.clone_into(&mut name),
                "task" => value.clone_into(&mut task),
                "because" => value.clone_into(&mut because),
                "command" => value.clone_into(&mut command),
                "proof" => value.clone_into(&mut proof),
                "expect" => {
                    if let Some(one) = expectation(value) {
                        expect.push(one);
                    }
                }
                // Nieznany klucz jest pomijany, nie jest błędem: model dopisze kiedyś wiersz,
                // o który nikt nie prosił, a jedna taka linia nie ma prawa skasować całej tury.
                _ => {}
            }
        }

        if name.is_empty() || task.is_empty() {
            proposed.unfinished += 1;
            continue;
        }
        if because.is_empty() {
            proposed.without_a_reason += 1;
            continue;
        }

        // Komenda bez wzorca dowodu spadłaby na sam kod wyjścia, a suita, która nie uruchomiła
        // ani jednego testu, kończy się zerem (niezmiennik 19). Model, który podał jedno bez
        // drugiego, nie dostaje połowy mechanizmu — dostaje zero, a człowiek widzi dwa puste
        // pola i wie, że ma je wypełnić razem albo wcale.
        let (command, proof) = if command.is_empty() || proof.is_empty() {
            (String::new(), String::new())
        } else {
            (command, proof)
        };

        let id = free_id(&name, &mut used);
        proposed.cases.push(Case {
            id,
            name,
            task,
            expect,
            command,
            proof,
            status: CaseStatus::Suggested,
            because,
            extra: Map::new(),
        });
    }

    proposed
}

/// Bloki odpowiedzi, licząc od każdego nagłówka [`OPENS`].
///
/// Dopasowanie po **początku wiersza po obcięciu spacji**, a nie po podciągu: zdanie „I wrote
/// one ## Case for each file" w środku prozy nie jest nagłówkiem, a parser szukający podciągu
/// zaczynałby od niego blok, którego nikt nie napisał.
fn blocks(said: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in said.lines() {
        if line.trim().eq_ignore_ascii_case(OPENS) {
            if let Some(lines) = current.take() {
                out.push(lines.join("\n"));
            }
            current = Some(Vec::new());
            continue;
        }
        if let Some(lines) = current.as_mut() {
            lines.push(line);
        }
    }
    if let Some(lines) = current {
        out.push(lines.join("\n"));
    }
    out
}

/// `fieldName = something` → oczekiwanie. Bez znaku równości: samo pole, bez treści.
fn expectation(value: &str) -> Option<Expect> {
    let (field, contains) = value.split_once('=').unwrap_or((value, ""));
    let field = field.trim();
    if field.is_empty() {
        return None;
    }
    Some(Expect {
        field: field.to_owned(),
        contains: contains.trim().to_owned(),
        describe: String::new(),
    })
}

/// Wolny identyfikator dla tej nazwy, z licznikiem, kiedy slug jest już zajęty.
///
/// Licznik, a nie odrzucenie: dwie kandydatki o zbliżonych nazwach są normalnym wynikiem tury,
/// a człowiek, który chce obu, nie ma jak zmusić modelu do wymyślenia innego slugu.
fn free_id(name: &str, used: &mut BTreeSet<String>) -> String {
    let base = slugify(name);
    if used.insert(base.clone()) {
        return base;
    }
    // Od dwójki, bo pierwszy zajęty nazywa się bez liczby i „case-1" obok „case" czytałoby się
    // jak dwie połowy jednej pary, a nie jak oryginał i jego sąsiad.
    for suffix in 2..u32::MAX {
        let candidate = format!("{base}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    base
}
