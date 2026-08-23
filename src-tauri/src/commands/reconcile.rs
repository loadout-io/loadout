//! Uzgodnienie biegów z **plikami**, w chwili otwarcia folderu.
//!
//! # Po co to istnieje
//!
//! Bieg, który zginął razem z aplikacją, zostawał w swoim `run.json` na zawsze jako `running`.
//! Zmierzone u właściciela 2026-08-23: trzy takie biegi naraz, siedem grup procesów dawno
//! martwych, a historia pokazywała je jako pracę w toku.
//!
//! Odzyskiwanie po awarii ISTNIAŁO i nie miało jak ich zobaczyć, z dwóch niezależnych powodów:
//!
//! 1. **Patrzyło nie tam.** `lib::recover_from_last_time` czyta wiersze z bazy otwartej przy
//!    starcie okna, czyli z `~/.loadout/loadout.db`. Biegi folderu mają WŁASNY indeks
//!    (`<folder>/.loadout/loadout.db`), więc w tamtej bazie ich nie ma. Zmierzone: biblioteka
//!    miała 19 biegów i ani jednego `running`, a obu zombie właściciela nie było w niej wcale.
//! 2. **Naprawiało nie to.** Wynik szedł wyłącznie do bazy (`store.writer().recovered`), a
//!    historia i diagnostyka czytają `run.json`. Nawet gdyby zobaczyło, plik dalej by kłamał.
//!
//! Ten moduł domyka oba: czyta stan z PLIKÓW (niezmiennik 4) i do plików go zapisuje.
//!
//! # Czego tu nie ma
//!
//! **Ani jednej reguły.** Wszystkie — strażnik czasu startu maszyny, użyteczność zapisanego
//! `pgid`, kolejność „przeczytaj, rozstrzygnij, dopiero działaj" — mieszkają w [`crate::recovery`]
//! i zostają tam. Ten plik dostarcza im wierszy z innego źródła i zapisuje ich wynik w innym
//! miejscu; druga kopia decyzji o tym, kiedy wolno strzelić do grupy procesów, byłaby tą, która
//! kiedyś strzeli po restarcie maszyny w niewinny proces.
//!
//! Nie ma tu też **automatycznego wznowienia** — z tego samego powodu, co w [`crate::recovery`]:
//! Loadout wykrywa, sprząta, oznacza i pyta [T7 §6.3].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::engine::supervisor;
use crate::recovery::{self, Machine, RecoveryRow};

/// Nazwa pliku biegu. Ta sama, którą pisze `commands::run` i czyta `commands::history`.
const RUN_FILE: &str = "run.json";

/// Katalog biegów wewnątrz folderu człowieka.
const RUNS_DIR: &str = ".loadout/runs";

/// Zdanie wpisywane krokowi, który nie przeżył zamknięcia aplikacji.
///
/// Po ludzku i bez naszych słów z drutu (niezmiennik 14): `reason` z [`crate::recovery`] jest
/// słowem dla bazy, a to jest zdanie dla człowieka, który patrzy na wiersz w historii.
const STEP_CUT_OFF: &str =
    "Loadout closed while this step was still running, so the step was cut off with it.";

/// Co uzgodnienie zastało i co z tym zrobiło — do dziennika, nie na ekran.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Ile biegów przepisano z `running` na przerwane.
    pub runs: usize,
    /// Ile kroków przepisano.
    pub steps: usize,
    /// Ile grup procesów udowodniono jako martwe.
    pub reaped: usize,
    /// Ile grup **wciąż żyje** mimo zamknięcia aplikacji. Nie zero znaczy sierotę palącą limit.
    pub still_alive: usize,
}

/// Uzgadnia biegi tego folderu z tym, co naprawdę żyje na maszynie.
///
/// Wołane z [`crate::workspace`] w chwili otwarcia folderu — raz na folder, spod zamka na liście
/// kart, czyli w jedynej chwili, w której nikt inny tych plików nie trzyma.
///
/// Nie oddaje odmowy. Folder bez katalogu biegów, bieg z nieczytelnym `run.json`, plik bez prawa
/// zapisu — każde z nich jest jednym biegiem mniej w tym uzgodnieniu, a nie folderem, którego nie
/// da się otworzyć (niezmiennik 5). Człowiek, któremu nie otwiera się projekt, bo jeden stary
/// plik biegu jest uszkodzony, traci znacznie więcej niż jeden wiersz historii.
#[must_use]
pub fn reconcile_runs(project: &Path) -> Reconciled {
    with_reaper(project, &mut |pgid| match supervisor::reap_group(pgid) {
        supervisor::GroupProof::Dead { .. } => recovery::ReapOutcome::ProvenDead,
        supervisor::GroupProof::Alive => recovery::ReapOutcome::StillAlive,
    })
}

