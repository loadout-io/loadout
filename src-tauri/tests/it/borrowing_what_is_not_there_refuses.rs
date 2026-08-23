//! AC-2 dla T-93: nazwa, której ten projekt nie ma, zatrzymuje bieg **zanim ruszy pierwszy
//! proces** — i zanim w ogóle powstanie katalog biegu.
//!
//! Alternatywa dla odmowy jest jedna i jest najdroższą wersją tej wady: pominąć pozycję i jechać
//! dalej. Człowiek zaznacza rolę, agent nie dostaje jej reguł, nic nie pada i nikt się o tym nie
//! dowiaduje — bo „agent nie zna tych reguł" jest z zewnątrz nieodróżnialne od „model nie uznał,
//! że warto po nie sięgnąć". Niezmiennik 12 mówi, kiedy odmowa ma paść: najpóźniej przy Starcie,
//! nigdy w trakcie biegu.
//!
//! # Cztery słabe wersje tego kryterium
//!
//! **Pierwsza: `assert!(result.is_err())`.** Przechodzi dla implementacji, która sprawdza wybór
//! dopiero w kroku — czyli dla takiej, która zakłada katalog biegu, odpala pierwszego agenta,
//! płaci za jego turę i odmawia drugiemu. Rozróżnia to licznik uruchomień dublera równy zeru.
//!
//! **Druga: sam licznik.** Katalog biegu powstaje **przed** pierwszym sterownikiem, więc zero
//! uruchomień jest prawdą także dla builda, który zdążył zapisać na dysk `run.json` i kopię
//! plików dla kroku. Kontrakt żąda odmowy przed katalogiem biegu, więc osobną asercją jest to,
//! że w `.loadout/runs/` nie przybył ani jeden wpis.
//!
//! **Trzecia: odmowa bez nazw.** Zdanie, które nie mówi ANI której pozycji dotyczy, ANI którego
//! folderu, zamienia jedno odznaczenie w przeszukiwanie listy w cudzym repozytorium. Sprawdzamy
//! obie nazwy osobno.
//!
//! **Czwarta, po drugiej stronie: odmawianie za dużo.** Build, który odmawia biegu w folderze bez
//! `.claude/`, jest nie do użycia w każdym repozytorium, które nigdy nie widziało Claude Code —
//! a brak tego katalogu jest normalnym stanem cudzego repozytorium (niezmiennik 5). Tak samo
//! z vendorem: umiejętność jedzie katalogiem pluginu, więc vendor bez tej drogi nie może jej
//! dostać i krok nie rusza; sam tekst do promptu **żadnej** drogi vendora nie potrzebuje, więc
//! ten sam vendor z pożyczonym plikiem roli ma pojechać normalnie.
//!
//! JEDEN `#[test]`: zaślepka, która nigdy nie odmawia, przechodzi dwa punkty z czterech —
//! rozbite na osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony".

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
// `too_many_lines` z tego samego powodu, dla którego to jest JEDEN `#[test]`: rozbity na osobne
// zestawy dałby w warstwie `before` obraz „w połowie zielony".
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Powód w całości przy tej samej stałej w `skills_reach_the_step.rs`.
const PATIENCE: Duration = Duration::from_secs(30);

/// Rola, którą ten projekt ma.
const ROLE: &str = "backend-dev";
/// Rola, której ten projekt nie ma pod żadną postacią.
const NOBODY: &str = "nobody";
/// Umiejętność, którą ten projekt ma.
const ALPHA: &str = "alpha";

/// Znacznik z sekcji `## Recurring patterns`.
const PATTERNS_MARK: &str = "PATTERNS-ONLY-2e07";

/// Znacznik zadania kroku — po nim widać, że krok naprawdę ruszył z tym, o co proszono.
const STEP_MARK: &str = "STEP-PROMPT-2e08";

fn learnings_file() -> String {
    format!(
        "# Learnings — {ROLE}\n\n## Recurring patterns (BINDING)\n\n\
         - {PATTERNS_MARK}: never hold a plain mutex across an await.\n\n## Run journal\n\n- none.\n"
    )
}

fn skill_file(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says in a line what it is for.\n---\n\n\
         Answer with a single sentence.\n"
    )
}

fn agent_file(vendor: &str) -> String {
    format!(
        "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d2
name: Hand
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
---
Do the work.
"
    )
}

