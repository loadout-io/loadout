//! Krok mówi „done" dopiero po tym, jak jego grupa procesów odpowiedziała `ESRCH`.
//!
//! Niezmiennik 6 czyta się dosłownie: **dopóki `kill(-pgid, 0)` nie dał `ESRCH`, grupa jest
//! żywa.** Ścieżka Stopu i limitu czasu miała ten dowód od T-03; ścieżka **udana** nie miała go
//! ani razu. `close()` zamyka wejście i zbiera LIDERA — a wnuka nie widzi żaden nasz `wait()`,
//! bo wnuk nie jest naszym dzieckiem [T7 §3.1: `total=2 orphaned=2` przy statusie dziecka
//! mówiącym „zabity"]. Kafelek zapalał się więc na „done" nad grupą, o którą nikt nie zapytał
//! jądra, i to jest błąd finansowy, nie higieniczny: osierocony agent pali limit w tle.
//!
//! # Słaba wersja tego kryterium: „bieg kończy się sukcesem"
//!
//! Przechodzi na implementacji, która melduje `succeeded` w chwili powrotu z `close()`.
//! Rozstrzyga **kolejność dwóch chwil**, i pytają o nią trzy asercje:
//!
//! * kontrola dodatnia — zanim lider wyjdzie, ta sama sonda MUSI oddać sukces. Bez niej `ESRCH`
//!   na końcu znaczy równie dobrze „procesu nigdy nie było", a całe kryterium przechodzi na
//!   pustym zbiorze;
//! * w chwili, w której linia `succeeded` dla kroku wychodzi z kolejki biegu do okna, sonda ma
//!   oddać `ESRCH`;
//! * punkt kontrolny za tym krokiem nie ma prawa zapytać człowieka, dopóki sonda widzi w grupie
//!   kogokolwiek — planista zdejmuje stopień wejściowy potomkom wyłącznie po `succeeded`, więc
//!   pytanie na ekranie jest drugim, niezależnym świadkiem tej samej bramy.
//!
//! **Wnuk ignoruje SIGTERM** (`trap '' TERM`) i to jest cała konstrukcja tego pomiaru. Proces,
//! który ginie od pierwszego sygnału, ginie w mikrosekundach — wtedy „wróciło po sygnale"
//! i „wróciło po dowodzie" wypadają w tej samej milisekundzie. Z ignorowanym TERM-em żaden
//! uczciwy dowód nie może przyjść przed końcem okna łaski i eskalacją do dziewiątki, więc okno,
//! w którym punkt kontrolny musi milczeć, jest prawdziwym oknem, a nie zaokrągleniem.
//!
//! # Dlaczego nadzór mieszka w pudełku, a nie w uchwycie dublera
//!
//! [`supervisor::Supervised`] zabija grupę także wtedy, gdy się go **porzuci** — gwardia w `Drop`
//! prowadzi dziewiątką bez łaski. Uchwyt sterownika ginie na końcu `one_turn`, czyli ZANIM krok
//! zapali swój stan na ekranie: dubler trzymający nadzór u siebie mierzyłby więc gwardię `Drop`,
//! a nie bramę, i przechodziłby także wtedy, gdy dowodu nie ma. `Drop` nie jest dowodem —
//! wysyła sygnał i nie pyta jądra o nic (niezmiennik 6). Dlatego nadzór trzyma tu **ława**,
//! a uchwyt tylko go pożycza: jedyną rzeczą, która może zabić wnuka, zostaje sama brama.
//!
//! # Co jeszcze stoi w tym pliku
//!
//! Dwa kryteria o drugiej połowie tej samej reguły, każde z własnym powodem wypisanym przy nim:
//! krok, którego grupy **nie da się** dowieść, kończy się `failed` ze zdaniem dla człowieka
//! i `death_proof: false` — nigdy „done"; a powtórzone zatrzymanie tej samej grupy nadal oddaje
//! `Dead`, tylko bez statusu.
//!
//! Testy odpalają prawdziwe procesy i **nie są** `#[ignore]`: cel z samymi pominiętymi testami
//! melduje „0 passed", a to nie jest dowód (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use loadout_lib::commands::run::{continue_run_inner, run_workflow_inner};
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof, StdinPlan, Supervised};
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Klucz kafelka z pliku workflow — po nim okno rozpoznaje swój kafelek, więc po nim rozpoznaje
/// go też ten test (`commands::run::Live::announce` wysyła `tile_key`).
const HAND: &str = "s_hand";

/// Punkt kontrolny za krokiem. Drugi świadek tej samej bramy.
const GATE: &str = "s_gate";

/// Okno łaski między SIGTERM a SIGKILL, podane argumentem zamiast wzięte ze stałej produkcyjnej
/// (`DEFAULT_GRACE` to pięć sekund). Jest zarazem tym, co rozdziela „wysłałem sygnał" od „mam
/// dowód": wnuk ignoruje TERM, więc grupa schodzi dopiero po tym oknie.
const GRACE: Duration = Duration::from_secs(1);

