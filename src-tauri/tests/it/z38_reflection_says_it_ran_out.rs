//! Z-38: refleksja, która zeszła na SWOIM suficie, mówi to zdaniem — na ekranie i w historii.
//!
//! # Wada, którą to sądzi (zmierzona na biegu meetnotes z 2026-09-04)
//!
//! Dwie godziny, 57,52 USD, dziewięć przekazań, „Learn from this run" włączone.
//! `logs/reflection.input.json` miał `context: []`, transkrypt tury kończył się
//! `Reached maximum budget ($0.08)` po sześciu turach i 22 sekundach, a `run.json` mówił
//! `reflection.why = "nothing-came-back"`. Na ekranie nie stało ani jedno zdanie. Trzy rzeczy
//! naraz: tura dostała osiem centów na przeczytanie dziewięciu przekazań, kazano jej ich szukać
//! po katalogu, a jej porażka przedstawiła się jako cisza modelu.
//!
//! # Trzy słabe wersje tego kryterium
//!
//! **Pierwsza: zawołać `what_this_run_taught_us` wprost i przeczytać zwróconą strukturę.**
//! Wartość zwrócona przez funkcję dowodzi, że mechanizm istnieje; zdanie na ekranie dowodzi, że
//! produkt działa (niezmiennik 29). Dlatego wszystko niżej jedzie przez `run_workflow_with_budget`,
//! a pytane są dwie powierzchnie, na które patrzy człowiek: wiersze, które NAPRAWDĘ wyszły
//! kanałem do okna, i bieg otwarty tą samą komendą, którą otwiera go panel historii.
//!
//! **Druga: sprawdzić sam kod `why`.** Kod jest wartością z drutu i na ekran nie trafia
//! (niezmiennik 14) — sądzony jest więc razem ze zdaniem, które z niego powstaje, i z kwotą,
//! której bez `budget_usd` nie da się w nim postawić.
//!
//! **Trzecia: uwierzyć, że sufit skaluje się z biegiem, patrząc na jedną liczbę.** Podłoga
//! (`REFLECTION_BUDGET_USD`) jest tą samą liczbą, którą oddaje implementacja niezmieniona —
//! dlatego krok tej ławki kosztuje 24 USD, czyli tyle, żeby 1 % z niego był trzy razy wyższy od
//! podłogi i niższy od sufitu. Zieleń przy 0,08 USD znaczy, że nic się nie stało.
//!
//! # Ten moduł KOMPILUJE SIĘ NA DRZEWIE SPRZED TEGO ZADANIA i to jest jego warunek
//!
//! AGENTS.md §2a pkt 4: test, który się nie skompilował, niczego nie uruchomił i niczego nie
//! dowiódł. Dlatego nie ma tu ani jednej nazwy, która powstała razem z poprawką — żadnego
//! `NotAsked::RanOutOfBudget`, `ReflectionWire::budget_usd` ani `REFLECTION_BUDGET_CAP_USD`.
//! Wszystko, o co pyta, jest sprawdzane przez API, które stało tu wcześniej: wiersze wyjęte
//! z kanału do okna, **surowy** `run.json` i prompt zapamiętany przez dubel. Na starym drzewie
//! moduł się kompiluje i pada na asercji — na kanale nie ma ani jednej linii, `reflection.why`
//! mówi `nothing-came-back`, a prompt nie zawiera ani nazwy pliku, ani tego, co krok powiedział.
//!
//! Nowe pola typowane — czyli że okno naprawdę czyta ten zapis, a nie tylko my w teście —
//! sądzi osobny moduł `z38_the_window_reads_the_ceiling`, który powstał już PO nośnikach.

