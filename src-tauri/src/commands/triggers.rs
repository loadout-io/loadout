//! Triggery zapisane w `~/.loadout/triggers/`: plik jest konfiguracja i prawda o kursorze.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const TRIGGERS_DIR: &str = "triggers";
const API: &str = "https://api.linear.app/graphql";
const TIMEOUT_SECONDS: u64 = 20;
const QUERY: &str = "query AssignedToMe { issues(filter: { assignee: { isMe: { eq: true } } }, orderBy: updatedAt) { nodes { id identifier title url description updatedAt } } }";

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
    pub api_key: Secret,
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
    #[error("This trigger source is not available. Choose `linear`.")]
    UnknownSource,
    #[error("The Linear API key has an invalid shape. Replace it with a `lin_api_...` key.")]
    InvalidKey,
    #[error("Linear returned an empty response. Check the connection and try again.")]
    EmptyAnswer,
    #[error("Linear returned a web page instead of issue data. Check the API address.")]
    HtmlAnswer,
    #[error("Linear returned data that is not JSON: {0}")]
    InvalidAnswer(serde_json::Error),
    #[error("Linear refused the request: {0}")]
    Api(String),
    #[error("Linear's response did not contain an issue list.")]
    MissingIssues,
    #[error("Loadout could not read the trigger cursor: {0}")]
    ReadCursor(io::Error),
    #[error("Loadout could not save the trigger cursor, so it did not accept the issue: {0}")]
    WriteCursor(io::Error),
    #[error("Loadout could not read the trigger library: {0}")]
    ReadLibrary(io::Error),
    #[error("Loadout could not save the trigger file: {0}")]
    WriteConfig(io::Error),
    #[error(
        "This trigger changed while Loadout was switching it. Review the file, then try again."
    )]
    ConfigChanged,
    #[error("Loadout could not read the trigger delivery ledger: {0}")]
    ReadLedger(io::Error),
    #[error("The trigger delivery ledger is invalid: {0}")]
    InvalidLedger(serde_json::Error),
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

/// Wypisuje cala biblioteke bez sekretow, lacznie z nazwanymi problemami pojedynczych plikow.
pub fn list(home: &Path) -> Result<Vec<TriggerEntry>, TriggerError> {
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
        if !kind.is_file() || name.starts_with('.') || !is_json {
            continue;
        }
        let slug = name.trim_end_matches(".json").to_owned();
        match load(home, &slug) {
            Ok(trigger) => out.push(TriggerEntry {
                slug,
                source: Some(trigger.source),
                condition: Some(trigger.condition),
                workflow: Some(trigger.workflow),
                enabled: Some(trigger.enabled),
                problem: None,
            }),
            Err(error) => out.push(TriggerEntry {
                slug,
                source: None,
                condition: None,
                workflow: None,
                enabled: None,
                // Biblioteka jest granica redakcji. Nawet wartosc wpisana omylkowo w `source`
                // nie moze stac sie komunikatem, bo mogla byc sekretem w zlym polu.
                problem: Some(library_problem(&error)),
            }),
        }
    }
    out.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(out)
}

