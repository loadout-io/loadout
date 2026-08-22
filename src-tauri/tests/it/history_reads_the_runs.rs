//! Historia biegów TEGO projektu: lista z dysku, jeden nieczytelny bieg jako WIERSZ,
//! i otwarty bieg czytany bez bazy.
//!
//! Trzy pytania, jedno na każdy plik tego zadania, i wszystkie trzy o pliki, nie o funkcje:
//!
//!   1. czy lista jest listą **tego** projektu i tylko jego (zamówienie właściciela: „pamiętaj
//!      że wszystko ma być per workspace ta historia"),
//!   2. czy jeden bieg z zepsutym `run.json` zostaje **jedną pozycją z uczciwym zdaniem**,
//!      zamiast zabrać całą historię (niezmiennik 5),
//!   3. czy otwarty bieg oddaje to, co po nim naprawdę zostało: kroki, przekazania i zapisany
//!      strumień — i czy ten strumień czyta się **tą samą kuracją**, którą widział człowiek
//!      w trakcie biegu (niezmienniki 15 i 23).
//!
//! # Słabe wersje tych kryteriów i dlaczego ich tu nie ma
//!
//! **`assert_eq!(list_runs_inner(project).len(), 4)`.** Przechodzi implementacja, która oddaje
//! same nazwy katalogów i nie otwiera ani jednego pliku — czyli dokładnie ta, która na ekranie
//! pokazuje cztery puste wiersze. Rozróżnia je treść: tytuł, stan, liczba kroków i suma kosztów
//! muszą przyjść **z pliku**, a plik jest tu wypisany literalnie, nigdy przez kod produkcyjny.
//!
//! **Test, który zepsuty `run.json` sprawdza samym `is_some()` na zdaniu.** Przechodzi na
//! implementacji, która oddaje jedną pozycję i gubi trzy pozostałe — a to jest ta awaria, która
//! naprawdę boli: człowiek edytuje jeden plik i traci widok całej historii. Rozróżnia je
//! **liczba i kolejność** wierszy w tym samym wywołaniu, w którym stoi zepsuty bieg.
//!
//! **Test, który zapisany strumień porównuje z listą wierszy wpisaną tutaj z palca.** Przechodzi
//! na drugiej, własnej kuracji — czyli na tym, przed czym stoi niezmiennik 15. Rozróżnia je
//! porównanie z wierszami, które w tym samym biegu testu wyprodukowała **żywa pompa**
//! (`engine::stream::pump`) z tych samych bajtów: te dwie drogi mają dać co do wiersza to samo,
//! a jeżeli kiedykolwiek dadzą co innego, to znaczy, że historia opowiada inną historię niż
//! ekran, na który człowiek patrzył.
//!
//! Pliki wypisujemy **literalnie**, nigdy przez `commands::run::spill` ani `write_handoff`:
//! odczyt, który czyta tylko to, co sam zapisał, nie odpowiada na pytanie o niezmiennik 4 ani
//! trochę. To jest ta sama zasada, którą zapisano w `memory_handoff_scan.rs`.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use loadout_lib::commands::history::{list_runs_inner, read_run_inner};
use loadout_lib::engine::line::{Line, LineKind};
use loadout_lib::engine::stream;
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Złoty plik z prawdziwego biegu — ten sam, którym `stream_curation_fixture` mierzy kurację.
const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../docs/research/fixtures/claude-stream.jsonl"
));

/// Ile bajtów ma mieć fikstura. Asercja, nie komentarz: przycięta mierzyłaby krótszy strumień.
const FIXTURE_BYTES: usize = 25_584;

/// Katalogi biegów, od najstarszego. Kolejność na liście ma być odwrotna.
const OLDEST: &str = "20260810-081500__0198a1f2-3b4c-7d5e-8f60-000000000001";
const MIDDLE_TORN: &str = "20260812-101112__0198a1f2-3b4c-7d5e-8f60-000000000002";
const MIDDLE_BARE: &str = "20260814-235959__0198a1f2-3b4c-7d5e-8f60-000000000003";
const NEWEST: &str = "20260816-194804__0198a1f2-3b4c-7d5e-8f60-000000000004";

