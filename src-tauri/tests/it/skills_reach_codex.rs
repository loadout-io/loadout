//! AC-4 dla T-79: pozostałych pięciu vendorów dostaje **równoważny zestaw** — i po biegu
//! w folderze człowieka nie ma ani jednego naszego pliku.
//!
//! Claude Code przyjmuje katalog umiejętności argumentem (`--plugin-dir`). Pozostałych pięciu
//! **nie umie go przyjąć w ogóle** [T5 §3.1]: dla nich „agent ma umiejętność" znaczy dosłownie
//! „plik leży w jego katalogu roboczym", pod `.agents/skills/`. To jest druga droga tego zadania
//! i jedyna, jaką ci vendorzy mają.
//!
//! I dlatego to kryterium ma drugą połowę. Katalog roboczy kroku bywa **folderem człowieka**
//! (`folder: { use: "project" }`, wartość domyślna formatu), a wtedy „połóż umiejętności
//! w katalogu roboczym" znaczy „dopisz katalog do cudzego repozytorium". Loadout obiecuje pisać
//! wyłącznie do własnego katalogu biegu (`docs/ARCHITECTURE.md` §8), a katalog dopisany do repo
//! człowieka zostaje tam po biegu na zawsze i wychodzi dopiero w `git status`.
//!
//! DWIE DROGI SĄ ZIELONE, I TO JEST ŚWIADOME. Krok, który potrzebuje umiejętności, a pracuje
//! wprost w folderze człowieka, wolno rozwiązać **własną kopią plików** albo **odmową**
//! ([`Why::WouldWriteIntoYourFolder`]) — to jest wybór produktowy, którego kryterium nie
//! przesądza. Czerwone jest wyłącznie trzecie wyjście: bieg kończy się sukcesem, krok pracuje
//! w folderze człowieka i nasz katalog zostaje w jego drzewie.
//!
//! **Słabą wersją tego kryterium jest `assert!(cwd.join(".agents/skills/alpha").exists())`.**
//! Przechodzi dla implementacji, która wysypuje do katalogu roboczego całą bibliotekę — czyli dla
//! tej, w której odznaczenie umiejętności na kroku nic nie znaczy. Rozróżnia to równość zbiorów
//! plus `gamma`: umiejętność zasiana w bibliotece i nieprzypisana nikomu.
//!
//! **Drugą słabą wersją jest sprawdzenie samego katalogu roboczego.** Przechodzi dla
//! implementacji, która położyła pliki poprawnie w kopii i **przy okazji** w oryginale. Rozróżnia
//! to migawka całego drzewa projektu, robiona przed biegiem i po nim, ze ścieżkami — nie
//! z licznikiem.
//!
//! **TRZECIĄ SŁABĄ WERSJĄ JEST MIGAWKA SAMYCH ŚCIEŻEK.** Zbiór nazw odpowiada wyłącznie na
//! pytanie „czy czegoś przybyło albo ubyło" i milczy o wszystkim, co dzieje się w plikach, które
//! już tam były: dopisanie `.agents/skills/` do **istniejącego** `.gitignore`, nadpisanie cudzego
//! `SKILL.md` naszą kopią, zdjęcie bitu wykonywalności ze skryptu przy kopiowaniu tam i z powrotem.
//! Każda z tych trzech zmian zostaje w repozytorium człowieka po biegu i każda wychodzi dopiero
//! w `git status` — czyli są dokładnie tym, czego to kryterium zabrania, a zbiór ścieżek pokazuje
//! je jako brak różnicy. Dlatego migawka niesie **rodzaj wpisu, jego treść co do bajtu i jego
//! prawa** — a katalogi są w niej osobnymi wpisami, bo pusty katalog dopisany do cudzego drzewa
//! też w nim zostaje.
//!
//! [`Why::WouldWriteIntoYourFolder`]: loadout_lib::skills::Why::WouldWriteIntoYourFolder

// Powód przy tej samej linii w `skills_reach_the_step.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „codex": to kryterium sądzi półkę w katalogu roboczym, a nie
/// adapter — półka jest ta sama dla pięciu vendorów [T5 §3.1].
const VENDOR: &str = "fake";

