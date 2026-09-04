//! Z-36: komenda, która trwa, jest widoczna od pierwszej sekundy — jednym wierszem, który
//! aktualizuje swój czas, a nie sześcioma wierszami po jednym na heartbeat.
//!
//! Zmierzone 2026-09-04 na liderze w meetnotes: siedem minut w jednym wywołaniu Basha
//! (`until grep … ; do sleep 5; done`, opis „Wait for Murmur binary to launch"), CLI słało co
//! 30 s `{"type":"tool_progress","elapsed_time_seconds":N,"heartbeat":true}` — i ekran nie
//! pokazał NIC. Wiersz `Ran … — ok` powstawał dopiero z pary `tool_use`+`tool_result`, a
//! `tool_progress` ginął już w klasyfikatorze (`stream.rs`, `KNOWN_TYPES`). Właściciel opisał
//! to jako „lider się zawiesza i nie odpisuje".
//!
//! **Słaba wersja tego kryterium: `assert!(lines.iter().any(|l| l.kind() == LineKind::Ran))`.**
//! Przechodzi dziś, bez jednej linii zmiany, bo wiersz o zakończonej komendzie zawsze był.
//! Rozróżnia je kolejność i treść: PIERWSZY wiersz tej komendy ma stanąć w chwili, w której
//! komenda ruszyła, i mówić `Working: … · 0s`, a każdy kolejny heartbeat ma go PRZEPISAĆ —
//! ten sam podmiot, większy czas — zamiast dołożyć wiersz obok.
//!
//! Asercje idą wyłącznie przez `Line::kind()` i `Line::text()`, czyli przez API, które istniało
//! przed tą zmianą: moduł kompiluje się na starym drzewie i pada w WYKONANIU (AGENTS.md §2a
//! p. 4). Test, który się nie kompiluje, niczego nie uruchomił.
//!
//! # Pięć dróg, bo tyle ich ma ta komenda
//!
//! Krótka bez heartbeatu, długa z heartbeatami, długa zakończona błędem, przerwana zejściem
//! procesu (domyka `Curator::flush`) i puszczona w tło. Droga, której się nie ruszy, jest tą,
//! o którą pyta pierwsze zgłoszenie.
//!
//! # Dlaczego wiersz nazywa KOMENDĘ, a nie opis, który model napisał sobie sam
//!
//! Bo tak stoi w `engine::line::ran_text` od początku i nic w tym zadaniu tego nie podważa:
//! `description` jest frazą czasownikową („Wait for Murmur binary to launch"), a wiersz ma
//! kształt `Ran <co> — <jak poszło>` i potrzebuje rzeczownika. Komenda jest przy tym jedyną
//! wartością w tym wierszu, której nikt nie wymyślił. Fikstura niesie oba pola, dokładnie jak
//! prawdziwy drut, więc ta preferencja jest tu ZMIERZONA, a nie założona.

use std::path::Path;

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Action, Curator, Line, LineKind, Seen, Tool};
use loadout_lib::engine::stream;
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Agent, którego strumień to jest.
const AGENT: &str = "lead";

/// Identyfikator wywołania — jeden na całą komendę, po nim wynik trafia do swojego wiersza.
const CALL: &str = "toolu_wait_01";

/// Komenda, która wisiała siedem minut. To ona jest podmiotem wiersza.
const COMMAND: &str = "until grep -q Murmur <(ps aux); do sleep 5; done";

/// Opis, który model napisał sobie sam. Jedzie w fiksturze, bo jedzie na prawdziwym drucie.
const DESCRIPTION: &str = "Wait for Murmur binary to launch";

/// Zapowiedź komendy: `tool_use` Basha z oboma polami wejścia.
fn tool_use() -> String {
    format!(
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{CALL}","name":"Bash","input":{{"command":"{COMMAND}","description":"{DESCRIPTION}"}}}}]}}}}"#
    )
}

