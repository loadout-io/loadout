//! AC-1 dla T-92: po biegu zostają najwyżej trzy kandydatki, każda z powodem.
//!
//! Podsystem pamięci jest zbudowany od strony **czytnika** i do dziś nie został użyty od strony
//! **pisarza**. Zmierzone 2026-08-23, po 23 biegach właściciela: `~/.loadout/memory/` **nie
//! istnieje**, tabela `memory` ma zero wierszy, a `record_candidate*` ma wołających wyłącznie
//! w testach i w imporcie. Sekcja Pamięć rysuje trzy strefy nad pustym katalogiem, budżety
//! pilnują zera, wymuszony wybór nie ma czego wybierać — cały mechanizm istnieje i nie ma
//! nadawcy. To jest niezmiennik 29 czytany od strony wejścia: mechanizm jest, ekran o nim mówi,
//! **nikt nigdy nic do niego nie napisał**.
//!
//! Właściciel obszedł ten brak poza produktem: krok „Learnings" na końcu workflow i agent
//! piszący do `.claude/learnings/` w repo gospodarza. To zadanie daje pamięci pierwszego pisarza,
//! który jest w produkcie, z dyscypliną z [T6 §5.3]: **jedna** tania refleksja po biegu, **najwyżej
//! trzy** kandydatki, każda z `because`, nigdy `in-use`.
//!
//! # Cztery słabe wersje tego kryterium
//!
//! **Pierwsza: zawołać funkcję refleksji wprost i policzyć pliki.** Przechodzi na funkcji bez
//! ani jednego produkcyjnego wołającego — dokładnie tego rodzaju szew ten podsystem ma już
//! sześć. Dlatego wszystko niżej jedzie przez `run_workflow_inner`, a dubler stoi tam, gdzie
//! stoi vendor: dostaje `RunSpec` i odpowiada tak, jak odpowiedziałby model.
//!
//! **Druga: `assert!(notes.len() <= 3)`.** Przechodzi na implementacji, która nie zapisuje nic.
//! Sufit i podłoga są tu osobnymi asercjami, a scenariusz z czterema parami jest jedynym, który
//! odróżnia „przycinamy do trzech" od „bierzemy, ile przyszło".
//!
//! **Trzecia: policzyć pliki i nie zajrzeć do środka.** Kandydatka zapisana jako `in-use` jest
//! zdaniem, które od tej chwili jedzie do KAŻDEGO promptu w tym projekcie, a nikt na nie nie
//! przystał. To jest ta jedna halucynacja, która staje się trwałym prawem projektu
//! [00-SYNTHESIS §2.1] — i wygląda w liczniku plików identycznie jak poprawny zapis.
//!
//! **Czwarta: nie sprawdzić, ILE razy pytamy.** „Jedna tania refleksja" przestaje być prawdą po
//! cichu: implementacja pytająca raz na krok wygląda tak samo w katalogu notatek i różni się
//! wyłącznie rachunkiem. Dubler liczy więc każdą turę, którą zobaczył.
//!
//! # Jak dubler odróżnia turę refleksji od kroku grafu
//!
//! Po znaczniku w instrukcji kroku, nie po modelu: rozpoznawanie po modelu znaczyłoby, że
//! implementacja z innym modelem daje „nie było refleksji" zamiast „refleksja miała zły model",
//! a to są dwie różne wady i mają się różnie nazywać. Model jest osobną asercją, przeciw
//! [`REFLECTION_MODEL`] — czyli przeciw stałej, a nie przeciw literałowi przepisanemu do testu.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` — trzy scenariusze jednego kryterium (trzy pary, cztery pary, zero par) dzielą
// jedną ławkę, jednego dublera i jeden zestaw asercji o kształcie notatki. Cięcie po granicy
// funkcji znaczyłoby trzy kopie tych asercji albo stan dzielony między testami, które cargo
// uruchamia równolegle.
//
// `clippy::format_push_string` — `answer_with` skleja odpowiedź modelu pętlą `push_str`. Ten sam
// powód, co przy poprzednim: przepisanie jej na `write!` byłoby przepisaniem fikstury, a fikstura
// jest kontraktem, przeciw któremu ta gałąź jest sądzona.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::too_many_lines
)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::memory::project_notes_root;
use loadout_lib::commands::run::{REFLECTION_MODEL, run_workflow_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Policy, Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::library::agents::Vendor;
use loadout_lib::memory::notes::{Note, Scope, Status, scan_notes};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi notatki, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_secs(20);

/// Sufit z [T6 §5.3]: „najwyżej trzy rzeczy warte zapamiętania".
const AT_MOST: usize = 3;

/// Znacznik instrukcji kroku grafu. Prompt zaczyna się od bloku „co wiadomo", więc kroku nie da
/// się rozpoznać po jego początku.
const STEP_MARK: &str = "IBEX-STEP-ONE";

const AGENT_ID: &str = "01990000-0000-7000-8000-0000000000a1";

const AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000a1
name: Backend Dev
summary: Works where the data is
color: slate
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

/// Jeden krok, jeden agent, własna kopia plików.
///
/// `whenItFails: stop` jest tu FIKSTURĄ, nie preferencją: od T-87 domyślne `carry-on` każe
/// krokowi, który padł, oddać dalej to, co zdążył powiedzieć (`Live::hand_on_its_last_words`) —
/// czyli zostawić plik w `handoffs/`. Scenariusz „bieg, po którym nie zostało nic" wymaga
/// jedynej drogi porażki, za którą nie biegnie nikt, więc nie ma komu tego pliku przeczytać.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_run_leaves_suggestions",
  "name": "One step that finishes something",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Backend",
      "agent": "01990000-0000-7000-8000-0000000000a1",
      "overrides": {},
      "instructions": "IBEX-STEP-ONE look at the queue and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "whenItFails": "stop",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

const CODEX_AGENT_ID: &str = "01990000-0000-7000-8000-0000000000d4";

/// Ten sam agent, tylko u **drugiego** vendora.
///
/// Istnieje po to, żeby „bieg poprosił fabrykę o Claude'a" dało się odróżnić od „bieg poprosił
/// fabrykę o vendora swojego kafelka": w grafie niżej nazwa Claude'a nie pada ani razu.
const CODEX_AGENT: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d4
name: Hand
summary: Does the work
color: moss
runsWith: codex
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

const CODEX_WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_run_leaves_suggestions_codex",
  "name": "One step on the other vendor",
  "steps": [
    {
      "kind": "agent",
      "id": "s_one",
      "name": "Hand",
      "agent": "01990000-0000-7000-8000-0000000000d4",
      "overrides": {},
      "instructions": "IBEX-STEP-ONE look at the queue and say what it is doing.",
      "folder": { "use": "fresh-copy" },
      "whenItFails": "stop",
      "at": { "x": 0, "y": 0 }
    }
  ],
  "links": []
}
"#;

