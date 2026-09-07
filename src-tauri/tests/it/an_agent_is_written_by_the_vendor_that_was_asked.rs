//! G-01/G-02: szkic agenta powstaje u WYBRANEGO vendora, w izolacji, i nie przyznaje sobie
//! niczego, czego człowiek nie zatwierdził.
//!
//! Dubler zastępuje wyłącznie vendora: zapisuje, z jakim `RunSpec` go zawołano, i oddaje
//! podstawioną odpowiedź. Walidacja, tożsamość, dopasowanie i ograniczenia procesu pochodzą
//! z produkcyjnego kodu.

#![allow(clippy::panic)]
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]
#![allow(clippy::assigning_clones, clippy::duration_suboptimal_units)]
#![allow(clippy::struct_field_names, clippy::implicit_clone)]

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::agent_generation::{GenerationFailed, generate};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Policy, Probe,
    RunSpec, SessionRef, Tokens, Voice,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::library::agent_generation::{Available, DRAFT_LIMIT_BYTES, Wanted, read_draft};
use loadout_lib::library::agents::{Tools, Vendor};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------------------------
// G-01: dopasowanie szkicu
// ---------------------------------------------------------------------------------------------

/// Tożsamość wybija backend. Model, który wybiera identyfikator, nadpisuje cudzego agenta.
#[test]
fn the_model_cannot_choose_the_identity() {
    let wanted = Wanted::from_one_vendor("A tester".to_owned(), Vendor::ClaudeCode, available());
    let asked = json!({
        "name": "QA", "summary": "Checks the work", "instructions": "Verify behaviour.",
        "id": "01990000-0000-7000-8000-000000009999", "schema": 9
    });
    let refused = read_draft(&wanted, asked.to_string().as_bytes());
    assert!(
        refused.is_err(),
        "the answer set its own identity and was taken anyway"
    );

    let clean = json!({"name": "QA", "summary": "Checks the work", "instructions": "Verify."});
    let draft = read_draft(&wanted, clean.to_string().as_bytes()).expect("a clean answer");
    assert_eq!(draft.agent.schema, loadout_lib::library::agents::Agent::example().schema);
    assert_ne!(draft.agent.id.to_string(), "01990000-0000-7000-8000-000000009999");
}

/// Vendor bierze się z przycisku, nie z odpowiedzi.
#[test]
fn the_button_decides_which_app_the_agent_runs_on() {
    for vendor in [Vendor::ClaudeCode, Vendor::Codex] {
        let wanted = Wanted::from_one_vendor("A tester".to_owned(), vendor, available());
        let asked = json!({"name": "QA", "summary": "s", "instructions": "i"});
        let draft = read_draft(&wanted, asked.to_string().as_bytes()).expect("a draft");
        assert_eq!(draft.agent.runs_with, vendor);
    }
}

/// Nieistniejąca umiejętność, połączenie i usługa zamieniają się w czytelny brak — nie
/// w milczące usunięcie i nie w twierdzenie, że agent je ma.
#[test]
fn what_does_not_exist_here_becomes_a_visible_gap() {
    let wanted = Wanted::from_one_vendor("A tester".to_owned(), Vendor::ClaudeCode, available());
    let asked = json!({
        "name": "QA", "summary": "s", "instructions": "i",
        "skills": ["testing", "time-travel"],
        "connections": ["playwright", "nowhere"],
        "serviceAccess": [{"service": "ghost", "operations": ["start"]}]
    });
    let draft = read_draft(&wanted, asked.to_string().as_bytes()).expect("a draft");
    assert_eq!(draft.agent.skills, vec!["testing".to_owned()]);
    assert_eq!(draft.agent.connections, vec!["playwright".to_owned()]);
    assert!(draft.agent.service_access.is_empty());
    assert_eq!(draft.missing.len(), 3, "{:?}", draft.missing);
    assert!(draft.missing.iter().any(|one| one.contains("time-travel")));
    assert!(draft.missing.iter().any(|one| one.contains("ghost")));
}

