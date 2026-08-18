//! AC-4 dla T-12: dwa kroki, które **mogą biec równocześnie**, nie piszą do jednego folderu —
//! i odmowa pada najpóźniej przy Starcie, nigdy w trakcie biegu (niezmiennik 12).
//!
//! WAGA UWAGI ZALEŻY OD TEGO, PO CO PYTAMY, i to jest rozstrzygnięcie właściciela z 2026-08-19.
//! Przy zapisie kolizja jest OSTRZEŻENIEM, przy Run PROBLEMEM. Powód jest mierzony na edytorze:
//! kafelki dokłada się na płótno luzem i dopiero potem łączy strzałkami — inaczej nie da się
//! zbudować trzech gałęzi wchodzących do jednego kroku. Dopóki ta reguła odmawiała przy zapisie,
//! drugi dołożony kafelek robił z dokumentu plik niezapisywalny i praca człowieka żyła wyłącznie
//! w pamięci okna. Bieg na tym nie traci nic: `check_to_run` woła się przed uruchomieniem
//! czegokolwiek, więc odmowa dalej wyprzedza pierwszego agenta.
//!
//! DLATEGO KAŻDY PRZYPADEK KOLIZJI JEST TU SĄDZONY DWA RAZY. Sam zapis nie rozróżnia dziś
//! „reguła przepuściła, bo nie ma kolizji" od „reguła przepuściła, bo jej nie ma" — obie dają
//! zero problemów. Przypadki (b) i (d) mierzą więc obie wagi naraz, a (c) i (d2) — te, w których
//! kolizji NIE MA — sądzone są `check_to_run`, czyli najsurowszym pytaniem, jakie ten walidator
//! zna. Wersja pytająca tylko `check` przechodziłaby po wyłączeniu reguły.
//!
//! „Mogą biec równocześnie" znaczy dokładnie jedno: **nie istnieje ścieżka po strzałkach** ani
//! z A do B, ani z B do A. Reguła, która porównuje folder na *wszystkich* parach kroków, jest
//! tą samą regułą pozbawioną tego zdania — i wtedy zwykły łańcuch `plan → build` jest odmową.
//! Ktoś zgłasza to jako błąd, ktoś inny „naprawia" regułę przez wyłączenie jej i zostaje martwy
//! kod. Dlatego przypadek (a) — łańcuch dzielący folder projektu — musi dać **zero** uwag.
//!
//! Słabą wersją jest porównanie pola `folder` przez `==`. Przechodzi przypadki (b) i (c),
//! a wykłada się na (d) i (e) — czyli dokładnie na tych dwóch, w których agenci naprawdę
//! nadpisują sobie pliki: zagnieżdżona ścieżka i jeden krok w kilku kopiach. Oba są w tym
//! samym pliku i to one nadają temu kryterium sens.
//!
//! Zagnieżdżenie porównujemy **po segmentach**, nie po prefiksie stringa: `/Users/x/api2` nie
//! leży w `/Users/x/api`, choć zaczyna się tymi samymi znakami. Fixture (d2) jest tu po to,
//! żeby najtańsza implementacja — `starts_with` na tekście — świeciła na czerwono.

use std::error::Error;

use serde_json::{Value, json};

use loadout_lib::workflow::WorkflowFile;
use loadout_lib::workflow::check::{Level, Note, check, check_to_run};

/// Zdanie z kryterium. Nazywa **oba** kroki — bez nich użytkownik wie, że coś koliduje, ale nie
/// wie z czym — i mówi, co zrobić.
const AT_THE_SAME_TIME: &str = "\"Research\" and \"Check\" can run at the same time and both \
     work in the project folder. Give one of them a fresh copy.";

/// Krok o zadanym folderze. Wszystko poza folderem jest kompletne, żeby żadna inna reguła nie
/// dołożyła drugiej uwagi do fixture, która mierzy tę jedną.
fn step(id: &str, name: &str, folder: &Value) -> Value {
    json!({
        "kind": "agent",
        "id": id,
        "name": name,
        "agent": "a_forge",
        "instructions": "Do the work.",
        "folder": folder
    })
}

fn project() -> Value {
    json!({ "use": "project" })
}

fn fresh_copy() -> Value {
    json!({ "use": "fresh-copy" })
}

fn pick(path: &str) -> Value {
    json!({ "use": "pick", "path": path })
}

fn workflow(steps: &[Value], links: &[(&str, &str)]) -> Result<WorkflowFile, Box<dyn Error>> {
    let links: Vec<Value> = links
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    let file = json!({
        "format": 1,
        "id": "wf_test",
        "name": "Test workflow",
        "steps": steps,
        "links": links
    });
    Ok(serde_json::from_value(file)?)
}

fn problems(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Problem)
        .collect()
}

/// Uwagi wagi „ostrzeżenie". Zapis ich nie blokuje — i to jest cała różnica, którą mierzy
/// przypadek (b): plik ma się zapisać, a człowiek ma i tak przeczytać zdanie o kolizji.
fn warnings(notes: &[Note]) -> Vec<&Note> {
    notes
        .iter()
        .filter(|note| note.level == Level::Warning)
        .collect()
}

