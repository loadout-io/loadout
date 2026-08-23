//! AC-1 dla T-93: kafelek, który coś pożyczył z tego repozytorium, dostaje **to** — i nic
//! poza tym; kafelek, który nie pożyczył nic, jedzie co do bajtu tak, jak jechał wczoraj.
//!
//! `src-tauri/src/inherit/` umiało wszystko poza jedną rzeczą: nie miało gdzie przeczytać, co
//! człowiek zaznaczył. `what_the_host_lends` wołało `from_the_host(..., &Chosen::default())`,
//! czyli pusty wybór, zawsze. Pytanie tego pliku brzmi więc nie „czy dziedziczenie działa"
//! (to jest zmierzone w `inherit_*`), tylko: **czy wybór zapisany na kafelku dojeżdża do tego
//! kafelka, i tylko do niego.**
//!
//! # Cztery słabe wersje tego kryterium
//!
//! **Pierwsza: jeden kafelek z wyborem i asercja `contains`.** Przechodzi dla implementacji,
//! w której wybór jest własnością BIEGU — jedno pole na wszystkie kafelki — bo przy jednym
//! kafelku obie odpowiadają tak samo. Rozróżniają to cztery kafelki w jednym biegu, każdy
//! z innym wyborem, i asercje o tym, czego każdy z nich **nie** dostał.
//!
//! **Druga: sprawdzenie samego promptu.** Umiejętność jedzie katalogiem pluginu w argv, a nie
//! tekstem, więc kryterium patrzące wyłącznie na prompt przechodzi dla builda, w którym
//! `--plugin-dir` nie powstaje w ogóle. Stąd zapis fragmentu argv, który sterownik naprawdę
//! dostał, i porównanie zawartości tego katalogu.
//!
//! **Trzecia: sprawdzenie samego argv.** Symetrycznie: learnings i podagent nie mają prawa
//! pojechać argumentem (niezmiennik 9, `ps` widzi argumenty każdego użytkownika maszyny), więc
//! kafelek pożyczający tekst ma NIE dostać ani jednej flagi.
//!
//! **Czwarta, najcichsza: kontrola opt-in zrobiona jako `!prompt.contains(MARKER)`.**
//! Przechodzi dla implementacji, która doklejała pusty nagłówek albo przecinek — czyli dla
//! takiej, w której każdy zapisany wcześniej bieg zaczyna odpowiadać inaczej. Kontrola jest
//! tu dlatego **porównaniem bajtów** dwóch przebiegów tego samego kafelka: raz w repozytorium
//! z pełnym `.claude/`, raz w repozytorium, które nie ma go wcale. To jest `inherit_is_opt_in`
//! powtórzone od strony biegu, tak jak żąda kontrakt.
//!
//! JEDEN `#[test]`: zaślepka, która nie pożycza niczego, przechodzi kontrolę opt-in i połowę
//! asercji negatywnych — rozbite na osobne zestawy dałyby w warstwie `before` obraz
//! „w połowie zielony".

// `unwrap()`, `expect()` i `panic!` w teście: panika w teście JEST jego wynikiem, a `?` na tej
// samej linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `too_many_lines` z tego
// samego powodu, dla którego to jest JEDEN `#[test]`: rozbity na osobne zestawy dałby
// w warstwie `before` obraz „w połowie zielony". `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tych linii ląduje to w bramce, nie tutaj.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// Powód w całości przy tej samej stałej w `skills_reach_the_step.rs`.
const PATIENCE: Duration = Duration::from_secs(30);

const VENDOR: &str = "fake";

/// Znacznik zadania każdego z czterech kafelków. Po nim rozpoznajemy, który start był czyj —
/// i po nim widać, czy zadanie kafelka przeżyło doklejenie cudzego tekstu.
const PLAIN_MARK: &str = "PLAIN-STEP-7a30";
const READER_MARK: &str = "READER-STEP-7a31";
const TOOLED_MARK: &str = "TOOLED-STEP-7a32";
const ROLE_MARK: &str = "ROLE-STEP-7a33";

