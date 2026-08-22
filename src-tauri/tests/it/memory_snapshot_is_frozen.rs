//! AC-4 dla T-80: bieg pamięta to, co wiedział przy starcie.
//!
//! Blok „co wiadomo" jest dziś liczony RAZ, przy planowaniu, i to jest właściwe zachowanie —
//! ale stoi na tym, że blok jest jeden na bieg. AC-2 każe policzyć trzeci blok **per krok**,
//! z tożsamości agenta, a najprostsza droga do tego prowadzi przez odczyt notatek w chwili
//! startu kroku. Wtedy dwa kroki jednego biegu dostają dwa różne konteksty, jeśli człowiek
//! w międzyczasie dopuścił notatkę albo poprawił zdanie — a różnicy nie widać nigdzie poza
//! rachunkiem za długość. To kryterium pilnuje, żeby zamrożony został ZBIÓR notatek, a nie
//! przypadkiem sam tekst jednego bloku.
//!
//! **Słabą wersją jest test, który tylko czyta prompt drugiego kroku.** Przechodzi dla
//! implementacji, która nie dokleja niczego. Dlatego pierwszy punkt niżej pyta, czy notatka
//! agenta w ogóle dojechała, a dopiero po nim pytamy, KTÓRA jej wersja.
//!
//! **Drugą słabą wersją jest edycja, która nie doszła.** Test, który nadpisuje plik i nie
//! sprawdza, że plik naprawdę się zmienił, dowodzi zamrożenia także wtedy, gdy zapis padł na
//! prawach dostępu. Stąd asercja o zawartości pliku PO biegu — z tym samym znacznikiem, którego
//! w promptach być nie może.
//!
//! **Trzecią jest zrzut, w którym stoi sama lista nazw.** „Co model wtedy wiedział" jest
//! pytaniem o TREŚĆ, więc odwołanie bez odcisku i bez liczby bajtów odpowiada na nie zdaniem
//! „jakaś notatka o tej nazwie" — a notatka o tej nazwie mogła się od tamtej pory zmienić
//! dokładnie tak, jak zmienia się w tym teście. Liczba bajtów w zrzucie jest tu policzona
//! z reguły SPRZED edycji i to ona rozróżnia zrzut zamrożony od zrzutu przepisanego na końcu.
//!
//! Zrzut czytamy z `run.json`, bo to on jest prawdą o biegu; `loadout.db` jest jego indeksem
//! i wolno go skasować (niezmiennik 4).

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` — oba pytania tego kryterium („co dostał krok drugi" i „co stoi w zrzucie")
// mierzą JEDEN bieg, w którym plik notatki zmienia się w trakcie. Rozbicie na dwa `#[test]`
// znaczyłoby dwa biegi albo stan dzielony między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use serde_json::Value as Json;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";
const PATIENCE: Duration = Duration::from_secs(20);

/// Zdanie, które ta notatka niosła, kiedy bieg ruszał.
const BEFORE: &str = "LYNX-AS-THE-RUN-STARTED";
/// Zdanie, które ktoś wpisał do tego samego pliku w trakcie biegu.
const AFTER: &str = "LYNX-EDITED-MID-RUN";
/// Notatka, która pojawiła się w katalogu już po starcie.
const LATER: &str = "LYNX-ARRIVED-MID-RUN";
/// Notatka, której nikt nie ruszał — kontrola dla obu kierunków.
const EVERY: &str = "LYNX-EVERYWHERE";

const FIRST_STEP: &str = "LYNX-STEP-ONE";
const SECOND_STEP: &str = "LYNX-STEP-TWO";

/// Plik notatki agenta. Jego nazwa jest tym, czego szukamy w zrzucie.
const OWNED_NOTE: &str = "the-queue-is-drained-in-one-place";
const SHARED_NOTE: &str = "prompts-travel-on-stdin";
const LATE_NOTE: &str = "somebody-wrote-this-mid-run";

/// Długości reguł w jednostkach. **Różne**, i to jest cała treść tej pary: zrzut, który
/// przepisano po biegu, poda liczbę bajtów zdania, które stoi w pliku TERAZ.
const WORTH_BEFORE: usize = 40;
const WORTH_AFTER: usize = 70;

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000b1
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

/// Dwa kroki, jedna strzałka: drugi rusza po pierwszym, więc edycja z pierwszego zdąży się
/// wydarzyć przed jego turą.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_memory_frozen",
  "name": "One after the other",
  "steps": [
    {
      "kind": "agent",
      "id": "s_first",
      "name": "First",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "LYNX-STEP-ONE go first and change nothing important.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_second",
      "name": "Second",
      "agent": "01990000-0000-7000-8000-0000000000b1",
      "overrides": {},
      "instructions": "LYNX-STEP-TWO go second and say what you were told.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_first", "to": "s_second" }]
}
"#;