/// Krok, którego strumień naprawdę zapisano. Nazwa pliku transkryptu bierze się z tego napisu.
const BUILD_STEP: &str = "0198a1f2-3b4c-7d5e-8f60-00000000000b";

/// Nazwa kafelka, którą niesie każdy wiersz tego strumienia.
const BUILDER: &str = "Build";

/// Proza z fikstury, dosłownie. Jedyny tekst, który w tym strumieniu wolno pokazać.
const PROSE: &str = "Greeting message stored in file.";

/// `run.json` najnowszego biegu, wypisany literalnie — dwa kroki, oba z kosztem.
const NEWEST_DESCRIPTION: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000004",
  "workflow_id": "ship-a-feature.json",
  "workflow_hash": "0123456789abcdef",
  "workflow_snapshot": {"format": 1},
  "title": "Ship a feature",
  "status": "succeeded",
  "concurrency": 3,
  "created_at": 1755373684000,
  "started_at": 1755373685000,
  "ended_at": 1755373991000,
  "error": null,
  "steps": [
    {
      "id": "0198a1f2-3b4c-7d5e-8f60-00000000000a",
      "node_key": "plan",
      "name": "Plan",
      "agent": "claude",
      "kind": "agent",
      "depends_on": [],
      "status": "succeeded",
      "attempt": 0,
      "cost_usd": 0.25,
      "summary": "Wrote the plan for the greeting file.",
      "error": null
    },
    {
      "id": "0198a1f2-3b4c-7d5e-8f60-00000000000b",
      "node_key": "build",
      "name": "Build",
      "agent": "claude",
      "kind": "agent",
      "depends_on": ["plan"],
      "status": "failed",
      "attempt": 1,
      "cost_usd": 0.75,
      "summary": "Stored the greeting.",
      "error": "The check would not run."
    }
  ]
}"#;

/// `run.json` najstarszego biegu — jeden krok, bez kosztu i bez zdania.
const OLDEST_DESCRIPTION: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000001",
  "workflow_id": "look-around.json",
  "workflow_hash": "fedcba9876543210",
  "workflow_snapshot": {"format": 1},
  "title": "Look around",
  "status": "cancelled",
  "concurrency": 1,
  "created_at": 1754812500000,
  "started_at": 1754812501000,
  "ended_at": 1754812600000,
  "error": null,
  "steps": [
    {
      "id": "0198a1f2-3b4c-7d5e-8f60-00000000000c",
      "node_key": "look",
      "name": "Look",
      "agent": "claude",
      "kind": "agent",
      "depends_on": [],
      "status": "cancelled",
      "attempt": 0,
      "cost_usd": null,
      "summary": null,
      "error": null
    }
  ]
}"#;

/// Opis, który jest na dysku i nie jest JSON-em. Tak wygląda plik po ręcznej edycji.
const TORN_DESCRIPTION: &str = "{\"title\": \"Half a file\", \"steps\": [";

/// Przekazanie najnowszego biegu, wypisane literalnie — front-matter i ciało, jak na dysku.
const HANDOFF_FILE: &str = "---
id: h_01K9F3Q0MZ
run: 0198a1f2-3b4c-7d5e-8f60-000000000004
step: 1
from: Plan
to: [Build]
kind: plan
title: What we are building
status: current
supersedes: null
reads: []
created: 2026-08-16T19:48:10Z
---
## Answer
Write the greeting into a file.
";