/// Przelotka vendora nie jest drugą drogą do uprawnień.
#[test]
fn a_generated_agent_cannot_widen_itself_through_raw_settings() {
    let wanted = Wanted::from_one_vendor("A tester".to_owned(), Vendor::ClaudeCode, available());
    let asked = json!({
        "name": "QA", "summary": "s", "instructions": "i",
        "vendorOptions": {"claude": {
            "--dangerously-skip-permissions": "",
            "--model": "something-else",
            "--verbose-tool-output": "true"
        }}
    });
    let draft = read_draft(&wanted, asked.to_string().as_bytes()).expect("a draft");
    let kept = draft.agent.vendor_options.get("claude").cloned().unwrap_or_default();
    assert!(
        !kept.contains_key("--dangerously-skip-permissions"),
        "a generated agent widened what it may do through a raw setting"
    );
    assert!(
        !kept.contains_key("--model"),
        "a generated agent set a flag Loadout sets itself"
    );
    assert!(
        kept.contains_key("--verbose-tool-output"),
        "a harmless raw setting was dropped, so the manual passthrough stopped working"
    );
    assert_eq!(draft.refused.len(), 2, "{:?}", draft.refused);
}

/// Ustawienie niedostępne u vendora nie może udawać działającego.
#[test]
fn a_tool_list_does_not_pretend_to_work_where_there_is_none() {
    let asked = json!({"name":"QA","summary":"s","instructions":"i","tools":["Read","Bash"]});

    let claude = Wanted::from_one_vendor("t".to_owned(), Vendor::ClaudeCode, available());
    let kept = read_draft(&claude, asked.to_string().as_bytes()).expect("a draft");
    assert_eq!(
        kept.agent.tools,
        Tools::Only(vec!["Read".to_owned(), "Bash".to_owned()])
    );

    let codex = Wanted::from_one_vendor("t".to_owned(), Vendor::Codex, available());
    let dropped = read_draft(&codex, asked.to_string().as_bytes()).expect("a draft");
    assert_eq!(
        dropped.agent.tools,
        Tools::Everything,
        "a list of tools was kept for an app that has no list of tools"
    );
    assert!(
        dropped.refused.iter().any(|one| one.contains("does not take a list of tools")),
        "the person was not told the list was dropped: {:?}",
        dropped.refused
    );
}

/// Model spoza zweryfikowanego katalogu nie wchodzi po cichu.
#[test]
fn a_model_nobody_verified_does_not_go_in_silently() {
    let wanted = Wanted::from_one_vendor("t".to_owned(), Vendor::ClaudeCode, available());
    let asked = json!({"name":"QA","summary":"s","instructions":"i","model":"opus-from-the-future"});
    let draft = read_draft(&wanted, asked.to_string().as_bytes()).expect("a draft");
    assert!(draft.agent.model.is_empty());
    assert!(
        draft.refused.iter().any(|one| one.contains("opus-from-the-future")),
        "{:?}",
        draft.refused
    );

    let known = json!({"name":"QA","summary":"s","instructions":"i","model":"opus"});
    let kept = read_draft(&wanted, known.to_string().as_bytes()).expect("a draft");
    assert_eq!(kept.agent.model, "opus");
}

/// Za długa i niepoprawna odpowiedź nie stają się szkicem.
#[test]
fn an_oversized_or_broken_answer_is_not_a_draft() {
    let wanted = Wanted::from_one_vendor("t".to_owned(), Vendor::ClaudeCode, available());
    let huge = vec![b'x'; DRAFT_LIMIT_BYTES + 1];
    assert!(read_draft(&wanted, &huge).is_err());
    assert!(read_draft(&wanted, b"I would suggest an agent that...").is_err());
    let no_name = json!({"summary": "s", "instructions": "i"});
    assert!(read_draft(&wanted, no_name.to_string().as_bytes()).is_err());
}

