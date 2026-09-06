//! WF-28: prawa sesji nad jednym rejestrem procesów. Brak uprawnienia jest domyślny.
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::Digest as _;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use super::{
    CopyLease, HeldProcess, Processes, ServiceOwner, ServiceOwnership, ServiceRef, ServiceState,
    StartedProcess, forget_if_current, launch, readiness,
};
use crate::bridge::host::Answers;
use crate::bridge::{Answer, Call};
use crate::engine::drivers::command::StartSpec;
use crate::engine::supervisor::{self, GroupProof, PublicationRoot, StepTag};
use crate::library::agents::{ServiceGrant, ServiceOperation};
use crate::workflow::{LaunchDescription, ServiceLifetime};

#[derive(Debug)]
pub(super) struct ManagedService {
    pub(super) description: LaunchDescription,
    tag: StepTag,
    /// Tylko podmiana krótkich wartości, nigdy przez await.
    pub(super) current: Mutex<Current>,
    mutation: AsyncMutex<()>,
    closed: CancellationToken,
}

#[derive(Debug)]
pub(super) struct Current {
    service: Arc<ServiceOwnership>,
    configured_lease: Option<CopyLease>,
    pub(super) process: Option<Arc<HeldProcess>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceView {
    pub service: ServiceRef,
    pub status: String,
    pub pgid: Option<i32>,
    pub cwd: PathBuf,
    pub readiness: Option<super::ServiceReadiness>,
    pub endpoints: Vec<super::ServiceEndpoint>,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i32>,
    #[serde(rename = "exitReason")]
    pub exit_reason: Option<String>,
}

impl ManagedService {
    fn view(&self) -> ServiceView {
        let current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        let record = current
            .service
            .record
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        ServiceView {
            service: record.reference.clone(),
            status: match record.state {
                ServiceState::Configured => "Configured, not started",
                ServiceState::Starting => "Starting",
                ServiceState::Running => "Running",
                ServiceState::Dead => "Stopped",
                ServiceState::Unproven | ServiceState::Unknown => {
                    "Could not confirm that it stopped"
                }
            }
            .to_owned(),
            pgid: record.pgid,
            cwd: record
                .process_cwd
                .clone()
                .unwrap_or_else(|| record.cwd.clone()),
            readiness: record.readiness.clone(),
            endpoints: record.endpoints.clone(),
            exit_code: record.exit_code,
            exit_reason: record.exit_reason.clone(),
        }
    }

    pub(super) fn release_if_configured(&self) {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        if current.process.is_none() && current.configured_lease.is_some() {
            current.service.state(ServiceState::Dead, None);
            if let Some(lease) = current.configured_lease.take() {
                lease.release();
            }
        }
    }
}

impl Processes {
    /// None jest anulowaniem, nie błędem: żaden status nie udaje wtedy gotowości.
    pub async fn start_service(
        &self,
        reference: &ServiceRef,
        cancel: &CancellationToken,
    ) -> io::Result<Option<ServiceView>> {
        let slot = self.managed_service(reference)?;
        {
            let _exclusive = slot.mutation.lock().await;
            if cancel.is_cancelled() || slot.closed.is_cancelled() {
                return Ok(None);
            }
            if let Err(why) = self.start_prepared(&slot, reference) {
                slot.release_if_configured();
                return Err(why);
            }
        }
        self.finish_service_start(&slot, reference, cancel).await
    }

