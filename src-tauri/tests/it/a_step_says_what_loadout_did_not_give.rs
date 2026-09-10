//! Krok mówi, co wczytał z folderu POZA tym, co włożył mu bieg (2026-09, Z-47).
//!
//! # Co ten plik sądzi
//!
//! Z-16 zapisał w `run.json`, co aplikacja agenta ogłosiła o sobie w `system/init`, i historia
//! to pokazuje. Nikt jednak nie porównywał tej listy z tym, co Loadout WŁOŻYŁ — a to jest całe
//! pytanie, dla którego tamten rekord powstał. `docs/ARCHITECTURE.md` §4: na 2.1.251 plik
//! instrukcji gospodarza docierał do kroku mimo `--setting-sources ""`, na 2.1.260 już nie,
//! i changelog o tym obrocie milczał. Następny taki obrót zobaczy wyłącznie ktoś, kto otworzy
//! panel i porówna listę ręcznie, pozycja po pozycji.
//!
//! # Porównanie nie potrzebuje ani jednego nowego klucza w pliku
//!
//! Rzeczy biegu są rozpoznawalne po nazwach, którymi bieg sam je przypiął: pluginy
//! `loadout-skills` i `loadout-inherited` (`inherit::rewrite`), umiejętności z nich wracają
//! w `init` jako `<plugin>:<nazwa>`, a przekierowany katalog pamięci jedzie do CLI jako klucz
//! `auto`. Zdanie powstaje więc przy ODCZYCIE — i dlatego bieg zapisany przed tą zmianą
//! dostaje je tak samo jak dzisiejszy (niezmiennik 4).
//!
//! # Dlaczego to jedzie przez PRAWDZIWY dekoder, a nie przez ręcznie złożone zdarzenie
//!
//! Z tego samego powodu, co w `a_step_says_what_the_folder_gave_it`: pytanie brzmi „czy te pola
//! przeżyją drogę z drutu", a zdarzenie złożone w teście przechodzi także wtedy, gdy `SystemLine`
//! dalej porzuca to, co ta linia niosła.
//!
//! # Sześć kroków, sześć dróg
//!
//! | krok | co ogłasza `init` | co ma stać na karcie |
//! |---|---|---|
//! | Skills | cztery umiejętności, jedna z pluginu biegu | „3 skills", nigdy cztery |
//! | Memory | `auto` biegu i drugi katalog pamięci | „a memory folder" |
//! | Plugins | plugin biegu i cudzy plugin | „a plugin" |
//! | Own | wyłącznie własność biegu | ani słowa |
//! | Quiet | sam `cwd`, bez ani jednej listy | ani słowa |
//! | Nameless | jedną umiejętność i ani słowa o folderze | zdanie bez członu „from …" |
//!
//! Own i Quiet są tymi, które łapią implementację mówiącą to zdanie zawsze: taka przechodzi trzy
//! pierwsze wiersze i kłamie o każdym kroku, który wziął wyłącznie to, co mu dano. Nameless jest
//! tym, który łapie zdanie urwane w połowie — „from " zakończone niczym czyta się jak napis,
//! który się nie dorysował, a rekord bez nazwy folderu zdarza się przy uszkodzonym pliku.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` **wyłącznie dodane**: pięć dróg tego kryterium mierzy JEDEN bieg pięciu
// kroków. Cięcie po granicy funkcji znaczyłoby pięć osobnych biegów albo stan dzielony między
// testami, które cargo uruchamia równolegle.
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
/// co do dowodów, a to kryterium sądzi odczyt biegu, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Nazwa katalogu, w którym stanął krok. Czytelna, bo to ona ma stanąć w zdaniu dla człowieka.
const FOLDER: &str = "ledger-ui";

/// Sesja z fikstury — ta sama w `init`, żeby dekoder miał czym podpisać zdarzenie.
const SESSION: &str = "01996500-0000-7000-8000-0000000000aa";

/// Nazwy pluginów, którymi bieg przypina SWÓJ materiał.
///
/// WYPISANE Z PALCA, choć składa je `inherit::rewrite`: tutaj są tym, co ogłasza vendor, a nie
/// tym, co pisze Loadout. Import tamtych stałych czytałby tę samą wartość po obu stronach
/// porównania, więc zmiana nazwy przechodziłaby bezszelestnie — a to jest właśnie ta zmiana,
/// po której krok zaczyna liczyć własne umiejętności biegu jako cudze.
const LIBRARY_PLUGIN: &str = "loadout-skills";
const INHERITED_PLUGIN: &str = "loadout-inherited";

