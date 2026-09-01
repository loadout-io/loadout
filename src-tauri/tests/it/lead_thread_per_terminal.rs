//! Wątek lidera należy do **terminalu**, nie do zakresu.
//!
//! # Po co to istnieje
//!
//! T-60 dało wątek per ZAKRES i to był krok w dobrą stronę: rozmowa o projekcie A przestała
//! odpowiadać o A po przełączeniu na B. Zakres był wtedy najdrobniejszą rzeczą, jaką okno umiało
//! nazwać, bo karta BYŁA folderem — `src/sections/run/tabs/store.ts` mówi to wprost: „w jednym
//! zakresie może stać najwyżej jedna karta".
//!
//! Od T-71 karta jest terminalem z własną tożsamością, a człowiek prosi o drugi terminal w tym
//! samym projekcie („kolejne workflow w naszym scope co mamy zaznaczone"). Wątek kluczowany
//! folderem oddaje wtedy obu terminalom JEDNĄ rozmowę: człowiek pisze w lewej karcie, a odpowiedź
//! pojawia mu się w prawej. To jest ta sama cicha porażka, przed którą stoi całe to zadanie —
//! terminal, który wygląda na osobny i dzieli strumień.
//!
//! # Słaba wersja tego kryterium
//!
//! Liczenie wątków. Przechodzi dla implementacji zakładającej nową rozmowę przy KAŻDEJ turze —
//! czyli dla lidera bez pamięci, który za każdym zdaniem zaczyna od zera i płaci za to
//! u dostawcy. Rozstrzyga to punkt (b): powrót do terminalu A ma trafić w tę samą rozmowę, co za
//! pierwszym razem, a nie w trzecią.
//!
//! # Kontrola przeciw pustemu przejściu
//!
//! Oba terminale mają JEDEN folder i ta asercja stoi w każdym przypadku. Bez niej cały plik
//! mierzyłby dwa foldery, czyli to, co `lead_thread_per_scope` już dowiódł — i co przechodzi dla
//! dzisiejszego rejestru, kluczowanego katalogiem.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `lead_thread_per_scope` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// 2026-08-20 — DOPISANE PRZY IMPLEMENTACJI, i to jest jedyna linia tego pliku, której nie
// napisała faza kontraktu. `two_terminals` bierze `&PathBuf`, a `clippy::ptr_arg` (kategoria
// `all`, czyli `deny`) chce `&Path`. Bramka `quick` woła `clippy --lib` i tego nie widzi;
// `full-clippy` sądzi `--all-targets`, więc widzi — i przewracał się na tej jednej linii przy
// każdej zieleni pięciu kryteriów. Wybór między `allow` a przepisaniem fikstury rozstrzyga to,
// czego wolno dotknąć: `allow` jest ADDYTYWNE i nie rusza ani jednej asercji, a przepisanie
// sygnatury pociąga dwa `clone()` w ciele pomocnika, czyli zmianę w wyroczni (AGENTS.md §7).
#![allow(clippy::ptr_arg)]

use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, ToAgent, Tokens, Voice,
};
use loadout_lib::engine::line::LineKind;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::Agent;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy wątki, nie przepustowość.
const LINES: usize = 32;

/// Ile razy oddajemy sterowanie, czekając na zadanie czytające tury.
///
/// Pętla z sufitem, nie `sleep`: czekamy na zdarzenie, a nie na zegar, więc test nie mierzy
/// planisty systemu. Ten sam chwyt i ten sam powód, co w `lead_thread_per_scope`.
const YIELDS: u8 = 64;

/// Pierwsze zdanie w lewym terminalu. Rozpoznawalne, bo to ono odróżnia jedną rozmowę od drugiej.
const FROM_THE_LEFT: &str = "what should the checker look at?";

/// Pierwsze zdanie w prawym terminalu.
const FROM_THE_RIGHT: &str = "and here, what is missing?";

/// Jedno uruchomienie rozmowy: czym się przedstawiła i gdzie stanęła.
///
/// **Identyfikator, a nie katalog**, i to jest cała konstrukcja tego pliku: oba terminale stoją
/// w TYM SAMYM folderze, więc katalog nie odróżnia ich od siebie. Odróżnia je to, co jest im
/// naprawdę własne — rozmowa, którą każdy z nich otworzył.
#[derive(Debug, Clone)]
struct Started {
    /// Identyfikator sesji u dostawcy, ten sam, którym przedstawia się uchwyt.
    id: String,
    /// Katalog roboczy tej rozmowy.
    cwd: PathBuf,
    /// Pierwsze zdanie, czyli to, po którym poznajemy, czyja to rozmowa.
    first: String,
}

