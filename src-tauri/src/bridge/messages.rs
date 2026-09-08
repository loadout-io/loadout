//! WF-11: jeden właściciel per run; host wiąże skrzynkę, model podaje tylko adres celu.
//! Zapis nie budzi kroku, nie zwalnia zależności i nie jest dowodem przeczytania wiadomości.

use super::{Answer, Call, host::Answers};
use crate::{
    commands::{lead_start::RunRef, processes::services::ServiceAccess},
    durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE},
    engine::{
        line::Line,
        supervisor::{PrivateFileModePolicy, PublicationEntryKind, PublicationRoot},
    },
    ipc::LineSink,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};
use tokio_util::sync::CancellationToken;

const MESSAGE_BYTES: usize = 16 * 1024;
const MESSAGE_COUNT: usize = 1000;
const TOTAL_BYTES: usize = 8 * 1024 * 1024;
const PAGE_COUNT: usize = 100;
const FILE_BYTES: usize = MESSAGE_BYTES * 6 + 16 * 1024;
const HEADER_BYTES: usize = 16 * 1024;
const INDEX: &str = "messages/index.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Peer {
    run_id: String,
    node_key: String,
    attempt: String,
}

#[derive(Clone)]
struct Member {
    peer: Peer,
    name: String,
    scope: Option<String>,
    expires: CancellationToken,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Message {
    sequence: u64,
    client_id: String,
    from: Peer,
    to: Peer,
    from_name: String,
    to_name: String,
    text: String,
    at: String,
}
impl Message {
    fn line(&self) -> Line {
        Line::MessageStored {
            agent: self.from_name.clone(),
            text: format!("{} stored a message for {}.", self.from_name, self.to_name),
            run_id: self.from.run_id.clone(),
            sequence: self.sequence,
            from_node: self.from.node_key.clone(),
            to_node: self.to.node_key.clone(),
            body: self.text.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    schema: u8,
    run: RunRef,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMessage {
    run: RunRef,
    message: Message,
}
struct Index {
    run: RunRef,
    messages: Vec<Message>,
}
struct State {
    index: Index,
    members: BTreeMap<String, Member>,
    bytes: usize,
    storage_failed: bool,
    root: Option<PublicationRoot>,
    messages_root: Option<PublicationRoot>,
}

pub(crate) struct Mailbox {
    run_dir: PathBuf,
    /// Krótkie operacje i synchroniczny bezpieczny zapis, nigdy await pod tym zamkiem.
    state: Mutex<State>,
    lines: LineSink,
}
impl fmt::Debug for Mailbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox").finish_non_exhaustive()
    }
}
impl Mailbox {
    pub(crate) fn new(run: RunRef, run_dir: PathBuf, lines: LineSink) -> Self {
        Self {
            run_dir,
            lines,
            state: Mutex::new(State {
                index: Index {
                    run,
                    messages: Vec::new(),
                },
                members: BTreeMap::new(),
                bytes: 0,
                storage_failed: false,
                root: None,
                messages_root: None,
            }),
        }
    }
    pub(crate) fn join(
        self: &Arc<Self>,
        node_key: String,
        name: &str,
        scope: Option<String>,
        expires: CancellationToken,
    ) -> Result<Messages, String> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state
            .members
            .get(&node_key)
            .is_some_and(|member| !member.expires.is_cancelled())
        {
            return Err("Messages are already connected for this exact step.".to_owned());
        }
        let peer = Peer {
            run_id: state.index.run.run_id.clone(),
            node_key: node_key.clone(),
            attempt: uuid::Uuid::now_v7().to_string(),
        };
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        state.members.insert(
            node_key,
            Member {
                peer: peer.clone(),
                name: name.clone(),
                scope,
                expires,
            },
        );
        Ok(Messages {
            mailbox: Arc::clone(self),
            peer,
            name,
        })
    }
    fn persist(&self, state: &mut State, message: &Message) -> Result<(), String> {
        if state.storage_failed {
            return Err(storage(
                "an earlier write failed; start a new run to send more messages",
            ));
        }
        let result = self.publish_message(state, message);
        // Po niepewnym błędzie fsync/rename plik mógł już powstać. Nie nadpisujemy tego
        // numeru ani nie wystawiamy ACK z pamięci; historia odczyta tylko całe trwałe pliki.
        if result.is_err() {
            state.storage_failed = true;
        }
        result
    }
    fn publish_message(&self, state: &mut State, message: &Message) -> Result<(), String> {
        let publisher = DurableFilePublisher::new(&self.run_dir);
        if state.root.is_none() {
            let root = PublicationRoot::open(&self.run_dir).map_err(storage)?;
            root.ensure_directory(Path::new("messages"), 0o700)
                .map_err(storage)?;
            let messages_root =
                PublicationRoot::open(&self.run_dir.join("messages")).map_err(storage)?;
            root.validate_path_identity(&self.run_dir)
                .map_err(storage)?;
            let header = serde_json::to_vec(&Header {
                schema: 1,
                run: state.index.run.clone(),
            })
            .map_err(storage)?;
            if header.len() > HEADER_BYTES {
                return Err(storage("the saved run address is too long"));
            }
            publisher
                .atomic_create_if_absent(
                    &self.run_dir.join(INDEX),
                    &header,
                    ModePolicy::Exact(PRIVATE_FILE_MODE),
                )
                .map_err(storage)?;
            state.root = Some(root);
            state.messages_root = Some(messages_root);
        }
        let root = state
            .root
            .as_ref()
            .ok_or_else(|| storage("the saved run is unavailable"))?;
        root.validate_path_identity(&self.run_dir)
            .map_err(storage)?;
        let messages_root = state
            .messages_root
            .as_ref()
            .ok_or_else(|| storage("the saved message folder is unavailable"))?;
        messages_root
            .validate_path_identity(&self.run_dir.join("messages"))
            .map_err(storage)?;
        let bytes = serde_json::to_vec(&StoredMessage {
            run: state.index.run.clone(),
            message: message.clone(),
        })
        .map_err(storage)?;
        if bytes.len() > FILE_BYTES {
            return Err(storage("the saved message file is full"));
        }
        // 2026-09-06: pomiar 8 MiB przekroczył 60 s, bo każdy send przepisywał całą
        // historię (~2 GiB). Niezmienny plik per wiadomość daje liniowy koszt zapisu;
        // indeks jest tylko pamięciowym odczytem tych plików, nie drugim źródłem prawdy.
        publisher
            .atomic_create_if_absent(
                &self.run_dir.join(message_path(message.sequence)),
                &bytes,
                ModePolicy::Exact(PRIVATE_FILE_MODE),
            )
            .map_err(storage)?;
        root.validate_path_identity(&self.run_dir)
            .map_err(storage)?;
        messages_root
            .validate_path_identity(&self.run_dir.join("messages"))
            .map_err(storage)?;
        Ok(())
    }
}
fn message_path(sequence: u64) -> PathBuf {
    PathBuf::from(format!("messages/{sequence:06}.json"))
}
fn storage(error: impl fmt::Display) -> String {
    format!("The message could not be stored safely: {error}. It was not reported as stored.")
}

pub(crate) struct Messages {
    mailbox: Arc<Mailbox>,
    peer: Peer,
    name: String,
}
impl fmt::Debug for Messages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Messages")
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Send {
    to: Peer,
    client_id: String,
    text: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadMessages {
    #[serde(default)]
    after_sequence: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
impl Messages {
    fn member<'a>(&self, state: &'a State) -> Result<&'a Member, String> {
        state
            .members
            .get(&self.peer.node_key)
            .filter(|one| one.peer == self.peer && !one.expires.is_cancelled())
            .ok_or_else(|| "This step has finished. Another try has its own messages.".to_owned())
    }
    fn dispatch(&self, call: &Call) -> Result<Value, String> {
        let mut state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let sender = self.member(&state)?.clone();
        match call.call.as_str() {
            "list_peers" => {
                let _: Empty = serde_json::from_value(call.input.clone()).map_err(|_| {
                    "The host chooses this run and sender; do not supply another identity."
                        .to_owned()
                })?;
                let peers: Vec<_> = state
                    .members
                    .values()
                    .filter(|one| {
                        one.peer != self.peer
                            && one.scope == sender.scope
                            && !one.expires.is_cancelled()
                    })
                    .map(|one| json!({"peer":one.peer,"name":one.name}))
                    .collect();
                Ok(json!({"peers":peers}))
            }
            "read_messages" => {
                let input:ReadMessages=serde_json::from_value(call.input.clone()).map_err(|_|"Read only this step's messages. Choosing a different recipient or run is not allowed.".to_owned())?;
                let mut found = state
                    .index
                    .messages
                    .iter()
                    .filter(|one| one.to == self.peer && one.sequence > input.after_sequence);
                let page: Vec<_> = found.by_ref().take(PAGE_COUNT).collect();
                let more = found.next().is_some();
                let after = page.last().map_or(input.after_sequence, |one| one.sequence);
                Ok(json!({"messages":page,"afterSequence":after,"more":more}))
            }
            "send_message" => {
                let input:Send=serde_json::from_value(call.input.clone()).map_err(|_|"Choose a listed recipient, a message ID and some text. Loadout chooses the sender and run.".to_owned())?;
                if input.client_id.trim().is_empty() || input.client_id.len() > 128 {
                    return Err("Use a nonempty message ID of at most 128 bytes.".to_owned());
                }
                if input.text.trim().is_empty() || input.text.len() > MESSAGE_BYTES {
                    return Err("A message must contain text and fit within 16 KiB. Nothing was shortened or stored.".to_owned());
                }
                // Deduplikacja poprzedza żywotność adresata: ponowiony ACK nadal opisuje TEN
                // utrwalony wpis, nie wysyła drugi raz do zakończonej lub przyszłej próby.
                if let Some(previous) = state.index.messages.iter().find(|one| {
                    one.from.node_key == self.peer.node_key && one.client_id == input.client_id
                }) {
                    if previous.to != input.to || previous.text != input.text {
                        return Err("This message ID already names different text or a different recipient. Use a new message ID.".to_owned());
                    }
                    return Ok(
                        json!({"status":"stored","sequence":previous.sequence,"said":"This exact message was already stored. This does not mean it was read."}),
                    );
                }
                let recipient=state.members.get(&input.to.node_key).filter(|one|one.peer==input.to && one.peer.run_id==self.peer.run_id
                    && one.scope==sender.scope && one.peer!=self.peer && !one.expires.is_cancelled()).cloned()
                    .ok_or_else(||"That exact recipient is unavailable in this run and context. Nothing was sent to another copy or try.".to_owned())?;
                if state.index.messages.len() >= MESSAGE_COUNT
                    || state.bytes.saturating_add(input.text.len()) > TOTAL_BYTES
                {
                    return Err("This run's message storage is full: at most 1000 messages and 8 MiB of text. Nothing was shortened or stored.".to_owned());
                }
                let message = Message {
                    sequence: state.index.messages.len() as u64 + 1,
                    client_id: input.client_id,
                    from: self.peer.clone(),
                    to: recipient.peer,
                    from_name: sender.name,
                    to_name: recipient.name,
                    text: input.text,
                    at: crate::commands::now_utc(),
                };
                self.mailbox.persist(&mut state, &message)?;
                state.bytes += message.text.len();
                state.index.messages.push(message.clone());
                let _ = self.mailbox.lines.send(message.line());
                Ok(
                    json!({"status":"stored","sequence":message.sequence,"said":"The message is stored. This does not mean it was read."}),
                )
            }
            _ => Err("This step cannot use that message action.".to_owned()),
        }
    }
    fn answer(&self, call: &Call) -> Answer {
        match self.dispatch(call) {
            Ok(value) => Answer::Ok(value),
            Err(text) => {
                let _ = self.mailbox.lines.send(Line::Problem {
                    agent: self.name.clone(),
                    text: text.clone(),
                    resets_at: None,
                });
                Answer::Refused(text)
            }
        }
    }
}

/// Kompozycja uprawnień nad jednym istniejącym hostem, nie kolejny most ani control plane.
pub(crate) struct StepDesk {
    pub services: Option<Arc<ServiceAccess>>,
    pub messages: Option<Messages>,
    pub context: Option<super::context::ContextDesk>,
}
impl fmt::Debug for StepDesk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StepDesk")
            .field("messages", &self.messages.is_some())
            .field("services", &self.services.is_some())
            .field("context", &self.context.is_some())
            .finish()
    }
}
impl StepDesk {
    pub(crate) fn tools(&self) -> Value {
        let mut tools = self
            .services
            .as_ref()
            .and_then(|one| one.tools().as_array().cloned())
            .unwrap_or_default();
        if self.messages.is_some() {
            /* PRZEZ `Verb::listed`, a nie własnym `json!` (2026-09-08, CT-03a): definicja
             * narzędzia powstaje w jednym miejscu, więc adnotacja o czasowniku tylko czytającym
             * dojeżdża i tutaj. Bez niej `codex exec` odbija KAŻDE wywołanie kroku zdaniem
             * o zatwierdzaniu — a to krok, nie lider, jest agentem biegu. */
            tools.extend(
                super::verbs::message_tools()
                    .iter()
                    .map(super::verbs::Verb::listed),
            );
        }
        if let Some(context) = &self.context {
            tools.extend(context.tools().as_array().into_iter().flatten().cloned());
        }
        json!(tools)
    }
}
#[async_trait]
impl Answers for StepDesk {
    async fn answer(&self, call: Call) -> Answer {
        if matches!(
            call.call.as_str(),
            "list_peers" | "send_message" | "read_messages"
        ) {
            return self.messages.as_ref().map_or_else(
                || Answer::Refused("Messages are not enabled for this step.".to_owned()),
                |one| one.answer(&call),
            );
        }
        if super::context::is_context_tool(&call.call) {
            return match &self.context {
                Some(context) => context.answer(call).await,
                None => Answer::Refused(
                    "Context is not available to this step. Nothing from another step was read."
                        .to_owned(),
                ),
            };
        }
        match &self.services {
            Some(services) => services.answer(call).await,
            None => Answer::Refused("This step cannot use that action.".to_owned()),
        }
    }
}

