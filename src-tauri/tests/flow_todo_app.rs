//! CAŁE FLOW PRODUKTU, na PRAWDZIWYCH agentach: sześć kroków budują todo listę.
//!
//! # Po co to istnieje
//!
//! To jest wyrocznia, o którą poprosił właściciel 2026-08-18 i jedyna w tym repo, która sądzi
//! produkt jako całość: „skonfigurujesz system np 6 agentów i na tej podstawie oni zbudują prostą
//! apkę… jeśli przejdziesz całe flow bez errorów". Audyt tego samego dnia przeliczył 231 kryteriów
//! akceptacji i nie znalazł ANI JEDNEGO, które przechodzi drogę użytkownika: była pocięta na
//! cztery odcinki sądzone rozłącznymi wyroczniami, każdy zielony, przy produkcie, w którym nie
//! wystartował ani jeden bieg.
//!
//! # Dlaczego `#[ignore]`, i dlaczego to NIE jest ucieczka
//!
//! Ten test uruchamia **sześć prawdziwych sesji `claude`** i płaci za nie. `checks/full-test.sh`
//! woła `cargo test --tests` **bez** `--include-ignored`, więc bramka go nie odpala — i tak ma
//! być: kryterium, które przy każdym `verify.sh` wystawia rachunek, zostaje wyłączone przez
//! pierwszą osobę, której się spieszy. Uruchamia się je świadomie:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test flow_todo_app -- --ignored --nocapture
//! ```
//!
//! Osobny CEL testowy, nie moduł w `tests/it/`, i to jest wymóg z `docs/STATUS.md`: test, który
//! uruchamia prawdziwe procesy i mierzy stan całej maszyny, w scalonym binarium mierzyłby też
//! trzysta cudzych testów.
//!
//! # Czego to NIE dowodzi
//!
//! Nie klika po prawdziwym oknie: na macOS okno Tauri to `WKWebView` i nie ma czym nim wysterować.
//! Zaczyna dokładnie tam, gdzie kończą się kryteria frontu — na funkcjach, które wołają skorupy
//! `#[tauri::command]`, tych samych, które wołają `save_agent`, `save_workflow` i `run_workflow`.
//!
//! # Kształt grafu i dlaczego taki
//!
//! ```text
//!            ┌── Scout A (własna kopia) ──┐
//!  Lead ─────┼── Scout B (własna kopia) ──┼──► Builder ──► Checker
//!            └── Scout C (własna kopia) ──┘
//! ```
//!
//! Trzej zwiadowcy nie są ze sobą połączeni, więc biegną **równocześnie** — i właśnie dlatego
//! każdy dostaje własną kopię plików. Bez tego `workflow::check` odmawia jeszcze przed startem
//! (niezmiennik 12: dwa kroki nie mogą pisać po tych samych ścieżkach), a odmowa pada przy
//! ZAPISIE, nie w trakcie biegu. `Lead`, `Builder` i `Checker` dzielą folder projektu i wolno im,
//! bo reguła liczy osiągalność PRZECHODNIĄ: `Lead` dosięga `Buildera` przez zwiadowcę, więc nie
//! mogą biec równocześnie.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{AgentDriver, claude::ClaudeDriver};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check_to_run};
use loadout_lib::workflow::{AgentStep, Folder, Link, Step, WorkflowFile};
use tauri::ipc::{Channel, InvokeResponseBody};
use uuid::Uuid;

/// Ile kroków ma naprawdę biec naraz. Trzy, bo trzech zwiadowców ma się NAŁOŻYĆ w czasie —
/// równoległość jest całą przesłanką tego produktu (niezmiennik 11).
const AT_ONCE: usize = 3;

/// Ile minut wolno jednemu krokowi. Sufit istnieje, żeby nieudane flow kosztowało minuty,
/// a nie godziny: krok bez limitu i agent, który się zapętlił, to rachunek bez sufitu.
const MINUTES: u32 = 6;

/// Ile czekamy na całe flow, zanim uznamy, że wisi.
const PATIENCE: Duration = Duration::from_mins(20);