/// To samo, z **wstrzykniętym** domykaczem grup procesów.
///
/// Istnieje z dokładnie tego samego powodu, co domknięcie w [`recovery::apply`], i powód ten jest
/// tam zapisany: kryterium akceptacji ma móc podstawić własny i sprawdzić, że NIC nie zostało
/// zabite, bez zabijania czegokolwiek na prawdziwej maszynie. Test wołający wersję z prawdziwym
/// `killpg` strzelałby do grupy o numerze wpisanym w fikstrze — a numery procesów przewijają się
/// w godzinach.
#[must_use]
pub fn with_reaper(
    project: &Path,
    reap: &mut dyn FnMut(i32) -> recovery::ReapOutcome,
) -> Reconciled {
    let (rows, where_they_live) = rows_from_files(project);
    if rows.is_empty() {
        return Reconciled::default();
    }

    let machine = Machine {
        boot_id: supervisor::machine_booted_at().unwrap_or_default(),
        own_pgid: supervisor::own_process_group(),
    };
    let plan = recovery::decide(&rows, &machine);
    let report = recovery::apply(&plan, reap);

    let mut done = Reconciled {
        reaped: report.reaped.len(),
        still_alive: report.unproven.len(),
        ..Reconciled::default()
    };
    for change in &plan.run_status {
        let Some(dir) = where_they_live.get(&change.run_id) else {
            continue;
        };
        let steps: Vec<(&str, &str)> = plan
            .step_status
            .iter()
            .map(|one| (one.step_id.as_str(), one.status.as_str()))
            .collect();
        if write_back(dir, &change.status, &steps) {
            done.runs += 1;
            done.steps += steps.len();
        }
    }
    done
}

/// Wiersze do rozstrzygnięcia, przeczytane z plików biegów tego folderu.
///
/// LUSTRO ZAPYTANIA Z `recovery::rows_to_judge`, co do warunku: bierzemy każdy krok biegu, który
/// stoi w `running`, **albo** krok stojący w `running` w biegu o innym statusie. Rozjazd tych
/// dwóch warunków znaczyłby, że po skasowaniu bazy odzyskiwanie sądzi inny zbiór niż przed nim.
fn rows_from_files(project: &Path) -> (Vec<RecoveryRow>, BTreeMap<String, PathBuf>) {
    let mut rows = Vec::new();
    let mut where_they_live = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) else {
        return (rows, where_they_live);
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(run) = read_run(&dir) else { continue };
        let run_status = run
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let run_id = run
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let boot = run
            .get("boot_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(steps) = run.get("steps").and_then(Value::as_array) else {
            continue;
        };
        let any_running = run_status == "running"
            || steps
                .iter()
                .any(|one| one.get("status").and_then(Value::as_str) == Some("running"));
        if !any_running {
            continue;
        }
        where_they_live.insert(run_id.clone(), dir);
        for step in steps {
            rows.push(RecoveryRow {
                step_id: text(step, "id"),
                run_id: run_id.clone(),
                run_status: run_status.clone(),
                step_status: text(step, "status"),
                run_boot_id: boot.clone(),
                pid: number(step, "pid"),
                pgid: number(step, "pgid"),
                session_id: step
                    .get("agent_session_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                attempt: step.get("attempt").and_then(Value::as_i64).unwrap_or(0),
            });
        }
    }
    (rows, where_they_live)
}

/// Wpisuje rozstrzygnięcie z powrotem do `run.json` — **w miejsce**, bez gubienia pól.
///
/// Czytamy i piszemy `Value`, a nie typowaną strukturę, i to jest rozstrzygnięcie: plik biegu
/// niesie migawkę grafu, przelotki vendorów i klucze, których ta wersja może nie znać. Przepisanie
/// go przez typ tej wersji skasowałoby wszystko, czego typ nie ma — czyli dokładnie tę wadę,
/// przed którą `AgentStep::extra` broni pliki workflow.
fn write_back(dir: &Path, run_status: &str, steps: &[(&str, &str)]) -> bool {
    let Some(mut run) = read_run(dir) else {
        return false;
    };
    let at = crate::commands::run::now_ms();
    let Some(map) = run.as_object_mut() else {
        return false;
    };
    map.insert("status".to_owned(), Value::String(run_status.to_owned()));
    if map.get("ended_at").is_none_or(Value::is_null) {
        map.insert("ended_at".to_owned(), Value::from(at));
    }
    if let Some(rows) = map.get_mut("steps").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            let Some(step) = row.as_object_mut() else {
                continue;
            };
            let id = step.get("id").and_then(Value::as_str).unwrap_or_default();
            let Some((_, status)) = steps.iter().find(|(want, _)| *want == id) else {
                continue;
            };
            step.insert("status".to_owned(), Value::String((*status).to_owned()));
            if step.get("ended_at").is_none_or(Value::is_null) {
                step.insert("ended_at".to_owned(), Value::from(at));
            }
            if step.get("error").is_none_or(Value::is_null) {
                step.insert("error".to_owned(), Value::String(STEP_CUT_OFF.to_owned()));
            }
        }
    }
    let Ok(text) = serde_json::to_string_pretty(&run) else {
        return false;
    };
    std::fs::write(dir.join(RUN_FILE), text + "\n").is_ok()
}

/// `run.json` tego katalogu, albo `None`. Nieczytelny plik jest jednym biegiem mniej.
fn read_run(dir: &Path) -> Option<Value> {
    let bytes = std::fs::read(dir.join(RUN_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn text(step: &Value, key: &str) -> String {
    step.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn number(step: &Value, key: &str) -> Option<i32> {
    step.get(key)
        .and_then(Value::as_i64)
        .and_then(|one| i32::try_from(one).ok())
}
