//! Odrzucony klucz wstrzymuje pukanie na trwałe; chwilowa awaria nie wstrzymuje niczego.
//!
//! Rozróżnienie należy do TYPU błędu, nie do treści zdania: `TriggerError::Api` powstaje, gdy
//! Linear odpowiedział `errors`, czyli odrzucił dokładnie ten klucz i odrzuci go przy każdym
//! następnym ticku. `Start`, `CurlFailed` i `EmptyAnswer` mówią wyłącznie o tej jednej próbie.
//!
//! Stan wstrzymania jest czytany z pliku triggera, więc ten sam plik odpowiada oknu otwartemu
//! ponownie. Fetcher jest tu licznikiem bez procesu i bez sieci — dokładnie tak, jak w
//! `trigger_busy_does_not_poll.rs`, z którego pochodzi ta fikstura.
//!
//! Druga funkcja jest strażnikiem: przechodzi także przed poprawką i ma przechodzić po niej.
//! Kwarantanna, która połknęłaby timeout, cicho zatrzymałaby watcher na jednej złej minucie.

#![allow(clippy::expect_used)]

use std::cell::Cell;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;

use loadout_lib::commands::triggers::{self, Trigger, TriggerError, TriggerPoll};
use loadout_lib::commands::workspaces;
use serde_json::json;
use tempfile::TempDir;

const NOW: i64 = 1_777_777_777_000;
const KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const SLUG: &str = "mine";

/// Fetcher, którego Linear odrzuca deterministycznie, liczący każde pytanie o sprawy.
fn refusing(calls: &Cell<usize>) -> impl FnOnce(&Trigger) -> Result<Vec<u8>, TriggerError> + '_ {
    move |_| {
        calls.set(calls.get() + 1);
        Err(TriggerError::Api)
    }
}

/// Ten sam licznik nad odpowiedzią, którą Linear naprawdę oddaje.
fn answering<'a>(
    calls: &'a Cell<usize>,
    id: &'a str,
    identifier: &'a str,
    hour: u8,
) -> impl FnOnce(&Trigger) -> Result<Vec<u8>, TriggerError> + 'a {
    move |_| {
        calls.set(calls.get() + 1);
        Ok(answer_at(id, identifier, hour))
    }
}

#[test]
fn a_refused_key_stops_the_knocking_until_one_retry() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let home = bench.home.path();
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW, |_| Ok(answer()))?,
        TriggerPoll::Armed,
        "the first read did not arm this trigger, so nothing below would be measured"
    );

    let calls = Cell::new(0_usize);
    let paused = triggers::poll_with(home, SLUG, NOW + 1, refusing(&calls)).map_err(|said| {
        format!("a refused key ended the tick with an error, not a hold: {said}")
    })?;
    let TriggerPoll::Refused { sentence } = paused else {
        return Err(format!("a refused key did not put this trigger on hold: {paused:?}").into());
    };
    assert_eq!(calls.get(), 1, "the refused tick did not ask Linear once");
    assert!(
        !sentence.is_empty(),
        "the trigger is on hold with no English sentence saying so"
    );

    // Plik jest prawdą (niezmiennik 4): to on, a nie stan procesu, odpowie oknu otwartemu
    // ponownie. Bez tego zdania „przestaje pukać" znaczyłoby tylko „do zamknięcia okna".
    assert!(
        String::from_utf8_lossy(&fs::read(bench.ledger_file())?).contains("paused"),
        "the hold was not written down, so a reopened window would start asking Linear again"
    );

    // Kryterium 1: kolejne tiki nie pukają. Stan siedzi w pliku triggera, więc ta sama
    // odpowiedź czeka też na okno otwarte ponownie.
    for at in [NOW + 2, NOW + 3, NOW + 4] {
        assert_eq!(
            triggers::poll_with(home, SLUG, at, refusing(&calls))?,
            TriggerPoll::Refused {
                sentence: sentence.clone()
            },
            "a tick after the refusal stopped saying that this trigger is on hold"
        );
        assert_eq!(
            calls.get(),
            1,
            "a tick asked Linear again with the key it had already refused"
        );
    }

    // Kryterium 3: Retry puka DOKŁADNIE raz i wraca do wstrzymania, gdy klucz jest ten sam.
    assert_eq!(
        triggers::resume_with(home, SLUG, NOW + 5, refusing(&calls))?,
        TriggerPoll::Refused {
            sentence: sentence.clone()
        },
        "the trigger did not go back on hold after Retry met the same refused key"
    );
    assert_eq!(calls.get(), 2, "Retry did not ask Linear exactly once");
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW + 6, refusing(&calls))?,
        TriggerPoll::Refused {
            sentence: sentence.clone()
        },
    );
    assert_eq!(
        calls.get(),
        2,
        "the tick after a failed Retry started asking Linear again"
    );

    // Drugie kliknięcie bez zmiany klucza: znowu najwyżej jedno pytanie, nigdy seria.
    triggers::resume_with(home, SLUG, NOW + 7, refusing(&calls))?;
    assert_eq!(calls.get(), 3, "the second Retry asked more than once");
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW + 8, refusing(&calls))?,
        TriggerPoll::Refused { sentence },
    );
    assert_eq!(
        calls.get(),
        3,
        "the second Retry left this trigger asking Linear on every tick"
    );

    a_repaired_key_gets_the_rhythm_back(&bench, &calls)
}

