//! AC-1 dla T-87: runda k+1 dostaje wejście pętli, własną poprzednią odpowiedź i to, co
//! powiedział sędzia — w kolejności, która nie zależy od tego, kto skończył pierwszy.
//!
//! # Co dziś dostaje runda druga
//!
//! Kontekst kroku składa `Live::handed_before` (`commands/run.rs`) i bierze **wyłącznie
//! bezpośrednich poprzedników po strzałce** w rozwiniętym grafie. Jedynym poprzednikiem rundy k+1
//! kroku roboczego jest powrót od sędziego, więc agent poprawiający dostaje jedno zdanie krytyki
//! i **nic więcej**: ani planu, od którego zaczął, ani własnej poprzedniej odpowiedzi, którą ma
//! poprawić.
//!
//! Zmierzone w biegu `20260823-145648` (`~/Projects/urc-monorepo/.loadout/runs/`, pliki
//! `logs/*.input.json`): krok `s_2#1` dostał **tylko** `12__verification-1`, a `s_2#2` **tylko**
//! `13__verification-1`. W czterech biegach dwie z trzech pętli nie zbiegły się ani razu —
//! dziewięć rund, zero przejść. Trudno się dziwić: każda runda zaczynała od zera.
//!
//! # SŁABĄ WERSJĄ tego kryterium jest „indeks rundy drugiej jest dłuższy"
//!
//! Przechodzi ją implementacja, która dokłada rundzie 2 wszystko, co bieg zdążył napisać —
//! razem z pracą sąsiedniej gałęzi, której ten agent nie ma prawa oglądać, i w kolejności
//! zależnej od tego, kto akurat skończył. Dlatego niżej stoi porównanie CAŁEJ listy, z jej
//! kolejnością, do listy wypisanej wprost.
//!
//! # I DLATEGO JEST TU KROK, KTÓREGO PĘTLA NIE DOTYCZY
//!
//! „Aside" stoi obok pętli i ma jednego poprzednika. Ma dostać dokładnie to, co dostaje dziś —
//! jedną pozycję. Implementacja, która „na wszelki wypadek" dokłada każdemu krokowi wszystko,
//! przechodzi obie asercje o pętli i przewraca się na tej jednej.
//!
//! # DRUGA ŁAWKA: PĘTLA ZA PĘTLĄ
//!
//! Wejściem pętli bywa inna pętla, i wtedy „to, co dostała runda pierwsza" ma DWIE różne
//! odpowiedzi. Runda pierwsza pyta o wejście przez to samo miejsce, co każdy krok za pętlą
//! (`Live::handed_before` → `leaving_a_loop` → `what_that_loop_produced`), więc dostaje to, co
//! tamta pętla naprawdę wyprodukowała. Runda druga liczyła to sama, po literalnym rodzicu
//! z grafu — a literalnym rodzicem jest runda OSTATNIA tamtej pętli, czyli węzeł, który po
//! werdykcie `pass` nie biegnie wcale i nie zostawia pliku.
//!
//! Skutek jest dokładnie odwrotny do tego, po co ta pętla jest: runda druga, która ma poprawiać,
//! widzi mniej niż runda pierwsza, która zaczynała. Druga ławka niżej sądzi ten jeden fakt.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{GroupId, GroupProof};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają własne wymagania co do
/// dowodów biegu, a to kryterium sądzi tekst promptu.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(2);

/// Ile razy pętla wolno próbuje. Trzy, bo dopiero runda trzecia odróżnia „widzę poprzednią"
/// od „widzę wszystkie poprzednie".
const TRIES: usize = 3;

/// Początek instrukcji każdego kroku — po nim, i tylko po nim, dubler poznaje, kto pyta.
/// `RunSpec` nie niesie nazwy kroku (niezmiennik 9), a instrukcja jest tym, co ten krok dostał.
const ASKED: [(&str, &str); 9] = [
    ("plan:", "Plan"),
    ("work:", "Work"),
    ("test:", "Tester"),
    ("ship:", "Ship"),
    ("aside:", "Aside"),
    ("early-work:", "Early"),
    ("early-test:", "Early check"),
    ("late-work:", "Late"),
    ("late-test:", "Late check"),
];

/// Nazwy kafelków w obu ławkach — te same, które stoją w plikach workflow niżej.
const NAMES: [&str; 9] = [
    "Plan",
    "Work",
    "Tester",
    "Aside",
    "Ship",
    "Early check",
    "Early",
    "Late check",
    "Late",
];

