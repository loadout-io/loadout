/* Lista kart zapisuje MIEJSCE, a nie pisownię, którą człowiek akurat wpisał (2026-09-07).
 *
 * PO CO TO ISTNIEJE. Klucz tej listy jest ścieżką folderu, a od chwili, w której korzeń projektu
 * jest rozwiązywany na wejściu (`ipc::project_folder`), reszta aplikacji zna projekt wyłącznie po
 * pisowni rzeczywistej. Lista pisana surowym napisem z okna dawała więc DWA wiersze nad jednym
 * folderem — a zapadka „jeden bieg naraz" liczy tożsamość kanonicznie
 * (`workspace::WorkspaceId`), więc druga karta wyglądałaby na wiecznie zajętą przez pierwszą.
 * Wyzwalacz nad takim projektem odmawiał wprost: `WorkspaceMismatch`, bo zamrożony w nim
 * workspace stoi tak, jak zapisała go lista, a bieg pyta o pisownię rozwiązaną.
 *
 * TO NIE JEST WADA SKRÓTÓW. Ten sam wiersz rozjedzie się przy `~/Projects/./x`, przy ukośniku
 * na końcu i po przeniesieniu katalogu. Skrót jedynie odsłania klasę: klucz zapisany jako tekst
 * ścieżki musi być liczony jednym rachunkiem, tym samym, co tożsamość karty.
 *
 * DRUGI PRZYPADEK jest o ludziach, którzy mają już plik na dysku: wiersz zapisany przed tą
 * zmianą ma się czytać jako to samo miejsce, bez proszenia kogokolwiek o ponowne dodanie karty.
 *
 * TRZECI pilnuje granicy tej migracji: wpis wskazujący na folder, którego nie ma, zostaje
 * DOKŁADNIE taki, jaki był. Powód stoi przy `list_workspaces_inner` — zniknięcie karty, bo dysk
 * zewnętrzny nie jest podłączony, wygląda jak utrata pracy.
 */
use std::error::Error;
use std::fs;

use loadout_lib::commands::workspaces::{
    WorkspaceWire, list_workspaces_inner, save_workspace_inner,
};

#[test]
fn two_spellings_of_one_folder_keep_one_row() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let real = tempfile::tempdir()?;
    let link = home.path().join("project-link");
    std::os::unix::fs::symlink(real.path(), &link)?;

    let after_link = save_workspace_inner(
        home.path(),
        "Through the link",
        link.to_str().ok_or("the link path is not valid text")?,
    )?;
    assert_eq!(
        after_link.len(),
        1,
        "the first save did not produce a row at all"
    );
    assert_eq!(
        after_link[0].folder,
        std::fs::canonicalize(&link)?.to_string_lossy(),
        "the list kept the spelling a person typed instead of the folder it names, so every \
         other part of the application — which knows this project by its real path — reads this \
         row as a different project"
    );

    let after_real = save_workspace_inner(
        home.path(),
        "The same folder",
        real.path().to_str().ok_or("the path is not valid text")?,
    )?;
    assert_eq!(
        after_real.len(),
        1,
        "one folder took two rows in the switcher, and the latch that allows one run per folder \
         counts them as one — so the second card would read as busy for as long as the first one \
         runs. It holds: {after_real:?}"
    );
    assert_eq!(
        after_real[0].name, "The same folder",
        "the second save added nothing and renamed nothing, so a person cannot correct the name"
    );
    Ok(())
}