// `too_many_lines` — jeden bieg odpowiada na cztery pytania tego kryterium (prompt, sufit,
// strumień, historia), a cięcie po granicy funkcji znaczyłoby albo cztery biegi po trzydzieści
// sekund, albo stan dzielony między testami, które cargo uruchamia równolegle. Ani jednego
// `unwrap()` tu nie ma: każda droga niepowodzenia fikstury wraca `?` z własnym zdaniem.
#![allow(clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::{REFLECTION_BUDGET_USD, run_workflow_with_budget};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::budget_argv;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::evidence::{ContextKind, EvidenceIdentity, EvidenceTarget, SafeInputManifest};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value as Json;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera — ten sam wybór, co w `a_run_leaves_suggestions.rs`.
const VENDOR: &str = "fake";

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Ile kosztował ten jeden krok.
///
/// Z POMIARU KRYTERIUM, nie z wygody: 1 % z 24 USD to 0,24 USD, czyli trzykrotność podłogi
/// [`REFLECTION_BUDGET_USD`] i mniej niż sufit. Bieg tańszy niż osiem dolarów dostaje podłogę
/// i jest wtedy nieodróżnialny od implementacji, która żadnego skalowania nie ma.
const STEP_COST_USD: f64 = 24.0;

/// Sufit, który ma z tego wyjść — wypisany słowo w słowo, nie policzony tą samą formułą,
/// którą sprawdza (niezmiennik 20).
const CEILING_USD: f64 = 0.24;

/// Twardy sufit jednej tury, wypisany słowo w słowo, nie wzięty ze stałej produkcyjnej
/// (niezmiennik 20) — i dzięki temu ten moduł nie zna ani jednej nazwy z poprawki.
const CAP_USD: f64 = 1.00;

/// Krok tak drogi, że jeden procent z niego przebija sufit tury: 1 % z 500 USD to 5 USD.
const RICH_STEP_USD: f64 = 500.0;

/// Zdanie, które ma zobaczyć człowiek. Wypisane tutaj słowo w słowo, nie sklejone z funkcji
/// produkcyjnej: kryterium czytające własną stałą kodu zawsze się z nią zgadza (niezmiennik 20).
const SENTENCE: &str = "Learn from this run didn't finish: the note-taker used its $0.24 before \
                        answering.";

/// Ile kosztowały kroki biegu z audytu 2026-09-04: dwie godziny, dziewięć przekazań.
///
/// NIERÓWNE CENTOWO PO PODZIELENIU, i to jest jedyny powód, dla którego ta ławka istnieje obok
/// tamtej: 1 % z tej kwoty to 0,5752 USD, czyli liczba, przy której „zaokrąglij do najbliższego"
/// i „zaokrąglij w dół" dają dwie RÓŻNE kwoty. Ławka z okrągłymi 24 USD nie odróżnia ich wcale.
const AUDIT_STEP_USD: f64 = 57.52;

/// Kwota, którą ma dostać proces i którą ma zobaczyć człowiek — jedna, ta sama, w centach.
const AUDIT_CEILING: &str = "0.58";

/// Nazwa flagi, przepisana z `claude --help`, nie z naszego kodu (niezmiennik 20).
const BUDGET_FLAG: &str = "--max-budget-usd";

/// I zdanie tego biegu, też słowo w słowo.
const AUDIT_SENTENCE: &str = "Learn from this run didn't finish: the note-taker used its $0.58 \
                              before answering.";

const AGENT_ID: &str = "01990000-0000-7000-8000-0000000000f3";

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f3
name: Backend Dev
summary: Works where the data is
color: slate
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

