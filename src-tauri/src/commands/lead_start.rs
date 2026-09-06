//! WF-07: jednorazowe przekazanie Startu z mostu przez okno do istniejącego wykonawcy.
//!
//! 2026-09 — to nie scheduler. Rejestr pamięta wyłącznie odbiór i odpowiedź jednego
//! żądania; `AppState` nadal posiada bieg, a run.json jego trwałą tożsamość.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::watch;
use tokio::time::Instant;
use uuid::Uuid;

/// Tożsamość biegu, nigdy nazwa ostatniego katalogu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRef {
    pub workspace: PathBuf,
    pub run_id: String,
}

/// Addytywny zapis w run.json. Nie zawiera tekstu zadania ani sekretów.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeadStartOrigin {
    pub request_id: String,
    pub conversation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReceipt {
    pub request_id: String,
    pub run: RunRef,
}

/// Rozstrzygnięty raz przez hosta. Front otrzymuje tylko ID i plan do wyświetlenia.
#[derive(Clone)]
pub struct StartRequest {
    pub origin: LeadStartOrigin,
    pub workspace: PathBuf,
    pub workflow: PathBuf,
    pub revision: String,
    pub task: Option<String>,
    pub replay: Option<Arc<super::replay::ReplayMaterial>>,
    pub restore: Option<Arc<super::result_restore::ResultMaterial>>,
}

impl fmt::Debug for StartRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 2026-09: zadanie jest wejściem modelu, więc Debug nie może stać się jego archiwum.
        f.debug_struct("StartRequest")
            .field("origin", &self.origin)
            .field("workspace", &self.workspace)
            .field("workflow", &self.workflow)
            .field("has_revision", &!self.revision.is_empty())
            .field("has_task", &self.task.is_some())
            .finish_non_exhaustive()
    }
}

/// Wąski szew wykonawcy: potwierdzenie dopiero po trwałym przygotowaniu.
#[derive(Clone)]
pub struct LeadStart {
    pub origin: LeadStartOrigin,
    pub expected_revision: String,
    pub replay: Option<Arc<super::replay::ReplayMaterial>>,
    requests: Arc<LeadStarts>,
}

impl fmt::Debug for LeadStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Rewizja to odwracalne base64 źródła, nie nieszkodliwy hash (WF-07, 2026-09).
        f.debug_struct("LeadStart")
            .field("origin", &self.origin)
            .field("has_revision", &!self.expected_revision.is_empty())
            .field("has_replay", &self.replay.is_some())
            .finish_non_exhaustive()
    }
}

impl LeadStart {
    #[must_use]
    pub fn new(
        requests: Arc<LeadStarts>,
        origin: LeadStartOrigin,
        expected_revision: String,
    ) -> Self {
        Self {
            origin,
            requests,
            expected_revision,
            replay: None,
        }
    }

    /// Woła tylko wspólna droga prepare → graph. Utrata odbiorcy nie anuluje biegu.
    pub fn prepared(&self, workspace: &Path, run_id: &str) {
        self.requests.finish(
            &self.origin.request_id,
            Ok(StartReceipt {
                request_id: self.origin.request_id.clone(),
                run: RunRef {
                    workspace: workspace.to_path_buf(),
                    run_id: run_id.to_owned(),
                },
            }),
        );
    }
}

/// Drop przed prepare musi obudzić most; po ack odmowa nie nadpisuje przyjęcia.
#[derive(Debug)]
pub struct AcceptanceGuard {
    requests: Arc<LeadStarts>,
    request_id: String,
}

impl AcceptanceGuard {
    #[must_use]
    pub fn new(requests: Arc<LeadStarts>, request_id: String) -> Self {
        Self {
            requests,
            request_id,
        }
    }
}

impl Drop for AcceptanceGuard {
    fn drop(&mut self) {
        self.requests.refuse(
            &self.request_id,
            "Nothing started: accepting this request was interrupted. Ask to start it again."
                .to_owned(),
        );
    }
}

#[derive(Clone)]
enum Phase {
    Preview,
    Consumed,
    Pending,
    Claimed,
    Finished(Result<StartReceipt, String>),
}

struct Entry {
    request: Option<StartRequest>,
    deadline: Instant,
    phase: Phase,
    changed: watch::Sender<()>,
}

pub struct LeadStarts {
    instance: Uuid,
    transport_timeout: Duration,
    /// `std::sync::Mutex` nigdy nie przeżywa await: tylko lookup/zmiana fazy (niezmiennik 8).
    entries: Mutex<HashMap<String, Entry>>,
}

impl fmt::Debug for LeadStarts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LeadStarts")
            .field("instance", &self.instance)
            .field("transport_timeout", &self.transport_timeout)
            .field(
                "requests",
                &self
                    .entries
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .len(),
            )
            .finish()
    }
}

