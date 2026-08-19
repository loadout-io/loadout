//! AC-2 dla T-43: zatrzymanie i limit czasu ubijają grupę, dowodzą tego, i są wartością.
//!
//! # Słaba wersja tego kryterium i co ją odrzuca
//!
//! Słabą wersją jest „funkcja wróciła w mniej niż N sekund". Przechodzi ją
//! `tokio::time::timeout(limit, handle.wait())` — trzy znaki krótsza i o cały niezmiennik 10
//! gorsza: zdejmuje ZADANIE RUSTA, a proces vendora zostaje żywy i pali limit u dostawcy do
//! końca świata. Z zewnątrz obie implementacje wyglądają identycznie: wracają równie szybko,
//! obie mówią „zatrzymane". Rozróżnia je **jedna** rzecz i tylko ona — czy na uchwycie zostało
//! zawołane `cancel()`, czyli czy ktokolwiek zapytał o [`GroupProof`].
//!
//! To jest błąd finansowy, nie higieniczny: osierocony agent pisze dalej i płaci za to człowiek.
//!
//! # Trzy asercje, które muszą stać razem
//!
//! * `cancel()` **zawołane** — to odrzuca zdjęte zadanie Rusta;
//! * odpowiedź jest **wartością** (`DraftOutcome::Cancelled`), nigdy `Err` (niezmiennik 7);
//! * `GroupProof::Alive` daje **zdanie**, a nie ciszę — to jest jedyny dowód, że dowód jest
//!   CZYTANY, a nie tylko odbierany. Implementacja wołająca `cancel()` i wyrzucająca wynik
//!   przechodzi obie poprzednie asercje i milczy o agencie, który dalej działa.
//!
//! # Zegar
//!
//! `start_paused`, tak jak w `step_deadline_stops_the_agent`: prawdziwe dwie minuty w teście są
//! niewykonalne, a limit liczony w minutach nie zejdzie niżej bez zmiany jednostki w definicji
//! agenta. Tokio przewija zegar samo, kiedy wszystko śpi — a jedyną rzeczą, która wtedy śpi,
//! jest limit draftu. Każde czekanie jest w tym pliku ograniczone z osobna: bieg, który wisi,
//! jest dla bramki „nic się nie uruchomiło" (rc 124), a nie czerwienią.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::skills::{DraftOutcome, Drafting, draft_skill_inner};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::library::agents::{Vendor, read_agent_file};
use loadout_lib::skills::Error as SkillError;
use tempfile::TempDir;
use tokio::sync::mpsc;

const VENDOR: &str = "claude-code";

/// Zdanie człowieka i zdanie, które wpisze zaraz po nim, nie czekając na pierwsze.
const WANT: &str = "Something that reads a change and says what to fix first.";
const WANT_AGAIN: &str = "Something that writes release notes from a list of changes.";

/// Agent, który ma to napisać, i jego limit — ta sama liczba, którą przewija zegar testu.
const WRITER_ID: &str = "01990000-0000-7000-8000-00000000e001";
const GIVE_UP_MINUTES: u32 = 2;

const WRITER: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000e001
name: Forge
summary: Writes code
color: clay
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 2
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Write the smallest change that makes the checks pass.
";

/// Gotowa umiejętność — dla gałęzi, w której tura KOŃCZY się sama.
const MODEL_TEXT: &str = concat!(
    "---\n",
    "name: pr-review\n",
    "description: Use this when somebody asks for a second look at a change.\n",
    "---\n",
    "\n",
    "Read the change first, then say in one paragraph what to fix.\n",
);

/// Sufit czekania na jeden draft. Zegar jest zatrzymany, więc godzina nie kosztuje ani jednej
/// prawdziwej sekundy; jest tu tylko po to, żeby zawieszona implementacja padła na zdaniu.
const PATIENCE: Duration = Duration::from_hours(1);

