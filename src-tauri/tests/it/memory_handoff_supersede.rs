//! AC-4 dla T-16: korekta to nowy plik; stary nie jest edytowany poza jedną linią statusu.
//!
//! Przekazania są niezmienne [T6 §9]: „a correction is a *new* handoff with `supersedes: <id>`;
//! the old one flips to `status: superseded` and stops being injected. Run history stays
//! truthful." Cała wartość tego zdania siedzi w ostatnim słowie — nadpisanie starego pliku
//! w miejscu daje **identycznie wyglądający** katalog biegu, w którym historia jest fałszywa.
//!
//! **Słabą wersją tego kryterium jest sprawdzenie samego nowego pliku.** Przechodzi ją
//! implementacja, która stary plik nadpisuje: nowy plik ma `supersedes`, ma `status: current`,
//! wszystko się zgadza — a poprzedniej wersji nie ma nigdzie i nikt się nie dowie, co krok
//! naprawdę powiedział za pierwszym razem.
//!
//! Rozróżniają dwie rzeczy: bajtowa równość ciała starego pliku sprzed i po korekcie, oraz
//! mapa `ścieżka → zawartość` całego katalogu przy **odrzuconym** drugim wywołaniu.
//!
//! Porównujemy bajty, nie sha256, którą nazywa AC-4: równość bajtów jest ściśle mocniejsza
//! (żadnej kolizji nie ma z definicji) i nie wymaga zależności, której `src-tauri/Cargo.toml`
//! nie ma — a Cargo.toml nie należy do T-16 (AGENTS.md §7).

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use loadout_lib::memory::Error;
use loadout_lib::memory::handoff::{self, Handoff, Kind, MetaDraft, Status, Written};

const BODY_FIRST: &str = "\
## Answer
The tenant is resolved after the guard runs, so a missing tenant is a 400.

## Evidence
- src/auth/tenant.middleware.ts:41

## Open
- Nothing.
";

const BODY_CORRECTION: &str = "\
## Answer
Wrong the first time: the tenant is resolved BEFORE the guard, so it is a 401.

## Evidence
- src/auth/tenant.middleware.ts:41 -- resolve() throws before the guard runs

## Open
- Unclear whether the mobile client relies on the 401.
";

/// `id`, którego w tym katalogu biegu nie ma i nie będzie.
const MISSING_ID: &str = "h_does_not_exist";

const BODY_SECOND_CORRECTION: &str = "\
## Answer
A third opinion nobody asked for.

## Evidence
- none

## Open
- none
";

/// Ten sam krok, ten sam agent, ten sam rodzaj — to jest kształt prawdziwej korekty i to jest
/// przypadek, w którym „nadpisz plik w miejscu" wygląda najbardziej rozsądnie. Nazwa drugiego
/// pliku rozjeżdża się z pierwszą sufiksem kolizji (AC-6), nie ręcznie podanym innym krokiem.
fn draft() -> MetaDraft {
    MetaDraft {
        run: "run_7f3a".to_owned(),
        step: 2,
        from: "research-auth".to_owned(),
        to: vec!["planner".to_owned()],
        kind: Kind::Findings,
        title: "Auth flow findings".to_owned(),
        reads: vec![],
    }
}

/// Mapa `ścieżka → zawartość` całego katalogu biegu, rekurencyjnie.
///
/// Zawartość, nie hasz: równość bajtów jest ściśle mocniejsza i nie wymaga zależności spoza
/// `src-tauri/Cargo.toml`, który do tego zadania nie należy.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.insert(path, bytes);
            }
        }
    }
    out
}

/// Ścieżki, które się między dwoma zdjęciami katalogu różnią — łącznie z tymi, które
/// przybyły albo zniknęły.
fn differences(
    before: &BTreeMap<PathBuf, Vec<u8>>,
    after: &BTreeMap<PathBuf, Vec<u8>>,
) -> Vec<PathBuf> {
    let paths: BTreeSet<&PathBuf> = before.keys().chain(after.keys()).collect();
    paths
        .into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

/// Pierwsze przekazanie i jego odczyt — punkt odniesienia dla wszystkiego dalej.
fn first(run_dir: &Path) -> (Written, Handoff) {
    let written = handoff::write_handoff(run_dir, draft(), BODY_FIRST)
        .expect("write_handoff refused the first handoff");
    let read = handoff::read_handoff(&written.path).expect("read_handoff cannot read our own file");
    (written, read)
}

#[test]
fn the_correction_is_a_new_file_and_the_old_one_keeps_its_body() {
    let run_dir = tempfile::tempdir().unwrap();
    let (old, before) = first(run_dir.path());
    let old_text = std::fs::read_to_string(&old.path).unwrap();

    let new = handoff::supersede(run_dir.path(), &before.meta.id, draft(), BODY_CORRECTION)
        .expect("supersede refused a handoff that is still current");

    assert_ne!(
        new.path, old.path,
        "the correction landed on the same path as the original, which means the first version \
         is gone. A run whose history can be rewritten cannot answer what an agent actually \
         said, and that is the whole reason handoffs are immutable [T6 §9]"
    );
    assert!(
        old.path.exists(),
        "the original file is no longer on disk at {}",
        old.path.display()
    );

    let corrected =
        handoff::read_handoff(&new.path).expect("read_handoff cannot read the new file");
    assert_eq!(
        corrected.meta.supersedes,
        Some(before.meta.id.clone()),
        "the new file names the one it replaces. Without that link the two files are two \
         unrelated opinions and nothing says which one is the correction"
    );
    assert_eq!(
        corrected.meta.status,
        Status::Current,
        "the correction is what is current now"
    );

    // Stary plik: ciało nietknięte, a z trzynastu pól zmienia się dokładnie jedno.
    let after = handoff::read_handoff(&old.path).expect("the old file stopped being readable");
    assert_eq!(
        after.body, before.body,
        "the body of the superseded handoff changed. Nothing in a correction gives anyone \
         licence to edit what the first agent wrote"
    );

    let mut expected = before.meta.clone();
    expected.status = Status::Superseded;
    assert_eq!(
        after.meta, expected,
        "exactly one of the thirteen fields moves, and it is `status`. Rewriting `created`, \
         `bytes` or `reads` while flipping the status turns the old file into a file that was \
         never written"
    );

    // Ta sama rzecz na surowym tekście: jedna linia różnicy, i jest to linia `status:`.
    let after_text = std::fs::read_to_string(&old.path).unwrap();
    assert_eq!(
        after_text.lines().count(),
        old_text.lines().count(),
        "the superseded file gained or lost lines; it should have had one of them rewritten"
    );
    let changed: Vec<(&str, &str)> = old_text
        .lines()
        .zip(after_text.lines())
        .filter(|(was, now)| was != now)
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "exactly one line of the old file changes. These changed: {changed:?}"
    );
    assert!(
        changed
            .first()
            .is_some_and(|(was, now)| was.starts_with("status:") && now.starts_with("status:")),
        "the one line that changed is the `status:` line; it was {changed:?}"
    );
}