impl Default for LeadStarts {
    fn default() -> Self {
        Self::with_transport_timeout(Duration::from_secs(30))
    }
}

const EXPIRED: &str =
    "Nothing started: the window did not accept this request in time. Ask to start it again.";
const UNKNOWN: &str = "Nothing started: this request belongs to an earlier app session or is no longer available. Ask to start it again.";

impl LeadStarts {
    #[must_use]
    pub(crate) fn ui_identity(&self) -> Uuid {
        self.instance
    }
    /// Jawny zegar pozwala sprawdzić produkcyjną granicę bez trzydziestu sekund testu.
    #[must_use]
    pub fn with_transport_timeout(transport_timeout: Duration) -> Self {
        Self {
            instance: Uuid::now_v7(),
            transport_timeout,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(
        &self,
        conversation_id: String,
        workspace: PathBuf,
        workflow: PathBuf,
        revision: String,
        task: Option<String>,
    ) -> Result<StartRequest, String> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        // Skończony rozmiar sesji, bez wyrzucania starych ID i osłabiania at-most-once.
        if entries.len() >= 4096 {
            return Err("Nothing started: this app session has accepted too many requests. Restart Loadout before asking again.".to_owned());
        }
        let origin = LeadStartOrigin {
            request_id: format!("{}:{}", self.instance, Uuid::now_v7()),
            conversation_id,
        };
        let request = StartRequest {
            origin,
            workspace,
            workflow,
            revision,
            task,
            replay: None,
            restore: None,
        };
        let (changed, _) = watch::channel(());
        entries.insert(
            request.origin.request_id.clone(),
            Entry {
                request: Some(request.clone()),
                deadline: Instant::now() + self.transport_timeout,
                phase: Phase::Pending,
                changed,
            },
        );
        Ok(request)
    }

    /// Jedyny atomowy wybór Pending → Claimed albo Expired. None oznacza duplikat.
    pub fn claim(&self, request_id: &str) -> Result<Option<StartRequest>, String> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries
            .get_mut(request_id)
            .ok_or_else(|| UNKNOWN.to_owned())?;
        if matches!(entry.phase, Phase::Pending) && Instant::now() >= entry.deadline {
            entry.phase = Phase::Finished(Err(EXPIRED.to_owned()));
            entry.request = None;
            entry.changed.send_replace(());
        }
        match &entry.phase {
            Phase::Preview | Phase::Consumed => {
                Err("Nothing started: this preview is not an accepted run request.".to_owned())
            }
            Phase::Pending => {
                entry.phase = Phase::Claimed;
                entry.changed.send_replace(());
                Ok(entry.request.take())
            }
            Phase::Claimed | Phase::Finished(Ok(_)) => Ok(None),
            Phase::Finished(Err(said)) => Err(said.clone()),
        }
    }

    pub fn refuse(&self, request_id: &str, said: String) {
        self.finish(request_id, Err(said));
    }

    /// WF-23: ten sam rejestr i późniejszy request ID. Podgląd nie zajmuje miejsca biegu.
    pub fn preview(
        &self,
        conversation: String,
        material: Arc<super::replay::ReplayMaterial>,
    ) -> Result<StartRequest, String> {
        let mut request = self.register(
            conversation,
            material.source.workspace.clone(),
            material.workflow.clone(),
            material.workflow_revision.clone(),
            material.task.clone(),
        )?;
        request.replay = Some(material);
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries
            .get_mut(&request.origin.request_id)
            .ok_or_else(|| UNKNOWN.to_owned())?;
        entry.request = Some(request.clone());
        entry.phase = Phase::Preview;
        entry.deadline = Instant::now() + Duration::from_mins(5);
        Ok(request)
    }