/// Jeden krok, który się udaje i coś przekazuje — czyli bieg, o który refleksja w ogóle pyta.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_z38_reflection_says_it_ran_out",
  "name": "One step that hands something on",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Backend",
      "agent": "01990000-0000-7000-8000-0000000000f3",
      "overrides": {},
      "instructions": "Look at the queue and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "whenItFails": "stop",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reflection_that_dies_on_its_ceiling_says_so_in_the_stream_and_in_the_history()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;
    // Fikstura, nie asercja kryterium: krok, którego agenta nie ma w bibliotece, jest biegiem,
    // który nigdy nie rusza — a wtedy wszystko niżej jest prawdą o biegu bez ani jednej tury.
    assert!(
        AGENT.contains(AGENT_ID) && WORKFLOW.contains(AGENT_ID),
        "the fixture names {AGENT_ID} in only one of the two files that have to agree on it"
    );

    let seen = Arc::new(Seen::default());
    let delivered = Delivered::default();
    let report = a_run_that_ran_out(&bench, &seen, &delivered).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 1],
        "the step has to finish and hand something on, or nobody asks this run what it taught us \
         and every assertion below is true of a run that never got there. It ended as {:?}",
        report.steps
    );
    let handed = one_handoff_of(&report.dir)?;
    // Bieg otwarty TĄ SAMĄ KOMENDĄ, którą otwiera go panel historii — czyta go i punkt o
    // prompcie (co ten bieg zapisał, że krok powiedział), i punkt czwarty (co zapisał
    // o prywatnej turze).
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no name to open it by")?;
    let opened = read_run_inner(bench.project.path(), folder)?;

    // ── 1. Tura dostała przekazania w prompcie, zamiast szukać ich po katalogu ─────────────
    let turns = seen.reflections();
    assert_eq!(
        turns.len(),
        1,
        "this run asked for a reflection {} time(s), and everything below is about the one turn \
         it takes",
        turns.len()
    );
    let turn = &turns[0];

    assert!(
        turn.prompt.contains(&handed),
        "the reflection was asked about this run without being told what it left behind. Its \
         prompt names none of the handed-on files, so the turn spends its first minutes listing \
         a directory to find what Loadout already knew — measured on 2026-09-04: nine files, \
         six turns, and the money gone before the question was answered. The file it was never \
         shown is {handed:?} and the prompt reads: {:?}",
        turn.prompt
    );
    let carried: Vec<&str> = turn
        .manifest
        .context
        .iter()
        .filter(|source| source.kind == ContextKind::Handoff)
        .map(|source| source.reference.as_str())
        .collect();
    assert_eq!(
        carried.len(),
        1,
        "the run's own record of what this turn was given lists {carried:?}. `context: []` next \
         to a prompt that does name the file is the record saying Loadout handed the turn \
         nothing — the exact line the 2026-09-04 audit read off `logs/reflection.input.json`"
    );
    assert_eq!(
        turn.manifest.prompt_bytes,
        turn.prompt.len(),
        "the record says the turn was given {} bytes of prompt while the prompt it really got is \
         {} bytes long. That number is the only measure of this turn's input anybody keeps, and \
         it counted the bare question for as long as the question was the whole prompt",
        turn.manifest.prompt_bytes,
        turn.prompt.len()
    );

    /* ── 1b. I to, co kroki NAPRAWDĘ powiedziały, nie tylko o co je poproszono ──────────────
     *
     * Nazwa pliku i tytuł przekazania to za mało, i to jest cała treść tego punktu: tytuł jest
     * INSTRUKCJĄ z pliku workflow (`commands::run::title_of`), czyli zdaniem, które człowiek
     * napisał PRZED biegiem. Refleksja, która dostaje same zlecenia, dalej nie wie, co z nich
     * wyszło, i dalej musi otworzyć każdy plik po kolei — czyli dokładnie to, za co bieg
     * z 2026-09-04 zapłacił całym budżetem tury.
     *
     * Zdanie bierzemy z ZAPISU BIEGU, nie z literału: to jest ta sama linia, którą człowiek
     * czyta przy kafelku, więc porównanie pyta o drogę między nią a promptem, a nie o dwa
     * napisy wpisane do testu (niezmiennik 20). */
    let said = opened
        .steps
        .first()
        .map(|step| step.summary.clone())
        .unwrap_or_default();
    assert!(
        !said.is_empty(),
        "this run recorded no summary for its only step, so the assertion below would ask \
         whether the prompt contains an empty string and would pass on nothing"
    );
    assert!(
        !WORKFLOW.contains(&said),
        "what the step said is also written in the workflow file, so finding it in the prompt \
         would prove nothing: the handoff index already carries the instruction. It said: \
         {said:?}"
    );
    assert!(
        turn.prompt.contains(&said),
        "the reflection was told which files this run left and what each step was ASKED to do, \
         and nothing about what any of them actually said. That is the run's own one-line \
         summary of the step — {said:?} — and without it the turn has to open every file to \
         learn the outcome it is being asked to draw a lesson from. The prompt reads: {:?}",
        turn.prompt
    );

    // ── 2. Sufit na miarę biegu, i to, co tura wydała, wchodzi do rachunku ─────────────────
    assert!(
        turn.ceiling.is_some_and(|had| close_to(had, CEILING_USD)),
        "the reflection after a run that cost ${STEP_COST_USD:.2} was allowed {:?}. One percent \
         of what the steps cost, never less than ${REFLECTION_BUDGET_USD:.2} — a fixed eight \
         cents is what killed the turn measured on 2026-09-04, and a run that spent two hours \
         has more than eight cents' worth of reading to do",
        turn.ceiling
    );
    let record = run_file(&report.dir)?;
    assert!(
        record.get("budget_usd").is_none(),
        "the bench put a ceiling on this run after all, so the next assertion would be about a \
         run whose price was written down for a reason that has nothing to do with this task"
    );
    let spent = record.get("spent_usd").and_then(Json::as_f64);
    assert!(
        spent.is_some_and(|all| close_to(all, STEP_COST_USD + CEILING_USD)),
        "the run's own record says it spent {spent:?}. Nobody put a ceiling on this run — which \
         is how most runs go — and until this task that alone deleted the whole key, so the turn \
         that died on its ceiling left no trace of its price anywhere on disk. A bill that \
         quietly drops it is wrong by whatever the private turn burned, and `run.json` is the \
         only file where that number exists at all (invariant 4)"
    );

    // ── 3. Zdanie tam, gdzie człowiek patrzy: w strumieniu biegu ──────────────────────────
    let said_on_screen = problems_on_screen(&delivered)?;
    assert!(
        said_on_screen.iter().any(|text| text == SENTENCE),
        "the rows this run put on screen said {said_on_screen:?}. The person left the control on \
         and paid for a turn that ran out of money before it answered; silence there is \
         indistinguishable from a turn that was never taken (invariant 29)"
    );

    /* ── 4. I ten sam fakt w zapisie, z którego panel historii składa swoje zdanie ──────────
     *
     * SUROWY `run.json`, nie typ z granicy: ten moduł ma się skompilować na drzewie sprzed
     * poprawki (nagłówek pliku), a tam pola `budget_usd` po prostu nie ma. Kody czytamy więc
     * tak, jak leżą w pliku — i to jest zarazem mocniejsze pytanie, bo `kebab-case` na drucie
     * jest częścią kontraktu z oknem (`said.ts` ma po jednym wierszu na każdy z tych kodów).
     * Że okno naprawdę je czyta, sądzi `z38_the_window_reads_the_ceiling`. */
    let did = record
        .get("reflection")
        .ok_or("the run's own record says nothing at all about the private turn")?;
    assert_eq!(
        did.get("why").and_then(Json::as_str),
        Some("ran-out-of-budget"),
        "the record says the turn ended as {:?}. `nothing-came-back` is the sentence for a model \
         that had nothing to say; this one had the answer coming and ran out of money, and the \
         two ask different things of the person reading them",
        did.get("why")
    );
    let had = did.get("budget_usd").and_then(Json::as_f64);
    assert!(
        had.is_some_and(|had| close_to(had, CEILING_USD)),
        "the record carries {had:?} as the ceiling this turn had. Without the amount the sentence \
         in the history panel has nothing to end with, and the number cannot be rebuilt from a \
         constant any more — it scales with what the run cost"
    );
    assert_eq!(
        did.get("ran").and_then(Json::as_bool),
        Some(false),
        "a turn that never answered is recorded as having run, and every sentence the screen \
         builds from here starts with that word"
    );

    Ok(())
}

