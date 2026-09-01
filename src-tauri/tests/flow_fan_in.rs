//! FIZYCZNY FAN-IN, na PRAWDZIWYCH agentach: dwoje rodziców pracuje w swoich kopiach, a krok
//! pod nimi zastaje na dysku pliki obojga.
//!
//! # Po co to istnieje
//!
//! Fan-in wylądował 2026-08-29 (`commands::fan_in`) i był dowiedziony WYŁĄCZNIE na dublerach
//! sterownika (`tests/it/parents_fold_into_one_copy.rs`,
//! `tests/it/copies_carry_the_try_they_hold.rs`). Dubler pisze do `spec.cwd` sam, w tym samym
//! procesie — więc dowodzi kodu, który składa katalogi, i nie dowodzi, że prawdziwa sesja
//! `claude` postawiona w złożonej kopii naprawdę zastaje w niej pracę obojga rodziców. Między
//! jednym a drugim leży cała warstwa, której dubler nie tyka: drzewo robocze gita, prawa do
//! katalogu i kolejność składania wobec startu procesu.
//!
//! # Dlaczego `#[ignore]`, i dlaczego to NIE jest ucieczka
//!
//! Ten test uruchamia **cztery prawdziwe sesje `claude`** i za nie płaci. Bramka woła
//! `cargo test --tests -- --test-threads=1` (`.loadout/h/checks.json`, check `rust-test`)
//! **bez** `--include-ignored`, więc go nie odpala — i tak ma być: kryterium, które przy każdym
//! przebiegu bramki wystawia rachunek, zostaje wyłączone przez pierwszą osobę, której się
//! spieszy. Uruchamia się je świadomie:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test flow_fan_in -- --ignored --nocapture
//! ```
//!
//! Osobny CEL, a nie drugi test w `flow_todo_app.rs`, i to jest wybór z dwóch powodów. Pierwszy:
//! nagłówek tamtego pliku dokumentuje `--test flow_todo_app -- --ignored`, więc to polecenie
//! płaciłoby od tego dnia za oba flow naraz, a tamto trwa 281 s. Drugi jest fikstury: ten test
//! wymaga projektu, który JEST repozytorium gita — bez commita nie ma ani pliku śledzonego, ani
//! bazy do porównania bajtów — a `Bench::new()` tamtego pliku repozytorium celowo nie zakłada.
//! `git init` w nim zamieniłby trzem zwiadowcom własną kopię z kopii plikowej na drzewo gita,
//! czyli byłby zmianą w części, która działa.
//!
//! # Kształt grafu i dlaczego taki
//!
//! ```text
//!                  ┌── Add a line  (własna kopia) ──┐
//!  Get ready ──────┤                                ├──► Put them together (to samo drzewo)
//!  (własna kopia)  └── Make a file (własna kopia) ──┘
//! ```
//!
//! Mały, bo każdy krok to pieniądze. Rodzice nie są ze sobą połączeni, więc biegną
//! równocześnie — i właśnie dlatego każde z nich ma własną kopię: bez tego `workflow::check`
//! odmawia jeszcze przed startem (niezmiennik 12).
//!
//! Krok wejściowy stoi we WŁASNEJ KOPII, choć nic nie pisze. Wtedy żaden krok tego biegu nie ma
//! prawa stać w folderze człowieka, więc asercja o nietkniętym projekcie sądzi izolację silnika,
//! a nie posłuszeństwo modelu.
//!
//! Jedno z rodziców dopisuje wiersz w pliku, który stoi w commicie (zmiana ŚLEDZONA), drugie
//! zakłada plik w katalogu, którego w projekcie nie ma (zmiana NIEŚLEDZONA). To są dwie różne
//! drogi przez `fan_in::what_it_says_now`, a implementacja licząca różnicę gitem gubi drugą
//! po cichu.
//!
//! # Gdzie leży dowód i czego ten test NIE sądzi
//!
//! Nośne jest kryterium (a): złożona kopia niesie OBA pliki z ich treścią. To jest zdanie
//! o dysku, do którego inteligencja modelu nie ma wstępu — rodzic albo zapisał swój plik, albo
//! nie. Kryterium (b), `together.txt` z fragmentami obojga, jest potwierdzeniem od strony
//! produktu: agent postawiony w tej kopii naprawdę oba pliki przeczytał.
//!
//! Że (b) nie da się przejść z samych przekazań, jest tu ZAGWARANTOWANE, a nie mile widziane.
//! Prompt kroku poniżej niesie ścieżki do przekazań obojga rodziców i prawo ich otwarcia
//! (`commands::run::index_of_what_came_before`), ale przekazanie jednego rodzica nie ma jak
//! nieść zdania drugiego: rodzice biegną równocześnie, w rozłącznych kopiach, i żadne z nich
//! nie widziało zdania tamtego. Dlatego asercja jest KRZYŻOWA — w przekazaniu jednego nie ma
//! zdania drugiego. Żadne pojedyncze przekazanie nie niesie więc obu zdań, a `together.txt`
//! niesie oba.
//!
//! Kształtu przekazania nie sądzimy dalej: „agent odpowiedział jednym słowem" jest zdaniem
//! o prozie, a ten plik pyta dysk (niezmiennik 20). Nie sądzimy też kliknięcia w okno — na macOS
//! okno Tauri to `WKWebView` i nie ma czym nim wysterować.
//!
//! # Dlaczego treść czytamy z GAŁĘZI, a nie wprost z katalogu roboczego
//!
//! Bo katalogu roboczego po biegu już nie ma: `isolate::finish` zapisuje pracę kroku na jego
//! gałęzi i zdejmuje drzewo (T-95, 2026-08-23), po tym jak dziesięć biegów zostawiło
//! kilkadziesiąt pełnych kopii repozytorium. Obietnica T-52 brzmi „praca jest po biegu osiągalna
//! z gita", więc złożoną kopię wyjmujemy z powrotem na dysk jednym `git worktree add` i dopiero
//! na tych plikach asertujemy. Człowiek dostaje ją pod wypisaną ścieżką i może do niej zajrzeć.

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::isolate;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{AgentDriver, claude::ClaudeDriver};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check_to_run};
use loadout_lib::workflow::{AgentStep, Folder, Link, Step, WorkflowFile};
use serde_json::Value;
use uuid::Uuid;

