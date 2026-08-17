//! AC-2 dla T-28: po anulowaniu biegu dwóch **prawdziwych** agentów nie zostaje ani jeden żywy
//! proces potomny.
//!
//! To jest druga połowa szkieletu chodzącego. Pierwsza (`skeleton_two_real_agents.rs`) dowodzi,
//! że dwa procesy naprawdę pracują naraz; ta dowodzi, że po Stopie naprawdę nie ma ich już
//! wcale — a to jest błąd **finansowy**, nie higieniczny: osierocony `claude` pali limit
//! u dostawcy w tle, przy statusie dziecka mówiącym „zabity" i przy zielonym teście
//! [T7 §3.1: `A after kill: total=2 orphaned=2`].
//!
//! **Słaba wersja tego kryterium:** sprawdzenie, że `cancel()` zwróciło `Ok`, albo że zadanie
//! tokio się zakończyło. Anulowanie zadania Rusta **nie zabija procesu systemowego** — to jest
//! dokładnie ta różnica, która zostawia osieroconego agenta (niezmienniki 6 i 10). Dlatego
//! mierzy tu system operacyjny, a nie nasz kod: `kill(-pgid, 0)` musi oddać `ESRCH`.
//!
//! **Kontrola przeciw pustej asercji jest częścią kryterium, nie ostrożnością.** Ta sama sonda
//! **przed** anulowaniem musi oddać sukces. Bez niej `ESRCH` znaczy „ten proces nigdy nie
//! istniał" i test przechodzi na biegu, w którym nic nie wstało — czyli dokładnie ten rodzaj
//! zieleni, dla którego to zadanie w ogóle powstało.
//!
//! Anulujemy w chwili, w której oba procesy już stoją, a tury jeszcze trwają. To jest tańsze
//! niż czekanie na koniec tury i **mocniejsze**: bieg zatrzymany w pół słowa jest tym stanem,
//! w którym sieroty zostają.
//!
//! **Ten test kosztuje pieniądze i na maszynie bez `claude` na PATH jest czerwony.** Decyzja
//! człowieka z 2026-08-16, spisana w `TASK.md`.

use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use loadout_lib::engine::StepId;
use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentEvent, Policy, RunSpec};
use loadout_lib::engine::scheduler::execute;
use loadout_lib::engine::step::{StepReport, StepState};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Ile kroków ma naprawdę działać naraz.
const AT_ONCE: usize = 2;

/// Sufit na start procesu.
const START_LIMIT: Duration = Duration::from_mins(1);

/// Ile czekamy, aż oba kroki naprawdę ruszą — czyli aż CLI ogłosi swoją sesję.
const LIVE_LIMIT: Duration = Duration::from_mins(2);

/// Odstęp między pytaniami „czy oba kroki już stoją".
const POLL: Duration = Duration::from_millis(10);

/// Sufit na cały bieg liczony od Stopu. Eskalacja to SIGTERM, okno łaski i dopiero SIGKILL —
/// wolno jej trwać, ale nie wolno jej trwać bez końca: zawieszenie daje bramce rc 124, a to
/// jest fałszywa czerwień, nie dowód.
const SETTLE_LIMIT: Duration = Duration::from_mins(1);

/// Ile zdarzeń mieści się w kanale sterownika, zanim ten zaczeka.
const EVENTS: usize = 256;

/// Treść zadania. Ta sama co w AC-1: jeden obrót, jedno słowo, żadnych narzędzi. Prompt jedzie
/// wyłącznie stdinem (niezmiennik 9).
const PROMPT: &str = "Reply with the single word: ready. Do not use any tools.";

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku, a `claude` na tej
/// maszynie jest skryptem powłoki, który odpala Node — model biegnie we **wnuku** [T7 §3.1].
// 2026-08-16 — `kill(2)` nie ma bezpiecznego opakowania w std. Plik testowy jest wyłączony ze
// wszystkich trzech granic architektury po ŚCIEŻCE (checks/quick-boundary.sh) i nie jest
// częścią wysyłanego artefaktu — a ten test z definicji pyta system operacyjny zamiast naszego
// kodu (niezmiennik 20). Ten sam kształt stoi w `supervisor_group_death.rs`.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza tylko istnienie i prawa.
    // Argumenty to zwykłe liczby, więc nie ma tu żadnego wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Co krok zdążył powiedzieć o sobie, zanim przyszedł Stop.
