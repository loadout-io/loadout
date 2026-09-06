//! Lider ma czym zatrzymać bieg — i wolno mu to zrobić dopiero po odpowiedzi człowieka.
//!
//! # Skąd to kryterium
//!
//! Bieg meetnotes `20260901-150035`. Lider (rozmowa `01a05cf0-…`) zapytał człowieka, czy ubić
//! bieg, dostał zgodę — i **nie miał czym**. Most znał wtedy `list_workflows`, `list_agents`,
//! `start_workflow` i `ask_the_person`, więc lider zrobił to, co potrafił: przeczytał `pgid`
//! z `run.json` i wykonał `kill -TERM -38475 -38476` narzędziem Bash, a 34 minuty później to samo
//! na drugiej parze. Loadout o tym nie wiedział: zapisał dwa kroki jako porażkę, `carry-on`
//! puścił bieg dalej i człowiek zapłacił za 17 minut Codeksa nad pustym miejscem.
//!
//! # Słaba wersja tego kryterium: „`stop_run` jest na liście czasowników"
//!
//! Przechodzi dla nazwy bez drogi — czyli dla narzędzia, które model widzi, obiecuje człowiekowi
//! i które za każdym razem oddaje błąd (niezmiennik 16 w najgorszym miejscu, bo obietnicę składa
//! wtedy zdanie agenta).
//!
//! Druga słaba wersja, i ta była tu naprawdę: **„na ekranie stanął wiersz `/stop`"**. Wiersz nie
//! jest zatrzymaniem — zatrzymaniem jest to, co okno z tym wierszem robi. Dlatego
//! `a_confirmed_stop_brings_the_run_down_and_names_what_it_stopped` odpala PRAWDZIWY bieg
//! z prawdziwą grupą procesów i kończy pytaniem do jądra (niezmiennik 6).
//!
//! # Dlaczego zgoda jest sprawdzana TUTAJ, a nie tylko w prompcie
//!
//! 2026-09-06 WF-10: confirmed od modelu nie dowodzi niczyjej zgody. Kryterium odpowiada
//! prawdziwą drogą człowieka do Waiting, odbiera jednorazowy token i używa go w `Desk.stop_run`.
//! Nie symuluje już wykonania Suggested przez okno; backend musi sam zakończyć istniejący bieg.
//!
//! Testy odpalają prawdziwe procesy i **nie są** `#[ignore]`: cel z samymi pominiętymi testami
//! melduje „0 passed", a to nie jest dowód (niezmiennik 19).

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `lead_reaches_loadouts_own_verbs` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// `panic!` w teście: odmowa czasownika, którego to kryterium potrzebuje, jest właśnie porażką.
#![allow(clippy::panic)]

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use loadout_lib::bridge::host::Answers;
use loadout_lib::bridge::library::{Desk, Waiting};
use loadout_lib::bridge::{Answer, Call, Role, verbs};
use loadout_lib::commands::lead_history::LeadRunLookup;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, Outcome, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::line::Line;
use loadout_lib::engine::supervisor::{self, GroupId, GroupProof, StdinPlan, Supervised};
use loadout_lib::ipc::{LineSource, line_channel};
use loadout_lib::library::agents::read_agent_file;
use loadout_lib::store::Store;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Pojemność strumienia wierszy. Z zapasem — mierzymy obecność, nie przepustowość.
const LINES: usize = 1_024;

/// Etykieta vendora dublera.
const VENDOR: &str = "fake";

/// Tytuł biegu w synteticznym `run.json` — dla przypadków, w których nic nie schodzi.
const TITLE: &str = "Ship a feature";

/// Tytuł biegu, który NAPRAWDĘ idzie. Ten sam napis, co `name` w pliku workflow niżej.
const RUN_TITLE: &str = "One step that never ends";

/// Katalog biegu. Nazwa otwiera się znacznikiem czasu UTC, dokładnie jak w produkcji.
const RUN_FOLDER: &str = "20260901-150035__01990000-0000-7000-8000-0000000039a1";

/// Okno łaski między SIGTERM a SIGKILL, podane argumentem zamiast wzięte ze stałej produkcyjnej.
const GRACE: Duration = Duration::from_secs(1);

