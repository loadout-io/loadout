//! Lider dostaje katalog pluginu z umiejętnościami swojej definicji — także z tych, które leżą
//! w repozytorium człowieka.
//!
//! # Co było zepsute
//!
//! `commands::chat::spec_for` składa sesję lidera z modelu, polityki, sieci, listy narzędzi
//! i briefu — i nie ma w niej ani pola na umiejętności, ani wywołania `AgentDriver::inheriting`.
//! Lider z niepustym `Agent.skills` dostawał więc **zero**: żadnego `--plugin-dir`, żadnej półki.
//! Człowiek zaznaczał umiejętność w formularzu agenta, ekran ją pokazywał, a rozmowa jej nie
//! miała — czyli dokładnie ta cicha porażka, przed którą stoi cały moduł `skills`: „agent nie zna
//! umiejętności" jest z zewnątrz nieodróżnialne od „model nie uznał, że warto po nią sięgnąć".
//!
//! Drugą połową był zasięg samego rozwiązywania. `StepSkills::for_the_step` pytało WYŁĄCZNIE
//! kanonicznej biblioteki (`~/.loadout/skills/<nazwa>/`), a `commands::skills::list_skills_in`
//! czyta półki vendorów — więc umiejętność napisana we własnym repozytorium była na liście
//! w sekcji Knowledge i **nie dało się jej podać nikomu**.
//!
//! # Dlaczego pytamy sterownika, który NAPRAWDĘ zaczął rozmowę
//!
//! Bo każde opakowanie oddaje **klon**, więc opakowanie założone wcześniej ginie, jeśli
//! późniejsze klonuje sterownik sprzed niego. Kolejność jest zapisana wprost: Connections →
//! dziedziczenie → dowody. Odwrócenie kompiluje się, rozmowa rusza, a znika albo katalog
//! pluginu, albo plik dowodu — i nie widać tego po niczym. Ten sam pomiar i ten sam powód stoi
//! w `the_lead_reaches_the_connections`.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(argv.iter().any(|a| a == "--plugin-dir"))`. Przechodzi dla implementacji, która
//! podaje ścieżkę katalogu, którego nie zbudowała — a plugin bez poziomu `skills/` ładuje się,
//! stoi w zdarzeniu startowym jako pełnoprawny wpis i rejestruje **zero** umiejętności [S1 §2].
//! Dlatego asercje niżej pytają też dysk: `SKILL.md` musi leżeć pod właściwym poziomem i musi
//! nieść bajty z półki tego repozytorium.

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam
// powód, co w `the_lead_reaches_the_connections` i w pozostałych plikach tego celu.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens, ValidatedImages,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy drogę, nie przepustowość.
const LINES: usize = 32;

/// Terminal, w którym stoi rozmowa.
const TERMINAL: &str = "terminal-1";

/// Nazwa umiejętności. Ta sama w definicji lidera i w nazwie katalogu na półce.
const SKILL: &str = "harbor-inventory";

/// Kod rozpoznawczy w CIELE `SKILL.md`, nie w opisie: opis jedzie do kontekstu już przy samej
/// rejestracji, więc odpowiedź, która go cytuje, nie dowodzi, że umiejętność się wykonała.
/// Tutaj służy do czegoś węższego — mówi, z KTÓREJ półki przyjechały bajty.
const FROM_THE_PROJECT: &str = "ORCA-7734";

/// Ten sam plik, ta sama nazwa, inna treść — kopia leżąca w bibliotece Loadouta.
const FROM_THE_LIBRARY: &str = "BEACON-1120";

/// Nazwa katalogu, w którym Claude Code szuka umiejętności tego repozytorium.
const SHELF: &str = ".claude/skills";

/// Jedyny katalog w folderze człowieka, który należy do Loadouta.
const OURS: &str = ".loadout";

fn skill_file(code: &str) -> String {
    format!(
        "---\nname: {SKILL}\ndescription: Reads the harbor inventory.\n---\n\nThe berth code is \
         {code}.\n"
    )
}

