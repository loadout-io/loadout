//! AC-5 dla T-10: dokładnie jeden `Finished` na turę, a koszt Codexa to „nie wiem", nie „zero".
//!
//! **Słaba wersja tego kryterium liczy `Finished` w przypadku (a) i stwierdza, że jest jeden.**
//! Przechodzi ją sterownik, który emituje `Finished` **wyłącznie na wyjściu procesu** — a wtedy
//! przypadek (c) wygląda identycznie jak sukces, `turn.failed` raportuje `ok == true`, i stożek
//! poniżej rusza na pustym przekazaniu, bo krok „się udał" i nic nie przekazał.
//!
//! Rozróżnia to tabela ze **wszystkimi czterema** przebiegami. Każdy jest osobnym testem, bo
//! każdy jest osobnym procesem i ma po sobie zostawić własne zdanie w wyjściu bramki:
//!
//! | # | Atrapa wypisuje | Kod wyjścia | Czego żądamy |
//! |---|---|---|---|
//! | a | `turn.completed` z `usage` | 0 | jeden `Finished`, `ok`, tokeny z drutu, **`cost_usd == None`** |
//! | b | `turn.failed` | 0 | jeden `Finished`, `!ok`, `Failed(<komunikat>)` |
//! | c | nic zamykającego | 0 | jeden `Finished`, `!ok`, powód nazywa brak zdarzenia tury |
//! | d | `turn.completed`, potem pada | 3 | **ten sam jeden** `Finished`, i ani jednego po nim |
//!
//! Wyjście procesu jest sygnałem **wtórnym** [T1 §8.5]. Przypadek (c) mówi, że kod 0 nie czyni
//! sukcesu z tury, która nie powiedziała, co zrobiła; przypadek (d) mówi, że kod ≠ 0 nie
//! odbiera wyniku turze, która już się wypowiedziała.
//!
//! **`cost_usd` musi być `None`.** Codex nie podaje kosztu w `usage`, a `Some(0.0)` wypisze na
//! ekranie `$0.00` i nauczy człowieka, że Codex jest darmowy. Szacowanie kosztu z tokenów jest
//! świadomie poza zakresem: cennik w kodzie to trzecie miejsce, w którym trzeba go aktualizować.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::{
    AgentEvent, AgentHandle, FinishReason, Outcome, Policy, RunSpec,
};
use loadout_lib::engine::line::{Curator, Line, Seen};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

/// Sufit na pojedyncze oczekiwanie. Regresja ma być czerwonym testem, nie zawieszeniem.
const LIMIT: Duration = Duration::from_secs(8);

/// Miejsce w kanale zdarzeń, z zapasem.
const CHANNEL: usize = 256;

/// Komunikat, który atrapa wkłada w `turn.failed`. Musi dojechać do powodu **dosłownie**:
/// to jest zdanie, które przeczyta człowiek pytający „dlaczego".
const COMPLAINT: &str = "the sandbox refused a write outside the workspace";

/// (a) Tura, która się udała, z `usage` w kształcie z T1 §6.2.
const COMPLETED: &str = r#"#!/bin/sh
printf '{"type":"thread.started","thread_id":"thread-a"}\n'
printf '{"type":"turn.started"}\n'
printf '{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122,"reasoning_output_tokens":64}}\n'
exit 0
"#;

/// (b) Tura, którą vendor zamknął błędem — kształt z prawdziwego biegu [T1 §6.2].
const FAILED: &str = r#"#!/bin/sh
printf '{"type":"thread.started","thread_id":"thread-b"}\n'
printf '{"type":"turn.started"}\n'
printf '{"type":"turn.failed","error":{"message":"the sandbox refused a write outside the workspace"}}\n'
exit 0
"#;

/// (c) Proces, który wyszedł czysto i **nie powiedział, co zrobił**.
const SILENT: &str = r#"#!/bin/sh
printf '{"type":"thread.started","thread_id":"thread-c"}\n'
printf '{"type":"turn.started"}\n'
exit 0
"#;

/// (d) Tura zakończona poprawnie, po której proces mimo to pada.
const COMPLETED_THEN_BROKE: &str = r#"#!/bin/sh
printf '{"type":"thread.started","thread_id":"thread-d"}\n'
printf '{"type":"turn.completed","usage":{"input_tokens":7,"cached_input_tokens":8,"output_tokens":9}}\n'
exit 3
"#;