// ---------------------------------------------------------------------------------------------
// G-02: wykonanie w izolacji
// ---------------------------------------------------------------------------------------------

/// Wybrany przycisk dociera do fabryki sterownika, a proces generatora nie dostaje praw
/// do repozytorium.
#[tokio::test]
async fn the_chosen_app_runs_it_with_nothing_to_read_and_nothing_to_write()
-> Result<(), Box<dyn Error>> {
    for vendor in [Vendor::ClaudeCode, Vendor::Codex] {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let driver = Writer {
            id: match vendor {
                Vendor::ClaudeCode => "claude-code",
                Vendor::Codex => "codex",
            },
            answers: Arc::new(Mutex::new(vec![
                json!({"name":"QA","summary":"s","instructions":"i"}).to_string(),
            ])),
            seen: Arc::clone(&seen),
            found: true,
        };
        let wanted = Wanted::from_one_vendor("A tester".to_owned(), vendor, available());
        let draft = generate(&driver, &wanted, &CancellationToken::new())
            .await
            .map_err(|why| why.said())?;
        assert_eq!(draft.agent.runs_with, vendor);

        let specs = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let spec = specs.first().ok_or("the app was never started")?;
        assert_eq!(
            spec.policy,
            Policy::ReadOnly,
            "the generator was allowed to change files"
        );
        assert!(!spec.web, "the generator was allowed out to the network");
        assert!(
            spec.started_empty,
            "the generator was started inside a folder that has something in it"
        );
        assert!(
            spec.prompt.contains("A tester"),
            "the description never reached the app that was supposed to read it"
        );
        assert!(
            spec.prompt.contains("Do not do the work"),
            "the generator was not told it writes settings rather than doing the work"
        );
    }
    Ok(())
}

/// Jedna korekta formatu, i ani jednej więcej.
#[tokio::test]
async fn a_broken_answer_gets_exactly_one_correction() -> Result<(), Box<dyn Error>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver = Writer {
        id: "claude-code",
        answers: Arc::new(Mutex::new(vec![
            "here is what I would do".to_owned(),
            json!({"name":"QA","summary":"s","instructions":"i"}).to_string(),
        ])),
        seen: Arc::clone(&seen),
        found: true,
    };
    let wanted = Wanted::from_one_vendor("t".to_owned(), Vendor::ClaudeCode, available());
    let draft = generate(&driver, &wanted, &CancellationToken::new())
        .await
        .map_err(|why| why.said())?;
    assert_eq!(draft.agent.name, "QA");
    let asked = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(asked.len(), 2, "the correction did not happen exactly once");
    assert!(
        asked[1].prompt.contains("Answer again with the single JSON object"),
        "the correction did not carry the validator's own sentence"
    );

    let stubborn = Writer {
        id: "claude-code",
        answers: Arc::new(Mutex::new(vec!["no".to_owned(), "still no".to_owned(), "no".to_owned()])),
        seen: Arc::new(Mutex::new(Vec::new())),
        found: true,
    };
    let why = generate(&stubborn, &wanted, &CancellationToken::new())
        .await
        .expect_err("a stubborn app produced a draft");
    assert!(matches!(why, GenerationFailed::NotADraft { .. }), "{why:?}");
    Ok(())
}

/// Brak CLI to czytelny brak możliwości, nie fałszywy sukces.
#[tokio::test]
async fn a_missing_app_says_so() -> Result<(), Box<dyn Error>> {
    let driver = Writer {
        id: "codex",
        answers: Arc::new(Mutex::new(Vec::new())),
        seen: Arc::new(Mutex::new(Vec::new())),
        found: false,
    };
    let wanted = Wanted::from_one_vendor("t".to_owned(), Vendor::Codex, available());
    let why = generate(&driver, &wanted, &CancellationToken::new())
        .await
        .expect_err("a missing app produced a draft");
    assert!(matches!(why, GenerationFailed::NoVendor { .. }), "{why:?}");
    assert!(why.said().contains("not available on this computer"));
    Ok(())
}