/// Klucz przekierowanej pamięci biegu — ten sam, którym CLI nazywa ją w `init`.
const AUTO_MEMORY: &str = "auto";

/// Instrukcja kroku → nazwa kroku. Krok rozpoznajemy po treści zadania, bo `RunSpec` nie niesie
/// nazwy kroku, a instrukcja jest tym, co ten krok naprawdę dostał.
const STEPS: [(&str, &str); 6] = [
    ("skills: ", "Skills"),
    ("memory: ", "Memory"),
    ("plugins: ", "Plugins"),
    ("own: ", "Own"),
    ("quiet: ", "Quiet"),
    ("nameless: ", "Nameless"),
];

/// Krok, do którego z folderu doszły trzy umiejętności obok jednej z pluginu biegu.
const SKILLS: &str = "Skills";
/// Krok, który obok pamięci biegu ogłosił drugi katalog pamięci.
const MEMORY: &str = "Memory";
/// Krok, który obok pluginu biegu ogłosił cudzy plugin.
const PLUGINS: &str = "Plugins";
/// Krok, który ogłosił wyłącznie to, co dostał od biegu.
const OWN: &str = "Own";
/// Krok, którego `init` nie ma ani jednej z tych list.
const QUIET: &str = "Quiet";
/// Krok, który wziął coś z folderu i nie powiedział, jak ten folder się nazywa.
const NAMELESS: &str = "Nameless";

/// Agent pięciu kroków tej ławki. Jeden, bo to kryterium sądzi LINIĘ Z DRUTU, nie konfigurację.
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

