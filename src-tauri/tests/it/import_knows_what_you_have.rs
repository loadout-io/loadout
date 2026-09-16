//! 2026-09-16 — drugi import właściciela, Loadout 0.6.2, projekt urc-monorepo.
//!
//! Ekran oddał czerwony pasek: „agents/project-manager-backlog.md already exists. Nothing was
//! imported." Pierwszy import wniósł ten plik, cztery pliki połączeń, 23 notatki i dwie
//! umiejętności — a drugi odmawiał W CAŁOŚCI na pierwszym z nich, nie oznaczając na ekranie ani
//! jednego wiersza, który już wylądował. Stan „już to mam" jest normalny, nie awaryjny, i był
//! znany JUŻ PRZY SKANIE: odpowiada na niego dysk biblioteki.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::import::{ApplySetup, apply_setup_inner, scan_setup_inner};
use loadout_lib::commands::memory::notes_root;
use loadout_lib::import::{ImportStatus, ItemKind, MigrationDraft};

/// Agent, którego biblioteka dostała przy PIERWSZYM imporcie. To on wywracał drugi.
const KEEPER: &str = "---\n\
                      name: keeper\n\
                      description: Keeps the backlog.\n\
                      ---\n\
                      Keep the backlog tidy.\n";

/// Agent, którego biblioteka jeszcze nie ma — cała reszta importu, w jednym pliku.
const NEWCOMER: &str = "---\n\
                        name: newcomer\n\
                        description: Writes the notes.\n\
                        ---\n\
                        Write down what happened.\n";

/// Agent, który WYMAGA umiejętności leżącej już w bibliotece. Bez niego ten zestaw nie sądziłby
/// najdroższej połowy wady: odznaczona pozycja przestaje domykać cudze zależności, więc import
/// odmawia drugi raz — tym razem zdaniem o brakującej umiejętności, której człowiek ma na dysku.
const FRONTEND: &str = "---\n\
                        name: frontend\n\
                        description: Builds the screen.\n\
                        skills: design-system-reference\n\
                        ---\n\
                        Build the screen the design system describes.\n";

const SKILL: &str = "---\n\
                     name: design-system-reference\n\
                     description: Explains the design system\n\
                     ---\n\
                     Follow the design system.\n";

/// Agent, który wymienia serwer narzędziowy w swoim nagłówku — kształt żywcem z repo właściciela
/// (`design-qa.md`, `e2e-author.md` i `figma-extractor.md` mają dokładnie ten klucz).
const NEEDS_A_SERVER: &str = "---\n\
                              name: design-qa\n\
                              description: Checks the screen.\n\
                              mcpServers:\n  \
                                playwright:\n    \
                                  type: http\n    \
                                    url: http://127.0.0.1:3846/mcp\n\
                              ---\n\
                              Check the screen against the design.\n";

/// Indeks wiązki pamięci: JEDEN wiersz na ekranie, DWA pliki na dysku.
///
/// Kształt żywcem z `.claude/agent-memory/<agent>/` — pliki obok `MEMORY.md` nie są osobnymi
/// pozycjami, więc cały katalog jedzie jednym ptaszkiem.
const MEMORY_INDEX: &str = "# What backend-dev learned here\n\
                            \n\
                            - [The queue is drained in one place](queue.md) — one drain\n\
                            - [The tenant is resolved first](tenant.md) — before the guard\n";

/// Strona pamięci w kształcie, z którego import robi notatkę: tytuł, zdanie, uzasadnienie.
fn memory_page(title: &str, rule: &str) -> String {
    format!(
        "# {title}\n\nMOOSE-{rule} and this is what would reach the model.\n\nWhy: it cost a run to find out\n"
    )
}

/// Bajty, które leżą w bibliotece PRZED tym importem — inne niż te w projekcie z rozmysłem:
/// „nie ruszył" da się odróżnić od „nadpisał tym samym" wyłącznie na różniącej się treści.
const KEEPER_IN_THE_LIBRARY: &str = "---\nname: keeper\n---\nThe copy a person already edited.\n";
const SKILL_IN_THE_LIBRARY: &str = "---\nname: design-system-reference\n---\nThe edited copy.\n";

