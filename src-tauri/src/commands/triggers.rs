//! Triggery zapisane w `~/.loadout/triggers/`: plik jest konfiguracja i prawda o kursorze.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::{Uuid, Version};

pub const TRIGGERS_DIR: &str = "triggers";
pub const DEFAULT_POLL_EVERY_MINUTES: u32 = 1;
const TRIGGER_SCHEMA: u32 = 1;
const API: &str = "https://api.linear.app/graphql";
const TIMEOUT_SECONDS: u64 = 20;
const MAX_API_KEY_BYTES: usize = 256;
pub const ISSUES_QUERY: &str = "query AssignedToMe { issues(filter: { assignee: { isMe: { eq: true } } }, orderBy: updatedAt) { nodes { id identifier title url description updatedAt } } }";
const VIEWER_QUERY: &str = "query ConnectionTest { viewer { id } }";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Linear,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn exposes(&self, expected: &str) -> bool {
        self.0 == expected
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    pub schema: u32,
    pub source: Source,
    pub enabled: bool,
    pub workflow: String,
    pub condition: String,
    #[serde(default = "default_poll_every_minutes")]
    pub poll_every_minutes: u32,
    pub api_key: Secret,
}

const fn default_poll_every_minutes() -> u32 {
    DEFAULT_POLL_EVERY_MINUTES
}

/// Dane wpisane w formularzu. Slug i `enabled` nie przychodza z okna: Rust wybija nazwe,
/// a edycja zachowuje osobny, trwaly przelacznik z biblioteki.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriggerDraft {
    pub source: String,
    pub condition: String,
    pub workflow: String,
    pub poll_every_minutes: u32,
    pub api_key: Option<Secret>,
}

impl fmt::Debug for TriggerDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source = (self.source == "linear").then_some("linear");
        let condition = (self.condition == "assigned-to-me").then_some("assigned-to-me");
        formatter
            .debug_struct("TriggerDraft")
            .field("source", &source.unwrap_or("<redacted>"))
            .field("condition", &condition.unwrap_or("<redacted>"))
            .field("workflow", &"<selected>")
            .field("poll_every_minutes", &self.poll_every_minutes)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Zredagowana migawka niesiona z listy do Edit/Delete jako ochrona przed utrata recznej zmiany.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriggerSnapshot {
    pub slug: String,
    pub source: Source,
    pub condition: String,
    pub workflow: String,
    pub enabled: bool,
    pub poll_every_minutes: u32,
    #[serde(rename = "hasApiKey")]
    pub key_saved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,
    pub body: String,
    pub updated_at: String,
}

/// Zredagowany wpis biblioteki triggerow, gotowy do przekroczenia granicy IPC.
///
/// Sekret celowo nie ma tu pola: `skip_serializing` chroniloby tylko jeden sposob wypisania,
/// a ten typ ma byc bezpieczny takze w `Debug` i w przyszlym loggerze.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerEntry {
    /// Nazwa pliku bez `.json`; jedyny identyfikator wysylany przez okno.
    pub slug: String,
    /// Zrodlo spraw.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Warunek zapisany przez czlowieka.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Prawdziwy identyfikator workflow z konfiguracji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// Czy zegar ma pytac ten trigger.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Czestotliwosc sprawdzania; uszkodzony wpis nie zmysla wartosci.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_every_minutes: Option<u32>,
    /// Sam fakt zapisania sekretu. Wartosc ani jej pochodna nie przekracza IPC.
    #[serde(rename = "hasApiKey", skip_serializing_if = "Option::is_none")]
    pub key_saved: Option<bool>,
    /// Nazwany problem z konkretnym plikiem; zdrowy wpis pomija to pole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Niepodrabialny uchwyt jednego trafienia przekazywany z Rusta z powrotem do Startu.
///
/// Nie niesie tresci sprawy ani sekretu. Pelna dostawa zostaje w ledgerze plikowym, a Start
/// sprawdza te cztery pola przeciwko niemu przed utworzeniem biegu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriggerClaim {
    /// Trigger, ktory zobaczyl sprawe.
    pub slug: String,
    /// Stabilny identyfikator dostawy, inny od identyfikatora sprawy.
    pub delivery_id: String,
    /// Workflow zamrozony z konfiguracji w chwili dostawy.
    pub workflow: String,
    /// UUID v7 przyszlego biegu, przydzielony przed pokazaniem dostawy oknu.
    pub run_id: String,
}

/// Pelna, trwala dostawa sprawy do otwartego okna.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriggerDelivery {
    /// Zredagowany uchwyt wracajacy pozniej do `run_workflow`.
    pub claim: TriggerClaim,
    /// Sprawa, z ktorej okno buduje kanoniczne zadanie.
    pub issue: Issue,
    /// Czas utworzenia receipt w milisekundach epoki; nie zegar webviewa.
    pub created_at: i64,
}

/// Wynik jednego tykniecia zegara triggera.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TriggerPoll {
    /// Rust odmowil przed zapytaniem zewnetrznego serwisu, bo jeden bieg juz ma uchwyt.
    Busy,
    /// Pierwszy odczyt zapisal zastany backlog jako widziany i niczego nie uruchomil.
    Armed,
    /// Jedna dostawa czeka na przejscie istniejaca droga Startu.
    Pending {
        /// Dostawa wraz z claimem przyszlego biegu.
        /// `Box` ogranicza kazdy lekki tick do rozmiaru wskaznika; Serde zachowuje ten sam
        /// obiekt `delivery`, wiec granica IPC i zapisany claim nie zmieniaja ksztaltu.
        delivery: Box<TriggerDelivery>,
    },
    /// Restart pogodził `bound` z istniejącym `run.json`; nic nie zostanie uruchomione drugi raz.
    Accepted {
        /// Workflow, który trwale przyjął sprawę.
        workflow: String,
        /// Czas receipt zapisany w ledgerze, nie czas ponownego montażu okna.
        receipt_at: i64,
    },
}

/// Trwaly stan dostawy, odczytywany wprost z pliku ledgeru.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum DeliveryState {
    /// Dostawa istnieje, ale nie zostala jeszcze zwiazana z projektem i biegiem.
    Pending,
    /// Start zarezerwowal docelowy `run.json`, lecz pierwszy atomowy zrzut jeszcze nie istnieje.
    Bound {
        /// Dokladny plik, ktory stanowi granice akceptacji tej dostawy.
        run_file: PathBuf,
    },
    /// Pierwszy `run.json` istnieje i ta dostawa nigdy nie moze uruchomic drugiego biegu.
    Accepted {
        /// Plik bedacy trwalym dowodem akceptacji.
        run_file: PathBuf,
        /// Czas receipt w milisekundach epoki, zapisany przez Rust.
        accepted_at: i64,
    },
    /// Delete trwale odrzucil dostawe, zanim konfiguracja zniknela z biblioteki.
    Cancelled,
}

/// Zredagowane pochodzenie zapisane w pierwszym `run.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrigin {
    /// Trigger, ktory dostarczyl prace.
    pub slug: String,
    /// Identyfikator receipt, po ktorym ledger i bieg godza sie po awarii.
    pub delivery_id: String,
    /// Stabilny identyfikator sprawy u zrodla.
    pub issue_id: String,
}