/// Heartbeat, dokładnie w kształcie zmierzonym na CLI: trzy klucze i **ani jednego `id`**.
fn heartbeat(seconds: u64) -> String {
    format!(r#"{{"type":"tool_progress","elapsed_time_seconds":{seconds},"heartbeat":true}}"#)
}

/// Wynik komendy.
fn tool_result(ok: bool) -> String {
    let error = if ok { "false" } else { "true" };
    format!(
        r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{CALL}","is_error":{error},"content":"Murmur is up"}}]}}}}"#
    )
}

/// `system/task_started` dla komendy puszczonej w tło.
fn backgrounded() -> String {
    format!(
        r#"{{"type":"system","subtype":"task_started","tool_use_id":"{CALL}","is_backgrounded":true,"description":"{DESCRIPTION}"}}"#
    )
}

/// Puszcza podane linie przez prawdziwą pompę i oddaje wiersze, które z niej wyszły.
async fn pumped(dir: &Path, lines: &[String]) -> anyhow::Result<Vec<Line>> {
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\n");
    }
    let source = dir.join("stdout.jsonl");
    tokio::fs::write(&source, &bytes).await?;
    let reader = BufReader::new(tokio::fs::File::open(&source).await?);

    let (tx, mut rx) = mpsc::channel(256);
    stream::pump(reader, &dir.join("agent-1.jsonl"), AGENT, tx).await?;

    let mut history = Vec::new();
    while let Some(line) = rx.recv().await {
        history.push(line);
    }
    Ok(history)
}

/// Zdania wierszy o komendzie, w kolejności, w jakiej wyszły z pompy.
fn said_about_commands(history: &[Line]) -> Vec<String> {
    history
        .iter()
        .filter(|line| line.kind() == LineKind::Ran)
        .map(|line| line.text().to_owned())
        .collect()
}

/// Sprawdza, że pierwsze zdanie o komendzie OTWIERA ją, i oddaje resztę do porównania co do
/// znaku.
///
/// # Dlaczego pierwszy wiersz nie jest porównywany dosłownie
///
/// Bo jego czas czyta ZEGAR ŚCIENNY tej maszyny: pompa stempluje wiersz chwilą od otwarcia
/// transkryptu, a vendor jeszcze nic o czasie nie powiedział. Na wolnej maszynie ten wiersz
/// mówi `· 1s` zamiast `· 0s` i ma rację — a kryterium, które pada od cudzej suity biegnącej
/// obok, jest flakiem, nie wyrocznią (zmierzone: ten test padł raz w pełnym przebiegu 1301
/// testów i przeszedł osobno). Pozostałe wiersze niosą czas vendora (30 s i więcej), więc
/// żaden realny poślizg planisty ich nie rusza. Dosłowne brzmienie otwarcia — `· 0s` — sądzi
/// [`the_wording_of_a_command_that_takes_no_time`], gdzie zegar jest argumentem.
fn after_the_command_opens(said: &[String]) -> &[String] {
    let opening = format!("Working: {COMMAND} · ");
    assert!(
        said.first()
            .is_some_and(|first| first.starts_with(&opening)),
        "the first word about this command has to be that it STARTED — `{opening}<time>`. \
         Without it a person watches a blank screen for as long as the command runs, which is \
         the whole defect this file was written against. The rows were {said:?}"
    );
    &said[1..]
}

#[tokio::test]
async fn the_row_of_one_command_keeps_its_time_growing_until_it_ends() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = pumped(
        dir.path(),
        &[
            tool_use(),
            heartbeat(30),
            heartbeat(300),
            heartbeat(450),
            tool_result(true),
        ],
    )
    .await?;

    let said = said_about_commands(&history);
    assert_eq!(
        after_the_command_opens(&said),
        [
            format!("Working: {COMMAND} · 30s"),
            format!("Working: {COMMAND} · 5m"),
            format!("Working: {COMMAND} · 7m 30s"),
            format!("Ran {COMMAND} — ok · 7m 30s"),
        ],
        "a command that took seven and a half minutes has to be on screen from the second it \
         started, and every heartbeat has to REWRITE that one row with a bigger number — same \
         call, same subject, one carrier. A history whose first word about this command is \
         `Ran … — ok` is the screen a person watched for seven minutes while it said nothing, \
         and a history with one row per heartbeat is the wall of text the whole view exists to \
         remove. All of the rows were {:?}",
        history.iter().map(Line::text).collect::<Vec<_>>(),
    );

    Ok(())
}

