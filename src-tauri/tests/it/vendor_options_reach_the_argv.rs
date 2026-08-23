//! AC-2 dla T-90: przelotka vendora dojeżdża do argv obu aplikacji i nie podnosi diala.
//!
//! # Po co to istnieje
//!
//! Przelotka `vendorOptions` jest decyzją D6, jest w formacie agenta, jest w formacie kroku, ma
//! własny filtr polityki (`library::agents::vendor_args_filtered`) i własną regułę zapisu
//! (`workflow::check::the_passthrough`) — a do procesu nie dociera **ani jedna flaga**. Filtr
//! nie ma w drzewie żadnego wołającego w ścieżce biegu. Człowiek dopisuje flagę, plik ją
//! zapisuje, walidator ją przepuszcza, a proces jej nie widzi; „vendor zignorował flagę" jest
//! z zewnątrz nieodróżnialne od „Loadout jej nie wysłał", więc nikt się o tym nie dowiaduje.
//!
//! # Wyrocznią jest GOTOWY FRAGMENT ARGV, nie stała w kodzie (niezmiennik 20)
//!
//! Dwa pomiary, jeden za drugim i oba konieczne:
//!
//! 1. **co niesie sterownik, który naprawdę poszedł do `start`** — dubler zapamiętuje fragment,
//!    który dostał przez `AgentDriver::configured`, i robi to na tej instancji, którą bieg
//!    faktycznie uruchomił. Trzy opakowania sterownika (Connections → dziedziczenie → dowody)
//!    oddają KLONY, więc fragment założony i zgubiony po drodze wygląda dokładnie tak samo jak
//!    fragment, którego nigdy nie było;
//! 2. **czy ten fragment naprawdę staje się argumentami** — ten sam fragment jedzie do
//!    prawdziwych budowniczych komend obu vendorów i pytamy o gotowe argv. Asercja na samej
//!    wartości zwróconej przez filtr przechodziłaby dla funkcji, której nikt nie woła — to jest
//!    dokładnie ten czwarty rodzaj dowodu, którego zabrania niezmiennik 29.
//!
//! # Dwa vendory, bo jeden nie rozstrzyga
//!
//! Claude bierze `--flaga wartość`, Codex `-c klucz=wartość` **jako opcję globalną**, czyli
//! przed podkomendą. Implementacja, która skleja jeden kształt dla obu, przechodzi każde
//! sprawdzenie zadane jednemu vendorowi i wywala drugiego przy pierwszym prawdziwym biegu.
//!
//! # I dial zostaje jedyną drogą do uprawnień
//!
//! Flaga eskalująca albo zarezerwowana jest **odmową startu ze zdaniem nazywającym flagę**,
//! nigdy cichym pominięciem. Ciche pominięcie uczy człowieka, że przelotka nie działa — więc
//! wpisuje to samo jeszcze raz, innym zapisem — zamiast tego, że została zablokowana.
//!
//! `--effort` i `model_reasoning_effort` dochodzą do list zarezerwowanych razem z tym zadaniem:
//! od T-91 ustawia je sam Loadout z pola „ile myśleć", a dwie strony ustawiające jedną rzecz to
//! cicha wygrana jednej z nich — dokładnie to, czego zakazuje D6.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunError, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::exec_argv;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Vendor, read_agent_file};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, RESERVED_CLAUDE, RESERVED_CODEX, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Wpis przelotki Claude'a. Flaga spoza obu list, żeby kryterium mierzyło przelotkę, a nie
/// przypadkiem regułę odmowy.
const CLAUDE_FLAG: &str = "--fallback-model";
const CLAUDE_VALUE: &str = "sonnet";

/// Wpis przelotki Codeksa. Klucz konfiguracji, bo tym kształtem mówi ten vendor.
const CODEX_KEY: &str = "model_verbosity";
const CODEX_VALUE: &str = "high";

/// Flaga, która JEST podniesieniem w samej nazwie i stoi z pustą wartością.
const RAISES_THE_DIAL: &str = "--dangerously-skip-permissions";

/// Nazwy, które od T-91 ustawia sam Loadout z pola „ile myśleć".
const CLAUDE_SETS_ITSELF: &str = "--effort";
const CODEX_SETS_ITSELF: &str = "model_reasoning_effort";

