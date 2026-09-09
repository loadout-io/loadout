//! WF-26: gotowość konkretnej generacji, nad tym samym właścicielem procesu.
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::PoisonError;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::{Processes, ServiceOwnership, ServiceRef, ServiceState, StartedProcess, one_of};
use crate::engine::supervisor::{self, GroupProof, ListenerOwner};
use crate::workflow::{ReadinessKind, ReadinessSpec, ServiceEndpointSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadinessState {
    Waiting,
    Ready,
    Failed,
    Stopped,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceReadiness {
    pub state: ReadinessState,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    pub service: ServiceRef,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub url: String,
    pub state: ReadinessState,
}

#[derive(Debug)]
pub enum ReadinessEnd {
    Ready(StartedProcess),
    Failed {
        message: String,
        proof: Option<GroupProof>,
    },
    Cancelled {
        proof: Option<GroupProof>,
    },
}

type EndpointsAndEnvironment = (Vec<ServiceEndpoint>, Vec<(String, OsString)>);

pub(super) fn resolve_endpoints(
    processes: &Processes,
    reference: &ServiceRef,
    requested: &[ServiceEndpointSpec],
) -> io::Result<EndpointsAndEnvironment> {
    let mut ports: BTreeSet<u16> = processes
        .list()
        .iter()
        .flat_map(|one| one.endpoints.iter().map(|endpoint| endpoint.port))
        .collect();
    let mut endpoints = Vec::new();
    let mut environment = Vec::new();
    for spec in requested {
        let host: IpAddr = spec.host.parse().map_err(io::Error::other)?;
        // Bind rezerwuje jedynie KANDYDAT; po spawnie sprawdzamy właściciela
        // listenera ponownie. Wolny port przed startem nie jest readiness.
        //
        // 2026-09-09 — NIEROZSTRZYGNIĘTE OKNO, opisane tu, bo tu się zaczyna. `ports` wyżej
        // bierze porty z REJESTRU już zarejestrowanych usług, a usługa trafia do rejestru
        // dopiero PO spawnie; rezerwujący listener jest przy tym puszczany kilka linii niżej.
        // Dwie usługi startujące równolegle mają więc chwilę, w której druga nie widzi portu
        // wybranego przez pierwszą.
        //
        // Objaw, zaobserwowany RAZ w pełnej suicie:
        // `two_copies_receive_different_automatically_allocated_endpoints` skończył się jednym
        // konsumentem jako `dependency-skipped`. Po dwóch naprawionych przyczynach flaka ten
        // moduł pada 1 na 10 w izolacji.
        //
        // TO JEST HIPOTEZA, NIE ROZPOZNANIE. Nie udało się jej odtworzyć: 12 przebiegów samego
        // testu i 8 pod sztucznym ruchem na portach dały zero czerwieni — sztuczny ruch nie
        // odtwarza dwóch RÓWNOCZESNYCH przydziałów wewnątrz tej aplikacji. Dlatego kod zostaje
        // nietknięty; zmiana rezerwacji na podstawie niepotwierdzonej teorii mogłaby zamienić
        // rzadki wyścig na częstą regresję.
        let mut reserved = None;
        for _ in 0..32 {
            let listener =
                TcpListener::bind(SocketAddr::new(host, spec.port)).map_err(|error| {
                    if error.kind() == io::ErrorKind::AddrInUse {
                        io::Error::new(
                            error.kind(),
                            "The app address is already used by another app.",
                        )
                    } else {
                        error
                    }
                })?;
            let port = listener.local_addr()?.port();
            if !ports.contains(&port) {
                reserved = Some((listener, port));
                break;
            }
            if spec.port != 0 {
                return Err(io::Error::other(
                    "The app address is already used by another app.",
                ));
            }
        }
        let (listener, port) = reserved.ok_or_else(|| {
            io::Error::other("Loadout could not choose a different port for this app.")
        })?;
        ports.insert(port);
        if let Some(name) = &spec.port_env {
            environment.push((name.clone(), port.to_string().into()));
        }
        let authority = SocketAddr::new(host, port).to_string();
        endpoints.push(ServiceEndpoint {
            service: reference.clone(),
            name: spec.name.clone(),
            host: spec.host.clone(),
            port,
            url: format!("http://{authority}"),
            state: ReadinessState::Waiting,
        });
        drop(listener);
    }
    Ok((endpoints, environment))
}

impl ServiceOwnership {
    pub(super) fn configure_readiness(
        &self,
        endpoints: Vec<ServiceEndpoint>,
        wait: bool,
    ) -> io::Result<()> {
        {
            let mut record = self.record.lock().unwrap_or_else(PoisonError::into_inner);
            record.endpoints = endpoints;
            record.readiness = wait.then(|| ServiceReadiness {
                state: ReadinessState::Waiting,
                message: "Waiting for the app to respond.".to_owned(),
            });
        }
        self.write(false)
    }

    fn set_readiness(
        &self,
        state: ReadinessState,
        message: &str,
        endpoint: Option<&str>,
    ) -> io::Result<bool> {
        {
            let mut record = self.record.lock().unwrap_or_else(PoisonError::into_inner);
            // Natural EOF mógł zamknąć wpis podczas probe. Nie wskrzeszamy go
            // ostatnim, spóźnionym HTTP 200 ani ponownym zapisem „ready”.
            if record.state == ServiceState::Dead {
                return Ok(false);
            }
            record.readiness = Some(ServiceReadiness {
                state,
                message: message.to_owned(),
            });
            for one in &mut record.endpoints {
                if endpoint.is_none_or(|name| name == one.name) {
                    one.state = state;
                }
            }
        }
        self.write(false)?;
        Ok(true)
    }
}

impl Processes {
    /// Pytania HTTP/TCP nie biorą miejsca z puli agentów. Wszystkie wyjścia
    /// odmowy/anulowania kończą TEN proces przez istniejący `stop_service`.
    pub async fn wait_until_ready(
        &self,
        reference: &ServiceRef,
        spec: &ReadinessSpec,
        cancel: &CancellationToken,
    ) -> ReadinessEnd {
        let entry = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .find(|entry| {
                entry
                    .service
                    .as_ref()
                    .is_some_and(|service| service.owner.reference == *reference)
            })
            .cloned();
        let Some(entry) = entry else {
            return ReadinessEnd::Failed {
                message: "The app exited before it was ready.".to_owned(),
                proof: None,
            };
        };
        let Some(service) = entry.service.as_ref() else {
            return ReadinessEnd::Failed {
                message: "The app has no saved owner.".to_owned(),
                proof: None,
            };
        };
        let endpoint = one_of(&entry)
            .endpoints
            .into_iter()
            .find(|endpoint| endpoint.name == spec.endpoint);
        let Some(endpoint) = endpoint else {
            return self
                .readiness_failed(
                    reference,
                    service,
                    "The app has no address to check.".to_owned(),
                )
                .await;
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(spec.timeout_seconds);
        loop {
            if cancel.is_cancelled() {
                return self.readiness_cancelled(reference, service).await;
            }
            if supervisor::group_is_empty(entry.pgid)
                || service
                    .record
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .state
                    == ServiceState::Dead
            {
                return self
                    .readiness_failed(
                        reference,
                        service,
                        "The app exited before it was ready.".to_owned(),
                    )
                    .await;
            }
            if tokio::time::Instant::now() >= deadline {
                return self.readiness_ran_out(reference, service).await;
            }
            // Własność jest sprawdzana przed i po odpowiedzi. Obcy listener,
            // który wygrał race po przydziale portu, nie uwalnia konsumenta.
            let probe = probe_owned_endpoint(&endpoint, spec, entry.pgid);
            // Nie anulujemy future sondy OS: jej własny limit musi dojść przez
            // supervisor::stop i dowód. Stop czeka najwyżej na tę ograniczoną sondę.
            let checked = probe.await;
            if cancel.is_cancelled() {
                return self.readiness_cancelled(reference, service).await;
            }
            if tokio::time::Instant::now() >= deadline {
                return self.readiness_ran_out(reference, service).await;
            }
            match checked {
                // Powód, dla którego port to nie wszystko, stoi przy `native_window_confirmed`.
                Ok(true) => match self.native_window_confirmed(reference, &entry).await {
                    Ok(true) => {
                        return self.mark_ready(reference, service, &endpoint, &entry).await;
                    }
                    Ok(false) => {}
                    Err(said) => return self.readiness_failed(reference, service, said).await,
                },
                Err(error) => {
                    return self
                        .readiness_failed(reference, service, error.to_string())
                        .await;
                }
                Ok(false) => {}
            }
            tokio::select! {
                () = cancel.cancelled() => return self.readiness_cancelled(reference, service).await,
                () = tokio::time::sleep(Duration::from_millis(40)) => {}
            }
        }
    }

    /// Jedno zdanie o limicie czasu, w jednym miejscu: czekanie sprawdza go dwa razy w obrocie
    /// (przed sondą i po niej), a dwa brzmienia tego samego faktu rozjechałyby się przy pierwszej
    /// poprawce.
    async fn readiness_ran_out(
        &self,
        reference: &ServiceRef,
        service: &ServiceOwnership,
    ) -> ReadinessEnd {
        self.readiness_failed(
            reference,
            service,
            "The app did not become ready within the requested time.".to_owned(),
        )
        .await
    }

    /// Czy ten cel ma już okno — albo czy w ogóle go potrzebuje.
    ///
    /// # P-02 punkt 6 / P-03b: dla celu natywnego gotowy port jest POŁOWĄ prawdy
    ///
    /// Scenariusz, który ma się odbyć w oknie, potrzebuje okna — a serwer deweloperski
    /// odpowiadający na porcie spełnia wyłącznie cel webowy. Incydent I-06 (2026-09-06):
    /// wymagania dopuszczały samo uruchomienie takiego serwera jako potwierdzenie zachowania
    /// widocznego wyłącznie w interfejsie.
    ///
    /// Brak okna NIE jest porażką: aplikacja natywna pokazuje je chwilę po starcie, więc
    /// pytanie wraca w następnym obrocie pętli, aż do limitu czasu. Brak ZGODY na pytanie jest
    /// osobnym wynikiem i kończy czekanie od razu — powtarzanie pytania, na które system nie
    /// pozwoli odpowiedzieć, jest wyłącznie czekaniem.
    ///
    /// `Ok(true)` znaczy „możesz ogłosić gotowość": cel webowy i `cli` nie mają okna z definicji,
    /// więc odpowiadają tak od razu. `Ok(false)` znaczy „jeszcze nie, zapytaj za chwilę".
    /// `Err(zdanie)` znaczy „nikt tu nigdy nie odpowie" — brak zgody albo nie ten system.
    async fn native_window_confirmed(
        &self,
        reference: &ServiceRef,
        entry: &super::HeldProcess,
    ) -> Result<bool, String> {
        let slot = self
            .managed_service(reference)
            .map_err(|why| why.to_string())?;
        if slot.description.kind != crate::workflow::TargetKind::Native {
            return Ok(true);
        }
        // Grupa procesów bez dodatniego identyfikatora nie ma jak zostać wskazana systemowi.
        let Ok(pid) = u32::try_from(entry.pgid) else {
            return Ok(false);
        };
        match crate::engine::native_ui::windows_of(self.native_ui_program(), pid).await {
            Ok(windows) => Ok(windows > 0),
            Err(crate::engine::native_ui::NativeUiAccess::Unknown { .. }) => Ok(false),
            Err(refused) => Err(refused.said().to_owned()),
        }
    }

    async fn mark_ready(
        &self,
        reference: &ServiceRef,
        service: &ServiceOwnership,
        endpoint: &ServiceEndpoint,
        entry: &super::HeldProcess,
    ) -> ReadinessEnd {
        match service.set_readiness(ReadinessState::Ready, "Ready to use.", Some(&endpoint.name)) {
            Ok(true) => ReadinessEnd::Ready(one_of(entry)),
            Ok(false) => {
                self.readiness_failed(
                    reference,
                    service,
                    "The app exited before it was ready.".to_owned(),
                )
                .await
            }
            Err(error) => {
                self.readiness_failed(
                    reference,
                    service,
                    format!("The app was ready, but Loadout could not save its address: {error}"),
                )
                .await
            }
        }
    }

    async fn readiness_failed(
        &self,
        reference: &ServiceRef,
        service: &ServiceOwnership,
        message: String,
    ) -> ReadinessEnd {
        let _ = service.set_readiness(ReadinessState::Failed, &message, None);
        let proof = self.stop_service(reference).await.ok().flatten();
        ReadinessEnd::Failed { message, proof }
    }

    async fn readiness_cancelled(
        &self,
        reference: &ServiceRef,
        service: &ServiceOwnership,
    ) -> ReadinessEnd {
        let _ = service.set_readiness(
            ReadinessState::Stopped,
            "Stopped before the app was ready.",
            None,
        );
        let proof = self.stop_service(reference).await.ok().flatten();
        ReadinessEnd::Cancelled { proof }
    }
}

async fn probe_owned_endpoint(
    endpoint: &ServiceEndpoint,
    spec: &ReadinessSpec,
    pgid: i32,
) -> io::Result<bool> {
    match supervisor::listener_owner(endpoint.port, pgid).await? {
        ListenerOwner::Missing => return Ok(false),
        ListenerOwner::Foreign => {
            return Err(io::Error::other("The app address is used by another app."));
        }
        ListenerOwner::Ours => {}
    }
    if !probe_endpoint(endpoint, spec).await? {
        return Ok(false);
    }
    Ok(supervisor::listener_owner(endpoint.port, pgid).await? == ListenerOwner::Ours)
}

async fn probe_endpoint(endpoint: &ServiceEndpoint, ready: &ReadinessSpec) -> io::Result<bool> {
    let address: SocketAddr = format!(
        "{}:{}",
        if endpoint.host.contains(':') {
            format!("[{}]", endpoint.host)
        } else {
            endpoint.host.clone()
        },
        endpoint.port
    )
    .parse()
    .map_err(io::Error::other)?;
    let request = async {
        let mut stream = match tokio::net::TcpStream::connect(address).await {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        if ready.kind == ReadinessKind::Tcp {
            return Ok(true);
        }
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            ready.path, address
        );
        stream.write_all(request.as_bytes()).await?;
        let mut header = Vec::new();
        // Tylko pierwsza linia odpowiedzi, z limitem przed alokacją; body i log
        // aplikacji nie są nam potrzebne do orzeczenia o gotowości.
        while header.len() < 4096 {
            let byte = match stream.read_u8().await {
                Ok(byte) => byte,
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(error) => return Err(error),
            };
            header.push(byte);
            if byte == b'\n' {
                break;
            }
        }
        let line = std::str::from_utf8(&header).map_err(io::Error::other)?;
        let mut parts = line.split_whitespace();
        let protocol = parts.next().unwrap_or_default();
        let code = parts.next().and_then(|code| code.parse::<u16>().ok());
        Ok(matches!(protocol, "HTTP/1.0" | "HTTP/1.1") && code == Some(ready.expected_status))
    };
    tokio::time::timeout(Duration::from_millis(300), request)
        .await
        .unwrap_or(Ok(false))
}
