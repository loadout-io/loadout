//! T-157: wartość, która wygląda na token albo klucz, nie ma prawa wjechać do pliku definicji.
//!
//! # Po co to istnieje
//!
//! Definicje workflowów i agentów są **zwykłymi plikami**: idą do gita, do kopii i do wyników
//! biegu. Sekret ma w tym produkcie dokładnie jedną drogę — env dziecka (niezmiennik 9) — a
//! przelotka `vendorOptions` i pole „co uruchomić" są dwiema jedynymi szparami, przez które
//! literał może się do pliku wcisnąć. Zmierzone przed tym zadaniem: nie było ani jednego
//! detektora przy zapisie, więc `--auth-header: sk-ant-api03-…` zapisywał się bez słowa.
//!
//! # Trzy rzeczy, które to kryterium sprawdza, i jedna, której pilnuje
//!
//! **(1) Odmowa PRZED dyskiem.** Nie „zapis się nie udał", a „nic nie powstało": po odmowie nie
//! ma pliku, nie ma katalogu, a przy nadpisaniu istniejącego leżą tam poprzednie bajty co do
//! znaku. Zapis, który waliduje po dotknięciu dysku, niszczy wersję, której miał bronić.
//!
//! **(2) Zdanie nazywa WIERSZ.** Odmowa bez nazwy pola jest samym niepokojem — człowiek nie wie,
//! co skasować. Pytamy więc o zdania, które człowiek naprawdę czyta (niezmiennik 29): uwagę
//! z zapisu workflowu (`SaveError::Refused`, ekran `couldNotSave` w edytorze) i komunikat błędu
//! zapisu agenta (`AgentError` → `error.to_string()` w `ipc::save_agent` → pole `refusal`
//! w sekcji Agents).
//!
//! **(3) Rozróżnienie należy do KSZTAŁTU, nie do nazwy pola.** Flaga o niewinnej nazwie
//! (`--auth-header`) też musi zostać złapana, więc reguła nie ma prawa opierać się na tym, jak
//! wpis się nazywa.
//!
//! **Kontrola, i to jest asercja, której nie przechodzi implementacja odmawiająca wszystkiemu.**
//! Zwykłe wartości konfiguracyjne — `opus`, `xhigh`, `workspace-write`, SHA gita, UUID, ścieżka
//! z cyframi, wiersz `npm test -- --reporter=dot` — zapisują się normalnie i plik ląduje.
//! Fałszywa odmowa jest tu gorsza niż brak sprawdzenia, bo blokuje pracę.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::library::agents::{Agent, VendorOptions, write_agent_file};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};

/// Nazwa vendora w przelotce — ta sama, którą pyta `workflow::check::reserved`.
const VENDOR: &str = "claude";

/// Nazwa kroku agenta w fikstyrze. To ona stoi na kafelku, więc to ona pada w uwadze.
const STEP: &str = "Build";

/// Nazwa kroku „sprawdź". Osobna, żeby uwaga o komendzie nie dała się pomylić z uwagą
/// o przelotce.
const CHECK: &str = "Publish";

/// Flaga o NIEWINNEJ NAZWIE — nie jest zarezerwowana, nie podnosi dialu, nie ma w sobie słowa
/// „key" ani „token". Jedyne, przez co ten wiersz odpada, jest kształt jego wartości.
const INNOCENT: &str = "--auth-header";

/// Literał w kształcie klucza jednego z vendorów. Zmyślony co do znaku, ale w prawdziwym
/// kształcie: prefiks rodziny plus czterdzieści znaków ogona.
const A_KEY: &str = "sk-ant-api03-Rf4mQ2xW9tLb7Yc3Nd8Ke1Ph5Zs0Vg6Ja2Uo4";

/// Adres z hasłem w środku. Wartość jest krótka z rozmysłu: gdyby reguła wymagała od hasła
/// długości, ten wiersz przeszedłby, a jest dokładnie tym, co ludzie wklejają do komend.
const AN_ADDRESS: &str = "curl https://ci:9f3b7c2e1a@example.com/build";

