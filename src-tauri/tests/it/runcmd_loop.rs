//! Pętla z limitem tur na ŻYWYM biegu: dwa końce, wyjście po `pass` i odmowa po wyczerpaniu prób.
//!
//! # Dlaczego ten plik jest konieczny
//!
//! Wszystkie części pętli były dowiedzione osobno: rozwinięcie grafu (`workflow::unroll`), werdykt
//! z ciała przekazania (`memory::handoff::verdict_in`), zakres tur i reguła koła
//! (`workflow::check`), klucze węzłów (`commands::run::tests`). Ani jedno kryterium nie sprawdzało
//! ich SKLEJKI — a pętla to właśnie sklejka: planista rozwija, sterownik mówi, strażnik pomija,
//! planista zatrzymuje. Każdy z tych czterech może być poprawny osobno i nie spotkać się
//! z pozostałymi.
//!
//! # Co dokładnie mierzą te dwa kryteria
//!
//! Licznik startów sterownika **per prompt**, bo `RunSpec` nie niesie numeru kroku — jego
//! instrukcje są jedyną rzeczą, po której da się kroki rozróżnić (niezmiennik 9). Runda pominięta
//! nie woła sterownika w ogóle, więc licznik jest jedynym miejscem, w którym różnica między
//! „runda przeszła" i „rundy nie było" jest widoczna z zewnątrz.
//!
//! **SŁABĄ WERSJĄ pierwszego kryterium** jest sprawdzenie, że krok za pętlą się wykonał.
//! Przechodzi ją implementacja, która przepala WSZYSTKIE rundy i dopiero potem idzie dalej —
//! czyli ta, w której limit tur kosztuje trzy razy tyle, ile powinien, i nikt tego nie widzi,
//! bo wynik jest ten sam. Dlatego asercja stoi na LICZBIE startów sędziego.
//!
//! **SŁABĄ WERSJĄ drugiego** jest sprawdzenie, że bieg wrócił błędem. Przechodzi ją implementacja,
//! w której krok za pętlą już się wykonał, a bieg zameldował porażkę PO nim. Dlatego asercja stoi
//! na tym, że sterownik nigdy nie zobaczył promptu kroku za pętlą — bo to jest cały powód, dla
//! którego limit tur istnieje: zła robota nie ma pojechać dalej.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::handoff;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Sufit cierpliwości jednego biegu. Cztery kroki dublera nie mają jak trwać dłużej.
const PATIENCE: Duration = Duration::from_secs(20);

/// Prompt sędziego pętli — jedyna rzecz, po której dubler go rozpoznaje.
const JUDGE_PROMPT: &str = "Run the suite and say whether it passed.";

/// Prompt kroku ZA pętlą. Jego pojawienie się u dublera znaczy „praca pojechała dalej".
const AFTER_PROMPT: &str = "Ship it.";

/// Instrukcja implementera. Ten krok jest CIAŁEM pętli, nie jej sędzią.
const WORK_PROMPT: &str = "Make the change.";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000c1
name: Hand
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// `implement → tester → ship`, i tester z powrotem do implementera, do trzech rund.
///
/// Każdy krok dostaje WŁASNĄ kopię plików: rundy jednego kroku dzielą katalog (i o to chodzi),
/// ale implementer i tester to dwa różne kroki, a te dwa nie mogą biec w jednym folderze przy
/// limicie dwóch naraz. Bez tego plik jest odmową z `one_folder_two_steps`, a nie fiksturą.
const LOOP_FILE: &str = r#"{
  "format": 1,
  "id": "wf_loop",
  "name": "Implement and test",
  "steps": [
    {
      "kind": "agent",
      "id": "s_impl",
      "name": "Implement",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "copies": 1,
      "instructions": "Make the change.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_test",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "copies": 1,
      "instructions": "Run the suite and say whether it passed.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_ship",
      "name": "Ship",
      "agent": "01990000-0000-7000-8000-0000000000c1",
      "overrides": {},
      "copies": 1,
      "instructions": "Ship it.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 312 }
    }
  ],
  "links": [
    { "from": "s_impl", "to": "s_test" },
    { "from": "s_test", "to": "s_ship" },
    { "from": "s_test", "to": "s_impl", "max_turns": 3 }
  ]
}"#;

