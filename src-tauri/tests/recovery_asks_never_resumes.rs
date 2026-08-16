//! AC-4 dla T-20: jedno pytanie na przerwany krok, dwie opcje, żadnej domyślnej.
//!
//! Loadout **nigdy** nie wznawia przerwanego agenta po cichu. Buduje wykrycie, sprzątnięcie
//! i pytanie [T7 §6.3]: błędne auto-wznowienie jest znacznie gorsze niż jedno uczciwe pytanie,
//! a `--resume` na sesji zabitej w połowie tury nie było w ogóle testowane [T7 §11.1].
//!
//! Dwie opcje niosą różne rzeczy i to jest cały kontrakt:
//!
//! * `Pick up where it left off` niesie `session_id` **z wiersza** — ten sam identyfikator,
//!   który dostał proces, bo sesja jest przydzielana **przed** spawnem [T7 §6.2, V]. Gdyby
//!   była wydłubywana z pierwszego zdarzenia `system/init`, istniałby proces, którego sesji
//!   nie umiemy nazwać, i wtedy ta opcja nie miałaby czego nieść.
//! * `Start this step again` niesie `attempt + 1` i **nową** sesję.
//!
//! **Słaba wersja tego kryterium to `assert_eq!(plan.ask.len(), 2)`.** Przechodzi ją
//! implementacja generująca pytanie dla każdego wiersza z `session_id` — akurat tyle ich tu
//! jest. Dlatego porównujemy zbiór `step_id` z oczekiwanym i osobno wymagamy, żeby nowa sesja
//! różniła się od zapisanej: implementacja, która przepisuje ten sam identyfikator, skleiłaby
//! dwie tury w jedną sesję i zgubiła granicę próby.
//!
//! Ostatnia asercja pilnuje rzeczy, której nie widać w żadnej wartości: **w `RecoveryPlan` nie
//! ma pola, które oznaczałoby „wznów samo" albo „ta opcja jest domyślna"**. Brak automatyki ma
//! być własnością typu, a nie ustawieniem, więc sprawdzamy to na kształcie — plan jedzie przez
//! `serde_json` i żaden klucz w całym drzewie nie ma prawa nazywać się `auto…`, `default`,
//! `selected` ani `primary`. To jest ten sam kształt, który zobaczy widok pracy (T-08 / T-09),
//! więc pole dołożone „tymczasowo, dla wygody UI" pada tutaj, a nie po instalacji u kogoś.

use anyhow::{Context, anyhow};
use loadout_lib::recovery::{self, Machine, OptionEffect, Question, RecoveryPlan, RecoveryRow};
use serde_json::Value as Json;

/// Czas startu systemu — zgodny, żeby to kryterium nie mierzyło strażnika z AC-1.
const BOOT: &str = "1786900000";
/// Własna grupa Loadouta.
const OWN_PGID: i32 = 501;

/// Bieg przerwany w połowie.
const RUN: &str = "0199ab00-0000-7000-8000-000000000401";
/// Bieg, którego wszystkie kroki zdążyły się skończyć.
const RUN_FINISHED: &str = "0199ab00-0000-7000-8000-000000000402";

/// Pierwszy przerwany krok — biegł, gdy Loadout zginął.
const STEP_RUNNING: &str = "step-running";
/// Drugi przerwany krok — miał permit, nie zdążył wystartować. To już jego trzecie podejście.
const STEP_READY: &str = "step-ready";

/// Sesja pierwszego przerwanego kroku, przydzielona przed spawnem.
const SESSION_RUNNING: &str = "5f6d1c22-0000-4000-8000-000000000001";
/// Sesja drugiego przerwanego kroku.
const SESSION_READY: &str = "5f6d1c22-0000-4000-8000-000000000002";

/// Liczba prób, które drugi krok ma już za sobą.
const READY_ATTEMPT: i64 = 2;

/// Fragmenty nazw pól, których w tym planie nie wolno znaleźć.
///
/// `auto…` to wznowienie bez pytania. `default`, `select…`, `chosen`, `preferred`,
/// `recommended` i `primary` to opcja wybrana z góry — a pytanie z podpowiedzianą odpowiedzią
/// jest pytaniem tylko z nazwy. Lista jest krótka i celowo dotyczy **nazw pól**, nie treści:
/// nazwa pola jest kontraktem, który przeżyje każdą zmianę tekstu.
const BANNED_KEY_FRAGMENTS: &[&str] = &[
    "auto",
    "default",
    "select",
    "chosen",
    "preferred",
    "recommended",
    "primary",
];