#[derive(Debug, Clone)]
struct Ran {
    /// Proces i jego grupa — to po niej pytamy jądro.
    group: GroupId,
    /// Katalog roboczy, który dostał ten krok.
    cwd: PathBuf,
    /// Co nadzór powiedział o grupie po anulowaniu. **Nie jest dowodem** — dowodem jest sonda
    /// niżej. Trzymamy to, żeby czerwony test umiał powiedzieć, czy sterownik w ogóle uważa,
    /// że posprzątał.
    proof: Option<String>,
}

/// Dwa kroki, dwa katalogi i miejsce, w którym meldują swoje grupy procesów.
#[derive(Debug)]
struct Bench {
    /// Prawdziwe CLI z `PATH`. Atrapa jest dokładnie tym, czego to kryterium nie przyjmuje.
    driver: ClaudeDriver,
    /// Katalog roboczy każdego kroku.
    dirs: Vec<PathBuf>,
    /// Co krok zameldował. `None`, dopóki jego proces nie wstał.
    ran: Mutex<Vec<Option<Ran>>>,
    /// Czy CLI kroku ogłosiło już swoją sesję (`system/init`).
    ///
    /// Osobno od [`Bench::ran`], a nie polem w nim, bo te dwa fakty przychodzą **z dwóch
    /// stron**: grupę oddaje `start()`, a ogłoszenie sesji pętla czytająca — i potrafi
    /// przyjść pierwsze.
    announced: Mutex<Vec<bool>>,
    /// Powody, dla których krok nie doszedł tam, gdzie miał dojść.
    why: Mutex<Vec<String>>,
}

impl Bench {
    /// Ławka na tyle kroków, ile katalogów.
    fn new(dirs: Vec<PathBuf>) -> Self {
        let steps = dirs.len();
        Self {
            driver: ClaudeDriver::new(),
            dirs,
            ran: Mutex::new(vec![None; steps]),
            announced: Mutex::new(vec![false; steps]),
            why: Mutex::new(Vec::new()),
        }
    }

    /// Zamek, który nie panikuje: panika w zadaniu planisty nie wraca ze swoim numerem kroku
    /// i zamienia czerwień z powodem w czerwień bez powodu.
    fn lock<T>(guarded: &Mutex<T>) -> MutexGuard<'_, T> {
        guarded.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Zapisuje, co poszło nie tak.
    fn blame(&self, id: StepId, reason: &str) {
        Self::lock(&self.why).push(format!("step {id}: {reason}"));
    }

    /// Wszystkie powody, jednym zdaniem.
    fn complaints(&self) -> String {
        let why = Self::lock(&self.why);
        if why.is_empty() {
            "no step said why".to_owned()
        } else {
            why.join(" · ")
        }
    }

    /// Zapisuje, że CLI tego kroku ogłosiło swoją sesję.
    fn announce(&self, id: StepId) {
        Self::lock(&self.announced)[id] = true;
    }

    /// Co nadzór powiedział o tej grupie — odczytane **po** biegu.
    ///
    /// Osobnym czytaniem, a nie z migawki sprzed Stopu: tamta z definicji ma tu `None`, bo
    /// dowód powstaje dopiero w anulowaniu. Migawka w komunikacie asercji mówiłaby wtedy
    /// „nadzór nie powiedział nic" o każdym biegu, także o tym, w którym powiedział wszystko.
    fn proof_for(&self, pgid: i32) -> Option<String> {
        Self::lock(&self.ran)
            .iter()
            .flatten()
            .find(|ran| ran.group.pgid == pgid)
            .and_then(|ran| ran.proof.clone())
    }

    /// Kroki, które **naprawdę ruszyły**: mają swoją grupę procesów i ogłosiły sesję.
    ///
    /// Oba warunki naraz, i to jest cała różnica między dowodem a jego pozorem. Sama grupa jest
    /// znana kilka milisekund po `spawn`, kiedy `claude` jest jeszcze skryptem powłoki tuż
    /// przed odpaleniem Node — zabicie go w tej chwili dowodzi, że umiemy zabić skrypt, a nie
    /// że umiemy zabić **wnuka**, który jako jedyny przeżył pomiar `total=2 orphaned=2`
    /// [T7 §3.1]. `system/init` jest pierwszą chwilą, w której wiadomo, że model naprawdę
    /// stoi po drugiej stronie.
    fn live(&self) -> Vec<Ran> {
        let announced = Self::lock(&self.announced).clone();
        Self::lock(&self.ran)
            .iter()
            .enumerate()
            .filter(|(id, _)| announced[*id])
            .filter_map(|(_, ran)| ran.clone())
            .collect()
    }

