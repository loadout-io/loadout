//! PISANIE DO ŻYWEGO AGENTA, na prawdziwej sesji `claude`: druga tura dochodzi i wraca.
//!
//! # Po co to istnieje
//!
//! Zgłoszenie właściciela 2026-08-18, dwa razy pod rząd: „i pisać z nim nie mogę", potem „dalej
//! nie działa pisanie do agenta przez terminal". Objawem był wiersz wejścia odpowiadający
//! „That one is not known here" na każde zdanie. Przyczyna leżała trzy warstwy niżej i to ona
//! jest tu sądzona: `stdin` sesji był **polem uchwytu**, więc pisanie wymagało `&mut self`,
//! a `commands::run::one_turn` trzyma ten uchwyt pożyczony mutowalnie przez CAŁĄ turę
//! (`handle.wait()` w `tokio::select!`). Okno nie miało jak dosięgnąć sesji, dopóki tura trwa —
//! a po turze `close()` porzuca potok, co JEST końcem sesji. Czyli nigdy.
//!
//! Naprawa: potok przechodzi na własność jednego zadania-pisarza, a uchwyt oddaje **głos**
//! ([`Voice`]) — klonowalny nadajnik bez `&mut`. Ten plik pyta o jedną rzecz i tylko o nią:
//! **czy to, co wyślemy głosem, naprawdę dojdzie do modelu i wróci w jego odpowiedzi.**
//!
//! # Słaba wersja tego kryterium
//!
//! `assert!(handle.voice().is_some())`. Przechodzi dla nadajnika wpiętego w kanał, którego nikt
//! nie czyta — czyli dla dokładnie tej ciszy, którą właściciel zgłosił. Odróżnia je pytanie
//! o SKUTEK: słowo, którego nie ma ani w prompcie pierwszej tury, ani w promptcie systemowym,
//! ma wrócić w prozie agenta. Jeśli wróciło, to znaczy, że model je przeczytał.
//!
//! # Dlaczego `#[ignore]`
//!
//! Uruchamia prawdziwą sesję `claude` i za nią płaci. `checks/full-test.sh` woła
//! `cargo test --tests` bez `--include-ignored`, więc bramka tego nie odpala:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test flow_say_to_agent -- --ignored --nocapture
//! ```

use std::error::Error;
use std::time::Duration;

use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, DecodedEvent, Policy, RunSpec, ToAgent, claude::ClaudeDriver,
};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Słowo, które ma wrócić. Nie ma go w pierwszym promptcie ani w promptcie systemowym, więc
/// jedyną drogą, którą może trafić do odpowiedzi, jest druga tura wysłana głosem.
const PASSWORD: &str = "PINEAPPLE";

/// Ile miejsca w kanale zdarzeń. Z zapasem: pełny kanał zatrzymałby pętlę czytającą agenta,
/// a mierzylibyśmy wtedy przyrząd, nie sesję.
const EVENTS: usize = 256;

/// Ile czekamy na jedną turę.
const PATIENCE: Duration = Duration::from_mins(3);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "uruchamia prawdziwa sesje claude i za nia placi; wolaj z --ignored"]
async fn a_second_turn_sent_by_voice_reaches_the_model() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let driver = ClaudeDriver::new();
    let (events, mut inbox) = mpsc::channel::<DecodedEvent>(EVENTS);

    /* Katalog roboczy: `temp_dir`, bo ta sesja niczego nie pisze. Prompt pierwszej tury każe
     * modelowi odpowiedzieć krótko i CZEKAĆ — dopiero to czyni drugą turę czymś, co da się
     * odróżnić od pierwszej. */
    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        cwd: std::env::temp_dir(),
        prompt: "Reply with just the word READY and nothing else. Then wait for my next message."
            .to_owned(),
        model: None,
        system_append: Some(
            "You answer in one word. Never run tools. Never write files.".to_owned(),
        ),
        // Tylko czytanie: ta sesja nie ma powodu dotykać dysku, a im mniej wolno, tym mniej
        // rzeczy może pójść inaczej niż mierzymy.
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };

    let mut handle = driver.start(spec, events).await?;

    // ── (a) GŁOS ISTNIEJE, i to jest warunek konieczny, nie teza ────────────────────────────
    let voice = handle
        .voice()
        .ok_or("the live session handed out no voice, so nothing could ever be said to it")?;

    // ── (b) PIERWSZA TURA SIĘ KOŃCZY ────────────────────────────────────────────────────────
    tokio::time::timeout(PATIENCE, handle.wait())
        .await
        .map_err(|_| format!("the first turn did not finish within {PATIENCE:?}"))??;

    // ── (c) DRUGA TURA JEDZIE GŁOSEM, nie przez uchwyt ─────────────────────────────────────
    //
    // `voice` był skopiowany PRZED `wait()`, czyli w chwili, w której uchwyt był pożyczony —
    // i to jest cała rzecz, której nie dało się zrobić przed tą naprawą.
    voice
        .send(ToAgent::Turn(format!(
            "Reply with just the word {PASSWORD} and nothing else."
        )))
        .await
        .map_err(|_| "the session stopped listening before the second turn could be sent")?;

    tokio::time::timeout(PATIENCE, handle.wait())
        .await
        .map_err(|_| format!("the second turn did not finish within {PATIENCE:?}"))??;

    // ── (d) SŁOWO WRÓCIŁO W PROZIE MODELU ───────────────────────────────────────────────────
    let mut said = String::new();
    while let Ok(decoded) = inbox.try_recv() {
        if let AgentEvent::Said { text } = &decoded.event {
            said.push_str(text);
            said.push('\n');
        }
    }
    // ── (e) ZAMKNIĘCIE WRACA, CHOĆ TEN TEST DALEJ TRZYMA KLON GŁOSU ─────────────────────────
    //
    // Ta asercja jest tu, bo dokładnie to było zepsute i zmierzone 2026-08-18: pisarz kończył
    // się WYŁĄCZNIE na zamkniętym kanale, więc `close()` czekał, aż zniknie każdy klon głosu.
    // Klon trzyma produkcja przez całą turę (`RunControl.voices`, po to, żeby dało się napisać
    // do pracującego agenta) i trzyma go ten test — skutkiem był `close()`, który nie wracał
    // NIGDY: 15 minut przy dwóch turach po trzy sekundy, sesja zeszła z sygnału. W biegu znaczy
    // to krok, który skończył pracę i nigdy się nie kończy.
    //
    // `voice` musi tu jeszcze ŻYĆ, i to jest cała treść tego kryterium; dlatego jest porzucony
    // dopiero pod asercją.
    let closed = tokio::time::timeout(Duration::from_secs(30), handle.close()).await;
    drop(voice);
    assert!(
        closed.is_ok(),
        "closing the session has to come back even while somebody else still holds a voice to \
         it. It did not, which means the writer is again waiting for the last sender to vanish — \
         and a step that finished its work never finishes."
    );

    assert!(
        said.contains(PASSWORD),
        "the word sent by voice has to come back in what the model said — that is the only proof \
         it was read. It is nowhere in the {} bytes the agent wrote:\n{said}",
        said.len()
    );
    Ok(())
}