/// Plik, który ma powstać. Jeden, żeby „udało się" dało się sprawdzić bez czytania prozy agenta.
const WANTED: &str = "todo.html";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "uruchamia szesc prawdziwych sesji claude i za nie placi; wolaj z --ignored"]
async fn six_agents_build_a_todo_list() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    println!("== folder pracy: {}", bench.project.display());

    // ── (a) SZEŚCIU AGENTÓW, każdy zapisany TĄ SAMĄ funkcją, którą woła `save_agent` ─────────
    let lead = saved(&bench, "Lead", "Plans the work and hands the plan on", LEAD)?;
    let scout = saved(
        &bench,
        "Scout",
        "Reads what is there and reports back",
        SCOUT,
    )?;
    let builder = saved(&bench, "Builder", "Writes the files", BUILDER)?;
    let checker = saved(
        &bench,
        "Checker",
        "Opens what was built and says if it works",
        CHECKER,
    )?;
    let library = bench.home.join("agents");
    let saved_agents = fs::read_dir(&library)?.count();
    assert_eq!(
        saved_agents,
        4,
        "four agent files have to be on disk before anything runs; {} were in {}",
        saved_agents,
        library.display()
    );

    // ── (b) WORKFLOW z sześciu kroków, zapisany przez tę samą funkcję, którą woła okno ───────
    let workflow = todo_workflow(&lead, &scout, &builder, &checker);
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
    let path = save_workflow_inner(&bench.home, "todo-list.json", &workflow)?;
    println!("== workflow: {}", path.display());

    // ── (c) BIEG, na PRAWDZIWYM sterowniku ──────────────────────────────────────────────────
    let store = Store::open(&bench.db())?;
    let deps = RunDeps {
        home: &bench.home,
        project: &bench.project,
        store: &store,
        drivers: real_drivers(),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: path,
        how_many_at_once: AT_ONCE,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let seen = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, seen.channel());
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

    flow_really_finished(&bench, &report, &seen)?;
    Ok(())
}

/// Czy flow NAPRAWDĘ przeszło: nikt nie obwinia, agenci nałożyli się w czasie, plik istnieje.
///
/// Osobna funkcja, bo trzy pytania i jedno ciało testu przekraczały sufit długości z `Cargo.toml`
/// (`clippy::too_many_lines`). Podział idzie po PYTANIACH, nie po liczbie linii: „czy się udało",
/// „czy było równolegle", „czy coś powstało" — każde z nich da się przeczytać osobno.
fn flow_really_finished(
    bench: &Bench,
    report: &loadout_lib::commands::RunReport,
    seen: &Delivered,
) -> Result<(), Box<dyn Error>> {
    // ── (d) ANI JEDEN KROK NIE PADŁ ─────────────────────────────────────────────────────────
    //
    // To jest zdanie właściciela — „jeśli przejdziesz całe flow bez errorów" — zamienione
    // w asercję. Zdanie o błędzie czytamy z `run.json`, bo tam bieg ZAPISAŁ, co poszło źle:
    // pytanie o linie na ekranie odpowiadałoby na inne pytanie (co widać), a nie na to.
    let book = fs::read_to_string(report.dir.join("run.json"))?;
    let blamed: Vec<String> = serde_json::from_str::<serde_json::Value>(&book)?
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    let name = step.get("name").and_then(serde_json::Value::as_str)?;
                    let error = step.get("error").and_then(serde_json::Value::as_str)?;
                    Some(format!("{name}: {error}"))
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        blamed.is_empty(),
        "the flow has to finish without a single step blaming anything. It said: {blamed:#?}"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 6],
        "all six steps have to end `succeeded`; they ended as {:?}",
        report.steps
    );
    assert_eq!(
        report.outcome,
        Outcome::Done,
        "a flow nobody stopped ends on its own"
    );

    // ── (e) RÓWNOLEGŁOŚĆ BYŁA PRAWDZIWA ─────────────────────────────────────────────────────
    //
    // Trzej zwiadowcy mają się NAŁOŻYĆ w czasie. To jest cała przesłanka produktu i defekt,
    // na którym umarł poprzedni prototyp: `max_parallel` było tam wyłącznie szerokością wysyłki, a cztery
    // „równoległe" pasy biegły w rozłącznych oknach po pół sekundy (niezmiennik 11).
    let overlap = seen.overlapping_agents()?;
    assert!(
        overlap >= 2,
        "at least two agents have to be working in the same moment for `{AT_ONCE} at once` to \
         mean anything; the most that ever overlapped was {overlap}"
    );

    // ── (f) PLIK NAPRAWDĘ POWSTAŁ ───────────────────────────────────────────────────────────
    //
    // Najmocniejsza asercja tego pliku i jedyna, której nie da się przejść prozą: agent mógł
    // napisać „I created the todo list" i nie stworzyć niczego. Pytamy dysk.
    let built = bench.project.join(WANTED);
    assert!(
        built.is_file(),
        "the whole point of the flow is a file at the end. Nothing is at {}. The project folder \
         held: {:?}",
        built.display(),
        listing(&bench.project)
    );
    let html = fs::read_to_string(&built)?;
    for wanted in ["<html", "todo", "<script", "<style"] {
        assert!(
            html.to_lowercase().contains(wanted),
            "the file that was built has to be a working page, and `{wanted}` is nowhere in it. \
             It is {} bytes long.",
            html.len()
        );
    }
    println!("== zbudowano {} ({} bajtow)", built.display(), html.len());
    println!(
        "== przekazania: {:?}",
        listing(&report.dir.join("handoffs"))
    );
    Ok(())
}

