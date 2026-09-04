//! Z-30: cztery drobne wycieki silnika, cztery niezależne kryteria.
//!
//! Każdy test odpowiada na jedno pytanie i żaden nie zależy od pozostałych — wady są rozłączne,
//! a wspólny test na cztery naraz byłby czterema powodami czerwieni pod jedną nazwą.
//!
//! 1. **Gwardia `Drop` strzelała w numer, którego mogło już nie być.** `Supervised` po naturalnym
//!    wyjściu procesu ginął z `proved_dead == false`, bo dowód `ESRCH` brał wyłącznie `stop()`.
//!    Numery grup przewijają się na macOS w godzinach (`kern.maxproc` = 16 000, powód stoi przy
//!    `machine_booted_at`), więc dziewiątka w zapamiętany `pgid` jest błędem poprawności
//!    [T7 ryzyko 2]. Sonda zerem przed sygnałem i dowód brany przy `wait()` zamykają obie połowy.
//! 2. **Sprzątanie sierot przy starcie szło po kolei**, blokując wątek startu na pełnym oknie
//!    łaski KAŻDEJ grupy. Pięć grup ignorujących SIGTERM to pięć takich okien jedno po drugim.
//! 3. **`stream::pump` nie miał sufitu na JEDNĄ linię NDJSON.** Ta sama treść stała naraz
//!    w buforze, w `serde_json::Value`, w mapie faktów o narzędziach i w zdekodowanym zdarzeniu.
//! 4. **Krok czekający na miejsce w puli czytał się jako `Running`.** Po panice zadania
//!    `settle_leftovers` zamykał go jako `Failed` — czyli jako coś, co pracowało i nie przeszło
//!    (niezmiennik 11).

// `clippy::panic` jest w tym drzewie `deny` i sądzi także `--all-targets`. Kryterium 4 jest
// jednak dokładnie o tym, co planista robi po panice zadania kroku, więc panika JEST tu
// materiałem pomiarowym — tak samo jak w `a_run_always_settles`.
#![allow(clippy::panic)]

use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::commands::reconcile;
use loadout_lib::engine::dag::Dag;
use loadout_lib::engine::line::Line;
use loadout_lib::engine::scheduler::{Route, Started, execute_routed_with_start};
use loadout_lib::engine::step::StepReport;
use loadout_lib::engine::step::StepState::{Cancelled, Failed, Skipped, Succeeded};
use loadout_lib::engine::stream;
use loadout_lib::engine::supervisor::{self, GroupProof, StdinPlan};
use loadout_lib::recovery::{ReapOutcome, ReapTarget};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Proces, który schodzi sam i natychmiast — droga „naturalne wyjście".
const QUICK: &str = r"#!/bin/sh
exit 0
";

/// Lider, który wypuszcza uciekiniera do **własnej** grupy procesów i czeka.
///
/// `set -m` włącza sterowanie zadaniami, więc zadanie w tle dostaje własną grupę — dokładnie to,
/// co Claude Code robi z każdą komendą narzędzia Bash. `setsid(1)` na macOS nie istnieje.
const LEADER: &str = r#"#!/bin/sh
# $1 = skrypt uciekiniera, $2 = znacznik
set -m
"$1" "$2-escapee" &
set +m
while :; do
  sleep 0.2
done
"#;

/// Uciekinier: nic nie robi poza tym, że jest widoczny w `ps` pod swoim znacznikiem. Własny
/// sufit ~12 s, żeby czerwony przebieg nie zostawił po sobie tej samej sieroty, którą mierzy.
const ESCAPEE: &str = r#"#!/bin/sh
# $1 = znacznik
i=0
while [ "$i" -lt 60 ]; do
  sleep 0.2
  i=$((i+1))
done
"#;

/// Ile grup sprząta kryterium 2. Pięć, bo tyle wymienia zlecenie i bo jedna nie odróżnia
/// „czekały obok siebie" od „miałem szczęście z kolejnością".
const GROUPS: usize = 5;

