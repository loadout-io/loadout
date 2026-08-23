//! AC-3 dla T-90: „Write results to" zapisuje odpowiedź tam, gdzie człowiek wskazał.
//!
//! # Po co to istnieje
//!
//! `writeResultsTo` ma wiersz w panelu kroku, jest nadpisywalne na kafelku, jest w formacie
//! agenta i w formacie nadpisań — i ma w całym drzewie **zero czytelników**. Człowiek wpisuje
//! ścieżkę, ekran ją przyjmuje, plik ją zapisuje, i nie powstaje nic. To jest martwa kontrolka
//! (niezmiennik 16) w najczystszej postaci: pusty katalog wygląda dokładnie tak samo jak
//! katalog, do którego agent nic nie miał do napisania.
//!
//! # Zapisuje LOADOUT, nie agent, i to jest treść tego kryterium
//!
//! Blok „jak odpowiadać" mówi każdemu krokowi wprost: *„Do not write your results to a file"*.
//! Gdyby tę ścieżkę miał obsłużyć agent, produkt kazałby mu robić dokładnie to, czego przed
//! chwilą zabronił — a krok z dialem „look only" nie umiałby tego wykonać i spaliłby turę na
//! próbie, dokładnie jak sześć kroków z biegu `20260823-145648`.
//!
//! # Trzy słabe wersje tego kryterium
//!
//! **„Plik istnieje".** Przechodzi dla implementacji, która zapisuje samo podsumowanie albo
//! obcina odpowiedź do pierwszego wiersza. Dlatego porównywana jest CAŁA treść, co do bajtu,
//! z tym, co powiedział agent.
//!
//! **„Plik jest w projekcie".** Ścieżka jest liczona **względem folderu kroku**, a nie folderu
//! projektu — i te dwa są tym samym dokładnie dla kroków `project`, czyli dla połowy fikstur.
//! Dlatego jeden krok tej ławki ma własną kopię plików i jego wynik ma wylądować w NIEJ.
//!
//! **„Nie ma pliku poza folderem".** Sprawdzenie samego skutku przechodzi dla implementacji,
//! która próbuje zapisać i cicho się poddaje — a wtedy człowiek dostaje bieg bez wyniku i bez
//! zdania. Ścieżka wyprowadzająca poza folder jest odmową **przed startem**, więc mierzymy
//! jedno i drugie: zdanie nazywające pole ORAZ to, że nie ruszył ani jeden agent.
//!
//! # I przekazanie zostaje przekazaniem
//!
//! Plik pod wskazaną ścieżką jest KOPIĄ, nie zamianą: indeks następnego kroku stoi na
//! `handoffs/`, więc zapis, który by je zastąpił, uciszyłby cały ruch między krokami, a bieg
//! dalej wyglądałby na udany.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunError, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::memory::handoff;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne
/// wymagania co do prywatnych dowodów, a to kryterium sądzi pliki, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Ścieżka, którą człowiek wpisał agentowi. **W podkatalogu**, którego jeszcze nie ma: katalogi
/// po drodze zakłada Loadout, a implementacja, która tego nie robi, wygląda jak zapis, który
/// się nie udał i nic o tym nie powiedział.
const ASKED_FOR: &str = "results/report.md";

/// Ścieżka wpisana NA KROKU, czyli nadpisanie. Efektywna wartość to ta, nie ta z definicji
/// agenta — inaczej wiersz w panelu kroku jest kontrolką bez skutku.
const OVERRIDDEN: &str = "out/own.md";

/// Etykieta pola, którą człowiek widzi nad kontrolką (`step-panel/panel.tsx`). Odmowa ma
/// nazywać JĄ, nie klucz z pliku: `writeResultsTo` nie istnieje na żadnym ekranie
/// (niezmiennik 14).
const FIELD_ON_SCREEN: &str = "Write results to";

/// Odpowiedź dublera. Kilka wierszy i nagłówki, bo zapis ma oddać CAŁOŚĆ — implementacja
/// zapisująca podsumowanie przechodzi każde pytanie o istnienie pliku.
const SAID: &str = "## Answer\nThe header row is in place.\n\n## Evidence\nnotes.txt:1\n\n## Open\nWhether the second row matters.\n";

fn agent_file(id: &str, name: &str, write_results_to: &str) -> String {
    format!(
        "---
schema: 1
id: {id}
name: {name}
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"{write_results_to}\"
tools: everything
skills: []
connections: []
---
Do the work.
"
    )
}