/// Ile czekamy, aż krok wystawi swoją grupę procesów.
///
/// HOJNE Z ROZMYSŁEM i niczego to nie osłabia: bariera jest PRZYGOTOWANIEM, nie pomiarem. Ten
/// plik pyta, czy bieg zszedł, a odpowiedź nie zależy od tego, jak długo wcześniej wstawała
/// powłoka na obciążonej maszynie.
const START_LIMIT: Duration = Duration::from_mins(2);

/// Odstęp między pytaniami o gotowość. Krótki, bo nic tu nie mierzy czasu.
const PROBE_POLL: Duration = Duration::from_millis(2);

/// Ile czekamy, zanim uznamy bieg albo Stop za zawieszone. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Krok, który **nie kończy się sam**. Schodzi wyłącznie przez eskalację Loadouta — czyli przez
/// tę drogę, o którą pyta to kryterium.
///
/// Plik ze skryptem i **pętla**, nigdy pojedyncza komenda: powłoka exec-optymalizuje ostatnią
/// komendę, a wtedy grupa, którą obserwuje test, należy do procesu, którego już nie ma [T7 §8.2].
const NEVER_ENDS: &str = r#"#!/bin/sh
# $1 = plik gotowości
: > "$1"
while :; do
  sleep 0.2
done
"#;

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000039a1
name: Hand
summary: Does the work
color: moss
runsWith: claude-code
model: opus
thinking: balanced
fileAccess: work-freely
giveUpAfterMinutes: 20
writeResultsTo: \"\"
tools: everything
skills: []
connections: []
---
Do the work.
";

/// Jeden krok, który nie kończy się sam.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_never_ends",
  "name": "One step that never ends",
  "steps": [
    {
      "kind": "agent",
      "id": "s_hand",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000039a1",
      "overrides": {},
      "instructions": "never end",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Wywołanie czasownika prosto na biurku, bez gniazda i bez vendora.
async fn stop_run(desk: &Desk, input: Value) -> Answer {
    desk.answer(Call {
        id: Value::from(1),
        call: "stop_run".to_owned(),
        input,
    })
    .await
}

/// Bieg zapisany na dysku, którego nikt nie prowadzi — dla przypadków, w których nic nie schodzi.
fn a_run_is_going(project: &Path) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".loadout").join("runs").join(RUN_FOLDER);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("run.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": "01990000-0000-7000-8000-0000000039a1",
            "workflow_id": "wf_ship_a_feature",
            "title": TITLE,
            "status": "running",
            "steps": [
                { "id": "s_reaserch", "name": "Reaserch", "status": "running" },
                { "id": "s_combine", "name": "Combine", "status": "pending" }
            ],
        }))?,
    )?;
    Ok(())
}

/// Biurko z drogą na ekran plus podsłuch tego, co na niej stanęło.
fn desk_that_shows(home: &Path, project: &Path) -> (Desk, LineSource) {
    let (sink, source) = line_channel(LINES);
    let desk = Desk::at(Some(home.to_path_buf()), project.to_path_buf())
        .showing(Arc::new(Mutex::new(sink)));
    (desk, source)
}

#[test]
fn the_lead_can_see_the_verb_at_all() {
    let listed = verbs::tool_list(Role::Lead);
    let names: Vec<&str> = listed
        .as_array()
        .expect("the verb table is an array of tool definitions")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();

    assert!(
        names.contains(&"stop_run"),
        "the lead cannot see a verb that stops a run, so the only thing left to it is what it \
         actually did on 2026-09-01: read the pgid out of run.json and send a signal by hand. \
         It saw: {names:?}"
    );

    let stop = verbs::for_role(Role::Lead)
        .into_iter()
        .find(|verb| verb.name == "stop_run")
        .expect("the lead can stop a run");
    let required: Vec<&str> = stop
        .schema
        .pointer("/required")
        .and_then(Value::as_array)
        .expect("the schema says what it needs")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        required,
        vec!["run_id", "approval_token"],
        "the call must name the exact run and a token obtained from a real human answer"
    );
    assert!(
        stop.describe.contains("kill"),
        "the description has to name the thing the lead must not do. It reached for kill because \
         it had nothing else; a description that says only what stop_run does leaves that habit \
         untouched. It said: {}",
        stop.describe
    );
}

