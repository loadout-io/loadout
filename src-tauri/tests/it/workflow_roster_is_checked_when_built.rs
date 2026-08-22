//! Workflow, którego Start odrzuci, ma być czerwony JUŻ PRZY BUDOWANIU — i naprawialny jednym
//! kliknięciem.
//!
//! # Co to mierzy
//!
//! 2026-08-22 — trzy odmowy pod rząd na jednym biegu właściciela, wszystkie po naciśnięciu Start
//! i wszystkie policzalne wcześniej. Naprawiasz pierwszą, klikasz Start, dostajesz drugą.
//! `workflow::check` sądzi w tym czasie sam plik i o bibliotece nie wie nic, więc płótno malowało
//! „Ready to run" nad workflow, który nie miał prawa ruszyć.
//!
//! # Słabą wersją każdego kryterium niżej jest policzenie uwag
//!
//! Liczba uwag przechodzi dla implementacji, która zgłasza cokolwiek na każdy krok, i dla takiej,
//! która nie umie odróżnić naprawy podniesieniem dialu od naprawy zdjęciem narzędzia z listy —
//! a to są dwa różne miejsca zapisu i dwie różne konsekwencje. Dlatego każde kryterium sądzi
//! TREŚĆ zdania i KSZTAŁT naprawy.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use loadout_lib::library::agents::{Agent, Color, FileAccess, Thinking, Tools, Vendor};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::Level;
use loadout_lib::workflow::roster::{Fix, check_the_roster};
use serde_json::json;
use uuid::Uuid;

fn agent(id: Uuid, name: &str, access: FileAccess, tools: Tools) -> Agent {
    Agent {
        schema: 1,
        id,
        name: name.to_owned(),
        summary: "Does the thing.".to_owned(),
        color: Color::Clay,
        instructions: "Do it.".to_owned(),
        runs_with: Vendor::ClaudeCode,
        model: "opus".to_owned(),
        thinking: Thinking::Balanced,
        file_access: access,
        give_up_after_minutes: 20,
        tools,
        skills: Vec::new(),
        connections: Vec::new(),
        write_results_to: String::new(),
        vendor_options: loadout_lib::library::agents::VendorOptions::new(),
    }
}

/// Jeden krok z tym agentem i tym nadpisaniem dialu.
fn file(agent_id: Uuid, override_access: Option<&str>) -> WorkflowFile {
    let overrides = match override_access {
        Some(value) => json!({ "fileAccess": value }),
        None => json!({}),
    };
    serde_json::from_value(json!({
        "format": 1,
        "id": "wf",
        "name": "Test",
        "steps": [{
            "kind": "agent",
            "id": "s_check",
            "name": "Figma check",
            "agent": agent_id.to_string(),
            "overrides": overrides,
            "instructions": "Check the work.",
            "folder": { "use": "fresh-copy" }
        }],
        "links": []
    }))
    .expect("the fixture is a workflow")
}

#[test]
fn a_step_whose_tools_are_above_its_dial_is_red_before_start_and_offers_the_dial() {
    let id = Uuid::now_v7();
    let saved = agent(
        id,
        "design-qa",
        FileAccess::AskFirst,
        Tools::Only(vec![
            "Read".to_owned(),
            "Bash".to_owned(),
            "Write".to_owned(),
            "Edit".to_owned(),
        ]),
    );
    // Dial ZE STOPNIA, nie z agenta: dokładnie to zrobił właściciel na kafelku „Figma check".
    let notes = check_the_roster(&file(id, Some("look-only")), &[saved], &[], &[]);

    let note = notes
        .first()
        .expect("the step refuses at Start, so it has to be red here");
    assert_eq!(
        note.level,
        Level::Problem,
        "this is not advice: Start turns this run down, so the canvas may not paint it ready"
    );
    assert_eq!(
        note.step_id.as_deref(),
        Some("s_check"),
        "the dot lands on the tile that owns it"
    );
    assert!(
        note.message.contains("Bash") && note.message.contains("look only"),
        "the sentence is the one Start would say, word for word — a person who reads it on the \
         tile and then again in the refusal has to see ONE fault, not two. Got: {:?}",
        note.message
    );
    assert_eq!(
        note.fix.as_deref(),
        Some(&Fix::WidenFileAccess {
            step: "s_check".to_owned(),
            to: FileAccess::AskFirst,
            from: FileAccess::LookOnly,
        }),
        "and the fix is the LOWEST dial that covers those tools: a repair that hands out 'work \
         freely' when 'ask first' would do buys the run with permissions nobody asked for"
    );
}

