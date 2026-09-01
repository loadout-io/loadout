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
//! Nie ma tu też **automatycznego wznowienia** — recovery wyłącznie sprząta osierocone grupy
//! i oznacza przerwane biegi oraz kroki. Jawne wznowienie istniejącej sesji należy do adaptera.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::durable_file::{DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy};
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

/// Zdanie dla biegu, który stał na pytaniu, kiedy okno zniknęło.
///
/// WŁASNE, a nie to samo, co dla kroku uciętego w pracy, i różnica jest dla człowieka całą
/// treścią: tam agent pracował i został przerwany, tu **nic nie pracowało** — bieg czekał na
/// odpowiedź, której nie było już komu podać.
const RUN_LEFT_ON_A_QUESTION: &str = "Loadout closed while this run was waiting for your answer, so there was nobody left to \
     carry it on. Start it again to pick the work up.";

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
    /* PUSTA LISTA NIE KOŃCZY TEGO PRZEBIEGU, i kryterium złapało tu prawdziwy błąd. „Nie ma
     * czego dobijać" nie znaczy „nie ma czego sprzątać": folder, w którym stoi wyłącznie bieg
     * zaparkowany na pytaniu, ma zero kroków w `running` — czyli dokładnie ten przypadek, dla
     * którego `settle_the_parked` powstało, i dokładnie ten, który wychodził stąd nietknięty.
     *
     * Wyjście wcześniej byłoby też DRUGIM miejscem wołania tego samego sprzątania, a drugie
     * miejsce jest tym, którego kryterium nie sądzi (mutacja jednego z nich nie zapala niczego).
     * Jeden przebieg do końca, jedno wołanie. */
    let parked = settle_the_parked(project);
    if rows.is_empty() {
        return Reconciled {
            runs: parked,
            ..Reconciled::default()
        };
    }

    let machine = Machine {
        boot_id: supervisor::machine_booted_at().unwrap_or_default(),
        own_pgid: supervisor::own_process_group(),
    };
    let plan = recovery::decide(&rows, &machine);
    let report = recovery::apply(&plan, reap);
    // 2026-08-27: sam licznik `unproven` ukrywał finansowo istotną sierotę przed człowiekiem.
    // Łączymy wynik domykacza z oryginalnym wierszem, bo tylko plik niesie oba identyfikatory,
    // które pozwalają rozpoznać ocalały proces bez zgadywania po samym PGID.
    let survivor_warnings: BTreeMap<String, String> = rows
        .iter()
        .filter_map(|row| {
            let pgid = row.pgid?;
            report
                .unproven
                .contains(&pgid)
                .then(|| (row.step_id.clone(), survivor_warning(row.pid, pgid)))
        })
        .collect();

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
        if write_back(dir, &change.status, &steps, &survivor_warnings) {
            done.runs += 1;
            done.steps += steps.len();
        }
    }
    done.runs += parked;
    done
}

/// Biegi, które stały na PYTANIU, kiedy okno zniknęło.
///
/// # Po co to jest osobno
///
/// Bo przebieg wyżej ich nie widzi i nie ma jak: pyta o kroki stojące w `running`, żeby mieć co
/// dobić, a bieg zaparkowany na punkcie kontrolnym **nie ma ani jednego takiego kroku**. Nic nie
/// pracuje, nic nie pali pieniędzy — i właśnie dlatego stoi tak w nieskończoność. Zmierzone
/// u właściciela 2026-08-23: bieg `20260819-160548` czekał na odpowiedź **czwarty dzień**, przez
/// kilkanaście restartów aplikacji, i żadne sprzątanie go nie dotykało.
///
/// # Dlaczego to jest porzucenie, a nie cierpliwość
///
/// Pytanie punktu kontrolnego żyje WYŁĄCZNIE w żywym strumieniu okna (`feed/model.ts`: `waiting`
/// bierze się z linii, która przyjechała na drucie). Okno, które zniknęło, zabrało je ze sobą —
/// a `continue_run` nie bierze identyfikatora biegu, więc nie ma czym w ten bieg wycelować.
/// Zostaje więc bieg, na który nie da się odpowiedzieć ŻADNĄ drogą. Nazwanie tego „pauzą" jest
/// obietnicą, której nie ma jak dotrzymać.
///
/// # Dlaczego wolno to zrobić bez pytania o rozruch maszyny
///
/// Bo tu nie ma do kogo strzelać. Strażnik `boot_id` broni niewinnych procesów przed sygnałem
/// (`recovery::decide`), a ten przebieg nie wysyła ani jednego sygnału — przepisuje jedno słowo
/// w pliku. Warunkiem jest za to CHWILA: sprzątanie biegnie, zanim to okno cokolwiek uruchomi
/// (`ipc::AppState::settle_everything_left_behind`), więc każda pauza zastana w tym momencie
/// należy do kogoś, kogo już nie ma.
///
/// Kroków nie ruszamy. Żaden z nich nie pracował, a `pending` mówi o nich prawdę: nie zaczęły się
/// i już się nie zaczną. Zdanie o tym niesie bieg, bo to jego dotyczy.
fn settle_the_parked(project: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) else {
        return 0;
    };
    let mut settled = 0;
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(run) = read_run(&dir) else { continue };
        if run.get("status").and_then(Value::as_str) != Some("paused") {
            continue;
        }
        /* Bieg z krokiem w `running` należy do przebiegu wyżej — ten ma co dobić, więc musi
         * przejść przez strażnika rozruchu maszyny. Tutaj wchodzą tylko te, przy których nie ma
         * ani jednego żywego kroku. */
        let anything_working = run
            .get("steps")
            .and_then(Value::as_array)
            .is_some_and(|steps| {
                steps
                    .iter()
                    .any(|one| one.get("status").and_then(Value::as_str) == Some("running"))
            });
        if anything_working {
            continue;
        }
        if write_back_with_reason(&dir, recovery::RUN_INTERRUPTED, RUN_LEFT_ON_A_QUESTION) {
            settled += 1;
        }
    }
    settled
}

