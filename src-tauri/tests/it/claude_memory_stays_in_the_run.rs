//! AC-5 dla T-92: auto-pamięć Claude'a pisze do katalogu biegu i staje się kandydatkami.
//!
//! Zmierzone 2026-08-23 w `system/init` każdego kroku Claude'a: `memory_paths.auto` wskazuje
//! `~/.claude/projects/<projekt>/memory/`, czyli katalog, który człowiek **dzieli ze swoimi
//! sesjami interaktywnymi**. Krok Loadouta pisze tam bez pytania i bez śladu w biegu: nikt tego
//! nie widzi, nikt tego nie kuruje, a zdanie napisane przez agenta w cudzym biegu wraca potem do
//! promptu człowieka jako jego własna notatka. [T6 §10.4] nazywa przekierowanie tego katalogu per
//! bieg „najlepszym leverem znalezionym w researchu"; `ClaudeDriver::with_settings`
//! i `RunSettings::write` są gotowe od T-53 i do dziś **nie mają wołającego**.
//!
//! # Trzy słabe wersje tego kryterium
//!
//! **Pierwsza: sprawdzić, że `autoMemoryDirectory` stoi w pliku, i skończyć.** Przechodzi dla
//! dokumentu, który przy okazji przepisał hurtem `permissions` gospodarza — czyli dla tego
//! jednego kształtu, przed którym stoi cały `host.rs`. Dlatego niżej porównują się **całe
//! zbiory kluczy**, na obu poziomach, dokładnie jak w `driver_claude_settings_file.rs`.
//!
//! **Druga: zawołać szew wprost i nie sprawdzić, czy bieg go dotyka.** `with_settings` istnieje
//! od T-53 i przez trzy zadania nie miało ani jednego produkcyjnego wołającego — mechanizm
//! kompletny i nieużywany wygląda w testach jednostkowych identycznie jak wpięty. Dlatego drugi
//! test jedzie przez `run_workflow_inner`, a dubler stoi tam, gdzie stoi vendor.
//!
//! **Trzecia: policzyć notatki i nie zajrzeć, skąd są.** Katalog auto-pamięci trzyma też
//! `MEMORY.md` — indeks, który Claude Code pisze sam i który jest spisem tytułów, nie wiedzą.
//! Notatka z niego jest kandydatką bez treści, a w liczniku plików wygląda tak samo jak dobra.
//!
//! # Czego ten plik NIE sądzi
//!
//! Że `--settings` stoi w argv i niesie ścieżkę: to jest AC-3 z T-53
//! (`driver_claude_settings_file.rs`) i jest tam zmierzone od tamtej pory. Tutaj sądzi się
//! **treść tego samego pliku** i **to, że ktoś go w ogóle zamawia**.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::notes_root;
use loadout_lib::commands::run::{STEP_MEMORY_DIR, run_workflow_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::host::deny_rules;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, StepSettings, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::notes::{Note, Scope, Status, scan_notes};
use loadout_lib::store::Store;
use serde_json::Value;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte mają w biegu własne wymagania co
/// do dowodów, a to kryterium sądzi pamięć, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Dwie reguły gospodarza ze znacznikiem osobliwym na tyle, żeby ich obecność w naszym pliku nie
/// dała się wytłumaczyć zbiegiem okoliczności. Dwie, a nie jedna, bo kolejność listy odmów jest
/// asercją: przetasowana po drodze jest listą, której człowiek nie zweryfikuje spojrzeniem.
const DENY_FIRST: &str = "Read(LOADOUT-T92-DENY-MARKER-A/**)";
const DENY_SECOND: &str = "Read(LOADOUT-T92-DENY-MARKER-B/**)";

/// Plik gospodarza w kształcie, który `host::deny_rules` naprawdę spotyka: obok `deny` stoją
/// pola, które nas **rozszerzają**, i one mają nie przejechać granicy.
const HOST_SETTINGS: &str = r#"{
  "permissions": {
    "deny": ["Read(LOADOUT-T92-DENY-MARKER-A/**)", "Read(LOADOUT-T92-DENY-MARKER-B/**)"],
    "allow": ["Bash(rm:*)"]
  },
  "hooks": { "PreToolUse": [] },
  "env": { "LOADOUT_T92": "1" }
}
"#;

