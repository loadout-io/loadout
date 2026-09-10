//! `run.json` niesie to, co aplikacja agenta wczytała z folderu, zanim krok powiedział słowo.
//!
//! # Co ten plik sądzi
//!
//! Krok staje w cudzym repozytorium i bierze stamtąd rzeczy, których Loadout mu nie dał. Linia
//! `system/init` mówi o nich wprost — wymienia `plugins`, `slash_commands`, `skills`,
//! `mcp_servers`, `memory_paths` i `agents` — a `SystemLine` czytała z niej dotąd cztery pola
//! i te sześć porzucała, więc w historii biegu nie było ani jednego miejsca, w którym dałoby się
//! to zobaczyć.
//!
//! **Ile z tego wchodzi, zależy od wersji CLI i raz już zmieniło się po cichu.** Na 2.1.251
//! docierał także `CLAUDE.md` gospodarza mimo `--setting-sources ""` i sześć kroków biegu
//! `20260823-145648` zapisało przez to pliki wyników wbrew temu, co kazał im Loadout; na 2.1.260
//! (zmierzone 2026-09-04, trzy przebiegi z kontrolą negatywną) już nie dociera. Changelog o tym
//! milczał — i to jest powód, dla którego to kryterium sądzi CYTAT z `system/init`, a nie wniosek
//! o izolacji: cytat przeżyje następną taką zmianę, wniosek nie.
//!
//! # Dlaczego to jedzie przez PRAWDZIWY dekoder, a nie przez ręcznie złożone zdarzenie
//!
//! Bo pytanie brzmi „czy te pola przeżyją drogę z drutu", a nie „czy struktura ma pola".
//! Zdarzenie złożone w teście przechodzi kryterium także wtedy, gdy `SystemLine` dalej porzuca
//! wszystko, co ta linia niosła — czyli dokładnie wtedy, gdy wada, dla której to zadanie
//! powstało, jest nietknięta.
//!
//! # Trzy kroki, trzy drogi, i żadna z nich nie jest ozdobą
//!
//! | krok | co wysyła | co ma stać w `run.json` |
//! |---|---|---|
//! | Build | pełną linię `init` przez prawdziwy dekoder | rekord z folderem i sześcioma listami |
//! | Tidy | samo `Started`, jak Codex i jak każdy dubler | ani jednego klucza |
//! | Judge | pełną linię `init`, a potem NIE PRZECHODZI | ten sam rekord co Build |
//!
//! Wiersz Tidy jest tym, który łapie implementację wpisującą pusty rekord każdemu krokowi:
//! taka przechodzi wiersz Build i dokłada klucz do KAŻDEGO kroku każdego biegu w historii —
//! kroków Codeksa, kafelków „sprawdź" i wszystkiego, co zapisano przed tą zmianą.
//!
//! Wiersz Judge jest tym, który mówi, KIEDY ten rekord powstaje: w chwili `init`, nie na końcu
//! kroku. Krok, który nie przeszedł, jest tą samą drogą co Stop człowieka, sufit czasu i panika
//! — księga na każdej z nich tylko DOPISUJE stan końcowy, a rekord leży w niej od pierwszego
//! zdarzenia tury. Krok, który przepadł po drodze, jest jedynym, którego to kryterium nie
//! przechodzi wprost, i jest nim dlatego, że nie ma go czym odróżnić od Tidy.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` **wyłącznie dodane**: trzy drogi tego kryterium mierzą JEDEN bieg trzech
// kroków i ten sam plik czytany dwiema drogami. Cięcie po granicy funkcji znaczyłoby trzy osobne
// biegi albo stan dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use loadout_lib::commands::history::read_run_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDecoder;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::stream::{Decoded, decode};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi plik biegu, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Nazwa katalogu, w którym stanął krok. Czytelna, bo to ona ma stanąć w zdaniu dla człowieka.
const FOLDER: &str = "ledger-ui";

