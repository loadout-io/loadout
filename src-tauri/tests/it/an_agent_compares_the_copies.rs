//! T-76: co NAPRAWDĘ jedzie do agenta, kiedy człowiek każe porównać kopie jednej pozycji.
//!
//! Ekran importu obiecuje przed skanem jedno zdanie: „Scan reads setup files only. It does not
//! run hooks, skills, agents, or connections." Ta droga woła prawdziwego agenta na prawdziwym
//! cudzym repozytorium, więc jest pierwszą, która tę obietnicę może po cichu unieważnić —
//! i dlatego kryterium pyta o [`RunSpec`], a nie o wartość zwróconą przez funkcję.
//!
//! # Słaba wersja tego kryterium i co ją odrzuca
//!
//! Słabą wersją jest `assert!(matches!(compared, Ok(CompareOutcome::Compared(_))))` — „funkcja
//! oddała porównanie". Przechodzi ją implementacja, która startuje agenta z `Policy::Unrestricted`,
//! z siecią, z korzeniem cudzego projektu w `cwd` i z pytaniem w argv: wszystko działa, ekran
//! pokazuje zdania, a obietnica z akapitu wyżej jest nieprawdą. Rozstrzygają porównania,
//! których wynik funkcji nie zna:
//!
//! * `policy`, `reaches_the_web` i `extra_dirs` przechwyconego `RunSpec`;
//! * `cwd`, sprawdzone przeciw korzeniowi skanowanego projektu — nie przeciw literałowi;
//! * treść OBU kopii w `prompt`, wzięta z tych samych stałych, które fikstura zapisała na dysk.
//!
//! Agent w fikstury jest zapisany jako `work-freely` z rozmysłem: implementacja, która kopiuje
//! dial z definicji, wygląda poprawnie do chwili, w której człowiek każe porównać kopie swojemu
//! najmocniejszemu agentowi.
//!
//! # Czego to nie dowodzi
//!
//! Że człowiek te zdania zobaczy. Tamtej połowy — kliknięcie w wierszu, odpowiedź w komórce
//! TEJ pozycji, status, który się nie rusza — dowodzi
//! `e2e/tests/an-agent-compares-the-copies.spec.ts`, w prawdziwym chromium. To nie jest ten sam
//! test i jeden nie zastępuje drugiego (niezmiennik 29).
//!
//! # Kanał
//!
//! `AgentDriver::start` bierze `mpsc::Sender<DecodedEvent>` i pcha w niego zdarzenia. Ta droga
//! nie pokazuje ani jednej z tych linii — widok strumienia ma jednego właściciela
//! (niezmiennik 13) — ale MUSI je odebrać: kanał bez odbiorcy staje na pełnym buforze i tura
//! nigdy się nie kończy. Dubler wysyła więc więcej zdarzeń, niż mieści kolejka, i każde
//! z limitem czasu: bieg, który wisi, jest dla bramki „nic się nie uruchomiło" (rc 124),
//! a nie czerwienią.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::Drivers;
use loadout_lib::commands::import::{CompareOutcome, Comparing, compare_copies_inner};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::import::{ImportStatus, translate};
use loadout_lib::library::agents::{Vendor, read_agent_file};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora, który ma sterownik. Ta sama, którą niesie `runsWith` w pliku fikstury.
const VENDOR: &str = "claude-code";

/// Identyfikator zapisanego agenta. Porównanie dostaje `id`, nie nazwę pliku: nazwa pliku
/// powstaje ze zmiennej nazwy agenta, a `id` przeżywa zmianę nazwy (T4 §5.1).
const WORK_FREELY_ID: &str = "01990000-0000-7000-8000-00000000c001";

/// Agent zapisany z **najwyższym** dialem. To on jest treścią kryterium o polityce: odpowiedź
/// wraca strumieniem, więc do pisania po dysku nie ma powodu, a dial wolno tylko obniżyć (D6).
const WORK_FREELY: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000c001
name: Forge
summary: Writes code
color: clay
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
Write the smallest change that makes the checks pass.
";

