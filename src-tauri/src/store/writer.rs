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

use rusqlite::Connection;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::migrate::migrate;
use super::{NewEvent, NewRun, NewStep, Pragmas, Result, StoreError, apply_pragmas, read_pragmas};

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
}

/// Uchwyt do jedynego zadania piszącego.
///
/// Klonowalny i **ma być** klonowany: osiem równoległych producentów trzyma osiem klonów
/// i wszystkie prowadzą do jednego połączenia. Kiedy ginie ostatni klon, kanał się zamyka
/// i zadanie kończy pracę — dlatego „zaczekaj na pisarza" znaczy „upuść wszystkie uchwyty,
/// potem czekaj", a nie „śpij chwilę".
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

/// Pętla zadania pisarza. Kończy się dopiero, kiedy zginie ostatni [`Writer`].
async fn serve(conn: Connection, mut inbox: mpsc::Receiver<Job>) {
    // SZKIELET (2026-08-16): zapis, transakcja na wsad i przeżycie złego wiersza są całą
    // treścią AC-2, AC-5 i AC-6, więc tutaj ich nie ma. Zlecenia są odbierane i kwitowane
    // `Ok`, ale nic nie ląduje w bazie. Pętla zostaje prawdziwa — bez niej `Writer::rows`
    // wisiałby w oczekiwaniu na odpowiedź i kryteria padłyby na zwisie, a zwis to nie jest
    // czerwień z właściwego powodu (rc 124 jest na liście podpisów, które bramka odrzuca).
    tracing::debug!("SZKIELET: the writer task answers Ok and writes nothing");

    while let Some(job) = inbox.recv().await {
        match job {
            Job::Rows(_, reply) => {
                // Odbiorca mógł już zniknąć — wołający ma prawo się rozmyślić. To nie jest
                // błąd zapisu i nie ma prawa zatrzymać pętli.
                let _ = reply.send(Ok(()));
            }
            Job::Pragmas(reply) => {
                let _ = reply.send(read_pragmas(&conn));
            }
        }
    }
}
