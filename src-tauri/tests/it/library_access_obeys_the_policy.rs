//! AC-2 dla T-70: polityka zostaje sufitem także w bibliotece.
//!
//! # Po co to istnieje
//!
//! Biblioteka wchodzi liderowi w `extra_dirs` po to, żeby „jakie mam workflow?" dało się
//! odpowiedzieć z plików, a nie z pamięci rozmowy. To jest zdanie o tym, **GDZIE** lider patrzy,
//! i o niczym więcej: co mu wolno z tym zrobić, mówi dalej jego dial (niezmiennik 23). Lider
//! `look only` bibliotekę CZYTA — na tym polega cała wartość tej zmiany — a pisze dopiero ten,
//! któremu człowiek dał `ask first` albo `work freely`.
//!
//! # Cicha porażka, przed którą stoi ten plik
//!
//! Podniesienie liderowi uprawnień „żeby mogło działać". Katalog dosypany razem z `Edit`, `Write`
//! i `Bash` wygląda z zewnątrz dokładnie jak katalog dosypany bez nich: rozmowa działa, odpowiada
//! i nikt nie zobaczy różnicy, dopóki lider ustawiony na „read only" nie nadpisze definicji
//! agenta. Człowiek zobaczy wtedy lidera, który zapisał plik, a nie awarię.
//!
//! # Słaba wersja tego kryterium
//!
//! Sprawdzenie samej obecności katalogów. Przechodzi dokładnie dla tej implementacji. Rozróżniają
//! to dwie rzeczy naraz: (a) czytelna połowa dialu ma dojechać do vendora BEZ ani jednego
//! narzędzia zapisu, a (d) ta sama definicja z drugiej pozycji dialu ma dać **inny** zestaw flag —
//! bo test, w którym oba końce dialu dają jedno argv, mierzy implementację, która dialu nie czyta.
//!
//! **Wyrocznią jest zbudowana komenda, nie źródło sterownika** (niezmiennik 20). Selftest w repo
//! źródłowym asertował obecność flagi w skrypcie, przechodził **na komentarzu**, a żywa flaga
//! brzmiała inaczej [raport 06 §2]. Dlatego napisy `dontAsk` i `bypassPermissions` stoją niżej
//! wypisane dosłownie, a nie zaimportowane z `permission_flags`: test czytający tę samą stałą, co
//! kod, zawsze się z nim zgadza i nie mierzy niczego.

// `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii zamieniłby
// nazwany komunikat asercji w bezimienne `Err`. Ten sam idiom i ten sam powód, co
// w `lead_comes_from_the_agent` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{Lead, Threads};
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use loadout_lib::workflow::file::load;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile miejsca w strumieniu linii rozmowy. Z zapasem — mierzymy argv, nie przepustowość.
const LINES: usize = 32;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile trwa jedna tura dublera.
const TURN: Duration = Duration::from_millis(20);

/// Identyfikator zapisanego lidera. Ten sam po obu stronach dialu, bo (d) pyta o TĘ SAMĄ
/// definicję: gdyby różniły się czymkolwiek poza dialem, dwa różne argv nie dowodziłyby niczego.
const LEAD_ID: &str = "01990000-0000-7000-8000-0000000000d1";

/// Instrukcje lidera — też te same po obu stronach, i z tego samego powodu.
const INSTRUCTIONS: &str = "You look after this folder and you answer in short sentences.";

/// Narzędzia, których lider `look only` nie ma prawa dostać — ani w zestawie, ani w liście
/// auto-zatwierdzania.
///
/// `Bash` gołe, bo `--tools` zna wyłącznie nazwy: składnia zakresowa (`Bash(git *)`) należy do
/// `--allowedTools`, więc sprawdzenie po samej nazwie łapie obie flagi jednym napisem.
const WRITES: [&str; 3] = ["Edit", "Write", "Bash"];

/// Tryb uprawnień, którego wymaga „read only" — **wypisany tutaj**, nie zaimportowany.
const READ_ONLY_MODE: &str = "dontAsk";

/// To samo dla „no limits".
const NO_LIMITS_MODE: &str = "bypassPermissions";

