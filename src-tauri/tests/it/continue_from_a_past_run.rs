//! Wznowienie z historii puszcza wskazany krok **i wszystko po nim** — z wejściem tamtego biegu.
//!
//! # Co to mierzy
//!
//! 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
//! Bieg, który padł na siódmym kroku z dziesięciu, ma sześć kroków skończonych, których nikt nie
//! chce powtarzać, i trzy, które nigdy nie ruszyły. Do tego dnia produkt umiał dwie rzeczy i żadna
//! z nich nie była tą: puścić CAŁY graf od zera albo powtórzyć DOKŁADNIE JEDEN kafelek
//! (`commands::Part::Just`, `rerun::again`).
//!
//! # SŁABĄ WERSJĄ jest „drugi krok pobiegł"
//!
//! Przechodzi ją `Part::Just(["s_two"])`, czyli funkcja, która stała w produkcie już wczoraj —
//! a wtedy „kontynuuj" znaczy „powtórz jeden kafelek" i trzeci krok nigdy nie ruszy. Rozróżniają
//! to trzy asercje naraz i wszystkie trzy są niżej:
//!
//! * (a) pierwszy krok NIE pobiegł — inaczej to jest zwykły bieg od zera,
//! * (b) drugi i trzeci pobiegły — inaczej to jest `Part::Just`,
//! * (c) trzeci ruszył PO drugim, nie razem z nim — czyli strzałki wewnątrz wycinka zostały.
//!
//! (c) jest tą, której nie da się ograć: wycinek bez strzałek wypuszcza oba kroki naraz, więc
//! przy puli jednego miejsca kolejność byłaby przypadkowa, a przy większej — równoczesna.
//!
//! # I czwarta, w drugą stronę
//!
//! (d) wznowiony krok dostaje na wejściu PRZEKAZANIE z tamtego biegu. Bez niego wznowienie jest
//! nowym biegiem z pustym kontekstem: agent pracuje od zera nad czymś, co reszta grafu już
//! zrobiła, i płaci za to u vendora.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest, rerun};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;

const PATIENCE: Duration = Duration::from_secs(30);

