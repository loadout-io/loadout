//! Wątek lidera należy do **zakresu**, nie do aplikacji.
//!
//! # Po co to istnieje
//!
//! Rozmowa jest dziś jedna na całą aplikację (`ipc::AppState.chat`), a `Chat::say` używa `cwd`
//! **wyłącznie przy zakładaniu sesji** — każda następna tura leci do żywego procesu, który siedzi
//! w folderze sprzed przełączenia. Skutek widzi człowiek: rozmawia o projekcie A, przełącza się na
//! B i dostaje odpowiedzi o A, bez ani jednego zdania ostrzeżenia. Komentarz przy tamtym polu
//! nazywa to wprost: „jedna na aplikację, nie jedna na zakres — i to jest do przemyślenia, kiedy
//! zakresy dostaną własne sesje".
//!
//! # Słaba wersja tego kryterium
//!
//! Sprawdzenie dwóch `cwd` przy PIERWSZYM zdaniu w każdym zakresie. Przechodzi dla implementacji,
//! która zakłada nową sesję **przy każdej turze** — czyli dla lidera bez pamięci, który za każdym
//! zdaniem zaczyna rozmowę od zera i płaci za to u dostawcy. Rozstrzyga punkt (b): powrót do
//! zakresu A ma trafić w tę samą sesję, a nie w trzecią.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::chat::{Lead, Threads};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, ToAgent, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::Agent;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Ile miejsca w strumieniu linii. Z zapasem — mierzymy wątki, nie przepustowość.
const LINES: usize = 32;

/// Ile razy oddajemy sterowanie, czekając na zadanie czytające tury.
///
/// Pętla z sufitem, nie `sleep`: czekamy na zdarzenie, a nie na zegar, więc test nie mierzy
/// planisty systemu. Ten sam chwyt i ten sam powód, co w `chat_never_starts_a_run`.
const YIELDS: u8 = 64;

/// Co dubler zapamiętał — wszystko z nazwą katalogu, bo pytanie tego pliku brzmi „czyj to wątek".
#[derive(Debug, Default)]
struct Watch {
    /// Po jednym wpisie na KAŻDE uruchomienie procesu, z katalogiem roboczym sesji.
    starts: Mutex<Vec<PathBuf>>,
    /// Tury, które dojechały do sesji GŁOSEM — czyli wszystko po pierwszym zdaniu w zakresie.
    turns: Mutex<Vec<(PathBuf, String)>>,
    /// Sesje, które oddały dowód śmierci swojej grupy (niezmiennik 6).
    proven_dead: Mutex<Vec<PathBuf>>,
}