/// Projekt i biblioteka właściciela w miniaturze: trzy pliki agentów, jedna umiejętność, a po
/// stronie biblioteki dwie z tych czterech rzeczy już leżą.
fn a_project_and_a_library_that_has_some_of_it()
-> Result<(tempfile::TempDir, tempfile::TempDir), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    fs::create_dir_all(repo.path().join(".claude/agents"))?;
    fs::write(repo.path().join(".claude/agents/keeper.md"), KEEPER)?;
    fs::write(repo.path().join(".claude/agents/newcomer.md"), NEWCOMER)?;
    fs::write(repo.path().join(".claude/agents/frontend.md"), FRONTEND)?;
    fs::create_dir_all(repo.path().join(".claude/skills/design-system-reference"))?;
    fs::write(
        repo.path()
            .join(".claude/skills/design-system-reference/SKILL.md"),
        SKILL,
    )?;

    fs::create_dir_all(library.path().join("agents"))?;
    fs::write(
        library.path().join("agents/keeper.md"),
        KEEPER_IN_THE_LIBRARY,
    )?;
    fs::create_dir_all(library.path().join("skills/design-system-reference"))?;
    fs::write(
        library
            .path()
            .join("skills/design-system-reference/SKILL.md"),
        SKILL_IN_THE_LIBRARY,
    )?;
    Ok((repo, library))
}

/// Dokładnie to, co odznacza ekran: pozycje niegotowe ORAZ te, których biblioteka już ma
/// (`typedExcludedIn` w `src/sections/import/setup.tsx`).
fn what_the_screen_leaves_out(draft: &MigrationDraft) -> Vec<String> {
    draft
        .items
        .iter()
        .filter(|item| item.status != ImportStatus::Ready || item.already_here)
        .map(|item| item.id.clone())
        .collect()
}

#[test]
fn a_second_import_lands_everything_the_library_does_not_have_yet() -> Result<(), Box<dyn Error>> {
    let (repo, library) = a_project_and_a_library_that_has_some_of_it()?;
    // Pusty katalog domowy: ten zestaw sądzi import PROJEKTU i nie ma prawa czytać
    // `~/.claude.json` człowieka, który akurat uruchomił testy.
    let nothing = tempfile::tempdir()?;

    let preview = scan_setup_inner(nothing.path(), repo.path(), library.path())?;
    let left_out = what_the_screen_leaves_out(&preview.draft);
    assert_eq!(
        left_out.len(),
        2,
        "the scan has to mark BOTH things the library already has, or the screen leaves a person \
         hunting for them by eye among dozens of rows"
    );
    /* TYMI SAMYMI KLUCZAMI, KTÓRE CZYTA OKNO (`alreadyHere` i `alreadyInTheLibrary`
     * w `src/sections/import/setup.tsx`). Przemianowanie po tej stronie granicy nie jest błędem
     * kompilacji — jest ekranem, który o tych wierszach milczy, czyli dokładnie tym, co ten
     * zestaw naprawia. */
    let wire = serde_json::to_value(&preview.draft)?;
    assert!(
        wire["items"]
            .as_array()
            .ok_or("the plan on the wire has no items at all")?
            .iter()
            .any(|item| item["alreadyHere"] == serde_json::Value::Bool(true)),
        "the row the window unticks has to carry that answer across the boundary"
    );
    assert!(
        !wire["alreadyInTheLibrary"]
            .as_array()
            .ok_or("the plan on the wire does not say which files the library already has")?
            .is_empty(),
        "and so does the list the connection ticks read"
    );

    apply_setup_inner(
        library.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: left_out,
            without_behavior: vec![],
        },
    )?;

    assert!(
        library.path().join("agents/newcomer.md").is_file(),
        "everything the library did not have yet has to land; before this, the whole import \
         refused on the first file that already existed and not one file was written"
    );
    assert!(
        library.path().join("agents/frontend.md").is_file(),
        "including the agent whose skill is already in the library — an unticked row still has \
         to close the dependencies of the rows around it"
    );
    assert_eq!(
        fs::read_to_string(library.path().join("agents/keeper.md"))?,
        KEEPER_IN_THE_LIBRARY,
        "and the file that was already there keeps its own bytes: Loadout does not replace what \
         it did not write"
    );
    assert_eq!(
        fs::read_to_string(
            library
                .path()
                .join("skills/design-system-reference/SKILL.md")
        )?,
        SKILL_IN_THE_LIBRARY,
        "same for the skill bundle"
    );
    Ok(())
}

