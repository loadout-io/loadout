//! Rozmowa z liderem kończy KAŻDĄ turę — także dziewiątą i dwudziestą czwartą.
//!
//! # Incydent 2026-09-07, zmierzony na żywej sesji
//!
//! Człowiek prowadził wieloturową rozmowę z liderem Claude. Po ośmiu odpowiedziach ekran
//! przestał pokazywać cokolwiek i został z pracą oraz przyciskiem przerwania. Kolejne
//! wiadomości nadal dochodziły do CLI — model wykonał polecenie i zapisał odpowiedź we własnym
//! transkrypcie o 23:09:49 — ale Loadout tej odpowiedzi nie odebrał już nigdy. W prywatnym
//! `logs/lead.jsonl` stoi DOKŁADNIE dziewięć rekordów `type: result`, a dziewiąty jest ostatnią
//! linią pliku: pętla czytająca zdążyła zapisać surową linię i stanęła na następnym kroku.
//!
//! Tym krokiem była druga kopia wyniku. `emit` wysyłało `Outcome` najpierw do kolejki
//! `AgentHandle::wait()`, a dopiero potem zdarzenie na ekran. Kolejka ma osiem miejsc, a rozmowa
//! prowadzona głosem NIGDY nie woła `wait()` — wyniki czyta `read_along` z kanału zdarzeń.
//! Dziewiąte `send().await` nie miało więc dokąd wejść i zatrzymało cały odczyt stdoutu.
//!
//! # Słabe wersje tego kryterium
//!
//! **„Sesja przyjmuje dwadzieścia cztery wiadomości".** Przechodzi na zepsutym kodzie w całości:
//! pisarz stdin działał przez cały incydent i to jest właśnie ta połowa, która wyglądała zdrowo.
//! Rozróżnia dopiero ZAKOŃCZENIE tury — terminalny prywatny paragon i wiersz na ekranie.
//!
//! **„Ostatnia odpowiedź doszła".** Też przechodzi: treść dziewiątej odpowiedzi zdążyła dojechać
//! do surowego pliku, zanim pętla stanęła. Blokada padła na jej ZAKOŃCZENIU.
//!
//! **Test wołający sam `handle.wait()`.** Opróżniałby kolejkę za plecami aktora, czyli
//! naprawiałby błąd w trakcie mierzenia go. Dlatego tury jadą wyłącznie przez `Threads::say_in`,
//! dokładnie tak, jak jedzie je okno.
//!
//! Atrapa to `#!/bin/sh`, nie vendor: żadnego konta, żadnego wywołania API, jeden proces przez
//! całą rozmowę.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use loadout_lib::commands::Drivers;
use loadout_lib::commands::agents::save_agent_inner;
use loadout_lib::commands::chat::{Lead, Terminal, Threads};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{AgentDriver, AgentHandle, DecodedEvent, Policy, RunSpec};
use loadout_lib::engine::line::LineKind;
use loadout_lib::engine::supervisor::GroupProof;
use loadout_lib::ipc::{LineSource, QUEUE_CAP, line_channel};
use loadout_lib::library::agents::{Agent, FileAccess, Vendor};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Ile tur prowadzi ta rozmowa. Trzykrotność ośmiu miejsc w kolejce, bo kryterium ma dowodzić
/// braku progu, a nie progu przesuniętego o kilka.
const TURNS: usize = 24;

/// Sufit na JEDNO oczekiwanie. Regresja ma się objawić czerwonym testem, nie zawieszeniem
/// całego celu: `cargo test` bez własnego sufitu oddaje rc 124, a to jest fałszywa czerwień.
const PATIENCE: Duration = Duration::from_secs(15);

/// Sufit na sprzątanie po nieudanym teście. Zepsuty kod wisi w `close()` na tej samej kolejce,
/// więc porzucenie uchwytu (i twarda gwardia `Drop` supervisora) musi mieć pierwszeństwo przed
/// czekaniem na czyste zejście.
const CLEANUP: Duration = Duration::from_secs(5);

/// Atrapa `claude`: jeden proces na całą sesję, jedna unikalna odpowiedź na każdą kopertę.
///
/// `turns.log` obok binarki jest jedynym syntetycznym znacznikiem tego testu — mówi, ile kopert
/// naprawdę doszło na wejście dziecka. W incydencie ta liczba rosła dalej, kiedy ekran już nie
/// dostawał niczego, i to jest ta różnica, którą kryterium musi widzieć.
const FAKE_CLI: &str = r#"#!/bin/sh
if [ "${1-}" = "--version" ]; then
  printf '%s\n' '2.1.238 (Claude Code)'
  exit 0