#[tokio::test]
async fn stop_run_refuses_until_the_person_confirmed() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    a_run_is_going(project.path())?;
    let (desk, mut stream) = desk_that_shows(home.path(), project.path());

    match stop_run(
        &desk,
        serde_json::json!({"run_id":"01990000-0000-7000-8000-0000000039a1"}),
    )
    .await
    {
        Answer::Refused(said) => {
            assert!(
                said.contains("no longer active"),
                "a saved running file is not a live process owner: {said}"
            );
            assert!(
                said.contains("status"),
                "the refusal must say how to find the actual addressed run: {said}"
            );
            assert!(matches!(stream.try_next(), Some(Line::Problem { text, .. }) if text == said));
        }
        Answer::Ok(value) => panic!("a run must never go down unasked: {value}"),
    }

    assert!(
        stream.try_next().is_none(),
        "the refusal must not also emit a mutating Suggested row"
    );

    // `false` jest KLUCZEM, którego schemat wymaga, i to jest inna droga niż brak klucza:
    // model, który zgadł „chyba tak", ma dostać tę samą odpowiedź co model, który nie zgadywał.
    match stop_run(&desk, serde_json::json!({ "confirmed": false })).await {
        Answer::Refused(said) => assert!(!said.is_empty()),
        Answer::Ok(value) => panic!("confirmed: false is not a yes: {value}"),
    }
    Ok(())
}

/// PRAWDZIWY BIEG SCHODZI, i to jest jedyne kryterium tego pliku, które o to pyta.
///
/// # Słaba wersja, którą ten przypadek zastąpił
///
/// „Na ekranie stanął wiersz `/stop`". Przechodziła dla biurka, które wiersz stawia, i dla biegu,
/// który dalej mieli i dalej płaci — czyli dla dokładnie tego stanu z 2026-09-01, w którym lider
/// napisał człowiekowi, że zatrzymał pracę. Wiersz nie jest zatrzymaniem; zatrzymaniem jest to,
/// co okno z tym wierszem robi.
///
/// Ten przypadek przechodzi więc CAŁĄ drogę: biurko → wiersz na ekranie → to, co z niego robi
/// okno (`autoStarts` → `runSuggestion('/stop')` → `stop`) → rdzeń, do którego trafia komenda
/// (`stop_if_anything_is_going`) → dowód z jądra. Modelowana jest wyłącznie warstwa okna, napisana
/// w JavaScript, bo jej z Rusta nie da się uruchomić — jej własnym sędzią jest
/// `src/sections/run/feed/a-suggested-stop-goes-the-one-way-a-run-is-stopped.test.ts`.
///
/// Samo pytanie do biurka i wszystko, co po nim, stoi w [`the_lead_stops_it_and_the_kernel_agrees`]
/// niżej; tutaj zostaje ława. Podział jest po to, żeby ta historia dała się przeczytać w całości,
/// a nie po to, żeby coś skrócić — nie ubyła z niej ani jedna asercja.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_confirmed_stop_brings_the_run_down_and_names_what_it_stopped()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let hand = bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("never-ends", WORKFLOW)?;
    let script = write_script(bench.project.path(), "never-ends.sh", NEVER_ENDS)?;
    let ready = bench.project.path().join("the-step-is-up");
    let store = Store::open(&bench.db())?;
    read_agent_file(&hand)?;

    let started: Arc<Mutex<Option<GroupId>>> = Arc::new(Mutex::new(None));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: drivers_for(Arc::new(Fake {
            script,
            ready: ready.clone(),
            started: Arc::clone(&started),
        })),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (run_sink, _run_lines) = line_channel(LINES);
    /* DWA STRUMIENIE, bo w produkcji są dwa obiekty: bieg pisze swoim `LineSink`, a rozmowa
     * swoim (`commands::chat`, „biurko widzi ten sam strumień, co rozmowa"). Okno skleja je
     * dopiero po swojej stronie, w jednym terminalu. */
    let (desk, mut conversation) = desk_that_shows(bench.home.path(), bench.project.path());
    let waiting = Arc::new(Waiting::default());
    let fixed = deps.control.clone();
    let desk = desk
        .hearing(Arc::clone(&waiting))
        .reading_runs_with(LeadRunLookup::new(move |_| Some(fixed.clone())));

    let watching = the_lead_stops_it_and_the_kernel_agrees(
        &desk,
        &waiting,
        &mut conversation,
        &deps,
        &started,
        &ready,
    );

    let (ran, checked) = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(&deps, &request, run_sink), watching)
    })
    .await
    .map_err(|_| format!("neither the run nor the lead's stop came back within {PATIENCE:?}"))?;
    checked?;
    let report = ran?;
    assert_eq!(
        report.outcome,
        Outcome::Cancelled,
        "the run has to report that it was stopped, not that it merely ended — this is the same \
         answer the person gets from the Stop button, and the lead must not have a second one"
    );
    Ok(())
}