#[derive(Debug, Error)]
pub enum TriggerError {
    #[error("Trigger names use letters, numbers, dots, dashes and underscores only.")]
    BadSlug,
    #[error("Loadout could not read the trigger file: {0}")]
    ReadConfig(io::Error),
    #[error("The trigger file has an invalid field: {0}")]
    InvalidConfig(serde_json::Error),
    #[error("Add a Linear API key as `api_key` in this trigger file, then try again.")]
    MissingKey,
    #[error("Enter a Linear API key, then try again.")]
    EditorMissingKey,
    #[error("The trigger source `{0}` is not available. Choose `linear`.")]
    UnknownSource(String),
    #[error("This trigger source is not available. Choose `linear`.")]
    UnknownSourceRedacted,
    #[error("This trigger file uses an unsupported format version.")]
    UnsupportedSchema,
    #[error("The Linear API key has an invalid shape. Replace it with a `lin_api_...` key.")]
    InvalidKey,
    #[error("Linear returned an empty response. Check the connection and try again.")]
    EmptyAnswer,
    #[error("Linear returned a web page instead of issue data. Check the API address.")]
    HtmlAnswer,
    #[error("Linear returned data that is not JSON: {0}")]
    InvalidAnswer(serde_json::Error),
    #[error("Linear refused the request. Check the key and try again.")]
    Api,
    #[error("Linear's response did not contain an issue list.")]
    MissingIssues,
    #[error("Linear did not confirm the signed-in account. Check the key and try again.")]
    MissingViewer,
    #[error("Linear refused this key. Replace it and try again.")]
    ConnectionRefused,
    #[error("Loadout could not read the trigger cursor: {0}")]
    ReadCursor(io::Error),
    #[error("Loadout could not save the trigger cursor, so it did not accept the issue: {0}")]
    WriteCursor(io::Error),
    #[error("Loadout could not read the trigger library: {0}")]
    ReadLibrary(io::Error),
    #[error("Loadout could not access the trigger folder: {0}")]
    TriggerDirectory(io::Error),
    #[error("Loadout's trigger folder must be a regular folder. Replace it, then try again.")]
    UnsafeTriggerDirectory,
    #[error("Loadout could not save the trigger file: {0}")]
    WriteConfig(io::Error),
    #[error("That trigger already exists. Create a new trigger instead.")]
    AlreadyExists,
    #[error("This trigger no longer exists. Reload the trigger list, then try again.")]
    MissingConfig,
    #[error("Choose an existing workflow before saving this trigger.")]
    MissingWorkflow,
    #[error("Choose how often to check Linear: 1, 5, 15 or 60 minutes.")]
    InvalidCadence,
    #[error("This trigger condition is not available. Choose issues assigned to you.")]
    InvalidCondition,
    #[error("This trigger is not a regular file, so Loadout left it unchanged.")]
    NotRegularConfig,
    #[error(
        "Loadout is still finishing an earlier delete. Reload the trigger list, then try again."
    )]
    DeleteInProgress,
    #[error("A run from this trigger is starting. Wait for it to start, then try deleting again.")]
    RunStarting,
    #[error(
        "This trigger changed while Loadout was switching it. Review the file, then try again."
    )]
    ConfigChanged,
    #[error("Loadout could not read the trigger delivery ledger: {0}")]
    ReadLedger(io::Error),
    #[error("The trigger delivery ledger is invalid: {0}")]
    InvalidLedger(serde_json::Error),
    #[error("The trigger delivery ledger uses an unsupported format version.")]
    UnsupportedLedgerSchema,
    #[error("Loadout could not save the trigger delivery ledger: {0}")]
    WriteLedger(io::Error),
    #[error("This trigger delivery is no longer available. Check the trigger again.")]
    InvalidClaim,
    #[error("Loadout could not read the run receipt: {0}")]
    ReadRun(io::Error),
    #[error("Loadout could not finish saving this run safely: {0}")]
    RunDurability(io::Error),
    #[error("The run receipt is invalid: {0}")]
    InvalidRun(serde_json::Error),
    #[error("The run receipt does not match this trigger delivery.")]
    RunMismatch,
    #[error("Loadout could not start curl for the Linear trigger: {0}")]
    Start(io::Error),
    #[error("Linear could not be checked because curl exited with status {0}.")]
    CurlFailed(String),
}

/// Etapy odzyskiwalnego cleanupu po Delete. Tombstone jest ostatnim, atomowym commitem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStage {
    WritingCandidate,
    BeforeWritingRemoval,
    AfterLedger,
    AfterCursor,
    BeforeCommit,
    AfterCommit,
}

/// Wypisuje cala biblioteke bez sekretow, lacznie z nazwanymi problemami pojedynczych plikow.
pub fn list(home: &Path) -> Result<Vec<TriggerEntry>, TriggerError> {
    list_with_cleanup(home, |_, _| Ok(()))
}

/// Produkcyjna lista z fault seamem dowodzacym, ze tombstone pozostaje czytelnikiem do commit.
pub fn list_with_cleanup<F>(home: &Path, mut observe: F) -> Result<Vec<TriggerEntry>, TriggerError>
where
    F: FnMut(CleanupStage, &Path) -> io::Result<()>,
{
    let _config_guard = config_guard();
    let Some(_) = existing_trigger_dir(home)? else {
        return Ok(Vec::new());
    };
    cleanup_writing_files(home, &mut observe)?;
    cleanup_tombstones(home, &mut observe)?;
    let dir = home.join(TRIGGERS_DIR);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(TriggerError::ReadLibrary(error)),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(TriggerError::ReadLibrary)?;
        let kind = entry.file_type().map_err(TriggerError::ReadLibrary)?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let is_json = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension == "json");
        if name.starts_with('.') || !is_json {
            continue;
        }
        let slug = name.trim_end_matches(".json").to_owned();
        if !kind.is_file() {
            out.push(TriggerEntry {
                slug,
                source: None,
                condition: None,
                workflow: None,
                enabled: None,
                poll_every_minutes: None,
                key_saved: None,
                problem: Some(
                    "This trigger is not a regular file, so Loadout left it unchanged.".to_owned(),
                ),
            });
            continue;
        }
        match load(home, &slug) {
            Ok(trigger) => out.push(TriggerEntry {
                slug,
                source: Some(trigger.source),
                condition: Some(trigger.condition),
                workflow: Some(trigger.workflow),
                enabled: Some(trigger.enabled),
                poll_every_minutes: Some(trigger.poll_every_minutes),
                key_saved: Some(true),
                problem: None,
            }),
            Err(error) => out.push(TriggerEntry {
                slug,
                source: None,
                condition: None,
                workflow: None,
                enabled: None,
                poll_every_minutes: None,
                key_saved: None,
                // Biblioteka jest granica redakcji. Nawet wartosc wpisana omylkowo w `source`
                // nie moze stac sie komunikatem, bo mogla byc sekretem w zlym polu.
                problem: Some(library_problem(&error)),
            }),
        }
    }
    out.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(out)
}

/// Rozpoznawalne `.writing` sa plikami transakcji Loadoutu, wiec prawdziwa lista jest ich
/// czytelnikiem po awarii procesu. Nieznane dotfile, katalogi i symlinki zostaja nietkniete.
fn cleanup_writing_files<F>(home: &Path, observe: &mut F) -> Result<(), TriggerError>
where
    F: FnMut(CleanupStage, &Path) -> io::Result<()>,
{
    let dir = home.join(TRIGGERS_DIR);
    let entries = fs::read_dir(&dir).map_err(TriggerError::ReadLibrary)?;
    let mut removed = false;
    for entry in entries {
        let entry = entry.map_err(TriggerError::ReadLibrary)?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(owner) = recognized_writing_file(&name) else {
            continue;
        };
        let kind = entry.file_type().map_err(TriggerError::ReadLibrary)?;
        if !kind.is_file() || kind.is_symlink() {
            continue;
        }
        observe(CleanupStage::WritingCandidate, &entry.path())
            .map_err(TriggerError::WriteConfig)?;
        let ledger_lock = match owner {
            WritingOwner::Config => None,
            WritingOwner::Ledger(slug) => Some(ledger_lock_for(home, &slug)?),
        };
        let _ledger_guard = ledger_lock
            .as_ref()
            .map(|lock| lock.lock().unwrap_or_else(PoisonError::into_inner));
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            // Wlasciciel mogl opublikowac aktywny temp przez rename, zanim cleanup przejal
            // jego slug lock. Brak starej nazwy jest wtedy sukcesem transakcji, nie awaria listy.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(TriggerError::ReadLibrary(error)),
        };
        if !metadata.file_type().is_file() {
            continue;
        }
        observe(CleanupStage::BeforeWritingRemoval, &entry.path())
            .map_err(TriggerError::WriteConfig)?;
        fs::remove_file(entry.path()).map_err(TriggerError::WriteConfig)?;
        removed = true;
    }
    if removed {
        File::open(&dir)
            .and_then(|directory| directory.sync_all())
            .map_err(TriggerError::WriteConfig)?;
    }
    Ok(())
}

enum WritingOwner {
    Config,
    Ledger(String),
}

fn recognized_writing_file(name: &str) -> Option<WritingOwner> {
    let body = name
        .strip_prefix('.')
        .and_then(|name| name.strip_suffix(".writing"))?;

    // `create_with`: `.linear-<uuid-v7>-<tempfile random>.writing`.
    if let Some(rest) = body.strip_prefix("linear-") {
        let bytes = rest.as_bytes();
        if bytes.len() > 37 && bytes.get(36) == Some(&b'-') {
            let uuid = std::str::from_utf8(&bytes[..36])
                .ok()
                .and_then(|raw| Uuid::parse_str(raw).ok());
            let random = &bytes[37..];
            if uuid.is_some_and(|id| id.get_version() == Some(Version::SortRand))
                && !random.is_empty()
                && random.iter().all(u8::is_ascii_alphanumeric)
            {
                return Some(WritingOwner::Config);
            }
        }
    }

    // `replace_atomically`: `.<regular-name>.json-<pid>-<nonce>.writing`.
    let (with_pid, nonce) = body.rsplit_once('-')?;
    let (file, pid) = with_pid.rsplit_once('-')?;
    let slug = file.strip_suffix(".json")?;
    let recognized = !pid.is_empty()
        && pid.bytes().all(|byte| byte.is_ascii_digit())
        && !nonce.is_empty()
        && nonce.bytes().all(|byte| byte.is_ascii_digit())
        && valid_slug(slug).is_ok();
    if !recognized {
        return None;
    }
    match slug.strip_suffix(".ledger") {
        Some(trigger_slug) if valid_slug(trigger_slug).is_ok() => {
            Some(WritingOwner::Ledger(trigger_slug.to_owned()))
        }
        _ => Some(WritingOwner::Config),
    }
}