fi
here="$(dirname "$0")"
printf '{"type":"system","subtype":"init","session_id":"lead-long-talk","capabilities":["interrupt_receipt_v1"]}\n'
turn=0
while IFS= read -r line; do
  # Prosba o przerwanie jedzie tym samym potokiem i NIE jest turą. Odpowiadamy na nią tak,
  # jak odpowiada CLI: potwierdzenie i wyjscie sesji. Policzenie jej jako tury zawyzaloby
  # licznik dostarczonych wiadomosci o samo zamkniecie rozmowy.
  case "$line" in
    *'"type":"control_request"'*)
      printf '{"type":"control_response","response":{"subtype":"success","request_id":"ack"}}
'
      exit 0
      ;;
    *'"type":"user"'*) ;;
    *) continue ;;
  esac
  turn=$((turn + 1))
  printf '%s\n' "$turn" >> "$here/turns.log"
  printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"REPLY_%s"}]}}\n' "$turn"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"total_cost_usd":0.001,"usage":{"input_tokens":1,"cache_read_input_tokens":2,"output_tokens":3},"duration_ms":1,"result":"REPLY_%s"}\n' "$turn"
done
exit 0
"#;

fn executable(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Fabryka sterowników, w której oba vendory prowadzą do prawdziwego `ClaudeDriver`.
fn drivers_over(binary: PathBuf) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::with_binary(binary));
    Arc::new(move |_vendor| Arc::clone(&driver))
}

fn saved_lead(library: &Path) -> Result<Lead, Box<dyn Error>> {
    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    "Long Talker".clone_into(&mut agent.name);
    agent.runs_with = Vendor::ClaudeCode;
    let _written = save_agent_inner(library, &agent, None)?;
    Lead::pointed_at(library, Some(&agent.id.to_string())).map_err(|error| error.to_string().into())
}

