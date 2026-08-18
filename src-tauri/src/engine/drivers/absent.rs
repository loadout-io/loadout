//! Sterownik vendora, którego adapter jeszcze nie powstał.
//!
//! DLACZEGO TO ISTNIEJE. Fabryka [`Drivers`] jest funkcją totalną: dostaje `Vendor` i musi oddać
//! uchwyt, bo planista dostaje go już po wybraniu kroku i nie ma gdzie odmówić. `Vendor::Codex`
//! jest w typie od pierwszego dnia (decyzja D3), a `CodexDriver` przyjeżdża dopiero z T-10 —
//! konto Codeksa było bez kredytów do 2026-08-20, więc S-3 nie mogło nawet nagrać złotego
//! strumienia.
//!
//! Alternatywa, której świadomie NIE wybrano: oddać dla Codeksa `ClaudeDriver`. Krok zaczyna
//! wtedy działać, kończy się sukcesem i **kłamie o tym, kto go wykonał** — a `SessionRef::vendor`
//! zapisuje tę odpowiedź do bazy, więc wznowienie wraca do niewłaściwego CLI. Kłamstwo, które
//! przeżywa restart aplikacji, jest gorsze niż krok, który się nie zaczął.
//!
//! `probe` odpowiada zgodnie z kontraktem traitu — „nie ma binarki" nie jest błędem, tylko
//! ekranem ustawień — a `start` odmawia zdaniem, które nazywa vendora i przyczynę.

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{AgentDriver, AgentHandle, DecodedEvent, Probe, RunSpec};

/// Vendor, który jest w typie, ale nie ma jeszcze adaptera.
#[derive(Debug)]
pub struct Absent {
    /// Etykieta vendora — ta sama, którą zapisałby prawdziwy sterownik.
    id: &'static str,
    /// Zadanie, po którym ten sterownik ma zniknąć. Jedzie do zdania odmowy, żeby czytający
    /// wiedział, na co czeka, zamiast zgłaszać awarię.
    owed_by: &'static str,
}

impl Absent {
    #[must_use]
    pub const fn new(id: &'static str, owed_by: &'static str) -> Self {
        Self { id, owed_by }
    }
}

#[async_trait]
impl AgentDriver for Absent {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        // Kontrakt traitu: brak CLI to ekran ustawień, nie awaria startu. Sonda mówi więc
        // „nie ma", a nie rzuca — inaczej brak jednego vendora zabijałby start aplikacji.
        Ok(Probe {
            found: false,
            version: None,
        })
    }

    async fn start(
        &self,
        _spec: RunSpec,
        _tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        anyhow::bail!(
            "Loadout has no adapter for {} yet ({} brings it). Pick an agent that runs on a \
             vendor Loadout can drive, or wait for that task to land.",
            self.id,
            self.owed_by
        )
    }
}