#[tokio::test]
async fn the_loop_stops_at_the_first_pass() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    /* Sędzia przepuszcza robotę w DRUGIEJ rundzie. Nie w pierwszej: pętla, która domyka się od
     * razu, nie odróżnia „wyszedł po `pass`" od „nigdy nie zawrócił". Nie w trzeciej: wtedy
     * wyjście po werdykcie jest nieodróżnialne od wyczerpania limitu. */
    let watch = Arc::new(Watch::passing_on_turn(2));

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let report = one_run(&deps, &request).await??;

    assert_eq!(
        watch.times(JUDGE_PROMPT),
        2,
        "the tester passed on its second try, so a third must never have started. Three starts \
         mean the run burned a whole agent turn on work nobody needed, and the result looks \
         identical from the outside. The driver saw: {:?}",
        watch.seen()
    );
    assert_eq!(
        watch.times(AFTER_PROMPT),
        1,
        "and the step after the loop has to run exactly once — that is what passing IS for"
    );
    /* Sześć węzłów: trzy rundy implementera i trzy testera, plus krok za pętlą. Wszystkie
     * `Succeeded`, także te pominięte — planista zmniejsza stopień wejściowy dzieci WYŁĄCZNIE po
     * tym stanie, więc gdyby pominięta runda wróciła czymkolwiek innym, `Ship` nie ruszyłby
     * nigdy. Że runda nie biegła, widać po liczniku startów wyżej, nie po jej stanie. */
    assert_eq!(
        report.steps.len(),
        7,
        "three turns of two steps plus the step after the loop; the report has {:?}",
        report.steps
    );
    assert!(
        report.steps.iter().all(|one| *one == StepState::Succeeded),
        "a loop that passed leaves nothing failed behind; it left {:?}",
        report.steps
    );
    Ok(())
}

#[tokio::test]
async fn the_work_after_the_loop_never_starts_when_the_tries_run_out() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    // Sędzia nie przepuszcza nigdy: `passing_on_turn` większe niż liczba rund w pliku.
    let watch = Arc::new(Watch::passing_on_turn(99));

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let report = one_run(&deps, &request).await??;

    assert_eq!(
        watch.times(AFTER_PROMPT),
        0,
        "THIS is the whole reason the limit exists: work that never passed must not go on. A run \
         that reports failure AFTER shipping has already shipped. The driver saw: {:?}",
        watch.seen()
    );
    assert_eq!(
        watch.times(JUDGE_PROMPT),
        3,
        "and all three tries have to be spent — a limit of three that gives up after two is a \
         different promise than the one on the arrow"
    );
    assert!(
        report.steps.contains(&StepState::Failed),
        "the run has to end red, or nothing tells the person their work did not pass; it ended \
         {:?}",
        report.steps
    );
    Ok(())
}

/// Jeden bieg z limitem cierpliwości. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny — czym.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let drain = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("agents").join(format!("{slug}.md")),
            text,
        )?;
        Ok(())
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    Arc::new(move |_| {
        Arc::new(Fake {
            watch: Arc::clone(&watch),
        }) as Arc<dyn AgentDriver>
    })
}

/// Co dubler widział i kiedy sędzia ma przepuścić robotę.
struct Watch {
    seen: Mutex<Vec<String>>,
    /// W której rundzie sędziego (licząc od jedynki) werdykt ma brzmieć `pass`.
    passing_on: usize,
    /// Czy sędzia w ogóle zapisuje wiersz wyniku.
    ///
    /// `false` odtwarza to, co robili WSZYSCY prawdziwi sędziowie właściciela: piszą prozą
    /// „PASS, przyjąć" i nie zostawiają wiersza, który Loadout umie przeczytać. Na 80 jego
    /// przekazaniach wiersz `outcome:` nie padł ani razu.
    says_how_it_went: bool,
}

