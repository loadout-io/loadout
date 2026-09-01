//! Kafelek „uruchom i zostaw" bierze wiersz powłoki od kroku przed sobą.
//!
//! # Zamówienie, dosłownie
//!
//! Właściciel 2026-08-30: „dajmy taki step o nazwie run preview app, tylko że agent sam ma
//! rozkminić jakie komendy użyć do odpalenia, my nie ingerujemy bo nie chcę w każdym projekcie
//! osobno wpisywać na front i backend command".
//!
//! # Dlaczego pole, a nie czwarty rodzaj kafelka
//!
//! D6 zabrania czwartego wprost („czwarty rodzaj dalej wymaga prawdziwej skargi z pomiarem, nie
//! wygody"), a to zamówienie jest sformułowane jako wygoda. To samo D6 mówi, co robić zamiast:
//! „nowa flaga to nowe POLE, nigdy nowy kafelek".
//!
//! # Które kryterium waży tu najwięcej
//!
//! Trzecie — o sekrecie. `workflow::check::a_command_carrying_a_secret` sądzi komendę **przy
//! zapisie**, nad tekstem z PLIKU. Komenda wyprodukowana przez agenta nie przechodzi tamtędy ani
//! razu i leci prosto do powłoki; uzasadnienie przy `const SHELL` opiera się wprost na tym, że
//! komendę napisał człowiek i że przeszła skan. Bez tego przypadku ta zmiana otwiera dziurę
//! w niezmienniku 9, i to taką, której nie widać w żadnym diffie pliku workflow — bo tam jest
//! wtedy pusto.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use loadout_lib::workflow::check::{Level, check, check_to_run};
use loadout_lib::workflow::{CommandFrom, Folder, ServeStep, Step, WorkflowFile};

/// Workflow z jednym krokiem agenta i jednym „uruchom i zostaw" za nim.
fn with_serve(serve: ServeStep) -> WorkflowFile {
    WorkflowFile {
        format: 1,
        id: "preview".to_owned(),
        name: "Run preview app".to_owned(),
        description: None,
        steps: vec![Step::Agent(agent_step()), Step::Serve(serve)],
        links: vec![loadout_lib::workflow::Link {
            from: "s_agent".to_owned(),
            to: "s_app".to_owned(),
            max_turns: None,
        }],
        extra: serde_json::Map::new(),
    }
}

/// Krok agenta, który poprzedza serwer — kompletny, żeby uwagi dotyczyły wyłącznie kafelka.
fn agent_step() -> loadout_lib::workflow::AgentStep {
    loadout_lib::workflow::AgentStep {
        id: "s_agent".to_owned(),
        name: "Work out how to start it".to_owned(),
        agent: "0195f0e0-0000-7000-8000-000000000001".to_owned(),
        overrides: serde_json::Map::new(),
        vendor_options: std::collections::BTreeMap::new(),
        copies: 1,
        instructions: "Find the one shell line that starts this app in dev.".to_owned(),
        skills: loadout_lib::workflow::Skills::default(),
        borrow: loadout_lib::workflow::Borrow::default(),
        folder: Folder::default(),
        /* PROSZONY O TO POLE, nie o samą prozę. Do 2026-08-30 stało tu `default()`, czyli krok,
         * który pola nie oddaje — a przypadek niżej nazywał się „when the step before hands one
         * over". Nazwa była nieprawdziwa, bo nikt wtedy nie sprawdzał, czy poprzednik jest o to
         * pole poproszony. */
        handover: loadout_lib::workflow::Handover::Form {
            fields: vec![loadout_lib::workflow::HandoverField {
                name: "command".to_owned(),
                describe: "the one shell line that starts this, ready to run in this project"
                    .to_owned(),
                required: Some(true),
            }],
        },
        when_it_fails: loadout_lib::workflow::WhenItFails::Stop,
        at: loadout_lib::workflow::Point::default(),
        extra: serde_json::Map::new(),
    }
}

/// Kafelek serwera z pustą komendą i wskazaniem pola.
fn waiting_for_the_field() -> ServeStep {
    ServeStep {
        id: "s_app".to_owned(),
        name: "Run preview app".to_owned(),
        command: String::new(),
        command_from: Some(CommandFrom {
            field: "command".to_owned(),
        }),
        folder: Folder::default(),
        at: loadout_lib::workflow::Point::default(),
        extra: serde_json::Map::new(),
    }
}

/// Uwagi poziomu „problem" o tym kafelku.
fn problems_about_the_app(file: &WorkflowFile) -> Vec<String> {
    check(file)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .filter(|note| note.step_id.as_deref() == Some("s_app"))
        .map(|note| note.message)
        .collect()
}

#[test]
fn an_empty_command_saves_when_the_step_before_hands_one_over() {
    let file = with_serve(waiting_for_the_field());

    assert_eq!(
        problems_about_the_app(&file),
        Vec::<String>::new(),
        "this tile has no command ON PURPOSE: the line that starts an app is different in every \
         repo, and typing it in makes one reusable file into a file for one project. Refusing it \
         here refuses exactly the shape this field exists for"
    );
}