/// Jak [`write_back`], ale zapisuje też zdanie na samym biegu i nie tyka kroków.
fn write_back_with_reason(dir: &Path, run_status: &str, why: &str) -> bool {
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
    if map.get("error").is_none_or(Value::is_null) {
        map.insert("error".to_owned(), Value::String(why.to_owned()));
    }
    publish_run(dir, &run)
}

/// Wiersze do rozstrzygnięcia, przeczytane z plików biegów tego folderu.
///
/// LUSTRO ZAPYTANIA Z `recovery::rows_to_judge`, co do warunku: bierzemy każdy krok biegu, który
/// stoi w `running` albo `paused`, **albo** żywy krok (`ready`/`running`) z biegu o innym statusie.
/// Rozjazd tych dwóch warunków znaczyłby, że po skasowaniu bazy odzyskiwanie sądzi inny zbiór niż
/// przed nim.
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
        let has_cut_off_work = matches!(run_status.as_str(), "running" | "paused")
            || steps.iter().any(|one| {
                matches!(
                    one.get("status").and_then(Value::as_str),
                    Some("ready" | "running")
                )
            });
        if !has_cut_off_work {
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
fn write_back(
    dir: &Path,
    run_status: &str,
    steps: &[(&str, &str)],
    survivor_warnings: &BTreeMap<String, String>,
) -> bool {
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
            let id = step
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let Some((_, status)) = steps.iter().find(|(want, _)| *want == id.as_str()) else {
                continue;
            };
            step.insert("status".to_owned(), Value::String((*status).to_owned()));
            if step.get("ended_at").is_none_or(Value::is_null) {
                step.insert("ended_at".to_owned(), Value::from(at));
            }
            if let Some(warning) = survivor_warnings.get(&id) {
                // 2026-08-27: wcześniejszy błąd kroku nie może ukryć faktu, że jego proces
                // przeżył sprzątanie; historia renderuje tylko to jedno pole błędu.
                step.insert("error".to_owned(), Value::String(warning.clone()));
            } else if step.get("error").is_none_or(Value::is_null) {
                step.insert("error".to_owned(), Value::String(STEP_CUT_OFF.to_owned()));
            }
        }
    }
    publish_run(dir, &run)
}

/// Publikuje pojedynczy zaktualizowany receipt wspólnym durable replace z T-202.
///
/// 2026-08-28 (T-152): recovery zachowuje nieznane pola przez `Value`, ale pełne bajty muszą
/// wejść przez ten sam fsync/rename/no-follow rdzeń co pozostałe pliki będące prawdą. Reconcile
/// nigdy nie tworzy `run.json`; polityka trybu jest wyłącznie bezpiecznym defaultem, gdyby cel
/// zniknął pomiędzy odczytem a publikacją.
fn publish_run(dir: &Path, run: &Value) -> bool {
    let Ok(mut bytes) = serde_json::to_vec_pretty(run) else {
        return false;
    };
    bytes.push(b'\n');
    DurableFilePublisher::new(dir)
        .atomic_replace(
            &dir.join(RUN_FILE),
            &bytes,
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
        )
        .is_ok()
}

/// Zdanie trafiające do `PastStepWire.error`, czyli jedynego błędu pokazywanego w historii.
fn survivor_warning(leader_pid: Option<i32>, process_group_id: i32) -> String {
    match leader_pid {
        Some(leader_pid) => format!(
            "This process survived Loadout's attempt to stop it. Inspect it manually: PID \
             {leader_pid}; PGID {process_group_id}."
        ),
        // Starszy plik może nie mieć PID-u. Nie wymyślamy liczby, ale nadal pokazujemy PGID,
        // który domykacz naprawdę sprawdził i po którym człowiek może rozpoznać grupę.
        None => format!(
            "This process group survived Loadout's attempt to stop it. Inspect it manually: \
             PGID {process_group_id}."
        ),
    }
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