/// Znacznik reguły numer `n`. Osobliwy na tyle, żeby nie mógł powstać z innego fragmentu tekstu.
fn rule_mark(n: usize) -> String {
    format!("IBEX-RULE-{n}")
}

/// Znacznik uzasadnienia numer `n`.
fn reason_mark(n: usize) -> String {
    format!("IBEX-REASON-{n}")
}

/// Odpowiedź modelu z `n` poprawnymi parami — dokładnie w kształcie, o który prosi prompt:
/// wiersz `rule:` i wiersz `because:`.
fn answer_with(n: usize) -> String {
    let mut out = String::from("Here is what I would keep from this run.\n\n");
    for i in 1..=n {
        out.push_str(&format!(
            "rule: {} the queue is drained in exactly one place\nbecause: {} run 7f3a step 2 \
             reproduced it twice\n\n",
            rule_mark(i),
            reason_mark(i)
        ));
    }
    out
}

/// Wszystkie notatki, które ten bieg zostawił na dysku.
fn notes_left(bench: &Bench) -> Vec<Note> {
    scan_notes(&project_notes_root(bench.project.path()))
        .expect("the project notes root has to be readable")
}

/// Bieg z jednym krokiem, który się udał i coś przekazał. Zwraca raport i to, co zobaczył dubler.
async fn a_run_that_finished(
    bench: &Bench,
    seen: &Arc<Seen>,
    reflection_says: String,
    step_succeeds: bool,
) -> Result<RunReport, Box<dyn Error>> {
    a_run_driven_by(bench, seen, reflection_says, step_succeeds, true).await
}