/// KWOTA NA EKRANIE JEST KWOTĄ, KTÓRĄ DOSTAJE PROCES — na biegu z audytu, co do centa.
///
/// # Wada, którą to sądzi (zgłoszona 2026-09-05)
///
/// Sufit `0.5752` czytał się w trzech miejscach jako trzy różne rzeczy. Zdanie na ekranie
/// i rachunek w `run.json` składa `{:.2}`, czyli zaokrąglenie do NAJBLIŻSZEGO centa: `$0.58`.
/// Argv vendora składa [`budget_argv`], które zaokrągla w DÓŁ, bo tam kwotą jest reszta budżetu
/// człowieka: `0.57`. Ekran obiecywał więc o cent więcej, niż proces kiedykolwiek zobaczył —
/// a przy turze, która ma zejść dokładnie na tym sufcie, ten cent jest całą różnicą między
/// „skończyły się pieniądze, o których ci powiedziałem" a „skończyły się jakieś inne".
///
/// # Dlaczego to jest OSOBNA ławka, a nie asercja przy tamtej
///
/// Bo tamta kosztuje 24,00 USD, czyli 1 % z niej to równe 24 centy — kwota, przy której obie
/// reguły zaokrąglania dają to samo i wada jest niewidoczna. Rozstrzyga liczba niepodzielna
/// centowo, i jest nią ta z prawdziwego biegu: 57,52 USD.
///
/// # I dlaczego przez PRAWDZIWE `budget_argv`
///
/// Bo dubel sterownika tej funkcji nie woła — dostaje samo `f64` w [`AgentDriver::with_budget`]
/// — więc ławka oparta wyłącznie na nim jest ślepa dokładnie na tę wadę: mierzy liczbę, która
/// do procesu nigdy nie dojeżdża w tej postaci. Fragment argv składa tu ta sama funkcja, którą
/// w produkcie woła `ClaudeDriver::configured`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_amount_on_screen_is_the_amount_the_process_gets() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;
    let seen = Arc::new(Seen::default());
    let delivered = Delivered::default();
    let report = a_run_costing(&bench, &seen, &delivered, Some(AUDIT_STEP_USD)).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 1],
        "the step of the audit bench did not finish, so nothing below is about the run this \
         case is named after. It ended as {:?}",
        report.steps
    );
    let ceiling = seen
        .reflections()
        .first()
        .and_then(|turn| turn.ceiling)
        .ok_or("this run never asked for a reflection, so there is no ceiling to compare")?;

    // ── 1. Co naprawdę pojedzie do procesu ────────────────────────────────────────────────
    assert_eq!(
        budget_argv(ceiling),
        vec![BUDGET_FLAG.to_owned(), AUDIT_CEILING.to_owned()],
        "the fragment of the command line this ceiling ({ceiling}) turns into is not the amount \
         the screen promises. This is the real composer, the one `ClaudeDriver::configured` \
         calls, and it rounds DOWN — so a ceiling that is not a whole number of cents reaches \
         the process a cent smaller than the sentence a person just read"
    );

    // ── 2. Co zostało zapisane ────────────────────────────────────────────────────────────
    let had = run_file(&report.dir)?
        .get("reflection")
        .and_then(|did| did.get("budget_usd"))
        .and_then(Json::as_f64);
    assert!(
        had.is_some_and(|had| close_to(had, 0.58)),
        "the run's own record says the turn had {had:?}. It has to be the same money as the \
         command line above and as the sentence below — one ceiling, three places, no rounding \
         between them"
    );

    // ── 3. I co przeczytał człowiek ───────────────────────────────────────────────────────
    let said_on_screen = problems_on_screen(&delivered)?;
    assert!(
        said_on_screen.iter().any(|text| text == AUDIT_SENTENCE),
        "the rows this run put on screen said {said_on_screen:?}. The audit run spent \
         ${AUDIT_STEP_USD:.2}, so one percent of it is what its private turn was allowed — and \
         the person has to be told the amount that was really in force"
    );

    Ok(())
}