/// CLI, które odrzuciło argv przed uruchomieniem tury. To jest dokładny kształt incydentu z
/// 2026-08-21: `-C` stało po `resume`, więc prawdziwy codex-cli 0.148.0 napisał tylko na stderr
/// i wyszedł 2. Sam `Finished::reason` nie jest widoczny w wierszu `Done`.
const REJECTED_ARGV: &str = r#"#!/bin/sh
printf '%s\n' "error: unexpected argument '-C' found" >&2
exit 2
"#;

/// Zapisuje wykonywalny skrypt i zwraca jego ścieżkę.
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
        prompt: "do the thing".to_owned(),
        model: Some("gpt-5-codex".to_owned()),
        system_append: None,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Uruchamia jedną turę na atrapie i oddaje **wszystkie** jej zdarzenia oraz to, co powiedział
/// `wait()`.
///
/// Uchwyt ginie przed odczytem kanału i to jest część pomiaru: sesja, po której zniknął ostatni
/// nadajnik, ma zamknąć kanał — czyli `recv()` oddaje `None` zamiast czekać w nieskończoność
/// na zdarzenie, które nigdy nie przyjdzie.
async fn one_turn(
    script: &str,
) -> Result<(Vec<AgentEvent>, anyhow::Result<Outcome>), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let binary = write_script(dir.path(), "codex", script)?;

    let (tx, mut rx) = mpsc::channel(CHANNEL);
    let driver = CodexDriver::with_binary(binary);
    let mut handle = timeout(LIMIT, driver.start_session(spec(dir.path()), tx)).await??;

    let outcome = timeout(LIMIT, handle.wait()).await?;
    drop(handle);

    let mut events = Vec::new();
    while let Some(decoded) = timeout(LIMIT, rx.recv()).await? {
        events.push(decoded.event);
    }
    Ok((events, outcome))
}

/// Ile razy strumień ogłosił koniec tury.
fn finishes(events: &[AgentEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, AgentEvent::Finished(_)))
        .count()
}

