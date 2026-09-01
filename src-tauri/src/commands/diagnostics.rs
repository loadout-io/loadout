//! Bezpieczny raport wsparcia dla jednego workspace.
//!
//! Raport powstaje od zera z zamknietych typow. Nie serializujemy prywatnego modelu produktu,
//! a potem nie probujemy go „oczyscic": przypadkowe nowe pole nie moze samo wejsc do schowka.

use std::fs;
use std::io;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

/// Stale, allowlistowane odmowy granicy schowka. Zrodlo bledu nigdy nie przechodzi do okna.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("Loadout could not collect diagnostics.")]
    Collect,
    #[error("Loadout could not copy diagnostics.")]
    Clipboard,
}

/// Jedyna odpowiedz, ktora wolno oddac webviewowi po skopiowaniu raportu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReceipt {
    pub runs: usize,
    pub conversations: usize,
    pub artifacts: usize,
}

/// Allowlistowany dokument, ktory pozostaje po stronie Rusta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportReport {
    text: String,
    receipt: DiagnosticsReceipt,
}

impl SupportReport {
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn receipt(&self) -> DiagnosticsReceipt {
        self.receipt
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportDocument {
    schema_version: u8,
    workspace: WorkspaceFacts,
    runs: Vec<RunFacts>,
    conversations: Vec<ConversationFacts>,
    receipt: DiagnosticsReceipt,
}

#[derive(Debug, Serialize)]
struct WorkspaceFacts {
    counts: Counts,
}

#[derive(Debug, Serialize)]
struct Counts {
    runs: usize,
    conversations: usize,
    artifacts: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunFacts {
    id: String,
    state: &'static str,
    complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<i64>,
    steps: Vec<StepFacts>,
    artifacts: ArtifactSet,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StepFacts {
    id: String,
    kind: &'static str,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor: Option<&'static str>,
    model: Presence,
    death_proof: Presence,
    /* CZY TEN KROK NAPRAWDĘ SIĘ WYKONAŁ — dwa boole, zero słów (2026-08-30).
     *
     * ZMIERZONE NA BIEGU WŁAŚCICIELA `20260829-204729`: trzynaście kroków, wszystkie
     * `succeeded`, a cztery z nich bez czasu startu, bez kodu wyjścia i bez jednego pliku
     * w `logs/`. Były to zbędne próby pętli — `run.json` mówił to wprost przez `executed`
     * i `process_started`, a ten zrzut oba pola gubił. Czytający widział więc krok meldujący
     * sukces bez śladu wykonania, czyli dokładnie tę klasę wady, dla której to repo powstało
     * (niezmiennik 19). Kosztowało to jedną błędną diagnozę.
     *
     * ZDANIA `summary` TU NIE MA I NIE BĘDZIE. Pisze je agent, a ten zrzut człowiek wkleja
     * obcym. Raport stoi na zamkniętej liście dozwolonych pól, nigdy na redagowaniu prywatnych
     * bajtów (T-34 AC-3) — dwa boole niosą całe rozróżnienie i ani jednego słowa. */
    executed: bool,
    process_started: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<i64>,
    exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cached_tokens: Option<u64>,
    artifacts: ArtifactSet,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversationFacts {
    id: String,
    vendor: &'static str,
    model_configured: bool,
    state: &'static str,
    complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<i64>,
    attempts: usize,
    turns: usize,
    agent_turns: u64,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
    exit_code: Option<i64>,
    death_proof: Presence,
    artifacts: ConversationArtifacts,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactSet {
    stdout: Presence,
    stderr: Presence,
    input_manifest: Presence,
    handoffs: Count,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversationArtifacts {
    stdout: Presence,
    stderr: Presence,
    input_manifest: Presence,
    turn_files: Count,
}

#[derive(Debug, Serialize)]
struct Presence {
    present: bool,
}

#[derive(Debug, Serialize)]
struct Count {
    total: usize,
}

/// Minimalny ksztalt czytany z prywatnego `run.json`.
///
/// Wszystkie teksty poza identyfikatorami sa ignorowane. Nawet `model`, `name`, `error`, argv i
/// env nie maja pola w tym typie, wiec nowy lub zlosliwy tekst nie moze trafic do raportu.
#[derive(Debug, Default, Deserialize)]
struct RunInput {
    #[serde(default)]
    status: String,
    #[serde(default)]
    created_at: Option<i64>,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    ended_at: Option<i64>,
    #[serde(default)]
    steps: Vec<StepInput>,
}

#[derive(Debug, Default, Deserialize)]
struct StepInput {
    #[serde(default)]
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    ended_at: Option<i64>,
    #[serde(default)]
    exit_code: Option<i64>,
    #[serde(default)]
    turns: Option<u64>,
    #[serde(default, alias = "inputTokens")]
    input_tokens: Option<u64>,
    #[serde(default, alias = "outputTokens")]
    output_tokens: Option<u64>,
    #[serde(default, alias = "cachedTokens")]
    cached_tokens: Option<u64>,
    #[serde(default)]
    effective: Option<EffectiveInput>,
    #[serde(default)]
    death_proof: bool,
    /* Brak klucza czyta się jako `false` i to jest uczciwe: `run.json` sprzed T-207 nie niósł
     * tych faktów, a wyprowadzanie ich ze statusu byłoby zgadywaniem tego, co to pole ma
     * rozstrzygać. Krok, który naprawdę poszedł, ma je zapisane. */
    #[serde(default)]
    executed: bool,
    #[serde(default)]
    process_started: bool,
}

#[derive(Debug, Default, Deserialize)]
struct EffectiveInput {
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationInput {
    #[serde(default)]
    vendor: String,
    #[serde(default)]
    model_configured: bool,
    #[serde(default)]
    state: String,
    #[serde(default)]
    complete: bool,
    #[serde(default)]
    failure_kind: Option<String>,
    #[serde(default)]
    created_at: Option<i64>,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    ended_at: Option<i64>,
    #[serde(default)]
    attempts: usize,
    #[serde(default)]
    turns: usize,
    #[serde(default)]
    agent_turns: u64,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cached_tokens: u64,
    #[serde(default)]
    exit_code: Option<i64>,
    #[serde(default)]
    death_proof: bool,
}

/// Buduje raport wyłącznie z allowlistowanych pol jednego workspace.
pub fn support_report(workspace: &Path) -> anyhow::Result<SupportReport> {
    require_real_directory(workspace)?;
    let loadout = workspace.join(".loadout");
    match fs::symlink_metadata(&loadout) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => anyhow::bail!("the Loadout data root is not a real directory"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let receipt = DiagnosticsReceipt {
                runs: 0,
                conversations: 0,
                artifacts: 0,
            };
            let document = ReportDocument {
                schema_version: 1,
                workspace: WorkspaceFacts {
                    counts: Counts {
                        runs: 0,
                        conversations: 0,
                        artifacts: 0,
                    },
                },
                runs: Vec::new(),
                conversations: Vec::new(),
                receipt,
            };
            return Ok(SupportReport {
                text: serde_json::to_string_pretty(&document)?,
                receipt,
            });
        }
        Err(error) => return Err(error.into()),
    }
    let (runs, run_artifacts) = scan_runs(&loadout.join("runs"))?;
    let (conversations, conversation_artifacts) =
        scan_conversations(&loadout.join("conversations"))?;
    let receipt = DiagnosticsReceipt {
        runs: runs.len(),
        conversations: conversations.len(),
        artifacts: run_artifacts.saturating_add(conversation_artifacts),
    };
    let document = ReportDocument {
        schema_version: 1,
        workspace: WorkspaceFacts {
            counts: Counts {
                runs: receipt.runs,
                conversations: receipt.conversations,
                artifacts: receipt.artifacts,
            },
        },
        runs,
        conversations,
        receipt,
    };
    let text = serde_json::to_string_pretty(&document)?;
    Ok(SupportReport { text, receipt })
}

/// Buduje raport i przekazuje jego tekst wprost rustowej granicy zapisu schowka.
///
/// Ten helper jest wspolny dla IPC i kryterium, zeby webview nigdy nie dostal tekstu raportu,
/// a blad collectora lub pluginu zostal zwiniety do stalego bezpiecznego wariantu.
pub fn copy_diagnostics_with<F, E>(
    workspace: &Path,
    write_clipboard: F,
) -> Result<DiagnosticsReceipt, DiagnosticsError>
where
    F: FnOnce(&str) -> Result<(), E>,
{
    let report = support_report(workspace).map_err(|_error| DiagnosticsError::Collect)?;
    write_clipboard(report.text()).map_err(|_error| DiagnosticsError::Clipboard)?;
    Ok(report.receipt())
}

fn scan_runs(root: &Path) -> anyhow::Result<(Vec<RunFacts>, usize)> {
    let mut out = Vec::new();
    let mut artifacts = 0_usize;
    for (id, run_dir) in real_children(root)? {
        let input = read_json::<RunInput>(&run_dir.join("run.json")).unwrap_or_default();
        let handoffs = count_real_files(&run_dir.join("handoffs"))?;
        let run_artifact_set = ArtifactSet {
            stdout: Presence { present: false },
            stderr: Presence { present: false },
            input_manifest: Presence { present: false },
            handoffs: Count { total: handoffs },
        };
        artifacts = artifacts.saturating_add(handoffs);
        let mut steps = Vec::new();
        for step in input.steps {
            let Some(step_id) = safe_identifier(&step.id) else {
                continue;
            };
            let set = step_artifacts(&run_dir, &step_id);
            let kind = safe_step_kind(&step.kind, &step.agent);
            let failure_kind =
                safe_failure_kind(kind, &step.status, step.exit_code, step.death_proof, &set);
            artifacts = artifacts.saturating_add(artifact_count(&set));
            steps.push(StepFacts {
                id: step_id,
                kind,
                state: safe_step_state(&step.status),
                vendor: safe_vendor(&step.agent),
                model: Presence {
                    /* Sam napis modelu jest arbitralny i moze byc sekretem wpisanym przez
                     * czlowieka. Raportuje sie wylacznie fakt konfiguracji, nigdy wartosc. */
                    present: step
                        .effective
                        .as_ref()
                        .and_then(|effective| effective.model.as_deref())
                        .is_some_and(|model| !model.trim().is_empty()),
                },
                /* `run.json` starszej wersji nie mial dowodu zejscia. `false` jest uczciwym
                 * brakiem dowodu; nie wyprowadzamy go z terminalnego statusu ani exit code. */
                death_proof: Presence {
                    present: step.death_proof,
                },
                executed: step.executed,
                process_started: step.process_started,
                failure_kind,
                started_at: step.started_at,
                ended_at: step.ended_at,
                exit_code: step.exit_code,
                turns: step.turns,
                input_tokens: step.input_tokens,
                output_tokens: step.output_tokens,
                cached_tokens: step.cached_tokens,
                artifacts: set,
            });
        }
        out.push(RunFacts {
            id,
            state: safe_run_state(&input.status),
            complete: is_terminal_run(&input.status),
            failure_kind: (input.status == "failed").then_some(
                if steps
                    .iter()
                    .any(|step| step.failure_kind == Some("evidenceIncomplete"))
                {
                    "evidenceIncomplete"
                } else if steps
                    .iter()
                    .any(|step| step.failure_kind == Some("processExit"))
                {
                    "processExit"
                } else if steps
                    .iter()
                    .any(|step| step.failure_kind == Some("checkFailed"))
                {
                    "checkFailed"
                } else {
                    "agentFailed"
                },
            ),
            created_at: input.created_at,
            started_at: input.started_at,
            ended_at: input.ended_at,
            steps,
            artifacts: run_artifact_set,
        });
    }
    out.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((out, artifacts))
}

fn scan_conversations(root: &Path) -> anyhow::Result<(Vec<ConversationFacts>, usize)> {
    let mut out = Vec::new();
    let mut artifacts = 0_usize;
    for (id, conversation_dir) in real_children(root)? {
        let input = read_json::<ConversationInput>(&conversation_dir.join("conversation.json"))
            .unwrap_or_default();
        let set = ConversationArtifacts {
            stdout: Presence {
                present: is_real_file(&conversation_dir.join("logs/lead.jsonl")),
            },
            stderr: Presence {
                present: is_real_file(&conversation_dir.join("logs/lead.stderr.log")),
            },
            input_manifest: Presence {
                present: is_real_file(&conversation_dir.join("input.json")),
            },
            turn_files: Count {
                total: count_real_files(&conversation_dir.join("turns"))?,
            },
        };
        artifacts = artifacts.saturating_add(conversation_artifact_count(&set));
        out.push(ConversationFacts {
            id,
            vendor: safe_conversation_vendor(&input.vendor),
            model_configured: input.model_configured,
            state: safe_conversation_state(&input.state),
            complete: input.complete,
            failure_kind: safe_conversation_failure(input.failure_kind.as_deref(), &input.state),
            created_at: input.created_at,
            started_at: input.started_at,
            ended_at: input.ended_at,
            attempts: input.attempts,
            turns: input.turns,
            agent_turns: input.agent_turns,
            input_tokens: input.input_tokens,
            output_tokens: input.output_tokens,
            cached_tokens: input.cached_tokens,
            exit_code: input.exit_code,
            death_proof: Presence {
                present: input.death_proof,
            },
            artifacts: set,
        });
    }
    out.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((out, artifacts))
}

fn step_artifacts(run_dir: &Path, step_id: &str) -> ArtifactSet {
    let logs = run_dir.join("logs");
    ArtifactSet {
        stdout: Presence {
            present: is_real_file(&logs.join(format!("agent-{step_id}.jsonl"))),
        },
        stderr: Presence {
            present: is_real_file(&logs.join(format!("agent-{step_id}.stderr.log"))),
        },
        input_manifest: Presence {
            present: is_real_file(&logs.join(format!("agent-{step_id}.input.json"))),
        },
        handoffs: Count { total: 0 },
    }
}

fn artifact_count(set: &ArtifactSet) -> usize {
    usize::from(set.stdout.present)
        .saturating_add(usize::from(set.stderr.present))
        .saturating_add(usize::from(set.input_manifest.present))
        .saturating_add(set.handoffs.total)
}

fn conversation_artifact_count(set: &ConversationArtifacts) -> usize {
    usize::from(set.stdout.present)
        .saturating_add(usize::from(set.stderr.present))
        .saturating_add(usize::from(set.input_manifest.present))
        .saturating_add(set.turn_files.total)
}

fn real_children(root: &Path) -> anyhow::Result<Vec<(String, std::path::PathBuf)>> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Ok(Vec::new()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    }
    let mut children = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let raw = entry.file_name();
        let Some(name) = raw.to_str().and_then(safe_identifier) else {
            continue;
        };
        children.push((name, entry.path()));
    }
    Ok(children)
}

fn count_real_files(root: &Path) -> anyhow::Result<usize> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Ok(0),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    }
    let mut count = 0_usize;
    for entry in fs::read_dir(root)? {
        let metadata = fs::symlink_metadata(entry?.path())?;
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    if !is_real_file(path) {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn require_real_directory(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("the selected workspace is not a real directory");
    }
    Ok(())
}

fn safe_identifier(value: &str) -> Option<String> {
    let path = Path::new(value);
    let safe = !value.is_empty()
        && value.len() <= 160
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    safe.then(|| value.to_owned())
}

fn safe_run_state(value: &str) -> &'static str {
    match value {
        "queued" => "queued",
        "running" => "running",
        "paused" => "paused",
        "succeeded" => "succeeded",
        "failed" => "failed",
        "cancelled" | "canceled" => "cancelled",
        /* 2026-08-23 — BEZ TEJ LINII TRZY BIEGI WŁAŚCICIELA MELDOWAŁY SIĘ JAKO `unknown`.
         *
         * `interrupted` pisze odzyskiwanie po biegu, który zginął razem z oknem, i pisze je
         * od dawna — do tego dnia tylko do bazy biblioteki, której `run.json` nigdy nie widział,
         * więc luka nie miała jak wyjść. Odkąd sprzątanie przepisuje pliki
         * (`ipc::AppState::settle_everything_left_behind`), ten status trafia na wejście tej
         * funkcji i wypada tu na `_`.
         *
         * To jest gorsze niż brak zdania: `unknown` plus `complete: false` czyta się jako „nie
         * wiadomo, co z tym biegiem, może jeszcze trwa" — czyli DOKŁADNIE ten stan, który
         * sprzątanie właśnie rozstrzygnęło. */
        "interrupted" => "interrupted",
        _ => "unknown",
    }
}

fn safe_step_state(value: &str) -> &'static str {
    match value {
        "pending" => "pending",
        "ready" => "ready",
        "running" => "running",
        "waiting" => "waiting",
        "succeeded" => "succeeded",
        "failed" => "failed",
        "cancelled" | "canceled" => "cancelled",
        "skipped" => "skipped",
        _ => "unknown",
    }
}

fn safe_vendor(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude-code" | "claudecode" => Some("claude"),
        "codex" => Some("codex"),
        _ => None,
    }
}

fn safe_conversation_vendor(value: &str) -> &'static str {
    match value {
        "claude" => "claude",
        "codex" => "codex",
        _ => "unknown",
    }
}

fn safe_conversation_state(value: &str) -> &'static str {
    match value {
        "active" => "active",
        "failed" => "failed",
        "cancelled" | "canceled" => "cancelled",
        "closed" => "closed",
        _ => "unknown",
    }
}

