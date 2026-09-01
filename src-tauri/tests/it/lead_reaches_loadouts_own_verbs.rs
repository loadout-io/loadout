//! Lider dostaje czasowniki Loadouta i oddaje nimi PRAWDZIWĄ bibliotekę tego człowieka.
//!
//! # Po co to istnieje
//!
//! Zgłoszenie właściciela 2026-08-29: „nie mam po prostu opcji pogadania z agentem i potem np
//! claude odpala nasze workflow które mamy zbudowane w apce". Pierwsza połowa tego zdania —
//! **żeby lider w ogóle wiedział, co ten człowiek ma** — nie miała do tego dnia żadnej drogi:
//! vendor w trybie bez terminala nie daje ani jednego narzędzia, którym dałoby się o to zapytać
//! (zmierzone, powód w nagłówku `crate::bridge`).
//!
//! # Trzy rzeczy sądzone tutaj, każda osobno
//!
//! (a) rozmowa dostaje most, czyli `mcp__loadout` ma jak trafić do `--allowedTools`;
//! (b) czasownik oddaje nazwy, którymi NAPRAWDĘ da się ten workflow uruchomić;
//! (c) pusta biblioteka dostaje ZDANIE, nie pustą listę.
//!
//! # Słabe wersje, których tu nie ma
//!
//! „`servers` jest niepuste" — przechodzi dla każdego połączenia, także cudzego. Sądzone jest
//! słowo `loadout`, bo to ono staje się `mcp__loadout`.
//!
//! „lista ma dwa wiersze" — przechodzi nad nazwami, których wiersz wejścia nie przyjmie. Sądzone
//! są NAZWY, i to te same, które zna `typable`.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `lead_thread_per_terminal` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// `panic!` w teście: panika w teście JEST jego wynikiem, a odmowa czasownika, którego to kryterium
// potrzebuje, jest właśnie taką porażką. Ten sam idiom, co `expect()` wyżej.
#![allow(clippy::panic)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call};
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::commands::workflows::save_workflow_inner;
use loadout_lib::commands::{Drivers, agents::save_agent_inner};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, DriverConfiguration, FinishReason,
    Outcome as TurnOutcome, Probe, RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::evidence::EvidenceTarget;
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::{Agent, Vendor};
use loadout_lib::workflow::WorkflowFile;
use tokio::sync::mpsc;

/// Pojemność strumienia wierszy. Z zapasem — mierzymy obecność, nie przepustowość.
const LINES: usize = 64;

/// Wywołanie czasownika prosto na biurku, bez gniazda i bez vendora.
async fn ask(desk: &Desk, verb: &str) -> Answer {
    desk.answer(Call {
        id: Value::from(1),
        call: verb.to_owned(),
        input: Value::Object(serde_json::Map::new()),
    })
    .await
}

/// Wartość z udanej odpowiedzi; odmowa jest tu porażką kryterium, nie stanem.
fn value_of(answer: Answer) -> Value {
    match answer {
        Answer::Ok(value) => value,
        Answer::Refused(said) => {
            panic!("the verb refused, and this criterion needs it to work: {said}")
        }
    }
}

/// Nazwy do wpisania z odpowiedzi czasownika, w kolejności, w której przyszły.
fn names(said: &Value, key: &str) -> Vec<String> {
    said.get(key)
        .and_then(Value::as_array)
        .expect("the answer carries its rows under that key")
        .iter()
        .filter_map(|row| row.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// Zapisuje workflow o tej nazwie w bibliotece i oddaje ścieżkę pliku.
///
/// Bez kroków, bo ten plik pyta wyłącznie o NAZWY. Liczbę kroków sądzi kryterium, które ich
/// potrzebuje; tutaj krok byłby fikstura udającą, że mierzy coś więcej.
fn saved_workflow(home: &Path, file_name: &str, name: &str) -> PathBuf {
    let file = WorkflowFile {
        format: 1,
        id: name.to_owned(),
        name: name.to_owned(),
        description: None,
        steps: Vec::new(),
        links: Vec::new(),
        extra: serde_json::Map::new(),
    };
    save_workflow_inner(home, None, file_name, &file, None)
        .expect("the library has to accept a workflow this test just built")
        .path
}

#[tokio::test]
async fn the_verb_hands_back_names_the_command_line_would_accept() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;

    let first = saved_workflow(home.path(), "ship-a-feature.json", "Ship a feature");
    let second = saved_workflow(home.path(), "review-and-fix.json", "Review and fix");
    assert!(
        first.exists() && second.exists(),
        "both files have to be on disk before this measures anything, or 'the verb listed them' \
         is a sentence about nothing"
    );

    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        project.path().to_path_buf(),
    );
    let said = value_of(ask(&desk, "list_workflows").await);

    let mut listed = names(&said, "workflows");
    listed.sort();
    assert_eq!(
        listed,
        vec!["review-and-fix".to_owned(), "ship-a-feature".to_owned()],
        "the lead gets the name a person would TYPE after /run, not the title. A title here \
         reads fine in the answer and refuses at the command line, which looks to the person \
         like a workflow that does not exist"
    );
    Ok(())
}