/// Jeden kafelek agenta; `borrow` wchodzi dosłownie tekstem, jaki poda wołający.
fn workflow_file(borrow: &str) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_borrow_missing",
  "name": "One step that borrows",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_only",
      "name": "Only step",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {{}},{borrow}
      "instructions": "{STEP_MARK}: do the work",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_name_this_project_does_not_have_stops_the_run() -> Result<(), Box<dyn Error>> {
    // ── (a) Rola, której u gospodarza nie ma: odmowa nazywająca rolę i folder ────────────
    let bench = Bench::new()?;
    bench.host_material()?;
    bench.agent(&agent_file("claude-code"))?;
    let workflow = bench.workflow(&workflow_file(&format!(
        "\n      \"borrow\": {{ \"learnings\": \"{NOBODY}\" }},"
    )))?;

    let before = bench.runs_so_far();
    let outcome = one_run(&bench, workflow).await?;

    let said = outcome
        .refusal
        .clone()
        .ok_or("a role this project does not have was accepted. Leaving it out quietly gives a run where the person picked a role, the agent got none of its rules, and nothing anywhere says so")?;
    assert!(
        said.contains(NOBODY),
        "the refusal does not name the role it is about: {said:?}. A refusal without the name \
         turns one unticked box into a search through somebody else's folder"
    );
    assert!(
        said.contains(&bench.project.path().display().to_string()),
        "the refusal does not say WHICH folder was looked in: {said:?}. The same role can be \
         present in one project and absent in the next, so the name alone leaves the person \
         guessing which of their folders is the one being talked about"
    );
    assert_eq!(
        outcome.started, 0,
        "the run started {} agent(s) before refusing. Refusing halfway is the expensive version \
         of this defect: the first agent is paid for, and the person reads a refusal about a run \
         that already spent money",
        outcome.started
    );
    assert_eq!(
        bench.runs_so_far(),
        before,
        "the run refused and left a run directory behind anyway. The refusal has to land before \
         the directory exists, or the history of this project fills up with runs that never \
         happened"
    );

    // ── (b) Folder bez `.claude/` i pusty wybór: to nie jest błąd ────────────────────────
    let bare = Bench::new()?;
    bare.agent(&agent_file("claude-code"))?;
    let plain = bare.workflow(&workflow_file(""))?;
    let quiet = one_run(&bare, plain).await?;
    assert!(
        quiet.refusal.is_none(),
        "a project with no .claude/ directory at all, and a step that borrows nothing, was \
         turned down: {:?}. Not having that folder is the normal state of somebody else's \
         repository, not a failure",
        quiet.refusal
    );
    assert_eq!(
        quiet.started, 1,
        "the step never ran in a project that had nothing to lend and was asked for nothing"
    );

    // ── (c) Vendor bez drogi na umiejętności: odmowa, która go nazywa ────────────────────
    let codex = Bench::new()?;
    codex.host_material()?;
    codex.agent(&agent_file("codex"))?;
    let with_skill = codex.workflow(&workflow_file(&format!(
        "\n      \"borrow\": {{ \"skills\": [\"{ALPHA}\"] }},"
    )))?;
    let before_codex = codex.runs_so_far();
    let refused = one_run(&codex, with_skill).await?;

    let about_the_app = refused.refusal.clone().ok_or(
        "this agent app has no way to be handed a skill directory, and the run started anyway. \
         The agent would answer as though there was nothing to know, and no screen would say so",
    )?;
    assert!(
        about_the_app.contains("Codex"),
        "the refusal does not name the agent app it is about: {about_the_app:?}. Without the \
         name the person cannot tell whether to unpick the skill or pick a different app"
    );
    assert_eq!(
        refused.started, 0,
        "the run started {} agent(s) before refusing over an app that cannot take the skill",
        refused.started
    );
    assert_eq!(
        codex.runs_so_far(),
        before_codex,
        "the refusal about the agent app landed after the run directory was already written"
    );

    // ── (d) …i ten sam vendor z samym tekstem jedzie normalnie ──────────────────────────
    let text_only = Bench::new()?;
    text_only.host_material()?;
    text_only.agent(&agent_file("codex"))?;
    let with_rules = text_only.workflow(&workflow_file(&format!(
        "\n      \"borrow\": {{ \"learnings\": \"{ROLE}\" }},"
    )))?;
    let went = one_run(&text_only, with_rules).await?;
    assert!(
        went.refusal.is_none(),
        "borrowing a role's rules was refused for an agent app that has no skill directory: \
         {:?}. Rules reach the agent as text on standard input, which every app takes",
        went.refusal
    );
    assert!(
        went.prompts.iter().any(|text| text.contains(PATTERNS_MARK)),
        "the step ran without the rules it borrowed. The prompts it was given were {:?}",
        went.prompts
    );

    Ok(())
}