fn safe_conversation_failure(failure: Option<&str>, state: &str) -> Option<&'static str> {
    match failure {
        Some("startFailed") => Some("startFailed"),
        Some("deliveryFailed") => Some("deliveryFailed"),
        Some("agentFailed") => Some("agentFailed"),
        Some("evidenceIncomplete") => Some("evidenceIncomplete"),
        Some("cancelled" | "canceled") => Some("cancelled"),
        Some(_) => Some("unknown"),
        None if matches!(state, "failed" | "cancelled" | "canceled") => Some("unknown"),
        None => None,
    }
}

fn is_terminal_run(value: &str) -> bool {
    // `interrupted` JEST końcem, i to nie jest drobiazg nazewniczy: bieg przerwany razem z oknem
    // nikogo już nie ma, kto by go prowadził. Bez niego odczyt mówi `complete: false` o biegu,
    // po którym właśnie posprzątano, i podsuwa czekanie zamiast odpowiedzi.
    matches!(
        value,
        "succeeded" | "failed" | "cancelled" | "canceled" | "interrupted"
    )
}

fn safe_failure_kind(
    kind: &str,
    state: &str,
    exit_code: Option<i64>,
    death_proof: bool,
    artifacts: &ArtifactSet,
) -> Option<&'static str> {
    if state != "failed" {
        return None;
    }
    if kind == "agent"
        && (!artifacts.stdout.present
            || !artifacts.stderr.present
            || !artifacts.input_manifest.present)
    {
        return Some("evidenceIncomplete");
    }
    if exit_code.is_some_and(|code| code != 0) {
        return Some("processExit");
    }
    if kind == "check" {
        return Some("checkFailed");
    }
    if !death_proof {
        return Some("agentFailed");
    }
    Some("unknown")
}

fn safe_step_kind(kind: &str, vendor: &str) -> &'static str {
    match kind {
        "agent" => "agent",
        "check" => "check",
        "checkpoint" => "checkpoint",
        // Stare run.json nie miały pola `kind`; niepusty zamknięty vendor nadal dowodzi agenta.
        _ if safe_vendor(vendor).is_some() => "agent",
        _ => "unknown",
    }
}
