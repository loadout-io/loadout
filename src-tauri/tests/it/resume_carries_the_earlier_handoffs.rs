//! AC-1 dla T-88: krok wznowiony z historii dostaje w prompcie to, co zostawili przed nim
//! poprzednicy — z TAMTEGO biegu.
//!
//! # Co dziś dostaje wznowiony krok
//!
//! `rerun::onward` ustawia `handoffs_from: Some(<stary bieg>)`, a `commands::run::seed_the_handoffs`
//! kopiuje pliki z `<stary>/handoffs/` do katalogu nowego biegu. I na tym się kończy: indeks
//! promptu składa `Live::handed_before` **wyłącznie** z tego, co kroki TEGO biegu zdążyły oddać,
//! więc skopiowane pliki nie trafiają do żadnego promptu. Do tego wycinek `Part::Onward`
//! zostawia tylko strzałki z obydwoma końcami w środku, więc głowa wycinka nie ma ani jednego
//! poprzednika i indeksu nie dostaje wcale.
//!
//! Skutek jest tym, co widać w biegu właściciela: wznowiony krok dostaje gałąź gita z pracą
//! poprzedniego biegu (`resume_starts_from_the_work_that_was_done.rs`) i **zero** materiału,
//! na którym ta praca stała. Przycisk „Pick up here" obiecuje więc coś, czego bieg nie robi.
//!
//! # SŁABĄ WERSJĄ jest sprawdzenie katalogu nowego biegu
//!
//! Pliki w `handoffs/` leżą tam OD POCZĄTKU — kopiuje je funkcja, która stała w produkcie już
//! wczoraj. Kryterium czytające katalog świeci więc nad defektem, który mierzy. Dlatego niżej
//! wyrocznią jest **zmontowany prompt**, zdjęty ze sterownika, i porównanie CAŁEJ listy wierszy
//! indeksu do listy wypisanej wprost.
//!
//! # I DLATEGO ŚCIEŻKI SĄDZI SIĘ CO DO ZNAKU
//!
//! Skończony bieg jest historią i nie ma prawa się zmienić dlatego, że ktoś go wznowił
//! (niezmiennik 4). Odnośnik do katalogu STAREGO biegu przeszedłby każdą asercję o nazwach
//! plików i o etykietach — a jest dokładnie tą wadą, przed którą broni kopiowanie. Dlatego
//! porównanie jest do ścieżek w katalogu NOWEGO biegu, a osobna asercja mówi, że nazwa starego
//! katalogu nie pada w prompcie ani razu.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest, rerun};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają własne wymagania co do
/// dowodów biegu, a to kryterium sądzi tekst promptu.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(2);

/// Zdanie, którym wiersz indeksu mówi, że plik przyszedł z wcześniejszego biegu.
///
/// Sądzone co do słowa, bo to jest **copy** czytane przez agenta: wiersz bez tego zdania stoi
/// w indeksie obok wierszy z tego biegu i wygląda jak praca, która właśnie powstała. Agent
/// czytający „popraw to" nad materiałem sprzed godziny nie ma jak się dowiedzieć, że tamten
/// bieg już się skończył.
const FROM_THE_RUN_BEFORE: &str = "what an earlier run left here";

/// Początek instrukcji każdego kroku — po nim, i tylko po nim, dubler poznaje, kto pyta.
/// `RunSpec` nie niesie nazwy kroku (niezmiennik 9), a instrukcja jest tym, co ten krok dostał.
const ASKED: [(&str, &str); 3] = [
    ("research:", "Research"),
    ("draft:", "Draft"),
    ("assemble:", "Assemble"),
];

/// Krok, który w PIERWSZYM biegu pada — i po którym wznawiamy. Pada, bo dokładnie tak wygląda
/// bieg, który ktoś wznawia: sześć kroków skończonych i jeden, który się nie udał.
const FAILS_FIRST_TIME: &str = "Assemble";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e1
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

