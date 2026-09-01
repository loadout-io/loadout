//! `@` podpowiada wyłącznie to, co leży w folderze projektu.
//!
//! Zgłoszenie właściciela 2026-09-01: „chce aby jak pisze @ miec opcje wyboru lokacji […] cos jak
//! w claude code". Podpowiedź bierze napis od człowieka i chodzi z nim po dysku, więc kryterium
//! pyta najpierw o granicę, a dopiero potem o wygodę: lista, która wychodzi poza wybrany folder,
//! pokazuje nazwy plików, których człowiek nie udostępnił.
use std::error::Error;
use std::fs;

use loadout_lib::commands::paths::{MOST, suggest};

/// Drzewko: dwa katalogi, plik, ukryty katalog i jeden z listy nigdy-nie-pokazuj.
fn tree() -> Result<tempfile::TempDir, Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for folder in [
        "src",
        "src/sections",
        "src/state",
        "docs",
        ".github",
        "node_modules",
    ] {
        fs::create_dir_all(root.path().join(folder))?;
    }
    fs::write(root.path().join("README.md"), "x")?;
    fs::write(root.path().join("src/main.rs"), "x")?;
    Ok(root)
}

#[test]
fn a_path_that_climbs_out_is_refused_not_trimmed() -> Result<(), Box<dyn Error>> {
    let root = tree()?;

    let answer = suggest(root.path(), "../", MOST);

    assert!(
        answer.is_err(),
        "climbing out of the project folder was allowed. Quietly trimming the `..` would be \
         worse than refusing: the person sees a list and cannot tell it is not the folder they \
         typed, and every name in it is a name from outside the folder they chose."
    );
    Ok(())
}

#[test]
fn an_absolute_path_is_refused_too() -> Result<(), Box<dyn Error>> {
    let root = tree()?;

    assert!(
        suggest(root.path(), "/etc/", MOST).is_err(),
        "an absolute path was accepted, so `@/etc/` lists the machine instead of the project"
    );
    Ok(())
}

#[test]
fn folders_come_before_files_and_carry_a_trailing_slash() -> Result<(), Box<dyn Error>> {
    let root = tree()?;

    let found = suggest(root.path(), "", MOST)?;
    let shown: Vec<&str> = found.iter().map(|one| one.path.as_str()).collect();

    assert_eq!(
        shown,
        vec!["docs/", "src/", "README.md"],
        "the list is not ordered folders-first, or a folder is missing its trailing slash. The \
         slash is not decoration: `@` points at a PLACE, so the next character a person types \
         should carry them into it rather than making them find the key."
    );
    Ok(())
}

#[test]
fn hidden_and_never_shown_folders_stay_out_until_asked_for() -> Result<(), Box<dyn Error>> {
    let root = tree()?;

    let plain: Vec<String> = suggest(root.path(), "", MOST)?
        .into_iter()
        .map(|one| one.path)
        .collect();
    assert!(
        !plain.iter().any(|one| one.starts_with(".github")),
        "a dot-folder showed up unasked, so the first press of `@` in any repository answers \
         with tooling instead of code. Found: {plain:?}"
    );
    assert!(
        !plain.iter().any(|one| one.starts_with("node_modules")),
        "node_modules was offered. It has tens of thousands of entries and an agent has nothing \
         to do in it. Found: {plain:?}"
    );

    let asked: Vec<String> = suggest(root.path(), ".", MOST)?
        .into_iter()
        .map(|one| one.path)
        .collect();
    assert!(
        asked.iter().any(|one| one.starts_with(".github")),
        "typing the dot did not bring the hidden folders back, so they are unreachable rather \
         than merely quiet. Found: {asked:?}"
    );
    Ok(())
}

#[test]
fn the_last_piece_filters_inside_the_folder_before_it() -> Result<(), Box<dyn Error>> {
    let root = tree()?;

    let found: Vec<String> = suggest(root.path(), "src/se", MOST)?
        .into_iter()
        .map(|one| one.path)
        .collect();

    assert_eq!(
        found,
        vec!["src/sections/".to_owned()],
        "the last piece was treated as a folder instead of a prefix, so typing half a name \
         answers with nothing and the person has to know the whole name before they can be \
         helped."
    );
    Ok(())
}

#[test]
fn the_list_stops_at_the_cap() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for number in 0..50 {
        fs::create_dir_all(root.path().join(format!("folder{number:03}")))?;
    }

    assert_eq!(
        suggest(root.path(), "", 5)?.len(),
        5,
        "the list ignored its ceiling. A suggestion list longer than the screen is a list nobody \
         reads, and one folder here can hold tens of thousands of entries."
    );
    Ok(())
}
