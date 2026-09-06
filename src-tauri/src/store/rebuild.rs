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
//! - `artifacts` w kolejności — kroki idą w kolejności z `run.json`, a katalog `handoffs/` jest
//!   **sortowany po nazwie**, bo `read_dir` nie obiecuje żadnej kolejności i na innym systemie
//!   plików oddałby inną.
//!
//! # Czego ten plik nie robi
//!
//! Nie kuruje i **nie oddaje ani jednego zdarzenia**. 2026-09 (Z-15): do tego dnia każda niepusta
//! linia surowego strumienia wchodziła tu do indeksu jako wiersz `raw` — 27 362 wiersze i 86 %
//! z 72 MB żywej biblioteki, zapisane obok pliku, z którego przyszły, i nieczytane przez ani jeden
//! `SELECT` w produkcie. Transkrypt na ekran składa `commands::history::read_run_inner` prosto
//! z `logs/agent-<krok>.jsonl`, więc te wiersze były drugą kopią prawdy, którą niezmiennik 4
//! trzyma w plikach. Zostaje po nich wiersz `artifacts` wskazujący palcem na plik.
//!
//! Kuracja i tak nie mieszka tutaj: mapowanie zdarzenie→linia (`system/init` nie daje nic,
//! sąsiednie odczyty sklejają się w oknie 2 s) jest kontraktem T-05 i stoi w `engine::stream`.
//! Odbudowa, która kurowałaby po swojemu, żeby nazwać poziom `headline`, byłaby drugą
//! implementacją tej samej polityki (niezmiennik 23) — i tą, o której nikt by nie pamiętał.
//!
//! Nie zapisuje też **niczego**: oddaje wiersze, a do bazy niesie je `store::writer`, bo pisze
//! wyłącznie on (niezmiennik 2). Dlatego w tym pliku nie ma ani jednego zdania SQL — i dlatego
//! `checks/quick-boundary.sh` przechodzi po nim gerpem bez wyjątku dla nazwy.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{NewArtifact, NewRun, NewStep, Result};

/// Opis biegu — bieg i jego kroki.
const RUN_FILE: &str = "run.json";

/// Surowe strumienie agentów, po jednym pliku na krok.
const LOGS_DIR: &str = "logs";

/// Pliki przekazań między krokami. Front-matter pisze Loadout, nie agent [T6 §10.2].
/// `pub(crate)`, bo pyta o tę nazwę także ponowne odpalenie kroku (`commands::run`), które
/// przenosi przekazania poprzedniego biegu. Druga stała z tym samym napisem rozjechałaby się
/// przy pierwszej zmianie układu katalogu biegu.
pub(crate) const HANDOFFS_DIR: &str = "handoffs";

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
    #[serde(default)]
    boot_id: Option<String>,
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

/// Wartość `concurrency`, kiedy `run.json` jej nie niesie.
fn default_concurrency() -> i64 {
    DEFAULT_CONCURRENCY
}

/// Czyta katalog biegu i oddaje wiersze. **Synchronicznie i bez zapisu** — wołający puszcza to
/// przez `spawn_blocking`, bo to jest wejście na dysk, a zamrożone okno jest gorsze niż wolne.
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
        // Z PLIKU, nie zmyślone: `run.json` zapisuje znacznik w chwili startu biegu, a odbudowa
        // dzieje się kiedyś potem. Plik sprzed wprowadzenia pola daje `None` i to jest poprawna
        // odpowiedź — brak strażnika wstrzymuje strzał (`recovery::reason::NO_BOOT_TIME`),
        // zamiast go przepuszczać.
        boot_id: file.boot_id,
    };

    let mut steps = Vec::with_capacity(file.steps.len());
    let mut artifacts = Vec::new();

    for step in file.steps {
        // Plik nie niesie własnego czasu, więc bierzemy jedyny, jaki stoi w `run.json`.
        let ts = step.started_at.unwrap_or(run.created_at);
        let log = run_dir
            .join(LOGS_DIR)
            .join(format!("agent-{}.jsonl", step.id));

        // 2026-09 (Z-15) — TYLKO `metadata`, ANI JEDNEGO BAJTU TREŚCI. Strumień długiego biegu waży
        // dziesiątki megabajtów; wczytanie go w całości po to, żeby przepisać każdą linię do bazy,
        // kosztowało dwie kopie w pamięci (`read_to_string` plus `line.to_owned()`) i drugą kopię
        // na dysku. Po pytanie o rozmiar `artifact` sięga tu drugi raz i to jest cała cena.
        if is_on_disk(&log)? {
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
    for directory in crate::memory::handoff::publication_directories(run_dir)? {
        for path in files_sorted_by_name(&directory.join(HANDOFFS_DIR))? {
            artifacts.push(artifact(
                run_dir,
                &run.id,
                None,
                KIND_HANDOFF,
                &path,
                run.created_at,
            )?);
        }
    }

    Ok(Indexed {
        run,
        steps,
        artifacts,
    })
}

/// Czy ten plik leży na dysku.
///
/// Brak surowego strumienia nie jest awarią: krok mógł zostać pominięty albo anulowany, zanim
/// vendor cokolwiek powiedział. Awarią jest dopiero błąd inny niż „nie ma takiego pliku".
fn is_on_disk(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
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