/// Projekt z czterema biegami, z których dwa nie dają się przeczytać.
///
/// Wypisane literalnie i pojedynczo, bez ani jednego wywołania kodu produkcyjnego: to jest cała
/// treść pytania „czy pliki są prawdą" (niezmiennik 4).
fn project_with_four_runs(root: &Path) -> PathBuf {
    let project = root.join("ledger-ui");
    let runs = project.join(".loadout").join("runs");

    std::fs::create_dir_all(runs.join(NEWEST).join("handoffs")).unwrap();
    std::fs::create_dir_all(runs.join(NEWEST).join("logs")).unwrap();
    std::fs::write(runs.join(NEWEST).join("run.json"), NEWEST_DESCRIPTION).unwrap();
    std::fs::write(
        runs.join(NEWEST).join("handoffs").join("01__Plan__plan.md"),
        HANDOFF_FILE,
    )
    .unwrap();

    std::fs::create_dir_all(runs.join(OLDEST)).unwrap();
    std::fs::write(runs.join(OLDEST).join("run.json"), OLDEST_DESCRIPTION).unwrap();

    // Bieg, którego opis leży na dysku i nie da się go przeczytać.
    std::fs::create_dir_all(runs.join(MIDDLE_TORN)).unwrap();
    std::fs::write(runs.join(MIDDLE_TORN).join("run.json"), TORN_DESCRIPTION).unwrap();

    // Bieg, po którym został sam katalog. Tak wygląda bieg ubity w połowie pierwszego zapisu.
    std::fs::create_dir_all(runs.join(MIDDLE_BARE).join("logs")).unwrap();

    project
}

/// Drugi projekt obok, z własnym biegiem. Ani jeden jego wiersz nie ma prawa wejść na tamtą listę.
fn project_next_door(root: &Path) -> PathBuf {
    let project = root.join("somebody-elses-app");
    let runs = project.join(".loadout").join("runs");
    let folder = "20260817-000000__0198a1f2-3b4c-7d5e-8f60-0000000000ff";
    std::fs::create_dir_all(runs.join(folder)).unwrap();
    std::fs::write(
        runs.join(folder).join("run.json"),
        OLDEST_DESCRIPTION.replace("Look around", "Not your history"),
    )
    .unwrap();
    project
}

#[test]
fn the_list_belongs_to_this_project_and_starts_with_the_newest_run() {
    let root = tempfile::tempdir().unwrap();
    let project = project_with_four_runs(root.path());
    let neighbour = project_next_door(root.path());

    let listed = list_runs_inner(&project);
    let folders: Vec<&str> = listed.iter().map(|one| one.folder.as_str()).collect();
    assert_eq!(
        folders,
        vec![NEWEST, MIDDLE_BARE, MIDDLE_TORN, OLDEST],
        "every run of this project has to stand on the list, newest first. A run missing here \
         is a run a person cannot reach at all, and the wrong order is a list whose first row \
         is not the thing that just happened."
    );

    let neighbours = list_runs_inner(&neighbour);
    assert_eq!(
        neighbours.len(),
        1,
        "the folder next door has exactly one run of its own, and this call has to read THAT \
         folder rather than anything cached from the call above"
    );
    assert!(
        listed.iter().all(|one| one.title != "Not your history"),
        "a run belonging to another folder reached this folder's list. The owner asked for this \
         to be per folder, and the only thing keeping it per folder is that the scan never \
         leaves the folder it was given."
    );
}

#[test]
fn the_rows_carry_what_the_files_say_not_what_the_scan_made_up() {
    let root = tempfile::tempdir().unwrap();
    let project = project_with_four_runs(root.path());
    let listed = list_runs_inner(&project);

    let newest = listed
        .iter()
        .find(|one| one.folder == NEWEST)
        .expect("the newest run is on the list");
    assert_eq!(
        newest.title, "Ship a feature",
        "the row has to carry the name the workflow gives itself, read out of the file. A row \
         showing the file name instead is a row nobody recognises."
    );
    assert_eq!(
        newest.state, "succeeded",
        "the row has to carry the word the file wrote, so the window can turn it into the one \
         a person reads. Anything else here is a guess about how the run ended."
    );
    assert_eq!(
        newest.steps, 2,
        "two steps are written in that file, so two is the only honest answer"
    );
    assert_eq!(
        newest.cost_usd,
        Some(1.0),
        "the row has to add up what the steps really cost. This run says 0.25 and 0.75."
    );
    assert_eq!(
        newest.when, "2026-08-16 19:48",
        "the row has to say when it ran, in a form a person reads at a glance. The folder name \
         carries that moment and it is the only thing that is there even for a run whose \
         description cannot be read."
    );
    assert!(
        newest.said.is_none(),
        "this run reads perfectly, so nothing may stand in the row that is reserved for saying \
         that it does not: {:?}",
        newest.said
    );

    let oldest = listed
        .iter()
        .find(|one| one.folder == OLDEST)
        .expect("the oldest run is on the list");
    assert_eq!(
        oldest.cost_usd, None,
        "not one step of that run said what it cost. Nothing measured and cost nothing are two \
         different sentences on a screen, and only None can carry the first one."
    );
}