/// Ile kroków ma naprawdę biec naraz. Dwa, bo dwoje rodziców ma się nałożyć w czasie —
/// równoległość jest całą przesłanką tego produktu (niezmiennik 11).
const AT_ONCE: usize = 2;

/// Ile minut wolno jednemu krokowi. Zadania są tu trywialne, więc sufit jest niski: krok bez
/// limitu i agent, który się zapętlił, to rachunek bez sufitu.
const MINUTES: u32 = 4;

/// Ile czekamy na całe flow, zanim uznamy, że wisi.
const PATIENCE: Duration = Duration::from_mins(10);

/// Ile kroków ma ten graf. Stała, bo liczba wchodzi do trzech asercji i do zdania o każdej z nich.
const STEPS: usize = 4;

/// Plik ŚLEDZONY przez gita: stoi w commicie fikstury, a pierwsze z rodziców dopisuje w nim
/// wiersz. Zmiana w takim pliku ma w różnicy gita reprezentację, więc to jest łatwiejsza połowa.
const NOTES: &str = "notes.txt";
const COMMITTED: &str = "the human wrote this line";
const LINE_ADDED: &str = "the first helper added this line";

/// Plik NIEŚLEDZONY, w katalogu, którego w projekcie nie ma — zakłada go drugie z rodziców.
/// O tym pliku git nie wie nic, więc implementacja składająca różnicę gita gubi go w ciszy.
const EXTRA: &str = "docs/extra.txt";
const FILE_MADE: &str = "the second helper made this file";

/// Plik, który ze zmian OBOJGA rodziców pisze krok poniżej.
const TOGETHER: &str = "together.txt";