/// Tombstone jest czytany przez prawdziwa sciezke listy: po crashu Delete nastepny odczyt
/// konczy cleanup konfiguracji i jej lokalnego stanu, zamiast zostawiac martwy artefakt.
fn cleanup_tombstones<F>(home: &Path, observe: &mut F) -> Result<(), TriggerError>
where
    F: FnMut(CleanupStage, &Path) -> io::Result<()>,
{
    let dir = home.join(TRIGGERS_DIR);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(TriggerError::ReadLibrary(error)),
    };
    let mut tombstones = Vec::new();
    for entry in entries {
        let entry = entry.map_err(TriggerError::ReadLibrary)?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(slug) = name
            .strip_prefix('.')
            .and_then(|name| name.strip_suffix(".deleted.json"))
        else {
            continue;
        };
        if valid_slug(slug).is_err() {
            continue;
        }
        let tombstone_kind = entry.file_type().map_err(TriggerError::ReadLibrary)?;
        if !tombstone_kind.is_file() || tombstone_kind.is_symlink() {
            continue;
        }
        tombstones.push((slug.to_owned(), entry.path()));
    }
    if tombstones.is_empty() {
        return Ok(());
    }

    for (slug, tombstone) in tombstones {
        // Zwykla lista nie czeka na siec innego triggera. Bierzemy zamek tylko tego sluga,
        // ktorego marker sprzatamy; kolejnosc pozostaje config -> ledger konkretnego sluga.
        let ledger_lock = ledger_lock_for(home, &slug)?;
        let _ledger_guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
        let metadata = fs::symlink_metadata(&tombstone).map_err(TriggerError::ReadLibrary)?;
        if !metadata.file_type().is_file() {
            continue;
        }
        let visible = home.join(TRIGGERS_DIR).join(format!("{slug}.json"));
        match fs::symlink_metadata(&visible) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(TriggerError::ReadLibrary(error)),
        }
        let ledger_file = ledger_path(home, &slug)?;
        let ledger = match fs::symlink_metadata(&ledger_file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(TriggerError::ReadLedger(error)),
            Ok(metadata) if metadata.file_type().is_file() => {
                let raw = fs::read(&ledger_file).map_err(TriggerError::ReadLedger)?;
                Some(parse_ledger(&raw)?)
            }
            Ok(_) => {
                return Err(TriggerError::ReadLedger(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "trigger ledger is not a regular file",
                )));
            }
        };
        if ledger.as_ref().is_some_and(|ledger| {
            ledger.deliveries.iter().any(|record| {
                matches!(
                    &record.state,
                    DeliveryState::Pending | DeliveryState::Bound { .. }
                )
            })
        }) {
            return Err(TriggerError::DeleteInProgress);
        }
        remove_if_present(&ledger_file).map_err(TriggerError::WriteConfig)?;
        observe(CleanupStage::AfterLedger, &tombstone).map_err(TriggerError::WriteConfig)?;
        remove_if_present(&cursor_path(home, &slug)).map_err(TriggerError::WriteConfig)?;
        observe(CleanupStage::AfterCursor, &tombstone).map_err(TriggerError::WriteConfig)?;
        // Pierwsza bariera czyni usuniecie sidecarow trwalym ZANIM zniknie jedyny czytelnik
        // recovery. Power loss nie moze zostawic ledgera bez tombstone'a, ktory go posprzata.
        File::open(&dir)
            .and_then(|directory| directory.sync_all())
            .map_err(TriggerError::WriteConfig)?;
        observe(CleanupStage::BeforeCommit, &tombstone).map_err(TriggerError::WriteConfig)?;
        remove_if_present(&tombstone).map_err(TriggerError::WriteConfig)?;
        observe(CleanupStage::AfterCommit, &tombstone).map_err(TriggerError::WriteConfig)?;
        File::open(&dir)
            .and_then(|directory| directory.sync_all())
            .map_err(TriggerError::WriteConfig)?;
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn library_problem(error: &TriggerError) -> String {
    match error {
        TriggerError::ReadConfig(_) => "Loadout could not read this trigger file.".to_owned(),
        TriggerError::InvalidConfig(_) => "This trigger file is not valid JSON.".to_owned(),
        TriggerError::MissingKey | TriggerError::InvalidKey => {
            "This trigger needs a valid Linear key.".to_owned()
        }
        TriggerError::UnknownSource(_) | TriggerError::UnknownSourceRedacted => {
            "This trigger uses an unavailable source. Choose linear.".to_owned()
        }
        TriggerError::UnsupportedSchema => {
            "This trigger uses an unsupported file format.".to_owned()
        }
        TriggerError::InvalidCondition => {
            "This trigger needs the assigned-to-me condition.".to_owned()
        }
        TriggerError::InvalidCadence => {
            "This trigger needs a 1, 5, 15 or 60 minute schedule.".to_owned()
        }
        _ => "This trigger file could not be loaded.".to_owned(),
    }
}

/// Atomowo zmienia `enabled`, zachowujac sekret, pozostale pola i prawa pliku.
pub fn set_enabled(home: &Path, slug: &str, enabled: bool) -> Result<TriggerEntry, TriggerError> {
    set_enabled_with(home, slug, enabled, |_, _| Ok(()))
}

/// Etapy atomowej podmiany udostepnione sedziemu bez ujawniania tresci konfiguracji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleStage {
    /// Plik tymczasowy ma juz odziedziczone prawa, ale jeszcze zero bajtow.
    BeforeContent,
    /// Caly plik jest zsynchronizowany; za chwile nastapi ostatnie porownanie snapshotu.
    BeforeCompare,
}

/// Wariant z obserwatorem dowodzacy kolejnosci praw i konfliktu bez wyscigu czasowego.
/// Produkcja wchodzi przez [`set_enabled`] z pustym obserwatorem.
pub fn set_enabled_with<F>(
    home: &Path,
    slug: &str,
    enabled: bool,
    mut observe: F,
) -> Result<TriggerEntry, TriggerError>
where
    F: FnMut(ToggleStage, &Path) -> io::Result<()>,
{
    let _guard = config_guard();
    valid_slug(slug)?;
    let dir = require_trigger_dir(home)?;
    let path = dir.join(format!("{slug}.json"));
    let mut original = File::open(&path).map_err(TriggerError::ReadConfig)?;
    let permissions = original
        .metadata()
        .map_err(TriggerError::ReadConfig)?
        .permissions();
    let mut snapshot = Vec::new();
    original
        .read_to_end(&mut snapshot)
        .map_err(TriggerError::ReadConfig)?;
    // Uchwyt snapshotu nie jest potrzebny po odczycie. Zamykamy go przed podmiana, zeby
    // rename nie zalezal od uniksowej semantyki otwartego pliku docelowego.
    drop(original);
    let mut trigger = parse_trigger(&snapshot)?;
    trigger.enabled = enabled;
    let bytes = serde_json::to_vec_pretty(&trigger).map_err(TriggerError::InvalidConfig)?;
    match replace_atomically(
        &path,
        &bytes,
        Some(permissions),
        Some(&snapshot),
        &mut observe,
    )
    .map_err(TriggerError::WriteConfig)?
    {
        Replaced::Saved => {}
        Replaced::Changed => return Err(TriggerError::ConfigChanged),
    }
    Ok(TriggerEntry {
        slug: slug.to_owned(),
        source: Some(trigger.source),
        condition: Some(trigger.condition),
        workflow: Some(trigger.workflow),
        enabled: Some(trigger.enabled),
        poll_every_minutes: Some(trigger.poll_every_minutes),
        key_saved: Some(true),
        problem: None,
    })
}

/// Punkty obserwacji zapisu formularza. Sedzia widzi kolejnosc, ale nigdy bajty z sekretem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorStage {
    /// Plik tymczasowy ma juz prywatne prawa, lecz nie ma jeszcze tresci.
    BeforeContent,
    /// Pelna tresc jest trwala; za chwile zapis sprawdzi migawke i opublikuje plik.
    BeforeCompare,
}

