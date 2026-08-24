//! AC-1 dla T-98: każda flaga, którą Loadout ustawia sam u Claude'a, jest odmową przelotki.
//!
//! # Po co to istnieje
//!
//! `RESERVED_CLAUDE` ma dziś osiem pozycji, a `ClaudeDriver::command` składa ich dwa razy tyle.
//! Do T-90 ta luka nie miała skutku, bo przelotka `vendorOptions` nie dojeżdżała do argv w ogóle
//! — filtr istniał, walidator o nim wiedział, a proces nie widział ani jednej flagi. Od T-90
//! dojeżdża, więc każda pozycja, której na liście brakuje, jest **cichą wygraną jednej ze
//! stron**: `--settings <własny plik>` podmienia nośnik, którym T-92 wnosi reguły `deny`
//! gospodarza, `--tools` rozszerza białą listę dostępności, a `--model` przestawia model spod
//! ręki człowieka, który wybrał inny w formularzu. To jest dokładnie to, czego zakazuje D6:
//! przelotka nie omija diala.
//!
//! # Dwie połowy, i dopiero razem coś znaczą
//!
//! **(1) Pokrycie mierzone na GOTOWEJ KOMENDZIE, nie na drugiej kopii listy.** Wyrocznia, która
//! wypisuje oczekiwane nazwy z palca, sprawdza wyłącznie, czy dwa napisy się zgadzają — i milczy
//! w dniu, w którym sterownik dokłada dziewiętnastą flagę. Dlatego pytamy `ClaudeDriver::command`
//! o argv jednej prawdziwej tury, ze wszystkim, co Loadout do niej wnosi (plik ustawień biegu,
//! katalog pluginu, plik serwerów, sufit wydatku, model, dopisek systemowy, katalog przekazań),
//! i żądamy, żeby **każda** nazwa z tego argv była odmową przelotki. Fragmenty, które `command`
//! tylko przenosi, biorą się tu z ich PRAWDZIWYCH producentów (`budget_argv`, `plugin_argv`,
//! `connections::runtime::for_driver`) — wpisane z palca dowodziłyby wyłącznie tego, że test
//! umie napisać flagę.
//!
//! **(2) Próbka z TASK.md, sprawdzana po nazwie.** Pokrycie z (1) łapie tylko to, co sterownik
//! składa dzisiaj; `--continue`, `--agents`, `--disallowedTools` i `--permission-prompt-tool`
//! Loadout ustawia sam dopiero potencjalnie, a przelotka może ich użyć już teraz. Te nazwy stoją
//! więc wypisane wprost — to jedyne miejsce w tym pliku, gdzie lista jest przepisana, i jest
//! przepisana z prozy zadania, a nie z kodu, który ma sądzić.
//!
//! # Słabe wersje, które ten plik odrzuca
//!
//! **„Zapis się nie udał".** Przechodzi ją implementacja odmawiająca KAŻDEJ niepustej przelotki
//! — czyli kasująca całą funkcję i świecąca na zielono. Rozróżnia to przypadek pozytywny niżej:
//! flaga, o której Loadout nigdy nie słyszał, musi przejść i dojechać do argv razem z wartością.
//!
//! **`assert!(RESERVED_CLAUDE.contains(&flag))`.** Sprawdza obecność napisu w stałej, a nie
//! zachowanie (niezmiennik 20), i przechodzi dla listy, której żadna odmowa nie czyta. Dlatego
//! pytamy o **zdania, które człowiek naprawdę zobaczy**: uwagę z zapisu kroku i zdanie, którym
//! bieg odmawia startu (niezmiennik 29). Dwoje drzwi, bo przelotka ma dwa nośniki — kafelek
//! workflow i plik `~/.loadout/agents/*.json` — i jedna lista, która ma zamykać oba naraz.
//!
//! **Dopasowanie po podciągu.** Kontrolą jest `--verbose-tool-output`: flaga, która zaczyna się
//! od zarezerwowanego `--verbose` i **nie jest** nim. Filtr pytający `starts_with` albo
//! `contains` zabija tu flagę ogłoszoną dziś rano — czyli dokładnie to, po co przelotka istnieje.

use std::error::Error;
use std::path::PathBuf;

use serde_json::{Value, json};

