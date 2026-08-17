//! AC-2 dla T-05: `thinking` **nigdy** nie wchodzi do historii (reguła 5 z `ARCHITECTURE` §6).
//!
//! Ta jedna reguła usuwa większość wrażenia ściany tekstu, więc jest też jedyną, którą łatwo
//! złamać „bezpiecznie": wiersz `thinking` bez tekstu wygląda na ekranie jak nic, a lista rośnie
//! o cztery niewidzialne wiersze na turę i wirtualizacja mierzy je wszystkie.
//!
//! **Słaba wersja tego kryterium to sprawdzenie, że historia nie zawiera napisu `Thinking…`.**
//! Przechodzi ją dokładnie ta implementacja: wiersze są, tylko puste. Rozróżnia je asercja na
//! **długości** historii i na tym, że **stan** kuratora się zmienił — czyli że myślenie w ogóle
//! dotarło, tylko nie tam, gdzie ma nie docierać.
//!
//! Cztery zdarzenia myślenia, jedno pole: `system/thinking_tokens` i blok `thinking`
//! w `assistant` schodzą się w tym samym [`AgentEvent::Thinking`] (dekoder T-04), i to jest
//! celowe — myślenie nigdy nie niesie tekstu, więc nie ma czym się różnić.

use loadout_lib::engine::drivers::AgentEvent;
use loadout_lib::engine::line::{Curator, Line, LineKind, Seen, Status};

/// Agent, który myśli.
const AGENT: &str = "builder";

/// Proza, która zamyka myślenie i jest jedynym wierszem, jaki z tej sekwencji zostaje.
const PROSE: &str = "Greeting message stored in file.";

/// Kiedy przychodzą cztery zdarzenia myślenia — `thinking_tokens`(100), `thinking_tokens`(200),
/// blok `thinking` w `assistant`, `thinking_tokens`(227). Liczby tokenów nie wchodzą do
/// [`AgentEvent`] i nie mają prawa wejść: to jest stan, nie treść.
const THINKING_AT_MS: [u64; 4] = [0, 40, 80, 120];

/// Kiedy przychodzi tekst.
const PROSE_AT_MS: u64 = 160;

/// Jedno zdarzenie w chwili `at_ms`, od jedynego agenta w tym teście.
fn seen(at_ms: u64, event: &AgentEvent) -> Seen<'_> {
    Seen {
        agent: AGENT,
        at_ms,
        event,
        tool: None,
    }
}

#[test]
fn four_thoughts_and_one_sentence_leave_a_single_row() {
    let mut curator = Curator::new();
    let mut history: Vec<Line> = Vec::new();

    for at_ms in THINKING_AT_MS {
        history.extend(curator.observe(seen(at_ms, &AgentEvent::Thinking)));
        assert!(
            history.is_empty(),
            "thinking is a status, not a row: the fixed slot at the bottom is overwritten and \
             the scrollback never hears about it. After the thought at {at_ms} ms the history \
             held {history:?}"
        );
        assert_eq!(
            curator.status(),
            Some(Status::Thinking),
            "the thought still has to ARRIVE — dropping it on the floor would leave the bottom \
             of the screen dead while the agent works, which is the other half of this rule. \
             The status after the thought at {at_ms} ms was {:?}",
            curator.status()
        );
    }

    let prose = AgentEvent::Said {
        text: PROSE.to_owned(),
    };
    history.extend(curator.observe(seen(PROSE_AT_MS, &prose)));
    history.extend(curator.flush());

    assert_eq!(
        history.len(),
        1,
        "four thoughts and one sentence are ONE row of history. Any other number means the \
         empty thinking rows are in the scrollback, where the virtualised list measures every \
         one of them. The history was {history:?}"
    );
    assert_eq!(
        history[0].kind(),
        LineKind::Note,
        "the row that survives is the prose, the only prose in the feed"
    );
    assert_eq!(
        curator.status(),
        None,
        "the slot empties when a real line lands: a spinner that keeps spinning after the agent \
         has spoken says the run is still working when it is not"
    );
}

#[test]
fn no_row_of_the_thinking_kind_ever_reaches_the_history() {
    let mut curator = Curator::new();
    let mut history: Vec<Line> = Vec::new();

    // Dwie tury: myślenie, zdanie, myślenie, zdanie. Enum MA wariant `thinking` (T2 §7.2 poz.
    // 4) i to jest w porządku — rysuje go stały slot. Kurator nigdy nie dokłada go do wektora.
    for turn in 0..2_u64 {
        for step in 0..3_u64 {
            history.extend(curator.observe(seen(turn * 1_000 + step * 10, &AgentEvent::Thinking)));
        }
        let prose = AgentEvent::Said {
            text: format!("{PROSE} ({turn})"),
        };
        history.extend(curator.observe(seen(turn * 1_000 + 100, &prose)));
    }
    history.extend(curator.flush());

    assert_eq!(
        history.len(),
        2,
        "six thoughts and two sentences are two rows. The history was {history:?}"
    );
    assert!(
        !history.iter().any(|line| line.kind() == LineKind::Thinking),
        "the enum has the thinking variant, the history never does. A row with that kind in the \
         scrollback is the wall of text coming back one invisible line at a time. The history \
         was {history:?}"
    );
}
