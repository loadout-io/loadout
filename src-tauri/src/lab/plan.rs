//! Zestaw → zwykły plik workflow. Jedna czysta funkcja i jeden schemat identyfikatorów.
//!
//! # Dlaczego graf, a nie własna pętla po przypadkach
//!
//! Bo pętla po przypadkach musiałaby mieć własną odpowiedź na „ile naraz", własne anulowanie,
//! własny sufit wydatku, własny dowód śmierci grupy i własne odzyskiwanie po awarii aplikacji.
//! Wszystkie te odpowiedzi już istnieją i wszystkie są trudne. Graf oddaje je za darmo i nie
//! kosztuje w `engine/` ani jednej linii — planista nie wie i nie ma prawa wiedzieć, że te
//! kroki powstały z zestawu (niezmiennik 27).
//!
//! # Kształt: jedna komórka to jeden albo dwa kroki
//!
//! ```text
//!            wariant A              wariant B
//!   case 1   [work]→[checks]        [work]→[checks]
//!   case 2   [work]→[checks]        [work]→[checks]
//! ```
//!
//! Komórki są **rozłączne** i to jest treść, nie oszczędność: przypadek drugi nie ma prawa
//! zależeć od tego, jak poszedł pierwszy, bo wtedy tabela mierzy kolejność, a nie pracę.
//! `workflow::check` zgłosi taki plik jako niepołączony — **ostrzeżeniem**, nie problemem
//! (`islands`), więc plan zapisze się i pobiegnie, a człowiek, który otworzy go na płótnie,
//! przeczyta prawdę: te kafelki naprawdę nie mają ze sobą nic wspólnego.
//!
//! # Dlaczego KAŻDA komórka dostaje własną kopię
//!
//! `Folder::FreshCopy` bez wyjątku, niezależnie od tego, co wolno agentowi wariantu. Dwa
//! powody, oba twarde. Pierwszy: dwa kroki nie mogą pisać po tych samych ścieżkach i odmowa
//! pada przy Starcie (niezmiennik 12) — komórki w jednym katalogu zabiłyby bieg, zanim ruszy
//! pierwszy proces. Drugi jest ważniejszy: **pomiar, który zmienia mierzony projekt, nie jest
//! pomiarem, tylko zmianą.** Zestaw puszczony dwa razy z rzędu w katalogu projektu mierzyłby
//! za drugim razem skutki pierwszego.

use std::collections::BTreeMap;

use serde_json::Map;

use crate::workflow::{
    AgentStep, CheckStep, Folder, Handover, HandoverField, Link, Point, Skills, Step, WhenItFails,
    WorkflowFile,
};

use super::{Case, EvalSet, Variant};

/// Rozdzielacz w identyfikatorze kroku planu.
///
/// Dwa znaki podkreślenia, a nie jeden: identyfikatory przypadków i wariantów są sluggami,
/// w których pojedynczy podkreślnik bywa (`case_1`), a podwójny nie bywa nigdy.
///
/// Zapis zestawu odmawia identyfikatorowi, który sam ten rozdzielacz zawiera
/// (`file::why_it_would_not_hold`), i to nie jest ostrożność, tylko **kolizja**: przypadek
/// `a__b` z kolumną `c` i przypadek `a` z kolumną `b__c` dają ten sam klucz kroku, więc jedna
/// z dwóch komórek czytałaby wynik drugiej.
pub const APART: &str = "__";

/// Prefiks kroku, który wykonuje pracę.
pub const WORK: &str = "work";

/// Prefiks kroku, który tę pracę sprawdza.
pub const CHECKS: &str = "checks";

/// Szerokość jednej kolumny w pikselach płótna — dziesięć skoków siatki na krok, dwa kroki
/// na kolumnę. Liczba jest wielokrotnością `workflow::GRID`, więc zapis niczego nie przyciąga
/// i plan nie brudzi się w gicie po samym otwarciu.
const COLUMN: f64 = 240.0;