/// Katalog prywatnego paragonu tej jednej rozmowy.
async fn conversation_root(workspace: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let root = workspace.join(".loadout").join("conversations");
    let began = Instant::now();
    loop {
        if let Ok(entries) = fs::read_dir(&root)
            && let Some(dir) = entries
                .filter_map(Result::ok)
                .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        {
            return Ok(dir.path());
        }
        if began.elapsed() >= PATIENCE {
            return Err(format!("no conversation was recorded under {}", root.display()).into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Czeka, aż paragon TEJ tury stanie się terminalny, i oddaje jego stan.
///
/// Stan, nie `bool`: „nadal `delivered`" i „`failed`" są dla człowieka dwiema różnymi
/// wiadomościami, a kryterium, które je zlewa, nie odróżnia blokady od porażki.
async fn turn_state(
    root: &Path,
    number: usize,
    seen: &mut Vec<(LineKind, String)>,
    source: &mut LineSource,
) -> Result<String, Box<dyn Error>> {
    let path = root.join("turns").join(format!("{number:04}.json"));
    let began = Instant::now();
    loop {
        drain(source, seen);
        if let Ok(bytes) = fs::read(&path)
            && let Ok(turn) = serde_json::from_slice::<Value>(&bytes)
            && let Some(state) = turn.get("state").and_then(Value::as_str)
            && state != "sending"
            && state != "delivered"
        {
            drain(source, seen);
            return Ok(state.to_owned());
        }
        if began.elapsed() >= PATIENCE {
            let state = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|turn| turn.get("state").and_then(Value::as_str).map(str::to_owned));
            return Err(format!(
                "turn {number} of this conversation never ended: its receipt still says {state:?} \
                 after {PATIENCE:?}. The agent app kept reading its input, so this is the 2026-09-07 \
                 incident: the loop that reads the agent output is parked on a second copy of the \
                 result that nobody in this conversation ever takes."
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn drain(source: &mut LineSource, seen: &mut Vec<(LineKind, String)>) {
    while let Some(line) = source.try_next() {
        seen.push((line.kind(), line.text().to_owned()));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_turn_of_a_long_lead_conversation_ends() -> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", FAKE_CLI)?;
    let drivers = drivers_over(binary);
    let lead = saved_lead(library.path())?;

    let threads = Threads::new();
    threads.library_is(library.path().to_path_buf());
    let terminal = Terminal {
        id: "terminal-that-keeps-talking".to_owned(),
        folder: workspace.path().to_path_buf(),
    };
    let (lines, mut source) = line_channel(QUEUE_CAP);
    threads.terminal_lines_go_to(&terminal, lines);

    let outcome = talk_for_all_of_it(&threads, &drivers, &lead, &terminal, &mut source).await;
    /* ZAMKNIĘCIE MA SUFIT I MA ODDAĆ DOWÓD. Na zepsutym kodzie `close()` czeka na czytnik
     * zaparkowany na tej samej kolejce, więc to jest druga połowa incydentu — a sufit tutaj
     * jest tym, co zamienia jego powrót w czerwony test zamiast w zawieszony cel. Nawet gdy
     * padnie, gwardia `Drop` supervisora zabija grupę tego testu. */
    let closed = tokio::time::timeout(CLEANUP, threads.close_at(&terminal.id)).await;
    let (seen, root) = outcome?;
    let proof = closed
        .map_err(|_| format!("closing this conversation did not come back within {CLEANUP:?}"))?;
    assert!(
        matches!(proof, Some(GroupProof::Dead { .. })),
        "a conversation of {TURNS} turns has to close with a proof its group is gone, not {proof:?}"
    );

    assert_replies_reached_the_screen(&seen);
    assert_nothing_was_counted_twice(&root, fixture.path())?;
    Ok(())
}

/// Dwadzieścia cztery tury przez tę samą drogę, którą jedzie okno.
async fn talk_for_all_of_it(
    threads: &Threads,
    drivers: &Drivers,
    lead: &Lead,
    terminal: &Terminal,
    source: &mut LineSource,
) -> Result<(Vec<(LineKind, String)>, PathBuf), Box<dyn Error>> {
    let mut seen = Vec::new();
    let mut root = None;
    for number in 1..=TURNS {
        threads
            .say_in(drivers, lead, terminal, &format!("question {number}"))
            .await?;
        if root.is_none() {
            root = Some(conversation_root(&terminal.folder).await?);
        }
        let at = root
            .clone()
            .ok_or("the conversation directory disappeared")?;
        let state = turn_state(&at, number, &mut seen, source).await?;
        assert_eq!(
            state, "succeeded",
            "turn {number} ended as {state}, and every turn of this fixture answers cleanly"
        );
    }
    let root = root.ok_or("the conversation directory disappeared")?;
    /* Paragon staje się terminalny ZANIM powstanie wiersz na ekranie: `read_along` publikuje go
     * w puli blokującej, a dopiero potem pyta kurator o wiersze. Bez tej bariery ostatnie
     * zakończenie bywa jeszcze w drodze i kryterium mierzyłoby wyścig, nie transport. */
    let began = Instant::now();
    loop {
        drain(source, &mut seen);
        if seen
            .iter()
            .filter(|(kind, _)| *kind == LineKind::Done)
            .count()
            >= TURNS
        {
            break;
        }
        if began.elapsed() >= PATIENCE {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok((seen, root))
}

/// Każda unikalna odpowiedź dochodzi TAM, GDZIE CZYTA JĄ CZŁOWIEK, i w swojej kolejności.
fn assert_replies_reached_the_screen(seen: &[(LineKind, String)]) {
    let prose: Vec<&str> = seen
        .iter()
        .filter(|(kind, _)| *kind == LineKind::Note)
        .map(|(_, text)| text.as_str())
        .collect();
    for number in 1..=TURNS {
        let wanted = format!("REPLY_{number}");
        assert!(
            prose.iter().any(|text| text.contains(&wanted)),
            "answer {number} of {TURNS} never reached the screen; the reader showed {prose:?}"
        );
    }
    let order: Vec<usize> = (1..=TURNS)
        .filter_map(|number| {
            prose
                .iter()
                .position(|text| text.contains(&format!("REPLY_{number}")))
        })
        .collect();
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "the answers reached the screen out of order: {order:?}"
    );
    let done = seen
        .iter()
        .filter(|(kind, _)| *kind == LineKind::Done)
        .count();
    assert_eq!(
        done, TURNS,
        "every turn ends exactly once on the screen, and {TURNS} were asked"
    );
}

/// Ani jedna tura nie została rozliczona dwa razy, a agent dostał dokładnie tyle kopert,
/// ile odpowiedzi wróciło.
fn assert_nothing_was_counted_twice(root: &Path, fixture: &Path) -> Result<(), Box<dyn Error>> {
    let after: Value = serde_json::from_slice(&fs::read(root.join("conversation.json"))?)?;
    for key in ["attempts", "turns"] {
        assert_eq!(
            after.get(key).and_then(Value::as_u64),
            Some(TURNS as u64),
            "the conversation receipt disagrees about {key}: {after}"
        );
    }
    assert!(
        !root.join("turns").join("0025.json").exists(),
        "a twenty-fifth turn receipt means one ending was accounted for twice"
    );
    let delivered = fs::read_to_string(fixture.join("turns.log"))?;
    assert_eq!(
        delivered.lines().count(),
        TURNS,
        "the agent app was handed a different number of messages than it answered"
    );
    Ok(())
}

/// Zwykły krok workflow dostaje swój wynik także wtedy, gdy zdążył on przyjść PRZED `wait()`.
///
/// Kryterium sąsiaduje z tym wyżej, bo obie ścieżki dzielą jedną kolejkę: naprawa, która
/// odblokowuje rozmowę kosztem wyścigu „wynik przyszedł przed oczekiwaniem", zamienia jedną
/// wadę na drugą i musi tu paść.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_step_gets_its_outcome_even_when_it_asks_late() -> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", FAKE_CLI)?;
    let driver = ClaudeDriver::with_binary(binary);
    let (tx, mut events) = mpsc::channel::<DecodedEvent>(256);
    let run_id = Uuid::now_v7();
    let mut handle: Box<dyn AgentHandle> = tokio::time::timeout(
        PATIENCE,
        driver.start(step_spec(run_id, workspace.path(), "first question"), tx),
    )
    .await??;

    // Wynik jest już PO DRUGIEJ STRONIE, zanim ktokolwiek zawoła `wait()`.
    wait_for_finished(&mut events).await?;
    let first = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    assert_eq!(first.text.trim(), "REPLY_1", "the first outcome was lost");

    tokio::time::timeout(PATIENCE, handle.send("second question".to_owned())).await??;
    wait_for_finished(&mut events).await?;
    let second = tokio::time::timeout(PATIENCE, handle.wait()).await??;
    assert_eq!(
        second.text.trim(),
        "REPLY_2",
        "the follow-up outcome lost its identity: a step that waits after the fact has to get \
         ITS turn back, not the previous one"
    );
    assert_eq!(first.session.id, second.session.id);

    let code = tokio::time::timeout(PATIENCE, handle.close()).await??;
    assert_eq!(code, Some(0), "closing the input is how a session ends");
    Ok(())
}

async fn wait_for_finished(
    events: &mut mpsc::Receiver<DecodedEvent>,
) -> Result<(), Box<dyn Error>> {
    tokio::time::timeout(PATIENCE, async {
        while let Some(decoded) = events.recv().await {
            if matches!(
                decoded.event,
                loadout_lib::engine::drivers::AgentEvent::Finished(_)
            ) {
                return Ok(());
            }
        }
        Err("the agent output ended before the turn did".to_owned())
    })
    .await
    .map_err(|_| "the turn never finished on the event side")??;
    Ok(())
}

fn step_spec(run_id: Uuid, cwd: &Path, prompt: &str) -> RunSpec {
    RunSpec {
        run_id,
        cwd: cwd.to_path_buf(),
        prompt: prompt.to_owned(),
        model: None,
        system_append: None,
        reaches_the_web: false,
        policy: Policy::ReadOnly,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    }
}

/// Jawny tryb ma NAZWANY kontrakt: po oddaniu zakończeń strumieniowi `wait()` mówi, że nie ma
/// tu na co czekać — i strumień płynie dalej.
///
/// # Słaba wersja tego kryterium
///
/// `assert!(handle.wait().await.is_err())` bez sufitu czasu. Przechodzi na implementacji, która
/// oddaje z `wait()` wiecznie oczekujący future, bo wtedy nie przechodzi w ogóle: cały cel wisi,
/// a bramka melduje rc 124, czyli czerwień nie do odróżnienia od zepsutej maszyny. Sufit jest
/// tu asercją, nie ostrożnością.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_that_gave_its_endings_to_the_stream_says_so_instead_of_waiting_forever()
-> Result<(), Box<dyn Error>> {
    let fixture = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let binary = executable(fixture.path(), "claude", FAKE_CLI)?;
    let driver = ClaudeDriver::with_binary(binary);
    let (tx, mut events) = mpsc::channel::<DecodedEvent>(256);
    let mut handle: Box<dyn AgentHandle> = tokio::time::timeout(
        PATIENCE,
        driver.start(
            step_spec(Uuid::now_v7(), workspace.path(), "first question"),
            tx,
        ),
    )
    .await??;

    handle.turn_endings_are_read_from_the_stream();
    let refused = tokio::time::timeout(PATIENCE, handle.wait())
        .await
        .map_err(|_| {
            "wait() on a session whose endings belong to the stream never came back; a future \
             that only ever pends is the same defect one call further on"
        })?;
    assert!(
        refused.is_err(),
        "wait() answered {refused:?} for a session whose turn endings are read somewhere else"
    );

    // A strumień płynie dalej — także PO tym, ile razy poprzednia wersja by już stanęła.
    for _ in 0..(TURNS - 1) {
        tokio::time::timeout(PATIENCE, handle.send("next question".to_owned())).await??;
    }
    let mut finished = 1_usize;
    while finished < TURNS {
        wait_for_finished(&mut events).await?;
        finished += 1;
    }
    assert_eq!(
        finished, TURNS,
        "the stream stopped delivering turn endings after the queue nobody drains filled up"
    );

    let code = tokio::time::timeout(PATIENCE, handle.close()).await??;
    assert_eq!(
        code,
        Some(0),
        "closing the input has to come back even after {TURNS} unclaimed turn endings"
    );
    Ok(())
}

// ── Żywa wyrocznia ────────────────────────────────────────────────────────────────────────

/// Ile tur prowadzi żywa rozmowa. Przekracza dawny próg, i to jest cały jej zakres — nie ma
/// generować pracy u modelu.
const LIVE_TURNS: usize = 12;

/// Ile czekamy na JEDNĄ żywą odpowiedź. Turą jest tu jedno zdanie, ale zimny start sesji
/// i pierwsza runda narzędzi potrafią kosztować minuty.
const LIVE_PATIENCE: Duration = Duration::from_mins(3);

/// Dwanaście tur PRAWDZIWEJ sesji `claude` przez produkcyjną drogę rozmowy.
///
/// # Dlaczego `#[ignore]`
///
/// Bo płaci za tury u dostawcy. `checks/full-test.sh` woła `cargo test --tests` bez
/// `--include-ignored`, więc bramka tego nie odpala i tak ma być:
///
/// ```text
/// cargo test --manifest-path src-tauri/Cargo.toml --test it \
///   lead_keeps_answering_past_the_eighth_turn::twelve_live -- --ignored --nocapture
/// ```
///
/// # Czego ta wyrocznia NIE dowodzi
///
/// Okna. Idzie tą samą funkcją, którą woła komenda Tauri ([`Threads::say_in`]), z prawdziwym
/// backendem i prawdziwym CLI — ale nie klika w interfejs. Sterowanie oknem tej aplikacji
/// wymaga zgody człowieka (D-5), więc to zostaje krokiem człowieka.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "uruchamia prawdziwa sesje claude i za nia placi; wolaj z --ignored"]
async fn twelve_live_turns_stay_in_one_session() -> Result<(), Box<dyn Error>> {
    let library = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    let driver: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&driver));

    let mut agent = Agent::example();
    agent.id = Uuid::now_v7();
    "Live Long Talker".clone_into(&mut agent.name);
    agent.runs_with = Vendor::ClaudeCode;
    // Tania i tylko-do-odczytu: ta wyrocznia mierzy transport, nie pracę modelu.
    "haiku".clone_into(&mut agent.model);
    agent.file_access = FileAccess::LookOnly;
    agent.reaches_the_web = false;
    let _written = save_agent_inner(library.path(), &agent, None)?;
    let lead = Lead::pointed_at(library.path(), Some(&agent.id.to_string()))
        .map_err(|error| error.to_string())?;

    let threads = Threads::new();
    threads.library_is(library.path().to_path_buf());
    let terminal = Terminal {
        id: "terminal-live-long-conversation".to_owned(),
        folder: workspace.path().to_path_buf(),
    };
    let (lines, mut source) = line_channel(QUEUE_CAP);
    threads.terminal_lines_go_to(&terminal, lines);

    let talked = live_turns(&threads, &drivers, &lead, &terminal, &mut source).await;
    let closed = tokio::time::timeout(LIVE_PATIENCE, threads.close_at(&terminal.id)).await;
    let root = talked?;
    let proof = closed.map_err(|_| "closing the live conversation never came back".to_owned())?;
    assert!(
        matches!(proof, Some(GroupProof::Dead { .. })),
        "a live conversation of {LIVE_TURNS} turns has to close with a death proof, not {proof:?}"
    );
    assert_one_live_session(&root)?;
    Ok(())
}

/// JEDNA sesja u dostawcy, a w niej [`LIVE_TURNS`] zakończonych tur.
///
/// Dowodzi tego surowy strumień, nie hasło wplecione w rozmowę. Brief lidera każe mu odmawiać
/// poleceń, które wyglądają na próbę wyprowadzenia go z roli, więc kryterium oparte na haśle
/// mierzy jego personę, nie transport — zmierzone 2026-09-08: dwanaście odmów przy dwunastu
/// zdanych turach. Pole `session_id`, powtórzone w każdej linii wyniku, jest natomiast faktem
/// z drutu, a liczba tych linii jest dokładnie tą, która w incydencie stanęła na dziewięciu.
fn assert_one_live_session(root: &Path) -> Result<(), Box<dyn Error>> {
    let raw = fs::read_to_string(root.join("logs").join("lead.jsonl"))?;
    let results: Vec<Value> = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line.get("type").and_then(Value::as_str) == Some("result"))
        .collect();
    assert!(
        results.len() >= LIVE_TURNS,
        "the raw stream holds {} finished turns, and {LIVE_TURNS} were asked for",
        results.len()
    );
    let sessions: std::collections::BTreeSet<&str> = results
        .iter()
        .filter_map(|line| line.get("session_id").and_then(Value::as_str))
        .collect();
    assert_eq!(
        sessions.len(),
        1,
        "one conversation has to be one session at the agent app; the stream names {sessions:?}"
    );
    Ok(())
}

/// Prowadzi żywe tury i oddaje katalog prywatnego paragonu tej rozmowy.
///
/// Zdania są krótkie i **na temat lidera**: patrz powód przy [`assert_one_live_session`].
async fn live_turns(
    threads: &Threads,
    drivers: &Drivers,
    lead: &Lead,
    terminal: &Terminal,
    source: &mut LineSource,
) -> Result<PathBuf, Box<dyn Error>> {
    let mut seen = Vec::new();
    let mut root = None;
    for number in 1..=LIVE_TURNS {
        let said = format!(
            "In one short sentence and nothing else: name one thing step {number} of a workflow \
             could do."
        );
        threads.say_in(drivers, lead, terminal, &said).await?;
        if root.is_none() {
            root = Some(conversation_root(&terminal.folder).await?);
        }
        let at = root
            .clone()
            .ok_or("the live conversation directory disappeared")?;
        let state = live_turn_state(&at, number, &mut seen, source).await?;
        assert_eq!(
            state, "succeeded",
            "live turn {number} of {LIVE_TURNS} ended as {state}"
        );
        println!("live turn {number}/{LIVE_TURNS}: {state}");
    }
    drain(source, &mut seen);
    let ended = seen
        .iter()
        .filter(|(kind, _)| *kind == LineKind::Done)
        .count();
    assert_eq!(
        ended, LIVE_TURNS,
        "every live turn has to end once on the screen, and {LIVE_TURNS} were asked"
    );
    root.ok_or_else(|| "the live conversation directory disappeared".into())
}

/// To samo czekanie, co w regresji, tylko z żywym sufitem.
async fn live_turn_state(
    root: &Path,
    number: usize,
    seen: &mut Vec<(LineKind, String)>,
    source: &mut LineSource,
) -> Result<String, Box<dyn Error>> {
    let path = root.join("turns").join(format!("{number:04}.json"));
    let began = Instant::now();
    loop {
        drain(source, seen);
        if let Ok(bytes) = fs::read(&path)
            && let Ok(turn) = serde_json::from_slice::<Value>(&bytes)
            && let Some(state) = turn.get("state").and_then(Value::as_str)
            && state != "sending"
            && state != "delivered"
        {
            drain(source, seen);
            return Ok(state.to_owned());
        }
        if began.elapsed() >= LIVE_PATIENCE {
            return Err(format!("live turn {number} never ended within {LIVE_PATIENCE:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