/// Dwie kopie tego samego skilla, które się RÓŻNIĄ — czyli dokładnie ten wiersz, o którym
/// adapter mówi „This skill has different copies. Let an agent compare them before import."
const AUDIT_HERE: &str = "---\nname: audit\ndescription: Audit it.\n---\nRead the code.";
const AUDIT_THERE: &str = "---\nname: audit\ndescription: Audit it.\n---\nRead the tests too.";

/// Zdanie, którego nie ma w żadnej z kopii i którego nie ma w promptcie: kontrola przeciw
/// asercji, która przechodzi na czymkolwiek.
const NOWHERE: &str = "Read the release notes.";

/// Ścieżki obu kopii wewnątrz skanowanego projektu.
const HERE: &str = ".agents/skills/audit/SKILL.md";
const THERE: &str = ".claude/skills/audit/SKILL.md";

/// Cel, po którym poznajemy scalony wiersz. Ta sama tożsamość, którą liczy `translate`.
const TARGET: &str = "skills/audit/SKILL.md";

/// To, co „powiedział agent". Ostatnia linia nazywa jedną z pokazanych ścieżek.
const MODEL_TEXT: &str = concat!(
    "The copy in .agents reads the code and nothing else. The copy in .claude also reads the\n",
    "tests, so it says more about what already works.\n",
    "\n",
    "I would keep .claude/skills/audit/SKILL.md, because it covers both.\n",
);

/// Ile zdarzeń dubler wysyła, zanim odda turę. Liczba jest większa niż jakakolwiek rozsądna
/// pojemność kanału z rozmysłem: w produkcji agent robiący `find /usr/share` sypie 121 000
/// linii na sekundę, a pętla kończy się na pierwszym zdarzeniu, które nie ma gdzie wejść.
const EVENTS: usize = 2_000;