#[test]
fn an_empty_command_still_refuses_when_nobody_hands_one_over() {
    let mut serve = waiting_for_the_field();
    serve.command_from = None;
    let file = with_serve(serve);

    let said = problems_about_the_app(&file);
    assert!(
        said.iter().any(|note| note.contains("Command to run")),
        "a tile with no command and nobody to get one from is a tile with no effect, and a run \
         that 'does' it teaches the person that it works. The refusal names the field they have \
         to fill: {said:?}"
    );
}

/// Ten sam plik, ale kroku przed kafelkiem **nikt nie prosi o pole** — oddaje samą prozę.
///
/// Osobna fikstura zamiast grzebania w gotowym pliku: sięgnięcie po `steps.first_mut()` i podmiana
/// pola każe przypadkowi wiedzieć, w której pozycji leży krok agenta, a to jest wiedza o kształcie
/// fikstury, nie o regule.
fn nobody_is_asked_for_the_field() -> WorkflowFile {
    let mut before = agent_step();
    before.handover = loadout_lib::workflow::Handover::default();

    let mut file = with_serve(waiting_for_the_field());
    file.steps = vec![Step::Agent(before), Step::Serve(waiting_for_the_field())];
    file
}

/// Uwagi o tym kafelku na wskazanym poziomie, sądzone tak jak sądzi się ZAPIS.
fn said_about_the_app(file: &WorkflowFile, level: Level) -> Vec<String> {
    check(file)
        .into_iter()
        .filter(|note| note.level == level)
        .filter(|note| note.step_id.as_deref() == Some("s_app"))
        .map(|note| note.message)
        .collect()
}

#[test]
fn saving_says_so_when_the_step_before_is_not_asked_for_the_field() {
    let file = nobody_is_asked_for_the_field();

    let said = said_about_the_app(&file, Level::Warning);
    assert!(
        said.iter()
            .any(|note| note.contains("command") && note.contains("Work out how to start it")),
        "the tile waits for a field the step before it was never asked for, so the run reaches \
         this tile only to refuse — after the person has waited through every step before it. \
         Saying it names the step they have to open and the field they have to add: {said:?}"
    );
    assert_eq!(
        said_about_the_app(&file, Level::Problem),
        Vec::<String>::new(),
        "at SAVE this is a warning, not a refusal. Between ticking the box on one tile and \
         asking the other one there is a moment where the file is half-wired on purpose, and a \
         save that refuses it throws away the person's work while they are still doing it"
    );
}

#[test]
fn running_refuses_the_same_shape_the_save_only_warned_about() {
    let file = nobody_is_asked_for_the_field();

    let refused: Vec<String> = check_to_run(&file)
        .into_iter()
        .filter(|note| note.level == Level::Problem)
        .map(|note| note.message)
        .collect();
    assert!(
        refused.iter().any(|note| note.contains("command")),
        "Run has nothing to start this tile with, and every step before it costs a vendor call. \
         A run that begins anyway spends that money to arrive at a refusal it could have said \
         before the first turn: {refused:?}"
    );
}

#[test]
fn a_tile_nothing_points_at_is_told_so_in_its_own_words() {
    let mut file = with_serve(waiting_for_the_field());
    file.links.clear();

    let said = said_about_the_app(&file, Level::Warning);
    assert!(
        said.iter().any(|note| note.contains("arrow")),
        "with no arrow into it there is no step to work the command out, and sending the person \
         to a step that does not exist is worse than saying nothing points at it: {said:?}"
    );
}

#[test]
fn a_way_back_is_not_a_step_before_it() {
    let mut file = with_serve(waiting_for_the_field());
    /* Powrót wchodzi do kroku dopiero w rundzie drugiej (`workflow::unroll`), a odmówić albo nie
     * odmówić trzeba w pierwszej — tej, która ruszy. Strzałka licząca się jako poprzednik
     * przepuszczałaby plik, w którym kafelek czeka na pole od kroku, który w rundzie pierwszej
     * jeszcze nie istnieje. */
    file.links = vec![loadout_lib::workflow::Link {
        from: "s_agent".to_owned(),
        to: "s_app".to_owned(),
        max_turns: Some(2),
    }];

    let said = said_about_the_app(&file, Level::Warning);
    assert!(
        said.iter().any(|note| note.contains("arrow")),
        "a way back reaches this tile only on the second round, and the first one is the round \
         that starts. Counting it as the step before hands the tile a command that will not \
         exist when it runs: {said:?}"
    );
}

#[test]
fn a_command_that_looks_like_a_secret_never_reaches_the_shell() {
    /* Ten przypadek stoi na CZYSTEJ FUNKCJI, bo o to samo pyta skan przy zapisie, i to jest
     * jedyna wspólna odpowiedź (niezmiennik 23). Komenda od agenta idzie przez ten sam
     * `secret_shaped` w `commands::run`; gdyby ta funkcja przestała rozpoznawać taki kształt,
     * obie drogi otwierałyby się naraz i nikt by tego nie zauważył. */
    let looks_like = "npx serve --token=ghp_0123456789abcdefghijklmnopqrstuvwxyzA";

    assert!(
        loadout_lib::workflow::check::a_command_carrying_a_secret_shape(looks_like).is_some(),
        "a command produced by an agent goes straight to the shell, and the save-time scan never \
         sees it — the whole justification for passing commands in plain text rests on a person \
         having written them and on that scan having run. If this stops recognising the shape, \
         the run-time refusal that guards the new path stops working too, silently"
    );
}