#[tokio::test]
async fn a_skill_in_this_repository_reaches_the_lead_as_a_plugin_dir() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    bench.on_the_shelf_of_the_project(FROM_THE_PROJECT)?;
    let after = bench
        .one_sentence("claude", &[SKILL.to_owned()], Seam::Present)
        .await;
    after
        .answered
        .map_err(|said| format!("the sentence to the lead was turned down: {said}"))?;

    let started = after
        .started
        .ok_or("the conversation never reached a driver at all")?;
    let at = started
        .inherited
        .iter()
        .position(|argument| argument == "--plugin-dir")
        .ok_or_else(|| {
            format!(
                "this lead agent was given a skill and its conversation started without a plugin \
                 folder: {:?}. The person ticked it, the screen shows it, and the agent never had \
                 it - which from the outside is the same as an agent that chose not to use it",
                started.inherited
            )
        })?;
    let carried = started.inherited.get(at + 1).ok_or_else(|| {
        format!(
            "--plugin-dir came through with nothing behind it: {:?}. A flag with an empty value \
             swallows the next argument as its own",
            started.inherited
        )
    })?;
    assert_eq!(
        Path::new(carried),
        bench.plugin_dir(),
        "the plugin folder has to stand in the one directory of this person's folder that \
         belongs to Loadout, named after the agent"
    );

    // POZIOM `skills/` JEST OBOWIĄZKOWY i to jest zmierzone [S1 §2]: bez niego plugin ładuje się,
    // stoi w zdarzeniu startowym jako pełnoprawny wpis i rejestruje ZERO umiejętności.
    let landed = bench
        .plugin_dir()
        .join("skills")
        .join(SKILL)
        .join("SKILL.md");
    let text = fs::read_to_string(&landed).map_err(|error| {
        format!(
            "the conversation was handed {} as its plugin folder and no skill lies under it \
             ({error}). A folder handed over empty loads green and teaches the model nothing",
            bench.plugin_dir().display()
        )
    })?;
    assert!(
        text.contains(FROM_THE_PROJECT),
        "the skill that reached the lead is not the one written in this repository: {text:?}"
    );
    assert!(
        bench
            .plugin_dir()
            .join(".claude-plugin")
            .join("plugin.json")
            .is_file(),
        "without a pinned name the prefix falls back to the folder name, and no screen can show \
         the same skill twice the same way"
    );

    // KOLEJNOŚĆ OPAKOWAŃ, ZMIERZONA NA STEROWNIKU, KTÓRY NAPRAWDĘ POSZEDŁ DO ROZMOWY. Każde
    // oddaje klon, więc to jedno pytanie rozstrzyga, czy któreś z nich nie zginęło.
    assert!(
        started.evidence,
        "the driver that started the conversation carries the skills and not its private \
         receipt. Each wrapper hands back a CLONE, so whichever goes on first is lost when the \
         next one clones the driver from before it - and nothing about that is visible"
    );

    Ok(())
}

#[tokio::test]
async fn a_lead_without_skills_starts_exactly_as_it_does_today() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.on_the_shelf_of_the_project(FROM_THE_PROJECT)?;
    let after = bench.one_sentence("claude", &[], Seam::Present).await;
    after
        .answered
        .map_err(|said| format!("the sentence to the lead was turned down: {said}"))?;

    let started = after
        .started
        .ok_or("the conversation never reached a driver at all")?;
    assert!(
        started.inherited.is_empty(),
        "this lead agent was given no skills at all, so it has to start byte for byte the way it \
         started before: {:?}. A flag added to every conversation 'just in case' is a flag with \
         an empty value, and that one swallows the next argument as its own",
        started.inherited
    );
    assert!(
        !bench.plugin_dir().exists(),
        "a lead agent with no skills had a folder written into this person's project. Loadout \
         writes nothing nobody reads, least of all inside somebody else's repository"
    );
    Ok(())
}