/// Trzy kroki w łańcuchu: jeden bez ścieżki, jeden ze ścieżką z definicji agenta, jeden ze
/// ścieżką nadpisaną na kroku i własną kopią plików.
///
/// Łańcuch, nie trzy luźne kafelki: dwa kroki, które mogą biec równocześnie w folderze projektu,
/// są odmową przed pierwszym procesem (niezmiennik 12).
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_results_are_written_where_asked",
  "name": "One quiet step and two that file their answer",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plain",
      "name": "Plain",
      "agent": "01990000-0000-7000-8000-00000000092a",
      "overrides": {},
      "instructions": "plain: do the work and say what you found.",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_filed",
      "name": "Filed",
      "agent": "01990000-0000-7000-8000-00000000092b",
      "overrides": {},
      "instructions": "filed: do the work and say what you found.",
      "at": { "x": 0, "y": 240 }
    },
    {
      "kind": "agent",
      "id": "s_own",
      "name": "Own",
      "agent": "01990000-0000-7000-8000-00000000092b",
      "overrides": { "writeResultsTo": "out/own.md" },
      "instructions": "own: do the work and say what you found.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 480 }
    }
  ],
  "links": [
    { "from": "s_plain", "to": "s_filed" },
    { "from": "s_filed", "to": "s_own" }
  ]
}
"#;

/// Jeden krok z własną kopią plików — ławka dla dwóch odmów.
const ONE_STEP: &str = r#"{
  "format": 1,
  "id": "wf_a_path_that_leads_out",
  "name": "One step that files its answer somewhere else",
  "steps": [
    {
      "kind": "agent",
      "id": "s_only",
      "name": "Only",
      "agent": "01990000-0000-7000-8000-00000000092c",
      "overrides": {},
      "instructions": "only: do the work and say what you found.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_answer_lands_under_the_path_the_person_typed() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let plain = bench.agent(
        "plain",
        &agent_file("01990000-0000-7000-8000-00000000092a", "Plain", ""),
    )?;
    let filer = bench.agent(
        "filer",
        &agent_file("01990000-0000-7000-8000-00000000092b", "Filer", ASKED_FOR),
    )?;
    let workflow = bench.workflow("results-where-asked", WORKFLOW)?;
    the_fixture_can_run(&workflow, &[&plain, &filer])?;

    let seen = Arc::new(Seen::default());
    let report = run_it(&bench, workflow, Arc::clone(&seen))
        .await?
        .map_err(|error| {
            format!("this fixture asks for nothing forbidden, so it has to run: {error}")
        })?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 3],
        "all three steps have to finish for the files below to mean anything; they ended as {:?}",
        report.steps
    );
    assert_eq!(
        seen.labels(),
        vec!["plain", "filed", "own"],
        "all three steps have to reach the agent app, or this criterion is about work that never \
         happened"
    );

    // ── (a) PLIK JEST, POD WSKAZANĄ ŚCIEŻKĄ, Z KATALOGIEM ZAŁOŻONYM PO DRODZE ────────────────
    let filed = bench.project.path().join(ASKED_FOR);
    assert!(
        filed.is_file(),
        "the person typed \"{ASKED_FOR}\" into {FIELD_ON_SCREEN} and nothing came out at {}. \
         A setting a screen accepts, a file records and the run ignores is a control with \
         nothing behind it (invariant 16) — and an empty folder looks exactly like an agent \
         with nothing to say",
        filed.display()
    );

    // ── (b) I NIESIE CAŁĄ ODPOWIEDŹ, NIE JEJ STRESZCZENIE ──────────────────────────────────
    assert_eq!(
        fs::read_to_string(&filed)?,
        SAID,
        "the file under {ASKED_FOR} does not hold what the agent actually said. A one-line \
         summary written there is worse than no file: it looks like the answer and is not, and \
         nobody compares it against a transcript nobody keeps"
    );

    // ── (c) ŚCIEŻKA NADPISANA NA KROKU WYGRYWA, I LICZY SIĘ OD FOLDERU TEGO KROKU ──────────
    // Krok pracuje we własnej kopii plików, więc „względem folderu kroku" i „względem folderu
    // projektu" są tu DWIEMA różnymi odpowiedziami — dla kroku `project` byłyby jedną.
    let mine = files_named(&report.dir, "own.md");
    assert_eq!(
        mine.len(),
        1,
        "the step with its own copy of your files, told to file its answer at \"{OVERRIDDEN}\", \
         left {} such file(s) inside this run: {mine:?}. The path is read from the folder the \
         step works in — the one it was given, not the one the workflow started in",
        mine.len()
    );
    assert!(
        !bench.project.path().join(OVERRIDDEN).exists(),
        "that step's answer landed in the project folder instead of its own copy. A step given \
         its own copy of your files must not write back into yours: that is the whole promise of \
         the setting, and breaking it silently edits the folder a person is working in"
    );

    // ── (d) PUSTE POLE ZNACZY, ŻE NIC SIĘ NIE DZIEJE ──────────────────────────────────────
    // Cały folder projektu, nie jedna ścieżka: implementacja wymyślająca domyślną nazwę pliku
    // przechodzi każde pytanie o konkretną ścieżkę i zasypuje folder człowieka.
    let left = files_in_the_project(bench.project.path())?;
    assert_eq!(
        left,
        BTreeSet::from(["notes.txt".to_owned(), ASKED_FOR.to_owned()]),
        "the run left {left:?} in the project folder. A step whose {FIELD_ON_SCREEN} is empty \
         asked for no file at all, and a run that writes one anyway is a run that edits a \
         person's folder without being told to"
    );

    // ── (e) A PRZEKAZANIE ZOSTAJE PRZEKAZANIEM ────────────────────────────────────────────
    // To jest KOPIA, nie zamiana: indeks następnego kroku stoi na `handoffs/`, więc zapis, który
    // by je zastąpił, uciszyłby cały ruch między krokami i bieg dalej wyglądałby na udany.
    let handed: BTreeSet<String> = handoff::scan_run_dir(&report.dir)?
        .into_iter()
        .map(|one| one.meta.from)
        .collect();
    assert!(
        handed.contains("Filed"),
        "the step that filed its answer under {ASKED_FOR} left nothing in the run's own handover \
         folder: {handed:?}. The next step reads that folder and nothing else, so a file written \
         instead of a handover cuts every step after it off from the work"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_path_that_leads_out_of_the_folder_stops_the_run_before_it_starts()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let absolute = bench.project.path().join("absolute.md");

    for asked in ["../escape.md", &absolute.display().to_string()] {
        let only = bench.agent(
            "only",
            &agent_file("01990000-0000-7000-8000-00000000092c", "Only", asked),
        )?;
        let workflow = bench.workflow("a-path-that-leads-out", ONE_STEP)?;
        the_fixture_can_run(&workflow, &[&only])?;

        let seen = Arc::new(Seen::default());
        let said = match run_it(&bench, workflow, Arc::clone(&seen)).await? {
            Ok(report) => format!(
                "nothing — the run went ahead and ended as {:?}",
                report.steps
            ),
            Err(error) => error.to_string(),
        };

        assert!(
            said.contains(FIELD_ON_SCREEN),
            "a step told to file its answer at \"{asked}\" ran anyway, or was stopped without \
             being told which setting stopped it. That path leaves the folder this step was \
             given, and a refusal that does not name the setting leaves the person hunting \
             through nine rows for it. Loadout said: {said:?}"
        );
        assert!(
            seen.labels().is_empty(),
            "the refusal for \"{asked}\" came after {} agent(s) had already started. A refusal \
             is due at the Start at the latest, never mid-run (invariant 12): a step stopped \
             halfway has already been paid for and has already touched files",
            seen.labels().len()
        );
    }

    assert!(
        !absolute.exists(),
        "a path given in full, from the root of the disk, still produced {}. Anywhere-on-disk is \
         precisely what this refusal exists to prevent",
        absolute.display()
    );
    Ok(())
}

