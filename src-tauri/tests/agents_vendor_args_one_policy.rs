//! AC-2 dla T-36: obie przelotki czytają TĘ SAMĄ listę, nie dwie kopie.
//!
//! To kryterium istnieje, bo dokładnie tak ta dziura powstała: filtr napisano raz, w jednym
//! miejscu — przy zapisie kroku workflow — a drugie miejsce, przelotka definicji agenta,
//! o nim nie wiedziało. Polityka mieszka w jednym rdzeniu, adaptery mają po pięć linii
//! (niezmiennik 23); druga kopia listy zakazanych flag to sposób, w jaki w repo źródłowym
//! po cichu umarło skanowanie sekretów.
//!
//! **Słabą wersją są dwa osobne testy, każdy ze swoją listą wpisaną ręcznie.** Przechodzą obok
//! siebie w nieskończoność i rozjeżdżają się po cichu — czyli odtwarzają tę samą wadę piętro
//! wyżej, tym razem w wyroczni. Dlatego tutaj jest **jedna pętla po jednej liście** i dwa
//! wywołania na każdy element: ten sam wpis przelotki podawany dwóm drzwiom, obie mają go
//! odmówić. Test pęka w dniu, w którym ktoś doda flagę tylko do jednej kopii.
//!
//! Import stałej jest tu więc treścią kryterium, a nie wygodą — i to jest jedyne miejsce, gdzie
//! tak jest. AC-1 wypisuje te same nazwy wprost, bo tam pytanie brzmi „czy `bypassPermissions`
//! przecieka do argv", i odpowiedź nie ma prawa zależeć od tego, czy lista jest pusta
//! (niezmiennik 20). Tutaj pytanie brzmi „czy to jest jedna lista", więc pusta lista jest
//! osobną awarią i ma osobną asercję: pętla bez ani jednego obrotu jest zielona, a nie
//! sprawdziła niczego.
//!
//! Po stronie workflow wołamy `check()`, a nie `save()`. `save()` odmawia zapisu i to jest
//! kryterium T-12; tutaj porównujemy politykę z polityką, dwie czyste funkcje, bez dysku
//! pomiędzy nimi.

use std::collections::BTreeMap;
use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::library::agents::{Agent, VendorOptions, vendor_args_filtered};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{FORBIDDEN_ESCALATIONS, Level, check};

/// Jedna nazwa vendora po obu stronach. Reguła o dialu jest **niezależna od vendora** — to
/// AC-1 przechodzi po obu aplikacjach; tutaj różnicą między dwoma wywołaniami ma być wyłącznie
/// to, którą przelotkę pytamy.
const VENDOR: &str = "claude";

/// Flaga, która sama w sobie jest legalna i nie stoi na żadnej liście zarezerwowanych. Nosi
/// podniesienie w **wartości**, więc łapie ją tylko reguła o dialu — nie ta o kolizji z flagą,
/// którą Loadout ustawia sam. Dzięki temu obie połówki pętli mierzą tę jedną regułę.
const CARRIER: &str = "--settings";

/// Nazwa kroku w fixture. To ona pada w uwadze, bo to ona stoi na kafelku.
const STEP: &str = "Build";

/// Wpis, którego nie ma i nie będzie na żadnej z list.
const INNOCENT: &str = "--some-new-flag";
const INNOCENT_VALUE: &str = "value";

/// Pusta wartość przy podniesieniu wpisanym jako **nazwa flagi**. Tak wygląda wiersz, którym
/// otwiera się TASK.md — `"--dangerously-skip-permissions": ""` — i tylko połówka reguły
/// patrząca na nazwę flagi go łapie. Bez tego obrotu obie przelotki są tu sprawdzane wyłącznie
/// przez `value.contains`, więc filtr zawężony do samej wartości przechodzi na zielono.
const NO_VALUE: &str = "";

/// `{"claude": {"<flaga>": "<wartość>"}}` — kształt na drucie, wspólny dla obu plików.
fn passthrough(flag: &str, value: &str) -> Value {
    let mut flags = serde_json::Map::new();
    flags.insert(flag.to_owned(), Value::String(value.to_owned()));
    let mut vendors = serde_json::Map::new();
    vendors.insert(VENDOR.to_owned(), Value::Object(flags));
    Value::Object(vendors)
}