/// Powód w całości przy tej samej stałej w `skills_reach_the_step.rs`.
const PATIENCE: Duration = Duration::from_secs(20);

/// Półka, do której zaglądają Codex, Cursor, Gemini CLI, opencode i Amp [T5 §3.1].
const SHELF: [&str; 2] = [".agents", "skills"];

/// Dwie umiejętności agenta.
const ALPHA: &str = "alpha";
const BETA: &str = "beta";
/// Umiejętność w bibliotece, nieprzypisana nikomu.
const GAMMA: &str = "gamma";

/// Krok z własną kopią plików — droga bez żadnej wątpliwości.
const OWN_COPY: &str = "Works on its own copy";
/// Krok pracujący wprost w folderze człowieka — tu rozstrzyga się druga połowa kryterium.
const YOUR_FOLDER: &str = "Works in your folder";

fn skill_file(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says in a line what it is for.\n---\n\n\
         Answer with a single sentence.\n"
    )
}

/// Agent z dwiema umiejętnościami.
const HAND: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d4
name: Hand
summary: Does the work
color: moss
runsWith: codex
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: [alpha, beta]
connections: []
---
Do the work.
";

/// Jeden krok. Różni je wyłącznie to, gdzie pracują.
fn workflow_file(step: &str, folder: &str) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_skills_to_codex",
  "name": "One step for the other five",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_only",
      "name": "{step}",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {{}},
      "instructions": "{step}",
      "folder": {{ "use": "{folder}" }},
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_with_its_own_copy_reaches_the_skills_through_its_working_directory()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let before = bench.human_files();

    let done = bench
        .one_run(&workflow_file(OWN_COPY, "fresh-copy"), OWN_COPY)
        .await?;
    let report = done.report.map_err(|said| {
        format!(
            "the run refused a step that has its own copy of your files, so \
                                 there was nothing to refuse about: {said}"
        )
    })?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step has to finish, or every assertion below is true of a step that never ran. \
         It ended as {:?}",
        report.steps
    );

    // (a) DOKŁADNIE WYBRANE, NA PÓŁCE, DO KTÓREJ CI VENDORZY ZAGLĄDAJĄ. Mierzone z katalogu
    //     roboczego kroku, bo dla tych pięciu vendorów nie ma innego kanału: żaden z nich nie
    //     umie przyjąć ścieżki argumentem [T5 §3.1].
    let seen = done.reachable.get(OWN_COPY).cloned().unwrap_or_default();
    assert_eq!(
        seen,
        set(&[ALPHA, BETA]),
        "the step's agent has {ALPHA} and {BETA}, and its working directory offers {seen:?} under \
         {SHELF:?}. For Codex, Cursor, Gemini CLI, opencode and Amp this shelf IS the answer to \
         \"does the agent have this skill\" - there is no second channel"
    );
    assert!(
        !seen.contains(GAMMA),
        "{GAMMA} sits in the library and no agent and no step ever asked for it, and the step \
         could reach it anyway. Emptying the whole shelf into every working directory makes every \
         narrowing on every step meaningless"
    );

    // (b) I ANI JEDNEGO NASZEGO PLIKU W DRZEWIE CZŁOWIEKA. Migawka rodzajem, treścią i prawami,
    //     nie licznikiem i nie samą ścieżką: „tyle samo plików" przechodzi dla biegu, który jeden
    //     podmienił, a „te same ścieżki" — dla biegu, który dopisał się do cudzego pliku.
    let after = bench.human_files();
    let moved = difference(&before, &after);
    assert!(
        moved.is_empty(),
        "the step worked on its own copy, and your project changed anyway: {moved:?}. Loadout \
         writes into its own run directory and nowhere else (docs/ARCHITECTURE.md section 8); \
         what it leaves in a folder of yours stays there after the run and shows up in git status"
    );
    assert_eq!(
        after,
        before,
        "the step worked on its own copy, and your project changed anyway. Loadout writes into \
         its own run directory and nowhere else (docs/ARCHITECTURE.md section 8); what it leaves \
         in a folder of yours stays there after the run and shows up in git status. Added or \
         removed: {:?}",
        difference(&before, &after)
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_step_working_in_your_folder_either_takes_a_copy_or_says_why_not()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let before = bench.human_files();
    let project = bench.project.path().to_path_buf();

    let done = bench
        .one_run(&workflow_file(YOUR_FOLDER, "project"), YOUR_FOLDER)
        .await?;

    // TA ASERCJA STOI PIERWSZA I DOTYCZY OBU DRÓG. Odmowa, która zdążyła coś napisać, jest gorsza
    // niż cichy zapis: człowiek czyta „nic nie zrobiłem" i ma w drzewie katalog, którego nie zna.
    let after = bench.human_files();
    let moved = difference(&before, &after);
    assert!(
        moved.is_empty(),
        "your project changed during this run: {moved:?}. Loadout writes into its own run \
         directory and nowhere else (docs/ARCHITECTURE.md section 8): anything it leaves in \
         somebody's repository outlives the run and turns up in git status - and a file it \
         rewrote in place turns up there just the same as one it added"
    );
    assert_eq!(
        after,
        before,
        "your project gained or lost files during this run. Loadout writes into its own run \
         directory and nowhere else (docs/ARCHITECTURE.md section 8): a skills directory quietly \
         added to somebody's repository outlives the run and turns up in git status. Added or \
         removed: {:?}",
        difference(&before, &after)
    );

    match done.report {
        // DROGA DRUGA: odmowa. Zdanie ma nazwać UMIEJĘTNOŚĆ i FOLDER — bez nazwy umiejętności
        // człowiek nie wie, co odznaczyć, a bez ścieżki nie wie, którego kroku to dotyczy.
        Err(said) => {
            assert!(
                said.contains(ALPHA) || said.contains(BETA),
                "the run refused to give this step its skills and the sentence names neither \
                 {ALPHA} nor {BETA}, so the human is left searching the workflow for what to \
                 change. It said: {said:?}"
            );
            assert!(
                said.contains(&project.display().to_string()),
                "the refusal is about the folder this step works in, and the sentence does not \
                 name it ({project:?}). It said: {said:?}"
            );
        }
        // DROGA PIERWSZA: własna kopia. Wtedy katalog roboczy NIE JEST folderem człowieka, i to
        // jest cała treść tej gałęzi — półka może być pełna, byle nie w cudzym drzewie.
        Ok(report) => {
            assert_eq!(
                report.steps,
                vec![StepState::Succeeded],
                "the run came back without refusing, so the step was supposed to work. It ended \
                 as {:?}",
                report.steps
            );
            let cwd =
                done.cwd.get(YOUR_FOLDER).cloned().ok_or(
                    "the step never reached the driver, so it never had a working directory",
                )?;
            assert_ne!(
                cwd, project,
                "this step was set to work straight inside your folder and it needs two skills. \
                 The run neither gave it a copy of your files nor said why not - it just ran in \
                 {cwd:?}, which is your project. Then either the agent silently went without the \
                 skills you picked, or our directory is sitting in your repository"
            );
            let seen = done.reachable.get(YOUR_FOLDER).cloned().unwrap_or_default();
            assert_eq!(
                seen,
                set(&[ALPHA, BETA]),
                "the run gave this step a working directory of its own ({cwd:?}) instead of \
                 refusing, so the skills have to be there: the copy exists precisely so they can \
                 be. It offers {seen:?}"
            );
        }
    }

    Ok(())
}