impl Watch {
    fn passing_on_turn(turn: usize) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            passing_on: turn,
            says_how_it_went: true,
        }
    }

    /// Sędzia, który pracuje i nie zapisuje wyniku tam, gdzie go czytamy.
    fn never_saying_how_it_went() -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            passing_on: usize::MAX,
            says_how_it_went: false,
        }
    }

    /// Zapisuje start i oddaje tekst, którym ta tura się skończy.
    ///
    /// Werdykt liczony z LICZBY startów sędziego, bo prompt jest w każdej rundzie identyczny —
    /// i to jest właściwa fikstura: sędzia nie wie, którą rundę biegnie, dokładnie jak prawdziwy
    /// agent w nowej sesji.
    fn entered(&self, prompt: &str) -> String {
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.push(prompt.to_owned());
        if !prompt.contains(JUDGE_PROMPT) {
            return "Done.".to_owned();
        }
        let turn = seen.iter().filter(|one| one.contains(JUDGE_PROMPT)).count();
        if !self.says_how_it_went {
            return format!("I looked at try {turn}. A few things read better now.");
        }
        if turn >= self.passing_on {
            return "All green.\n\nOUTCOME: PASS".to_owned();
        }
        format!("Two tests are red on try {turn}.\n\nOUTCOME: FAIL")
    }

    /// Ile razy dubler zobaczył prompt zawierający ten fragment.
    fn times(&self, needle: &str) -> usize {
        self.lock()
            .iter()
            .filter(|one| one.contains(needle))
            .count()
    }

    fn seen(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Fake {
    watch: Arc<Watch>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let said = self.watch.entered(&spec.prompt);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;

        Ok(Box::new(Turn {
            events,
            session,
            said,
        }))
    }
}

/// Jedna tura dublera. `said` staje się ciałem przekazania — i to z niego czyta się werdykt.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/* 2026-08-23 — KONTRAKT, KTÓREGO DRUGA POŁOWA NIGDY NIE POWSTAŁA.
 *
 * `memory::handoff::verdict_in` czyta wynik sędziego z całego wiersza `outcome: pass`, a jego
 * własny komentarz twierdzi: „Sędzia dostaje w prompcie zdanie o tym, jak zapisać werdykt".
 * Nie dostawał. Na 80 przekazaniach z ośmiu prawdziwych biegów wiersz `outcome:` nie padł ANI
 * RAZU, więc każda pętla przepalała komplet rund i kończyła się `Failed` — także wtedy, gdy
 * sędzia napisał prozą „## Werdykt: **PASS** … przyjąć". Pod taką pętlą schodził cały stożek.
 *
 * DLACZEGO DWA ISTNIEJĄCE KRYTERIA TEGO NIE ŁAPAŁY. Bo `Watch::entered` oddaje `OUTCOME: PASS`
 * SAM Z SIEBIE. Fikstura znała kontrakt, którego produkt nikomu nie mówił — i to jest ta klasa
 * wady, w której test jest zielony dokładnie dlatego, że nie pyta o produkt. Oba tamte kryteria
 * zostają zielone, gdy `ask_for_an_outcome` skasować co do bajtu.
 *
 * DLATEGO NIŻEJ NIE MA ASERCJI „prompt zawiera napis". Wiersz WYJĘTY Z PROMPTU jedzie do
 * NASZEGO parsera. Jeżeli poprosimy sędziego o cokolwiek, czego `verdict_in` nie przyjmie,
 * kryterium pada — dwie połowy kontraktu spotykają się w jednym miejscu i nie mają jak się
 * rozjechać po cichu.
 */

/// Wyjmuje wiersz, o który prompt prosi w odwrotnych apostrofach — albo `None`, gdy nie prosi.
///
/// Odwrotne apostrofy są tu wymogiem, nie ozdobą: bez nich `prompt.contains("outcome: pass")`
/// trafiałby także w zdanie „napisz outcome: pass na końcu", czyli w opowieść o wierszu zamiast
/// w sam wiersz. Parser czyta CAŁY wiersz, więc kryterium musi pytać o dokładnie tę rzecz.
fn asked_of_the_judge(prompt: &str, which: &str) -> Option<String> {
    let wanted = format!("outcome: {which}");
    prompt.contains(&format!("`{wanted}`")).then_some(wanted)
}