/// Skrypt, który czeka `$3` sekund, DOPISUJE swoją nazwę do wspólnego pliku i schodzi zerem.
///
/// Dopisuje, nie nadpisuje, bo plik ma być DZIENNIKIEM KOLEJNOŚCI, a nie listą tego, co pobiegło.
///
/// SEN JEST TU PO TO, ŻEBY (c) MIAŁO CO ROZRÓŻNIAĆ, i to jest zmierzone: bez niego wycinek
/// pozbawiony strzałek wypuszcza oba kroki naraz i one MIMO TO zapisują się w kolejności węzłów
/// — czyli asercja o kolejności świeci, nie sądząc niczego. Kiedy krok wskazany trwa sekundę,
/// a następny po nim jest natychmiastowy, dziennik bez strzałek wychodzi ODWRÓCONY.
const NOTES: &str = r#"#!/bin/sh
sleep "$3"
printf '%s\n' "$2" >> "$1"
echo "1 passed"
exit 0
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn it_picks_up_at_the_named_step_and_walks_the_rest_of_the_graph()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let log = bench.project.path().join("who-ran");
    let notes = bench.script("notes.sh", NOTES)?;
    let workflow = bench.workflow("three-steps", &three_steps(&notes, &log))?;
    let store = Store::open(&bench.db())?;
    let processes = Arc::new(Processes::new());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: no_drivers(),
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };

    // ── PIERWSZY BIEG, CAŁY. To jest ten, który leży potem w historii ──────────────────────
    let first = one_run(
        &deps,
        &RunRequest {
            workflow: workflow.clone(),
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await??;
    assert_eq!(
        fs::read_to_string(&log)?,
        "s_one\ns_two\ns_three\n",
        "the fixture itself is wrong if the plain run does not walk the graph in order"
    );
    fs::write(&log, "")?;

    // ── WZNOWIENIE OD DRUGIEGO KROKU, przez tę samą drogę, którą idzie ekran ───────────────
    let folder = first
        .dir
        .file_name()
        .and_then(|one| one.to_str())
        .ok_or("the run directory has no name")?;
    let again = rerun::onward(bench.home.path(), bench.project.path(), folder, "s_two", 2)?;
    let report = one_run(&deps, &again.request).await??;

    let who = fs::read_to_string(&log)?;

    // ── (a) PIERWSZY KROK NIE POBIEGŁ ─────────────────────────────────────────────────────
    assert!(
        !who.contains("s_one"),
        "the step before the named one ran again. That is a run from zero wearing another name — \
         and it is exactly the forty-eight minutes the owner did not want to pay twice. Got: \
         {who:?}"
    );

    // ── (b) DRUGI I TRZECI POBIEGŁY ───────────────────────────────────────────────────────
    // Bez trzeciego to jest `Part::Just`, czyli funkcja, która stała w produkcie już wczoraj.
    let mut ran: Vec<&str> = who.lines().collect();
    ran.sort_unstable();
    assert_eq!(
        ran,
        vec!["s_three", "s_two"],
        "picking up at a step means that step AND everything the graph puts after it. A run that \
         stops after the named tile is `run this one tile again`, which this product already had. \
         Got: {who:?}"
    );

    // ── (c) I TRZECI PO DRUGIM, NIE RAZEM Z NIM ───────────────────────────────────────────
    // Tej nie zazieleni wycinek bez strzałek: taki wypuszcza oba kroki w tej samej chwili.
    assert_eq!(
        who.lines().collect::<Vec<_>>(),
        vec!["s_two", "s_three"],
        "the order is the arrow the person drew. A slice with its arrows stripped releases both \
         steps at once, and then this file says whatever the scheduler happened to do first"
    );
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "two steps in the report, both finished — one per tile that was supposed to run"
    );

    // ── (d) I WEJŚCIE PRZYSZŁO Z TAMTEGO BIEGU ────────────────────────────────────────────
    let carried = report.dir.join("handoffs");
    /* Po fragmencie nazwy, nie po całej: przekazanie nazywa się `NN__<krok>__findings.md`, a NN
     * jest pozycją w kolejności zapisu — czyli liczbą, która zmienia się z kształtem grafu.
     * Asercja na pełnej nazwie sądziłaby numerację, a pytanie brzmi „czy praca tamtego kroku tu
     * dojechała". */
    let names: Vec<String> = fs::read_dir(&carried)
        .map(|entries| {
            entries
                .flatten()
                .map(|one| one.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        names.iter().any(|one| one.contains("s-one")),
        "the handoff of the step that did NOT run again has to be in this run's folder: it is the \
         whole reason the picked-up step has anything to work from. Without it the agent starts \
         from an empty context and pays a vendor to redo what the graph already did. Found: \
         {names:?}"
    );
    Ok(())
}

/// Trzy kroki w łańcuchu, każdy dopisujący swoją nazwę do wspólnego dziennika.
fn three_steps(notes: &Path, log: &Path) -> String {
    let step = |id: &str, y: i32, waits: &str| {
        format!(
            r#"    {{
      "kind": "check",
      "id": "{id}",
      "name": "{id}",
      "command": "{notes} {log} {id} {waits}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 24, "y": {y} }}
    }}"#,
            notes = notes.display(),
            log = log.display(),
        )
    };
    format!(
        r#"{{
  "format": 1,
  "id": "wf_three_steps",
  "name": "Three steps",
  "steps": [
{},
{},
{}
  ],
  "links": [
    {{ "from": "s_one", "to": "s_two" }},
    {{ "from": "s_two", "to": "s_three" }}
  ]
}}"#,
        step("s_one", 24, "0"),
        // SEKUNDA TYLKO TUTAJ. Krok wznowienia trwa, następny po nim jest natychmiastowy — więc
        // dziennik odróżnia „poszedł po nim" od „wypuszczono oba naraz".
        step("s_two", 168, "1"),
        step("s_three", 312, "0"),
    )
}

async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let drain = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
}

/// Fabryka, która PANIKUJE: żaden krok w tym pliku nie ma vendora.
fn no_drivers() -> Drivers {
    Arc::new(|_| -> Arc<dyn AgentDriver> {
        panic!("no step in this workflow names a vendor, so nothing may ask for an agent driver")
    })
}

