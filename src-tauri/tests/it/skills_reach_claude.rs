//! AC-3 dla T-79: Claude widzi wybrane umiejętności — **w katalogu tego biegu**, pod ścieżką,
//! którą naprawdę dostaje jego proces.
//!
//! Umiejętność, którą Loadout posiada, jedzie do Claude Code jedyną drogą, jaką ten vendor zna:
//! katalogiem pluginu podanym w `--plugin-dir`. Ta droga ma trzy sposoby na to, żeby wyglądać
//! zielono i nie zadziałać, i wszystkie trzy są zmierzone, nie wymyślone [S1 §2, §3]:
//!
//! 1. **Zły poziom.** `<katalog>/alpha/SKILL.md` daje plugin, który się ładuje, pojawia się
//!    w `init.plugins` jako pełnoprawny wpis i rejestruje **zero** umiejętności (przebieg M3,
//!    54 → 54). `<katalog>/skills/alpha/SKILL.md` rejestruje obie (M3a, 54 → 56). Nie ma błędu
//!    i nie ma ostrzeżenia.
//! 2. **Katalog gdzie indziej niż w biegu.** Artefakt biegu poza biegiem nie znika razem z nim
//!    (niezmiennik 4, `docs/ARCHITECTURE.md` §8), a artefakt dopisany do folderu człowieka jest
//!    zmianą, o której właściciel repozytorium dowiaduje się z `git status`.
//! 3. **Flaga bez wartości.** `--plugin-dir` bez ścieżki połyka następną flagę sterownika jako
//!    swój argument — a w tym samym argv stoi `--setting-sources ""`, gdzie pusty argument JEST
//!    poprawny, więc pomylenie tych dwóch kształtów jest realne i nie wygląda jak błąd.
//!
//! **Słabą wersją tego kryterium jest `assert!(argv.contains("--plugin-dir"))`.** Przechodzi dla
//! sterownika, który dokłada flagę zawsze — czyli dla biegu bez ani jednej umiejętności podaje
//! vendorowi ścieżkę do katalogu, którego nie ma, i to jest awaria startu procesu, a nie brak
//! funkcji. Rozróżnia to druga połowa testu, na kroku z pustym wyborem, i dlatego stoi w tym
//! samym `#[test]`: rozbita na osobny zestaw dałaby w warstwie `before` obraz „w połowie zielony".
//!
//! DLACZEGO PRAWDZIWY BIEG, A NIE SAMO `wire::from_the_host`. Bo pytanie brzmi „czy to, co
//! zbudowaliśmy, dojechało do komendy, którą naprawdę uruchamiamy", a katalog biegu istnieje
//! dopiero w biegu. Fragment argv bierzemy więc z dublera — dokładnie taki, jaki dostałby
//! prawdziwy sterownik (niezmiennik 23) — i **ten sam fragment** podajemy potem prawdziwemu
//! `ClaudeDriver`, żeby zobaczyć go w argv procesu. Dwie połowy jednej drogi, bez ani jednej
//! ścieżki zmyślonej przez test.
//!
//! O NAZWIE PLUGINU, czyli o tym, co człowiek zobaczy w `system/init`. Umiejętności wracają
//! z sesji z przedrostkiem od nazwy pluginu (`s1-plugin-a:alpha` [S1 §2]), więc zdarzenie
//! inicjujące potwierdza rejestrację **tych** nazw tylko wtedy, gdy przedrostek jest przypięty
//! manifestem. Nazwa wzięta z katalogu biegu zmieniałaby się co bieg i żaden ekran nie mógłby
//! pokazać jej dwa razy tak samo — dlatego punkt (e) pyta o manifest i o to, czy jego nazwa jest
//! niezależna od biegu.
//!
//! # Czego punkt (e) NIE dowodzi i skąd bierze się drugi test w tym pliku
//!
//! Punkt (e) składa nazwy, którymi sesja **ogłosi się**, z półki na dysku i z przedrostka
//! z manifestu. Oba te źródła są prawdziwe i żadne z nich nie jest listą wpisaną przez test —
//! ale żadne z nich nie jest też **odpowiedzią vendora**. Reguła „katalog musi mieć poziom
//! `skills/`" jest zmierzona [S1 §2, M3 vs M3a], czyli jest naszą wiedzą o Claude Code z jednego
//! dnia i jednej wersji CLI; zdanie „a więc te nazwy się zarejestrują" wypowiada tu nasz model
//! vendora, nie sam vendor. Vendor, który jutro przestanie wczytywać ten układ, przechodzi
//! wszystkie sześć punktów niżej i rejestruje zero umiejętności — czyli dokładnie ten fałszywy
//! zielony ptaszek, przed którym stoi całe to zadanie.
//!
//! Odpowiedź vendora czyta więc [`claude_itself_announces_exactly_the_skills_this_run_placed`]:
//! uruchamia PRAWDZIWE Claude Code z tym samym fragmentem argv, bierze linię `system`/`init`
//! z transkryptu tego biegu i przepuszcza ją przez [`place::discovery_from_init`] — czyli przez
//! tę samą regułę, którą Loadout czyta zdarzenia inicjujące wszędzie indziej (T-18), a nie przez
//! `init.contains(nazwa)` napisane na miejscu.
//!
//! **Ten test jest `#[ignore]`, bo sięga do konta i do sieci**, a kryterium padające razem
//! z `Wi-Fi` nie jest czerwienią kodu (ten sam powód i ten sam kształt, co `flow_skill.rs`).
//! Linia `check:` tego kryterium nie podaje `--include-ignored` i **nie da się jej stąd zmienić**:
//! `TASK.md` jest wyrocznią. To znaczy dokładnie tyle, że w bramce zostaje dowód z dysku
//! i manifestu, a dowód od vendora przechodzi się ręcznie — `cargo test --test it
//! skills_reach_claude:: -- --ignored` — po każdej zmianie w `inherit::rewrite` i po każdej
//! podbitej wersji CLI. Jest to zapisane tutaj, a nie przemilczane.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` z tego samego powodu i **wyłącznie dodane**, nie w miejsce niczego: sześć
// punktów tego kryterium (a–f) mierzy JEDNĄ drogę i musi stać w jednym `#[test]` — rozbite na
// osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony", co nagłówek tego pliku
// nazywa wprost. Cięcie po granicy funkcji rozdzieliłoby przypadek pozytywny od kontroli przeciw
// pustemu przejściu, czyli zdjęłoby dokładnie tę asercję, która odróżnia sterownik stawiający
// flagę zawsze.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::skills::place;
use loadout_lib::skills::place::Discovery;
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Etykieta vendora dublera. Nie „claude": dubler stoi tu po to, żeby oddać fragment argv, a nie
/// żeby udawać sterownik, który ten fragment potem konsumuje.
const VENDOR: &str = "fake";