use loadout_lib::connections::runtime;
use loadout_lib::connections::{Connection, Transport};
use loadout_lib::engine::drivers::claude::{ClaudeDriver, RunSettings, budget_argv};
use loadout_lib::engine::drivers::{Policy, RunSpec, SessionRef};
use loadout_lib::inherit::Rewritten;
use loadout_lib::inherit::rewrite::plugin_argv;
use loadout_lib::library::agents::{Agent, VendorOptions, passthrough_refused, vendor_args};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};
use uuid::Uuid;

/// Nazwa vendora w przelotce — ta sama, którą pyta `workflow::check::reserved`.
const VENDOR: &str = "claude";

/// Nazwa jedynego kroku w fikstyrze. To ona stoi na kafelku, więc to ona pada w uwadze.
const STEP: &str = "Build";

/// Wartość doklejana do sprawdzanej flagi. Neutralna z rozmysłem: gdyby niosła podniesienie,
/// każda odmowa dałaby się wytłumaczyć regułą o dialu i lista zarezerwowanych mogłaby zostać
/// pusta.
const HARMLESS: &str = "whatever";

/// Flaga, której Loadout nie ustawia i nie ustawi — **kontrola całego pliku**.
///
/// Wybrana tak, żeby przewracała filtr dopasowujący po początku napisu: `--verbose` JEST
/// zarezerwowane (bez niego CLI odmawia startu), a `--verbose-tool-output` jest inną flagą tego
/// samego vendora. Odmowa tutaj znaczy, że flaga ogłoszona dziś rano nie jest do użycia po
/// południu — czyli że przelotki nie ma (D6).
const FREE: &str = "--verbose-tool-output";
const FREE_VALUE: &str = "on";

/// Nazwy wymienione w AC-1 zadania: dotychczasowe osiem plus czternaście dopisywanych.
///
/// Przepisane z PROZY ZADANIA, nie z `workflow::check` — import sprawdzałby, że test i kod
/// czytają tę samą stałą, także wtedy, gdy ktoś ją opróżni (niezmiennik 20). Pokrycie tego,
/// co sterownik składa naprawdę, mierzy osobny test niżej i tam żadna nazwa nie jest przepisana.
const NAMED_IN_THE_TASK: [&str; 22] = [
    // ── osiem, które lista ma dziś ────────────────────────────────────────────────────────
    "--session-id",
    "--output-format",
    "--input-format",
    "--verbose",
    "--permission-mode",
    "--strict-mcp-config",
    "--setting-sources",
    "--effort",
    // ── czternaście, których jej brakuje ──────────────────────────────────────────────────
    "--settings",
    "--add-dir",
    "--mcp-config",
    "--plugin-dir",
    "--tools",
    "--allowedTools",
    "--disallowedTools",
    "--append-system-prompt",
    "--model",
    "--max-budget-usd",
    "--resume",
    "--continue",
    "--agents",
    "--permission-prompt-tool",
];

/// Reguła `deny`, która ma trafić do pliku ustawień biegu. Treść jest tu bez znaczenia — chodzi
/// wyłącznie o to, żeby `--settings` w ogóle powstało.
const DENY_RULE: &str = "Read(LOADOUT-T98-MARKER/**)";

/// Nazwa zatwierdzonego połączenia, z którego bierze się `--mcp-config`.
const SERVER: &str = "x";

// ── co Loadout naprawdę wysyła ────────────────────────────────────────────────────────────

/// `RunSpec` z KAŻDYM polem, które dokłada flagę: model, dopisek systemowy, katalog przekazań
/// i sieć. Pola puste znaczą tu „flagi nie będzie", więc `RunSpec` domyślny mierzyłby połowę
/// wiersza argv.
fn spec(resume: Option<SessionRef>) -> RunSpec {
    RunSpec {
        run_id: Uuid::from_u128(0x0199_0000_0000_7000_8000_0000_0000_0980),
        cwd: PathBuf::from("."),
        prompt: "rename the widget".to_owned(),
        model: Some("opus".to_owned()),
        system_append: Some("Answer in English.".to_owned()),
        policy: Policy::EditInFolder,
        reaches_the_web: true,
        tools: None,
        extra_dirs: vec![PathBuf::from("handoffs")],
        resume,
    }
}