/// Wartości, które MUSZĄ się zapisać. Każda jest tu z powodu, nie dla liczby:
///
/// - `on`, `opus`, `xhigh`, `workspace-write` — zwykłe ustawienia, za krótkie na cokolwiek;
/// - `sandbox_workspace_write.network_access` — 37 znaków, ale kropka cięta na dwa krótsze
///   ciągi i tylko jedna klasa znaków;
/// - SHA gita i UUID — długie, lecz o DWÓCH klasach znaków, nie trzech;
/// - ścieżka z cyframi i wielkimi literami — trzy klasy, więc jedynym, co ją ratuje, jest to,
///   że ukośnik ciągu nie przedłuża. To ta wartość pada, kiedy próg jest ustawiony zbyt nisko;
/// - wiersz komendy z myślnikami i `=`.
const ORDINARY: [&str; 9] = [
    "on",
    "opus",
    "xhigh",
    "workspace-write",
    "sandbox_workspace_write.network_access",
    "e1da96a3d4b5c6d7e8f90123456789abcdef0123",
    "3f0f5f1e-0000-7000-8000-000000000980",
    "/Users/someone/Projects/loadout-h-p8-t157",
    "npm test -- --reporter=dot",
];

/// Flaga, której Loadout nie ustawia i nie ustawi — nośnik dla wartości kontrolnych.
const FREE: &str = "--verbose-tool-output";

// ── fikstury ──────────────────────────────────────────────────────────────────────────────

/// `{"claude": {"<flaga>": "<wartość>"}}` — kształt na drucie, wspólny dla obu nośników.
fn passthrough(flag: &str, value: &str) -> Value {
    json!({ VENDOR: { flag: value } })
}

/// Workflow o jednym kroku agenta, w którym jedyną rzeczą, o którą można się potknąć, jest
/// przelotka. Taki plik zapisuje się dziś czysto.
fn workflow_offering(flag: &str, value: &str) -> Result<WorkflowFile, Box<dyn Error>> {
    let file = json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            {
                "kind": "agent",
                "id": "s1",
                "name": STEP,
                "agent": "a_forge",
                "instructions": "Do the work.",
                "vendorOptions": passthrough(flag, value)
            }
        ],
        "links": []
    });
    Ok(serde_json::from_value(file)?)
}

/// Workflow o jednym kroku „sprawdź". Dowód jest niepusty, więc bez tej reguły plik przechodzi.
fn workflow_running(command: &str) -> Result<WorkflowFile, Box<dyn Error>> {
    let file = json!({
        "format": 1,
        "id": "wf_ship",
        "name": "Ship a feature",
        "steps": [
            {
                "kind": "check",
                "id": "s1",
                "name": CHECK,
                "command": command,
                "proof": "tests passed"
            }
        ],
        "links": []
    });
    Ok(serde_json::from_value(file)?)
}

/// Ten sam wpis przelotki, tyle że w pliku definicji agenta.
fn agent_offering(flag: &str, value: &str) -> Agent {
    let mut flags = std::collections::BTreeMap::new();
    flags.insert(flag.to_owned(), value.to_owned());

    let mut options = VendorOptions::new();
    options.insert(VENDOR.to_owned(), flags);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

/// Zdanie, którym zapis workflowu odmówił — `None`, kiedy plik wylądował.
///
/// `expected` jest rewizją, którą „okno" przeczytało: bez niej drugi zapis tej samej ścieżki
/// odbija się o ochronę przed spóźnionym pisarzem i odmowa mówiłaby o czymś zupełnie innym.
fn refusal_of(
    workflow: &WorkflowFile,
    path: &std::path::Path,
    expected: Option<&str>,
) -> Result<Option<String>, String> {
    match save(workflow, path, expected) {
        Ok(_revision) => Ok(None),
        Err(SaveError::Refused(note)) => Ok(Some(note.message)),
        Err(other) => Err(format!(
            "saving the fixture has to end either as a written file or as a refusal, and this \
             was neither: {other:?}"
        )),
    }
}

// ── kryterium ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_key_in_a_step_option_is_refused_and_the_file_never_appears() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    let said = refusal_of(&workflow_offering(INNOCENT, A_KEY)?, &path, None)?
        .ok_or("a step carrying a literal key saved without a word")?;

    assert!(
        said.contains(INNOCENT),
        "the refusal does not name the line that carries it, so the person has nothing to \
         delete. It read: {said:?}"
    );
    assert!(
        !said.contains(A_KEY),
        "the refusal quotes the value itself, which puts the secret into the window, into the \
         activity of this run and into anything that copies it. Name the line, never the value. \
         It read: {said:?}"
    );
    assert!(
        !path.exists(),
        "the workflow was refused and the file is on disk anyway. Nothing may be written — not \
         partially, not \"without that one line\""
    );
    Ok(())
}

