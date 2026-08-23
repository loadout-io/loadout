//! Przekazania, które sekcja Pamięć pokazuje, należą do FOLDERU, o który zapytano.
//!
//! # Co było zepsute
//!
//! `ipc::list_handoffs` czytało `AppState::project` — pole ustawiane RAZ przy starcie okna na
//! `LOADOUT_PROJECT` albo na `~/.loadout/workspace` (`lib.rs`). Pierwsze nie jest w tym repo
//! ustawiane nigdzie, drugie nie istnieje na dysku. `run_dirs` na nieistniejącym katalogu
//! oddaje pustą listę **bez błędu**, więc trzecia strefa sekcji Pamięć — ta, która jest jej
//! nagłówną obietnicą („What agents leave for each other lands here") — pokazywała
//! „Nothing yet…" i nawet nie zapalała odmowy.
//!
//! Zmierzone 2026-08-23 na maszynie właściciela: `~/.loadout/workspace` nie istnieje, a
//! w `~/Projects/urc-monorepo/.loadout/runs/*/handoffs/` leżało **80 prawdziwych plików**.
//!
//! # Dlaczego to kryterium wygląda tak, a nie inaczej
//!
//! **Słabą wersją jest „`list_handoffs_inner` na katalogu z przekazaniami oddaje niepustą
//! listę".** Przechodzi ją implementacja, która czyta ZAWSZE ten sam katalog — a to jest
//! dokładnie defekt, który tu naprawiamy. Rozróżnia je dopiero DRUGI projekt: ta sama funkcja
//! pytana o dwa różne foldery musi dać dwie różne odpowiedzi.
//!
//! Trzecia asercja pilnuje, że pusto znaczy pusto, a nie „popsuło się po cichu": folder bez
//! ani jednego biegu ma oddać pustą listę, a nie odmowę — inaczej człowiek z nowym projektem
//! dostawałby błąd zamiast zaproszenia.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::handoffs::list_handoffs_inner;

/// Przekazanie tak, jak pisze je `memory::handoff`: płaski front-matter i ciało pod nim.
fn a_handoff(step: &str, from: &str) -> String {
    format!(
        "---\n\
         id: h_{step}\n\
         run: 01a00000-0000-7000-8000-00000000000{step}\n\
         step: {step}\n\
         from: {from}\n\
         to: []\n\
         kind: findings\n\
         title: what {from} found\n\
         status: current\n\
         supersedes: \n\
         reads: []\n\
         created: 2026-08-23T01:00:0{step}Z\n\
         bytes: 42\n\
         est_tokens: 11\n\
         ---\n\n\
         ## Answer\n\n\
         {from} looked at it.\n"
    )
}

/// Zakłada projekt z jednym biegiem i jednym przekazaniem w środku.
fn a_project_with_one_handoff(root: &Path, run: &str, from: &str) -> Result<(), Box<dyn Error>> {
    let handoffs = root
        .join(".loadout")
        .join("runs")
        .join(run)
        .join("handoffs");
    fs::create_dir_all(&handoffs)?;
    fs::write(handoffs.join("00__step__findings.md"), a_handoff("1", from))?;
    Ok(())
}

#[test]
fn the_list_answers_about_the_folder_it_was_given() -> Result<(), Box<dyn Error>> {
    let one = tempfile::tempdir()?;
    let two = tempfile::tempdir()?;
    let empty = tempfile::tempdir()?;
    a_project_with_one_handoff(one.path(), "20260823-010000__01a00000-aaaa", "Planner")?;
    a_project_with_one_handoff(two.path(), "20260823-020000__01a00000-bbbb", "Reviewer")?;

    let first = list_handoffs_inner(one.path())?;
    let second = list_handoffs_inner(two.path())?;

    assert_eq!(
        first
            .iter()
            .map(|one| one.from.as_str())
            .collect::<Vec<_>>(),
        vec!["Planner"],
        "the first folder has exactly one thing passed along, by Planner. It answered {first:?}"
    );
    /* TA ASERCJA JEST CAŁYM KRYTERIUM. Bez niej przechodzi funkcja, która czyta zawsze ten sam
     * katalog — a to jest defekt, przez który sekcja Pamięć stała pusta nad folderem pełnym
     * plików. Dwa foldery, dwie odpowiedzi, albo zakres jest ozdobą. */
    assert_eq!(
        second
            .iter()
            .map(|one| one.from.as_str())
            .collect::<Vec<_>>(),
        vec!["Reviewer"],
        "asked about the second folder it answered about the first. Whatever folder is chosen \
         in the side menu is the only thing this list is about. It answered {second:?}"
    );
    assert!(
        list_handoffs_inner(empty.path())?.is_empty(),
        "a folder that has run nothing yet has to come back empty, not refused: an empty list \
         is what the invitation on the screen is for, and a refusal there reads as a fault"
    );
    Ok(())
}
