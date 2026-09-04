//! Z-50: to, co Loadout mówi o mocach lidera, jest tym, co niesie jego argv.
//!
//! # Po co to istnieje
//!
//! Zdanie pod polem rozmowy brzmiało do 2026-09-04 „it can talk things through and prepare, but
//! only /run starts work" i było jedno dla każdego lidera. Ten sam lider z dialem „work freely"
//! jechał tymczasem z ośmioma narzędziami — `Bash`, `Edit` i `Write` włącznie — więc zdanie
//! obiecywało coś, czego program nie trzyma (niezmiennik 4). Zmierzone w rozmowie z 2026-09-01:
//! lider zrobił sobie kopię repozytorium, zmienił kod, uruchomił testy i zatrzymał dwa kroki.
//!
//! # Dlaczego porównujemy ZBIORY, a nie napisy
//!
//! Bo napis jest po angielsku i mieszka w oknie (decyzja D5), a to, co lider może, jest faktem
//! o argv. Porównanie napisów byłoby więc albo przepisaniem copy do tego pliku, albo asercją
//! o obecności stringa — czyli dokładnie tym, przed czym stoi niezmiennik 20. Zbiór mocy da się
//! odczytać z OBU stron niezależnie: z gotowej komendy vendora i z odpowiedzi, którą dostaje okno.
//!
//! # Słaba wersja tego kryterium
//!
//! Jeden lider i `assert_eq!`. Przechodzi dla odpowiedzi, która zawsze mówi „wszystko" — i dla
//! takiej, która zawsze mówi „nic", jeśli trafi się na lidera bez narzędzi. Dlatego każdy
//! przypadek niżej ma kontrolę: sprawdza, CO w tym zbiorze stoi, a przypadek z liderem
//! zawężonym żąda, żeby ten zbiór był INNY niż zbiór lidera z pełną listą.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom, co w pozostałych
// plikach tego celu.
#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{
    Lead, Terminal, Threads, WhatTheLeadCanDo, what_the_lead_can_do_inner,
};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::build_exec_argv;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSink, line_channel};
use loadout_lib::library::agents::{Agent, FileAccess, Tools, Vendor};

const LINES: usize = 64;

/// Nazwy trzech mocy. Osobne napisy, a nie trzy `assert_eq!` na polach: kiedy zbiory się
/// rozjadą, komunikat ma powiedzieć KTÓREJ zabrakło, a nie „lewy nie równa się prawemu".
const CHANGES_FILES: &str = "changes files";
const RUNS_COMMANDS: &str = "runs commands";
const HELD_TO_THE_FOLDER: &str = "held to the folder";

/// Sterownik-dubler, który zapamiętuje CAŁY `RunSpec`, jaki dostał.
///
/// Cały, a nie samą listę narzędzi: z tej jednej wartości powstaje i argv vendora, i prompt
/// systemowy, więc dopiero ona pozwala zapytać, czy oba mówią to samo.
#[derive(Debug, Clone)]
struct Watching {
    seen: Arc<Mutex<Vec<RunSpec>>>,
    vendor: &'static str,
    narrows: bool,
}

#[async_trait]
impl AgentDriver for Watching {
    fn id(&self) -> &'static str {
        self.vendor
    }

    /// Fakt o vendorze, nie o naszym guście: Codex nie ma odpowiednika `--tools`, więc jego moce
    /// wynikają wyłącznie z piaskownicy.
    fn narrows_its_tools(&self) -> bool {
        self.narrows
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(self.vendor.to_owned()),
        })
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.clone());
        let (voice, _heard) = mpsc::channel(4);
        Ok(Box::new(Quiet {
            events,
            session: SessionRef {
                vendor: self.vendor,
                id: spec.run_id.to_string(),
            },
            voice,
        }))
    }
}