#[test]
fn only_the_correction_is_current_and_the_old_file_is_still_there() {
    let run_dir = tempfile::tempdir().unwrap();
    let (old, before) = first(run_dir.path());
    let new = handoff::supersede(run_dir.path(), &before.meta.id, draft(), BODY_CORRECTION)
        .expect("supersede refused a handoff that is still current");

    let all = handoff::scan_run_dir(run_dir.path()).expect("scan_run_dir failed on our own run");
    assert_eq!(
        all.len(),
        2,
        "both files are still in the run directory: the superseded one is archived, not \
         deleted [T6 §9]. The scan returned {:?}",
        all.iter().map(|h| &h.path).collect::<Vec<_>>()
    );

    let current: Vec<&Handoff> = all
        .iter()
        .filter(|entry| entry.meta.status == Status::Current)
        .collect();
    assert_eq!(
        current.len(),
        1,
        "filtering the scan by `current` leaves exactly one handoff. Two means the superseded \
         one is still being injected into the next step's prompt, which is the failure this \
         status exists to prevent. It left {:?}",
        current.iter().map(|h| &h.path).collect::<Vec<_>>()
    );
    assert_eq!(
        current.first().map(|entry| &entry.path),
        Some(&new.path),
        "the one current handoff is the correction, not the original at {}",
        old.path.display()
    );
}

/// `AlreadySuperseded` i `NoSuchHandoff` to dwie różne odmowy i tylko jedna z nich jest
/// wyżej sprawdzona. Wołający, który dostanie „już poprawione" na `id`, którego w tym biegu
/// nigdy nie było, pójdzie szukać nieistniejącego pliku zamiast poprawić literówkę.
#[test]
fn correcting_an_id_this_run_never_had_is_refused_by_name() {
    let run_dir = tempfile::tempdir().unwrap();
    let (_, before) = first(run_dir.path());
    assert_ne!(
        before.meta.id, MISSING_ID,
        "the fixture id collided with the one this test asks for"
    );

    let snapshot = tree(run_dir.path());
    let refused = handoff::supersede(run_dir.path(), MISSING_ID, draft(), BODY_CORRECTION);

    assert!(
        matches!(&refused, Err(Error::NoSuchHandoff { id }) if id == MISSING_ID),
        "correcting an id this run does not hold is its own refusal, and it names the id. \
         `is_err()` alone passes on `AlreadySuperseded`, which sends the caller looking for a \
         file that was never written. It returned {refused:?}"
    );

    let changed = differences(&snapshot, &tree(run_dir.path()));
    assert!(
        changed.is_empty(),
        "the refused correction still touched the disk. These paths differ: {changed:?}"
    );
}

#[test]
fn correcting_the_same_handoff_twice_is_refused_and_changes_nothing_on_disk() {
    let run_dir = tempfile::tempdir().unwrap();
    let (_, before) = first(run_dir.path());
    handoff::supersede(run_dir.path(), &before.meta.id, draft(), BODY_CORRECTION)
        .expect("supersede refused a handoff that is still current");

    let snapshot = tree(run_dir.path());
    let again = handoff::supersede(
        run_dir.path(),
        &before.meta.id,
        draft(),
        BODY_SECOND_CORRECTION,
    );

    assert!(
        matches!(&again, Err(Error::AlreadySuperseded { .. })),
        "a handoff that has already been corrected has nothing left to hand over. Accepting \
         the second correction leaves two files claiming to replace the same one, and the \
         chain stops answering which is the latest. It returned {again:?}"
    );

    let after = tree(run_dir.path());
    let changed = differences(&snapshot, &after);
    assert!(
        changed.is_empty(),
        "the refused correction still touched the disk. A call that fails halfway leaves the \
         run in a state no code path ever meant to produce. These paths differ: {changed:?}"
    );
}