/// PODŁOGA I SUFIT TEJ SAMEJ FORMUŁY, czyli druga połowa kryterium o cenie.
///
/// Bez tego testu „jeden procent" jest sprawdzony w jednym punkcie — a formuła bez granic po
/// obu stronach daje pięć dolarów za jedno pytanie o drogi bieg (sufit) i zero za bieg, którego
/// kroki ceny nie podały (podłoga). To drugie jest przy tym najczęstszym biegiem w tym drzewie:
/// vendor bez ceny w wyniku nie jest wyjątkiem, tylko stanem normalnym.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ceiling_stays_between_its_floor_and_its_cap() -> Result<(), Box<dyn Error>> {
    let rich = Bench::new()?;
    rich.agent("backend", AGENT)?;
    let rich_seen = Arc::new(Seen::default());
    let report = a_run_costing(
        &rich,
        &rich_seen,
        &Delivered::default(),
        Some(RICH_STEP_USD),
    )
    .await?;
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 1],
        "the expensive step did not finish, so nothing here is about the ceiling of a run that \
         cost anything. It ended as {:?}",
        report.steps
    );
    let ceiling = rich_seen
        .reflections()
        .first()
        .and_then(|turn| turn.ceiling);
    assert!(
        ceiling.is_some_and(|had| close_to(had, CAP_USD)),
        "one question about a run that cost ${RICH_STEP_USD:.2} was allowed {ceiling:?}. A \
         percentage with no upper bound makes the cheapest turn of the run its most expensive \
         one, and \"one cheap reflection after the run\" stops being true where nobody looks — \
         on the bill"
    );

    // ── I bieg, którego kroki nie podały ceny wcale ───────────────────────────────────────
    let free = Bench::new()?;
    free.agent("backend", AGENT)?;
    let free_seen = Arc::new(Seen::default());
    let free_report = a_run_costing(&free, &free_seen, &Delivered::default(), None).await?;
    assert_eq!(free_report.steps, vec![StepState::Succeeded; 1]);
    let floor = free_seen
        .reflections()
        .first()
        .and_then(|turn| turn.ceiling);
    assert!(
        floor.is_some_and(|had| close_to(had, REFLECTION_BUDGET_USD)),
        "a run whose steps reported no price at all gave its question {floor:?}. One percent of \
         nothing is nothing, and a turn with no money is a turn that is never taken — this run \
         still handed something on and still has a question worth asking"
    );

    Ok(())
}

