//! Budowanie opracowania przez wybraną aplikację agenta.
//!
//! Każda partia uruchamia świeżą rozmowę, ale wszystkie biorą to samo miejsce ze wspólnej puli.
//! Uchwyt programu żyje aż do dowodu śmierci grupy; brak dowodu nie jest zamieniany w anulowanie.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::Drivers;
use super::agent_apps::AgentAppState;
use crate::context::build::{self, Batch, Frozen};
use crate::context::files;
use crate::context::findings::{self, Findings, NotFindings};
use crate::context::limits::{CORRECTIONS, Refusal};
use crate::context::prompt;
use crate::context::{
    BuildEnd, BuildStage, ContextBuild, ContextBuildRead, Error, RevisionEdit, SourceOutcome,
};
use crate::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Policy, RunSpec, StepSettings,
};
use crate::engine::limits::{Limiter, Slot, Weight};
use crate::engine::supervisor::{FilesystemFence, GroupProof, StepTag};
use crate::library::agents::Vendor;

#[derive(Clone, Debug)]
pub struct BuildContextRequest {
    pub set_id: String,
    pub operation_id: String,
    pub app: Option<Vendor>,
    pub model: Option<String>,
    pub generation: u64,
    pub deadline: Duration,
    pub budget_usd: Option<f64>,
}

pub async fn build_context_inner(
    home: &Path,
    project: &Path,
    drivers: &Drivers,
    slots: &Limiter,
    request: &BuildContextRequest,
    cancel: &CancellationToken,
) -> Result<ContextBuildRead, Error> {
    let started = tokio::time::Instant::now();
    let library = files::library_root(home);
    let app = match choose_vendor(home, &library, drivers, request).await {
        Ok(Some(app)) => app,
        Ok(None) => {
            return failed_before_start(&library, request, Refusal::NoAgentApp.to_string());
        }
        Err(said) => return failed_before_start(&library, request, said),
    };
    let app_name = name_of(app);
    let frozen = match build::freeze_and_split(
        &library,
        &request.set_id,
        app_name,
        normalized_model(request.model.as_deref()),
        request.budget_usd,
    ) {
        Ok(frozen) => frozen,
        Err(failure) => {
            let draft_revision = files::read_set(&library, &request.set_id)
                .map(|read| read.set.draft_revision)
                .unwrap_or_default();
            let mut state = build::initial_state(
                request,
                app_name,
                draft_revision,
                String::new(),
                failure.sources,
                &super::now_utc(),
            );
            state.stage = failure.stage;
            return finish_failure(&library, &mut state, failure.said);
        }
    };
    let mut state = build::initial_state(
        request,
        app_name,
        frozen.set.draft_revision,
        frozen.input_fingerprint.clone(),
        frozen.sources.clone(),
        &super::now_utc(),
    );
    state.stage = BuildStage::Splitting;
    state.batches_total = frozen.batches.len();
    state.said = format!(
        "The material is split into {} batches.",
        state.batches_total
    );
    build::write_state(&library, &state)?;

    let driver = drivers(app);
    let run = BuildRun {
        library: &library,
        home,
        project,
        driver: &driver,
        slots,
        request,
        frozen: &frozen,
        app,
        started,
        cancel,
    };
    let accepted = match read_batches(&run, &mut state).await? {
        BatchesEnd::Ready(accepted) => accepted,
        BatchesEnd::Ended(read) => return Ok(*read),
    };
    publish_build(&library, &frozen, &mut state, accepted, app_name)
}

struct BuildRun<'a> {
    library: &'a Path,
    home: &'a Path,
    project: &'a Path,
    driver: &'a Arc<dyn AgentDriver>,
    slots: &'a Limiter,
    request: &'a BuildContextRequest,
    frozen: &'a Frozen,
    app: Vendor,
    started: tokio::time::Instant,
    cancel: &'a CancellationToken,
}

enum BatchesEnd {
    Ready(Vec<Findings>),
    Ended(Box<ContextBuildRead>),
}

