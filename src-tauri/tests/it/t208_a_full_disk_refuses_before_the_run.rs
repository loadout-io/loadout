//! T-208, połowa dyskowa: bieg odmawia PRZED startem, kiedy na dysku nie ma miejsca na jego pracę.
//!
//! PO CO. Bieg pisze do własnych drzew roboczych i do transkryptów przez cały czas trwania.
//! Dysk, który skończy się w połowie, nie daje czystej odmowy — zostawia obcięty
//! `agent-<id>.jsonl`, drzewo w połowie wypisane i `run.json`, który mówi „running" o czymś,
//! co już nie żyje. Odmowa przed startem jest jedynym momentem, w którym ta awaria jest tania.
//!
//! SŁABA WERSJA: test wyłącznie na tym, że przy małej liczbie wraca `Some`. Przechodzi dla
//! funkcji odmawiającej ZAWSZE, a taka zablokowałaby każdy bieg na każdej maszynie. Dlatego
//! obie strony progu są sądzone, a granica czytana ze stałej ze źródła, nie przepisana z palca:
//! wpisana ręcznie liczba przechodzi także wtedy, gdy próg w kodzie mówi co innego.

use loadout_lib::commands::run::{ROOM_FLOOR_BYTES, no_room_refusal};
use loadout_lib::workflow::check::Level;

#[test]
fn there_is_no_refusal_while_there_is_room() {
    assert!(
        no_room_refusal(ROOM_FLOOR_BYTES).is_none(),
        "exactly the floor is still enough room; refusing here would block a healthy machine"
    );
    assert!(
        no_room_refusal(ROOM_FLOOR_BYTES * 40).is_none(),
        "a disk with plenty of room must never turn a run down"
    );
}

#[test]
fn a_disk_without_room_is_turned_down_before_anything_starts()
-> Result<(), Box<dyn std::error::Error>> {
    let refusal =
        no_room_refusal(ROOM_FLOOR_BYTES - 1).ok_or("one byte under the floor must refuse")?;
    assert_eq!(
        refusal.level,
        Level::Problem,
        "this is a problem, not a note: a second later the run would be writing"
    );
    assert!(
        refusal.step_id.is_none(),
        "no single step owns this — the whole run has nowhere to write"
    );
    Ok(())
}

/// Zdanie ma nieść OBIE liczby, bo „za mało miejsca" zostawia człowieka ze zgadywaniem.
#[test]
fn the_refusal_says_how_much_is_free_and_how_much_is_needed()
-> Result<(), Box<dyn std::error::Error>> {
    let refusal = no_room_refusal(400_000_000).ok_or("400 MB is under the floor")?;
    let said = refusal.message;
    assert!(
        said.contains("0.4 GB"),
        "the sentence must say how much room is left. It said: {said}"
    );
    assert!(
        said.contains("1.1 GB"),
        "the sentence must say how much a run needs. It said: {said}"
    );
    assert!(
        said.contains("Free some space"),
        "the sentence must say what to do about it. It said: {said}"
    );
    Ok(())
}

/// Odczyt wolnego miejsca jest ŻYWY, a nie stałą, o którą nikt nie pyta.
#[test]
fn the_reading_comes_from_the_real_filesystem() -> Result<(), Box<dyn std::error::Error>> {
    let here = tempfile::tempdir()?;
    let free = loadout_lib::engine::supervisor::free_bytes(here.path())?;
    assert!(
        free > 0,
        "a writable temporary folder always has some room; a reading of zero means the answer \
         never left the syscall"
    );
    assert!(
        loadout_lib::engine::supervisor::free_bytes(&here.path().join("no-such-place")).is_err(),
        "a path that does not exist has no free space to report, and saying otherwise would let \
         the floor pass on a broken reading"
    );
    Ok(())
}