/// Plik instrukcji gospodarza. Ławka KŁADZIE GO NA DYSKU w tym samym katalogu, który poda `init`
/// — i właśnie dlatego, że tam leży, rekord nie ma prawa go wymienić (2026-09-04, Z-16).
const HOST_INSTRUCTIONS: &str = "CLAUDE.md";

/// Sesja z fikstury — ta sama w `init`, żeby dekoder miał czym podpisać zdarzenie.
const SESSION: &str = "01996500-0000-7000-8000-0000000000aa";

/// Instrukcja kroku → nazwa kroku. Krok rozpoznajemy po treści zadania, bo `RunSpec` nie niesie
/// nazwy kroku, a instrukcja jest tym, co ten krok naprawdę dostał.
const STEPS: [(&str, &str); 3] = [
    ("build: ", "Build"),
    ("tidy: ", "Tidy"),
    ("judge: ", "Judge"),
];

/// Krok, który przedstawia się pełną linią `init` i przechodzi.
const BUILD: &str = "Build";
/// Krok, który nadaje samo `Started` — jak Codex, jak kafelek „sprawdź", jak każdy stary bieg.
const TIDY: &str = "Tidy";
/// Krok, który przedstawia się pełną linią `init`, a potem nie przechodzi.
const JUDGE: &str = "Judge";

/// Umiejętności, które CLI wymieniło o sobie w `init`.
const SKILLS: [&str; 3] = ["deep-research", "frontend-design", "brainstorming"];

/// Polecenia z ukośnikiem, które CLI wymieniło o sobie w `init`.
const SLASH_COMMANDS: [&str; 2] = ["deep-research", "design-sync"];

/// Katalogi pluginów, które CLI wymieniło o sobie w `init` — po nazwie, nie po ścieżce.
const PLUGINS: [&str; 1] = ["superpowers"];

/// Serwery narzędzi, które CLI wymieniło o sobie w `init` — po nazwie, nie po statusie.
const SERVERS: [&str; 1] = ["figma"];

/// Klucze `memory_paths`. **Tylko klucze**: wartością jest bezwzględna ścieżka w katalogu
/// domowym człowieka, a ta nie ma czego szukać w pliku, który zostaje po biegu.
const MEMORY_PATHS: [&str; 1] = ["auto"];

/// Podagenci, których CLI wymieniło o sobie w `init`.
const AGENTS: [&str; 2] = ["Explore", "Plan"];

/// Agent trzech kroków tej ławki. Jeden, bo to kryterium sądzi LINIĘ Z DRUTU, nie konfigurację.
const HAND: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000f1
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