/// Znacznik z sekcji `## Recurring patterns` pliku roli.
const PATTERNS_MARK: &str = "PATTERNS-ONLY-9c14";
/// Znacznik z części pliku roli, która przekroczyć granicy nie ma prawa.
const JOURNAL_MARK: &str = "JOURNAL-ONLY-9c15";
/// Znacznik z ciała podagenta.
const SUBAGENT_MARK: &str = "SUBAGENT-ONLY-9c16";
/// Znacznik z umiejętności, którą ktoś zaznaczył.
const ALPHA_MARK: &str = "ALPHA-ONLY-9c17";
/// Znacznik z umiejętności, której nie zaznaczył nikt.
const GAMMA_MARK: &str = "GAMMA-ONLY-9c18";

/// Nazwa roli u gospodarza, bez rozszerzenia.
const ROLE: &str = "backend-dev";
/// Nazwa podagenta u gospodarza, bez rozszerzenia.
const SUBAGENT: &str = "release-engineer";

/// Trzecia linia prawdziwych plików ról u gospodarza: cytat blokowy, w którym stoi **dosłownie**
/// `` `## Recurring patterns` `` przed prawdziwym nagłówkiem [zmierzone 2026-08-19]. Bez niego
/// implementacja przepisująca cięcie zamiast wołać `scan::recurring_patterns` wypada tak samo
/// jak poprawna.
const QUOTE_ABOUT_THE_SECTION: &str = "> Auto-loaded by the orchestrator. `## Recurring patterns` is BINDING and the rest of this file is not.\n";

fn learnings_file() -> String {
    format!(
        "# Learnings — {ROLE}\n\n{QUOTE_ABOUT_THE_SECTION}\n\
         ## Recurring patterns (BINDING — do NOT repeat)\n\n\
         - {PATTERNS_MARK}: a migration that drops a column is never additive.\n\n\
         ## Run journal\n\n{JOURNAL_MARK} — 2026-08-01, three rounds, nobody reads this twice.\n"
    )
}

fn subagent_file() -> String {
    format!(
        "---\nname: {SUBAGENT}\nmodel: opus\n---\n\n\
         {SUBAGENT_MARK} — cut the notes from the merged pull requests.\n"
    )
}

fn skill_file(name: &str, mark: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Reads one file and says in a line what it is for.\n---\n\n\
         {mark} — answer with a single sentence.\n"
    )
}

/// Definicja agenta. `skills: []`, bo umiejętności BIBLIOTEKI mają w tym kryterium nie
/// uczestniczyć: ich katalog pluginu jedzie tą samą flagą i zlałby się z tym, co przyszło
/// z repozytorium.
fn agent_file(vendor: &str) -> String {
    format!(
        "---
schema: 1
id: 01990000-0000-7000-8000-0000000000d2
name: Hand
summary: Does the work
color: moss
runsWith: {vendor}
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
"
    )
}

/// Jeden kafelek agenta. `borrow` wchodzi dosłownie takim tekstem, jaki poda wołający — pusty
/// napis znaczy „tego klucza w pliku NIE MA", czyli dokładnie plik zapisany przed tym zadaniem.
fn step(id: &str, mark: &str, borrow: &str) -> String {
    format!(
        r#"    {{
      "kind": "agent",
      "id": "{id}",
      "name": "Step {id}",
      "agent": "01990000-0000-7000-8000-0000000000d2",
      "overrides": {{}},{borrow}
      "instructions": "{mark}: do the work",
      "folder": {{ "use": "fresh-copy" }},
      "at": {{ "x": 0, "y": 0 }}
    }}"#
    )
}