async fn read_batches(run: &BuildRun<'_>, state: &mut ContextBuild) -> Result<BatchesEnd, Error> {
    let mut accepted = Vec::new();
    let mut spent = 0.0_f64;
    for batch in &run.frozen.batches {
        if let Some(cached) = build::cached_batch(run.frozen, &run.request.operation_id, batch) {
            accepted.push(cached);
            state.batches_done = state.batches_done.saturating_add(1);
            mark_batch(
                state,
                batch,
                SourceOutcome::Processed,
                "Read from the saved batch.",
            );
            build::write_state(run.library, state)?;
            continue;
        }
        state.stage = BuildStage::Extracting;
        state.said = format!(
            "{} is reading batch {} of {}.",
            shown_name(run.app),
            state.batches_done + 1,
            state.batches_total
        );
        build::write_state(run.library, state)?;
        let turn = BatchTurn {
            home: run.home,
            project: run.project,
            driver: run.driver,
            slots: run.slots,
            request: run.request,
            frozen: run.frozen,
            batch,
        };
        match extract_batch(&turn, run.started, run.cancel, &mut spent).await {
            BatchEnd::Ready { findings, model } => {
                if state.model.is_none() && !model.trim().is_empty() {
                    state.model = Some(model);
                }
                build::save_batch(run.frozen, &run.request.operation_id, batch, &findings)?;
                accepted.push(findings);
                state.batches_done = state.batches_done.saturating_add(1);
                mark_batch(state, batch, SourceOutcome::Processed, "Processed.");
                build::write_state(run.library, state)?;
            }
            BatchEnd::Cancelled { proof } => {
                return cancel_batch(run.library, state, batch, &proof)
                    .map(Box::new)
                    .map(BatchesEnd::Ended);
            }
            BatchEnd::OutOfTime { proof } => {
                return timeout_batch(run.library, state, batch, &proof)
                    .map(Box::new)
                    .map(BatchesEnd::Ended);
            }
            BatchEnd::StillRunning(said) => {
                mark_batch(state, batch, SourceOutcome::Failed, &said);
                settle_unread_sources(state);
                state.end = BuildEnd::StillRunning;
                state.said = said;
                state.changed_at = super::now_utc();
                build::write_state(run.library, state)?;
                return view(run.library, state.clone())
                    .map(Box::new)
                    .map(BatchesEnd::Ended);
            }
            BatchEnd::Failed(said) => {
                mark_batch(state, batch, SourceOutcome::Failed, &said);
                return finish_failure(run.library, state, said)
                    .map(Box::new)
                    .map(BatchesEnd::Ended);
            }
        }
    }
    Ok(BatchesEnd::Ready(accepted))
}

fn cancel_batch(
    library: &Path,
    state: &mut ContextBuild,
    batch: &Batch,
    proof: &GroupProof,
) -> Result<ContextBuildRead, Error> {
    mark_batch(
        state,
        batch,
        SourceOutcome::Failed,
        "You stopped this batch.",
    );
    settle_unread_sources(state);
    match proof {
        GroupProof::Dead { .. } => {
            state.end = BuildEnd::Cancelled;
            "You stopped this build, and Loadout made sure the agent stopped. The earlier ready version is unchanged."
                .clone_into(&mut state.said);
        }
        GroupProof::Alive { .. } => {
            state.end = BuildEnd::StillRunning;
            "Loadout asked the agent to stop but could not make sure it stopped, so it may still be running. This build remains in use."
                .clone_into(&mut state.said);
        }
    }
    state.changed_at = super::now_utc();
    build::write_state(library, state)?;
    view(library, state.clone())
}

fn timeout_batch(
    library: &Path,
    state: &mut ContextBuild,
    batch: &Batch,
    proof: &GroupProof,
) -> Result<ContextBuildRead, Error> {
    mark_batch(
        state,
        batch,
        SourceOutcome::Failed,
        "This batch ran out of time.",
    );
    settle_unread_sources(state);
    match proof {
        GroupProof::Dead { .. } => {
            state.end = BuildEnd::Failed;
            // 2026-09-08 — DWIE rzeczy naraz. Po pierwsze, obie gałęzie mają nazywać tę samą
            // rzecz tym samym słowem: człowiek czyta „time limit" w gałęzi `Alive`, więc
            // w `Dead` nie ma prawa stać inne pojęcie. Po drugie, stało tu „20 minutes"
            // WPISANE NA SZTYWNO, czyli limit żył w dwóch miejscach naraz (niezmiennik 13) —
            // pierwsza zmiana wartości w `limits` zostawiłaby zdanie, które kłamie.
            "Building reached its time limit, so Loadout stopped the agent and made sure it ended. Try Rebuild context; the earlier ready version is unchanged."
                .clone_into(&mut state.said);
        }
        GroupProof::Alive { .. } => {
            state.end = BuildEnd::StillRunning;
            "Building reached its time limit, but Loadout could not make sure the agent stopped, so it may still be running. This build remains in use."
                .clone_into(&mut state.said);
        }
    }
    state.changed_at = super::now_utc();
    build::write_state(library, state)?;
    view(library, state.clone())
}