fn agent_file(id: &str, name: &str, vendor: &str, options: &str) -> String {
    format!(
        "---
schema: 1
id: {id}
name: {name}
summary: Does the work
color: moss
runsWith: {vendor}
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
vendorOptions: {options}
---
Do the work.
"
    )
}

/// Trzy kroki w łańcuchu: jeden bez przelotki, jeden z przelotką Claude'a, jeden z Codeksa.
///
/// Łańcuch, a nie trzy luźne kafelki: kroki, które mogą biec równocześnie i celują w te same
/// pliki, są odmową przed pierwszym procesem (niezmiennik 12). `fresh-copy` załatwia to samo
/// i przy okazji daje każdemu krokowi własne drzewo.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_vendor_options_reach_the_argv",
  "name": "One plain step and two with a passthrough",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plain",
      "name": "Plain",
      "agent": "01990000-0000-7000-8000-00000000091a",
      "overrides": {},
      "instructions": "plain: do the work with nothing extra.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_claude",
      "name": "Claudine",
      "agent": "01990000-0000-7000-8000-00000000091b",
      "overrides": {},
      "instructions": "claudine: do the work.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    },
    {
      "kind": "agent",
      "id": "s_codex",
      "name": "Codie",
      "agent": "01990000-0000-7000-8000-00000000091c",
      "overrides": {},
      "instructions": "codie: do the work.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 480 }
    }
  ],
  "links": [
    { "from": "s_plain", "to": "s_claude" },
    { "from": "s_claude", "to": "s_codex" }
  ]
}
"#;