/// Prompt systemowy zwiadowcy i reszty — krótkie, bo zadanie kroku niesie treść.
const LEAD: &str = "You plan work for other agents. Be brief and concrete. Never write code.";
const SCOUT: &str = "You look at what exists and report findings in a few short lines.";
const BUILDER: &str = "You write files. Prefer one self-contained file over many.";
const CHECKER: &str = "You check whether what was built actually works, and say so plainly.";

/// Agent zapisany na dysk przez produkcyjną ścieżkę zapisu, gotowy do nazwania w kroku.
fn saved(bench: &Bench, name: &str, what: &str, brief: &str) -> Result<Agent, Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    name.clone_into(&mut agent.name);
    what.clone_into(&mut agent.summary);
    brief.clone_into(&mut agent.instructions);
    agent.runs_with = Vendor::ClaudeCode;
    // Pisanie plików jest tu przesłanką, nie ryzykiem: bez niego `Builder` nie ma czym zbudować.
    agent.file_access = FileAccess::WorkFreely;
    agent.give_up_after_minutes = MINUTES;
    save_agent_inner(&bench.home, &agent)?;
    Ok(agent)
}

/* ── ZADANIA KROKÓW ────────────────────────────────────────────────────────────────────────
 * Treść, którą czyta agent. Stoją tu jako stałe, a nie w ciele `todo_workflow`, żeby kształt
 * grafu dało się przeczytać jako tabelę — i żeby zmiana zadania nie była zmianą struktury. */

/// Orchestrator: plan, bez kodu.
const TASK_LEAD: &str = "We are building a single-page todo list as one self-contained file \
     called todo.html. Write a short plan: what the page must do, what it must look like, and \
     what to check at the end. Three or four short sections, no code.";

/// Zwiadowca 1: co taka lista musi umieć.
const TASK_SCOUT_A: &str = "Read the plan you were handed. In at most five lines, say what a \
     todo list has to do to be useful: adding, completing, removing, and keeping items after a \
     reload. No code.";

/// Zwiadowca 2: jak ma wyglądać.
const TASK_SCOUT_B: &str = "Read the plan you were handed. In at most five lines, describe how \
     the page should look: dark background, one accent colour, readable spacing, no images. \
     No code.";

/// Zwiadowca 3: co kliknąć, żeby sprawdzić.
const TASK_SCOUT_C: &str = "Read the plan you were handed. In at most five lines, list what \
     someone should click to be sure the page works. No code.";

/// Implementer: jeden plik, bez sieci i bez budowania.
const TASK_BUILD: &str = "Read everything you were handed and write ONE file called todo.html \
     in the folder you are working in. It must be a complete page: <html>, inline <style>, \
     inline <script>, no network requests and no build step. A person must be able to add an \
     item, tick it off, delete it, and still see their items after reloading. Write the file, \
     then say the path you wrote.";

/// Tester: sprawdza i poprawia w tym samym pliku.
const TASK_CHECK: &str = "Read todo.html in the folder you are working in. Check that it is a \
     complete page and that adding, ticking off, deleting and surviving a reload are all really \
     implemented in the script. If something is missing, fix it in that same file. Then say \
     plainly whether it works.";

/// Jeden wiersz tabeli kroków: klucz, nazwa na ekranie, kto to robi, zadanie i folder.
struct Planned<'a> {
    key: &'a str,
    name: &'a str,
    who: &'a Agent,
    task: &'a str,
    folder: Folder,
}

