//! AC-2 dla T-23: schemat zna dokładnie te rodzaje kroku, które ma znać — a rodzaj spoza tej
//! listy nie istnieje i odmowa go NAZYWA.
//!
//! **Przepisane 2026-08-20 (T-55, decyzja człowieka o D6).** Do tego dnia listą były dwa
//! rodzaje, a nieznanym był `check` — i ten plik zgłaszał go jako brakujący, z nazwy, w
//! komentarzu niżej. Człowiek dołożył go świadomie, więc lista rośnie do trzech, a nieznanym
//! zostaje **`review`**. Ten wybór nie jest dowolny: D7 i niezmiennik 27 mówią, że silnik nie
//! ma prawa znać etapu o tej nazwie — etap nazwany w kodzie JEST domyślny i nie da się go
//! wyłączyć konfiguracją. Wyrocznia dostaje więc przypadek mocniejszy, niż miała: pilnuje
//! teraz reguły, która jest wciąż żywa, zamiast tej, którą właśnie rozstrzygnięto.
//!
//! Sposób sądzenia jest NIETKNIĘTY i taki ma zostać — zmieniła się lista, nie metoda.
//!
//! Różnica między „nie użyliśmy trzeciego rodzaju" a „trzeciego rodzaju nie ma" jest całą treścią
//! tego kryterium. Pierwsze zdanie jest o tym, jak akurat narysowano graf, i przestaje być prawdą
//! przy pierwszej cichej poprawce. Drugie jest o schemacie i da się je uruchomić: krok
//! o `"kind": "check"` przepuszczony przez parser T-12 musi wrócić jako odmowa, która **nazywa**
//! nieznany rodzaj.
//!
//! Stąd trzy asercje, z których żadna sama nie wystarcza:
//!
//! - równość zbiorów, nie zawieranie. `kinds.contains("agent") && kinds.contains("checkpoint")`
//!   przechodzi na pliku, do którego ktoś dołożył trzeci rodzaj, żeby graf się zmieścił — czyli
//!   dokładnie na tej awarii, której to zadanie ma zapobiec;
//! - kontrola negatywna: nietknięty plik przechodzi przez tę samą drogę (kopia na dysku →
//!   `file::load`). Bez niej odmowa przy zmutowanej kopii mogłaby pochodzić od instalacji
//!   fikstury, a nie od schematu;
//! - komunikat ma nazwać `check` jako nieznany rodzaj. `message.contains("check")` jest tu
//!   asercją pustą, bo `checkpoint` zawiera `check` jako prefiks i przechodzi na samej liście
//!   dozwolonych wariantów. Ogólny błąd parsowania pozwoliłby przyszłej cichej zmianie schematu
//!   przejść niezauważenie.
//!
//! Surowy `serde_json::Value`, nie typ z T-12: typ ma dwa warianty z definicji, więc pytanie
//! „ile rodzajów jest w pliku" zadane typowi zawsze odpowiada „najwyżej dwa" i nic nie mierzy.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use loadout_lib::workflow::file;

/// Rodzaje, których MIERZONY PLIK naprawdę używa — nie te, które zna schemat. Do 2026-08-20
/// jedna stała odpowiadała na oba pytania i mogła, bo odpowiedź była ta sama. Po dołożeniu
/// rodzaju `check` (D6) przestała: schemat zna trzy, a `ship-task.json` używa dwóch, bo
/// etapy sprawdzenia i wejścia na trunk stoją w nim nadal na kafelku kontrolnym.
///
/// Ta asercja pilnuje PLIKU i o to w niej chodziło od początku: „ktoś dołożył rodzaj, żeby
/// graf się zmieścił" jest zdaniem o pliku, nie o schemacie. Przepisanie `s_gate` i `s_land`
/// na kroki sprawdzenia jest osobną pracą — dopóki jej nie ma, ta lista ma dwie pozycje
/// i każde jej wydłużenie musi być czyjąś świadomą decyzją, a nie skutkiem ubocznym.
const IN_THE_FILE: [&str; 2] = ["agent", "checkpoint"];

/// Rodzaj, którego w schemacie nie ma i **nie ma prawa być**: etap recenzji nazwany w kodzie.
/// D7 i niezmiennik 27 — kolejność mieszka wyłącznie w grafie, a krok z agentem-recenzentem
/// jest dla silnika zwykłym krokiem. Kafelek o tej nazwie czyniłby recenzję domyślną i nie do
/// wyłączenia konfiguracją, czyli odwracałby decyzję D7 po cichu.
const UNKNOWN: &str = "review";

