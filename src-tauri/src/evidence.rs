//! Prywatne dowody agenta oraz bezpieczny opis tego, co weszlo do tury.
//!
//! Ten modul jest wspolnym szwem obu adapterow. Rozstrzyga nazwy plikow, zapisuje bezpieczny
//! manifest i trzyma jedna monotoniczna odpowiedz na pytanie „czy kazdy zapis sie udal".
//! Adapter zna bajty vendora, ale nie wymysla ukladu katalogu; rozmowa zna dowod smierci,
//! ale nie moze oglosic kompletnosci po nieudanym zapisie.

use std::fmt;
use std::io::{self, Read as _};
use std::path::Component;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::engine::supervisor::{self, PrivateFileAccess, PrivateFilePublisher};

const LOGS_DIR: &str = "logs";

/// Miejsce i logiczna tozsamosc jednego prywatnego kompletu dowodow.
#[derive(Clone)]
pub struct EvidenceTarget {
    anchor: PathBuf,
    root: PathBuf,
    identity: EvidenceIdentity,
    input: SafeInputManifest,
    healthy: Arc<AtomicBool>,
    receipt: Arc<tokio::sync::Mutex<()>>,
}

/// Rodzaj sesji, bez vendorowego identyfikatora i bez arbitralnego tekstu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceIdentity {
    /// Jeden fizyczny krok biegu; jego stdout zachowuje nazwe czytana przez `store::rebuild`.
    WorkflowStep { step_id: String },
    /// Jedna prywatna tura refleksji po biegu; nie jest i nie udaje kroku grafu.
    Reflection,
    /// Rozmowa Lead, ktora nie jest i nie udaje workflow.
    LeadConversation { conversation_id: Uuid },
}

/// Allowlistowany manifest zrodel wejscia. Nie niesie tresci ani skrotu promptu.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeInputManifest {
    /// Ile bajtow mial finalny tekst przekazany stdinem; sama liczba, nigdy tekst ani hash.
    pub prompt_bytes: usize,
    /// Zrodla w kolejnosci, w ktorej zlozono kontekst.
    pub context: Vec<ContextSource>,
    /// Obrazy opisane wylacznie typem i rozmiarem.
    pub images: Vec<ImageFact>,
}

/// Jedno zrodlo kontekstu bez jego tresci.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSource {
    /// Zamkniety rodzaj z allowlisty.
    pub kind: ContextKind,
    /// Wzgledny identyfikator albo sciezka; nigdy absolutna sciezka gospodarza.
    pub reference: String,
    /// Rozmiar materialu, ktory wszedl do promptu.
    pub bytes: usize,
}

/// Dozwolone rodzaje zrodel kontekstu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextKind {
    WorkflowStep,
    RunTask,
    MemoryNote,
    InheritedSkill,
    InheritedLearning,
    Handoff,
}

/// Bezpieczny fakt o obrazie. Bajty obrazu nie implementuja tego typu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFact {
    pub mime: String,
    pub bytes: usize,
}

/// Zamknięty vendor rozmowy. Vendorowy identyfikator sesji nie ma odpowiednika w tym typie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationVendor {
    Claude,
    Codex,
    /// Zamknięty fallback dla dublera albo przyszłego adaptera; nigdy surowe `driver.id()`.
    Unknown,
}

/// Bezpieczne fakty znane przed startem pierwszej tury.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversationMetadata {
    pub vendor: ConversationVendor,
    pub model_configured: bool,
}

/// Zamknięty rodzaj porażki receipt-u; nigdy arbitralny tekst błędu vendora.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceFailureKind {
    StartFailed,
    DeliveryFailed,
    AgentFailed,
    EvidenceIncomplete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ConversationState {
    Active,
    Failed,
    Cancelled,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AttemptState {
    Sending,
    Delivered,
    Succeeded,
    Failed,
    Cancelled,
}

/// Allowlistowane liczniki jednej terminalnej tury.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnCounters {
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
}

