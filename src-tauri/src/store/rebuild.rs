//! Czytanie katalogu biegu: `<repo>/.loadout/runs/<ts>__<id>/` → wiersze indeksu.
//!
//! To jest druga połowa niezmiennika 4 i jedyne miejsce, w którym zdanie „`loadout.db` wolno
//! skasować i nic się nie stanie" jest **sprawdzalne** zamiast deklarowane. Wszystko, czego ten
//! plik nie umie odtworzyć z dysku, po skasowaniu bazy **przestaje istnieć** — a łamie się to
//! cicho: `steps.cost_usd` albo `steps.summary` zapisane wyłącznie do bazy w trakcie biegu
//! wyglądają poprawnie przez trzy tygodnie, bo nikt bazy nie kasuje.
//!
//! # Każda kolumna musi być FUNKCJĄ PLIKÓW
//!
//! AC-4 porównuje zrzut sprzed skasowania bazy ze zrzutem po odbudowie, kolumna po kolumnie,
//! wyliczając listę kolumn z `PRAGMA table_info`. To wymaganie jest ostrzejsze, niż wygląda:
//! **żadna** wartość nie ma prawa pochodzić z zegara ani z generatora. Konkretnie i z powodami:
//!
//! - `artifacts.id` — klucz wyliczony z biegu i ścieżki względnej, nigdy świeży uuid.
//! - `artifacts.created_at` — z `run.json`, nigdy `mtime` pliku: czas modyfikacji zmienia się
//!   przy kopiowaniu katalogu, więc indeks przestałby zgadzać się sam ze sobą po `cp -r`.
//! - `events.ts` — surowy strumień agenta **nie niesie znaczników czasu** (`logs/agent-<id>.jsonl`
//!   to linie wprost od vendora), więc jedyny czas, jaki da się odtworzyć z plików, to moment
//!   startu kroku. Dosypywanie do niego numeru linii wyglądałoby dokładniej i byłoby zmyśleniem;
//!   kolejność transkryptu niesie `events.seq`, i to jest jego zadanie.
//! - `events` w kolejności `seq` — kroki idą w kolejności z `run.json`, linie w kolejności
//!   z pliku, a katalog `handoffs/` jest **sortowany po nazwie**, bo `read_dir` nie obiecuje
//!   żadnej kolejności i na innym systemie plików oddałby inną.
//!
//! # Czego ten plik nie robi
//!
//! Nie kuruje. Każda niepusta linia surowego strumienia wchodzi jako jedno zdarzenie na poziomie
//! `raw`; mapowanie zdarzenie→linia (`system/init` nie daje nic, sąsiednie odczyty sklejają się
//! w oknie 2 s) jest kontraktem T-05 i mieszka w `engine::stream`. Odbudowa, która kurowałaby po
//! swojemu, byłaby drugą implementacją tej samej polityki — i tą, o której nikt by nie pamiętał.
//!
//! Nie zapisuje też **niczego**: oddaje wiersze, a do bazy niesie je `store::writer`, bo pisze
//! wyłącznie on (niezmiennik 2). Dlatego w tym pliku nie ma ani jednego zdania SQL — i dlatego
//! `checks/quick-boundary.sh` przechodzi po nim gerpem bez wyjątku dla nazwy.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{NewArtifact, NewEvent, NewRun, NewStep, Result};

/// Opis biegu — bieg i jego kroki.
const RUN_FILE: &str = "run.json";

/// Surowe strumienie agentów, po jednym pliku na krok.
const LOGS_DIR: &str = "logs";

/// Pliki przekazań między krokami. Front-matter pisze Loadout, nie agent [T6 §10.2].
const HANDOFFS_DIR: &str = "handoffs";

/// Poziom, na którym ląduje surowa linia strumienia. Jeden z trzech dozwolonych przez `CHECK`.
const LEVEL_RAW: &str = "raw";

/// `artifacts.kind` surowego strumienia agenta.
const KIND_RAW_LOG: &str = "raw_log";

/// `artifacts.kind` pliku przekazania.
///
/// Poza czwórką wymienioną w komentarzu T7 §5.4, bo tamta lista jest wyliczeniem przykładów,
/// a nie `CHECK`iem — a „file" nie powiedziałoby szynie, co to za plik.
const KIND_HANDOFF: &str = "handoff";

/// Ile kroków naraz, kiedy `run.json` tego nie mówi. Ta sama liczba stoi jako `DEFAULT`
/// w `schema::STATEMENTS`; rozjazd dałby bieg, który po odbudowie opowiada o sobie co innego.
const DEFAULT_CONCURRENCY: i64 = 3;