fn publish_build(
    library: &Path,
    frozen: &Frozen,
    state: &mut ContextBuild,
    accepted: Vec<Findings>,
    app_name: &str,
) -> Result<ContextBuildRead, Error> {
    state.stage = BuildStage::Grouping;
    "Loadout is grouping the accepted findings and keeping conflicts separate."
        .clone_into(&mut state.said);
    build::write_state(library, state)?;
    let merged = findings::merge(accepted);
    state.stage = BuildStage::Publishing;
    "Loadout is checking every referenced file before publishing the version."
        .clone_into(&mut state.said);
    build::write_state(library, state)?;
    match build::publish_revision(library, frozen, &merged, state, &super::now_utc()) {
        Ok(revision) => {
            state.stage = BuildStage::Ready;
            state.end = BuildEnd::Ready;
            state.revision_id = Some(revision.id.clone());
            "This context is ready.".clone_into(&mut state.said);
            state.changed_at = super::now_utc();
            build::write_state(library, state)?;
            Ok(ContextBuildRead {
                build: Some(state.clone()),
                revision: Some(revision),
                build_with: app_name.to_owned(),
            })
        }
        Err(error) => {
            let said = error.to_string();
            for source in &mut state.sources {
                if source.outcome != SourceOutcome::Excluded {
                    source.outcome = SourceOutcome::Failed;
                    source.said.clone_from(&said);
                }
            }
            finish_failure(library, state, said)
        }
    }
}

/// Odczyt po powrocie do sekcji. Żywa generacja zachowuje postęp, cudza staje się przerwana.
pub async fn read_context_build_inner(
    home: &Path,
    drivers: &Drivers,
    set_id: &str,
    live: Option<(&str, u64)>,
) -> Result<ContextBuildRead, Error> {
    let library = files::library_root(home);
    let mut state = build::read_build(&library, set_id, None)?;
    if let Some(saved) = state.take() {
        let is_live = live.is_some_and(|(operation, generation)| {
            operation == saved.operation_id && generation == saved.generation
        });
        state = Some(if is_live {
            saved
        } else {
            build::interrupt_unowned(&library, saved, &super::now_utc())?
        });
    }
    let running_app = state.as_ref().and_then(|build| {
        matches!(build.end, BuildEnd::Running | BuildEnd::StillRunning).then(|| build.app.as_str())
    });
    let build_with = if let Some(app) = running_app {
        // 2026-09-08 (CT-04) — trwającego budowania nie przełączamy po cichu nawet wtedy,
        // kiedy kolejna sonda zmieniła odpowiedź; ten wybór jest już zamrożonym wejściem.
        app.to_owned()
    } else {
        let request = BuildContextRequest {
            set_id: set_id.to_owned(),
            operation_id: String::new(),
            app: state.as_ref().and_then(|build| vendor_of(&build.app)),
            model: None,
            generation: 0,
            deadline: Duration::from_mins(crate::context::limits::BUILD_MINUTES),
            budget_usd: None,
        };
        let selected = choose_vendor(home, &library, drivers, &request)
            .await
            .ok()
            .flatten()
            .or(request.app)
            .unwrap_or(Vendor::ClaudeCode);
        name_of(selected).to_owned()
    };
    Ok(ContextBuildRead {
        build: state,
        revision: build::read_latest_revision(&library, set_id)?,
        build_with,
    })
}

