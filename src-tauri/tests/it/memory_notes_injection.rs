//! AC-1 dla T-17: notatka „suggested" nie występuje w zmontowanym tekście bloku — ani razu.
//!
//! To jest cała wartość podsystemu postawiona przed sądem [`FOUNDATIONS` §2.2: „only 'in use'
//! notes go into a prompt"]. Cicha porażka, przed którą stoi ten plik, jest banalna i dlatego
//! groźna: filtr po statusie stoi w jednym miejscu — na liście do wyświetlenia — a przy
//! składaniu bloku ktoś dokleja „a na końcu jeszcze kandydatki, żeby model miał kontekst".
//! Wszystkie testy dalej są zielone, bo sprawdzają `note.status`, a nie **zmontowany tekst**.
//! Od tej chwili jedna halucynacja agenta jest trwałym prawem projektu [`FOUNDATIONS` §2.1:
//! „bez tego jedna halucynacja staje się permanentnym folklorem"].
//!
//! **Słabą wersją tego kryterium jest `assert_eq!(block.used.len(), 2)`.** Przechodzi na
//! implementacji, która dokleja „Also suggested: …" na końcu tekstu: flaga mówi prawdę, prompt
//! kłamie. Rozróżnia wyłącznie asercja na `block.text` — i dlatego każde zdanie niżej pyta
//! o łańcuch, a `used` jest sprawdzane tylko jako to, co nie ma prawa się z nim rozjechać.
//!
//! **Drugą słabą wersją jest implementacja zwracająca zawsze `""`.** Przechodzi połowę tego
//! pliku i pada na kierunku drugim: ta sama piątka z jedną kandydatką przestawioną na `InUse`
//! musi dać tekst, który ją zawiera. Oba kierunki są tutaj z tego powodu.
//!
//! Notatki są budowane w pamięci, nie na dysku: to kryterium pyta o filtr, a nie o skan —
//! o skan pyta AC-5.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use loadout_lib::memory::est_tokens;
use loadout_lib::memory::notes::{Block, Budget, Kind, Note, NoteId, Scope, Status, what_you_know};

/// Sentinele. Wystarczająco dziwne, żeby nie mogły powstać przypadkiem z żadnego innego
/// fragmentu tekstu — pytanie „czy ta notatka tu jest" ma mieć jedną odpowiedź.
const USE_1: &str = "ZEBRA-USE-1";
const USE_2: &str = "ZEBRA-USE-2";
const SUG_1: &str = "ZEBRA-SUG-1";
const SUG_2: &str = "ZEBRA-SUG-2";
const SUG_3: &str = "ZEBRA-SUG-3";

/// Notatka zbudowana w pamięci. `est_tokens` liczymy tak, jak liczy je skan: z długości
/// `rule`, bo `rule` jest jedyną częścią notatki, która trafia do promptu.
fn note(id: &str, sentinel: &str, status: Status, occurrences: u32, modified: &str) -> Note {
    let rule = format!("{sentinel} the tenant is resolved before the guard runs");
    Note {
        id: NoteId(id.to_owned()),
        scope: Scope::ThisProject,
        // Notatka projektu jest niczyja: właściciela ma wyłącznie zakres `this-agent` (T-80).
        agent: None,
        // I napisano ją tutaj, więc nie ma projektu, z którego by przyjechała (T-80).
        project: None,
        // Nie zaproponował jej także żaden bieg.
        from: None,
        kind: Kind::Fact,
        title: format!("what {id} is about"),
        because: format!("run 7f3a step 2 reproduced it, and {id} is where it was written down"),
        status,
        occurrences,
        modified: modified.to_owned(),
        last_used_at: None,
        est_tokens: est_tokens(rule.len()),
        rule,
        path: PathBuf::from(format!("notes/{id}.md")),
        extra: BTreeMap::new(),
    }
}