#[tokio::test]
async fn the_tester_is_told_how_to_say_how_it_went() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::passing_on_turn(2));

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };
    one_run(&deps, &request).await??;

    let seen = watch.seen();
    let judged: Vec<&String> = seen
        .iter()
        .filter(|one| one.contains(JUDGE_PROMPT))
        .collect();
    assert!(
        !judged.is_empty(),
        "the fixture is wrong if the tester never started; the driver saw {seen:?}"
    );

    for prompt in &judged {
        let pass = asked_of_the_judge(prompt, "pass").ok_or(
            "the tester was never told how to say the work is good enough to build on, so the              only thing it can do is guess — and a guess is read as fail",
        )?;
        let fail = asked_of_the_judge(prompt, "fail")
            .ok_or("and it was never told how to send the work back either")?;

        assert_eq!(
            handoff::verdict_in(&pass),
            handoff::Verdict::Pass,
            "we ask the tester for a line our own reader does not accept. That is how a whole              run dies quietly: the tester answers exactly as asked, we read nothing, and              nothing is what we call fail. We asked for: {pass:?}"
        );
        /* `pass` PRZED `fail` i oczekiwany `Fail`. Sama asercja `verdict_in(fail) == Fail`
         * przeszłaby dla PUSTEGO napisu — `Fail` jest wartością domyślną — czyli także wtedy,
         * gdy parser tego wiersza w ogóle nie widzi. Postawiony za `pass` musi go przebić,
         * a to potrafi wyłącznie wiersz naprawdę przeczytany. */
        assert_eq!(
            handoff::verdict_in(&format!("{pass}\n{fail}")),
            handoff::Verdict::Fail,
            "the last word has to win, so the line we hand the tester for sending work back              must actually be read — not merely fall through to the default. We asked for:              {fail:?}"
        );
    }

    /* I DRUGA STRONA, bez której powyższe przeszłoby dla promptu doklejanego wszystkim.
     * Prośba o wynik skierowana do kroku, którego wyniku nikt nie czyta, jest poleceniem bez
     * skutku — tym samym, co kontrolka bez handlera (niezmiennik 16). */
    let mut plain = 0_usize;
    for prompt in &seen {
        if prompt.contains(WORK_PROMPT) || prompt.contains(AFTER_PROMPT) {
            plain += 1;
            assert!(
                !prompt.contains("outcome:"),
                "a step nobody judges was asked to hand down an outcome. Its answer is read by                  no one, so the sentence is an order with no effect — and it teaches the model                  to write a line that means nothing here. The prompt was: {prompt:?}"
            );
        }
    }
    assert!(
        plain > 0,
        "the fixture is wrong if neither the step inside the loop nor the one after it ever ran"
    );
    Ok(())
}

/* 2026-08-23 — CZERWONY KROK BEZ ANI JEDNEGO ZDANIA.
 *
 * Do dziś krok, którego sędzia nie przepuścił po ostatniej próbie, dostawał `"error": null`
 * w `run.json`. Jedynym śladem było `summary` ucięte do 240 bajtów — a że sędziowie piszą
 * prozą, potrafiło zaczynać się słowem „PASS". Właściciel dostawał więc czerwony krok, którego
 * podsumowanie mówi, że przeszedł, i wiersz „Done" pod spodem.
 *
 * DWA STANY, DWA ZDANIA, i to jest cała treść tego kryterium. Dla biegu „nie przepuścił"
 * i „nic nie powiedział" są tym samym — `Verdict::default()` jest `Fail` i tak zostaje. Dla
 * człowieka to robota do poprawki kontra zepsuty kontrakt: pierwsza to popraw prompt kroku,
 * druga to popraw sędziego. Jedno zdanie na oba stany kazałoby zgadywać, którą czynność wykonać.
 *
 * SŁABĄ WERSJĄ jest `assert!(error.is_some())`. Przechodzi ją implementacja z jednym zdaniem na
 * oba stany — czyli dokładnie ta, której to kryterium ma zabronić. Dlatego niżej stoi
 * porównanie DWÓCH biegów i asercja, że ich zdania są RÓŻNE.
 */