/// Ile dubler czeka na miejsce w kanale, zanim uzna, że nikt go nie słucha. Zegar testu jest
/// zatrzymany (`start_paused`), więc to czekanie nie kosztuje ani jednej prawdziwej sekundy.
const PATIENCE: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_copies_travel_in_the_question_and_nothing_else_is_opened() {
    let world = World::new();
    let asked = ask(&world).await;

    let specs = asked.watch.specs();
    assert_eq!(
        specs.len(),
        1,
        "comparing the copies has to reach AgentDriver::start exactly once. Zero is the state \
         this task exists to end -- the screen has been printing \"Let an agent compare them \
         before import\" at seventeen rows and calling nobody. More than one means the person \
         pays twice for one question"
    );
    let spec = &specs[0];

    // Kontrola przeciw pustej asercji: obie kopie NAPRAWDĘ różnią się treścią, więc „prompt
    // niesie obie" jest pytaniem o dwie różne rzeczy, a nie o jedną napisaną dwa razy.
    assert_ne!(
        AUDIT_HERE, AUDIT_THERE,
        "the fixture stopped exercising the one case this whole path exists for: two copies \
         that differ"
    );
    assert!(
        spec.prompt.contains("Read the code."),
        "the question does not carry what the first copy says, so the agent is being asked to \
         compare something it has not been shown. It got: {:?}",
        spec.prompt
    );
    assert!(
        spec.prompt.contains("Read the tests too."),
        "the question does not carry what the second copy says. One copy in the prompt is not \
         a comparison, it is a summary"
    );
    assert!(
        spec.prompt.contains(HERE) && spec.prompt.contains(THERE),
        "the question does not name the files it quotes, so nothing the agent says can be \
         pinned to a copy the person can see on screen"
    );
    assert!(
        !spec.prompt.contains(NOWHERE),
        "the prompt carries text that is in neither copy, so the assertions above prove nothing \
         about where its content came from"
    );

    assert_eq!(
        spec.policy,
        Policy::ReadOnly,
        "comparing copies asked for {:?} on behalf of an agent saved as work-freely. The answer \
         is a stream of text about two files that already travelled inside the question, so \
         there is nothing to write -- and a pass-through may only LOWER the safety dial (D6)",
        spec.policy
    );
    assert!(
        !spec.reaches_the_web,
        "the agent comparing two local files was given the internet. Nobody on this screen \
         asked for that, and the screen promises the opposite before the scan even runs"
    );
    assert!(
        spec.extra_dirs.is_empty(),
        "the turn was given directories to read: {:?}. Everything it needs is in the question; \
         a folder in reach is a folder it may open, and \"Scan reads setup files only\" stops \
         being true the moment one is there",
        spec.extra_dirs
    );
    assert!(
        !spec.cwd.starts_with(world.project()),
        "the turn works inside the scanned project ({:?}), which hands a read-only agent the \
         whole of somebody else's repository while it was asked about two files",
        spec.cwd
    );
    assert!(
        !asked.watch.stalled(),
        "the driver ran out of room in the event channel, so this path is not receiving events. \
         An unreceived channel stops at its buffer and the turn never ends"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_answer_names_both_files_and_keeps_the_choice_with_the_person() {
    let world = World::new();
    let asked = ask(&world).await;

    let Ok(CompareOutcome::Compared(compared)) = &asked.compared else {
        // Bez `panic!`: `clippy::panic` jest `deny` i sądzi także `--all-targets`.
        unreachable!(
            "a turn that finished with prose about two copies has to come back as a comparison. \
             It came back as {:?}",
            asked.compared
        )
    };

    assert_eq!(
        compared.compared,
        vec![PathBuf::from(HERE), PathBuf::from(THERE)],
        "the answer has to name every file it read, in the order it read them. A row that comes \
         out of this knowing less about where it came from than it knew before is the one thing \
         this path may not do"
    );
    assert_eq!(
        compared.said,
        MODEL_TEXT.trim(),
        "what the agent said travels word for word. A summary written here is a second voice in \
         a place where the person is weighing somebody's actual sentences"
    );
    assert_eq!(
        compared.keep.as_deref(),
        Some(Path::new(THERE)),
        "the agent named one of the two paths it was shown, so the suggestion is readable from \
         its own prose -- and it has to be one of THOSE paths, never a third one invented here"
    );

    // GRANICA: agent doradza, decyduje człowiek. Ta sama pozycja, przeczytana jeszcze raz tą
    // samą drogą, dalej czeka na rozstrzygnięcie -- porównanie niczego nie zaimportowało
    // i niczego nie rozstrzygnęło (AGENTS.md §2).
    let after = translate::preview(world.project()).unwrap();
    let row = after
        .draft
        .items
        .iter()
        .find(|item| item.target.as_deref() == Some(Path::new(TARGET)))
        .expect("the audit skill left the plan");
    assert_eq!(
        row.status,
        ImportStatus::NeedsChoice,
        "the item stopped needing a choice after an agent looked at it, so the screen now claims \
         a decision that nobody made. A second opinion is advice; the person imports"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stop_comes_back_as_cancelled_only_with_a_proof_that_the_group_died() {
    let world = World::new();
    let stopped = stop_a_turn_that(Ending::Dies, &world).await;

    assert_eq!(
        stopped,
        Ok(CompareOutcome::Cancelled),
        "Stop with a proof of death is a VALUE, never an error (invariant 7). Err(Cancelled) \
         forces every caller to tell \"this failed\" from \"a person stopped it\", and a \
         distinction lost once is lost everywhere. It came back as {stopped:?}"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stop_without_a_proof_says_the_agent_may_still_be_running() {
    let world = World::new();
    let stopped = stop_a_turn_that(Ending::Lingers, &world).await;

    let said = stopped.err().unwrap_or_default();
    assert!(
        said.contains("may still be running"),
        "the group still answers signal zero, so Loadout has no proof it died -- and until \
         there is one, it is alive (invariant 6). Silence is the most expensive answer here: an \
         orphaned agent reads on and the person pays for it, which is a money bug, not a \
         hygiene one. It said: {said:?}"
    );
}

// ── jak zadajemy pytanie ───────────────────────────────────────────────────────────────────

/// Jedno porównanie na tym projekcie: wynik plus to, co zobaczył sterownik.
#[derive(Debug)]
struct Asked {
    compared: Result<CompareOutcome, String>,
    watch: Arc<Watch>,
}

async fn ask(world: &World) -> Asked {
    let watch = Arc::new(Watch::default());
    let drivers = drivers_for(Arc::clone(&watch), Ending::Finishes);
    let comparing = Comparing::new();
    let compared = compare_copies_inner(
        &world.library,
        world.personal(),
        &drivers,
        &comparing,
        world.project(),
        &world.merged_row_id(),
        WORK_FREELY_ID,
    )
    .await;
    Asked { compared, watch }
}

/// Jedno porównanie zatrzymane przez człowieka w trakcie tury.
///
/// Zegar testu jest zatrzymany (`start_paused`), a tokio przesuwa go do najbliższego timera
/// dopiero wtedy, kiedy nie ma nic gotowego do wykonania — więc sen po stronie testu wypada
/// dokładnie w chwili, w której tura stoi już na `handle.wait()`. Nie kosztuje to ani jednej
/// prawdziwej sekundy, a Stop trafia w turę, która NAPRAWDĘ trwa.
///
/// Tura, której nikt nie zatrzyma, śpi [`LINGER`] — dłużej niż limit czasu agenta, więc
/// implementacja, która Stopa nie odbiera, kończy się NAZWANĄ odmową o limicie, a nie
/// zawieszeniem. Bieg, który wisi, jest dla bramki „nic się nie uruchomiło", nie czerwienią.
async fn stop_a_turn_that(ending: Ending, world: &World) -> Result<CompareOutcome, String> {
    let watch = Arc::new(Watch::default());
    let drivers = drivers_for(Arc::clone(&watch), ending);
    let comparing = Comparing::new();
    let row = world.merged_row_id();

    let asking = compare_copies_inner(
        &world.library,
        world.personal(),
        &drivers,
        &comparing,
        world.project(),
        &row,
        WORK_FREELY_ID,
    );
    let pressing = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        comparing.stop();
    };
    let (compared, ()) = tokio::join!(asking, pressing);
    compared
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

/// Biblioteka użytkownika i cudzy projekt obok niej, na czas jednego testu.
struct World {
    tmp: TempDir,
    /// `~/.loadout`. Katalog domowy jest jego RODZICEM, więc biblioteka nie może leżeć wprost
    /// w katalogu tymczasowym.
    library: PathBuf,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let library = tmp.path().join(".loadout");
        fs::create_dir_all(library.join("agents")).unwrap();

        let world = Self { tmp, library };
        let saved = world.library.join("agents").join("forge.md");
        fs::write(&saved, WORK_FREELY).unwrap();
        // Przesłanka, nie kryterium: definicja, której biblioteka nie umie przeczytać, daje
        // czerwień nie do odróżnienia od braku zachowania.
        assert!(
            read_agent_file(&saved).is_ok(),
            "the fixture agent cannot be read back by the library, so this criterion could never \
             pass: {:?}",
            read_agent_file(&saved).err().map(|error| error.to_string())
        );

        // Cudzy projekt: ta sama rzecz w dwóch aplikacjach, z RÓŻNĄ treścią. Wzorzec fikstury
        // jest ten sam, co w `import_one_thing_is_one_row.rs`.
        write(world.project(), HERE, AUDIT_HERE);
        write(world.project(), THERE, AUDIT_THERE);
        world
    }

    /// Katalog cudzego repozytorium — ten, który człowiek wpisuje w pole „Project folder".
    fn project(&self) -> &Path {
        self.tmp.path()
    }

    /// Katalog domowy człowieka, z którego czyta się jego własne zakresy MCP.
    ///
    /// Katalog tymczasowy, nie prawdziwy `HOME`: zestaw testowy nie ma prawa czytać
    /// konfiguracji tego, kto akurat uruchomił testy.
    fn personal(&self) -> &Path {
        self.tmp.path()
    }

    /// Identyfikator scalonego wiersza obu kopii, policzony tą samą drogą, którą liczy go
    /// ekran — nie wpisany tu z pamięci.
    fn merged_row_id(&self) -> String {
        let preview = translate::preview(self.project()).unwrap();
        let row = preview
            .draft
            .items
            .iter()
            .find(|item| item.target.as_deref() == Some(Path::new(TARGET)))
            .expect("the audit skill is not in the plan, so there is no row to compare");
        assert_eq!(
            row.status,
            ImportStatus::NeedsChoice,
            "the fixture stopped producing a row that waits for a choice, so this whole file \
             would be asking about a decision that does not exist"
        );
        row.id.clone()
    }
}

fn write(root: &Path, path: &str, content: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

// ── dubler sterownika ──────────────────────────────────────────────────────────────────────

/// Co dostał sterownik i czy kanał się zatkał.
#[derive(Debug, Default)]
struct Watch {
    specs: Mutex<Vec<RunSpec>>,
    stalled: Mutex<bool>,
}

impl Watch {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn started(&self, spec: RunSpec) {
        self.specs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec);
    }

    fn specs(&self) -> Vec<RunSpec> {
        self.lock().clone()
    }

    fn stalls(&self) {
        *self.stalled.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }

    fn stalled(&self) -> bool {
        *self.stalled.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock(&self) -> MutexGuard<'_, Vec<RunSpec>> {
        // Zatruty zamek nie ma prawa zgubić pomiaru: panika w jednej turze oślepiłaby asercję,
        // która akurat dowodzi, co ta tura dostała.
        self.specs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Jak kończy tura atrapy — trzy stany, które ta droga musi umieć rozróżnić.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// Tura wraca sama, z prozą o dwóch kopiach.
    Finishes,
    /// Tura trwa, a po Stopie grupa schodzi i jest na to dowód.
    Dies,
    /// Tura trwa, a po Stopie grupa nadal odpowiada na sygnał zerowy.
    Lingers,
}

/// Ile śpi tura, której nikt nie zatrzyma. Dłużej niż limit czasu agenta z fikstury, żeby
/// implementacja gubiąca Stop padała na odmowie o limicie, a nie wisiała.
const LINGER: Duration = Duration::from_hours(24);

/// Fabryka: atrapa dla vendora, który ma sterownik, i ta sama atrapa dla drugiego — tożsamość
/// vendora nie jest pytaniem tego pliku, a `Vendor` jest enumem zamkniętym.
fn drivers_for(watch: Arc<Watch>, ending: Ending) -> Drivers {
    let fake: Arc<dyn AgentDriver> = Arc::new(Fake { watch, ending });
    Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode | Vendor::Codex => Arc::clone(&fake),
    })
}

/// Atrapa sterownika. Mieszka w pliku testu, bo `engine::drivers::fake` jest dublerem PLANISTY
/// i nie implementuje tego traitu.
#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
    ending: Ending,
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
        self.watch.started(spec);
        Ok(Box::new(Turn {
            watch: Arc::clone(&self.watch),
            events,
            session,
            ending: self.ending,
        }))
    }
}

/// Jedna tura atrapy: zalew zdarzeń, potem proza o dwóch kopiach — albo czekanie na Stop.
#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    ending: Ending,
}

impl Turn {
    /// Wysyła jedno zdarzenie. `false` znaczy „dalej nie ma po co" — kanał zamknięty albo zatkany.
    async fn push(&self, event: AgentEvent) -> bool {
        match tokio::time::timeout(PATIENCE, self.events.send(event.into())).await {
            Ok(Ok(())) => true,
            // Odbiorca zniknął: zdarzenia nie mają gdzie iść, ale nikt nie wisi.
            Ok(Err(_)) => false,
            Err(_) => {
                self.watch.stalls();
                false
            }
        }
    }
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
        for index in 0..EVENTS {
            let line = AgentEvent::Said {
                text: format!("line {index}"),
            };
            if !self.push(line).await {
                break;
            }
        }

        if self.ending != Ending::Finishes {
            // Tura, którą człowiek zaraz zatrzyma. `one_turn` porzuci tę przyszłość na `select!`,
            // więc sen nigdy się nie kończy — i o to chodzi.
            tokio::time::sleep(LINGER).await;
        }

        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            // Dokładnie końcowa wypowiedź modelu i nic poza nią.
            text: MODEL_TEXT.to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        };
        let _ = self.push(AgentEvent::Finished(outcome.clone())).await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        match self.ending {
            Ending::Lingers => GroupProof::Alive {
                group: self.group(),
            },
            Ending::Dies | Ending::Finishes => GroupProof::Dead { status: None },
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