struct Bench {
    home: TempDir,
    project: TempDir,
    scripts: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let scripts = TempDir::new()?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self {
            home,
            project,
            scripts,
        })
    }

    fn script(&self, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.scripts.path().join(name);
        fs::write(&path, body)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
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

/* 2026-08-23 — I DRUGA POŁOWA TEJ NAPRAWY, CZYLI TA, KTÓREJ BRAKOWAŁO.
 *
 * Wznowienie ma zacząć od PRACY poprzedniego biegu, nie od czystego `HEAD` — i to jest defekt
 * zmierzony na `urc-monorepo`: krok „Front" dostał pusty checkout i zaczął przepisywać 164 pliki,
 * które poprzedni bieg zacommitował jedną gałąź obok.
 *
 * Naprawa ma dwa kawałki i tylko jeden był sprawdzony. `isolate::make_from` sądzi
 * `resume_starts_from_the_work_that_was_done.rs`: podaj punkt startu, a drzewo go dostanie.
 * Czego nikt nie sądził, to KTO TEN PUNKT LICZY — `commands::run::where_it_left_off` czyta
 * `run.json` poprzedniego biegu, składa nazwę gałęzi i sprawdza, czy istnieje.
 *
 * SŁABĄ WERSJĄ jest więc każde z tamtych dwóch kryteriów osobno: gdyby ta funkcja zawsze oddawała
 * `None` — literówka w nazwie pliku, inny klucz w JSON-ie, `seeded_from`, które nie dojeżdża —
 * OBA zostają zielone, a wznowienie po cichu wraca do `HEAD`. Kryterium świecące nad martwą
 * funkcją jest dokładnie tą klasą wady, dla której to repo powstało.
 *
 * Dlatego niżej biegnie PRAWDZIWY bieg, potem PRAWDZIWE wznowienie, a wyrocznią jest licznik
 * w pliku: krok dopisuje do niego linię, więc drzewo odbite od gałęzi poprzedniego biegu daje
 * DWIE, a odbite od `HEAD` — jedną. Jedna liczba, dwa nierozróżnialne inaczej stany.
 */

/// Krok, który DOPISUJE linię do pliku w swoim drzewie roboczym.
///
/// Dopisuje, nie nadpisuje, i to jest cała wyrocznia: liczba linii mówi, ILE RAZY ten krok
/// pobiegł nad tym samym drzewem — a to jest dokładnie pytanie „czy wznowienie widziało
/// poprzednią pracę".
const APPENDS: &str = r#"#!/bin/sh
printf 'one more\n' >> THE-WORK.md
echo "1 passed"
exit 0
"#;

/// Krok PO nim: pracuje w tej samej kopii i **nic w niej nie zmienia**.
///
/// Nie dopisuje, bo licznik ma mierzyć jeden krok, nie dwa. Sprawdza za to, że praca poprzednika
/// jest na miejscu — bez tego bieg przeszedłby także nad drzewem, w którym nic nie ma.
const LOOKS: &str = r#"#!/bin/sh
test -f THE-WORK.md || exit 1
echo "1 passed"
exit 0
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picked_up_step_opens_the_tree_where_that_step_left_off() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    a_repo_with_one_commit(bench.project.path())?;
    let appends = bench.script("appends.sh", APPENDS)?;
    let looks = bench.script("looks.sh", LOOKS)?;
    let workflow = bench.workflow("build-then-look", &build_then_look(&appends, &looks))?;
    let store = Store::open(&bench.db())?;
    let processes = Arc::new(Processes::new());
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: no_drivers(),
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };

    // ── PIERWSZY BIEG. Zostawia `THE-WORK.md` z jedną linią, na własnej gałęzi ─────────────
    let first = one_run(
        &deps,
        &RunRequest {
            workflow: workflow.clone(),
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        },
    )
    .await??;
    assert_eq!(
        lines_of(&first.dir.join("work/s_build/THE-WORK.md")),
        1,
        "the fixture is wrong if the first run does not leave exactly one line behind"
    );

    // ── WZNOWIENIE OD TEGO SAMEGO KROKU ───────────────────────────────────────────────────
    let folder = first
        .dir
        .file_name()
        .and_then(|one| one.to_str())
        .ok_or("the run directory has no name")?;
    let again = rerun::onward(
        bench.home.path(),
        bench.project.path(),
        folder,
        "s_build",
        1,
    )?;
    let second = one_run(&deps, &again.request).await??;

    assert_eq!(
        lines_of(&second.dir.join("work/s_build/THE-WORK.md")),
        2,
        "the picked-up step opened a tree that does not carry what it wrote last time. One line \
         means it started from HEAD — a clean checkout — and did its work over again from \
         nothing. That is the owner's defect exactly, and it survives BOTH of the other two \
         criteria: one of them proves that `make_from` honours a starting point it is handed, \
         the other proves which steps run. Neither asks who works out the starting point."
    );
    Ok(())
}

/// Ile linii ma ten plik. `0`, kiedy pliku nie ma — czyli „krok niczego nie zostawił".
fn lines_of(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

/// Projekt, który JEST repozytorium gita — bez tego nie ma gałęzi, więc nie ma czego wznawiać.
fn a_repo_with_one_commit(at: &Path) -> Result<(), Box<dyn Error>> {
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "test@example.test"],
        vec!["config", "user.name", "Test"],
    ] {
        Command::new("git").args(args).current_dir(at).status()?;
    }
    fs::write(at.join("README.md"), "one\n")?;
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(at)
        .status()?;
    Command::new("git")
        .args(["commit", "--quiet", "-m", "first"])
        .current_dir(at)
        .status()?;
    Ok(())
}

/// Krok pracujący we własnej kopii, a po nim sprawdzenie w tej samej kopii.
fn build_then_look(appends: &Path, looks: &Path) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_build_then_look",
  "name": "Build, then look",
  "steps": [
    {{
      "kind": "check",
      "id": "s_build",
      "name": "Build",
      "command": "{appends}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 24, "y": 24 }}
    }},
    {{
      "kind": "check",
      "id": "s_look",
      "name": "Look",
      "command": "{looks}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "same-copy" }},
      "at": {{ "x": 24, "y": 168 }}
    }}
  ],
  "links": [{{ "from": "s_build", "to": "s_look" }}]
}}"#,
        appends = appends.display(),
        looks = looks.display(),
    )
}
