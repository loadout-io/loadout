//! Lista umiejętności niesie zdanie „po co to jest", przeczytane z `SKILL.md`.
//!
//! # Co było zepsute
//!
//! `InstalledWire` niósł nazwę katalogu i znacznik pochodzenia — nic poza tym. Sekcja
//! Umiejętności była przez to siatką gołych napisów: żeby dowiedzieć się, co którakolwiek robi,
//! trzeba było otworzyć plik poza aplikacją. Komentarz w `src/sections/skills/index.tsx`
//! przyznawał to wprost i kończył się słowami „Zgłoszone człowiekowi".
//!
//! # Dlaczego trzeci przypadek jest tu najważniejszy
//!
//! Dwa pierwsze punkty — „opis dojeżdża" i „brak opisu daje pusty napis" — przechodzi także
//! implementacja, która rozbija front-matter własnym `split(':')`. Trzeci jej nie przechodzi:
//! `description: "Reads a PDF: the whole thing"` ma DWUKROPEK W ŚRODKU WARTOŚCI i jest w cudzysłowie.
//! Ręczny rozbiór urwie go na pierwszym dwukropku albo zostawi cudzysłowy na ekranie.
//!
//! To jest cały powód, dla którego ta ścieżka woła `place::read_doc` i `place::field`, a nie
//! własny parser: trzecia kopia reguły o cudzysłowach byłaby tą, która o nich nie wie
//! (niezmiennik 13).

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::skills::list_skills_in;
use loadout_lib::skills::Scope;
use loadout_lib::skills::place::destinations;

/// Kładzie `SKILL.md` o tej treści we WSZYSTKICH globalnych półkach, tak jak robi to instalacja.
///
/// We wszystkich, bo lista czyta oba drzewa vendorów i scala je w zbiór — fikstura pisząca
/// w jedno sądziłaby przypadkowo tę półkę, którą pętla obeszła pierwszą.
fn place(home: &Path, name: &str, body: &str) -> Result<(), Box<dyn Error>> {
    for shelf in destinations(Scope::Global, home, None) {
        let dir = shelf.join(name);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("SKILL.md"), body)?;
    }
    Ok(())
}

/// Umiejętność z opisem — dokładnie taka, jaką pisze nasz własny emiter.
fn with_description(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\nDo the thing.\n")
}

#[test]
fn the_list_carries_the_sentence_from_each_skill_file() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    /* DOM TO RODZIC BIBLIOTEKI, a nie sama biblioteka: `roots_for` liczy go jako
     * `library.parent()`, więc fikstura, która kładzie pliki „w bibliotece", sądziłaby katalog,
     * do którego lista nigdy nie zagląda — i przechodziła jako pusta. */
    let home = root.path();
    let library = home.join(".loadout");
    fs::create_dir_all(&library)?;
    place(
        home,
        "read-a-mockup",
        &with_description(
            "read-a-mockup",
            "Turns a screenshot into a written description",
        ),
    )?;
    place(
        home,
        "old-helper",
        "---\nname: old-helper\n---\n\nNo description at all.\n",
    )?;
    /* CUDZYSŁOWY I DWUKROPEK W ŚRODKU. To jest ten przypadek, którego ręczny rozbiór nie
     * przechodzi — a nasz emiter cytuje właśnie wtedy, gdy w wartości jest dwukropek. */
    place(
        home,
        "exact-diff",
        &with_description("exact-diff", "\"Shows the change: as it really is\""),
    )?;

    let listed = list_skills_in(&library, None)?;
    let said = |name: &str| -> String {
        listed
            .iter()
            .find(|one| one.name == name)
            .map_or_else(|| "<not listed>".to_owned(), |one| one.summary.clone())
    };

    assert_eq!(
        listed.len(),
        3,
        "the fixture put three skills on the shelves and the list found {}; every check below \
         would then be about the wrong set. It found: {:?}",
        listed.len(),
        listed.iter().map(|one| &one.name).collect::<Vec<_>>()
    );
    assert_eq!(
        said("read-a-mockup"),
        "Turns a screenshot into a written description",
        "the sentence from the skill file has to reach the list. Without it the section is a \
         grid of folder names, and the only way to learn what any of them does is to open a \
         file outside this app"
    );
    assert_eq!(
        said("old-helper"),
        "",
        "a file that says nothing has to come back saying nothing, not saying something wrong. \
         The screen turns this into its own sentence; a guess made here would be a fact invented \
         at the boundary"
    );
    assert_eq!(
        said("exact-diff"),
        "Shows the change: as it really is",
        "a description with a colon inside it, quoted the way our own writer quotes it, came \
         back cut or still wearing its quotes. That is what a hand-rolled split on ':' does, \
         and it is why this path reads the front matter with the one reader we already have"
    );
    Ok(())
}