#[tokio::test]
async fn an_empty_library_gets_a_sentence_and_not_an_empty_list() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;

    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        project.path().to_path_buf(),
    );
    let said = value_of(ask(&desk, "list_workflows").await);

    assert_eq!(
        said.get("count").and_then(Value::as_u64),
        Some(0),
        "nothing saved yet is a true count, not an error"
    );
    assert!(
        said.get("note")
            .and_then(Value::as_str)
            .is_some_and(|note| note.contains("Workflows")),
        "an empty array reads to a model as 'the check failed', and the lead then guesses or \
         calls again. The sentence says what is true and names where the person goes next. It \
         carried: {:?}",
        said.get("note")
    );
    Ok(())
}

#[tokio::test]
async fn the_agent_verb_answers_from_the_same_library() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;

    let mut agent = Agent::example();
    agent.name = "Note taker".to_owned();
    agent.summary = "Writes the notes nobody else will".to_owned();
    save_agent_inner(home.path(), &agent, None).expect("the library accepts this agent");

    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        project.path().to_path_buf(),
    );
    let said = value_of(ask(&desk, "list_agents").await);

    assert_eq!(
        names(&said, "agents"),
        vec!["note-taker".to_owned()],
        "the same rule as for workflows: the name a person would type, not the title"
    );
    Ok(())
}

// ── START: CO LIDER MOŻE, A CZEGO NIE, ZANIM COKOLWIEK RUSZY ───────────────────────────────

/// Biurko z drogą na ekran plus podsłuch tego, co na niej stanęło.
fn desk_that_shows(home: &Path, project: &Path) -> (Desk, LineSource) {
    let (sink, source) = line_channel(LINES);
    let desk = Desk::at(Some(home.to_path_buf()), project.to_path_buf())
        .showing(Arc::new(Mutex::new(sink)));
    (desk, source)
}

#[tokio::test]
async fn starting_a_workflow_nobody_has_names_the_ones_they_do() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    saved_workflow(home.path(), "ship-a-feature.json", "Ship a feature");
    let (desk, _stream) = desk_that_shows(home.path(), project.path());

    let said = desk
        .answer(Call {
            id: Value::from(1),
            call: "start_workflow".to_owned(),
            input: serde_json::json!({ "workflow": "shipp-a-feature" }),
        })
        .await;

    match said {
        Answer::Refused(sentence) => {
            assert!(
                sentence.contains("shipp-a-feature"),
                "the refusal names what was asked for, so the lead can see it was a typo: \
                 {sentence}"
            );
            assert!(
                sentence.contains("ship-a-feature"),
                "and it names what this person HAS. A refusal without the list leaves the lead \
                 exactly where it was, and its next move is guessing another name: {sentence}"
            );
        }
        Answer::Ok(value) => panic!("a workflow nobody has must never start: {value}"),
    }
    Ok(())
}

