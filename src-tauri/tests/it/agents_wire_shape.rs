//! Kryterium 1 dla T-11: zapisany agent ma **dokładnie** piętnaście kluczy i ani jednego
//! z podkreśleniem.
//!
//! Słaba wersja tego kryterium to `assert!(v.get("thinking").is_some())` powtórzone
//! piętnaście razy. Ona przechodzi w dniu, w którym ktoś dołoży `temperature` — a to jest
//! jedyny defekt, przed którym to kryterium broni. Formularz agenta **rośnie**: każde pole
//! da się uzasadnić pojedynczo, a suma po trzech miesiącach to strona ustawień poprzedniego prototypu
//! z 28 atrybutami, których nikt nie tyka. Dlatego niżej stoi równość posortowanego wektora
//! kluczy z listą wypisaną na sztywno, nie piętnaście pytań o obecność.
//!
//! Lista jest wypisana TUTAJ, a nie czytana ze struktury. Pętla po polach `Agent` pytałaby
//! definicję o nią samą i przeszłaby dla każdej definicji, jaka kiedykolwiek powstanie.
//!
//! Drugi test pyta o podkreślenia i pyta o nie **rekurencyjnie**. Enum niosący dane
//! potrzebuje w serde obu atrybutów naraz — `rename_all` i `rename_all_fields` (04 §2.5);
//! brak drugiego wysłał kiedyś `started_at` do frontendu, który czyta wyłącznie
//! `camelCase`, i położył ekran.
//! Klucz zagnieżdżony w `tools` jest dokładnie tym miejscem, w którym ta pomyłka się chowa.

use std::error::Error;

use loadout_lib::library::agents::{Agent, FileAccess, Thinking, Tools};
use serde_json::Value;

/// Piętnaście kluczy, po posortowaniu: jedenaście z T4 §3.1 (razem z ukrytym `id`),
/// trzy z §3.2 i `schema`. (T4 pisze „eleven fields, nine visible" — to liczy samą tabelę
/// §3.1; arytmetyka raportu jest luźna, liczba kluczy nie jest.)
const KEYS: [&str; 16] = [
    "color",
    "connections",
    "fileAccess",
    "giveUpAfterMinutes",
    "id",
    "instructions",
    "model",
    "name",
    "reachesTheWeb",
    "runsWith",
    "schema",
    "skills",
    "summary",
    "thinking",
    "tools",
    "writeResultsTo",
];

/// Ścieżki wszystkich kluczy z podkreśleniem, także tych zagnieżdżonych.
fn underscored(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key.contains('_') {
                    found.push(format!("{path}{key}"));
                }
                underscored(child, &format!("{path}{key}."), found);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                underscored(child, &format!("{path}{index}."), found);
            }
        }
        _ => {}
    }
}

#[test]
fn a_saved_agent_carries_exactly_these_sixteen_keys() -> Result<(), Box<dyn Error>> {
    let wire = serde_json::to_value(Agent::example())?;
    let object = wire
        .as_object()
        .ok_or("an agent has to serialise to a JSON object")?;

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();

    assert_eq!(
        keys, KEYS,
        "a saved agent has to carry these sixteen keys and no others. A seventeenth one is how the \
         form starts growing towards the settings page nobody fills in; a missing one is a \
         setting the user set and Loadout dropped"
    );
    Ok(())
}

#[test]
fn no_key_anywhere_in_the_wire_shape_carries_an_underscore() -> Result<(), Box<dyn Error>> {
    let mut agent = Agent::example();
    // `tools` jest jedynym miejscem, w którym agent ma zagnieżdżony obiekt — czyli jedynym,
    // w którym brak `rename_all_fields` przepuściłby snake_case przez kolejny poziom.
    agent.tools = Tools::Only(vec!["Read".to_string(), "Grep".to_string()]);
    let wire = serde_json::to_value(agent)?;

    let mut found = Vec::new();
    underscored(&wire, "", &mut found);

    assert!(
        found.is_empty(),
        "every key the frontend reads is camelCase. These carry an underscore, so the screen \
         that reads them gets undefined and shows nothing: {found:?}"
    );
    Ok(())
}

#[test]
fn the_three_enums_spell_their_values_the_way_the_frontend_reads_them() -> Result<(), Box<dyn Error>>
{
    let thinking = [
        (Thinking::Quick, "quick"),
        (Thinking::Balanced, "balanced"),
        (Thinking::Deep, "deep"),
        (Thinking::Deepest, "deepest"),
    ];
    for (value, spelling) in thinking {
        assert_eq!(
            serde_json::to_value(value)?,
            serde_json::json!(spelling),
            "the four thinking levels are the wire values the form binds to; {value:?} has to \
             be written as {spelling}"
        );
    }

    let access = [
        (FileAccess::LookOnly, "look-only"),
        (FileAccess::AskFirst, "ask-first"),
        (FileAccess::WorkFreely, "work-freely"),
    ];
    for (value, spelling) in access {
        assert_eq!(
            serde_json::to_value(value)?,
            serde_json::json!(spelling),
            "the safety dial is the one setting a user must never misread; {value:?} has to be \
             written as {spelling}"
        );
    }

    assert_eq!(
        serde_json::to_value(Tools::Everything)?,
        serde_json::json!("everything"),
        "all tools is a value, not an absence — an absent key would delete the setting under \
         RFC 7396"
    );
    assert_eq!(
        serde_json::to_value(Tools::Only(vec!["Read".to_string()]))?,
        serde_json::json!({ "only": ["Read"] }),
        "a named list of tools is written as an object with one key, and that key is `only`"
    );
    Ok(())
}

/* 2026-08-23 — DOMYŚLNA SIECI JEST WŁĄCZONA, TAKŻE DLA PLIKÓW ZAPISANYCH WCZEŚNIEJ.
 *
 * Rozstrzygnięcie właściciela („niech to będzie true by default") stoi na jego liczbach:
 * 18 zapisanych agentów, ani jeden z siecią, bo do wyłączonej domyślnej trzeba było TRAFIĆ.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie nowo zbudowanego agenta. Przechodzi ją `#[serde(default)]` na
 * `bool`, czyli `false` — a wtedy domyślna obowiązuje wyłącznie agentów utworzonych po tej
 * zmianie, a cała istniejąca biblioteka zostaje tam, gdzie była. To są dwie różne odpowiedzi
 * na jedno pytanie. Dlatego sądzony jest PLIK BEZ TEGO KLUCZA, wypisany tu literalnie.
 */
#[test]
fn an_agent_saved_before_this_key_existed_reads_back_with_the_web_on() -> Result<(), Box<dyn Error>>
{
    let older = r#"{
      "schema": 1,
      "id": "0198a1f2-3b4c-7d5e-8f60-000000000001",
      "name": "Scout",
      "summary": "Looks things up",
      "color": "clay",
      "instructions": "Find out how this works.",
      "runsWith": "claude-code",
      "model": "sonnet",
      "thinking": "balanced",
      "fileAccess": "look-only",
      "giveUpAfterMinutes": 20,
      "tools": "everything",
      "skills": [],
      "connections": [],
      "writeResultsTo": ""
    }"#;

    let read: Agent = serde_json::from_str(older)?;

    assert!(
        read.reaches_the_web,
        "an agent saved before this key existed has to come back with the web on. Reading it as \
         off would mean the default holds for new agents only, and every agent the person \
         already has stays behind — one question with two answers"
    );
    Ok(())
}