/// Sześć kroków w łańcuchu, sześć różnych linii `init`.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_not_given",
  "name": "Five steps, five ways in",
  "steps": [
    {
      "kind": "agent",
      "id": "s_skills",
      "name": "Skills",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "skills: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_memory",
      "name": "Memory",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "memory: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_plugins",
      "name": "Plugins",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "plugins: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_own",
      "name": "Own",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "own: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 720, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_quiet",
      "name": "Quiet",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "quiet: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 960, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_nameless",
      "name": "Nameless",
      "agent": "01990000-0000-7000-8000-0000000000f1",
      "overrides": {},
      "instructions": "nameless: say hello and stop.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 1200, "y": 0 }
    }
  ],
  "links": [
    { "from": "s_skills", "to": "s_memory" },
    { "from": "s_memory", "to": "s_plugins" },
    { "from": "s_plugins", "to": "s_own" },
    { "from": "s_own", "to": "s_quiet" },
    { "from": "s_quiet", "to": "s_nameless" }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn read_run_says_what_loadout_did_not_give_this_step() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND)?;
    let workflow = bench.workflow("not-given", WORKFLOW)?;
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
        vec![StepState::Succeeded; STEPS.len()],
        "the fixture is wrong if the five steps did not all get through; nothing below means \
         what it says. They ended as {:?}",
        report.steps
    );

    // TĄ SAMĄ DROGĄ, KTÓREJ UŻYWA OKNO (niezmiennik 29). Pole struktury dowiodłoby, że
    // mechanizm istnieje; ta komenda jest tym, co panel historii naprawdę czyta.
    let folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run directory has no name")?;
    let opened = serde_json::to_value(read_run_inner(bench.project.path(), folder)?)?;

    // ── (a) TRZY UMIEJĘTNOŚCI Z FOLDERU OBOK JEDNEJ Z PLUGINU BIEGU ─────────────────────────
    assert_eq!(
        not_given(&opened, SKILLS)?,
        Some(said("3 skills")),
        "this step announced four skills and one of them is the one the run itself put there. \
         A sentence counting four sends whoever reads it looking for a fourth thing the folder \
         never handed over; a missing sentence leaves the whole comparison to be done by hand, \
         which is the state this record was written to end. The step came over as: {:?}",
        step_named(&opened, SKILLS)
    );

    // ── (b) DRUGI KATALOG PAMIĘCI OBOK PRZEKIEROWANEGO ──────────────────────────────────────
    assert_eq!(
        not_given(&opened, MEMORY)?,
        Some(said("a memory folder")),
        "the run redirects one memory folder and this step announced a second one. That second \
         one is somebody else's directory, read before the step said a word — exactly the kind \
         of context this sentence exists to name. The step came over as: {:?}",
        step_named(&opened, MEMORY)
    );

    // ── (c) CUDZY PLUGIN OBOK PLUGINU BIEGU ─────────────────────────────────────────────────
    assert_eq!(
        not_given(&opened, PLUGINS)?,
        Some(said("a plugin")),
        "this step announced two plugins and one of them the run put there itself. The other one \
         came from the folder. The step came over as: {:?}",
        step_named(&opened, PLUGINS)
    );

    // ── (d) KROK, KTÓRY WZIĄŁ WYŁĄCZNIE TO, CO MU DANO, MILCZY ──────────────────────────────
    let own = step_named(&opened, OWN).ok_or("the opened run has no step called Own")?;
    assert!(
        !names_in(record_of(own)?, "skills").is_empty(),
        "the fixture is wrong if this step announced nothing at all — then its silence below \
         proves only that an empty record says nothing. It came over as: {own}"
    );
    assert_eq!(
        not_given(&opened, OWN)?,
        None,
        "this step read the run's own plugins and the run's own memory folder and nothing else, \
         and the card tells a person it went looking elsewhere. A sentence that also fires for \
         steps which took only what they were handed makes every step look the same, and then \
         the one step that really did read somebody else's folder is invisible among them. The \
         step came over as: {own}"
    );

    // ── (e) KROK, KTÓRY NIE OGŁOSIŁ ŻADNEJ Z TYCH LIST, MILCZY TAK SAMO ─────────────────────
    let quiet = step_named(&opened, QUIET).ok_or("the opened run has no step called Quiet")?;
    assert_eq!(
        record_of(quiet)?.get("folder").and_then(Value::as_str),
        Some(FOLDER),
        "the fixture is wrong if this step has no record at all — the point of this way in is a \
         record whose lists are empty, not a missing one. It came over as: {quiet}"
    );
    assert_eq!(
        not_given(&opened, QUIET)?,
        None,
        "this step named its folder and not one thing it took from it, and the card says it read \
         something anyway. Every step of Codex, every tile that only runs the checks and every \
         run written before 2026-09 is in that same position. The step came over as: {quiet}"
    );

    // ── (f) KROK, KTÓRY NIE NAZWAŁ FOLDERU, MÓWI ZDANIE BEZ TEGO CZŁONU ─────────────────────
    assert_eq!(
        not_given(&opened, NAMELESS)?,
        Some("This step also read 1 skill that Loadout did not give it".to_owned()),
        "this step took a skill from a folder it never named, and the sentence either ends with \
         a dangling \"from \" or does not come at all. A line that stops mid-word reads like \
         markup that failed to draw, and silence loses the one fact this record has. The step \
         came over as: {:?}",
        step_named(&opened, NAMELESS)
    );

    // ── (g) ZDANIE POWSTAJE PRZY ODCZYCIE, WIĘC DOSTAJE JE TAKŻE STARY BIEG ─────────────────
    // Plik biegu nie zyskuje ani jednego klucza: porównanie stać na same nazwy, którymi bieg
    // przypiął swoje rzeczy. Gdyby zdanie było zapisywane, każdy bieg zapisany do dziś milczałby
    // na zawsze — a to są wszystkie biegi, które ktokolwiek ma na dysku (niezmiennik 4).
    let on_disk = fs::read_to_string(report.dir.join("run.json"))?;
    assert!(
        !on_disk.contains("did not give it"),
        "the sentence went into the file that stays after the run. Then every run written before \
         today opens without it — and those are all the runs anybody has. The file was: {on_disk}"
    );

    Ok(())
}

/// Zdanie w brzmieniu, w którym ma stanąć na karcie kroku i w historii biegu.
fn said(what: &str) -> String {
    format!("This step also read {what} from {FOLDER} that Loadout did not give it")
}

