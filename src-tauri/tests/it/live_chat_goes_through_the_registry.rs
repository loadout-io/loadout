//! Żywa rozmowa idzie przez REJESTR wątków, a nie przez jedno pole obok niego.
//!
//! # Po co to istnieje
//!
//! `commands::chat::Threads` stoi w tym drzewie od 2026-08-20, ma własne kryteria i **żywa
//! aplikacja go nie konstruuje**: `ipc::AppState` trzyma `Mutex<Option<Chat>>`, więc zdanie
//! z okna chodzi starą drogą, do jednej rozmowy na całą aplikację. Pisarz T-60 zapisał powód
//! wprost („WĄTEK PER ZAKRES ISTNIEJE I NIE STOI TUTAJ"): podstawienie wymagało klucza obok
//! `folder`, czyli zmiany w pliku, na który jego mandat nie pozwalał. Odmówił podstawienia
//! połowy i miał rację — rozmowa, w której każde zdanie odbija się o „wskaż lidera", jest
//! odmową, której człowiek nie ma jak spełnić.
//!
//! To zadanie posiada oba końce tej drogi, więc zdejmuje blokadę w całości.
//!
//! # Słaba wersja tego kryterium
//!
//! Test na `Threads` zbudowanym w teście. Przechodzi DZIŚ — i to jest dokładnie ta wada, którą
//! recenzent T-70 znalazł na zielonej bramce: mechanizm dowiedziony, produkt go nie woła. Dlatego
//! każda asercja niżej jedzie przez `AppState`, czyli przez to, co rozpakowuje skorupa
//! `#[tauri::command]`, a ostatni przypadek pyta o rzecz, której żaden bieg nie pokaże: czy
//! stare pole naprawdę zniknęło, zamiast zostać obok jako drugi dom dla tej samej rozmowy.
//!
//! # Kontrola przeciw pustemu przejściu
//!
//! Fikstura ma dwa terminale i JEDEN folder. Dwa foldery mierzyłyby to, co T-60 już dowiodło.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{AppState, LineSource, line_channel};
use loadout_lib::library::agents::Agent;
use loadout_lib::store::Store;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy drogę, nie przepustowość.
const LINES: usize = 32;

/// Tożsamość lewego terminalu, znak w znak taka, jak wybiłoby ją okno.
const LEFT: &str = "terminal-1";

/// Tożsamość prawego terminalu. Ten sam folder, inna karta.
const RIGHT: &str = "terminal-2";

/// Co dubler zapamiętał: specyfikacja KAŻDEGO uruchomienia rozmowy.
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
        self.watch
            .started
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec);
        /* Odbiornik głosu żyje tak długo, jak rozmowa: porzucony razem ze `start` zamykałby kanał,
         * a wtedy każda następna tura odbijałaby się o „stopped listening" i mierzylibyśmy własne
         * sprzątanie. Same tury nikogo w tym pliku nie interesują — pyta o nie kryterium obok. */
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
            took: Duration::from_millis(1),
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

/// Biblioteka człowieka i folder pracy — tyle, ile potrzebuje `AppState`.
#[derive(Debug)]
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    /// Folder zakresu, w którym stoją OBA terminale — jako napis, bo tak jedzie z okna.
    fn folder(&self) -> String {
        self.project.path().to_string_lossy().into_owned()
    }

    /// Stan aplikacji złożony tak, jak składa go `src-tauri/src/lib.rs`.
    fn app(&self, drivers: Drivers) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.db())?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            drivers,
        ))
    }

    /// Zapisuje lidera w bibliotece i oddaje jego identyfikator — dokładnie ten napis, którym
    /// okno go wskazuje.
    ///
    /// Przez `save_agent_inner`, nie przez własny zapis pliku: lider zapisany inną drogą niż
    /// produkcyjna sprawdzałby czytnik na bajtach, których produkcja nigdy nie wyprodukuje.
    fn saved_lead(&self) -> Result<String, Box<dyn Error>> {
        let agent = Agent {
            id: Uuid::from_u128(71),
            name: "Lead".to_owned(),
            ..Agent::example()
        };
        save_agent_inner(self.home.path(), &agent)?;
        Ok(agent.id.to_string())
    }
}