impl Watch {
    fn starts(&self) -> Vec<PathBuf> {
        self.starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn turns(&self) -> Vec<(PathBuf, String)> {
        self.turns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn proven_dead(&self) -> Vec<PathBuf> {
        self.proven_dead
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Wszystko, co usłyszała sesja tego zakresu — po pierwszym zdaniu, czyli głosem.
    fn heard_in(&self, cwd: &Path) -> Vec<String> {
        self.turns()
            .into_iter()
            .filter(|(scope, _)| scope.as_path() == cwd)
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
        let cwd = spec.cwd.clone();
        self.watch
            .starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(cwd.clone());

        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        /* ODBIORNIK ŻYJE TAK DŁUGO, JAK SESJA. Porzucony razem ze `start` zamykałby kanał, więc
         * druga tura odbijałaby się o „stopped listening" i mierzylibyśmy własne sprzątanie
         * zamiast rozmowy. Zadanie przepisuje tury do `Watch` RAZEM Z KATALOGIEM, bo „druga tura
         * doszła" i „druga tura doszła do właściwego zakresu" to dwa różne zdania. */
        let (voice, mut heard) = mpsc::channel(4);
        let watch = Arc::clone(&self.watch);
        let mine = cwd.clone();
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
            cwd,
            watch: Arc::clone(&self.watch),
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
    /// Katalog tej sesji. Jedyny sposób, żeby dowód śmierci dał się przypisać do zakresu.
    cwd: PathBuf,
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
            .push(self.cwd.clone());
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/// Dubler pod jedną fabryką: który vendor, o to pyta AC-1, a nie ten plik.
fn one_vendor() -> (Drivers, Arc<Watch>) {
    let watch = Arc::new(Watch::default());
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch: Arc::clone(&watch),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));
    (drivers, watch)
}

/// Lider, o którym ten plik nie ma nic do powiedzenia — jego definicję sądzi AC-1.
fn any_lead() -> Lead {
    Lead {
        agent: Agent::example(),
    }
}

/// Otwiera strumień tego zakresu tak, jak otwiera go okno (`open_chat`).
fn watching(threads: &mut Threads, cwd: &Path) -> LineSource {
    let (sink, source) = line_channel(LINES);
    threads.lines_go_to(cwd.to_path_buf(), sink);
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
async fn two_scopes_are_two_threads_and_coming_back_lands_in_the_first()
-> Result<(), Box<dyn Error>> {
    let there = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let a = there.path().to_path_buf();
    let b = elsewhere.path().to_path_buf();

    // ── (e) KONTROLA FIKSTURY: DWA RÓŻNE KATALOGI ───────────────────────────────────────────
    //
    // Bez tego zdania cały plik mógłby mierzyć jeden zakres dwa razy — i wtedy „dwie sesje"
    // byłoby zielone dla implementacji, która nie zna pojęcia zakresu.
    assert_ne!(
        a, b,
        "the fixture has to hand out two DIFFERENT folders, or every assertion below is about one \
         scope measured twice"
    );

    let (drivers, watch) = one_vendor();
    let lead = any_lead();
    let mut threads = Threads::new();
    let _stream_a = watching(&mut threads, &a);
    let _stream_b = watching(&mut threads, &b);

    threads
        .say(
            &drivers,
            &lead,
            a.clone(),
            "what should the checker look at?",
        )
        .await
        .expect("the first sentence in a scope opens its thread");
    threads
        .say(&drivers, &lead, b.clone(), "and here, what is missing?")
        .await
        .expect("the first sentence in the OTHER scope opens its own thread");

    // ── (a) DWIE SESJE, KAŻDA W SWOIM KATALOGU ──────────────────────────────────────────────
    let mut opened = watch.starts();
    opened.sort();
    let mut wanted = vec![a.clone(), b.clone()];
    wanted.sort();
    assert_eq!(
        opened, wanted,
        "a sentence in scope A and a sentence in scope B have to open TWO threads, each with its \
         own working folder. Today the second sentence goes to the process of the FIRST scope, so \
         the person talks about project A, switches to B and gets answers about A — with no \
         sentence of warning."
    );

    // ── (b) POWRÓT DO A TRAFIA W TĘ SAMĄ SESJĘ ──────────────────────────────────────────────
    //
    // I to ten punkt odróżnia „wątek na zakres" od „wątek na turę". Implementacja startująca
    // proces na każde zdanie przechodzi (a) i płaci zimny start za każde zdanie, gubiąc przy tym
    // całą rozmowę: kolejne zdanie trafia do modelu, który nie słyszał poprzedniego.
    threads
        .say(&drivers, &lead, a.clone(), "and add a second reviewer")
        .await
        .expect("coming back to a scope is another turn of its thread");
    until_heard(&watch, 1).await;

    assert_eq!(
        watch.starts().len(),
        2,
        "coming back to scope A started a third thread. Two scopes are two threads — for good — \
         and a thread per turn is a lead with no memory of what you just said. It opened: {:?}",
        watch.starts()
    );
    assert_eq!(
        watch.heard_in(&a),
        vec!["and add a second reviewer".to_owned()],
        "the sentence said on returning to scope A has to reach A's OWN thread. An implementation \
         that quietly drops it looks identical on the thread count and loses half the \
         conversation; one that sends it to B's thread answers about the wrong project."
    );
    assert!(
        watch.heard_in(&b).is_empty(),
        "a sentence said in scope A reached the thread of scope B. That is exactly today's defect, \
         only with the folders swapped: {:?}",
        watch.heard_in(&b)
    );
    Ok(())
}

#[tokio::test]
async fn the_other_scope_keeps_its_thread_while_the_window_looks_away() -> Result<(), Box<dyn Error>>
{
    // ── (c) SESJA ZAKRESU B ŻYJE, KIEDY OKNO PATRZY NA A ────────────────────────────────────
    //
    // Zamknięcie cudzej rozmowy przy przełączeniu byłoby zgubieniem wątku, o który chodzi całe to
    // zadanie — i byłoby to zgubienie CICHE, bo z ekranu A nie widać, co się stało z B.
    let there = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let a = there.path().to_path_buf();
    let b = elsewhere.path().to_path_buf();
    assert_ne!(a, b, "two scopes, two folders (see the control above)");

    let (drivers, watch) = one_vendor();
    let lead = any_lead();
    let mut threads = Threads::new();
    let _stream_a = watching(&mut threads, &a);
    let _stream_b = watching(&mut threads, &b);

    threads
        .say(&drivers, &lead, a.clone(), "hello from the first project")
        .await?;
    threads
        .say(&drivers, &lead, b.clone(), "hello from the second")
        .await?;

    /* OKNO WRACA NA A: nowy kanał, ta sama para wątków. Tę drogę woła każdy montaż ekranu pracy
     * i każde przeładowanie okna — pierwsza wersja `open_chat` zamykała wtedy rozmowę i było to
     * widać w dzienniku („closed its books delivered=0", trzy razy pod rząd). */
    let _again_a = watching(&mut threads, &a);

    assert!(
        threads.is_live_in(&b),
        "the thread of the scope the window is NOT looking at went down. Switching scopes is a \
         click in the side menu, not an instruction to end a conversation."
    );
    assert!(
        threads.is_live_in(&a),
        "reopening the screen for a scope must not end that scope's own thread either — the \
         session at the provider has no reason to know the window reloaded."
    );

    threads
        .say(&drivers, &lead, b.clone(), "still there?")
        .await?;
    until_heard(&watch, 1).await;

    assert_eq!(
        watch.starts().len(),
        2,
        "talking to the scope the window had left behind started a fresh thread instead of \
         continuing its own. It opened: {:?}",
        watch.starts()
    );
    assert_eq!(
        watch.heard_in(&b),
        vec!["still there?".to_owned()],
        "the turn had to reach the thread that was already standing in that scope"
    );
    Ok(())
}

#[tokio::test]
async fn closing_the_window_ends_every_thread_and_each_one_proves_it() -> Result<(), Box<dyn Error>>
{
    // ── (d) WSZYSTKIE WĄTKI SCHODZĄ, KAŻDY Z DOWODEM (NIEZMIENNIK 6) ────────────────────────
    //
    // Rozmowa porzucona żywa przechodzi pod PID 1 i pracuje dalej (`recovery.rs`, nagłówek),
    // a odzyskiwanie po niej nie posprząta, bo rozmowa nie ma wpisu w indeksie biegów. Osierocony
    // agent pali limit w tle: to jest błąd finansowy, nie higieniczny.
    let there = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let a = there.path().to_path_buf();
    let b = elsewhere.path().to_path_buf();
    assert_ne!(a, b, "two scopes, two folders (see the control above)");

    let (drivers, watch) = one_vendor();
    let lead = any_lead();
    let mut threads = Threads::new();
    let _stream_a = watching(&mut threads, &a);
    let _stream_b = watching(&mut threads, &b);

    threads.say(&drivers, &lead, a.clone(), "first").await?;
    threads.say(&drivers, &lead, b.clone(), "second").await?;
    assert_eq!(
        watch.starts().len(),
        2,
        "the fixture has to have TWO threads standing before we close anything, or \"every thread \
         went down\" is a sentence about one thread"
    );

    let proofs = threads.close().await;

    assert_eq!(
        proofs.len(),
        2,
        "closing the window has to come back with one proof PER THREAD. One number saying \"two \
         closed\" cannot tell a group that is gone from a group that is still answering — and \
         a single Alive among the Dead is exactly the state nobody would learn about."
    );
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "a thread came back without a proof of death. Until `kill(-pgid, 0)` gives ESRCH the \
         group is alive (invariant 6), so anything else here means \"I sent a signal\" reported \
         as \"nothing is left\": {proofs:?}"
    );

    let mut asked = watch.proven_dead();
    asked.sort();
    let mut wanted = vec![a.clone(), b.clone()];
    wanted.sort();
    assert_eq!(
        asked, wanted,
        "not every thread was ASKED for its proof. This is the assertion the returned list cannot \
         make on its own: a `close()` that drops the handles and reports two Dead values it never \
         obtained looks identical from the outside, and leaves both agents running."
    );

    assert!(
        !threads.is_live_in(&a) && !threads.is_live_in(&b),
        "the window closed and a thread is still standing. Closing is not a hint."
    );
    Ok(())
}