/// Powód w całości przy tej samej stałej w `skills_reach_the_step.rs`.
const PATIENCE: Duration = Duration::from_secs(20);

/// Flaga, którą Claude Code przyjmuje katalog pluginu. Jedna, bo jedna jest [S1 §3].
const PLUGIN_FLAG: &str = "--plugin-dir";

/// Poziom, bez którego plugin ładuje się i rejestruje ZERO umiejętności [S1 §2, M3 vs M3a].
const SKILLS_LEVEL: &str = "skills";

/// Manifest, z którego bierze się przedrostek nazw w `system/init`.
const MANIFEST: &str = ".claude-plugin/plugin.json";

/// Dwie umiejętności agenta — obie mają dojechać.
const ALPHA: &str = "alpha";
const BETA: &str = "beta";
/// Umiejętność, która leży w bibliotece i **nie jest przypisana nikomu**. Implementacja podająca
/// vendorowi zawartość `~/.loadout/skills/` przechodzi każdą asercję o `alpha` i `beta` i wykłada
/// się dopiero na niej.
const GAMMA: &str = "gamma";

/// Nazwa kroku, który dziedziczy obie umiejętności swojego agenta.
const CARRIES: &str = "Carries both";
/// Nazwa kroku, który wyłączył je wszystkie zapisem `[]`.
const CARRIES_NOTHING: &str = "Carries nothing";