/// Bieg z sufitem człowieka, którego jedyny krok się udaje, a tura refleksji schodzi na cenie.
async fn a_run_that_ran_out(
    bench: &Bench,
    seen: &Arc<Seen>,
    delivered: &Delivered,
) -> Result<RunReport, Box<dyn Error>> {
    a_run_costing(bench, seen, delivered, Some(STEP_COST_USD)).await
}

/// To samo, z jawną ceną jedynego kroku. `None` znaczy „vendor ceny nie podał".
async fn a_run_costing(
    bench: &Bench,
    seen: &Arc<Seen>,
    delivered: &Delivered,
    step_cost: Option<f64>,
) -> Result<RunReport, Box<dyn Error>> {
    let workflow = bench.workflow("z38-reflection-says-it-ran-out", WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(seen), step_cost),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, delivered.channel());
    /* BEZ SUFITU BIEGU, i to jest treść, nie ustawienie ławki (2026-09, Z-38): dokładnie tak
     * biegnie workflow, przy którym człowiek niczego nie ograniczył, czyli bieg domyślny. Do
     * tego zadania `run.json` takiego biegu nie zapisywał `spent_usd` wcale — a to jest jedyne
     * miejsce, w którym cena prywatnej tury w ogóle istnieje. Ławka z sufitem ukryłaby tę wadę
     * pod liczbą, którą sama postawiła. */
    let report = tokio::time::timeout(
        PATIENCE,
        run_workflow_with_budget(&deps, &request, sink, None),
    )
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(report)
}