#[tokio::test]
async fn starting_a_workflow_puts_it_on_the_screen_before_anything_runs()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    saved_workflow(home.path(), "ship-a-feature.json", "Ship a feature");
    let (desk, mut stream) = desk_that_shows(home.path(), project.path());

    let said = desk
        .answer(Call {
            id: Value::from(1),
            call: "start_workflow".to_owned(),
            input: serde_json::json!({
                "workflow": "ship-a-feature",
                "task": "build the CSV parser",
            }),
        })
        .await;
    assert!(
        matches!(said, Answer::Ok(_)),
        "a workflow this person has must be accepted"
    );

    let line = stream
        .try_next()
        .expect("asking for a start has to put a row on the screen");
    match line {
        Line::Suggested {
            auto,
            command,
            text,
            ..
        } => {
            assert!(
                auto,
                "the row the lead's own decision produced runs by itself. Without this flag the \
                 person is back to copying a command out of a sentence, which is the whole thing \
                 they asked to stop doing"
            );
            assert_eq!(
                command, "/run ship-a-feature build the CSV parser",
                "the command is byte for byte what a person would type, because the window \
                 takes it apart with the SAME function Enter uses. A second start policy would \
                 drift in silence: the 'how many at once' number would be read, logged, and \
                 different"
            );
            assert!(
                text.contains("Ship a feature"),
                "and the row says what is starting, by its real name — the person's only \
                 protection under 'it just starts' is seeing what started, in the second it \
                 started. It said: {text}"
            );
        }
        other => panic!("the row has to be the one carrying a command: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn a_desk_with_no_screen_refuses_to_start_anything() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    saved_workflow(home.path(), "ship-a-feature.json", "Ship a feature");
    /* BEZ `showing`: rozmowa, której okno jeszcze nie otworzyło strumienia. */
    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        project.path().to_path_buf(),
    );

    let said = desk
        .answer(Call {
            id: Value::from(1),
            call: "start_workflow".to_owned(),
            input: serde_json::json!({ "workflow": "ship-a-feature" }),
        })
        .await;

    assert!(
        matches!(said, Answer::Refused(_)),
        "a run nobody can see is the one failure 'it just starts' cannot afford. With no stream \
         open, work would begin and the person would have no way to know — not which workflow, \
         not that anything started at all"
    );
    Ok(())
}

#[tokio::test]
async fn a_verb_nobody_has_is_refused_by_name() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        project.path().to_path_buf(),
    );

    match ask(&desk, "delete_everything").await {
        Answer::Refused(said) => assert!(
            said.contains("delete_everything"),
            "the refusal has to name the verb. Without the name the model cannot tell a typo \
             from a verb Loadout does not have, so it repeats the same call. It said: {said}"
        ),
        Answer::Ok(value) => panic!("a verb nobody has must never answer: {value}"),
    }
    Ok(())
}

// ── PYTANIE: CZY NAPRAWDĘ CZEKA, I CZY ODPOWIEDŹ TRAFIA DO WŁAŚCIWEGO ──────────────────────

/// Biurko, które umie pokazać pytanie i usłyszeć odpowiedź.
fn desk_that_asks(home: &Path, project: &Path) -> (Desk, LineSource, Arc<Waiting>) {
    let (sink, source) = line_channel(LINES);
    let waiting = Arc::new(Waiting::default());
    let desk = Desk::at(Some(home.to_path_buf()), project.to_path_buf())
        .showing(Arc::new(Mutex::new(sink)))
        .hearing(Arc::clone(&waiting));
    (desk, source, waiting)
}

fn asking(question: &str, options: &Value) -> Call {
    Call {
        id: Value::from(1),
        call: "ask_the_person".to_owned(),
        input: serde_json::json!({ "question": question, "options": options }),
    }
}