fn skill_file(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says in a line what it is for.\n---\n\n\
         Answer with a single sentence.\n"
    )
}

/// Agent z dwiema umiejętnościami. `gamma` nie jest tu wymieniona.
const SCRIBE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d3
name: Scribe
summary: Writes things down
color: slate
runsWith: claude-code
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

/// Jeden krok, jedno nadpisanie, jedna nazwa. Dwa biegi po tym samym pliku różnią się wyłącznie
/// tym, co stoi w `overrides` — czyli dokładnie tym, o co pyta punkt (f).
fn workflow_file(step: &str, overrides: &str) -> String {
    format!(
        r#"{{
  "format": 1,
  "id": "wf_skills_to_claude",
  "name": "One step for Claude",
  "steps": [
    {{
      "kind": "agent",
      "id": "s_only",
      "name": "{step}",
      "agent": "01990000-0000-7000-8000-0000000000d3",
      "overrides": {overrides},
      "instructions": "do the work",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }}
  ],
  "links": []
}}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_plugin_directory_lands_in_this_run_and_its_path_reaches_the_claude_command()
-> Result<(), Box<dyn Error>> {
    // ── Przypadek pozytywny: krok dziedziczy obie umiejętności agenta ──────────────────────
    let bench = Bench::new()?;
    let (report, flags) = bench
        .one_run(&workflow_file(CARRIES, "{}"), CARRIES)
        .await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded],
        "the step has to finish, or every assertion below is true of a step that never ran. \
         It ended as {:?}",
        report.steps
    );

    // (a) RAZ I Z WARTOŚCIĄ. Dwa razy znaczy dwa katalogi i CLI wybierające jeden z nich, a flaga
    //     bez wartości połyka następny argument sterownika — i to drugie nie wygląda jak błąd,
    //     bo `--setting-sources ""` z sąsiedniego zadania stoi w tym samym argv.
    let count = times(&flags, PLUGIN_FLAG);
    assert_eq!(
        count, 1,
        "the driver was handed {PLUGIN_FLAG} {count} time(s) for a step with two skills. One \
         plugin directory is named exactly once. The fragment was {flags:?}"
    );
    never_a_flag_without_a_value(&flags);
    let plugin = value_after(&flags, PLUGIN_FLAG)
        .ok_or("the driver was handed --plugin-dir with nothing after it")?;

    // (b) POD KATALOGIEM TEGO BIEGU. Katalog pluginu jest wyjściem builda i ma zniknąć razem
    //     z biegiem (niezmiennik 4). Sterownik wybierający miejsce sam kładzie go w `$TMPDIR`,
    //     czyli zostawia artefakt biegu poza biegiem.
    assert!(
        plugin.starts_with(&report.dir),
        "the flag points at {plugin:?}, which is not under this run's directory ({:?}). A run \
         artefact outside the run survives the run, and nothing ever deletes it \
         (docs/ARCHITECTURE.md section 8)",
        report.dir
    );

    // (c) DOKŁADNIE WYBRANE, NA POZIOMIE, KTÓRY VENDOR CZYTA. Półka mierzona spod ścieżki
    //     Z ARGV, nie spod tej, o której wie test: katalog o poziom obok tego, który powstał,
    //     jest dokładnie tą cichą porażką, przed którą stoi całe to kryterium.
    let shelf = plugin.join(SKILLS_LEVEL);
    assert_eq!(
        skills_under(&shelf),
        set(&[ALPHA, BETA]),
        "the process is pointed at {plugin:?}, and {shelf:?} holds {:?}. The `skills/` level is \
         mandatory and measured: a plugin directory without it loads, shows up in init.plugins as \
         a healthy entry, and registers zero skills (S1 section 2, run M3: 54 -> 54)",
        skills_under(&shelf)
    );
    assert!(
        !skills_under(&shelf).contains(GAMMA),
        "{GAMMA} sits in the library and no agent and no step ever asked for it, and it is inside \
         the directory this run hands to Claude anyway. Handing an agent the whole shelf makes \
         every narrowing on every step meaningless"
    );

    // (d) BAJT W BAJT z kanoniczną kopią. Porównanie po `String` z `trim` przechodzi dla
    //     implementacji, która przepuściła plik przez `place::emit`: emiter zwraca poprawny
    //     `SKILL.md`, tylko INNY — przestawione pola, zdjęte pola spoza specyfikacji, przecytowane
    //     skalary. Treść promptu umiejętności ma być tą, którą człowiek zapisał.
    for name in [ALPHA, BETA] {
        let written = shelf.join(name).join("SKILL.md");
        assert!(
            fs::symlink_metadata(&written).is_ok(),
            "argv points the process at {plugin:?}, and {written:?} is not there. A directory the \
             vendor cannot read is the same green as no skills at all: the plugin loads and \
             registers nothing, with a healthy-looking entry in the startup event"
        );
        assert_eq!(
            fs::read(&written)?,
            fs::read(bench.library(name))?,
            "{written:?} is not byte for byte the canonical copy in the library"
        );
    }

    // (e) PRZEDROSTEK, KTÓRY POTWIERDZI ZDARZENIE INICJUJĄCE. Umiejętności wracają w `system/init`
    //     jako `<plugin>:<nazwa>` [S1 §2], więc bez przypiętej nazwy pluginu przedrostek spada do
    //     nazwy katalogu biegu — a wtedy dwa biegi tej samej umiejętności ogłaszają się pod dwiema
    //     różnymi nazwami i żaden ekran nie może pokazać ich tak samo.
    let manifest = plugin.join(MANIFEST);
    let text = fs::read_to_string(&manifest).map_err(|error| {
        format!(
            "the plugin has no manifest at {manifest:?}, so the names the session announces \
                 come from the run directory and change every run: {error}"
        )
    })?;
    let pinned = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| value.get("name")?.as_str().map(str::to_owned))
        .ok_or_else(|| format!("{manifest:?} does not name the plugin: {text:?}"))?;
    // Nazwy, którymi ta sesja ogłosi się w `system/init`: półka **z dysku**, przepuszczona przez
    // przedrostek **z manifestu**. Dwa źródła, oba prawdziwe — żadne z nich nie jest listą, którą
    // ten test sam sobie wpisał.
    let announced = skills_under(&shelf)
        .iter()
        .map(|name| format!("{pinned}:{name}"))
        .collect::<BTreeSet<_>>();
    let expected = set(&[ALPHA, BETA])
        .iter()
        .map(|name| format!("{pinned}:{name}"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        announced, expected,
        "the session announces the skills it registered as <plugin>:<name> (S1 section 2). This \
         run will announce {announced:?}, and the human picked {expected:?}"
    );
    let run_folder = report
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    assert!(
        !pinned.contains(run_folder) && !pinned.contains(&report.id),
        "the plugin is pinned as {pinned:?}, which carries this run ({run_folder:?} / {:?}) in its \
         name. Then the same skill announces itself under a different name every run, and no \
         screen can show the human twice what they picked once",
        report.id
    );

    // ── (f) Kontrola przeciw pustemu przejściu ─────────────────────────────────────────────
    //
    // Ten sam agent, to samo repozytorium, wyłącznie `[]` na kroku. Implementacja stawiająca flagę
    // zawsze przechodzi wszystko powyżej i wykłada się tutaj — przy starcie procesu, na katalogu,
    // którego nie ma.
    let empty = Bench::new()?;
    let (bare_report, bare_flags) = empty
        .one_run(
            &workflow_file(CARRIES_NOTHING, r#"{ "skills": [] }"#),
            CARRIES_NOTHING,
        )
        .await?;

    assert_eq!(
        bare_report.steps,
        vec![StepState::Succeeded],
        "the step that cleared its skills still has to run - it asked for nothing, not for a \
         failure. It ended as {:?}",
        bare_report.steps
    );
    never_a_flag_without_a_value(&bare_flags);
    let bare_count = times(&bare_flags, PLUGIN_FLAG);
    assert_eq!(
        bare_count, 0,
        "this step cleared its skills and {PLUGIN_FLAG} still stands {bare_count} time(s) in what \
         the driver was handed. The vendor would be pointed at a directory that does not exist, \
         and that is a process which fails to start - not a feature that is missing. The fragment \
         was {bare_flags:?}"
    );
    // …i nie powstał żaden katalog, którym dałoby się tę flagę uzasadnić. Bez tego „pusty katalog
    // nie trafia do argv" jest spełnialne przez implementację, która katalog i tak stworzyła,
    // tylko go nie wymieniła — a katalog, który powstał, prędzej czy później zostanie komuś podany.
    let left_behind = plugin_dirs_under(&bare_report.dir);
    assert!(
        left_behind.is_empty(),
        "nothing was selected on this step, and the run directory holds a plugin directory anyway: \
         {left_behind:?}. A plugin that loads with zero skills is the exact green this whole task \
         exists to remove"
    );

    // ── Druga połowa drogi: ten sam fragment w argv prawdziwej komendy ─────────────────────
    //
    // Fragment jedzie do sterownika gotowy (niezmiennik 23), więc to jest cała wiedza, jaką adapter
    // ma o umiejętnościach — i całe pytanie brzmi, czy przeżywa złożenie komendy.
    let command = ClaudeDriver::new()
        .with_inherited(flags.clone())
        .command(&spec(bench.project.path()));
    let argv = argv_of(&command);
    never_a_flag_without_a_value_in(&argv);
    let in_argv = argv
        .iter()
        .filter(|arg| *arg == Path::new(PLUGIN_FLAG))
        .count();
    assert_eq!(
        in_argv, 1,
        "the fragment carried {PLUGIN_FLAG} once, and the command Claude would really run carries \
         it {in_argv} time(s). argv was {argv:?}"
    );
    let named = argv
        .iter()
        .position(|arg| *arg == Path::new(PLUGIN_FLAG))
        .and_then(|at| argv.get(at + 1))
        .ok_or("the command carries --plugin-dir with nothing after it")?;
    assert_eq!(
        named.as_path(),
        plugin,
        "the run built its plugin directory at {plugin:?} and the command points the process at \
         {named:?}. A flag naming a directory one level away from the one that exists is the \
         quiet failure this task is about"
    );

    let bare_command = ClaudeDriver::new()
        .with_inherited(bare_flags)
        .command(&spec(empty.project.path()));
    let bare_argv = argv_of(&bare_command);
    never_a_flag_without_a_value_in(&bare_argv);
    let bare_in_argv = bare_argv
        .iter()
        .filter(|arg| *arg == Path::new(PLUGIN_FLAG))
        .count();
    assert_eq!(
        bare_in_argv, 0,
        "nothing was selected and the command still carries {PLUGIN_FLAG} {bare_in_argv} time(s): \
         {bare_argv:?}"
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
const LIVE_STEP: &str = "01996500-0000-7000-8000-0000000000e3";

/// Zadanie tury: najtańsze, jakie da się zadać. To kryterium pyta o zdarzenie INICJUJĄCE, więc
/// treść odpowiedzi nie ma tu głosu, a każde dłuższe zadanie kosztuje pieniądze bez powodu.
const CHEAP: &str = "Reply with the single word: ok";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "uruchamia prawdziwe Claude Code (konto, siec, koszt); wolaj z --ignored"]
async fn claude_itself_announces_exactly_the_skills_this_run_placed() -> Result<(), Box<dyn Error>>
{
    // Katalog pluginu buduje PRAWDZIWY BIEG, tak samo jak w teście wyżej: to jest ta ścieżka,
    // którą naprawdę dostanie proces, i nic w tym pliku jej nie zmyśla.
    let bench = Bench::new()?;
    let (report, flags) = bench
        .one_run(&workflow_file(CARRIES, "{}"), CARRIES)
        .await?;
    let plugin = value_after(&flags, PLUGIN_FLAG)
        .ok_or("the run built no plugin directory, so there is nothing to ask Claude about")?;
    let shelf = plugin.join(SKILLS_LEVEL);
    let manifest = plugin.join(MANIFEST);
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("the plugin has no manifest at {manifest:?}: {error}"))?;
    let pinned = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|value| value.get("name")?.as_str().map(str::to_owned))
        .ok_or_else(|| format!("{manifest:?} does not name the plugin: {text:?}"))?;
    // Miejsca, w które pisaliśmy — to jest cała treść zgłoszenia „vendor tego nie widzi"
    // (`Discovery::NotSeen::looked_in`), więc jadą tam prawdziwe ścieżki, nie pusta lista.
    let wrote = [ALPHA, BETA]
        .iter()
        .map(|name| shelf.join(name))
        .collect::<Vec<_>>();

    // ── Prawdziwe CLI, ten sam fragment argv ──────────────────────────────────────────────
    let driver = ClaudeDriver::new();
    let probe = tokio::time::timeout(LIVE, driver.probe()).await??;
    assert!(
        probe.found,
        "this oracle asks Claude Code itself and there is no claude on PATH. It is deliberately \
         #[ignore]d for exactly that reason - install the CLI, log in, and run it again; it \
         reported {probe:?}"
    );

    let logs = report.dir.join("logs");
    fs::create_dir_all(&logs)?;
    let (lines_tx, _lines) = mpsc::channel(CHANNEL);
    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    let live = driver.with_inherited(flags).with_transcript(
        loadout_lib::engine::drivers::claude::Transcript {
            run_dir: report.dir.clone(),
            step: LIVE_STEP.to_owned(),
            agent: "Scribe".to_owned(),
            lines: lines_tx,
        },
    );

    let mut handle: Box<dyn AgentHandle> = tokio::time::timeout(
        LIVE,
        live.start(cheap_turn(bench.project.path()), events_tx),
    )
    .await??;
    let _ = tokio::time::timeout(LIVE, handle.wait()).await??;
    // Koniec sesji, nie koniec tury: bez tego czasownika skończony krok zostawia żywy proces
    // [T1 §2], a pętla czytająca nigdy nie dojdzie do końca strumienia.
    let _ = tokio::time::timeout(LIVE, handle.close()).await??;
    tokio::time::timeout(LIVE, async { while events.recv().await.is_some() {} }).await?;

    // ── To, co powiedział vendor ──────────────────────────────────────────────────────────
    let transcript = logs.join(format!("agent-{LIVE_STEP}.jsonl"));
    // Brak pliku czytamy jako pustkę celowo: ma paść asercja o zdarzeniu, a nie błąd wejścia-
    // wyjścia, który bramka słusznie czyta jako fałszywą czerwień (AGENTS.md §2a p. 5).
    let stream = fs::read_to_string(&transcript).unwrap_or_default();
    let init = stream
        .lines()
        .find(|line| is_init(line))
        .unwrap_or_default();
    assert!(
        !init.is_empty(),
        "the session left no system/init event in {transcript:?}, so there is no answer from the \
         vendor to read. Without it this test can only repeat what we put on disk, which is what \
         the six points above already do. The transcript holds {} line(s)",
        stream.lines().count()
    );

    // ROZSTRZYGA `discovery_from_init`, NIE `init.contains(nazwa)`: nazwa umiejętności potrafi
    // stać w `cwd` i w nazwie serwera narzędzi, nie będąc w żadnej z dwóch tablic zdarzenia —
    // a wtedy szukanie po całej linii mówi „widzi" i to jest ten sam fałszywy zielony ptaszek.
    for name in [ALPHA, BETA] {
        let announced = format!("{pinned}:{name}");
        assert_eq!(
            place::discovery_from_init(&announced, init, &wrote),
            Discovery::Seen,
            "this run put {name} in the directory it hands Claude, and the session that really \
             started does not announce it as {announced:?}. A plugin directory that loads and \
             registers nothing looks exactly like a healthy one from the outside (S1 section 2, \
             run M3: 54 -> 54). The event was: {init}"
        );
    }

    // …i ANI JEDNEJ NAZWY WIĘCEJ. Zasiana w bibliotece, nieprzypisana nikomu — implementacja
    // podająca vendorowi całą bibliotekę wykłada się dopiero tutaj, i to ustami vendora.
    let stranger = format!("{pinned}:{GAMMA}");
    assert_eq!(
        place::discovery_from_init(&stranger, init, &wrote),
        Discovery::NotSeen {
            looked_in: wrote.clone()
        },
        "{GAMMA} sits in the library and nobody ever asked for it, and the session announces it \
         anyway. Handing an agent the whole shelf makes every narrowing on every step \
         meaningless. The event was: {init}"
    );

    Ok(())
}