/// Czeka, aż droga budowania zapisze dowód końca po anulowaniu.
pub async fn stop_context_build_inner(
    home: &Path,
    set_id: &str,
    operation_id: &str,
    cancel: &CancellationToken,
) -> Result<ContextBuild, Error> {
    cancel.cancel();
    let library = files::library_root(home);
    let until = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(state) = build::read_build(&library, set_id, Some(operation_id))?
            && state.end != BuildEnd::Running
        {
            return Ok(state);
        }
        if tokio::time::Instant::now() >= until {
            let mut state = build::read_build(&library, set_id, Some(operation_id))?
                .ok_or(Error::NoSuchBuild)?;
            state.end = BuildEnd::StillRunning;
            "Loadout asked the agent to stop but could not make sure it stopped, so it may still be running. This build remains in use."
                .clone_into(&mut state.said);
            state.changed_at = super::now_utc();
            build::write_state(&library, &state)?;
            return Ok(state);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Korekta człowieka jest następną wersją, nigdy zapisem w miejscu starej.
pub fn save_context_revision_inner(
    home: &Path,
    set_id: &str,
    edit: &RevisionEdit,
) -> Result<ContextBuildRead, Error> {
    let library = files::library_root(home);
    let revision = build::save_human_revision(&library, set_id, edit, &super::now_utc())?;
    Ok(ContextBuildRead {
        build: build::read_build(&library, set_id, None)?,
        build_with: revision.app.clone(),
        revision: Some(revision),
    })
}

enum BatchEnd {
    Ready { findings: Findings, model: String },
    Cancelled { proof: GroupProof },
    OutOfTime { proof: GroupProof },
    StillRunning(String),
    Failed(String),
}

struct TurnAnswer {
    text: String,
    model: String,
    cost_usd: Option<f64>,
}

struct BatchTurn<'a> {
    home: &'a Path,
    project: &'a Path,
    driver: &'a Arc<dyn AgentDriver>,
    slots: &'a Limiter,
    request: &'a BuildContextRequest,
    frozen: &'a Frozen,
    batch: &'a Batch,
}

async fn extract_batch(
    turn: &BatchTurn<'_>,
    started: tokio::time::Instant,
    cancel: &CancellationToken,
    spent: &mut f64,
) -> BatchEnd {
    let first = prompt::asked_for(
        &turn.frozen.set,
        &turn.frozen.draft,
        &turn.batch.material,
        &turn.batch.references,
        &turn.frozen.prior_human,
    );
    let mut asked = first.clone();
    for correction in 0..=CORRECTIONS {
        if turn
            .request
            .budget_usd
            .is_some_and(|budget| *spent >= budget)
        {
            return BatchEnd::Failed(
                "This build reached its spending limit. Change that limit in Settings before rebuilding."
                    .to_owned(),
            );
        }
        let left = turn.request.deadline.saturating_sub(started.elapsed());
        let budget_left = turn
            .request
            .budget_usd
            .map(|budget| (budget - *spent).max(0.0));
        if left.is_zero() {
            return BatchEnd::OutOfTime {
                proof: GroupProof::Dead { status: None },
            };
        }
        match one_turn(turn, &asked, left, cancel, correction, budget_left).await {
            TurnEnd::Ready(answer) => {
                if let Some(cost) = answer.cost_usd {
                    *spent += cost;
                    if turn
                        .request
                        .budget_usd
                        .is_some_and(|budget| *spent > budget)
                    {
                        return BatchEnd::Failed(
                            "This build reached its spending limit. Change that limit in Settings before rebuilding."
                                .to_owned(),
                        );
                    }
                }
                match findings::read_findings(answer.text.as_bytes(), &turn.batch.references) {
                    Ok(findings) => {
                        return BatchEnd::Ready {
                            findings,
                            model: answer.model,
                        };
                    }
                    Err(why) if correction < CORRECTIONS => {
                        asked = prompt::corrected(&first, &why.said());
                    }
                    Err(why) => return BatchEnd::Failed(not_findings(&why)),
                }
            }
            TurnEnd::Cancelled(proof) => return BatchEnd::Cancelled { proof },
            TurnEnd::OutOfTime(proof) => return BatchEnd::OutOfTime { proof },
            TurnEnd::StillRunning(said) => return BatchEnd::StillRunning(said),
            TurnEnd::Failed(said) => return BatchEnd::Failed(said),
        }
    }
    BatchEnd::Failed("The answer was not usable after its one format correction.".to_owned())
}

enum TurnEnd {
    Ready(TurnAnswer),
    Cancelled(GroupProof),
    OutOfTime(GroupProof),
    StillRunning(String),
    Failed(String),
}

async fn one_turn(
    turn: &BatchTurn<'_>,
    asked: &str,
    left: Duration,
    cancel: &CancellationToken,
    correction: usize,
    budget_left: Option<f64>,
) -> TurnEnd {
    let PreparedTurn {
        driver,
        slot,
        scratch,
    } = match prepare_turn(turn, left, cancel, correction, budget_left).await {
        Ok(prepared) => prepared,
        Err(ended) => return ended,
    };
    let (events, mut inbox) = mpsc::channel::<DecodedEvent>(64);
    let (spending, over_budget) = mpsc::channel::<f64>(1);
    let seen_model = Arc::new(Mutex::new(String::new()));
    let model_for_drain = Arc::clone(&seen_model);
    let drain = tokio::spawn(async move {
        while let Some(DecodedEvent { event, .. }) = inbox.recv().await {
            match event {
                AgentEvent::Started { model, .. } => {
                    *model_for_drain
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = model;
                }
                AgentEvent::Spending { estimate_usd }
                    if budget_left.is_some_and(|allowed| estimate_usd >= allowed) =>
                {
                    // 2026-09-08 (CT-04) — Codex nie ma flagi sufitu. Jego istniejący
                    // pomiar w trakcie tury musi więc dojść do właściciela uchwytu, bo tylko
                    // ten może zakończyć grupę i zaczekać na dowód jej śmierci.
                    let _ = spending.try_send(estimate_usd);
                }
                _ => {}
            }
        }
    });
    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        cwd: scratch,
        prompt: asked.to_owned(),
        model: normalized_model(turn.request.model.as_deref()).map(str::to_owned),
        system_append: None,
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };
    let handle = match driver
        .start_conversation(spec, turn.batch.images.clone(), events)
        .await
    {
        Ok(handle) => handle,
        Err(error) => {
            drain.abort();
            return TurnEnd::Failed(format!(
                "{} did not start this batch: {error}. Check that it is installed and signed in, then try Rebuild context.",
                shown_driver(turn.driver.as_ref())
            ));
        }
    };
    finish_started_turn(handle, slot, drain, over_budget, seen_model, cancel, left).await
}