#[test]
fn a_run_that_cannot_be_read_is_one_row_with_an_honest_sentence() {
    let root = tempfile::tempdir().unwrap();
    let project = project_with_four_runs(root.path());
    let listed = list_runs_inner(&project);

    assert_eq!(
        listed.len(),
        4,
        "two of these four runs cannot be read, and both of them still have to be rows. A scan \
         that gives up on the first unreadable file turns one hand-edited file into an empty \
         history — and that is the failure a person meets after touching one file."
    );

    let torn = listed
        .iter()
        .find(|one| one.folder == MIDDLE_TORN)
        .expect("the run with the half-written description is on the list");
    let bare = listed
        .iter()
        .find(|one| one.folder == MIDDLE_BARE)
        .expect("the run with nothing but a folder is on the list");

    let torn_said = torn.said.clone().unwrap_or_default();
    let bare_said = bare.said.clone().unwrap_or_default();
    assert!(
        !torn_said.is_empty() && !bare_said.is_empty(),
        "a row Loadout cannot read has to SAY so. Silence there looks exactly like a run that \
         did nothing, and a person has no way of telling those two apart. Got {torn_said:?} and \
         {bare_said:?}"
    );
    assert_ne!(
        torn_said, bare_said,
        "there is a file to look at in one case and nothing left in the other, so the two \
         sentences answer different questions and cannot be the same sentence"
    );
    assert_eq!(
        torn.when, "2026-08-12 10:11",
        "even a run whose description is gibberish still has the moment it ran, and it is the \
         only thing that tells one unreadable row from another"
    );
    assert!(
        torn.title.is_empty() && torn.state.is_empty() && torn.steps == 0,
        "nothing may be invented for a run that could not be read. A made-up title reads exactly \
         like a real one."
    );
}

#[test]
fn a_folder_that_never_ran_anything_is_an_empty_list_and_not_a_failure() {
    let root = tempfile::tempdir().unwrap();
    let fresh = root.path().join("brand-new");
    std::fs::create_dir_all(&fresh).unwrap();

    assert!(
        list_runs_inner(&fresh).is_empty(),
        "a folder where nothing has ever run has no history, and that is a normal state, not a \
         broken disk. A red bar on a fresh install teaches people to ignore red bars."
    );
}

#[test]
fn opening_a_run_gives_its_steps_and_what_they_passed_on() {
    let root = tempfile::tempdir().unwrap();
    let project = project_with_four_runs(root.path());

    let opened = read_run_inner(&project, NEWEST).expect("that run is right there on disk");
    assert_eq!(
        opened.title, "Ship a feature",
        "the open run has to name itself the way the file names it"
    );

    let names: Vec<&str> = opened.steps.iter().map(|one| one.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Plan", "Build"],
        "the steps have to come back in the order the file wrote them, which is the order of \
         the graph. Order by whatever the file system felt like would read differently every \
         time the screen is opened."
    );

    let build = opened
        .steps
        .iter()
        .find(|one| one.name == BUILDER)
        .expect("the second step is there");
    assert_eq!(
        build.summary, "Stored the greeting.",
        "the one sentence a step left behind is the whole reason for opening it again"
    );
    assert_eq!(
        build.error, "The check would not run.",
        "and the reason it did not work has to come back too, or the screen shows a step that \
         failed and no reason for it"
    );
    assert_eq!(
        build.cost_usd,
        Some(0.75),
        "the cost is written per step in that file, so it comes from there"
    );

    let titles: Vec<&str> = opened
        .handoffs
        .iter()
        .map(|one| one.title.as_str())
        .collect();
    assert_eq!(
        titles,
        vec!["What we are building"],
        "what one step handed to the next is the only way a result travels between them, and it \
         is a file on disk. An open run that shows none of them shows half of what happened."
    );
}

