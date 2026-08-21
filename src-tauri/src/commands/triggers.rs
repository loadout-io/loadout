//! Triggery zapisane w `~/.loadout/triggers/`: plik jest konfiguracja i prawda o kursorze.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TriggerPoll {
    /// Rust odmowil przed zapytaniem zewnetrznego serwisu, bo jeden bieg juz ma uchwyt.
    Busy,
    /// Pierwszy odczyt zapisal zastany backlog jako widziany i niczego nie uruchomil.
    Armed,
    /// Jedna dostawa czeka na przejscie istniejaca droga Startu.
    Pending {
        /// Dostawa wraz z claimem przyszlego biegu.
        delivery: TriggerDelivery,
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
#[serde(rename_all = "camelCase", tag = "status")]
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
    #[error("The trigger source `{0}` is not available. Choose `linear`.")]
    UnknownSource(String),
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
    #[error("Loadout could not start curl for the Linear trigger: {0}")]
    Start(io::Error),
    #[error("Linear could not be checked because curl exited with status {0}.")]
    CurlFailed(String),
}

/// Wypisuje cala biblioteke bez sekretow, lacznie z nazwanymi problemami pojedynczych plikow.
pub fn list(_home: &Path) -> Result<Vec<TriggerEntry>, TriggerError> {
    todo!("T-65 AC-7: list the redacted trigger library")
}

/// Atomowo zmienia `enabled`, zachowujac sekret, pozostale pola i prawa pliku.
pub fn set_enabled(
    _home: &Path,
    _slug: &str,
    _enabled: bool,
) -> Result<TriggerEntry, TriggerError> {
    todo!("T-65 AC-7: persist the trigger switch")
}

/// Przetwarza odpowiedz fetchera przez trwaly ledger identyfikatorow i dostaw.
///
/// Fetcher jest argumentem, zeby AC-4 moglo dowiesc, ze zajety rustowy uchwyt nie dotyka ani
/// sieci, ani plikow. Produkcja poda tu `curl`, a test licznik bez procesu.
pub fn poll_with<F>(
    _home: &Path,
    _slug: &str,
    _created_at: i64,
    _fetch: F,
) -> Result<TriggerPoll, TriggerError>
where
    F: FnOnce(&Trigger) -> Result<Vec<u8>, TriggerError>,
{
    todo!("T-65 AC-4/8: stage every unseen issue before advancing the cursor")
}

/// Produkcyjny wariant [`poll_with`], którego fetcherem jest bezpieczna komenda `curl`.
pub fn poll(_home: &Path, _slug: &str, _created_at: i64) -> Result<TriggerPoll, TriggerError> {
    todo!("T-65 AC-4/8: poll Linear through the durable delivery ledger")
}

/// Wszystkie oczekujace dostawy sluga w deterministycznej kolejnosci.
pub fn pending_deliveries(_home: &Path, _slug: &str) -> Result<Vec<TriggerDelivery>, TriggerError> {
    todo!("T-65 AC-8: recover pending deliveries from files")
}

/// Wiaze pending z dokladnym przyszlym `run.json`; ponowienie tego samego wiazania jest
/// idempotentne, a inny claim jest odmowa.
pub fn bind_delivery(
    _home: &Path,
    _claim: &TriggerClaim,
    _run_file: &Path,
) -> Result<(), TriggerError> {
    todo!("T-65 AC-8: durably bind a delivery to its preallocated run")
}

/// Cofa wiazanie, jezeli plan odmowil zanim powstal pierwszy `run.json`.
pub fn release_delivery(_home: &Path, _claim: &TriggerClaim) -> Result<(), TriggerError> {
    todo!("T-65 AC-8: leave a refused workflow pending")
}

/// Domyka ledger dopiero po atomowym pierwszym `run.json`.
pub fn accept_delivery(
    _home: &Path,
    _claim: &TriggerClaim,
    _run_file: &Path,
    _accepted_at: i64,
) -> Result<(), TriggerError> {
    todo!("T-65 AC-8: accept only after the run file exists")
}

/// Godzi `bound` po restarcie: istniejacy, pasujacy `run.json` staje sie `accepted`, a jego
/// brak zostawia ten sam claim i UUID do ponowienia.
pub fn reconcile_delivery(
    _home: &Path,
    _claim: &TriggerClaim,
) -> Result<DeliveryState, TriggerError> {
    todo!("T-65 AC-8: reconcile a crash from files without starting twice")
}

/// Odczytuje stan jednej dostawy do testu granicy akceptacji i do komunikatu zegara.
pub fn delivery_state(_home: &Path, _claim: &TriggerClaim) -> Result<DeliveryState, TriggerError> {
    todo!("T-65 AC-8: read the durable delivery state")
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
    let wire: TriggerWire = serde_json::from_slice(&raw).map_err(TriggerError::InvalidConfig)?;
    let source = match wire.source.as_str() {
        "linear" => Source::Linear,
        other => return Err(TriggerError::UnknownSource(other.to_owned())),
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