impl EvidenceTarget {
    /// Prywatne dowody kroku pod istniejacym katalogiem biegu.
    #[must_use]
    pub fn workflow_step(run_dir: PathBuf, step_id: String, input: SafeInputManifest) -> Self {
        Self {
            anchor: run_dir.clone(),
            root: run_dir,
            identity: EvidenceIdentity::WorkflowStep { step_id },
            input,
            healthy: Arc::new(AtomicBool::new(true)),
            receipt: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Prywatne dowody refleksji pod katalogiem tego samego biegu.
    #[must_use]
    pub fn reflection(_run_dir: PathBuf, _input: SafeInputManifest) -> Self {
        todo!("T-126: give reflection its exact private evidence target")
    }

    /// Prywatne dowody rozmowy pod workspace, nigdy w globalnej bibliotece i nigdy w `runs/`.
    #[must_use]
    pub fn lead(workspace: &Path, conversation_id: Uuid, input: SafeInputManifest) -> Self {
        Self {
            anchor: workspace.to_path_buf(),
            root: workspace
                .join(".loadout")
                .join("conversations")
                .join(conversation_id.to_string()),
            identity: EvidenceIdentity::LeadConversation { conversation_id },
            input,
            healthy: Arc::new(AtomicBool::new(true)),
            receipt: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn identity(&self) -> &EvidenceIdentity {
        &self.identity
    }

    #[must_use]
    pub fn input(&self) -> &SafeInputManifest {
        &self.input
    }

    /// Surowy stdout vendora. Nazwa kroku jest kontraktem z `store::rebuild`.
    #[must_use]
    pub fn stdout_path(&self) -> PathBuf {
        match &self.identity {
            EvidenceIdentity::WorkflowStep { step_id } => self
                .root
                .join(LOGS_DIR)
                .join(format!("agent-{step_id}.jsonl")),
            EvidenceIdentity::Reflection => {
                todo!("T-126: name reflection stdout without pretending it is a workflow step")
            }
            EvidenceIdentity::LeadConversation { .. } => {
                self.root.join(LOGS_DIR).join("lead.jsonl")
            }
        }
    }

    /// Surowy stderr vendora, osobno od NDJSON-u czytanego przez odbudowe.
    #[must_use]
    pub fn stderr_path(&self) -> PathBuf {
        match &self.identity {
            EvidenceIdentity::WorkflowStep { step_id } => self
                .root
                .join(LOGS_DIR)
                .join(format!("agent-{step_id}.stderr.log")),
            EvidenceIdentity::Reflection => {
                todo!("T-126: name reflection stderr without pretending it is a workflow step")
            }
            EvidenceIdentity::LeadConversation { .. } => {
                self.root.join(LOGS_DIR).join("lead.stderr.log")
            }
        }
    }

    /// Allowlistowany opis wejscia, nigdy samo wejscie ani jego hash.
    #[must_use]
    pub fn input_path(&self) -> PathBuf {
        match &self.identity {
            EvidenceIdentity::WorkflowStep { step_id } => self
                .root
                .join(LOGS_DIR)
                .join(format!("agent-{step_id}.input.json")),
            EvidenceIdentity::Reflection => {
                todo!("T-126: name reflection input without pretending it is a workflow step")
            }
            EvidenceIdentity::LeadConversation { .. } => self.root.join("input.json"),
        }
    }

    /// Zaklada katalog, zapisuje bezpieczny manifest i otwiera oba potoki w trybie append.
    ///
    /// Append jest istotny dla Codeksa: kolejna tura workflow moze byc nowym procesem, ale
    /// pozostaje ta sama logiczna sesja. `create` bez `append` ucinalby pierwsza ture dopiero
    /// przy drugim zdaniu, czyli w chwili, ktorej test jednego procesu nigdy nie widzi.
    pub async fn open(&self) -> io::Result<EvidenceStreams> {
        let result = self.open_inner().await;
        if result.is_err() {
            self.mark_incomplete();
        }
        result
    }

    /// Przygotowuje prywatny katalog bez uruchamiania procesu i bez otwierania strumieni.
    pub async fn prepare(&self) -> io::Result<()> {
        let result = async {
            self.prepare_directories().await?;
            self.write_manifest().await
        }
        .await;
        if result.is_err() {
            self.mark_incomplete();
        }
        result
    }

    /// Zakłada bezpieczną księgę rozmowy w stanie aktywnym.
    pub async fn begin_conversation(&self, metadata: ConversationMetadata) -> io::Result<()> {
        let EvidenceIdentity::LeadConversation { conversation_id } = &self.identity else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workflow evidence cannot become a lead conversation",
            ));
        };
        self.prepare().await?;
        let now = now_ms();
        let document = ConversationDocument {
            schema_version: 1,
            id: *conversation_id,
            vendor: metadata.vendor,
            model_configured: metadata.model_configured,
            state: ConversationState::Active,
            complete: false,
            created_at: now,
            started_at: now,
            ended_at: None,
            attempts: 0,
            turns: 0,
            failure_kind: None,
            exit_code: None,
            death_proof: false,
            agent_turns: 0,
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(io::Error::other)?;
        write_new_json(&self.anchor, &self.root.join("conversation.json"), &bytes).await
    }

    /// Rejestruje próbę przed dostarczeniem. `sending` jest jawne, więc odmowa nie zostawia
    /// pliku udającego zaakceptowaną turę.
    pub async fn begin_turn(&self, number: usize, input: &SafeInputManifest) -> io::Result<()> {
        validate_manifest(input)?;
        let _guard = self.receipt.lock().await;
        let turns = self.root.join("turns");
        ensure_directory(&turns).await?;
        let document = TurnDocument {
            schema_version: 1,
            attempt: number,
            state: AttemptState::Sending,
            started_at: now_ms(),
            delivered_at: None,
            ended_at: None,
            prompt_bytes: input.prompt_bytes,
            images: input.images.clone(),
            failure_kind: None,
            turns: 0,
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(io::Error::other)?;
        let turn_path = turns.join(format!("{number:04}.json"));
        write_new_json(&self.anchor, &turn_path, &bytes).await?;
        let mut conversation = self.read_conversation()?;
        if conversation.complete || number != conversation.attempts.saturating_add(1) {
            let _ = tokio::fs::remove_file(&turn_path).await;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the conversation attempt sequence is not contiguous",
            ));
        }
        conversation.attempts = number;
        if let Err(error) = self.write_conversation(&conversation).await {
            let _ = tokio::fs::remove_file(&turn_path).await;
            return Err(error);
        }
        Ok(())
    }

    /// Potwierdza, że vendor przyjął próbę. Licznik aktywnej rozmowy zmienia się od razu.
    pub async fn accept_turn(&self, number: usize) -> io::Result<()> {
        let _guard = self.receipt.lock().await;
        let mut conversation = self.read_conversation()?;
        if conversation.complete || conversation.attempts != number {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the conversation cannot accept this attempt",
            ));
        }
        let mut turn = self.read_turn(number)?;
        if conversation.turns.saturating_add(1) == number {
            conversation.turns = number;
        } else if conversation.turns != number {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the delivered conversation turn sequence is not contiguous",
            ));
        }
        match turn.state {
            AttemptState::Sending => {
                turn.state = AttemptState::Delivered;
                turn.delivered_at = Some(now_ms());
                self.write_turn_document(number, &turn).await?;
            }
            // Bardzo szybki vendor może wypchnąć `Finished` zanim zadanie wysyłające odzyska
            // sterowanie. Terminalny receipt jest wtedy mocniejszym dowodem dostarczenia.
            AttemptState::Delivered | AttemptState::Succeeded => {}
            AttemptState::Failed | AttemptState::Cancelled if turn.delivered_at.is_some() => {}
            AttemptState::Failed | AttemptState::Cancelled => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a failed conversation attempt cannot become delivered",
                ));
            }
        }
        self.write_conversation(&conversation).await
    }

    /// Utrwala terminalny wynik konkretnej zaakceptowanej próby.
    pub async fn finish_turn(
        &self,
        number: usize,
        counters: TurnCounters,
        ok: bool,
        cancelled: bool,
    ) -> io::Result<()> {
        let _guard = self.receipt.lock().await;
        let mut turn = self.read_turn(number)?;
        if !matches!(turn.state, AttemptState::Sending | AttemptState::Delivered) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the conversation attempt already has a terminal result",
            ));
        }
        let outcome_proves_delivery = turn.state == AttemptState::Sending;
        if outcome_proves_delivery {
            turn.delivered_at = Some(now_ms());
        }
        turn.state = if cancelled {
            AttemptState::Cancelled
        } else if ok {
            AttemptState::Succeeded
        } else {
            AttemptState::Failed
        };
        turn.ended_at = Some(now_ms());
        turn.failure_kind = if cancelled {
            Some(EvidenceFailureKind::Cancelled)
        } else if ok {
            None
        } else {
            Some(EvidenceFailureKind::AgentFailed)
        };
        turn.turns = counters.turns;
        turn.input_tokens = counters.input_tokens;
        turn.output_tokens = counters.output_tokens;
        turn.cached_tokens = counters.cached_tokens;

        let mut conversation = self.read_conversation()?;
        if outcome_proves_delivery {
            if conversation.turns.saturating_add(1) != number {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the completed conversation turn sequence is not contiguous",
                ));
            }
            conversation.turns = number;
        }
        conversation.agent_turns = conversation.agent_turns.saturating_add(counters.turns);
        conversation.input_tokens = conversation
            .input_tokens
            .saturating_add(counters.input_tokens);
        conversation.output_tokens = conversation
            .output_tokens
            .saturating_add(counters.output_tokens);
        conversation.cached_tokens = conversation
            .cached_tokens
            .saturating_add(counters.cached_tokens);
        if cancelled {
            conversation.state = ConversationState::Cancelled;
            conversation.failure_kind = turn.failure_kind;
        } else if !ok {
            conversation.state = ConversationState::Failed;
            conversation.failure_kind = turn.failure_kind;
        }
        // `conversation.json` jest agregatem czytanym przez support report, a terminalny plik
        // tury jest widocznym commit pointem próby. Publikujemy agregat pierwszy: kto zobaczy
        // `succeeded`/`failed` w `turns/NNNN.json`, ma już gwarancję aktualnych liczników i stanu
        // rozmowy. Odwrotna kolejność dawała realne okno `turn=succeeded, agentTurns=0`.
        self.write_conversation(&conversation).await?;
        if let Err(error) = self.write_turn_document(number, &turn).await {
            // Agregatu nie cofamy ścieżkowo: kolejny zapis mógłby otworzyć drugi wyścig. Target
            // zostaje nieodwracalnie niekompletny, więc `finish_conversation` nigdy nie ogłosi
            // takiej rozmowy jako kompletnej.
            self.mark_incomplete();
            return Err(error);
        }
        Ok(())
    }

    /// Kończy niedostarczoną próbę jawnie, zamiast zostawiać fantomowy plik `sending`.
    pub async fn fail_turn(&self, number: usize, failure: EvidenceFailureKind) -> io::Result<()> {
        let _guard = self.receipt.lock().await;
        let mut turn = self.read_turn(number)?;
        if turn.state != AttemptState::Sending {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "only an undelivered conversation attempt can fail delivery",
            ));
        }
        turn.state = if failure == EvidenceFailureKind::Cancelled {
            AttemptState::Cancelled
        } else {
            AttemptState::Failed
        };
        turn.ended_at = Some(now_ms());
        turn.failure_kind = Some(failure);
        self.write_turn_document(number, &turn).await?;
        let mut conversation = self.read_conversation()?;
        conversation.state = if failure == EvidenceFailureKind::Cancelled {
            ConversationState::Cancelled
        } else {
            ConversationState::Failed
        };
        conversation.failure_kind = Some(failure);
        self.write_conversation(&conversation).await
    }

    /// Ogłasza kompletność dopiero po dowodzie `Dead`, drainie i zdrowym flushu adaptera.
    pub async fn finish_conversation(
        &self,
        exit_code: Option<i32>,
        death_proof: bool,
    ) -> io::Result<()> {
        let EvidenceIdentity::LeadConversation { conversation_id } = &self.identity else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workflow evidence cannot finish a lead conversation",
            ));
        };
        if !self.is_healthy() {
            return Err(io::Error::other("the conversation evidence is incomplete"));
        }
        if !death_proof {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a conversation cannot finish without GroupProof::Dead",
            ));
        }
        let _guard = self.receipt.lock().await;
        let mut document = self.read_conversation()?;
        if document.id != *conversation_id || document.complete {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the conversation receipt does not match the active conversation",
            ));
        }
        for number in 1..=document.attempts {
            let mut turn = self.read_turn(number)?;
            if matches!(turn.state, AttemptState::Sending | AttemptState::Delivered) {
                turn.state = AttemptState::Cancelled;
                turn.ended_at = Some(now_ms());
                turn.failure_kind = Some(EvidenceFailureKind::Cancelled);
                self.write_turn_document(number, &turn).await?;
                document.state = ConversationState::Cancelled;
                document.failure_kind = Some(EvidenceFailureKind::Cancelled);
            }
        }
        document.complete = true;
        document.ended_at = Some(now_ms());
        document.exit_code = exit_code;
        document.death_proof = true;
        if document.state == ConversationState::Active {
            document.state = ConversationState::Closed;
        }
        self.write_conversation(&document).await
    }

    fn read_conversation(&self) -> io::Result<ConversationDocument> {
        read_private_json(&self.anchor, &self.root.join("conversation.json"))
    }

    async fn write_conversation(&self, document: &ConversationDocument) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(document).map_err(io::Error::other)?;
        replace_json(&self.anchor, &self.root.join("conversation.json"), &bytes).await
    }

    fn read_turn(&self, number: usize) -> io::Result<TurnDocument> {
        read_private_json(
            &self.anchor,
            &self.root.join("turns").join(format!("{number:04}.json")),
        )
    }

    async fn write_turn_document(&self, number: usize, turn: &TurnDocument) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(turn).map_err(io::Error::other)?;
        replace_json(
            &self.anchor,
            &self.root.join("turns").join(format!("{number:04}.json")),
            &bytes,
        )
        .await
    }

    async fn open_inner(&self) -> io::Result<EvidenceStreams> {
        self.prepare_directories().await?;
        self.write_manifest().await?;
        let stdout =
            EvidenceWriter::append(&self.anchor, &self.stdout_path(), Arc::clone(&self.healthy))?;
        let stderr =
            EvidenceWriter::append(&self.anchor, &self.stderr_path(), Arc::clone(&self.healthy))?;
        Ok(EvidenceStreams { stdout, stderr })
    }

    async fn write_manifest(&self) -> io::Result<()> {
        validate_manifest(&self.input)?;
        let bytes = serde_json::to_vec_pretty(&self.input).map_err(io::Error::other)?;
        let path = self.input_path();
        match open_private(&self.anchor, &path, PrivateFileAccess::Read) {
            Ok(mut file) => {
                let mut recorded = Vec::new();
                file.read_to_end(&mut recorded)?;
                if recorded == bytes {
                    return Ok(());
                }
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "the evidence manifest already exists with different safe facts",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        write_new_json(&self.anchor, &path, &bytes).await
    }

    async fn prepare_directories(&self) -> io::Result<()> {
        require_directory(&self.anchor).await?;
        match &self.identity {
            EvidenceIdentity::WorkflowStep { step_id } => {
                if self.root != self.anchor {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "the workflow evidence root does not match its run directory",
                    ));
                }
                require_plain_name(step_id)?;
            }
            EvidenceIdentity::Reflection => {
                todo!("T-126: validate the reflection target under its run directory")
            }
            EvidenceIdentity::LeadConversation { conversation_id } => {
                let loadout = safe_child(&self.anchor, ".loadout")?;
                ensure_directory(&loadout).await?;
                let conversations = safe_child(&loadout, "conversations")?;
                ensure_directory(&conversations).await?;
                let expected = safe_child(&conversations, &conversation_id.to_string())?;
                if self.root != expected {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "the lead evidence identity does not match its directory",
                    ));
                }
                ensure_directory(&self.root).await?;
            }
        }
        ensure_directory(&self.root.join(LOGS_DIR)).await
    }

    /// Jedna porazka jest nieodwracalna dla tego artefaktu.
    pub fn mark_incomplete(&self) {
        self.healthy.store(false, Ordering::Release);
    }

    /// `true` znaczy tylko „zapis dotad byl zdrowy". Dowod `Dead` doklada warstwa rozmowy.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConversationDocument {
    schema_version: u8,
    id: Uuid,
    vendor: ConversationVendor,
    model_configured: bool,
    state: ConversationState,
    complete: bool,
    created_at: i64,
    started_at: i64,
    ended_at: Option<i64>,
    attempts: usize,
    turns: usize,
    failure_kind: Option<EvidenceFailureKind>,
    exit_code: Option<i32>,
    death_proof: bool,
    agent_turns: u64,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TurnDocument {
    schema_version: u8,
    attempt: usize,
    state: AttemptState,
    started_at: i64,
    delivered_at: Option<i64>,
    ended_at: Option<i64>,
    prompt_bytes: usize,
    images: Vec<ImageFact>,
    failure_kind: Option<EvidenceFailureKind>,
    turns: u64,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
}

