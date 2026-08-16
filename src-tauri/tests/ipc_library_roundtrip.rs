//! AC-3 dla T-27: agent zapisany przez komendę wraca z listy i znika po usunięciu.
//!
//! Na katalogu tymczasowym, przez funkcje `*_inner` — **bez Tauri**. To jest cały powód, dla
//! którego warstwa komend bierze katalog argumentem zamiast sięgać po `State<'_, AppState>`:
//! stanu Tauri nie da się zbudować w teście, a `&Path` da się w jednym wierszu [04 §2.1].
//! Kryterium wymagające żywego okna nie umie być czerwone z właściwego powodu, bo `Failed to
//! launch` stoi na liście `NOT_A_REAL_RED`.
//!
//! # Dlaczego odczyt idzie DRUGĄ komendą
//!
//! **Słaba wersja tego kryterium: zapisz i sprawdź, że funkcja zwróciła `Ok`.** Przechodzi ją
//! implementacja pisząca do `/dev/null` — a to jest cała różnica między „komenda istnieje"
//! a „komenda cokolwiek robi". Rozróżnia je odczyt **inną** komendą i porównanie WARTOŚCI:
//! zapisany agent ma wrócić z listy z tymi samymi piętnastoma polami, a nie z tym samym kodem
//! wyjścia (niezmiennik 19 w duchu: kod powrotu nie jest dowodem).
//!
//! Usunięcie ma dwie asercje z tego samego powodu. „Nie ma go na liście" przechodzi na
//! implementacji, która odfiltrowała wiersz i zostawiła plik — czyli na agencie, który wraca
//! po restarcie (pliki są prawdą, niezmiennik 4). Dlatego druga asercja pyta dysk.

use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

use loadout_lib::commands::agents::{delete_agent_inner, list_agents_inner, save_agent_inner};
use loadout_lib::commands::mint::new_id_inner;
use loadout_lib::library::agents::{Agent, Color, Thinking};
use tempfile::TempDir;

/// Agent różniący się od sąsiada w czterech polach naraz.
///
/// Cztery, nie jedno: porównanie całej struktury jest tanie, a fikstura, w której dwaj agenci
/// różnią się wyłącznie nazwą, przechodzi na implementacji gubiącej wszystko poza nazwą.
/// `id` przychodzi z **mennicy**, nie z tego pliku — front nie wybija identyfikatorów.
fn agent(name: &str, summary: &str, color: Color, thinking: Thinking) -> Agent {
    Agent {
        id: new_id_inner(),
        name: name.to_owned(),
        summary: summary.to_owned(),
        color,
        thinking,
        ..Agent::example()
    }
}

#[test]
fn an_agent_saved_by_the_command_comes_back_from_the_list_and_leaves_with_it()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;

    let forge = agent("Forge", "Writes code", Color::Clay, Thinking::Balanced);
    let scribe = agent(
        "Scribe",
        "Writes down what happened",
        Color::Moss,
        Thinking::Deep,
    );

    let forge_file = save_agent_inner(home.path(), &forge)?;
    let scribe_file = save_agent_inner(home.path(), &scribe)?;
    assert_ne!(
        forge_file, scribe_file,
        "two agents are two files. One path for both means the second save landed on the \
         first, and the list would then be one row short with nothing to explain it"
    );

    let both = list_agents_inner(home.path())?;
    assert_eq!(
        both.len(),
        2,
        "two agents went in, so two come back. Got: {both:?}"
    );

    let saved_forge = both
        .iter()
        .find(|one| one.id == forge.id)
        .ok_or("the agent that was saved first is not in the list the command handed back")?;
    assert_eq!(
        *saved_forge, forge,
        "the agent comes back with the values it went in with — every field, not just the name. \
         Comparing the whole definition is what tells a real write apart from one that kept the \
         name and dropped the settings"
    );

    let saved_scribe = both
        .iter()
        .find(|one| one.id == scribe.id)
        .ok_or("the agent that was saved second is not in the list the command handed back")?;
    assert_eq!(*saved_scribe, scribe, "and so does the second one");

    delete_agent_inner(home.path(), &forge.id.to_string())?;

    let left = list_agents_inner(home.path())?;
    assert_eq!(
        left,
        vec![scribe],
        "one agent was removed, so the other one — untouched — is what is left. Got: {left:?}"
    );
    assert!(
        !forge_file.exists(),
        "the row is gone from the list but {} is still on disk. Files are the truth and the \
         index is only an index (invariant 4), so this agent comes back at the next start and \
         looks like a failed delete",
        forge_file.display()
    );
    assert!(
        scribe_file.exists(),
        "removing one agent took the other one's file down with it: {} is gone",
        scribe_file.display()
    );
    Ok(())
}

#[test]
fn the_mint_hands_out_ids_that_sort_by_the_time_they_were_made() {
    let first = new_id_inner();
    // Pięć milisekund, bo v7 niesie czas z dokładnością do milisekundy: bez przerwy oba
    // identyfikatory mogą wypaść w tej samej i porównanie mówiłoby wtedy o losowym ogonie,
    // a nie o czasie.
    sleep(Duration::from_millis(5));
    let second = new_id_inner();

    assert_ne!(
        first, second,
        "the mint handed out the same id twice; two agents with one id are one agent written \
         down two ways, and the one saved later wins"
    );
    assert_eq!(
        first.get_version_num(),
        7,
        "an id has to be v7, the kind that carries the time it was made. v4 is a random number: \
         it works as a name and gives nothing to sort by, which is why the window is not allowed \
         to make one with `crypto.randomUUID()` [T4 §5.1]. Got: {first}"
    );
    assert_eq!(
        second.get_version_num(),
        7,
        "and so does the second one. Got: {second}"
    );
    assert!(
        first.to_string() < second.to_string(),
        "an id made later has to sort after one made earlier — that is the whole reason for \
         v7, and it is what lets a list of agents come out in the order they were created \
         without a single date field. Got {first} then {second}"
    );
}
