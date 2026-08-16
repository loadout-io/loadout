//! Jedyny plik w repo, któremu wolno otworzyć **zapisujące** połączenie do `SQLite`.
//!
//! Niezmiennik 2, i nie jest to preferencja stylistyczna: `SQLite` dopuszcza jednego pisarza,
//! więc drugie połączenie zapisujące nie jest „czasem wolniejsze", tylko jest zakleszczeniem
//! [T7 ryzyko 7]. Zamiast walczyć z `SQLITE_BUSY`, wszystkie zapisy idą kanałem do **jednego**
//! zadania tokio, które zbiera to, co przyszło, i zapisuje wsadem [T7 §5.3]. Sprawdzenie
//! `checks/quick-boundary.sh` zna ten plik z nazwy i pozwala tu na to, na co nigdzie indziej.
//!
//! Zmierzone [T7 §5.3], na `SQLite` 3.53.2 i zaindeksowanej tabeli `events`:
//!
//! | Konfiguracja | Wierszy na sekundę |
//! |---|---|
//! | dziennik rollback, 1 wiersz na transakcję | 2 698 |
//! | WAL + `synchronous=NORMAL`, 1 wiersz na transakcję | 67 144 |
//! | WAL + `synchronous=NORMAL`, **100 wierszy na transakcję** | **662 238** |
//!
//! Czyli: zapomniany WAL kosztuje **25×**, a wsad jest o rząd wielkości ponad potrzebą i tyle
//! ma wystarczyć. Nie ma tu kryterium na przepustowość i nie ma go celowo — byłoby to kryterium,
//! które mierzy maszynę.
//!
//! Druga własność, która nie wychodzi sama: **jeden zły wiersz nie zabija pisarza.** Wsad wraca
//! w całości (transakcja), błąd wraca do wołającego, a zadanie bierze następne zlecenie.
//! Implementacja, która ratuje atomowość, kończąc zadanie, przechodzi połowę AC-6 i zmusza
//! użytkownika do restartu aplikacji po jednym złym zdarzeniu.
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Pętla zadania **odbiera zlecenia i odpowiada `Ok`, nie dotykając połączenia**. To jest
//! świadomie zła wartość: baza zostaje pusta, więc AC-2, AC-4, AC-5 i AC-6 padają na porównaniu
//! stanu, a nie na braku celu testowego. Żadnego z nich nie da się na tym szkielecie przejść —
//! AC-5 porównuje zbiór 4000 treści, a AC-6 wymaga błędu, którego szkielet nigdy nie oddaje.

use std::path::Path;

use rusqlite::{Connection, params};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::migrate::migrate;
use super::{
    NewArtifact, NewEvent, NewRun, NewStep, Pragmas, Result, StoreError, apply_pragmas,
    read_pragmas,
};

/// Ile zleceń mieści się w kanale, zanim nadawca zaczeka.
///
/// Kanał **ograniczony**, nigdy `unbounded_channel`: nieograniczony kanał zamienia wolniejszego
/// pisarza w rosnącą stertę i awaria przychodzi jako pamięć, a nie jako czekanie — czyli
/// najpóźniej jak się da i bez śladu, kto ją spowodował.
const CHANNEL_DEPTH: usize = 1024;

/// Wiersze, które idą do bazy. Jeden wariant, jedna transakcja.
///
/// `Box` przy `NewRun` i `NewStep` nie jest ozdobą: bez niego największy wariant ma ~350 bajtów,
/// najmniejszy ~8, i `clippy::large_enum_variant` słusznie zwraca uwagę, że każdy element kanału
/// płaci rozmiarem za wariant, którego nie niesie.
#[derive(Debug)]
enum Rows {
    /// Nowy bieg.
    Run(Box<NewRun>),
    /// Nowy krok.
    Step(Box<NewStep>),
    /// Nowy artefakt — **ścieżka** do pliku, nigdy jego treść [T7 §5.4].
    Artifact(Box<NewArtifact>),
    /// Wsad zdarzeń — całość albo nic.
    Events(Vec<NewEvent>),
}