/// Zdanie tego kroku z drutu — `None`, kiedy krok nic takiego nie mówi.
///
/// Błędem jest wyłącznie krok, którego na drucie nie ma: brak KLUCZA nie jest tu błędem osobno,
/// bo `None` i tak przewraca asercje pozytywne, a `Err` w tym miejscu czytałby się jak wada
/// fikstury.
fn not_given(run: &Value, name: &str) -> Result<Option<String>, String> {
    let step = step_named(run, name).ok_or_else(|| format!("the opened run has no step {name}"))?;
    Ok(step
        .get("whatLoadoutDidNotGive")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

/// Wpis kroku o tej nazwie, prosto z odpowiedzi komendy okna.
fn step_named<'a>(run: &'a Value, name: &str) -> Option<&'a Value> {
    run.get("steps")?
        .as_array()?
        .iter()
        .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
}

/// Rekord Z-16 tego kroku — ten, z którego liczy się zdanie.
fn record_of(step: &Value) -> Result<&Value, String> {
    step.get("loadedByTheApp")
        .ok_or_else(|| format!("this step carries no record of what it loaded: {step}"))
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

/// Linia `system/init` tego kroku, w kształcie, w którym przychodzi z prawdziwego CLI.
///
/// Wartości `memory_paths` są bezwzględnymi ścieżkami w cudzym katalogu domowym, tak jak na
/// prawdziwym drucie — do pliku biegu jadą z nich same klucze.
fn init_line(step: &str, folder: &Path) -> String {
    let mut line = json!({
        "type": "system",
        "subtype": "init",
        "session_id": SESSION,
        "model": "opus",
        "tools": ["Read", "Write"]
    });
    let Some(fields) = line.as_object_mut() else {
        return line.to_string();
    };
    // Krok „Nameless" jest jedynym, który nie mówi, gdzie stanął.
    if step != NAMELESS {
        fields.insert("cwd".to_owned(), json!(folder));
    }
    // Krok „Quiet" nie dokłada ani jednej z tych list — i to jest cała jego droga.
    match step {
        SKILLS => {
            fields.insert(
                "skills".to_owned(),
                json!([
                    format!("{LIBRARY_PLUGIN}:pdf"),
                    "deep-research",
                    "frontend-design",
                    "brainstorming"
                ]),
            );
        }
        MEMORY => {
            fields.insert(
                "memory_paths".to_owned(),
                json!({
                    AUTO_MEMORY: "/Users/somebody/.loadout/runs/memory/",
                    "project": "/Users/somebody/.claude/projects/ledger/memory/"
                }),
            );
        }
        PLUGINS => {
            fields.insert(
                "plugins".to_owned(),
                json!([
                    { "name": LIBRARY_PLUGIN, "source": "local", "version": "1.0.0" },
                    { "name": "superpowers", "source": "marketplace", "version": "1.4.0" }
                ]),
            );
        }
        OWN => {
            fields.insert(
                "skills".to_owned(),
                json!([
                    format!("{LIBRARY_PLUGIN}:pdf"),
                    format!("{INHERITED_PLUGIN}:house-style")
                ]),
            );
            fields.insert(
                "plugins".to_owned(),
                json!([{ "name": LIBRARY_PLUGIN }, { "name": INHERITED_PLUGIN }]),
            );
            fields.insert(
                "memory_paths".to_owned(),
                json!({ AUTO_MEMORY: "/Users/somebody/.loadout/runs/memory/" }),
            );
        }
        NAMELESS => {
            fields.insert("skills".to_owned(), json!(["deep-research"]));
        }
        _ => (),
    }
    line.to_string()
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
    /// Katalog, który linia `init` poda jako swój.
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
            .map_or(QUIET, |(_, name)| *name);

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        /* PRAWDZIWY DEKODER, a nie ręcznie złożone zdarzenie. Fikstura, która składa zdarzenie
         * sama, przechodzi to kryterium także wtedy, gdy `SystemLine` gubi pola tej linii —
         * czyli wtedy, gdy porównywać nie ma czego. */
        let mut reader = ClaudeDecoder::new();
        let Decoded::Events(decoded) = decode(&mut reader, &init_line(step, &self.folder)) else {
            return Err(anyhow!(
                "the fixture line is not one this decoder recognises, so this criterion would \
                 judge the fixture and not the code"
            ));
        };
        for one in decoded {
            let _ = events.send(one).await;
        }

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
    /// Katalog gospodarza, o którym mówi linia `init`.
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