/// Jeden krok, którego agent próbuje przemycić coś przez przelotkę.
const ONE_STEP: &str = r#"{
  "format": 1,
  "id": "wf_one_step_with_a_passthrough",
  "name": "One step with a passthrough",
  "steps": [
    {
      "kind": "agent",
      "id": "s_only",
      "name": "Only",
      "agent": "01990000-0000-7000-8000-00000000091d",
      "overrides": {},
      "instructions": "only: do the work.",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_passthrough_of_both_vendors_reaches_the_argv() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let plain = bench.agent(
        "plain",
        &agent_file(
            "01990000-0000-7000-8000-00000000091a",
            "Plain",
            "claude-code",
            "{}",
        ),
    )?;
    let claudine = bench.agent(
        "claudine",
        &agent_file(
            "01990000-0000-7000-8000-00000000091b",
            "Claudine",
            "claude-code",
            &format!(r#"{{"claude": {{"{CLAUDE_FLAG}": "{CLAUDE_VALUE}"}}}}"#),
        ),
    )?;
    let codie = bench.agent(
        "codie",
        &agent_file(
            "01990000-0000-7000-8000-00000000091c",
            "Codie",
            "codex",
            &format!(r#"{{"codex": {{"{CODEX_KEY}": "{CODEX_VALUE}"}}}}"#),
        ),
    )?;
    let workflow = bench.workflow("vendor-options", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&plain, &claudine, &codie])?;

    let seen = Arc::new(Seen::default());
    let outcome = run_it(&bench, workflow, Arc::clone(&seen)).await?;
    outcome.map_err(|error| format!("this fixture raises nothing, so it has to run: {error}"))?;

    let steps = seen.snapshot();
    let labels: Vec<&str> = steps.iter().map(|one| one.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["plain", "claudine", "codie"],
        "all three steps have to reach their agent app for the fragments below to mean anything; \
         the run entered {labels:?}"
    );

    // ── (a) CLAUDE DOSTAJE PARĘ „FLAGA, WARTOŚĆ" ─────────────────────────────────────────────
    let claude_fragment = fragment_of(&steps, "claudine");
    assert!(
        pair_stands_in(&claude_fragment, CLAUDE_FLAG, CLAUDE_VALUE),
        "the agent's passthrough never reached the agent app that ran it: the fragment it \
         carried was {claude_fragment:?}. A setting a person writes, a file keeps and a checker \
         approves, which the run then does not send, is a control with nothing behind it \
         (invariant 16) — and \"the vendor ignored my flag\" looks exactly the same from outside"
    );

    // ── (b) CODEX DOSTAJE `-c klucz=wartość` ────────────────────────────────────────────────
    let codex_fragment = fragment_of(&steps, "codie");
    assert!(
        pair_stands_in(&codex_fragment, "-c", &format!("{CODEX_KEY}={CODEX_VALUE}")),
        "the other agent app's passthrough never reached it either, or reached it in the shape \
         of the first one: the fragment was {codex_fragment:?}. These two apps take extra \
         settings differently, so one shape for both passes every question asked about one of \
         them and breaks the other on its first real run"
    );

    // ── (c) I FRAGMENT NAPRAWDĘ STAJE SIĘ ARGUMENTAMI ───────────────────────────────────────
    // Gotowa komenda, nie napis w pliku: selftest w repo źródłowym asertował obecność flagi
    // w skrypcie, przechodził NA KOMENTARZU, a żywa flaga brzmiała inaczej [raport 06 §2].
    let claude_command = claude_argv(&claude_fragment);
    assert!(
        pair_stands_in(&claude_command, CLAUDE_FLAG, CLAUDE_VALUE),
        "the fragment reaches the driver and does not become arguments: the command came out as \
         {claude_command:?}. A key without its value beside it is worse than nothing — the next \
         argument is swallowed as its value"
    );
    let codex_command = codex_argv(&codex_fragment);
    let exec_at = codex_command.iter().position(|one| one == "exec");
    let key_at = codex_command
        .iter()
        .position(|one| one == &format!("{CODEX_KEY}={CODEX_VALUE}"));
    assert!(
        pair_stands_in(&codex_command, "-c", &format!("{CODEX_KEY}={CODEX_VALUE}")),
        "the other app's fragment does not become arguments either: the command came out as \
         {codex_command:?}"
    );
    assert!(
        matches!((key_at, exec_at), (Some(key), Some(exec)) if key < exec),
        "the extra setting stands after the subcommand in {codex_command:?}. This app takes it as \
         a global option only: given later it is refused outright, and the whole turn ends before \
         the work reaches it — measured the same way for the working directory on 0.148.0"
    );

    // ── (d) A KROK BEZ PRZELOTKI MA ARGV CO DO BAJTU TAKIE JAK DZIŚ ─────────────────────────
    let plain_fragment = fragment_of(&steps, "plain");
    assert!(
        plain_fragment.is_empty(),
        "the step whose agent asks for nothing extra was handed {plain_fragment:?}. Nothing is \
         the only honest answer here: a run that adds arguments for everybody changes the \
         command line of every step that never asked, and three landed guards pin those exact \
         strings"
    );
    assert_eq!(
        claude_argv(&plain_fragment),
        claude_argv(&[]),
        "the command built for a step with no extra settings differs from the command built with \
         no fragment at all, so this task changed the command line of every step that asked for \
         nothing"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_flag_that_raises_the_dial_stops_the_run_and_names_it() -> Result<(), Box<dyn Error>> {
    let refusal = one_step_with(&format!(r#"{{"claude": {{"{RAISES_THE_DIAL}": ""}}}}"#)).await?;

    assert!(
        refusal.said.contains(RAISES_THE_DIAL),
        "an agent whose passthrough carries {RAISES_THE_DIAL} started anyway, or was stopped \
         without being told what stopped it. What an agent may do with your files is set on one \
         dial and nowhere else (D6); a silent drop teaches the person the passthrough does not \
         work, so they write the same thing again in another spelling. Loadout said: {:?}",
        refusal.said
    );
    assert!(
        refusal.entered.is_empty(),
        "the refusal came after {} agent(s) had already been started. A refusal is due at the \
         Start at the latest, never mid-run (invariant 12) — the whole point is that nothing \
         with those permissions ever exists",
        refusal.entered.len()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_flag_loadout_sets_itself_stops_the_run_and_names_it() -> Result<(), Box<dyn Error>> {
    for (options, named) in [
        (
            format!(r#"{{"claude": {{"{CLAUDE_SETS_ITSELF}": "max"}}}}"#),
            CLAUDE_SETS_ITSELF,
        ),
        (
            format!(r#"{{"codex": {{"{CODEX_SETS_ITSELF}": "high"}}}}"#),
            CODEX_SETS_ITSELF,
        ),
    ] {
        let refusal = one_step_with(&options).await?;
        assert!(
            refusal.said.contains(named),
            "an agent set {named} through its passthrough and the run went ahead, or stopped \
             without naming it. Loadout sets that itself from the \"how much thinking\" setting \
             since T-91, so two sides now write one thing — and whichever loses, loses quietly, \
             which is the one outcome D6 forbids. Loadout said: {:?}",
            refusal.said
        );
        assert!(
            refusal.entered.is_empty(),
            "the refusal for {named} came after {} agent(s) had already been started",
            refusal.entered.len()
        );
    }
    Ok(())
}

#[test]
fn the_two_names_loadout_now_sets_itself_are_refused_when_a_step_writes_them()
-> Result<(), Box<dyn Error>> {
    // Obie połowy jednej reguły. Sama lista jest faktem o kodzie; zdanie z walidatora jest tym,
    // co człowiek naprawdę czyta, kiedy zapisuje kafelek (niezmiennik 29).
    assert!(
        RESERVED_CLAUDE.contains(&CLAUDE_SETS_ITSELF),
        "the list of names Loadout sets for this app does not carry {CLAUDE_SETS_ITSELF}: \
         {RESERVED_CLAUDE:?}"
    );
    assert!(
        RESERVED_CODEX.contains(&CODEX_SETS_ITSELF),
        "and the other app's list does not carry {CODEX_SETS_ITSELF}: {RESERVED_CODEX:?}"
    );

    for (vendor, named) in [("claude", CLAUDE_SETS_ITSELF), ("codex", CODEX_SETS_ITSELF)] {
        let text = step_with_options(vendor, named);
        let file = serde_json::from_str(&text)?;
        let said: Vec<String> = check(&file)
            .into_iter()
            .filter(|note| note.level == Level::Problem)
            .map(|note| note.message)
            .collect();
        assert!(
            said.iter().any(|one| one.contains(named)),
            "a step that writes {named} into its own extra settings saves without a word: \
             {said:?}. Until this task the clash had no effect, because nothing reached the \
             command line; the moment it does, the person has to be told which line to delete"
        );
    }
    Ok(())
}

/// Plik workflow z jednym krokiem, którego przelotka podaje `named`.
fn step_with_options(vendor: &str, named: &str) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_a_step_that_sets_it_too",
  "name": "A step that sets it too",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_only",
      "name": "Only",
      "agent": "01990000-0000-7000-8000-00000000091d",
      "overrides": {{}},
      "vendorOptions": {{ "{vendor}": {{ "{named}": "high" }} }},
      "instructions": "only: do the work.",
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

/// Co bieg powiedział człowiekowi i czy zdążył cokolwiek uruchomić.
struct Refused {
    said: String,
    entered: Vec<Step>,
}

/// Jeden bieg jednego kroku, którego agent niesie tę przelotkę.
async fn one_step_with(options: &str) -> Result<Refused, Box<dyn Error>> {
    let bench = Bench::new()?;
    let only = bench.agent(
        "only",
        &agent_file(
            "01990000-0000-7000-8000-00000000091d",
            "Only",
            "claude-code",
            options,
        ),
    )?;
    let workflow = bench.workflow("one-step", ONE_STEP)?;
    the_fixture_can_run(&workflow, &[&only])?;

    let seen = Arc::new(Seen::default());
    let outcome = run_it(&bench, workflow, Arc::clone(&seen)).await?;
    let said = match outcome {
        Ok(report) => format!(
            "nothing — the run went ahead and ended as {:?}",
            report.steps
        ),
        Err(error) => error.to_string(),
    };
    Ok(Refused {
        said,
        entered: seen.snapshot(),
    })
}

/// Fragment argv, który niósł sterownik uruchomiony dla tego kroku.
fn fragment_of(steps: &[Step], label: &str) -> Vec<String> {
    steps
        .iter()
        .find(|one| one.label == label)
        .map(|one| one.arguments.clone())
        .unwrap_or_default()
}

/// Czy `key` i `value` stoją w tej liście **obok siebie i w tej kolejności**.
///
/// Obecność samej nazwy nie wystarcza i nigdy nie wystarczała: flaga bez wartości połyka
/// następny argument jako swój, więc „lista zawiera obie rzeczy" przechodzi dla komendy, która
/// znaczy co innego, niż wygląda.
fn pair_stands_in(arguments: &[String], key: &str, value: &str) -> bool {
    arguments
        .windows(2)
        .any(|pair| pair[0] == key && pair[1] == value)
}

/// Argumenty gotowej komendy Claude'a, zbudowane z tego fragmentu.
fn claude_argv(arguments: &[String]) -> Vec<String> {
    let configured = DriverConfiguration {
        arguments: arguments.to_vec(),
        environment: Vec::new(),
        servers: Vec::new(),
    };
    ClaudeDriver::new()
        .with_configuration(configured)
        .command(&spec())
        .as_std()
        .get_args()
        .map(|one| one.to_string_lossy().into_owned())
        .collect()
}

/// Pełne argv jednej tury Codeksa, zbudowane z tego fragmentu.
fn codex_argv(arguments: &[String]) -> Vec<String> {
    let configured = DriverConfiguration {
        arguments: arguments.to_vec(),
        environment: Vec::new(),
        servers: Vec::new(),
    };
    exec_argv(&configured, &spec())
}

fn spec() -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: None,
        system_append: None,
        policy: Policy::EditInFolder,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej pliki agentów mają dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka. Czerwień w fazie kontraktu wygląda
/// identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium nie da się spełnić nigdy",
/// a agent, którego plik się nie wczytuje, odmawia biegu w każdej implementacji.
fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran, so this criterion could never pass: \
         {problems:?}"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

/// Jeden bieg tej fikstury. Oddaje wynik biegu **nietknięty**, bo połowa kryteriów tego pliku
/// mierzy właśnie odmowę.
async fn run_it(
    bench: &Bench,
    workflow: PathBuf,
    seen: Arc<Seen>,
) -> Result<Result<RunReport, RunError>, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(seen),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };
    // Okno jest tu czarną dziurą: to kryterium sądzi argumenty, nie wiersze.
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(outcome)
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Jeden krok, który naprawdę wszedł do sterownika.
#[derive(Debug, Clone)]
struct Step {
    /// To, co stoi przed pierwszym dwukropkiem instrukcji — `RunSpec` nazwy kroku nie niesie.
    label: String,
    /// Fragment argv, który niósł sterownik uruchomiony dla tego kroku.
    arguments: Vec<String>,
}

#[derive(Debug, Default)]
struct Seen(Mutex<Vec<Step>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, step: Step) {
        self.lock().push(step);
    }

    fn snapshot(&self) -> Vec<Step> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Step>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn label_of(prompt: &str) -> String {
    prompt
        .split_once(':')
        .map_or_else(|| prompt.to_owned(), |(head, _)| head.trim().to_owned())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Fabryka oddająca dubler o etykiecie TEGO vendora.
///
/// Etykieta jest treścią, nie ozdobą: kształt fragmentu przelotki zależy od tego, KTÓRA
/// aplikacja go dostaje, więc dubler bez własnej etykiety mierzyłby jedną odpowiedź dwa razy.
fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let claude: Arc<dyn AgentDriver> = Arc::new(Fake::new("claude", Arc::clone(&seen)));
    let codex: Arc<dyn AgentDriver> = Arc::new(Fake::new("codex", seen));
    Arc::new(move |vendor| match vendor {
        Vendor::Codex => Arc::clone(&codex),
        Vendor::ClaudeCode => Arc::clone(&claude),
    })
}

/// Dubler sterownika, który **zapamiętuje fragment argv, jaki naprawdę niósł przy starcie**.
#[derive(Debug, Clone)]
struct Fake {
    vendor: &'static str,
    seen: Arc<Seen>,
    arguments: Vec<String>,
}

impl Fake {
    fn new(vendor: &'static str, seen: Arc<Seen>) -> Self {
        Self {
            vendor,
            seen,
            arguments: Vec::new(),
        }
    }
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        self.vendor
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(self.vendor.to_owned()),
        })
    }

    /// Klon niosący fragment — dokładnie tak, jak robią to obaj prawdziwi sterownicy.
    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            arguments: configuration.arguments.clone(),
            ..self.clone()
        }))
    }

    /// Ten dubler nosi etykietę prawdziwego vendora, a bieg odmawia startu takiemu sterownikowi
    /// bez szwu na prywatne dowody. Zapisu nie udajemy — robi go silnik.
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen.record(Step {
            label: label_of(&spec.prompt),
            arguments: self.arguments.clone(),
        });
        let session = SessionRef {
            vendor: self.vendor,
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
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
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
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "## Answer\nDone.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n".to_owned(),
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
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
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
