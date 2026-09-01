//! AC-2 dla T-98: klucze `-c`, którymi Codex podnosi uprawnienia, są odmową — **po prefiksie**.
//!
//! # Po co to istnieje
//!
//! `RESERVED_CODEX` ma cztery pozycje (`-C`, `-s`, `--json`, `model_reasoning_effort`) i porównuje
//! klucz przez równość. Przelotka Codeksa jedzie do argv jako `-c klucz=wartość`
//! (`library::agents::vendor_argv`), a ten vendor przyjmuje tą drogą **całą swoją konfigurację**.
//! Zmierzone na trunku 2026-08-24, wszystkie przechodzą:
//!
//! - `-c sandbox_mode=workspace-write` — podniesienie z „look only" bez tknięcia diala. Filtr
//!   podniesień zna wyłącznie literał `danger-full-access`, a to jest inna wartość tego samego
//!   ustawienia;
//! - `-c sandbox_workspace_write.network_access=true` — sieć włączona z pominięciem pola, które
//!   ją włącza (`RunSpec::reaches_the_web`);
//! - `-c approval_policy=never` — nikt już o nic nie pyta;
//! - `-c mcp_servers.x.command=/bin/sh` — dowolny proces uruchomiony przez agenta jako „serwer
//!   narzędziowy", obok listy zatwierdzonych Connections;
//! - `-c model_provider=…` i `-c model_providers.custom.base_url=…` — cały ruch, razem z promptem,
//!   przekierowany pod cudzy adres. To jest eksfiltracja zapisana jako ustawienie.
//!
//! # Dlaczego po PREFIKSIE, a nie po równości
//!
//! Bo klucz nie jest jeden. Rodziny `mcp_servers.*` i `model_providers.*` mają w środku nazwę,
//! którą wpisuje człowiek (`.command`, `.args`, `.env`, `.base_url` po niej), a lista
//! równościowa musiałaby znać ją z góry — czyli nie istnieje. Prefiks jest tu jedynym
//! kształtem, który zamyka rodzinę,
//! i dlatego kryterium sądzi go na kluczu Z NAZWĄ W ŚRODKU (`mcp_servers.x.command`), a nie na
//! samym prefiksie.
//!
//! # Słabe wersje, które ten plik odrzuca
//!
//! **„Zapis się nie udał".** Przechodzi ją implementacja odrzucająca każdą przelotkę Codeksa.
//! Rozróżnia to kontrola: `profile=ci` musi się zapisać i dojechać do argv jako `-c profile=ci`,
//! w tym kształcie i z tą wartością.
//!
//! **Prefiks pożerający sąsiadów.** `model_provider` jako prefiks nie ma prawa zjeść
//! `model_verbosity` ani `model_reasoning_summary` — obie są zwykłymi ustawieniami tego vendora
//! i obie stoją niżej jako kontrola. Klucz `model_reasoning_effort` zostaje odmową jak dziś,
//! bo od T-91 ustawia go sam Loadout z pola „ile myśleć"; prefiks `model_provider` go nie łapie,
//! więc pozycja równościowa musi na liście zostać.
//!
//! **Pytanie zadane samej funkcji.** Odmowa dowodzona na wartości zwróconej przez filtr
//! przechodzi także wtedy, gdy nikt tej funkcji nie woła (niezmiennik 29). Dlatego pytamy
//! o zdania, które człowiek czyta: uwagę z zapisu kafelka i zdanie odmowy startu.

use std::collections::BTreeMap;
use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::library::agents::{Agent, VendorOptions, passthrough_refused, vendor_argv};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};

/// Nazwa vendora w przelotce.
const VENDOR: &str = "codex";

/// Nazwa jedynego kroku w fikstyrze — to ona stoi na kafelku.
const STEP: &str = "Build";

/// Klucze, którymi ten vendor podnosi sobie uprawnienia, razem z wartością, którą naprawdę by
/// im nadano. Wartości są tu prawdziwe z rozmysłem: żadna z nich nie niesie
/// `danger-full-access`, więc żadnej nie ratuje filtr podniesień — łapie je wyłącznie lista
/// kluczy, czyli to, co sądzi to kryterium.
const RAISES: [(&str, &str); 6] = [
    ("sandbox_mode", "workspace-write"),
    ("sandbox_workspace_write.network_access", "true"),
    ("approval_policy", "never"),
    // NAZWA SERWERA W ŚRODKU KLUCZA — to jest cały powód, dla którego dopasowanie idzie po
    // prefiksie. Lista równościowa musiałaby znać `x` z góry.
    ("mcp_servers.x.command", "/bin/sh"),
    ("model_provider", "elsewhere"),
    (
        "model_providers.custom.base_url",
        "https://example.invalid/v1",
    ),
];

/// Klucz, który Loadout ustawia sam od T-91 i który **zostaje** odmową. Prefiks `model_provider`
/// go nie łapie, więc jego pozycja na liście jest osobna i musi przetrwać przepisanie reguły.
const STILL_REFUSED: (&str, &str) = ("model_reasoning_effort", "high");

/// Ustawienia tego vendora, których Loadout nie tyka i tknąć nie ma prawa.
///
/// `model_verbosity` i `model_reasoning_summary` sąsiadują alfabetycznie z `model_provider`
/// i z `model_reasoning_effort` — filtr dopasowujący „coś, co zaczyna się od `model`" zabija
/// oba, a przelotka istnieje po to, żeby ustawienie ogłoszone dziś rano było do użycia po
/// południu (D6).
const FREE: [(&str, &str); 3] = [
    ("profile", "ci"),
    ("model_verbosity", "high"),
    ("model_reasoning_summary", "detailed"),
];

