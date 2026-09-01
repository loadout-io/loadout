//! Agent, który powiedział na strumień skarg więcej, niż Loadout zachowuje, ma to POWIEDZIEĆ.
//!
//! Do dziś sterownik czytał pierwsze cztery kilobajty i porzucał resztę **bez śladu**: nie było
//! flagi, nie było zdania, nie było liczby. Człowiek dostawał początek i nie miał ani jednego
//! sposobu dowiedzieć się, że reszta w ogóle istnieje — a leży ona całą długością w `logs/`,
//! dokąd to zdanie ma wysłać.
//!
//! **Słaba wersja tego kryterium pyta funkcję albo flagę.** Przechodzi ją mechanizm kompletny
//! i niepodłączony: `truncated` ustawione poprawnie w pętli drenowania i nieczytane przez nikogo
//! jest dokładnie tą klasą wady, dla której powstał niezmiennik 29. Dlatego pomiar leży na
//! `Line::text()` z jedynej kuracji — czyli tam, gdzie zdanie widzi człowiek.
//!
//! **Test dowodzi dwóch rzeczy naraz i obie są potrzebne.** Drugą jest to, że strumień został
//! przeczytany DO KOŃCA: potok o pojemności ~64 KB, którego nikt nie odbiera, zatrzymuje dziecko
//! na `write`, więc implementacja, która po limicie przestanie drenować, zawiesza gadatliwego
//! agenta — a z okna wygląda to jak agent, który myśli. Atrapa sypie ~180 KB, czyli trzy razy
//! pojemność potoku, i kończy znacznikiem: brak znacznika na końcu pliku znaczy, że ktoś
//! przestał czytać, a zawieszenie pada tu jako [`LIMIT`], nie jako zieleń.
//!
//! Przypadek odwrotny w tym samym module blokuje implementację mówiącą o obcięciu ZAWSZE: krótka
//! skarga mieści się w limicie i nie ma prawa wywołać ani słowa o obcięciu.
//!
//! Oba testy odpalają prawdziwy proces (atrapę `claude`), więc **nie** są `#[ignore]`: cel, który
//! melduje `0 passed`, nie jest dowodem (niezmiennik 19).

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentEvent, AgentHandle, Policy, RunSpec};
use loadout_lib::engine::line::{Curator, Line, Seen};
use loadout_lib::evidence::{EvidenceTarget, SafeInputManifest};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na każde pojedyncze oczekiwanie. Regresja ma się objawić jako **czerwony test**, nie
/// jako zawieszenie — a zawieszenie jest tu jednym z dwóch mierzonych zachowań: sterownik, który
/// przestaje drenować po limicie, zatrzymuje dziecko na pełnym potoku i wraca dopiero tutaj.
const LIMIT: Duration = Duration::from_secs(20);

/// Ile miejsca ma kanał zdarzeń. Z zapasem, bo pełny kanał zatrzymuje pętlę czytającą, a to
/// wygląda dokładnie jak zawieszony agent.
const CHANNEL: usize = 256;

/// Krok, którego to strumień. Po nim nazywa się plik `logs/agent-<krok>.stderr.log`.
const STEP: &str = "s_one";

/// Nazwa katalogu biegu z `docs/ARCHITECTURE.md` §8: `<ts>__<id>`.
const RUN_DIR: &str = "2026-08-29T09-00-00Z__01996500";

/// Ostatnia linia, jaką atrapa wypisuje na strumień skarg. Jej obecność na KOŃCU pliku jest
/// jedynym dowodem, że nikt nie przestał czytać w połowie.
const MARKER: &str = "LAST-COMPLAINT-MARKER";

/// Kawałek zdania o obcięciu, po którym poznajemy ten wiersz. Sam napis stoi w sterowniku —
/// tutaj potrzebna jest kotwica, a nie druga kopia treści.
const CUT: &str = "only the first";

/// Pierwsza linia gadatliwej skargi. Ma dojechać do wiersza `Done`, tak jak dojeżdżała dotąd:
/// uwaga o obcięciu ma stać OBOK niej, a nie zamiast niej.
const FIRST_COMPLAINT: &str = "the agent is unhappy about something 0";