// ── pomiary ────────────────────────────────────────────────────────────────────────────────

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// Drzewo człowieka tak, jak zostawiłby je `git status`: ścieżka względna → co pod nią stoi.
type Tree = BTreeMap<PathBuf, Entry>;

/// Jeden wpis drzewa — WSZYSTKO, co po biegu ma być takie samo.
///
/// Nie sama ścieżka: plik nadpisany w miejscu, katalog zamieniony w dowiązanie i skrypt, który
/// stracił bit wykonywalności, mają tę samą ścieżkę przed biegiem i po nim. Prawa trzymamy
/// obcięte do bitów uprawnień, bo reszta trybu niesie rodzaj wpisu, a ten stoi już w wariancie.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// Katalog. Pusty katalog dopisany do cudzego drzewa też w nim zostaje.
    Dir { mode: u32 },
    /// Zwykły plik, co do bajtu. `None` znaczy „nie dało się przeczytać" — inny stan niż plik
    /// pusty i **nie** powód, żeby przerwać pomiar (AGENTS.md §2a p. 5).
    File { mode: u32, bytes: Option<Vec<u8>> },
    /// Dowiązanie, po celu: podmieniony cel jest zmianą, której rozmiar ani prawa nie widzą.
    Link { target: PathBuf },
}

/// Co się między dwiema migawkami zmieniło — po ludzku, bo to zdanie ląduje w komunikacie.
///
/// Trzy rodzaje różnic, nie jeden: dopisane, zabrane i **zmienione na miejscu**. To ostatnie jest
/// całym powodem, dla którego migawka niesie coś więcej niż ścieżki.
fn difference(before: &Tree, after: &Tree) -> Vec<String> {
    let mut said = Vec::new();
    for (path, was) in before {
        match after.get(path) {
            None => said.push(format!("gone: {}", path.display())),
            Some(now) if now != was => {
                said.push(format!("changed {}: {}", how(was, now), path.display()));
            }
            Some(_) => {}
        }
    }
    said.extend(
        after
            .keys()
            .filter(|path| !before.contains_key(*path))
            .map(|path| format!("added: {}", path.display())),
    );
    said.sort();
    said
}