#[tokio::test]
async fn a_question_waits_for_the_person_and_hands_back_what_they_said()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let (desk, mut stream, waiting) = desk_that_asks(home.path(), project.path());

    let asked = tokio::spawn(async move {
        desk.answer(asking(
            "Which parser should I fix?",
            &serde_json::json!(["src/parser.rs", "tools/import/csv.rs"]),
        ))
        .await
    });

    /* PYTANIE JEST NA EKRANIE, ZANIM PADNIE ODPOWIEDŹ. Kolejność jest tu treścią: wersja, która
     * pyta i jedzie dalej, zostawia człowieka z pytaniem, na które nikt już nie czeka. */
    let mut line = None;
    for _ in 0..64 {
        if let Some(seen) = stream.try_next() {
            line = Some(seen);
            break;
        }
        tokio::task::yield_now().await;
    }
    match line.expect("the question has to reach the screen") {
        Line::Asked { text, options, .. } => {
            assert_eq!(text, "Which parser should I fix?");
            assert_eq!(
                options,
                vec!["src/parser.rs".to_owned(), "tools/import/csv.rs".to_owned()],
                "the options the model offered become the buttons this person clicks. Dropped \
                 here, every question turns into a blank box — which is the shape people stop \
                 answering"
            );
        }
        other => panic!("the row has to be the question: {other:?}"),
    }

    assert!(
        !asked.is_finished(),
        "the turn has to be STANDING here. A question the agent does not wait for is not a \
         question: it goes on and does the work before anybody clicks, and the answer arrives \
         for a decision already made"
    );

    assert!(
        waiting.answer("Lead", "src/parser.rs".to_owned()),
        "somebody was waiting, so the answer has to find them"
    );

    let said = asked.await?;
    assert_eq!(
        said,
        Answer::Ok(Value::String("src/parser.rs".to_owned())),
        "and the answer comes back INSIDE the same turn, so the agent carries on knowing it — \
         not as a fresh turn after the work is done"
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_meant_for_somebody_else_leaves_the_question_standing()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let (desk, _stream, waiting) = desk_that_asks(home.path(), project.path());

    let asked = tokio::spawn(async move {
        desk.answer(asking("Which one?", &serde_json::json!([])))
            .await
    });
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }

    /* PODPIS SIĘ NIE ZGADZA: to jest odpowiedź na pytanie KAFELKA KONTROLNEGO, które stoi w tym
     * samym strumieniu. Bez tego rozróżnienia klik na punkcie kontrolnym odblokowywałby przy
     * okazji lidera — zdaniem, które go nie dotyczy, i w chwili, w której nikt tego nie widzi. */
    assert!(
        !waiting.answer("Forge", "carry on".to_owned()),
        "nobody by that name was waiting, and saying otherwise would mean the window believes an \
         answer was delivered when it was not"
    );
    assert!(
        !asked.is_finished(),
        "and the lead's question has to be STILL STANDING. Taken by somebody else's answer, the \
         agent carries on with a sentence that was never about its question"
    );

    assert!(waiting.answer("Lead", "this one".to_owned()));
    assert_eq!(
        asked.await?,
        Answer::Ok(Value::String("this one".to_owned())),
        "and the right answer still reaches it afterwards"
    );
    Ok(())
}

/// 2026-08-30 — TEN PRZYPADEK POWSTAŁ Z POMIARU, KTÓRY OBALIŁ MOJE ZAŁOŻENIE.
///
/// Pierwsza wersja porzucała `Arc<Waiting>` trzymany przez kryterium i oczekiwała, że czekanie
/// się skończy. **Zawisła.** Powód jest strukturalny: `Waiting` żyje WEWNĄTRZ biurka, a biurko
/// żyje tak długo, jak jego własna, czekająca przyszłość — więc kopia kryterium nigdy nie jest
/// ostatnia i nadawca nie ginie.
///
/// Prawdziwe zamknięcie rozmowy działa inaczej i **nie tędy**: `Bridge::drop` przerywa zadanie
/// przyjmujące połączenia, a przerwane zadanie porzuca całą przyszłość razem z odbiornikiem.
/// Czekanie kończy się więc **anulowaniem**, nie zamkniętym kanałem.
///
/// Gałąź `Err` w `ask` zostaje mimo to i ma tu swojego sędziego, bo jest osiągalna drugą drogą:
/// kolejne `park` zastępuje poprzednie. To jest podłoga, na której stoi obietnica „nigdy cisza" —
/// tura porzucona bez zdania wisiałaby tak długo, jak żyje aplikacja, i nic na ekranie nie
/// mówiłoby dlaczego.
#[tokio::test]
async fn a_question_that_loses_its_channel_gets_a_sentence_and_never_silence()
-> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let (desk, _stream, waiting) = desk_that_asks(home.path(), project.path());

    let asked = tokio::spawn(async move {
        desk.answer(asking("Still there?", &serde_json::json!([])))
            .await
    });
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }

    /* DRUGIE PYTANIE ZASTĘPUJE PIERWSZE, więc nadawca pierwszego jest porzucany. */
    let _second = waiting.park_for_test("Lead");

    match asked.await? {
        Answer::Refused(said) => assert!(
            !said.is_empty(),
            "the model has to be told nobody answered. Silence here is a turn that hangs for as \
             long as the app lives, and no line anywhere says why"
        ),
        Answer::Ok(value) => panic!("nobody answered, so this must not read as an answer: {value}"),
    }
    Ok(())
}

