//! AC-1 dla T-86: **każdy** krok agenta kończy prompt tym samym blokiem — nie tylko sędzia pętli.
//!
//! Loadout ma wobec agenta trzy konkretne oczekiwania i do dziś nie mówi mu ani jednego.
//! Ostatnia wypowiedź tury JEST przekazaniem (`Live::one_turn` → `hand_over`), `reshape()`
//! dopisuje brakujące nagłówki `## Answer / ## Evidence / ## Open`, a wyników nie zapisuje się
//! do plików, bo robi to Loadout. Zmierzone w transkryptach biegu `20260823-145648`
//! (`~/Projects/urc-monorepo/.loadout/runs/`): **sześć** kroków Claude'a zaczyna podsumowanie od
//! „*Write access is disabled in this session, so I can't create the handoff file*". Agent palił
//! tury na próbę zapisania pliku wyników, bo tak każą mu instrukcje gospodarza, a dial `look-only`
//! to blokuje. Gdyby wiedział, że jego odpowiedź **jest** tym, co przekazuje dalej, nie próbowałby
//! wcale.
//!
//! # Kontrakt, który to kryterium egzekwuje
//!
//! Blok ma powiedzieć co najmniej trzy rzeczy i każda ma tu swój fragment rozpoznawczy — tekst
//! może brzmieć inaczej, ale te fragmenty muszą w nim stać dosłownie, bo inaczej kryterium nie ma
//! jak odróżnić zdania o wyniku od zdania o czymkolwiek innym:
//!
//! ```text
//! Your last message is what this step passes on. The step after yours reads it and
//! nothing else, so put in it everything the next step needs.
//!
//! Answer under these three headings, in this order:
//!
//! ## Answer
//! ## Evidence
//! ## Open
//!
//! Do not write your results to a file. Loadout files your last message for you, and
//! a file you write yourself is read by nobody.
//! ```
//!
//! # SŁABA WERSJA numer jeden, i mówi o niej wprost sam kontrakt
//!
//! `assert!(BLOCK.contains("## Answer"))` — asercja na **stałej**. Przechodzi dla biegu, w którym
//! stała istnieje, a `prompt_for` jej nie dokleja, czyli dla martwej kontrolki z niezmiennika 16.
//! Dlatego każda asercja niżej stoi na tekście, który dojechał do sterownika przez `RunSpec`.
//!
//! # SŁABA WERSJA numer dwa: sądzić jeden krok
//!
//! Krok bez poprzedników i krok z trzema idą w `prompt_for` **dwiema różnymi gałęziami** — ta
//! pierwsza wraca `return`em zaraz po `handed.is_empty()`. Implementacja dokładająca blok tylko
//! w jednej z nich zostawia połowę biegu bez ani jednego zdania i nikt tego nie zobaczy, bo prompt
//! kroku nie trafia na żaden ekran. Ławka ma więc oba rodzaje kroku naraz.
//!
//! # SŁABA WERSJA numer trzy: „prompt zawiera trzy nagłówki"
//!
//! Przechodzi dla bloku, który prosi o `## Answer`, `## Findings`, `## Next` — czyli o kształt,
//! którego nasz własny `reshape()` nie uzna i naprawi go przy każdej turze. Dlatego nagłówki
//! WYJĘTE Z PROMPTU jadą do `memory::handoff::write_handoff` i muszą wrócić z pustym `repaired`:
//! dwie połowy jednego kontraktu spotykają się w jednym miejscu i nie mają jak rozjechać się po
//! cichu. Ten sam wzorzec, którym `runcmd_loop` sądzi zdanie o wyniku.
//!
//! # Dlaczego blok NIE MOŻE nieść słowa `outcome:`
//!
//! `runcmd_loop.rs` sądzi drugą stronę tej samej reguły: krok, którego nikt nie sądzi, nie ma
//! prawa dostać prośby o wynik, bo jego odpowiedzi nie czyta nikt (niezmiennik 16). Blok wspólny
//! dla wszystkich kroków, który by o to prosił, przewraca tamto kryterium — plik spoza `OWNS`
//! tego zadania. Asercja o tym stoi tu wprost, żeby ta czerwień padła tutaj, a nie w cudzym pliku.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// `too_many_lines` **wyłącznie dodane**, nie w miejsce niczego: siedem punktów tego kryterium
// mierzy JEDEN bieg pięciu kroków, dzielących jedną ławkę, jeden magazyn i jedną migawkę tego, co
// zobaczył dubler. Cięcie po granicy funkcji znaczyłoby pięć osobnych biegów albo stan dzielony
// między testami, które cargo uruchamia równolegle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome as TurnOutcome,
    Probe, RunSpec, SessionRef, Tokens,
};
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::memory::handoff::{self, Kind, MetaDraft};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Etykieta vendora dublera. Nie „claude" i nie „codex": tamte dwie mają w biegu własne wymagania
/// co do dowodów, a to kryterium sądzi tekst promptu, nie sterownik.
const VENDOR: &str = "fake";