/// Czy ta linia jest zdarzeniem inicjującym sesję.
///
/// Po dwóch polach, nie po `contains("init")`: słowo `init` potrafi stać w treści dowolnej
/// wiadomości, a wtedy przez werdykt szłaby linia, która o umiejętnościach nie mówi nic.
fn is_init(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|event| {
        event.get("type").and_then(serde_json::Value::as_str) == Some("system")
            && event.get("subtype").and_then(serde_json::Value::as_str) == Some("init")
    })
}

/// Jedna tania tura prawdziwego CLI.
fn cheap_turn(cwd: &Path) -> RunSpec {
    RunSpec {
        prompt: CHEAP.to_owned(),
        ..spec(cwd)
    }
}

// ── pomiary ────────────────────────────────────────────────────────────────────────────────

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// Ile razy ta flaga stoi we fragmencie.
fn times(flags: &[String], flag: &str) -> usize {
    flags.iter().filter(|arg| *arg == flag).count()
}

/// Wartość stojąca **zaraz za** flagą.
fn value_after(flags: &[String], flag: &str) -> Option<PathBuf> {
    let at = flags.iter().position(|arg| arg == flag)?;
    flags.get(at + 1).map(PathBuf::from)
}

/// Fragment nigdy nie niesie `--plugin-dir` z wartością o zerowej długości.
///
/// Pytamy w KAŻDYM przypadku, nie tylko w pozytywnym: `--setting-sources ""` stoi w tym samym
/// argv i tam pusty argument jest **poprawny**, więc pomylenie tych dwóch kształtów jest realne,
/// a skutek — połknięcie następnej flagi jako wartości — nie wygląda jak błąd.
fn never_a_flag_without_a_value(flags: &[String]) {
    for (index, argument) in flags.iter().enumerate() {
        assert!(
            argument != PLUGIN_FLAG || flags.get(index + 1).is_some_and(|value| !value.is_empty()),
            "the fragment carries {PLUGIN_FLAG} with nothing after it, so the driver's next flag \
             becomes its argument: {flags:?}"
        );
    }
}