#[test]
fn the_wording_of_a_command_that_takes_no_time() {
    /* ZEGAR JEST TU ARGUMENTEM, nie ścianą, i to jest ta sama reguła, na której stoi cały
     * `Curator` (`engine::line`, typ `Seen`): kurator z własnym zegarem nie da się przetestować
     * bez `sleep`, a test ze `sleep` mierzy planistę systemu operacyjnego. Dwa zdarzenia w tej
     * samej milisekundzie to komenda, która wróciła, zanim ktokolwiek doliczył pierwszą sekundę
     * — czyli dokładnie ta droga, o której to kryterium mówi. */
    let mut curator = Curator::new();
    let start = AgentEvent::ToolStart {
        id: CALL.to_owned(),
        label: DESCRIPTION.to_owned(),
    };
    let started = Tool::Started {
        action: Action::Ran,
        target: COMMAND.to_owned(),
    };
    let mut history = curator.observe(Seen {
        agent: AGENT,
        at_ms: 0,
        event: &start,
        tool: Some(&started),
    });
    let end = AgentEvent::ToolEnd {
        id: CALL.to_owned(),
        ok: true,
        summary: "Murmur is up".to_owned(),
    };
    history.extend(curator.observe(Seen {
        agent: AGENT,
        at_ms: 0,
        event: &end,
        tool: None,
    }));

    assert_eq!(
        said_about_commands(&history),
        vec![
            format!("Working: {COMMAND} · 0s"),
            format!("Ran {COMMAND} — ok"),
        ],
        "a command opens its row the moment it starts, at zero, and a command that came back \
         before anybody counted a second has no time to show — so its closing row reads exactly \
         as it read before this change. `· 0s` glued to the end of every line of every \
         transcript this product has ever written is a number that says nothing, standing where \
         the eye looks for the answer. All of the rows were {:?}",
        history.iter().map(Line::text).collect::<Vec<_>>(),
    );
}

#[tokio::test]
async fn a_long_command_that_failed_says_how_long_it_had_been_failing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = pumped(
        dir.path(),
        &[
            tool_use(),
            heartbeat(30),
            heartbeat(450),
            tool_result(false),
        ],
    )
    .await?;

    let said = said_about_commands(&history);
    assert_eq!(
        said.last().map(String::as_str),
        Some(format!("Ran {COMMAND} — didn't work · 7m 30s").as_str()),
        "the command failed after seven and a half minutes and the row that closes it has to say \
         both things. How long it ran is the difference between a command that broke at once and \
         one that hung. All of the rows were {said:?}"
    );
    let closed = history
        .iter()
        .rfind(|line| line.kind() == LineKind::Ran)
        .ok_or_else(|| anyhow::anyhow!("no row about the command came out at all"))?;
    assert!(
        closed.expanded(),
        "a command that did not work opens itself (rule 3): a person who has to click to find \
         out why it broke will not click. The row was {closed:?}"
    );

    Ok(())
}

#[tokio::test]
async fn a_command_the_process_died_under_is_closed_with_the_time_it_had() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    // Strumień urywa się PO heartbeatach i przed wynikiem — tak wygląda proces, który zszedł
    // w połowie komendy. Domyka ją `Curator::flush`, czyli ta sama droga, co dotąd.
    let history = pumped(dir.path(), &[tool_use(), heartbeat(30), heartbeat(300)]).await?;

    let said = said_about_commands(&history);
    assert_eq!(
        after_the_command_opens(&said),
        [
            format!("Working: {COMMAND} · 30s"),
            format!("Working: {COMMAND} · 5m"),
            format!("Ran {COMMAND} — didn't work · 5m"),
        ],
        "a stream that stops in the middle of a command is a command that did not work from a \
         person's side: the process left and never said what it did. The row closing it keeps \
         the last time anybody counted — dropping it here would leave the transcript saying a \
         five-minute hang took no time at all. All of the rows were {:?}",
        history.iter().map(Line::text).collect::<Vec<_>>(),
    );

    Ok(())
}

#[tokio::test]
async fn a_command_pushed_into_the_background_stops_pretending_to_be_in_flight()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = pumped(dir.path(), &[tool_use(), backgrounded(), tool_result(true)]).await?;

    let said = said_about_commands(&history);
    assert_eq!(
        said.last().map(String::as_str),
        Some(format!("Started in the background: {COMMAND}").as_str()),
        "a command pushed into the background is not work in flight: nobody is waiting on it and \
         its clock is not ours to show. The last word about it has to be that it was started and \
         let go. All of the rows were {said:?}"
    );
    assert!(
        said.iter().all(|text| text.contains(COMMAND)),
        "and every one of those rows is about the SAME command — one call, one carrier, so the \
         window can rewrite the row in place instead of stacking three of them. The rows were \
         {said:?}"
    );
    assert!(
        !said.iter().any(|text| text.contains("— ok")),
        "the result that comes back the moment a background command is let go says only that it \
         STARTED. A row reading `— ok` here tells a person a server that is still booting has \
         already finished. The rows were {said:?}"
    );

    Ok(())
}