/// To samo, tylko z jawną odpowiedzią na pytanie „czy ten sterownik bierze turę Loadouta".
async fn a_run_driven_by(
    bench: &Bench,
    seen: &Arc<Seen>,
    reflection_says: String,
    step_succeeds: bool,
    takes_loadouts_turn: bool,
) -> Result<RunReport, Box<dyn Error>> {
    let workflow = bench.workflow("run-leaves-suggestions", WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(
            Arc::clone(seen),
            reflection_says,
            step_succeeds,
            takes_loadouts_turn,
        ),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
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
    Ok(report)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_finished_run_leaves_at_most_three_candidates_each_carrying_its_reason()
-> Result<(), Box<dyn Error>> {
    // ── Trzy pary → trzy pliki ────────────────────────────────────────────────────────────
    let bench = Bench::new()?;
    bench.agent("backend", AGENT)?;
    // Fikstura, nie asercja kryterium: krok, którego agenta nie ma w bibliotece, jest biegiem,
    // który nigdy nie rusza — a wtedy wszystko niżej jest prawdą o biegu bez ani jednej tury.
    assert!(
        AGENT.contains(AGENT_ID) && WORKFLOW.contains(AGENT_ID),
        "the fixture names {AGENT_ID} in only one of the two files that have to agree on it"
    );
    let seen = Arc::new(Seen::default());
    let report = a_run_that_finished(&bench, &seen, answer_with(3), true).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 1],
        "the step has to finish and hand something on, or there is nothing to reflect about and \
         every assertion below is true of a run that never got there. It ended as {:?}",
        report.steps
    );

    // JEDNA tura, nie jedna na krok. „Tania" przestaje być prawdą po cichu: katalog notatek
    // wygląda tak samo, a różni się wyłącznie rachunkiem za bieg.
    let turns = seen.reflections();
    assert_eq!(
        turns.len(),
        1,
        "this run asked for a reflection {} time(s). One short turn after the run [T6 section \
         5.3] — zero means memory still has no writer in the product and the section keeps \
         drawing three zones over an empty directory, more than one means every run quietly pays \
         for the same question again",
        turns.len()
    );
    let turn = &turns[0];

    // Kształt tej tury: czyta i nie zapisuje, siedzi w katalogu biegu, ma model ze stałej.
    assert_eq!(
        turn.policy,
        Policy::ReadOnly,
        "the reflection ran with {:?}. It is asked what this run taught us, not to change \
         anything: a turn that may write is one that can edit the work it is summarising, in a \
         directory the person is not watching any more",
        turn.policy
    );
    assert_eq!(
        turn.cwd, report.dir,
        "the reflection ran in {:?} instead of the run directory {:?}. The run directory is what \
         it is being asked about — handoffs, logs and run.json are all there (invariant 4) — and \
         anywhere else it is a turn asked to remember a run it cannot see",
        turn.cwd, report.dir
    );
    assert_eq!(
        turn.model.as_deref(),
        Some(REFLECTION_MODEL),
        "the reflection ran on {:?}. The model comes from one constant and stays there: a run on \
         the expensive model has no reason to think expensively about what it learned, and a \
         model chosen at the call site is two different bills for one thing",
        turn.model
    );

    let left = notes_left(&bench);
    assert_eq!(
        left.len(),
        3,
        "three good pairs left {} note(s) on disk. This is the first writer memory has ever had \
         in the product; zero here is the state this task exists to end",
        left.len()
    );

    for note in &left {
        // Nigdy `in-use`. Kandydatka zapisana jako używana jest zdaniem, które od tej chwili
        // jedzie do KAŻDEGO promptu w tym projekcie, a nikt na nie nie przystał — jedna
        // halucynacja staje się wtedy trwałym prawem projektu [00-SYNTHESIS section 2.1].
        assert_eq!(
            note.status,
            Status::Suggested,
            "a note an agent proposed came out as {:?}. Only a person promotes [ARCHITECTURE \
             section 2 q. 5]; a candidate written straight into use reaches every prompt in this \
             project without anybody ever agreeing to it. The note reads: {}",
            note.status,
            note.rule
        );
        assert_eq!(
            note.scope,
            Scope::ThisProject,
            "a note from this run came out with scope {:?}. What one run taught us is true of \
             this project — `everywhere` would carry it into every other project on this \
             machine, and a scope nobody chose is the widest one nobody noticed. The note reads: \
             {}",
            note.scope,
            note.rule
        );
        assert_eq!(
            note.from.as_deref(),
            Some(report.id.as_str()),
            "a note from this run says it came from {:?} instead of run {}. Without the run \
             there is no way back to the transcript that produced the sentence, and a claim \
             nobody can trace is one nobody can retire either [T6 section 5.1]",
            note.from,
            report.id
        );
        assert!(
            (1..=3).any(|n| note.because.contains(&reason_mark(n))),
            "a note came back with {:?} as its reason. The `because:` line the model wrote is \
             what has to land there: `no because, no memory` [T6 section 10.3] is not satisfied \
             by a sentence Loadout invented on the model's behalf, and a reason that came from \
             somewhere else cannot later be checked against the run that produced it",
            note.because
        );
    }

    let rules: Vec<&str> = left.iter().map(|note| note.rule.as_str()).collect();
    for n in 1..=3 {
        let mark = rule_mark(n);
        assert_eq!(
            rules.iter().filter(|rule| rule.contains(&mark)).count(),
            1,
            "the rule marked {mark} is on disk {} time(s), and once is the whole answer. The \
             notes read: {rules:?}",
            rules.iter().filter(|rule| rule.contains(&mark)).count()
        );
    }

    // ── Cztery pary → dalej trzy pliki ────────────────────────────────────────────────────
    //
    // Jedyny scenariusz, który odróżnia „przycinamy do trzech" od „bierzemy, ile przyszło".
    // Sufit jest po to, żeby lista rosła wolniej, niż człowiek nadąża ją czytać: nieobsługiwana
    // akrecja instrukcji jest samą chorobą, a nie objawem [T6 section 5.1].
    let greedy = Bench::new()?;
    greedy.agent("backend", AGENT)?;
    let greedy_seen = Arc::new(Seen::default());
    let greedy_report = a_run_that_finished(&greedy, &greedy_seen, answer_with(4), true).await?;
    assert_eq!(greedy_report.steps, vec![StepState::Succeeded; 1]);

    let greedy_left = notes_left(&greedy);
    assert_eq!(
        greedy_left.len(),
        AT_MOST,
        "a model that offered four things to remember left {} note(s). The ceiling is {AT_MOST} \
         and it is the whole anti-bloat mechanism of this subsystem [T6 section 5.3]: a writer \
         that takes whatever arrives turns the section into a list nobody reads, and a list \
         nobody reads is a promotion gate that is only a ritual",
        greedy_left.len()
    );
    let greedy_rules: Vec<&str> = greedy_left.iter().map(|note| note.rule.as_str()).collect();
    let recognised = greedy_rules
        .iter()
        .filter(|rule| (1..=4).any(|n| rule.contains(&rule_mark(n))))
        .count();
    assert_eq!(
        recognised, AT_MOST,
        "{recognised} of the notes on disk carry a rule the model actually wrote. Trimming to \
         three means keeping three of the four sentences, not inventing one: {greedy_rules:?}"
    );

    // ── Zero par → zero plików ────────────────────────────────────────────────────────────
    //
    // Kontrola przeciw pustej asercji z drugiej strony: bez niej wszystko powyżej jest też
    // prawdą o implementacji, która zapisuje trzy notatki niezależnie od tego, co model
    // powiedział.
    let quiet = Bench::new()?;
    quiet.agent("backend", AGENT)?;
    let quiet_seen = Arc::new(Seen::default());
    let quiet_report = a_run_that_finished(
        &quiet,
        &quiet_seen,
        "Nothing here was surprising enough to keep.".to_owned(),
        true,
    )
    .await?;
    assert_eq!(quiet_report.steps, vec![StepState::Succeeded; 1]);
    assert_eq!(
        quiet_seen.reflections().len(),
        1,
        "the run that had nothing to learn still has to be asked — otherwise `zero pairs means \
         zero notes` is a statement about a question nobody put"
    );

    let quiet_left = notes_left(&quiet);
    assert!(
        quiet_left.is_empty(),
        "a model that named nothing worth keeping left {} note(s) behind: {:?}. A writer that \
         produces a note per run whatever the answer is fills the section with sentences the \
         model did not stand behind",
        quiet_left.len(),
        quiet_left
            .iter()
            .map(|note| note.rule.as_str())
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_that_handed_nothing_on_is_never_asked() -> Result<(), Box<dyn Error>> {
    // KONTROLA STOI PIERWSZA i jest tu obowiązkowa: „bieg bez przekazań nie woła refleksji"
    // jest prawdziwe także o kodzie, który nie woła refleksji nigdy — czyli o tym, który jest
    // tu dzisiaj. Bez pary „woła / nie woła" ta asercja certyfikuje pustkę.
    let asked = Bench::new()?;
    asked.agent("backend", AGENT)?;
    let asked_seen = Arc::new(Seen::default());
    let asked_report = a_run_that_finished(&asked, &asked_seen, answer_with(1), true).await?;

    assert_eq!(asked_report.steps, vec![StepState::Succeeded; 1]);
    assert!(
        handoffs_in(&asked_report.dir) > 0,
        "the control run left nothing in handoffs/, so it is the same case as the run below and \
         the pair stops telling anything apart"
    );
    assert_eq!(
        asked_seen.reflections().len(),
        1,
        "a run that finished and handed its result on was not asked what it taught us. That is \
         the case this whole criterion is about"
    );

    // ── I bieg, po którym nie zostało nic ─────────────────────────────────────────────────
    //
    // Krok, który padł, nie oddaje wyniku i nie zostawia przekazania (`Live::hand_over` biegnie
    // wyłącznie po udanym kroku). Nie ma czego streścić: refleksja nad biegiem, który nic nie
    // przekazał, to tura zapłacona za przeczytanie pustego katalogu.
    let nothing = Bench::new()?;
    nothing.agent("backend", AGENT)?;
    let nothing_seen = Arc::new(Seen::default());
    let nothing_report =
        a_run_that_finished(&nothing, &nothing_seen, answer_with(3), false).await?;

    assert_eq!(
        nothing_report.steps,
        vec![StepState::Failed; 1],
        "the fixture needs this step to fail, because a failed step is the one case in which \
         Loadout writes no handoff at all. It ended as {:?}",
        nothing_report.steps
    );
    assert_eq!(
        handoffs_in(&nothing_report.dir),
        0,
        "this run left files in handoffs/ after all, so it is not the case the criterion names"
    );
    assert_eq!(
        nothing_seen.reflections().len(),
        0,
        "a run that handed nothing on was still asked what it taught us. There is nothing to \
         read: the reflection works from the run directory, and this one holds no result from \
         anybody — so the turn is paid for and answers about an empty folder"
    );
    assert!(
        notes_left(&nothing).is_empty(),
        "a run that handed nothing on left notes behind anyway: {:?}",
        notes_left(&nothing)
            .iter()
            .map(|note| note.rule.as_str())
            .collect::<Vec<_>>()
    );

    Ok(())
}

/// DRUGA POŁOWA TEGO KRYTERIUM, i to nie jest jego ozdoba.
///
/// Wszystko wyżej jest prawdą o szwie, który tura refleksji dostaje **od testu**. Szew
/// z domyślnym `None`, którego produkcja nigdy nie podaje, jest funkcją wyglądającą na gotową
/// i niebiegnącą ani razu — czyli dokładnie tym kształtem awarii, który to zadanie naprawia po
/// stronie pamięci. Ten podsystem ma już sześć takich szwów i to jest cały powód, dla którego
/// `~/.loadout/memory/` nie istniało po 23 biegach.
///
/// Ten test sądzi więc trzy rzeczy, których tamte dwa nie widzą:
///
/// 1. **Szew jest opt-in.** Sterownik, który go nie podaje — czyli każdy inny dubel w tym
///    drzewie — nie widzi tury, o którą nie prosił żaden kafelek. To jest cena zapłacona
///    świadomie: pierwsza wersja brała sterownik z fabryki i przewróciła 26 cudzych
///    specyfikacji, z których każda liczy albo enumeruje wywołania sterownika.
/// 2. **Bieg pyta o vendora, którego wybrał Loadout, a nie o tego z ostatniego kafelka.**
///    Graf niżej biegnie na Codeksie i nie nazywa Claude'a ani razu — więc pytanie o niego
///    mogło paść wyłącznie z tury refleksji. Gdyby padło o Codeksa, produkcja dostałaby `None`
///    i mechanizm byłby martwy przy zielonych asercjach wyżej.
/// 3. **Sterownik, którym Loadout jedzie naprawdę, ten szew podaje.** `ClaudeDriver` jest tym,
///    co fabryka z `lib.rs` wydaje dla [`Vendor::ClaudeCode`].
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_turn_rides_a_seam_the_shipping_driver_supplies() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", CODEX_AGENT)?;
    // Fikstura, nie asercja kryterium — ten sam powód, co przy [`AGENT_ID`] wyżej.
    assert!(
        CODEX_AGENT.contains(CODEX_AGENT_ID) && CODEX_WORKFLOW.contains(CODEX_AGENT_ID),
        "the fixture names {CODEX_AGENT_ID} in only one of the two files that have to agree on it"
    );
    assert!(
        !CODEX_WORKFLOW.contains(AGENT_ID),
        "the graph names the Claude agent after all, so asking the factory for that vendor no \
         longer tells the reflection apart from the step"
    );
    let seen = Arc::new(Seen::default());
    let report = a_run_on_codex(&bench, &seen, answer_with(3)).await?;

    assert_eq!(
        report.steps,
        vec![StepState::Succeeded; 1],
        "the step has to finish and hand something on, or the run below is never a candidate for \
         a reflection at all and every assertion here is true of nothing. It ended as {:?}",
        report.steps
    );
    assert!(
        handoffs_in(&report.dir) > 0,
        "this run left nothing in handoffs/, so it is the run that is never asked anyway and the \
         silence below says nothing about the seam"
    );

    assert_eq!(
        seen.reflections().len(),
        0,
        "a driver that does not supply the seam was asked for a reflection {} time(s). Every \
         other double in this tree is exactly this driver — it says nothing about \
         `AgentDriver::reflecting` and takes the default `None` — so a turn reaching it here is a \
         turn reaching all of them, and 26 green specs that count driver calls go red for a \
         behaviour their product never asked for",
        seen.reflections().len()
    );
    assert!(
        notes_left(&bench).is_empty(),
        "a run whose driver never took Loadout's turn left notes behind anyway: {:?}. They can \
         only have come from somewhere that is not the model's answer",
        notes_left(&bench)
            .iter()
            .map(|note| note.rule.as_str())
            .collect::<Vec<_>>()
    );

    let vendors = seen.vendors_asked_for();
    assert!(
        vendors.contains(&Vendor::ClaudeCode),
        "this run asked the factory for {vendors:?} and never for {:?}. The only step in the \
         graph runs on the other vendor, so that is the one the run asks for on its behalf — and \
         a reflection asking for the step's vendor is one that gets `None` from the driver \
         Loadout actually ships, on every run whose last tile was not Claude's",
        Vendor::ClaudeCode
    );

    // ── I sterownik, którym Loadout jedzie naprawdę ────────────────────────────────────────
    let shipping: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
    assert!(
        shipping.reflecting().is_some(),
        "the driver `lib.rs` hands out for {:?} does not take Loadout's own turn. Everything \
         above then describes a seam nothing in the product ever supplies: memory keeps its \
         reader, its budgets and its forced choice, and still has no writer — which is the state \
         this task exists to end",
        Vendor::ClaudeCode
    );

    Ok(())
}

/// Bieg, w którym jedyny kafelek jedzie **drugim** vendorem.
async fn a_run_on_codex(
    bench: &Bench,
    seen: &Arc<Seen>,
    reflection_says: String,
) -> Result<RunReport, Box<dyn Error>> {
    let workflow = bench.workflow("run-leaves-suggestions-codex", CODEX_WORKFLOW)?;
    let store = Store::open(&bench.db())?;

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        // `false`: ten dubel szwu NIE podaje, czyli wygląda jak każdy inny dubel w tym drzewie.
        drivers: fake_drivers(Arc::clone(seen), reflection_says, true, false),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 1,
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
    Ok(report)
}

/// Ile plików leży w `handoffs/` tego biegu. Brak katalogu to zero, nie błąd.
fn handoffs_in(run_dir: &std::path::Path) -> usize {
    fs::read_dir(run_dir.join("handoffs")).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count()
    })
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Tura, która nie była żadnym krokiem grafu — czyli refleksja, jeśli w ogóle padła.
#[derive(Debug, Clone)]
struct Asked {
    cwd: PathBuf,
    model: Option<String>,
    policy: Policy,
}