/// Ile zdarzeń wchodzi w jednej transakcji.
///
/// Zmierzone [T7 §5.3]: 100 wierszy na transakcję to 662 238 wierszy/s, wobec 67 144 przy jednym
/// wierszu. To jest **jedyny** powód tej stałej — nie ma tu kryterium na przepustowość i nie ma
/// go celowo, bo mierzyłoby maszynę.
pub(crate) const EVENTS_PER_TRANSACTION: usize = 100;

/// Wiersze wyczytane z katalogu biegu, gotowe do wysłania pisarzowi.
#[derive(Debug)]
pub(crate) struct Indexed {
    /// Bieg.
    pub(crate) run: NewRun,
    /// Jego kroki, w kolejności z `run.json`.
    pub(crate) steps: Vec<NewStep>,
    /// Jego zdarzenia, w kolejności, w jakiej mają dostać `seq`.
    pub(crate) steps_events: Vec<NewEvent>,
    /// Jego artefakty: surowe strumienie i pliki przekazań.
    pub(crate) artifacts: Vec<NewArtifact>,
}

/// `run.json`, tak jak leży na dysku.
///
/// Pola nieistotne są `Option` albo mają `default`, a nieznanych nie odrzucamy (niezmiennik 5):
/// plik zapisany przez nowszą wersję Loadouta ma się dać przeczytać, a nie wywrócić odbudowę.
#[derive(Debug, Deserialize)]
struct RunFile {
    id: String,
    workflow_id: String,
    /// Kopia grafu **jak biegł**. Trzymana jako `Value`, bo tu nas nie obchodzi jej kształt —
    /// obchodzi nas, żeby wróciła do bazy dokładnie taka, jaka przyszła [T7 §5.4].
    workflow_snapshot: serde_json::Value,
    title: String,
    status: String,
    #[serde(default = "default_concurrency")]
    concurrency: i64,
    created_at: i64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    error: Option<String>,
    #[serde(default)]
    steps: Vec<StepFile>,
}

/// Krok w `run.json`.
///
/// `cost_usd`, `summary` i `agent_session_id` są tu **z rozmysłem wypisane**: to są dokładnie te
/// trzy kolumny, które łamią niezmiennik 4 po cichu, jeśli ktoś zapisze je tylko do bazy.
#[derive(Debug, Deserialize)]
struct StepFile {
    id: String,
    node_key: String,
    name: String,
    agent: String,
    #[serde(default)]
    depends_on: Vec<String>,
    status: String,
    #[serde(default)]
    attempt: i64,
    agent_session_id: Option<String>,
    pid: Option<i64>,
    pgid: Option<i64>,
    exit_code: Option<i64>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    cost_usd: Option<f64>,
    summary: Option<String>,
    error: Option<String>,
}

