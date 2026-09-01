//! Workflow, którego nie da się uruchomić, przestaje dostawać próby — i mówi o tym zdaniem.
//!
//! Odmowa PLANU nie jest odmową źródła: `release_delivery` cofa dostawę do `Pending`, więc
//! następny tick czyta z ledgera tę samą sprawę i wydaje ją drodze Startu jeszcze raz. Workflow
//! skasowany z katalogu albo plik odrzucony przez sprawdzenie przed startem daje dziś serię
//! prób co minutę, bez końca i bez zdania, które człowiek mógłby przeczytać.
//!
//! Trzy, nie jedna: pierwsza odmowa bywa chwilowa (plik właśnie się zapisuje, dysk był pełny
//! przez sekundę). Trzy pod rząd z tym samym plikiem to stan, którego następna minuta nie
//! naprawi — i dopiero on wstrzymuje trigger.
//!
//! Wstrzymanie siedzi w PLIKU triggera, nie w stanie okna: dlatego licznik prób jest częścią
//! zapisanej dostawy, a nie polem procesu. Fetcher jest tu licznikiem bez procesu i bez sieci,
//! dokładnie jak w `trigger_key_refusal_pauses_the_watch.rs`, z którego pochodzi ta ławka.
//!
//! Druga funkcja jest strażnikiem drugiego powodu: wstrzymanie po odrzuconym KLUCZU nadal
//! oddaje pracę zapisaną wcześniej. Kwarantanna, która połknęłaby także tamtą dostawę, cicho
//! zgubiłaby robotę, o której nikt więcej nie przypomni.

#![allow(clippy::expect_used)]

use std::cell::Cell;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use loadout_lib::commands::triggers::{
    self, KEY_REFUSED_SENTENCE, Trigger, TriggerDelivery, TriggerError, TriggerPoll,
};
use loadout_lib::commands::workspaces;
use serde_json::json;
use tempfile::TempDir;

const NOW: i64 = 1_777_777_777_000;
const KEY: &str = "lin_api_1234567890123456789012345678901234567890";
const SLUG: &str = "mine";

/// Ile razy ta sama sprawa ma wyjść na drogę Startu, zanim trigger przestanie ją wydawać.
///
/// Ta liczba stoi też słowem w zdaniu, które czyta człowiek, więc test pilnuje obu naraz.
const TRIES: i64 = 3;

/// Licznik pytań o sprawy nad odpowiedzią, którą Linear naprawdę oddaje.
fn answering<'a>(
    calls: &'a Cell<usize>,
    id: &'a str,
) -> impl FnOnce(&Trigger) -> Result<Vec<u8>, TriggerError> + 'a {
    move |_| {
        calls.set(calls.get() + 1);
        Ok(answer_at(id))
    }
}

#[test]
fn a_workflow_that_never_starts_stops_the_trigger_and_says_so() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let home = bench.home.path();
    let calls = Cell::new(0_usize);
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW, answering(&calls, "old"))?,
        TriggerPoll::Armed,
        "the first read did not arm this trigger, so nothing below would be measured"
    );

    // Trzy próby o tę samą sprawę: pierwsza ją zapisuje, dwie następne wydają ją ponownie, bo
    // droga Startu odmówiła i oddała dostawę do ledgera. Tak wygląda skasowany workflow.
    let saved = three_tries_reach_the_start_path(&bench, &calls)?;

    let held = triggers::poll_with(home, SLUG, NOW + TRIES + 1, answering(&calls, "new"))?;
    let TriggerPoll::Refused { sentence } = held else {
        return Err(format!("a fourth try went out to the Start path: {held:?}").into());
    };
    assert!(
        !sentence.is_empty(),
        "the trigger stopped trying with no English sentence saying so"
    );
    assert_ne!(
        sentence, KEY_REFUSED_SENTENCE,
        "a workflow that never starts borrowed the wording about a refused key"
    );
    assert!(
        sentence.contains("Retry"),
        "the sentence does not name the one way back: {sentence}"
    );

    // Plik jest prawdą (niezmiennik 4): to on, a nie stan procesu, odpowie oknu otwartemu
    // ponownie. Bez tego zapisu „przestał próbować" znaczyłoby tylko „do zamknięcia okna".
    assert!(
        String::from_utf8_lossy(&fs::read(bench.ledger_file())?).contains("paused"),
        "the hold was not written down, so a reopened window would start this run again"
    );
    let reopened = triggers::poll_with(home, SLUG, NOW + TRIES + 2, answering(&calls, "new"))?;
    assert_eq!(
        reopened,
        TriggerPoll::Refused {
            sentence: sentence.clone()
        },
        "a tick after the hold stopped saying that this trigger gave up"
    );
    assert_eq!(
        calls.get(),
        usize::try_from(TRIES)? + 1,
        "a trigger that gave up on its own workflow kept asking Linear for more work"
    );

    one_retry_hands_the_work_back(&bench, &calls, &saved, &sentence)
}

