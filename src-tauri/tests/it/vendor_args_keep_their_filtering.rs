//! AC-4 dla T-98: stara wyrocznia stoi na NOWEJ przesłance.
//!
//! # Co się zmienia i dlaczego to jest decyzja, a nie poprawka
//!
//! `agents_vendor_args_filtered.rs` (T-36) używa `--settings` jako **przykładu flagi
//! niezarezerwowanej** — nosi na niej wartość `bypassPermissions`, żeby pokazać, że łapie ją
//! reguła czytająca wartość, a nie lista. Od T-92 tę flagę ustawia sam Loadout: to nią jedzie
//! przepisany `deny` gospodarza i ona wskazuje plik ustawień biegu. Dopisanie jej do listy
//! zarezerwowanych (AC-1) zmienia więc przesłankę tamtego pliku, a nie jego wynik — dlatego
//! `docs/STATUS.md` zostawił to człowiekowi, a mandat właściciela D-1 (2026-08-24) rozstrzygnął:
//! tamten test dostaje inny przykład flagi wolnej, asercje zostają tak samo mocne.
//!
//! # Dlaczego to jest osobne kryterium, a nie dopisek do AC-1
//!
//! Bo zmiana przesłanki ma dokładnie dwa sposoby, żeby wyjść źle, i oba są ciche:
//!
//! 1. **Nowy przykład flagi wolnej okazuje się zarezerwowany.** Wtedy tamten plik przestaje
//!    mierzyć przelotkę i zaczyna mierzyć listę: „niewinna flaga zostaje" pada nie dlatego, że
//!    filtr jest zły, tylko dlatego, że przykład jest zły — a naprawia się to zwykle osłabiając
//!    asercję.
//! 2. **`--settings` ląduje na liście i nikt nie sprawdza, czy odmowa naprawdę pada.** Pozycja
//!    w stałej, której żadne zdanie nie czyta, to jest ta sama martwa kontrolka, o którą chodzi
//!    w niezmienniku 16.
//!
//! Ten plik pyta o obie strony jednym ruchem: przykład wolny ma przejść **wszystkimi** drzwiami,
//! a `--settings` ma być odmową **wszystkimi**. Sądzi zachowanie, nie treść tamtego pliku — plik
//! testowy nie jest wyrocznią dla drugiego pliku testowego (niezmiennik 22).
//!
//! # Trzeci test to kształt starej wyroczni, przepisany na nowe stałe
//!
//! Trzy asercje T-36 dopiero razem coś znaczą i wszystkie trzy stoją niżej: **niewinna flaga
//! zostaje razem ze swoją wartością**, **każde podniesienie znika z osobna, po nazwie**, i
//! **odmowa NAZYWA to, co odrzuciła**. Różnica jest jedna: nośnikiem wartości jest teraz flaga,
//! której Loadout nie ustawia. Jeżeli po zmianie przykładu któraś z tych trzech przestaje
//! działać, to nie przykład był zły — tylko filtr.

use std::collections::BTreeMap;
use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::library::agents::{
    Agent, VendorOptions, passthrough_refused, vendor_args, vendor_args_filtered,
};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};

const VENDOR: &str = "claude";
const STEP: &str = "Build";

/// Nowy przykład flagi wolnej — ten, którym mandat D-1 zastępuje `--settings`.
///
/// Musi spełniać naraz trzy warunki, bo inaczej stara wyrocznia zaczyna mierzyć co innego:
/// nie jest żadną z pozycji AC-1, nie jest podniesieniem, i **nie jest prefiksem ani sufiksem
/// żadnej z nich w sposób, który przeżyje dopasowanie po podciągu** — zaczyna się od
/// zarezerwowanego `--verbose`, więc przechodzi wyłącznie tam, gdzie dopasowanie idzie po kluczu.
const FREE: &str = "--verbose-tool-output";
const FREE_VALUE: &str = "on";

/// Flaga, która przechodzi ze starej wyroczni na stronę ODMÓW.
const SETTINGS: &str = "--settings";
/// Ścieżka pliku, którym podmienia się nośnik reguł `deny` gospodarza. Wartość niewinna
/// z rozmysłu: gdyby niosła podniesienie, odmowę tłumaczyłaby reguła o dialu i pozycja na liście
/// zarezerwowanych mogłaby nie istnieć.
const SETTINGS_VALUE: &str = "mine.json";

