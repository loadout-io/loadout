//! AC-4 dla T-05: domyślnie zwinięte, a błąd rozwija się sam i pokazuje **ostatnie** 20 linii
//! (reguły 1–3).
//!
//! **Słaba wersja tego kryterium to `assert!(line.expanded())` dla nieudanej komendy.**
//! Przechodzi ją implementacja, która rozwija błąd i wkleja wszystkie sześćdziesiąt linii, i ta,
//! która wkleja pierwsze dwadzieścia — a pierwsze dwadzieścia linii wyjścia builda to zawsze
//! banner, nigdy przyczyna. Rozróżnia je para: `detail.len() == 20` **oraz**
//! `detail[0] == wyjście[40]`.
//!
//! Reguła 2 jest tu mierzona na **rodzaju**, nie na wierszu zbudowanym ręcznie z `expanded:
//! true`: gdyby to było pole ustawiane przy budowie, tabelę reguł mógłby nadpisać dowolny
//! wołający i „czysty widok" znowu zależałby od warstwy wyżej (niezmiennik 15).

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Action, Curator, Line, LineKind, Seen, Tool};

/// Agent, który uruchamia komendę.
const AGENT: &str = "builder";

/// Identyfikator wywołania — po nim wynik trafia do swojego wiersza.
const TOOL_ID: &str = "toolu_run_01";

/// Komenda, którą agent uruchomił.
const COMMAND: &str = "npm test";

/// Ile linii ma wyjście, na którym mierzymy regułę 3.
const OUTPUT_LINES: usize = 60;

/// Ile linii pokazuje rozwinięty błąd.
const TAIL_LINES: usize = 20;

/// Sufit podglądu w bajtach [T2 §6.3, obrona 2]: 200 KB wyniku narzędzia ma kosztować 2 KB na
/// granicy z widokiem, a reszta zostaje na dysku i za kliknięciem.
const PREVIEW_LIMIT: usize = 2_048;

/// Sześćdziesiąt linii wyjścia, każda rozpoznawalna po numerze i wystarczająco długa, żeby całe
/// wyjście przekroczyło sufit podglądu — inaczej „podgląd ≤ 2 KB" przechodziłby na wklejeniu
/// całości.
fn output_lines() -> Vec<String> {
    (1..=OUTPUT_LINES)
        .map(|number| format!("line {number:02} of the build output {}", "-".repeat(60)))
        .collect()
}

/// Puszcza przez kuratora jedną komendę: start, a potem wynik z pełnym wyjściem.
///
/// Pełne wyjście wchodzi przez [`Tool::Ended`], bo [`AgentEvent::ToolEnd`] niesie z definicji
/// **jednolinijkowe** podsumowanie — reguła 3 nie miałaby z czego wziąć dwudziestu linii.
fn ran(ok: bool, output: &[String]) -> Vec<Line> {
    let mut curator = Curator::new();

    let start = AgentEvent::ToolStart {
        id: TOOL_ID.to_owned(),
        label: "Running the tests".to_owned(),
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
        id: TOOL_ID.to_owned(),
        ok,
        summary: "the tests".to_owned(),
    };
    let ended = Tool::Ended {
        output: output.join("\n"),
    };
    // Wynik przychodzi wewnątrz okna sklejania: to kryterium jest o zwijaniu, nie o oknie, więc
    // nie ma prawa zależeć od tego, czy komenda trwała dłużej niż dwie sekundy.
    history.extend(curator.observe(Seen {
        agent: AGENT,
        at_ms: 900,
        event: &end,
        tool: Some(&ended),
    }));
    history.extend(curator.flush());
    history
}

/// Rodzaje, które są widoczne od razu: proza, pytania, błędy i struktura [T2 §7.3, reguła 2].
fn open_by_default() -> Vec<Line> {
    vec![
        Line::Run {
            agent: AGENT.to_owned(),
            text: "Fix the login bug · Research → Plan → Build".to_owned(),
        },
        Line::Step {
            agent: AGENT.to_owned(),
            text: "Planning".to_owned(),
        },
        Line::Agent {
            agent: AGENT.to_owned(),
            text: "Researcher 2 joined".to_owned(),
        },
        Line::Note {
            agent: AGENT.to_owned(),
            text: "Greeting message stored in file.".to_owned(),
        },
        Line::Asked {
            agent: AGENT.to_owned(),
            text: "Needs your answer: which database?".to_owned(),
            options: vec!["Postgres".to_owned(), "SQLite".to_owned()],
        },
        Line::Handoff {
            agent: AGENT.to_owned(),
            text: "Planner → Implementer".to_owned(),
        },
        Line::Problem {
            agent: AGENT.to_owned(),
            text: "Couldn't reach the API".to_owned(),
            resets_at: None,
        },
        Line::Done {
            agent: AGENT.to_owned(),
            text: "Done · 2 turns · 6.2s · $0.15".to_owned(),
            turns: 2,
            duration_ms: 6_220,
            cost_usd: Some(0.148_362_900_000_000_02),
            ended: loadout_lib::engine::line::Ended::Well,
        },
    ]
}