    pub async fn restart_service(
        &self,
        reference: &ServiceRef,
        cancel: &CancellationToken,
    ) -> io::Result<Option<ServiceView>> {
        let slot = self.managed_service(reference)?;
        let next = {
            let _exclusive = slot.mutation.lock().await;
            if cancel.is_cancelled() || slot.closed.is_cancelled() {
                return Ok(None);
            }
            let (old, entry) = {
                let current = slot.current.lock().unwrap_or_else(PoisonError::into_inner);
                if current.service.owner.reference != *reference {
                    return Err(io::Error::other(
                        "this app reference is stale; read its current status before trying again",
                    ));
                }
                let entry = current
                    .process
                    .clone()
                    .ok_or_else(|| io::Error::other("this app has not started yet"))?;
                (Arc::clone(&current.service), entry)
            };
            let mut next = reference.clone();
            next.generation = next
                .generation
                .checked_add(1)
                .ok_or_else(|| io::Error::other("this app cannot start another instance"))?;
            let lease = self.next_copy_lease(&old.owner.cwd, reference, &next)?;
            let proof = entry.prove().await;
            if matches!(proof, Some(GroupProof::Alive { .. })) {
                lease.release();
                return Err(io::Error::other(
                    "the old app could not be proved stopped; no new app was started",
                ));
            }
            forget_if_current(&self.held, &entry);
            if cancel.is_cancelled() || slot.closed.is_cancelled() {
                lease.release();
                return Ok(None);
            }
            let mut owner = old.owner.clone();
            owner.reference = next.clone();
            let cwd = match launch::folder(&owner.cwd, &slot.description.subdirectory) {
                Ok(cwd) => cwd,
                Err(why) => {
                    lease.release();
                    return Err(why);
                }
            };
            let service = match ServiceOwnership::create(
                owner,
                &slot.tag,
                lease.identity,
                cwd,
                ServiceState::Configured,
                false,
            ) {
                Ok(service) => Arc::new(service),
                Err(why) => {
                    lease.release();
                    return Err(why);
                }
            };
            *slot.current.lock().unwrap_or_else(PoisonError::into_inner) = Current {
                service,
                configured_lease: Some(lease),
                process: None,
            };
            if let Err(why) = self.start_prepared(&slot, &next) {
                slot.release_if_configured();
                return Err(why);
            }
            next
        };
        self.finish_service_start(&slot, &next, cancel).await
    }

    async fn finish_service_start(
        &self,
        slot: &ManagedService,
        reference: &ServiceRef,
        cancel: &CancellationToken,
    ) -> io::Result<Option<ServiceView>> {
        if let Some(ready) = &slot.description.readiness {
            match self.wait_until_ready(reference, ready, cancel).await {
                super::ReadinessEnd::Ready(_) => {}
                super::ReadinessEnd::Cancelled { .. } => return Ok(None),
                super::ReadinessEnd::Failed { message, .. } => {
                    return Err(io::Error::other(message));
                }
            }
        } else if cancel.is_cancelled() {
            let _proof = self.stop_service(reference).await?;
            return Ok(None);
        }
        Ok(Some(self.service_status(reference)?))
    }

    pub fn service_logs(
        &self,
        reference: &ServiceRef,
        offset: usize,
        limit: usize,
        window: Option<&str>,
    ) -> io::Result<Value> {
        if limit == 0 || limit > 4096 {
            return Err(io::Error::other(
                "choose a log page size between 1 and 4096",
            ));
        }
        let slot = self.managed_service(reference)?;
        let output = slot
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .process
            .as_ref()
            .map(|entry| entry.output.said())
            .unwrap_or_default();
        let text = safe_output(&output);
        let revision = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
        if window.is_some_and(|window| window != revision) {
            return Err(io::Error::other(
                "the app wrote more output; read a new first page",
            ));
        }
        let rest = text.get(offset..).ok_or_else(|| {
            io::Error::other("that log position is no longer available; read the first page again")
        })?;
        let mut end = rest.len().min(limit);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let page = &rest[..end];
        if end == 0 && !rest.is_empty() {
            return Err(io::Error::other(
                "choose a larger log page to include the next complete character",
            ));
        }
        let view = slot.view();
        Ok(
            json!({"service":reference,"status":view.status,"exitCode":view.exit_code,"exitReason":view.exit_reason,
            "text":page,"next":offset+page.len(),"done":offset+page.len()==text.len(),"window":revision,"limited":output.len()>=64*1024}),
        )
    }

