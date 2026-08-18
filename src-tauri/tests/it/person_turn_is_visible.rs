//! Tura człowieka wchodzi do historii biegu — i wchodzi dopiero wtedy, gdy naprawdę doszła.
//!
//! # Po co to istnieje
//!
//! Zgłoszenie właściciela 2026-08-19, w trzech podejściach. Najpierw „jak coś piszę w terminal np
//! siema, to agent nie odpisuje i to się wgl nie wysyła", a po chwili zdanie, które okazało się
//! dokładną diagnozą: **„a może odpisuje on, ale na pewno nie widać moich wiadomości"**.
//!
//! I tak było. Droga do żywej sesji istniała od 2026-08-18 (`say_to_agent_inner`,
//! `engine::drivers::Voice`), ale tura człowieka **nie miała nośnika na drucie**: `Line::Note`
//! jest opisany jako „jedyna proza w widoku" i należy do agenta. Zdanie wpisane w wiersz wejścia
//! szło więc do modelu i nie zostawiało po sobie ANI JEDNEGO śladu — ani na ekranie, ani
//! w `run.json`. Człowiek widział strumień, w którym agent odpowiada na pytanie, którego nie
//! widać, więc wiersz wejścia wyglądał na zepsuty niezależnie od tego, czy działał.
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(say_to_agent_inner(…).await.is_ok())`. To już przechodziło, kiedy właściciel zgłaszał
//! problem trzeci raz — bo `Ok(())` znaczy „nikt nie odmówił", a nie „widać to". Rozstrzyga
//! wyłącznie pytanie o STRUMIEŃ: czy w linach tego biegu stanął wiersz z tym zdaniem.
//!
//! Druga słaba wersja, groźniejsza, bo wygląda na mocniejszą: dopisanie wiersza **przed** wysyłką.
//! Przechodzi to samo porównanie tekstu, a kłamie w pliku — historia twierdziłaby, że agent
//! usłyszał zdanie, które odbiło się o zamkniętą sesję. Dlatego drugi przypadek sprawdza sesję,
//! która przestała słuchać: odmowa **i ani jednej linii**.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `ipc_read_paths` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::commands::RunControl;
use loadout_lib::commands::run::say_to_agent_inner;
use loadout_lib::engine::drivers::ToAgent;
use loadout_lib::engine::line::LineKind;
use loadout_lib::ipc::line_channel;
use tokio::sync::mpsc;

/// Pojemność udawanego głosu kroku.
const ROOM: usize = 4;

/// Pojemność strumienia linii. Z zapasem — mierzymy obecność wiersza, nie przepustowość pompy.
const LINES: usize = 16;

/// Krok, który słucha: wpięty głos plus odbiornik, który musi zostać u wołającego.
///
/// Bez oddania odbiornika kanał ginie razem z funkcją i każda wysyłka odmawiałaby
/// „stopped listening" — czyli test mierzyłby własne sprzątanie.
fn listening(control: &RunControl, step: &str) -> mpsc::Receiver<ToAgent> {
    let (voice, heard) = mpsc::channel(ROOM);
    control.step_can_hear(step, voice);
    heard
}

/// Rodzaj i tekst pierwszego wiersza, który bieg wypuścił — albo zdanie o tym, że nie wypuścił nic.
///
/// Bez `panic!` (jest zabroniony przez `Cargo.toml`): brak wiersza ma się pokazać w komunikacie
/// asercji obok tego, czego się spodziewaliśmy, a nie zniknąć w panice helpera.
fn first_line(source: &mut loadout_lib::ipc::LineSource) -> (Option<LineKind>, String) {
    match source.try_next() {
        Some(line) => (Some(line.kind()), line.text().to_owned()),
        None => (None, "nothing reached the run's stream at all".to_owned()),
    }
}

#[tokio::test]
async fn what_the_person_writes_shows_up_in_the_run() {
    let control = RunControl::new();
    let (sink, mut source) = line_channel(LINES);
    // Tym samym wywołaniem, którym prawdziwy bieg oddaje uchwytowi swój strumień
    // (`run_workflow_with_slots`).
    control.lines_go_to(sink);
    let mut heard = listening(&control, "Builder");

    say_to_agent_inner(&control, None, "siema")
        .await
        .expect("one agent is working, so there is nothing to choose and nothing to refuse");

    // ── (a) DOSZŁO DO AGENTA ────────────────────────────────────────────────────────────────
    //
    // Najpierw to, bo wiersz w historii bez zdania w sesji byłby echem samego siebie: ekran
    // pokazywałby rozmowę, której agent nie słyszał.
    assert!(
        heard.try_recv().is_ok(),
        "the sentence has to reach the agent's own channel; showing it on screen without sending \
         it would be a transcript of a conversation that never happened"
    );

    // ── (b) I WIDAĆ TO W BIEGU ──────────────────────────────────────────────────────────────
    let (kind, text) = first_line(&mut source);
    assert_eq!(
        kind,
        Some(LineKind::Told),
        "the person's turn has to reach the run's stream as its own kind. `note` would sign it \
         with the agent's name, and no line at all is the defect the owner reported three times: \
         \"na pewno nie widac moich wiadomosci\". What arrived was: {text}"
    );
    assert_eq!(
        text, "siema",
        "the line has to carry what the person wrote, word for word — a summary of one's own \
         sentence is not one's own sentence"
    );
}

#[tokio::test]
async fn a_sentence_that_was_refused_leaves_no_trace() {
    let control = RunControl::new();
    let (sink, mut source) = line_channel(LINES);
    control.lines_go_to(sink);
    /* Nadajnik zostaje w rejestrze, odbiornik ginie: dokładnie ten stan, w którym sesja zeszła,
     * a rejestr jeszcze o tym nie wie. Wysyłka musi się o to odbić. */
    drop(listening(&control, "Builder"));

    let refused = say_to_agent_inner(&control, None, "siema").await;
    assert!(
        refused.is_err(),
        "a session that stopped listening has to refuse, not swallow the sentence"
    );

    /* TO JEST CAŁA TREŚĆ TEGO PRZYPADKU. Wiersz dopisany PRZED wysyłką przechodzi porównanie
     * tekstu z pierwszego testu i kłamie w pliku: `run.json` twierdziłby, że agent usłyszał
     * zdanie, które nigdy do niego nie dojechało — a plik jest tym, co zostaje (niezmiennik 4). */
    let (kind, text) = first_line(&mut source);
    assert_eq!(
        kind, None,
        "a refused sentence must not appear in the run's history: the file would then claim the \
         agent heard something it never received. A line arrived anyway, saying: {text}"
    );
}
