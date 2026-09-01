//! Zrzut diagnostyczny odróżnia krok, który NIE BYŁ POTRZEBNY, od kroku, który nie ruszył.
//!
//! ZMIERZONE NA PRAWDZIWYM BIEGU WŁAŚCICIELA, 2026-08-30. Bieg `20260829-204729` skończył się
//! sukcesem: trzynaście kroków, wszystkie `succeeded`. Cztery z nich nie miały ani czasu startu,
//! ani kodu wyjścia, ani jednego pliku w `logs/` — bo `Combine` i `QA` są krokami pętli, próba
//! zerowa przeszła i próby 1 i 2 były zbędne. `run.json` mówi to WPROST:
//!
//!     node_key = "s_7#1", status = "succeeded", executed = false, process_started = false,
//!     summary  = "Not needed: the work already passed in an earlier try."
//!
//! Zrzut diagnostyczny gubił po drodze `executed` i `process_started`. Czytający widział więc
//! cztery kroki meldujące sukces bez śladu wykonania — czyli DOKŁADNIE tę klasę wady, dla której
//! to repo powstało (niezmiennik 19: kod wyjścia to nie dowód). Kosztowało to jedną błędną
//! diagnozę, zanim ktokolwiek zszedł do `run.json`.
//!
//! `ExecutionFacts` z T-207 istniały już wtedy i stały w pliku obok siebie. Brakowało wyłącznie
//! dwóch pól w kształcie receipt.
//!
//! CZEGO TEN ZESTAW PILNUJE Z DRUGIEJ STRONY, i to jest połowa jego treści: zdanie `summary`
//! pisze AGENT, więc nie ma prawa wejść do zrzutu, który człowiek wkleja obcym. Raport wsparcia
//! jest budowany z zamkniętej listy dozwolonych pól, nigdy przez redagowanie prywatnych bajtów
//! (T-34 AC-3, `support_report_excludes_private_content.rs`). Dwa boole niosą całe rozróżnienie
//! i zero treści — i tak ma zostać.
//!
//! SŁABA WERSJA: test wyłącznie na tym, że `executed` jest w zrzucie. Przechodzi dla
//! implementacji wpisującej `true` zawsze, a taka mówi o kroku, który nie ruszył, że się wykonał.
//! Dlatego oba stany są sądzone w jednej scenie, na dwóch krokach o tym samym `status`.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::diagnostics::support_report;
use serde_json::{Value, json};

const RUN: &str = "20260830-000000__run-loop-tries";
const RAN: &str = "step-that-ran";
const NOT_NEEDED: &str = "step-that-was-not-needed";
const AGENT_SENTENCE: &str = "Not needed: the work already passed in an earlier try.";

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

/// Bieg pętli: jedna próba poszła i przeszła, druga była zbędna. Oba kroki są `succeeded`.
fn seed(active: &Path) -> Result<(), Box<dyn Error>> {
    let run = active.join(".loadout/runs").join(RUN);
    write(
        &run.join("run.json"),
        serde_json::to_vec_pretty(&json!({
            "id": RUN,
            "workflow_id": "wf-loop",
            "workflow_snapshot": {"format": 1, "id": "wf-loop", "steps": [], "links": []},
            "status": "succeeded",
            "concurrency": 2,
            "created_at": 1_788_036_449_225_i64,
            "started_at": 1_788_036_452_038_i64,
            "ended_at": 1_788_043_286_233_i64,
            "steps": [
                {
                    "id": RAN, "node_key": "s_7#0", "name": "Combine", "agent": "codex",
                    "depends_on": [], "status": "succeeded", "attempt": 1,
                    "executed": true, "process_started": true, "death_proof": true,
                    "exit_code": 0, "turns": 1,
                    "started_at": 1_788_041_055_571_i64, "ended_at": 1_788_042_309_250_i64
                },
                {
                    "id": NOT_NEEDED, "node_key": "s_7#1", "name": "Combine", "agent": "codex",
                    "depends_on": [], "status": "succeeded", "attempt": 0,
                    "executed": false, "process_started": false, "death_proof": false,
                    "exit_code": null, "turns": null,
                    "started_at": null, "ended_at": null,
                    "summary": AGENT_SENTENCE
                }
            ]
        }))?,
    )?;
    write(
        &run.join(format!("logs/agent-{RAN}.jsonl")),
        b"{\"kind\":\"line\"}",
    )?;
    Ok(())
}

fn step_of<'a>(document: &'a Value, id: &str) -> Option<&'a Value> {
    document.get("runs")?.as_array()?.iter().find_map(|run| {
        run.get("steps")?
            .as_array()?
            .iter()
            .find(|step| step.get("id").and_then(Value::as_str) == Some(id))
    })
}

#[test]
fn the_receipt_says_which_succeeded_step_actually_ran() -> Result<(), Box<dyn Error>> {
    let active = tempfile::tempdir()?;
    seed(active.path())?;

    let report = support_report(active.path())?;
    let document: Value = serde_json::from_str(report.text())?;

    let ran = step_of(&document, RAN).ok_or("the step that ran left the receipt")?;
    let idle = step_of(&document, NOT_NEEDED).ok_or("the step that was not needed left it")?;

    // Kontrola przeciw pustej asercji: oba kroki mówią o sobie to samo, więc `state` ich nie
    // rozróżnia i całe pytanie zależy od pól niżej.
    assert_eq!(
        ran.get("state").and_then(Value::as_str),
        Some("succeeded"),
        "the scene needs both steps to carry the same state, or it proves nothing"
    );
    assert_eq!(
        idle.get("state").and_then(Value::as_str),
        Some("succeeded"),
        "the scene needs both steps to carry the same state, or it proves nothing"
    );

    assert_eq!(
        ran.get("executed").and_then(Value::as_bool),
        Some(true),
        "a step with a transcript, an exit code and a death proof did run, and the receipt has \
         to say so"
    );
    assert_eq!(
        idle.get("executed").and_then(Value::as_bool),
        Some(false),
        "a try the loop never needed did NOT run. Without this field a reader sees a step \
         reporting success with no evidence of execution — the very shape this repo exists to \
         catch (invariant 19)"
    );
    assert_eq!(
        idle.get("processStarted").and_then(Value::as_bool),
        Some(false),
        "no process was ever started for a try that was not needed"
    );
    Ok(())
}

#[test]
fn the_receipt_still_carries_none_of_the_agent_words() -> Result<(), Box<dyn Error>> {
    let active = tempfile::tempdir()?;
    seed(active.path())?;

    let report = support_report(active.path())?;
    let text = report.text();

    // Kontrola przeciw pustej asercji: zdanie NAPRAWDĘ stoi w pliku, z którego czytamy.
    let run = active
        .path()
        .join(".loadout/runs")
        .join(RUN)
        .join("run.json");
    assert!(
        fs::read_to_string(&run)?.contains(AGENT_SENTENCE),
        "the scene has to seed the sentence, or the absence below is the absence of nothing"
    );
    assert!(
        !text.contains(AGENT_SENTENCE),
        "the reason a try was skipped is written by an agent, and this receipt is pasted to \
         strangers. Two booleans carry the whole distinction and no words (T-34 AC-3)"
    );
    Ok(())
}
