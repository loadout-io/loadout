//! AC-3 dla T-98: filtr podniesień dopasowuje po REGULE, nie po samym podciągu — i zna sufit
//! wydatku.
//!
//! # Dwie rzeczy naraz, bo to jedna zmiana
//!
//! **(1) Sufit wydatku jest podniesieniem.** `--max-budget-usd` składa Loadout z tego, ile
//! zostało w księdze biegu (`claude::budget_argv`), i to jedyna zapora między biegiem bez
//! nadzoru a rachunkiem. Przelotka podająca ją drugi raz nie „dubluje flagi" — ona ustawia
//! sufit sama, po swojemu, i wygrywa albo przegrywa po cichu. To jest dług T-94, opisany
//! w `docs/STATUS.md` jako jedna pozycja w `FORBIDDEN_ESCALATIONS`.
//!
//! **(2) Podniesienie ma być KLUCZEM, nie fragmentem.** Dziś reguła brzmi
//! `flag.contains(raise) || value.contains(raise)`, więc po stronie NAZWY łapie każdą flagę,
//! w której literał gdziekolwiek się pojawi. Dopóki lista miała trzy pozycje, kosztowało to
//! niewiele; z chwilą, w której dochodzi do niej nazwa zwykłej flagi vendora, `contains` zaczyna
//! zabijać flagi, których na liście nie ma i nigdy nie miało być — a przelotka istnieje dokładnie
//! po to, żeby flaga ogłoszona dziś rano była do użycia po południu (D6). Odmowa, która myli się
//! w tę stronę, jest gorsza od braku odmowy: mówi człowiekowi, że jego wiersz podnosi dial,
//! kiedy nie podnosi.
//!
//! # Co ten plik pilnuje po drugiej stronie
//!
//! **Wartości zostają skanowane podciągiem** i to jest kontrola, nie przeoczenie: `--sandbox`
//! nie jest zarezerwowane, a `--sandbox danger-full-access` omija dial tak samo skutecznie jak
//! `-s`. Reguła czytająca wyłącznie klucze przechodzi każde pytanie zadane o nazwy i przepuszcza
//! całą tę rodzinę.
//!
//! **Klucz przed `=` też jest kluczem.** `--dangerously-skip-permissions=true` to ten sam wiersz
//! zapisany inaczej. Przepisanie reguły z `contains` na równość, które o tym zapomni, otwiera
//! furtkę o jeden znak szeroką — i jest to furtka, którą dziś `contains` przypadkiem zamyka.
//!
//! **Lista podniesień jest niezależna od vendora.** Zamyka też nazwę aplikacji, o której Loadout
//! jeszcze nie słyszał — `reserved()` dla takiej nazwy oddaje pustą listę z rozmysłu, więc jedyną
//! rzeczą, która tam stoi, jest ta lista. Ostatni test tego pliku pyta dokładnie o to i dlatego
//! nie da się go przejść dopisując pozycję do listy zarezerwowanych któregokolwiek z dwóch
//! znanych vendorów (niezmiennik 23: jedna polityka, nie druga kopia obok).

use std::collections::BTreeMap;
use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::library::agents::{Agent, VendorOptions, passthrough_refused, vendor_args};
use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::file::{SaveError, save};

/// Nazwa jedynego kroku w fikstyrze — to ona stoi na kafelku.
const STEP: &str = "Build";

/// Sufit wydatku. Nazwa jest faktem o vendorze i mieszka w `engine::drivers::claude`; tutaj stoi
/// wypisana, bo kryterium pyta, czy POLITYKA ją zna — a nie, czy dwie stałe są sobie równe
/// (niezmiennik 20).
const CEILING: &str = "--max-budget-usd";

/// Kwota, którą przelotka próbowałaby postawić zamiast tej z księgi biegu.
const OVER_THE_TOP: &str = "999";

/// Podniesienie zapisane jako nazwa flagi — pozycja, którą lista ma dziś.
const SKIP_PERMISSIONS: &str = "--dangerously-skip-permissions";

