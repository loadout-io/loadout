//! AC-3 dla T-05: sklejanie sąsiednich wierszy tego samego rodzaju w oknie 2 s, liczonym
//! **od pierwszego** wiersza grupy (reguła 4).
//!
//! **Słaba wersja tego kryterium to jeden przypadek: dwa `Read` dziesięć milisekund od siebie
//! dają jeden wiersz.** Przechodzi ją implementacja „zawsze sklejaj sąsiednie tego samego
//! rodzaju", która nie zamyka grupy nigdy — a wtedy agent czytający jeden plik na sekundę przez
//! pięć minut daje jeden, puchnący wiersz `Read 300 files` i widok stoi w miejscu.
//!
//! Rozróżnia je przypadek (b): czwarty `Read` o 2 100 ms musi **założyć nowy wiersz**. To jest
//! jedyna asercja, która odróżnia okno stałe (biegnące od pierwszego wiersza grupy) od
//! przesuwnego (biegnącego od ostatniego) — a przy oknie przesuwnym grupa nie zamyka się nigdy,
//! dopóki agent pracuje.
//!
//! Czas jedzie **argumentem** ([`Seen::at_ms`]), nie z zegara czytanego w środku kuratora.
//! Inaczej tego testu nie da się napisać bez `sleep`, a test ze `sleep` mierzy planistę systemu
//! operacyjnego, nie okno sklejania.

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Action, Curator, Line, LineKind, Seen, Tool};

/// Dwa agenty, bo klucz grupy zawiera identyfikator agenta (przypadek d).
const AGENT_A: &str = "builder";
/// Drugi agent tego samego biegu.
const AGENT_B: &str = "reviewer";

/// Zdarzenie startu czynności. Każde wywołanie ma własne `id`: to po nim wynik trafia do swojego
/// wiersza, więc dwa różne czytania z tym samym `id` byłyby wejściem, jakiego vendor nie wysyła.
fn tool_start(id: &str, action: Action) -> AgentEvent {
    let label = match action {
        Action::Read => "Reading a file",
        Action::Edit => "Editing a file",
        _ => "Working",
    };
    AgentEvent::ToolStart {
        id: id.to_owned(),
        label: label.to_owned(),
    }
}

/// Podaje kuratorowi jedną czynność i oddaje wiersze, które przez nią się domknęły.
fn act(
    curator: &mut Curator,
    agent: &str,
    at_ms: u64,
    id: &str,
    action: Action,
    target: &str,
) -> Vec<Line> {
    let event = tool_start(id, action);
    let tool = Tool::Started {
        action,
        target: target.to_owned(),
    };
    curator.observe(Seen {
        agent,
        at_ms,
        event: &event,
        tool: Some(&tool),
    })
}

/// Ścieżki jednego wiersza jako `&str`, w kolejności.
fn paths_of(line: &Line) -> Vec<&str> {
    line.paths().iter().map(String::as_str).collect()
}

#[test]
fn three_reads_inside_the_window_become_one_row_that_counts_them() {
    let mut curator = Curator::new();
    let mut history: Vec<Line> = Vec::new();

    for (index, (at_ms, path)) in [(0_u64, "/w/a.rs"), (400, "/w/b.rs"), (1_900, "/w/c.rs")]
        .into_iter()
        .enumerate()
    {
        history.extend(act(
            &mut curator,
            AGENT_A,
            at_ms,
            &format!("t{index}"),
            Action::Read,
            path,
        ));
    }
    history.extend(curator.flush());

    assert_eq!(
        history.len(),
        1,
        "three reads inside two seconds are one row. The history was {history:?}"
    );
    assert_eq!(
        history[0].kind(),
        LineKind::Read,
        "reads coalesce into a read row, not into something new"
    );
    assert_eq!(
        history[0].count(),
        3,
        "the row says HOW MANY files, because that number is the whole point of collapsing them"
    );
    assert_eq!(
        paths_of(&history[0]),
        ["/w/a.rs", "/w/b.rs", "/w/c.rs"],
        "expanding the row shows the three files in the order they were read; a row that keeps \
         only the last one turns 'Read 3 files' into a lie the moment somebody clicks it"
    );
}

#[test]
fn the_fourth_read_past_two_seconds_opens_a_second_row() {
    let mut curator = Curator::new();
    let mut history: Vec<Line> = Vec::new();

    for (index, (at_ms, path)) in [
        (0_u64, "/w/a.rs"),
        (400, "/w/b.rs"),
        (1_900, "/w/c.rs"),
        (2_100, "/w/d.rs"),
    ]
    .into_iter()
    .enumerate()
    {
        history.extend(act(
            &mut curator,
            AGENT_A,
            at_ms,
            &format!("t{index}"),
            Action::Read,
            path,
        ));
    }
    history.extend(curator.flush());

    assert_eq!(
        history.len(),
        2,
        "the window runs from the FIRST row of the group, so 2 100 ms is outside it and starts a \
         new row. One row here means the window slides with every event and never closes — the \
         swelling 'Read 300 files' that leaves the view standing still. The history was \
         {history:?}"
    );
    assert_eq!(
        history[0].count(),
        3,
        "the closed group kept the three that were inside the window"
    );
    assert_eq!(
        history[1].count(),
        1,
        "the read past the window starts counting again from one"
    );
    assert_eq!(
        paths_of(&history[1]),
        ["/w/d.rs"],
        "and it carries its own file, not the ones before it"
    );
}

#[test]
fn a_different_kind_between_two_reads_breaks_the_group() {
    let mut curator = Curator::new();
    let mut history: Vec<Line> = Vec::new();

    history.extend(act(&mut curator, AGENT_A, 0, "t0", Action::Read, "/w/a.rs"));
    history.extend(act(
        &mut curator,
        AGENT_A,
        100,
        "t1",
        Action::Edit,
        "/w/b.rs",
    ));
    history.extend(act(
        &mut curator,
        AGENT_A,
        200,
        "t2",
        Action::Read,
        "/w/c.rs",
    ));
    history.extend(curator.flush());

    let kinds: Vec<LineKind> = history.iter().map(Line::kind).collect();
    assert_eq!(
        kinds,
        [LineKind::Read, LineKind::Edit, LineKind::Read],
        "only ADJACENT rows of the same kind coalesce. Merging across the edit would tell the \
         reader that two files were read together when in truth something was written between \
         them — and the order is the only thing the feed is for. The history was {history:?}"
    );
}

#[test]
fn two_agents_reading_in_the_same_window_are_two_rows() {
    let mut curator = Curator::new();
    let mut history: Vec<Line> = Vec::new();

    history.extend(act(&mut curator, AGENT_A, 0, "a0", Action::Read, "/w/a.rs"));
    history.extend(act(
        &mut curator,
        AGENT_B,
        100,
        "b0",
        Action::Read,
        "/w/b.rs",
    ));
    history.extend(curator.flush());

    assert_eq!(
        history.len(),
        2,
        "the group key holds the agent, so two agents reading at the same moment are two rows. \
         One row here credits one agent with the other's work, and the rail beside the feed \
         stops matching the feed. The history was {history:?}"
    );
    assert_ne!(
        history[0].agent(),
        history[1].agent(),
        "and the two rows belong to the two different agents"
    );
    assert_eq!(
        history[0].count(),
        1,
        "neither agent read more than one file"
    );
    assert_eq!(
        history[1].count(),
        1,
        "neither agent read more than one file"
    );
}