/// Klucz, którym `claude` włącza auto-pamięć [T6 §10.4].
const MEMORY_ENABLED: &str = "autoMemoryEnabled";

/// Klucz, którym `claude` przyjmuje katalog auto-pamięci [T6 §10.4].
const MEMORY_DIR: &str = "autoMemoryDirectory";

/// Identyfikator jedynego kroku. Katalog auto-pamięci jest **per krok**, więc ten napis jest
/// jednocześnie nazwą katalogu, którego szukamy pod `mem/`.
const STEP_ID: &str = "s_one";

/// Nazwa tego samego kroku. Inna niż identyfikator z rozmysłem: dzięki temu komunikat asercji
/// mówi, po czym implementacja nazwała katalog, zamiast milczeć o różnicy.
const STEP_NAME: &str = "Backend";

/// Nazwa agenta z biblioteki. To ona ma stanąć w polu `agent:` notatki, bo notatka o zakresie
/// „ten agent", która nie umie powiedzieć którego, nie wchodzi do żadnego promptu.
const AGENT_NAME: &str = "Backend Dev";

/// Znacznik pierwszego pliku tematycznego, który agent zostawił w swojej auto-pamięci.
const TOPIC_ONE: &str = "IBEX-TOPIC-QUEUE";

/// Znacznik drugiego.
const TOPIC_TWO: &str = "IBEX-TOPIC-TENANCY";

/// Znacznik indeksu, który Claude Code pisze sam. Nie jest wiedzą i nie ma zostać kandydatką.
const INDEX_MARK: &str = "IBEX-INDEX-NOT-A-NOTE";

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000b5
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

/// Jeden krok, jeden agent, folder projektu — żeby katalog roboczy kroku BYŁ projektem
/// gospodarza i żeby „reguły z `host::deny_rules`" miały jedną, niesporną odpowiedź.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_claude_memory_stays_in_the_run",
  "name": "One step that writes something down",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Backend",
      "agent": "01990000-0000-7000-8000-0000000000b5",
      "overrides": {},
      "instructions": "Look at the queue and say what it is doing.",
      "folder": { "use": "project" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Posortowane klucze obiektu JSON. `None`, kiedy to w ogóle nie jest obiekt.
fn keys(value: &Value) -> Option<Vec<&str>> {
    let mut names: Vec<&str> = value.as_object()?.keys().map(String::as_str).collect();
    names.sort_unstable();
    Some(names)
}

/// Pliki leżące bezpośrednio pod tym katalogiem, po nazwie, posortowane.
fn files_in(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect()
        },
    );
    found.sort();
    found
}