/// Ile czekamy na to, żeby krok w ogóle wystał sobie grupę procesów i zainstalował trap.
///
/// HOJNE Z ROZMYSŁEM, i to niczego nie osłabia: obie bariery, które tej stałej używają, są
/// PRZYGOTOWANIEM, a nie pomiarem. Ten test mierzy KOLEJNOŚĆ dwóch chwil, a ta nie zależy od
/// tego, jak długo wcześniej wstawała powłoka na obciążonej maszynie.
const START_LIMIT: Duration = Duration::from_mins(2);

/// Odstęp między pytaniami sondy. Krótki, bo mierzymy KOLEJNOŚĆ dwóch chwil, a nie czas.
const PROBE_POLL: Duration = Duration::from_millis(2);

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Pojemność kolejki linii. Ta ława wypuszcza kilkanaście wierszy, więc zapas jest formalnością.
const ROOMY: usize = 1_024;

/// Krok, którego **lider wychodzi zerem** i zostawia za sobą wnuka ignorującego SIGTERM.
///
/// Trzy rzeczy w tym skrypcie są konieczne, każda z osobnym powodem:
///
/// * `trap '' TERM` stoi PRZED plikiem gotowości — bez tej kolejności sygnał potrafi dotrzeć,
///   zanim trap się wykona, wnuk ginie wtedy akcją domyślną i test oskarża poprawną
///   implementację o powrót bez dowodu;
/// * lider czeka na plik zgody od testu i dopiero potem wychodzi. Kontrola dodatnia ma być
///   pomiarem, a nie wyścigiem z eskalacją, która zaczyna się w chwili końca tury;
/// * pętla krótkich snów, nigdy pojedyncza komenda: powłoka exec-optymalizuje ostatnią komendę
///   i znacznik znika wtedy z `argv`, a skan `ps` przestaje cokolwiek widzieć [T7 §8.2].
const LEAVES_A_SURVIVOR: &str = r#"#!/bin/sh
# $1 = plik gotowości wnuka, $2 = plik zgody na wyjście lidera, $3 = znacznik dla ps
(
  trap '' TERM
  : > "$1"
  while :; do
    sleep 0.2
  done
) &
while [ ! -f "$2" ]; do
  sleep 0.02
done
exit 0
"#;

/// Krok, który schodzi sam i natychmiast. Dla drugiego kryterium tego pliku.
const ENDS_AT_ONCE: &str = r"#!/bin/sh
exit 0
";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000002a1
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
skills: []
connections: []
---
Do the work.
";

/// `hand → gate`, gdzie `gate` jest kafelkiem kontrolnym („Ask me first").
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_done_needs_a_proof",
  "name": "A done step proves its group is dead",
  "steps": [
    {
      "kind": "agent",
      "id": "s_hand",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000002a1",
      "overrides": {},
      "instructions": "leave something running behind you",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "checkpoint",
      "id": "s_gate",
      "name": "Ask me first",
      "question": "Did everything it started stop?",
      "at": { "x": 240, "y": 0 }
    }
  ],
  "links": [{ "from": "s_hand", "to": "s_gate" }]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_says_done_only_after_its_process_group_answers_esrch() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("done-needs-a-proof", WORKFLOW)?;
    let script = write_script(
        bench.project.path(),
        "leaves-a-survivor.sh",
        LEAVES_A_SURVIVOR,
    )?;
    let marker = unique_marker("done-proof");
    let ready = bench.project.path().join("trap-installed");
    let go = bench.project.path().join("the-leader-may-go");
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    let started: Arc<Mutex<Option<GroupId>>> = Arc::new(Mutex::new(None));
    let keeper: Keeper = Arc::new(Mutex::new(None));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers_for(Arc::new(Fake {
            script,
            ready: ready.clone(),
            go: go.clone(),
            marker: marker.clone(),
            started: Arc::clone(&started),
            keeper: Arc::clone(&keeper),
        })),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(ROOMY);
    let watching = watch_the_wire(source, &started, &ready, &go, bench.project.path());
    let answering = async {
        let _paused = wait_until_paused(bench.project.path()).await?;
        continue_run_inner(&deps, None).await?;
        Ok::<(), Box<dyn Error>>(())
    };

    let (ran, seen, answered) = tokio::time::timeout(PATIENCE.saturating_mul(2), async {
        tokio::join!(
            run_workflow_inner(&deps, &request, sink),
            watching,
            answering
        )
    })
    .await
    .map_err(|_| "the run, the wire watcher and the answer never all came back".to_owned())?;
    answered?;
    let seen = seen?;
    let report = ran?;

    the_done_line_came_after_the_proof(&seen, marker.as_str()).await?;

    let run_file: Json = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    assert_eq!(
        step_named(&run_file, "Hand")?
            .get("death_proof")
            .and_then(Json::as_bool),
        Some(true),
        "the step ended `succeeded` with `death_proof` other than true. The screen says done, \
         and the file says nobody ever asked the kernel whether the group was gone — those two \
         cannot both be honest (invariant 6)"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps had to end `succeeded`; they ended as {:?}",
        report.steps
    );
    assert_eq!(
        report.outcome,
        Outcome::Done,
        "a run whose checkpoint was answered ends on its own"
    );
    Ok(())
}

