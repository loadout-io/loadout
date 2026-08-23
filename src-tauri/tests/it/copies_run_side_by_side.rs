//! AC-1 dla T-90: krok z `copies: 3` biegnie jako trzy nakładające się w czasie kopie.
//!
//! # Po co to istnieje
//!
//! `copies` jest w schemacie od T3 §4.4, ma w panelu kroku wiersz „How many at once", ma
//! walidator zakresu 1–8 i walidator kolizji folderów — i **nie zmienia biegu ani o jotę**.
//! `commands/run.rs` mówi to wprost w swoim własnym nagłówku: „Krok z `copies: 3` biegnie tu
//! jako jedna sesja". Zmierzone na biegu właściciela: dwa kroki po `copies: 2` dały **22**
//! kroki zamiast 28. Człowiek ustawia liczbę, ekran ją przyjmuje, plik ją zapisuje, a robota
//! wykonuje się raz — to jest martwa kontrolka (niezmiennik 16) schowana o warstwę głębiej,
//! bo „agent zrobił to raz" jest z zewnątrz nieodróżnialne od „agent zrobił to trzy razy i trzy
//! razy wyszło tak samo".
//!
//! # Trzy słabe wersje tego kryterium i co je odrzuca
//!
//! **„Wszystkie trzy się skończyły".** To jest dokładnie ta asercja, którą poprzedni prototyp przechodził
//! przy czterech pasach biegnących jeden po drugim w rozłącznych oknach po ~0,5 s [raport 01
//! §7.3]. Rozwinięcie kopii na trzy kroki wykonywane po kolei jest tańsze w implementacji,
//! wygląda identycznie w raporcie i **nie jest tym, o co człowiek prosi**, ustawiając „ile
//! naraz". Dlatego mierzone są PRZEDZIAŁY, tak jak w `engine_overlap.rs`: przecięcie okien musi
//! wynieść co najmniej połowę kroku.
//!
//! **„Powstały trzy kroki".** Przechodzi dla rozwinięcia, które robi trzy KAFELKI — a wtedy okno
//! rysuje trzy karty agentów zamiast jednej i warunek właściciela „nie ma być widać, że
//! spawnujemy nowych agentów" pada. Rozstrzyga to `stepId` w wierszach stanu: wszystkie trzy
//! kopie mówią do okna JEDNYM kluczem kafelka, a różnią się wyłącznie podpisem.
//!
//! **„Prompt zawiera podstawienie".** Przechodzi dla implementacji, która podstawia `{{copy}}`
//! i uruchamia jedną sesję — czyli wpisuje w prompt liczbę, której nic po drugiej stronie nie
//! odpowiada. Dlatego podstawienie sądzimy na TRZECH promptach naraz i wymagamy, żeby dały
//! trzy RÓŻNE numery.
//!
//! # Czego to kryterium pilnuje poza samym rozwinięciem
//!
//! Krok za kopiami dostaje **trzy** przekazania w indeksie, a nie jedno: krok scalający, który
//! widzi wynik jednej kopii, jest gorszy niż brak kopii, bo kosztuje trzy razy tyle i oddaje
//! tyle samo. I kopie wewnątrz pętli **mnożą się** przez rundy, a nie zastępują ich — druga
//! ławka niżej jest właśnie o tym, bo implementacja, która rozwija kopie zamiast rund albo
//! rundy zamiast kopii, przechodzi wszystko powyżej.
//!
//! Runtime jest **wielowątkowy z prawdziwymi snami**, nigdy `start_paused`: czas wirtualny
//! implikuje runtime jednowątkowy i przeskakuje do przodu, kiedy runtime staje bezczynny, więc
//! „nakładanie się" przestaje cokolwiek znaczyć [T7 §8.1].

// `expect()`/`unwrap()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` DODANE, nie w miejsce czegokolwiek: pięć punktów pierwszego kryterium mierzy
// JEDEN bieg dzielący jedną ławkę, jedną migawkę okien i jeden `run.json`. Cięcie po granicy
// funkcji znaczyłoby pięć osobnych biegów albo stan dzielony między testami, które cargo
// uruchamia równolegle.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use loadout_lib::workflow::check::{Level, check};
use loadout_lib::workflow::file::load;
use serde_json::Value as Json;
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne
/// wymagania co do prywatnych dowodów, a to kryterium sądzi kształt biegu, nie sterownik.
const VENDOR: &str = "fake";