#[test]
fn a_row_written_before_this_change_reads_as_the_place_it_names() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let real = tempfile::tempdir()?;
    let link = home.path().join("project-link");
    std::os::unix::fs::symlink(real.path(), &link)?;

    /* Plik dokładnie taki, jaki zapisywała poprzednia wersja: pisownia z okna, znak w znak. */
    let as_written = link.to_string_lossy().into_owned();
    fs::write(
        home.path().join("workspaces.json"),
        serde_json::to_vec(&vec![WorkspaceWire {
            id: as_written.clone(),
            name: "Written before".to_owned(),
            folder: as_written.clone(),
        }])?,
    )?;

    let rows = list_workspaces_inner(home.path())?;
    assert_eq!(rows.len(), 1, "the row written before this change was lost");
    assert_eq!(
        rows[0].folder,
        std::fs::canonicalize(&link)?.to_string_lossy(),
        "a card saved by an earlier version still names a folder the rest of the application \
         does not recognise, so its triggers refuse and its runs open a second row"
    );
    assert_eq!(
        rows[0].id, rows[0].folder,
        "the key and the folder of one row stopped being the same string"
    );
    Ok(())
}

#[test]
fn a_row_whose_folder_is_gone_is_left_exactly_as_it_was() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let missing = "/Users/somebody/an-external-disk-that-is-not-plugged-in";
    fs::write(
        home.path().join("workspaces.json"),
        serde_json::to_vec(&vec![WorkspaceWire {
            id: missing.to_owned(),
            name: "On the external disk".to_owned(),
            folder: missing.to_owned(),
        }])?,
    )?;

    let rows = list_workspaces_inner(home.path())?;
    assert_eq!(
        rows,
        vec![WorkspaceWire {
            id: missing.to_owned(),
            name: "On the external disk".to_owned(),
            folder: missing.to_owned(),
        }],
        "a card pointing at a folder that is not mounted right now was rewritten or dropped. It \
         has to stay exactly as it was: disappearing from the switcher reads as lost work"
    );
    Ok(())
}

/* CZWARTY PRZYPADEK: wyzwalacz zamrożony pisownią sprzed tej zmiany dalej nazywa swój projekt.
 *
 * Plik wyzwalacza niesie workspace jako TEKST, a `require_registered_workspace` sprawdzał go
 * równością napisów wobec listy kart. Od chwili, w której lista nazywa miejsce, ten sam tekst
 * przestawał się w niej znajdować — i wyzwalacz zapisany wczoraj odmawiał dziś
 * `WorkspaceNotRegistered` nad projektem, który stoi dokładnie tam, gdzie stał.
 *
 * Kontrola dodatnia stoi obok w tym samym przypadku: folder, którego na liście NIE MA, ma dalej
 * być odmówiony. Bez niej „znajduje każdy" przeszłoby to kryterium tak samo dobrze.
 */
#[test]
fn a_trigger_frozen_with_an_older_spelling_still_names_its_project() -> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::triggers::{self, TriggerDraft};

    let home = tempfile::tempdir()?;
    let real = tempfile::tempdir()?;
    let link = home.path().join("project-link");
    std::os::unix::fs::symlink(real.path(), &link)?;
    fs::create_dir_all(home.path().join("triggers"))?;
    fs::create_dir_all(home.path().join("workflows"))?;
    fs::write(
        home.path().join("workflows/ship.json"),
        br#"{
  "format": 1,
  "id": "wf_ship",
  "name": "Ship",
  "steps": [{ "kind": "checkpoint", "id": "inspect", "name": "Inspect", "at": { "x": 0, "y": 0 } }],
  "links": []
}"#,
    )?;

    /* Karta zarejestrowana ścieżką rzeczywistą — czyli tak, jak zapisuje ją dzisiejsza lista. */
    save_workspace_inner(
        home.path(),
        "The project",
        real.path().to_str().ok_or("the path is not valid text")?,
    )?;

    let draft = |workspace: &std::path::Path| TriggerDraft {
        source: "linear".to_owned(),
        condition: "assigned-to-me".to_owned(),
        workflow: "ship.json".to_owned(),
        workspace: workspace.to_string_lossy().into_owned(),
        poll_every_minutes: 1,
        token_environment: Some("LOADOUT_TEST_TRIGGER_TOKEN".to_owned()),
    };

    let made = triggers::create_with(
        home.path(),
        draft(&link),
        || uuid::Uuid::from_u128(0x0198_a1f2_0000_7000_8000_0000_0000_0001),
        |_stage, _path| Ok(()),
    );
    assert!(
        made.is_ok(),
        "a trigger naming the project by the spelling a person typed was turned down as if that \
         project were not registered at all, so every trigger saved before the card list started \
         naming places stops firing: {:?}",
        made.err().map(|error| error.to_string())
    );

    /* KONTROLA DODATNIA: folder spoza listy dalej jest odmawiany. */
    let stranger = tempfile::tempdir()?;
    let refused = triggers::create_with(
        home.path(),
        draft(stranger.path()),
        || uuid::Uuid::from_u128(0x0198_a1f2_0000_7000_8000_0000_0000_0002),
        |_stage, _path| Ok(()),
    );
    assert!(
        refused.is_err(),
        "a folder that is on nobody's card list was accepted as a trigger's project"
    );
    Ok(())
}