#[test]
fn the_settings_file_points_this_step_at_its_own_memory_directory() -> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let project = TempDir::new()?;
    fs::create_dir_all(project.path().join(".claude"))?;
    fs::write(
        project.path().join(".claude").join("settings.json"),
        HOST_SETTINGS,
    )?;

    // Fikstura, nie asercja kryterium: bez tej linii wszystko niżej jest też prawdą o pliku
    // gospodarza, którego nie dało się przeczytać, bo `deny_rules` odpowiada wtedy pustą listą.
    let deny = deny_rules(project.path());
    assert_eq!(
        deny,
        vec![DENY_FIRST.to_owned(), DENY_SECOND.to_owned()],
        "the fixture host file gave {deny:?}, so this test would be measuring an empty rule list"
    );

    let wanted = run.path().join(STEP_MEMORY_DIR).join(STEP_ID);
    let step = StepSettings {
        dir: run.path().to_path_buf(),
        memory: wanted.clone(),
        deny: deny.clone(),
    };

    // PRZEZ TRAIT, dokładnie tą drogą, którą chodzi bieg: fabryka z `lib.rs` wydaje
    // `Arc<dyn AgentDriver>` raz na aplikację, więc konkretny typ jest w kroku już zgubiony
    // i budowniczy z T-53 jest stamtąd nieosiągalny.
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
    assert!(
        driver.with_settings(&step).is_some(),
        "the claude driver has no way to take a settings file through the trait. The builder on \
         the concrete type has been complete and unreachable since T-53: a run holds \
         `Arc<dyn AgentDriver>`, so without this seam every step keeps writing its memory into \
         the directory the person shares with their own sessions [T6 section 10.4]"
    );

    let written = files_in(run.path());
    assert_eq!(
        written.len(),
        1,
        "asking for a settings file left {} file(s) in the run directory: {written:?}. One \
         document, because `--settings` names exactly one and the reader of it is exactly one \
         process (invariant 21)",
        written.len()
    );
    let path = &written[0];
    let raw = fs::read_to_string(path)?;
    let doc: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("the settings file does not parse as JSON ({error}): {raw:?}"))?;

    let top = keys(&doc).ok_or("the settings file is not a JSON object at the top level")?;
    assert_eq!(
        top,
        vec![MEMORY_DIR, MEMORY_ENABLED, "permissions"],
        "the top level of our settings file carries {top:?}. Whole key sets, not `is_some()`: \
         asking only whether the memory key is there passes for a document that also copied the \
         host's `env`, `hooks` and `sandbox` across, which is the one shape `host.rs` exists to \
         stop. The file was {raw:?}"
    );

    assert_eq!(
        doc.get(MEMORY_ENABLED),
        Some(&Value::Bool(true)),
        "{MEMORY_ENABLED} came out as {:?}. Redirecting the directory without turning the \
         feature on is a step that writes nowhere; leaving the key out is a step that writes \
         where it always did. The file was {raw:?}",
        doc.get(MEMORY_ENABLED)
    );
    assert_eq!(
        doc.get(MEMORY_DIR).and_then(Value::as_str),
        Some(wanted.to_string_lossy().as_ref()),
        "{MEMORY_DIR} points at {:?} instead of {wanted:?}. That path is the whole mechanism: \
         anywhere else and this step keeps appending to the directory the person reads in their \
         own interactive sessions, with nothing in the run saying it happened",
        doc.get(MEMORY_DIR).and_then(Value::as_str)
    );

    let permissions = doc
        .get("permissions")
        .ok_or("the settings file has no permissions object")?;
    let inner = keys(permissions).ok_or("permissions is not a JSON object")?;
    assert_eq!(
        inner,
        vec!["deny"],
        "permissions carries {inner:?}. The host's `allow` list is somebody else's policy and \
         does not cross this boundary — and one file is what carries both halves, because \
         `--settings` names one document. The file was {raw:?}"
    );
    let rules: Vec<&str> = permissions
        .get("deny")
        .and_then(Value::as_array)
        .ok_or("permissions.deny is not an array")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        rules,
        vec![DENY_FIRST, DENY_SECOND],
        "the deny list came out as {rules:?}. Wiring `--settings` enforces the host's rewritten \
         refusals at the same time, and it has to carry them in the order it was handed: a list \
         of refusals reshuffled on the way is one no person can verify at a glance"
    );

    Ok(())
}