/// Ile trwa jedna tura dublera. Prawdziwy sen, bo przecięcie okien liczy się na zegarze.
const TURN: Duration = Duration::from_millis(240);

/// Ile z tego musi być wspólne dla wszystkich trzech kopii. Połowa tury: prawdziwa
/// równoległość daje tu prawie całe 240 ms, a wykonanie po kolei daje zero — próg w połowie
/// nie rozstrzyga się na styk i nie zależy od tego, jak szybko maszyna wystartuje kolejne
/// zadanie.
const MIN_SHARED: Duration = Duration::from_millis(120);

/// Ile czekamy, zanim uznamy bieg za zawieszony. Bieg, który wisi, jest dla bramki „nie
/// uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(30);

/// Ile kopii zamawia pierwsza ławka.
const COPIES: usize = 3;

/// Podpisy, którymi trzy kopie mają się przedstawić oknu i `run.json`.
///
/// Wypisane tutaj słowo w słowo, nie sklejone z tej samej funkcji, którą sprawdzają: kryterium
/// czytające własną stałą kodu zawsze się z nim zgadza i nie mierzy niczego (niezmiennik 20).
const SIGNED: [&str; COPIES] = ["Build (1 of 3)", "Build (2 of 3)", "Build (3 of 3)"];

/// Klucz kafelka z pliku — ten, po którym okno rozpoznaje SWOJĄ kartę.
const TILE: &str = "s_build";

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-00000000090a
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

