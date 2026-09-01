//! Czasowniki Loadouta: co dostaje lider, a czego nie dostaje krok biegu.
//!
//! # Po co ten most w ogóle istnieje
//!
//! Zmierzone 2026-08-29 na `claude 2.1.251`, dokładnie flagami Loadouta: w trybie `-p` vendor
//! **nie daje** narzędzia `AskUserQuestion` — ani domyślnie (27 narzędzi w `system/init`), ani
//! przez `--tools`. Model odpowiedział wprost: „I don't have an `AskUserQuestion` tool available
//! in this session". Agent nie ma więc ŻADNEJ drogi, żeby sięgnąć po cokolwiek, co należy do
//! Loadouta — ani zapytać człowieka, ani zobaczyć jego bibliotekę, ani uruchomić jego workflow.
//!
//! # Dlaczego ROLA, a nie pole w definicji agenta
//!
//! Rozstrzygnięcie właściciela 2026-08-30 (specyfikacja §5.2). Wskazanie lidera JEST zgodą
//! człowieka, wyrażoną tam, gdzie już mieszka. Wersja z przełącznikiem w formularzu została
//! przez niego odrzucona w rozwidleniu, a osobno wymagałaby siedemnastego klucza w zapisanym
//! agencie — czego broni `agents_wire_shape`.
//!
//! # Które kryterium jest tu ważniejsze
//!
//! Drugie. „Lider ma trzy czasowniki" jest zdaniem o wygodzie; „krok biegu nie ma żadnego" jest
//! zdaniem o bezpieczeństwie. Krok, który umie wystartować bieg, startuje go w środku cudzej
//! pracy — i nie jest to „domyślnie wyłączone", tylko STRUKTURALNIE NIEMOŻLIWE.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::bridge::{Role, verbs};

#[test]
fn a_lead_gets_the_library_and_the_start() {
    let names: Vec<&str> = verbs::for_role(Role::Lead)
        .iter()
        .map(|verb| verb.name)
        .collect();

    assert_eq!(
        names,
        vec![
            "ask_the_person",
            "list_workflows",
            "list_agents",
            "start_workflow"
        ],
        "the lead is the orchestrator: it has to be able to ask when it does not know, to see \
         what this person built, and to start it. These names travel to the model, so they are \
         part of the contract and not an implementation detail.\n\n\
         `ask_the_person` stands FIRST on purpose: it is the one a model reaches for before it \
         guesses, and the order of this list is the order it reads them in"
    );
}

#[test]
fn a_run_step_gets_nothing_at_all() {
    assert!(
        verbs::for_role(Role::Step).is_empty(),
        "a step inside a run must not be able to start another run. Not 'off by default' — \
         absent, so the model never learns the verb exists and never promises the person \
         something it cannot do"
    );
}

#[test]
fn the_tool_list_is_shaped_the_way_mcp_asks_for_it() {
    let listed = verbs::tool_list(Role::Lead);
    let tools = listed
        .as_array()
        .expect("the verb table is an array of tool definitions");

    assert_eq!(tools.len(), 4, "four verbs, four entries");

    let first = tools.first().expect("the array carries the first verb");
    assert_eq!(
        first.get("name").and_then(serde_json::Value::as_str),
        Some("ask_the_person")
    );
    assert!(
        first
            .get("description")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|said| !said.is_empty()),
        "a verb without a description is a verb the model will not reach for"
    );
    assert_eq!(
        first
            .pointer("/inputSchema/type")
            .and_then(serde_json::Value::as_str),
        Some("object"),
        "MCP names this key `inputSchema`, not `schema`. The wrong key means the vendor drops the \
         tool in silence, and from the outside that looks exactly like a lead that did not want \
         to use it"
    );
}

/// Zadanie WYMAGANE zmuszałoby lidera do wymyślania pracy workflow, którego kroki już mówią, co
/// robią — a wymyślone zdanie jedzie stąd do sześciu agentów jako polecenie.
#[test]
fn starting_a_workflow_needs_the_name_and_nothing_else() {
    let start = verbs::for_role(Role::Lead)
        .into_iter()
        .find(|verb| verb.name == "start_workflow")
        .expect("the lead can start a workflow");

    let required: Vec<&str> = start
        .schema
        .pointer("/required")
        .and_then(serde_json::Value::as_array)
        .expect("the schema says what it needs")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();

    assert_eq!(required, vec!["workflow"]);
}