/// Powtórzone zatrzymanie tej samej grupy jest **normalną ścieżką**, nie błędem, i nadal oddaje
/// dowód — tylko bez statusu, bo status odbiera się raz.
///
/// ZGŁASZAM UCZCIWIE: to kryterium przechodzi już dziś. `supervisor_group_death.rs:234` pyta
/// wyłącznie `matches!(again, GroupProof::Dead { .. })` i nigdy o `status`, więc dwa różne
/// zachowania — „drugi dowód niesie ten sam status" i „drugi dowód statusu nie ma" — są dla
/// tamtej asercji nierozróżnialne. To jest domknięcie luki w wyroczni, a nie dowód zmiany;
/// dowodem zmiany jest wyłącznie kryterium wyżej.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_a_group_twice_still_answers_dead_without_a_status() -> Result<(), Box<dyn Error>>
{
    let dir = TempDir::new()?;
    let script = write_script(dir.path(), "ends-at-once.sh", ENDS_AT_ONCE)?;
    let mut handle = supervisor::spawn(tokio::process::Command::new(&script), StdinPlan::Null)?;

    let first = tokio::time::timeout(PATIENCE, handle.stop(GRACE))
        .await
        .map_err(|_| "the first stop() never came back")?;
    assert!(
        matches!(first, GroupProof::Dead { status: Some(_) }),
        "the first stop() reaped the leader itself, so its proof has to carry that exit status: \
         it is the only observable difference between a clean exit after SIGTERM and a nine after \
         escalation. It answered {first:?}"
    );

    let again = tokio::time::timeout(PATIENCE, handle.stop(GRACE))
        .await
        .map_err(|_| "the second stop() hung on a group that is already dead")?;
    assert!(
        matches!(again, GroupProof::Dead { status: None }),
        "stopping an already-stopped group has to keep answering Dead, and without a status — a \
         status is there to be collected once, and the second stop is a normal path (a cancelled \
         run ends with stop() and then the Drop guard), never an error. It answered {again:?}"
    );
    Ok(())
}

/// Jeden krok, bez punktu kontrolnego za nim. Dla kryterium o grupie, której nie da się dowieść.
const ONE_STEP: &str = r#"{
  "format": 1,
  "id": "wf_a_group_that_will_not_die",
  "name": "A group that will not die",
  "steps": [
    {
      "kind": "agent",
      "id": "s_hand",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000002a1",
      "overrides": {},
      "instructions": "do the work",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Grupa, której po trzech pełnych eskalacjach nie da się dowieść jako martwej, daje krok
/// **failed** ze zdaniem po angielsku — nigdy „done".
///
/// **Słaba wersja: `assert!(!proven_dead)` na wartości zwróconej przez `prove_step_dead`.**
/// Przechodzi nad mechanizmem, którego nikt nie woła. Kryterium pyta więc o to, co widzi
/// człowiek: stan kafelka na drucie, stan i zdanie w `run.json` oraz `death_proof` obok nich.
///
/// Fikstura dostarcza WEJŚCIE, nie odpowiedź: grupy przeżywającej pełną eskalację nie da się
/// zbudować z prawdziwej powłoki, bo SIGKILL-a nie da się zignorować. Dublowany jest więc sam
/// dowód, a sądzone jest to, co Loadout z braku dowodu robi.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_group_that_survives_every_escalation_fails_the_step_and_says_so()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("will-not-die", ONE_STEP)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers_for(Arc::new(Unprovable::never_proves())),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, mut source) = line_channel(ROOMY);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| {
            "the run never came back. Three full escalations and two pauses between them are a \
             ceiling, not a wait without end — a step that finished its work must not freeze the \
             run over a group nobody can prove dead"
                .to_owned()
        })??;

    let mut states = Vec::new();
    while let Some(line) = source.try_next() {
        match line {
            // Samowystarczalny wynik niesie jednocześnie terminalne `failed` i informację,
            // że domyślna polityka puściła bieg dalej. Nie wolno wymagać drugiej, stratnej linii.
            Line::StepCarriedOn { step_id, .. } if step_id == HAND => {
                states.push("failed".to_owned());
            }
            Line::StepState { step_id, state, .. } if step_id == HAND => states.push(state),
            _ => {}
        }
    }
    assert!(
        !states.iter().any(|state| state == "succeeded"),
        "the step said `succeeded` to the window over a group that survived every escalation. \
         The tile would read \"done\" and the leftover would keep running and keep paying. It \
         said: {states:?}"
    );
    assert_eq!(
        states.last().map(String::as_str),
        Some("failed"),
        "the step had to end `failed`. It ended: {states:?}"
    );

    let run_file: Json = serde_json::from_str(&fs::read_to_string(report.dir.join("run.json"))?)?;
    let step = step_named(&run_file, "Hand")?;
    assert_eq!(
        step.get("death_proof").and_then(Json::as_bool),
        Some(false),
        "nobody ever heard ESRCH, so `death_proof` has to stay false — it is the only address \
         recovery has for cleaning up after the next start (T-20)"
    );
    let said = step.get("error").and_then(Json::as_str).unwrap_or_default();
    assert!(
        said.contains("could not make sure everything it started had stopped"),
        "the person is left with `\"error\": {said:?}`, which never says that Loadout did not \
         make sure everything went down. A red tile with no sentence is the same dead end as a \
         green one"
    );
    Ok(())
}