/// Zlecenie dla zadania pisarza.
#[derive(Debug)]
enum Job {
    /// Zapisz te wiersze i powiedz, jak poszło.
    Rows(Rows, oneshot::Sender<Result<()>>),
    /// Przeczytaj pragmy z **własnego** połączenia i je oddaj.
    ///
    /// Istnieje wyłącznie po to, żeby dało się je zmierzyć: połączenie pisarza jest jedynym,
    /// do którego nikt z zewnątrz nie ma referencji, więc bez tego zlecenia AC-3 sprawdzałoby
    /// wszystkie konstruktory oprócz tego jednego, który pisze.
    Pragmas(oneshot::Sender<Result<Pragmas>>),
    /// Przestań przyjmować nowe zlecenia, dopisz to, co już stoi w kanale, i skończ.
    ///
    /// Idzie **kanałem**, a nie osobnym sygnałem, i to jest cała jego wartość: kanał jest
    /// FIFO, więc wszystko wysłane wcześniej zostaje obsłużone, zanim to zlecenie dojdzie.
    /// Zamknięcie sygnałem obok kanału ścigałoby się z wysłanymi wsadami i „zapisane" znowu
    /// znaczyłoby „wysłane".
    Close,
}

/// Uchwyt do jedynego zadania piszącego.
///
/// Klonowalny i **ma być** klonowany: osiem równoległych producentów trzyma osiem klonów
/// i wszystkie prowadzą do jednego połączenia. Kiedy ginie ostatni klon, kanał się zamyka
/// i zadanie kończy pracę — ale to jest droga zapasowa, nie sposób zamykania. Zamyka
/// [`Writer::shutdown`], bo „zginął ostatni klon" jest warunkiem, którego wołający nie
/// kontroluje (2026-08-16: patrz komentarz przy tej metodzie).
#[derive(Debug, Clone)]
pub struct Writer {
    /// Kanał do zadania.
    jobs: mpsc::Sender<Job>,
}

impl Writer {
    /// Dopisuje bieg.
    pub async fn insert_run(&self, run: NewRun) -> Result<()> {
        self.rows(Rows::Run(Box::new(run))).await
    }

    /// Dopisuje krok.
    pub async fn insert_step(&self, step: NewStep) -> Result<()> {
        self.rows(Rows::Step(Box::new(step))).await
    }

    /// Dopisuje artefakt: wiersz wskazujący palcem na plik, który już leży na dysku.
    pub async fn insert_artifact(&self, artifact: NewArtifact) -> Result<()> {
        self.rows(Rows::Artifact(Box::new(artifact))).await
    }

    /// Dopisuje wsad zdarzeń. **Jedno wywołanie, jedna transakcja** — sto wierszy albo zero,
    /// nigdy pięćdziesiąt sześć sierot i dziura w transkrypcie, o której nikt nie wie.
    pub async fn append_events(&self, batch: Vec<NewEvent>) -> Result<()> {
        self.rows(Rows::Events(batch)).await
    }

    /// Wspólna droga wszystkich zapisów: wyślij, zaczekaj na odpowiedź, oddaj ją wołającemu.
    async fn rows(&self, rows: Rows) -> Result<()> {
        let (reply, answer) = oneshot::channel();
        self.jobs
            .send(Job::Rows(rows, reply))
            .await
            .map_err(|_| StoreError::WriterGone)?;
        answer.await.map_err(|_| StoreError::WriterGone)?
    }

    /// Pragmy połączenia pisarza.
    pub(crate) async fn pragmas(&self) -> Result<Pragmas> {
        let (reply, answer) = oneshot::channel();
        self.jobs
            .send(Job::Pragmas(reply))
            .await
            .map_err(|_| StoreError::WriterGone)?;
        answer.await.map_err(|_| StoreError::WriterGone)?
    }