/// Co dubler zapamiętał.
#[derive(Debug, Default)]
struct Watch {
    /// Po jednym wpisie na KAŻDE uruchomienie rozmowy.
    starts: Mutex<Vec<Started>>,
    /// Tury, które dojechały GŁOSEM — czyli wszystko po pierwszym zdaniu w terminalu.
    turns: Mutex<Vec<(String, String)>>,
    /// Rozmowy, które oddały dowód śmierci swojej grupy (niezmiennik 6).
    proven_dead: Mutex<Vec<String>>,
}

impl Watch {
    fn starts(&self) -> Vec<Started> {
        self.starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn turns(&self) -> Vec<(String, String)> {
        self.turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn proven_dead(&self) -> Vec<String> {
        self.proven_dead
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Rozmowa, która zaczęła się TYM zdaniem — albo `None`, kiedy takiej nie było.
    fn opened_with(&self, first: &str) -> Option<Started> {
        self.starts().into_iter().find(|one| one.first == first)
    }

    /// Wszystko, co usłyszała ta rozmowa po swoim pierwszym zdaniu.
    fn heard_by(&self, id: &str) -> Vec<String> {
        self.turns()
            .into_iter()
            .filter(|(who, _)| who == id)
            .map(|(_, said)| said)
            .collect()
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
            .starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Started {
                id: session.id.clone(),
                cwd: spec.cwd.clone(),
                first: spec.prompt.clone(),
            });

        /* ODBIORNIK ŻYJE TAK DŁUGO, JAK ROZMOWA. Porzucony razem ze `start` zamykałby kanał, więc
         * druga tura odbijałaby się o „stopped listening" i mierzylibyśmy własne sprzątanie
         * zamiast rozmowy. Zadanie przepisuje tury do `Watch` RAZEM Z IDENTYFIKATOREM, bo „druga
         * tura doszła" i „druga tura doszła do właściwej rozmowy" to dwa różne zdania. */
        let (voice, mut heard) = mpsc::channel(4);
        let watch = Arc::clone(&self.watch);
        let mine = session.id.clone();
        tokio::spawn(async move {
            while let Some(said) = heard.recv().await {
                if let ToAgent::Turn(text) = said {
                    watch
                        .turns
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((mine.clone(), text));
                }
            }
        });
        Ok(Box::new(Turn {
            events,
            session,
            voice,
            watch: Arc::clone(&self.watch),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
    watch: Arc<Watch>,
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
        /* DOWÓD WYDAJE SIĘ TUTAJ I TYLKO TUTAJ. To jest jedyna metoda uchwytu oddająca
         * `GroupProof`, więc zapis w tej liście jest jedynym śladem, po którym da się odróżnić
         * „poprosiłem o dowód" od „porzuciłem uchwyt i nazwałem to zamknięciem". */
        self.watch
            .proven_dead
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.session.id.clone());
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/// Dubler pod jedną fabryką: który vendor, o to pyta `lead_comes_from_the_agent`, a nie ten plik.
fn one_vendor() -> (Drivers, Arc<Watch>) {
    let watch = Arc::new(Watch::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch: Arc::clone(&watch),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    (drivers, watch)
}

/// Lider, o którym ten plik nie ma nic do powiedzenia — jego definicję sądzi inne kryterium.
fn any_lead() -> Lead {
    Lead {
        agent: Agent::example(),
    }
}

/// Dwa terminale w jednym folderze — i kontrola, że fikstura jest tym, czym mówi.
///
/// To jest punkt (e). Bez niego cały plik mierzyłby dwa FOLDERY, czyli to, co T-60 już dowiodło,
/// a dzisiejszy rejestr przechodzi na tym bez zmiany choćby jednej linii.
fn two_terminals(folder: &PathBuf) -> (Terminal, Terminal) {
    let left = Terminal {
        id: "terminal-1".to_owned(),
        folder: folder.clone(),
    };
    let right = Terminal {
        id: "terminal-2".to_owned(),
        folder: folder.clone(),
    };
    assert_ne!(
        left.id, right.id,
        "the fixture has to hand out two DIFFERENT terminals, or every assertion below is about \
         one conversation measured twice"
    );
    assert_eq!(
        left.folder, right.folder,
        "and both of them have to stand in ONE folder. Two folders would be asking what \
         lead_thread_per_scope already answered, and that question passes today."
    );
    (left, right)
}

/// Otwiera strumień tego terminalu tak, jak otwiera go okno (`open_chat`).
fn watching(threads: &Threads, terminal: &Terminal) -> LineSource {
    let (sink, source) = line_channel(LINES);
    threads.terminal_lines_go_to(terminal, sink);
    source
}

/// Oddaje sterowanie, dopóki zadanie czytające tury nie dogoni — albo aż skończy się sufit.
async fn until_heard(watch: &Watch, how_many: usize) {
    for _ in 0..YIELDS {
        if watch.turns().len() >= how_many {
            return;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn two_terminals_in_one_folder_are_two_threads_and_coming_back_lands_in_the_first()
-> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let folder = here.path().to_path_buf();
    let (left, right) = two_terminals(&folder);

    let (drivers, watch) = one_vendor();
    let lead = any_lead();
    let threads = Threads::new();
    let _stream_left = watching(&threads, &left);
    let _stream_right = watching(&threads, &right);

    threads
        .say_in(&drivers, &lead, &left, FROM_THE_LEFT)
        .await
        .expect("the first sentence in a terminal opens its own conversation");
    threads
        .say_in(&drivers, &lead, &right, FROM_THE_RIGHT)
        .await
        .expect("the first sentence in the OTHER terminal opens a second one");

    // ── (a) DWIE ROZMOWY, KAŻDA W TYM SAMYM KATALOGU ────────────────────────────────────────
    let opened = watch.starts();
    assert_eq!(
        opened.len(),
        2,
        "two terminals standing in one folder have to open TWO conversations. A registry keyed by \
         folder hands them one, so the person writes in the left card and the answer shows up in \
         the right one — a terminal that looks separate and shares its stream. It opened: {opened:?}"
    );
    assert!(
        opened.iter().all(|one| one.cwd == folder),
        "a conversation stood somewhere other than the folder its terminal is in. The terminal \
         carries the folder; it does not choose one. It opened: {opened:?}"
    );

    // ── (b) POWRÓT DO LEWEGO TRAFIA W TĘ SAMĄ ROZMOWĘ ───────────────────────────────────────
    //
    // I to ten punkt odróżnia „wątek na terminal" od „wątek na turę". Implementacja startująca
    // rozmowę na każde zdanie przechodzi (a), płaci zimny start za każdym razem i gubi wszystko,
    // co powiedziano: następne zdanie trafia do modelu, który nie słyszał poprzedniego.
    threads
        .say_in(&drivers, &lead, &left, "and add a second reviewer")
        .await
        .expect("coming back to a terminal is another turn of its conversation");
    until_heard(&watch, 1).await;

    let mine = watch
        .opened_with(FROM_THE_LEFT)
        .expect("the left terminal opened a conversation with the sentence it was given");
    let theirs = watch
        .opened_with(FROM_THE_RIGHT)
        .expect("and the right one with its own");

    assert_eq!(
        watch.starts().len(),
        2,
        "coming back to the left terminal opened a third conversation. Two terminals are two \
         conversations — for good — and one per turn is a lead agent with no memory of what you \
         just said. It opened: {:?}",
        watch.starts()
    );
    assert_eq!(
        watch.heard_by(&mine.id),
        vec!["and add a second reviewer".to_owned()],
        "the sentence said on returning to the left terminal has to reach ITS conversation. An \
         implementation that quietly drops it looks identical on the count and loses half of what \
         was said; one that sends it to the other terminal answers the wrong card."
    );
    assert!(
        watch.heard_by(&theirs.id).is_empty(),
        "a sentence said in the left terminal reached the conversation of the right one. Both \
         cards stand in the same folder, so this is the defect this file exists to stop: {:?}",
        watch.heard_by(&theirs.id)
    );
    Ok(())
}

#[tokio::test]
async fn closing_one_terminal_ends_its_own_thread_and_proves_it() -> Result<(), Box<dyn Error>> {
    // ── (c) ZAMKNIĘCIE TERMINALU KOŃCZY JEGO ROZMOWĘ, Z DOWODEM (NIEZMIENNIK 6) ──────────────
    //
    // Rozmowa porzucona żywa przechodzi pod PID 1 i pracuje dalej (`recovery.rs`, nagłówek),
    // a odzyskiwanie po niej nie posprząta, bo rozmowa nie ma wpisu w indeksie biegów. Osierocony
    // agent pali limit w tle: to jest błąd finansowy, nie higieniczny.
    let here = tempfile::tempdir()?;
    let folder = here.path().to_path_buf();
    let (left, right) = two_terminals(&folder);

    let (drivers, watch) = one_vendor();
    let lead = any_lead();
    let threads = Threads::new();
    let _stream_left = watching(&threads, &left);
    let _stream_right = watching(&threads, &right);

    threads
        .say_in(&drivers, &lead, &left, FROM_THE_LEFT)
        .await?;
    threads
        .say_in(&drivers, &lead, &right, FROM_THE_RIGHT)
        .await?;
    assert_eq!(
        watch.starts().len(),
        2,
        "the fixture has to have TWO conversations standing before we close anything, or \"the \
         other one is still there\" is a sentence about nothing"
    );

    let mine = watch
        .opened_with(FROM_THE_LEFT)
        .expect("the left terminal opened its own conversation");
    let proof = threads.close_at(&left.id).await;

    assert!(
        matches!(proof, Some(GroupProof::Dead { .. })),
        "closing a terminal came back without a proof of death. Until `kill(-pgid, 0)` gives \
         ESRCH the group is alive (invariant 6), so anything else here means \"I sent a signal\" \
         reported as \"nothing is left\": {proof:?}"
    );
    assert_eq!(
        watch.proven_dead(),
        vec![mine.id.clone()],
        "the conversation of the terminal being closed was never ASKED for its proof. This is the \
         assertion the returned value cannot make on its own: a close that drops the handle and \
         reports a Dead it never obtained looks identical from the outside, and leaves the agent \
         running."
    );

    assert!(
        !threads.is_live_at(&left.id),
        "the terminal was closed and its conversation is still standing. Closing is not a hint."
    );
    assert!(
        threads.is_live_at(&right.id),
        "closing one terminal ended the conversation of the other. Both cards stand in the same \
         folder, so a registry keyed by folder ends both — and the person who closed the left \
         card loses the work they were doing in the right one, with nothing said about it."
    );
    Ok(())
}

#[tokio::test]
async fn closing_the_window_ends_every_thread_and_each_one_proves_it() -> Result<(), Box<dyn Error>>
{
    // ── (d) ZAMKNIĘCIE OKNA KOŃCZY WSZYSTKIE ────────────────────────────────────────────────
    let here = tempfile::tempdir()?;
    let folder = here.path().to_path_buf();
    let (left, right) = two_terminals(&folder);

    let (drivers, watch) = one_vendor();
    let lead = any_lead();
    let threads = Threads::new();
    let _stream_left = watching(&threads, &left);
    let _stream_right = watching(&threads, &right);

    threads
        .say_in(&drivers, &lead, &left, FROM_THE_LEFT)
        .await?;
    threads
        .say_in(&drivers, &lead, &right, FROM_THE_RIGHT)
        .await?;
    assert_eq!(
        watch.starts().len(),
        2,
        "the fixture has to have TWO conversations standing before the window closes, or \"every \
         one of them went down\" is a sentence about one"
    );

    let proofs = threads.close().await;

    assert_eq!(
        proofs.len(),
        2,
        "closing the window has to come back with one proof PER TERMINAL. One number saying \"two \
         closed\" cannot tell a group that is gone from a group that is still answering — and \
         a single Alive among the Dead is exactly the state nobody would learn about."
    );
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "a conversation came back without a proof of death (invariant 6): {proofs:?}"
    );
    assert_eq!(
        watch.proven_dead().len(),
        2,
        "not every conversation was ASKED for its proof. Two terminals in one folder are two \
         live agents, and a close that only knows about folders asks once and leaves one of them \
         running: {:?}",
        watch.proven_dead()
    );
    assert!(
        !threads.is_live_at(&left.id) && !threads.is_live_at(&right.id),
        "the window closed and a conversation is still standing"
    );
    Ok(())
}

// ── ACTOR: CODEX CZEKA PER TERMINAL, A STOP MA OSOBNE PASMO ─────────────────────────────

#[derive(Debug, Clone, Copy)]
enum ActorMode {
    BlockingCodex,
    DeadVoice,
    AliveVoice,
    ImmediateFirstAnswer,
    FinalAnswerOnCancel,
    FirstReceiptFailure,
}

#[derive(Debug, Default)]
struct ActorWatch {
    starts: AtomicUsize,
    cancels: AtomicUsize,
    drops: AtomicUsize,
}

struct ActorFake {
    mode: ActorMode,
    watch: Arc<ActorWatch>,
    waiting: mpsc::Sender<()>,
    evidence: Option<EvidenceTarget>,
}

#[async_trait]
impl AgentDriver for ActorFake {
    fn id(&self) -> &'static str {
        "actor-fake"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("actor-fake".to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.watch.starts.fetch_add(1, Ordering::SeqCst);
        if matches!(self.mode, ActorMode::FirstReceiptFailure) {
            let evidence = self
                .evidence
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("the Lead omitted its evidence target"))?;
            /* Psujemy DOKŁADNIE receipt po udanym starcie dublera. Dzięki temu wyrocznia
             * przechodzi przez produkcyjne `accept_turn(1)`, a nie testuje pomocnik obok niego. */
            std::fs::write(evidence.root().join("conversation.json"), b"{")?;
        }
        let session = SessionRef {
            vendor: "actor-fake",
            id: spec.run_id.to_string(),
        };
        if matches!(self.mode, ActorMode::ImmediateFirstAnswer) {
            events
                .send(
                    (AgentEvent::Said {
                        text: "first answer".to_owned(),
                    })
                    .into(),
                )
                .await?;
        }
        let voice = match self.mode {
            ActorMode::BlockingCodex
            | ActorMode::ImmediateFirstAnswer
            | ActorMode::FinalAnswerOnCancel
            | ActorMode::FirstReceiptFailure => None,
            ActorMode::DeadVoice | ActorMode::AliveVoice => {
                /* Nadajnik wygląda jak prawdziwa zdolność, ale odbiorca już nie żyje. To jest
                 * awaria, która wcześniej zostawiała na zawsze martwy wpis w `Threads::live`. */
                let (voice, heard) = mpsc::channel(1);
                drop(heard);
                Some(voice)
            }
        };
        Ok(Box::new(ActorTurn {
            mode: self.mode,
            watch: Arc::clone(&self.watch),
            waiting: self.waiting.clone(),
            blocks_in_wait: matches!(self.mode, ActorMode::BlockingCodex)
                && spec.prompt == FROM_THE_LEFT,
            waited: false,
            events,
            session,
            voice,
        }))
    }

    fn with_evidence(&self, target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            mode: self.mode,
            watch: Arc::clone(&self.watch),
            waiting: self.waiting.clone(),
            evidence: Some(target),
        }))
    }
}

#[derive(Debug)]
struct ActorTurn {
    mode: ActorMode,
    watch: Arc<ActorWatch>,
    waiting: mpsc::Sender<()>,
    blocks_in_wait: bool,
    waited: bool,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Option<Voice>,
}

impl ActorTurn {
    fn outcome(&self) -> TurnOutcome {
        TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        }
    }
}

#[async_trait]
impl AgentHandle for ActorTurn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        self.voice.clone()
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        if !self.waited {
            anyhow::bail!("send was called before wait completed");
        }
        self.waited = false;
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        if self.blocks_in_wait {
            let _ = self.waiting.send(()).await;
            std::future::pending::<()>().await;
        }
        self.waited = true;
        let outcome = self.outcome();
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        if matches!(self.mode, ActorMode::FinalAnswerOnCancel) {
            let _ = self
                .events
                .send(
                    (AgentEvent::Said {
                        text: "final answer while stopping".to_owned(),
                    })
                    .into(),
                )
                .await;
        }
        let attempt = self.watch.cancels.fetch_add(1, Ordering::SeqCst);
        if matches!(
            self.mode,
            ActorMode::AliveVoice | ActorMode::FirstReceiptFailure
        ) && attempt == 0
        {
            GroupProof::Alive { group: None }
        } else {
            GroupProof::Dead { status: None }
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

impl Drop for ActorTurn {
    fn drop(&mut self) {
        self.watch.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn actor_vendor(mode: ActorMode) -> (Drivers, Arc<ActorWatch>, mpsc::Receiver<()>) {
    let watch = Arc::new(ActorWatch::default());
    let (waiting, heard_wait) = mpsc::channel(4);
    let driver: Arc<dyn AgentDriver> = Arc::new(ActorFake {
        mode,
        watch: Arc::clone(&watch),
        waiting,
        evidence: None,
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    (drivers, watch, heard_wait)
}

async fn two_visible_lines(source: &mut LineSource) -> Vec<(LineKind, String)> {
    let mut seen = Vec::new();
    for _ in 0..YIELDS {
        while let Some(line) = source.try_next() {
            seen.push((line.kind(), line.text().to_owned()));
        }
        if seen.len() >= 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    seen
}

#[tokio::test(flavor = "current_thread")]
async fn dead_proof_drains_the_final_answer_before_close_returns() -> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-final-answer".to_owned(),
        folder: here.path().to_path_buf(),
    };
    let (drivers, _watch, _heard_wait) = actor_vendor(ActorMode::FinalAnswerOnCancel);
    let lead = any_lead();
    let threads = Threads::new();
    let mut stream = watching(&threads, &terminal);

    threads
        .say_in(&drivers, &lead, &terminal, "please finish this")
        .await?;
    assert!(matches!(
        threads.close_at(&terminal.id).await,
        Some(GroupProof::Dead { .. })
    ));

    assert_eq!(
        two_visible_lines(&mut stream).await,
        vec![
            (LineKind::Told, "please finish this".to_owned()),
            (LineKind::Note, "final answer while stopping".to_owned()),
        ],
        "Dead is not permission to abort the reader: events queued by cancel must drain through \
         Curator before close returns"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn the_first_answer_never_overtakes_the_persons_first_turn() -> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-first-order".to_owned(),
        folder: here.path().to_path_buf(),
    };
    let (drivers, _watch, _heard_wait) = actor_vendor(ActorMode::ImmediateFirstAnswer);
    let lead = any_lead();
    let threads = Threads::new();
    let mut stream = watching(&threads, &terminal);

    threads
        .say_in(&drivers, &lead, &terminal, "first question")
        .await?;
    assert_eq!(
        two_visible_lines(&mut stream).await,
        vec![
            (LineKind::Told, "first question".to_owned()),
            (LineKind::Note, "first answer".to_owned()),
        ],
        "the reader must not forward a fast vendor answer before the accepted first turn is visible"
    );
    assert!(matches!(
        threads.close_at(&terminal.id).await,
        Some(GroupProof::Dead { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn a_waiting_codex_terminal_does_not_block_another_terminal_or_stop()
-> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let folder = here.path().to_path_buf();
    let (left, right) = two_terminals(&folder);
    let (drivers, _watch, mut heard_wait) = actor_vendor(ActorMode::BlockingCodex);
    let lead = any_lead();
    let threads = Arc::new(Threads::new());
    let _stream_left = watching(threads.as_ref(), &left);
    let _stream_right = watching(threads.as_ref(), &right);

    threads
        .say_in(&drivers, &lead, &left, FROM_THE_LEFT)
        .await?;
    threads
        .say_in(&drivers, &lead, &right, FROM_THE_RIGHT)
        .await?;

    let waiting_threads = Arc::clone(&threads);
    let waiting_drivers = Arc::clone(&drivers);
    let waiting_lead = lead.clone();
    let waiting_left = left.clone();
    let blocked = tokio::spawn(async move {
        waiting_threads
            .say_in(
                &waiting_drivers,
                &waiting_lead,
                &waiting_left,
                "second turn waits for the first process",
            )
            .await
    });
    timeout(Duration::from_secs(1), heard_wait.recv())
        .await
        .expect("the left actor has to enter its deliberately blocked wait")
        .expect("the wait signal channel must stay open");

    timeout(
        Duration::from_secs(1),
        threads.say_in(&drivers, &lead, &right, "right terminal stays responsive"),
    )
    .await
    .expect("terminal B was blocked by terminal A's handle.wait")
    .expect("terminal B preserves its own wait-before-send order");

    let right_proof = timeout(Duration::from_secs(1), threads.close_at(&right.id))
        .await
        .expect("closing terminal B was blocked by terminal A")
        .expect("terminal B has a live conversation to close");
    assert!(matches!(right_proof, GroupProof::Dead { .. }));

    let left_proof = timeout(Duration::from_secs(1), threads.close_at(&left.id))
        .await
        .expect("Stop could not interrupt the blocked wait in its own actor")
        .expect("terminal A has a live conversation to close");
    assert!(matches!(left_proof, GroupProof::Dead { .. }));
    assert!(
        blocked.await?.is_err(),
        "the interrupted follow-up must not be reported as delivered"
    );
    Ok(())
}

#[tokio::test]
async fn a_dead_voice_is_replaced_by_a_fresh_conversation_on_the_next_message()
-> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-dead-voice".to_owned(),
        folder: here.path().to_path_buf(),
    };
    let (drivers, watch, _heard_wait) = actor_vendor(ActorMode::DeadVoice);
    let lead = any_lead();
    let threads = Threads::new();
    let _stream = watching(&threads, &terminal);

    threads.say_in(&drivers, &lead, &terminal, "first").await?;
    assert!(
        threads
            .say_in(&drivers, &lead, &terminal, "second")
            .await
            .is_err(),
        "the dead Voice must refuse the follow-up"
    );
    threads
        .say_in(&drivers, &lead, &terminal, "fresh third")
        .await
        .expect("Dead proof must make the next message open a fresh conversation");
    assert_eq!(watch.starts.load(Ordering::SeqCst), 2);
    assert!(matches!(
        threads.close_at(&terminal.id).await,
        Some(GroupProof::Dead { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn alive_proof_keeps_the_handle_registered_until_stop_can_prove_dead()
-> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-alive-proof".to_owned(),
        folder: here.path().to_path_buf(),
    };
    let (drivers, watch, _heard_wait) = actor_vendor(ActorMode::AliveVoice);
    let lead = any_lead();
    let threads = Threads::new();
    let _stream = watching(&threads, &terminal);

    threads.say_in(&drivers, &lead, &terminal, "first").await?;
    let refusal = threads
        .say_in(&drivers, &lead, &terminal, "second")
        .await
        .expect_err("the dead Voice has to fail before cleanup is judged");
    assert!(
        refusal.to_string().contains("still tracking it"),
        "Alive must not promise a fresh conversation: {refusal}"
    );
    assert!(
        threads.is_live_at(&terminal.id),
        "GroupProof::Alive was dropped out of the registry together with its only handle"
    );
    assert_eq!(watch.starts.load(Ordering::SeqCst), 1);

    let proof = threads
        .close_at(&terminal.id)
        .await
        .expect("the retained handle must be available to retry Stop");
    assert!(matches!(proof, GroupProof::Dead { .. }));
    assert_eq!(watch.cancels.load(Ordering::SeqCst), 2);
    assert!(!threads.is_live_at(&terminal.id));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn first_receipt_failure_keeps_its_unregistered_handle_until_dead()
-> Result<(), Box<dyn Error>> {
    let here = tempfile::tempdir()?;
    let terminal = Terminal {
        id: "terminal-first-receipt-refusal".to_owned(),
        folder: here.path().to_path_buf(),
    };
    let (drivers, watch, _heard_wait) = actor_vendor(ActorMode::FirstReceiptFailure);
    let lead = any_lead();
    let threads = Threads::new();
    let _stream = watching(&threads, &terminal);

    let refusal = timeout(
        Duration::from_secs(3),
        threads.say_in(&drivers, &lead, &terminal, "first"),
    )
    .await
    .expect("cleanup stopped retrying before its second deterministic proof")
    .expect_err("a corrupt first-turn receipt was accepted");
    assert!(
        refusal.to_string().contains("did not send the message"),
        "the person did not receive the fixed safe receipt refusal: {refusal}"
    );
    assert_eq!(watch.starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        watch.cancels.load(Ordering::SeqCst),
        2,
        "GroupProof::Alive dropped the only handle instead of retrying Stop on that same handle"
    );
    assert_eq!(
        watch.drops.load(Ordering::SeqCst),
        1,
        "the handle was leaked or dropped before its deterministic Dead proof"
    );
    assert!(
        !threads.is_live_at(&terminal.id),
        "a conversation that never passed its first receipt was registered as live"
    );
    assert!(
        threads.close_at(&terminal.id).await.is_none(),
        "the failed first turn left a hidden handle in the terminal registry"
    );
    Ok(())
}