/// Dwie w użyciu i trzy sugerowane. `sug-loudest` ma **najnowszy** `modified` i **najwyższe**
/// `occurrences` z całej piątki: gdyby cokolwiek wpuszczało notatki „po świeżości" albo „po
/// powtórzeniach", weszłaby pierwsza.
fn five() -> Vec<Note> {
    vec![
        note(
            "use-tenant",
            USE_1,
            Status::InUse,
            1,
            "2026-08-10T09:00:00Z",
        ),
        note("use-guard", USE_2, Status::InUse, 1, "2026-08-11T09:00:00Z"),
        note(
            "sug-loudest",
            SUG_1,
            Status::Suggested,
            9,
            "2026-08-16T23:59:59Z",
        ),
        note(
            "sug-second",
            SUG_2,
            Status::Suggested,
            1,
            "2026-08-12T09:00:00Z",
        ),
        note(
            "sug-third",
            SUG_3,
            Status::Suggested,
            1,
            "2026-08-13T09:00:00Z",
        ),
    ]
}

fn block_of(notes: &[Note]) -> Block {
    what_you_know(notes, Budget::of(Scope::ThisProject))
}

#[test]
fn the_text_carries_both_notes_a_person_approved() {
    let block = block_of(&five());

    for sentinel in [USE_1, USE_2] {
        assert!(
            block.text.contains(sentinel),
            "{sentinel} is a note a person put to use, and the block does not carry it. Half of \
             this criterion is satisfied by a function that always returns an empty string — \
             this line is the half that is not. The whole block reads:\n{}",
            block.text
        );
    }

    assert_eq!(
        block.used,
        vec![
            NoteId("use-guard".to_owned()),
            NoteId("use-tenant".to_owned())
        ],
        "and the receipt agrees with the text. This is the WEAK half of the criterion: on its \
         own it passes for a block that appends \"Also suggested: …\" underneath, because the \
         list stays honest while the text lies. It earns its place only next to the lines above \
         and below, where it stops the two from disagreeing"
    );
}

#[test]
fn not_one_of_the_three_suggested_notes_reaches_the_text() {
    let block = block_of(&five());

    for sentinel in [SUG_1, SUG_2, SUG_3] {
        assert!(
            !block.text.contains(sentinel),
            "{sentinel} was never approved by a person and it stands in the text that goes to \
             the model. From this moment one hallucination is permanent project folklore \
             (FOUNDATIONS §2.1). The whole block, header and footer included, reads:\n{}",
            block.text
        );
    }

    for suggested in ["sug-loudest", "sug-second", "sug-third"] {
        let id = NoteId(suggested.to_owned());
        assert!(
            !block.used.contains(&id),
            "{suggested} is named in the receipt as something the model was told"
        );
    }
}

#[test]
fn a_set_of_nothing_but_suggested_notes_renders_to_an_empty_string() {
    let only_suggested: Vec<Note> = five()
        .into_iter()
        .filter(|note| note.status == Status::Suggested)
        .collect();
    assert_eq!(
        only_suggested.len(),
        3,
        "the fixture itself has to hold three suggested notes, or the test below asks nothing"
    );

    let block = block_of(&only_suggested);

    assert_eq!(
        block.text, "",
        "nothing was approved, so there is nothing to say — and a heading with no items under \
         it is worse than silence: it teaches the model that this section is sometimes empty, \
         and it costs length for nothing"
    );
    assert!(
        block.used.is_empty(),
        "and nothing is claimed to have been used either, but {:?} is",
        block.used
    );
}

#[test]
fn the_same_five_notes_carry_a_suggested_one_the_moment_a_person_puts_it_to_use() {
    let mut flipped = five();
    let loudest = flipped
        .iter_mut()
        .find(|note| note.id == NoteId("sug-loudest".to_owned()))
        .expect("the fixture lost the note this test is about");
    loudest.status = Status::InUse;

    let block = what_you_know(&flipped, Budget::of(Scope::ThisProject));

    assert!(
        block.text.contains(SUG_1),
        "the only thing that changed is the status of this one note, so the only correct \
         difference is that its text is now in the block. Without this direction the whole \
         criterion is passed by a function that returns an empty string, and the filter would \
         be standing on something other than the status. The block reads:\n{}",
        block.text
    );
    for sentinel in [USE_1, USE_2] {
        assert!(
            block.text.contains(sentinel),
            "{sentinel} was in use before and still is, so it cannot have left"
        );
    }
    for sentinel in [SUG_2, SUG_3] {
        assert!(
            !block.text.contains(sentinel),
            "{sentinel} was not touched and is still only suggested, so flipping its neighbour \
             must not have let it in"
        );
    }
}