/// Powody wpisane do `run.json` przy krokach, które padły.
fn why_the_steps_failed(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let text = std::fs::read_to_string(dir.join("run.json"))?;
    let described: serde_json::Value = serde_json::from_str(&text)?;
    let steps = described["steps"]
        .as_array()
        .ok_or("run.json has no steps array")?;
    Ok(steps
        .iter()
        .filter(|one| one["status"] == "failed")
        .filter_map(|one| one["error"].as_str().map(str::to_owned))
        .collect())
}

/// Puszcza pętlę, której sędzia nigdy nie przepuszcza, i oddaje powody z `run.json`.
async fn why_it_ended(watch: Watch) -> Result<(Vec<String>, PathBuf), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::new(watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let report = one_run(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await??;
    let why = why_the_steps_failed(&report.dir)?;
    Ok((why, report.dir))
}

#[tokio::test]
async fn a_step_the_tester_stopped_says_which_way_it_stopped() -> Result<(), Box<dyn Error>> {
    // Sędzia MÓWI, że nie przeszło — i mówi to tak, jak umiemy przeczytać.
    let (spoke, _kept) = why_it_ended(Watch::passing_on_turn(usize::MAX)).await?;
    // Sędzia pracuje i nie zostawia wiersza wyniku — to robili wszyscy prawdziwi sędziowie.
    let (silent, _also) = why_it_ended(Watch::never_saying_how_it_went()).await?;

    let said = spoke.first().ok_or(
        "a run whose tester never passed the work left no reason at all in run.json. That is          the defect: a red step with nothing to read, and a summary that can begin with the          word PASS because the tester wrote prose",
    )?;
    let quiet = silent
        .first()
        .ok_or("and the same for a tester that never said how it went")?;

    assert_ne!(
        said, quiet,
        "both runs got the same sentence, so the human cannot tell work that was turned down          from a tester that never answered. Those are two different things to go and fix: one          is the step's prompt, the other is the tester itself. The sentence was {said:?}"
    );
    assert!(
        quiet.contains("never said"),
        "a tester that answered nothing has to be named as answering nothing. Anything else          sends the human to reread work that was never judged. It said {quiet:?}"
    );
    assert!(
        quiet.contains("outcome: pass") || quiet.contains("outcome: fail"),
        "and it has to name the line we actually read, because that is the one thing the human          can change. It said {quiet:?}"
    );
    Ok(())
}

/* 2026-08-23 — DZIEWIĘTNAŚCIE PRZEKAZAŃ, DZIEWIĘTNAŚCIE IDENTYCZNYCH TYTUŁÓW.
 *
 * `title_of` czytało `AgentJob::prompt`, którego komentarz twierdził „instrukcje kroku,
 * dosłownie z pliku workflow". Przestało to być prawdą, odkąd `plan_step` składa w tym polu blok
 * „co wiadomo" i nagłówek zadania biegu — więc od chwili, w której bieg zaczął nosić zadanie,
 * KAŻDY tytuł zaczynał się tym samym zdaniem.
 *
 * Zmierzone na biegu właściciela `20260823-011240`: lista „co kroki sobie przekazały" to
 * dwadzieścia wierszy, z których każdy czyta się „What the person asked for, for this whole run:
 * zrób analizę i reaserch…". Nie da się z niej wybrać niczego.
 *
 * SŁABĄ WERSJĄ jest „tytuł jest niepusty". Przechodzi ją dokładnie ten defekt. Rozróżnia je
 * pytanie o DWA różne kafelki naraz plus asercja, że w tytule nie ma zadania biegu — bo to
 * zadanie było jedyną treścią wszystkich dziewiętnastu.
 */

/// Zadanie biegu — brzmi inaczej niż każda instrukcja w pliku, i o to chodzi.
const WHOLE_RUN_ASKED: &str = "compare the districts and pick one to live in";

/// Pary `from` → `title` z front-matterów przekazań tego biegu.
fn titles_of(dir: &Path) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir.join("handoffs")) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let text = std::fs::read_to_string(entry.path())?;
        let field = |key: &str| -> String {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        out.push((field("from:"), field("title:")));
    }
    Ok(out)
}