    pub fn preview_request(
        &self,
        id: &str,
        conversation: &str,
        project: &Path,
    ) -> Result<StartRequest, String> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.get_mut(id).ok_or_else(|| UNKNOWN.to_owned())?;
        if matches!(entry.phase, Phase::Preview) && Instant::now() >= entry.deadline {
            entry.phase = Phase::Finished(Err(
                "This repeat preview expired. Review it again; nothing started.".to_owned(),
            ));
            entry.request = None;
            entry.changed.send_replace(());
        }
        let request = entry
            .request
            .as_ref()
            .filter(|request| {
                matches!(entry.phase, Phase::Preview)
                    && request.origin.conversation_id == conversation
                    && request.workspace == project
            })
            .ok_or_else(|| {
                "This repeat preview is no longer available for this conversation. Review it again."
                    .to_owned()
            })?;
        Ok(request.clone())
    }

    pub fn restore_preview(
        self: &Arc<Self>,
        conversation: String,
        material: Arc<super::result_restore::ResultMaterial>,
    ) -> Result<StartRequest, String> {
        let mut request = self.register(
            conversation,
            material.source.workspace.clone(),
            PathBuf::new(),
            String::new(),
            None,
        )?;
        request.restore = Some(material);
        {
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            let entry = entries
                .get_mut(&request.origin.request_id)
                .ok_or_else(|| UNKNOWN.to_owned())?;
            entry.request = Some(request.clone());
            entry.phase = Phase::Preview;
            entry.deadline = Instant::now() + Duration::from_mins(5);
        }
        // Preview chroni pliki tylko przez swoje życie. Weak nie podtrzymuje starej instancji.
        let owner = Arc::downgrade(self);
        let id = request.origin.request_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_mins(5)).await;
            if let Some(owner) = owner.upgrade() {
                let mut entries = owner.entries.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(entry) = entries.get_mut(&id)
                    && matches!(entry.phase, Phase::Preview)
                {
                    entry.phase = Phase::Consumed;
                    entry.request = None;
                    entry.changed.send_replace(());
                }
            }
        });
        Ok(request)
    }

    pub fn consume_restore(
        &self,
        id: &str,
        conversation: &str,
        project: &Path,
    ) -> Result<StartRequest, String> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.get_mut(id).ok_or_else(|| UNKNOWN.to_owned())?;
        let request = entry
            .request
            .as_ref()
            .filter(|request| {
                matches!(entry.phase, Phase::Preview)
                    && Instant::now() < entry.deadline
                    && request.restore.is_some()
                    && request.origin.conversation_id == conversation
                    && request.workspace == project
            })
            .ok_or_else(|| {
                "This saved-file preview expired or was already used. Review it again.".to_owned()
            })?
            .clone();
        entry.request = None;
        entry.phase = Phase::Consumed;
        entry.changed.send_replace(());
        Ok(request)
    }

    /// Atomowo po zużyciu host-issued zgody. Dopiero teraz zaczyna się timeout transportu.
    pub fn activate_preview(
        &self,
        id: &str,
        conversation: &str,
        project: &Path,
    ) -> Result<StartRequest, String> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.get_mut(id).ok_or_else(|| UNKNOWN.to_owned())?;
        let request = entry
            .request
            .as_ref()
            .filter(|request| {
                matches!(entry.phase, Phase::Preview)
                    && request.restore.is_none()
                    && Instant::now() < entry.deadline
                    && request.origin.conversation_id == conversation
                    && request.workspace == project
            })
            .ok_or_else(|| {
                "This repeat preview expired or was already used. Review it again.".to_owned()
            })?
            .clone();
        entry.phase = Phase::Pending;
        entry.deadline = Instant::now() + self.transport_timeout;
        entry.changed.send_replace(());
        Ok(request)
    }

    pub async fn preview_expires(&self, id: &str) {
        let (deadline, mut changed) = {
            let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(entry) = entries.get(id) else {
                return;
            };
            (entry.deadline, entry.changed.subscribe())
        };
        tokio::select! { () = tokio::time::sleep_until(deadline) => {}, _ = changed.changed() => {} }
    }

    fn finish(&self, request_id: &str, result: Result<StartReceipt, String>) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = entries.get_mut(request_id) {
            // Odpowiedź jest monotoniczna. Późna porażka grafu nie cofa prawdziwego ack.
            if matches!(entry.phase, Phase::Finished(_)) {
                return;
            }
            if result.is_ok() && !matches!(entry.phase, Phase::Claimed) {
                return;
            }
            entry.phase = Phase::Finished(result);
            entry.request = None;
            entry.changed.send_replace(());
        }
    }

    /// Most czeka na przyjęcie, nie na koniec grafu. Po claim nie ma transportowego timeoutu.
    pub async fn wait(&self, request_id: &str) -> Result<StartReceipt, String> {
        let mut changed = {
            let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            entries
                .get(request_id)
                .ok_or_else(|| UNKNOWN.to_owned())?
                .changed
                .subscribe()
        };
        loop {
            let (phase, deadline) = {
                let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
                let entry = entries.get(request_id).ok_or_else(|| UNKNOWN.to_owned())?;
                (entry.phase.clone(), entry.deadline)
            };
            match phase {
                Phase::Preview | Phase::Consumed => {
                    return Err(
                        "Nothing started: this preview is not an accepted run request.".to_owned(),
                    );
                }
                Phase::Finished(result) => return result,
                Phase::Pending => {
                    if tokio::time::timeout_at(deadline, changed.changed())
                        .await
                        .is_err()
                    {
                        let mut entries =
                            self.entries.lock().unwrap_or_else(PoisonError::into_inner);
                        if let Some(entry) = entries.get_mut(request_id)
                            && matches!(entry.phase, Phase::Pending)
                        {
                            entry.phase = Phase::Finished(Err(EXPIRED.to_owned()));
                            entry.request = None;
                            entry.changed.send_replace(());
                        }
                    }
                }
                Phase::Claimed => {
                    changed.changed().await.map_err(|_| UNKNOWN.to_owned())?;
                }
            }
        }
    }
}