/// Klucze kroków. Są też kluczami katalogów roboczych i ogonami nazw gałęzi
/// (`workflow::check::work_key_for`, `isolate::branch_for`), bo żaden z tych kroków nie biegnie
/// w więcej niż jednej kopii.
const READY: &str = "s_ready";
const ADD: &str = "s_add";
const MAKE: &str = "s_make";
const JOIN: &str = "s_join";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "uruchamia cztery prawdziwe sesje claude i za nie placi; wolaj z --ignored"]
async fn two_agents_in_their_own_copies_and_the_step_below_reads_both_files()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    println!("== folder pracy: {}", bench.root.display());

    // ── (a) JEDEN AGENT, zapisany TĄ SAMĄ funkcją, którą woła `save_agent` ──────────────────
    let helper = saved(&bench, "Helper", "Does one small thing with files", HELPER)?;

    // ── (b) WORKFLOW z czterech kroków, zapisany funkcją, którą woła okno ───────────────────
    let workflow = fan_in_workflow(&helper);
    let blockers: Vec<String> = check_to_run(&workflow)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        blockers.is_empty(),
        "the workflow would be refused before it ran, so nothing below means what it says. \
         The validator said: {blockers:?}"
    );
    // `None`: ta biblioteka jest świeża, więc plik ma tu powstać, a nie kogokolwiek nadpisać.
    let path = save_workflow_inner(&bench.home, "fan-in.json", &workflow, None)?.path;
    println!("== workflow: {}", path.display());

    // ── (c) BIEG, na PRAWDZIWYM sterowniku ─────────────────────────────────────────────────
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: &bench.home,
        project: &bench.project,
        store: &store,
        drivers: real_drivers(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: path,
        how_many_at_once: AT_ONCE,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let said = Said::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, said.channel());
    let collect = async move {
        let _ = pump.await;
    };
    let began = Instant::now();
    let (report, ()) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, sink), collect)
    })
    .await
    .map_err(|_| format!("the flow did not finish within {PATIENCE:?}"))?;
    let report = report?;
    println!("== bieg skonczyl sie po {:?}", began.elapsed());

    flow_really_finished(&report, &said)?;
    let folded = folded_copy_back_on_disk(&bench, &report)?;
    both_parents_are_in_it(&folded)?;
    neither_parent_copy_holds_the_other_half(&bench, &report)?;
    the_step_below_really_read_them(&folded)?;
    no_handoff_carries_both_sentences(&report)?;
    the_project_never_moved(&bench)?;
    nothing_is_still_running(&report)?;
    Ok(())
}

/// Czy bieg w ogóle doszedł do końca. Bez tego asercje o plikach mówią o cudzym stanie.
fn flow_really_finished(report: &RunReport, said: &Said) -> Result<(), Box<dyn Error>> {
    // Zdanie o błędzie czytamy z `run.json`, bo tam bieg ZAPISAŁ, co poszło źle.
    let book = fs::read_to_string(report.dir.join("run.json"))?;
    let blamed: Vec<String> = steps_in(&book)?
        .iter()
        .filter_map(|step| {
            let name = step.get("name").and_then(Value::as_str)?;
            let error = step.get("error").and_then(Value::as_str)?;
            Some(format!("{name}: {error}"))
        })
        .collect();
    assert!(
        blamed.is_empty(),
        "the flow has to finish without a single step blaming anything. It said: {blamed:#?}"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; STEPS],
        "all {STEPS} steps have to end `succeeded`; they ended as {:?}. The run said: {}",
        report.steps,
        said.text()
    );
    assert_eq!(
        report.outcome,
        Outcome::Done,
        "a flow nobody stopped ends on its own"
    );
    Ok(())
}

/// Wyjmuje złożoną kopię z jej gałęzi z powrotem na dysk i oddaje katalog z prawdziwymi plikami.
///
/// Powód, dla którego to jest gałąź, a nie katalog kroku, stoi w całości w nagłówku modułu:
/// katalog roboczy znika razem z końcem biegu (T-95).
fn folded_copy_back_on_disk(bench: &Bench, report: &RunReport) -> Result<PathBuf, Box<dyn Error>> {
    let mine = format!("loadout/{}/", report.id);
    let branches = isolate::branches_under(&bench.project, &mine);
    let branch = isolate::branch_for(&report.id, JOIN);
    assert!(
        branches.contains(&branch),
        "the folded copy's work has to be reachable from git after the run, and there is no \
         {branch}. That branch is where the step below left what it worked on, so without it \
         there is nothing to read. The run left: {branches:?}"
    );
    let folded = bench.root.join("folded");
    git(
        &bench.project,
        &[
            "worktree",
            "add",
            "--detach",
            &folded.display().to_string(),
            &branch,
        ],
    )?;
    println!("== zlozona kopia: {}", folded.display());
    println!("== galezie biegu: {branches:?}");
    Ok(folded)
}