/// Dwa niezalezne ujscia jednego procesu. Czytelnicy stdout i stderr moga je posiadac osobno.
#[derive(Debug)]
pub struct EvidenceStreams {
    pub stdout: EvidenceWriter,
    pub stderr: EvidenceWriter,
}

/// Append-only plik, ktory przy pierwszej porazce zatruwa wspolny stan targetu.
#[derive(Debug)]
pub struct EvidenceWriter {
    file: tokio::fs::File,
    healthy: Arc<AtomicBool>,
}

impl EvidenceWriter {
    fn append(anchor: &Path, path: &Path, healthy: Arc<AtomicBool>) -> io::Result<Self> {
        let file = match open_private(anchor, path, PrivateFileAccess::Append) {
            Ok(file) => tokio::fs::File::from_std(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match open_private(anchor, path, PrivateFileAccess::CreateAppend) {
                    Ok(file) => tokio::fs::File::from_std(file),
                    // Inna instancja mogła utworzyć plik między dwiema atomowymi próbami. Nadal
                    // otwieramy go przez ten sam no-follow/owner-only kontrakt.
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        tokio::fs::File::from_std(open_private(
                            anchor,
                            path,
                            PrivateFileAccess::Append,
                        )?)
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        Ok(Self { file, healthy })
    }

    pub async fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let result = self.file.write_all(bytes).await;
        if result.is_err() {
            self.healthy.store(false, Ordering::Release);
        }
        result
    }

    pub async fn close(mut self) -> io::Result<()> {
        let result = async {
            self.file.flush().await?;
            self.file.sync_all().await
        }
        .await;
        if result.is_err() {
            self.healthy.store(false, Ordering::Release);
        }
        result
    }
}

fn open_private(
    anchor: &Path,
    path: &Path,
    access: PrivateFileAccess,
) -> io::Result<std::fs::File> {
    let relative = path.strip_prefix(anchor).map_err(|_error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a private evidence path escaped its anchor",
        )
    })?;
    supervisor::open_private_file(anchor, relative, access)
}