/// Punkty obserwacji Delete oddzielaja trwaly koniec pracy od atomowego znikniecia configu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteStage {
    /// Pending sa juz trwale anulowane, ale konfiguracja nadal jest widoczna.
    AfterCancellation,
    /// Konfiguracja ma juz ukryta nazwe; cleanup moze zostac dokonczony po restarcie.
    AfterHide,
}

/// Tworzy kompletny trigger pod slugiem wybitym przez Rust.
pub fn create(home: &Path, draft: TriggerDraft) -> Result<TriggerEntry, TriggerError> {
    create_with(home, draft, Uuid::now_v7, |_, _| Ok(()))
}

/// Wariant z obserwatorem dla dowodu praw i publikacji no-clobber.
pub fn create_with<M, F>(
    home: &Path,
    draft: TriggerDraft,
    mint: M,
    mut observe: F,
) -> Result<TriggerEntry, TriggerError>
where
    M: FnOnce() -> Uuid,
    F: FnMut(EditorStage, &Path) -> io::Result<()>,
{
    let _guard = config_guard();
    let trigger = trigger_from_draft(home, draft, TRIGGER_SCHEMA, true, None)?;
    let slug = format!("linear-{}", mint());
    let parent = ensure_trigger_dir(home)?;
    let path = parent.join(format!("{slug}.json"));
    let bytes = serde_json::to_vec_pretty(&trigger).map_err(TriggerError::InvalidConfig)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(&format!(".{slug}-"))
        .suffix(".writing")
        .tempfile_in(&parent)
        .map_err(TriggerError::WriteConfig)?;
    // `tempfile` otwiera plik z 0600 w samym create(2), przed tym seamem i pierwszym bajtem.
    // To zachowuje niezmiennik 3: kod platformowy zostaje w zaleznosci, nie w tym module.
    observe(EditorStage::BeforeContent, temporary.path()).map_err(TriggerError::WriteConfig)?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(TriggerError::WriteConfig)?;
    observe(EditorStage::BeforeCompare, temporary.path()).map_err(TriggerError::WriteConfig)?;
    let persisted = match temporary.persist_noclobber(&path) {
        Ok(file) => file,
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(TriggerError::AlreadyExists);
        }
        Err(error) => return Err(TriggerError::WriteConfig(error.error)),
    };
    drop(persisted);
    File::open(&parent)
        .and_then(|directory| directory.sync_all())
        .map_err(TriggerError::WriteConfig)?;
    Ok(entry_for(&slug, &trigger))
}

/// Zmienia niesekretne pola, a pusty klucz zachowuje sekret ze swiezej wersji pliku.
pub fn update(
    home: &Path,
    slug: &str,
    expected: &TriggerSnapshot,
    draft: TriggerDraft,
) -> Result<TriggerEntry, TriggerError> {
    update_with(home, slug, expected, draft, |_, _| Ok(()))
}

/// Wariant z obserwatorem dla deterministycznego testu konfliktu i praw pliku.
pub fn update_with<F>(
    home: &Path,
    slug: &str,
    expected: &TriggerSnapshot,
    draft: TriggerDraft,
    mut observe: F,
) -> Result<TriggerEntry, TriggerError>
where
    F: FnMut(EditorStage, &Path) -> io::Result<()>,
{
    let _guard = config_guard();
    valid_slug(slug)?;
    let dir = require_trigger_dir(home)?;
    let path = dir.join(format!("{slug}.json"));
    let (snapshot, permissions) = read_regular_config(&path)?;
    let current = parse_trigger(&snapshot)?;
    if snapshot_for(slug, &current) != *expected {
        return Err(TriggerError::ConfigChanged);
    }
    let trigger = trigger_from_draft(
        home,
        draft,
        current.schema,
        current.enabled,
        Some(current.api_key),
    )?;
    let bytes = serde_json::to_vec_pretty(&trigger).map_err(TriggerError::InvalidConfig)?;
    let mut editor_observer = |stage, path: &Path| {
        let stage = match stage {
            ToggleStage::BeforeContent => EditorStage::BeforeContent,
            ToggleStage::BeforeCompare => EditorStage::BeforeCompare,
        };
        observe(stage, path)
    };
    match replace_atomically(
        &path,
        &bytes,
        Some(permissions),
        Some(&snapshot),
        &mut editor_observer,
    )
    .map_err(TriggerError::WriteConfig)?
    {
        Replaced::Saved => Ok(entry_for(slug, &trigger)),
        Replaced::Changed => Err(TriggerError::ConfigChanged),
    }
}

/// Konczy nieprzyjeta prace i usuwa konfiguracje bez okna z aktywnym configiem nad ledgerem.
pub fn delete(home: &Path, slug: &str, expected: &TriggerSnapshot) -> Result<(), TriggerError> {
    delete_with(home, slug, expected, |_, _| Ok(()))
}

/// Wariant z obserwatorem zasadza awarie po obu stronach atomowego ukrycia konfiguracji.
pub fn delete_with<F>(
    home: &Path,
    slug: &str,
    expected: &TriggerSnapshot,
    mut observe: F,
) -> Result<(), TriggerError>
where
    F: FnMut(DeleteStage, &Path) -> io::Result<()>,
{
    let _config_guard = config_guard();
    valid_slug(slug)?;
    let ledger_lock = ledger_lock_for(home, slug)?;
    let _ledger_guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = require_trigger_dir(home)?;
    let path = dir.join(format!("{slug}.json"));
    let (snapshot, _) = read_regular_config(&path)?;
    let trigger = parse_trigger(&snapshot)?;
    if snapshot_for(slug, &trigger) != *expected {
        return Err(TriggerError::ConfigChanged);
    }
    let tombstone = tombstone_path(home, slug);
    match fs::symlink_metadata(&tombstone) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(TriggerError::ReadConfig(error)),
        Ok(_) => return Err(TriggerError::DeleteInProgress),
    }

    let mut ledger = read_ledger(home, slug)?;
    // Bound nalezy juz do drogi Startu. Delete nie moze cofnac tego wiazania ani schowac
    // konfiguracji przed pierwszym run.json; operator powinien dokonczyc Start, potem usunac.
    if ledger
        .deliveries
        .iter()
        .any(|record| matches!(&record.state, DeliveryState::Bound { .. }))
    {
        return Err(TriggerError::RunStarting);
    }
    let mut cancelled = false;
    for record in &mut ledger.deliveries {
        if matches!(&record.state, DeliveryState::Pending) {
            record.state = DeliveryState::Cancelled;
            cancelled = true;
        }
    }
    if cancelled {
        write_ledger(home, slug, &ledger)?;
    }
    observe(DeleteStage::AfterCancellation, &path).map_err(TriggerError::WriteConfig)?;

    // Reczna zmiana publicznych pol po potwierdzeniu zostaje widoczna. Ledger jest juz
    // bezpiecznie anulowany, wiec odmowa nie zostawia pracy udajacej Pending.
    let (current, _) = read_regular_config(&path)?;
    if current != snapshot {
        return Err(TriggerError::ConfigChanged);
    }
    fs::rename(&path, &tombstone).map_err(TriggerError::WriteConfig)?;
    let parent = path.parent().ok_or_else(|| {
        TriggerError::WriteConfig(io::Error::other("trigger file has no parent directory"))
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(TriggerError::WriteConfig)?;
    observe(DeleteStage::AfterHide, &tombstone).map_err(TriggerError::WriteConfig)?;
    Ok(())
}

/// Rozstrzyga klucz podany w formularzu albo zapisany pod slugiem; niczego nie zapisuje.
pub fn connection_key(
    home: &Path,
    slug: Option<&str>,
    api_key: Option<Secret>,
) -> Result<Secret, TriggerError> {
    let key = if let Some(key) = api_key {
        key
    } else {
        let slug = slug.ok_or(TriggerError::EditorMissingKey)?;
        valid_slug(slug)?;
        let path = require_trigger_dir(home)?.join(format!("{slug}.json"));
        let (raw, _) = read_regular_config(&path)?;
        parse_trigger(&raw)?.api_key
    };
    if !valid_key(key.as_str()) {
        return Err(TriggerError::InvalidKey);
    }
    Ok(key)
}

fn trigger_from_draft(
    home: &Path,
    draft: TriggerDraft,
    schema: u32,
    enabled: bool,
    saved_key: Option<Secret>,
) -> Result<Trigger, TriggerError> {
    let TriggerDraft {
        source,
        condition,
        workflow,
        poll_every_minutes,
        api_key,
    } = draft;
    if source != "linear" {
        return Err(TriggerError::UnknownSourceRedacted);
    }
    if condition != "assigned-to-me" {
        return Err(TriggerError::InvalidCondition);
    }
    if !valid_cadence(poll_every_minutes) {
        return Err(TriggerError::InvalidCadence);
    }
    let api_key = api_key
        .or(saved_key)
        .ok_or(TriggerError::EditorMissingKey)?;
    if !valid_key(api_key.as_str()) {
        return Err(TriggerError::InvalidKey);
    }
    // Ten sam loader, ktory zasila prawdziwa biblioteke, jest jedyna odpowiedzia na pytanie,
    // czy wybrana nazwa istnieje i nie wychodzi przez `../` poza katalog workflow.
    super::workflows::load_workflow_inner(home, &workflow)
        .map_err(|_| TriggerError::MissingWorkflow)?;
    Ok(Trigger {
        schema,
        source: Source::Linear,
        enabled,
        workflow,
        condition,
        poll_every_minutes,
        api_key,
    })
}

fn read_regular_config(path: &Path) -> Result<(Vec<u8>, fs::Permissions), TriggerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(TriggerError::MissingConfig);
        }
        Err(error) => return Err(TriggerError::ReadConfig(error)),
    };
    if !metadata.file_type().is_file() {
        return Err(TriggerError::NotRegularConfig);
    }
    let mut file = File::open(path).map_err(TriggerError::ReadConfig)?;
    if !file.metadata().map_err(TriggerError::ReadConfig)?.is_file() {
        return Err(TriggerError::NotRegularConfig);
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(TriggerError::ReadConfig)?;
    Ok((bytes, metadata.permissions()))
}