/// Workflow o jednym kroku, w którym jedyną rzeczą, o którą można się potknąć, jest przelotka.
fn workflow_that_offers(flag: &str, value: &str) -> Result<WorkflowFile, Box<dyn Error>> {
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

/// Ten sam wpis przelotki, tyle że w definicji agenta.
fn agent_that_offers(flag: &str, value: &str) -> Agent {
    let mut flags = BTreeMap::new();
    flags.insert(flag.to_owned(), value.to_owned());

    let mut options = VendorOptions::new();
    options.insert(VENDOR.to_owned(), flags);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

#[test]
fn every_flag_on_the_one_list_is_refused_by_both_passthroughs() -> Result<(), Box<dyn Error>> {
    assert!(
        !FORBIDDEN_ESCALATIONS.is_empty(),
        "the list of escalations is empty, so the loop below turns zero times and this file \
         proves nothing while reporting a pass. An empty policy is the loudest possible \
         version of the defect this criterion is about"
    );

    for escalation in FORBIDDEN_ESCALATIONS {
        // ── Podniesienie w WARTOŚCI, pod niewinną flagą ───────────────────────────────────
        // ── Pierwsze wywołanie: przelotka KROKU WORKFLOW ──────────────────────────────────
        let notes = check(&workflow_that_offers(CARRIER, escalation)?);
        assert!(
            notes
                .iter()
                .any(|note| note.level == Level::Problem && note.message.contains(escalation)),
            "the workflow step passthrough let `{escalation}` through, or refused it without \
             saying which value it was. Notes: {notes:?}"
        );

        // ── Drugie wywołanie: przelotka DEFINICJI AGENTA, ten sam wpis ────────────────────
        let handed = vendor_args_filtered(&agent_that_offers(CARRIER, escalation), VENDOR);
        assert!(
            handed.args.is_empty(),
            "`{escalation}` reached the argv built from an agent definition. The workflow step \
             refuses this exact entry; a file in ~/.loadout/agents/ carrying it walks past the \
             dial completely (D6). Got: {:?}",
            handed.args
        );
        assert!(
            handed
                .refused
                .iter()
                .any(|refusal| { refusal.flag == CARRIER && refusal.escalation == escalation }),
            "the agent-side refusal does not name `{escalation}` as the reason it dropped \
             `{CARRIER}`. Naming the same word the list holds is what makes this ONE policy \
             rather than two that happen to agree today. Report: {:?}",
            handed.refused
        );

        // ── To samo podniesienie w NAZWIE FLAGI, z pustą wartością ────────────────────────
        // Ta połowa reguły jest osobnym pytaniem: `{"claude": {"--dangerously-skip-permissions":
        // ""}}` omija dial bez wpisania ani jednego znaku w wartość. Obie przelotki czytają tę
        // samą listę TYM SAMYM sposobem albo nie czytają jej tym samym sposobem — a jedna lista
        // pod dwiema różnymi regułami to znowu dwie polityki, tyle że gorzej ukryte.

        // ── Trzecie wywołanie: przelotka KROKU WORKFLOW ───────────────────────────────────
        let notes = check(&workflow_that_offers(escalation, NO_VALUE)?);
        assert!(
            notes
                .iter()
                .any(|note| note.level == Level::Problem && note.message.contains(escalation)),
            "the workflow step passthrough let `{escalation}` through as the flag NAME, or \
             refused it without saying which flag it was. A raise written as the key needs no \
             value at all. Notes: {notes:?}"
        );

        // ── Czwarte wywołanie: przelotka DEFINICJI AGENTA, ten sam wpis ───────────────────
        let handed = vendor_args_filtered(&agent_that_offers(escalation, NO_VALUE), VENDOR);
        assert!(
            handed.args.is_empty(),
            "`{escalation}` reached the argv built from an agent definition when it stood as the \
             flag name. This is the literal line D6 is about, and it carries no value for a \
             value-only filter to catch. Got: {:?}",
            handed.args
        );
        assert!(
            handed
                .refused
                .iter()
                .any(|refusal| { refusal.flag == escalation && refusal.escalation == escalation }),
            "the agent-side refusal does not name `{escalation}` as both the row to delete and \
             the reason it went. Report: {:?}",
            handed.refused
        );
    }
    Ok(())
}

#[test]
fn a_flag_on_neither_list_is_accepted_by_both_passthroughs() -> Result<(), Box<dyn Error>> {
    // Bez tej połówki pętla wyżej przechodzi dla filtra, który odrzuca WSZYSTKO — czyli kasuje
    // całą przelotkę i nadal świeci na zielono. Przelotka istnieje po to, żeby flaga ogłoszona
    // rano była do użycia po południu, bez wydania Loadouta (D6).
    let notes = check(&workflow_that_offers(INNOCENT, INNOCENT_VALUE)?);
    assert!(
        !notes.iter().any(|note| note.level == Level::Problem),
        "a flag Loadout has never heard of blocks a workflow. Notes: {notes:?}"
    );

    let handed = vendor_args_filtered(&agent_that_offers(INNOCENT, INNOCENT_VALUE), VENDOR);
    assert_eq!(
        handed.args,
        [INNOCENT, INNOCENT_VALUE],
        "the same flag has to reach argv from the agent definition, with its value next to it. \
         Got: {:?}",
        handed.args
    );
    assert!(
        handed.refused.is_empty(),
        "and nothing was refused, so there is nothing to tell the user about. Report: {:?}",
        handed.refused
    );
    Ok(())
}