#[test]
fn codex_has_nowhere_to_take_a_settings_file() -> Result<(), Box<dyn Error>> {
    let run = TempDir::new()?;
    let step = StepSettings {
        dir: run.path().to_path_buf(),
        memory: run.path().join(STEP_MEMORY_DIR).join(STEP_ID),
        deny: vec![DENY_FIRST.to_owned()],
    };

    let driver: Arc<dyn AgentDriver> = Arc::new(CodexDriver::new());
    assert!(
        driver.with_settings(&step).is_none(),
        "the codex driver accepted a settings file. `None` is not a shortfall here, it is the \
         answer: this document is a claude shape, and a vendor that cannot load it must not be \
         handed its path — the caller is the one who has to know (the same reason \
         `AgentDriver::inheriting` returns an Option)"
    );
    assert!(
        files_in(run.path()).is_empty(),
        "a driver that says it cannot take a settings file wrote one anyway: {:?}. That file has \
         no reader at all, and a document nobody loads in the run directory is rubbish that \
         looks like isolation",
        files_in(run.path())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn what_the_step_wrote_into_its_memory_comes_back_as_candidates() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;
    let seen = Arc::new(Seen::default());
    let report = a_run_of_one_step(&bench, &seen).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 1],
        "the step has to finish, or there is no turn to have written anything down and every \
         assertion below is true of a run that never got there. It ended as {:?}",
        report.steps
    );

    // ── 1. Bieg w ogóle zamawia plik ustawień dla tego kroku ──────────────────────────────
    let asked = seen.settings();
    assert_eq!(
        asked.len(),
        1,
        "the run asked for step settings {} time(s). Once per step: the seam has been complete \
         and callerless since T-53, so zero here means every claude step in this product still \
         writes its automatic memory into `~/.claude/projects/<project>/memory/`",
        asked.len()
    );
    let got = &asked[0];

    assert_eq!(
        got.dir, report.dir,
        "the settings file was ordered into {:?} instead of the run directory {:?}. Run \
         artefacts live under the run (docs/ARCHITECTURE.md section 8); anywhere else and it is \
         a run artefact outside its run",
        got.dir, report.dir
    );

    let wanted = report.dir.join(STEP_MEMORY_DIR).join(STEP_ID);
    assert_eq!(
        got.memory, wanted,
        "this step's memory was pointed at {:?} instead of {wanted:?}. Per step and keyed the \
         way every other per-step directory in a run is keyed - `work/<step id>` - because two \
         steps of one run are often two different agents, and one directory for both produces a \
         note nobody can say whose it is. The step's id is {STEP_ID:?} and its name is \
         {STEP_NAME:?}",
        got.memory
    );
    assert!(
        got.memory.starts_with(&report.dir),
        "this step's memory landed at {:?}, outside the run directory {:?}. Inside the run is \
         the entire point: that is what stops the step appending to the directory the person \
         shares with their own interactive sessions, and what makes what it wrote readable \
         afterwards together with the rest of the run",
        got.memory,
        report.dir
    );
    assert_eq!(
        got.deny,
        vec![DENY_FIRST.to_owned(), DENY_SECOND.to_owned()],
        "the settings this step was given carry {:?} as the host's refusals. The same file \
         carries both halves, so wiring it up enforces what the host repo asked to forbid at the \
         same time - and a run that drops them silently is one that forbids less than the \
         project it works in",
        got.deny
    );

    // ── 2. To, co agent zapisał, wraca jako kandydatki ────────────────────────────────────
    let left = notes_left(&bench);
    let rules: Vec<&str> = left.iter().map(|note| note.rule.as_str()).collect();
    assert_eq!(
        left.len(),
        2,
        "the two topical files this step left in its memory directory came back as {} note(s): \
         {rules:?}. Zero means the directory was redirected into the run and then nobody read \
         it, which trades a shared directory for a forgotten one",
        left.len()
    );
    assert!(
        rules.iter().any(|rule| rule.contains(TOPIC_ONE))
            && rules.iter().any(|rule| rule.contains(TOPIC_TWO)),
        "the notes read {rules:?} and the step wrote {TOPIC_ONE} and {TOPIC_TWO}. What the agent \
         put on disk is what has to land in the note: a sentence Loadout composed instead is one \
         nobody can check against the turn that produced it"
    );
    assert!(
        !rules.iter().any(|rule| rule.contains(INDEX_MARK)),
        "MEMORY.md came back as a note: {rules:?}. Claude Code writes that file itself as an \
         index of the others, so it is a list of titles rather than anything learned - and a \
         candidate with no content in it costs a person the same read as a real one"
    );

    for note in &left {
        assert_eq!(
            note.status,
            Status::Suggested,
            "a note taken out of this step's memory came out as {:?}. Only a person promotes \
             [ARCHITECTURE section 2 q. 5]; a sentence an agent wrote to itself, put straight \
             into use, reaches every later prompt without anybody agreeing to it. It reads: {}",
            note.status,
            note.rule
        );
        assert_eq!(
            note.scope,
            Scope::ThisAgent,
            "a note from this step came out with scope {:?}. This is what ONE agent wrote for \
             itself, so it belongs to that agent: `this-project` carries one agent's working \
             habit into every other agent's prompt. It reads: {}",
            note.scope,
            note.rule
        );
        assert_eq!(
            note.agent.as_deref(),
            Some(AGENT_NAME),
            "a note scoped to one agent says it belongs to {:?}. Without the name the third \
             scope has nothing to filter on and never enters any prompt at all - which is the \
             defect T-80 closed and this writer must not reopen. It reads: {}",
            note.agent,
            note.rule
        );
        assert!(
            note.because.contains(&report.id)
                && (note.because.contains(STEP_ID) || note.because.contains(STEP_NAME)),
            "a note from this step gives {:?} as its reason, and it has to name where it came \
             from - this run ({}) and this step ({STEP_ID} / {STEP_NAME}). `no because, no \
             memory` [T6 section 10.3] is not satisfied by a blank, and a claim with no route \
             back to the turn that made it is one nobody can retire either",
            note.because,
            report.id
        );
    }

    Ok(())
}