/// `research → draft → assemble`, czyli łańcuch z kontraktu.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą.
const CHAIN: &str = r#"{
  "format": 1,
  "id": "wf_resume_carries_the_earlier_handoffs",
  "name": "Research, draft, assemble",
  "steps": [
    {
      "kind": "agent",
      "id": "s_a",
      "name": "Research",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "research: find out how it works.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_b",
      "name": "Draft",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "draft: write the first pass.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_c",
      "name": "Assemble",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "assemble: put the whole thing together.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    }
  ],
  "links": [
    { "from": "s_a", "to": "s_b" },
    { "from": "s_b", "to": "s_c" }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_picked_up_step_is_handed_what_the_run_before_it_left() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("chain", CHAIN)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };

    // ── PIERWSZY BIEG: dwa kroki oddają wynik, trzeci pada ────────────────────────────────
    let first = one_run(
        &deps,
        &RunRequest {
            workflow,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await?;
    let ran = watch.take();
    assert_eq!(
        (who(&ran), first.steps.clone()),
        (
            vec![
                "Research".to_owned(),
                "Draft".to_owned(),
                "Assemble".to_owned()
            ],
            vec![
                StepState::Succeeded,
                StepState::Succeeded,
                StepState::Failed
            ]
        ),
        "the bench is only a bench if the first run walked the chain and stopped on the last \
         step. Everything below is about picking that run back up, so a first run that ended \
         some other way measures nothing"
    );

    /* Kto zostawił który plik — z front-mattera, nie z numeru w nazwie: numer jest pozycją
     * w kolejności zapisu i zmienia się z kształtem grafu, a pytanie brzmi „czyja praca".
     *
     * TRZY PLIKI, NIE DWA: krok, który padł, też zostawia to, co zdążył powiedzieć
     * (`Live::hand_on_its_last_words`). Jest tu wymieniony po to, żeby porównanie indeksu niżej
     * było ostre — wznowiony krok nie ma dostać SWOJEJ poprzedniej odpowiedzi, tylko materiał,
     * na którym miał pracować. */
    let left = who_left_what(&first.dir)?;
    assert_eq!(
        left.keys().cloned().collect::<Vec<_>>(),
        vec![
            "Assemble".to_owned(),
            "Draft".to_owned(),
            "Research".to_owned()
        ],
        "the first run has to leave a file for each of its three steps. Found: {left:?}"
    );

    // ── WZNOWIENIE OD KROKU, KTÓRY PADŁ ───────────────────────────────────────────────────
    let folder = first
        .dir
        .file_name()
        .and_then(|one| one.to_str())
        .ok_or("the run directory has no name")?;
    let again = rerun::onward(bench.home.path(), bench.project.path(), folder, "s_c", 1)?;
    let second = one_run(&deps, &again.request).await?;
    let picked = watch.take();
    assert_eq!(
        (who(&picked), second.steps.clone()),
        (vec!["Assemble".to_owned()], vec![StepState::Succeeded]),
        "picking up at the last step has to run that step and nothing else, or the prompt below \
         belongs to some other step"
    );

    // ── CAŁY INDEKS, CO DO WIERSZA ────────────────────────────────────────────────────────
    let handed = index_rows(&picked[0].prompt);
    assert_eq!(
        handed,
        vec![
            row("Research", &second.dir, &left["Research"]),
            row("Draft", &second.dir, &left["Draft"]),
        ],
        "the picked-up step was handed nothing the run before it left. Today its prompt has no \
         index at all: the slice keeps only arrows with both ends inside it, so the step at the \
         head of the slice has no step before it, and the files copied into this run's folder \
         are named by nobody. The agent is asked to assemble work it cannot see, and pays a \
         vendor to find it all over again. The prompt was: {:?}",
        picked[0].prompt
    );

    // ── I PRAWO OTWARCIA TYCH PLIKÓW ──────────────────────────────────────────────────────
    // Odnośnik, którego agentowi nie wolno otworzyć, jest kontrolką bez handlera (niezmiennik
    // 16) — i kosztował 9 z 10 minut kroku w biegu `20260819-223942`.
    assert!(
        picked[0].extra_dirs.contains(&second.dir.join("handoffs")),
        "the step was given paths it is not allowed to open. A run that hands an agent a \
         reference and withholds the right to read it spends the step's whole budget on work \
         that was lying ready next to it. Got: {:?}",
        picked[0].extra_dirs
    );

    // ── STARY BIEG JEST NIEZMIENNY ────────────────────────────────────────────────────────
    assert!(
        !picked[0].prompt.contains(&display(&first.dir)),
        "the prompt points back into the finished run's folder. That folder is history and has \
         to look next week exactly as it looks now (invariant 4) — an agent working straight out \
         of it edits a record of something that already happened. The prompt was: {:?}",
        picked[0].prompt
    );
    Ok(())
}

// ── czytanie promptu ───────────────────────────────────────────────────────────────────────

/// Wiersze indeksu z promptu: `(kto zostawił, ścieżka, czym to jest)`, w kolejności wystąpień.
///
/// Tylko wiersze wskazujące plik przekazania: prompt niesie też instrukcję kroku i umowę
/// o odpowiedzi, a te bywają wypunktowane.
fn index_rows(prompt: &str) -> Vec<(String, String, String)> {
    prompt
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter(|line| line.contains("/handoffs/"))
        .filter_map(|line| {
            let (from, rest) = line.split_once(": ")?;
            let (path, what) = rest.rsplit_once(" (")?;
            Some((
                from.to_owned(),
                path.to_owned(),
                what.trim_end_matches(')').to_owned(),
            ))
        })
        .collect()
}

/// Wiersz, którego się spodziewamy: ten plik, w katalogu TEGO biegu, z etykietą wcześniejszego.
fn row(from: &str, run_dir: &Path, file: &str) -> (String, String, String) {
    (
        from.to_owned(),
        display(&run_dir.join("handoffs").join(file)),
        FROM_THE_RUN_BEFORE.to_owned(),
    )
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

/// Nazwa kroku → nazwa pliku, który ten krok zostawił w tym biegu.
///
/// Czytane z front-mattera (`from:`), nie z nazwy pliku: nazwa niesie slug i numer kroku, czyli
/// dwa ustalenia, które kryterium ma prawo przeżyć.
fn who_left_what(run_dir: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(run_dir.join("handoffs")) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let text = fs::read_to_string(entry.path())?;
        if let Some(from) = text
            .lines()
            .find_map(|line| line.strip_prefix("from: "))
            .map(|value| value.trim().trim_matches('"').to_owned())
        {
            out.insert(from, entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(out)
}

/// Kto był o co poproszony, w kolejności startów.
fn who(asked: &[Asked]) -> Vec<String> {
    asked.iter().map(|one| one.who.clone()).collect()
}

// ── bieg ───────────────────────────────────────────────────────────────────────────────────

async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(deps, request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;
    Ok(report)
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Kto został o co poproszony, co dokładnie dostał na wejściu i co wolno mu było otworzyć.
#[derive(Debug, Clone)]
struct Asked {
    who: String,
    prompt: String,
    extra_dirs: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct Watch {
    seen: Mutex<Vec<Asked>>,
    /// Ile razy krok, który pada za pierwszym razem, był o coś proszony.
    ///
    /// Liczone OSOBNO od `seen`, bo tamtą listę zdejmuje się między biegami — a „pierwszy raz"
    /// jest jeden na cały test, i to jest cała fikstura: krok pada, człowiek wznawia, krok się
    /// udaje. Licznik zerowany razem z listą kazałby mu paść drugi raz.
    tries: AtomicUsize,
}

impl Watch {
    /// Zapisuje start i oddaje to, czym ta tura się skończy: tekst i czy się udała.
    fn entered(&self, spec: &RunSpec) -> (String, bool) {
        let who = who_is_asked(&spec.prompt);
        self.lock().push(Asked {
            who: who.clone(),
            prompt: spec.prompt.clone(),
            extra_dirs: spec.extra_dirs.clone(),
        });
        let ok = who != FAILS_FIRST_TIME || self.tries.fetch_add(1, Ordering::SeqCst) > 0;
        let body =
            format!("## Answer\n{who} is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n");
        (body, ok)
    }

    /// Zdejmuje i oddaje wszystko, co widziała. Dwa biegi w jednym teście mają być czytane
    /// osobno, a nie jednym ciągiem.
    fn take(&self) -> Vec<Asked> {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Asked>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Nazwa kafelka po początku jego instrukcji. Instrukcja, której nie ma w tablicy, ląduje pod
/// SWOJĄ treścią — wtedy asercja o tym, kto biegł, pada i pokazuje, czego ławka nie rozpoznała.
fn who_is_asked(prompt: &str) -> String {
    ASKED
        .iter()
        .find(|(opening, _)| prompt.starts_with(opening))
        .map_or_else(|| prompt.to_owned(), |(_, name)| (*name).to_owned())
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
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
        let (said, ok) = self.watch.entered(&spec);
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
            ok,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
    ok: bool,
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
            ok: self.ok,
            reason: if self.ok {
                FinishReason::Completed
            } else {
                FinishReason::Failed("the step could not finish this time".to_owned())
            },
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