/// Ile razy oddajemy sterowanie, czekając, aż dubler zapisze pierwsze uruchomienie.
const YIELDS: usize = 10_000;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stopping_it_comes_back_as_a_value_with_the_group_proven_dead() {
    let world = World::new();
    let watch = Arc::new(Watch::default());
    let drivers = drivers_for(Arc::clone(&watch), Turns::Never, Proves::Dead);
    let drafting = Drafting::new();

    let (drafted, ()) = inside_the_hour(async {
        tokio::join!(
            draft_skill_inner(&world.library, &drivers, &drafting, WANT, WRITER_ID),
            async {
                once_it_is_writing(&watch).await;
                drafting.stop();
            }
        )
    })
    .await;

    assert_eq!(
        watch.starts(),
        1,
        "the draft never reached a driver, so there was nothing to stop and the rest of this \
         criterion has nothing to look at"
    );
    assert!(
        matches!(drafted, Ok(DraftOutcome::Cancelled)),
        "stopping a draft has to come back as a VALUE, not an error (invariant 7): Err(Cancelled) \
         makes every caller re-derive the difference between \"this failed\" and \"a person \
         stopped it\", and a difference lost once is lost everywhere. It came back as {drafted:?}"
    );
    assert_eq!(
        watch.cancels(),
        1,
        "stopping has to go through the driver's cancel(), exactly once. Dropping the Rust task \
         instead -- tokio::time::timeout around the turn, or simply returning early -- leaves the \
         process group alive and burning the vendor's limit (invariants 6 and 10). That is the \
         only thing that tells the two implementations apart, because both come back just as fast"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn over_its_limit_it_goes_through_the_driver_too() {
    let world = World::new();
    let watch = Arc::new(Watch::default());
    let drivers = drivers_for(Arc::clone(&watch), Turns::Never, Proves::Dead);
    let drafting = Drafting::new();

    // Nikt tu nie naciska Stop. Jedyną rzeczą, która potem śpi, jest limit draftu, więc zegar
    // przeskakuje do niego sam.
    let drafted = inside_the_hour(draft_skill_inner(
        &world.library,
        &drivers,
        &drafting,
        WANT,
        WRITER_ID,
    ))
    .await;

    assert_eq!(
        watch.starts(),
        1,
        "the draft never reached a driver, so it never had a turn to run out of time"
    );
    assert_eq!(
        watch.cancels(),
        1,
        "a draft past its {GIVE_UP_MINUTES} minute limit has to end the SAME way a Stop does: \
         through the driver's cancel(). An assertion about elapsed time alone would not catch \
         this, because tokio::time::timeout around wait() finishes just as fast and never calls \
         cancel() -- it drops the Rust task and leaves the agent writing, with nobody watching \
         and the person paying"
    );
    assert!(
        !matches!(drafted, Ok(DraftOutcome::Wrote(_))),
        "a draft that ran out of time has no skill to hand over, so it may not report one. It \
         came back as {drafted:?}"
    );
    assert!(
        !what_the_person_reads(&drafted).is_empty(),
        "running out of time has to say so. Silence here reads as a control that did nothing, and \
         the person presses it again -- say which number to change so there is something to do \
         about it"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_group_that_may_still_be_alive_gets_a_sentence_and_never_a_success() {
    let world = World::new();
    let watch = Arc::new(Watch::default());
    let drivers = drivers_for(Arc::clone(&watch), Turns::Never, Proves::Alive);
    let drafting = Drafting::new();

    let (drafted, ()) = inside_the_hour(async {
        tokio::join!(
            draft_skill_inner(&world.library, &drivers, &drafting, WANT, WRITER_ID),
            async {
                once_it_is_writing(&watch).await;
                drafting.stop();
            }
        )
    })
    .await;

    assert_eq!(
        watch.cancels(),
        1,
        "the draft never asked the driver for a proof, so there is no answer to judge here"
    );
    assert!(
        !matches!(drafted, Ok(DraftOutcome::Wrote(_))),
        "an agent that may still be running has not written anything, so this may not come back \
         as a written skill. It came back as {drafted:?}"
    );
    assert!(
        !what_the_person_reads(&drafted).is_empty(),
        "GroupProof::Alive means the group still answers signal zero, so Loadout does NOT know \
         the agent stopped -- and until there is proof we treat it as alive (invariant 6). That \
         has to arrive as a sentence a person can read. The same run with GroupProof::Dead is \
         silent and comes back as Cancelled, so the proof is the only thing that differs: an \
         implementation that calls cancel() and throws the answer away passes every other \
         assertion in this file and says nothing about an agent that is still burning money"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_second_question_is_refused_and_the_first_one_is_left_alone() {
    let world = World::new();
    let watch = Arc::new(Watch::default());
    let drivers = drivers_for(Arc::clone(&watch), Turns::Never, Proves::Dead);
    let drafting = Drafting::new();

    let (first, (second, touched)) = inside_the_hour(async {
        tokio::join!(
            draft_skill_inner(&world.library, &drivers, &drafting, WANT, WRITER_ID),
            async {
                once_it_is_writing(&watch).await;
                let second =
                    draft_skill_inner(&world.library, &drivers, &drafting, WANT_AGAIN, WRITER_ID)
                        .await;
                // Zdjęte ZANIM naciśniemy Stop: potem obie drogi wyglądają tak samo.
                let touched = watch.cancels();
                drafting.stop();
                (second, touched)
            }
        )
    })
    .await;

    assert_eq!(
        watch.starts(),
        1,
        "the second question reached a driver while the first one was still writing. The draft has \
         its own explicit limit of one, because there is no shared pool of slots to take one from: \
         the limit on how many at once is per run today, and the function that takes a shared pool \
         has no caller in production"
    );
    assert!(
        !what_the_person_reads(&second).is_empty(),
        "the second question has to be a refusal WITH A SENTENCE. Silence looks exactly like a \
         broken control: the person presses it again, and then reports a bug. It came back as \
         {second:?}"
    );
    assert_eq!(
        touched, 0,
        "the second question took the first one's agent down. Refusing means leaving the first \
         draft untouched -- a refusal that cancels what is already running is worse than a queue, \
         because the person loses the answer they were waiting for and never learns why"
    );
    assert!(
        matches!(first, Ok(DraftOutcome::Cancelled)),
        "the first draft was stopped by a person after the refusal and has to come back as \
         Cancelled. It came back as {first:?}"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_working_folder_of_the_draft_is_left_in_the_library() {
    let world = World::new();
    let before = files(&world.library);
    assert!(
        !before.is_empty(),
        "the fixture library holds no files, so comparing it before and after would pass over any \
         implementation at all"
    );

    // (1) Draft, który się udał: jest z czego przeczytać trzy pola, więc gdzieś musiał stanąć plik.
    let wrote = Arc::new(Watch::default());
    let answers = drivers_for(Arc::clone(&wrote), Turns::Answers, Proves::Dead);
    let drafted = inside_the_hour(draft_skill_inner(
        &world.library,
        &answers,
        &Drafting::new(),
        WANT,
        WRITER_ID,
    ))
    .await;
    assert!(
        matches!(drafted, Ok(DraftOutcome::Wrote(_))),
        "the turn finished with a skill in it and the draft did not hand it over, so this test \
         never got as far as the folder it is about: {drafted:?}"
    );
    assert_eq!(
        files(&world.library),
        before,
        "a draft that finished left a file behind in the library. The three fields go into the form \
         from T-42 and NOTHING is saved yet: the save path is one (author_skill), and it is the one \
         that composes the file, scans it and puts down the canonical copy. A folder left here is a \
         skill nobody reviewed, sitting where this section keeps the ones somebody did"
    );

    // (2) Draft zatrzymany w połowie — ta sama biblioteka, ta sama odpowiedź.
    let stopped = Arc::new(Watch::default());
    let never = drivers_for(Arc::clone(&stopped), Turns::Never, Proves::Dead);
    let drafting = Drafting::new();
    let (halted, ()) = inside_the_hour(async {
        tokio::join!(
            draft_skill_inner(&world.library, &never, &drafting, WANT, WRITER_ID),
            async {
                once_it_is_writing(&stopped).await;
                drafting.stop();
            }
        )
    })
    .await;
    assert_eq!(
        stopped.starts(),
        1,
        "the second half of this test never reached a driver, so nothing was interrupted and the \
         comparison below is about a draft that never happened"
    );
    assert!(
        matches!(halted, Ok(DraftOutcome::Cancelled)),
        "the stopped draft came back as {halted:?}"
    );
    assert_eq!(
        files(&world.library),
        before,
        "a draft that was stopped half way left its working folder in the library. However it \
         ends -- written, stopped, out of time -- the library has to look the way it looked \
         before: a folder left behind by an interrupted draft is the one nobody will ever go \
         looking for"
    );
}

// ── czekanie, zawsze ograniczone ───────────────────────────────────────────────────────────

/// Wynik pracy albo nazwana porażka, kiedy nie przyszedł.
async fn inside_the_hour<T>(work: impl Future<Output = T>) -> T {
    match tokio::time::timeout(PATIENCE, work).await {
        Ok(done) => done,
        Err(_) => unreachable!(
            "the draft never came back. A turn that hangs is \"nothing ran\" to the gate (rc 124), \
             not a red criterion -- so every wait in this file is bounded here, in the test"
        ),
    }
}

/// Oddaje sterowanie, dopóki dubler nie zapisze pierwszego uruchomienia.
///
/// Pętla jest ograniczona z rozmysłem: droga, w której draft nigdy nie dochodzi do sterownika,
/// ma paść na asercji o liczbie uruchomień, a nie zawiesić bramkę.
async fn once_it_is_writing(watch: &Watch) {
    for _ in 0..YIELDS {
        if watch.starts() > 0 {
            return;
        }
        tokio::task::yield_now().await;
    }
}

/// Zdanie, które człowiek dostanie z tego wyniku. `""` znaczy „cisza".
fn what_the_person_reads(drafted: &Result<DraftOutcome, SkillError>) -> String {
    match drafted {
        Ok(_) => String::new(),
        Err(error) => error.to_string(),
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

struct World {
    _tmp: TempDir,
    /// `~/.loadout`. Katalog domowy jest jego RODZICEM (`commands::skills::global_roots`).
    library: PathBuf,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let library = tmp.path().join(".loadout");
        fs::create_dir_all(library.join("agents")).unwrap();
        let path = library.join("agents").join("forge.md");
        fs::write(&path, WRITER).unwrap();
        // Przesłanka, nie kryterium: definicja, której biblioteka nie umie przeczytać, daje
        // czerwień nie do odróżnienia od braku zachowania.
        assert!(
            read_agent_file(&path).is_ok(),
            "the fixture agent cannot be read back by the library, so no criterion in this file \
             could ever pass: {:?}",
            read_agent_file(&path).err().map(|error| error.to_string())
        );
        Self { _tmp: tmp, library }
    }
}

/// Każdy PLIK pod tym katalogiem, ścieżką względną i posortowany.
///
/// Pliki, nie katalogi, i to jest wybór: katalog roboczy draftu zawsze niesie `SKILL.md` — bez
/// niego nie ma czego przeczytać rdzeniem — więc porównanie plików łapie każdą prawdziwą
/// pozostałość. Pusty katalog, w którym nie leży ani jeden bajt, nie jest umiejętnością, której
/// nikt nie przejrzał, i karanie za niego zamieniłoby to kryterium w spór o `mkdir`.
fn files(root: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    walk(root, root, &mut found);
    found
}

fn walk(dir: &Path, root: &Path, into: &mut BTreeSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, into);
        } else {
            into.insert(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
}

// ── dubler sterownika ──────────────────────────────────────────────────────────────────────

/// Jak kończy się tura dublera.
#[derive(Debug, Clone, Copy)]
enum Turns {
    /// Nigdy nie wraca sama — dokładnie tak wygląda model, który jeszcze pisze.
    Never,
    /// Oddaje gotową umiejętność od razu.
    Answers,
}

/// Co `cancel()` powie o grupie.
#[derive(Debug, Clone, Copy)]
enum Proves {
    /// `kill(-pgid, 0)` dał `ESRCH`: w grupie nie ma już nikogo.
    Dead,
    /// Grupa nadal odpowiada. Wynik do obsłużenia, nie błąd do zalogowania.
    Alive,
}

/// Ile razy sterownik został uruchomiony i ile razy dostał `cancel()`.
#[derive(Debug, Default)]
struct Watch {
    starts: Mutex<usize>,
    cancels: Mutex<usize>,
}

impl Watch {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn started(&self) {
        *self.starts.lock().unwrap_or_else(PoisonError::into_inner) += 1;
    }

    fn starts(&self) -> usize {
        *self.starts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cancelled(&self) {
        *self.cancels.lock().unwrap_or_else(PoisonError::into_inner) += 1;
    }

    fn cancels(&self) -> usize {
        *self.cancels.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn drivers_for(watch: Arc<Watch>, turns: Turns, proves: Proves) -> Drivers {
    let fake: Arc<dyn AgentDriver> = Arc::new(Fake {
        watch,
        turns,
        proves,
    });
    Arc::new(move |vendor| match vendor {
        // Jeden dubler dla obu vendorów: ten plik nie sądzi wyboru sterownika (to jest AC-1),
        // tylko to, co się dzieje z żywą turą.
        Vendor::ClaudeCode | Vendor::Codex => Arc::clone(&fake),
    })
}

/// Atrapa sterownika. Mieszka w pliku testu, bo `engine::drivers::fake` jest dublerem PLANISTY
/// i nie implementuje tego traitu.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
    turns: Turns,
    proves: Proves,
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
        self.watch.started();
        Ok(Box::new(Turn {
            watch: Arc::clone(&self.watch),
            events,
            session,
            turns: self.turns,
            proves: self.proves,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    turns: Turns,
    proves: Proves,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(GroupId {
            pid: 4242,
            pgid: 4242,
        })
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: MODEL_TEXT.to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        match self.turns {
            Turns::Never => {
                // Model, który jeszcze pisze. Nic poza limitem czasu i Stopem nie kończy tej tury.
                std::future::pending::<()>().await;
                unreachable!("pending() never resolves")
            }
            Turns::Answers => {
                let _ = tokio::time::timeout(
                    PATIENCE,
                    self.events
                        .send(AgentEvent::Finished(outcome.clone()).into()),
                )
                .await;
                Ok(outcome)
            }
        }
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.cancelled();
        match self.proves {
            Proves::Dead => GroupProof::Dead { status: None },
            Proves::Alive => GroupProof::Alive,
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