#[test]
fn ticking_something_you_already_have_says_how_many_clash() -> Result<(), Box<dyn Error>> {
    let (repo, library) = a_project_and_a_library_that_has_some_of_it()?;
    let nothing = tempfile::tempdir()?;

    let preview = scan_setup_inner(nothing.path(), repo.path(), library.path())?;
    // Ptaszki postawione z powrotem przy OBU pozycjach, których biblioteka już ma — jawna decyzja
    // człowieka o wniesieniu ich mimo wszystko. Odmowa zostaje, bo nadpisania tu nie ma; ma tylko
    // przestać nazywać jeden plik z pięćdziesięciu.
    let refusal = apply_setup_inner(
        library.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: preview.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: vec![],
            without_behavior: vec![],
        },
    )
    .err()
    .ok_or("ticking a file the library already has cannot quietly overwrite it")?
    .to_string();

    assert!(
        refusal.contains("2 row(s)"),
        "the refusal has to say HOW MANY rows clash; naming one file out of fifty leaves a person \
         re-running the import once per file: {refusal}"
    );
    assert!(
        refusal.contains("untick") || refusal.contains("Untick"),
        "and it has to name the move that gets past it: {refusal}"
    );
    assert!(
        !refusal.contains(&format!(
            "{} already exists",
            Path::new("agents/keeper.md").display()
        )),
        "and it stops singling out the first file it tripped over: {refusal}"
    );
    Ok(())
}

/// JEDEN PTASZEK, DWA PLIKI — i odmowa ma mówić o jednym ptaszku.
///
/// 2026-09-17, po weryfikacji: licznik kolidujących PLIKÓW podany jako licznik WIERSZY jest tą
/// samą wadą, co nazywanie jednego pliku z pięćdziesięciu, tylko odwróconą — każe szukać dwóch
/// ptaszków tam, gdzie stoi jeden. Wiązka `.claude/agent-memory/<agent>/` jest najkrótszą drogą
/// do tego stanu: pliki obok `MEMORY.md` nie są osobnymi pozycjami, więc dwie strony pamięci
/// jadą JEDNYM wierszem.
#[test]
fn a_row_that_is_two_files_counts_as_one_row() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let library = tempfile::tempdir()?;
    let nothing = tempfile::tempdir()?;
    let remembered = repo.path().join(".claude/agent-memory/backend-dev");
    fs::create_dir_all(&remembered)?;
    fs::write(remembered.join("MEMORY.md"), MEMORY_INDEX)?;
    fs::write(
        remembered.join("queue.md"),
        memory_page("The queue is drained in one place", "ONE-DRAIN"),
    )?;
    fs::write(
        remembered.join("tenant.md"),
        memory_page("The tenant is resolved first", "BEFORE-THE-GUARD"),
    )?;
    fs::create_dir_all(repo.path().join(".claude/agents"))?;
    fs::write(repo.path().join(".claude/agents/keeper.md"), KEEPER)?;

    // PIERWSZY import — ten, po którym biblioteka ma już obie strony pamięci.
    let first = scan_setup_inner(nothing.path(), repo.path(), library.path())?;
    apply_setup_inner(
        library.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: first.draft.source_hashes.clone(),
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: what_the_screen_leaves_out(&first.draft),
            without_behavior: vec![],
        },
    )?;
    let landed = fs::read_dir(notes_root(library.path()).join("notes"))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|kind| kind == "md"))
        .count();
    assert_eq!(
        landed, 2,
        "this fixture only judges what it means to judge if that one row really is two files"
    );

    // DRUGI import, z ptaszkiem postawionym z powrotem przy TYM JEDNYM wierszu pamięci.
    let second = scan_setup_inner(nothing.path(), repo.path(), library.path())?;
    let bundle = second
        .draft
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Memory)
        .ok_or("the memory bundle has no row at all")?;
    assert!(
        bundle.already_here,
        "the scan has to mark the whole bundle, or there is no tick to put back"
    );
    let ticked_back = bundle.id.clone();
    let excluded: Vec<String> = what_the_screen_leaves_out(&second.draft)
        .into_iter()
        .filter(|id| *id != ticked_back)
        .collect();

    let refusal = apply_setup_inner(
        library.path(),
        nothing.path(),
        &ApplySetup {
            workspace: repo.path().to_path_buf(),
            expected_source_hashes: second.draft.source_hashes,
            enable_connections: vec![],
            leave_out: vec![],
            excluded_items: excluded,
            without_behavior: vec![],
        },
    )
    .err()
    .ok_or("the library already has both of those pages, so this cannot quietly overwrite them")?
    .to_string();

    assert!(
        refusal.contains("1 row(s)"),
        "one tick went back on, so the refusal has to name ONE row: {refusal}"
    );
    assert!(
        !refusal.contains("2 row(s)"),
        "counting files as rows sends a person hunting for a second tick that does not exist: \
         {refusal}"
    );
    assert!(
        refusal.contains("2 of the file(s)"),
        "and the file count stays true next to it, because that is what the library really has: \
         {refusal}"
    );
    Ok(())
}