    /// Konfiguracja ma własność kopii, lecz nie proces, adres ani obietnicę gotowości.
    pub fn configure_description(
        &self,
        description: &LaunchDescription,
        tag: StepTag,
        mut owner: ServiceOwner,
    ) -> io::Result<ServiceRef> {
        crate::workflow::check::launch_description(description).map_err(io::Error::other)?;
        let cwd = launch::folder(&owner.cwd, &description.subdirectory).map_err(|why| {
            io::Error::other(format!("the app folder is not available safely: {why}"))
        })?;
        // Alias /var normalizujemy dopiero po sprawdzeniu oryginalnych korzeni no-follow.
        PublicationRoot::open(&owner.reference.workspace)?;
        PublicationRoot::open(&owner.cwd)?;
        PublicationRoot::open(&owner.run_dir)?;
        owner.reference.workspace = supervisor::publication_root_key(&owner.reference.workspace)?;
        owner.cwd = supervisor::publication_root_key(&owner.cwd)?;
        owner.run_dir = supervisor::publication_root_key(&owner.run_dir)?;
        let reference = owner.reference.clone();
        let lease = self.acquire_copy_lease(&owner.cwd, &reference)?;
        let service = match ServiceOwnership::create(
            owner,
            &tag,
            lease.identity,
            cwd,
            ServiceState::Configured,
            true,
        ) {
            Ok(service) => Arc::new(service),
            Err(why) => {
                lease.release();
                return Err(why);
            }
        };
        let slot = Arc::new(ManagedService {
            description: description.clone(),
            tag,
            current: Mutex::new(Current {
                service,
                configured_lease: Some(lease),
                process: None,
            }),
            mutation: AsyncMutex::new(()),
            closed: CancellationToken::new(),
        });
        let mut services = self.services.lock().unwrap_or_else(PoisonError::into_inner);
        if services.contains_key(&reference.service_id) {
            slot.release_if_configured();
            return Err(io::Error::other("this app is already configured"));
        }
        services.insert(reference.service_id.clone(), slot);
        Ok(reference)
    }

    pub(super) fn managed_service(
        &self,
        reference: &ServiceRef,
    ) -> io::Result<Arc<ManagedService>> {
        let slot = self
            .services
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&reference.service_id)
            .cloned()
            .ok_or_else(|| io::Error::other("this app is no longer available to this agent"))?;
        if slot.view().service != *reference {
            return Err(io::Error::other(
                "this app reference is stale or belongs to another run",
            ));
        }
        Ok(slot)
    }

    pub(super) fn start_prepared(
        &self,
        slot: &ManagedService,
        reference: &ServiceRef,
    ) -> io::Result<StartedProcess> {
        if slot.closed.is_cancelled() {
            return Err(io::Error::other(
                "this app's owner has finished; nothing was started",
            ));
        }
        let service = {
            let current = slot.current.lock().unwrap_or_else(PoisonError::into_inner);
            if current.service.owner.reference != *reference
                || current.configured_lease.is_none()
                || current.process.is_some()
            {
                return Err(io::Error::other(
                    "this app is already started or its reference changed",
                ));
            }
            Arc::clone(&current.service)
        };
        let cwd = launch::folder(&service.owner.cwd, &slot.description.subdirectory)?;
        let identity = service
            .record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .copy_identity;
        if PublicationRoot::open(&service.owner.cwd)?.identity() != identity {
            return Err(io::Error::other(
                "the app's working folder changed; nothing was started",
            ));
        }
        let (endpoints, mut environment) =
            readiness::resolve_endpoints(self, reference, &slot.description.endpoints)?;
        environment.extend(
            slot.description
                .environment
                .iter()
                .map(|(name, value)| (name.clone(), std::ffi::OsString::from(value))),
        );
        environment.extend(isolated_data(&slot.description, &service.owner)?);
        service.configure_readiness(endpoints, slot.description.readiness.is_some())?;
        {
            let mut record = service
                .record
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            record.state = ServiceState::Starting;
        }
        // Starting jest trwałe PRZED spawn; po awarii brak PGID nie udaje braku procesu.
        service.write(false)?;
        let lease = slot
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .configured_lease
            .take()
            .ok_or_else(|| io::Error::other("the app no longer holds its working folder"))?;
        self.start_registered(
            &StartSpec {
                command: slot.description.command.clone(),
                cwd,
            },
            Some(slot.tag.clone()),
            Some(service),
            Some(lease),
            &environment,
        )
    }

    pub fn service_status(&self, reference: &ServiceRef) -> io::Result<ServiceView> {
        Ok(self.managed_service(reference)?.view())
    }