#[tokio::test]
async fn a_handoff_is_titled_by_what_its_own_step_was_asked() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::new(Watch::passing_on_turn(1))),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let report = one_run(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 2,
            // Z ZADANIEM, bo bez niego ten defekt nie istnieje: nagłówek doklejał się do promptu
            // wyłącznie wtedy, gdy bieg miał o co poproszony.
            task: Some(WHOLE_RUN_ASKED.to_owned()),
            part: None,
            handoffs_from: None,
        },
    )
    .await??;

    let titled = titles_of(&report.dir)?;
    assert!(
        titled.len() >= 2,
        "the run left fewer than two handoffs, so asking whether their titles differ would be a \
         question about one thing. It left: {titled:?}"
    );
    for (from, title) in &titled {
        assert!(
            !title.contains(WHOLE_RUN_ASKED),
            "the handoff from {from:?} is titled with what the WHOLE RUN was asked for. Every \
             step of a run shares that sentence, so every title comes out the same and the list \
             of what was passed along cannot be read at all. It said: {title:?}"
        );
    }
    let distinct: std::collections::BTreeSet<&str> =
        titled.iter().map(|(_, title)| title.as_str()).collect();
    assert!(
        distinct.len() >= 2,
        "every handoff in this run carries the same title, even though its steps were asked for \
         different things. Rounds of one loop SHOULD share a title - they share an instruction - \
         but two different tiles must not. They said: {distinct:?}"
    );
    /* I POZYTYWNIE: tytuł ma nieść zdanie SWOJEGO kafelka, nie cudze i nie nasze. */
    let tester = titled
        .iter()
        .find(|(from, _)| from == "Tester")
        .ok_or("the tester left no handoff at all")?;
    assert!(
        tester.1.contains("Run the suite"),
        "the tester's handoff is titled with something other than what the tester was asked to \
         do. That sentence is the only one about this step a person actually wrote. It said: {:?}",
        tester.1
    );
    Ok(())
}

/* 2026-08-23 — ZAMOWIENIE WLASCICIELA: „workflows zawsze ma miec opcje kontynuacji a nie slepe
 * punkty".
 *
 * Do dzis kazda porazka konczyla sie identycznie: `StepReport::Failed`, po ktorym planista
 * malowal caly stozek potomkow na `skipped` — bez zdania i bez wyboru. Bieg wlasciciela
 * `20260823-092142` stracil przez to `Synteze`, `Design` i `Implementation`, mimo ze dwie
 * z trzech weryfikacji przeszly.
 *
 * SLABA WERSJA TEGO KRYTERIUM to „krok za petla pobiegl". Przechodzi ja implementacja, ktora
 * przy `carry-on` melduje krok jako UDANY — a wtedy pasek pokazuje wypelniony blok nad robota,
 * ktorej tester nie przepuscil, czyli klamie o dokladnie tej jednej rzeczy, dla ktorej ten
 * produkt powstal. Dlatego drugi punkt pyta o STAN sedziego i wymaga czerwieni.
 */

/// Ten sam plik co `LOOP_FILE`, ale tester ma jechac dalej mimo nieprzepuszczenia.
fn loop_that_carries_on() -> String {
    let marked = LOOP_FILE.replace(
        r#""instructions": "Run the suite and say whether it passed.","#,
        "\"instructions\": \"Run the suite and say whether it passed.\",\n      \
         \"whenItFails\": \"carry-on\",",
    );
    assert_ne!(
        marked, LOOP_FILE,
        "the fixture did not actually mark the tester"
    );
    marked
}