/// DLACZEGO LICZNIK „AGENTS" POKAZUJE MNIEJ, NIŻ JEST PLIKÓW — i czy ktokolwiek to mówi.
///
/// U właściciela `.claude/agents/` ma trzynaście plików, a licznik pokazał dziesięć. Trzy z nich
/// (`design-qa.md`, `e2e-author.md`, `figma-extractor.md`) wymieniają serwer narzędziowy
/// w nagłówku, a połączenia startują wyłączone — więc wiersz staje się zablokowany i ekran
/// odznacza go za człowieka. Reguła jest UZASADNIONA i zostaje: Loadout nie odtworzy agenta bez
/// serwera, którego ten agent wymienia. Zdaniem, które było wadą, było `Blocked because
/// connection:figma will not be imported or enabled.` — klucz z drutu na ekranie (niezmiennik 14)
/// i ani słowa o ruchu, który to odblokowuje.
#[test]
fn a_row_blocked_by_a_tool_server_says_which_one_and_what_to_do() -> Result<(), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    let nothing = tempfile::tempdir()?;
    fs::create_dir_all(repo.path().join(".claude/agents"))?;
    fs::write(repo.path().join(".claude/agents/keeper.md"), KEEPER)?;
    fs::write(repo.path().join(".claude/agents/newcomer.md"), NEWCOMER)?;
    fs::write(
        repo.path().join(".claude/agents/design-qa.md"),
        NEEDS_A_SERVER,
    )?;

    let preview = scan_setup_inner(nothing.path(), repo.path(), nothing.path())?;
    let ticked = preview
        .draft
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Agent && item.status == ImportStatus::Ready)
        .count();
    assert_eq!(
        ticked, 2,
        "three agent files, two rows ticked — this is the whole arithmetic behind a counter that \
         said ten over thirteen files"
    );

    let blocked = preview
        .draft
        .items
        .iter()
        .find(|item| item.status == ImportStatus::MissingDependencies)
        .ok_or("the agent that names a tool server is not blocked at all")?;
    assert!(
        blocked.status_message.contains("playwright"),
        "the row has to name the one thing that is missing: {}",
        blocked.status_message
    );
    assert!(
        !blocked.status_message.contains("connection:"),
        "and it has to name it in words, never by its wire key (invariant 14): {}",
        blocked.status_message
    );
    assert!(
        blocked.status_message.contains("tick"),
        "and it has to say what to do about it, or a person reads a dead end: {}",
        blocked.status_message
    );
    Ok(())
}