fn workflow_file(steps: &[String]) -> String {
    format!(
        "{{\n  \"format\": 1,\n  \"id\": \"wf_borrow\",\n  \"name\": \"Borrowing\",\n  \
         \"steps\": [\n{}\n  ],\n  \"links\": []\n}}\n",
        steps.join(",\n")
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_step_gets_what_it_borrowed_and_nothing_else() -> Result<(), Box<dyn Error>> {
    // ── Świat A: repozytorium z pełnym `.claude/` ────────────────────────────────────────
    let rich = Bench::new()?;
    rich.host_material()?;
    rich.agent(&agent_file("claude-code"))?;
    let four = rich.workflow(&workflow_file(&[
        step("s_plain", PLAIN_MARK, ""),
        step(
            "s_reader",
            READER_MARK,
            &format!("\n      \"borrow\": {{ \"learnings\": \"{ROLE}\" }},"),
        ),
        step(
            "s_tooled",
            TOOLED_MARK,
            "\n      \"borrow\": { \"skills\": [\"alpha\"] },",
        ),
        step(
            "s_role",
            ROLE_MARK,
            &format!("\n      \"borrow\": {{ \"agent\": \"{SUBAGENT}\" }},"),
        ),
    ]))?;

    let (refusal, seen) = one_run(&rich, four, 4).await?;
    assert!(
        refusal.is_none(),
        "the run turned this workflow down: {refusal:?}. Every name on every step is really in \
         this project's .claude/, so nothing here was supposed to be refused"
    );
    assert_eq!(
        seen.len(),
        4,
        "four steps had to start and {} did. Without all four there is no way to tell a choice \
         that belongs to one tile from a choice that belongs to the whole run",
        seen.len()
    );

    let plain = started_with(&seen, PLAIN_MARK);
    let reader = started_with(&seen, READER_MARK);
    let tooled = started_with(&seen, TOOLED_MARK);
    let role = started_with(&seen, ROLE_MARK);

    // ── Kafelek, który pożyczył plik roli: tekst w prompcie, ani jednej flagi ────────────
    assert!(
        reader.prompt.contains(PATTERNS_MARK),
        "the step that borrowed the {ROLE} file never saw a word of it. Its whole prompt was \
         {:?}",
        reader.prompt
    );
    assert!(
        !reader.prompt.contains(JOURNAL_MARK),
        "the whole {ROLE} file reached the prompt, not the rules section of it. On the owner's \
         own files that is 1701 useful bytes out of 32922, paid for on every try of every run"
    );
    assert!(
        reader.prompt.contains(READER_MARK),
        "the borrowed text REPLACED this step's own instructions instead of standing above \
         them. Nobody sees that from outside: the agent simply answers about something else"
    );
    assert!(
        reader.flags.is_none(),
        "the borrowed rules travelled as a command-line argument: {:?}. Text goes in on standard \
         input and nowhere else — arguments are readable by every user of this machine",
        reader.flags
    );
    assert!(
        reader
            .system_append
            .as_ref()
            .is_none_or(|text| !text.contains(PATTERNS_MARK)),
        "the borrowed rules were also pasted into the agent's system prompt, and that field \
         becomes an argument"
    );

    // ── Kafelek, który pożyczył umiejętność: katalog w argv, i tylko wybrana pozycja ─────
    let carried = tooled
        .flags
        .as_ref()
        .ok_or("the step that borrowed a skill was handed no plugin directory at all")?;
    assert_eq!(
        carried.len(),
        2,
        "a plugin directory travels as a flag AND its value; {carried:?} would swallow the next \
         argument or name nothing"
    );
    let dir = PathBuf::from(&carried[1]);
    let alpha = fs::read_to_string(dir.join("skills").join("alpha").join("SKILL.md"))
        .map_err(|error| format!("the chosen skill never reached {}: {error}", dir.display()))?;
    assert!(
        alpha.contains(ALPHA_MARK),
        "the file under the plugin directory is not the skill that was picked: {alpha:?}"
    );
    assert!(
        !anything_says(&dir, GAMMA_MARK),
        "the skill nobody picked is sitting under {}. One picked out of three honoured as \
         \"this project: yes or no\" is not a choice",
        dir.display()
    );
    assert!(
        !tooled.prompt.contains(PATTERNS_MARK) && !tooled.prompt.contains(SUBAGENT_MARK),
        "this step picked a skill and was handed another step's text as well. Two tiles with \
         different choices have to end up with different things, or the choice belongs to the \
         run and not to the tile"
    );

    // ── Kafelek, który pożyczył opis roli: ciało w prompcie, front-matter nie ────────────
    assert!(
        role.prompt.contains(SUBAGENT_MARK),
        "the step that borrowed the {SUBAGENT} description never saw it. Its whole prompt was \
         {:?}",
        role.prompt
    );
    assert!(
        !role.prompt.contains("model: opus"),
        "the front-matter of the borrowed description crossed over. That block is machinery, \
         not prose: one of its fields starts a process outside every group Loadout can prove \
         dead"
    );
    assert!(
        !role.prompt.contains(PATTERNS_MARK) && role.flags.is_none(),
        "this step borrowed a role description and was handed {:?} plus somebody else's rules",
        role.flags
    );

    // ── Kafelek bez wyboru: ani flagi, ani jednego bajtu z tego repozytorium ─────────────
    assert!(
        plain.flags.is_none(),
        "a step with no borrow of its own was handed {:?}. A full .claude/ in the folder \
         somebody opened is not consent",
        plain.flags
    );
    for mark in [PATTERNS_MARK, JOURNAL_MARK, SUBAGENT_MARK, ALPHA_MARK] {
        assert!(
            !plain.prompt.contains(mark),
            "{mark:?} reached a step that borrowed nothing. The person running Loadout never saw \
             what is in this project's .claude/, and never asked for it"
        );
    }

    // ── KONTROLA: ten sam kafelek w repozytorium bez `.claude/`, bajt w bajt ─────────────
    // Porównanie bajtów, nie `!contains`: nagłówek nad pustką, przecinek albo pusta linia
    // doklejone „na wszelki wypadek" zmieniają odpowiedź każdego biegu zapisanego wcześniej,
    // a żadna asercja o obecności znacznika tego nie widzi.
    let bare = Bench::new()?;
    bare.agent(&agent_file("claude-code"))?;
    let alone = bare.workflow(&workflow_file(&[step("s_plain", PLAIN_MARK, "")]))?;
    let (refused_bare, bare_seen) = one_run(&bare, alone, 1).await?;
    assert!(
        refused_bare.is_none(),
        "a project without a .claude/ directory at all turned the run down: {refused_bare:?}. \
         Nothing to lend is a normal state of somebody else's folder, not a failure"
    );
    let elsewhere = started_with(&bare_seen, PLAIN_MARK);
    assert_eq!(
        plain.prompt, elsewhere.prompt,
        "the same step, asked for the same work, got a different prompt in a project that has a \
         .claude/ than in one that does not — and it borrowed nothing in either. Every workflow \
         saved before today runs through this line"
    );
    assert_eq!(
        plain.system_append, elsewhere.system_append,
        "the same step got a different system prompt depending on what the folder happens to \
         hold, and it borrowed nothing"
    );
    assert_eq!(
        plain.flags, elsewhere.flags,
        "the same step got a different command line depending on what the folder happens to \
         hold, and it borrowed nothing"
    );

    Ok(())
}

/// Start, którego zadanie niesie ten znacznik. Panika z nazwą znacznika, bo brak startu jest
/// wynikiem, o którym asercja wyżej już powiedziała.
fn started_with<'a>(seen: &'a [Started], mark: &str) -> &'a Started {
    seen.iter()
        .find(|one| one.prompt.contains(mark))
        .unwrap_or_else(|| panic!("no step carrying {mark} ever started"))
}