/// Ostatnia część kryterium 3: po kliknięciu z naprawionym kluczem trigger pyta jak zwykle.
fn a_repaired_key_gets_the_rhythm_back(
    bench: &Bench,
    calls: &Cell<usize>,
) -> Result<(), Box<dyn Error>> {
    let home = bench.home.path();
    let healed = triggers::resume_with(
        home,
        SLUG,
        NOW + 9,
        answering(calls, "issue-b", "LOAD-2", 9),
    )?;
    assert!(
        matches!(healed, TriggerPoll::Pending { ref delivery } if delivery.issue.id == "issue-b"),
        "Retry with a repaired key did not deliver the waiting issue: {healed:?}"
    );
    assert_eq!(
        calls.get(),
        4,
        "Retry with a repaired key did not ask Linear"
    );
    let back = triggers::poll_with(
        home,
        SLUG,
        NOW + 10,
        answering(calls, "issue-c", "LOAD-3", 10),
    )?;
    assert!(
        !String::from_utf8_lossy(&fs::read(bench.ledger_file())?).contains("paused"),
        "the repaired trigger kept the hold written down, so a reopened window would stay stuck"
    );
    assert!(
        !matches!(back, TriggerPoll::Refused { .. }),
        "a repaired trigger was still on hold on its next tick: {back:?}"
    );
    assert_eq!(
        calls.get(),
        5,
        "a repaired trigger stopped asking Linear on its next tick"
    );
    Ok(())
}

#[test]
fn a_broken_connection_keeps_knocking() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let home = bench.home.path();
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW, |_| Ok(answer()))?,
        TriggerPoll::Armed
    );

    let calls = Cell::new(0_usize);
    for (at, broken) in [
        (
            NOW + 1,
            TriggerError::Start(io::Error::other("curl is gone")),
        ),
        (
            NOW + 2,
            TriggerError::CurlFailed("exit status: 7".to_owned()),
        ),
        (NOW + 3, TriggerError::EmptyAnswer),
    ] {
        let said = triggers::poll_with(home, SLUG, at, |_| {
            calls.set(calls.get() + 1);
            Err(broken)
        });
        assert!(
            said.is_err(),
            "a connection that failed once was answered as a lasting refusal: {said:?}"
        );
    }
    assert_eq!(
        calls.get(),
        3,
        "one failed connection stopped this trigger from asking Linear again"
    );

    let back = triggers::poll_with(
        home,
        SLUG,
        NOW + 4,
        answering(&calls, "issue-b", "LOAD-2", 9),
    )?;
    assert!(
        matches!(back, TriggerPoll::Pending { ref delivery } if delivery.issue.id == "issue-b"),
        "the tick after a failed connection did not ask Linear normally: {back:?}"
    );
    assert_eq!(calls.get(), 4, "the next tick never reached Linear");
    Ok(())
}

fn answer() -> Vec<u8> {
    answer_at("old", "LOAD-1", 8)
}

fn answer_at(id: &str, identifier: &str, hour: u8) -> Vec<u8> {
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":id, "identifier":identifier, "title":format!("Issue {identifier}"),
        "url":format!("https://linear.app/loadout/issue/{identifier}"), "description":null,
        "updatedAt":format!("2026-08-21T{hour:02}:00:00.000Z")
    }]}}}))
    .expect("answer JSON")
}

#[derive(Debug)]
struct Bench {
    home: TempDir,
    #[expect(
        dead_code,
        reason = "the workspace folder must outlive every poll in the test"
    )]
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join(triggers::TRIGGERS_DIR))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        let workspace = project
            .path()
            .to_str()
            .ok_or("test workspace is not UTF-8")?;
        workspaces::save_workspace_inner(home.path(), "Trigger tests", workspace)?;
        fs::write(
            home.path().join(triggers::TRIGGERS_DIR).join("mine.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": 1, "source": "linear", "enabled": true,
                "workflow": "ship-it", "workspace": workspace,
                "condition": "assigned-to-me", "api_key": KEY
            }))?,
        )?;
        Ok(Self { home, project })
    }

    /// Ta sama ukryta nazwa, którą pisze `write_ledger` i sprząta cleanup Delete.
    fn ledger_file(&self) -> PathBuf {
        self.home
            .path()
            .join(triggers::TRIGGERS_DIR)
            .join(format!(".{SLUG}.ledger.json"))
    }
}
