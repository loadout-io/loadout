//! AC-2 dla T-79: umiejętność, której krok nie może dostać, zatrzymuje bieg **przed pierwszym
//! procesem** — zdaniem, które nazywa i umiejętność, i krok.
//!
//! Alternatywa dla odmowy jest jedna i jest najdroższą wersją tej wady: przyciąć listę i jechać
//! dalej. Człowiek zaznacza pięć umiejętności, agent dostaje trzy, nic nie pada i nikt się o tym
//! nie dowiaduje — bo „agent nie zna tej umiejętności" jest z zewnątrz nieodróżnialne od „model
//! nie uznał, że warto po nią sięgnąć". Niezmiennik 12 mówi, kiedy ta odmowa ma paść:
//! najpóźniej przy Starcie, nigdy w trakcie biegu.
//!
//! **Słabą wersją tego kryterium jest `assert!(result.is_err())`.** Przechodzi dla implementacji,
//! która sprawdza umiejętności dopiero w kroku — czyli dla tej, która zakłada katalog biegu,
//! odpala pierwszego agenta, płaci za jego turę i odmawia drugiemu. Rozróżnia to **licznik
//! uruchomień dublera równy zeru**: brak wyjścia niczego by nie rozróżnił, bo dubler i tak nic
//! nie pisze.
//!
//! **TRZECIĄ SŁABĄ WERSJĄ JEST SĄDZENIE SAMEGO WALIDATORA.** „Nie da się przeczytać" i „nie
//! przechodzi walidatora" to w `StepSkills::for_the_step` dwie osobne gałęzie — dwa pytania do
//! dysku, jedno po drugim — a przypadek z treścią nie do przyjęcia dotyka wyłącznie drugiej.
//! Pierwszą sądzą dwa kształty pliku, którego `read_to_string` nie zwróci: katalog pod nazwą
//! pliku i bajty, które nie są UTF-8. Zmierzone mutacją: nieudany odczyt potraktowany jak zdrowa
//! umiejętność przechodzi każdy inny przypadek w tym pliku.
//!
//! ZDANIA ODMOWY NIE MA W TYM PLIKU JAKO LITERAŁU i to jest połowa jego wartości. Składamy je
//! z `skills::Missing`, czyli z typu, w którym ono mieszka; przepisane tutaj byłoby drugą kopią,
//! a druga kopia jednego zdania jest zawsze tą nieaktualną (niezmiennik 23). Sprawdzamy
//! `contains`, nie równość: bieg ma prawo dopowiedzieć swoje, nie ma prawa zgubić ani jednej
//! z dwóch nazw.

// Powód przy tej samej linii w `skills_reach_the_step.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
use loadout_lib::skills::{Missing, Why};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "fake";

/// Powód w całości przy tej samej stałej w `skills_reach_the_step.rs`.
const PATIENCE: Duration = Duration::from_secs(20);

/// Umiejętność, która JEST w bibliotece i JEST na agencie.
const ALPHA: &str = "alpha";
/// Umiejętność, która jest w bibliotece i **nie jest** na agencie — krok próbuje ją dobrać.
const BETA: &str = "beta";
/// Umiejętność, której nie ma nigdzie: ani w bibliotece, ani na agencie.
const NOWHERE: &str = "nowhere";
/// Umiejętność, której `SKILL.md` nie przechodzi walidatora.
const BROKEN: &str = "broken";
/// Umiejętność, której `SKILL.md` **stoi na swoim miejscu i nie da się go przeczytać**: katalog
/// w miejscu pliku.
///
/// To jest INNA GAŁĄŹ niż `broken` i dlatego stoi osobno. `StepSkills::for_the_step` pyta dysk
/// dwa razy — najpierw `read_to_string`, potem walidator — i tylko druga z tych odpowiedzi jest
/// zmierzona przez `broken`. Kryterium, które sądzi wyłącznie treść nie do przyjęcia, przechodzi
/// dla implementacji, w której nieudany odczyt jest po cichu pustym plikiem albo `?` wyniesionym
/// wyżej jako błąd wejścia-wyjścia: pierwsze daje agenta bez umiejętności, o której człowiek myśli,
/// że ją ma, drugie daje zdanie o systemie plików zamiast zdania o umiejętności i kroku.
const UNREADABLE: &str = "unreadable";
/// Umiejętność, której `SKILL.md` jest plikiem — tyle że nie tekstem.
///
/// Drugi kształt tej samej gałęzi, bo `read_to_string` pada z dwóch różnych powodów i oba są
/// realne: katalog w miejscu pliku (narzędzie, które rozpakowało archiwum obok) i bajty, które nie
/// są UTF-8 (plik binarny zapisany pod tą nazwą). Deterministyczne na każdej maszynie — inaczej niż
/// odebranie praw do odczytu, które dla użytkownika `root` nic nie znaczy.
const NOT_TEXT: &str = "not-text";