fn read_private_json<T: for<'de> Deserialize<'de>>(anchor: &Path, path: &Path) -> io::Result<T> {
    let mut file = open_private(anchor, path, PrivateFileAccess::Read)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn safe_child(parent: &Path, name: &str) -> io::Result<PathBuf> {
    require_plain_name(name)?;
    Ok(parent.join(name))
}

fn require_plain_name(name: &str) -> io::Result<()> {
    let mut parts = Path::new(name).components();
    if name.is_empty()
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an evidence path component is not a plain relative name",
        ));
    }
    Ok(())
}

async fn require_directory(path: &Path) -> io::Result<()> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an evidence directory is not a real directory",
        ));
    }
    Ok(())
}

async fn ensure_directory(path: &Path) -> io::Result<()> {
    match tokio::fs::create_dir(path).await {
        Ok(()) => require_directory(path).await,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => require_directory(path).await,
        Err(error) => Err(error),
    }
}

fn validate_manifest(input: &SafeInputManifest) -> io::Result<()> {
    for source in &input.context {
        let reference = Path::new(&source.reference);
        if source.reference.is_empty()
            || reference.is_absolute()
            || !reference
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a context reference is not a safe relative path",
            ));
        }
    }
    for image in &input.images {
        if !matches!(
            image.mime.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an image fact has a MIME outside the closed allowlist",
            ));
        }
    }
    Ok(())
}