    /// Mówi zadaniu, żeby dopisało to, co już stoi w kanale, i skończyło.
    ///
    /// Zlecenie idzie **kanałem**, a nie osobnym sygnałem, bo kanał jest FIFO: wszystko wysłane
    /// wcześniej zostaje obsłużone, zanim to dojdzie. Sygnał obok kanału ścigałby się
    /// z wysłanymi wsadami i „zapisane" znowu znaczyłoby „wysłane".
    ///
    /// 2026-08-16 — DLACZEGO TO ISTNIEJE, skoro zginięcie ostatniego klonu też kończy pętlę:
    /// bo wołający tego warunku nie kontroluje. Kto trzyma własny klon [`Writer`] (a trzyma go
    /// każdy, kto cokolwiek zapisuje) i woła `Store::close`, ten czekałby na zadanie, które nie
    /// ma prawa się skończyć — kanał wciąż ma nadawcę. To nie jest błąd, tylko zawis, czyli
    /// najgorszy kształt tej awarii: bez komunikatu i bez kodu wyjścia.
    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.jobs
            .send(Job::Close)
            .await
            .map_err(|_| StoreError::WriterGone)
    }
}

/// Otwiera bazę do zapisu, ustawia pragmy, migruje i startuje zadanie pisarza.
///
/// Kolejność jest nośna i pochodzi z meetnotes [00-SYNTHESIS §3, `SQLite`]: open → pragmy →
/// `busy_timeout` → `migrate()`. Odwrócenie dwóch ostatnich daje migrację, która biegnie
/// z wyłączonymi kluczami obcymi, czyli w innym świecie niż aplikacja.
pub(crate) fn start(handle: &Handle, path: &Path) -> Result<(Writer, JoinHandle<()>)> {
    let conn = Connection::open(path).map_err(|source| StoreError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    apply_pragmas(&conn)?;
    migrate(&conn)?;

    let (jobs, inbox) = mpsc::channel(CHANNEL_DEPTH);
    let task = handle.spawn(serve(conn, inbox));
    Ok((Writer { jobs }, task))
}

/// Bieg wchodzi jednym zdaniem; kolumny wypisane jawnie, nigdy `INSERT INTO runs VALUES (…)`
/// po pozycjach — kolumna dołożona jutro przestawiłaby wtedy wszystkie następne po cichu.
const INSERT_RUN: &str = "INSERT INTO runs \
     (id, workflow_id, workflow_snapshot, title, status, concurrency, \
      created_at, started_at, ended_at, error) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// Krok. `status` i `attempt` przechodzą przez `CHECK` i `STRICT` w schemacie, nie przez Rusta.
const INSERT_STEP: &str = "INSERT INTO steps \
     (id, run_id, node_key, name, agent, depends_on, status, attempt, agent_session_id, \
      pid, pgid, exit_code, started_at, ended_at, cost_usd, summary, error) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)";

/// Artefakt: wskazanie palcem na plik.
const INSERT_ARTIFACT: &str = "INSERT INTO artifacts \
     (id, run_id, step_id, kind, name, path, bytes, created_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

/// Zdarzenie. `seq` nie występuje: nadaje je `SQLite`.
const INSERT_EVENT: &str = "INSERT INTO events (run_id, step_id, ts, kind, level, body) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

/// Pętla zadania pisarza. Kończy się na [`Job::Close`] albo kiedy zginie ostatni [`Writer`].
async fn serve(mut conn: Connection, mut inbox: mpsc::Receiver<Job>) {
    while let Some(job) = inbox.recv().await {
        match job {
            Job::Rows(rows, reply) => {
                // Wynik idzie do wołającego i **nie** zatrzymuje pętli. To jest cała druga
                // połowa AC-6: implementacja, która ratuje atomowość, kończąc zadanie, zostawia
                // użytkownika z biegiem, w którym nic więcej się nie zapisze, i z aplikacją do
                // restartu po jednym złym zdarzeniu.
                let outcome = write(&mut conn, &rows);
                // Odbiorca mógł już zniknąć — wołający ma prawo się rozmyślić. To nie jest
                // błąd zapisu i nie ma prawa zatrzymać pętli.
                let _ = reply.send(outcome);
            }
            Job::Pragmas(reply) => {
                let _ = reply.send(read_pragmas(&conn));
            }
            // Kanał jest FIFO, więc w tym miejscu wszystko, co ktokolwiek wysłał przed
            // zamknięciem, jest już zapisane. Dlatego wolno wyjść bez dopytywania.
            Job::Close => break,
        }
    }
}

/// Jedno zlecenie, jedna droga do bazy.
fn write(conn: &mut Connection, rows: &Rows) -> Result<()> {
    match rows {
        Rows::Run(run) => insert_run(conn, run),
        Rows::Step(step) => insert_step(conn, step),
        Rows::Artifact(artifact) => insert_artifact(conn, artifact),
        Rows::Events(batch) => append_events(conn, batch),
    }
}

/// Dopisuje bieg.
fn insert_run(conn: &Connection, run: &NewRun) -> Result<()> {
    conn.execute(
        INSERT_RUN,
        params![
            run.id,
            run.workflow_id,
            run.workflow_snapshot,
            run.title,
            run.status,
            run.concurrency,
            run.created_at,
            run.started_at,
            run.ended_at,
            run.error,
        ],
    )?;
    Ok(())
}

/// Dopisuje krok.
fn insert_step(conn: &Connection, step: &NewStep) -> Result<()> {
    conn.execute(
        INSERT_STEP,
        params![
            step.id,
            step.run_id,
            step.node_key,
            step.name,
            step.agent,
            step.depends_on,
            step.status,
            step.attempt,
            step.agent_session_id,
            step.pid,
            step.pgid,
            step.exit_code,
            step.started_at,
            step.ended_at,
            step.cost_usd,
            step.summary,
            step.error,
        ],
    )?;
    Ok(())
}

/// Dopisuje artefakt.
fn insert_artifact(conn: &Connection, artifact: &NewArtifact) -> Result<()> {
    conn.execute(
        INSERT_ARTIFACT,
        params![
            artifact.id,
            artifact.run_id,
            artifact.step_id,
            artifact.kind,
            artifact.name,
            artifact.path,
            artifact.bytes,
            artifact.created_at,
        ],
    )?;
    Ok(())
}

/// Dopisuje wsad zdarzeń **w jednej transakcji**: wszystkie wiersze albo żaden.
///
/// Odmowa niesie liczbę wierszy całego wsadu, a nie tego jednego, który ją wywołał, i to jest
/// różnica, o którą chodzi: wołający ma się dowiedzieć, ile zdarzeń **nie weszło**, bo „jeden
/// wiersz odrzucony" i „sto wierszy wróciło" proszą o zupełnie inną naprawę.
///
/// Wyjście przez `?` porzuca [`rusqlite::Transaction`] bez `commit()`, a jej `Drop` wycofuje
/// całość. Nie polegamy na tym w milczeniu — to jest jedyny powód, dla którego pięćdziesiąt
/// sześć sierot nie zostaje w transkrypcie, a dziura w transkrypcie jest gorsza niż brak wsadu,
/// bo nikt nie umie jej zobaczyć.
fn append_events(conn: &mut Connection, batch: &[NewEvent]) -> Result<()> {
    let rows = batch.len();
    let transaction = conn.transaction()?;
    {
        // `prepare_cached`, bo ta sama treść wraca przy każdym wsadzie przez cały bieg,
        // a osiem producentów po 500 wysłań to 4000 przejść przez to miejsce (AC-5).
        let mut statement = transaction.prepare_cached(INSERT_EVENT)?;
        for event in batch {
            statement
                .execute(params![
                    event.run_id,
                    event.step_id,
                    event.ts,
                    event.kind,
                    event.level,
                    event.body,
                ])
                .map_err(|source| StoreError::Batch { rows, source })?;
        }
    }
    transaction.commit()?;
    Ok(())
}