#[derive(Debug, Default)]
struct Seen {
    turns: Mutex<Vec<Asked>>,
    /// O którego vendora poprosił bieg fabrykę. Refleksja jest turą Loadouta, więc vendor jest
    /// jeden i wybrany — a wybrany przez kogo innego niż fabryka byłby dwoma rachunkami za jedno
    /// pytanie.
    vendors: Mutex<Vec<Vendor>>,
}

impl Seen {
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym
    /// wywołaniu, więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, asked: Asked) {
        Self::lock(&self.turns).push(asked);
    }

    fn reflections(&self) -> Vec<Asked> {
        Self::lock(&self.turns).clone()
    }

    fn record_vendor(&self, vendor: Vendor) {
        Self::lock(&self.vendors).push(vendor);
    }

    fn vendors_asked_for(&self) -> Vec<Vendor> {
        Self::lock(&self.vendors).clone()
    }

    fn lock<T>(what: &Mutex<Vec<T>>) -> MutexGuard<'_, Vec<T>> {
        what.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(
    seen: Arc<Seen>,
    reflection_says: String,
    step_succeeds: bool,
    takes_loadouts_turn: bool,
) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake {
        seen: Arc::clone(&seen),
        reflection_says,
        step_succeeds,
        takes_loadouts_turn,
    });
    Arc::new(move |vendor| {
        seen.record_vendor(vendor);
        Arc::clone(&driver)
    })
}