    /// Czeka, aż **wszystkie** kroki naprawdę ruszą.
    ///
    /// Zwraca to, co zobaczyła, także wtedy, gdy jest tego za mało — asercja wołającego ma
    /// powiedzieć, czego zabrakło, a nie paść na czekaniu.
    async fn all_live(&self, want: usize, limit: Duration) -> Vec<Ran> {
        let deadline = Instant::now() + limit;
        loop {
            let live = self.live();
            if live.len() >= want || Instant::now() >= deadline {
                return live;
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Jeden krok: wstaje, melduje swoją grupę i **czeka na Stop od środka**.
    ///
    /// Token widzi sam krok i to jest warunek konieczny, nie wygoda: zdjęcie zadania Ruska
    /// z zewnątrz (`JoinSet::abort_all`, `tokio::time::timeout` wokół kroku) wraca równie
    /// szybko i wygląda tak samo, a po drugiej stronie zostawia żywą grupę procesów palącą
    /// limit u dostawcy (niezmienniki 6 i 10).
    async fn step(self: Arc<Self>, id: StepId, cancel: CancellationToken) -> StepReport {
        let cwd = self.dirs[id].clone();

        let (tx, mut inbox) = mpsc::channel(EVENTS);
        // Kanał musi być OPRÓŻNIANY: pętla czytająca sterownika zatrzymuje się na pełnym
        // buforze, a zatrzymana pętla wygląda dokładnie jak zawieszony agent. Przy okazji to
        // jest jedyne miejsce, z którego widać `system/init` — czyli chwilę, w której agent
        // naprawdę stoi.
        let watcher = Arc::clone(&self);
        let _drain = tokio::spawn(async move {
            while let Some(event) = inbox.recv().await {
                if matches!(event, AgentEvent::Started { .. }) {
                    watcher.announce(id);
                }
            }
        });

        let spec = RunSpec {
            run_id: Uuid::now_v7(),
            cwd: cwd.clone(),
            prompt: PROMPT.to_owned(),
            model: None,
            system_append: None,
            policy: Policy::ReadOnly,
            extra_dirs: Vec::new(),
            resume: None,
        };

        let mut handle = match timeout(START_LIMIT, self.driver.start(spec, tx)).await {
            Ok(Ok(handle)) => handle,
            Ok(Err(error)) => {
                self.blame(id, &format!("the agent would not start: {error}"));
                return StepReport::Failed;
            }
            Err(_elapsed) => {
                self.blame(
                    id,
                    &format!("the agent did not start within {START_LIMIT:?}"),
                );
                return StepReport::Failed;
            }
        };

        // Grupa jest znana ZANIM cokolwiek popłynie ze stdout [T7 §6.2] i meldujemy ją od razu:
        // po anulowaniu nie ma już kogo o nią zapytać, a to ona jest tu jedynym adresem, pod
        // który da się zapukać z zewnątrz.
        let Some(group) = handle.group() else {
            self.blame(id, "the agent ran without a process group");
            return StepReport::Failed;
        };
        {
            let mut ran = Self::lock(&self.ran);
            ran[id] = Some(Ran {
                group,
                cwd,
                proof: None,
            });
        }

        let finished = {
            let waiting = handle.wait();
            tokio::pin!(waiting);
            tokio::select! {
                // `biased`, bo tura, która właśnie się skończyła, ma pierwszeństwo przed Stopem
                // wpadającym w tej samej chwili.
                biased;
                done = &mut waiting => Some(done),
                () = cancel.cancelled() => None,
            }
            // Pożyczka uchwytu kończy się razem z tym blokiem — dopiero po nim wolno zawołać
            // `cancel()` na tym samym uchwycie.
        };

        let Some(done) = finished else {
            // ANULOWANIE IDZIE PRZEZ STEROWNIK. To jest ta jedna droga, która wraca
            // z `GroupProof`, a nie z „wysłałem sygnał" (niezmiennik 6).
            let proof = handle.cancel().await;
            let dead = matches!(proof, GroupProof::Dead { .. });
            {
                let mut ran = Self::lock(&self.ran);
                if let Some(entry) = ran[id].as_mut() {
                    entry.proof = Some(format!("{proof:?}"));
                }
            }
            if !dead {
                self.blame(id, "the supervisor could not prove the group was gone");
            }
            return StepReport::Cancelled;
        };

        // Tura zdążyła się skończyć przed Stopem. To nie jest awaria testu, tylko bieg, który
        // niczego nie dowodzi — asercja niżej powie to wprost, bo grupa będzie już martwa
        // z własnej woli.
        match done {
            Ok(_turn) => {
                self.blame(
                    id,
                    "the turn finished before the run was stopped, so this step had nothing \
                     left to kill",
                );
                StepReport::Succeeded
            }
            Err(error) => {
                self.blame(id, &format!("the turn ended without a result: {error}"));
                StepReport::Failed
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_two_real_agents_leaves_no_live_process_behind() -> Result<(), Box<dyn Error>> {
    let first_dir = tempfile::tempdir()?;
    let second_dir = tempfile::tempdir()?;
    let bench = Arc::new(Bench::new(vec![
        first_dir.path().to_path_buf(),
        second_dir.path().to_path_buf(),
    ]));

    // Token TEGO biegu, nigdy globalny `AtomicBool`: bool przecieka między biegami
    // (niezmiennik 7).
    let token = CancellationToken::new();
    let run = Arc::clone(&bench);
    let cancel = token.clone();
    let runner = tokio::spawn(async move {
        // Graf powstaje w zadaniu, bo `execute` pożycza go na cały bieg.
        let dag = Dag::new(2, &[]).map_err(|error| error.to_string())?;
        let outcome = execute(&dag, AT_ONCE, cancel, move |id, token| {
            Arc::clone(&run).step(id, token)
        })
        .await;
        Ok::<_, String>(outcome)
    });

    // ── Oba procesy naprawdę stoją, zanim cokolwiek zabijemy ──────────────────────────────
    let live = bench.all_live(2, LIVE_LIMIT).await;
    assert_eq!(
        live.len(),
        2,
        "both agents have to be really under way before the stop — process group taken AND \
         session announced. Otherwise the whole proof below runs on an empty set and ESRCH \
         means 'this process never existed' — {}",
        bench.complaints()
    );
    assert_ne!(
        live[0].group.pgid, live[1].group.pgid,
        "each agent has to lead its own process group; one group for both means stopping one \
         of them signals the other, and neither proof says what it seems to say"
    );
    assert_ne!(
        live[0].cwd, live[1].cwd,
        "each agent works in its own folder (invariant 12)"
    );

    // ── KONTROLA DODATNIA: ta sama sonda, przed anulowaniem, musi oddać sukces ─────────────
    for step in &live {
        group_probe(step.group.pgid).map_err(|error| {
            format!(
                "kill(-{}, 0) has to find a live group BEFORE the stop, otherwise the ESRCH \
                 below proves nothing at all: it would only mean this process never existed. \
                 The kernel answered {error}",
                step.group.pgid
            )
        })?;
    }

    // ── Stop ──────────────────────────────────────────────────────────────────────────────
    token.cancel();
    let outcome = timeout(SETTLE_LIMIT, runner).await.map_err(|_elapsed| {
        format!("the run did not come back within {SETTLE_LIMIT:?} of the stop")
    })???;

    // ── DOWÓD: dla każdej grupy jądro musi odpowiedzieć ESRCH ─────────────────────────────
    for step in &live {
        let answer = group_probe(step.group.pgid);
        let errno = answer.err().and_then(|error| error.raw_os_error());
        assert_eq!(
            errno,
            Some(libc::ESRCH),
            "kill(-{}, 0) still finds somebody in this agent's group after the run came back \
             from a stop. That is the measurement which returned total=2 orphaned=2 in T7 3.1 \
             while the child's own exit status said 'killed', and an orphaned agent burns quota \
             in the background — a financial bug, not a hygiene one (invariant 6). The \
             supervisor said {:?} about this group",
            step.group.pgid,
            bench.proof_for(step.group.pgid)
        );
    }

    // Kontekst, nie dowód: bieg zatrzymany przez człowieka ma się czytać jako zatrzymany,
    // a nie jako cudza awaria (niezmiennik 7, ARCHITECTURE §5).
    assert!(
        outcome.cancelled,
        "a run stopped by a person has to come back as cancelled; it came back as {:?} — {}",
        outcome.states,
        bench.complaints()
    );
    assert!(
        outcome
            .states
            .iter()
            .all(|state| *state == StepState::Cancelled),
        "every step of a stopped run is cancelled, never skipped: skipped means somebody \
         upstream failed and the screen would blame the wrong thing. They ended as {:?} — {}",
        outcome.states,
        bench.complaints()
    );

    Ok(())
}