/// (a) ZŁOŻONA KOPIA NIESIE PRACĘ OBOJGA RODZICÓW, co do zdania.
///
/// Najmocniejsza asercja tego pliku i jedyna, której nie da się przejść prozą: rodzic mógł
/// napisać „I added the line" i nie zapisać niczego. Pytamy dysk.
fn both_parents_are_in_it(folded: &Path) -> Result<(), Box<dyn Error>> {
    let notes = fs::read_to_string(folded.join(NOTES)).map_err(|error| {
        format!(
            "{NOTES} is not even in the folded copy at {}: {error}. It held: {:?}",
            folded.display(),
            listing(folded)
        )
    })?;
    assert!(
        notes.contains(COMMITTED),
        "the folded copy lost the line that was already in the commit, so it is not the human's \
         project with the steps' work laid on top — it is something else. {NOTES} says: {notes:?}"
    );
    assert!(
        notes.contains(LINE_ADDED),
        "the step above wrote its line into a file git tracks, and the folded copy does not have \
         it. {NOTES} says: {notes:?}"
    );
    let extra = fs::read_to_string(folded.join(EXTRA)).map_err(|error| {
        format!(
            "{EXTRA} is not in the folded copy: {error}. Git does not track that file and it sits \
             in a folder that did not exist before, so an implementation that carries only \
             tracked changes loses it silently — and silently is the whole problem. The copy \
             held: {:?}",
            listing(folded)
        )
    })?;
    assert!(
        extra.contains(FILE_MADE),
        "{EXTRA} came through empty of what the step above put in it. It says: {extra:?}"
    );
    println!("== {NOTES} w zlozonej kopii: {notes:?}");
    println!("== {EXTRA} w zlozonej kopii: {extra:?}");
    Ok(())
}

/// Ta sama asercja przyłożona do kopii KAŻDEGO z rodziców jest fałszywa — i to jest dowód, że
/// kryterium (a) rozróżnia złożenie od wybrania jednej z dwóch kopii.
///
/// Wybranie jednej jest najtańszą złą implementacją fan-inu: krok kończy się sukcesem i po cichu
/// nie widzi połowy pracy. Ta funkcja kosztuje zero pieniędzy, bo czyta gałęzie tego samego biegu.
fn neither_parent_copy_holds_the_other_half(
    bench: &Bench,
    report: &RunReport,
) -> Result<(), Box<dyn Error>> {
    let adder = isolate::branch_for(&report.id, ADD);
    let maker = isolate::branch_for(&report.id, MAKE);
    assert!(
        git(&bench.project, &["show", &format!("{adder}:{EXTRA}")]).is_err(),
        "the copy of the step that added a line also has {EXTRA}, so the folded copy could have \
         been that copy and criterion (a) would prove nothing. Something outside this run put \
         that file there"
    );
    let theirs = git(&bench.project, &["show", &format!("{maker}:{NOTES}")])?;
    assert!(
        !theirs.contains(LINE_ADDED),
        "the copy of the step that made a file already carries the other step's line, so the two \
         copies are not disjoint and criterion (a) would prove nothing. {NOTES} on {maker} says: \
         {theirs:?}"
    );
    Ok(())
}

/// (b) KROK PONIŻEJ NAPRAWDĘ OBA PLIKI PRZECZYTAŁ.
fn the_step_below_really_read_them(folded: &Path) -> Result<(), Box<dyn Error>> {
    let both = fs::read_to_string(folded.join(TOGETHER)).map_err(|error| {
        format!(
            "the step below was asked to write {TOGETHER} out of the two files it was handed, and \
             there is nothing at {}: {error}. The copy held: {:?}",
            folded.join(TOGETHER).display(),
            listing(folded)
        )
    })?;
    for wanted in [LINE_ADDED, FILE_MADE] {
        assert!(
            both.contains(wanted),
            "{TOGETHER} has to carry the text of BOTH files the steps above wrote, and \
             {wanted:?} is nowhere in it. That means the step below worked on half of what came \
             before it. It wrote: {both:?}"
        );
    }
    println!("== {TOGETHER}: {both:?}");
    Ok(())
}