/// Ile czekamy na bieg, zanim uznamy go za zawieszony. Bieg, który wisi, jest dla bramki
/// „nie uruchomiło się" (rc 124), a nie czerwienią — więc limit stoi tutaj, w teście.
const PATIENCE: Duration = Duration::from_mins(1);

/// Zdanie, po którym poznajemy początek bloku. Wszystko od niego do końca promptu jest blokiem.
///
/// Fragment, nie całe zdanie: kryterium ma sądzić, że blok dojechał i gdzie stoi, a nie
/// przepisywać copy słowo w słowo.
const OPENS: &str = "Your last message";

/// Trzy rzeczy, które blok ma powiedzieć, i fragment, po którym kryterium poznaje każdą z nich.
const MUST_SAY: [(&str, &str); 3] = [
    (
        "that the agent's last message IS what this step passes on, so nothing worth keeping may \
         be left out of it",
        OPENS,
    ),
    (
        "that the answer goes under three fixed headings",
        "## Answer",
    ),
    (
        "that results do not go into files, because Loadout files the last message itself",
        "Do not write",
    ),
];

/// Nagłówki, które nasz własny zapis przekazania przyjmuje bez poprawiania (`memory::handoff`).
const THE_SHAPE_WE_ACCEPT: [&str; 3] = ["Answer", "Evidence", "Open"];

/// Wiersz, którym sędzia pętli oddaje wynik. Blok wspólny dla wszystkich kroków nie ma prawa go
/// nieść — powód w nagłówku pliku.
const THE_JUDGE_LINE: &str = "outcome:";

/// Agent bez ani jednej umiejętności i bez połączeń: to kryterium sądzi prompt, nie wyposażenie.
const HAND: &str = "---
schema: 1
id: 01990000-0000-7000-8000-0000000000e1
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

/// Instrukcja kroku → nazwa kroku. Krok rozpoznajemy po treści zadania, bo `RunSpec` nie niesie
/// nazwy kroku, a instrukcja jest tym, co ten krok naprawdę dostał. Tablica, a nie łańcuch
/// `if`-ów: przy pięciu krokach gałąź `else` cichcem przypisywałaby cudzy prompt krokowi,
/// którego nikt nie rozpoznał.
const STEPS: [(&str, &str); 5] = [
    ("alone", "Alone"),
    ("left", "Left"),
    ("right", "Right"),
    ("join", "Join"),
    ("judge", "Judge"),
];

/// Krok bez ani jednego poprzednika.
const ALONE: &str = "Alone";
/// Krok z TRZEMA poprzednikami — druga gałąź `prompt_for`, ta za `handed.is_empty()`.
const JOIN: &str = "Join";
/// Sędzia pętli: jedyny krok, który ma dostać blok **i** zdanie o wyniku, w tej kolejności.
const JUDGE: &str = "Judge";