fn snapshot_for(slug: &str, trigger: &Trigger) -> TriggerSnapshot {
    TriggerSnapshot {
        slug: slug.to_owned(),
        source: trigger.source.clone(),
        condition: trigger.condition.clone(),
        workflow: trigger.workflow.clone(),
        enabled: trigger.enabled,
        poll_every_minutes: trigger.poll_every_minutes,
        key_saved: true,
    }
}

fn entry_for(slug: &str, trigger: &Trigger) -> TriggerEntry {
    TriggerEntry {
        slug: slug.to_owned(),
        source: Some(trigger.source.clone()),
        condition: Some(trigger.condition.clone()),
        workflow: Some(trigger.workflow.clone()),
        enabled: Some(trigger.enabled),
        poll_every_minutes: Some(trigger.poll_every_minutes),
        key_saved: Some(true),
        problem: None,
    }
}

/// Probe nie przyjmuje `home`: w jego typie nie istnieje sciezka do kursora ani ledgeru.
pub fn test_connection_with<F>(api_key: &Secret, fetch: F) -> Result<(), TriggerError>
where
    F: FnOnce(&Secret, &str) -> Result<Vec<u8>, TriggerError>,
{
    if !valid_key(api_key.as_str()) {
        return Err(TriggerError::InvalidKey);
    }
    let answer = fetch(api_key, VIEWER_QUERY)?;
    parse_connection_response(&answer)
}

/// Produkcyjny probe uzywa tego samego bezpiecznego budowniczego curl co watcher.
pub fn test_connection(api_key: &Secret) -> Result<(), TriggerError> {
    test_connection_with(api_key, |key, query| {
        let output = build_linear_curl_command(key, query)
            .output()
            .map_err(TriggerError::Start)?;
        if !output.status.success() {
            return Err(TriggerError::CurlFailed(output.status.to_string()));
        }
        Ok(output.stdout)
    })
}