#[derive(Debug)]
struct Quiet {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Quiet {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
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
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
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

/// Lider o tej definicji: prawdziwa rozmowa i prawdziwa odpowiedź komendy, z jednej biblioteki.
///
/// Obie strony pytania biorą się z tego samego zapisanego pliku i tej samej fabryki sterowników
/// — inaczej porównywalibyśmy dwa różne loadouty i zieleń nie mówiłaby nic o produkcie.
async fn both_sides(agent: &Agent) -> Result<(RunSpec, WhatTheLeadCanDo), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    save_agent_inner(home.path(), agent, None).expect("the library accepts this lead");

    let vendor = match agent.runs_with {
        Vendor::ClaudeCode => "claude",
        Vendor::Codex => "codex",
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Watching {
        seen: Arc::clone(&seen),
        vendor,
        narrows: matches!(agent.runs_with, Vendor::ClaudeCode),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

    let lead = Lead::pointed_at(home.path(), Some(&agent.id.to_string()))
        .expect("the lead this test just saved is in the library");
    let terminal = Terminal {
        id: "terminal-1".to_owned(),
        folder: project.path().to_path_buf(),
    };
    let threads = Threads::new();
    threads.library_is(home.path().to_path_buf());
    let (sink, _source): (LineSink, _) = line_channel(LINES);
    threads.terminal_lines_go_to(&terminal, sink);
    threads
        .say_in(&drivers, &lead, &terminal, "what can you do?")
        .await
        .map_err(|error| error.to_string())?;

    let spec = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .first()
        .cloned()
        .ok_or("the conversation never reached the driver, so there is no argv to read")?;
    let can = what_the_lead_can_do_inner(home.path(), &drivers, Some(&agent.id.to_string()))
        .map_err(|error| error.to_string())?;
    Ok((spec, can))
}

/// Odpowiedź komendy jako zbiór.
fn as_a_set(can: WhatTheLeadCanDo) -> BTreeSet<&'static str> {
    let mut set = BTreeSet::new();
    if can.changes_files {
        set.insert(CHANGES_FILES);
    }
    if can.runs_commands {
        set.insert(RUNS_COMMANDS);
    }
    if can.held_to_the_folder {
        set.insert(HELD_TO_THE_FOLDER);
    }
    set
}

/// Wartość stojąca **zaraz za** flagą — ta sama technika, co w `claude_argv_policy`.
fn value_after(args: &[&OsStr], flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| *arg == OsStr::new(flag))?;
    args.get(at + 1)
        .map(|value| value.to_string_lossy().into_owned())
}

/// Moce odczytane z GOTOWEJ komendy Claude'a — z `--tools` i z trybu uprawnień.
fn in_the_claude_argv(spec: &RunSpec) -> Result<BTreeSet<&'static str>, Box<dyn Error>> {
    let command = ClaudeDriver::new().command(spec);
    let args: Vec<&OsStr> = command.as_std().get_args().collect();
    let tools = value_after(&args, "--tools").ok_or("this argv carries no --tools at all")?;
    let mode =
        value_after(&args, "--permission-mode").ok_or("this argv carries no --permission-mode")?;

    let carries = |name: &str| tools.split(',').any(|one| one == name);
    let mut set = BTreeSet::new();
    if carries("Edit") || carries("Write") {
        set.insert(CHANGES_FILES);
    }
    /* GOŁE `Bash` W `--tools` JEST JEDYNĄ PRAWDZIWĄ BRAMĄ POWŁOKI — zmierzone 2026-08-30 trzema
     * sondami i zapisane w `the_dial_tells_the_truth_about_the_shell`. `Bash(git *)` stojące
     * w `--allowedTools` nie ogranicza niczego w trybach, których Loadout używa. */
    if carries("Bash") {
        set.insert(RUNS_COMMANDS);
    }
    // `bypassPermissions` jest jedynym szczeblem, który sięga poza folder pracy — to on odróżnia
    // „No limits" od „Can edit this folder" (`claude::permission_flags`).
    if mode != "bypassPermissions" {
        set.insert(HELD_TO_THE_FOLDER);
    }
    Ok(set)
}

