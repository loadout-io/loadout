//! ROZMOWA Z AGENTEM WIODĄCYM na prawdziwej sesji `claude`: odpowiedź wraca i sesja pamięta.
//!
//! # Po co to istnieje
//!
//! Rozstrzygnięcie właściciela 2026-08-19: „tak buduj, i nie, tylko komendy determinują akcje
//! workflow". Rozmowa ma istnieć i ma być rozmową — a nie jedną turą, po której model zapomina,
//! o czym mówiliście. Kryteria na dublerze (`tests/it/chat_never_starts_a_run.rs`) dowodzą, że
//! **kod** trzyma jedną sesję; ten plik pyta, czy MODEL po drugiej stronie naprawdę ją kontynuuje.
//!
//! # Słaba wersja tego kryterium
//!
//! „Coś wróciło w strumieniu". Przechodzi dla implementacji, która na każde zdanie odpala świeży
//! proces: odpowiedź jest, kontekstu nie ma, a różnicy nie widać, dopóki nie zapytasz o coś
//! z pierwszej tury. Rozstrzyga więc SŁOWO, które padło tylko w pierwszym zdaniu i o które
//! pytamy w drugim: jeśli wróciło, sesja jest jedna.
//!
//! # Dlaczego `#[ignore]`
//!
//! Uruchamia prawdziwą sesję `claude` i za nią płaci. `checks/full-test.sh` woła `cargo test
//! --tests` bez `--include-ignored`, więc bramka tego nie odpala:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test flow_lead_agent_chat -- --ignored --nocapture
//! ```

use std::error::Error;
use std::time::Duration;

use loadout_lib::commands::chat::Chat;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::line::LineKind;
use loadout_lib::ipc::{LineSource, line_channel};

/// Słowo, które padnie **tylko** w pierwszej turze. Jedyną drogą, którą może wrócić w odpowiedzi
/// na drugie pytanie, jest sesja pamiętająca pierwsze.
const PASSWORD: &str = "PINEAPPLE";

/// Ile miejsca w strumieniu linii. Z zapasem: rozmowa z narzędziami sypie wierszami.
const LINES: usize = 512;

/// Ile czekamy na jedną odpowiedź.
const PATIENCE: Duration = Duration::from_mins(3);

/// Wszystko, co agent wiodący powiedział prozą, sklejone.
fn prose(source: &mut LineSource) -> String {
    let mut said = String::new();
    while let Some(line) = source.try_next() {
        if line.kind() == LineKind::Note {
            said.push_str(line.text());
            said.push('\n');
        }
    }
    said
}

/// Czeka, aż w strumieniu pojawi się proza — albo aż skończy się cierpliwość.
///
/// Pętla z sufitem, nie `sleep` na sztywno: mierzymy zdarzenie, a nie zegar, więc test nie ocenia
/// planisty systemu operacyjnego. Zbieramy PO DRODZE, bo `try_next` zdejmuje wiersze z kolejki.
async fn wait_for_prose(source: &mut LineSource, into: &mut String) -> bool {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while tokio::time::Instant::now() < deadline {
        into.push_str(&prose(source));
        if !into.trim().is_empty() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "uruchamia prawdziwa sesje claude i za nia placi; wolaj z --ignored"]
async fn the_lead_agent_answers_and_remembers() -> Result<(), Box<dyn Error + Send + Sync>> {
    let driver = ClaudeDriver::new();
    let (sink, mut source) = line_channel(LINES);
    let mut chat = Chat::new(sink);
    /* Katalog roboczy: `temp_dir`. Rozmowa ma prawo pisać w swoim folderze (`Policy::EditInFolder`),
     * a my nie chcemy, żeby napisała cokolwiek w repo. */
    let here = std::env::temp_dir();

    // ── (a) PIERWSZE ZDANIE ZAKŁADA SESJĘ I WRACA ODPOWIEDZIĄ ────────────────────────────────
    chat.say(
        &driver,
        here.clone(),
        &format!("Remember the word {PASSWORD}. Reply with just the word OK and nothing else."),
    )
    .await?;
    assert!(chat.is_live(), "the first sentence has to open the session");

    let mut first = String::new();
    assert!(
        wait_for_prose(&mut source, &mut first).await,
        "the lead agent said nothing within {PATIENCE:?}. A conversation where only one side \
         appears is the defect this whole feature exists to remove."
    );

    // ── (b) DRUGA TURA IDZIE DO TEJ SAMEJ SESJI ─────────────────────────────────────────────
    //
    // I to jest cała treść tego pliku. Implementacja odpalająca świeży proces na zdanie odpowie
    // tu cokolwiek, ale nie tym słowem — bo go nie słyszała.
    chat.say(
        &driver,
        here,
        "What word did I ask you to remember? Reply with just that word.",
    )
    .await?;

    let mut second = String::new();
    assert!(
        wait_for_prose(&mut source, &mut second).await,
        "the second turn produced no prose within {PATIENCE:?}"
    );

    // ── (c) ZAMKNIĘCIE WRACA ────────────────────────────────────────────────────────────────
    //
    // Zanim asercja o treści, bo proces ma zejść niezależnie od tego, co model odpowiedział —
    // rozmowa porzucona żywa przechodzi pod PID 1 i pracuje dalej (`recovery.rs`, nagłówek).
    let closed = tokio::time::timeout(Duration::from_secs(30), chat.close()).await;
    assert!(
        closed.is_ok(),
        "closing the conversation has to come back; a session left alive survives the window and \
         keeps spending money nobody is watching"
    );

    assert!(
        second.contains(PASSWORD),
        "the word from the FIRST turn has to come back in the answer to the second — that is the \
         only proof the session is one conversation rather than a process per sentence. The second \
         answer was {} bytes:\n{second}\n\nThe first answer had been:\n{first}",
        second.len()
    );
    Ok(())
}