/// Czy którykolwiek plik pod `root` niesie ten napis.
fn anything_says(root: &Path, needle: &str) -> bool {
    let Ok(listing) = fs::read_dir(root) else {
        return false;
    };
    for entry in listing.flatten() {
        let path = entry.path();
        let found = if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
            anything_says(&path, needle)
        } else {
            fs::read(&path).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
        };
        if found {
            return true;
        }
    }
    false
}

/// Jeden bieg. Oddaje zdanie odmowy (albo `None`) i wszystko, co dostały uruchomione kroki.
async fn one_run(
    bench: &Bench,
    workflow: PathBuf,
    at_once: usize,
) -> Result<(Option<String>, Vec<Started>), Box<dyn Error>> {
    let store = Store::open(&bench.db())?;
    let seen: Arc<Mutex<Vec<Started>>> = Arc::new(Mutex::new(Vec::new()));
    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: recording_drivers(Arc::clone(&seen)),
        processes: std::sync::Arc::new(loadout_lib::commands::processes::Processes::new()),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: at_once,
        task: None,
        part: None,
        handoffs_from: None,
    };

    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let outcome = tokio::time::timeout(PATIENCE, run_workflow_inner(&deps, &request, sink))
        .await
        .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    let _ = tokio::time::timeout(PATIENCE, pump).await;

    let refusal = match outcome {
        Ok(_) => None,
        Err(error) => Some(error.to_string()),
    };
    let taken = std::mem::take(&mut *seen.lock().unwrap());
    Ok((refusal, taken))
}