/// Cała skarga agenta, który mieści się w limicie.
const SHORT_COMPLAINT: &str = "not logged in";

/// Atrapa `claude`, która wypisuje na strumień skarg ~180 KB — trzy razy pojemność potoku.
///
/// Wyłącznie polecenia wbudowane powłoki: nadzorca robi `env_clear()` i przepuszcza sześć nazw,
/// więc fikstura oparta o `seq` czy `awk` po cichu przestałaby działać. Kopertę czytamy przed
/// pierwszym `printf`, tak jak prawdziwe CLI.
const LOUD: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "2.1.233 (Claude Code)"
  exit 0
fi

IFS= read -r envelope

i=0
while [ "$i" -lt 4000 ]; do
  printf 'the agent is unhappy about something %s\n' "$i" >&2
  i=$((i + 1))
done
printf 'LAST-COMPLAINT-MARKER\n' >&2
exit 7
"#;

/// Atrapa `claude`, której cała skarga mieści się w limicie.
///
/// Skarga leci PRZED odczytem koperty, więc między nią a wyjściem procesu stoi pełna droga
/// powrotna przez potok wejścia — inaczej wiersz `Done` mierzyłby wyścig, a nie treść.
const QUIET: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "2.1.233 (Claude Code)"
  exit 0
fi