struct PreparedTurn {
    driver: Arc<dyn AgentDriver>,
    slot: Slot,
    scratch: std::path::PathBuf,
}

async fn prepare_turn(
    turn: &BatchTurn<'_>,
    left: Duration,
    cancel: &CancellationToken,
    correction: usize,
    budget_left: Option<f64>,
) -> Result<PreparedTurn, TurnEnd> {
    if cancel.is_cancelled() {
        return Err(TurnEnd::Cancelled(GroupProof::Dead { status: None }));
    }
    let slot = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(TurnEnd::Cancelled(GroupProof::Dead { status: None })),
        () = tokio::time::sleep(left) => return Err(TurnEnd::OutOfTime(GroupProof::Dead { status: None })),
        slot = turn.slots.place(Weight::Ordinary) => slot,
    };
    let scratch = turn
        .frozen
        .folder
        .join("builds")
        .join(&turn.request.operation_id)
        .join("private")
        .join(format!("{}-{}", turn.batch.id, correction));
    let memory = scratch.join("memory");
    if fs::create_dir_all(&memory).is_err() {
        return Err(TurnEnd::Failed(
            "Loadout could not make the private folder for this batch. Try Rebuild context."
                .to_owned(),
        ));
    }
    let settings = StepSettings {
        dir: scratch.clone(),
        work_key: turn.request.operation_id.clone(),
        memory,
        deny: crate::engine::drivers::host::deny_rules(&turn.frozen.folder),
    };
    let limited = budget_left
        .and_then(|budget| turn.driver.with_budget(budget))
        .unwrap_or_else(|| Arc::clone(turn.driver));
    let prepared = match limited.prepare_protected_step(&settings) {
        Some(Ok(prepared)) => prepared,
        Some(Err(error)) => {
            return Err(TurnEnd::Failed(format!(
                "{} could not prepare private access for this batch: {error}. Try Rebuild context.",
                shown_driver(turn.driver.as_ref())
            )));
        }
        None => {
            return Err(TurnEnd::Failed(format!(
                "{} cannot protect this batch's files, so no agent was started.",
                shown_driver(turn.driver.as_ref())
            )));
        }
    };
    let mut readable_files = prepared.readable_files;
    readable_files.extend(turn.batch.readable_files.clone());
    /* 2026-09-09 — KATALOG ROBOCZY MUSI BYĆ CZYTELNY DLA PROCESU, KTÓRY W NIM STOI.
     *
     * Sterowniki rezerwują wyłącznie swój prywatny PODKATALOG (`<scratch>/claude`,
     * `CODEX_HOME`) i dwa pojedyncze pliki, a `RunSpec.cwd` wskazuje sam `scratch` — który leży
     * wewnątrz ukrytego `home` i nie miał ani jednego wyjątku. Powłoka startująca w takim
     * miejscu nie umie przejść w górę i umiera na:
     *
     *     shell-init: error retrieving current directory: getcwd: cannot access parent
     *     directories: Operation not permitted
     *
     * Właściciel zobaczył to przy każdym źródle, a `Try building again` nie miało jak pomóc.
     * Dotyczyło OBU vendorów, więc przełączenie na drugiego niczego nie zmieniało.
     *
     * Przeżyło, bo żaden test nie uruchamiał prawdziwej powłoki w tym katalogu: atrapy
     * sterowników nie wołają `getcwd`. Świadek stoi teraz w
     * `a_batch_runs_where_its_working_directory_is_readable`.
     *
     * Czytelny, nie zapisywalny: pisać wolno dalej tylko w prywatnym podkatalogu sterownika.
     * `scratch` niesie wyłącznie pliki TEJ partii, więc to nie jest poszerzenie zakresu. */
    let mut readable_roots = prepared.readable_roots;
    readable_roots.push(scratch.clone());
    /* 2026-09-09 (CT-09, znalezisko z natywnego QA) — BRAKUJĄCY KATALOG DOSTAJE ZDANIE, NIE
     * `errno`. Właściciel zobaczył „Loadout could not protect this batch's files: No such file
     * or directory (os error 2). No agent was started." przy każdym z trzech źródeł i nie miał
     * z tego jak wywnioskować, czego brakuje. `os error 2` jest żargonem (niezmiennik 14),
     * a `FilesystemFence::new` nie niesie ścieżki, na której się przewrócił.
     *
     * Sprawdzamy TU, bo tutaj znamy nazwy. Granica dalej ODMAWIA — nie dorabiamy katalogu
     * w tle, bo ochrona miejsca, którego nie ma, to nie to samo, co ochrona miejsca, które
     * powstanie w trakcie biegu. Przyczynę usuwa `lib.rs::project_dir_ready`, które zakłada
     * katalog projektu na starcie; to jest siatka na resztę przypadków, w tym literówkę
     * w `LOADOUT_PROJECT`. */
    if let Some(missing) = [turn.home, turn.project]
        .into_iter()
        .find(|path| !path.is_dir())
    {
        return Err(TurnEnd::Failed(format!(
            "Loadout keeps this folder away from the agent, but it is not there: {}. Make it, or \
             pick a different project folder, and start the build again.",
            missing.display()
        )));
    }
    let fence = match FilesystemFence::new(
        prepared.writable_roots,
        readable_roots,
        vec![turn.home.to_path_buf(), turn.project.to_path_buf()],
    )
    .and_then(|fence| fence.reading_files(readable_files))
    {
        Ok(fence) => fence,
        Err(error) => {
            return Err(TurnEnd::Failed(format!(
                "Loadout could not protect this batch's files: {error}. No agent was started."
            )));
        }
    };
    let Some(protected) = prepared.driver.with_filesystem_fence(&fence) else {
        return Err(TurnEnd::Failed(format!(
            "{} cannot enforce protected file access, so no agent was started.",
            shown_driver(turn.driver.as_ref())
        )));
    };
    if let Err(error) = fence.prove_available().await {
        return Err(TurnEnd::Failed(format!(
            "Loadout could not prove protected file access for this batch: {error}. No agent was started."
        )));
    }
    let protected = protected
        .for_step(&StepTag::new(&turn.request.operation_id, "context-build"))
        .unwrap_or(protected);
    Ok(PreparedTurn {
        driver: protected,
        slot,
        scratch,
    })
}