/// Rodzaje, które są mechaniką: widać, że się stały, treść jest za kliknięciem.
fn collapsed_by_default() -> Vec<Line> {
    vec![
        Line::Read {
            agent: AGENT.to_owned(),
            text: "Read 3 files".to_owned(),
            count: 3,
            paths: vec!["/w/a.rs".to_owned(), "/w/b.rs".to_owned()],
            detail_id: Some(1),
        },
        Line::Search {
            agent: AGENT.to_owned(),
            text: "Searched for \"auth token\" — 12 matches".to_owned(),
            count: 12,
            paths: vec!["/w/a.rs".to_owned()],
            detail_id: Some(2),
        },
        Line::Edit {
            agent: AGENT.to_owned(),
            text: "Edited /w/a.rs  +12 −4".to_owned(),
            count: 1,
            paths: vec!["/w/a.rs".to_owned()],
            added: 12,
            removed: 4,
            detail_id: Some(3),
        },
        Line::Ran {
            agent: AGENT.to_owned(),
            text: "Ran the tests — ok".to_owned(),
            ok: true,
            preview: "line 01".to_owned(),
            detail: Vec::new(),
            detail_id: Some(4),
        },
        Line::Memory {
            agent: AGENT.to_owned(),
            text: "Saved a note — api-conventions.md".to_owned(),
            path: "api-conventions.md".to_owned(),
        },
    ]
}

#[test]
fn a_command_that_worked_stays_shut_and_leaves_its_output_behind_a_click() {
    let output = output_lines();
    let history = ran(true, &output);

    assert_eq!(
        history.len(),
        1,
        "one command is one row, whatever its output was (rule 1). The history was {history:?}"
    );
    let line = &history[0];
    assert_eq!(
        line.kind(),
        LineKind::Ran,
        "a command that ran is a ran row"
    );
    assert!(
        !line.expanded(),
        "a command that worked is mechanics, and mechanics are collapsed: sixty lines of output \
         nobody asked for is the wall of text this whole task exists to remove"
    );
    assert!(
        line.detail_id().is_some(),
        "collapsed does not mean lost — the row has to point at the full output, or 'show more' \
         has nothing to open and the output is gone for good"
    );
    assert!(
        !line.preview().is_empty(),
        "the row carries a preview, so the detail pane has something to show before it fetches"
    );
    assert!(
        line.preview().len() <= PREVIEW_LIMIT,
        "the preview is capped at {PREVIEW_LIMIT} bytes: a 200 KB tool result has to cost 2 KB \
         at the boundary, not 200 KB. This one was {} bytes",
        line.preview().len()
    );
}

#[test]
fn a_command_that_did_not_work_opens_itself_at_the_last_twenty_lines() {
    let output = output_lines();
    let history = ran(false, &output);

    assert_eq!(
        history.len(),
        1,
        "one command is one row, failed or not. The history was {history:?}"
    );
    let line = &history[0];
    assert!(
        line.expanded(),
        "failure is the one place a wall of text is wanted, and it opens itself: a person who \
         has to click to find out why the build broke will not click"
    );
    assert_eq!(
        line.detail().len(),
        TAIL_LINES,
        "exactly the last twenty lines. All sixty is the wall of text again; the first twenty \
         are the banner, never the reason. The row showed {:?}",
        line.detail()
    );
    assert_eq!(
        line.detail()[0],
        output[OUTPUT_LINES - TAIL_LINES],
        "the first line shown is line 41 of the output — the tail, not the head"
    );
    assert_eq!(
        line.detail()[TAIL_LINES - 1],
        output[OUTPUT_LINES - 1],
        "and the last line shown is the last line of the output, which is where the reason lives"
    );
}

#[test]
fn prose_questions_failures_and_structure_are_open_while_mechanics_are_not() {
    for line in open_by_default() {
        assert!(
            line.expanded(),
            "{:?} is prose, a question, a failure or structure, so it is visible without a \
             click (rule 2)",
            line.kind()
        );
    }
    for line in collapsed_by_default() {
        assert!(
            !line.expanded(),
            "{:?} is mechanics, so it is one collapsed line and its body is behind it (rule 2)",
            line.kind()
        );
    }
}

#[test]
fn no_row_ever_carries_a_second_line_of_text() {
    let prose = "Greeting message stored in file.\nIt is two lines long.\nAnd then a third.";
    let event = AgentEvent::Said {
        text: prose.to_owned(),
    };
    let mut curator = Curator::new();
    let mut history = curator.observe(Seen {
        agent: AGENT,
        at_ms: 0,
        event: &event,
        tool: None,
    });
    history.extend(curator.flush());

    assert_eq!(
        history.len(),
        1,
        "three lines of prose are one row (rule 1). The history was {history:?}"
    );
    assert!(
        !history[0].text().is_empty(),
        "and the row still says something"
    );
    assert!(
        !history[0].text().contains('\n'),
        "one action, one line: text with a newline in it is a row whose height nobody can \
         predict, and the virtualised list measures every one of them. The row said {:?}",
        history[0].text()
    );

    for line in open_by_default().into_iter().chain(collapsed_by_default()) {
        assert!(
            !line.text().contains('\n'),
            "no kind carries a second line of text; {:?} did",
            line.kind()
        );
    }
}