/// Cała historia zatrzymania, od pytania lidera do odpowiedzi jądra — biegnie OBOK żywego biegu.
///
/// Osobna funkcja, a nie blok w środku kryterium wyżej, bo tamto stoi pod sufitem stu linii;
/// ten sam podział i ten sam powód, co przy `Live::stop_overdue_agent` po stronie produkcji.
/// Nie ubyła tu ani jedna asercja — zmieniło się wyłącznie miejsce, w którym stoją.
///
/// # Kontrola dodatnia i kontrola negatywna, obie na TYM SAMYM biegu
///
/// Przed zgodą grupa procesów kroku musi odpowiadać na sygnał zerowy — bez tego `ESRCH` na końcu
/// znaczy równie dobrze „biegu nigdy nie było", a kryterium przechodzi na pustym zbiorze. A po
/// odmowie (bez `confirmed`) ta sama grupa musi odpowiadać **nadal**: „nic nie schodzi bez zgody"
/// jest zdaniem o żywym procesie, nie o treści odmowy.
async fn the_lead_stops_it_and_the_kernel_agrees(
    desk: &Desk,
    waiting: &Waiting,
    conversation: &mut LineSource,
    deps: &RunDeps<'_>,
    started: &Mutex<Option<GroupId>>,
    ready: &Path,
) -> Result<(), Box<dyn Error>> {
    let group = wait_for_group(started, START_LIMIT).await?;
    assert!(
        wait_for_file(ready, START_LIMIT).await,
        "the step never reported that it was up, so there was nothing running to stop and every \
         probe below would be about an empty group"
    );
    // ── Kontrola dodatnia ─────────────────────────────────────────────────────────────────
    assert!(
        group_probe(group.pgid).is_ok(),
        "kill(-{}, 0) does not find the step's process group even before the lead asks, so ESRCH \
         at the end would prove nothing: it would mean the run was never there",
        group.pgid
    );

    // ── (a) BEZ ZGODY NIC NIE SCHODZI, i to jest zdanie o żywym procesie ───────────────────
    let run_id = deps
        .control
        .run_address()
        .ok_or("the running step has no durable identity")?
        .id;
    match stop_run(desk, serde_json::json!({"run_id":run_id,"confirmed":true})).await {
        Answer::Refused(said) => assert!(
            said.contains("ask_the_person"),
            "the refusal has to name the move that unblocks it: {said}"
        ),
        Answer::Ok(value) => panic!("a run must never go down unasked: {value}"),
    }
    assert!(
        group_probe(group.pgid).is_ok(),
        "the run went down even though nobody confirmed it. The person never answered, and the \
         work they were paying for is gone"
    );
    assert!(
        deps.control.is_working(),
        "and the run itself has to still be going: a refusal that settles the run has refused in \
         words only"
    );

    // ── (b) ZE ZGODĄ — odpowiedź dla modelu i wiersz dla człowieka ────────────────────────
    let approval = actual_human_confirmation(desk, waiting, conversation, &run_id).await?;
    let value = match stop_run(
        desk,
        serde_json::json!({ "run_id":run_id,"approval_token":approval }),
    )
    .await
    {
        Answer::Ok(value) => value,
        Answer::Refused(sentence) => {
            panic!("the person said yes and the run still did not go down: {sentence}")
        }
    };
    let note = value
        .get("said")
        .and_then(Value::as_str)
        .expect("the answer carries a sentence for the model");
    assert!(
        note.contains(RUN_TITLE),
        "the answer has to tell the lead WHAT it stopped, by the name this person knows it by — \
         otherwise it reports back \"stopped the run\" and neither of them can tell which. It \
         said: {note}"
    );
    assert!(
        note.contains("1 step"),
        "and how much work that was. It said: {note}"
    );

    // ── (c) TO, CO OKNO ROBI Z TYM WIERSZEM ───────────────────────────────────────────────
    let mut receipt = false;
    while let Some(line) = conversation.try_next() {
        assert!(
            !matches!(line, Line::Suggested { auto: true, .. }),
            "the host delegated control back to an unaddressed UI command"
        );
        if matches!(line, Line::Note { text, .. } if text == note) {
            receipt = true;
        }
    }
    assert!(
        receipt,
        "the person did not see the same final receipt as the Lead"
    );
    assert!(
        !deps.control.is_working(),
        "the receipt preceded the real run settling"
    );

    // ── (d) DOWÓD Z JĄDRA (niezmiennik 6) ─────────────────────────────────────────────────
    assert_eq!(
        group_probe(group.pgid)
            .err()
            .and_then(|error| error.raw_os_error()),
        Some(libc::ESRCH),
        "the stop came back while kill(-{}, 0) still finds somebody in the step's group. That is \
         the whole defect this verb exists to close: the lead tells the person the run is over, \
         the agent keeps writing and keeps paying (invariant 6)",
        group.pgid
    );
    Ok(())
}

