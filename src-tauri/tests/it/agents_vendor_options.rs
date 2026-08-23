//! Kryterium 8 dla T-11: opcje vendora przelatują nietknięte, a Loadout ich nie interpretuje.
//!
//! To jest przelotka z `DECISIONS-LOCKED.md` §D6. Bez niej każda nowa flaga vendora wymaga
//! **wydania Loadouta**; z nią — wpisania jednej linii w formularzu agenta tego samego dnia,
//! w którym vendor tę flagę ogłosi.
//!
//! Słaba wersja tego kryterium to `assert!(args.contains("--jakas-nowa-flaga"))`. Ona
//! przechodzi dla implementacji, która wkleja sam klucz i gubi wartość — a `--effort` bez
//! `high` to albo błąd składni, albo, gorzej, flaga, która znaczy co innego. Rozróżnia to
//! **sąsiedztwo** pary w zwróconym wektorze.
//!
//! Druga połowa pilnuje determinizmu zapisu. Mapa o nieokreślonej kolejności daje plik, który
//! zmienia się sam przy każdym zapisie: `git diff` na katalogu agentów przestaje cokolwiek
//! znaczyć, a „czy ktoś tego agenta ruszał" traci odpowiedź. Dlatego porównujemy bajty po
//! dwóch kolejnych zapisach.
//!
//! Nieznany vendor przeżywa zapis i **nie** trafia do argumentów Claude'a. To nie jest błąd:
//! przelotka ma przetrwać vendora, którego jeszcze nie wspieramy — dokładnie po to jest.
//!
//! Czego tu świadomie nie ma: asercji na zbudowanej komendzie. Argv buduje sterownik,
//! `claude.rs` (T-04) i `codex.rs` (T-10), i to jest cudzy plik — jedna polityka, jedno
//! miejsce (niezmiennik 23). To zadanie dostarcza sterownikowi czystą funkcję i za nią
//! odpowiada.

use std::collections::BTreeMap;
use std::error::Error;

use loadout_lib::library::agents::{
    Agent, Color, FileAccess, Thinking, Tools, Vendor, VendorOptions, read_agent_file, vendor_args,
    write_agent_file,
};
use tempfile::TempDir;
use uuid::Uuid;

const NEW_FLAG: &str = "--jakas-nowa-flaga";
const NEW_VALUE: &str = "wartosc";
const SECOND_FLAG: &str = "--druga-flaga";
const SECOND_VALUE: &str = "druga-wartosc";

fn passthrough() -> VendorOptions {
    let mut claude = BTreeMap::new();
    claude.insert(NEW_FLAG.to_string(), NEW_VALUE.to_string());
    claude.insert(SECOND_FLAG.to_string(), SECOND_VALUE.to_string());

    // Vendor, którego jeszcze nie wspieramy. Ma przeżyć zapis i nie ma prawa dopisać się
    // do argumentów Claude'a.
    let mut gemini = BTreeMap::new();
    gemini.insert("--thinking-budget".to_string(), "8192".to_string());

    let mut all = VendorOptions::new();
    all.insert("claude".to_string(), claude);
    all.insert("gemini".to_string(), gemini);
    all
}

fn forge() -> Result<Agent, Box<dyn Error>> {
    Ok(Agent {
        schema: 1,
        id: Uuid::parse_str("019897b4-8f3a-7c21-9d44-0b6a1e2c5f77")?,
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
        vendor_options: passthrough(),
    })
}

/// Pozycja wartości, która stoi zaraz za tym kluczem.
fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|item| item == flag)?;
    args.get(at + 1).map(String::as_str)
}

#[test]
fn a_flag_loadout_has_never_heard_of_survives_a_save_and_a_load() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let agent = forge()?;

    let written = write_agent_file(dir.path(), &agent)?;
    let read_back = read_agent_file(&written)?;

    assert_eq!(
        read_back.vendor_options, agent.vendor_options,
        "the passthrough comes back exactly as it went in, character for character. Loadout \
         does not read what is inside it, so it has nothing to normalise and no right to"
    );
    Ok(())
}

#[test]
fn every_flag_is_handed_over_with_its_value_next_to_it() -> Result<(), Box<dyn Error>> {
    let agent = forge()?;

    let args = vendor_args(&agent, "claude");

    assert_eq!(
        args.len(),
        4,
        "two flags with two values is four items, once. A flag pasted without its value, or \
         pasted twice, is how a passthrough turns into a syntax error at spawn time. Got: \
         {args:?}"
    );
    assert_eq!(
        value_after(&args, NEW_FLAG),
        Some(NEW_VALUE),
        "the value has to sit immediately after its flag, or the CLI reads the next flag as \
         this one's value. Got: {args:?}"
    );
    assert_eq!(
        value_after(&args, SECOND_FLAG),
        Some(SECOND_VALUE),
        "the same for the second pair. Got: {args:?}"
    );

    assert!(
        !args.iter().any(|item| item.contains("thinking-budget")),
        "options written for another agent app must not reach this one. Got: {args:?}"
    );
    Ok(())
}

#[test]
fn saving_the_same_agent_twice_writes_the_same_bytes() -> Result<(), Box<dyn Error>> {
    let agent = forge()?;
    let first_dir = TempDir::new()?;
    let second_dir = TempDir::new()?;

    let first = std::fs::read(write_agent_file(first_dir.path(), &agent)?)?;
    let second = std::fs::read(write_agent_file(second_dir.path(), &agent)?)?;

    assert_eq!(
        first, second,
        "two saves of one agent have to produce the same bytes. A map with no defined order \
         gives a file that rewrites itself on every save, and then the folder of agents can no \
         longer answer the question of who changed what"
    );
    Ok(())
}