#[test]
fn a_chain_may_share_the_project_folder() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", &project()),
            step("b", "Check", &project()),
        ],
        &[("a", "b")],
    )?;

    let notes = check(&workflow);

    assert!(
        notes.is_empty(),
        "`b` starts after `a` finishes, so they never write at the same time — this is the \
         most ordinary workflow there is and refusing it makes the rule the first thing \
         somebody switches off. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn two_steps_with_no_arrow_warn_at_save_and_refuse_at_run() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", &project()),
            step("b", "Check", &project()),
        ],
        &[],
    )?;

    /* PRZY ZAPISIE: ostrzeżenie i ANI JEDEN problem. Dwa kafelki leżące luzem to normalny stan
     * pracy na płótnie — człowiek dopiero pociągnie strzałkę — a `workflow::file::save` odmawia
     * na pierwszym problemie, więc problem w tym miejscu zamyka plik na klucz i kasuje autosave. */
    let saving = check(&workflow);
    let warned = warnings(&saving);

    assert!(
        problems(&saving).is_empty(),
        "a draft where the tiles are not wired up yet has to SAVE: the refusal at save time is \
         exactly what stopped `+ Add step` from ever placing a loose tile. Got: {saving:?}"
    );
    assert!(
        warned.iter().any(|note| note.message == AT_THE_SAME_TIME),
        "warning, not silence: the human has to read at save time the same sentence that will \
         stop Start, or the collision is a surprise at the worst moment. Got: {saving:?}"
    );

    /* PRZY RUN: ten sam plik, to samo zdanie, waga problemu. Bez tej połowy reguła jest ozdobą:
     * dwie gałęzie nadpisywałyby sobie pliki, a walidator tylko by o tym wspomniał. */
    let running = check_to_run(&workflow);
    let refused = problems(&running);

    assert_eq!(
        refused.len(),
        1,
        "one collision between two steps is one thing to fix, and before a run it has to be a \
         REFUSAL — agents overwriting one another is the failure this whole rule exists for. \
         Got: {running:?}"
    );
    assert_eq!(
        refused[0].message, AT_THE_SAME_TIME,
        "the message has to name both steps and say what to do; 'path conflict' names neither"
    );
    Ok(())
}

#[test]
fn a_fresh_copy_takes_the_collision_away() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", &project()),
            step("b", "Check", &fresh_copy()),
        ],
        &[],
    )?;

    /* `check_to_run`, nie `check`: przy zapisie kolizja jest dziś tylko ostrzeżeniem, więc
     * pytanie o problemy przechodziłoby także dla reguły WYŁĄCZONEJ. Run jest najsurowszym
     * pytaniem, jakie ten walidator zna, i tylko ono rozstrzyga, że świeża kopia naprawdę
     * zdejmuje kolizję. */
    let notes = check_to_run(&workflow);

    assert!(
        problems(&notes).is_empty(),
        "'a fresh copy just for this step' is the answer the message tells the user to pick, \
         so taking it has to actually solve the problem — including at Start, which is where \
         the refusal now lands. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_folder_inside_the_other_folder_is_the_same_collision() -> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", &pick("/Users/x/api")),
            step("b", "Check", &pick("/Users/x/api/src")),
        ],
        &[],
    )?;

    let notes = check_to_run(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "one step writing inside the other's folder is the same overwriting, so it is the same \
         one problem — comparing the two folders with `==` misses it entirely. Got: {notes:?}"
    );
    let message = &problems[0].message;
    assert!(
        message.contains("Research") && message.contains("Check"),
        "both steps have to be named: the user has to know which pair to separate. It reads: \
         {message}"
    );
    Ok(())
}

#[test]
fn a_folder_that_merely_starts_with_the_same_letters_is_not_a_collision()
-> Result<(), Box<dyn Error>> {
    let workflow = workflow(
        &[
            step("a", "Research", &pick("/Users/x/api")),
            step("b", "Check", &pick("/Users/x/api2")),
        ],
        &[],
    )?;

    /* Znowu Run: przy zapisie ta asercja przechodziłaby dla reguły wyłączonej. */
    let notes = check_to_run(&workflow);

    assert!(
        problems(&notes).is_empty(),
        "`/Users/x/api2` is a different folder that happens to share a prefix; refusing it \
         means the rule compares text instead of path segments, and then the user is told to \
         fix something that is not broken. Got: {notes:?}"
    );
    Ok(())
}

#[test]
fn a_step_in_several_copies_collides_with_itself() -> Result<(), Box<dyn Error>> {
    let mut step = step("a", "Research", &project());
    step["copies"] = json!(3);
    let workflow = workflow(&[step], &[])?;

    let notes = check(&workflow);
    let problems = problems(&notes);

    assert_eq!(
        problems.len(),
        1,
        "three copies of one step run at the same time by definition, so one folder for all \
         three is three agents overwriting one another. This is the one branch of the rule that \
         stays a refusal AT SAVE: a pair without an arrow is a passing state the human fixes \
         with a gesture on the canvas, but a step colliding with itself has no arrow that would \
         fix it — only a field does, so there is nothing to wait for. Got: {notes:?}"
    );
    assert_eq!(
        problems[0].step_id.as_deref(),
        Some("a"),
        "the badge belongs on the step that carries the copies"
    );
    Ok(())
}