async fn write_new_json(anchor: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_json(anchor, path, bytes, false).await
}

async fn replace_json(anchor: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_json(anchor, path, bytes, true).await
}

/// Zapisuje mały, bezpieczny JSON przez losowy plik 0600 w tym samym katalogu.
///
/// Całe rozwiązanie ścieżki, kontrola istniejącego celu, zapis i publikacja odbywają się przez
/// jeden deskryptor katalogu w supervisorze. Dzięki temu podmiana rodzica na symlink między
/// sprawdzeniem i `rename` nie może przekierować prywatnych bajtów do sąsiedniego workspace.
async fn atomic_json(anchor: &Path, path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let relative = path.strip_prefix(anchor).map_err(|_error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a private evidence document escaped its anchor",
        )
    })?;
    let anchor = anchor.to_path_buf();
    let relative = relative.to_path_buf();
    let bytes = bytes.to_vec();
    tokio::task::spawn_blocking(move || {
        PrivateFilePublisher::open(&anchor, &relative)?.publish(&bytes, replace)
    })
    .await
    .map_err(io::Error::other)?
}

impl fmt::Debug for EvidenceTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Absolutny root jest prywatny i nie wchodzi do zwyklego dziennika ani support reportu.
        formatter
            .debug_struct("EvidenceTarget")
            .field("root", &"<private workspace path>")
            .field("identity", &self.identity)
            .field("input", &self.input)
            // `anchor` jest absolutna sciezka, a `healthy` szczegolem synchronizacji. Celowe
            // pominiecie nazywamy w typie, zamiast pozwalac clippy uznac je za przeoczenie.
            .finish_non_exhaustive()
    }
}