/// Podniesienie zapisane jako **wartość** zwykłej flagi.
const BYPASS: &str = "bypassPermissions";
/// Podniesienie zapisane jako **nazwa** flagi, z pustą wartością.
const SKIP_PERMISSIONS: &str = "--dangerously-skip-permissions";

/// Flaga, o której Loadout nigdy nie słyszał i której nie ma prawa tknąć.
const INNOCENT: &str = "--some-new-flag";
const INNOCENT_VALUE: &str = "value";

// ── drzwi, za którymi człowiek to czyta ───────────────────────────────────────────────────

struct Doors {
    on_save: Option<String>,
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

fn passthrough(flag: &str, value: &str) -> Value {
    let mut flags = serde_json::Map::new();
    flags.insert(flag.to_owned(), Value::String(value.to_owned()));
    let mut vendors = serde_json::Map::new();
    vendors.insert(VENDOR.to_owned(), Value::Object(flags));
    Value::Object(vendors)
}

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

/// Agent z dowolną liczbą wpisów przelotki jednego vendora.
fn agent_with(entries: &[(&str, &str)]) -> Agent {
    let mut flags = BTreeMap::new();
    for (flag, value) in entries {
        flags.insert((*flag).to_owned(), (*value).to_owned());
    }

    let mut options = VendorOptions::new();
    options.insert(VENDOR.to_owned(), flags);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

fn what_loadout_says(flag: &str, value: &str) -> Result<Doors, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    // `None`: pliku jeszcze nie ma. Rewizja zapisu nie interesuje tego kryterium — pyta ono
    // wyłącznie o to, czy walidator odmówił i jakim zdaniem.
    let on_save = match save(&workflow_offering(flag, value)?, &path, None) {
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
        on_plan: passthrough_refused(&agent_with(&[(flag, value)])),
    })
}

/// Wartość stojąca **zaraz za** tym kluczem. Sama obecność klucza niczego nie dowodzi: flaga bez
/// wartości połyka następny argument jako swój.
fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|item| item == flag)?;
    args.get(at + 1).map(String::as_str)
}

// ── kryterium ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_old_oracle_runs_in_full() {
    // AC-4 mówi o starej wyroczni. Wołamy jej pięć prawdziwych testów przez jeden moduł,
    // zamiast ładować ten sam plik drugi raz (co full-clippy odrzuca jako duplicate_mod).
    super::agents_vendor_args_filtered::claude_gets_the_harmless_flag_and_not_one_escalation();
    super::agents_vendor_args_filtered::codex_gets_the_harmless_flag_and_not_one_escalation();
    super::agents_vendor_args_filtered::the_refusal_says_which_line_to_delete_and_what_it_tried_to_raise();
    super::agents_vendor_args_filtered::the_plain_argv_builder_is_filtered_too_not_only_its_talking_twin();
    super::agents_vendor_args_filtered::the_example_this_file_leans_on_is_free_and_the_old_one_is_not();
}

#[test]
fn the_flag_the_old_oracle_now_calls_free_really_is_free() -> Result<(), Box<dyn Error>> {
    let doors = what_loadout_says(FREE, FREE_VALUE)?;

    assert!(
        doors.on_save.is_none() && doors.on_plan.is_empty(),
        "`{FREE}` is refused somewhere: {:?}. It is the example the older oracle leans on for \
         \"a flag that raises nothing survives\", so the moment it stops surviving, that whole \
         file measures the list instead of the filter — and the usual repair is to weaken the \
         assertion",
        doors.all()
    );

    let handed = vendor_args_filtered(&agent_with(&[(FREE, FREE_VALUE)]), VENDOR);
    assert_eq!(
        handed.args,
        [FREE, FREE_VALUE],
        "and it has to reach the arguments with its value beside it, and nothing else with it"
    );
    assert!(
        handed.refused.is_empty(),
        "nothing was dropped, so there is nothing to tell the person about: {:?}",
        handed.refused
    );
    Ok(())
}