/// Sześć kroków, trzy z nich równoległe. Kształt i powód stoją w nagłówku modułu.
///
/// TABELA, nie sześć wywołań w ciele: przy szóstym kroku ta funkcja przekraczała sufit długości
/// z `Cargo.toml` (`clippy::too_many_lines`), a sufit jest tu po coś — ciało, w którym treść
/// zadania i kształt grafu są przemieszane, czyta się gorzej niż lista wierszy plus jedna pętla.
fn todo_workflow(lead: &Agent, scout: &Agent, builder: &Agent, checker: &Agent) -> WorkflowFile {
    let plan = [
        Planned {
            key: "s_lead",
            name: "Lead",
            who: lead,
            task: TASK_LEAD,
            folder: Folder::Project,
        },
        Planned {
            key: "s_scout_a",
            name: "Scout A",
            who: scout,
            task: TASK_SCOUT_A,
            folder: Folder::FreshCopy,
        },
        Planned {
            key: "s_scout_b",
            name: "Scout B",
            who: scout,
            task: TASK_SCOUT_B,
            folder: Folder::FreshCopy,
        },
        Planned {
            key: "s_scout_c",
            name: "Scout C",
            who: scout,
            task: TASK_SCOUT_C,
            folder: Folder::FreshCopy,
        },
        Planned {
            key: "s_build",
            name: "Builder",
            who: builder,
            task: TASK_BUILD,
            folder: Folder::Project,
        },
        Planned {
            key: "s_check",
            name: "Checker",
            who: checker,
            task: TASK_CHECK,
            folder: Folder::Project,
        },
    ];
    let arrows = [
        ("s_lead", "s_scout_a"),
        ("s_lead", "s_scout_b"),
        ("s_lead", "s_scout_c"),
        ("s_scout_a", "s_build"),
        ("s_scout_b", "s_build"),
        ("s_scout_c", "s_build"),
        ("s_build", "s_check"),
    ];
    WorkflowFile {
        format: 1,
        id: Uuid::now_v7().to_string(),
        name: "Build a todo list".to_owned(),
        description: None,
        steps: plan
            .iter()
            .map(|one| {
                Step::Agent(AgentStep {
                    id: one.key.to_owned(),
                    name: one.name.to_owned(),
                    agent: one.who.id.to_string(),
                    overrides: serde_json::Map::new(),
                    vendor_options: std::collections::BTreeMap::new(),
                    copies: 1,
                    instructions: one.task.to_owned(),
                    skills: loadout_lib::workflow::Skills::default(),
                    folder: one.folder.clone(),
                    handover: loadout_lib::workflow::Handover::default(),
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
/// Codex dostaje tu `ClaudeDriver` i to jest świadome: ten test nie sprawdza rozdziału vendorów
/// (robi to `Absent` w `lib.rs`), a każdy agent w tej fiksturze i tak biegnie na Claude.
fn real_drivers() -> Drivers {
    let claude: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
    Arc::new(move |_vendor| Arc::clone(&claude))
}

/// Biblioteka i projekt tego przebiegu.
///
/// **Nie `TempDir`**: katalog ma przeżyć test, żeby człowiek mógł otworzyć `todo.html` i zobaczyć,
/// co powstało. Nazwa niesie znacznik czasu, więc drugi przebieg nie wchodzi w pierwszy.
struct Bench {
    home: PathBuf,
    project: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let root = std::env::temp_dir().join(format!("loadout-flow-{stamp}"));
        let home = root.join("library");
        let project = root.join("todo");
        fs::create_dir_all(&project)?;
        fs::create_dir_all(project.join(".loadout"))?;
        fs::create_dir_all(&home)?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.join(".loadout").join("loadout.db")
    }
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

/// Wiersze, które NAPRAWDĘ wyszły kanałem do okna.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<std::sync::Mutex<Vec<(Instant, InvokeResponseBody)>>>);

impl Delivered {
    fn channel(&self) -> Channel<Vec<loadout_lib::engine::line::Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            let at = Instant::now();
            if let Ok(mut seen) = seen.lock() {
                seen.push((at, body));
            }
            Ok(())
        })
    }

    /// Ilu agentów najwięcej mówiło w tym samym oknie czasu.
    ///
    /// Chwila ODBIORU paczki, bo `Line` nie niesie znacznika i nie ma nieść. Okno dwóch sekund
    /// jest tu z rozmysłem szerokie: pytanie brzmi „czy pracowali razem", a nie „czy odpowiedzieli
    /// w tej samej milisekundzie". Wersja pytająca o liczbę zakończeń przechodziłaby dla biegu
    /// sekwencyjnego — i to jest dokładnie ten fałszywy dowód równoległości, który miał poprzedni prototyp.
    fn overlapping_agents(&self) -> Result<usize, Box<dyn Error>> {
        let seen = self
            .0
            .lock()
            .map_err(|error| format!("the recorder was poisoned: {error}"))?;
        let mut stamped: Vec<(Instant, String)> = Vec::new();
        for (at, body) in seen.iter() {
            for row in body.clone().deserialize::<Vec<serde_json::Value>>()? {
                if let Some(agent) = row.get("agent").and_then(serde_json::Value::as_str) {
                    stamped.push((*at, agent.to_owned()));
                }
            }
        }
        let window = Duration::from_secs(2);
        let mut most = 0;
        for (anchor, _) in &stamped {
            let mut names: Vec<&str> = stamped
                .iter()
                .filter(|(at, _)| at.duration_since(*anchor) < window)
                .map(|(_, name)| name.as_str())
                .collect();
            names.sort_unstable();
            names.dedup();
            most = most.max(names.len());
        }
        Ok(most)
    }
}