fn never_a_flag_without_a_value_in(argv: &[PathBuf]) {
    for (index, argument) in argv.iter().enumerate() {
        assert!(
            argument != Path::new(PLUGIN_FLAG)
                || argv
                    .get(index + 1)
                    .is_some_and(|value| !value.as_os_str().is_empty()),
            "argv carries {PLUGIN_FLAG} with nothing after it, so the driver's next flag becomes \
             its argument: {argv:?}"
        );
    }
}

/// Argumenty komendy, tak jak zobaczy je proces.
fn argv_of(command: &tokio::process::Command) -> Vec<PathBuf> {
    command.as_std().get_args().map(PathBuf::from).collect()
}

/// `RunSpec` do złożenia komendy. Polityka i model są tu bez znaczenia — mierzy je T-53.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "do the work".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Nazwy katalogów umiejętności leżących pod `<dir>` — pusto, kiedy tej półki nie ma.
///
/// Katalog bez `SKILL.md` **nie liczy się**: to jest ta sama reguła, po której poznaje się
/// umiejętność w cudzym repozytorium (`inherit::scan::skills`), i bez niej pusty katalog
/// o właściwej nazwie udawałby dojechaną umiejętność.
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

/// Katalogi pod tym biegiem, które wyglądają jak katalog pluginu — po zawartości, nie po nazwie.
///
/// Po zawartości, bo nazwa jest wyborem implementacji, a pytanie brzmi „czy vendorowi jest co
/// podać". Katalog z manifestem albo z półką `skills/` jest katalogiem, który prędzej czy później
/// zostanie komuś podany.
fn plugin_dirs_under(run: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(run) else {
        return Vec::new();
    };
    let mut found = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.join(MANIFEST).is_file() || !skills_under(&path.join(SKILLS_LEVEL)).is_empty()
        })
        .collect::<Vec<_>>();
    found.sort();
    found
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Fragment argv, który warstwa wyżej podała sterownikowi. `None` znaczy „nie pytano", czyli
/// nie było czego nieść — i to jest inny stan niż „podano pustą listę".
#[derive(Debug, Default)]
struct Handed(Mutex<Vec<String>>);