/// Ile jedna grupa czeka na pozostałe, zanim uzna, że stoi w tej kolejce sama.
///
/// Sekundy, nie milisekundy: to jest sufit na **awarię**, a nie pomiar. Zielony przebieg mija go
/// bez czekania, czerwony płaci go raz — pierwsza grupa, która się poddaje, podnosi zapadkę,
/// więc pozostałe cztery wracają natychmiast.
const TOGETHER_PATIENCE: Duration = Duration::from_secs(2);

/// Sufit z `stream.rs`, przepisany tu jako liczba, a nie zaimportowany.
///
/// Świadomie: import przypiąłby kryterium do stałej i test przechodziłby także wtedy, gdyby ktoś
/// podniósł sufit do gigabajta. Ta liczba jest tu obietnicą, którą składamy człowiekowi.
const LINE_CAP: usize = 16 * 1024 * 1024;

/// Kto już czeka na swoje okno łaski i czy pętla zdarzeń zdążyła w tym czasie dostać turę.
///
/// Dwa pola pod jednym zamkiem, nie dwa liczniki obok: bariera zwalnia dopiero, gdy oba warunki
/// są spełnione naraz, a warunek złożony z dwóch niezależnych zamków nie ma jak być atomowy.
#[derive(Debug, Default)]
struct Together {
    /// Ile grup stoi w tej chwili w swoim domykaczu.
    waiting: usize,
    /// Czy w tym czasie coś asynchronicznego dostało turę — czyli czy okno by odpowiedziało.
    the_window_kept_its_turn: bool,
}

/// Bieg zastany w folderze przez okno, które dopiero co się otworzyło: `running` z żywym `pgid`.
///
/// Kształt jest ten sam, co w `runs_left_over_are_reconciled`, bo czyta go ten sam `rows_from_files`
/// — a `boot_id` musi być tym, co maszyna mówi TERAZ, inaczej strażnik z `recovery::decide` słusznie
/// wstrzyma strzał i plan wyjdzie pusty.
fn put_a_left_over_run(project: &Path, which: usize, boot: &str) -> Result<(), Box<dyn Error>> {
    let pgid = 30_200 + i32::try_from(which).unwrap_or_default();
    let dir = project
        .join(".loadout")
        .join("runs")
        .join(format!("20260904-01003{which}__z30-run-{which}"));
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("run.json"),
        format!(
            r#"{{
  "id": "z30-run-{which}",
  "workflow_id": "leaks.json",
  "title": "Left over {which}",
  "status": "running",
  "boot_id": "{boot}",
  "started_at": 1787446837880,
  "ended_at": null,
  "steps": [
    {{
      "id": "z30-step-{which}",
      "node_key": "s_1",
      "name": "Was working",
      "agent": "codex",
      "kind": "agent",
      "depends_on": [],
      "status": "running",
      "attempt": 0,
      "pid": {pgid},
      "pgid": {pgid},
      "started_at": 1787446837880,
      "ended_at": null,
      "error": null
    }}
  ]
}}
"#
        ),
    )?;
    Ok(())
}