async fn finish_started_turn(
    mut handle: Box<dyn AgentHandle>,
    slot: Slot,
    drain: tokio::task::JoinHandle<()>,
    mut over_budget: mpsc::Receiver<f64>,
    seen_model: Arc<Mutex<String>>,
    cancel: &CancellationToken,
    left: Duration,
) -> TurnEnd {
    let ended = tokio::select! {
        biased;
        () = cancel.cancelled() => {
            let proof = handle.cancel().await;
            if matches!(proof, GroupProof::Alive { .. }) {
                // 2026-09-08 (CT-04) — miejsce nie wraca nad żywą grupą. Oddanie go tutaj
                // pozwoliłoby następnej pracy wystartować ponad „ile naraz" (niezmienniki 6, 11).
                std::mem::forget(slot);
            }
            drain.abort();
            return TurnEnd::Cancelled(proof);
        }
        () = tokio::time::sleep(left) => {
            let proof = handle.cancel().await;
            if matches!(proof, GroupProof::Alive { .. }) {
                // Ten sam obowiązek co przy Stopie: brak dowodu pozostaje właścicielem miejsca.
                std::mem::forget(slot);
            }
            drain.abort();
            return TurnEnd::OutOfTime(proof);
        }
        Some(_) = over_budget.recv() => {
            let proof = handle.cancel().await;
            drain.abort();
            if matches!(proof, GroupProof::Alive { .. }) {
                std::mem::forget(slot);
                return TurnEnd::StillRunning(
                    "This build reached its spending limit, but Loadout could not make sure the agent stopped, so it may still be running. This build remains in use."
                        .to_owned(),
                );
            }
            drop(slot);
            return TurnEnd::Failed(
                "This build reached its spending limit. Change that limit in Settings before rebuilding."
                    .to_owned(),
            );
        }
        result = handle.wait() => result,
    };
    let outcome = match ended {
        Ok(outcome) => outcome,
        Err(error) => {
            let proof = handle.cancel().await;
            if matches!(proof, GroupProof::Alive { .. }) {
                std::mem::forget(slot);
                drain.abort();
                return TurnEnd::StillRunning(
                    "The connection to the agent broke, and it may still be running. This build remains in use."
                        .to_owned(),
                );
            }
            drain.abort();
            return TurnEnd::Failed(format!(
                "The connection to the agent broke: {error}. Try Rebuild context."
            ));
        }
    };
    let closed = handle.close().await;
    let proof = handle.proof_of_death().await;
    drop(handle);
    let _ = drain.await;
    if matches!(proof, GroupProof::Alive { .. }) {
        std::mem::forget(slot);
        return TurnEnd::StillRunning(
            "The agent answered, but Loadout could not make sure it stopped, so it may still be running. This build remains in use."
                .to_owned(),
        );
    }
    drop(slot);
    if let Err(error) = closed {
        return TurnEnd::Failed(format!(
            "The agent did not close cleanly: {error}. Try Rebuild context."
        ));
    }
    if !outcome.ok && !matches!(outcome.reason, FinishReason::Completed) {
        return TurnEnd::Failed(match outcome.reason {
            FinishReason::Failed(said) if !said.trim().is_empty() => format!(
                "The agent stopped before finishing this batch. It said: {}",
                said.trim()
            ),
            FinishReason::LimitReached => "The agent reached one of its own limits. Check its account, then try Rebuild context."
                .to_owned(),
            _ => "The agent stopped before finishing this batch. Try Rebuild context.".to_owned(),
        });
    }
    let model = seen_model
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    TurnEnd::Ready(TurnAnswer {
        text: outcome.text,
        model,
        cost_usd: outcome.cost_usd,
    })
}

