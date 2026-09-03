//! Z-11: krok, który naprawdę niesie umiejętność, dostaje też narzędzie, którym się ją odpala.
//!
//! `skills_reach_claude.rs` dowodzi, że katalog pluginu powstaje i że jego ścieżka dojeżdża do
//! argv. To jest połowa drogi: umiejętność **rejestruje się** w `system/init` i dalej nigdy nie
//! odpala, bo lista dostępności (`--tools`) nie wymienia narzędzia `Skill` przy żadnej z trzech
//! pozycji dialu, a `--tools` jest jedyną prawdziwą bramą [powód stoi przy `tools_for`].
//!
//! Zmierzone 2026-09-01 na `claude` 2.1.257, sondami A i C:
//!
//! | argv | co zrobił agent |
//! |---|---|
//! | `--tools Read,Grep,Glob` + proza prosząca o umiejętność | 14 wywołań `Grep`, potem „I don't have it" |
//! | to samo `+ Skill` | `Skill{skill:"loadout-skills:…"}` → treść `SKILL.md` |
//!
//! `Skill` **nie startuje procesu** — wczytuje `SKILL.md` do kontekstu tury — więc nie podlega
//! powodowi z niezmiennika 6, którym uzasadniona jest nieobecność `Agent` i całej rodziny
//! `Task`/`Workflow` na suficie polityki.
//!
//! # Czego ten plik pilnuje po obu stronach
//!
//! Narzędzie wchodzi **per krok**, wyłącznie tam, gdzie w odziedziczonym fragmencie stoi
//! `--plugin-dir`. Sterownik dopisujący `Skill` zawsze obiecywałby w argv narzędzie, za którym nie
//! ma ani jednej umiejętności — a to jest kłamstwo tego samego rodzaju, co lista dozwolonych przy
//! `bypassPermissions`. Dlatego kontrola z pustym fragmentem stoi w tym samym `#[test]`: rozbita
//! na osobny zestaw dałaby w warstwie `before` obraz „w połowie zielony".
//!
//! Fragment argv bierze się tu z `inherit::rewrite`, a nie z napisu wpisanego przez test: to ten
//! sam kompozytor, którym `commands::run::hand_the_skills_to_the_steps` napełnia `plugin_flags`
//! każdego kroku, więc kształt „dwuelementowy albo pusty" jest tu prawdziwy, a nie założony.
//!
//! # Dowód od vendora stoi osobno i jest `#[ignore]`
//!
//! [`claude_itself_uses_a_skill_this_run_placed`] uruchamia PRAWDZIWE Claude Code, bo asercja
//! o pozycji w `--tools` mówi wyłącznie to, co MY o tym vendorze wiemy z jednego dnia i jednej
//! wersji CLI. Sięga do konta i do sieci, a kryterium padające razem z Wi-Fi nie jest czerwienią
//! kodu — ten sam powód i ten sam kształt, co w `skills_reach_claude.rs`. Bramka go nie woła
//! (`rust-test` biegnie bez `--ignored`); ręcznie:
//! `cargo test --test it skills_reach_the_toolbox:: -- --ignored`.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` z tego samego powodu i **wyłącznie dodane**: przypadek pozytywny, kontrola
// przeciw sterownikowi dopisującemu narzędzie zawsze i gałąź zawężonej listy mierzą JEDNĄ drogę
// i muszą stać w jednym `#[test]` — cięcie po granicy funkcji zdjęłoby dokładnie tę asercję,
// która odróżnia sterownik obiecujący `Skill` bez ani jednej umiejętności za nim.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::claude::{ClaudeDriver, Transcript, tool_surface};
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, Policy, RunSpec};
use loadout_lib::inherit::rewrite;
use loadout_lib::skills::StepSkills;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Narzędzie, którym ten vendor odpala umiejętność. Zmierzone 2026-09-01, sonda C.
const SKILL: &str = "Skill";

/// Twarda lista **dostępności**: czego na niej nie ma, tego proces nie ma pod ręką.
const TOOLS: &str = "--tools";

/// Lista **auto-zatwierdzania**. `Unrestricted` nie dostaje jej w ogóle i to jest kryterium
/// sąsiedniego zadania (T-04), nie ozdoba.
const ALLOWED: &str = "--allowedTools";

/// Flaga, którą Claude Code przyjmuje katalog pluginu — i jedyny sygnał, po którym sterownik
/// pozna, że ten krok naprawdę coś niesie.
const PLUGIN_FLAG: &str = "--plugin-dir";

/// Trzy pozycje dialu, w kolejności z `docs/DECISIONS-LOCKED.md` §D6.
const POLICIES: [Policy; 3] = [Policy::ReadOnly, Policy::EditInFolder, Policy::Unrestricted];