/// Czym różnią się dwa wpisy pod jedną ścieżką.
fn how(was: &Entry, now: &Entry) -> &'static str {
    match (was, now) {
        (
            Entry::File {
                mode: before,
                bytes: had,
            },
            Entry::File {
                mode: after,
                bytes: has,
            },
        ) => match (had == has, before == after) {
            (false, false) => "contents and permissions of",
            (false, true) => "contents of",
            _ => "permissions of",
        },
        (Entry::Dir { .. }, Entry::Dir { .. }) => "permissions of",
        (Entry::Link { .. }, Entry::Link { .. }) => "the target of",
        _ => "what kind of thing sits at",
    }
}

/// Nazwy katalogów umiejętności leżących pod `<dir>` — pusto, kiedy tej półki nie ma.
///
/// Katalog bez `SKILL.md` **nie liczy się**: bez tej reguły pusty katalog o właściwej nazwie
/// udawałby dojechaną umiejętność.
fn skills_under(dir: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// Całe drzewo pod `root`, ścieżkami względnymi, z pominięciem `skip` na pierwszym poziomie.
fn tree_under(root: &Path, skip: &str) -> Tree {
    let mut found = Tree::new();
    walk(root, Path::new(""), skip, &mut found);
    found
}

fn walk(dir: &Path, prefix: &Path, skip: &str, found: &mut Tree) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    for entry in listing.filter_map(Result::ok) {
        let relative = prefix.join(entry.file_name());
        if relative == Path::new(skip) {
            continue;
        }
        // `symlink_metadata`, nie `metadata`: dowiązanie do katalogu poza drzewem jest plikiem
        // TEGO drzewa, a wejście w nie liczyłoby cudze pliki jako nasze.
        let Ok(kind) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let mode = kind.permissions().mode() & 0o7777;
        if kind.is_dir() {
            found.insert(relative.clone(), Entry::Dir { mode });
            walk(&entry.path(), &relative, skip, found);
        } else if kind.is_symlink() {
            found.insert(
                relative,
                Entry::Link {
                    target: fs::read_link(entry.path()).unwrap_or_default(),
                },
            );
        } else {
            found.insert(
                relative,
                Entry::File {
                    mode,
                    bytes: fs::read(entry.path()).ok(),
                },
            );
        }
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co dubler zobaczył: katalog roboczy kroku i to, po co ten krok może z niego sięgnąć.
#[derive(Debug, Default)]
struct Seen {
    cwd: Mutex<BTreeMap<String, PathBuf>>,
    reachable: Mutex<BTreeMap<String, BTreeSet<String>>>,
}

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guardy powstają i giną w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłyby do `await`.
    fn record(&self, step: &str, cwd: &Path) {
        lock(&self.cwd).insert(step.to_owned(), cwd.to_path_buf());
        lock(&self.reachable).insert(
            step.to_owned(),
            skills_under(
                &SHELF
                    .iter()
                    .fold(cwd.to_path_buf(), |at, part| at.join(part)),
            ),
        );
    }
}

fn lock<T>(what: &Mutex<T>) -> MutexGuard<'_, T> {
    what.lock().unwrap_or_else(PoisonError::into_inner)
}