impl Handed {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, flags: &[String]) {
        *self.lock() = flags.to_vec();
    }

    fn snapshot(&self) -> Vec<String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn recording_drivers(handed: Arc<Handed>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        handed,
        flags: Vec::new(),
    });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, którego jedyną treścią jest fragment argv, jaki dostał.
#[derive(Debug)]
struct Fake {
    handed: Arc<Handed>,
    /// Fragment przyniesiony przez warstwę wyżej — dokładnie tyle, ile wie o nim adapter
    /// (niezmiennik 23). Pusty znaczy „nie było czego nieść".
    flags: Vec<String>,
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

    /// Szew, którym gotowy fragment argv dojeżdża do sterownika. `Some`, bo ten dubler UMIE go
    /// przyjąć — sterownik oddający tu `None` przy niepustym fragmencie zatrzymuje krok, i tak
    /// ma być (`Live::carrying_what_we_inherited`).
    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            handed: Arc::clone(&self.handed),
            flags: flags.to_vec(),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Zapis TUTAJ, a nie w `inheriting`: krok, którego nikt o fragment nie zapytał, ma zostawić
        // pustą listę, a nie brak wpisu. Inaczej „zero flag" byłoby nieodróżnialne od „dubler nie
        // wystartował".
        self.handed.record(&self.flags);

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
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;