/// Kafelek, który stoi obok pętli i niczego z niej nie widzi. Kontrola tego kryterium.
const ASIDE: &str = "Aside";

/// W której próbie sędzia danej gałęzi mówi „przeszło". Sędzia spoza tej listy nie przepuszcza
/// nigdy — wszystkie jego rundy mają naprawdę pobiec.
const PASSES_ON: [(&str, usize); 1] = [("Early check", 1)];

const HAND_FILE: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d1
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

/// `plan → work → tester → ship`, powrót `tester → work` do trzech rund, i „aside" obok pętli.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą. Rundy JEDNEGO kroku dzielą katalog i o to
/// chodzi — bez tego runda 2 nie widzi poprawek rundy 1.
const LOOP_FILE: &str = r#"{
  "format": 1,
  "id": "wf_round_sees_its_own_past",
  "name": "A loop that remembers",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "plan: say what to build.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_work",
      "name": "Work",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "work: make the change.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_test",
      "name": "Tester",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "test: say whether it is good enough to build on.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_aside",
      "name": "Aside",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "aside: write the note nobody sends back.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 264, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_ship",
      "name": "Ship",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "ship: put it out.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 456 }
    }
  ],
  "links": [
    { "from": "s_plan", "to": "s_work" },
    { "from": "s_plan", "to": "s_aside" },
    { "from": "s_work", "to": "s_test" },
    { "from": "s_test", "to": "s_ship" },
    { "from": "s_test", "to": "s_work", "max_turns": 3 }
  ]
}"#;