fn rule_worth(units: usize, sentinel: &str) -> String {
    let wanted = units * 4;
    let mut rule = format!("{sentinel} a sentence long enough to be worth {units} units ");
    assert!(
        rule.len() < wanted,
        "the sentinel alone is longer than the note is supposed to be"
    );
    while rule.len() < wanted {
        rule.push('x');
    }
    rule
}

fn note_file(scope: &str, agent: Option<&str>, title: &str, rule: &str) -> String {
    let owner = agent.map_or_else(String::new, |name| format!("agent: {name}\n"));
    format!(
        "---\n\
         scope: {scope}\n\
         {owner}\
         kind: rule\n\
         title: {title}\n\
         rule: {rule}\n\
         because: somebody watched this happen twice and wrote it down the second time\n\
         status: in-use\n\
         occurrences: 1\n\
         modified: 2026-08-20T09:00:00Z\n\
         last_used_at: null\n\
         ---\n"
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_note_edited_mid_run_does_not_change_what_the_next_step_is_told()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;
    bench.note(
        SHARED_NOTE,
        &note_file(
            "everywhere",
            None,
            "Prompts travel on stdin",
            &rule_worth(WORTH_BEFORE, EVERY),
        ),
    )?;
    bench.note(
        OWNED_NOTE,
        &note_file(
            "this-agent",
            Some("backend-dev"),
            "The queue is drained in one place",
            &rule_worth(WORTH_BEFORE, BEFORE),
        ),
    )?;

    let workflow = bench.workflow("memory-frozen", WORKFLOW)?;
    let store = Store::open(&bench.db())?;
    let seen = Arc::new(Seen::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen), bench.notes()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 2],
        "both steps have to finish, or everything below is true of a step that never ran. They \
         ended as {:?}",
        report.steps
    );

    // Najpierw: edycja NAPRAWDĘ doszła. Bez tej linii cały ten plik dowodzi zamrożenia także
    // wtedy, gdy zapis w trakcie biegu po prostu się nie udał.
    let on_disk = fs::read_to_string(bench.notes().join(format!("{OWNED_NOTE}.md")))?;
    assert!(
        on_disk.contains(AFTER),
        "the note file was supposed to be rewritten while the run was going and it still holds \
         its old sentence. Nothing below proves anything until this line is true. The file \
         reads:\n{on_disk}"
    );
    assert!(
        bench.notes().join(format!("{LATE_NOTE}.md")).is_file(),
        "the note that was supposed to appear mid-run is not on disk either"
    );

    let looked = seen.snapshot();
    let first = looked
        .get(FIRST_STEP)
        .ok_or("the first step never reached the driver")?;
    let second = looked
        .get(SECOND_STEP)
        .ok_or("the second step never reached the driver")?;

    // (a) NOTATKA AGENTA W OGÓLE DOJECHAŁA. Ten punkt stoi pierwszy, bo bez niego całe „nie
    //     zmieniło się" jest prawdą o pustym miejscu.
    assert!(
        first.contains(BEFORE),
        "the first step was never told the note its own agent holds, so this criterion has \
         nothing to freeze. The prompt reads:\n{first}"
    );

    // (b) DRUGI KROK DOSTAJE TO, CO BIEG WIEDZIAŁ NA STARCIE. Implementacja, która czyta
    //     katalog notatek w chwili startu kroku, daje tu zdanie sprzed dwóch sekund, a bieg
    //     przestaje być jedną odpowiedzią na pytanie „co model o tym wiedział".
    assert!(
        second.contains(BEFORE),
        "the second step was told a different version of the note than the first one. The set \
         of notes is frozen before the first process starts: two steps of one run that read the \
         directory at two different moments answer \"what did the model know\" twice, and the \
         difference shows up nowhere except in the bill for length. The prompt reads:\n{second}"
    );
    assert!(
        !second.contains(AFTER),
        "the sentence somebody typed into the note WHILE the run was going reached a step of \
         that same run. The prompt reads:\n{second}"
    );
    assert!(
        !second.contains(LATER),
        "a note that did not exist when this run started reached its second step. A run that \
         picks up notes as they appear cannot be explained afterwards: the same workflow, the \
         same files, and a different prompt depending on what somebody approved in between. \
         The prompt reads:\n{second}"
    );
    assert!(
        second.contains(EVERY),
        "and the scope nobody touched is still there — freezing the set is not the same as \
         dropping it. The prompt reads:\n{second}"
    );

    // (c) ZRZUT MÓWI, CO MODEL WTEDY WIEDZIAŁ. Odwołanie, odcisk i liczba bajtów — tyle, żeby
    //     dało się później odpowiedzieć na to pytanie, i ani jednego bajtu treści notatki
    //     (`run.json` nie jest kopią pamięci, tylko rachunkiem z niej).
    let run_file: Json = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let recorded = run_file
        .get("memory")
        .and_then(Json::as_array)
        .ok_or_else(|| {
            format!(
                "run.json carries no record of what this run knew. The files are the truth about \
                 a run (invariant 4), so a note that reached a prompt and left no trace in \
                 run.json is a fact about the run that nobody can recover afterwards. The file \
                 holds these keys: {:?}",
                run_file
                    .as_object()
                    .map(|map| map.keys().collect::<Vec<_>>())
            )
        })?;

    let owned = entry_for(recorded, OWNED_NOTE).ok_or_else(|| {
        format!(
            "run.json records nothing about {OWNED_NOTE}, which reached both prompts: {recorded:?}"
        )
    })?;
    let reference = owned
        .get("reference")
        .and_then(Json::as_str)
        .ok_or("the record of that note has no reference to the note itself")?;
    assert!(
        !reference.starts_with('/'),
        "the reference is an absolute path on this machine ({reference}). What belongs in the \
         record is which note it was, not where this laptop keeps it"
    );
    assert!(
        owned
            .get("hash")
            .and_then(Json::as_str)
            .is_some_and(|hash| !hash.is_empty()),
        "the record has no fingerprint of the note. A name on its own answers \"what did the \
         model know\" with \"some note called that\" — and that note has changed since, exactly \
         as it changed during this run. The record reads: {owned:?}"
    );
    assert_eq!(
        owned.get("bytes").and_then(Json::as_u64),
        Some((WORTH_BEFORE * 4) as u64),
        "the record has to count the sentence this run really carried. {} bytes is the rule as \
         it stood when the run started; {} is the one somebody typed in while it was running. A \
         record written at the end, from the files as they are then, gives the second number and \
         looks exactly like a record written at the start. The record reads: {owned:?}",
        WORTH_BEFORE * 4,
        WORTH_AFTER * 4
    );
    assert!(
        entry_for(recorded, SHARED_NOTE).is_some(),
        "and the note every project holds is recorded too — the record covers what the run \
         knew, not one scope of it. The record reads: {recorded:?}"
    );
    assert!(
        entry_for(recorded, LATE_NOTE).is_none(),
        "the note that appeared after the run started stands in the record of what this run \
         knew. It reached no prompt and it must not be listed as though it had. The record \
         reads: {recorded:?}"
    );

    Ok(())
}