/// Teksty wierszy `problem`, które NAPRAWDĘ wyszły kanałem do okna — w kolejności wyjścia.
fn problems_on_screen(delivered: &Delivered) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(delivered
        .lines()?
        .iter()
        .filter(|row| row.get("kind").and_then(Json::as_str) == Some("problem"))
        .filter_map(|row| row.get("text").and_then(Json::as_str).map(str::to_owned))
        .collect())
}

/// Dwie kwoty w dolarach, porównane z tolerancją jednego setnego centa.
///
/// `f64` z dzielenia nie jest bit w bit tym samym, co literał wpisany do testu, a kryterium
/// o cenie nie jest kryterium o arytmetyce zmiennoprzecinkowej.
fn close_to(what: f64, expected: f64) -> bool {
    (what - expected).abs() < 1e-4
}

/// Nazwa jedynego pliku, który ten bieg zostawił w `handoffs/`.
fn one_handoff_of(dir: &Path) -> Result<String, Box<dyn Error>> {
    let mut names: Vec<String> = fs::read_dir(dir.join("handoffs"))
        .map_err(|error| format!("this run left no handoffs/ directory at all: {error}"))?
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    match names.len() {
        1 => Ok(names.swap_remove(0)),
        other => Err(format!(
            "the bench needs exactly one handed-on file for the prompt to name, and this run \
             left {other}: {names:?}"
        )
        .into()),
    }
}

/// `run.json` tego biegu, przeczytany jako zwykły JSON — tak, jak czyta go historia.
fn run_file(dir: &Path) -> Result<Json, Box<dyn Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dir.join("run.json"),
    )?)?)
}

// ── co wyszło kanałem do okna ──────────────────────────────────────────────────────────────

/// Wiersze, które **naprawdę wyszły kanałem** do okna, w kolejności wyjścia.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    fn channel(&self) -> Channel<Vec<loadout_lib::engine::line::Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi z konstrukcji, a nie z uwagi w komentarzu.
            if let Ok(mut seen) = seen.lock() {
                seen.push(body);
            }
            Ok(())
        })
    }

    fn lines(&self) -> Result<Vec<Json>, Box<dyn Error>> {
        let seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::new();
        for body in seen.iter().cloned() {
            out.extend(body.deserialize::<Vec<Json>>()?);
        }
        Ok(out)
    }
}

// ── co zobaczył dubler ─────────────────────────────────────────────────────────────────────

/// Tura, która nie była żadnym krokiem grafu — czyli refleksja, jeśli w ogóle padła.
#[derive(Debug, Clone)]
struct Asked {
    prompt: String,
    /// Sufit, z którym ten sterownik został złożony (`AgentDriver::with_budget`).
    ceiling: Option<f64>,
    /// Manifest, który pojechał do `logs/reflection.input.json`.
    manifest: SafeInputManifest,
}

#[derive(Debug, Default)]
struct Seen {
    turns: Mutex<Vec<Asked>>,
}

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, asked: Asked) {
        Self::lock(&self.turns).push(asked);
    }

    fn reflections(&self) -> Vec<Asked> {
        Self::lock(&self.turns).clone()
    }

    fn lock<T>(what: &Mutex<Vec<T>>) -> MutexGuard<'_, Vec<T>> {
        what.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>, step_cost: Option<f64>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        seen,
        // Sterownik z fabryki prowadzi KROKI; turę Loadouta prowadzi dopiero ten, który wyjdzie
        // z `reflecting()` — tak samo, jak w produkcji.
        is_loadouts_turn: false,
        step_cost,
        ceiling: None,
        manifest: SafeInputManifest::default(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug, Clone)]