async fn choose_vendor(
    home: &Path,
    library: &Path,
    drivers: &Drivers,
    request: &BuildContextRequest,
) -> Result<Option<Vendor>, String> {
    let checked = super::agent_apps::check_agent_apps_inner(drivers).await;
    let state = |vendor| {
        let name = name_of(vendor);
        checked
            .iter()
            .find(|entry| entry.app == name)
            .map_or(AgentAppState::CouldNotCheck, |entry| entry.state)
    };
    let claude = state(Vendor::ClaudeCode);
    let codex = state(Vendor::Codex);
    let claude_found = claude == AgentAppState::Found;
    let codex_found = codex == AgentAppState::Found;
    if claude_found != codex_found {
        return Ok(if claude_found {
            Some(Vendor::ClaudeCode)
        } else {
            Some(Vendor::Codex)
        });
    }
    if !claude_found {
        if claude == AgentAppState::CouldNotCheck || codex == AgentAppState::CouldNotCheck {
            let names = [("Claude Code", claude), ("Codex", codex)]
                .into_iter()
                .filter_map(|(name, state)| (state == AgentAppState::CouldNotCheck).then_some(name))
                .collect::<Vec<_>>()
                .join(" and ");
            return Err(format!(
                "Loadout could not check whether {names} can run here. Check that app, then try Rebuild context."
            ));
        }
        return Ok(None);
    }
    if let Some(chosen) = request.app {
        return Ok(Some(chosen));
    }
    if let Ok(Some(last)) = build::read_build(library, &request.set_id, None) {
        match last.app.as_str() {
            "claude-code" => return Ok(Some(Vendor::ClaudeCode)),
            "codex" => return Ok(Some(Vendor::Codex)),
            _ => {}
        }
    }
    let lead = super::settings::read_settings_inner(home)
        .ok()
        .map(|settings| settings.default_lead)
        .unwrap_or_default();
    if !lead.is_empty()
        && let Ok(agents) = super::agents::list_agents_inner(home)
        && let Some(agent) = agents.iter().find(|agent| agent.id.to_string() == lead)
    {
        return Ok(Some(agent.runs_with));
    }
    Ok(Some(Vendor::ClaudeCode))
}