/// Trzy kroki bez poprzedników wchodzą w jeden, a za nim stoi pętla z sędzią.
///
/// Każdy krok na WŁASNEJ KOPII plików: dwa kroki piszące po tych samych ścieżkach są odmową
/// `check_to_run` (niezmiennik 12), a nie fiksturą. Rundy JEDNEGO kroku dzielą katalog i o to
/// chodzi — sędzia przepuszcza w rundzie zerowej, więc runda pierwsza nigdy nie startuje.
const WORKFLOW: &str = r#"{
  "format": 1,
  "id": "wf_told_how_to_answer",
  "name": "Three in, one out, one judge",
  "steps": [
    {
      "kind": "agent",
      "id": "s_alone",
      "name": "Alone",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "alone: read the file nobody else opened and say what it is for.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 0, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_left",
      "name": "Left",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "left: look at the older half and say what changed.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_right",
      "name": "Right",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "right: look at the newer half and say what changed.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 480, "y": 0 }
    },
    {
      "kind": "agent",
      "id": "s_join",
      "name": "Join",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "join: put the three answers together into one.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 160 }
    },
    {
      "kind": "agent",
      "id": "s_judge",
      "name": "Judge",
      "agent": "01990000-0000-7000-8000-0000000000e1",
      "overrides": {},
      "instructions": "judge: say whether the work is good enough to build on.",
      "folder": { "use": "fresh-copy" },
      "at": { "x": 240, "y": 320 }
    }
  ],
  "links": [
    { "from": "s_alone", "to": "s_join" },
    { "from": "s_left", "to": "s_join" },
    { "from": "s_right", "to": "s_join" },
    { "from": "s_join", "to": "s_judge" },
    { "from": "s_judge", "to": "s_join", "max_turns": 2 }
  ]
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_agent_step_ends_its_prompt_with_the_same_block() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.agent("hand", HAND)?;
    let workflow = bench.workflow("told-how-to-answer", WORKFLOW)?;
    let store = Store::open(&bench.db())?;
    let seen = Arc::new(Seen::default());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: fake_drivers(Arc::clone(&seen)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 3,
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

    let looked = seen.snapshot();
    let mut named = STEPS
        .iter()
        .map(|(_, name)| (*name).to_owned())
        .collect::<Vec<_>>();
    named.sort();
    assert_eq!(
        looked.keys().cloned().collect::<Vec<_>>(),
        named,
        "every step has to reach the driver under its own name, or the assertions below are true \
         of steps that never ran. The run ended as {:?} and the driver saw: {:?}",
        report.steps,
        looked.keys().collect::<Vec<_>>()
    );

    // ── (a) KAŻDY KROK, NIE TYLKO SĘDZIA ──────────────────────────────────────────────────────
    // Pierwsza asercja, bo bez niej wszystkie następne mierzą krok, któremu akurat się poszczęściło.
    for (instruction, name) in STEPS {
        let prompt = looked.get(name).cloned().unwrap_or_default();
        for (what, fragment) in MUST_SAY {
            assert!(
                prompt.contains(fragment),
                "the step \"{name}\" was never told {what}. Six steps of one owner's run burned \
                 their turns trying to save a file of results that Loadout was going to write \
                 from their answer anyway; an agent that is told none of this cannot do better \
                 than guess. Its prompt was: {prompt:?}"
            );
        }
        // …a ZADANIE KROKU PRZEŻYŁO. Implementacja, która prompt zastępuje blokiem, przechodzi
        // każdą asercję wyżej i oddaje agentowi umowę zamiast roboty.
        assert!(
            prompt.starts_with(instruction),
            "the step \"{name}\" lost its own task: the prompt no longer starts with what the \
             workflow file asked of it. It was {prompt:?}"
        );
    }

    // ── (b) O KSZTAŁT, KTÓRY NASZ WŁASNY ZAPIS PRZYJMUJE ─────────────────────────────────────
    // Nagłówki wyjęte Z PROMPTU, nie wpisane tutaj z pamięci: prośba o `## Findings` przeszłaby
    // „prompt zawiera trzy nagłówki" i kazała `reshape()` naprawiać każdą turę do końca świata.
    let alone = looked.get(ALONE).cloned().unwrap_or_default();
    let asked_for = headings_asked_for(&alone);
    assert_eq!(
        asked_for, THE_SHAPE_WE_ACCEPT,
        "the block asks the agent for {asked_for:?}. Our own writer accepts \
         {THE_SHAPE_WE_ACCEPT:?} in that order and repairs everything else, so any other list is \
         an agreement one side of this product never signed"
    );

    // I to samo pytanie zadane naszemu zapisowi, nie naszej pamięci o nim.
    let scratch = TempDir::new()?;
    let mut body = String::new();
    for name in &asked_for {
        // `write!` do `String`, nie `map(format!).collect()`: ten drugi alokuje bufor pośredni na
        // każdą sekcję (clippy `format_collect`), a zapis do `String` nie ma jak zawieść — błąd
        // może zwrócić wyłącznie sam formatter.
        let _ = write!(body, "## {name}\nwhat this step found.\n\n");
    }
    let written = handoff::write_handoff(scratch.path(), draft(), &body)?;
    assert!(
        written.repaired.is_empty() && !written.truncated,
        "an answer written EXACTLY as the block asks still had to be repaired: Loadout added \
         {:?}. Then the agreement is broken on our side, and every honest agent pays for it in \
         every turn",
        written.repaired
    );

    // ── (c) JEDNA STAŁA, NIE DWA WARIANTY ────────────────────────────────────────────────────
    // Krok bez poprzedników wraca z `prompt_for` inną gałęzią niż krok z trzema. Porównanie CO DO
    // BAJTU jest jedyną asercją, która widzi dwa osobne teksty mówiące mniej więcej to samo —
    // a przy dwóch kopiach zawsze poprawia się tę, której akurat nikt nie czyta.
    let join = looked.get(JOIN).cloned().unwrap_or_default();
    let block_alone = block_of(&alone).ok_or("the step with no steps before it got no block")?;
    let block_join = block_of(&join).ok_or("the step with three steps before it got no block")?;
    assert_eq!(
        block_alone, block_join,
        "the step with nothing before it and the step with three before it end their prompts \
         with two different texts. It is one agreement, so it is one constant"
    );

    // ── (d) I STOI ZA INDEKSEM PRZEKAZAŃ, NIE PRZED NIM ─────────────────────────────────────
    // Indeks jest listą ścieżek do plików poprzedników; blok jest umową o tym, jak odpowiedzieć.
    // Umowa wciśnięta przed listę materiałów czyta się jak opis pierwszego z nich.
    let names_them_all = STEPS[..3]
        .iter()
        .all(|(_, name)| join.contains(name) && join.contains("handoffs/"));
    assert!(
        names_them_all,
        "the fixture is wrong if the joining step's prompt does not list all three steps before \
         it: {join:?}"
    );
    let index_at = join.find("handoffs/").unwrap_or(usize::MAX);
    let block_at = join.len() - block_join.len();
    assert!(
        index_at < block_at,
        "the block stands BEFORE the list of what the steps before this one left. The list is \
         material, the block is the agreement about the answer, and an agreement read first \
         looks like a caption for the first file on the list"
    );

    // ── (e) SĘDZIA DOSTAJE OBA, I W TEJ KOLEJNOŚCI ──────────────────────────────────────────
    let judge = looked.get(JUDGE).cloned().unwrap_or_default();
    let block_judge = block_of(&judge).ok_or("the judging step got no block at all")?;
    assert!(
        block_judge.starts_with(block_alone),
        "the judging step does not carry the same block as every other step, byte for byte, \
         before its own sentence about the outcome. It ended with: {block_judge:?}"
    );
    assert!(
        block_judge.contains(THE_JUDGE_LINE),
        "the judging step lost its own sentence about how to say how it went. That sentence is \
         the only channel a loop has, and without it every loop burns all of its turns and ends \
         as failed. It ended with: {block_judge:?}"
    );

    // ── (f) A ZWYKŁY KROK NIE DOSTAJE GO ANI BAJTU ──────────────────────────────────────────
    // Prośba o wynik skierowana do kroku, którego wyniku nikt nie czyta, jest poleceniem bez
    // skutku (niezmiennik 16) — i przewraca `runcmd_loop`, plik spoza OWNS tego zadania.
    assert!(
        !block_alone.contains(THE_JUDGE_LINE),
        "the block every step gets carries the sentence only the judging step may get. Its \
         answer is read by nobody, so the line means nothing there — and it teaches the model to \
         write it everywhere. The block was: {block_alone:?}"
    );

    Ok(())
}