/// Moce odczytane z argv Codeksa — z jednej flagi, bo tyle ten vendor ma.
fn in_the_codex_argv(spec: &RunSpec) -> Result<BTreeSet<&'static str>, Box<dyn Error>> {
    let argv = build_exec_argv(spec);
    // `-s`, bo tak nazywa tę flagę `codex exec`. Nazwy trybów są te same, którymi mówi App
    // Server lidera — pilnuje tego `codex_lead_uses_protocol_sandbox`.
    let at = argv
        .iter()
        .position(|arg| arg == "-s")
        .ok_or("this argv carries no sandbox flag at all")?;
    let mode = argv.get(at + 1).cloned().unwrap_or_default();

    let mut set = BTreeSet::new();
    if mode != "read-only" {
        set.insert(CHANGES_FILES);
    }
    /* KOMENDY ZAWSZE. Piaskownica Codeksa ogranicza ZAPIS, nie wykonanie: `read-only` dalej
     * uruchamia to, o co model poprosi, tylko nie pozwala zapisać wyniku. Zdanie, które by tego
     * nie powiedziało, obiecywałoby rozmowę tam, gdzie stoi powłoka. */
    set.insert(RUNS_COMMANDS);
    if mode != "danger-full-access" {
        set.insert(HELD_TO_THE_FOLDER);
    }
    Ok(set)
}

/// Lider z tą listą, tym dialem i tym programem.
fn lead_with(tools: Tools, access: FileAccess, runs_with: Vendor) -> Agent {
    let mut agent = Agent::example();
    "Scout".clone_into(&mut agent.name);
    agent.runs_with = runs_with;
    agent.file_access = access;
    agent.tools = tools;
    agent
}

/// Czy prompt systemowy tej sesji mówi o uruchamianiu komend.
///
/// Z `RunSpec`, nie z `Lead::brief()` wołanego drugi raz: interesuje nas ten prompt, który
/// NAPRAWDĘ pojechał do vendora razem z tym argv.
fn the_brief_mentions_commands(spec: &RunSpec) -> bool {
    spec.system_append
        .as_deref()
        .unwrap_or_default()
        .to_lowercase()
        .contains("run commands")
}

#[tokio::test]
async fn a_lead_with_the_full_list_is_described_by_its_own_argv() -> Result<(), Box<dyn Error>> {
    let (spec, can) = both_sides(&lead_with(
        Tools::Everything,
        FileAccess::WorkFreely,
        Vendor::ClaudeCode,
    ))
    .await?;
    let argv = in_the_claude_argv(&spec)?;

    // KONTROLA: ten lider naprawdę dostaje obie moce. Bez niej równość niżej przechodzi także
    // wtedy, gdy obie strony milczą — a milcząca zgoda dwóch pustych zbiorów jest tym samym
    // zielonym, które to zadanie zdejmuje.
    assert!(
        argv.contains(CHANGES_FILES) && argv.contains(RUNS_COMMANDS),
        "the argv of a `work freely` lead has to carry Edit/Write and Bash, or this case is not \
         about the lead the audit measured. It carried: {argv:?}"
    );
    assert_eq!(
        as_a_set(can),
        argv,
        "what Loadout tells the window this lead can do is not what its own argv carries. The \
         sentence under the field is built from this answer, so a gap here is a promise on screen \
         that the process does not keep (invariant 4)"
    );
    Ok(())
}