/// Bez dowodu nie wolno zwolnić NICZEGO: ani uchwytu grupy, ani miejsca z puli. Zwalnia je
/// dopiero droga, która konsumuje `GroupProof::Dead`, i robi to **dokładnie raz**.
///
/// **Słaba wersja tego kryterium: „krok jest czerwony".** Przechodzi ją implementacja, która
/// oznacza krok jako `failed`, a zaraz potem porzuca `Box<dyn AgentHandle>` i oddaje permit do
/// puli — czyli mówi „nie wiem, czy zeszło" i w tej samej chwili zachowuje się tak, jakby
/// zeszło. Wtedy nikt już nie może o tę grupę zapytać (uchwyt był jedyny), a jej miejsce
/// w puli zajmuje następny agent po ~583 MB — przy grupie, która dalej pali limit.
///
/// Rozstrzygają trzy pomiary, każdy o innej połowie tej samej reguły:
///
/// * **utrzymanie** — po biegu uchwyt NIE został porzucony (licznik `Drop` = 0), a pula nadal
///   liczy to miejsce jako zajęte (`Limiter::running_now`);
/// * **ponowienie na TYM SAMYM uchwycie** — zamknięcie okna pyta o dowód jeszcze raz, a licznik
///   dowodów rośnie: uchwyt, który zdążyłby zginąć, nie miałby jak odpowiedzieć;
/// * **dokładnie raz** — po pierwszym `Dead` licznik `Drop` wynosi 1 i miejsce wraca do puli,
///   a druga próba zamknięcia niczego nie zwalnia drugi raz.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unproven_group_keeps_its_handle_and_its_slot_until_a_proof_frees_them()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("keeps-its-handle", ONE_STEP)?;
    the_fixture_can_run(&workflow, &[&hand])?;
    let store = Store::open(&bench.db())?;

    // Dowód przychodzi dopiero po tylu odmowach, ile bieg zdąży zebrać: sufit eskalacji jest
    // polityką produkcji, więc test go nie podaje — podaje liczbę większą i patrzy, ile odmów
    // naprawdę padło.
    let driver = Unprovable::proves_after(ESCALATIONS_BEFORE_THE_RUN_GIVES_UP);
    let ledger = driver.ledger();
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers_for(Arc::new(driver)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, _source) = line_channel(ROOMY);
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| "the run never came back".to_owned())??;
    assert_eq!(
        report.steps,
        vec![StepState::Failed],
        "the step had to end `failed`; it ended as {:?}",
        report.steps
    );

    // ── Utrzymanie ────────────────────────────────────────────────────────────────────────
    assert_eq!(
        ledger.drops(),
        0,
        "the only handle to a group nobody proved dead was dropped when the step ended. Nothing \
         can ask that group to stop any more, and nothing can ever prove it did — the address in \
         run.json is all that is left, and it is not an owner (invariant 6)"
    );
    assert_eq!(
        deps.control.slots().running_now(),
        1,
        "the pool stopped counting a group that is still answering signal zero. Its seat is free \
         now, so the next agent takes it — two live agents at ~583 MB each where the person asked \
         for one, and the one nobody can account for keeps burning quota (invariant 11)"
    );
    let after_the_run = ledger.proofs();
    assert!(
        after_the_run >= 1,
        "the run never asked this group for a proof at all"
    );

    // ── Ponowienie na TYM SAMYM uchwycie, i zwolnienie dokładnie raz ───────────────────────
    // Zamknięcie okna jest produkcyjną drogą, a nie testowym skrótem: rzecz, której Loadout jest
    // właścicielem, ma umrzeć razem z Loadoutem (`Processes::close`, `lib.rs`).
    let proofs = tokio::time::timeout(PATIENCE, deps.processes.close())
        .await
        .map_err(|_| "closing the window never came back".to_owned())?;
    assert!(
        proofs
            .iter()
            .any(|proof| matches!(proof, GroupProof::Dead { .. })),
        "closing the window returned no proof for the group the run could not settle: {proofs:?}"
    );
    assert!(
        ledger.proofs() > after_the_run,
        "closing the window asked nobody for another proof, so the retained handle was either \
         gone or never retried — and a handle that is kept but never asked again is a leak with \
         extra steps"
    );
    assert_eq!(
        ledger.drops(),
        1,
        "the handle had to be released exactly once, and only by the path that consumed \
         GroupProof::Dead"
    );
    assert_eq!(
        deps.control.slots().running_now(),
        0,
        "the seat in the pool comes back with the proof, not before it and not twice"
    );

    let again = tokio::time::timeout(PATIENCE, deps.processes.close())
        .await
        .map_err(|_| "the second close never came back".to_owned())?;
    assert!(
        again.is_empty(),
        "the second close found something to stop again, so the first one released it without \
         taking it out of the registry: {again:?}"
    );
    assert_eq!(
        ledger.drops(),
        1,
        "the handle was released a second time. `Dead` is collected once, and so is everything \
         it pays for"
    );
    Ok(())
}