/// Dubler pod jedną fabryką: który vendor, o to pyta `lead_comes_from_the_agent`.
fn one_vendor() -> (Drivers, Arc<Watch>) {
    let watch = Arc::new(Watch::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch: Arc::clone(&watch),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    (drivers, watch)
}

/// Okno otwiera strumień tego terminalu — dokładnie ta droga, którą woła montaż ekranu pracy.
async fn watching(state: &AppState, terminal: &str, folder: &str) -> Result<LineSource, String> {
    let (sink, source) = line_channel(LINES);
    state
        .watching_the_lead(terminal, Some(folder), sink)
        .await?;
    Ok(source)
}

#[tokio::test]
async fn two_terminals_of_one_folder_open_two_threads_through_the_live_road()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let who = bench.saved_lead()?;
    let folder = bench.folder();
    let (drivers, watch) = one_vendor();
    let state = bench.app(drivers)?;

    // ── (e) KONTROLA FIKSTURY: DWA TERMINALE, JEDEN FOLDER ──────────────────────────────────
    assert_ne!(
        LEFT, RIGHT,
        "the fixture has to hand out two DIFFERENT terminals, or every assertion below is about \
         one card measured twice"
    );

    let _left = watching(&state, LEFT, &folder).await?;
    let _right = watching(&state, RIGHT, &folder).await?;

    state
        .say_to_the_lead(
            LEFT,
            Some(&folder),
            Some(&who),
            "what should the checker look at?",
        )
        .await
        .map_err(|said| {
            format!("the first sentence in the left terminal was turned down: {said}")
        })?;
    state
        .say_to_the_lead(
            RIGHT,
            Some(&folder),
            Some(&who),
            "and here, what is missing?",
        )
        .await
        .map_err(|said| {
            format!("the first sentence in the right terminal was turned down: {said}")
        })?;

    // ── (b) DWA ZDANIA TĄ DROGĄ ZAKŁADAJĄ DWA WĄTKI ─────────────────────────────────────────
    let opened = watch.started();
    assert_eq!(
        opened.len(),
        2,
        "two terminals of one folder, both spoken to through the road the window uses, have to \
         open TWO conversations. The old road keeps one for the whole application, so the second \
         card answers with the first card conversation — and nothing on screen says so. It \
         opened {} of them.",
        opened.len()
    );
    assert!(
        opened.iter().all(|spec| spec.cwd == bench.project.path()),
        "a conversation stood somewhere other than the folder the window named. The folder \
         travels with the sentence; it is not a value picked on the other side. It opened: {:?}",
        opened
            .iter()
            .map(|spec| spec.cwd.clone())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn without_a_pointed_at_lead_the_road_refuses_and_names_the_next_move()
-> Result<(), Box<dyn Error>> {
    // ── (d) WSKAZANIE JEDZIE Z OKNA ─────────────────────────────────────────────────────────
    //
    // Cichy powrót do zaszytego vendora jest tu gorszy niż odmowa: rozmowa idzie, płaci
    // i odpowiada — tylko nie ten agent, którego człowiek wybrał, a jedyną rzeczą, która się
    // zmieniła, był jego własny klik.
    let bench = Bench::new()?;
    let folder = bench.folder();
    let (drivers, watch) = one_vendor();
    let state = bench.app(drivers)?;
    let _left = watching(&state, LEFT, &folder).await?;

    let refusal = state
        .say_to_the_lead(LEFT, Some(&folder), None, "who is there?")
        .await
        .err()
        .ok_or("a sentence with nobody pointed at as the lead agent was taken, not refused")?;

    assert!(
        refusal.contains("Pick a lead agent"),
        "the refusal has to name the next move (DESIGN §8): a person who is told \"no\" and not \
         told what to do stays exactly where they were. It said: {refusal}"
    );
    assert!(
        watch.started().is_empty(),
        "nobody was pointed at and a conversation started anyway. That is the hard-wired vendor \
         surviving as a default branch — it looks like a working choice, it is paid for, and \
         there is no signal telling it apart from the lead agent the person picked. It opened: \
         {:?}",
        watch.started().len()
    );
    Ok(())
}

#[test]
fn the_application_keeps_no_second_home_for_the_conversation() {
    /* ── (a) + (c) KRYTERIUM STRUKTURALNE ────────────────────────────────────────────────────
     *
     * Dwa domy dla odpowiedzi „gdzie mieszka ta rozmowa" to pierwsza rzecz, która się rozjedzie
     * (niezmiennik 13), i rozjedzie się po cichu: jedna droga zakłada wątki per terminal, druga
     * pisze do jednej rozmowy na całą aplikację, a z ekranu obie wyglądają tak samo.
     *
     * ŹRÓDŁO, bo pytanie dotyczy POLA, a pola nie widać z żadnego biegu. Test „dwa terminale dały
     * dwa wątki" przechodzi także wtedy, gdy stare pole stoi obok nietknięte — dopóki nikt nim nie
     * pisze. Nikt nim nie pisze DZISIAJ; jutro dopisze się gałąź, o której nikt nie pomyślał.
     *
     * To samo jedno pytanie zamyka (a): skoro pojedynczej rozmowy w tym pliku nie ma, skorupa
     * `say_to_orchestrator` nie ma czego zawołać poza rejestrem. Druga asercja pilnuje, żeby
     * ta droga naprawdę miała w tym pliku wołającego — inaczej „przez rejestr" byłoby zdaniem
     * o funkcji, którą woła wyłącznie test.
     */
    let source = include_str!("../../src/ipc.rs");
    /* Bez linii komentarza: ten plik OPISUJE historię tej zmiany, więc skan po całości łapałby
     * własną dokumentację. Ten sam zabieg i ten sam powód, co w `chat_never_starts_a_run`. */
    let code: String = source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with("/*") && !trimmed.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n");

    let words: Vec<&str> = code
        .split(|letter: char| !letter.is_alphanumeric() && letter != '_')
        .collect();

    assert!(
        words.contains(&"Threads"),
        "the application state does not name the registry at all. Without this the assertion \
         below would pass for a file that simply lost both roads, which is not what anybody wants."
    );
    assert!(
        !words.contains(&"Chat"),
        "the single conversation is still a field of the application state. It has to go, not \
         stay next to the registry as a dead one: two homes for \"where does this conversation \
         live\" is the first thing that drifts apart, and the screen cannot tell which one \
         answered."
    );

    let wired = code.matches("say_to_the_lead").count();
    assert!(
        wired >= 2,
        "the road the window uses is declared here and called by nobody in this file, so the \
         command shell is still reaching somewhere else. A mechanism the product does not call \
         is exactly the defect this criterion exists to close; it appeared {wired} time(s)."
    );
}