#[tokio::test]
async fn a_lead_narrowed_to_reading_is_described_the_same_way() -> Result<(), Box<dyn Error>> {
    let narrowed = lead_with(
        Tools::Only(vec![
            "Read".to_owned(),
            "Grep".to_owned(),
            "Glob".to_owned(),
        ]),
        FileAccess::AskFirst,
        Vendor::ClaudeCode,
    );
    let (spec, can) = both_sides(&narrowed).await?;
    let argv = in_the_claude_argv(&spec)?;

    assert!(
        !argv.contains(CHANGES_FILES) && !argv.contains(RUNS_COMMANDS),
        "a list of Read, Grep and Glob has to reach argv as itself — this case is the one that \
         proves the answer is READ and not assumed from the dial. It carried: {argv:?}"
    );
    assert_eq!(
        as_a_set(can),
        argv,
        "the same lead, narrowed in the agent form, is still described by its own argv. An answer \
         taken from the dial alone would say `work freely` powers here and be wrong twice over"
    );
    Ok(())
}

#[tokio::test]
async fn the_two_leads_are_not_given_the_same_answer() -> Result<(), Box<dyn Error>> {
    let (_, full) = both_sides(&lead_with(
        Tools::Everything,
        FileAccess::WorkFreely,
        Vendor::ClaudeCode,
    ))
    .await?;
    let (_, reading) = both_sides(&lead_with(
        Tools::Only(vec!["Read".to_owned()]),
        FileAccess::AskFirst,
        Vendor::ClaudeCode,
    ))
    .await?;

    assert_ne!(
        as_a_set(full),
        as_a_set(reading),
        "both leads got the same answer, so the two cases above are measuring one constant twice. \
         The whole point is that the sentence a person reads depends on the loadout in front of \
         them"
    );
    Ok(())
}

#[tokio::test]
async fn the_brief_talks_about_commands_exactly_when_the_argv_does() -> Result<(), Box<dyn Error>> {
    let (with_a_shell, _) = both_sides(&lead_with(
        Tools::Everything,
        FileAccess::AskFirst,
        Vendor::ClaudeCode,
    ))
    .await?;
    let (without, _) = both_sides(&lead_with(
        Tools::Only(vec!["Read".to_owned(), "Grep".to_owned()]),
        FileAccess::AskFirst,
        Vendor::ClaudeCode,
    ))
    .await?;

    assert!(
        in_the_claude_argv(&with_a_shell)?.contains(RUNS_COMMANDS),
        "the control for this case: the first lead has to reach argv with a shell"
    );
    assert!(
        the_brief_mentions_commands(&with_a_shell),
        "this lead has Bash in its argv and its system prompt never mentions running commands. A \
         model with a tool it was not told about is a model that will not use it — and the person \
         reading the screen was just told it can. It was told:\n{:?}",
        with_a_shell.system_append
    );
    assert!(
        !the_brief_mentions_commands(&without),
        "and this one has no shell at all, so promising it one leaves the model saying it will \
         run something it cannot. It was told:\n{:?}",
        without.system_append
    );
    Ok(())
}

/// Codex nie zawęża listy narzędzi ani jedną flagą, więc jego moce wynikają z piaskownicy —
/// i to jest cała treść tego przypadku: ta sama odpowiedź, inna droga do niej.
#[tokio::test]
async fn a_codex_lead_takes_its_powers_from_the_sandbox() -> Result<(), Box<dyn Error>> {
    for (access, changes) in [
        (FileAccess::LookOnly, false),
        (FileAccess::WorkFreely, true),
    ] {
        let (spec, can) = both_sides(&lead_with(Tools::Everything, access, Vendor::Codex)).await?;
        let argv = in_the_codex_argv(&spec)?;

        assert_eq!(
            argv.contains(CHANGES_FILES),
            changes,
            "the control for {access:?}: the sandbox in argv decides whether this lead can write"
        );
        assert_eq!(
            as_a_set(can),
            argv,
            "Codex is described by the same argv it runs with ({access:?}). Judging it by the \
             Claude tool list would say it runs nothing, because it has no --tools to read"
        );
        assert!(
            as_a_set(can).contains(RUNS_COMMANDS),
            "and a Codex lead always runs commands: its sandbox limits writing, not running. A \
             sentence that hid this would promise a conversation where a shell stands"
        );
    }
    Ok(())
}