#[test]
fn a_name_that_is_not_one_run_in_this_folder_is_refused_by_name() {
    let root = tempfile::tempdir().unwrap();
    let project = project_with_four_runs(root.path());

    // Nazwa przyjeżdża tu z linii, którą wpisał człowiek. Odczyt, który wychodzi poza katalog
    // biegów, czyta to, na co go skierowano — więc te cztery mają być odmową, nie odczytem.
    for asked in ["../../etc", "runs/..", "/etc/passwd", ""] {
        let said = match read_run_inner(&project, asked) {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        };
        assert!(
            said.contains("is not the name of one run"),
            "{asked:?} is not the name of one run in this folder, and the refusal has to say \
             exactly that. A name carrying a separator is a different mistake from a name that \
             is simply not here, and one sentence for both answers the wrong question for one \
             of them. It said: {said:?}"
        );
    }

    let said = match read_run_inner(&project, "20260101-000000__nothing-here") {
        Ok(_) => String::new(),
        Err(error) => error.to_string(),
    };
    assert!(
        said.contains("There is no run called") && said.contains("20260101-000000__nothing-here"),
        "a well-formed name that is simply not in this folder has to come back saying so, and \
         naming what was asked for — otherwise a person reading it cannot tell which of the \
         names they typed was wrong. It said: {said:?}"
    );
}

#[tokio::test]
async fn the_saved_stream_reads_back_as_the_rows_the_live_view_showed() {
    assert_eq!(
        FIXTURE.len(),
        FIXTURE_BYTES,
        "the golden file is not the one this criterion was written against, so nothing below \
         means what it says"
    );

    let root = tempfile::tempdir().unwrap();
    let project = project_with_four_runs(root.path());
    let run_dir = project.join(".loadout").join("runs").join(NEWEST);

    // ŻYWY BIEG, po prawdziwej drodze: pompa czyta bajty vendora, zapisuje je do transkryptu
    // kroku i wypuszcza wiersze na ekran. Ten sam plik, którego szuka potem historia.
    let source = root.path().join("stdout.jsonl");
    tokio::fs::write(&source, FIXTURE).await.unwrap();
    let reader = BufReader::new(tokio::fs::File::open(&source).await.unwrap());
    let (tx, mut rx) = mpsc::channel(256);
    stream::pump(
        reader,
        &run_dir
            .join("logs")
            .join(format!("agent-{BUILD_STEP}.jsonl")),
        BUILDER,
        tx,
    )
    .await
    .unwrap();

    let mut live: Vec<Line> = Vec::new();
    while let Some(line) = rx.recv().await {
        live.push(line);
    }
    assert!(
        !live.is_empty(),
        "the live pump produced no rows at all from the golden file, so the comparison below \
         would run between two empty lists and pass on nothing"
    );

    let opened = read_run_inner(&project, NEWEST).expect("that run is right there on disk");
    let build = opened
        .steps
        .iter()
        .find(|one| one.name == BUILDER)
        .expect("the step whose stream was recorded is there");

    let shape = |lines: &[Line]| -> Vec<(LineKind, String)> {
        lines
            .iter()
            .map(|line| (line.kind(), line.text().to_owned()))
            .collect()
    };
    assert_eq!(
        shape(&build.lines),
        shape(&live),
        "reading a run back has to give the very rows the person watched while it was going. \
         Two readings of one stream mean the history tells a different story than the screen \
         did, and nothing on either screen says which one is true."
    );
    assert!(
        build.lines.iter().any(|line| line.text() == PROSE),
        "the one thing the agent actually said in that stream is missing from the run read back \
         off disk"
    );

    let plan = opened
        .steps
        .iter()
        .find(|one| one.name == "Plan")
        .expect("the first step is there");
    assert!(
        plan.lines.is_empty(),
        "nothing was ever recorded for that step, so it has no rows. Rows appearing there would \
         belong to somebody else."
    );
}