fn library_problem(error: &TriggerError) -> String {
    match error {
        TriggerError::ReadConfig(_) => "Loadout could not read this trigger file.".to_owned(),
        TriggerError::InvalidConfig(_) => "This trigger file is not valid JSON.".to_owned(),
        TriggerError::MissingKey | TriggerError::InvalidKey => {
            "This trigger needs a valid Linear key.".to_owned()
        }
        TriggerError::UnknownSource => {
            "This trigger uses an unavailable source. Choose linear.".to_owned()
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
    let path = home.join(TRIGGERS_DIR).join(format!("{slug}.json"));
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
        problem: None,
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
    let _guard = ledger_guard();
    let trigger = load(home, slug)?;
    let mut ledger = read_ledger(home, slug)?;
    // 2026-08-21, T-65: poll zna tylko `home`, nie zaufany root projektu. Bound pozostaje
    // lokalnym Pending; jedyne pogodzenie `run.json` robi droga Startu po dowodzie sciezki.
    // Inaczej symlink w zapisanym run_file pozwolilby samemu watcherowi zaakceptowac obcy plik.
    if ledger.cursor_dirty
        && ledger
            .deliveries
            .iter()
            .any(|record| !matches!(&record.state, DeliveryState::Accepted { .. }))
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

/// Produkcyjny wariant [`poll_with`], którego fetcherem jest bezpieczna komenda `curl`.
pub fn poll(home: &Path, slug: &str, created_at: i64) -> Result<TriggerPoll, TriggerError> {
    poll_with(home, slug, created_at, |trigger| {
        let output = build_curl_command(trigger)
            .output()
            .map_err(TriggerError::Start)?;
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
    let _guard = ledger_guard();
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
    }
}

/// Cofa wiazanie, jezeli plan odmowil zanim powstal pierwszy `run.json`.
pub fn release_delivery(home: &Path, claim: &TriggerClaim) -> Result<(), TriggerError> {
    let _guard = ledger_guard();
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
    let _guard = ledger_guard();
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
    let _guard = ledger_guard();
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
    let _guard = ledger_guard();
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
    let _guard = ledger_guard();
    let ledger = read_ledger(home, &claim.slug)?;
    Ok(exact_record(&ledger, claim)?.delivery.clone())
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

fn ledger_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn config_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn config_guard() -> MutexGuard<'static, ()> {
    config_lock().lock().unwrap_or_else(PoisonError::into_inner)
}

fn ledger_guard() -> MutexGuard<'static, ()> {
    ledger_lock().lock().unwrap_or_else(PoisonError::into_inner)
}

fn ledger_path(home: &Path, slug: &str) -> Result<PathBuf, TriggerError> {
    valid_slug(slug)?;
    Ok(home.join(TRIGGERS_DIR).join(format!(".{slug}.ledger.json")))
}

fn read_ledger(home: &Path, slug: &str) -> Result<Ledger, TriggerError> {
    let path = ledger_path(home, slug)?;
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Ledger::default()),
        Err(error) => return Err(TriggerError::ReadLedger(error)),
    };
    serde_json::from_slice(&raw).map_err(TriggerError::InvalidLedger)
}

fn write_ledger(home: &Path, slug: &str, ledger: &Ledger) -> Result<(), TriggerError> {
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
    fs::create_dir_all(parent)?;
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
    ledger
        .deliveries
        .iter()
        .filter(|record| !matches!(&record.state, DeliveryState::Accepted { .. }))
}

fn accepted_poll(ledger: &Ledger) -> Option<TriggerPoll> {
    ledger
        .deliveries
        .iter()
        .filter_map(|record| match &record.state {
            DeliveryState::Accepted { accepted_at, .. } => {
                Some((*accepted_at, record.delivery.claim.workflow.as_str()))
            }
            DeliveryState::Pending | DeliveryState::Bound { .. } => None,
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
        DeliveryState::Pending => return Err(TriggerError::InvalidClaim),
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
        DeliveryState::Pending | DeliveryState::Accepted { .. } => {
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
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct Answer {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<ApiError>,
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
struct ApiError {
    message: Option<String>,
}

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
    let raw = fs::read(home.join(TRIGGERS_DIR).join(format!("{slug}.json")))
        .map_err(TriggerError::ReadConfig)?;
    parse_trigger(&raw)
}

fn parse_trigger(raw: &[u8]) -> Result<Trigger, TriggerError> {
    let wire: TriggerWire = serde_json::from_slice(raw).map_err(TriggerError::InvalidConfig)?;
    let source = match wire.source.as_str() {
        "linear" => Source::Linear,
        _ => return Err(TriggerError::UnknownSource),
    };
    let api_key = wire
        .api_key
        .filter(|key| !key.trim().is_empty())
        .ok_or(TriggerError::MissingKey)?;
    if !valid_key(&api_key) {
        return Err(TriggerError::InvalidKey);
    }
    Ok(Trigger {
        schema: wire.schema,
        source,
        enabled: wire.enabled,
        workflow: wire.workflow,
        condition: wire.condition,
        api_key: Secret::new(api_key),
    })
}

#[must_use]
pub fn curl_config(trigger: &Trigger) -> String {
    let body = serde_json::json!({ "query": QUERY }).to_string();
    format!(
        "proto = \"=https\"\nmax-time = \"{TIMEOUT_SECONDS}\"\nfail\nsilent\nshow-error\nurl = \"{API}\"\nrequest = \"POST\"\nheader = \"Content-Type: application/json\"\nheader = \"Authorization: {}\"\ndata = {}\n",
        trigger.api_key.as_str(),
        curl_quote(&body),
    )
}

#[must_use]
pub fn build_curl_command(trigger: &Trigger) -> Command {
    let mut command = Command::new("curl");
    command.arg("--config").arg("-");
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    match config_on_stdin(&curl_config(trigger)) {
        Ok(reader) => command.stdin(reader),
        Err(_) => command.stdin(Stdio::null()),
    };
    command
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
        let said = answer
            .errors
            .into_iter()
            .filter_map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(TriggerError::Api(if said.is_empty() {
            "the API returned an error".to_owned()
        } else {
            said
        }));
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

pub fn check_answer(home: &Path, slug: &str, answer: &[u8]) -> Result<Option<Issue>, TriggerError> {
    valid_slug(slug)?;
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
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
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
    fs::create_dir_all(parent).map_err(TriggerError::WriteCursor)?;
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
