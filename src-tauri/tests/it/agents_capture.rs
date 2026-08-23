//! Kryterium 4 dla T-11: `capture()` nie przepuszcza pól, których nie wolno nadpisać,
//! i nigdy nie produkuje `null`.
//!
//! Słaba wersja tego kryterium to `assert_eq!(OVERRIDABLE.len(), 9)`. Stała ma dziewięć
//! pozycji także wtedy, gdy `retain` po niej nigdy się nie wykonuje — a wtedy patch dalej
//! wynosi `runsWith` na krok. Dlatego niżej stoi równość **zbioru kluczy wyprodukowanego
//! patcha**, nie długość listy stałych.
//!
//! Dlaczego akurat `runsWith` nie jest kosmetyczne: przełączenie vendora na kroku
//! unieważniłoby połowę pozostałych pól — na przykład listę `tools`, której Codex nie ma jak
//! uszanować [T4 §6.4]. Odmowa na poziomie typu kasuje całą klasę walidacji.
//!
//! Druga połowa pliku pilnuje reguły „żadnych nulli". W RFC 7396 `null` w patchu **kasuje
//! klucz**, a skasowany klucz to plik ustawień, który się nie wczyta [T4 §4.3, zweryfikowane
//! lokalnie]. Dlatego brak jest zawsze wartością: „bez limitu" to `0`, „bez umiejętności" to
//! `[]`, „wszystkie narzędzia" to wariant `Everything`.

use std::error::Error;

use loadout_lib::library::agents::{
    Agent, Color, FileAccess, Thinking, Tools, Vendor, VendorOptions, capture, validate_no_nulls,
};
use serde_json::Value;
use uuid::Uuid;

const ID: &str = "019897b4-8f3a-7c21-9d44-0b6a1e2c5f77";
const OTHER_ID: &str = "019897b4-8f3a-7c21-9d44-0b6a1e2c5f78";

fn forge() -> Result<Agent, Box<dyn Error>> {
    Ok(Agent {
        schema: 1,
        id: Uuid::parse_str(ID)?,
        name: "Forge".to_string(),
        summary: "Writes code".to_string(),
        color: Color::Clay,
        instructions: "Write the smallest change that makes the checks pass.".to_string(),
        runs_with: Vendor::ClaudeCode,
        model: "opus".to_string(),
        thinking: Thinking::Balanced,
        file_access: FileAccess::WorkFreely,
        give_up_after_minutes: 20,
        tools: Tools::Everything,
        reaches_the_web: false,
        skills: Vec::new(),
        connections: Vec::new(),
        write_results_to: "handoffs/build.md".to_string(),
        vendor_options: VendorOptions::new(),
    })
}

/// Czy gdziekolwiek w tym dokumencie stoi `null`.
fn holds_a_null(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(map) => map.values().any(holds_a_null),
        Value::Array(items) => items.iter().any(holds_a_null),
        _ => false,
    }
}

#[test]
fn a_step_can_only_carry_the_settings_it_is_allowed_to_change() -> Result<(), Box<dyn Error>> {
    let base = forge()?;
    let mut edited = base.clone();
    edited.name = "Anvil".to_string();
    edited.id = Uuid::parse_str(OTHER_ID)?;
    edited.runs_with = Vendor::Codex;
    edited.thinking = Thinking::Deep;

    let patch = capture(&base, &edited)?;
    let wire = serde_json::to_value(patch)?;
    let object = wire
        .as_object()
        .ok_or("what a step stores has to be a JSON object")?;

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();

    assert_eq!(
        keys,
        ["thinking"],
        "four settings were edited and exactly one of them may live on a step. A step that \
         carries runsWith would switch the agent to the other app and invalidate half of the \
         rest — the tools list first. Name and id are the agent's identity, not the step's"
    );
    Ok(())
}

#[test]
fn no_differences_at_all_produce_an_empty_patch() -> Result<(), Box<dyn Error>> {
    let base = forge()?;

    let patch = capture(&base, &base)?;

    assert_eq!(
        serde_json::to_value(patch)?,
        serde_json::json!({}),
        "a step where the user changed nothing stores nothing. Any key here would show up as \
         `1 changed` on a step nobody touched, and would then survive the next edit of the agent"
    );
    Ok(())
}

#[test]
fn an_empty_value_on_the_wire_is_refused_and_the_message_names_the_setting()
-> Result<(), Box<dyn Error>> {
    let error = validate_no_nulls(&serde_json::json!({ "instructions": null }))
        .err()
        .ok_or("a null on the wire has to be refused before it becomes a merge patch")?;
    let message = error.to_string();

    assert!(
        message.contains("instructions"),
        "the message has to name the setting that came in empty; under RFC 7396 that null \
         deletes the key and leaves a file that will not load. It reads: {message}"
    );

    validate_no_nulls(&serde_json::json!({ "thinking": "deep", "giveUpAfterMinutes": 45 }))?;
    Ok(())
}

#[test]
fn nothing_is_ever_written_as_an_absence() -> Result<(), Box<dyn Error>> {
    let agent = Agent {
        give_up_after_minutes: 0,
        reaches_the_web: false,
        skills: Vec::new(),
        connections: Vec::new(),
        tools: Tools::Everything,
        write_results_to: String::new(),
        ..forge()?
    };

    let wire = serde_json::to_value(agent)?;

    assert_eq!(
        wire.get("giveUpAfterMinutes"),
        Some(&serde_json::json!(0)),
        "no time limit is the number zero, never a missing key"
    );
    assert_eq!(
        wire.get("skills"),
        Some(&serde_json::json!([])),
        "no skills is an empty list, never a missing key"
    );
    assert_eq!(
        wire.get("tools"),
        Some(&serde_json::json!("everything")),
        "all tools is a named value, never a missing key"
    );
    assert!(
        !holds_a_null(&wire),
        "a saved agent must not contain a single null anywhere. Under RFC 7396 a null in a \
         patch deletes the key it stands on, so one null in a file is one setting that \
         disappears the next time a step merges over it. It reads: {wire}"
    );
    Ok(())
}