printf 'not logged in\n' >&2
IFS= read -r envelope
exit 3
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
///
/// Plik ze skryptem, nigdy `sh -c "…"` i nigdy kopia binarki systemowej: skopiowany plik
/// systemowy dostaje na `macOS` `SIGKILL` od podpisu kodu [T7 §8.2].
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// `RunSpec` jednej tury.
fn spec(cwd: &Path) -> RunSpec {
    RunSpec {
        run_id: Uuid::now_v7(),
        cwd: cwd.to_path_buf(),
        prompt: "say what this folder is for".to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Adres pliku z całością, tak jak ma stać w zdaniu: **względem katalogu biegu**.
///
/// Liczony, nie wpisany na sztywno — asercja na wklejonym napisie przeszłaby także wtedy, gdyby
/// zdanie wysyłało pod adres, którego nikt nie pisze.
fn where_all_of_it_is(target: &EvidenceTarget) -> String {
    let path = target.stderr_path();
    path.strip_prefix(target.root())
        .unwrap_or(path.as_path())
        .display()
        .to_string()
}

/// Puszcza jeden krok przez sterownik i wraca dopiero wtedy, gdy pętla czytająca skończyła,
/// a dowody są domknięte.
///
/// `close()` czeka na zadanie skarg (`finish_evidence`), więc dopiero po nim plik z całością ma
/// na dysku wszystko, co agent powiedział — pytanie o niego wcześniej mierzyłoby bufor, nie tee.
async fn run_one_step(
    home: &Path,
    run_dir: &Path,
    script: &str,
) -> Result<(Vec<AgentEvent>, EvidenceTarget), Box<dyn Error>> {
    let binary = write_script(home, "claude", script)?;
    // `logs/` powstaje razem z katalogiem biegu, tak jak w `commands::run`.
    fs::create_dir_all(run_dir.join("logs"))?;

    let target = EvidenceTarget::workflow_step(
        run_dir.to_path_buf(),
        STEP.to_owned(),
        SafeInputManifest {
            prompt_bytes: 30,
            context: Vec::new(),
            images: Vec::new(),
        },
    );
    let driver = ClaudeDriver::with_binary(binary)
        .with_evidence(target.clone())
        .ok_or("the claude driver refused the place its evidence goes")?;

    let (events_tx, mut events) = mpsc::channel(CHANNEL);
    let mut handle: Box<dyn AgentHandle> =
        timeout(LIMIT, driver.start(spec(home), events_tx)).await??;

    let mut seen = Vec::new();
    timeout(LIMIT, async {
        while let Some(decoded) = events.recv().await {
            seen.push(decoded.event);
        }
    })
    .await?;

    let _code = timeout(LIMIT, handle.close()).await??;
    Ok((seen, target))
}

/// Wiersze, które z tych zdarzeń zobaczy człowiek. Jedyna kuracja, ta sama co w biegu.
fn curated(events: &[AgentEvent]) -> Vec<Line> {
    let mut curator = Curator::new();
    let mut lines = Vec::new();
    for (at_ms, event) in events.iter().enumerate() {
        lines.extend(curator.observe(Seen {
            agent: "builder",
            at_ms: u64::try_from(at_ms).unwrap_or_default(),
            event,
            tool: None,
        }));
    }
    lines.extend(curator.flush());
    lines
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_that_out_talks_the_limit_says_where_all_of_it_is() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let home = dir.path().join("home");
    let run_dir = dir.path().join(RUN_DIR);
    fs::create_dir_all(&home)?;

    let (events, target) = run_one_step(&home, &run_dir, LOUD).await?;

    // ── Strumień przeczytany DO KOŃCA ─────────────────────────────────────────────────────
    // Ten strażnik przechodzi od pierwszego dnia i ma tak zostać: jest tu po to, żeby zdanie
    // o obcięciu nie dało się kupić za `break` po limicie, czyli za zawieszone dziecko.
    let written = fs::read_to_string(target.stderr_path())?;
    assert!(
        written.trim_end().ends_with(MARKER),
        "the file with all of it has to end with the LAST thing the agent said. A file that \
         stops earlier means somebody stopped reading the pipe once the kept part was full - and \
         a pipe nobody reads holds the child on write, which from the window looks exactly like \
         an agent that is thinking. The file ends with {:?}",
        written.chars().rev().take(60).collect::<String>()
    );

    // ── Zdanie dochodzi do CZŁOWIEKA ──────────────────────────────────────────────────────
    let lines = curated(&events);
    let at = lines
        .iter()
        .position(|line| line.text().contains(CUT))
        .ok_or_else(|| {
            format!(
                "the agent out-talked the limit and no row said so. The kept part is the first \
                 few kilobytes; without this row a person reads it as everything the agent said, \
                 and has no way to learn the rest exists. The stream produced {lines:?}"
            )
        })?;
    assert!(
        matches!(lines[at], Line::Problem { .. }),
        "the sentence has to arrive as the row a person reads before the summary: {:?}",
        lines[at]
    );
    let where_it_is = where_all_of_it_is(&target);
    assert!(
        lines[at].text().contains(&where_it_is),
        "saying that something was cut and not saying where the rest is leaves the person with \
         a warning and nowhere to go. It has to name {where_it_is:?}, and it said {:?}",
        lines[at].text()
    );

    // ── Powód porażki NIE zmienia treści ──────────────────────────────────────────────────
    let done = lines
        .iter()
        .position(|line| matches!(line, Line::Done { .. }))
        .ok_or("the turn ended without the ordinary terminal summary")?;
    assert!(
        at < done,
        "the note about the cut stands BEFORE the summary, like every other problem row: {lines:?}"
    );
    assert!(
        lines[done].text().contains(FIRST_COMPLAINT),
        "the Done row still carries the agent's own first line - the note about the cut is a row \
         beside it, never a suffix pushed past the 160-character ceiling of the stream row. It \
         said {:?}",
        lines[done].text()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_complaint_that_fits_says_nothing_about_being_cut() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let home = dir.path().join("home");
    let run_dir = dir.path().join(RUN_DIR);
    fs::create_dir_all(&home)?;

    let (events, target) = run_one_step(&home, &run_dir, QUIET).await?;

    let lines = curated(&events);
    assert!(
        lines.iter().all(|line| !line.text().contains(CUT)),
        "this agent said thirteen characters and all thirteen were kept, so a word about the \
         beginning being all there is would be a lie - and an implementation that always says it \
         passes the loud case for free. The stream produced {lines:?}"
    );
    let where_it_is = where_all_of_it_is(&target);
    assert!(
        lines.iter().all(|line| !line.text().contains(&where_it_is)),
        "and nothing sends the person to a file for a complaint they already read in full: \
         {lines:?}"
    );

    let done = lines
        .iter()
        .find(|line| matches!(line, Line::Done { .. }))
        .ok_or("the turn ended without the ordinary terminal summary")?;
    assert!(
        done.text().contains(SHORT_COMPLAINT),
        "the summary still answers 'why', with the agent's own words. It said {:?}",
        done.text()
    );

    Ok(())
}