// ── co Loadout mówi człowiekowi ───────────────────────────────────────────────────────────

/// Zdania, które ten jeden wpis przelotki wywołuje w obu miejscach, gdzie człowiek je czyta.
struct Doors {
    /// Uwaga z zapisu kafelka — `None`, kiedy workflow zapisał się bez słowa.
    on_save: Option<String>,
    /// Zdania, którymi bieg odmawia startu definicji agenta.
    on_plan: Vec<String>,
}

impl Doors {
    fn all(&self) -> Vec<&str> {
        self.on_save
            .iter()
            .chain(&self.on_plan)
            .map(String::as_str)
            .collect()
    }
}

/// `{"codex": {"<klucz>": "<wartość>"}}`.
fn passthrough(key: &str, value: &str) -> Value {
    let mut keys = serde_json::Map::new();
    keys.insert(key.to_owned(), Value::String(value.to_owned()));
    let mut vendors = serde_json::Map::new();
    vendors.insert(VENDOR.to_owned(), Value::Object(keys));
    Value::Object(vendors)
}

fn workflow_offering(key: &str, value: &str) -> Result<WorkflowFile, Box<dyn Error>> {
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
                "vendorOptions": passthrough(key, value)
            }
        ],
        "links": []
    });
    Ok(serde_json::from_value(file)?)
}

fn agent_offering(key: &str, value: &str) -> Agent {
    let mut keys = BTreeMap::new();
    keys.insert(key.to_owned(), value.to_owned());

    let mut options = VendorOptions::new();
    options.insert(VENDOR.to_owned(), keys);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

fn what_loadout_says(key: &str, value: &str) -> Result<Doors, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    // `None`: pliku jeszcze nie ma. Rewizja zapisu nie interesuje tego kryterium — pyta ono
    // wyłącznie o to, czy walidator odmówił i jakim zdaniem.
    let on_save = match save(&workflow_offering(key, value)?, &path, None) {
        Ok(_revision) => None,
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
        on_plan: passthrough_refused(&agent_offering(key, value)),
    })
}

// ── kryterium ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_key_that_raises_what_the_agent_may_do_is_refused_and_named() -> Result<(), Box<dyn Error>> {
    for (key, value) in RAISES.into_iter().chain([STILL_REFUSED]) {
        let doors = what_loadout_says(key, value)?;

        let on_save = doors
            .on_save
            .as_deref()
            .is_some_and(|said| said.contains(key));
        assert!(
            on_save,
            "a step writing `{key}={value}` into its extra settings for the other agent app saves \
             without a word, or is refused without naming the line to delete. That app takes its \
             whole configuration this way, so an extra setting written here is not an extra \
             setting — it is the dial, set from the side. It read: {:?}",
            doors.on_save
        );

        let on_plan = doors.on_plan.iter().any(|said| said.contains(key));
        assert!(
            on_plan,
            "the same line inside an agent definition starts the run anyway, or stops it without \
             naming `{key}`. A file in ~/.loadout/agents/ carrying it walks past the dial \
             completely (D6), and a refusal that names nothing leaves the person with nothing to \
             delete. It read: {:?}",
            doors.on_plan
        );
    }
    Ok(())
}

#[test]
fn the_family_is_closed_by_its_prefix_not_by_the_one_name_we_thought_of()
-> Result<(), Box<dyn Error>> {
    // Nazwy w środku powstają mechanicznie w czasie testu. Kilkadziesiąt różnych członów
    // odróżnia prawdziwą regułę prefiksową od listy dokładnych przykładów zaszytych pod test.
    for number in 0..16 {
        let name = format!("loadout_t98_{number:02x}");
        for key in [
            format!("mcp_servers.{name}.command"),
            format!("mcp_servers.{name}.args"),
            format!("model_providers.{name}.base_url"),
            format!("model_providers.{name}.env_key"),
        ] {
            let doors = what_loadout_says(&key, "whatever")?;
            let on_save = doors
                .on_save
                .as_deref()
                .is_some_and(|said| said.contains(&key));
            let on_plan = doors.on_plan.iter().any(|said| said.contains(&key));
            assert!(
                on_save && on_plan,
                "`{key}` was accepted through at least one way of carrying extra settings. Its \
                 middle part is chosen by the person, so a list of exact names cannot close the \
                 family. Loadout said: {:?}",
                doors.all()
            );
        }
    }
    Ok(())
}

#[test]
fn a_setting_loadout_never_touches_still_reaches_the_app_in_its_own_shape()
-> Result<(), Box<dyn Error>> {
    for (key, value) in FREE {
        let doors = what_loadout_says(key, value)?;

        assert!(
            doors.on_save.is_none(),
            "`{key}` blocks the save: {:?}. It is a plain setting of that app, and it sits next \
             to a reserved one in the alphabet — a rule matching by the start of the text takes \
             both. The passthrough exists so that a setting announced this morning is usable this \
             afternoon, without a release of Loadout (D6)",
            doors.on_save
        );
        assert!(
            doors.on_plan.is_empty(),
            "`{key}` stops the run: {:?}. This is the assertion an implementation that refuses \
             every passthrough of this app cannot pass",
            doors.on_plan
        );

        // I dojeżdża w kształcie, którym mówi TEN vendor: opcja globalna `-c klucz=wartość`,
        // nie para „flaga wartość" z drugiej aplikacji.
        assert_eq!(
            vendor_argv(&agent_offering(key, value), VENDOR),
            ["-c".to_owned(), format!("{key}={value}")],
            "`{key}` survives the filter and does not reach the arguments in the shape this app \
             takes them"
        );
    }
    Ok(())
}
