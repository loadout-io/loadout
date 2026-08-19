//! Pętla z limitem tur, warstwa pliku: **oznaczona** strzałka wsteczna jest dozwolona, każda inna
//! dalej nie.
//!
//! Projekt stoi w `docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md`, decyzje
//! właściciela są tam zapisane przy każdym punkcie. Kształt, którego brakowało: implementer
//! wysyła do testera, tester zdaje raport — `fail` wraca do implementera, `pass` puszcza bieg
//! dalej. Do 2026-08-19 jedyną formą rundy poprawek było wypisanie każdej rundy osobnym krokiem,
//! czyli zamrożenie liczby prób w pliku i łańcuch identycznych kafelków na płótnie.
//!
//! DLACZEGO CYKL NIE STAJE SIĘ PO PROSTU LEGALNY. Cykl, którego nikt nie oznaczył, jest pomyłką
//! — najczęściej strzałką pociągniętą w złą stronę — i ma nią zostać. Reguła w jednym zdaniu:
//! *po usunięciu strzałek z `max_turns` graf musi być bez cykli*. Dlatego przypadek (b) niżej
//! jest tym samym grafem co (a), różniącym się WYŁĄCZNIE obecnością pola, i musi dać Problem.
//!
//! SŁABĄ WERSJĄ TEGO KRYTERIUM jest sprawdzenie, że plik z `max_turns` się wczytuje. Przechodzi
//! ją implementacja, która wyłączyła `a_circle` całkowicie — czyli przepuszcza też pętlę
//! nieoznaczoną i bieg kręci się bez końca. Stąd obie połowy na jednym grafie.
//!
//! DRUGĄ SŁABĄ WERSJĄ jest sprawdzanie zakresu `max_turns` na wartości, która nie jest ani
//! zerem, ani sufitem. `0` znaczy „pętla, która nie wykonuje się ani razu" i jest osobnym
//! rodzajem nonsensu niż `11`; oba są niżej.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check, check_to_run};

fn step(id: &str, name: &str) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "a_forge",
        "instructions": "Do the work.",
        "folder": { "use": "fresh-copy" }
    })
}

/// Strzałka bez oznaczenia — zwykłe „po".
fn arrow(from: &str, to: &str) -> Value {
    json!({ "from": from, "to": to })
}

/// Strzałka wsteczna, czyli pętla z limitem tur.
fn back(from: &str, to: &str, turns: u32) -> Value {
    json!({ "from": from, "to": to, "max_turns": turns })
}

fn workflow(steps: &[Value], links: &[Value]) -> Result<WorkflowFile, Box<dyn Error>> {
    Ok(serde_json::from_value(json!({
        "format": 1,
        "id": "wf_loop",
        "name": "Implement and test",
        "steps": steps,
        "links": links
    }))?)
}

/// Implementer → tester, i tester z powrotem do implementera.
fn implement_and_test(back_edge: Value) -> Result<WorkflowFile, Box<dyn Error>> {
    workflow(
        &[
            step("s_impl", "Implement"),
            step("s_test", "Tester"),
            step("s_ship", "Ship"),
        ],
        &[
            arrow("s_impl", "s_test"),
            back_edge,
            arrow("s_test", "s_ship"),
        ],
    )
}

fn problems(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

#[test]
fn a_marked_back_edge_is_allowed() -> Result<(), Box<dyn Error>> {
    let workflow = implement_and_test(back("s_test", "s_impl", 3))?;

    let notes = check_to_run(&workflow);

    assert!(
        problems(&notes).is_empty(),
        "this is the whole feature: the tester sends the work back to the implementer and the \
         run tries again. Refusing it here means the loop cannot be expressed at all. \
         Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn the_same_circle_without_a_limit_is_still_refused() -> Result<(), Box<dyn Error>> {
    let workflow = implement_and_test(arrow("s_test", "s_impl"))?;

    let notes = check(&workflow);

    assert_eq!(
        problems(&notes).len(),
        1,
        "the same graph, differing ONLY in whether the back edge carries a limit. An unmarked \
         circle is an arrow drawn the wrong way round and has to stay a refusal — without this \
         half, allowing the loop means allowing a run that never ends. Got: {notes:?}"
    );
    Ok(())
}

/// Uwaga o liczbie rund, a nie o czymkolwiek innym.
///
/// SĄDZIMY TREŚĆ, NIE LICZBĘ UWAG, i to jest różnica, na której ten plik stał zanim reguła
/// powstała: `max_turns: 0` był wtedy odrzucany, ale **przez regułę koła** — bo powrót nie był
/// jeszcze rozpoznawany i domykał cykl jak każda inna strzałka. Kryterium liczące same problemy
/// świeciło się więc na zielono nad nieistniejącym kodem. Zdanie musi nazwać krok i podać
/// zakres, bo to jest cała robota, jaką ta uwaga ma do wykonania (DESIGN §8).
fn only_note_about_turns(workflow: &WorkflowFile) -> String {
    let notes = check(workflow);
    let found = problems(&notes);
    assert_eq!(
        found.len(),
        1,
        "one bad number is one thing to fix. Got: {notes:?}"
    );
    let message = found[0].message.clone();
    assert!(
        message.contains("Tester") && message.contains("1 to 10"),
        "the note has to name the step the way back leaves from and say which numbers are \
         allowed; a note about a circle here would mean the way back is not recognised at all \
         and this criterion is passing over nothing. It reads: {message}"
    );
    message
}

#[test]
fn a_loop_that_never_runs_is_refused() -> Result<(), Box<dyn Error>> {
    let workflow = implement_and_test(back("s_test", "s_impl", 0))?;

    let message = only_note_about_turns(&workflow);

    assert!(
        message.contains("0 times"),
        "zero turns is a loop that cannot happen — a drawn arrow with no effect, invariant 16 \
         written into a file. It reads: {message}"
    );
    Ok(())
}

#[test]
fn more_turns_than_the_ceiling_is_refused() -> Result<(), Box<dyn Error>> {
    let workflow = implement_and_test(back("s_test", "s_impl", 11))?;

    let message = only_note_about_turns(&workflow);

    assert!(
        message.contains("11 times"),
        "ten rounds of two agents is already a long unattended night and a real bill; the \
         ceiling is the same kind of guard as the one on copies. It reads: {message}"
    );
    Ok(())
}