        let bench = Self { home, project };
        bench.agent(SCRIBE)?;
        for name in [ALPHA, BETA, GAMMA] {
            bench.skill(name)?;
        }
        Ok(bench)
    }

    fn agent(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("agents").join("scribe.md"), text)?;
        Ok(())
    }

    /// Kanoniczna kopia jednej umiejętności: `<dane>/skills/<nazwa>/SKILL.md`.
    fn skill(&self, name: &str) -> Result<(), Box<dyn Error>> {
        let dir = self.home.path().join("skills").join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), skill_file(name))?;
        Ok(())
    }

    fn library(&self, name: &str) -> PathBuf {
        self.home.path().join("skills").join(name).join("SKILL.md")
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    /// Jeden bieg po jednym kroku. Oddaje raport i fragment argv, który dostał dubler.
    async fn one_run(
        &self,
        workflow: &str,
        slug: &str,
    ) -> Result<(loadout_lib::commands::RunReport, Vec<String>), Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{}.json", slug.replace(' ', "-")));
        fs::write(&path, workflow)?;

        let store = Store::open(&self.db())?;
        let handed = Arc::new(Handed::default());
        let deps = RunDeps {
            home: self.home.path(),
            project: self.project.path(),
            store: &store,
            drivers: recording_drivers(Arc::clone(&handed)),
            processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow: path,
            how_many_at_once: 2,
            task: None,
            part: None,
            handoffs_from: None,
        };

        let (sink, source) = line_channel(QUEUE_CAP);
        let pump = spawn_pump(source, Channel::new(|_| Ok(())));
        let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
            .await
            .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
        let _ = tokio::time::timeout(PATIENCE, pump).await;

        Ok((report, handed.snapshot()))
    }
}