/// Wysokość wiersza. Też wielokrotność skoku siatki.
const ROW: f64 = 120.0;

/// Która połowa komórki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// Krok agenta.
    Work,
    /// Krok „sprawdź", który orzeka o jego pracy.
    Checks,
}

/// Identyfikator kroku dla tej komórki.
///
/// Składany w JEDNYM miejscu i czytany z powrotem przez to samo złożenie: `lab::results` nie
/// rozbiera tego napisu, tylko woła tę funkcję jeszcze raz i szuka wyniku w mapie kroków biegu.
/// Rozbiór byłby drugą odpowiedzią na to samo pytanie i rozjechałby się z tą pierwszą przy
/// pierwszej zmianie kształtu klucza (niezmiennik 13).
#[must_use]
pub fn key_for(case: &str, variant: &str, half: Half) -> String {
    let prefix = match half {
        Half::Work => WORK,
        Half::Checks => CHECKS,
    };
    format!("{prefix}{APART}{case}{APART}{variant}")
}

/// Składa plan: przypadki **w użyciu** razy warianty.
///
/// Kandydatki nie wchodzą — filtruje je [`EvalSet::running_cases`], w jednym miejscu i z tego
/// samego powodu, dla którego notatka `suggested` nie trafia do żadnego promptu.
///
/// Plan jest **czystą funkcją zestawu**: ta sama para wejściowa daje ten sam plik, co do bajtu.
/// Dzięki temu dwa przebiegi tego samego zestawu różnią się wyłącznie tym, co powiedziały
/// modele — a nie tym, co złożył Loadout.
#[must_use]
pub fn compose(set: &EvalSet, id: String, name: String) -> WorkflowFile {
    let cases = set.running_cases();
    let mut steps: Vec<Step> = Vec::with_capacity(cases.len() * set.variants.len() * 2);
    let mut links: Vec<Link> = Vec::new();

    for (column, variant) in set.variants.iter().enumerate() {
        for (row, case) in cases.iter().enumerate() {
            let at = |half: Half| Point {
                // Kolumna zajmuje dwa sloty szerokości, bo krok „sprawdź" stoi po prawej od
                // swojej pracy — czyli tam, gdzie czyta się go jako „a potem".
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "indeks kolumny i wiersza to liczby rzedu dziesiatek; f64 trzyma je \
                              doklad­nie dlugo po tym, jak macierz przestanie miescic sie na ekranie"
                )]
                x: COLUMN * (column as f64 * 2.0 + if half == Half::Work { 0.0 } else { 1.0 }),
                #[expect(clippy::cast_precision_loss, reason = "jak wyzej — indeks wiersza")]
                y: ROW * row as f64,
            };

            let work = key_for(&case.id, &variant.id, Half::Work);
            steps.push(Step::Agent(work_step(
                case,
                variant,
                work.clone(),
                at(Half::Work),
            )));

            if case.command.trim().is_empty() {
                continue;
            }
            let checks = key_for(&case.id, &variant.id, Half::Checks);
            steps.push(Step::Check(checks_step(
                case,
                checks.clone(),
                at(Half::Checks),
            )));
            links.push(Link {
                from: work,
                to: checks,
                max_turns: None,
            });
        }
    }

    WorkflowFile {
        format: crate::workflow::file::CURRENT,
        id,
        name,
        description: Some(format!(
            "One run of the set \"{}\". Loadout writes this file; edit the set instead.",
            set.name
        )),
        steps,
        links,
        extra: Map::new(),
    }
}

/// Nazwa kroku pracy — **jedyne miejsce w repo, które ją składa**.
///
/// Stoi tu jako funkcja, a nie jako `format!` w miejscu użycia, bo czyta ją także
/// [`super::results`]: przekazanie zna krok, który je zostawił, wyłącznie po NAZWIE
/// (`memory::handoff::Meta::from`), więc ta nazwa jest kluczem złączenia. Druga kopia tego
/// napisu rozjechałaby się przy pierwszej zmianie separatora i wynik komórki przestałby się
/// odnajdywać — po cichu, bo brak przekazania czyta się dokładnie jak przekazanie puste.
///
/// Jednoznaczność bierze się z tego, że zapis zestawu odmawia dwóch przypadków o jednej nazwie
/// i dwóch kolumn o jednej nazwie (`file::why_it_would_not_hold`).
#[must_use]
pub fn work_name(case: &Case, variant: &Variant) -> String {
    format!("{} · {}", case.name.trim(), variant.name.trim())
}