#[derive(Debug, Clone)]
struct Fake {
    seen: Arc<Seen>,
    /// Co model odpowiada, kiedy zapytać go, czego ten bieg nauczył.
    reflection_says: String,
    /// Czy krok grafu ma się udać. Nieudany krok nie zostawia przekazania.
    step_succeeds: bool,
    /// Czy ten dubel **podaje szew** tury Loadouta. `false` jest tu wartością, nie brakiem: tak
    /// wygląda każdy inny dubel w tym drzewie i dlatego żaden z nich tej tury nie widzi.
    takes_loadouts_turn: bool,
}

#[async_trait]
impl AgentDriver for Fake {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Szew tury Loadouta — opt-in, dokładnie jak w produkcji.
    ///
    /// Domyślna implementacja na traicie oddaje `None`, więc dubel milczący o tej metodzie nie ma
    /// jak zobaczyć tury, o którą nie prosił żaden krok grafu. To NIE jest wygoda testu: pierwsza
    /// wersja tego mechanizmu brała sterownik prosto z fabryki i przewróciła 26 cudzych
    /// specyfikacji, z których każda liczy albo enumeruje wywołania sterownika.
    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        self.takes_loadouts_turn
            .then(|| Arc::new(self.clone()) as Arc<dyn AgentDriver>)
    }

    fn with_settings(
        &self,
        _settings: &loadout_lib::engine::drivers::StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        Some(Ok(Arc::new(self.clone())))
    }

    fn with_evidence(
        &self,
        _target: loadout_lib::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        (dollars > 0.0).then(|| Arc::new(self.clone()) as Arc<dyn AgentDriver>)
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
        // Krok grafu poznajemy po znaczniku jego instrukcji; wszystko inne jest turą, o którą
        // ten krok nie prosił — czyli refleksją, jeśli ktokolwiek ją zada.
        let is_step = spec.prompt.contains(STEP_MARK);
        if !is_step {
            self.seen.record(Asked {
                cwd: spec.cwd.clone(),
                model: spec.model.clone(),
                policy: spec.policy,
            });
        }

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
            ok: !is_step || self.step_succeeds,
            says: if is_step {
                "The queue drains in one place.".to_owned()
            } else {
                self.reflection_says.clone()
            },
        }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
    ok: bool,
    says: String,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
        None
    }

    async fn send(&mut self, _text: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<TurnOutcome> {
        let outcome = TurnOutcome {
            ok: self.ok,
            reason: if self.ok {
                FinishReason::Completed
            } else {
                FinishReason::Failed("this step could not do what it was given".to_owned())
            },
            text: self.says.clone(),
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

    async fn cancel(&mut self) -> loadout_lib::engine::supervisor::GroupProof {
        loadout_lib::engine::supervisor::GroupProof::Dead { status: None }
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
        // Ten sam korzeń, który rozwiązuje `commands::memory::project_notes_root`. ISTNIEJE i jest PUSTY:
        // „zero notatek" ma znaczyć „nikt nic nie zapisał", a nie „nie ma gdzie zapisywać".
        fs::create_dir_all(project_notes_root(project.path()).join("notes"))?;
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