fn graph_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.loadout/workflows/ship-task.json")
}

/// Surowa treść pliku. Asercja z własnym komunikatem stoi przed odczytem, bo `No such file or
/// directory` jest podpisem fałszywej czerwieni i bramka odrzuciłaby taką czerwień jako niebyłą.
fn raw() -> Result<String, Box<dyn Error>> {
    let path = graph_path();
    assert!(
        path.exists(),
        "the harness workflow has not been written yet: {}",
        path.display()
    );
    Ok(fs::read_to_string(&path)?)
}

fn document() -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_str(&raw()?)?)
}

/// Zbiór różnych wartości `steps[].kind`, czytany z surowego dokumentu.
fn kinds(document: &Value) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let steps = document
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("the harness workflow has no list of steps")?;
    Ok(steps
        .iter()
        .filter_map(|step| step.get("kind").and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

/// Ta sama treść z jednym krokiem przestawionym na rodzaj spoza schematu. Zmieniamy WYŁĄCZNIE
/// `kind` — reszta pól zostaje, żeby odmowa mogła dotyczyć rodzaju, a nie brakującego pola.
fn with_a_third_kind(document: &Value) -> Result<Value, Box<dyn Error>> {
    let mut document = document.clone();
    let steps = document
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .ok_or("the harness workflow has no list of steps to change")?;
    let first = steps
        .first_mut()
        .and_then(Value::as_object_mut)
        .ok_or("the harness workflow has no first step to change")?;
    first.insert("kind".to_owned(), Value::from(UNKNOWN));
    Ok(document)
}

/// Kładzie dokument na dysku i wczytuje go publiczną powierzchnią T-12 — tą samą, którą wczytuje
/// się plik zapisany przez człowieka albo zmergowany gitem.
fn through_the_parser(document: &Value) -> Result<Result<(), String>, Box<dyn Error>> {
    let elsewhere = tempfile::tempdir()?;
    let path = elsewhere.path().join("ship-task.json");
    fs::write(&path, serde_json::to_string_pretty(document)?)?;
    Ok(match file::load(&path) {
        Ok(_) => Ok(()),
        Err(refusal) => Err(refusal.to_string()),
    })
}

/// Czy komunikat nazywa `check` jako osobne słowo, a nie tylko wylicza `checkpoint` wśród
/// dozwolonych wariantów. `contains("check")` przechodzi na samym `checkpoint`, więc nie
/// odróżnia „odmówiono, bo tego rodzaju nie ma" od „odmówiono z jakiegokolwiek powodu".
fn names_the_unknown_kind(message: &str) -> bool {
    message
        .match_indices(UNKNOWN)
        .any(|(at, _)| !message[at..].starts_with("checkpoint"))
}

#[test]
fn the_file_holds_exactly_the_kinds_it_should() -> Result<(), Box<dyn Error>> {
    let kinds = kinds(&document()?)?;
    let known: BTreeSet<String> = IN_THE_FILE.iter().map(|kind| (*kind).to_owned()).collect();

    assert_eq!(
        kinds, known,
        "equal, not 'contains': a third kind quietly added so the graph would fit is the exact \
         failure this task exists to catch, and 'contains' reports it as a pass"
    );
    Ok(())
}

#[test]
fn the_untouched_file_goes_through_that_same_parser() -> Result<(), Box<dyn Error>> {
    let outcome = through_the_parser(&document()?)?;

    assert!(
        outcome.is_ok(),
        "the negative control: if the file as written does not survive the copy-and-load round \
         trip, then the refusal in the next test says something about the fixture instead of \
         about the schema. It reads: {outcome:?}"
    );
    Ok(())
}

#[test]
fn a_third_kind_of_step_is_refused_and_the_message_names_it() -> Result<(), Box<dyn Error>> {
    let mutated = with_a_third_kind(&document()?)?;

    let Err(refusal) = through_the_parser(&mutated)? else {
        return Err(format!(
            "a step of kind {UNKNOWN} was accepted, so the schema has three kinds and the whole \
             finding of this task is wrong"
        )
        .into());
    };

    assert!(
        names_the_unknown_kind(&refusal),
        "the refusal has to name {UNKNOWN} as the kind it does not know. A message that only \
         lists the allowed variants would let a future schema change add a third kind without \
         anybody noticing. It reads: {refusal}"
    );
    Ok(())
}
