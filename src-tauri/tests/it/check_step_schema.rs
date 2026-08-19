//! AC-1 dla T-55: schemat przyjmuje krok „sprawdź", ODMAWIA zapisu kroku bez dowodu i nie łamie
//! plików zapisanych, zanim ten rodzaj kroku istniał.
//!
//! # SŁABA WERSJA numer jeden
//!
//! `assert!(!check(&workflow).is_empty())` po skasowaniu wzorca dowodu. Przechodzi dla
//! implementacji, która oddaje `Level::Warning` — a ostrzeżenie **nie blokuje `save()`**
//! (`file::save` odrzuca wyłącznie na pierwszym `Level::Problem`), więc plik, który miał być
//! odrzucony, ląduje na dysku i biegnie. Rozróżniają to dwie asercje i obie są niżej: porównanie
//! `note.level` z `Level::Problem` **oraz** odczyt bajtów pliku po odmowie.
//!
//! # SŁABA WERSJA numer dwa
//!
//! Sam obieg tam i z powrotem. Przechodzi dla builda, który podniósł `format` na `2` „bo doszedł
//! rodzaj kroku" — czyli dla takiego, w którym każdy workflow zapisany wczoraj przestaje się
//! otwierać. Dodanie wariantu do enuma tagowanego wewnętrznie JEST addytywne (niezmiennik 25);
//! podniesienie wersji wymagałoby migracji, której nie ma. Rozróżnia to
//! [`a_file_written_before_this_kind_existed_still_opens`].
//!
//! # Dlaczego punkt (d) jest tu w ogóle
//!
//! `cargo test` pisze po `target/`, więc krok „sprawdź" NIE jest krokiem tylko do odczytu.
//! Cicha wersja złamania niezmiennika 12 wygląda tak: `facts()` oddaje dla niego `folder: None`,
//! „bo to tylko sprawdzenie" — a wtedy `one_folder_two_steps` pomija go całkowicie
//! (`let (Some(mine), Some(theirs)) = … else continue`) i dwa równoległe kroki budujące w jednym
//! katalogu zapisują się bez słowa.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check};
use loadout_lib::workflow::file::{self, SaveError, save};

/// Żargon, którego zdanie o kroku „sprawdź" nie ma prawa nieść (niezmiennik 14,
/// `checks/quick-vocabulary.sh`). „Proof that it ran" jest zdaniem; „regex" nie jest.
const JARGON: [&str; 3] = ["regex", "exit code", "pattern"];

/// Plik na dysku PRZED każdą próbą zapisu — zapisany w jednej linii, czyli formatem, którego
/// `save()` nigdy nie produkuje.
///
/// To jest celowe: gdyby był sformatowany tak samo, porównanie bajtów po odmowie przechodziłoby
/// także dla implementacji, która plik nadpisała — przypadkiem tą samą treścią.
const ON_DISK: &str = "{\"format\":1,\"id\":\"wf_ship\",\"name\":\"Ship a feature\",\"steps\":[{\"kind\":\"agent\",\"id\":\"s_plan\",\"name\":\"Plan the work\",\"agent\":\"a_planner\",\"instructions\":\"Sketch the steps.\"}],\"links\":[]}";

/// Plik zapisany, ZANIM ten rodzaj kroku istniał — dosłowny tekst, nie zbudowany z naszych typów.
///
/// Zbudowany z naszych typów odpowiadałby na inne pytanie („czy umiemy odczytać to, co sami
/// napiszemy"), a pytanie brzmi: czy plik, który leży dziś na dyskach, dalej się otwiera.
const OLDER_FILE: &str = r#"{
  "format": 1,
  "id": "wf_ship",
  "name": "Ship a feature",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan the work",
      "agent": "a_planner",
      "overrides": {},
      "copies": 1,
      "instructions": "Sketch the steps.",
      "skills": "all",
      "folder": { "use": "fresh-copy" },
      "handover": "notes",
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "checkpoint",
      "id": "s_ok",
      "name": "Does the plan look right?",
      "at": { "x": 24, "y": 168 }
    }
  ],
  "links": [{ "from": "s_plan", "to": "s_ok" }]
}
"#;

fn parsed(file: Value) -> Result<WorkflowFile, Box<dyn Error>> {
    Ok(serde_json::from_value(file)?)
}

/// Workflow z jednym krokiem „sprawdź". `command` i `proof` podaje wołający, żeby ten sam kształt
/// obsłużył i obieg, i dwie odmowy.
fn one_check(command: &str, proof: &str) -> Result<WorkflowFile, Box<dyn Error>> {
    parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [{
            "kind": "check",
            "id": "s_check",
            "name": "Run the checks",
            "command": command,
            "proof": proof,
            "at": { "x": 24, "y": 24 },
            // Klucz, którego TA wersja nie zna — dopisany ręcznie albo przez nowszy build.
            // Bez `#[serde(flatten)] extra` starszy Loadout zapisuje plik z powrotem i kasuje
            // pracę nowszego BEZ JEDNEGO KOMUNIKATU [T3 §3.2, uruchomione na tej maszynie].
            "note": "z nowszego builda"
        }],
        "links": []
    }))
}