/// Wynik jednego biegu: zdanie odmowy, licznik uruchomień i prompty, które dostały kroki.
struct Outcome {
    refusal: Option<String>,
    started: usize,
    prompts: Vec<String>,
}

async fn one_run(bench: &Bench, workflow: PathBuf) -> Result<Outcome, Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let started = Arc::new(AtomicUsize::new(0));
    let prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: counting_drivers(Arc::clone(&started), Arc::clone(&prompts)),
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

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let refusal = match outcome {
        Ok(_) => None,
        Err(error) => Some(error.to_string()),
    };
    let taken = std::mem::take(&mut *prompts.lock().unwrap());
    Ok(Outcome {
        refusal,
        started: started.load(Ordering::SeqCst),
        prompts: taken,
    })
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn counting_drivers(
    started: Arc<AtomicUsize>,
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
) -> Drivers {
    Arc::new(move |vendor| {
        let takes_a_directory = !matches!(vendor, loadout_lib::library::agents::Vendor::Codex);
        Arc::new(Counting {
            started: Arc::clone(&started),
            prompts: Arc::clone(&prompts),
            takes_a_directory,
        }) as Arc<dyn AgentDriver>
    })
}

/// Dubler, którego treścią jest licznik uruchomień i to, czy UMIE przyjąć katalog pluginu.
///
/// Rozróżnienie po vendorze siedzi tutaj, a nie w bieganym kodzie, i to jest cały sens tego
/// dublera: prawdziwy Codex nie nadpisuje `AgentDriver::inheriting`, więc dostaje domyślne
/// `None`. Dubler, który umiałby wszystko, mierzyłby przypadek, którego w produkcji nie ma.
#[derive(Debug)]
struct Counting {
    started: Arc<AtomicUsize>,
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
    takes_a_directory: bool,
}

#[async_trait]
impl AgentDriver for Counting {
    fn id(&self) -> &'static str {
        if self.takes_a_directory {
            "claude"
        } else {
            "codex"
        }
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("fake".to_owned()),
        })
    }

    fn inheriting(&self, _flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        if !self.takes_a_directory {
            return None;
        }
        Some(Arc::new(Self {
            started: Arc::clone(&self.started),
            prompts: Arc::clone(&self.prompts),
            takes_a_directory: true,
        }))
    }

    /// Bez tego dubler o identyfikatorze `claude` albo `codex` stanąłby na braku szwu dowodów
    /// i licznik pokazywałby zero z powodu, o którym to kryterium nie mówi.
    fn with_evidence(
        &self,
        _target: loadout_lib::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            started: Arc::clone(&self.started),
            prompts: Arc::clone(&self.prompts),
            takes_a_directory: self.takes_a_directory,
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Zamek wzięty i oddany w jednym wyrażeniu, bez `await` w środku (niezmiennik 8).
        {
            self.prompts.lock().unwrap().push(spec.prompt.clone());
        }
        self.started.fetch_add(1, Ordering::SeqCst);
        let session = SessionRef {
            vendor: "fake",
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
        fs::create_dir_all(home.path().join("skills"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn host_material(&self) -> Result<(), Box<dyn Error>> {
        let claude = self.project.path().join(".claude");
        let dir = claude.join("skills").join(ALPHA);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), skill_file(ALPHA))?;
        fs::create_dir_all(claude.join("learnings"))?;
        fs::write(
            claude.join("learnings").join(format!("{ROLE}.md")),
            learnings_file(),
        )?;
        Ok(())
    }

    fn agent(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("agents").join("hand.md"), text)?;
        Ok(())
    }

    fn workflow(&self, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("borrow.json");
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    /// Ile katalogów biegu leży dziś w tym projekcie. Zero przed pierwszym biegiem — także
    /// wtedy, gdy `.loadout/runs/` jeszcze nie istnieje.
    fn runs_so_far(&self) -> usize {
        fs::read_dir(self.project.path().join(".loadout").join("runs"))
            .map_or(0, |listing| listing.flatten().count())
    }
}
