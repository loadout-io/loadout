//! AC-8 dla T-12: przelotka nie może nadpisać flagi, którą Loadout ustawia sam.
//!
//! Przelotka istnieje po to, żeby nowa flaga vendora była wpisem w formularzu tego samego dnia,
//! w którym vendor ją ogłosi — a nie wydaniem Loadouta (D6). Ma jednak dwie granice i obie są
//! **przy zapisie**: kolizja z flagą, którą ustawiamy sami, oraz próba podniesienia dialu „co
//! agent może zrobić z plikami". Cicha wygrana którejkolwiek strony jest gorsza niż odmowa:
//! `--output-format` podany dwa razy to bieg, w którym strumień zdarzeń nagle nie jest
//! strumieniem zdarzeń, i nikt nie wie dlaczego.
//!
//! Słabą wersją jest „zapis się nie udał". Przechodzi ją implementacja, która odrzuca **każdy**
//! workflow z niepustą przelotką — czyli kasuje całą funkcję i nadal świeci na zielono.
//! Rozróżnia wyłącznie przypadek pozytywny: legalna, niekolidująca flaga **musi** się zapisać,
//! w tym samym pliku testowym, i musi wrócić z dysku znak w znak.
//!
//! Listy flag są tu wypisane wprost, a nie zaimportowane z `workflow::check`. Import
//! sprawdzałby, że test i kod czytają tę samą stałą — także wtedy, gdy ktoś ją opróżni
//! (niezmiennik 20: test sprawdza zachowanie, nie obecność stringa).
//!
//! Druga granica jest **niezależna od listy** i dlatego ma własne przypadki: `--sandbox` nie
//! jest flagą zarezerwowaną, a `--sandbox danger-full-access` omija dial dokładnie tak samo
//! jak `-s`.

use std::error::Error;
use std::fs;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};

/// Flagi, które Loadout ustawia sam, wołając `claude` (ARCHITECTURE §6b).
const RESERVED_CLAUDE: [&str; 7] = [
    "--session-id",
    "--output-format",
    "--input-format",
    "--verbose",
    "--permission-mode",
    "--strict-mcp-config",
    "--setting-sources",
];

/// To samo dla `codex`.
const RESERVED_CODEX: [&str; 3] = ["-C", "-s", "--json"];

/// Nazwa jedynego kroku w każdej fixture — to ona ma paść w komunikacie, bo to ona stoi na
/// kafelku.
const STEP: &str = "Build";

/// `{"<vendor>": {"<flag>": "<value>"}}`.
fn passthrough(vendor: &str, flag: &str, value: &str) -> Value {
    let mut flags = serde_json::Map::new();
    flags.insert(flag.to_owned(), Value::String(value.to_owned()));
    let mut vendors = serde_json::Map::new();
    vendors.insert(vendor.to_owned(), Value::Object(flags));
    Value::Object(vendors)
}

/// Workflow z jednym krokiem i podaną przelotką. Poza przelotką nie ma w nim nic, o co można
/// się potknąć: jeden krok, jedna kopia, folder projektu.
fn workflow_with(vendor_options: Value) -> Result<WorkflowFile, Box<dyn Error>> {
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
                "vendorOptions": vendor_options
            }
        ],
        "links": []
    });
    Ok(serde_json::from_value(file)?)
}

/// Zapisuje i wymaga odmowy; zwraca zdanie, które zobaczy użytkownik.
fn refused(vendor_options: Value) -> Result<String, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    let workflow = workflow_with(vendor_options)?;
    let error = save(&workflow, &path)
        .err()
        .ok_or("save() accepted a passthrough it had to refuse")?;

    let description = format!("{error:?}");
    let SaveError::Refused(note) = error else {
        return Err(
            format!("this has to be a refusal, not an I/O failure; got: {description}").into(),
        );
    };
    assert_eq!(
        note.step_id.as_deref(),
        Some("s1"),
        "the note lands on the step whose passthrough it is talking about"
    );
    assert!(
        !path.exists(),
        "a refused save may not leave a file behind: half a workflow on disk is worse than none"
    );
    Ok(note.message)
}

#[test]
fn a_flag_loadout_sets_itself_is_refused_by_name_for_claude() -> Result<(), Box<dyn Error>> {
    for flag in RESERVED_CLAUDE {
        let message = refused(passthrough("claude", flag, "whatever"))?;
        assert!(
            message.contains(flag),
            "the user has to be told which entry to delete; a refusal that does not name the \
             flag sends them to guess. Refusing {flag} reads: {message}"
        );
        assert!(
            message.contains(STEP),
            "and which step it is in — a workflow has many. Refusing {flag} reads: {message}"
        );
    }
    Ok(())
}

#[test]
fn a_flag_loadout_sets_itself_is_refused_by_name_for_codex() -> Result<(), Box<dyn Error>> {
    for flag in RESERVED_CODEX {
        let message = refused(passthrough("codex", flag, "whatever"))?;
        assert!(
            message.contains(flag),
            "`{flag}` is ours to set, so it has to be named when it is refused. It reads: \
             {message}"
        );
        assert!(
            message.contains(STEP),
            "and the step has to be named too. Refusing {flag} reads: {message}"
        );
    }
    Ok(())
}

#[test]
fn the_passthrough_may_not_raise_what_an_agent_can_do_to_files() -> Result<(), Box<dyn Error>> {
    let sandbox = refused(passthrough("codex", "--sandbox", "danger-full-access"))?;
    assert!(
        sandbox.contains("danger-full-access"),
        "`--sandbox` is not on any reserved list, so only a rule that reads the VALUE catches \
         this — and it has to say which value. It reads: {sandbox}"
    );
    assert!(
        sandbox.contains(STEP),
        "named step, as everywhere else. It reads: {sandbox}"
    );

    let settings = refused(passthrough("claude", "--settings", "bypassPermissions"))?;
    assert!(
        settings.contains("bypassPermissions"),
        "the same rule from the other vendor's side: what an agent may do with your files is \
         set in one place and the passthrough is not it. It reads: {settings}"
    );
    Ok(())
}

#[test]
fn a_new_vendor_flag_that_collides_with_nothing_saves_normally() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    let mut options = passthrough("claude", "--some-new-flag", "value");
    options["codex"] = json!({ "model_reasoning_summary": "detailed" });

    save(&workflow_with(options.clone())?, &path)?;

    let written: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    assert_eq!(
        written["steps"][0]["vendorOptions"], options,
        "this is the assertion an implementation that refuses every passthrough cannot pass — \
         and the passthrough exists so that a flag announced this morning is usable this \
         afternoon, without a release of Loadout"
    );
    Ok(())
}