#[tokio::test]
async fn the_stream_says_which_folder_the_skill_came_from() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.on_the_shelf_of_the_project(FROM_THE_PROJECT)?;
    bench.in_the_library(FROM_THE_LIBRARY)?;
    let after = bench
        .one_sentence("claude", &[SKILL.to_owned()], Seam::Present)
        .await;
    after
        .answered
        .map_err(|said| format!("the sentence to the lead was turned down: {said}"))?;

    let shelf = bench.project.path().join(SHELF).join(SKILL);
    let said = after
        .lines
        .iter()
        .position(|line| matches!(line, Line::Note { text, .. } if text.contains(SKILL)))
        .ok_or_else(|| {
            format!(
                "the same skill name lies both in this repository and in this person's library, \
                 and the stream never said which one the lead agent got. A choice nobody saw is \
                 the one that later reads as an agent that 'did not know how': {:?}",
                said_on_screen(&after.lines)
            )
        })?;
    let sentence = said_on_screen(&after.lines).join("\n");
    assert!(
        sentence.contains(&shelf.display().to_string()),
        "the sentence has to name the folder the bytes came from - a person looks for a path, \
         not for a verdict: {sentence:?}"
    );
    assert!(
        sentence.contains(
            &bench
                .library()
                .join("skills")
                .join(SKILL)
                .display()
                .to_string()
        ),
        "the sentence names the winner and stays silent about the copy it passed over, so \
         nothing on the screen says there was a choice at all: {sentence:?}"
    );

    /* PRZED PIERWSZĄ ODPOWIEDZIĄ, nie pod nią. Wiersz z turą człowieka powstaje dopiero wtedy,
     * gdy zdanie naprawdę pojechało (`Chat::say`, ta sama reguła co w biegu), więc stoi za
     * wszystkim, co rozmowa miała do powiedzenia o swojej konfiguracji. Zdanie postawione za nim
     * jest zdaniem w miejscu, w którym człowiek już przestał go szukać. */
    let told = after
        .lines
        .iter()
        .position(|line| matches!(line, Line::Told { .. }))
        .unwrap_or(usize::MAX);
    assert!(
        said < told,
        "the sentence about where the skill came from stands after the turn it describes"
    );
    Ok(())
}

#[tokio::test]
async fn a_lead_on_an_app_that_cannot_take_skills_is_told_so() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.on_the_shelf_of_the_project(FROM_THE_PROJECT)?;
    let after = bench
        .one_sentence("codex", &[SKILL.to_owned()], Seam::Missing)
        .await;
    after
        .answered
        .map_err(|said| format!("the sentence to the lead was turned down: {said}"))?;

    let sentence = said_on_screen(&after.lines).join("\n");
    assert!(
        sentence.contains("Codex"),
        "this agent app has no way to be handed a skill, and the lead answered as though it had \
         one. The sentence has to name the app, because that is what tells the person whether to \
         untick the skill or to switch the app: {sentence:?}"
    );
    assert!(
        !bench.plugin_dir().exists(),
        "a plugin folder was written for an app that has no way to read it, and it was written \
         inside this person's project"
    );
    Ok(())
}

/// Wszystko, co lider powiedział człowiekowi, zanim ruszyła tura.
fn said_on_screen(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| match line {
            Line::Note { text, .. } | Line::Problem { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Czy ten dubler ma szew dziedziczenia. Dwa vendory, dwie odpowiedzi — i to jest jedyna
/// różnica, o którą chodzi w kryterium o programie, który umiejętności przyjąć nie umie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seam {
    Present,
    Missing,
}

/// Co dubler zapamiętał o sterowniku, który NAPRAWDĘ zaczął rozmowę.
#[derive(Debug, Clone, Default)]
struct Started {
    /// Fragment argv przyniesiony przez dziedziczenie.
    inherited: Vec<String>,
    /// Czy ten sam sterownik niósł też prywatny receipt tej rozmowy.
    evidence: bool,
}

#[derive(Debug, Default)]
struct Watch(Mutex<Option<Started>>);

fn lock<T>(what: &Mutex<T>) -> MutexGuard<'_, T> {
    what.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Clone)]
struct Fake {
    watch: Arc<Watch>,
    vendor: &'static str,
    seam: Seam,
    inherited: Vec<String>,
    evidence: bool,
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