/// Uwagi wagi problemu, w kolejności zgłoszenia.
fn problems(file: &WorkflowFile) -> Vec<Note> {
    check(file)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

/// Zdania wszystkich uwag — do komunikatu, kiedy asercja padnie.
fn said(file: &WorkflowFile) -> Vec<String> {
    check(file)
        .into_iter()
        .map(|note| format!("{:?}: {}", note.level, note.message))
        .collect()
}

#[test]
fn a_check_step_survives_the_round_trip_and_keeps_a_key_it_does_not_know()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");
    let before = one_check("./verify.sh full", r"(\d+) passed")?;

    save(&before, &path)?;
    let after = file::load(&path)?;

    assert_eq!(
        after, before,
        "the whole file has to come back equal — not field by field. A workflow is the one thing \
         in Loadout a person can LOSE, and a round trip that drops a field looks identical until \
         the day somebody reopens the file"
    );

    let text = fs::read_to_string(&path)?;
    let document: Value = serde_json::from_str(&text)?;
    let kind = document
        .get("steps")
        .and_then(|steps| steps.get(0))
        .and_then(|step| step.get("kind"))
        .and_then(Value::as_str);
    assert_eq!(
        kind,
        Some("check"),
        "on the wire the kind key reads exactly \"check\". Compared on the PARSED document, not \
         by grepping the text: a grep for `\"kind\": \"check\"` also matches a comment, a name or \
         somebody's instructions, and then the assertion passes over nothing (invariant 20). The \
         file says: {text}"
    );

    let kept = document
        .get("steps")
        .and_then(|steps| steps.get(0))
        .and_then(|step| step.get("note"))
        .and_then(Value::as_str);
    assert_eq!(
        kept,
        Some("z nowszego builda"),
        "a key this build does not know has to survive the round trip in `extra`. Without it one \
         open in an older Loadout eats configuration the newer build cannot rebuild, and leaves \
         no message behind. The file says: {text}"
    );
    Ok(())
}

#[test]
fn a_check_without_a_proof_is_refused_and_the_file_on_disk_does_not_move()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    /* DWIE ODMOWY, NIE JEDNA. Krok bez komendy i krok bez dowodu to dwa różne stany i naprawia
     * się je w dwóch różnych polach kafelka — a implementacja pilnująca tylko jednego z nich
     * przepuszcza drugi, czyli „sprawdzenie", które sprawdza samo siebie (AGENTS.md §4). */
    for (what, file) in [
        ("no proof", one_check("./verify.sh full", "")?),
        ("no command", one_check("", r"(\d+) passed")?),
    ] {
        let found = problems(&file);
        let note = found.first().ok_or_else(|| {
            format!(
                "a check step with {what} is READY AND LYING: it would run and judge on the exit \
                 code alone, and a suite that ran zero tests exits zero (invariant 19). That has \
                 to be a Problem, not a Warning — a Warning does not stop `save()`. check() said: \
                 {:?}",
                said(&file)
            )
        })?;
        assert_eq!(
            note.level,
            Level::Problem,
            "only a Problem stops a save. This is the difference between a file that is refused \
             and a file that lands on disk and runs ({what})"
        );
        assert_eq!(
            note.step_id.as_deref(),
            Some("s_check"),
            "the note has to name the step it is about, because that id is where the dot lands on \
             the tile and what `fitView` jumps to when the person clicks the note ({what})"
        );
        let plain = note.message.to_lowercase();
        for word in JARGON {
            assert!(
                !plain.contains(word),
                "the sentence a person reads may not carry \"{word}\" (invariant 14, \
                 checks/quick-vocabulary.sh). It said: {}",
                note.message
            );
        }

        // ── I DOPIERO TERAZ TO, CO ROZSTRZYGA: bajty pliku po odmowie ─────────────────────
        fs::write(&path, ON_DISK)?;
        let bytes = fs::read(&path)?;
        let error = save(&file, &path)
            .err()
            .ok_or_else(|| format!("save() wrote a workflow whose check step has {what}"))?;
        let described = format!("{error:?}");
        let SaveError::Refused(refusal) = error else {
            return Err(format!("a refusal is not an I/O failure; got: {described}").into());
        };
        assert_eq!(
            refusal.level,
            Level::Problem,
            "the refusal carries the first Problem, and only a Problem may refuse ({what})"
        );
        assert_eq!(
            fs::read(&path)?,
            bytes,
            "the workflow that was on disk is the person's last good version. An implementation \
             that writes first and validates afterwards destroys it at exactly the moment the \
             check was meant to save it ({what})"
        );
    }
    Ok(())
}