/// Przetwarza odpowiedz fetchera przez trwaly ledger identyfikatorow i dostaw.
///
/// Fetcher jest argumentem, zeby AC-4 moglo dowiesc, ze zajety rustowy uchwyt nie dotyka ani
/// sieci, ani plikow. Produkcja poda tu `curl`, a test licznik bez procesu.
pub fn poll_with<F>(
    home: &Path,
    slug: &str,
    created_at: i64,
    fetch: F,
) -> Result<TriggerPoll, TriggerError>
where
    F: FnOnce(&Trigger) -> Result<Vec<u8>, TriggerError>,
{
    let ledger_lock = ledger_lock_for(home, slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let trigger = load(home, slug)?;
    let mut ledger = read_ledger(home, slug)?;
    // 2026-08-21, T-65: poll zna tylko `home`, nie zaufany root projektu. Bound pozostaje
    // lokalnym Pending; jedyne pogodzenie `run.json` robi droga Startu po dowodzie sciezki.
    // Inaczej symlink w zapisanym run_file pozwolilby samemu watcherowi zaakceptowac obcy plik.
    if ledger.cursor_dirty
        && ledger.deliveries.iter().any(|record| {
            matches!(
                &record.state,
                DeliveryState::Pending | DeliveryState::Bound { .. }
            )
        })
    {
        // Ledger jest prawda o identyfikatorach. Po awarii kursora najpierw oddajemy juz
        // zapisana prace; nastepny czysty tick znow pobierze wszystkie unseen.
        ledger.cursor_dirty = false;
        write_ledger(home, slug, &ledger)?;
        return Ok(poll_fallback(&ledger));
    }
    if !trigger.enabled {
        return Ok(poll_fallback(&ledger));
    }

    // Lokalny Pending jest przygotowany PRZED siecia, ale udany fetch nadal przechodzi przez
    // caly batch i dopisuje wszystkie unseen. Tylko blad zewnetrznego serwisu schodzi do tej
    // trwalej pracy zamiast chowac ja za stanem offline.
    let local_pending = pending_poll(&ledger);
    let answer = match fetch(&trigger) {
        Ok(answer) => answer,
        Err(error) => {
            if let Some(pending) = local_pending {
                return Ok(pending);
            }
            return Err(error);
        }
    };
    let mut issues = deduplicate(parse_response(&answer)?);
    let newest = issues
        .iter()
        .map(|issue| issue.updated_at.as_str())
        .max()
        .map(str::to_owned);

    if !ledger.armed {
        ledger.armed = true;
        ledger
            .seen_ids
            .extend(issues.iter().map(|issue| issue.id.clone()));
        ledger.cursor_dirty = newest.is_some();
        write_ledger(home, slug, &ledger)?;
        if let Some(newest) = newest.as_deref() {
            write_cursor(&cursor_path(home, slug), newest)?;
            ledger.cursor_dirty = false;
            write_ledger(home, slug, &ledger)?;
        }
        return Ok(TriggerPoll::Armed);
    }

    let mut changed = false;
    for issue in issues.drain(..) {
        if ledger.seen_ids.insert(issue.id.clone()) {
            changed = true;
            ledger.deliveries.push(DeliveryRecord {
                delivery: TriggerDelivery {
                    claim: TriggerClaim {
                        slug: slug.to_owned(),
                        delivery_id: Uuid::now_v7().to_string(),
                        workflow: trigger.workflow.clone(),
                        run_id: Uuid::now_v7().to_string(),
                    },
                    issue,
                    created_at,
                },
                state: DeliveryState::Pending,
            });
        }
    }
    // Receipt wygrywa z kursorem: awaria drugiego zapisu zostawia pracę do odzyskania.
    if newest.is_some() {
        ledger.cursor_dirty = true;
        changed = true;
    }
    if changed {
        write_ledger(home, slug, &ledger)?;
    }
    if let Some(newest) = newest.as_deref() {
        write_cursor(&cursor_path(home, slug), newest)?;
        ledger.cursor_dirty = false;
        write_ledger(home, slug, &ledger)?;
    }
    Ok(poll_fallback(&ledger))
}

/// Prawdziwa krawedz watchera z wstrzyknietym wykonaniem procesu do deterministycznego testu.
pub fn poll_with_curl_runner<F>(
    home: &Path,
    slug: &str,
    created_at: i64,
    run: F,
) -> Result<TriggerPoll, TriggerError>
where
    F: FnOnce(Command, &str) -> Result<Vec<u8>, TriggerError>,
{
    poll_with(home, slug, created_at, |trigger| {
        let config = curl_config(trigger);
        run(build_curl_command(trigger), &config)
    })
}

/// Produkcyjny wariant [`poll_with`], którego fetcherem jest bezpieczna komenda `curl`.
pub fn poll(home: &Path, slug: &str, created_at: i64) -> Result<TriggerPoll, TriggerError> {
    poll_with_curl_runner(home, slug, created_at, |mut command, _| {
        let output = command.output().map_err(TriggerError::Start)?;
        if !output.status.success() {
            return Err(TriggerError::CurlFailed(output.status.to_string()));
        }
        Ok(output.stdout)
    })
}

/// Wiaze pending z dokladnym przyszlym `run.json`; ponowienie tego samego wiazania jest
/// idempotentne, a inny claim jest odmowa.
pub fn bind_delivery(
    home: &Path,
    claim: &TriggerClaim,
    run_file: &Path,
) -> Result<(), TriggerError> {
    let ledger_lock = ledger_lock_for(home, &claim.slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut ledger = read_ledger(home, &claim.slug)?;
    let record = exact_record_mut(&mut ledger, claim)?;
    match &record.state {
        DeliveryState::Pending => {
            record.state = DeliveryState::Bound {
                run_file: run_file.to_path_buf(),
            };
            write_ledger(home, &claim.slug, &ledger)
        }
        DeliveryState::Bound { run_file: found }
        | DeliveryState::Accepted {
            run_file: found, ..
        } if found == run_file => Ok(()),
        DeliveryState::Bound { .. } | DeliveryState::Accepted { .. } => {
            Err(TriggerError::InvalidClaim)
        }
        DeliveryState::Cancelled => Err(TriggerError::InvalidClaim),
    }
}

/// Cofa wiazanie, jezeli plan odmowil zanim powstal pierwszy `run.json`.
pub fn release_delivery(home: &Path, claim: &TriggerClaim) -> Result<(), TriggerError> {
    let ledger_lock = ledger_lock_for(home, &claim.slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut ledger = read_ledger(home, &claim.slug)?;
    let record = exact_record_mut(&mut ledger, claim)?;
    if matches!(&record.state, DeliveryState::Bound { .. }) {
        record.state = DeliveryState::Pending;
        write_ledger(home, &claim.slug, &ledger)?;
    }
    Ok(())
}

/// Domyka ledger dopiero po atomowym pierwszym `run.json`.
pub fn accept_delivery(
    home: &Path,
    claim: &TriggerClaim,
    run_file: &Path,
    accepted_at: i64,
) -> Result<(), TriggerError> {
    let ledger_lock = ledger_lock_for(home, &claim.slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut ledger = read_ledger(home, &claim.slug)?;
    accept_in_ledger(&mut ledger, claim, run_file, accepted_at)?;
    write_ledger(home, &claim.slug, &ledger)
}

/// Godzi `bound` po restarcie: dopiero pasujacy `run.json`, ktorego plik i katalog przeszedl
/// wstrzyknieta granice durability, staje sie `accepted`; brak zostawia ten sam claim i UUID.
pub fn reconcile_delivery<F>(
    home: &Path,
    claim: &TriggerClaim,
    durable_read: F,
) -> Result<DeliveryState, TriggerError>
where
    F: FnOnce(&Path) -> io::Result<Option<Vec<u8>>>,
{
    let ledger_lock = ledger_lock_for(home, &claim.slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut ledger = read_ledger(home, &claim.slug)?;
    let before = delivery_state(&ledger, claim)?.clone();
    reconcile_one(&mut ledger, claim, durable_read)?;
    let after = delivery_state(&ledger, claim)?.clone();
    if after != before {
        write_ledger(home, &claim.slug, &ledger)?;
    }
    Ok(after)
}

/// Trwaly receipt, ktory wolno pokazac podczas innego biegu bez fetchu i bez zapisu.
/// Pending albo bound ma pierwszenstwo: wtedy UI dostaje zwykle `busy`, nie stary sukces.
pub fn accepted_while_busy(home: &Path, slug: &str) -> Result<Option<TriggerPoll>, TriggerError> {
    let ledger_lock = ledger_lock_for(home, slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let ledger = read_ledger(home, slug)?;
    if pending_deliveries(&ledger).next().is_some() {
        return Ok(None);
    }
    Ok(accepted_poll(&ledger))
}

/// Odtwarza pelna dostawe po zredagowanym claimie i odmawia kazdej roznicy w czterech polach.
pub fn claimed_delivery(
    home: &Path,
    claim: &TriggerClaim,
) -> Result<TriggerDelivery, TriggerError> {
    let ledger_lock = ledger_lock_for(home, &claim.slug)?;
    let _guard = ledger_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let ledger = read_ledger(home, &claim.slug)?;
    let record = exact_record(&ledger, claim)?;
    if matches!(&record.state, DeliveryState::Cancelled) {
        return Err(TriggerError::InvalidClaim);
    }
    Ok(record.delivery.clone())
}

const LEDGER_SCHEMA: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema: u32,
    armed: bool,
    #[serde(default)]
    cursor_dirty: bool,
    #[serde(default)]
    seen_ids: BTreeSet<String>,
    #[serde(default)]
    deliveries: Vec<DeliveryRecord>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            schema: LEDGER_SCHEMA,
            armed: false,
            cursor_dirty: false,
            seen_ids: BTreeSet::new(),
            deliveries: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryRecord {
    delivery: TriggerDelivery,
    state: DeliveryState,
}

#[derive(Deserialize)]
struct RunReceipt {
    id: String,
    created_at: i64,
    trigger_origin: TriggerOrigin,
}

fn existing_trigger_dir(home: &Path) -> Result<Option<PathBuf>, TriggerError> {
    let dir = home.join(TRIGGERS_DIR);
    match fs::symlink_metadata(&dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(TriggerError::TriggerDirectory(error)),
        Ok(metadata) if metadata.file_type().is_dir() => Ok(Some(dir)),
        Ok(_) => Err(TriggerError::UnsafeTriggerDirectory),
    }
}

fn require_trigger_dir(home: &Path) -> Result<PathBuf, TriggerError> {
    existing_trigger_dir(home)?.ok_or(TriggerError::MissingConfig)
}

fn ensure_trigger_dir(home: &Path) -> Result<PathBuf, TriggerError> {
    if let Some(dir) = existing_trigger_dir(home)? {
        return Ok(dir);
    }
    fs::create_dir_all(home).map_err(TriggerError::TriggerDirectory)?;
    let dir = home.join(TRIGGERS_DIR);
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(TriggerError::TriggerDirectory(error)),
    }
    existing_trigger_dir(home)?.ok_or(TriggerError::UnsafeTriggerDirectory)
}

fn require_regular_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "trigger folder is not a regular directory",
        ))
    }
}

fn ledger_locks() -> &'static Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn ledger_lock_for(home: &Path, slug: &str) -> Result<Arc<Mutex<()>>, TriggerError> {
    valid_slug(slug)?;
    let key = home.join(TRIGGERS_DIR).join(slug);
    let mut locks = ledger_locks()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    Ok(Arc::clone(
        locks.entry(key).or_insert_with(|| Arc::new(Mutex::new(()))),
    ))
}

fn config_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn config_guard() -> MutexGuard<'static, ()> {
    config_lock().lock().unwrap_or_else(PoisonError::into_inner)
}

fn ledger_path(home: &Path, slug: &str) -> Result<PathBuf, TriggerError> {
    valid_slug(slug)?;
    Ok(home.join(TRIGGERS_DIR).join(format!(".{slug}.ledger.json")))
}

fn read_ledger(home: &Path, slug: &str) -> Result<Ledger, TriggerError> {
    if existing_trigger_dir(home)?.is_none() {
        return Ok(Ledger::default());
    }
    let path = ledger_path(home, slug)?;
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Ledger::default()),
        Err(error) => return Err(TriggerError::ReadLedger(error)),
    };
    parse_ledger(&raw)
}

fn parse_ledger(raw: &[u8]) -> Result<Ledger, TriggerError> {
    let ledger: Ledger = serde_json::from_slice(raw).map_err(TriggerError::InvalidLedger)?;
    if ledger.schema != LEDGER_SCHEMA {
        return Err(TriggerError::UnsupportedLedgerSchema);
    }
    Ok(ledger)
}

