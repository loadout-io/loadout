//! AC-2 dla T-55: werdykt kroku „sprawdź" powstaje z DWÓCH rzeczy naraz — kodu wyjścia
//! **oraz** dopasowania wzorca dowodu.
//!
//! # Dlaczego to jest osobne kryterium, a nie szczegół implementacji
//!
//! Niezmiennik 19 w jednym zdaniu: kod wyjścia to nie dowód. Suita, która nie uruchomiła **ani
//! jednego** testu, wychodzi zerem — `cargo test` z pomylonym filtrem, `vitest` bez plików,
//! `os._exit(0)` na poziomie modułu. To jest cały powód, dla którego pole `proof` w ogóle
//! istnieje, i cały powód, dla którego ta funkcja bierze trzy argumenty, nie jeden.
//!
//! # SŁABA WERSJA i dlaczego jest słaba
//!
//! Przetestowanie samej PRZEKĄTNEJ — czyli (a) `rc 0` + licznik przejść → przeszło i (d) `rc 101`
//! + panika → nie przeszło. Ta para przechodzi dla TRZECH różnych implementacji naraz:
//!
//! * tej, która czyta sam kod wyjścia i wzorzec ignoruje,
//! * tej, która czyta samo dopasowanie i kod wyjścia ignoruje,
//! * i tej poprawnej.
//!
//! Rozróżniają je **wyłącznie** przypadki spoza przekątnej: (b) `rc 0` bez licznika zabija
//! pierwszą, (c) `rc 1` z licznikiem zabija drugą. Kryterium bez obu z nich jest kryterium,
//! które nie potrafi zaświecić — dlatego tabela niżej ma cztery wiersze, nie dwa.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use loadout_lib::engine::drivers::command::{passed, proof_matches};

/// Wzorzec dowodu, ten sam we wszystkich czterech przebiegach.
///
/// Ta sama notacja, którą człowiek pisze w linii `expect:` naszej własnej bramki
/// (`AGENTS.md` §2a punkt 4): `(\d+)` znaczy „co najmniej jedna cyfra", wszystko poza nią jest
/// literałem. Jedna notacja, jedno znaczenie — w bramce i w aplikacji.
const PROOF: &str = r"(\d+) passed";

#[test]
fn the_verdict_needs_the_exit_code_and_the_proof_to_agree() {
    // (a) Zero i licznik przejść w wyjściu — jedyny układ, który jest przejściem.
    assert!(
        passed(Some(0), "test result: ok. 12 passed; 0 failed", PROOF),
        "a command that exited clean AND printed a pass count is the only shape of a check that \
         passed; anything less than both is a guess"
    );

    // (b) SEDNO NIEZMIENNIKA 19. Zero, ale nic nie ruszyło.
    assert!(
        !passed(Some(0), "error: no test target matched", PROOF),
        "a suite that ran ZERO tests exits zero — that is the whole reason `proof` exists. An \
         implementation that reads only the exit code goes green here, and every empty run in \
         this product would read as a passing one (invariant 19)"
    );

    // (c) Lustro (b): licznik JEST w wyjściu, a komenda padła.
    assert!(
        !passed(Some(1), "test result: FAILED. 11 passed; 1 failed", PROOF),
        "eleven tests passed and one failed, so the check did NOT pass. An implementation that \
         reads only the proof goes green here — this is the assertion that kills it"
    );

    // (d) Panika: ani kodu zero, ani licznika.
    assert!(
        !passed(Some(101), "thread 'main' panicked", PROOF),
        "a panic is not a pass under any reading"
    );

    // Piąty przebieg: `None` to nie zero. Proces zginął od sygnału, więc kodu po prostu NIE MA,
    // a brak odpowiedzi nie jest odpowiedzią „udało się" (niezmiennik 6 czytany od tej strony).
    assert!(
        !passed(None, "test result: ok. 12 passed; 0 failed", PROOF),
        "no exit code means the command was killed, and `None` is not zero. Treating a missing \
         code as success turns every stopped check into a passing one"
    );
}

#[test]
fn the_one_metacharacter_means_at_least_one_digit() {
    assert!(
        !proof_matches(PROOF, "test result: ok.  passed; 0 failed"),
        "`(\\d+)` means AT LEAST ONE digit, so zero digits must not match. Without this the \
         metacharacter is an ornament and the pattern is just a substring search on \" passed\""
    );
    assert!(
        proof_matches(PROOF, "test result: ok. 1 passed; 0 failed"),
        "one digit is enough"
    );
    assert!(
        proof_matches(PROOF, "test result: ok. 1234 passed; 0 failed"),
        "and so are four — `+` is not `?`"
    );
    assert!(
        proof_matches("0 failed", "test result: ok. 12 passed; 0 failed"),
        "a pattern with no metacharacter is a plain substring, nothing more. This is what a \
         person writes nine times out of ten and it has to keep working"
    );
}

#[test]
fn the_proof_is_matched_against_stdout_and_stderr_joined() {
    /* `cargo test` pisze podsumowanie na wyjście, a `npm` swoje na strumień skarg — więc wzorzec
     * ma trafić w OBA. Jeden strumień odczytany bez drugiego znaczy, że połowa prawdziwych
     * komend sprawdzających jest dla werdyktu niewidoczna, a to jest ta sama cicha porażka co
     * (b) wyżej: wygląda na „nic nie ruszyło", choć ruszyło i przeszło. */
    let joined = "npm warn config production is deprecated\nTests  7 passed (7)\n";
    assert!(
        proof_matches("(\\d+) passed", joined),
        "the proof is matched against stdout and stderr JOINED; here the counter arrives after a \
         warning, exactly as npm writes it"
    );
    assert!(
        passed(Some(0), joined, "(\\d+) passed"),
        "and the verdict reads the same joined text — otherwise every npm-shaped command reads \
         as a suite that never ran"
    );
}