/* TRZY KRYTERIA Z ŻYWEJ PRÓBY (2026-09-07).
 *
 * Wszystkie trzy wady wyglądały pod dublerem na działające, bo dubler odpowiada dokładnie tym,
 * co mu wpiszemy, i nie patrzy na numer sesji. Wyszły dopiero na prawdziwym `claude`. Kryteria
 * sądzą więc to, co Loadout WYSYŁA i CZYM to wysyła — a nie odpowiedź, którą sami podstawiliśmy.
 */

/// Prośba wymienia dopuszczalne słowa dla zbiorów zamkniętych.
///
/// Bez tego model zgaduje: żywy `claude` odpowiadał `"color": "amber"` i cały szkic — razem
/// z instrukcjami, na których człowiekowi zależy najbardziej — szedł do kosza na jednym słowie.
#[tokio::test]
async fn the_request_names_every_word_the_reader_will_accept() -> Result<(), Box<dyn Error>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver = Writer {
        id: "claude-code",
        answers: Arc::new(Mutex::new(vec![
            json!({"name": "QA", "summary": "s", "instructions": "i"}).to_string(),
        ])),
        seen: Arc::clone(&seen),
        found: true,
    };
    let wanted = Wanted::from_one_vendor("A tester".to_owned(), Vendor::ClaudeCode, available());
    generate(&driver, &wanted, &CancellationToken::new())
        .await
        .expect("a draft");

    let asked = seen.lock().unwrap_or_else(PoisonError::into_inner)[0]
        .prompt
        .clone();
    for word in ["slate", "plum", "clay", "moss", "rose"] {
        assert!(
            asked.contains(word),
            "the request never says {word:?} is a colour this reader takes, so the model has to \
             guess one of five words it was never shown: {asked}"
        );
    }
    for word in ["quick", "balanced", "deep", "deepest"] {
        assert!(asked.contains(word), "the request hides the thinking levels: {asked}");
    }
    for word in ["look-only", "ask-first", "work-freely"] {
        assert!(asked.contains(word), "the request hides the file-access words: {asked}");
    }
    Ok(())
}

/// Prośba pokazuje KSZTAŁT odpowiedzi, a nie same nazwy kluczy.
///
/// Nazwa bez typu znaczy zgadywanie budowy: żywy `claude` oddawał `agentMessages` obiektem
/// zamiast wartością tak/nie, a `because` mapą zamiast listą.
#[tokio::test]
async fn the_request_shows_the_shape_and_that_shape_is_one_this_reader_accepts()
-> Result<(), Box<dyn Error>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver = Writer {
        id: "claude-code",
        answers: Arc::new(Mutex::new(vec![
            json!({"name": "QA", "summary": "s", "instructions": "i"}).to_string(),
        ])),
        seen: Arc::clone(&seen),
        found: true,
    };
    let wanted = Wanted::from_one_vendor("A tester".to_owned(), Vendor::ClaudeCode, available());
    generate(&driver, &wanted, &CancellationToken::new())
        .await
        .expect("a draft");
    let asked = seen.lock().unwrap_or_else(PoisonError::into_inner)[0]
        .prompt
        .clone();

    /* PRZYKŁAD MUSI SAM PRZECHODZIĆ WŁASNY KONTRAKT. Prośba pokazująca kształt, którego
     * czytelnik nie przyjmuje, jest gorsza niż brak przykładu: model robi dokładnie to, o co
     * poprosiliśmy, i dostaje odmowę. */
    let example = serde_json::to_vec(
        &loadout_lib::library::agent_generation::Answered::example(),
    )?;
    read_draft(&wanted, &example)
        .expect("the shape the request shows is not a shape read_draft accepts");
    let shown = String::from_utf8(example)?;
    let one_key = shown
        .split('"')
        .find(|piece| *piece == "agentMessages")
        .unwrap_or("agentMessages");
    assert!(
        asked.contains(one_key) && asked.contains("keep the structure"),
        "the request lists key names without ever showing what shape they take: {asked}"
    );
    Ok(())
}