/// Dwie pętle jedna za drugą: `plan → early → early check`, powrót do dwóch rund, a za nim
/// `late → late check` z własnym powrotem.
///
/// Sędzia pierwszej pętli przepuszcza w rundzie PIERWSZEJ, więc jej runda ostatnia nie biegnie
/// wcale i nie zostawia pliku — a to ona jest literalnym rodzicem wejścia drugiej pętli.
/// Ta pierwsza pętla jest więc wejściem, którego nie da się przeczytać z samego grafu.
const TWO_LOOPS_IN_A_ROW: &str = r#"{
  "format": 1,
  "id": "wf_loop_after_a_loop",
  "name": "A loop whose input is a loop",
  "steps": [
    {
      "kind": "agent",
      "id": "s_plan",
      "name": "Plan",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "plan: say what to build.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 24 }
    },
    {
      "kind": "agent",
      "id": "s_early_work",
      "name": "Early",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "early-work: do the first part.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 168 }
    },
    {
      "kind": "agent",
      "id": "s_early_test",
      "name": "Early check",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "early-test: say whether the first part is good enough.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 312 }
    },
    {
      "kind": "agent",
      "id": "s_late_work",
      "name": "Late",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "late-work: build on the first part.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 456 }
    },
    {
      "kind": "agent",
      "id": "s_late_test",
      "name": "Late check",
      "agent": "01990000-0000-7000-8000-0000000000d1",
      "overrides": {},
      "instructions": "late-test: say whether the second part is good enough.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 24, "y": 600 }
    }
  ],
  "links": [
    { "from": "s_plan", "to": "s_early_work" },
    { "from": "s_early_work", "to": "s_early_test" },
    { "from": "s_early_test", "to": "s_early_work", "max_turns": 2 },
    { "from": "s_early_test", "to": "s_late_work" },
    { "from": "s_late_work", "to": "s_late_test" },
    { "from": "s_late_test", "to": "s_late_work", "max_turns": 2 }
  ]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_later_try_is_given_the_input_of_the_loop_and_everything_it_already_did()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("loop", LOOP_FILE)?;
    let store = Store::open(&bench.db())?;
    // Sędzia nie przepuszcza nigdy, więc wszystkie trzy rundy naprawdę biegną.
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let seen = watch.seen();
    let work = prompts_of("Work", &seen);
    let judged = prompts_of("Tester", &seen);
    assert_eq!(
        (work.len(), judged.len()),
        (TRIES, TRIES),
        "the fixture is wrong unless all three tries really ran; every assertion below would \
         then be about a round that never happened. The run ended as {:?} and the driver was \
         asked by: {:?}",
        report.steps,
        watch.who()
    );

    // Kto napisał który plik — wyjęte z samych przekazań, nie z numerów węzłów.
    let wrote = who_wrote_what(&report.dir)?;

    // ── RUNDA DRUGA ──────────────────────────────────────────────────────────────────────────
    assert_eq!(
        named(&files_listed(&work[1]), &wrote),
        vec!["Plan try 1", "Work try 1", "Tester try 1"],
        "the second try of the work step was handed something other than the input of the loop, \
         its own first answer and what the tester said about it. Today it gets the tester alone: \
         the agent is asked to fix work it cannot see, against a plan it was never shown, and \
         that is why nine rounds of one owner's runs converged zero times. The prompt was: {:?}",
        work[1]
    );

    // ── RUNDA TRZECIA: OBIE POPRZEDNIE, NIE SAMA OSTATNIA ────────────────────────────────────
    assert_eq!(
        named(&files_listed(&work[2]), &wrote),
        vec![
            "Plan try 1",
            "Work try 1",
            "Work try 2",
            "Tester try 1",
            "Tester try 2"
        ],
        "the third try does not see both earlier tries. An implementation that carries only the \
         round just before it loses the first attempt entirely, so the agent repeats the mistake \
         the tester already turned down once — and the order has to be step first, then try, or \
         two runs of the same file read differently depending on who answered faster. The \
         prompt was: {:?}",
        work[2]
    );

    // ── KONTROLA: KROK, KTÓREGO PĘTLA NIE DOTYCZY ────────────────────────────────────────────
    let aside = prompts_of(ASIDE, &seen);
    assert_eq!(
        aside.len(),
        1,
        "the fixture is wrong if the step beside the loop did not run exactly once"
    );
    assert_eq!(
        named(&files_listed(&aside[0]), &wrote),
        vec!["Plan try 1"],
        "the step beside the loop was given more than the one step before it. Handing every \
         agent everything the run has written so far is the easy way to pass the two points \
         above, and it hands this one work from a branch it has no business reading. The \
         prompt was: {:?}",
        aside[0]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_later_try_of_the_second_loop_still_sees_what_the_first_one_left()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND_FILE)?;
    let workflow = bench.workflow("two-in-a-row", TWO_LOOPS_IN_A_ROW)?;
    let store = Store::open(&bench.db())?;
    let watch = Arc::new(Watch::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&watch)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let report = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))??;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let seen = watch.seen();
    let early = prompts_of("Early", &seen);
    let late = prompts_of("Late", &seen);
    assert_eq!(
        (early.len(), late.len()),
        (1, 2),
        "the bench is only a bench if the first part passed on its first try — its last try must \
         never run — while the second part really tried twice. The run ended as {:?} and the \
         driver was asked by: {:?}",
        report.steps,
        watch.who()
    );

    let wrote = who_wrote_what(&report.dir)?;

    // ── KONTROLA: PIERWSZA PRÓBA DRUGIEJ PĘTLI ───────────────────────────────────────────────
    assert_eq!(
        named(&files_listed(&late[0]), &wrote),
        vec!["Early try 1", "Early check try 1"],
        "the first try of the second part was not given what the first part really ended on. \
         Every point below is about the try AFTER this one, so a bench that is already wrong \
         here measures nothing. The prompt was: {:?}",
        late[0]
    );

    // ── DRUGA PRÓBA WIDZI TO SAMO WEJŚCIE, NIE MNIEJ ─────────────────────────────────────────
    assert_eq!(
        named(&files_listed(&late[1]), &wrote),
        vec![
            "Early try 1",
            "Early check try 1",
            "Late try 1",
            "Late check try 1"
        ],
        "the second try of the second part lost the work it was given to build on. It reads its \
         input straight off the arrow, and the step on the other end of that arrow is the LAST \
         try of the first part — the one that never ran, because the first part passed early and \
         the tries after a pass are skipped. So the try that is supposed to be fixing something \
         is handed less than the try that started from nothing, which is the exact opposite of \
         what trying again is for. The prompt was: {:?}",
        late[1]
    );
    Ok(())
}

// ── co dojechało do sterownika ─────────────────────────────────────────────────────────────

/// Prompty tego kafelka, w kolejności startów. Rundy pętli idą po sobie, nigdy równolegle,
/// więc ta kolejność JEST kolejnością rund.
fn prompts_of(name: &str, seen: &[(String, String)]) -> Vec<String> {
    seen.iter()
        .filter(|(who, _)| who == name)
        .map(|(_, prompt)| prompt.clone())
        .collect()
}

