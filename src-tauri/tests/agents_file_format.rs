//! Kryterium 2 dla T-11: plik agenta na dysku — treść to instrukcje, a nieznany klucz jest
//! odmową z nazwą pliku.
//!
//! Słaba wersja tego kryterium wczytuje plik i sprawdza `agent.name == "Forge"`. Ona
//! przechodzi dla parsera, który treść pod front-matterem po prostu wyrzuca — a treść to
//! 80% definicji agenta. Dlatego niżej stoi porównanie `instructions` z wielolinijkowym
//! literałem, który ma pustą linię **w środku** i pustą linię **na końcu**: obcinanie
//! białych znaków jest tu awarią, nie porządkiem, bo instrukcje pisze człowiek i akapity
//! są ich częścią.
//!
//! Drugi kierunek jest równie ważny: front-matter **nie ma** klucza `instructions`. Gdyby
//! miał, najdłuższe pole definicji miałoby dwa źródła prawdy i pierwsza ręczna edycja pliku
//! rozjechałaby je po cichu [T4 §5.1].
//!
//! Nieznany klucz i nieznany kolor są odmową, a nie ostrzeżeniem. Zmierzone w T4 §9:
//! `claude --agents '{"broken":{"model":"sonnet"}}' -p "hi"` kończy się **kodem 0, bez słowa
//! na stderr** — zepsuta definicja wygląda dokładnie tak samo jak zła instrukcja w promptcie.
//! Odmowa musi więc nazwać plik, żeby dało się go otworzyć i poprawić [T4 §10].
//!
//! Pięć kolorów przechodzi, szósty nie. Bez tej pierwszej połowy „`neon` daje `Err`"
//! przechodzi dla parsera, który odmawia **wszystkiego**.

use std::error::Error;

use loadout_lib::library::agents::{
    Agent, Color, FileAccess, Thinking, Tools, Vendor, VendorOptions, read_agent_file,
    write_agent_file,
};
use tempfile::TempDir;
use uuid::Uuid;

const ID: &str = "019897b4-8f3a-7c21-9d44-0b6a1e2c5f77";

/// Instrukcje z pustą linią w środku i pustą linią na końcu. Oba te białe znaki są
/// asercją, nie ozdobą: pierwsza to akapit, druga to dowód, że nikt nic nie obcina.
const BODY: &str = "Write the smallest change that makes the checks pass.\n\nIf a check looks \
                    wrong, say so instead of changing it.\n\n";

/// Plik agenta w formacie z T4 §5.1. `extra` dokłada jeden wiersz front-mattera (razem
/// z jego znakiem końca linii) albo jest pusty.
fn file_text(color: &str, extra: &str) -> String {
    format!(
        "---\n\
         schema: 1\n\
         id: {ID}\n\
         name: Forge\n\
         summary: Writes code\n\
         color: {color}\n\
         runsWith: claude-code\n\
         model: opus\n\
         thinking: balanced\n\
         fileAccess: work-freely\n\
         giveUpAfterMinutes: 20\n\
         writeResultsTo: handoffs/build.md\n\
         tools: everything\n\
         skills: []\n\
         connections: []\n\
         {extra}\
         ---\n\
         {BODY}"
    )
}

fn forge() -> Result<Agent, Box<dyn Error>> {
    Ok(Agent {
        schema: 1,
        id: Uuid::parse_str(ID)?,
        name: "Forge".to_string(),
        summary: "Writes code".to_string(),
        color: Color::Clay,
        instructions: BODY.to_string(),
        runs_with: Vendor::ClaudeCode,
        model: "opus".to_string(),
        thinking: Thinking::Balanced,
        file_access: FileAccess::WorkFreely,
        give_up_after_minutes: 20,
        tools: Tools::Everything,
        skills: Vec::new(),
        connections: Vec::new(),
        write_results_to: "handoffs/build.md".to_string(),
        vendor_options: VendorOptions::new(),
    })
}

/// Front-matter i treść, rozdzielone tak, jak rozdziela je format: `---` na początku,
/// `---` w osobnym wierszu, reszta to treść.
fn front_matter_and_body(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some((&rest[..end], &rest[end + "\n---\n".len()..]))
}

#[test]
fn the_body_of_the_file_is_the_instructions_character_for_character() -> Result<(), Box<dyn Error>>
{
    let dir = TempDir::new()?;
    let path = dir.path().join("forge.md");
    std::fs::write(&path, file_text("clay", ""))?;

    let agent = read_agent_file(&path)?;

    assert_eq!(
        agent.instructions, BODY,
        "the text under the front matter is the instructions, whole. A blank line inside is a \
         paragraph the person wrote, and a blank line at the end is not whitespace to tidy \
         away — trimming either rewrites what the user typed"
    );
    assert_eq!(
        agent.name, "Forge",
        "the front matter still has to be read; this half is the cheap half"
    );
    Ok(())
}

#[test]
fn the_written_front_matter_never_repeats_the_instructions() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let written = write_agent_file(dir.path(), &forge()?)?;
    let text = std::fs::read_to_string(&written)?;

    let (front, body) = front_matter_and_body(&text)
        .ok_or("a saved agent has to be front matter, then a closing fence, then the body")?;

    assert!(
        !front
            .lines()
            .any(|line| line.trim_start().starts_with("instructions")),
        "the front matter must not carry an instructions key: that would be two sources of \
         truth for the longest field in the definition. Front matter reads:\n{front}"
    );
    assert!(
        body.contains("Write the smallest change"),
        "the instructions have to be in the body, where they can be hand-edited and read in a \
         diff without escaping. Body reads:\n{body}"
    );
    Ok(())
}

#[test]
fn a_setting_loadout_does_not_know_is_refused_and_the_message_names_the_file()
-> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("forge.md");
    std::fs::write(&path, file_text("clay", "temperature: 0.3\n"))?;

    let error = read_agent_file(&path)
        .err()
        .ok_or("a file carrying temperature: 0.3 has to be refused, not quietly accepted")?;
    let message = error.to_string();

    assert!(
        message.contains("forge.md"),
        "the message has to name the file, because the only thing the user can do with it is \
         open it. It reads: {message}"
    );
    assert!(
        message.contains("temperature"),
        "the message has to name the setting it choked on, or the user reads their whole file \
         looking for it. It reads: {message}"
    );
    Ok(())
}

#[test]
fn the_five_colours_load_and_a_sixth_one_is_refused() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;

    for name in ["slate", "plum", "clay", "moss", "rose"] {
        let path = dir.path().join(format!("{name}.md"));
        std::fs::write(&path, file_text(name, ""))?;

        let agent = read_agent_file(&path)?;
        assert_eq!(
            serde_json::to_value(agent.color)?,
            serde_json::json!(name),
            "{name} is one of the five identity colours, so it has to load and come back \
             spelled the same way"
        );
    }

    let path = dir.path().join("neon.md");
    std::fs::write(&path, file_text("neon", ""))?;

    let error = read_agent_file(&path)
        .err()
        .ok_or("neon is not one of the five identity colours, so the file has to be refused")?;

    let message = error.to_string();
    assert!(
        message.contains("neon.md"),
        "a refused colour is still a refused file, and the message has to name it so the user \
         can open it. It reads: {message}"
    );
    Ok(())
}