/// Każda tura ma własny numer sesji, a korekta niesie CAŁĄ prośbę.
///
/// Prawdziwy `claude` odmawia drugiego uruchomienia z tym samym numerem („Session ID … is
/// already in use"), więc jedyna korekta formatu nie mogła się nigdy odbyć. A skoro to nowa
/// sesja, to nie pamięta niczego: samo zażalenie bez pierwotnej prośby wracało prozą.
#[tokio::test]
async fn the_one_correction_is_a_fresh_session_carrying_the_whole_request()
-> Result<(), Box<dyn Error>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver = Writer {
        id: "claude-code",
        answers: Arc::new(Mutex::new(vec![
            json!({"name": "QA", "summary": "s", "color": "amber"}).to_string(),
            json!({"name": "QA", "summary": "s", "instructions": "i"}).to_string(),
        ])),
        seen: Arc::clone(&seen),
        found: true,
    };
    let described = "A reviewer that reads a diff".to_owned();
    let wanted = Wanted::from_one_vendor(described.clone(), Vendor::ClaudeCode, available());
    generate(&driver, &wanted, &CancellationToken::new())
        .await
        .expect("the correction never produced a draft");

    let turns = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(turns.len(), 2, "the correction turn never ran at all");
    assert_ne!(
        turns[0].session, turns[1].session,
        "both turns went out under one session number, and the real app refuses the second one \
         with \"Session ID … is already in use\" — so the only correction is spent on a refusal"
    );
    assert!(
        turns[1].prompt.contains(&described),
        "the correction went to a fresh session carrying only the complaint, so the model was \
         asked to answer again a question it was never shown: {}",
        turns[1].prompt
    );
    assert!(
        turns[1].prompt.contains("amber"),
        "the correction never says what was wrong the first time: {}",
        turns[1].prompt
    );
    Ok(())
}

/// Anulowanie dotyczy TEJ operacji.
#[tokio::test]
async fn cancelling_one_generation_ends_only_that_one() -> Result<(), Box<dyn Error>> {
    let cancel = CancellationToken::new();
    let other = CancellationToken::new();
    let driver = Writer {
        id: "claude-code",
        answers: Arc::new(Mutex::new(vec!["never returned".to_owned()])),
        seen: Arc::new(Mutex::new(Vec::new())),
        found: true,
    };
    let wanted = Wanted::from_one_vendor("t".to_owned(), Vendor::ClaudeCode, available());
    cancel.cancel();
    let why = generate(&driver, &wanted, &cancel)
        .await
        .expect_err("a cancelled generation produced a draft");
    assert_eq!(why, GenerationFailed::Cancelled);
    assert!(
        !other.is_cancelled(),
        "cancelling one generation reached another operation"
    );
    Ok(())
}

fn available() -> Available {
    Available {
        skills: vec!["testing".to_owned()],
        connections: vec!["playwright".to_owned()],
        services: vec!["preview".to_owned()],
        models: vec!["opus".to_owned(), "sonnet".to_owned()],
    }
}

/// Co dubler zapamiętał o jednym wywołaniu.
#[derive(Clone)]
struct Started {
    prompt: String,
    /// Czy w chwili STARTU ten katalog był pusty. Sprawdzone tu, a nie po powrocie:
    /// katalog należy do operacji i schodzi razem z nią.
    started_empty: bool,
    policy: Policy,
    web: bool,
    /// Numer sesji TEJ tury. Prawdziwy `claude` odmawia drugiego uruchomienia z tym samym.
    session: String,
}

struct Writer {
    id: &'static str,
    answers: Arc<Mutex<Vec<String>>>,
    seen: Arc<Mutex<Vec<Started>>>,
    found: bool,
}