/// Historyczny czytelnik nie potrzebuje runtime, bazy ani dawnych uchwytów sesji.
pub(crate) fn history(
    project: &Path,
    run_dir: &Path,
    run_id: &str,
    node_key: &str,
) -> io::Result<Vec<Line>> {
    let root = PublicationRoot::open(run_dir)?;
    let mut opened =
        match root.open_private_existing(Path::new(INDEX), PrivateFileModePolicy::ExactOwnerOnly) {
            Ok(Some(opened)) => opened,
            Ok(None) => return Ok(Vec::new()),
            Err(crate::engine::supervisor::PrivateLeafError::Io(error))
                if error.kind() == io::ErrorKind::NotFound =>
            {
                return Ok(Vec::new());
            }
            Err(error) => return Err(io::Error::other(error)),
        };
    let mut bytes = Vec::new();
    opened
        .file_mut()
        .take(HEADER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > HEADER_BYTES {
        return Err(io::Error::other("the saved message address is too large"));
    }
    let header: Header = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if header.schema != 1 || header.run.workspace != project || header.run.run_id != run_id {
        return Err(io::Error::other(
            "the message history does not belong to this run",
        ));
    }
    let messages_root = PublicationRoot::open(&run_dir.join("messages"))?;
    let mut entries = root.list_directory(Path::new("messages"))?;
    entries.retain(|one| one.name != "index.json");
    if entries.len() > MESSAGE_COUNT {
        return Err(io::Error::other(
            "the message history exceeds its count limit",
        ));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut total = 0usize;
    let mut lines = Vec::new();
    for (position, entry) in entries.iter().enumerate() {
        let relative = message_path(position as u64 + 1);
        if entry.kind != PublicationEntryKind::Regular
            || relative.file_name() != Some(entry.name.as_os_str())
        {
            return Err(io::Error::other(
                "the saved message list is incomplete or has been replaced",
            ));
        }
        let mut file = root
            .open_private_existing(&relative, PrivateFileModePolicy::ExactOwnerOnly)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("a saved message is missing"))?;
        let mut bytes = Vec::new();
        file.file_mut()
            .take(FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > FILE_BYTES {
            return Err(io::Error::other("a saved message exceeds its size limit"));
        }
        let saved: StoredMessage = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        let message = saved.message;
        total = total.saturating_add(message.text.len());
        if saved.run != header.run
            || message.sequence != position as u64 + 1
            || message.text.len() > MESSAGE_BYTES
            || total > TOTAL_BYTES
            || message.from.run_id != run_id
            || message.to.run_id != run_id
        {
            return Err(io::Error::other(
                "a saved message does not belong to this run or exceeds its limits",
            ));
        }
        if !root
            .validate_private_identity(&relative, file.identity())
            .map_err(io::Error::other)?
        {
            return Err(io::Error::other(
                "a saved message changed while it was read",
            ));
        }
        if message.from.node_key == node_key {
            lines.push(message.line());
        }
    }
    root.validate_path_identity(run_dir)?;
    messages_root.validate_path_identity(&run_dir.join("messages"))?;
    if !root
        .validate_private_identity(Path::new(INDEX), opened.identity())
        .map_err(io::Error::other)?
    {
        return Err(io::Error::other(
            "the message history changed while it was read",
        ));
    }
    Ok(lines)
}