struct Fake {
    seen: Arc<Seen>,
    /// Czy ten egzemplarz sterownika jest tym, który wyszedł z [`AgentDriver::reflecting`].
    ///
    /// TĄ DROGĄ ODRÓŻNIAMY TURĘ LOADOUTA OD KROKU GRAFU, a nie znacznikiem w instrukcji, jak
    /// robi to `a_run_leaves_suggestions.rs`. To jest pomiar, nie gust: tytuł przekazania
    /// powstaje z instrukcji kroku, a od Z-38 indeks przekazań jedzie w prompcie refleksji —
    /// więc znacznik kroku stoi w promptach OBU tur i przestaje cokolwiek rozróżniać. Ten szew
    /// nazywa tę turę tak, jak nazywa ją produkcja, i nie da się go pomylić z żadnym krokiem.
    is_loadouts_turn: bool,
    /// Ile kosztuje krok grafu. `None` znaczy „vendor ceny nie podał" — stan normalny, nie awaria.
    step_cost: Option<f64>,
    /// Kwota z ostatniego [`AgentDriver::with_budget`] — niesiona dalej razem ze sterownikiem,
    /// bo to jedyny sposób, żeby powiązać sufit z turą, która go dostała.
    ceiling: Option<f64>,
    manifest: SafeInputManifest,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Szew tury Loadouta — opt-in, dokładnie jak w produkcji: domyślne `None` na traicie
    /// znaczy, że dubel milczący o tej metodzie nie widzi tury, o którą nie prosił żaden krok.
    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            is_loadouts_turn: true,
            ..self.clone()
        }))
    }

    fn with_settings(
        &self,
        _settings: &loadout_lib::engine::drivers::StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(Ok(Arc::new(self.clone())))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        // Tylko manifest tury Loadouta; krok grafu ma własny i mierzy go inne kryterium.
        let manifest = match target.identity() {
            EvidenceIdentity::Reflection => target.input().clone(),
            _ => self.manifest.clone(),
        };
        Some(Arc::new(Self {
            manifest,
            ..self.clone()
        }))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        (dollars > 0.0).then(|| {
            Arc::new(Self {
                ceiling: Some(dollars),
                ..self.clone()
            }) as Arc<dyn AgentDriver>
        })
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
        if self.is_loadouts_turn {
            self.seen.record(Asked {
                prompt: spec.prompt.clone(),
                ceiling: self.ceiling,
                manifest: self.manifest.clone(),
            });
        }

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
            is_step: !self.is_loadouts_turn,
            // TURA LOADOUTA WYDAJE DOKŁADNIE SWÓJ SUFIT, bo tak wygląda `error_max_budget_usd`
            // u prawdziwego vendora: proces schodzi w chwili, w której rachunek dobija do kwoty
            // z flagi. Kwota z testu byłaby tu drugą odpowiedzią na to samo pytanie.
            cost_usd: if self.is_loadouts_turn {
                self.ceiling
            } else {
                self.step_cost
            },
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    is_step: bool,
    cost_usd: Option<f64>,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        // TURA LOADOUTA KOŃCZY SIĘ DOKŁADNIE TAK, JAK KOŃCZY JĄ PRAWDZIWY VENDOR: `is_error`
        // z `subtype: "error_max_budget_usd"`, czyli [`FinishReason::LimitReached`] i cena
        // równa temu, co zdążył wydać (`engine::drivers::claude`, `finish`).
        let outcome = TurnOutcome {
            ok: self.is_step,
            reason: if self.is_step {
                FinishReason::Completed
            } else {
                FinishReason::LimitReached
            },
            text: if self.is_step {
                "The queue drains in one place.".to_owned()
            } else {
                String::new()
            },
            cost_usd: self.cost_usd,
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

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

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
        fs::create_dir_all(
            loadout_lib::commands::memory::project_notes_root(project.path()).join("notes"),
        )?;
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
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