/// Wpis zrzutu, którego odwołanie wskazuje ten plik notatki.
fn entry_for<'a>(recorded: &'a [Json], note: &str) -> Option<&'a Json> {
    recorded.iter().find(|entry| {
        entry
            .get("reference")
            .and_then(Json::as_str)
            .is_some_and(|reference| reference.contains(note))
    })
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, String>>);

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, step: &str, prompt: String) {
        self.lock().insert(step.to_owned(), prompt);
    }

    fn snapshot(&self) -> BTreeMap<String, String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>, notes: PathBuf) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen, notes });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który przy pierwszym kroku robi to, co człowiek robi w trakcie biegu: poprawia
/// jedno zdanie w notatce i dopisuje drugą.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
    notes: PathBuf,
}

impl Fake {
    fn rewrite_the_memory(&self) {
        fs::write(
            self.notes.join(format!("{OWNED_NOTE}.md")),
            note_file(
                "this-agent",
                Some("backend-dev"),
                "The queue is drained in one place",
                &rule_worth(WORTH_AFTER, AFTER),
            ),
        )
        .expect("the fixture could not rewrite the note while the run was going");
        fs::write(
            self.notes.join(format!("{LATE_NOTE}.md")),
            note_file(
                "this-agent",
                Some("backend-dev"),
                "Somebody wrote this mid run",
                &rule_worth(WORTH_BEFORE, LATER),
            ),
        )
        .expect("the fixture could not add a note while the run was going");
    }
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
        let step = [FIRST_STEP, SECOND_STEP]
            .into_iter()
            .find(|marker| spec.prompt.contains(marker))
            .map_or_else(|| spec.prompt.clone(), ToOwned::to_owned);
        self.seen.record(&step, spec.prompt.clone());
        if step == FIRST_STEP {
            self.rewrite_the_memory();
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
        fs::create_dir_all(home.path().join("memory").join("notes"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
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

    /// `~/.loadout/memory/notes` — ten sam korzeń, który rozwiązuje `commands::memory`.
    fn notes(&self) -> PathBuf {
        self.home.path().join("memory").join("notes")
    }

    fn note(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.notes().join(format!("{slug}.md")), text)?;
        Ok(())
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path: PathBuf = self
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