/// Podniesienie zapisane jako wartość — druga połowa reguły, ta, która zostaje podciągiem.
const FULL_ACCESS: &str = "danger-full-access";

/// Flaga wolna, używana jako **nośnik wartości**. Nie jest niczyją flagą zarezerwowaną, więc
/// łapie ją wyłącznie reguła czytająca wartość.
const CARRIER: &str = "--verbose-tool-output";

/// Nazwa aplikacji, o której Loadout nie słyszał. `reserved()` oddaje dla niej pustą listę
/// z rozmysłu (D6: przelotka ma przetrwać vendora, którego jeszcze nie wspieramy), więc jedyną
/// regułą, jaka nad nią stoi, jest lista podniesień.
const UNKNOWN_APP: &str = "somethingelse";

// ── drzwi, za którymi człowiek to czyta ───────────────────────────────────────────────────

struct Doors {
    on_save: Option<String>,
    on_plan: Vec<String>,
}

impl Doors {
    fn names(&self, text: &str) -> bool {
        self.on_save
            .iter()
            .chain(&self.on_plan)
            .any(|said| said.contains(text))
    }

    fn all(&self) -> Vec<&str> {
        self.on_save
            .iter()
            .chain(&self.on_plan)
            .map(String::as_str)
            .collect()
    }

    fn quiet(&self) -> bool {
        self.on_save.is_none() && self.on_plan.is_empty()
    }
}

fn passthrough(vendor: &str, flag: &str, value: &str) -> Value {
    let mut flags = serde_json::Map::new();
    flags.insert(flag.to_owned(), Value::String(value.to_owned()));
    let mut vendors = serde_json::Map::new();
    vendors.insert(vendor.to_owned(), Value::Object(flags));
    Value::Object(vendors)
}

fn workflow_offering(
    vendor: &str,
    flag: &str,
    value: &str,
) -> Result<WorkflowFile, Box<dyn Error>> {
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
                "vendorOptions": passthrough(vendor, flag, value)
            }
        ],
        "links": []
    });
    Ok(serde_json::from_value(file)?)
}

fn agent_offering(vendor: &str, flag: &str, value: &str) -> Agent {
    let mut flags = BTreeMap::new();
    flags.insert(flag.to_owned(), value.to_owned());

    let mut options = VendorOptions::new();
    options.insert(vendor.to_owned(), flags);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

fn what_loadout_says(vendor: &str, flag: &str, value: &str) -> Result<Doors, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ship-a-feature.json");

    let on_save = match save(&workflow_offering(vendor, flag, value)?, &path) {
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
        on_plan: passthrough_refused(&agent_offering(vendor, flag, value)),
    })
}

// ── kryterium ─────────────────────────────────────────────────────────────────────────────

#[test]
fn setting_the_spending_ceiling_through_the_passthrough_is_refused_and_named()
-> Result<(), Box<dyn Error>> {
    let doors = what_loadout_says("claude", CEILING, OVER_THE_TOP)?;

    assert!(
        doors
            .on_save
            .as_deref()
            .is_some_and(|said| said.contains(CEILING)),
        "a step writing `{CEILING}` into its extra settings saves without a word, or is refused \
         without naming the line to delete. Loadout works that number out from what is left of \
         the run's ceiling and sends it itself; a passthrough that sets it too is not a duplicate \
         argument, it is a second answer to \"how much may this spend\" — and the loser loses \
         quietly, which for money is the one outcome nobody notices until the bill. It read: {:?}",
        doors.on_save
    );
    assert!(
        doors.on_plan.iter().any(|said| said.contains(CEILING)),
        "the same line inside an agent definition starts the run anyway, or stops it without \
         naming `{CEILING}`: {:?}",
        doors.on_plan
    );
    Ok(())
}

