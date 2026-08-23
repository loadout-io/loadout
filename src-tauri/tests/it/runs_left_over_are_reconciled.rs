//! Bieg, który zginął razem z aplikacją, przestaje kłamać przy otwarciu folderu.
//!
//! # Co było zepsute
//!
//! `run.json` biegu ubitego razem z oknem zostawał w `running` **na zawsze**. Zmierzone
//! u właściciela 2026-08-23: trzy takie biegi naraz, siedem grup procesów dawno martwych,
//! a historia pokazywała je jako pracę w toku.
//!
//! Odzyskiwanie ISTNIAŁO i nie miało jak ich zobaczyć, z dwóch niezależnych powodów: czytało
//! bazę BIBLIOTEKI (a biegi folderu mają własny indeks i własne pliki), a wynik zapisywało
//! WYŁĄCZNIE do bazy — podczas gdy historia i diagnostyka czytają `run.json`.
//!
//! # Słaba wersja tego kryterium
//!
//! „Po uzgodnieniu bieg nie stoi w `running`". Przechodzi ją funkcja, która przepisuje KAŻDY
//! bieg w folderze — a wtedy skończony bieg dostaje status przerwanego i człowiek traci historię
//! tego, co naprawdę się udało. Rozróżnia je drugi bieg w tej samej fikstrze: zamknięty,
//! porównywany BAJT W BAJT przed i po.
//!
//! Trzeci punkt pilnuje rzeczy, której nie widać, dopóki nie zaboli: `run.json` niesie migawkę
//! grafu i klucze, których ta wersja może nie znać. Uzgodnienie przepisujące plik przez typ tej
//! wersji skasowałoby wszystko, czego typ nie ma.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::reconcile::with_reaper;
use loadout_lib::engine::supervisor::machine_booted_at;
use loadout_lib::recovery::ReapOutcome;
use serde_json::Value;

/// Grupa procesów, o którą fikstura każe zapytać. Nikt jej nie zabija — domykacz jest podstawiony.
const DEAD_GROUP: i32 = 33559;

/// Klucz, którego ta wersja nie zna. Ma przeżyć zapis.
const STRANGER: &str = "something-a-newer-build-wrote";

fn a_run(status: &str, step_status: &str, boot: &str) -> String {
    format!(
        r#"{{
  "id": "01a02c22-346e-73c2-9555-83670e3f93e3",
  "workflow_id": "deep-research.json",
  "workflow_hash": "abc",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Deep research",
  "status": "{status}",
  "concurrency": 3,
  "created_at": 1787446834286,
  "boot_id": "{boot}",
  "started_at": 1787446837880,
  "ended_at": null,
  "error": null,
  "{STRANGER}": {{ "kept": true }},
  "steps": [
    {{
      "id": "01a02c22-3474-74f1-b850-611803ce3144",
      "node_key": "s_1",
      "name": "Plan steps",
      "agent": "codex",
      "kind": "agent",
      "depends_on": [],
      "status": "{step_status}",
      "attempt": 0,
      "agent_session_id": "01a02c22-3474-74f1-b850-611803ce3144",
      "pid": {DEAD_GROUP},
      "pgid": {DEAD_GROUP},
      "started_at": 1787446837880,
      "ended_at": null,
      "error": null
    }}
  ]
}}
"#
    )
}

/// Bieg, który skończył się normalnie. Ani jeden jego bajt nie ma prawa się zmienić.
const FINISHED: &str = r#"{
  "id": "01a02c25-065d-7050-b8b0-3eed4e1ef2b5",
  "status": "succeeded",
  "ended_at": 1787447357700,
  "steps": [
    { "id": "s_only", "name": "Did it", "status": "succeeded", "ended_at": 1787447357700 }
  ]
}
"#;

fn put(project: &Path, folder: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".loadout").join("runs").join(folder);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("run.json"), text)?;
    Ok(())
}

fn read(project: &Path, folder: &str) -> Result<Value, Box<dyn Error>> {
    let text = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(folder)
            .join("run.json"),
    )?;
    Ok(serde_json::from_str(&text)?)
}

const LEFT_OVER: &str = "20260823-010034__01a02c22-346e-73c2-9555-83670e3f93e3";
const CLOSED: &str = "20260823-010339__01a02c25-065d-7050-b8b0-3eed4e1ef2b5";

#[test]
fn a_run_left_running_by_a_closed_window_is_written_off() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path();
    // TEN SAM rozruch maszyny, co teraz: tylko wtedy zapisany `pgid` opisuje cokolwiek
    // prawdziwego, a strażnik z `recovery::decide` w ogóle wypuszcza strzał.
    let boot = machine_booted_at().ok_or("this machine does not say when it booted")?;
    put(project, LEFT_OVER, &a_run("running", "running", &boot))?;
    put(project, CLOSED, FINISHED)?;
    let before = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(CLOSED)
            .join("run.json"),
    )?;

    let mut asked: Vec<i32> = Vec::new();
    let done = with_reaper(project, &mut |pgid| {
        asked.push(pgid);
        ReapOutcome::ProvenDead
    });

    assert_eq!(
        asked,
        vec![DEAD_GROUP],
        "the group of the step that was left running has to be asked about exactly once. Asking \
         about nothing leaves an orphan burning the provider's limit; asking about somebody \
         else's number is a signal sent to an innocent process"
    );
    assert_eq!(
        done.runs, 1,
        "exactly one run was left over; it said {done:?}"
    );

    let repaired = read(project, LEFT_OVER)?;
    assert_eq!(
        repaired["status"].as_str(),
        Some("interrupted"),
        "the run still reads as running, so the history keeps showing work in progress that \
         nobody is doing. It said: {:?}",
        repaired["status"]
    );
    assert_eq!(
        repaired["steps"][0]["status"].as_str(),
        Some("failed"),
        "the step inside it still reads as running"
    );
    assert!(
        !repaired["steps"][0]["error"].is_null(),
        "the step was cut off and says nothing about it - that is the empty red row the owner \
         spent a day looking at"
    );
    assert!(
        !repaired["ended_at"].is_null(),
        "a run that is over has to say when. Without it the history cannot even sort it"
    );

    /* TRZECI PUNKT: klucz, ktorego ta wersja nie zna, przezyl zapis. `run.json` niesie migawke
     * grafu i pola dolozone przez nowszy build; przepisanie pliku przez typ TEJ wersji skasowaloby
     * wszystko, czego typ nie ma — i nie zostawiloby po tym ani jednego komunikatu. */
    assert_eq!(
        repaired[STRANGER]["kept"].as_bool(),
        Some(true),
        "repairing the run threw away a key this build does not know. A newer build wrote it, \
         and one open in an older Loadout would silently eat it"
    );

    /* I CZWARTY, ktory odroznia to kryterium od slabej wersji: bieg zamkniety jest nietkniety
     * BAJT W BAJT. Uzgodnienie przepisujace kazdy plik zamienia historie tego, co sie udalo,
     * w historie przerwan. */
    let after = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(CLOSED)
            .join("run.json"),
    )?;
    assert_eq!(
        after, before,
        "a run that finished on its own was rewritten too. Reconciling is for runs nobody is \
         carrying any more - not for every file in the folder"
    );
    Ok(())
}