#[test]
fn a_tool_from_a_connection_is_taken_off_the_agent_instead_of_widening_anything() {
    let id = Uuid::now_v7();
    let saved = agent(
        id,
        "design-qa",
        FileAccess::WorkFreely,
        Tools::Only(vec!["Read".to_owned(), "mcp__playwright".to_owned()]),
    );

    let notes = check_the_roster(&file(id, None), &[saved], &[], &[]);
    let note = notes
        .first()
        .expect("no dial covers a tool from a connection, so this is red");

    assert_eq!(
        note.fix.as_deref(),
        Some(&Fix::DropTools {
            agent: id.to_string(),
            agent_name: "design-qa".to_owned(),
            tools: vec!["mcp__playwright".to_owned()],
        }),
        "widening the dial cannot fix this one and offering it would be a button that changes \
         permissions and leaves the refusal standing. The connection is what brings these tools"
    );
    assert!(
        note.message.contains("the connection brings them"),
        "and the sentence says where they DO come from, or the person takes the tool off and has \
         no idea how the agent is supposed to reach Playwright at all. Got: {:?}",
        note.message
    );
}

#[test]
fn a_step_that_fits_its_dial_says_nothing() {
    let id = Uuid::now_v7();
    let saved = agent(
        id,
        "planner",
        FileAccess::LookOnly,
        Tools::Only(vec![
            "Read".to_owned(),
            "Grep".to_owned(),
            "Glob".to_owned(),
        ]),
    );

    assert!(
        check_the_roster(&file(id, None), &[saved], &[], &[]).is_empty(),
        "without this line every assertion above also passes for a validator that paints every \
         step red, and a screen that is always red is a screen nobody reads"
    );
}

#[test]
fn a_connection_that_is_missing_and_one_that_is_off_are_two_different_sentences() {
    use loadout_lib::connections::{Connection, Transport};

    let id = Uuid::now_v7();
    let mut saved = agent(id, "design-qa", FileAccess::WorkFreely, Tools::Everything);
    saved.connections = vec!["figma".to_owned(), "playwright".to_owned()];

    let off = Connection {
        id: "playwright".to_owned(),
        name: "playwright".to_owned(),
        enabled: false,
        transport: Transport::Stdio {
            command: "npx".to_owned(),
            args: vec!["playwright-mcp".to_owned()],
            environment: Vec::new(),
        },
        source: ".mcp.json".into(),
        source_hash: "abc".to_owned(),
        origin: loadout_lib::connections::Origin::Project,
    };

    let notes = check_the_roster(&file(id, None), &[saved], &[off], &[]);
    let said: Vec<&str> = notes.iter().map(|note| note.message.as_str()).collect();

    assert!(
        said.iter()
            .any(|one| one.contains("figma") && one.contains("nothing saved under that name")),
        "a name that is nowhere is fixed by importing it. Got: {said:?}"
    );
    assert!(
        said.iter()
            .any(|one| one.contains("playwright") && one.contains("turned off")),
        "and a name that is saved but off is fixed by a tick in the import dialog — one sentence \
         for both would leave half the readers with an instruction that cannot work in their \
         case. Got: {said:?}"
    );
    assert!(
        notes.iter().all(|note| note.fix.is_none()),
        "neither one gets a button: turning a tool connection on is the person's decision, and a \
         repair that makes it for them is the opposite of the rule that connections stay off"
    );
}