/// Jednokrokowy workflow: istnieje wyłącznie po to, żeby biblioteka miała co zapisać i oddała
/// ścieżkę, pod którą trzyma workflow. Pisany ręcznie — fikstura zbudowana naszym serializatorem
/// definiowałaby kształt, zamiast go sprawdzać [04 §6.4].
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_one_step",
  "name": "One step",
  "steps": [
    {
      "kind": "agent",
      "id": "s_only",
      "name": "Only",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "do the one thing",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

// ── (a) CZYTA I NIE PISZE ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn look_only_reaches_the_library_without_a_single_writing_tool() -> Result<(), Box<dyn Error>>
{
    let bench = TempDir::new()?;
    let reached = reached(&bench.path().join("look-only"), FileAccess::LookOnly).await?;

    // Katalogi są tam, gdzie mają być — czyli wśród tych, które lider ma prawo otworzyć.
    for folder in reached.library() {
        assert!(
            reached.added_dirs().iter().any(|dir| dir == &folder),
            "a lead set to \"read only\" did not reach {}. Reading the library is the whole value \
             of this change: without it \"what workflows do I have?\" is answered out of the \
             conversation, not out of the files. It was handed {:?}",
            folder,
            reached.added_dirs()
        );
    }

    // A dial został tam, gdzie go zostawił człowiek.
    let available = reached.value_of("--tools").ok_or(
        "the turn reached the vendor with no --tools at all, so nothing says what this lead has \
         in reach",
    )?;
    let approved = reached
        .value_of("--allowedTools")
        .ok_or("\"read only\" has to carry an auto-approval list; it carried none")?;
    for tool in WRITES {
        assert!(
            !available.contains(tool),
            "a lead set to \"read only\" was handed {tool} in --tools. --tools is the hard \
             availability list, so this lead can change the very files it was only supposed to \
             read — and a lead that writes when the person said \"look only\" does not look like \
             a failure, it looks like a lead that saved a file. It carried {available}"
        );
        assert!(
            !approved.contains(tool),
            "a lead set to \"read only\" was handed {tool} in --allowedTools. In a conversation \
             nobody is watching the permission prompt, so \"it will ask\" does not mean \"it will \
             not do it\". It carried {approved}"
        );
    }

    // (c) TRYB UPRAWNIEŃ WYNIKA DALEJ WYŁĄCZNIE Z POLITYKI. Dosypanie katalogu nie ma prawa
    // przestawić dialu: sieć i zapis dałyby się wtedy „kupić" trybem, który zatwierdza wszystko.
    assert_eq!(
        reached.value_of("--permission-mode").as_deref(),
        Some(READ_ONLY_MODE),
        "\"read only\" has to reach the vendor as {READ_ONLY_MODE}, exactly as it did before the \
         library was ever added. argv was {:?}",
        reached.args
    );
    Ok(())
}

// ── (b) TEN, KTÓREMU WOLNO, MA JEDNO I DRUGIE ──────────────────────────────────────────────

#[tokio::test]
async fn work_freely_reaches_the_library_and_may_change_it() -> Result<(), Box<dyn Error>> {
    let bench = TempDir::new()?;
    let reached = reached(&bench.path().join("work-freely"), FileAccess::WorkFreely).await?;

    for folder in reached.library() {
        assert!(
            reached.added_dirs().iter().any(|dir| dir == &folder),
            "a lead set to \"no limits\" did not reach {}, so \"fix that step in my workflow\" \
             stays a promise it cannot keep. It was handed {:?}",
            folder,
            reached.added_dirs()
        );
    }

    let available = reached.value_of("--tools").ok_or(
        "the turn reached the vendor with no --tools at all, so nothing says what this lead has \
         in reach",
    )?;
    for tool in WRITES {
        assert!(
            available.contains(tool),
            "a lead set to \"no limits\" reached the vendor without {tool}. Reaching the library \
             it may not change is the same dead end as not reaching it: the person asked for a \
             lead that prepares files. It carried {available}"
        );
    }

    // (c) po drugiej stronie dialu, tą samą drogą.
    assert_eq!(
        reached.value_of("--permission-mode").as_deref(),
        Some(NO_LIMITS_MODE),
        "\"no limits\" has to reach the vendor as {NO_LIMITS_MODE}, exactly as it did before the \
         library was ever added. argv was {:?}",
        reached.args
    );
    Ok(())
}

// ── (d) KONTROLA: TA SAMA DEFINICJA, DWA RÓŻNE ZESTAWY FLAG ────────────────────────────────

#[tokio::test]
async fn one_definition_on_two_dial_positions_reaches_two_different_commands()
-> Result<(), Box<dyn Error>> {
    // Bez tego przypadku wszystko wyżej przechodzi dla implementacji, która dial ignoruje
    // i wypisuje jedno argv wszystkim: obie asercje o narzędziach byłyby wtedy zdaniami o dwóch
    // różnych liderach, a nie o dwóch różnych politykach.
    let bench = TempDir::new()?;
    let look = reached(&bench.path().join("look-only"), FileAccess::LookOnly).await?;
    let free = reached(&bench.path().join("work-freely"), FileAccess::WorkFreely).await?;

    assert_ne!(
        look.value_of("--tools"),
        free.value_of("--tools"),
        "one definition on two dial positions reached the vendor with the same tool set, so this \
         file measures an implementation that never reads the dial. \"read only\" carried {:?}, \
         \"no limits\" carried {:?}",
        look.value_of("--tools"),
        free.value_of("--tools")
    );
    assert_ne!(
        look.value_of("--permission-mode"),
        free.value_of("--permission-mode"),
        "one definition on two dial positions reached the vendor in the same permission mode. \
         That is the dial gone: whatever the person picks, the lead is handed the same ceiling"
    );

    // Ta sama definicja NAPRAWDĘ jest ta sama — inaczej dwa różne argv nie mówiłyby o dialu.
    assert_eq!(
        look.value_of("--append-system-prompt").is_some(),
        free.value_of("--append-system-prompt").is_some(),
        "the two sides of this control have to differ in the dial and in nothing else"
    );

    // I obie strony naprawdę doszły do biblioteki: kontrola, w której jedna z nich nie dostała
    // nic, byłaby zielona także dla implementacji dosypującej katalogi wyłącznie temu, kto pisze.
    for side in [&look, &free] {
        assert_eq!(
            side.added_dirs().len(),
            side.library().len(),
            "one side of the control reached {} folder(s) instead of {}, so \"two different flag \
             sets\" would be true for a lead that reaches nothing at all",
            side.added_dirs().len(),
            side.library().len()
        );
    }
    Ok(())
}

// ── Droga: zapisana definicja → wskazany lider → jedno zdanie → gotowa komenda ─────────────

/// Co dojechało do vendora dla jednej pozycji dialu.
#[derive(Debug)]
struct Reached {
    /// Argumenty **zbudowanej komendy**, czyli to, co zobaczyłby `ps`.
    args: Vec<String>,
    /// Katalog agentów tej biblioteki, wzięty z jej własnego zapisu.
    agents_dir: PathBuf,
    /// Katalog workflow tej biblioteki, wzięty z jej własnego zapisu.
    workflows_dir: PathBuf,
}

impl Reached {
    /// Obie połowy biblioteki, jako napisy porównywalne z argv.
    fn library(&self) -> Vec<String> {
        [&self.agents_dir, &self.workflows_dir]
            .into_iter()
            .map(|dir| dir.to_string_lossy().into_owned())
            .collect()
    }

    /// Wartość stojąca zaraz za flagą; `None`, kiedy flagi nie ma.
    fn value_of(&self, flag: &str) -> Option<String> {
        let at = self.args.iter().position(|arg| arg == flag)?;
        self.args.get(at + 1).cloned()
    }

    /// Wszystkie katalogi doklejone do tej tury — flaga powtarzalna, więc pytamy o listę.
    fn added_dirs(&self) -> Vec<String> {
        self.args
            .iter()
            .enumerate()
            .filter(|(_, arg)| *arg == "--add-dir")
            .filter_map(|(at, _)| self.args.get(at + 1).cloned())
            .collect()
    }
}

/// Cała droga dla jednej pozycji dialu: definicja zapisana w bibliotece, wskazana jako lider,
/// jedno zdanie, gotowa komenda tury.
///
/// Osobny korzeń biblioteki na pozycję, bo definicje mają się różnić **wyłącznie** dialem — a dwa
/// pliki o tej samej nazwie nie zmieszczą się w jednym katalogu.
async fn reached(root: &Path, access: FileAccess) -> Result<Reached, Box<dyn Error>> {
    let scope = TempDir::new()?;
    let lead_agent = definition(access)?;
    let agents_dir = folder_of(&save_agent_inner(root, &lead_agent, None)?.path)?;
    let workflows_dir = folder_of(&saved_workflow(root, scope.path())?)?;

    // KONTROLA PRZECIW PUSTEMU PRZEJŚCIU, przy każdym wywołaniu: ścieżka, której nie ma w drzewie
    // fikstury, przechodzi każdą asercję o nieobecności i żadnej o obecności.
    assert!(
        agents_dir.is_dir() && workflows_dir.is_dir(),
        "the fixture library is missing one of its two folders ({} / {}), so this case would \
         compare argv against a path nobody wrote",
        agents_dir.display(),
        workflows_dir.display()
    );
    assert!(
        !scope.path().starts_with(root),
        "the fixture's working folder lies inside the library, so the lead would reach both \
         folders with nothing added at all"
    );

    let lead = Lead::pointed_at(root, Some(&lead_agent.id.to_string()))
        .map_err(|refusal| refusal.to_string())
        .expect("the agent was just saved, so the pointed-at lead has to resolve");

    let spec = one_sentence(root, &lead, scope.path()).await?;
    Ok(Reached {
        args: argv(&spec),
        agents_dir,
        workflows_dir,
    })
}

/// Definicja agenta, jaką człowiek zapisał w bibliotece — jedna na cały plik, z jednym polem
/// zmiennym.
///
/// `Agent::example()` jako baza, bo „jak wygląda zapisany agent" ma w tym repo jedną odpowiedź
/// (`library::agents`), a ręcznie wypisane piętnaście pól byłoby drugą.
fn definition(access: FileAccess) -> Result<Agent, Box<dyn Error>> {
    Ok(Agent {
        id: Uuid::parse_str(LEAD_ID)?,
        name: "Lead".to_owned(),
        runs_with: Vendor::ClaudeCode,
        file_access: access,
        instructions: INSTRUCTIONS.to_owned(),
        reaches_the_web: false,
        write_results_to: String::new(),
        ..Agent::example()
    })
}

/// Workflow zapisany **produkcyjną drogą**, żeby oddał ścieżkę, pod którą biblioteka go trzyma.
///
/// Napis `"workflows"` wpisany w ten plik zgadzałby się z produkcją dokładnie do dnia, w którym
/// produkcja go zmieni — a wtedy asercja porównywałaby ścieżkę, której nikt nie czyta, ze
/// ścieżką, której nikt nie pisze.
fn saved_workflow(library: &Path, scratch: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let drafted = scratch.join("drafted-by-hand.json");
    fs::write(&drafted, WORKFLOW)?;
    Ok(save_workflow_inner(library, "one-step.json", &load(&drafted)?, None)?.path)
}

/// Katalog, w którym leży ten plik.
fn folder_of(file: &Path) -> Result<PathBuf, Box<dyn Error>> {
    Ok(file
        .parent()
        .ok_or_else(|| format!("{} has no folder above it", file.display()))?
        .to_path_buf())
}

/// Jedno zdanie powiedziane wskazanemu liderowi → specyfikacja jego sesji.
///
/// Strumień zakładamy tak, jak zakłada go okno (`open_chat` → `lines_go_to`), bo wątek bez kanału
/// jest wątkiem, którego wierszy nikt nie odbiera — a to jest inny stan niż ten, o który pytamy.
async fn one_sentence(library: &Path, lead: &Lead, cwd: &Path) -> Result<RunSpec, Box<dyn Error>> {
    let (drivers, watch) = one_vendor();
    let (sink, _source) = line_channel(LINES);
    let threads = Threads::new();
    threads.library_is(library.to_path_buf());
    threads.lines_go_to(cwd.to_path_buf(), sink);
    threads
        .say(&drivers, lead, cwd.to_path_buf(), "what have I got saved?")
        .await
        .map_err(|refusal| refusal.to_string())?;
    watch
        .started()
        .into_iter()
        .next()
        .ok_or_else(|| "the first sentence to a pointed-at lead has to open a session".into())
}

/// Argumenty gotowej komendy jednej tury — jako właścicielskie napisy, bo komenda ginie razem
/// z tą funkcją.
///
/// Przez sterownik, nie przez odczytanie pól `RunSpec`: zwrócona wartość dowodzi, że mechanizm
/// istnieje, a argv dowodzi, że dojechało tam, gdzie patrzy vendor (AGENTS.md niezmiennik 29).
fn argv(spec: &RunSpec) -> Vec<String> {
    ClaudeDriver::new()
        .command(spec)
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

// ── Dubler sterownika ──────────────────────────────────────────────────────────────────────

/// Co dubler zapamiętał: specyfikacja KAŻDEGO uruchomienia, w kolejności startu.
///
/// **Ten zamek nigdy nie przechodzi przez `await`** (niezmiennik 8): cały dostęp jest zamknięty
/// w synchronicznych metodach, więc nie ma wyrażenia, w którym guard dożyłby do punktu
/// zawieszenia.
#[derive(Debug, Default)]
struct Watch {
    started: Mutex<Vec<RunSpec>>,
}

impl Watch {
    fn started(&self) -> Vec<RunSpec> {
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn saw(&self, spec: RunSpec) {
        self.started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec);
    }
}

/// Fabryka oddająca ten sam dubler każdemu vendorowi: o wybór vendora pyta AC-1 z T-60, nie ten
/// plik.
fn one_vendor() -> (Drivers, Arc<Watch>) {
    let watch = Arc::new(Watch::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch: Arc::clone(&watch),
    });
    (Arc::new(move |_vendor| Arc::clone(&driver)), watch)
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
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        self.watch.saw(spec);
        /* Odbiornik głosu żyje tak długo, jak sesja: porzucony razem ze `start` zamykałby kanał,
         * a wtedy każda następna tura odbijałaby się o „stopped listening" i mierzylibyśmy własne
         * sprzątanie. */
        let (voice, mut heard) = mpsc::channel(4);
        tokio::spawn(async move { while heard.recv().await.is_some() {} });
        Ok(Box::new(Turn {
            events,
            session,
            voice,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
    }

    fn group(&self) -> Option<GroupId> {
        // Dubler nie ma procesu, więc nie ma grupy. Zmyślony `pgid` byłby liczbą, po której
        // sprzątanie strzelałoby w cudzy proces.
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        tokio::time::sleep(TURN).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: TURN,
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