// ── (a) ROZMOWA NAPRAWDĘ DOSTAJE MOST ──────────────────────────────────────────────────────

/// Sterownik-dubler, który zapamiętuje konfigurację, jaką dostał.
///
/// Kształt uchwytu i zdarzeń jest przepisany z `lead_thread_per_terminal` — ten plik nie sądzi
/// sterownika, tylko to, co do niego DOJECHAŁO.
#[derive(Debug, Clone)]
struct Watching {
    seen: Arc<Mutex<Vec<DriverConfiguration>>>,
}

#[async_trait]
impl AgentDriver for Watching {
    fn id(&self) -> &'static str {
        "claude"
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some("claude".to_owned()),
        })
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(configuration.clone());
        Some(Arc::new(self.clone()))
    }

    /* Rozmowa ODMAWIA startu sterownikowi, który nie umie nieść prywatnego dowodu tury, i to
     * jest poprawne zachowanie produktu — nie luka w dublerze. Tutaj oddajemy siebie, bo ten
     * plik sądzi konfigurację, a nie dowody. */
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let (voice, _heard) = mpsc::channel(4);
        Ok(Box::new(Quiet {
            events,
            session: SessionRef {
                vendor: "claude",
                id: spec.run_id.to_string(),
            },
            voice,
        }))
    }
}

/// Uchwyt, który kończy turę od razu i schodzi z dowodem.
#[derive(Debug)]
struct Quiet {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    voice: Voice,
}

#[async_trait]
impl AgentHandle for Quiet {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn voice(&self) -> Option<Voice> {
        Some(self.voice.clone())
    }

    fn group(&self) -> Option<GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: true,
            reason: FinishReason::Completed,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send((AgentEvent::Finished(outcome.clone())).into())
            .await;
        Ok(outcome)
    }

    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

/// Otwiera strumień tego terminalu tak, jak otwiera go okno.
fn watching_lines(threads: &Threads, terminal: &Terminal) -> LineSource {
    let (sink, source) = line_channel(LINES);
    threads.terminal_lines_go_to(terminal, sink);
    source
}

#[tokio::test]
async fn a_conversation_carries_loadouts_own_server() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;

    let mut agent = Agent::example();
    agent.runs_with = Vendor::ClaudeCode;
    save_agent_inner(home.path(), &agent, None).expect("the library accepts this lead");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(Watching {
        seen: Arc::clone(&seen),
    });
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

    let lead = Lead::pointed_at(home.path(), Some(&agent.id.to_string()))
        .expect("the lead this test just saved is in the library");
    let terminal = Terminal {
        id: "terminal-1".to_owned(),
        folder: project.path().to_path_buf(),
    };

    let threads = Threads::new();
    threads.library_is(home.path().to_path_buf());
    let _stream = watching_lines(&threads, &terminal);

    threads
        .say_in(&drivers, &lead, &terminal, "what do I have?")
        .await
        .expect("the first sentence opens the conversation");

    /* KONTROLA PRZECIW PUSTEMU PRZEJŚCIU I PRZECIW MYLĄCEJ CZERWIENI. Bez niej „konfiguracja nie
     * niosła `loadout`" i „sterownik nie dostał żadnej konfiguracji" czytają się identycznie,
     * a to są dwie zupełnie różne awarie: pierwsza jest wadą szwu, druga znaczy, że most w ogóle
     * nie wstał. */
    assert!(
        !seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty(),
        "the driver was never configured at all, so the bridge did not open. That is a different \
         failure from 'the configuration was missing loadout', and reading one as the other costs \
         an hour"
    );

    let carried: Vec<String> = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .flat_map(|configuration| configuration.servers.clone())
        .collect();

    assert!(
        carried.iter().any(|server| server == "loadout"),
        "this name is what becomes `mcp__loadout` in --allowedTools, and without it the vendor \
         refuses every one of Loadout's own verbs — measured on Figma, 2026-08-22, twenty wasted \
         minutes. The conversation carried: {carried:?}"
    );
    Ok(())
}