// ── dubler ─────────────────────────────────────────────────────────────────────────────────

/// Co jeden krok naprawdę dostał: prompt, prompt systemowy i fragment argv.
///
/// `flags: None` znaczy „`inheriting` nie zostało wołane w ogóle", czyli argv co do bajtu takie,
/// jak przed tym zadaniem. Pusty wektor znaczyłby co innego i dlatego to jest `Option`.
#[derive(Debug, Clone)]
struct Started {
    prompt: String,
    system_append: Option<String>,
    flags: Option<Vec<String>>,
}

fn recording_drivers(seen: Arc<Mutex<Vec<Started>>>) -> Drivers {
    let driver: Arc<dyn AgentDriver> = Arc::new(Recording { seen, flags: None });
    Arc::new(move |_vendor| Arc::clone(&driver))
}

#[derive(Debug)]
struct Recording {
    seen: Arc<Mutex<Vec<Started>>>,
    flags: Option<Vec<String>>,
}

#[async_trait]
impl AgentDriver for Recording {
    fn id(&self) -> &'static str {
        VENDOR
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: Some(VENDOR.to_owned()),
        })
    }

    /// Ten dubler UMIE przyjąć gotowy fragment argv i **zapamiętuje go**, bo to jest połowa
    /// przedmiotu tego kryterium.
    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            seen: Arc::clone(&self.seen),
            flags: Some(flags.to_vec()),
        }))
    }

    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Zamek wzięty i oddany w jednym wyrażeniu, bez `await` w środku (niezmiennik 8).
        {
            self.seen.lock().unwrap().push(Started {
                prompt: spec.prompt.clone(),
                system_append: spec.system_append.clone(),
                flags: self.flags.clone(),
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
        Ok(Box::new(Turn { events, session }))
    }
}

#[derive(Debug)]
struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
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
            text: String::new(),
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
        fs::create_dir_all(home.path().join("skills"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        fs::write(project.path().join("notes.txt"), "written by the human")?;
        Ok(Self { home, project })
    }

    /// Pełne `.claude/` gospodarza: trzy umiejętności, plik roli i podagent.
    ///
    /// Trzy, a nie jedna: przy jednej „wybrane jedno z trzech" nie odróżnia wyboru od
    /// przełącznika, a asercja o pozycji, której nikt nie zaznaczył, nie ma czego mierzyć.
    fn host_material(&self) -> Result<(), Box<dyn Error>> {
        let claude = self.project.path().join(".claude");
        for (name, mark) in [
            ("alpha", ALPHA_MARK),
            ("beta", "BETA-ONLY-9c19"),
            ("gamma", GAMMA_MARK),
        ] {
            let dir = claude.join("skills").join(name);
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("SKILL.md"), skill_file(name, mark))?;
        }
        fs::create_dir_all(claude.join("learnings"))?;
        fs::write(
            claude.join("learnings").join(format!("{ROLE}.md")),
            learnings_file(),
        )?;
        fs::create_dir_all(claude.join("agents"))?;
        fs::write(
            claude.join("agents").join(format!("{SUBAGENT}.md")),
            subagent_file(),
        )?;
        Ok(())
    }

    fn agent(&self, text: &str) -> Result<(), Box<dyn Error>> {
        fs::write(self.home.path().join("agents").join("hand.md"), text)?;
        Ok(())
    }

    fn workflow(&self, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.home.path().join("workflows").join("borrow.json");
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
