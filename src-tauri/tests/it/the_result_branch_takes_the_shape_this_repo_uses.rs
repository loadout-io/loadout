//! Gałąź wyniku nazywa się tak, jak nazywa się w TYM repozytorium.
//!
//! Właściciel 2026-09-01: „ja daje po prostu ID tam przy starcie a loadout dopasowuje juz nazwe
//! brancza od preferencji repo". Kryteria pilnują obu połówek tej umowy: przedrostek jest
//! ZMIERZONY, a tam, gdzie nie ma czego zmierzyć, nie ma też czego dokleić.
//!
//! SŁABA WERSJA pytałaby, czy nazwa zawiera identyfikator. Przeszłaby dla implementacji, która
//! doszywa ten sam przedrostek wszędzie — czyli narzuca konwencję zamiast ją czytać.
use loadout_lib::commands::branch_name::{compose, convention, taken};

fn names(all: &[&str]) -> Vec<String> {
    all.iter().map(|one| (*one).to_owned()).collect()
}

#[test]
fn the_prefix_this_repo_really_uses_wins() {
    let repo = names(&[
        "main",
        "task-T-148",
        "task-T-149",
        "task-T-150",
        "task-T-151",
        "ui",
    ]);

    assert_eq!(convention(&repo).as_deref(), Some("task-"));
    assert_eq!(
        compose(&repo, "T-160"),
        "task-T-160",
        "the identifier did not take the shape this repository writes, so the branch a person \
         opens a pull request from reads unlike every other branch beside it."
    );
}

#[test]
fn a_slash_convention_is_read_the_same_way() {
    let repo = names(&[
        "main",
        "feat/import",
        "feat/lab",
        "feat/triggers",
        "fix/icon",
    ]);

    assert_eq!(compose(&repo, "LOAD-42"), "feat/LOAD-42");
}

#[test]
fn nothing_is_invented_where_there_is_no_convention() {
    let repo = names(&["main", "develop", "spike", "jakub-test"]);

    assert_eq!(
        convention(&repo),
        None,
        "a convention was declared from branches that do not have one"
    );
    assert_eq!(
        compose(&repo, "T-160"),
        "T-160",
        "a prefix was glued on in a repository that never uses one. Guessing here is the same \
         defect as a fake completion in a text field: it looks like knowledge we do not have."
    );
}

#[test]
fn an_identifier_that_already_carries_the_prefix_does_not_get_a_second_one() {
    let repo = names(&["task-T-148", "task-T-149", "task-T-150", "main"]);

    assert_eq!(
        compose(&repo, "task-T-160"),
        "task-T-160",
        "the prefix was doubled. A person who typed the whole name wanted that name, and \
         `task-task-T-160` is a branch nobody asked for."
    );
}

#[test]
fn our_own_bookkeeping_never_becomes_the_convention() {
    /* Po kilku biegach gałęzi `loadout/…` jest w repozytorium więcej niż wszystkich innych.
     * Liczone razem z resztą narzucałyby człowiekowi nazwę, której nigdy nie wybrał. */
    let repo = names(&[
        "main",
        "loadout/01a05e4b-83b8/s_1",
        "loadout/01a05e4b-83b8/s_2",
        "loadout/01a05e4b-83b8/s_3",
        "loadout/01a05e21-826d/s_1",
        "loadout/01a05e21-826d/s_2",
    ]);

    assert_eq!(
        convention(&repo),
        None,
        "Loadout's own branches were read as the repository's habit, so after a few runs every \
         result branch would be named after our bookkeeping instead of the person's convention."
    );
}

#[test]
fn a_handful_of_strays_does_not_become_a_habit() {
    let mut repo = names(&["fix/a", "fix/b", "fix/c"]);
    for number in 0..40 {
        repo.push(format!("branch{number}"));
    }

    assert_eq!(
        convention(&repo),
        None,
        "three branches out of forty-three were treated as this repository's convention. A floor \
         counted in absolute terms says nothing about the background it sits against."
    );
}

#[test]
fn a_name_already_in_use_is_known_before_the_run_starts() {
    let repo = names(&["main", "task-T-150"]);

    assert!(
        taken(&repo, "task-T-150"),
        "a name already on a branch was reported free, so the work is done first and has \
         nowhere to land second"
    );
    assert!(!taken(&repo, "task-T-160"));
}
