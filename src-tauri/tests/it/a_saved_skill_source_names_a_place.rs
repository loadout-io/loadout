/* Zapisane źródło umiejętności nazywa MIEJSCE, a nie jedną pisownię ścieżki (2026-09-07).
 *
 * PO CO TO ISTNIEJE. Agent zapisuje wybrane źródło umiejętności jako ścieżkę BEZWZGLĘDNĄ
 * do półki, a `bundle::resolve` sprawdza je równością napisów wobec półek policzonych z korzeni
 * TEGO projektu. Dopóki obie strony powstawały z tej samej pisowni projektu, równość działała.
 * Od chwili, w której korzeń projektu jest rozwiązywany na wejściu (`ipc::project_folder`),
 * półki liczą się z pisowni rzeczywistej, a zapis w pliku agenta niesie tę, którą człowiek
 * wpisał kiedyś — i wybór przestaje pasować do samego siebie. Zmierzone: bieg odmawiał zdaniem
 * „The selected source for <nazwa> is not a known skill folder in this project or library."
 * o półce, która stoi dokładnie tam, gdzie stała.
 *
 * TO NIE JEST WADA SKRÓTÓW. Ta sama awaria wydarzy się, gdy człowiek przeniesie albo przemianuje
 * katalog projektu: zapisana ścieżka bezwzględna przestaje pasować do policzonej. Skrót jedynie
 * odsłania klasę wady, którą jest porównywanie zapisanych ścieżek jako napisów.
 *
 * CZEGO TA POPRAWKA NIE MA PRAWA OSŁABIĆ, i drugi przypadek jest właśnie o tym. Porównanie jest
 * ZAPORĄ: „jawne źródło musi być jedną ze znanych półek TEGO projektu/użytkownika". Katalog
 * spoza półek ma dalej być odmawiany, także wtedy, gdy istnieje i da się go przeczytać.
 */
use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::skills::Roots;
use loadout_lib::skills::bundle;

const NAME: &str = "harbor-inventory";

/// Najmniejszy pakiet, który `from_source` przyjmie: katalog z `SKILL.md` i nagłówkiem.
fn a_skill_at(shelf: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(shelf)?;
    fs::write(
        shelf.join("SKILL.md"),
        format!(
            "---\nname: {NAME}\ndescription: Counts what is in the harbour.\n---\n\nCount it.\n"
        ),
    )?;
    Ok(())
}

/// Korzenie wskazujące na ten projekt i tę bibliotekę.
fn roots(home: &Path, project: &Path, data: &Path) -> Roots {
    Roots {
        home: home.to_path_buf(),
        project: Some(project.to_path_buf()),
        data: data.to_path_buf(),
    }
}

#[test]
fn a_source_written_in_another_spelling_of_the_same_folder_still_resolves()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let real = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;

    /* Półka projektu, dokładnie tam, gdzie liczy ją `place::shelves_of`. */
    let shelf = real.path().join(".claude").join("skills").join(NAME);
    a_skill_at(&shelf)?;

    /* Ta sama półka, nazwana przez skrót nad katalogiem projektu — czyli tak, jak zapisałby ją
    agent zapisany, zanim korzeń zaczął być rozwiązywany. */
    let link = home.path().join("project-link");
    std::os::unix::fs::symlink(real.path(), &link)?;
    let as_written = link.join(".claude").join("skills").join(NAME);
    assert!(
        as_written != shelf,
        "the fixture wrote the source in the same spelling the shelves are counted in, so this \
         case would pass without resolving anything"
    );

    let resolved = bundle::resolve(
        &roots(home.path(), real.path(), data.path()),
        NAME,
        Some(as_written.as_path()),
    )
    .map_err(|error| {
        format!(
            "a source that names the very shelf this project has was turned down, so an agent \
             saved before the project root was resolved can no longer run at all: {error}"
        )
    })?;

    assert_eq!(
        resolved.source, shelf,
        "the run reads the skill THROUGH the path that was written down instead of through the \
         shelf it recognised. A name that resolves to a shelf is accepted as a name; the bytes \
         have to come from the shelf, or a link swapped after the check decides what the agent reads"
    );
    Ok(())
}

#[test]
fn a_source_outside_the_known_shelves_is_still_refused() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;

    a_skill_at(&project.path().join(".claude").join("skills").join(NAME))?;
    /* Prawdziwy, czytelny pakiet — tylko nie na żadnej półce tego projektu ani biblioteki. */
    let outside = elsewhere.path().join(NAME);
    a_skill_at(&outside)?;

    let said = bundle::resolve(
        &roots(home.path(), project.path(), data.path()),
        NAME,
        Some(outside.as_path()),
    )
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();

    assert!(
        said.contains("is not a known skill folder"),
        "a folder outside the shelves of this project was accepted as a skill source, so an agent \
         file can point the run at anything on disk. It said: {said}"
    );
    Ok(())
}
