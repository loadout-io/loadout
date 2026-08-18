//! Pisanie do agenta: pięć odmów, każda innym zdaniem, i jedna droga, którą tekst dochodzi.
//!
//! # Po co to istnieje
//!
//! Do 2026-08-18 cała ta polityka mieszkała w `#[tauri::command]` w `ipc.rs` — czterdzieści linii
//! decyzji w skorupie, która ma mieć dwie (niezmiennik 1 i 23). Koszt był jeden i konkretny:
//! `State<'_, AppState>` nie da się zbudować bez żywego Tauri, a `harness/gate.py` słusznie nie
//! uznaje „Failed to launch" za czerwień kodu — więc na ANI JEDNĄ z tych odmów nie dało się
//! napisać kryterium. Zachowanie, którego nikt nie sprawdził, jest zachowaniem, o którym nie
//! wiemy nic; a to jest zachowanie wiersza wejścia, czyli jedynego miejsca, przez które człowiek
//! rozmawia z agentem.
//!
//! # Czego ten plik NIE sądzi
//!
//! Tego, czy tekst dochodzi do prawdziwego modelu — to jest osobna wyrocznia na żywej sesji
//! (`src-tauri/tests/flow_say_to_agent.rs`, `#[ignore]`, bo płaci za `claude`). Tutaj adresatem
//! jest kanał, który sami zakładamy, więc mierzymy WYBÓR ADRESATA i TREŚĆ ODMOWY: dwie rzeczy,
//! które da się rozstrzygnąć bez ani jednego procesu.

// `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii oddawałby
// błąd jako wartość, której nikt nie czyta. Ten sam idiom i ten sam powód, co w `ipc_read_paths`
// i w 117 innych miejscach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::commands::run::say_to_agent_inner;
use loadout_lib::commands::{RunControl, RunError};
use loadout_lib::engine::drivers::ToAgent;
use tokio::sync::mpsc;

/// Pojemność udawanego głosu. Jeden wpis wystarcza — sprawdzamy pierwszą linię, nie przepustowość.
const ROOM: usize = 4;

/// Krok, który słucha, plus odbiornik jego linii.
///
/// Odbiornik wraca do wołającego, bo bez niego kanał ginie razem z funkcją i każda wysyłka
/// odmawiałaby „stopped listening" — czyli test mierzyłby własne sprzątanie.
fn listening(control: &RunControl, step: &str) -> mpsc::Receiver<ToAgent> {
    let (voice, heard) = mpsc::channel(ROOM);
    control.step_can_hear(step, voice);
    heard
}

#[tokio::test]
async fn a_line_reaches_the_one_agent_that_is_working() {
    let control = RunControl::new();
    let mut heard = listening(&control, "Builder");

    say_to_agent_inner(&control, None, "also add a dark mode toggle")
        .await
        .expect("one agent is working, so there is nothing to choose and nothing to refuse");

    // NA KANALE, nie w odpowiedzi komendy: `Ok(())` znaczy tylko „nikt nie odmówił". Pytanie,
    // które ma sens, brzmi „czy to WYSZŁO", i odpowiada na nie wyłącznie odbiornik.
    let got = heard
        .try_recv()
        .expect("the line has to reach the agent's own channel");
    assert_eq!(
        turn(&got),
        "also add a dark mode toggle",
        "the agent has to get what the person wrote, not a summary of it"
    );
}

#[tokio::test]
async fn the_text_is_trimmed_but_never_emptied() {
    let control = RunControl::new();
    let mut heard = listening(&control, "Builder");

    say_to_agent_inner(&control, None, "  ship it  ")
        .await
        .expect("spaces around a sentence are not a reason to refuse it");

    let got = heard.try_recv().expect("the line has to reach the channel");
    assert_eq!(
        turn(&got),
        "ship it",
        "leading and trailing spaces are typing, not content — the agent gets the sentence"
    );
}

#[tokio::test]
async fn an_empty_line_is_refused_and_nothing_is_sent() {
    let control = RunControl::new();
    let mut heard = listening(&control, "Builder");

    let said = refusal(say_to_agent_inner(&control, None, "   ").await);
    assert_eq!(
        said, "Write something first, then press Enter.",
        "Enter on an empty row has to say what to do, not what failed"
    );
    // Odmowa, która najpierw wysyła, a potem odmawia, jest gorsza niż brak odmowy: agent
    // dostaje wtedy pustą turę, za którą ktoś płaci.
    assert!(
        heard.try_recv().is_err(),
        "a refused line must not reach the agent at all"
    );
}

