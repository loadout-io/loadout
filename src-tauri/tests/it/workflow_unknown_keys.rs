//! AC-2 dla T-12: klucz, którego ta wersja nie zna, przeżywa zapis — a klucz znany, którego
//! w pliku nie było, dostaje wartość domyślną. **W tym samym wyniku.**
//!
//! To kryterium celowo **nie jest** round-tripem, i to jest cała jego konstrukcja. „Wejście
//! równa się wyjściu" przechodzi tożsamość: implementacja, która w ogóle nie parsuje, tylko
//! przepisuje bajty, zdaje każdy taki test i nie umie zapisać żadnej zmiany. Dlatego wynik
//! musi nieść **jednocześnie**:
//!
//! - `temperature` i `retries` — klucze, których ten build nie rozumie, nietknięte, razem
//!   z typem liczbowym (`0.3` zostaje ułamkiem, `3` zostaje liczbą całkowitą),
//! - `copies: 1` — klucz, który rozumiemy i którego w wejściu **nie było**.
//!
//! Słaba wersja to `assert!(serde_json::from_str::<WorkflowFile>(src).is_ok())`. Przechodzi ją
//! struktura bez `deny_unknown_fields` i bez `extra`: serde po prostu zignoruje `temperature`,
//! a zapis go skasuje — czyli dokładnie ta awaria, przed którą to kryterium stoi. Starszy build
//! zjada wtedy konfigurację nowszego i nie zostawia po tym ani jednego komunikatu.

use std::error::Error;
use std::fs;

use serde_json::{Value, json};

use loadout_lib::workflow::file::{load, save};

/// Krok dokładnie taki, jak w kryterium: dwa klucze z przyszłości i **brak** `copies`.
const A_STEP_FROM_A_NEWER_BUILD: &str = r#"{
  "format": 1,
  "id": "wf_ship",
  "name": "Ship a feature",
  "steps": [
    {
      "kind": "agent",
      "id": "s1",
      "name": "Build",
      "agent": "a1",
      "temperature": 0.3,
      "retries": { "max": 3 }
    }
  ],
  "links": []
}
"#;

#[test]
fn a_key_this_version_does_not_know_survives_a_save() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("newer.json");
    fs::write(&source, A_STEP_FROM_A_NEWER_BUILD)?;
    let target = dir.path().join("saved.json");

    save(&load(&source)?, &target)?;

    let written: Value = serde_json::from_str(&fs::read_to_string(&target)?)?;
    let step = &written["steps"][0];

    assert_eq!(
        step["temperature"],
        json!(0.3),
        "a build that does not understand `temperature` still may not eat it: this is the one \
         line that stops an older Loadout from deleting a newer one's work. The step reads: \
         {step}"
    );
    assert!(
        step["temperature"].is_f64(),
        "0.3 has to come back as 0.3 and not as 0 — a number that changes type on the way \
         through is data loss with a friendlier face"
    );
    assert_eq!(
        step["retries"],
        json!({ "max": 3 }),
        "nested unknown values survive whole, not flattened and not stringified"
    );
    assert!(
        step["retries"]["max"].is_u64(),
        "3 stays a whole number; re-encoding it as 3.0 is the same class of loss as dropping it"
    );
    Ok(())
}

#[test]
fn a_known_key_that_was_missing_is_written_with_its_default() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("newer.json");
    fs::write(&source, A_STEP_FROM_A_NEWER_BUILD)?;
    let target = dir.path().join("saved.json");

    save(&load(&source)?, &target)?;

    let written: Value = serde_json::from_str(&fs::read_to_string(&target)?)?;
    let step = &written["steps"][0];

    assert_eq!(
        step["copies"],
        json!(1),
        "`copies` was absent from the input, so the saved file has to state it: this is the \
         assertion that identity — never parsing, just copying bytes — cannot pass. The step \
         reads: {step}"
    );
    assert_eq!(
        step["temperature"],
        json!(0.3),
        "and it has to state it in the SAME file that still carries the key we do not \
         understand; two passing tests over two different outputs would prove neither"
    );
    Ok(())
}