/// Ani jedno przekazanie nie niesie obu zdań, więc kryterium (b) nie da się przejść z promptu.
///
/// Powód, dla którego ta asercja jest KRZYŻOWA — a nie „przekazanie mówi tylko `done`" — stoi
/// w nagłówku modułu: kształt prozy agenta nie jest tu niczyją obietnicą, a rozłączność kopii
/// obojga rodziców jest.
fn no_handoff_carries_both_sentences(report: &RunReport) -> Result<(), Box<dyn Error>> {
    let dir = report.dir.join("handoffs");
    let files = listing(&dir);
    for (whose, theirs) in [("add-a-line", FILE_MADE), ("make-a-file", LINE_ADDED)] {
        let name = files
            .iter()
            .find(|one| one.contains(whose))
            .ok_or_else(|| {
                format!(
                    "no handoff in {} comes from {whose}; it held {files:?}",
                    dir.display()
                )
            })?;
        let text = fs::read_to_string(dir.join(name))?;
        assert!(
            !text.contains(theirs),
            "{name} carries {theirs:?}, a sentence only the OTHER step above knew. The two of \
             them work at the same time in disjoint copies, so this cannot happen — and while it \
             does, the step below could have taken that sentence from this file instead of from \
             the folded copy. The handoff says:\n{text}"
        );
        println!("== przekazanie {name}:\n{text}");
    }
    Ok(())
}

/// (c) FOLDER CZŁOWIEKA JEST NIETKNIĘTY. Kroki pracują w kopiach dokładnie po to.
fn the_project_never_moved(bench: &Bench) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        fs::read_to_string(bench.project.join(NOTES))?,
        format!("{COMMITTED}\n"),
        "the project's own {NOTES} changed. Steps work in copies precisely so that the folder the \
         human is editing never moves under them"
    );
    for stray in [EXTRA, TOGETHER] {
        assert!(
            !bench.project.join(stray).exists(),
            "{stray} appeared in the project folder. Nothing a step wrote in its own copy has a \
             way back into the human's files without them asking for it"
        );
    }
    Ok(())
}

/// (d) PO BIEGU NIE ZOSTAJE ANI JEDEN ŻYWY PROCES POTOMNY.
///
/// Osierocony `claude` pali limit w tle, więc to jest błąd finansowy, nie higieniczny
/// (niezmiennik 6). Pytamy jądro, bo status zebrany przez `wait()` mówi wyłącznie o bezpośrednim
/// dziecku, a zapłacone są wnuki.
fn nothing_is_still_running(report: &RunReport) -> Result<(), Box<dyn Error>> {
    let book = fs::read_to_string(report.dir.join("run.json"))?;
    let steps = steps_in(&book)?;
    let mut asked = 0;
    for step in &steps {
        let name = step
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("a step with no name");
        assert_eq!(
            step.get("death_proof").and_then(Value::as_bool),
            Some(true),
            "\"{name}\" ended without a real proof that its process group went down. Loadout does \
             not know the state \"probably dead\": until there is a proof we treat the group as \
             alive"
        );
        let pgid = step.get("pgid").and_then(Value::as_i64).ok_or_else(|| {
            format!(
                "\"{name}\" left no process group in run.json, so there is no \
                                    address to ask about — and clean-up after a crash has \
                                    nothing to work with"
            )
        })?;
        let pgid = i32::try_from(pgid)?;
        assert!(
            nobody_is_left_in(pgid),
            "somebody is still in \"{name}\"'s process group {pgid} after the flow came back. \
             `ps` says:\n{}",
            who_is_left_in(pgid)
        );
        asked += 1;
    }
    assert_eq!(
        asked, STEPS,
        "every one of the {STEPS} steps ran a real session, so every one of them has a group to \
         ask about. Only {asked} of them did"
    );
    println!("== {asked} grup procesow, wszystkie zeszly");
    Ok(())
}