/// Umiejętność, którą ten bieg kładzie na półce.
const SKILL_NAME: &str = "loadout-oracle";

/// Kod, który istnieje **wyłącznie** w ciele `SKILL.md`.
///
/// Bez niego żywa wyrocznia nie odróżnia agenta, który umiejętność wczytał, od agenta, który
/// zgadł z jej nazwy — a to jest dokładnie ta różnica, o którą całe to zadanie chodzi.
const SECRET: &str = "QX7-PLUM-42";

fn skill_file() -> String {
    format!(
        "---\nname: {SKILL_NAME}\ndescription: Hands over the one-time code this run asks for. \
         Use it whenever someone asks for the loadout oracle code.\n---\n\n\
         The code is {SECRET}. Reply with exactly that code and nothing else.\n"
    )
}

#[test]
fn a_step_with_a_skill_is_handed_the_tool_that_runs_it() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let carrying = bench.fragment()?;

    // Fragment naprawdę niesie katalog pluginu. Bez tej linii wszystko niżej pyta o krok, który
    // umiejętności nie ma, i przechodzi na sterowniku, który nie robi nic.
    assert!(
        carrying.iter().any(|argument| argument == PLUGIN_FLAG),
        "the composer handed this step {carrying:?}, and {PLUGIN_FLAG} is not in it. Then every \
         assertion below is true of a step that carries no skill at all"
    );

    // ── Przypadek pozytywny: krok z umiejętnością, wszystkie trzy pozycje dialu ────────────
    for policy in POLICIES {
        let command = ClaudeDriver::new()
            .with_inherited(carrying.clone())
            .command(&spec(policy, bench.project.path()));
        let argv = argv_of(&command);

        let available = list_after(&argv, TOOLS).ok_or_else(|| {
            format!("the command for a {policy:?} step carries no {TOOLS} at all: {argv:?}")
        })?;
        assert_eq!(
            times(&available, SKILL),
            1,
            "this step is handed a plugin directory and {TOOLS} says {available:?}. Whatever is \
             not on that list the process does not have to hand, so the skill registers in the \
             startup event and can never be used. Exactly one entry, because a repeated name is \
             noise that hides where the permission came from"
        );

        // DRUGA KOLUMNA TEJ SAMEJ DECYZJI. Dostępność bez zatwierdzenia jest przy
        // `--permission-mode dontAsk` bezużyteczna: agent pyta, nikt nie odpowiada, i z zewnątrz
        // wygląda to jak narzędzie, które zawsze odmawia. Czy ta polityka w ogóle ma kolumnę
        // zatwierdzania, mówi tabela sterownika — nie kopia tej tabeli po stronie testu.
        let approved = list_after(&argv, ALLOWED);
        assert_eq!(
            approved.is_some(),
            tool_surface(policy, None).approved.is_some(),
            "{policy:?} says {ALLOWED} should be there: {}, and the command says: {}. A list of \
             approved tools next to bypassPermissions names a limit that does not hold, and \
             whoever reads that line believes the list",
            tool_surface(policy, None).approved.is_some(),
            approved.is_some()
        );
        if let Some(approved) = approved {
            assert_eq!(
                times(&approved, SKILL),
                1,
                "{TOOLS} hands this step the skill tool and {ALLOWED} says {approved:?}. \
                 Available and not approved means the agent asks and nobody answers, because \
                 there is no human at the keyboard during a run"
            );
        }
    }

    // ── Kontrola: ten sam sterownik, pusty fragment ───────────────────────────────────────
    //
    // Implementacja dopisująca `Skill` zawsze przechodzi wszystko powyżej i wykłada się tutaj.
    // Narzędzie bez ani jednej umiejętności za nim jest kłamstwem w argv tego samego rodzaju, co
    // lista dozwolonych przy `bypassPermissions`.
    for policy in POLICIES {
        let command = ClaudeDriver::new()
            .with_inherited(Vec::new())
            .command(&spec(policy, bench.project.path()));
        let argv = argv_of(&command);

        let available = list_after(&argv, TOOLS).ok_or_else(|| {
            format!("the command for a {policy:?} step carries no {TOOLS} at all: {argv:?}")
        })?;
        assert_eq!(
            times(&available, SKILL),
            0,
            "nothing was inherited on this step and {TOOLS} says {available:?} anyway. The tool \
             is there and there is no skill behind it, so argv promises something the run cannot \
             keep"
        );
        if let Some(approved) = list_after(&argv, ALLOWED) {
            assert_eq!(
                times(&approved, SKILL),
                0,
                "nothing was inherited on this step and {ALLOWED} says {approved:?} anyway"
            );
        }
    }

    // ── Agent z własną, zawężoną listą ───────────────────────────────────────────────────
    //
    // „Agent uses: Read" ma dalej znaczyć Read — plus to jedno narzędzie, którym odpala się
    // umiejętność, którą ten sam krok naprawdę niesie. Ani sufit polityki (wtedy zawężenie
    // z formularza jest ustawieniem, które ekran przyjmuje, a bieg przycina po cichu), ani lista
    // bez `Skill` (wtedy zawężenie odbiera umiejętność, o którą nikt jej nie prosił).
    let narrowed = RunSpec {
        tools: Some(vec!["Read".to_owned()]),
        ..spec(Policy::ReadOnly, bench.project.path())
    };
    let command = ClaudeDriver::new()
        .with_inherited(carrying)
        .command(&narrowed);
    let argv = argv_of(&command);
    assert_eq!(
        list_after(&argv, TOOLS),
        Some(vec!["Read".to_owned(), SKILL.to_owned()]),
        "the agent asked for Read and this step carries a skill, so {TOOLS} is that list plus the \
         one tool that runs it. argv was {argv:?}"
    );

    Ok(())
}