/// Nazwy flag z argv jednej prawdziwej tury — **zebrane z gotowej komendy**, nie wypisane.
///
/// Za flagę uznajemy każdy argument zaczynający się od myślnika i to jest reguła, nie skrót:
/// żadna wartość w tej fikstyrze myślnikiem się nie zaczyna (ścieżki, uuid, `opus`, listy
/// narzędzi, kwota, zdanie po angielsku), a `--setting-sources` niesie argument o ZEROWEJ
/// długości, który tym samym też nie wpada.
///
/// Dwa przebiegi, bo `--session-id` i `--resume` wykluczają się wzajemnie — dokładnie jedno
/// z dwóch, nigdy oba. Jeden przebieg nie zobaczyłby drugiej z tych flag nigdy.
fn flags_loadout_hands_the_app() -> Result<Vec<String>, Box<dyn Error>> {
    let run = tempfile::tempdir()?;

    let mut names: Vec<String> = Vec::new();
    for resume in [
        None,
        Some(SessionRef {
            vendor: "claude",
            id: "3f0f5f1e-0000-7000-8000-000000000980".to_owned(),
        }),
    ] {
        // Plik ustawień biegu piszemy naprawdę: `--settings` bez pliku pod podaną ścieżką zabija
        // CLI dopiero w produkcji, więc sterownik stawia tę flagę wyłącznie razem z nim.
        let settings = RunSettings::write(run.path(), &[DENY_RULE.to_owned()])?;

        // `--mcp-config` z producenta, który je składa naprawdę: plik serwerów powstaje na dysku,
        // a nazwa flagi jest faktem o vendorze i mieszka w `connections::runtime`.
        let connection = Connection::imported(
            SERVER.to_owned(),
            SERVER.to_owned(),
            Transport::Stdio {
                command: "x-server".to_owned(),
                args: vec!["--stdio".to_owned()],
                environment: Vec::new(),
            },
            run.path().join("mcp.json"),
            "0000".to_owned(),
        );
        let mut configuration = runtime::for_driver(run.path(), VENDOR, &[connection], |_| None)?;
        // …i sufit wydatku, tą samą drogą: `DriverConfiguration::arguments`.
        configuration.arguments.extend(budget_argv(5.0));

        // `--plugin-dir`, też z prawdziwego kompozytora. Fragment jest dwuelementowy albo pusty,
        // więc niepusta lista nazw jest tu warunkiem, nie ozdobą.
        let inherited = plugin_argv(&Rewritten {
            dir: run.path().join("plugin"),
            names: vec!["alpha".to_owned()],
        });

        let command = ClaudeDriver::new()
            .with_settings(settings)
            .with_inherited(inherited)
            .with_configuration(configuration)
            .command(&spec(resume));

        for argument in command.as_std().get_args() {
            let text = argument.to_string_lossy().into_owned();
            if text.starts_with('-') && !names.contains(&text) {
                names.push(text);
            }
        }
    }
    Ok(names)
}

// ── co Loadout mówi człowiekowi ───────────────────────────────────────────────────────────

/// Zdania, które ten jeden wpis przelotki wywołuje w obu miejscach, gdzie człowiek je czyta.
struct Doors {
    /// Uwaga z zapisu kafelka — `None`, kiedy workflow zapisał się bez słowa.
    on_save: Option<String>,
    /// Zdania, którymi bieg odmawia startu definicji agenta. Puste, kiedy nie odmawia.
    on_plan: Vec<String>,
}

impl Doors {
    fn names_on_save(&self, flag: &str) -> bool {
        self.on_save
            .as_deref()
            .is_some_and(|said| said.contains(flag))
    }

    fn names_on_plan(&self, flag: &str) -> bool {
        self.on_plan.iter().any(|said| said.contains(flag))
    }

    fn all(&self) -> Vec<&str> {
        self.on_save
            .iter()
            .chain(&self.on_plan)
            .map(String::as_str)
            .collect()
    }
}

/// `{"claude": {"<flaga>": "<wartość>"}}` — kształt na drucie, wspólny dla obu nośników.
fn passthrough(flag: &str, value: &str) -> Value {
    let mut flags = serde_json::Map::new();
    flags.insert(flag.to_owned(), Value::String(value.to_owned()));
    let mut vendors = serde_json::Map::new();
    vendors.insert(VENDOR.to_owned(), Value::Object(flags));
    Value::Object(vendors)
}

/// Workflow o jednym kroku, w którym jedyną rzeczą, o którą można się potknąć, jest przelotka.
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