#[test]
fn a_refused_save_leaves_the_previous_bytes_alone() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    // Najpierw czysty zapis tego samego pliku — dopiero on daje czego bronić.
    let revision = save(&workflow_offering(FREE, "on")?, &path, None)?;
    let before = std::fs::read(&path)?;

    let said = refusal_of(&workflow_offering(INNOCENT, A_KEY)?, &path, Some(&revision))?
        .ok_or("overwriting a good file with one that carries a literal key was allowed")?;
    assert!(said.contains(INNOCENT), "it read: {said:?}");

    assert_eq!(
        std::fs::read(&path)?,
        before,
        "the refused save changed the file that was already there. A save that touches the disk \
         before it validates destroys the version the check was defending, byte for byte"
    );
    Ok(())
}

#[test]
fn an_agent_option_carrying_a_key_never_reaches_the_disk() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    // Katalog, którego JESZCZE NIE MA: `write_agent_file` sam go zakłada, więc jego brak po
    // odmowie jest dowodem, że brama stoi przed dotknięciem dysku, a nie za nim.
    let agents = home.path().join("agents");

    let agent = agent_offering(INNOCENT, A_KEY);
    let error = write_agent_file(&agents, &agent, None)
        .err()
        .ok_or("an agent definition carrying a literal key was written to disk")?;

    // `to_string()`, bo dokładnie tę drogę przechodzi zdanie na ekran: `ipc::save_agent` mapuje
    // błąd przez `error.to_string()`, a sekcja Agents pokazuje go w polu `refusal`.
    let said = error.to_string();
    assert!(
        said.contains(INNOCENT),
        "the refusal does not name the line that carries it. It read: {said:?}"
    );
    assert!(
        !said.contains(A_KEY),
        "the refusal quotes the value itself. It read: {said:?}"
    );
    assert!(
        !agents.exists(),
        "the agent was refused and {} exists anyway. Not even the directory may appear",
        agents.display()
    );
    Ok(())
}

#[test]
fn a_password_in_the_address_a_check_runs_is_refused() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    let said = refusal_of(&workflow_running(AN_ADDRESS)?, &path, None)?
        .ok_or("a check step whose command carries a password in a web address saved cleanly")?;

    assert!(
        said.contains(CHECK),
        "the refusal does not name the tile it is about, so clicking it lands nowhere. It read: \
         {said:?}"
    );
    assert!(
        said.contains("command"),
        "the refusal does not say which field carries it. \"Publish\" has three fields a person \
         could look at, and only one of them is the command. It read: {said:?}"
    );
    assert!(
        !said.contains("9f3b7c2e1a"),
        "the refusal quotes the password itself. It read: {said:?}"
    );
    assert!(
        !path.exists(),
        "the workflow was refused and the file is on disk anyway"
    );
    Ok(())
}

#[test]
fn an_ordinary_setting_still_saves() -> Result<(), Box<dyn Error>> {
    for value in ORDINARY {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("ship-a-feature.json");

        assert_eq!(
            refusal_of(&workflow_offering(FREE, value)?, &path, None)?,
            None,
            "an ordinary setting is refused: {value:?}. A false refusal blocks work, which is \
             worse here than no check at all — and this is the assertion an implementation that \
             refuses everything cannot pass"
        );
        assert!(
            path.exists(),
            "the save reported success and there is no file for {value:?}"
        );

        let agents = dir.path().join("agents");
        write_agent_file(&agents, &agent_offering(FREE, value), None).map_err(|error| {
            format!("an agent carrying the same ordinary setting {value:?} is refused: {error}")
        })?;
    }
    Ok(())
}

#[test]
fn an_ordinary_command_still_saves() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    assert_eq!(
        refusal_of(
            &workflow_running("npm test -- --reporter=dot")?,
            &path,
            None
        )?,
        None,
        "an ordinary command is refused, so no check step can be saved at all"
    );
    assert!(
        path.exists(),
        "the save reported success and there is no file"
    );
    Ok(())
}