/// Krok agenta jednej komórki.
fn work_step(case: &Case, variant: &Variant, id: String, at: Point) -> AgentStep {
    AgentStep {
        id,
        name: work_name(case, variant),
        agent: variant.agent.clone(),
        // Patch wariantu jedzie NIETKNIĘTY. Scalanie z definicją agenta mieszka
        // w `library::agents::resolve` i ma tam zostać — drugie scalanie tutaj rozjechałoby się
        // z tamtym przy pierwszym nowym polu agenta (niezmiennik 23).
        overrides: variant.overrides.clone(),
        vendor_options: BTreeMap::new(),
        copies: 1,
        instructions: case.task.clone(),
        skills: Skills::default(),
        borrow: crate::workflow::Borrow::default(),
        // Powód w całości stoi w nagłówku modułu: pomiar, który zmienia mierzony projekt, nie
        // jest pomiarem.
        folder: Folder::FreshCopy,
        handover: handover_for(case),
        // Praca, która nie wyszła, nie ma czego sprawdzać: krok „sprawdź" tej komórki zostaje
        // pominięty i komórka jest czerwona. Stożek kończy się na tej jednej komórce, bo
        // komórki nie mają między sobą ani jednej strzałki.
        when_it_fails: WhenItFails::Stop,
        at,
        extra: Map::new(),
    }
}

/// Krok „sprawdź" jednej komórki.
fn checks_step(case: &Case, id: String, at: Point) -> CheckStep {
    CheckStep {
        id,
        name: format!("{} · checks", case.name.trim()),
        command: case.command.clone(),
        proof: case.proof.clone(),
        // W drzewie, które właśnie zbudowała praca tej komórki — inaczej komenda sprawdzałaby
        // katalog, w którym nic się nie wydarzyło.
        folder: Folder::SameCopy,
        // Po kroku „sprawdź" tej komórki nie stoi nic, więc ta wartość nie ma stożka do
        // pomalowania. Zostaje domyślna, żeby nie wchodziła do pliku i nie sugerowała wyboru,
        // którego nikt nie dokonał.
        when_it_fails: WhenItFails::default(),
        at,
        extra: Map::new(),
    }
}

/// Umówione pola tej komórki — i **ani słowa o tym, czego się w nich spodziewamy**.
///
/// # To jest cała treść tej funkcji
///
/// Pole `Expect::contains` **nie wchodzi do promptu**. Prompt mówiący „w tym polu ma paść słowo
/// X" mierzy, czy model umie przepisać słowo X — a nie to, czy potrafi wykonać pracę, po
/// której to słowo pada. Dokładnie ta sama pułapka stoi o warstwę wyżej przy pisaniu
/// kandydatek: przypadek napisany z tekstu, który testuje, przechodzi, bo z niego pochodzi.
///
/// Do promptu wchodzi więc **nazwa pola** i opis, który napisał człowiek. Obecności pola
/// pilnuje bieg (`commands::run::missing_a_required_field`), a jego treści — [`super::results`],
/// już po fakcie, z zapisanego przekazania.
fn handover_for(case: &Case) -> Handover {
    if case.expect.is_empty() {
        return Handover::default();
    }
    Handover::Form {
        fields: case
            .expect
            .iter()
            .map(|expect| HandoverField {
                name: expect.field.trim().to_owned(),
                describe: if expect.describe.trim().is_empty() {
                    "what you found".to_owned()
                } else {
                    expect.describe.trim().to_owned()
                },
                required: Some(true),
            })
            .collect(),
    }
}