fn failed_before_start(
    library: &Path,
    request: &BuildContextRequest,
    said: String,
) -> Result<ContextBuildRead, Error> {
    let shown_app = request.app.map_or("claude-code", name_of);
    let (draft_revision, input_fingerprint, mut sources) = match build::freeze_and_split(
        library,
        &request.set_id,
        shown_app,
        normalized_model(request.model.as_deref()),
        request.budget_usd,
    ) {
        Ok(frozen) => (
            frozen.set.draft_revision,
            frozen.input_fingerprint,
            frozen.sources,
        ),
        Err(failure) => {
            let draft_revision = files::read_set(library, &request.set_id)?
                .set
                .draft_revision;
            (draft_revision, String::new(), failure.sources)
        }
    };
    // 2026-09-08 (CT-04) — nawet odmowa przed startem rozlicza dokładne fragmenty, które
    // zostałyby partiami. Wpis `whole` dla wielostronicowego PDF-a ukrywałby, czy kompletność
    // dotyczy jednej strony czy całego źródła.
    for source in &mut sources {
        if source.outcome != SourceOutcome::Excluded {
            source.outcome = SourceOutcome::Failed;
            source.said.clone_from(&said);
        }
    }
    let mut state = build::initial_state(
        request,
        shown_app,
        draft_revision,
        input_fingerprint,
        sources,
        &super::now_utc(),
    );
    finish_failure(library, &mut state, said)
}

fn finish_failure(
    library: &Path,
    state: &mut ContextBuild,
    said: String,
) -> Result<ContextBuildRead, Error> {
    settle_unread_sources(state);
    state.end = BuildEnd::Failed;
    state.said = said;
    state.changed_at = super::now_utc();
    build::write_state(library, state)?;
    view(library, state.clone())
}

fn view(library: &Path, state: ContextBuild) -> Result<ContextBuildRead, Error> {
    Ok(ContextBuildRead {
        build_with: state.app.clone(),
        revision: build::read_latest_revision(library, &state.set_id)?,
        build: Some(state),
    })
}

fn mark_batch(state: &mut ContextBuild, batch: &Batch, outcome: SourceOutcome, said: &str) {
    for progress in &mut state.sources {
        let reference = crate::context::SourceReference {
            source_id: progress.source_id.clone(),
            part: progress.part.clone(),
        };
        if batch.references.contains(&reference) {
            progress.outcome = outcome;
            said.clone_into(&mut progress.said);
        }
    }
    state.changed_at = super::now_utc();
}

fn settle_unread_sources(state: &mut ContextBuild) {
    // 2026-09-08 (CT-04) — po końcu budowania `Unknown` nie oznacza już „czeka". Każdy
    // fragment musi dostać końcowy wynik, inaczej ekran zachowuje pozór żywego postępu po
    // nazwanej porażce, anulowaniu albo braku dowodu śmierci.
    for source in &mut state.sources {
        if source.outcome == SourceOutcome::Unknown {
            source.outcome = SourceOutcome::Failed;
            "This part was not read because the build ended before reaching it."
                .clone_into(&mut source.said);
        }
    }
}

fn not_findings(why: &NotFindings) -> String {
    format!(
        "{} The one format correction was already used. Try Rebuild context.",
        why.said()
    )
}

fn normalized_model(model: Option<&str>) -> Option<&str> {
    model.map(str::trim).filter(|model| !model.is_empty())
}

const fn name_of(vendor: Vendor) -> &'static str {
    match vendor {
        Vendor::ClaudeCode => "claude-code",
        Vendor::Codex => "codex",
    }
}

fn vendor_of(app: &str) -> Option<Vendor> {
    match app {
        "claude-code" => Some(Vendor::ClaudeCode),
        "codex" => Some(Vendor::Codex),
        _ => None,
    }
}

const fn shown_name(vendor: Vendor) -> &'static str {
    match vendor {
        Vendor::ClaudeCode => "Claude Code",
        Vendor::Codex => "Codex",
    }
}

fn shown_driver(driver: &dyn AgentDriver) -> &'static str {
    if driver.id() == "codex" {
        "Codex"
    } else {
        "Claude Code"
    }
}