fn write_ledger(home: &Path, slug: &str, ledger: &Ledger) -> Result<(), TriggerError> {
    ensure_trigger_dir(home)?;
    let path = ledger_path(home, slug)?;
    let bytes = serde_json::to_vec_pretty(ledger).map_err(TriggerError::InvalidLedger)?;
    let permissions = fs::metadata(&path).ok().map(|meta| meta.permissions());
    replace_atomically(&path, &bytes, permissions, None, &mut |_, _| Ok(()))
        .map(|_| ())
        .map_err(TriggerError::WriteLedger)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replaced {
    Saved,
    Changed,
}

fn replace_atomically<F>(
    path: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
    expected: Option<&[u8]>,
    observe: &mut F,
) -> io::Result<Replaced>
where
    F: FnMut(ToggleStage, &Path) -> io::Result<()>,
{
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("file has no parent directory"))?;
    require_regular_directory(parent)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temp = parent.join(format!(
        ".{}-{}-{nonce}.writing",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    let result = (|| -> io::Result<Replaced> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        // Prawa ida na UCHWYT przed pierwszym bajtem. Okno z sekretem w pliku o prawach
        // wynikajacych z umasku nie istnieje nawet wtedy, gdy zapis duzego pliku trwa dlugo.
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        observe(ToggleStage::BeforeContent, &temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // Windows nie pozwala przeniesc ani posprzatac otwartego pliku tak swobodnie jak
        // macOS. Trwala zawartosc jest juz na dysku, wiec zamykamy uchwyt przed compare/rename.
        drop(file);
        if let Some(expected) = expected {
            observe(ToggleStage::BeforeCompare, &temp)?;
            let current = match fs::read(path) {
                Ok(current) => current,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Ok(Replaced::Changed);
                }
                Err(error) => return Err(error),
            };
            if current != expected {
                return Ok(Replaced::Changed);
            }
            // Granica przenosnosci: std nie daje compare-and-swap dla nazwy pliku. Wszystkie
            // zapisy Loadoutu sa pod `config_lock`, a reczna edycje wykrywamy w ostatnim
            // mozliwym odczycie; obcy proces moze jeszcze trafic w kilka instrukcji do rename.
        }
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()?;
        Ok(Replaced::Saved)
    })();
    if !matches!(&result, Ok(Replaced::Saved)) {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn deduplicate(issues: Vec<Issue>) -> Vec<Issue> {
    let mut by_id = BTreeMap::<String, Issue>::new();
    for issue in issues {
        match by_id.get(&issue.id) {
            Some(previous) if previous.updated_at >= issue.updated_at => {}
            _ => {
                by_id.insert(issue.id.clone(), issue);
            }
        }
    }
    let mut issues = by_id.into_values().collect::<Vec<_>>();
    issues.sort_by(|left, right| {
        left.updated_at
            .cmp(&right.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    issues
}

fn poll_fallback(ledger: &Ledger) -> TriggerPoll {
    if let Some(pending) = pending_poll(ledger) {
        return pending;
    }
    accepted_poll(ledger).unwrap_or(TriggerPoll::Armed)
}

fn pending_poll(ledger: &Ledger) -> Option<TriggerPoll> {
    pending_deliveries(ledger)
        .next()
        .map(|record| TriggerPoll::Pending {
            delivery: Box::new(record.delivery.clone()),
        })
}

/// Jedna definicja "jeszcze nie przyjete": `pending` i crash-window `bound` sa dalej praca,
/// ktora musi wygrac z historycznym receipt w pollu oraz w busy guardzie.
fn pending_deliveries(ledger: &Ledger) -> impl Iterator<Item = &DeliveryRecord> {
    ledger.deliveries.iter().filter(|record| {
        matches!(
            &record.state,
            DeliveryState::Pending | DeliveryState::Bound { .. }
        )
    })
}

fn accepted_poll(ledger: &Ledger) -> Option<TriggerPoll> {
    ledger
        .deliveries
        .iter()
        .filter_map(|record| match &record.state {
            DeliveryState::Accepted { accepted_at, .. } => {
                Some((*accepted_at, record.delivery.claim.workflow.as_str()))
            }
            DeliveryState::Pending | DeliveryState::Bound { .. } | DeliveryState::Cancelled => None,
        })
        .max_by_key(|(accepted_at, _)| *accepted_at)
        .map(|(receipt_at, workflow)| TriggerPoll::Accepted {
            workflow: workflow.to_owned(),
            receipt_at,
        })
}

fn exact_record<'a>(
    ledger: &'a Ledger,
    claim: &TriggerClaim,
) -> Result<&'a DeliveryRecord, TriggerError> {
    ledger
        .deliveries
        .iter()
        .find(|record| record.delivery.claim == *claim)
        .ok_or(TriggerError::InvalidClaim)
}

/// Stan claimu czytany przez crash reconciliation z tego samego dokladnego dopasowania,
/// ktorego uzywa bind i akceptacja; nie istnieje publiczna sonda tylko dla testu.
fn delivery_state<'a>(
    ledger: &'a Ledger,
    claim: &TriggerClaim,
) -> Result<&'a DeliveryState, TriggerError> {
    Ok(&exact_record(ledger, claim)?.state)
}

fn exact_record_mut<'a>(
    ledger: &'a mut Ledger,
    claim: &TriggerClaim,
) -> Result<&'a mut DeliveryRecord, TriggerError> {
    ledger
        .deliveries
        .iter_mut()
        .find(|record| record.delivery.claim == *claim)
        .ok_or(TriggerError::InvalidClaim)
}

fn read_run(path: &Path) -> Result<RunReceipt, TriggerError> {
    let raw = fs::read(path).map_err(TriggerError::ReadRun)?;
    serde_json::from_slice(&raw).map_err(TriggerError::InvalidRun)
}

fn receipt_matches(
    receipt: &RunReceipt,
    record: &DeliveryRecord,
    run_file: &Path,
) -> Result<(), TriggerError> {
    let expected_file = match &record.state {
        DeliveryState::Bound { run_file } | DeliveryState::Accepted { run_file, .. } => run_file,
        DeliveryState::Pending | DeliveryState::Cancelled => {
            return Err(TriggerError::InvalidClaim);
        }
    };
    let claim = &record.delivery.claim;
    let origin = &receipt.trigger_origin;
    if expected_file != run_file
        || receipt.id != claim.run_id
        || receipt.created_at != record.delivery.created_at
        || origin.slug != claim.slug
        || origin.delivery_id != claim.delivery_id
        || origin.issue_id != record.delivery.issue.id
    {
        return Err(TriggerError::RunMismatch);
    }
    Ok(())
}

fn accept_in_ledger(
    ledger: &mut Ledger,
    claim: &TriggerClaim,
    run_file: &Path,
    accepted_at: i64,
) -> Result<(), TriggerError> {
    let record = exact_record_mut(ledger, claim)?;
    if matches!(&record.state, DeliveryState::Accepted { .. }) {
        receipt_matches(&read_run(run_file)?, record, run_file)?;
        return Ok(());
    }
    let receipt = read_run(run_file)?;
    receipt_matches(&receipt, record, run_file)?;
    record.state = DeliveryState::Accepted {
        run_file: run_file.to_path_buf(),
        accepted_at,
    };
    Ok(())
}

fn reconcile_one<F>(
    ledger: &mut Ledger,
    claim: &TriggerClaim,
    durable_read: F,
) -> Result<(), TriggerError>
where
    F: FnOnce(&Path) -> io::Result<Option<Vec<u8>>>,
{
    let record = exact_record(ledger, claim)?;
    let run_file = match &record.state {
        DeliveryState::Bound { run_file } => run_file.clone(),
        DeliveryState::Pending | DeliveryState::Accepted { .. } | DeliveryState::Cancelled => {
            return Ok(());
        }
    };
    // Czytamy i syncujemy przez jeden produkcyjny seam. Dzięki temu bajty, które ponizej
    // autoryzuja Accepted, pochodza z tego samego otwartego pliku, ktory przeszedl durability.
    let Some(raw) = durable_read(&run_file).map_err(TriggerError::RunDurability)? else {
        return Ok(());
    };
    let receipt: RunReceipt = serde_json::from_slice(&raw).map_err(TriggerError::InvalidRun)?;
    let accepted_at = receipt.created_at;
    receipt_matches(&receipt, record, &run_file)?;
    exact_record_mut(ledger, claim)?.state = DeliveryState::Accepted {
        run_file,
        accepted_at,
    };
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TriggerWire {
    schema: u32,
    source: String,
    enabled: bool,
    workflow: String,
    condition: String,
    #[serde(default = "default_poll_every_minutes")]
    poll_every_minutes: u32,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct Answer {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Deserialize)]
struct ViewerAnswer {
    data: Option<ViewerData>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Deserialize)]
struct ViewerData {
    viewer: Option<Viewer>,
}

#[derive(Deserialize)]
struct Viewer {
    id: String,
}

#[derive(Deserialize)]
struct Data {
    issues: Issues,
}

#[derive(Deserialize)]
struct Issues {
    nodes: Vec<IssueWire>,
}

#[derive(Deserialize)]
struct ApiError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueWire {
    id: String,
    identifier: String,
    title: String,
    url: String,
    description: Option<String>,
    updated_at: String,
}