/// Ścieżki plików o tej nazwie, gdziekolwiek pod tym katalogiem.
fn files_named(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|one| one == name) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Wszystko, co leży w folderze projektu poza katalogiem samego Loadouta — po ścieżkach
/// względnych.
///
/// `.loadout/` odpada, bo to jest wyjście builda: katalog biegu, jego kopie plików i indeks.
/// Pytanie brzmi „co ten bieg zostawił w folderze CZŁOWIEKA".
fn files_in_the_project(root: &Path) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_owned()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at)?.flatten() {
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_string_lossy().into_owned();
            if relative == ".loadout" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(relative);
            }
        }
    }
    Ok(out)
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej pliki agentów mają dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy".
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

/// Jeden bieg tej fikstury. Oddaje wynik biegu **nietknięty**, bo połowa tego pliku mierzy odmowę.
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
    // Okno jest tu czarną dziurą: to kryterium sądzi pliki, nie wiersze.
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(outcome)
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Seen(Mutex<Vec<String>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, label: String) {
        self.lock().push(label);
    }

    fn labels(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Etykieta kroku: to, co stoi przed pierwszym dwukropkiem instrukcji — `RunSpec` nazwy kroku
/// nie niesie.
fn label_of(prompt: &str) -> String {
    prompt
        .split_once(':')
        .map_or_else(|| prompt.to_owned(), |(head, _)| head.trim().to_owned())
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: zapisuje, że krok ruszył, i oddaje jedną odpowiedź.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
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
        self.seen.record(label_of(&spec.prompt));
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
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: SAID.to_owned(),
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
        // Żeby „własna kopia twoich plików" miała co kopiować, a folder projektu miał znany
        // stan początkowy — punkt (d) porównuje go w całości.
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