fn skill_file(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says in a line what it is for.\n---\n\n\
         Answer with a single sentence.\n"
    )
}

/// `SKILL.md` bez pola `name` — jedna z ośmiu przyczyn, dla których walidator referencyjny
/// odmawia (`skills::place::validate_strict`, „Missing required field in frontmatter: name").
///
/// Wybrane świadomie: plik JEST, da się go przeczytać, i mimo to nie jest umiejętnością. Wersja
/// „katalog bez pliku" byłaby nie do odróżnienia od nazwy spoza biblioteki.
const BROKEN_FILE: &str = "---\ndescription: Has no name, so it is not a skill.\n---\n\nNothing.\n";

/// Agent z jedną umiejętnością. Każdy przypadek niżej podmienia jego listę albo listę kroku.
fn agent_file(skills: &str) -> String {
    format!(
        "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d2
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
skills: {skills}
connections: []
---
Do the work.
"
    )
}

/// Jeden krok, jedna nazwa kroku, jedno miejsce, w którym `overrides` bywa różne.
fn workflow_file(overrides: &str) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_missing_skill",
  "name": "One step that cannot have it",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_only",
      "name": "{STEP}",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {overrides},
      "instructions": "do the work",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

/// Nazwa kroku, czyli to, czego człowiek szuka na płótnie. Odmowa bez niej zamienia jedno
/// odznaczenie w przeszukiwanie workflow.
const STEP: &str = "Only step";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_name_the_library_never_heard_of_stops_the_run() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent(&agent_file(&format!("[{ALPHA}, {NOWHERE}]")))?;
    bench.skill(ALPHA, &skill_file(ALPHA))?;
    let workflow = bench.workflow(&workflow_file("{}"))?;

    let (refusal, started) = one_run(&bench, workflow).await?;

    let expected = Missing {
        step: STEP.to_owned(),
        skill: NOWHERE.to_owned(),
        why: Why::NotInTheLibrary,
    }
    .to_string();
    assert!(
        refusal
            .as_ref()
            .is_some_and(|said| said.contains(&expected)),
        "the agent asks for a skill that is not saved anywhere, and the run had to stop and say \
         so in one sentence naming both the skill and the step. Expected to find {expected:?}; \
         the run answered {refusal:?}"
    );
    assert_eq!(
        started, 0,
        "the run started {started} agent(s) before refusing. Refusing halfway is the expensive \
         version of this defect: the first agent is paid for, and the human reads a refusal about \
         a run that already spent money (invariant 12 - refuse at Start, never mid-run)"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_name_the_agent_was_never_given_stops_the_run() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent(&agent_file(&format!("[{ALPHA}]")))?;
    bench.skill(ALPHA, &skill_file(ALPHA))?;
    bench.skill(BETA, &skill_file(BETA))?;
    // W bibliotece jest, na agencie nie ma — czyli krok próbuje POSZERZYĆ, a wolno mu tylko
    // zawężać. Cicha alternatywa jest gorsza niż odmowa: krok z uprawnieniami, których nie ma
    // na żadnym ekranie.
    let workflow = bench.workflow(&workflow_file(&format!(r#"{{ "skills": ["{BETA}"] }}"#)))?;

    let (refusal, started) = one_run(&bench, workflow).await?;

    let expected = Missing {
        step: STEP.to_owned(),
        skill: BETA.to_owned(),
        why: Why::NotOnTheAgent,
    }
    .to_string();
    assert!(
        refusal
            .as_ref()
            .is_some_and(|said| said.contains(&expected)),
        "{BETA} is saved in the library and is not on this step's agent, so the step asked for \
         more than its agent has. Expected to find {expected:?}; the run answered {refusal:?}"
    );
    assert_eq!(
        started, 0,
        "the run started {started} agent(s) before refusing; the refusal has to land before the \
         first one is paid for"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_that_is_not_a_skill_stops_the_run_too() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent(&agent_file(&format!("[{BROKEN}]")))?;
    bench.skill(BROKEN, BROKEN_FILE)?;
    let workflow = bench.workflow(&workflow_file("{}"))?;

    // Fikstura ma naprawdę nie przechodzić walidatora — inaczej ten przypadek mówi o czymś
    // innym, niż myśli.
    let doc = loadout_lib::skills::place::read_doc(BROKEN_FILE);
    assert!(
        loadout_lib::skills::place::validate_strict(BROKEN, &doc).is_err(),
        "the fixture stopped being invalid, so this case would be measuring a healthy skill"
    );

    let (refusal, started) = one_run(&bench, workflow).await?;

    let said = refusal.clone().unwrap_or_default();
    assert!(
        said.contains(BROKEN) && said.contains(STEP),
        "a SKILL.md that cannot be read as a skill is the same refusal as a missing one: the \
         agent would answer as though there was nothing to know either way. The sentence has to \
         name the skill ({BROKEN}) and the step ({STEP}); the run answered {refusal:?}"
    );
    assert_eq!(
        started, 0,
        "the run started {started} agent(s) before refusing; the refusal has to land before the \
         first one is paid for"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_skill_md_nobody_can_read_stops_the_run_too() -> Result<(), Box<dyn Error>> {
    for (name, fixture) in [
        (UNREADABLE, Fixture::DirectoryInPlaceOfTheFile),
        (NOT_TEXT, Fixture::BytesThatAreNotText),
    ] {
        let bench = Bench::new()?;
        bench.agent(&agent_file(&format!("[{name}]")))?;
        let dir = bench.unreadable_skill(name, fixture)?;
        let workflow = bench.workflow(&workflow_file("{}"))?;

        // Fikstura ma naprawdę być NIE DO ODCZYTU, a katalog umiejętności ma naprawdę stać na
        // swoim miejscu — inaczej ten przypadek mierzy nazwę spoza biblioteki, czyli to samo,
        // co pierwszy test w tym pliku, i o dwóch różnych naprawach mówi jednym zdaniem.
        assert!(
            fs::symlink_metadata(&dir).is_ok(),
            "{name} has to be saved in the library for this case to be about a file that cannot \
             be read; without the directory it would be the same case as a name nobody saved"
        );
        assert!(
            fs::read_to_string(dir.join("SKILL.md")).is_err(),
            "the fixture for {name} became readable, so this case would be measuring the \
             validator instead of the read that never got to it"
        );

        let (refusal, started) = one_run(&bench, workflow).await?;

        let expected = Missing {
            step: STEP.to_owned(),
            skill: name.to_owned(),
            why: Why::Unusable,
        }
        .to_string();
        assert!(
            refusal
                .as_ref()
                .is_some_and(|said| said.contains(&expected)),
            "a SKILL.md that cannot be read at all is the same refusal as one that is not a \
             skill: from outside, an agent without the skill and an agent whose skill could not \
             be opened answer identically. Expected to find {expected:?}; the run answered \
             {refusal:?}"
        );
        assert_eq!(
            started, 0,
            "the run started {started} agent(s) before refusing; the refusal has to land before \
             the first one is paid for"
        );
    }
    Ok(())
}

/// Dwa sposoby, na które `SKILL.md` istnieje i nie daje się przeczytać.
#[derive(Debug, Clone, Copy)]
enum Fixture {
    /// Katalog pod nazwą pliku.
    DirectoryInPlaceOfTheFile,
    /// Plik, którego bajty nie są tekstem.
    BytesThatAreNotText,
}

/// Jeden bieg. Oddaje zdanie odmowy (albo `None`, kiedy bieg mimo wszystko poszedł) i licznik
/// uruchomień dublera.
async fn one_run(
    bench: &Bench,
    workflow: PathBuf,
) -> Result<(Option<String>, usize), Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let started = Arc::new(AtomicUsize::new(0));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: counting_drivers(Arc::clone(&started)),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
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
    Ok((refusal, started.load(Ordering::SeqCst)))
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn counting_drivers(started: Arc<AtomicUsize>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Counting { started });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, którego jedyną treścią jest licznik uruchomień. Zero jest tu całą asercją.
#[derive(Debug)]
struct Counting {
    started: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentDriver for Counting {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    /// Ten dubler UMIE przyjąć gotowy fragment argv — inaczej krok stanąłby na braku szwu
    /// i licznik pokazywałby zero z powodu, o którym to kryterium nie mówi.
    fn inheriting(&self, _flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            started: Arc::clone(&self.started),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.started.fetch_add(1, Ordering::SeqCst);
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

    fn agent(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("agents").join("hand.md"), text)?;
        Ok(())
    }

    fn skill(&self, name: &str, text: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.home.path().join("skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), text)?;
        Ok(())
    }

    /// Umiejętność zapisana w bibliotece, której `SKILL.md` nie da się przeczytać. Oddaje jej
    /// katalog, bo to on ma stać na dysku, kiedy odczyt pada.
    fn unreadable_skill(&self, name: &str, fixture: Fixture) -> Result<PathBuf, Box<dyn Error>> {
        let dir = self.home.path().join("skills").join(name);
        let file = dir.join("SKILL.md");
        match fixture {
            Fixture::DirectoryInPlaceOfTheFile => fs::create_dir_all(&file)?,
            Fixture::BytesThatAreNotText => {
                fs::create_dir_all(&dir)?;
                // Bajty, których nie da się zdekodować jako UTF-8 — samotny bajt startowy
                // sekwencji czterobajtowej i ciąg dalszy bez początku.
                fs::write(&file, [0xF0_u8, 0x28, 0x8C, 0xBC, 0x80])?;
            }
        }
        Ok(dir)
    }

    fn workflow(&self, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("missing.json");
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
