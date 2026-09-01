//! Nazwa workflow do wpisania znaczy to samo po obu stronach granicy.
//!
//! # Po co to istnieje
//!
//! Ta sama reguła musi żyć dwa razy i nie da się tego uniknąć: wiersz wejścia normalizuje to, co
//! człowiek NAPISAŁ (czyli potrzebuje funkcji, nie wartości), a czasownik `list_workflows` oddaje
//! liderowi nazwy, którymi ma się posłużyć. Dwie implementacje jednej reguły rozjeżdżają się
//! cicho, a skutek nie wygląda jak błąd: lider proponuje `przeglad-kodu`, Enter odpowiada „There
//! is no workflow called…", i człowiek widzi workflow, którego rzekomo nie ma.
//!
//! # Dlaczego wspólna fikstura, a nie przepisane pary
//!
//! Bo pary wpisane po obu stronach z palca są zielone także wtedy, gdy obie strony mylą się tak
//! samo — a przy dwóch niezależnych implementacjach to jest najczęstszy sposób, w jaki takie
//! kryterium kłamie (niezmiennik 20). Plik jest jeden i czyta go także
//! `src/sections/run/typable-matches-rust.test.ts`.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `chat_never_starts_a_run` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::commands::workflows::typable;

/// Wspólna wyrocznia, wbudowana w binarium: plik, którego nie ma, ma nie skompilować kryterium,
/// a nie oddać pustej listy par i przejść.
const FIXTURE: &str = include_str!("../../../docs/patterns/fixtures/typable-names.json");

#[test]
fn every_pair_in_the_shared_fixture_holds_here_too() {
    let read: serde_json::Value =
        serde_json::from_str(FIXTURE).expect("the shared fixture has to be readable JSON");
    let pairs = read
        .get("pairs")
        .and_then(serde_json::Value::as_array)
        .expect("the fixture carries its pairs under `pairs`");

    assert!(
        pairs.len() >= 10,
        "a fixture that shrank to a handful of pairs stops being an oracle and starts being a \
         formality. It carried {} pairs",
        pairs.len()
    );

    let mut wrong: Vec<String> = Vec::new();
    for pair in pairs {
        let given = pair
            .get("given")
            .and_then(serde_json::Value::as_str)
            .expect("every pair says what it was given");
        let want = pair
            .get("typable")
            .and_then(serde_json::Value::as_str)
            .expect("every pair says what it should become");
        let got = typable(given);
        if got != want {
            wrong.push(format!(
                "{given:?} -> {got:?}, but the window says {want:?}"
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "these names mean one thing here and another in the command line, so the lead would \
         hand the person a name that Enter refuses:\n  {}",
        wrong.join("\n  ")
    );
}