/// Sufit na prawdziwą sesję: model i sieć, nie atrapa. Regresja ma się objawić czerwienią,
/// a nie zawieszeniem.
const LIVE: Duration = Duration::from_mins(3);

/// Ile miejsca mają kanały. Z zapasem: pełny kanał zatrzymuje pętlę czytającą, a zatrzymana
/// pętla wygląda dokładnie jak zawieszony agent.
const CHANNEL: usize = 256;

/// Krok, którego strumień zapisujemy — po jego identyfikatorze nazywa się plik transkryptu.
const LIVE_STEP: &str = "01996500-0000-7000-8000-0000000000f1";

/// Prośba prozą, wprost o użycie umiejętności. Bez „proszę użyj" sonda mierzyłaby ochotę modelu,
/// a nie to, czy narzędzie w ogóle jest pod ręką.
const ASK: &str = "Use the loadout-oracle skill and reply with the exact code it gives you, and \
                   nothing else.";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "uruchamia prawdziwe Claude Code (konto, siec, koszt); wolaj z --ignored"]
async fn claude_itself_uses_a_skill_this_run_placed() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let carrying = bench.fragment()?;

    let driver = ClaudeDriver::new();
    let found = tokio::time::timeout(LIVE, driver.probe()).await??;
    assert!(
        found.found,
        "this oracle asks Claude Code itself and there is no claude on PATH. It is deliberately \
         #[ignore]d for exactly that reason - install the CLI, log in, and run it again; it \
         reported {found:?}"
    );

    // `Policy::ReadOnly` z rozmysłu: to jedyna pozycja dialu, przy której widać, czy sama para
    // `--tools` + `--allowedTools` wystarcza pod `dontAsk`. Przy `work freely` zatwierdzone jest
    // wszystko, więc zielony wynik nie mówiłby nic o tej zmianie.
    let logs = bench.run.path().join("logs");
    fs::create_dir_all(&logs)?;
    let (lines_tx, _lines) = mpsc::channel(CHANNEL);
    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    let live = driver.with_inherited(carrying).with_transcript(Transcript {
        run_dir: bench.run.path().to_path_buf(),
        step: LIVE_STEP.to_owned(),
        agent: "Oracle".to_owned(),
        lines: lines_tx,
    });

    let ask = RunSpec {
        prompt: ASK.to_owned(),
        ..spec(Policy::ReadOnly, bench.project.path())
    };
    let mut handle: Box<dyn AgentHandle> =
        tokio::time::timeout(LIVE, live.start(ask, events_tx)).await??;
    let _ = tokio::time::timeout(LIVE, handle.wait()).await??;
    // Koniec sesji, nie koniec tury: bez tego czasownika skończony krok zostawia żywy proces
    // [T1 §2], a pętla czytająca nigdy nie dojdzie do końca strumienia.
    let _ = tokio::time::timeout(LIVE, handle.close()).await??;
    tokio::time::timeout(LIVE, async { while events.recv().await.is_some() {} }).await?;

    let transcript = logs.join(format!("agent-{LIVE_STEP}.jsonl"));
    // Brak pliku czytamy jako pustkę celowo: ma paść asercja o wywołaniu narzędzia, a nie błąd
    // wejścia-wyjścia, który bramka słusznie czyta jako fałszywą czerwień.
    let stream = fs::read_to_string(&transcript).unwrap_or_default();
    assert!(
        !stream.is_empty(),
        "the run left nothing in {transcript:?}, so there is no answer from the vendor to read"
    );

    // ROZSTRZYGA PARSOWANIE PÓL, NIGDY `stream.contains(\"Skill\")`: po tej zmianie ta nazwa
    // stoi także na liście z zdarzenia startowego i w ścieżce katalogu pluginu, więc szukanie po
    // całej linii byłoby zielone nad narzędziem, którego nikt nie użył.
    assert!(
        reached_for(&stream, SKILL),
        "the agent was asked in plain words to use the skill this run placed, and it never \
         reached for {SKILL}. Measured 2026-09-01 on 2.1.257: without that name on the \
         availability list the same request produced 14 searches and the sentence \"I don't have \
         it\". What it did instead: {:?}",
        what_it_said(&stream)
    );

    // …i UŻYŁO GO NAPRAWDĘ. Wywołanie, które nic nie wczytało, jest tym samym zielonym ptaszkiem
    // co plugin rejestrujący zero umiejętności: kod stoi wyłącznie w ciele `SKILL.md`.
    let said = what_it_said(&stream);
    assert!(
        said.contains(SECRET),
        "the agent reached for {SKILL} and its answer does not carry {SECRET:?}, which exists \
         nowhere but inside the skill this run placed. It answered: {said:?}"
    );

    Ok(())
}