/// Trzy kroki w łańcuchu, trzy różne drogi.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą. Łańcuch, bo dopiero on czyni kolejność
/// startów faktem o planie, a nie o tym, który wątek pierwszy dostał czas.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_folder_gave_it",
  "name": "Three steps, three ways in",
  "steps": [
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Build",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "build: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_tidy",
      "name": "Tidy",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "tidy: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_judge",
      "name": "Judge",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "judge: say hello and give up.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_build", "to": "s_tidy" },
    { "from": "s_tidy", "to": "s_judge" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_json_carries_what_the_cli_loaded_from_the_folder() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND)?;
    let workflow = bench.workflow("folder-gave-it", WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        library: bench.home.path().to_path_buf(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(&bench.folder),
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

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![
            StepState::Succeeded,
            StepState::Succeeded,
            StepState::Failed
        ],
        "the fixture is wrong if the three steps did not end the three ways it was built for; \
         nothing below means what it says. They ended as {:?}",
        report.steps
    );

    let run_file = report.dir.join("run.json");
    let run: Value = serde_json::from_str(&fs::read_to_string(&run_file)?)?;

    // ── (a) KROK, KTÓRY SIĘ PRZEDSTAWIŁ ─────────────────────────────────────────────────────
    let build = step_named(&run, BUILD).ok_or("run.json has no step called Build")?;
    let loaded = build.get("loaded_by_the_app").ok_or_else(|| {
        format!(
            "the line this step opened with named a folder, a plugin, two slash commands, three \
             skills, a tool server, a memory directory and two subagents, and the file that \
             outlives this run says nothing about any of it. Then \"the folder told this agent \
             something else\" is unanswerable after the fact. The entry was: {build}"
        )
    })?;

    assert_eq!(
        loaded.get("kind").and_then(Value::as_str),
        Some("loadedByTheApp"),
        "the record has to name itself as one of the closed kinds of context this run allows, \
         the same way frozen notes and handed-over answers do. The record was: {loaded}"
    );
    assert_eq!(
        loaded.get("folder").and_then(Value::as_str),
        Some(FOLDER),
        "the record has to name the folder this came out of, because that is the whole question \
         a person is asking. The record was: {loaded}"
    );
    /* SAMA OBECNOŚĆ PLIKU NA DYSKU NIE JEST DOWODEM, ŻE KTOKOLWIEK GO WCZYTAŁ (2026-09-04, Z-16).
     *
     * Ławka kładzie {HOST_INSTRUCTIONS} dokładnie w tym katalogu, który `init` podał jako swój —
     * i to jest cały sens tej asercji: warunek, w którym najłatwiej o skrót „plik tam leży, więc
     * agent go czyta". Sonda na 2.1.260 pokazała, że przy argv Loadouta ten plik do kroku NIE
     * dociera, a `system/init` nie ma pola, które by o nim mówiło. Rekord ma więc nieść wyłącznie
     * to, co CLI o sobie ogłosiło; nazwa tego pliku nie ma w nim czego szukać, bo byłaby
     * zgadywaniem podanym człowiekowi jako fakt. */
    assert!(
        !loaded.to_string().contains(HOST_INSTRUCTIONS),
        "the folder really holds {HOST_INSTRUCTIONS} and the record names it anyway. Lying about \
         which file an agent read is worse than saying nothing: it sends whoever debugs this run \
         to the wrong page. The record was: {loaded}"
    );

    for (key, wanted) in [
        ("plugins", PLUGINS.to_vec()),
        ("slash_commands", SLASH_COMMANDS.to_vec()),
        ("skills", SKILLS.to_vec()),
        ("mcp_servers", SERVERS.to_vec()),
        ("memory_paths", MEMORY_PATHS.to_vec()),
        ("agents", AGENTS.to_vec()),
    ] {
        assert_eq!(
            names_in(loaded, key),
            wanted
                .iter()
                .map(|one| (*one).to_owned())
                .collect::<Vec<String>>(),
            "the list \"{key}\" has to arrive whole and in the order the line gave it. The \
             record was: {loaded}"
        );
    }

    // Bezwzględna ścieżka w katalogu domowym człowieka jest wartością `memory_paths`, nie jego
    // kluczem — i tylko klucze wchodzą do pliku, tak samo jak `evidence::validate_manifest`
    // odmawia ścieżek gospodarza. Sprawdzane na CAŁYM rekordzie, bo to jest reguła o rekordzie.
    assert!(
        !loaded.to_string().contains("/Users/"),
        "a path from somebody's home directory reached the file that stays on disk after the \
         run. The record was: {loaded}"
    );

    // ── (b) KROK, KTÓRY SIĘ NIE PRZEDSTAWIŁ, NIE MA KLUCZA W OGÓLE ───────────────────────────
    let tidy = step_named(&run, TIDY).ok_or("run.json has no step called Tidy")?;
    assert_eq!(
        tidy.get("loaded_by_the_app"),
        None,
        "this step never said what it had loaded, and its entry carries a record of it anyway. \
         Every step of Codex, every tile that only runs the checks and every run written before \
         this change is in exactly that position — a key saying \"nothing\" on all of them is \
         length paid for silence, and worse, it reads as an answer. The entry was: {tidy}"
    );

    // ── (c) KROK, KTÓRY NIE PRZESZEDŁ, MA GO TAK SAMO ───────────────────────────────────────
    let judge = step_named(&run, JUDGE).ok_or("run.json has no step called Judge")?;
    let failed_but_loaded = judge.get("loaded_by_the_app").ok_or_else(|| {
        format!(
            "this step opened with the same line as Build and then did not pass, and its record \
             is gone. The record belongs to the moment the agent introduced itself, not to the \
             end of the step — a step that failed is the one a person opens history for. The \
             entry was: {judge}"
        )
    })?;
    assert_eq!(
        failed_but_loaded.get("folder").and_then(Value::as_str),
        Some(FOLDER),
        "and it has to be the same record, not an emptied one. The record was: {failed_but_loaded}"
    );

    // ── (d) TĄ SAMĄ DROGĄ, KTÓREJ UŻYWA OKNO ────────────────────────────────────────────────
    // `run.json` dowodzi, że fakt przeżył bieg; ta komenda dowodzi, że ma którędy dojść do
    // człowieka. Między jednym a drugim mieszka klasa wady, dla której to repo powstało.
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no name")?;
    let opened = read_run_inner(bench.project.path(), folder)?;
    let through_the_window = serde_json::to_value(&opened)?;
    let build_wire =
        step_named(&through_the_window, BUILD).ok_or("the opened run has no step called Build")?;
    let wire_record = build_wire.get("loadedByTheApp").ok_or_else(|| {
        format!(
            "the run file carries this record and the command the window reads runs with drops \
             it, so the screen has nothing to draw. The step was: {build_wire}"
        )
    })?;
    assert_eq!(
        wire_record.get("folder").and_then(Value::as_str),
        Some(FOLDER),
        "the folder is the one word that sentence needs. It came over as: {wire_record}"
    );
    assert_eq!(
        names_in(wire_record, "skills").len(),
        SKILLS.len(),
        "and so is the count of skills. It came over as: {wire_record}"
    );

    // ── (e) PLIK BEZ TEGO KLUCZA DALEJ SIĘ OTWIERA ──────────────────────────────────────────
    // Czyli plik zapisany przez każdego Loadouta sprzed tego zadania (niezmienniki 4 i 25).
    let mut older = run.clone();
    forget_the_record(&mut older);
    assert_ne!(
        older, run,
        "the fixture is wrong if stripping the record changes nothing — then (e) proves only \
         that the same file reads twice"
    );
    fs::write(&run_file, serde_json::to_string_pretty(&older)?)?;

    let from_the_older = read_run_inner(bench.project.path(), folder)?;
    assert_eq!(
        serde_json::to_value(&from_the_older)?
            .get("steps")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(STEPS.len()),
        "a run.json written before this key existed has to open into the same rows. Every run in \
         every project's history was written without it, and a reader that needs it turns them \
         all into runs nobody can open"
    );

    Ok(())
}