/// Nazwy plików przekazań wymienionych w tym prompcie, w kolejności wystąpień.
///
/// Po katalogu, nie po całej ścieżce: prompt niesie ścieżkę bezwzględną, bo katalogiem roboczym
/// kroku jest jego własna kopia plików, a nie katalog biegu.
fn files_listed(prompt: &str) -> Vec<String> {
    prompt
        .match_indices("handoffs/")
        .map(|(at, marker)| {
            let rest = &prompt[at + marker.len()..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            rest[..end]
                .trim_end_matches([',', ';', ':', ')'])
                .to_owned()
        })
        .collect()
}

/// Te same pliki, nazwane tym, kto je napisał i w której próbie.
fn named(files: &[String], wrote: &BTreeMap<String, String>) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            wrote
                .get(file)
                .cloned()
                .unwrap_or_else(|| format!("nobody we know wrote {file}"))
        })
        .collect()
}

/// Plik przekazania → podpis tego, kto go zostawił („Work try 2").
///
/// Czytane z CIAŁA pliku, nie z numeru w nazwie: numer węzła jest ustaleniem `workflow::unroll`,
/// a to kryterium pyta o to, czyja praca dojechała do agenta, nie o to, jak ją ponumerowano.
fn who_wrote_what(dir: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir.join("handoffs")) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let text = fs::read_to_string(entry.path())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(mark) = signature_in(&text) {
            out.insert(name, mark);
        }
    }
    Ok(out)
}

/// Podpis wpisany przez dublera w ciało odpowiedzi — albo nic.
fn signature_in(text: &str) -> Option<String> {
    for name in NAMES {
        for try_number in 1..=TRIES {
            let mark = format!("{name} try {try_number}");
            if text.contains(&mark) {
                return Some(mark);
            }
        }
    }
    None
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Kto został o co poproszony i co dokładnie dostał na wejściu.
#[derive(Debug, Default)]
struct Watch(Mutex<Vec<(String, String)>>);

impl Watch {
    /// Zapisuje start i oddaje tekst, którym ta tura się skończy.
    ///
    /// Numer próby liczony z LICZBY startów tego kafelka, bo instrukcja jest w każdej rundzie
    /// identyczna — i to jest właściwa fikstura: agent nie wie, którą rundę biegnie, dokładnie
    /// jak prawdziwy agent w nowej sesji.
    fn entered(&self, prompt: &str) -> String {
        let mut seen = self.lock();
        let who = who_is_asked(prompt);
        seen.push((who.clone(), prompt.to_owned()));
        let try_number = seen.iter().filter(|(name, _)| *name == who).count();
        let body = format!(
            "## Answer\n{who} try {try_number} is done.\n\n## Evidence\nnotes.txt:1\n\n## Open\n\
             nothing.\n"
        );
        if who == "Tester" || who == "Late check" {
            // Nigdy nie przepuszcza: wszystkie rundy mają naprawdę pobiec.
            return format!("{body}\noutcome: fail\n");
        }
        let Some((_, passes_on)) = PASSES_ON.iter().find(|(name, _)| *name == who) else {
            return body;
        };
        if try_number >= *passes_on {
            format!("{body}\noutcome: pass\n")
        } else {
            format!("{body}\noutcome: fail\n")
        }
    }

    fn seen(&self) -> Vec<(String, String)> {
        self.lock().clone()
    }

    fn who(&self) -> Vec<String> {
        self.lock().iter().map(|(who, _)| who.clone()).collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(String, String)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Nazwa kafelka po początku jego instrukcji. Instrukcja, której nie ma w tablicy, ląduje pod
/// SWOJĄ treścią, nie pod cudzą nazwą — wtedy asercja o liczbie rund pada i pokazuje, czego
/// ławka nie rozpoznała.
fn who_is_asked(prompt: &str) -> String {
    ASKED
        .iter()
        .find(|(opening, _)| prompt.starts_with(opening))
        .map_or_else(|| prompt.to_owned(), |(_, name)| (*name).to_owned())
}

fn fake_drivers(watch: Arc<Watch>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { watch });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Fake {
    watch: Arc<Watch>,
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
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let said = self.watch.entered(&spec.prompt);
        let session = SessionRef {
            vendor: VENDOR,
            id: spec.run_id.to_string(),
        };
        let _ = events
            .send(
                (AgentEvent::Started {
                    session: session.clone(),
                    model: spec.model.clone().unwrap_or_default(),
                    tools: Vec::new(),
                    capabilities: Vec::new(),
                })
                .into(),
            )
            .await;
        Ok(Box::new(Turn {
            events,
            session,
            said,
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    said: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
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
            text: self.said.clone(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::ZERO,
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

// ── ławka ──────────────────────────────────────────────────────────────────────────────────

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
        // Żeby „własna kopia twoich plików" miała co kopiować.
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    fn agent(&self, slug: &str, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.home.path().join("agents").join(format!("{slug}.md")),
            text,
        )?;
        Ok(())
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