    pub(super) fn release_configured(
        &self,
        workspace: Option<&std::path::Path>,
        run: Option<&str>,
    ) {
        let slots: Vec<_> = self
            .services
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        for slot in slots {
            let current = slot.current.lock().unwrap_or_else(PoisonError::into_inner);
            let owner = &current.service.owner;
            let selected = workspace.is_none_or(|path| owner.reference.workspace == path)
                && run.is_none_or(|id| {
                    owner.reference.run_id == id && owner.lifetime == ServiceLifetime::Run
                });
            drop(current);
            if selected {
                slot.closed.cancel();
                slot.release_if_configured();
            }
        }
    }

    pub(super) async fn stop_managed(
        &self,
        slot: &ManagedService,
        reference: &ServiceRef,
    ) -> io::Result<Option<GroupProof>> {
        let _exclusive = slot.mutation.lock().await;
        let entry = {
            let current = slot.current.lock().unwrap_or_else(PoisonError::into_inner);
            if current.service.owner.reference != *reference {
                return Err(io::Error::other(
                    "this app reference is stale or belongs to another run",
                ));
            }
            current.process.clone()
        };
        let Some(entry) = entry else {
            slot.release_if_configured();
            return Ok(None);
        };
        let proof = entry.prove().await;
        if proof
            .as_ref()
            .is_none_or(|proof| matches!(proof, GroupProof::Dead { .. }))
        {
            forget_if_current(&self.held, &entry);
        }
        Ok(proof)
    }
}