/// Podaje ten wpis obu drzwiom i oddaje, co za każdymi powiedziano.
fn what_loadout_says(flag: &str, value: &str) -> Result<Doors, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    let on_save = match save(&workflow_offering(flag, value)?, &path) {
        Ok(()) => None,
        Err(SaveError::Refused(note)) => Some(note.message),
        Err(other) => {
            return Err(format!(
                "saving the fixture has to end either as a written file or as a refusal, and \
                 this was neither: {other:?}"
            )
            .into());
        }
    };

    Ok(Doors {
        on_save,
        on_plan: passthrough_refused(&agent_offering(flag, value)),
    })
}

// ── kryterium ─────────────────────────────────────────────────────────────────────────────

#[test]
fn every_flag_the_command_builder_sets_is_refused_in_the_passthrough() -> Result<(), Box<dyn Error>>
{
    let flags = flags_loadout_hands_the_app()?;

    // Pętla bez ani jednego obrotu jest zielona i nie sprawdziła niczego. Liczba jest dolną
    // granicą, nie pomiarem: transport sam wnosi sześć argumentów, a wiersz z izolacją, sesją,
    // uprawnieniami i narzędziami dokłada resztę.
    assert!(
        flags.len() >= 10,
        "the command built for one real turn carries {} named argument(s): {flags:?}. Either the \
         driver stopped setting them or this fixture stopped switching them on — and with an \
         empty list the loop below turns zero times and reports a pass",
        flags.len()
    );

    for flag in &flags {
        let doors = what_loadout_says(flag, HARMLESS)?;
        assert!(
            doors.names_on_save(flag),
            "Loadout hands the agent app `{flag}` itself, but a step writing the same thing \
             saves without naming the line to delete: {:?}. The reserved list is judged here \
             against the command that is really built, so a flag added to the driver without \
             a line on that list shows up as this failure",
            doors.on_save
        );
        assert!(
            doors.names_on_plan(flag),
            "Loadout hands the agent app `{flag}` itself, but an agent definition writing the \
             same thing reaches Start or is refused without naming the line: {:?}. Both ways \
             of carrying extra settings must close independently",
            doors.on_plan
        );
    }
    Ok(())
}

#[test]
fn the_names_this_task_lists_are_refused_at_the_save_and_at_the_plan() -> Result<(), Box<dyn Error>>
{
    for flag in NAMED_IN_THE_TASK {
        let doors = what_loadout_says(flag, HARMLESS)?;

        // Dwoje drzwi z osobna, bo przelotka ma dwa nośniki i jedna lista ma zamykać oba naraz.
        // Wpis zamknięty tylko po jednej stronie jest tą samą dziurą o jeden plik dalej: kafelek
        // odmawia, a ten sam wiersz w `~/.loadout/agents/*.json` przechodzi.
        let named_on_save = doors.names_on_save(flag);
        assert!(
            named_on_save,
            "a step writing `{flag}` into its extra settings saves without a word, or is refused \
             without being told which line to delete. Loadout sets that argument itself, so the \
             person has to learn it here — at the save — and not from a run that behaves oddly \
             three steps later. It read: {:?}",
            doors.on_save
        );

        let named_on_plan = doors.names_on_plan(flag);
        assert!(
            named_on_plan,
            "the same line inside an agent definition starts the run anyway, or stops it without \
             naming `{flag}`. A silent drop teaches the person that the passthrough does not \
             work — so they write the same thing again in another spelling — instead of that it \
             was blocked. It read: {:?}",
            doors.on_plan
        );
    }
    Ok(())
}

#[test]
fn a_flag_loadout_never_sets_still_goes_through_with_its_value() -> Result<(), Box<dyn Error>> {
    let doors = what_loadout_says(FREE, FREE_VALUE)?;

    assert!(
        doors.on_save.is_none(),
        "a flag Loadout has never heard of blocks the save: {:?}. `{FREE}` begins with \
         `--verbose`, which IS ours to set, and is a different argument of the same app — a rule \
         matching by the start of the text kills it. The passthrough exists so that an argument \
         announced this morning is usable this afternoon, without a release of Loadout (D6)",
        doors.on_save
    );
    assert!(
        doors.on_plan.is_empty(),
        "and the run refuses to start over it: {:?}. This is the assertion an implementation \
         that refuses every passthrough cannot pass",
        doors.on_plan
    );

    assert_eq!(
        vendor_args(&agent_offering(FREE, FREE_VALUE), VENDOR),
        [FREE, FREE_VALUE],
        "it survives the filter but does not reach the arguments with its value beside it. A key \
         without its value swallows the next argument as its own, which is worse than dropping \
         both"
    );
    Ok(())
}