/// Wpis kroku o tej nazwie, prosto z `run.json` albo z odpowiedzi komendy okna.
fn step_named<'a>(run: &'a Value, name: &str) -> Option<&'a Value> {
    run.get("steps")?
        .as_array()?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
}

/// Lista napisów spod tego klucza — pusta, kiedy klucza nie ma albo nie jest listą.
fn names_in(record: &Value, key: &str) -> Vec<String> {
    record
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Ten sam `run.json`, tylko bez tego zapisu — czyli tak, jak wyglądał przed Z-16.
fn forget_the_record(run: &mut Value) {
    let Some(steps) = run.get_mut("steps").and_then(Value::as_array_mut) else {
        return;
    };
    for step in steps {
        if let Some(fields) = step.as_object_mut() {
            fields.remove("loaded_by_the_app");
        }
    }
}

/// Linia `system/init` w kształcie, w którym przychodzi z prawdziwego CLI.
///
/// `cwd` wskazuje katalog, który NAPRAWDĘ istnieje i naprawdę trzyma `CLAUDE.md`: pytanie „czy
/// ten plik tam leży" ma dostać odpowiedź z dysku, a nie z fikstury.
fn init_line(folder: &Path) -> String {
    json!({
        "type": "system",
        "subtype": "init",
        "cwd": folder,
        "session_id": SESSION,
        "model": "opus",
        "tools": ["Read", "Write"],
        "capabilities": ["interrupt_receipt_v1"],
        // Pluginy i serwery narzędzi jadą jako obiekty, nie napisy — tak wygląda drut.
        "plugins": [{
            "name": PLUGINS[0],
            "path": "/Users/somebody/.claude/plugins/superpowers",
            "source": "marketplace",
            "version": "1.4.0"
        }],
        "slash_commands": SLASH_COMMANDS,
        "skills": SKILLS,
        "mcp_servers": [{ "name": SERVERS[0], "status": "connected" }],
        // Wartością jest bezwzględna ścieżka w cudzym katalogu domowym. Kluczem — jedno słowo.
        "memory_paths": { MEMORY_PATHS[0]: "/Users/somebody/.claude/projects/ledger/memory/" },
        "agents": AGENTS
    })
    .to_string()
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(folder: &Path) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        folder: folder.to_path_buf(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    /// Katalog, który linia `init` poda jako swój — ten sam, w którym leży `CLAUDE.md`.
    folder: PathBuf,
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
        let step = STEPS
            .iter()
            .find(|(instruction, _)| spec.prompt.starts_with(instruction))
            .map_or(TIDY, |(_, name)| *name);

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        if step == TIDY {
            // BEZ LINII `init`. Tak wygląda krok Codeksa, kafelek „sprawdź" i każdy dubler w tym
            // drzewie: sesja jest ogłoszona, a o folderze nie pada ani słowo.
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
        } else {
            /* PRAWDZIWY DEKODER, a nie ręcznie złożone zdarzenie. Fikstura, która składa
             * zdarzenie sama, przechodzi to kryterium także wtedy, gdy `SystemLine` dalej
             * porzuca sześć pól tej linii — czyli wtedy, gdy wada jest nietknięta. */
            let mut reader = ClaudeDecoder::new();
            let Decoded::Events(decoded) = decode(&mut reader, &init_line(&self.folder)) else {
                return Err(anyhow!(
                    "the fixture line is not one this decoder recognises, so this criterion \
                     would judge the fixture and not the code"
                ));
            };
            for one in decoded {
                let _ = events.send(one).await;
            }
        }

        Ok(Box::new(Turn {
            events,
            session,
            gives_up: step == JUDGE,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    /// Czy ta tura ma się nie udać.
    gives_up: bool,
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
            ok: !self.gives_up,
            reason: if self.gives_up {
                FinishReason::Failed("the step could not do the work".to_owned())
            } else {
                FinishReason::Completed
            },
            text: "Said hello.".to_owned(),
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
    /// Katalog gospodarza z `CLAUDE.md` w środku — ten, o którym mówi linia `init`.
    folder: PathBuf,
    /// Trzymany, żeby katalog wyżej przeżył cały bieg.
    _host: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let host = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;

        // Nazwany katalog, nie sam `TempDir`: nazwa folderu jedzie do pliku i na ekran, więc
        // fikstura ma ją wybrać, a nie odziedziczyć losowy przyrostek z `$TMPDIR`.
        let folder = host.path().join(FOLDER);
        fs::create_dir_all(&folder)?;
        fs::write(
            folder.join(HOST_INSTRUCTIONS),
            "# House rules\n\nAlways write your answer to results.md.\n",
        )?;

        Ok(Self {
            home,
            project,
            folder,
            _host: host,
        })
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