/// Jedna linia surowego strumienia. Interesuje nas z niej **wyłącznie** rodzaj.
#[derive(Debug, Deserialize)]
struct LogLine {
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// Wartość `concurrency`, kiedy `run.json` jej nie niesie.
fn default_concurrency() -> i64 {
    DEFAULT_CONCURRENCY
}

/// Czyta katalog biegu i oddaje wiersze. **Synchronicznie i bez zapisu** — wołający puszcza to
/// przez `spawn_blocking`, bo surowy strumień długiego biegu bywa duży, a zamrożone okno jest
/// gorsze niż wolne.
pub(crate) fn read(run_dir: &Path) -> Result<Indexed> {
    let text = fs::read_to_string(run_dir.join(RUN_FILE))?;
    let file: RunFile = serde_json::from_str(&text)?;

    let run = NewRun {
        id: file.id,
        workflow_id: file.workflow_id,
        // `to_string`, nie tekst wycięty z pliku: `serde_json::Value` trzyma obiekt jako mapę
        // uporządkowaną, więc ten sam wejściowy JSON daje ten sam napis przy każdej odbudowie.
        workflow_snapshot: serde_json::to_string(&file.workflow_snapshot)?,
        title: file.title,
        status: file.status,
        concurrency: file.concurrency,
        created_at: file.created_at,
        started_at: file.started_at,
        ended_at: file.ended_at,
        error: file.error,
    };

    let mut steps = Vec::with_capacity(file.steps.len());
    let mut steps_events = Vec::new();
    let mut artifacts = Vec::new();

    for step in file.steps {
        // Surowa linia nie niesie własnego czasu, więc bierzemy jedyny, jaki stoi w plikach.
        let ts = step.started_at.unwrap_or(run.created_at);
        let log = run_dir
            .join(LOGS_DIR)
            .join(format!("agent-{}.jsonl", step.id));

        if let Some(stream) = read_if_present(&log)? {
            for line in stream.lines().filter(|line| !line.trim().is_empty()) {
                steps_events.push(NewEvent {
                    run_id: run.id.clone(),
                    step_id: Some(step.id.clone()),
                    ts,
                    kind: kind_of(line),
                    level: LEVEL_RAW.to_owned(),
                    body: Some(line.to_owned()),
                });
            }
            artifacts.push(artifact(
                run_dir,
                &run.id,
                Some(&step.id),
                KIND_RAW_LOG,
                &log,
                ts,
            )?);
        }

        steps.push(NewStep {
            id: step.id,
            run_id: run.id.clone(),
            // Tablica JSON, nie tekst sklejony przecinkami: kolumna trzyma to, co graf.
            depends_on: serde_json::to_string(&step.depends_on)?,
            node_key: step.node_key,
            name: step.name,
            agent: step.agent,
            status: step.status,
            attempt: step.attempt,
            agent_session_id: step.agent_session_id,
            pid: step.pid,
            pgid: step.pgid,
            exit_code: step.exit_code,
            started_at: step.started_at,
            ended_at: step.ended_at,
            cost_usd: step.cost_usd,
            summary: step.summary,
            error: step.error,
        });
    }

    // Przekazania nie mają `step_id`: który krok je napisał, mówi konwencja nazwy pliku
    // (`01__research__findings.md`), a ta konwencja jest kontraktem T-16. Zgadywanie jej tutaj
    // byłoby drugim miejscem, w którym mieszka ten sam format nazwy.
    for path in files_sorted_by_name(&run_dir.join(HANDOFFS_DIR))? {
        artifacts.push(artifact(
            run_dir,
            &run.id,
            None,
            KIND_HANDOFF,
            &path,
            run.created_at,
        )?);
    }

    Ok(Indexed {
        run,
        steps,
        steps_events,
        artifacts,
    })
}

/// Rodzaj zdarzenia: pole `type` z linii albo `raw`, kiedy linia nie jest naszym JSON-em.
///
/// Linii nieznanego kształtu **nie porzucamy** — to jest odwrotność niezmiennika 5 i tak ma być.
/// Tam chodzi o bieg, który nie ma się wywalić na nowym typie zdarzenia; tutaj plik jest prawdą,
/// a linia, której nie umiemy nazwać, dalej jest linią, która się wydarzyła.
fn kind_of(line: &str) -> String {
    serde_json::from_str::<LogLine>(line)
        .ok()
        .and_then(|parsed| parsed.kind)
        .unwrap_or_else(|| LEVEL_RAW.to_owned())
}

/// Treść pliku albo `None`, kiedy pliku nie ma.
///
/// Brak surowego strumienia nie jest awarią: krok mógł zostać pominięty albo anulowany, zanim
/// vendor cokolwiek powiedział. Awarią jest dopiero błąd inny niż „nie ma takiego pliku".
fn read_if_present(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Pliki katalogu, **posortowane po nazwie**, albo pusto, kiedy katalogu nie ma.
///
/// Sortowanie nie jest kosmetyką: `read_dir` nie obiecuje żadnej kolejności, więc bez niego
/// `artifacts` wychodziłby w kolejności systemu plików i odbudowa na innej maszynie dałaby inny
/// zrzut niż ten, który skasowano.
fn files_sorted_by_name(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Wiersz `artifacts` dla jednego pliku.
///
/// `id` jest wyliczone z biegu i ścieżki **względnej**, więc jest stabilne między odbudowami
/// i nie niesie w sobie katalogu, w którym akurat stoi repozytorium.
fn artifact(
    run_dir: &Path,
    run_id: &str,
    step_id: Option<&str>,
    kind: &str,
    path: &Path,
    created_at: i64,
) -> Result<NewArtifact> {
    let relative = path.strip_prefix(run_dir).unwrap_or(path);
    Ok(NewArtifact {
        id: format!("{run_id}::{}", relative.to_string_lossy()),
        run_id: run_id.to_owned(),
        step_id: step_id.map(ToOwned::to_owned),
        kind: kind.to_owned(),
        name: path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        path: path.to_string_lossy().into_owned(),
        // Plik większy niż `i64::MAX` nie ma reprezentacji w kolumnie `INTEGER`; oddajemy wtedy
        // brak rozmiaru zamiast liczby, która skłamie.
        bytes: i64::try_from(fs::metadata(path)?.len()).ok(),
        created_at,
    })
}