/// Kiedy nic nie idzie, odpowiedzią jest to samo zdanie, którym odpowiada okno na `/stop`.
///
/// PRZED PYTANIEM O ZGODĘ, i to jest wybór: zgoda na czynność bez skutku jest pytaniem, którego
/// nie ma o co zadać — a lider, który je zada, wysyła człowieka do klikania nad biegiem, którego
/// nie ma.
#[tokio::test]
async fn a_folder_with_nothing_going_says_so_instead_of_asking() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    let (desk, mut stream) = desk_that_shows(home.path(), project.path());

    let here = project
        .path()
        .file_name()
        .expect("a temporary folder has a name")
        .to_string_lossy()
        .into_owned();
    match stop_run(
        &desk,
        serde_json::json!({ "run_id":"01990000-0000-7000-8000-0000000039a1", "confirmed": true }),
    )
    .await
    {
        Answer::Refused(said) => assert!(
            said.contains(&here),
            "the sentence has to name the folder it looked in: with two cards open, \"nothing is \
             running\" over a neighbour's working agent is the defect Z-35 closed on the button. \
             It said: {said}"
        ),
        Answer::Ok(value) => panic!("nothing was running, so nothing could be stopped: {value}"),
    }
    assert!(
        matches!(stream.try_next(), Some(Line::Problem { .. })),
        "the person must see why nothing could be stopped"
    );
    Ok(())
}

/// Rozmowa bez okna nie zatrzymuje niczego — ta sama granica, co przy starcie.
#[tokio::test]
async fn a_desk_with_no_screen_refuses_to_stop_anything() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    a_run_is_going(project.path())?;
    /* BEZ `showing`: rozmowa, której okno jeszcze nie otworzyło strumienia. */
    let desk = Desk::at(
        Some(home.path().to_path_buf()),
        project.path().to_path_buf(),
    );

    assert!(
        matches!(
            stop_run(&desk, serde_json::json!({ "confirmed": true })).await,
            Answer::Refused(_)
        ),
        "the row on the screen IS the stop, so with no stream there is nothing to send — and a \
         lead that answers 'stopped' over a run that is still going is the worst of the two \
         possible lies"
    );
    Ok(())
}

// ── ŁAWA ───────────────────────────────────────────────────────────────────────────────────

/// Komenda, którą okno naprawdę by uruchomiło z tego strumienia — model `autoStarts`, warunek
/// w warunek.
///
/// TRZY WARUNKI, KAŻDY OSOBNO KONIECZNY (`src/sections/run/auto-start.ts`): rodzaj `suggested`,
/// `auto == true` i niepusta komenda, plus podpis, bez którego okno porzuca wiersz w ciszy. Wiersz,
/// który któregokolwiek z nich nie spełnia, jest zatrzymaniem, które nigdy się nie wydarzy —
/// i dlatego to kryterium pyta o nie tutaj, a nie sięga po pole wariantu na siłę.
async fn actual_human_confirmation(
    desk: &Desk,
    waiting: &Waiting,
    stream: &mut LineSource,
    run_id: &str,
) -> Result<String, Box<dyn Error>> {
    let ask = desk.answer(Call {
        id: Value::from(2),
        call: "ask_the_person".to_owned(),
        input: serde_json::json!({"operation":"stop_run","run_id":run_id}),
    });
    tokio::pin!(ask);
    let human = async {
        loop {
            if let Some(Line::Asked {
                text,
                question: Some(question),
                ..
            }) = stream.try_next()
            {
                assert!(text.contains(RUN_TITLE));
                assert!(waiting.answer_exact("Lead", &question.question_id, "Stop run".to_owned()));
                return;
            }
            tokio::time::sleep(PROBE_POLL).await;
        }
    };
    tokio::select! {
        reply = &mut ask => panic!("the host did not ask the human before stopping: {reply:?}"),
        () = human => {},
    }
    match ask.await {
        Answer::Ok(value) => value["approvalToken"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "human answer produced no token".into()),
        Answer::Refused(said) => Err(said.into()),
    }
}