/// Ta sama kontrola kształtów sekretów, co przy komendzie. Redagujemy PRZED stronicowaniem.
fn safe_output(output: &str) -> String {
    let complete = if output.len() >= 64 * 1024 {
        output.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        output
    };
    complete
        .split_inclusive('\n')
        .map(|line| {
            if crate::workflow::check::secret_shaped(line).is_some() {
                "[Sensitive output omitted.]\n"
            } else {
                line
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct ServiceAccess {
    processes: Arc<Processes>,
    workspace: PathBuf,
    run_id: Option<String>,
    grants: Vec<ServiceGrant>,
    expires: CancellationToken,
}

impl ServiceAccess {
    pub fn for_step(
        processes: Arc<Processes>,
        workspace: PathBuf,
        run_id: String,
        grants: Vec<ServiceGrant>,
        expires: CancellationToken,
    ) -> Self {
        Self {
            processes,
            workspace,
            run_id: Some(run_id),
            grants,
            expires,
        }
    }

    pub fn for_lead(
        processes: Arc<Processes>,
        workspace: PathBuf,
        grants: Vec<ServiceGrant>,
        expires: CancellationToken,
    ) -> Self {
        Self {
            processes,
            workspace,
            run_id: None,
            grants,
            expires,
        }
    }

    #[must_use]
    pub fn tools(&self) -> Value {
        let mut tools = Vec::new();
        for (name, operation, description) in [
            (
                "service_status",
                ServiceOperation::Read,
                "Read the selected app's status and addresses.",
            ),
            (
                "service_logs",
                ServiceOperation::Read,
                "Read a limited, safe page of the selected app's output.",
            ),
            (
                "service_start",
                ServiceOperation::Start,
                "Start an app that this workflow already configured.",
            ),
            (
                "service_restart",
                ServiceOperation::Restart,
                "Restart one exact app instance after proving the old process stopped.",
            ),
            (
                "service_stop",
                ServiceOperation::Stop,
                "Stop one exact app instance and prove its process stopped.",
            ),
        ] {
            if self
                .grants
                .iter()
                .any(|grant| grant.operations.contains(&operation))
            {
                let mut tool = json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":{"service":{"type":"object"},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":4096},"window":{"type":"string"}},"additionalProperties":false}});
                if self.run_id.is_none() && operation != ServiceOperation::Read {
                    tool["description"] = json!(format!(
                        "{description} First use ask_the_person with this operation and exact service reference. Only its one-use approvalToken authorizes the change."
                    ));
                    tool["inputSchema"]["properties"]["approval_token"] = json!({"type":"string"});
                }
                tools.push(tool);
            }
        }
        json!(tools)
    }

    fn permits(&self, reference: &ServiceRef, operation: ServiceOperation) -> bool {
        !self.expires.is_cancelled()
            && supervisor::publication_root_key(&self.workspace)
                .is_ok_and(|path| path == reference.workspace)
            && self
                .run_id
                .as_ref()
                .is_none_or(|run| run == &reference.run_id)
            && self.grants.iter().any(|grant| {
                grant.service == reference.node_key && grant.operations.contains(&operation)
            })
    }

    fn reference_for(&self, input: &Value, operation: ServiceOperation) -> io::Result<ServiceRef> {
        let reference: ServiceRef = serde_json::from_value(
            input.get("service").cloned().unwrap_or(Value::Null),
        )
        .map_err(|_| io::Error::other("choose one exact configured app before using this tool"))?;
        if !self.permits(&reference, operation) {
            return Err(io::Error::other(
                "this agent was not allowed to do that with this app",
            ));
        }
        self.processes.service_status(&reference)?;
        Ok(reference)
    }

    /// Host pyta o zgodę nad konkretną generacją. Nie rozszerza to grantów sesji.
    pub(crate) fn approval_subject(
        &self,
        input: &Value,
        operation: ServiceOperation,
    ) -> io::Result<(ServiceRef, CancellationToken)> {
        if self.run_id.is_some()
            || operation == ServiceOperation::Read
            || operation == ServiceOperation::Unknown
        {
            return Err(io::Error::other(
                "this agent cannot request that app confirmation",
            ));
        }
        let reference = self.reference_for(input, operation)?;
        let slot = self.processes.managed_service(&reference)?;
        let current = slot.current.lock().unwrap_or_else(PoisonError::into_inner);
        let record = current
            .service
            .record
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot.closed.is_cancelled() || record.state == ServiceState::Dead {
            return Err(io::Error::other(
                "this app instance has already stopped; read its current status first",
            ));
        }
        Ok((reference, current.service.ended.clone()))
    }

    pub(crate) async fn approval_expired(&self, generation: &CancellationToken) {
        tokio::select! {
            () = generation.cancelled() => {},
            () = self.expires.cancelled() => {},
        }
    }

    /// Woła wyłącznie Desk po zużyciu Waiting dla dokładnego `ApprovalScope`. Ponowna
    /// walidacja po odpowiedzi zamyka wyścig zamknięcia sesji/podmiany generacji.
    pub(crate) async fn approved_mutation(
        &self,
        input: &Value,
        operation: ServiceOperation,
    ) -> io::Result<Value> {
        let (reference, _) = self.approval_subject(input, operation)?;
        self.change(&reference, operation).await
    }

    async fn change(
        &self,
        reference: &ServiceRef,
        operation: ServiceOperation,
    ) -> io::Result<Value> {
        match operation {
            ServiceOperation::Start => Ok(self
                .processes
                .start_service(reference, &self.expires)
                .await?
                .map_or_else(|| json!({"status":"Cancelled"}), |view| json!(view))),
            ServiceOperation::Restart => Ok(self
                .processes
                .restart_service(reference, &self.expires)
                .await?
                .map_or_else(|| json!({"status":"Cancelled"}), |view| json!(view))),
            ServiceOperation::Stop => {
                let proof = self.processes.stop_service(reference).await?;
                if matches!(proof, Some(GroupProof::Alive { .. })) {
                    return Err(io::Error::other(
                        "Loadout could not confirm that this app stopped",
                    ));
                }
                Ok(json!(self.processes.service_status(reference)?))
            }
            _ => Err(io::Error::other("that operation cannot change an app")),
        }
    }

    pub(crate) async fn dispatch(&self, call: &Call) -> io::Result<Value> {
        let input = call
            .input
            .as_object()
            .ok_or_else(|| io::Error::other("app tool arguments must be an object"))?;
        let allowed: &[&str] = if call.call == "service_logs" {
            &["service", "offset", "limit", "window"]
        } else {
            &["service"]
        };
        if input.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(io::Error::other(
                "app tools accept only an existing app reference and the requested log page, never a command or folder",
            ));
        }
        let operation = match call.call.as_str() {
            "service_status" | "service_logs" => ServiceOperation::Read,
            "service_start" => ServiceOperation::Start,
            "service_restart" => ServiceOperation::Restart,
            "service_stop" => ServiceOperation::Stop,
            _ => {
                return Err(io::Error::other("that tool is not available to this agent"));
            }
        };
        if call.call == "service_status"
            && call
                .input
                .as_object()
                .is_some_and(serde_json::Map::is_empty)
        {
            let slots: Vec<_> = self
                .processes
                .services
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .cloned()
                .collect();
            return Ok(
                json!({"services":slots.iter().map(|slot| slot.view()).filter(|view| self.permits(&view.service, operation)).collect::<Vec<_>>()}),
            );
        }
        let reference = self.reference_for(&call.input, operation)?;
        let status = self.processes.service_status(&reference)?;
        match call.call.as_str() {
            "service_status" => Ok(json!(status)),
            "service_logs" => {
                let number = |name: &str, fallback: usize| -> io::Result<usize> {
                    call.input.get(name).map_or(Ok(fallback), |value| {
                        value
                            .as_u64()
                            .and_then(|value| usize::try_from(value).ok())
                            .ok_or_else(|| {
                                io::Error::other(
                                    "the log page position and size must be whole positive numbers",
                                )
                            })
                    })
                };
                let window = call
                    .input
                    .get("window")
                    .map(|value| {
                        value
                            .as_str()
                            .ok_or_else(|| io::Error::other("the log page marker is not valid"))
                    })
                    .transpose()?;
                self.processes.service_logs(
                    &reference,
                    number("offset", 0)?,
                    number("limit", 4096)?,
                    window,
                )
            }
            "service_start" | "service_restart" | "service_stop" if self.run_id.is_some() => {
                self.change(&reference, operation).await
            }
            _ => Err(io::Error::other("This app operation is not available yet.")),
        }
    }
}