fn row(
    step_id: &str,
    run_id: &str,
    step_status: &str,
    pgid: Option<i32>,
    session_id: Option<&str>,
    attempt: i64,
) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: run_id.to_owned(),
        run_status: "running".to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: Some(BOOT.to_owned()),
        pid: pgid,
        pgid,
        session_id: session_id.map(str::to_owned),
        attempt,
    }
}

/// Pięć wierszy, z czego dwa przerwane.
fn rows() -> Vec<RecoveryRow> {
    vec![
        row(
            STEP_RUNNING,
            RUN,
            "running",
            Some(5001),
            Some(SESSION_RUNNING),
            0,
        ),
        row(
            STEP_READY,
            RUN,
            "ready",
            Some(5002),
            Some(SESSION_READY),
            READY_ATTEMPT,
        ),
        row(
            "step-succeeded",
            RUN,
            "succeeded",
            Some(5003),
            Some("5f6d1c22-0000-4000-8000-000000000003"),
            0,
        ),
        row("step-pending", RUN, "pending", None, None, 0),
        row(
            "step-skipped",
            RUN,
            "skipped",
            None,
            Some("5f6d1c22-0000-4000-8000-000000000005"),
            0,
        ),
    ]
}

/// Bieg, w którym nie ma o co pytać: każdy krok doszedł do stanu końcowego, zanim Loadout zginął.
///
/// Sam bieg zostaje w `running`, więc odzyskiwanie i tak się nim zajmie — inaczej pusta lista
/// pytań byłaby skutkiem pominięcia całego biegu, a nie decyzji o każdym kroku z osobna.
fn finished_rows() -> Vec<RecoveryRow> {
    vec![
        row(
            "done-succeeded",
            RUN_FINISHED,
            "succeeded",
            Some(5101),
            Some("5f6d1c22-0000-4000-8000-000000000011"),
            0,
        ),
        row(
            "done-failed",
            RUN_FINISHED,
            "failed",
            Some(5102),
            Some("5f6d1c22-0000-4000-8000-000000000012"),
            1,
        ),
        row(
            "done-cancelled",
            RUN_FINISHED,
            "cancelled",
            Some(5103),
            Some("5f6d1c22-0000-4000-8000-000000000013"),
            0,
        ),
    ]
}

/// Kroki, o które plan pyta, posortowane.
fn asked_steps(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .ask
        .iter()
        .map(|question| question.step_id.clone())
        .collect();
    ids.sort();
    ids
}

/// Pytanie o dany krok.
fn question_about<'plan>(
    plan: &'plan RecoveryPlan,
    step_id: &str,
) -> anyhow::Result<&'plan Question> {
    plan.ask
        .iter()
        .find(|question| question.step_id == step_id)
        .ok_or_else(|| {
            anyhow!(
                "the plan asks nothing about {step_id}; it asks about {:?}",
                asked_steps(plan)
            )
        })
}

/// Sesja, którą niesie opcja „podejmij tam, gdzie stanęło".
fn pick_up_session(question: &Question) -> anyhow::Result<&str> {
    let option = &question.options[0];
    assert_eq!(
        option.label, "Pick up where it left off",
        "the first option of the question about {} has to be the one that carries on. The two \
         sentences are data fixed by this task; the run view only renders them",
        question.step_id
    );
    match &option.effect {
        OptionEffect::PickUp { session_id } => Ok(session_id.as_str()),
        OptionEffect::StartOver { .. } => Err(anyhow!(
            "the first option of the question about {} starts over instead of carrying on",
            question.step_id
        )),
    }
}

/// Sesja i próba, które niesie opcja „zacznij od nowa".
fn start_over_payload(question: &Question) -> anyhow::Result<(&str, i64)> {
    let option = &question.options[1];
    assert_eq!(
        option.label, "Start this step again",
        "the second option of the question about {} has to be the one that starts over",
        question.step_id
    );
    match &option.effect {
        OptionEffect::StartOver {
            session_id,
            attempt,
        } => Ok((session_id.as_str(), *attempt)),
        OptionEffect::PickUp { session_id } => Err(anyhow!(
            "the second option of the question about {} resumes session {session_id} instead of \
             starting over",
            question.step_id
        )),
    }
}