/// Pyta jądro, czy w grupie `pgid` jest jeszcze ktokolwiek — **nie wysyłając sygnału**.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku, a zapłacone są wnuki.
// `kill(2)` nie ma bezpiecznego opakowania w std. Plik testowy jest wyłączony ze wszystkich
// trzech granic architektury po ŚCIEŻCE (checks/boundary.sh), bo nie jest częścią wysyłanego
// artefaktu — a ten test z definicji pyta system operacyjny zamiast naszego kodu (niezmiennik 20).
// Ta sama konstrukcja stoi w tests/it/run_stop_waits_for_proof.rs.
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza tylko istnienie i prawa.
    // Argumenty to zwykłe liczby, więc nie ma tu żadnego wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Czeka, aż plik się pojawi. `false`, kiedy się nie doczekał.
async fn wait_for_file(path: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
    false
}

/// Czeka, aż krok wystawi swoją grupę procesów.
async fn wait_for_group(
    started: &Mutex<Option<GroupId>>,
    limit: Duration,
) -> Result<GroupId, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        // Zamek brany i oddany w jednym wyrażeniu: między nim a `await` niżej nie ma ani jednej
        // instrukcji (niezmiennik 8).
        let seen = *started.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(group) = seen {
            return Ok(group);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no step started a process group within {limit:?}, so there is nothing for the \
                 lead to stop. Either the run never reached the driver, or it came back before \
                 it got there"
            )
            .into());
        }
        tokio::time::sleep(PROBE_POLL).await;
    }
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Fabryka, która dla każdego vendora oddaje ten sam dubler.
fn drivers_for(driver: Arc<dyn AgentDriver>) -> Drivers {
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("agents"))?;
        fs::create_dir_all(home.path().join("workflows"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("agents").join(format!("{slug}.md"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}

/// Dubler sterownika: stawia **prawdziwy** proces we własnej grupie i oddaje jego `pgid`.
///
/// Prawdziwy proces, a nie atrapa, bo przedmiotem tego kryterium jest odpowiedź **jądra**.
/// Zmyślony `GroupProof::Dead` przechodziłby każdą asercję o wartości zwracanej i nie mówiłby
/// nic o tym, czy cokolwiek zginęło.
#[derive(Debug)]
struct Fake {
    script: PathBuf,
    ready: PathBuf,
    started: Arc<Mutex<Option<GroupId>>>,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    async fn start(
        &self,
        spec: RunSpec,
        _events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };

        let mut command = tokio::process::Command::new(&self.script);
        command.arg(&self.ready);
        let child = supervisor::spawn(command, StdinPlan::Null)?;
        let group = child.group();
        *self.started.lock().unwrap_or_else(PoisonError::into_inner) = Some(group);

        Ok(Box::new(Turn {
            session,
            child,
            group,
        }))
    }
}

/// Jedna tura dublera: żywa grupa procesów, która schodzi wyłącznie przez `cancel`.
#[derive(Debug)]
struct Turn {
    session: SessionRef,
    child: Supervised,
    group: GroupId,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        Some(self.group)
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        // Krok nie kończy się sam, więc to czekanie kończy dopiero śmierć procesu — czyli
        // eskalacja z `cancel`. Wartość jest tu dla kompletności typu, nie dla kryterium.
        let _status = self.child.wait().await?;
        Ok(TurnOutcome {
            ok: false,
            reason: FinishReason::Cancelled,
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
            session: self.session.clone(),
        })
    }

    async fn cancel(&mut self) -> GroupProof {
        // Pełna eskalacja z nadzoru: TERM na grupę, okno łaski, KILL na grupę, i dopiero potem
        // dowód. Adapter, który skraca ją do `start_kill`, traci wznawialność sesji [T1 §4.6].
        self.child.stop(GRACE).await
    }

    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(None)
    }
}
