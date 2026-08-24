//! AC-3 dla T-100: każda późniejsza runda sędziego widzi wszystkie wcześniejsze próby pracy.
//!
//! Trzecia runda rozróżnia „cała historia" od „tylko ostatnia próba". Pierwsza runda i krok
//! po pętli są kontrolami: ich indeksy nie mają zmienić znaczenia przy dokładaniu historii.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use super::the_tester_gets_an_outcome_field::{Script, run_fixture};

const TRIES: usize = 3;

const LOOP: &str = r#"{
  "format": 1,
  "id": "wf_t100_tester_sees_earlier_tries",
  "name": "A tester with every earlier try",
  "steps": [
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "work: make the change.",
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_tester",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "whenItFails": "carry-on",
      "instructions": "tester: decide whether it is good enough.",
      "at": { "x": 0, "y": 200 }
    },
    {
      "kind": "agent",
      "id": "s_after",
      "name": "After",
      "agent": "01990000-0000-7000-8000-000000000100",
      "overrides": {},
      "instructions": "after: build on what the loop left.",
      "at": { "x": 0, "y": 400 }
    }
  ],
  "links": [
    { "from": "s_work", "to": "s_tester" },
    { "from": "s_tester", "to": "s_after" },
    { "from": "s_tester", "to": "s_work", "max_turns": 3 }
  ]
}"#;

const WORK_1: &str =
    "## Answer\nWork try 1 is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n";
const WORK_2: &str =
    "## Answer\nWork try 2 is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n";
const WORK_3: &str =
    "## Answer\nWork try 3 is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n";
const TESTER_1: &str = "## Answer\nTester try 1 is done.\n\noutcome: fail\n\n## Evidence\nnotes.txt:1\n\n## Open\ntry again.\n";
const TESTER_2: &str = "## Answer\nTester try 2 is done.\n\noutcome: fail\n\n## Evidence\nnotes.txt:1\n\n## Open\ntry again.\n";
const TESTER_3: &str = "## Answer\nTester try 3 is done.\n\noutcome: fail\n\n## Evidence\nnotes.txt:1\n\n## Open\nno tries left.\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_tester_try_has_the_earlier_work_in_order() -> Result<(), Box<dyn Error>> {
    let observed = run_fixture(
        "tester-history",
        LOOP,
        Script::new(&[
            ("work", &[WORK_1, WORK_2, WORK_3]),
            ("tester", &[TESTER_1, TESTER_2, TESTER_3]),
        ]),
    )
    .await?;
    let tester = observed.calls_for("tester");
    assert_eq!(
        tester.len(),
        TRIES,
        "the fixture is not a three-round loop; the driver calls were {:?}",
        observed.calls
    );
    let wrote = who_wrote_what(&observed.report.dir)?;

    // Runda zero zachowuje dokładnie dzisiejszy indeks: tylko bieżąca praca, ze zwykłą
    // etykietą poprzednika. Nie wolno dopisać odnośnika do próby, która jeszcze nie istnieje.
    let first = rows_named(&tester[0].prompt, &wrote);
    assert_eq!(
        signatures(&first),
        vec!["Work try 1"],
        "the first tester try was given anything except the current work: {first:?}"
    );
    assert_eq!(
        first[0].1, "what the step before left",
        "the first-round index changed even though there is no earlier try to add: {first:?}"
    );

    // Runda druga: próba pierwsza i bieżąca druga, potem własny poprzedni werdykt. Kolejność
    // jest numerem kroku, a więc pozycją w pliku i rundą, nigdy chwilą zakończenia procesu.
    let second = rows_named(&tester[1].prompt, &wrote);
    assert_eq!(
        signatures(&second),
        vec!["Work try 1", "Work try 2", "Tester try 1"],
        "the second tester try cannot compare the replacement with the work it replaced, or \
         the index is not in deterministic step-then-round order: {second:?}"
    );

    // Dopiero runda trzecia odróżnia pełną historię od implementacji niosącej jeden poprzedni
    // plik. Obie wcześniejsze prace muszą zostać obok bieżącej i obu wcześniejszych ocen.
    let third = rows_named(&tester[2].prompt, &wrote);
    assert_eq!(
        signatures(&third),
        vec![
            "Work try 1",
            "Work try 2",
            "Work try 3",
            "Tester try 1",
            "Tester try 2"
        ],
        "the last tester try does not see every earlier implementation try in deterministic \
         order: {third:?}"
    );
    for (signature, number) in [("Work try 1", 1), ("Work try 2", 2)] {
        let label = third
            .iter()
            .find(|(who, _)| who == signature)
            .map(|(_, label)| label)
            .ok_or_else(|| format!("the row for {signature:?} disappeared"))?;
        assert!(
            label.contains(&format!("try {number} of {TRIES}")) && !label.contains("your own"),
            "the older implementation {signature:?} is present but its existing try label \
             does not identify the round, or falsely calls it the tester's own answer: {label:?}"
        );
    }

    // Krok poza pętlą ma nadal dostać wyłącznie to, czym pętla naprawdę się skończyła. Pełna
    // historia jest pomocą sędziego, nie zmianą publicznego wyniku pętli.
    let after = observed.calls_for("after");
    assert_eq!(after.len(), 1, "the control step after the loop never ran");
    let outside = rows_named(&after[0].prompt, &wrote);
    assert_eq!(
        signatures(&outside),
        vec!["Work try 3", "Tester try 3"],
        "the step outside the loop was given the tester's private comparison history: \
         {outside:?}"
    );
    assert_eq!(
        outside
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<&str>>(),
        vec![
            "what the step before left",
            "the step before did not pass; this is what it said"
        ],
        "the byte-level meaning of the outside index changed: {outside:?}"
    );
    Ok(())
}

fn signatures(rows: &[(String, String)]) -> Vec<&str> {
    rows.iter().map(|(who, _)| who.as_str()).collect()
}

/// Wiersze indeksu nazwane treścią pliku oraz etykietą relacji z prawdziwego promptu.
fn rows_named(prompt: &str, wrote: &BTreeMap<String, String>) -> Vec<(String, String)> {
    prompt
        .lines()
        .filter(|line| line.contains("handoffs/"))
        .map(|row| {
            let who = wrote
                .iter()
                .find(|(file, _)| row.contains(file.as_str()))
                .map_or_else(
                    || format!("nobody we know wrote {row}"),
                    |(_, who)| who.clone(),
                );
            let label = row
                .rsplit_once(" (")
                .and_then(|(_, tail)| tail.strip_suffix(')'))
                .unwrap_or_default()
                .to_owned();
            (who, label)
        })
        .collect()
}

fn who_wrote_what(dir: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir.join("handoffs")) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let text = fs::read_to_string(entry.path())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        for signature in [
            "Work try 1",
            "Work try 2",
            "Work try 3",
            "Tester try 1",
            "Tester try 2",
            "Tester try 3",
        ] {
            if text.contains(signature) {
                out.insert(name.clone(), signature.to_owned());
                break;
            }
        }
    }
    Ok(out)
}