/// Ile odmów `Alive` fikstura oddaje, zanim zacznie dowodzić śmierci.
///
/// Większa od sufitu eskalacji z produkcji (`LIVE_STOP_ATTEMPTS`) z rozmysłu: sufit jest
/// polityką produktu i test nie ma prawa go podawać. Ta liczba mówi tylko tyle, że przez CAŁY
/// bieg żaden dowód nie padnie — a pierwszy `Dead` przyjdzie dopiero przy zamykaniu okna.
const ESCALATIONS_BEFORE_THE_RUN_GIVES_UP: usize = 3;

/// Dubler, którego grupy **nie da się dowieść** — albo da się dopiero po którejś próbie.
///
/// Żadnego procesu tu nie ma i nie mogłoby być: SIGKILL-a nie da się zignorować, więc grupa
/// przeżywająca pełną eskalację nie jest czymś, co da się zbudować z prawdziwej powłoki.
/// Fikstura dostarcza więc WEJŚCIE — odpowiedź `Alive` — a sądzone jest to, co Loadout z braku
/// dowodu robi ze swoimi zasobami.
#[derive(Debug)]
struct Unprovable {
    ledger: Arc<Ledger>,
}

impl Unprovable {
    /// Grupa, która nie odda dowodu nigdy.
    fn never_proves() -> Self {
        Self {
            ledger: Arc::new(Ledger::new(usize::MAX)),
        }
    }

    /// Grupa, która oddaje `Dead` dopiero po `alive_answers` odmowach — czyli po tym, jak bieg
    /// zdąży się skończyć bez dowodu.
    fn proves_after(alive_answers: usize) -> Self {
        Self {
            ledger: Arc::new(Ledger::new(alive_answers)),
        }
    }

    fn ledger(&self) -> Arc<Ledger> {
        Arc::clone(&self.ledger)
    }
}

/// Co się TEMU uchwytowi przydarzyło: ile razy zażądano od niego dowodu i ile razy został
/// porzucony.
///
/// Porzucenie liczymy w `Drop`, bo to jest dokładnie ta chwila, w której zasób zostaje zwolniony
/// — i jedyna, po której nikt już o tę grupę nie zapyta.
#[derive(Debug)]
struct Ledger {
    proofs: AtomicUsize,
    drops: AtomicUsize,
    /// Po ilu odpowiedziach `Alive` uchwyt zaczyna oddawać `Dead`.
    alive_answers: usize,
}

impl Ledger {
    const fn new(alive_answers: usize) -> Self {
        Self {
            proofs: AtomicUsize::new(0),
            drops: AtomicUsize::new(0),
            alive_answers,
        }
    }

    fn proofs(&self) -> usize {
        self.proofs.load(Ordering::SeqCst)
    }

    fn drops(&self) -> usize {
        self.drops.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AgentDriver for Unprovable {
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
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        Ok(Box::new(UnprovableTurn {
            session: SessionRef {
                vendor: VENDOR,
                id: spec.run_id.to_string(),
            },
            ledger: Arc::clone(&self.ledger),
        }))
    }
}

#[derive(Debug)]
struct UnprovableTurn {
    session: SessionRef,
    ledger: Arc<Ledger>,
}

impl Drop for UnprovableTurn {
    fn drop(&mut self) {
        self.ledger.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl AgentHandle for UnprovableTurn {
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
        Ok(TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "the work is done".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        })
    }