/// Co `run.json` tego biegu mówi teraz o sobie — czyli zdanie, które człowiek widzi w historii.
fn read_run_status(project: &Path, which: usize) -> Result<Option<String>, Box<dyn Error>> {
    let text = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(format!("20260904-01003{which}__z30-run-{which}"))
            .join("run.json"),
    )?;
    let said: serde_json::Value = serde_json::from_str(&text)?;
    Ok(said
        .get("status")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

/// Co gwardia `Drop` zrobiła jednej zapamiętanej grupie. Kolejność wpisów JEST tu asercją:
/// `Killed` bez `Asked` przed sobą to strzał w numer, o który nikt nie zapytał.
#[derive(Debug, PartialEq, Eq)]
enum Guarded {
    /// Sonda sygnałem zerowym — niczego nie dostarcza, pyta o istnienie.
    Asked(i32),
    /// Dziewiątka: przez uchwyt dziecka dla lidera, przez `killpg` dla każdej innej grupy.
    Killed(i32),
}

/// Znacznik unikalny dla tego przebiegu — po nim `ps` rozpozna nasze procesy.
fn unique_marker(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-z30-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta JĄDRO, czy w grupie `pgid` jest jeszcze ktokolwiek — bez wysyłania sygnału.
// Pomiar ma pochodzić od systemu operacyjnego, nie od naszego kodu (niezmiennik 20): test
// wołający `supervisor::group_is_empty` sprawdzałby zgodność naszej funkcji z samą sobą. Pliki
// testowe są wyłączone z granic architektury po ścieżce (`checks/quick-boundary.sh`).
#[allow(unsafe_code)]
fn group_is_gone(pgid: i32) -> bool {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza istnienie i prawa. Argumenty
    // to zwykłe liczby, więc nie ma tu wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Czeka najwyżej `limit`, aż grupa przestanie odpowiadać.
async fn wait_until_gone(pgid: i32, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if group_is_gone(pgid) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Wiersze `ps` niosące `marker` — także te osierocone do `ppid == 1`.
async fn ps_scan(marker: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-eo", "pid,ppid,pgid,args"])
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(marker))
        .map(str::to_owned)
        .collect())
}

/// Czeka, aż `ps` pokaże co najmniej `want` procesów ze znacznikiem.
async fn wait_for_rows(
    marker: &str,
    want: usize,
    limit: Duration,
) -> Result<Vec<String>, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        let rows = ps_scan(marker).await?;
        if rows.len() >= want || Instant::now() >= deadline {
            return Ok(rows);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/* KRYTERIUM 1. Naturalne wyjście daje dowód śmierci, a gwardia `Drop` pyta, zanim strzeli.
 *
 * Trzy części, bo drogi procesu są trzy i wada dotykała każdej inaczej: sama polityka gwardii
 * (do kogo wolno strzelić), zejście naturalne (skąd bierze się dowód) i porzucony uchwyt nad
 * ŻYWĄ grupą (dowodu nie ma, więc eskalacja ma zadziałać tak samo jak przedtem).
 *
 * NA STARYM KODZIE pada część pierwsza i druga: gwardia strzelała bez pytania, a `wait()` nie
 * ustawiał `proved_dead` nigdy. */
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn natural_exit_proves_esrch_and_drop_never_kills_an_absent_group()
-> Result<(), Box<dyn Error>> {
    /* 1. Polityka gwardii, razem z KOLEJNOŚCIĄ obu czynności. Numery są zmyślone — o to chodzi,
     *    bo tu nie leci ani jeden sygnał.
     *
     *    Jeden wspólny dziennik na sondy i strzały, nie dwie listy obok siebie: pytanie brzmi
     *    „czy ten strzał miał przed sobą swoją sondę", a dwie rozłączne listy odpowiadają
     *    wyłącznie na „czy w ogóle o coś pytaliśmy". Grupa lidera stoi w tym dzienniku pierwsza
     *    i **też** musi mieć swoją sondę: to na niej poległa pierwsza wersja tej poprawki, bo
     *    lider schodził obok tej pętli, przez `start_kill()` bez ani jednego pytania. */
    let leader = 30_100;
    let recycled = 30_101;
    let still_there = 30_102;
    let did = RefCell::new(Vec::new());
    supervisor::kill_what_still_answers(
        &[leader, recycled, still_there],
        |pgid| {
            did.borrow_mut().push(Guarded::Asked(pgid));
            pgid == recycled
        },
        |pgid| did.borrow_mut().push(Guarded::Killed(pgid)),
    );
    assert_eq!(
        did.into_inner(),
        vec![
            Guarded::Asked(leader),
            Guarded::Killed(leader),
            Guarded::Asked(recycled),
            Guarded::Asked(still_there),
            Guarded::Killed(still_there),
        ],
        "every remembered group — the leader's included — is asked with the zero signal, and \
         only the ones that answered are killed. A kill with no Asked in front of it is a shot \
         into a number that was freed since the last scan and may belong to somebody else by \
         now: a correctness bug, not a theoretical risk [T7 risk 2]"
    );

    // 2. Zejście naturalne: `wait()` zebrał lidera, więc dowód `ESRCH` jest do wzięcia za darmo
    //    i gwardia nie ma już czego zabijać.
    let dir = tempfile::tempdir()?;
    let quick = write_script(dir.path(), "quick.sh", QUICK)?;
    let mut handle = supervisor::spawn(Command::new(&quick), StdinPlan::Null)?;
    let group = handle.group();
    let status = handle.wait().await?;
    assert!(
        status.success(),
        "the fixture is supposed to exit cleanly, otherwise this measures the wrong thing: \
         {status:?}"
    );
    assert!(
        format!("{handle:?}").contains("proved_dead: true"),
        "a process that exited by itself and was reaped leaves an empty group, so the ESRCH that \
         invariant 6 asks for is already there. Without taking it here the handle dies knowing \
         nothing, and the Drop guard sends SIGKILL to a number that may have been handed to \
         somebody else in the meantime: {handle:?}"
    );
    assert!(
        matches!(
            handle.stop(Duration::from_secs(2)).await,
            GroupProof::Dead { .. }
        ),
        "stopping a handle that already has its proof still answers Dead, without a single \
         signal: a repeated stop is an ordinary path, not a mistake"
    );
    drop(handle);
    assert!(
        group_is_gone(group.pgid),
        "nothing in this test may bring the group back to life"
    );

    // 3. Porzucony uchwyt nad ŻYWĄ grupą: dowodu nie ma, więc sonda ma odpowiedzieć „jest" i cała
    //    eskalacja ma zadziałać, łącznie z uciekinierem, który siedzi we własnej grupie.
    let marker = unique_marker("drop-guard");
    let escapee = write_script(dir.path(), "escapee.sh", ESCAPEE)?;
    let lead = write_script(dir.path(), "leader.sh", LEADER)?;
    let mut command = Command::new(&lead);
    command.arg(&escapee).arg(&marker);

    let live = {
        let handle = supervisor::spawn(command, StdinPlan::Null)?;
        let live = handle.group();
        let running = wait_for_rows(&marker, 1, Duration::from_secs(5)).await?;
        assert!(
            !running.is_empty(),
            "the escapee was supposed to be running before the handle is dropped; ps saw nothing"
        );
        live
        // Uchwyt ginie TUTAJ, bez ani jednego `stop()` — to jest ścieżka wczesnego `?`.
    };
    assert!(
        wait_until_gone(live.pgid, Duration::from_secs(5)).await,
        "one second after the handle was dropped, the leader group is still answering. Probing \
         before the signal may not weaken the last line of defence: a group left behind is a \
         claude burning quota with nobody holding anything to stop it [T7 §3.1]"
    );
    let survivors = ps_scan(&marker).await?;
    assert!(
        survivors.is_empty(),
        "ps still finds processes carrying the marker after the handle was dropped: {survivors:?}"
    );

    Ok(())
}

/* KRYTERIUM 2. Pięć sierot zastanych w FOLDERZE czeka OBOK SIEBIE, nie jedna po drugiej.
 *
 * Droga jest tu produkcyjna i to jest cała poprawka po odrzuceniu: `with_reaper` to dokładnie ta
 * funkcja, którą woła `reconcile_runs_keeping`, czyli sprzątanie odpalane przy pierwszym dotknięciu
 * folderu (`ipc::AppState::settle_everything_left_behind`). Ta sama funkcja rozkłada plan, co
 * `reap_what_is_ours_concurrently` po stronie biblioteki — jeden rdzeń, dwie drogi (niezmiennik 23).
 * Wersja pytająca sam rdzeń przechodziła nad tym, że folder dalej czekał pięć razy po kolei.
 *
 * Bariera, nie zegar: test mierzący czas mierzyłby obciążenie maszyny. Każda grupa melduje
 * przyjście i czeka, aż zamelduje ostatnia — przy sprzątaniu po kolei pierwsza z nich nie
 * doczeka się nikogo, bo pozostałe cztery jeszcze się nie zaczęły.
 *
 * DRUGI WARUNEK BARIERY MIERZY OKNO: żeby ktokolwiek ruszył dalej, flagę musi postawić zadanie
 * asynchroniczne, czyli pętla zdarzeń tego runtime'u. `#[tokio::test]` bez `flavor` daje runtime
 * jednowątkowy, więc sprzątanie wykonane wprost na tym wątku nie miałoby jak jej postawić — tak
 * samo, jak nie miałoby jak pokazać okna. Uzgodnienie jedzie tu do `spawn_blocking` dokładnie tak,
 * jak robi to `ipc`.
 *
 * Domykacz jest wstrzyknięty i nie dotyka systemu ani razu: numery grup są zmyślone, a strzał
 * do numeru wpisanego w fikstrze byłby strzałem w cudzą pracę.
 *
 * NA STARYM KODZIE pierwsza grupa czeka `TOGETHER_PATIENCE` i wraca sama. */
#[tokio::test]
async fn five_startup_groups_reap_concurrently_without_blocking_the_runtime()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path().to_path_buf();
    // TEN SAM rozruch maszyny, co teraz: inaczej strażnik z `recovery::decide` wstrzyma strzał
    // i plan wyjdzie pusty, a bariera nie miałaby na kogo czekać.
    let boot = supervisor::machine_booted_at().ok_or("this machine does not say when it booted")?;
    for which in 0..GROUPS {
        put_a_left_over_run(&project, which, &boot)?;
    }

    let together = Arc::new((Mutex::new(Together::default()), Condvar::new()));
    let saw_everyone = Arc::new(AtomicUsize::new(0));
    let waited_alone = Arc::new(AtomicBool::new(false));

    let watching = tokio::spawn({
        let together = Arc::clone(&together);
        async move {
            let (state, everyone) = &*together;
            let deadline = Instant::now() + TOGETHER_PATIENCE;
            while Instant::now() < deadline {
                {
                    let mut now = state.lock().unwrap_or_else(PoisonError::into_inner);
                    if now.waiting == GROUPS {
                        now.the_window_kept_its_turn = true;
                        everyone.notify_all();
                        return true;
                    }
                    // Zamek ginie razem z tym blokiem, PRZED `await` (niezmiennik 8).
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            false
        }
    });

    let reap = {
        let together = Arc::clone(&together);
        let saw_everyone = Arc::clone(&saw_everyone);
        let waited_alone = Arc::clone(&waited_alone);
        move |_target: &ReapTarget| {
            if waited_alone.load(Ordering::SeqCst) {
                // Ktoś przed nami zmierzył już, że stoi w tej kolejce sam. Czekanie drugi raz
                // kupiłoby tę samą odpowiedź za kolejne dwie sekundy.
                return ReapOutcome::ProvenDead;
            }
            let (state, everyone) = &*together;
            let mut now = state.lock().unwrap_or_else(PoisonError::into_inner);
            now.waiting += 1;
            everyone.notify_all();
            let (_now, when) = everyone
                .wait_timeout_while(now, TOGETHER_PATIENCE, |so_far| {
                    so_far.waiting < GROUPS || !so_far.the_window_kept_its_turn
                })
                .unwrap_or_else(PoisonError::into_inner);
            if when.timed_out() {
                waited_alone.store(true, Ordering::SeqCst);
            } else {
                saw_everyone.fetch_add(1, Ordering::SeqCst);
            }
            ReapOutcome::ProvenDead
        }
    };

    // `spawn_blocking` wokół CAŁEGO uzgodnienia — tak samo, jak robi to
    // `ipc::AppState::settle_what_the_last_window_left`.
    let at = project.clone();
    let settled = tokio::task::spawn_blocking(move || reconcile::with_reaper(&at, reap)).await?;
    let the_window_kept_its_turn = watching.await?;

    assert_eq!(
        saw_everyone.load(Ordering::SeqCst),
        GROUPS,
        "all {GROUPS} groups have to be waiting at the same moment. One orphan that ignores \
         SIGTERM costs a full grace window plus the proof after the kill, so five of them done \
         one after another is that window five times over — and it is spent while somebody is \
         waiting for the folder to open"
    );
    assert!(
        the_window_kept_its_turn,
        "the event loop never got a turn while the five groups were waiting, so settling a folder \
         still holds the thread that has to answer the window"
    );
    assert_eq!(
        settled.reaped, GROUPS,
        "waiting side by side may not lose a single answer: every group still has to reach the \
         report, under the same three-answers-three-lists rule as before"
    );
    assert_eq!(
        settled.still_alive, 0,
        "no group was left unproven in this run: every one of them answered ProvenDead"
    );

    // A TERAZ ZDANIE, KTÓRE CZYTA CZŁOWIEK (niezmiennik 29): pięć biegów w historii folderu
    // przestało twierdzić, że pracują. Raport dowodzi mechanizmu, `run.json` dowodzi produktu.
    assert_eq!(
        settled.runs, GROUPS,
        "every run left behind by the closed window has to be written off, not just counted"
    );
    for which in 0..GROUPS {
        let said = read_run_status(&project, which)?;
        assert_eq!(
            said.as_deref(),
            Some("interrupted"),
            "run {which} still says it is working after the folder was settled, and that is the \
             sentence a person reads in history"
        );
    }

    Ok(())
}

/* KRYTERIUM 3. Linia ponad sufit jedzie do tee, liczy się RAZ i nie dochodzi do dekodera.
 *
 * Linia przesadzona jest tu POPRAWNYM zdarzeniem `assistant` — i to jest cały pomiar. Gdyby była
 * śmieciem, stary kod też policzyłby ją jako nierozpoznaną i test przechodziłby nad wadą.
 * Zdanie z jej środka nie ma prawa dojść na ekran, bo dekoder nie ma prawa jej zobaczyć.
 *
 * NA STARYM KODZIE `read_until` wciąga całe 16 MiB do bufora, po czym ta sama treść stoi
 * jednocześnie w `serde_json::Value`, w mapie faktów i w zdarzeniu — a wiersz z niej dociera
 * do widoku. */
#[tokio::test]
async fn oversized_ndjson_is_teed_counted_once_and_never_decoded() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let tee = dir.path().join("agent-z30.jsonl");

    let padding = "x".repeat(LINE_CAP);
    let over = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"over the cap {padding}"}}]}}}}"#
    );
    let under =
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"under the cap"}]}}"#;
    let mut input = Vec::new();
    input.extend_from_slice(over.as_bytes());
    input.push(b'\n');
    input.extend_from_slice(under.as_bytes());
    input.push(b'\n');

    let (sender, mut inbox) = tokio::sync::mpsc::channel::<Line>(64);
    let stats = stream::pump(&input[..], &tee, "z30", sender).await?;

    let mut said = Vec::new();
    while let Ok(line) = inbox.try_recv() {
        said.push(line);
    }
    let said = format!("{said:?}");

    assert_eq!(
        stats.lines, 2,
        "both lines went through the loop, the long one exactly once: a line counted twice is a \
         number nobody can read"
    );
    assert_eq!(
        stats.unrecognised, 1,
        "the line over the cap is the one nobody could read, and it has to be counted before it \
         disappears — zero here means the pump swallowed it as nothing"
    );
    assert!(
        !said.contains("over the cap"),
        "the line over the cap reached the decoder, so its whole content stood in the buffer, in \
         serde_json::Value, in the tool facts and in the decoded event at the same time: {}",
        &said[..said.len().min(400)]
    );
    assert!(
        said.contains("under the cap"),
        "the next well-formed line still has to be handled: the pump never ends a run on one \
         line (invariant 5). What came out: {}",
        &said[..said.len().min(400)]
    );

    let teed = fs::read(&tee)?;
    assert!(
        teed == input,
        "the file is the truth (invariant 4) and the tee happens before parsing, so the long \
         line belongs in it byte for byte: {} bytes written against {} bytes read",
        teed.len(),
        input.len()
    );

    Ok(())
}