#[tokio::test]
async fn with_nobody_working_the_refusal_says_what_to_press() {
    let control = RunControl::new();

    let said = refusal(say_to_agent_inner(&control, None, "hello?").await);
    assert_eq!(
        said, "No agent is working right now, so there is nobody to talk to. Press Start first.",
        "this is the sentence the person sees most often, so it has to name the next move"
    );
}

#[tokio::test]
async fn with_several_working_the_refusal_names_them() {
    let control = RunControl::new();
    let _builder = listening(&control, "Builder");
    let _checker = listening(&control, "Checker");

    let said = refusal(say_to_agent_inner(&control, None, "ship it").await);
    /* WYMIENIA NAZWY, i to jest cała treść tego kryterium. Wysłanie tekstu do losowego z dwóch
     * pracujących agentów jest kontrolką, która robi coś innego, niż mówi (niezmiennik 16) —
     * a odmowa bez listy zamienia jedno kliknięcie w zgadywanie. */
    assert!(
        said.contains("2 agents are working"),
        "the refusal has to say how many are working; it said: {said}"
    );
    assert!(
        said.contains("Builder"),
        "the refusal has to show a name the person can actually type; it said: {said}"
    );
}

#[tokio::test]
async fn a_named_agent_that_is_not_working_is_told_apart_from_none_at_all() {
    let control = RunControl::new();
    let _builder = listening(&control, "Builder");

    let said = refusal(say_to_agent_inner(&control, Some("Bulider"), "ship it").await);
    /* Dwie różne pomyłki, dwa różne zdania: literówka w nazwie przy pracujących krokach ma
     * pokazać, jakie one są, a krok, który zszedł, ma o tym powiedzieć wprost. Jedno wspólne
     * „could not send" kazałoby człowiekowi zgadywać, którą z tych dwóch rzeczy naprawia. */
    assert!(
        said.contains("Bulider") && said.contains("Builder"),
        "the refusal has to repeat the name that was asked for AND the ones that are working, \
         so the typo is visible; it said: {said}"
    );

    let quiet = RunControl::new();
    let said = refusal(say_to_agent_inner(&quiet, Some("Builder"), "ship it").await);
    assert_eq!(
        said, "That agent already finished, so there is nothing listening any more.",
        "a step that is gone is a different answer than a name nobody has"
    );
}

#[tokio::test]
async fn a_step_that_went_quiet_stops_receiving() {
    let control = RunControl::new();
    let heard = listening(&control, "Builder");
    control.step_went_quiet("Builder");
    drop(heard);

    let said = refusal(say_to_agent_inner(&control, None, "still there?").await);
    assert_eq!(
        said, "No agent is working right now, so there is nobody to talk to. Press Start first.",
        "once a step is unregistered the window must not be offered a conversation with it"
    );
}

#[tokio::test]
async fn a_session_that_stopped_reading_says_so_by_name() {
    let control = RunControl::new();
    // Nadajnik zostaje w rejestrze, odbiornik ginie — to jest dokładnie ten stan, w którym sesja
    // zeszła, a rejestr jeszcze o tym nie wie. Cisza byłaby tu nie do odróżnienia od agenta,
    // który myśli.
    drop(listening(&control, "Builder"));

    let said = refusal(say_to_agent_inner(&control, None, "ship it").await);
    assert_eq!(
        said, "\"Builder\" stopped listening before that could reach it.",
        "the person has to learn which agent went away, by name"
    );
}

/// Zdanie odmowy, albo zdanie o tym, że nic nie odmówiło.
///
/// Czytamy `Display`, bo to **dokładnie** ten napis, który okno pokazuje człowiekowi
/// (`ipc::say_to_agent` robi `error.to_string()`). Sprawdzanie samego wariantu przechodziłoby
/// dla komunikatu, którego nikt nie zrozumie.
fn refusal(result: Result<(), RunError>) -> String {
    match result {
        // Napis, nie panika: każda asercja niżej porównuje zdanie odmowy, więc „nic nie
        // odmówiło" trafia do komunikatu porównania i widać, co przyszło zamiast odmowy.
        Ok(()) => "nothing was refused at all".to_owned(),
        Err(error) => error.to_string(),
    }
}

/// Treść tury, albo zdanie o tym, co przyszło zamiast niej.
///
/// Bez `panic!`: przerwanie w miejscu tury ma się pokazać w komunikacie asercji razem ze zdaniem,
/// którego się spodziewaliśmy — panika w helperze mówi tylko „zła odmiana" i gubi to, po czym
/// poznaje się, w którą stronę poszło.
fn turn(said: &ToAgent) -> &str {
    match said {
        ToAgent::Turn(text) => text,
        ToAgent::Interrupt(_) => "an interrupt, which is not a turn from a person",
    }
}