fn watching_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który zagląda tam, gdzie zagląda pięciu z sześciu vendorów.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
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

    /// Ten dubler UMIE przyjąć gotowy fragment argv — inaczej krok stanąłby na braku szwu, a to
    /// kryterium nie o tym mówi.
    fn inheriting(&self, _flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Krok rozpoznajemy po nazwie zapisanej w jego instrukcji: `RunSpec` nie niesie nazwy
        // kroku, a instrukcja jest tym, co ten krok naprawdę dostał (niezmiennik 9).
        let step = if spec.prompt.contains(YOUR_FOLDER) {
            YOUR_FOLDER
        } else {
            OWN_COPY
        };
        self.seen.record(step, &spec.cwd);

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
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
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
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
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
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

/// Wynik jednego biegu: raport albo zdanie odmowy, plus to, co zobaczył dubler.
struct Done {
    report: Result<loadout_lib::commands::RunReport, String>,
    cwd: BTreeMap<String, PathBuf>,
    reachable: BTreeMap<String, BTreeSet<String>>,
}

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
        fs::create_dir_all(home.path().join("skills"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        // Drzewo człowieka: kilka plików w kilku miejscach, żeby migawka miała czym się różnić.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        fs::create_dir_all(project.path().join("src"))?;
        fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")?;
        // PLIK, DO KTÓREGO KUSI DOPISAĆ SIĘ PO CICHU. Implementacja kładąca półkę w cudzym
        // drzewie i „sprzątająca po sobie" wpisem w `.gitignore` zostawia zmianę, której zbiór
        // ścieżek nie widzi — a właściciel repozytorium widzi ją w pierwszym `git diff`.
        fs::write(project.path().join(".gitignore"), "target/\n")?;
        // SKRYPT Z BITEM WYKONYWALNOŚCI. Kopia tam i z powrotem przez `write(read(..))` gubi ten
        // bit po cichu (powód w całości przy `skills::place::copy_the_skill`), a plik o tej samej
        // ścieżce i tej samej treści przestaje dać się uruchomić.
        let script = project.path().join("src").join("run.sh");
        fs::write(&script, "#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;

        let bench = Self { home, project };
        fs::write(bench.home.path().join("agents").join("hand.md"), HAND)?;
        for name in [ALPHA, BETA, GAMMA] {
            bench.skill(name)?;
        }
        Ok(bench)
    }

    /// Kanoniczna kopia jednej umiejętności: `<dane>/skills/<nazwa>/SKILL.md`.
    fn skill(&self, name: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.home.path().join("skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), skill_file(name))?;
        Ok(())
    }

    /// Drzewo człowieka **bez** `.loadout/`. Ten jeden katalog jest nasz i wolno nam do niego
    /// pisać — cała reszta tego folderu nie jest i nie wolno.
    fn human_files(&self) -> Tree {
        tree_under(self.project.path(), ".loadout")
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    async fn one_run(&self, workflow: &str, slug: &str) -> Result<Done, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{}.json", slug.replace(' ', "-")));
        fs::write(&path, workflow)?;

        let store = Store::open(&self.db())?;
        let seen = Arc::new(Seen::default());
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: watching_drivers(Arc::clone(&seen)),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: path,
            how_many_at_once: 2,
            task: None,
        };

        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
        let _ = tokio::time::timeout(PATIENCE, pump).await;

        Ok(Done {
            report: outcome.map_err(|error| error.to_string()),
            cwd: lock(&seen.cwd).clone(),
            reachable: lock(&seen.reachable).clone(),
        })
    }
}