pub fn load(home: &Path, slug: &str) -> Result<Trigger, TriggerError> {
    valid_slug(slug)?;
    let path = require_trigger_dir(home)?.join(format!("{slug}.json"));
    let (raw, _) = read_regular_config(&path)?;
    parse_trigger(&raw)
}

fn parse_trigger(raw: &[u8]) -> Result<Trigger, TriggerError> {
    let wire: TriggerWire = serde_json::from_slice(raw).map_err(TriggerError::InvalidConfig)?;
    if wire.schema != TRIGGER_SCHEMA {
        return Err(TriggerError::UnsupportedSchema);
    }
    let source = match wire.source.as_str() {
        "linear" => Source::Linear,
        unknown => {
            let error = public_unsupported_source(unknown)
                .map_or(TriggerError::UnknownSourceRedacted, |name| {
                    TriggerError::UnknownSource(name.to_owned())
                });
            return Err(error);
        }
    };
    let api_key = wire
        .api_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(TriggerError::MissingKey)?;
    if !valid_key(&api_key) {
        return Err(TriggerError::InvalidKey);
    }
    // 2026-08-21, T-74: T-65 zapisywalo czytelne `assigned to me`. Loader utrzymuje te
    // istniejace pliki, lecz formularz i kazdy nowy zapis pozostaja przy jednym kanonie.
    let condition = match wire.condition.as_str() {
        "assigned-to-me" | "assigned to me" => "assigned-to-me".to_owned(),
        _ => return Err(TriggerError::InvalidCondition),
    };
    if !valid_cadence(wire.poll_every_minutes) {
        return Err(TriggerError::InvalidCadence);
    }
    Ok(Trigger {
        schema: wire.schema,
        source,
        enabled: wire.enabled,
        workflow: wire.workflow,
        condition,
        poll_every_minutes: wire.poll_every_minutes,
        api_key: Secret::new(api_key),
    })
}

fn public_unsupported_source(source: &str) -> Option<&'static str> {
    /* T-64 wymaga nazwac bezpieczna nazwe zrodla, ale T-65 ujawnilo, ze dowolny regex nazwy
     * przepusci tez jakis sekret. Odbijamy tylko jawne nazwy integracji, nigdy surowe pole. */
    match source {
        "clickup" => Some("clickup"),
        "jira" => Some("jira"),
        "slack" => Some("slack"),
        _ => None,
    }
}

#[must_use]
pub fn curl_config(trigger: &Trigger) -> String {
    linear_curl_config(&trigger.api_key, ISSUES_QUERY)
}

/// Jeden budowniczy konfiguracji dla watchera i probe. Klucz oraz query zostaja na stdin.
#[must_use]
pub fn linear_curl_config(api_key: &Secret, query: &str) -> String {
    let body = serde_json::json!({ "query": query }).to_string();
    format!(
        "proto = \"=https\"\nmax-time = \"{TIMEOUT_SECONDS}\"\nfail\nsilent\nshow-error\nurl = \"{API}\"\nrequest = \"POST\"\nheader = \"Content-Type: application/json\"\nheader = \"Authorization: {}\"\ndata = {}\n",
        api_key.as_str(),
        curl_quote(&body),
    )
}

/// Proces curl dla dowolnego stalego zapytania Lineara; argv i env nie niosa sekretu.
#[must_use]
pub fn build_linear_curl_command(api_key: &Secret, query: &str) -> Command {
    let mut command = Command::new("curl");
    command.arg("--config").arg("-");
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    match config_on_stdin(&linear_curl_config(api_key, query)) {
        Ok(reader) => command.stdin(reader),
        Err(_) => command.stdin(Stdio::null()),
    };
    command
}

#[must_use]
pub fn build_curl_command(trigger: &Trigger) -> Command {
    build_linear_curl_command(&trigger.api_key, ISSUES_QUERY)
}

fn parse_connection_response(bytes: &[u8]) -> Result<(), TriggerError> {
    if bytes.is_empty() {
        return Err(TriggerError::EmptyAnswer);
    }
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'<')
    {
        return Err(TriggerError::HtmlAnswer);
    }
    let answer: ViewerAnswer =
        serde_json::from_slice(bytes).map_err(TriggerError::InvalidAnswer)?;
    // Linear moze odbic dowolny fragment zadania w `message`, takze sekret. Probe zwraca
    // wlasne naprawialne zdanie i nigdy nie przenosi tekstu serwera przez IPC.
    if !answer.errors.is_empty() {
        return Err(TriggerError::ConnectionRefused);
    }
    let id = answer
        .data
        .and_then(|data| data.viewer)
        .map(|viewer| viewer.id)
        .filter(|id| !id.trim().is_empty())
        .ok_or(TriggerError::MissingViewer)?;
    drop(id);
    Ok(())
}

pub fn parse_response(bytes: &[u8]) -> Result<Vec<Issue>, TriggerError> {
    if bytes.is_empty() {
        return Err(TriggerError::EmptyAnswer);
    }
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'<')
    {
        return Err(TriggerError::HtmlAnswer);
    }
    let answer: Answer = serde_json::from_slice(bytes).map_err(TriggerError::InvalidAnswer)?;
    if !answer.errors.is_empty() {
        // Wiadomosc serwera jest niezaufana i moze odbic Authorization. Stale zdanie jest
        // jedyna wartoscia, ktora wolno pokazac i zapisac przez `refused`.
        return Err(TriggerError::Api);
    }
    let data = answer.data.ok_or(TriggerError::MissingIssues)?;
    Ok(data
        .issues
        .nodes
        .into_iter()
        .map(|issue| Issue {
            id: issue.id,
            identifier: issue.identifier,
            title: issue.title,
            url: issue.url,
            body: issue.description.unwrap_or_default(),
            updated_at: issue.updated_at,
        })
        .collect())
}

#[must_use]
pub fn cursor_path(home: &Path, slug: &str) -> PathBuf {
    home.join(TRIGGERS_DIR).join(format!(".{slug}.cursor"))
}

/// Ukryta nazwa oznaczajaca, ze ledger jest zakonczony, a cleanup Delete da sie ponowic.
#[must_use]
pub fn tombstone_path(home: &Path, slug: &str) -> PathBuf {
    home.join(TRIGGERS_DIR)
        .join(format!(".{slug}.deleted.json"))
}

pub fn check_answer(home: &Path, slug: &str, answer: &[u8]) -> Result<Option<Issue>, TriggerError> {
    valid_slug(slug)?;
    require_trigger_dir(home)?;
    let issues = parse_response(answer)?;
    let path = cursor_path(home, slug);
    let cursor = match fs::read_to_string(&path) {
        Ok(cursor) => Some(cursor.trim().to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(TriggerError::ReadCursor(error)),
    };
    let newest = issues
        .into_iter()
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at));
    let Some(newest) = newest else {
        return Ok(None);
    };
    if cursor
        .as_deref()
        .is_none_or(|seen| newest.updated_at.as_str() > seen)
    {
        write_cursor(&path, &newest.updated_at)?;
        return Ok(cursor.map(|_| newest));
    }
    Ok(None)
}

pub fn check(home: &Path, slug: &str) -> Result<Option<Issue>, TriggerError> {
    let trigger = load(home, slug)?;
    if !trigger.enabled {
        return Ok(None);
    }
    let output = build_curl_command(&trigger)
        .output()
        .map_err(TriggerError::Start)?;
    if !output.status.success() {
        return Err(TriggerError::CurlFailed(output.status.to_string()));
    }
    check_answer(home, slug, &output.stdout)
}

fn valid_slug(slug: &str) -> Result<(), TriggerError> {
    if slug.is_empty()
        || slug.starts_with('.')
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(TriggerError::BadSlug);
    }
    Ok(())
}

fn valid_key(key: &str) -> bool {
    key.starts_with("lin_api_")
        && key.len() >= 40
        && key.len() <= MAX_API_KEY_BYTES
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

const fn valid_cadence(minutes: u32) -> bool {
    matches!(minutes, 1 | 5 | 15 | 60)
}

fn curl_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn config_on_stdin(config: &str) -> io::Result<io::PipeReader> {
    let (reader, mut writer) = io::pipe()?;
    writer.write_all(config.as_bytes())?;
    Ok(reader)
}

fn write_cursor(path: &Path, value: &str) -> Result<(), TriggerError> {
    let parent = path.parent().ok_or_else(|| {
        TriggerError::WriteCursor(io::Error::other("cursor has no parent directory"))
    })?;
    require_regular_directory(parent).map_err(TriggerError::WriteCursor)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temp = parent.join(format!(".cursor-{}-{nonce}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = File::create(&temp)?;
        writeln!(file, "{value}")?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(TriggerError::WriteCursor)
}