#[async_trait]
impl Answers for ServiceAccess {
    async fn answer(&self, call: Call) -> Answer {
        match self.dispatch(&call).await {
            Ok(value) => Answer::Ok(value),
            Err(why) => Answer::Refused(why.to_string()),
        }
    }
}

/// P-02: dokąd instancja testowa zapisuje swoje dane — i dlaczego nigdy do katalogu człowieka.
///
/// # Dwie rzeczy, obie zmierzone
///
/// **Aplikacja natywna bez własnej podmiany katalogu danych nie startuje.** To jest dosłownie
/// brak adaptera, nie izolacja: instancja testowa, która pisze do prawdziwego katalogu
/// użytkownika, jest gorsza od jej braku, bo scenariusz „skasuj nagranie" wykonuje się na jego
/// nagraniach. Cel webowy zostaje bez tego wymagania — serwer bez własnych danych jest zwykły.
///
/// **Katalog leży przy BIEGU**, nie w kopii roboczej: kopia jest porównywana z bazą, więc dane
/// aplikacji w jej środku wyglądałyby jak praca agenta.
fn isolated_data(
    description: &crate::workflow::LaunchDescription,
    owner: &super::ServiceOwner,
) -> io::Result<Vec<(String, std::ffi::OsString)>> {
    /* PODMIANY `HOME` NIE SPRAWDZAMY TUTAJ i to jest świadome: robi to już walidator opisu
     * uruchomienia (`workflow::check`, „The app cannot replace the environment variable …").
     * Druga kopia tej samej polityki rozjechałaby się z pierwszą przy pierwszej poprawce
     * (niezmiennik 23), a lista zastrzeżonych zmiennych mieszka tam razem z powodem. */
    let Some(name) = description
        .test_data_env
        .as_ref()
        .map(|one| one.trim())
        .filter(|one| !one.is_empty())
    else {
        if description.kind == crate::workflow::TargetKind::Native {
            return Err(io::Error::other(
                "This app has no way to keep test data apart from yours, so Loadout did not                  start it. That is a missing setting in the app, not a result about it: give it                  the name of the setting it reads for a test data folder.",
            ));
        }
        return Ok(Vec::new());
    };
    let at = owner
        .run_dir
        .join("app-data")
        .join(&owner.reference.service_id);
    std::fs::create_dir_all(&at)?;
    Ok(vec![(name.to_owned(), at.into_os_string())])
}