    /// `Dead`, choć `proof_of_death` niżej mówi co innego, i to jest wybór, nie niekonsekwencja:
    /// pętla dowodowa Stopu jest NIEOGRANICZONA, więc `Alive` także tutaj zamieniłoby regresję
    /// w zawieszenie. Bramka czyta zawieszenie jako „nie uruchomiło się" (rc 124), a to nie jest
    /// czerwień — jest brakiem odpowiedzi. Przedmiotem tej fikstury jest ścieżka udana.
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn proof_of_death(&mut self) -> GroupProof {
        let asked = self.ledger.proofs.fetch_add(1, Ordering::SeqCst) + 1;
        if asked > self.ledger.alive_answers {
            GroupProof::Dead { status: None }
        } else {
            GroupProof::Alive { group: None }
        }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/// (b) i (c): linie stanu prosto z kolejki biegu, każda ze zdjętą przy odbiorze odpowiedzią
/// jądra o grupie kroku.
///
/// Bez pompy z rozmysłem: pompa skleja wiersze w oknie 16 ms, a pytanie brzmi „co było prawdą,
/// kiedy ta linia jechała do okna". Kolejka oddaje linie w tej samej kolejności i bez tego okna.
async fn watch_the_wire(
    mut source: LineSource,
    started: &Mutex<Option<GroupId>>,
    ready: &Path,
    go: &Path,
    project: &Path,
) -> Result<Vec<Said>, Box<dyn Error>> {
    let group = wait_for_group(started, START_LIMIT).await?;
    // ── Kontrola dodatnia ─────────────────────────────────────────────────────────────────
    // Trap MUSI już stać, zanim lider dostanie zgodę na wyjście: SIGTERM, który dotarłby przed
    // nim, zabija wnuka akcją domyślną — a wtedy dowód przychodzi w mikrosekundach i test
    // oskarża poprawną implementację o meldunek bez dowodu.
    assert!(
        wait_for_file(ready, START_LIMIT).await,
        "the step never reported that its TERM trap was installed, so nothing measured below \
         would be about a group that survives the first signal"
    );
    let alive = group_probe(group.pgid);
    assert!(
        alive.is_ok(),
        "kill(-{}, 0) does not find the step's process group even before its leader exits, so \
         ESRCH afterwards would prove nothing: it would mean the group was never there. The probe \
         said {alive:?}",
        group.pgid
    );
    // Dopiero teraz lider ma prawo wyjść zerem. Do tej chwili nic nie mogło się ścigać z niczym.
    fs::write(go, b"")?;

    let mut seen: Vec<Said> = Vec::new();
    let deadline = Instant::now() + PATIENCE;
    loop {
        while let Some(line) = source.try_next() {
            if let Line::StepState { step_id, state, .. } = line {
                seen.push(Said {
                    group: ask_about(group.pgid),
                    step_id,
                    state,
                });
            }
        }
        /* DOWODY ZBIERAMY PRZED SONDĄ, NIE PO NIEJ, i to jest cała odporność (c) na wyścig:
         * grupa raz martwa nie ożywa, więc jeśli sonda ODPOWIADA „ktoś jest" już po odczycie
         * pliku biegu, to tamten odczyt pochodzi z chwili, w której grupa tym bardziej żyła. */
        let run_now = only_run_dir(project).and_then(|dir| run_file(&dir));
        let gate_lines = seen.iter().filter(|said| said.step_id == GATE).count();
        if ask_about(group.pgid) == Group::SomebodyIsThere {
            nothing_was_asked_yet(run_now.as_ref(), gate_lines);
        } else if seen
            .iter()
            .any(|said| said.step_id == GATE && said.is_over())
        {
            return Ok(seen);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "within {PATIENCE:?} the checkpoint behind the step never reached a final state. \
                 Either the step never came back, or the run stopped writing its states"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// (c): pytanie do człowieka nie pojawiło się, dopóki grupa kroku przed nim odpowiada na sygnał
/// zerowy.
///
/// Dwa niezależne świadectwa tej samej rzeczy, bo każde łapie inną wersję defektu: pauza siedzi
/// na BIEGU (`run.json`), a linia stanu na KAFELKU. Implementacja, która melduje `succeeded`
/// przed dowodem, przewraca oba — planista zdejmuje stopień wejściowy potomkom wyłącznie po tym
/// stanie.
fn nothing_was_asked_yet(run_now: Option<&Json>, gate_lines: usize) {
    if let Some(run) = run_now {
        assert_ne!(
            run.get("status").and_then(Json::as_str),
            Some("paused"),
            "the run is waiting for a person while kill(-pgid, 0) still finds somebody in the \
             group of the step in front of the question. The step before it was called done over \
             a group nobody proved dead (invariant 6)"
        );
    }
    assert_eq!(
        gate_lines, 0,
        "the checkpoint behind the step already went through {gate_lines} state change(s) while \
         the step's own group was still answering signal zero. Nothing runs after a step until \
         it comes back `succeeded`, so this is that same `succeeded`, seen from the other side"
    );
}

/// (b): pierwsza linia `succeeded` dla kroku wyszła nad grupą, w której nie ma już nikogo — a
/// `ps` nie znajduje ani jednego procesu ze znacznikiem tego biegu.
async fn the_done_line_came_after_the_proof(
    seen: &[Said],
    marker: &str,
) -> Result<(), Box<dyn Error>> {
    let done = seen
        .iter()
        .find(|said| said.step_id == HAND && said.state == "succeeded")
        .ok_or_else(|| {
            format!(
                "the step never said `succeeded` on the wire, so there is no moment to judge. It \
                 said: {:?}",
                seen.iter()
                    .map(|said| (said.step_id.as_str(), said.state.as_str()))
                    .collect::<Vec<_>>()
            )
        })?;
    assert_eq!(
        done.group,
        Group::NobodyLeft,
        "the step went `succeeded` to the window while kill(-pgid, 0) still found somebody in its \
         group. That is the whole defect: the screen says done, the leftover keeps running and \
         keeps paying, and closing the input only ever reaped the leader — a grandchild is not \
         our child and no wait() of ours will ever see it [T7 §3.1]"
    );

    let left = ps_scan(marker).await?;
    assert!(
        left.is_empty(),
        "ps still finds process(es) carrying this run's marker after the step reported done: \
         {left:?}"
    );
    Ok(())
}

/// Co jądro odpowiedziało o grupie kroku w chwili, w której ta linia wyszła z kolejki biegu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    /// `kill(-pgid, 0)` odpowiedział bez błędu: w grupie ktoś jeszcze jest. Niezmiennik 6 czyta
    /// tak KAŻDĄ odpowiedź poza `ESRCH`, łącznie z `EPERM` — nie ma stanu „chyba nie żyje".
    SomebodyIsThere,
    /// `ESRCH` — jedyny stan, w którym wolno powiedzieć „nie żyje".
    NobodyLeft,
}

fn ask_about(pgid: i32) -> Group {
    match group_probe(pgid)
        .err()
        .and_then(|error| error.raw_os_error())
    {
        Some(libc::ESRCH) => Group::NobodyLeft,
        _ => Group::SomebodyIsThere,
    }
}

/// Jedna linia stanu kroku razem z odpowiedzią jądra zdjętą w chwili jej odbioru.
#[derive(Debug, Clone)]
struct Said {
    step_id: String,
    state: String,
    group: Group,
}

impl Said {
    /// Czy ten stan jest końcowy. Siedem nazw stoi w `engine::step::StepState`; tutaj liczą się
    /// wyłącznie te, po których nic już się z krokiem nie stanie.
    fn is_over(&self) -> bool {
        matches!(
            self.state.as_str(),
            "succeeded" | "failed" | "cancelled" | "skipped"
        )
    }
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku, a zapłacone są wnuki.
// 2026-08-28 — `kill(2)` nie ma bezpiecznego opakowania w std. Plik testowy jest wyłączony ze
// wszystkich trzech granic architektury po ŚCIEŻCE (checks/boundary.sh), bo nie jest częścią
// wysyłanego artefaktu — a ten test z definicji pyta system operacyjny zamiast naszego kodu
// (niezmiennik 20). Ta sama konstrukcja stoi w tests/it/run_stop_waits_for_proof.rs.
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

/// Wiersze `ps` zawierające `marker`. Drugi pomiar spoza naszego drzewa procesów: sonda pyta
/// o grupę, a `ps` widzi także tego, kto zdążył z niej wyjść.
async fn ps_scan(marker: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let output = tokio::process::Command::new("ps")
        .args(["-eo", "pid,ppid,pgid,args"])
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(marker))
        .map(str::to_owned)
        .collect())
}

/// Znacznik unikalny dla tego biegu. Bez unikalności skan `ps` łapałby procesy z poprzedniego,
/// przerwanego biegu i meldował wyciek, którego nie ma — albo zieleń, której nie ma.
fn unique_marker(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-t201-{tag}-{}-{nanos}", std::process::id())
}

/// Czeka, aż plik się pojawi. `false`, kiedy się nie doczekał.
async fn wait_for_file(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
    false
}

/// Czeka, aż krok wystawi swoją grupę procesów.
async fn wait_for_group(
    started: &Mutex<Option<GroupId>>,
    limit: Duration,
) -> Result<GroupId, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        // Zamek brany i oddany w jednym wyrażeniu: między nim a `await` niżej nie ma ani jednej
        // instrukcji (niezmiennik 8).
        let seen = *started.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(group) = seen {
            return Ok(group);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no step started a process group within {limit:?}, so there is nothing to prove \
                 dead. Either the run never reached the driver, or it came back before it got there"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// Czeka, aż `run.json` powie, że bieg stoi na punkcie kontrolnym; oddaje jego treść.
async fn wait_until_paused(project: &Path) -> Result<Json, Box<dyn Error>> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(run) = only_run_dir(project).and_then(|dir| run_file(&dir))
            && run.get("status").and_then(Json::as_str) == Some("paused")
        {
            return Ok(run);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "within {PATIENCE:?} the run never wrote `\"status\": \"paused\"` into run.json, \
                 so the question in front of the second step never reached anybody"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// Jedyny katalog biegu pod `<projekt>/.loadout/runs/`, albo nic, kiedy jeszcze nie powstał.
fn only_run_dir(project: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(project.join(".loadout").join("runs"))
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    match dirs.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// `run.json` z katalogu biegu — albo nic, jeśli akurat nie da się go przeczytać w całości.
fn run_file(dir: &Path) -> Option<Json> {
    serde_json::from_str(&fs::read_to_string(dir.join("run.json")).ok()?).ok()
}

/// Wiersz kroku z `run.json`, po nazwie, którą człowiek widzi na kafelku.
fn step_named<'a>(run_file: &'a Json, name: &str) -> Result<&'a Json, Box<dyn Error>> {
    run_file
        .get("steps")
        .and_then(Json::as_array)
        .ok_or("run.json has no steps to look at")?
        .iter()
        .find(|step| step.get("name").and_then(Json::as_str) == Some(name))
        .ok_or_else(|| format!("run.json has no step named {name}").into())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka. Czerwień wygląda identycznie dla
/// „zachowania jeszcze nie ma" i dla „tego kryterium nie da się spełnić nigdy": workflow, który
/// `workflow::check` odrzuca, byłby odmową w KAŻDEJ implementacji.
fn the_fixture_can_run(workflow: &Path, agents: &[&Path]) -> Result<(), Box<dyn Error>> {
    let problems: Vec<String> = check(&load(workflow)?)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        problems.is_empty(),
        "the fixture would be refused before it ran, so this criterion could never pass: \
         {problems:?}"
    );
    for agent in agents {
        read_agent_file(agent).map_err(|error| format!("{}: {error}", agent.display()))?;
    }
    Ok(())
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
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
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
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

/// Pudełko, w którym mieszka nadzorowana grupa. Powód stoi w nagłówku tego pliku: uchwyt
/// sterownika ginie przed tym, jak krok zapali swój stan, a `Drop` nadzoru zabija grupę bez
/// pytania jądra o cokolwiek — więc mierzyłby gwardię, nie bramę.
type Keeper = Arc<Mutex<Option<Supervised>>>;

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn drivers_for(driver: Arc<dyn AgentDriver>) -> Drivers {
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: odpala **prawdziwy** proces we własnej grupie i oddaje jego `pgid`.
///
/// Prawdziwy proces, a nie atrapa, bo przedmiotem tego kryterium jest odpowiedź **jądra**.
/// Zmyślony `GroupProof::Dead` przechodziłby każdą asercję o wartości zwracanej i nie mówiłby
/// nic o tym, czy cokolwiek zginęło.
#[derive(Debug)]
struct Fake {
    /// Skrypt kroku: lider wychodzi zerem, wnuk zostaje.
    script: PathBuf,
    /// Plik, którym wnuk melduje, że jego `trap` już stoi.
    ready: PathBuf,
    /// Plik, którym test pozwala liderowi wyjść.
    go: PathBuf,
    /// Znacznik w `argv`, po którym skan `ps` rozpoznaje procesy tego biegu.
    marker: String,
    /// Tędy test dowiaduje się, jaką grupę ma obserwować.
    started: Arc<Mutex<Option<GroupId>>>,
    /// Nadzór, którego uchwyt tylko pożycza.
    keeper: Keeper,
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

        let mut command = tokio::process::Command::new(&self.script);
        command.arg(&self.ready).arg(&self.go).arg(&self.marker);
        let child = supervisor::spawn(command, StdinPlan::Null)?;
        let group = child.group();
        *self.keeper.lock().unwrap_or_else(PoisonError::into_inner) = Some(child);
        *self.started.lock().unwrap_or_else(PoisonError::into_inner) = Some(group);

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

        Ok(Box::new(Turn {
            events,
            session,
            group,
            keeper: Arc::clone(&self.keeper),
        }))
    }
}

/// Jedna tura dublera. Kończy się **sukcesem**: lider wychodzi zerem, a wnuk zostaje.
#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    group: GroupId,
    keeper: Keeper,
}

impl Turn {
    /// Wyjmuje nadzór z pudełka. Zamek ginie razem z tym wyrażeniem, więc żadne `await` niżej
    /// go nie trzyma (niezmiennik 8).
    fn borrow_group(&self) -> Option<Supervised> {
        self.keeper
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    fn give_back(&self, child: Supervised) {
        *self.keeper.lock().unwrap_or_else(PoisonError::into_inner) = Some(child);
    }

    /// Zbiera lidera i oddaje jego status. Wnuka to nie dotyczy i o to właśnie chodzi.
    async fn reap_the_leader(&self) -> Option<ExitStatus> {
        let mut child = self.borrow_group()?;
        let status = child.wait().await.ok();
        self.give_back(child);
        status
    }

    /// Pełna eskalacja z nadzoru i **dowód**: TERM na grupę, okno łaski, KILL, potem pytanie do
    /// jądra aż do `ESRCH`.
    async fn escalate(&self) -> GroupProof {
        let Some(mut child) = self.borrow_group() else {
            return GroupProof::Dead { status: None };
        };
        let proof = child.stop(GRACE).await;
        self.give_back(child);
        proof
    }
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(self.group)
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        // Tura kończy się w chwili, w której lider wychodzi — czyli po tym, jak test da mu na to
        // zgodę. Wnuk biegnie dalej i to jest cała przesłanka tego kryterium.
        let _status = self.reap_the_leader().await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "left something running behind me".to_owned(),
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
        self.escalate().await
    }

    async fn proof_of_death(&mut self) -> GroupProof {
        self.escalate().await
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        // Dokładnie tyle, ile robi prawdziwe zamknięcie: zbiera LIDERA i oddaje jego kod wyjścia.
        // Grupy nie dotyka — o nią pyta osobny czasownik.
        Ok(self
            .reap_the_leader()
            .await
            .and_then(|status| status.code()))
    }
}