/// Wszystkie notatki, które ten bieg zostawił na dysku.
fn notes_left(bench: &Bench) -> Vec<Note> {
    scan_notes(&notes_root(bench.home.path())).expect("the notes root has to be readable")
}

/// Bieg z jednym krokiem, który się udał. Zwraca raport i to, co zobaczył dubler.
async fn a_run_of_one_step(bench: &Bench, seen: &Arc<Seen>) -> Result<RunReport, Box<dyn Error>> {
    let workflow = bench.workflow("claude-memory-stays", WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(seen)),
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
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(report)
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Seen(Mutex<Vec<StepSettings>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, settings: StepSettings) {
        self.lock().push(settings);
    }

    fn settings(&self) -> Vec<StepSettings> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<StepSettings>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen, memory: None });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    /// Katalog auto-pamięci, który ten klon dostał. `None` znaczy „nikt go jeszcze nie zamówił".
    memory: Option<PathBuf>,
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

    /// Klon z własnym katalogiem — dokładnie ten kształt, który ma `ClaudeDriver::inheriting`.
    fn with_settings(&self, settings: &StepSettings) -> Option<Arc<dyn AgentDriver>> {
        self.seen.record(settings.clone());
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
            memory: Some(settings.memory.clone()),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Tura pisze do swojej auto-pamięci tak, jak pisze do niej `claude`: pliki tematyczne
        // obok indeksu, który CLI utrzymuje samo.
        if let Some(dir) = &self.memory {
            fs::create_dir_all(dir)?;
            fs::write(
                dir.join("queue.md"),
                format!("# Queue\n\n{TOPIC_ONE} the queue is drained in exactly one place.\n"),
            )?;
            fs::write(
                dir.join("tenancy.md"),
                format!("# Tenancy\n\n{TOPIC_TWO} the tenant is resolved before the guard.\n"),
            )?;
            fs::write(
                dir.join("MEMORY.md"),
                format!("# Index\n\n{INDEX_MARK}\n- queue\n- tenancy\n"),
            )?;
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
        // Ani jednej pary `rule:`/`because:`: notatki w tym teście mają przyjść z katalogu
        // auto-pamięci, a nie z tury refleksji, którą sądzi AC-1.
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "The queue drains in one place.".to_owned(),
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
        // Ten sam korzeń, który rozwiązuje `commands::memory::notes_root`. ISTNIEJE i jest PUSTY:
        // „zero notatek" ma znaczyć „nikt nic nie zapisał", a nie „nie ma gdzie zapisywać".
        fs::create_dir_all(home.path().join("memory").join("notes"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Repo gospodarza ze swoimi regułami — krok pracuje w folderze projektu, więc to jest
        // dokładnie ten plik, który `host::deny_rules` przepisze.
        fs::create_dir_all(project.path().join(".claude"))?;
        fs::write(
            project.path().join(".claude").join("settings.json"),
            HOST_SETTINGS,
        )?;
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