#[test]
fn a_file_written_before_this_kind_existed_still_opens() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("older.json");
    fs::write(&path, OLDER_FILE)?;

    let opened = file::load(&path).map_err(|error| {
        format!(
            "a file with only agent and checkpoint steps has to keep opening: {error}. Adding a \
             variant to an internally tagged enum IS additive (invariant 25)"
        )
    })?;
    assert_eq!(
        opened.steps.len(),
        2,
        "both steps of the older file have to come back, not one"
    );

    assert_eq!(
        file::CURRENT,
        1,
        "raising the format version because a kind of step was added makes EVERY workflow saved \
         yesterday unreadable to an older build, and demands a migration that does not exist. \
         This assertion is the only thing in the tree that catches that bump (invariant 25)"
    );
    assert!(
        file::MIGRATIONS.is_empty(),
        "and an empty migration table is the correct state, not a gap: one version until there \
         is a second one. A migration written 'for the future' is forbidden here"
    );

    // Kontrola pozytywna do dwóch asercji wyżej: to, że stary plik się otwiera, nie może być
    // prawdą przez to, że NIC się nie otwiera.
    let fresh = dir.path().join("with-a-check.json");
    save(&one_check("./verify.sh quick", r"(\d+) passed")?, &fresh)?;
    assert!(
        file::load(&fresh).is_ok(),
        "and the new kind still opens too, or this test proves only that loading is broken \
         everywhere equally"
    );
    Ok(())
}

#[test]
fn two_checks_in_one_folder_are_seen_by_the_collision_rule() -> Result<(), Box<dyn Error>> {
    // Dwa kroki „sprawdź", ANI JEDNEJ STRZAŁKI między nimi, oba w folderze projektu. Bez strzałki
    // znaczy „mogą biec równocześnie", a to jest dokładnie ten układ, w którym dwie komendy
    // budujące piszą po tym samym `target/`.
    let both = parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            { "kind": "check", "id": "s_rust", "name": "Run the Rust checks",
              "command": "cargo test", "proof": "(\\d+) passed",
              "folder": { "use": "project" } },
            { "kind": "check", "id": "s_web", "name": "Run the web checks",
              "command": "npm test", "proof": "(\\d+) passed",
              "folder": { "use": "project" } }
        ],
        "links": []
    }))?;

    let notes = check(&both);
    /* Asercja stoi na TREŚCI zdania z `one_folder_two_steps`, nie na tym, że jakakolwiek uwaga
     * powstała. Dwa niepodłączone kroki dają też ostrzeżenie o wyspie („is not connected to the
     * rest of the workflow"), więc `!notes.is_empty()` przechodziłoby dla implementacji, w której
     * reguła kolizji tego rodzaju kroku nie widzi wcale — czyli dla tej, w której niezmiennik 12
     * po cichu przestaje obowiązywać dla całej klasy kroków. */
    let collision = notes
        .iter()
        .find(|note| note.message.contains("can run at the same time"));
    let collision = collision.ok_or_else(|| {
        format!(
            "two checks with no arrow between them both work in the project folder, and `cargo \
             test` writes to target/ — so this is invariant 12 verbatim. If facts() hands the \
             collision rule `folder: None` for this kind of step, one_folder_two_steps skips it \
             entirely and both steps save without a word. check() said: {:?}",
            said(&both)
        )
    })?;
    assert!(
        collision.message.contains("Run the Rust checks")
            && collision.message.contains("Run the web checks"),
        "the note names both tiles by the names a person reads, not by their ids: {}",
        collision.message
    );

    // Kontrola negatywna: ta sama para POŁĄCZONA strzałką nie może kolidować, bo wtedy nie biegnie
    // równocześnie. Reguła, która odmawia zwykłego łańcucha, zostaje wyłączona przez pierwszego,
    // kto ją zobaczy — i wtedy nie chroni już niczego.
    let in_a_row = parsed(json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            { "kind": "check", "id": "s_rust", "name": "Run the Rust checks",
              "command": "cargo test", "proof": "(\\d+) passed",
              "folder": { "use": "project" } },
            { "kind": "check", "id": "s_web", "name": "Run the web checks",
              "command": "npm test", "proof": "(\\d+) passed",
              "folder": { "use": "project" } }
        ],
        "links": [{ "from": "s_rust", "to": "s_web" }]
    }))?;
    assert!(
        !check(&in_a_row)
            .iter()
            .any(|note| note.message.contains("can run at the same time")),
        "one check after another is a chain, not a collision: {:?}",
        said(&in_a_row)
    );
    Ok(())
}