/// Wszystko od pierwszego zdania bloku do końca promptu.
fn block_of(prompt: &str) -> Option<&str> {
    prompt.find(OPENS).map(|at| &prompt[at..])
}

/// Nazwy sekcji, o które prosi ten prompt — w kolejności, w jakiej o nie prosi.
///
/// Pierwsze słowo za `## `, nie cały wiersz: blok wolno napisać jako `## Answer — what the next
/// step needs`, a to jest dalej prośba o sekcję `Answer`. Kolejność zostaje kolejnością wystąpień,
/// bo ona JEST częścią umowy (`memory::handoff::reshape` naprawia także zamienione miejscami).
fn headings_asked_for(prompt: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (at, _) in prompt.match_indices("## ") {
        let rest = &prompt[at + 3..];
        let end = rest
            .find(|glyph: char| glyph.is_whitespace())
            .unwrap_or(rest.len());
        let name = rest[..end].trim_matches(|glyph: char| !glyph.is_alphanumeric());
        if !name.is_empty() {
            names.push(name.to_owned());
        }
    }
    names
}

/// Front-matter dla jednej próbnej odpowiedzi. Wszystkie pola są tu nasze — agent daje sam tekst.
fn draft() -> MetaDraft {
    MetaDraft {
        run: "01990000-0000-7000-8000-0000000000ff".to_owned(),
        step: 0,
        from: ALONE.to_owned(),
        to: vec![JOIN.to_owned()],
        kind: Kind::Findings,
        title: "an answer written exactly the way the block asks for it".to_owned(),
        reads: Vec::new(),
    }
}