/// Stany krokow z `run.json`, po nazwie kafelka.
fn states_of(dir: &Path) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let text = std::fs::read_to_string(dir.join("run.json"))?;
    let described: serde_json::Value = serde_json::from_str(&text)?;
    Ok(described["steps"]
        .as_array()
        .ok_or("run.json has no steps")?
        .iter()
        .map(|one| {
            (
                one["name"].as_str().unwrap_or_default().to_owned(),
                one["status"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect())
}

/// Powody zapisane przy krokach, po nazwie kafelka.
fn reasons_of(dir: &Path) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let text = std::fs::read_to_string(dir.join("run.json"))?;
    let described: serde_json::Value = serde_json::from_str(&text)?;
    Ok(described["steps"]
        .as_array()
        .ok_or("run.json has no steps")?
        .iter()
        .filter_map(|one| {
            Some((
                one["name"].as_str()?.to_owned(),
                one["error"].as_str()?.to_owned(),
            ))
        })
        .collect())
}

/// Puszcza plik, ktorego tester NIGDY nie przepuszcza, i oddaje to, co zostalo w `run.json`.
///
/// Czyta plik W SRODKU, a nie oddaje sciezki: `Bench` trzyma katalogi tymczasowe i kasuje je,
/// wychodzac z tej funkcji — sciezka oddana na zewnatrz wskazuje wtedy na nic.
async fn what_a_never_passing_run_left(
    file: &str,
) -> Result<(Vec<(String, String)>, Vec<(String, String)>), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", file)?;
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::new(Watch::passing_on_turn(usize::MAX))),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let report = one_run(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await??;
    Ok((states_of(&report.dir)?, reasons_of(&report.dir)?))
}

#[tokio::test]
async fn a_step_set_to_carry_on_lets_the_work_through_and_still_reads_red()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", &loop_that_carries_on())?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::passing_on_turn(usize::MAX));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let report = one_run(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await??;

    assert_eq!(
        watch.times(AFTER_PROMPT),
        1,
        "the tester never passed and the step after the loop was set to carry on, so it has to \
         run exactly once. Zero means the dead end is still there; more than once means the loop \
         let it through on every round. The driver saw: {:?}",
        watch.seen()
    );

    let states = states_of(&report.dir)?;
    let tester: Vec<&str> = states
        .iter()
        .filter(|(name, _)| name == "Tester")
        .map(|(_, state)| state.as_str())
        .collect();
    assert!(
        tester.contains(&"failed"),
        "the tester never passed the work, so it has to read as failed even though the run \
         carried on. A filled block promises the step worked - and that promise over work a \
         tester turned down is the one lie this whole product exists to prevent. It read: \
         {tester:?}"
    );
    assert!(
        !states.iter().any(|(_, state)| state == "skipped"),
        "something was still skipped even though the work was set to carry on. It read: {states:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_skipped_step_says_which_step_stopped_it() -> Result<(), Box<dyn Error>> {
    // Plik BEZ ustawienia, czyli dokladnie tak, jak wygladaja wszystkie istniejace workflow.
    let (states, reasons) = what_a_never_passing_run_left(LOOP_FILE).await?;
    assert!(
        states.iter().any(|(_, state)| state == "skipped"),
        "the fixture is wrong if nothing was skipped; the point below would be about an empty \
         set. It read: {states:?}"
    );

    let after = reasons
        .iter()
        .find(|(name, _)| name == "Ship")
        .ok_or_else(|| {
            format!(
                "the step after the loop was skipped and left no reason at all in run.json. That \
                 is the dead end the owner asked about: three empty rows and not one sentence \
                 about what killed them. The file said: {reasons:?}"
            )
        })?;
    assert!(
        after.1.contains("Tester"),
        "the skipped step has a reason, but it does not name the step that stopped it - so the \
         person still has to walk the graph themselves to find out. It said: {:?}",
        after.1
    );
    Ok(())
}