#[async_trait]
impl AgentDriver for Writer {
    fn id(&self) -> &'static str {
        self.id
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: self.found,
            version: None,
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Started {
                prompt: spec.prompt.clone(),
                started_empty: std::fs::read_dir(&spec.cwd)
                    .is_ok_and(|mut entries| entries.next().is_none()),
                policy: spec.policy,
                web: spec.reaches_the_web,
                session: spec.run_id.to_string(),
            });
        let mut answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
        let text = if answers.is_empty() {
            String::new()
        } else {
            answers.remove(0)
        };
        Ok(Box::new(Wrote {
            text,
            events,
            session: SessionRef {
                vendor: "claude-code",
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Wrote {
    text: String,
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Wrote {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<GroupId> {
        None
    }
    fn voice(&self) -> Option<Voice> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        anyhow::bail!("generation is one turn")
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let result = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: self.text.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        let _ = self
            .events
            .send(AgentEvent::Finished(result.clone()).into())
            .await;
        Ok(result)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}

// ---------------------------------------------------------------------------------------------
// §11 punkt 1, ŻYWA połowa: oba przyciski naprawdę wołają swój własny program
// ---------------------------------------------------------------------------------------------

/// Wszystko wyżej sądzi produkcyjną logikę na dublerze vendora — i to jest właściwy kształt dla
/// bramki, bo odpowiada o KODZIE. Ten jeden przypadek odpowiada o czym innym: czy prawdziwy
/// `claude` i prawdziwy `codex`, zawołane tak, jak woła je przycisk, oddają szkic, który
/// produkcyjne wczytywanie przyjmuje. Tego nie da się wywnioskować z dublera, bo dubler zawsze
/// odpowiada tak, jak go napisaliśmy.
///
/// `--ignored`, bo odpowiedź zależy od dwóch programów zainstalowanych na TEJ maszynie
/// i od zalogowania w nich — na komputerze bez nich musiałby być czerwony, choć nic nie jest
/// zepsute (ten sam powód, co przy `native_scenarios::what_this_computer_can_actually_do`).
///
/// ```text
/// cargo test --manifest-path src-tauri/Cargo.toml --test it \
///   an_agent_is_written_by_the_vendor_that_was_asked::both_buttons -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "asks the real claude and the real codex on this computer"]
async fn both_buttons_really_write_an_agent_with_their_own_app() {
    let described = "A reviewer that reads a diff and names the risk it carries. \
                     It must not change any files."
        .to_owned();
    for vendor in [Vendor::ClaudeCode, Vendor::Codex] {
        let driver: Box<dyn AgentDriver> = match vendor {
            Vendor::ClaudeCode => Box::new(loadout_lib::engine::drivers::claude::ClaudeDriver::new()),
            Vendor::Codex => Box::new(loadout_lib::engine::drivers::codex::CodexDriver::new()),
        };
        let wanted = Wanted::from_one_vendor(described.clone(), vendor, available());
        let cancel = CancellationToken::new();

        let draft = match generate(driver.as_ref(), &wanted, &cancel).await {
            Ok(draft) => draft,
            Err(why) => panic!("{vendor:?} was asked for an agent and gave none: {why:?}"),
        };

        println!(
            "{vendor:?} wrote: name={:?} model={:?} tools={:?} missing={:?}",
            draft.agent.name, draft.agent.model, draft.agent.tools, draft.missing
        );

        assert_eq!(
            draft.agent.runs_with, vendor,
            "the app the agent runs on came from the answer instead of the button"
        );
        assert!(
            !draft.agent.instructions.trim().is_empty(),
            "{vendor:?} returned an agent with no instructions at all, which is a form with \
             nothing in the field a person came here to get"
        );
        assert!(
            !draft.agent.name.trim().is_empty() && !draft.agent.summary.trim().is_empty(),
            "{vendor:?} returned an agent the library cannot show: name or summary is empty"
        );
        assert_eq!(
            draft.agent.file_access,
            loadout_lib::library::agents::FileAccess::LookOnly,
            "the description said this role must not change files, and {vendor:?} still gave it \
             the right to write. The draft is the thing a person presses Save on"
        );
    }
}