#[test]
fn the_settings_file_moves_to_the_refusals_side() -> Result<(), Box<dyn Error>> {
    let doors = what_loadout_says(SETTINGS, SETTINGS_VALUE)?;

    assert!(
        doors
            .on_save
            .as_deref()
            .is_some_and(|said| said.contains(SETTINGS)),
        "a step handing the agent app its own `{SETTINGS}` saves without a word, or is refused \
         without naming the line: {:?}. Loadout writes that file per run and it is the one place \
         the host's own deny rules come from — a passthrough pointing the app at a different \
         file replaces them wholesale, quietly, and everything downstream still looks right",
        doors.on_save
    );
    assert!(
        doors.on_plan.iter().any(|said| said.contains(SETTINGS)),
        "the same line inside an agent definition starts the run anyway, or stops it without \
         naming `{SETTINGS}`: {:?}",
        doors.on_plan
    );
    Ok(())
}

#[test]
fn the_three_assertions_of_the_older_oracle_still_hold_on_the_new_example()
-> Result<(), Box<dyn Error>> {
    // Ten sam agent, co w T-36, tylko nośnikiem wartości jest flaga wolna zamiast `--settings`:
    // jedno podniesienie w wartości, jedno w nazwie, i jedna flaga, która nikomu nie wadzi.
    let agent = agent_with(&[
        (FREE, BYPASS),
        (SKIP_PERMISSIONS, ""),
        (INNOCENT, INNOCENT_VALUE),
    ]);
    let handed = vendor_args_filtered(&agent, VENDOR);

    // ── (1) NIEWINNA FLAGA ZOSTAJE, RAZEM ZE SWOJĄ WARTOŚCIĄ ─────────────────────────────
    assert_eq!(
        value_after(&handed.args, INNOCENT),
        Some(INNOCENT_VALUE),
        "the flag that raises nothing has to survive with its value next to it. This is the \
         assertion an implementation that refuses every passthrough cannot pass. Got: {:?}",
        handed.args
    );
    assert_eq!(
        handed.args,
        [INNOCENT, INNOCENT_VALUE],
        "and nothing else: two entries dropped means exactly two entries dropped. `argv is \
         shorter` is the assertion this one replaces — it passes for a filter that cuts a flag at \
         random. Got: {:?}",
        handed.args
    );

    // ── (2) KAŻDE PODNIESIENIE ZNIKA Z OSOBNA, PO NAZWIE ─────────────────────────────────
    for raise in [BYPASS, SKIP_PERMISSIONS] {
        assert!(
            !handed.args.iter().any(|item| item.contains(raise)),
            "`{raise}` reached the arguments handed to the app. What an agent may do with your \
             files is set on the dial and nowhere else (D6); a passthrough that can raise it \
             makes the dial decorative. Got: {:?}",
            handed.args
        );
    }

    // ── (3) I ODMOWA MÓWI, KTÓRY WIERSZ SKASOWAĆ I DLACZEGO ──────────────────────────────
    // Para `(wiersz do skasowania, powód)`. Nazwanie samego wiersza nie wystarcza: flaga wolna
    // sama w sobie jest legalna, więc odmowa bez powodu czyta się jak awaria Loadouta.
    for (flag, escalation) in [(FREE, BYPASS), (SKIP_PERMISSIONS, SKIP_PERMISSIONS)] {
        let named = handed.refused.iter().find(|refusal| refusal.flag == flag);
        let Some(named) = named else {
            return Err(format!(
                "nothing in the report names `{flag}`. A silent refusal teaches the person that \
                 the passthrough does not work, instead of that it was blocked — and then they \
                 type the same thing again, spelled differently. Report: {:?}",
                handed.refused
            )
            .into());
        };
        assert_eq!(
            named.escalation, escalation,
            "`{flag}` was refused for the wrong reason, or for none that can be shown. The \
             sentence a person reads has to name what this line tried to raise. Report: {:?}",
            handed.refused
        );
    }

    assert!(
        !handed
            .refused
            .iter()
            .any(|refusal| refusal.flag == INNOCENT),
        "`{INNOCENT}` raises nothing, so it may not appear among the refusals. A report that \
         names every entry names none of them. Report: {:?}",
        handed.refused
    );
    assert_eq!(
        handed.refused.len(),
        2,
        "two entries in this passthrough raise the dial, so there are two refusals — one per line \
         the person has to delete. Report: {:?}",
        handed.refused
    );

    // I te same drzwi bez raportu odpowiadają to samo. Dwa filtry to dwie odpowiedzi, z których
    // podpięta jest zawsze starsza.
    assert_eq!(
        vendor_args(&agent, VENDOR),
        handed.args,
        "the two entry points disagree about the same passthrough"
    );
    Ok(())
}