/// Wiersze kroków z `run.json`.
fn steps_in(book: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    Ok(serde_json::from_str::<Value>(book)?
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Pyta jądro, czy w grupie `pgid` został ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu.
/// Ta sama konstrukcja stoi w `tests/it/a_done_step_proves_its_group_is_dead.rs`: `kill(2)` nie
/// ma bezpiecznego opakowania w bibliotece standardowej, a ten plik jest testem, więc nie jest
/// częścią wysyłanego artefaktu (`checks/boundary.sh` czyta wyłącznie `src-tauri/src`).
#[allow(unsafe_code)]
fn nobody_is_left_in(pgid: i32) -> bool {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza tylko istnienie grupy i prawa
    // do niej. Argumenty to zwykłe liczby, więc nie ma tu wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    rc != 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Wiersze `ps` należące do tej grupy. Drugi pomiar spoza naszego drzewa procesów, wyłącznie do
/// zdania asercji: sonda mówi „ktoś tu jest", a to mówi kto.
fn who_is_left_in(pgid: i32) -> String {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-eo", "pid,pgid,args"])
        .output()
    else {
        return "ps could not be run, so there is no second opinion".to_owned();
    };
    let wanted = pgid.to_string();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.split_whitespace().nth(1) == Some(wanted.as_str()))
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prompt systemowy — krótki, bo zadanie kroku niesie całą treść.
const HELPER: &str = "You do exactly what the step asks and nothing more. Never use git.";

/* ── ZADANIA KROKÓW ────────────────────────────────────────────────────────────────────────
 * Treść, którą czyta agent. Stoją tu jako stałe, żeby kształt grafu dało się przeczytać jako
 * tabelę, i żeby zmiana zadania nie była zmianą struktury.
 *
 * Każde zadanie jest dla modelu trywialne i jednoznaczne, bo ta wyrocznia sądzi SILNIK, nie
 * inteligencję agenta: im mniej modelowi wolno zinterpretować, tym mniej ona miga.
 *
 * „Do not create, change or delete any other file" jest wymogiem MECHANIKI, nie stylu: dwa pliki
 * o tej samej ścieżce i różnych bajtach w kopiach rodziców to `fan_in::Trouble::TwoAnswers`,
 * a wtedy krok poniżej nie startuje wcale i mierzylibyśmy odmowę zamiast składania.
 *
 * „Never use git" tak samo: krok, który sam zacommituje swoją pracę, zostawia katalog czysty,
 * a `isolate::finish` czyta czysty katalog jako „nic się nie zmieniło" i zdejmuje razem z drzewem
 * także gałąź — czyli treść, którą ten test czyta po biegu. */

/// Krok wejściowy: nic nie robi. Jest, bo dwoje rodziców musi mieć wspólny początek.
const TASK_READY: &str = "This step has nothing to do. Do not create, change or delete any file \
     and never use git. Just say that everything is ready.";

/// Rodzic 1: zmiana ŚLEDZONA przez gita, w pliku, który stoi w commicie.
///
/// Zdanie do zapisania stoi tu ZA setnym bajtem instrukcji i to nie jest przypadek:
/// `commands::run::title_of` wkłada pierwsze 120 bajtów zadania w tytuł przekazania tego kroku,
/// a przekazanie jest jedną z rzeczy, które krok poniżej wolno otworzyć. Zdanie wcześniej byłoby
/// zdaniem, które agent poniżej mógł przeczytać, nie zaglądając w złożoną kopię — i asercja
/// krzyżowa z `no_handoff_carries_both_sentences` przestałaby cokolwiek rozróżniać.
const TASK_ADD: &str = "In the folder you are working in there is a file called notes.txt. Add \
     one new line at the very end of that file. The new line has to say exactly this and nothing \
     more: the first helper added this line. Keep the line that is already in the file. Do not \
     create, change or delete any other file, and never use git.";

/// Rodzic 2: zmiana NIEŚLEDZONA, w katalogu, którego w projekcie nie ma.
const TASK_MAKE: &str = "In the folder you are working in, make a folder called docs if it is \
     not there already, and create a file in it called extra.txt. The whole content of that file \
     has to be exactly this one line and nothing more: the second helper made this file. Do not \
     create, change or delete any other file, and never use git.";

/// Krok poniżej: czyta oba pliki i pisze trzeci z treścią obu.
const TASK_JOIN: &str = "In the folder you are working in there are two files: notes.txt and \
     docs/extra.txt. Read both of them. Then write a new file called together.txt whose content \
     is the text of notes.txt followed by the text of docs/extra.txt, copied exactly. Do not \
     change notes.txt or docs/extra.txt, and never use git. Then say the path you wrote.";

/// Agent zapisany na dysk produkcyjną ścieżką zapisu, gotowy do nazwania w kroku.
fn saved(bench: &Bench, name: &str, what: &str, brief: &str) -> Result<Agent, Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    name.clone_into(&mut agent.name);
    what.clone_into(&mut agent.summary);
    brief.clone_into(&mut agent.instructions);
    agent.runs_with = Vendor::ClaudeCode;
    // Pisanie plików jest tu przesłanką, nie ryzykiem: bez niego rodzice nie mają czym zapisać
    // tego, co ten test w ogóle mierzy.
    agent.file_access = FileAccess::WorkFreely;
    agent.give_up_after_minutes = MINUTES;
    /* 2026-08-29 — PUSTE, CHOĆ `Agent::example()` DAJE TU `handoffs/build.md`. Zmierzone na
     * pierwszym przebiegu tej wyroczni: pod tę ścieżkę pisze sam Loadout
     * (`commands::run::Live::file_the_answer`), a nie agent, więc oboje rodziców zostawiało
     * w swojej kopii ten sam plik z własną odpowiedzią — czyli `fan_in::Trouble::TwoAnswers`
     * i krok poniżej, który nie startuje wcale. Mierzylibyśmy odmowę zamiast składania.
     *
     * Puste pole znaczy „nie proszę o żaden plik" (`commands::run::where_results_go`) i jest tym
     * samym, co niesie definicja z importu (`import::adapters`) oraz atrapa agenta w
     * `tests/it/parents_fold_into_one_copy.rs`. */
    agent.write_results_to = String::new();
    save_agent_inner(&bench.home, &agent, None)?;
    Ok(agent)
}

/// Jeden wiersz tabeli kroków: klucz, nazwa na ekranie, zadanie i folder.
struct Planned<'a> {
    key: &'a str,
    name: &'a str,
    task: &'a str,
    folder: Folder,
}

/// Cztery kroki, dwa z nich równoległe. Kształt i powód stoją w nagłówku modułu.
///
/// TABELA, nie cztery wywołania w ciele: to samo rozwiązanie, co w `flow_todo_app::todo_workflow`
/// i z tego samego powodu — ciało, w którym treść zadania i kształt grafu są przemieszane, czyta
/// się gorzej niż lista wierszy plus jedna pętla, a sufit długości z `Cargo.toml` jest tu po coś.
fn fan_in_workflow(helper: &Agent) -> WorkflowFile {
    let plan = [
        Planned {
            key: READY,
            name: "Get ready",
            task: TASK_READY,
            folder: Folder::FreshCopy,
        },
        Planned {
            key: ADD,
            name: "Add a line",
            task: TASK_ADD,
            folder: Folder::FreshCopy,
        },
        Planned {
            key: MAKE,
            name: "Make a file",
            task: TASK_MAKE,
            folder: Folder::FreshCopy,
        },
        Planned {
            key: JOIN,
            name: "Put them together",
            task: TASK_JOIN,
            // To samo drzewo, w którym pracował krok przede mną — a przed tym krokiem stoją DWA,
            // więc ten wariant tu SKŁADA, a nie wskazuje (`workflow::Folder::SameCopy`).
            folder: Folder::SameCopy,
        },
    ];
    let arrows = [(READY, ADD), (READY, MAKE), (ADD, JOIN), (MAKE, JOIN)];
    WorkflowFile {
        format: 1,
        id: Uuid::now_v7().to_string(),
        name: "Two helpers, one copy below".to_owned(),
        description: None,
        steps: plan
            .iter()
            .map(|one| {
                Step::Agent(AgentStep {
                    id: one.key.to_owned(),
                    name: one.name.to_owned(),
                    agent: helper.id.to_string(),
                    overrides: serde_json::Map::new(),
                    vendor_options: std::collections::BTreeMap::new(),
                    copies: 1,
                    instructions: one.task.to_owned(),
                    skills: loadout_lib::workflow::Skills::default(),
                    borrow: loadout_lib::workflow::Borrow::default(),
                    folder: one.folder.clone(),
                    handover: loadout_lib::workflow::Handover::default(),
                    // Domyślne „Stop": krok, który nie przeszedł, zatrzymuje to, co po nim.
                    when_it_fails: loadout_lib::workflow::WhenItFails::default(),
                    at: loadout_lib::workflow::Point::default(),
                    extra: serde_json::Map::new(),
                })
            })
            .collect(),
        links: arrows
            .iter()
            .map(|(from, to)| Link {
                from: (*from).to_owned(),
                to: (*to).to_owned(),
                // Zwykłe „po", nie powrót: ten workflow nie ma pętli, a `Some(_)` zamieniłoby
                // każdą strzałkę w potencjalne koło (`workflow::Link::max_turns`).
                max_turns: None,
            })
            .collect(),
        extra: serde_json::Map::new(),
    }
}

/// Fabryka oddająca PRAWDZIWY sterownik dla obu vendorów.
///
/// Codex dostaje tu `ClaudeDriver` i to jest świadome: ten test nie sprawdza rozdziału vendorów,
/// a każdy krok tej fikstury i tak biegnie na Claude.
fn real_drivers() -> Drivers {
    let claude: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
    Arc::new(move |_vendor| Arc::clone(&claude))
}

/// Biblioteka i projekt tego przebiegu.
///
/// **Nie `TempDir`**: katalog ma przeżyć test, żeby człowiek mógł zajrzeć do złożonej kopii
/// i zobaczyć, co w niej naprawdę stanęło. Nazwa niesie znacznik czasu, więc drugi przebieg nie
/// wchodzi w pierwszy.
struct Bench {
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
}

impl Bench {
    /// Projekt, który JEST repozytorium gita z jednym commitem.
    ///
    /// Commit jest tu warunkiem pomiaru, nie porządkiem: bez niego `notes.txt` nie jest plikiem
    /// śledzonym, więc obie zmiany tego biegu byłyby tą samą, łatwiejszą połową.
    fn new() -> Result<Self, Box<dyn Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let root = std::env::temp_dir().join(format!("loadout-fan-in-{stamp}"));
        let home = root.join("library");
        let project = root.join("join");
        fs::create_dir_all(&project)?;
        fs::create_dir_all(project.join(".loadout"))?;
        fs::create_dir_all(&home)?;
        fs::write(project.join(NOTES), format!("{COMMITTED}\n"))?;
        // Katalog biegu leży w `.loadout/`, więc bez tego wiersza każda kopia niosłaby ze sobą
        // katalog poprzedniego biegu, a `git status` w drzewie kroku nigdy nie byłby czysty.
        fs::write(project.join(".gitignore"), ".loadout/\n")?;
        git(&project, &["init", "--quiet"])?;
        git(&project, &["add", "-A"])?;
        git(&project, &["commit", "--quiet", "-m", "the human's work"])?;
        Ok(Self {
            root,
            home,
            project,
        })
    }

    fn db(&self) -> PathBuf {
        self.project.join(".loadout").join("loadout.db")
    }
}

/// Wołanie gita z tożsamością podaną na miejscu — ta sama konstrukcja, co w
/// `tests/it/parents_fold_into_one_copy.rs`: commit fikstury nie ma prawa zależeć od tego, czy
/// ktoś ustawił `user.email` na tej maszynie, a podpisywanie czekałoby na hasło.
fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=Loadout Test"])
        .args(["-c", "user.email=test@loadout.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Co leży w katalogu — do zdania asercji, nie do logiki.
fn listing(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Paczki, które NAPRAWDĘ wyszły kanałem do okna — czyli to, co bieg powiedział człowiekowi.
#[derive(Debug, Clone, Default)]
struct Said(Arc<Mutex<Vec<Value>>>);

impl Said {
    fn channel(&self) -> tauri::ipc::Channel<Vec<loadout_lib::engine::line::Line>> {
        let sink = Arc::clone(&self.0);
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(text) = body
                && let Ok(value) = serde_json::from_str(&text)
            {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(value);
            }
            Ok(())
        })
    }

    /// Wszystko, co bieg powiedział, jednym tekstem.
    fn text(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}