/// Trzy próby pod rząd, wszystkie o dokładnie tej samej zapisanej sprawie.
fn three_tries_reach_the_start_path(
    bench: &Bench,
    calls: &Cell<usize>,
) -> Result<TriggerDelivery, Box<dyn Error>> {
    let mut saved: Option<TriggerDelivery> = None;
    for number in 1..=TRIES {
        let handed = triggers::poll_with(
            bench.home.path(),
            SLUG,
            NOW + number,
            answering(calls, "new"),
        )?;
        let TriggerPoll::Pending { delivery } = handed else {
            return Err(format!("try {number} never reached the Start path: {handed:?}").into());
        };
        if let Some(first) = saved.as_ref() {
            assert_eq!(
                delivery.claim.delivery_id, first.claim.delivery_id,
                "try {number} handed out a different piece of work"
            );
        }
        saved = Some(*delivery);
    }
    saved.ok_or_else(|| "no try ever reached the Start path".into())
}

/// Kryterium 3: praca nie ginie, a drugie poddanie się mówi to samo zdanie, nie zaczyna serii.
fn one_retry_hands_the_work_back(
    bench: &Bench,
    calls: &Cell<usize>,
    saved: &TriggerDelivery,
    sentence: &str,
) -> Result<(), Box<dyn Error>> {
    let home = bench.home.path();
    let back = triggers::resume_with(home, SLUG, NOW + 20, answering(calls, "new"))?;
    let TriggerPoll::Pending { delivery } = back else {
        return Err(format!("Retry did not hand the saved work back: {back:?}").into());
    };
    assert_eq!(
        delivery.claim.delivery_id, saved.claim.delivery_id,
        "Retry invented new work instead of returning the issue that was waiting"
    );
    assert!(
        !String::from_utf8_lossy(&fs::read(bench.ledger_file())?).contains("paused"),
        "Retry left the hold written down, so the next tick would refuse the work it just gave"
    );

    // Ten sam workflow nadal nie startuje: po tym samym budżecie wraca to samo zdanie.
    for number in 2..=TRIES {
        let handed = triggers::poll_with(home, SLUG, NOW + 20 + number, answering(calls, "new"))?;
        assert!(
            matches!(handed, TriggerPoll::Pending { .. }),
            "try {number} after Retry never reached the Start path: {handed:?}"
        );
    }
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW + 20 + TRIES + 1, answering(calls, "new"))?,
        TriggerPoll::Refused {
            sentence: sentence.to_owned()
        },
        "a trigger that failed to start again did not go back on hold with the same sentence"
    );
    Ok(())
}

/// Strażnik: wstrzymanie po odrzuconym kluczu dalej oddaje pracę zapisaną przed odmową.
#[test]
fn a_refused_key_still_hands_out_the_work_it_already_saved() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let home = bench.home.path();
    let calls = Cell::new(0_usize);
    assert_eq!(
        triggers::poll_with(home, SLUG, NOW, answering(&calls, "old"))?,
        TriggerPoll::Armed
    );
    let handed = triggers::poll_with(home, SLUG, NOW + 1, answering(&calls, "new"))?;
    let TriggerPoll::Pending { delivery } = handed else {
        return Err(format!("the saved issue never reached the Start path: {handed:?}").into());
    };

    let refused = triggers::poll_with(home, SLUG, NOW + 2, |_| {
        calls.set(calls.get() + 1);
        Err(TriggerError::Api)
    })?;
    assert_eq!(
        refused,
        TriggerPoll::Pending {
            delivery: delivery.clone()
        },
        "a refused key swallowed the work this trigger had already saved"
    );
    assert!(
        String::from_utf8_lossy(&fs::read(bench.ledger_file())?).contains("paused"),
        "the refused key did not put this trigger on hold"
    );
    Ok(())
}

fn answer_at(id: &str) -> Vec<u8> {
    let hour = if id == "old" { 8 } else { 9 };
    serde_json::to_vec(&json!({"data":{"issues":{"nodes":[{
        "id":id, "identifier":format!("LOAD-{id}"), "title":format!("Issue {id}"),
        "url":format!("https://linear.app/loadout/issue/{id}"), "description":null,
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
