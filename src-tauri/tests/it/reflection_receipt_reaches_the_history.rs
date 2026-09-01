//! Rachunek prywatnej tury WYCHODZI z `run.json` na granicę, którą czyta okno.
//!
//! # Po co to jest, skoro zdanie sądzi kryterium frontowe
//!
//! Bo tamto stoi na atrapie granicy: `read_run` jest tam podmienione i oddaje kształt wpisany
//! w teście. Prawdziwa droga ma w środku rzecz, której żadna atrapa nie odtworzy — `run.json`
//! niesie klucze MIESZANE. `commands::run::ReflectionReceipt` wypisuje `ran`, `kept`,
//! `discardedAgain` (jawny `rename`) i `dropped_without_reason` (bez renamu), więc czytelnik
//! z jedną konwencją nazw czyta połowę pliku jako zera i nie mówi o tym ani słowa. Ekran byłby
//! wtedy zielony i kłamałby liczbą.
//!
//! # Słabe wersje tego kryterium i dlaczego ich tu nie ma
//!
//! **`assert!(opened.reflection.is_some())`.** Przechodzi ją implementacja, która wstawia
//! `ReflectionWire::default()` dla każdego biegu — czyli mówi „nic nie zostawił" o biegu, który
//! zostawił dwie notatki. Rozstrzygają WARTOŚCI, wszystkie cztery, i każda inna od pozostałych,
//! żeby zamiana dwóch pól miejscami była widoczna.
//!
//! **Test, który sprawdza tylko bieg z rachunkiem.** Przechodzi ją implementacja, która brakowi
//! klucza nadaje wyzerowaną strukturę — a to jest dokładnie to zlanie stanów, przed którym stoi
//! niezmiennik 17: „nie wiemy" przedstawione jako „nie robiliśmy tego". Dlatego drugi test żąda
//! `None`, a nie zer.
//!
//! `run.json` jest wypisany LITERALNIE, nigdy przez kod produkcyjny: odczyt, który czyta tylko
//! to, co sam zapisał, nie odpowiada na pytanie o niezmiennik 4 ani trochę. Ta sama zasada, co
//! w `history_reads_the_runs.rs` obok.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use loadout_lib::commands::history::read_run_inner;

/// Bieg, po którym prywatna tura naprawdę coś zostawiła.
const LEARNED: &str = "20260829-101500__0198a1f2-3b4c-7d5e-8f60-000000000101";

/// Bieg zapisany, zanim to pole w ogóle istniało.
const OLDER_THAN_THE_FIELD: &str = "20260829-101504__0198a1f2-3b4c-7d5e-8f60-000000000105";

/// `run.json` z rachunkiem, wypisany DOKŁADNIE tak, jak zapisuje go `commands::run`.
///
/// Cztery różne liczby i cztery różne klucze — dwa w `camelCase`, dwa w `snake_case`. Ta
/// mieszanka nie jest tu dla ozdoby: jest tym, co ten test mierzy.
const WITH_A_RECEIPT: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000101",
  "workflow_id": "ship-a-feature.json",
  "title": "Ship a feature",
  "status": "succeeded",
  "reflection": {
    "ran": true,
    "kept": 2,
    "discardedAgain": 3,
    "dropped_without_reason": 4,
    "cost_usd": 0.19
  },
  "steps": [
    {
      "id": "0198a1f2-3b4c-7d5e-8f60-00000000000b",
      "node_key": "build",
      "name": "Build",
      "agent": "claude",
      "status": "succeeded"
    }
  ]
}"#;

/// `run.json` sprzed tego pola. Jedyna różnica wobec pliku wyżej to brak klucza `reflection`.
const WITHOUT_A_RECEIPT: &str = r#"{
  "id": "0198a1f2-3b4c-7d5e-8f60-000000000105",
  "workflow_id": "ship-a-feature.json",
  "title": "Ship a feature",
  "status": "succeeded",
  "steps": [
    {
      "id": "0198a1f2-3b4c-7d5e-8f60-00000000000c",
      "node_key": "build",
      "name": "Build",
      "agent": "claude",
      "status": "succeeded"
    }
  ]
}"#;

/// Projekt z dwoma biegami: jednym z rachunkiem i jednym sprzed tego pola.
fn a_project_with_both(root: &Path) -> PathBuf {
    let project = root.join("ledger-ui");
    let runs = project.join(".loadout").join("runs");
    for (folder, description) in [
        (LEARNED, WITH_A_RECEIPT),
        (OLDER_THAN_THE_FIELD, WITHOUT_A_RECEIPT),
    ] {
        std::fs::create_dir_all(runs.join(folder)).unwrap();
        std::fs::write(runs.join(folder).join("run.json"), description).unwrap();
    }
    project
}

#[test]
fn it_carries_what_the_reflection_did_out_of_run_json() {
    let root = tempfile::tempdir().unwrap();
    let project = a_project_with_both(root.path());

    let opened = read_run_inner(&project, LEARNED).expect("that run is right there on disk");
    let did = opened.reflection.expect(
        "the run's own record says what Loadout's private turn did with it, and nothing carries \
         that across to the window. A person leaves the control on, pays for the turn, and has \
         no place at all to see whether anything came of it.",
    );

    assert!(
        did.ran,
        "the file says the turn went, and the answer says it did not. Every sentence the screen \
         can build from here starts with that word."
    );
    assert_eq!(
        did.kept, 2,
        "the number of notes is the one thing a person opens a finished run for, and it has to \
         come from the file rather than from the shape of anything else in it"
    );
    assert_eq!(
        did.discarded_again, 3,
        "\"you already turned this one down\" is a different fact from \"nothing was worth \
         keeping\", and the file counts it separately"
    );
    assert_eq!(
        did.dropped_without_reason, 4,
        "this one is written in `run.json` as `dropped_without_reason` while its neighbour is \
         written as `discardedAgain` — one file, two spellings. A reader with a single naming \
         convention reads this key as zero and says nothing about it, so the screen goes green \
         and quietly loses a count."
    );
}

#[test]
fn a_run_recorded_before_this_field_says_nothing_rather_than_zero() {
    let root = tempfile::tempdir().unwrap();
    let project = a_project_with_both(root.path());

    let opened =
        read_run_inner(&project, OLDER_THAN_THE_FIELD).expect("that run is right there on disk");
    assert!(
        opened.reflection.is_none(),
        "a run recorded before this key existed knows nothing about the private turn, and a \
         zeroed receipt says it went and found nothing. Those are two different sentences on a \
         screen and only the missing value can carry the first one (invariant 17). It gave: {:?}",
        opened.reflection
    );
}