/* KRYTERIUM 4. Krok, który czekał na miejsce, nie jest krokiem, który pracował.
 *
 * Pięć kroków bez ani jednej strzałki, żeby stożek nie mieszał się do wyniku, i pięć różnych
 * zakończeń: panika po potwierdzeniu startu, panika przed nim, sukces, zwykła porażka
 * i anulowanie w środku kroku. Cztery ostatnie mają znaczyć dokładnie to, co dziś.
 *
 * NA STARYM KODZIE krok numer 1 kończy jako `Failed`: `Running` wpisywał permit planisty, a nie
 * miejsce z puli aplikacji. */
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn panic_distinguishes_waiting_from_actually_running() -> Result<(), Box<dyn Error>> {
    let dag = Dag::new(5, &[])?;
    let outcome = execute_routed_with_start(
        &dag,
        5,
        CancellationToken::new(),
        move |id, _cancel, started: Started| async move {
            match id {
                // Naprawdę ruszył i padł w trakcie: to jest `failed`.
                0 => {
                    started.now();
                    panic!("this step was really running when it broke");
                }
                // Stał w kolejce po miejsce z puli, kiedy jego zadanie padło. Nie zrobił nic.
                1 => panic!("this step never got a slot, so it never started"),
                2 => {
                    started.now();
                    StepReport::Succeeded
                }
                3 => {
                    started.now();
                    StepReport::Failed
                }
                _ => {
                    started.now();
                    StepReport::Cancelled
                }
            }
        },
        |_, _| Route::All,
    )
    .await;

    assert_eq!(
        outcome.states,
        vec![Failed, Skipped, Succeeded, Failed, Cancelled],
        "a step that panicked while it was really running is `failed`; a step that panicked \
         while it was still queueing for a slot never did anything, so it is `skipped`. Both \
         read as `failed` while the scheduler wrote `running` at its own permit — and that is \
         the very difference invariant 11 stands on"
    );
    assert!(
        !outcome.cancelled,
        "nobody pressed Stop in this run, so the run itself is not cancelled"
    );

    /* Ta sama droga po Stopie: krok, który nie zdążył ruszyć, jest `cancelled`, nie `skipped` —
     * nikt wyżej nie padł, człowiek zatrzymał bieg [T7 §9.3]. Token gaśnie W ŚRODKU kroku, żeby
     * pętla wysyłki zdążyła go wypuścić: anulowanie sprzed startu ma własną, starszą drogę. */
    let alone = Dag::new(1, &[])?;
    let stop = CancellationToken::new();
    let outcome = execute_routed_with_start(
        &alone,
        1,
        stop.clone(),
        move |_id, cancel: CancellationToken, _started: Started| async move {
            cancel.cancel();
            panic!("this step was still queueing when the person pressed Stop");
        },
        |_, _| Route::All,
    )
    .await;

    assert_eq!(
        outcome.states,
        vec![Cancelled],
        "after Stop a step that never started reads as `cancelled`: `skipped` would tell the \
         person that somebody upstream broke, and `failed` that this step did"
    );
    assert!(outcome.cancelled, "the run itself was stopped by a person");

    Ok(())
}