// ── co dubler zobaczył ─────────────────────────────────────────────────────────────────────

/// Prompt, który dojechał do sterownika, po nazwie kroku.
#[derive(Debug, Default)]
struct Seen(Mutex<BTreeMap<String, String>>);

impl Seen {
    /// PIERWSZY prompt kroku wygrywa: rundy pętli dzielą nazwę, a to kryterium sądzi rundę, która
    /// naprawdę ruszyła, nie ostatnią, która się nadpisała.
    ///
    /// **Synchroniczne z rozmysłem** (niezmiennik 8): guard powstaje i ginie w jednym wywołaniu,
    /// więc nie ma wyrażenia, w którym dożyłby do `await`.
    fn record(&self, step: &str, prompt: String) {
        self.lock().entry(step.to_owned()).or_insert(prompt);
    }

    fn snapshot(&self) -> BTreeMap<String, String> {
        self.lock().clone()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

fn fake_drivers(seen: Arc<Seen>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Fake { seen });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

/// Dubler, który zatrzymuje `RunSpec` i oddaje odpowiedź pasującą do kroku.
#[derive(Debug)]
struct Fake {
    seen: Arc<Seen>,
}

/// Nazwa kroku po treści jego zadania. Zadanie, którego nie ma w tablicy, ląduje pod SWOJĄ
/// treścią, nie pod cudzą nazwą: asercja o nazwach kroków ma wtedy paść i pokazać, czego test
/// nie rozpoznał.
fn step_named(prompt: &str) -> String {
    STEPS
        .iter()
        .find(|(instruction, _)| prompt.starts_with(instruction))
        .map_or_else(|| prompt.to_owned(), |(_, name)| (*name).to_owned())
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
        let step = step_named(&spec.prompt);
        self.seen.record(&step, spec.prompt.clone());

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
            said: answer_from(&step),
        }))
    }
}

/// Co ten krok odpowiada. Sędzia przepuszcza w rundzie zerowej, żeby pętla nie odbijała pracy,
/// której to kryterium nie mierzy.
fn answer_from(step: &str) -> String {
    if step == JUDGE {
        return "## Answer\nThe work is good enough to build on.\n\noutcome: pass\n".to_owned();
    }
    format!("## Answer\n{step} did the work.\n\n## Evidence\nnotes.txt:1\n\n## Open\nnothing.\n")
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

    fn group(&self) -> Option<loadout_lib::engine::supervisor::GroupId> {
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