    fn configured(&self, _configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        if self.seam == Seam::Missing {
            return None;
        }
        Some(Arc::new(Self {
            inherited: flags.to_vec(),
            ..self.clone()
        }))
    }

    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            evidence: true,
            ..self.clone()
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // TO JEST PUNKT POMIARU: sterownik, który dojechał tutaj, jest tym, który naprawdę
        // rozmawia. O opakowania pytamy jego, a nie tego, który wyszedł z fabryki.
        *lock(&self.watch.0) = Some(Started {
            inherited: self.inherited.clone(),
            evidence: self.evidence,
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

    async fn start_conversation(
        &self,
        spec: RunSpec,
        _images: ValidatedImages,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.start(spec, tx).await
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
            text: "here is what I would do".to_owned(),
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

/// Co zostało po jednym zdaniu do lidera.
struct Afterwards {
    /// Co granica odpowiedziała człowiekowi.
    answered: Result<(), String>,
    /// Sterownik, który naprawdę zaczął rozmowę — albo `None`, kiedy żaden nie ruszył.
    started: Option<Started>,
    /// Wiersze, które trafiły na ekran, w kolejności wysłania.
    lines: Vec<Line>,
}

struct Bench {
    home: TempDir,
    project: TempDir,
}

/// Identyfikator lidera. Stała, bo jest zarazem nazwą katalogu pluginu.
const WHO: u128 = 20_260_902;

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(OURS))?;
        fs::create_dir_all(home.path().join(OURS))?;
        Ok(Self { home, project })
    }

    /// Biblioteka tego człowieka — `~/.loadout`, czyli to, co okno mówi przy montażu ekranu.
    fn library(&self) -> PathBuf {
        self.home.path().join(OURS)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(OURS).join("loadout.db")
    }

    fn folder(&self) -> String {
        self.project.path().to_string_lossy().into_owned()
    }

    /// Gdzie ma stanąć katalog pluginu tego lidera.
    fn plugin_dir(&self) -> PathBuf {
        self.project
            .path()
            .join(OURS)
            .join("skills")
            .join(Uuid::from_u128(WHO).to_string())
    }

    /// Umiejętność napisana w TYM repozytorium — półka, którą czyta Claude Code.
    fn on_the_shelf_of_the_project(&self, code: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.project.path().join(SHELF).join(SKILL);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), skill_file(code))?;
        Ok(())
    }

    /// Ta sama nazwa, druga treść: kanoniczna kopia w bibliotece Loadouta.
    fn in_the_library(&self, code: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.library().join("skills").join(SKILL);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), skill_file(code))?;
        Ok(())
    }

    /// Zapisuje lidera przez produkcyjną drogę i oddaje jego identyfikator.
    fn saved_lead(&self, skills: &[String]) -> Result<String, Box<dyn Error>> {
        let agent = Agent {
            id: Uuid::from_u128(WHO),
            name: "Lead".to_owned(),
            skills: skills.to_vec(),
            ..Agent::example()
        };
        save_agent_inner(&self.library(), &agent, None)?;
        Ok(agent.id.to_string())
    }

    /// Jedno zdanie do lidera i wszystko, co po nim zostało.
    async fn one_sentence(
        &self,
        vendor: &'static str,
        skills: &[String],
        seam: Seam,
    ) -> Afterwards {
        let who = self.saved_lead(skills).expect("the lead has to be saved");
        let folder = self.folder();
        let watch = Arc::new(Watch::default());
        let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
            watch: Arc::clone(&watch),
            vendor,
            seam,
            inherited: Vec::new(),
            evidence: false,
        });
        let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

        let store = Store::open(&self.db()).expect("the index has to open");
        let state = AppState::new(
            self.library(),
            self.project.path().to_path_buf(),
            store,
            drivers,
        );
        let mut watching: LineSource = {
            let (sink, source) = line_channel(LINES);
            state
                .watching_the_lead(TERMINAL, Some(&folder), sink)
                .expect("the window has to be able to watch this folder");
            source
        };

        let answered = state
            .say_to_the_lead(
                TERMINAL,
                Some(&folder),
                Some(&who),
                "what does the harbor hold?",
            )
            .await;

        let mut lines = Vec::new();
        while let Some(line) = watching.try_next() {
            lines.push(line);
        }
        Afterwards {
            answered,
            started: lock(&watch.0).clone(),
            lines,
        }
    }
}