// ── pomiary ────────────────────────────────────────────────────────────────────────────────

/// Argumenty komendy, tak jak zobaczy je proces.
fn argv_of(command: &tokio::process::Command) -> Vec<String> {
    command
        .as_std()
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

/// Pozycje listy stojącej **zaraz za** tą flagą. `None` znaczy „tej flagi w argv nie ma".
///
/// Rozbite po przecinku, a nie porównywane jako napis: obie te flagi jadą do CLI jednym
/// argumentem z przecinkami, więc pytanie „czy `Skill` jest na liście" jest pytaniem o pozycję.
fn list_after(argv: &[String], flag: &str) -> Option<Vec<String>> {
    let at = argv.iter().position(|argument| argument == flag)?;
    Some(
        argv.get(at + 1)?
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>(),
    )
}

/// Ile razy ta nazwa stoi na liście. Nazwa, nie podnapis: `Skill` nie ma się zliczyć z pozycji,
/// która tylko zaczyna się tak samo.
fn times(list: &[String], name: &str) -> usize {
    list.iter().filter(|entry| *entry == name).count()
}

/// Bloki treści ze wszystkich wypowiedzi agenta w tym strumieniu.
fn blocks_of(stream: &str) -> Vec<Value> {
    stream
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event.get("type").and_then(Value::as_str) == Some("assistant"))
        .filter_map(|event| event.get("message")?.get("content")?.as_array().cloned())
        .flatten()
        .collect()
}

/// Czy agent sięgnął w tym strumieniu po narzędzie o tej nazwie.
fn reached_for(stream: &str, name: &str) -> bool {
    blocks_of(stream).iter().any(|block| {
        block.get("type").and_then(Value::as_str) == Some("tool_use")
            && block.get("name").and_then(Value::as_str) == Some(name)
    })
}

/// Co agent napisał — same bloki tekstu, sklejone.
fn what_it_said(stream: &str) -> String {
    blocks_of(stream)
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `RunSpec` do złożenia komendy. Prompt jest tu bez znaczenia — komenda go nie niesie
/// (niezmiennik 9), a żywa wyrocznia niżej nadpisuje go swoim.
fn spec(policy: Policy, cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "do the work".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

struct Bench {
    /// Kanoniczna kopia umiejętności — to, co w produkcie jest `<dane>/skills/`.
    library: TempDir,
    /// Katalog biegu: tu powstaje katalog pluginu i tu ląduje transkrypt kroku.
    run: TempDir,
    /// Katalog roboczy kroku, czyli `cwd` procesu.
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let library = TempDir::new()?;
        let run = TempDir::new()?;
        let project = TempDir::new()?;

        let shelf = library.path().join("skills").join(SKILL_NAME);
        fs::create_dir_all(&shelf)?;
        fs::write(shelf.join("SKILL.md"), skill_file())?;
        // Żeby katalog roboczy kroku miał w sobie cokolwiek do przeczytania.
        fs::write(project.path().join("notes.txt"), "written by the human")?;

        Ok(Self {
            library,
            run,
            project,
        })
    }

    /// Fragment argv tego kroku — z PRAWDZIWEGO kompozytora, nie z napisu wpisanego przez test.
    ///
    /// To ta sama para wywołań, którą `commands::run::hand_the_skills_to_the_steps` napełnia
    /// `plugin_flags` każdego kroku, więc obietnica „dwuelementowy albo pusty" jest tu prawdziwa,
    /// a nie założona.
    fn fragment(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let skills = StepSkills {
            names: vec![SKILL_NAME.to_owned()],
            dirs: vec![self.shelf()],
        };
        let rewritten =
            rewrite::plugin_dir_from_the_library(&skills, &self.run.path().join("plugin"))?;
        Ok(rewrite::plugin_argv(&rewritten))
    }

    fn shelf(&self) -> PathBuf {
        self.library.path().join("skills").join(SKILL_NAME)
    }
}