/// Sprawdza to, co jest wspólne dla wszystkich czterech przebiegów: dokładnie jedno zdarzenie
/// końca i **ani jednego po nim**.
fn one_ending_and_nothing_after(events: &[AgentEvent], case: &str) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        finishes(events),
        1,
        "{case}: exactly one end of turn, always. Zero leaves the step sitting in `running` for \
         the rest of the run; two make the rail draw a second summary for a turn that happened \
         once. The stream produced {events:?}"
    );
    let at = events
        .iter()
        .position(|event| matches!(event, AgentEvent::Finished(_)))
        .ok_or_else(|| format!("{case}: nothing in this stream ended the turn"))?;
    assert_eq!(
        at,
        events.len() - 1,
        "{case}: the end of turn has to be the LAST thing on the channel. Anything after it is \
         an event about a turn that is already over, and the rail has nowhere left to put it. \
         The stream produced {events:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_completed_turn_hands_over_its_tokens_and_admits_it_knows_no_cost()
-> Result<(), Box<dyn Error>> {
    let (events, outcome) = one_turn(COMPLETED).await?;
    one_ending_and_nothing_after(&events, "(a) turn.completed")?;
    let outcome = outcome?;

    assert!(
        outcome.ok,
        "turn.completed means the turn completed. It came out as {outcome:?}"
    );
    assert_eq!(
        outcome.reason,
        FinishReason::Completed,
        "and it finished for the plainest of reasons. It came out as {:?}",
        outcome.reason
    );

    assert_eq!(
        outcome.tokens.input, 24_763,
        "fresh input, straight from usage.input_tokens"
    );
    assert_eq!(
        outcome.tokens.cached, 24_448,
        "usage.cached_input_tokens, and this is the number that says whether context isolation \
         works at all - reading the wrong field here makes that measurement silently meaningless"
    );
    assert_eq!(
        outcome.tokens.output, 122,
        "output, straight from usage.output_tokens"
    );

    assert!(
        outcome.cost_usd.is_none(),
        "Codex does not report a cost, so the honest answer is 'I do not know'. Some(0.0) prints \
         $0.00 on the screen and teaches the person that Codex is free - and that number then \
         sums into a total nobody ordered. It came out as {:?}",
        outcome.cost_usd
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_turn_says_so_and_carries_the_vendors_own_sentence() -> Result<(), Box<dyn Error>>
{
    let (events, outcome) = one_turn(FAILED).await?;
    one_ending_and_nothing_after(&events, "(b) turn.failed")?;
    let outcome = outcome?;

    assert!(
        !outcome.ok,
        "turn.failed is a failure. A driver that ends turns on process exit reports this one as \
         a success, because the process exited 0. It came out as {outcome:?}"
    );

    let FinishReason::Failed(why) = &outcome.reason else {
        return Err(format!(
            "(b): a failed turn needs a readable reason, not a bare verdict: {:?}",
            outcome.reason
        )
        .into());
    };
    assert!(
        why.contains(COMPLAINT),
        "the reason has to carry what the vendor actually said. That sentence is the answer to \
         the question somebody asks next, and it is already written for us in error.message. \
         It said {why:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clean_exit_without_a_closing_event_is_still_a_failure() -> Result<(), Box<dyn Error>> {
    let (events, outcome) = one_turn(SILENT).await?;
    one_ending_and_nothing_after(&events, "(c) no closing event")?;
    let outcome = outcome?;

    assert!(
        !outcome.ok,
        "a process that exited cleanly and never said what it did has nothing to hand on. \
         Process exit is the secondary signal; the closing event is the primary one. \
         It came out as {outcome:?}"
    );

    let FinishReason::Failed(why) = &outcome.reason else {
        return Err(format!(
            "(c): the turn has to end somehow, with a reason a person can read: {:?}",
            outcome.reason
        )
        .into());
    };
    assert!(
        why.to_lowercase().contains("turn"),
        "the reason has to NAME what was missing - the event that ends the turn - because that \
         is the one sentence telling whoever reads it where to look. 'The agent stopped' says \
         nothing and could mean anything. It said {why:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_that_breaks_after_the_turn_does_not_get_a_second_ending()
-> Result<(), Box<dyn Error>> {
    let (events, outcome) = one_turn(COMPLETED_THEN_BROKE).await?;
    one_ending_and_nothing_after(&events, "(d) turn.completed then exit 3")?;
    let outcome = outcome?;

    assert!(
        outcome.ok,
        "the turn already said it completed, and the process falling over afterwards does not \
         take that back - by then the work is done and the handoff is written. Exit code is the \
         secondary signal [T1 8.5]. It came out as {outcome:?}"
    );
    assert_eq!(
        outcome.tokens.output, 9,
        "and the outcome the caller gets is the FIRST one, the one the turn actually reported, \
         not a second one synthesised from the exit code"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cli_parse_error_reaches_the_visible_problem_before_the_summary()
-> Result<(), Box<dyn Error>> {
    let (events, outcome) = one_turn(REJECTED_ARGV).await?;
    one_ending_and_nothing_after(&events, "(e) CLI rejected argv")?;
    assert_eq!(
        events.len(),
        2,
        "an EOF failure needs a Notice before Finished. FinishReason is not part of the Done \
         line, so a lone Finished produces only `Didn't work · 0 turns` and hides the cause. \
         The live stream produced {events:?}"
    );

    let mut curator = Curator::new();
    let mut lines = Vec::new();
    for (at_ms, event) in events.iter().enumerate() {
        lines.extend(curator.observe(Seen {
            agent: "Lead",
            at_ms: u64::try_from(at_ms).unwrap_or_default(),
            event,
            tool: None,
        }));
    }
    lines.extend(curator.flush());

    let Some(Line::Problem { text, .. }) = lines.first() else {
        return Err(format!(
            "the CLI's sentence did not become the Problem row a person sees before Done: \
             {lines:?}"
        )
        .into());
    };
    assert!(
        text.contains("unexpected argument '-C'"),
        "the visible Problem row replaced the CLI's diagnosis with {text:?}"
    );
    assert!(
        matches!(lines.get(1), Some(Line::Done { .. })),
        "the visible diagnosis must be followed by the ordinary terminal summary: {lines:?}"
    );

    let outcome = outcome?;
    assert!(!outcome.ok);
    Ok(())
}