/// Trzy kopie jednego kroku, a za nimi krok, który je scala.
///
/// `fresh-copy` nie jest ozdobą: krok w kilku kopiach biegnie równocześnie SAM ZE SOBĄ, więc
/// bez własnej kopii plików `check_to_run` odmawia go przed pierwszym procesem (niezmiennik 12)
/// i to kryterium nie miałoby jak przejść w żadnej implementacji.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_copies_side_by_side",
  "name": "Three copies and one join",
  "steps": [
    {
      "kind": "agent",
      "id": "s_build",
      "name": "Build",
      "agent": "01990000-0000-7000-8000-00000000090a",
      "overrides": {},
      "copies": 3,
      "instructions": "build: this is copy {{copy}} of {{copies}}.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_join",
      "name": "Join",
      "agent": "01990000-0000-7000-8000-00000000090a",
      "overrides": {},
      "instructions": "join: put what the copies found into one answer.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": [{ "from": "s_build", "to": "s_join" }]
}
"#;

/// Dwie kopie WEWNĄTRZ pętli o dwóch rundach. Sędzia nie przepuszcza, więc obie rundy biegną.
const LOOPED: &str = r#"{
  "format": 1,
  "id": "wf_copies_inside_a_loop",
  "name": "Two copies, two turns",
  "steps": [
    {
      "kind": "agent",
      "id": "s_try",
      "name": "Try",
      "agent": "01990000-0000-7000-8000-00000000090a",
      "overrides": {},
      "copies": 2,
      "instructions": "try: this is copy {{copy}} of {{copies}}.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_judge",
      "name": "Judge",
      "agent": "01990000-0000-7000-8000-00000000090a",
      "overrides": {},
      "instructions": "judge: say whether the work is good enough to build on.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 240 }
    }
  ],
  "links": [
    { "from": "s_try", "to": "s_judge" },
    { "from": "s_judge", "to": "s_try", "max_turns": 2 }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_copies_of_one_step_share_a_window_and_hand_on_three_answers()
-> Result<(), Box<dyn Error>> {
    let ran = one_run("copies-side-by-side", WORKFLOW, COPIES).await?;
    let windows = ran.watch.windows("build");
    let prompts = ran.watch.prompts("build");

    // ── (a) TRZY KOPIE NAPRAWDĘ RUSZYŁY ──────────────────────────────────────────────────────
    // Pierwsza asercja, bo bez niej wszystkie następne mówią o kopiach, których nie było.
    assert_eq!(
        prompts.len(),
        COPIES,
        "the step asked for {COPIES} copies at once and the agent app was entered {} time(s). \
         A number a person sets, a screen accepts and a file records, and the work is done once, \
         is a control with nothing behind it (invariant 16). The prompts it saw were: {prompts:?}",
        prompts.len()
    );

    // ── (b) I NAPRAWDĘ NARAZ ─────────────────────────────────────────────────────────────────
    // Przecięcie WSZYSTKICH trzech okien, nie pary: implementacja, która puszcza dwie kopie
    // razem i trzecią po nich, przechodzi każde pytanie o parę.
    let shared = all_three_share(&windows);
    assert!(
        shared >= MIN_SHARED,
        "the three copies shared {shared:?} of a {TURN:?} turn. \"How many at once\" has to mean \
         at once: copies unrolled into steps that run one after another finish just as well, cost \
         three times as much and give the person nothing they asked for — the defect that let \
         poprzedni prototyp report a parallelism it never had (invariant 11). The windows were {:?}",
        spans(&windows)
    );

    // ── (c) JEDNA KARTA, TRZY PODPISY ────────────────────────────────────────────────────────
    // To jest warunek właściciela „nie ma być widać, że spawnujemy nowych agentów", zapisany
    // w tym, co naprawdę wyszło do okna. Trzy kafelki zamiast trzech kopii jednego kafelka
    // przechodzą (a) i (b) i przewracają ekran.
    let started = ran.running_rows();
    let tiles: BTreeSet<&str> = started
        .iter()
        .map(|(tile, _)| tile.as_str())
        .filter(|tile| *tile == TILE || tile.starts_with("s_build"))
        .collect();
    assert_eq!(
        tiles,
        BTreeSet::from([TILE]),
        "the copies reached the window under {tiles:?}. All of them are one tile on the canvas, \
         so all of them say so with one key — a key per copy makes the window draw three cards \
         for one tile, and the person drew one"
    );

    let signed: Vec<&str> = started
        .iter()
        .filter(|(tile, _)| tile == TILE)
        .map(|(_, name)| name.as_str())
        .collect();
    let mut sorted = signed.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        SIGNED.to_vec(),
        "the three copies signed themselves {signed:?}. Every line of work on screen, and every \
         row of the run's own record, has to say WHICH copy said it — three rows under one name \
         are three rows a person cannot tell apart"
    );

    // ── (d) KAŻDA KOPIA WIE, KTÓRA JEST ─────────────────────────────────────────────────────
    // Podstawienie sądzone na trzech promptach naraz: implementacja podstawiająca stałą jedynkę
    // przechodzi każdą asercję o pojedynczym prompcie.
    let numbered: BTreeSet<String> = prompts
        .iter()
        .filter_map(|prompt| prompt.lines().next().map(str::to_owned))
        .collect();
    assert_eq!(
        numbered,
        BTreeSet::from([
            "build: this is copy 1 of 3.".to_owned(),
            "build: this is copy 2 of 3.".to_owned(),
            "build: this is copy 3 of 3.".to_owned(),
        ]),
        "the copies were told {numbered:?}. Each one has to be told which of how many it is: \
         three agents given the same words do the same work three times, which is the most \
         expensive way to get one answer"
    );

    // ── (e) I KROK ZA NIMI DOSTAJE TRZY ODPOWIEDZI, NIE JEDNĄ ───────────────────────────────
    let join = ran
        .watch
        .prompts("join")
        .first()
        .cloned()
        .ok_or("the joining step never reached the agent app, so it was told nothing")?;
    assert_eq!(
        join.matches("handoffs/").count(),
        COPIES,
        "the step after the copies was pointed at {} of the {COPIES} answers left for it. \
         A joining step that reads one copy costs three times as much as no copies at all and \
         gives back the same one answer. Its prompt was: {join:?}",
        join.matches("handoffs/").count()
    );
    for name in SIGNED {
        assert!(
            join.contains(name),
            "the joining step was never told that \"{name}\" left it anything. An index that \
             names the copies is the only way its reader can tell three answers apart. Its \
             prompt was: {join:?}"
        );
    }

    // ── (f) I `run.json` ZAPISUJE TRZY KROKI, KAŻDY POD SWOIM KLUCZEM ───────────────────────
    // Klucze muszą się różnić, bo indeks biegu ma na nich `UNIQUE (run_id, node_key)`: trzy
    // kopie o jednym kluczu to bieg, który zapisze jedną i zgubi dwie (niezmiennik 4).
    let recorded = steps_in_run_file(&ran.report)?;
    let ours: Vec<&Json> = recorded
        .iter()
        .filter(|step| {
            step.get("name")
                .and_then(Json::as_str)
                .is_some_and(|name| name.starts_with("Build"))
        })
        .collect();
    assert_eq!(
        ours.len(),
        COPIES,
        "the run's own record has {} row(s) for the step that ran in {COPIES} copies. Files are \
         the truth and the database is only its index (invariant 4), so a copy missing here is a \
         copy whose cost, its answer and its agent are gone. The rows were: {recorded:?}",
        ours.len()
    );
    let keys: BTreeSet<&str> = ours
        .iter()
        .filter_map(|step| step.get("node_key").and_then(Json::as_str))
        .collect();
    assert_eq!(
        keys.len(),
        COPIES,
        "the {COPIES} copies were written down under {} distinct key(s): {keys:?}. The index of \
         this run keeps one row per key, so two copies sharing one key means one of them is \
         silently thrown away",
        keys.len()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copies_inside_a_loop_multiply_by_its_turns_instead_of_replacing_them()
-> Result<(), Box<dyn Error>> {
    // Dwie kopie w pętli o dwóch rundach to CZTERY tury tego kroku. Implementacja, która
    // rozwija kopie i gubi rundy, oddaje dwie; ta, która rozwija rundy i gubi kopie, też oddaje
    // dwie — i obie wyglądają w raporcie jak bieg, który się udał.
    let ran = one_run("copies-inside-a-loop", LOOPED, 2).await?;
    let tries = ran.watch.prompts("try");

    assert_eq!(
        tries.len(),
        4,
        "a step set to run in two copies, inside a loop that goes round twice, was entered {} \
         time(s). Two copies times two turns is four tries: a run that gives two has quietly \
         dropped one of the two things the person asked for, and both losses look the same from \
         outside. The prompts it saw were: {tries:?}",
        tries.len()
    );

    let numbered: BTreeSet<String> = tries
        .iter()
        .filter_map(|prompt| prompt.lines().next().map(str::to_owned))
        .collect();
    assert_eq!(
        numbered,
        BTreeSet::from([
            "try: this is copy 1 of 2.".to_owned(),
            "try: this is copy 2 of 2.".to_owned(),
        ]),
        "the tries were told {numbered:?}. Both copies have to be told which of how many they \
         are, in EVERY turn of the loop — a second turn that forgets is a second turn in which \
         two agents do the same work"
    );

    let signed: BTreeSet<String> = ran
        .running_rows()
        .into_iter()
        .filter(|(tile, _)| tile == "s_try")
        .map(|(_, name)| name)
        .collect();
    assert_eq!(
        signed,
        BTreeSet::from(["Try (1 of 2)".to_owned(), "Try (2 of 2)".to_owned()]),
        "the copies inside the loop signed themselves {signed:?}, and they reach the window under \
         one tile key in every turn — the same condition that keeps three copies from drawing \
         three cards"
    );
    Ok(())
}

/// Największe przecięcie WSZYSTKICH podanych okien: od najpóźniejszego startu do najwcześniejszego
/// końca. Zero, kiedy choć jedna para się rozmija.
fn all_three_share(windows: &[(Instant, Instant)]) -> Duration {
    let Some(latest_start) = windows.iter().map(|&(from, _)| from).max() else {
        return Duration::ZERO;
    };
    let Some(earliest_end) = windows.iter().map(|&(_, to)| to).min() else {
        return Duration::ZERO;
    };
    earliest_end.saturating_duration_since(latest_start)
}

/// Okna jako czasy trwania — czytelne w komunikacie asercji.
fn spans(windows: &[(Instant, Instant)]) -> Vec<Duration> {
    windows
        .iter()
        .map(|&(from, to)| to.saturating_duration_since(from))
        .collect()
}

/// Kroki zapisane w `run.json` tego biegu.
fn steps_in_run_file(report: &RunReport) -> Result<Vec<Json>, Box<dyn Error>> {
    let text = fs::read_to_string(report.dir.join("run.json"))?;
    let run: Json = serde_json::from_str(&text)?;
    let steps = run
        .get("steps")
        .and_then(Json::as_array)
        .ok_or("the run's own record describes no steps at all")?;
    Ok(steps.clone())
}

// ── jeden bieg ─────────────────────────────────────────────────────────────────────────────

/// Wszystko, co po biegu jest potrzebne do sądzenia.
struct Ran {
    report: RunReport,
    watch: Arc<Watch>,
    /// Wiersze, które NAPRAWDĘ wyszły kanałem do okna.
    delivered: Vec<Json>,
}

impl Ran {
    /// Pary `(klucz kafelka, podpis)` z wierszy „ten krok właśnie rusza".
    ///
    /// `running`, nie `succeeded`: krok, który padł albo został pominięty, też się przedstawił,
    /// a pytanie brzmi „ile kopii ruszyło i pod jakim podpisem".
    fn running_rows(&self) -> Vec<(String, String)> {
        self.delivered
            .iter()
            .filter(|row| row.get("kind").and_then(Json::as_str) == Some("stepState"))
            .filter(|row| row.get("state").and_then(Json::as_str) == Some("running"))
            .filter_map(|row| {
                Some((
                    row.get("stepId").and_then(Json::as_str)?.to_owned(),
                    row.get("agent").and_then(Json::as_str)?.to_owned(),
                ))
            })
            .collect()
    }
}

async fn one_run(
    slug: &str,
    workflow: &str,
    how_many_at_once: usize,
) -> Result<Ran, Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let path = bench.workflow(slug, workflow)?;
    the_fixture_can_run(&path, &[&hand])?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow: path,
        how_many_at_once,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let recorder = Delivered::default();
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, recorder.channel());

    // `sink` wjeżdża do biegu i ginie razem z jego powrotem — dopiero wtedy pompa widzi koniec
    // producenta i wypycha ostatnią, niepełną paczkę. Pompy nie wolno zabijać z zewnątrz:
    // wiersze, których to kryterium szuka, są w tej ostatniej paczce.
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let delivered = recorder.lines()?;
    Ok(Ran {
        report,
        watch,
        delivered,
    })
}

/// Fikstura ma przejść walidator **bez ani jednego problemu**, a jej plik agenta ma dać się
/// przeczytać.
///
/// To nie jest część kryterium, tylko jego przesłanka, i dlatego stoi przed biegiem. Czerwień
/// w fazie kontraktu wygląda identycznie dla „zachowania jeszcze nie ma" i dla „tego kryterium
/// nie da się spełnić nigdy": krok w trzech kopiach poza własną kopią plików jest odmową przy
/// zapisie (niezmiennik 12), więc bez `fresh-copy` ta ławka nie doszłaby nawet do planisty.
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

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Jedno wejście do sterownika: czym go zagadano i kiedy wszedł oraz wyszedł.
#[derive(Debug)]
struct Entered {
    /// Pierwsze słowo instrukcji kroku, przed dwukropkiem — po nim poznajemy, czyje to okno.
    label: String,
    /// Prompt, który naprawdę dojechał do sterownika.
    prompt: String,
    from: Instant,
    to: Option<Instant>,
}

/// Obserwator sterownika: okno i prompt każdego uruchomienia.
///
/// Wejście zapisuje `start`, a wyjście — koniec tury, **przed** oddaniem permitu przez planistę.
/// Zapisane okna leżą więc w środku okien permitów, nigdy poza nimi: pomiar może zaniżyć
/// nakładanie się, ale nie może go zmyślić.
#[derive(Debug, Default)]
struct Watch {
    seen: Mutex<Vec<Entered>>,
}

impl Watch {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn entered(&self, prompt: &str) -> usize {
        let mut seen = self.lock();
        seen.push(Entered {
            label: label_of(prompt),
            prompt: prompt.to_owned(),
            from: Instant::now(),
            to: None,
        });
        seen.len() - 1
    }

    /// Krok wyszedł, jakkolwiek się skończył. Pierwsze wyjście wygrywa.
    fn left(&self, at: usize) {
        let mut seen = self.lock();
        if let Some(one) = seen.get_mut(at) {
            one.to.get_or_insert_with(Instant::now);
        }
    }

    /// Domknięte okna kroków o tej etykiecie. Okno bez końca nie wchodzi — i dlatego liczba
    /// promptów jest sprawdzana osobno.
    fn windows(&self, label: &str) -> Vec<(Instant, Instant)> {
        self.lock()
            .iter()
            .filter(|one| one.label == label)
            .filter_map(|one| Some((one.from, one.to?)))
            .collect()
    }

    /// Prompty kroków o tej etykiecie, w kolejności wejścia do sterownika.
    fn prompts(&self, label: &str) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|one| one.label == label)
            .map(|one| one.prompt.clone())
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Entered>> {
        // Zatruty zamek nie ma prawa zgubić pomiaru: panika w jednej kopii oślepiłaby asercję,
        // która akurat dowodzi, że pozostałe biegły naraz.
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Etykieta kroku: to, co stoi przed pierwszym dwukropkiem instrukcji.
///
/// Prompt niesie potem indeks przekazań i umowę o odpowiedzi, więc rozpoznajemy krok po jego
/// własnym pierwszym słowie — `RunSpec` nazwy kroku nie niesie.
fn label_of(prompt: &str) -> String {
    prompt
        .split_once(':')
        .map_or_else(|| prompt.to_owned(), |(head, _)| head.trim().to_owned())
}

/// Wiersze, które **naprawdę wyszły kanałem** do okna, w kolejności wyjścia.
#[derive(Debug, Clone, Default)]
struct Delivered(Arc<Mutex<Vec<InvokeResponseBody>>>);

impl Delivered {
    fn channel(&self) -> Channel<Vec<Line>> {
        let seen = Arc::clone(&self.0);
        Channel::new(move |body| {
            // `std::sync::Mutex` w domknięciu SYNCHRONICZNYM: nie ma tu `await`, więc
            // niezmiennik 8 stoi z konstrukcji, a nie z uwagi w komentarzu.
            if let Ok(mut seen) = seen.lock() {
                seen.push(body);
            }
            Ok(())
        })
    }

    fn lines(&self) -> Result<Vec<Json>, Box<dyn Error>> {
        let seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::new();
        for body in seen.iter().cloned() {
            out.extend(body.deserialize::<Vec<Json>>()?);
        }
        Ok(out)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler sterownika: zapisuje okno i prompt, mówi jedno zdanie i kończy turę.
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
        let at = self.watch.entered(&spec.prompt);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
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
            watch: Arc::clone(&self.watch),
            events,
            session,
            at,
            said: answer_for(&label_of(&spec.prompt)),
        }))
    }
}

/// Co ten krok odpowiada.
///
/// Sędzia pętli **nie przepuszcza**, i to jest treść drugiej ławki: pętla, która domknie się po
/// pierwszej rundzie, nigdy nie pokaże, czy kopie mnożą się przez rundy.
fn answer_for(label: &str) -> String {
    if label == "judge" {
        return "## Answer\nThe work has to be done again.\n\noutcome: fail\n".to_owned();
    }
    format!("## Answer\n{label} did the work.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n")
}

/// Jedna tura dublera.
#[derive(Debug)]
struct Turn {
    watch: Arc<Watch>,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    at: usize,
    said: String,
}

#[async_trait]
impl AgentHandle for Turn {
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
        tokio::time::sleep(TURN).await;
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: TURN,
            session: self.session.clone(),
        };
        self.watch.left(self.at);
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        self.watch.left(self.at);
        // Dubler nie ma procesu, więc dowód śmierci jest tu prawdą z konstrukcji, a nie
        // uproszczeniem: nie ma czego zabijać i nie ma czego przeżyć (niezmiennik 6).
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        self.watch.left(self.at);
        Ok(Some(0))
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

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
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
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