/// Każdy klucz obiektu w drzewie JSON, razem ze ścieżką, na której stoi.
fn keys_in(value: &Json, path: &str, found: &mut Vec<String>) {
    match value {
        Json::Object(fields) => {
            for (key, child) in fields {
                let here = format!("{path}.{key}");
                keys_in(child, &here, found);
                found.push(here);
            }
        }
        Json::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                keys_in(child, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn every_interrupted_step_gets_one_question_with_two_options_and_no_default() -> anyhow::Result<()>
{
    let machine = Machine {
        boot_id: BOOT.to_owned(),
        own_pgid: OWN_PGID,
    };

    let plan = recovery::decide(&rows(), &machine);

    // ── Dokładnie te dwa kroki, nie „dokładnie dwa" ────────────────────────────────────────
    assert_eq!(
        asked_steps(&plan),
        vec![STEP_READY.to_owned(), STEP_RUNNING.to_owned()],
        "two of these five rows were interrupted and the question list has to name exactly \
         those two. Four of the five carry a session_id, so an implementation that asks about \
         every row that has one produces a list of the wrong content — and one of the right \
         length the moment somebody adds a sixth row"
    );

    // ── Pierwszy krok: sesja z wiersza, próba 0 -> 1 ───────────────────────────────────────
    let running = question_about(&plan, STEP_RUNNING)?;
    assert_eq!(
        pick_up_session(running)?,
        SESSION_RUNNING,
        "carrying on means --resume with the session the process actually had. That id was \
         written down BEFORE the spawn [T7 §6.2, V], which is the only reason it exists at all \
         after a crash"
    );
    let (running_fresh, running_attempt) = start_over_payload(running)?;
    assert_eq!(
        running_attempt, 1,
        "starting over is try 1 for a step that had 0 tries behind it"
    );
    assert_ne!(
        running_fresh, SESSION_RUNNING,
        "starting over needs a NEW session. Reusing the recorded id glues two turns into one \
         session and loses the boundary between the tries — the transcript then shows one \
         conversation that contradicts itself halfway through"
    );

    // ── Drugi krok: ta sama para, inne liczby ──────────────────────────────────────────────
    let ready = question_about(&plan, STEP_READY)?;
    assert_eq!(
        pick_up_session(ready)?,
        SESSION_READY,
        "the ready step never spawned, but its session was assigned before the spawn, so it \
         has one to carry on with"
    );
    let (ready_fresh, ready_attempt) = start_over_payload(ready)?;
    assert_eq!(
        ready_attempt,
        READY_ATTEMPT + 1,
        "the step already had {READY_ATTEMPT} tries behind it, so starting over is the next one"
    );
    assert_ne!(
        ready_fresh, SESSION_READY,
        "starting over needs a NEW session here too"
    );

    // ── Dwie nowe sesje, nie jedna ─────────────────────────────────────────────────────────
    assert_ne!(
        running_fresh, ready_fresh,
        "the two steps got the same fresh session id. Two agents writing into one session is \
         one transcript holding two conversations, and neither of them can be resumed"
    );
    for fresh in [running_fresh, ready_fresh] {
        assert!(
            uuid::Uuid::parse_str(fresh).is_ok(),
            "the fresh session id {fresh:?} is not a uuid. `claude --session-id` takes a \
             caller-supplied UUID and nothing else [T7 §6.2, V], so an id in any other shape is \
             one the step can never actually be started with"
        );
    }

    // ── Bieg bez ani jednego przerwanego kroku ─────────────────────────────────────────────
    let finished = recovery::decide(&finished_rows(), &machine);
    assert!(
        finished.ask.is_empty(),
        "every step of this run reached a final state before the crash, so there is nothing to \
         ask about. A question here is a question about work that is already done: {:?}",
        asked_steps(&finished)
    );
    assert!(
        finished.reap.is_empty(),
        "…and nothing to reap either, though all three rows still carry the pgid they ran \
         under: {:?}",
        finished.reap
    );

    // ── Brak automatyki jest własnością typu ───────────────────────────────────────────────
    // Nie da się tego sprawdzić na wartości, bo chodzi o pole, którego ma NIE być. Da się na
    // kształcie: plan jedzie przez serde tą samą drogą, którą pojedzie do widoku pracy.
    let wire = serde_json::to_value(&plan)
        .context("the plan has to be serialisable, because the run view is where it is answered")?;
    let mut keys = Vec::new();
    keys_in(&wire, "plan", &mut keys);
    assert!(
        !keys.is_empty(),
        "the serialised plan has no fields at all, so the sweep below would pass on an empty \
         object and prove nothing"
    );
    for key in &keys {
        let name = key.rsplit('.').next().unwrap_or(key).to_lowercase();
        for fragment in BANNED_KEY_FRAGMENTS {
            assert!(
                !name.contains(fragment),
                "the plan carries a field named {key}, and {fragment:?} in a field name is \
                 either automatic resume or an answer picked in advance. Loadout never resumes \
                 an interrupted agent by itself and never pre-picks one of the two options \
                 [T7 §6.3, §9.4] — the absence of that field is the property, not a setting \
                 somebody can flip"
            );
        }
    }

    Ok(())
}