/* PIATY PRZYPADEK: plik wyzwalacza zapisany wczesniej oddaje oknu pisownie, ktora lista zna.
 *
 * `require_registered_workspace` szuka juz po miejscu, wiec BIEG takiego wyzwalacza rusza. Okno
 * jednak porownuje napisy: formularz zestawia `value.workspace` z `workspace.folder` wiersza
 * listy (`src/sections/triggers/form.tsx`) i przy rozjezdzie pisze „Saved workspace is no longer
 * available", a Save blokuje zdaniem „Choose an available workspace to save this trigger."
 * Czlowiek nie ma wtedy jak poprawic wyzwalacza, ktory dziala.
 *
 * Loader ma na to precedens dwa pola wyzej: `condition` zapisane jako „assigned to me" wraca
 * z niego jako jedyny kanon „assigned-to-me". Ta sama zasada, to samo miejsce.
 */
#[test]
fn a_trigger_file_written_earlier_hands_the_window_the_spelling_the_list_knows()
-> Result<(), Box<dyn Error>> {
    use loadout_lib::commands::triggers;

    let home = tempfile::tempdir()?;
    let real = tempfile::tempdir()?;
    let link = home.path().join("project-link");
    std::os::unix::fs::symlink(real.path(), &link)?;
    fs::create_dir_all(home.path().join("triggers"))?;

    save_workspace_inner(
        home.path(),
        "The project",
        real.path().to_str().ok_or("the path is not valid text")?,
    )?;

    /* Plik dokladnie taki, jaki zapisywala wczesniejsza wersja: pisownia z okna, znak w znak. */
    fs::write(
        home.path().join("triggers/linear-old.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "source": "linear",
            "enabled": true,
            "workflow": "ship.json",
            "workspace": link.to_string_lossy(),
            "condition": "assigned-to-me",
            "poll_every_minutes": 1,
            "token_environment": "LOADOUT_TEST_TRIGGER_TOKEN"
        }))?,
    )?;

    /* Przez `list`, bo to jest droga, ktora okno naprawde czyta wyzwalacze. Prawda na dysku
    zostaje nietknieta: `load` oddaje dalej pisownie z pliku, bo karmi tez sprawdzenie
    „czy plik zmienil sie pod edytorem". */
    let listed = triggers::list(home.path())?;
    let loaded = listed
        .into_iter()
        .find(|one| one.slug == "linear-old")
        .ok_or("the trigger written by hand did not reach the window at all")?;
    assert_eq!(
        loaded.workspace.as_deref(),
        Some(
            std::fs::canonicalize(&link)?
                .to_string_lossy()
                .into_owned()
                .as_str()
        ),
        "a trigger saved before the card list started naming places hands the window a folder no \
         row on that list carries, so the editor calls its own workspace unavailable and refuses \
         to save the trigger a person is trying to correct"
    );
    Ok(())
}