#[test]
fn the_ceiling_is_refused_under_a_name_no_reserved_list_of_ours_can_hold()
-> Result<(), Box<dyn Error>> {
    // Ta sama flaga pod nazwą aplikacji, o której Loadout nie słyszał. `reserved()` oddaje dla
    // niej pustą listę i tak ma zostać, więc jedyną rzeczą, jaka może to zatrzymać, jest lista
    // podniesień — czyli dokładnie ta pozycja, o którą prosi to kryterium. Bez tego testu
    // kryterium przechodzi dla listy zarezerwowanych z dopisanym wierszem, a lista zarezerwowanych
    // jest per vendor: to samo podniesienie trzeba by wtedy dopisywać przy każdej nowej aplikacji,
    // czyli mieć tyle kopii jednej reguły, ile vendorów (niezmiennik 23).
    let doors = what_loadout_says(UNKNOWN_APP, CEILING, OVER_THE_TOP)?;

    assert!(
        doors.names(CEILING),
        "`{CEILING}` written under an app Loadout does not know went through: {:?}. The list of \
         names one app sets for itself cannot hold this — it is per app, on purpose, so that a \
         new app needs no release of Loadout. What has to hold it is the one list of things the \
         passthrough may never raise, and that list is read for every app there is",
        doors.all()
    );
    Ok(())
}

#[test]
fn a_raise_carried_in_the_value_stays_a_refusal_whatever_the_key_is_called()
-> Result<(), Box<dyn Error>> {
    // Kontrola przepisania reguły. Filtr, który przestawi się na same klucze, przechodzi każde
    // pytanie zadane o nazwy i przepuszcza całą rodzinę „niewinna flaga, groźna wartość" —
    // a to jest połowa, przez którą ta dziura powstała.
    let doors = what_loadout_says("claude", CARRIER, FULL_ACCESS)?;

    assert!(
        doors.names(FULL_ACCESS),
        "`{CARRIER} {FULL_ACCESS}` went through: {:?}. That key is on no list of ours and it does \
         not have to be — what an agent may do with your files is set on one dial (D6), and a \
         value can walk past it just as well as a name. A rule that reads only keys passes every \
         question asked about names and misses this whole family",
        doors.all()
    );

    // I ta sama wartość zapisana z drugiej strony — jako klucz przed `=`. To jest ten sam wiersz
    // w innym zapisie, a przepisanie reguły z podciągu na równość zamyka go albo otwiera, i różni
    // się to jednym znakiem w implementacji.
    let with_equals = what_loadout_says("claude", &format!("{SKIP_PERMISSIONS}=true"), "")?;
    assert!(
        with_equals.names(SKIP_PERMISSIONS),
        "`{SKIP_PERMISSIONS}=true` went through: {:?}. Written this way it is the same line, and \
         a person who was refused once writes it the other way round — that is what a refusal \
         with no name teaches them to do",
        with_equals.all()
    );
    Ok(())
}

#[test]
fn a_longer_name_that_merely_contains_a_raise_is_a_different_flag() -> Result<(), Box<dyn Error>> {
    // Ta asercja jest cała treść „po kluczu, nie po podciągu". Obie nazwy niżej zaczynają się od
    // pozycji z listy i ŻADNA z nich nią nie jest: to inne argumenty tej samej aplikacji. Reguła
    // podciągowa odmawia obu i mówi przy tym nieprawdę — że ten wiersz podnosi dial. Gdyby
    // któraś z nich kiedyś naprawdę coś podnosiła, dostanie własny wiersz na liście; to jest
    // różnica między polityką a zgadywaniem.
    for (flag, value) in [
        (format!("{SKIP_PERMISSIONS}-audit-log"), "runs.jsonl"),
        (format!("{CEILING}-warning"), "3"),
    ] {
        let doors = what_loadout_says("claude", &flag, value)?;
        assert!(
            doors.quiet(),
            "`{flag}` was refused: {:?}. It only begins with a name from the list; it is not that \
             name. A rule searching for the text anywhere in the key kills arguments that were \
             announced this morning and raise nothing at all — and the passthrough exists for \
             exactly those (D6)",
            doors.all()
        );
        assert_eq!(
            vendor_args(&agent_offering("claude", &flag, value), "claude"),
            [flag.as_str(), value],
            "`{flag}` survives the refusal and does not reach the arguments with its value beside \
             it"
        );
    }
    Ok(())
}
