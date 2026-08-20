//! AC-1 dla T-69: żaden start nie podmienia uchwytu żywego biegu.
//!
//! # Co dokładnie kosztuje pieniądze
//!
//! `AppState.live` jest JEDNYM uchwytem, a Stop czyta wyłącznie jego: `stop_run` woła
//! `commands::run::stop_run_inner`, którego pierwsza linia brzmi `deps.control.stop()`, a
//! `deps` to klon tego, co stoi w `live` w tej chwili. Start, który ten uchwyt PODMIENIA pod
//! żywym biegiem, nie zatrzymuje więc niczego — zabiera tylko jedyną drogę, którą tamten bieg
//! dawał się zatrzymać. Agent pracuje dalej i dalej płaci, a dowodu śmierci grupy nie ma komu
//! zażądać, bo uchwyt, który jako jedyny o tamtym biegu wiedział, został nadpisany
//! (niezmienniki 6 i 11). Z okna nie ma po tym żadnej drogi do tamtego biegu.
//!
//! # Dlaczego macierz par, a nie jeden przypadek
//!
//! Bo cicha porażka tej naprawy jest JEDNOSTRONNA. Warunek dopisany do jednej funkcji zamyka
//! `/ask` → Start i zostawia otwarte Start → Start oraz Start → `/ask`; wystarczy jedna
//! otwarta para, żeby agent płacił w tle. Test wyłącznie na parze `/ask` → Start przechodzi
//! dla takiej połowicznej naprawy i wygląda przy tym dokładnie jak test, który coś sprawdził.
//! Dlatego [`PAIRS`] wymienia wszystkie cztery uporządkowane pary dwóch dróg — w obu
//! kolejnościach i każdą z nich samą ze sobą.
//!
//! # Dlaczego asercją jest TOŻSAMOŚĆ uchwytu, a nie prawdziwy bieg z agentami
//!
//! Bo pytanie tego kryterium brzmi „czy Stop dosięga PIERWSZEGO biegu", a Stop dosięga tego,
//! czyj token anulowania trzyma. Tożsamość tokena jest więc całą treścią odpowiedzi i mierzy
//! się bez ani jednego procesu: anulujemy przez uchwyt wzięty tak, jak bierze go skorupa Stopu
//! (`AppState::deps`), i patrzymy, czy zapaliło się to na tokenie PIERWSZEGO biegu. Uchwyt
//! podmieniony daje token, którego nikt nie anulował — czyli dokładnie tego żywego agenta,
//! o którym mówi nagłówek.
//!
//! Świadomie NIE wołamy tu `stop_run_inner`: tamta funkcja czeka na `settle()` żywego biegu
//! (dowód śmierci grupy, niezmiennik 6), więc na uchwycie bez prawdziwych kroków czekałaby bez
//! końca — a bieg, który wisi, jest dla bramki „nie uruchomiło się", nie czerwienią. Czekanie
//! na dowód sądzi `run_stop_waits_for_proof`; tutaj sądzimy, KOMU ten dowód zostanie zażądany.
//!
//! # Słaba wersja tego kryterium i co ją rozstrzyga
//!
//! Najsłabsza: sprawdzić samą odmowę. Odmowa przechodzi także wtedy, gdy implementacja odmawia
//! i JEDNAK podmienia uchwyt — czyli kiedy człowiek dostaje zdanie i traci Stop w tej samej
//! chwili. Rozstrzyga to `stop_still_reaches_the_first_run`. Druga słaba wersja: blokada na
//! zawsze. Przechodzi wszystkie asercje wyżej i zamienia `/ask` w komendę do jednorazowego
//! użycia; rozstrzygają `the_next_start_goes_through_once_the_first_run_is_down` i
//! `every_road_starts_when_nothing_is_going`.
//!
//! Trzecia, dołożona 2026-08-20 w rundzie naprawczej: odmowa, która pyta „czy coś PRACUJE".
//! Przechodzi wszystkie pary powyżej, bo fikstura wchodzi do roboty (`begin()`) przed drugim
//! startem — a zostawia otwartą szczelinę między podmianą uchwytu i pierwszą linią biegu, w
//! której nic jeszcze nie pracuje i drugi start podmienia uchwyt tak samo cicho jak przed całą
//! naprawą. Rozstrzyga to `a_start_holds_the_handle_before_the_run_reaches_its_first_line`.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use loadout_lib::commands::{Drivers, RunDeps};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::absent::Absent;
use loadout_lib::ipc::AppState;
use loadout_lib::store::Store;
use tempfile::TempDir;

/// Droga do biegu — po jednej na komendę, która potrafi go zacząć.
///
/// Dwie, nie trzy: `run_workflow` obsługuje przycisk Start, komendę `/run` w wierszu wejścia
/// i zielony Run w edytorze (`src/sections/run/launch.ts` jest dla wszystkich trzech jedną
/// polityką), a `run_agent` obsługuje `/ask`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Road {
    /// Bieg z pliku: przycisk Start, `/run`, zielony Run w edytorze.
    FromAFile,
    /// Jeden agent z jednym zdaniem: `/ask` w wierszu wejścia.
    OneAgent,
}

impl Road {
    /// Nazwa komendy, którą ta droga obsługuje. Stoi w komunikatach asercji, bo „druga droga
    /// odmówiła" bez nazwy nie mówi, KTÓRA para jest otwarta.
    fn command(self) -> &'static str {
        match self {
            Self::FromAFile => "run_workflow",
            Self::OneAgent => "run_agent",
        }
    }

    /// Uchwyt nowego biegu tak, jak bierze go skorupa tej komendy w `src-tauri/src/ipc.rs`.
    ///
    /// To jest jedyne miejsce, w którym ten plik wie, że droga z pliku woła
    /// `AppState::begin_run`, a `/ask` woła `AppState::begin_a_run`. Kryterium sądzi ZACHOWANIE
    /// obu dróg, nie ich kształt: implementacja, w której jedna deleguje do drugiej, przechodzi
    /// tak samo, bo pytanie brzmi „co się stanie z uchwytem", a nie „ile jest funkcji".
    fn take<'a>(self, state: &'a AppState, project: &'a Path) -> Result<RunDeps<'a>, String> {
        match self {
            Self::FromAFile => state.begin_run(project),
            Self::OneAgent => state.begin_a_run(project),
        }
    }
}

/// WSZYSTKIE uporządkowane pary dróg: obie kolejności plus każda droga sama ze sobą.
///
/// Cztery, a nie dwie: para jest uporządkowana, bo naprawa dopisana do jednej funkcji zamyka
/// dokładnie jeden kierunek i zostawia drugi otwarty.
const PAIRS: [(Road, Road); 4] = [
    (Road::FromAFile, Road::FromAFile),
    (Road::FromAFile, Road::OneAgent),
    (Road::OneAgent, Road::FromAFile),
    (Road::OneAgent, Road::OneAgent),
];

/// Obie drogi osobno — do kontroli przeciw pustemu przejściu.
const ROADS: [Road; 2] = [Road::FromAFile, Road::OneAgent];

/// (a) Drugi start przy żywym pierwszym nie podmienia uchwytu i kończy się odmową.
#[tokio::test]
async fn no_second_start_takes_the_handle_from_a_live_run() -> Result<(), Box<dyn Error>> {
    for (first_road, second_road) in PAIRS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let project = bench.project.path();
        let first = a_run_is_going(&state, project, first_road)?;

        // `let … else`, nie `match`: ta sama asercja co do znaku, tylko w formie, którą
        // `-D warnings` przepuszcza (`clippy::manual_let_else` odrzuca `match` z ramieniem
        // wychodzącym).
        let Err(said) = second_road.take(&state, project) else {
            return Err(format!(
                "{} handed out a handle while a run started by {} was still going. That swap is \
                 silent and it costs money: from this moment Stop reaches the SECOND run, the \
                 first keeps writing and keeps paying, and nobody holds the only handle it could \
                 have been stopped by (invariants 6 and 11). One open pair is enough — this one \
                 is {} then {}",
                second_road.command(),
                first_road.command(),
                first_road.command(),
                second_road.command()
            )
            .into());
        };
        assert!(
            !said.trim().is_empty(),
            "{} turned the second start down with an empty answer, and silence in the place \
             where a person just asked for work is the same as no refusal at all (invariant 7)",
            second_road.command()
        );

        // TO JEST ASERCJA, KTÓREJ KLON W RĘKU NIE ZASTĄPI: implementacja, która odmawia i JEDNAK
        // podmienia uchwyt, przechodzi wszystko powyżej. Świeży uchwyt melduje „nie pracuję",
        // więc żywym uchwytem jest dalej ten pierwszy wtedy i tylko wtedy, gdy to zdanie jest
        // prawdziwe.
        assert!(
            state.deps().control.is_working(),
            "after the refused start by {} the live handle no longer belongs to the run that {} \
             started, so Stop has nothing to stop and the first agent is orphaned",
            second_road.command(),
            first_road.command()
        );
        drop(first);
    }
    Ok(())
}

/// (b) Odmowa nazywa następny ruch — nie jest ani ciszą, ani paniką.
#[tokio::test]
async fn every_refusal_names_the_next_move() -> Result<(), Box<dyn Error>> {
    for (first_road, second_road) in PAIRS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let project = bench.project.path();
        let first = a_run_is_going(&state, project, first_road)?;

        // Samo dojście do tej linii jest asercją „nie panika": panika w agentowym runtime
        // zabiera cały bieg (AGENTS.md §4), a odmowa jest zwykłą wartością (niezmiennik 7).
        let Err(said) = second_road.take(&state, project) else {
            return Err(format!(
                "{} started a second run beside a live one instead of saying why not, so there \
                 is no sentence here to judge. The pair was {} then {}",
                second_road.command(),
                first_road.command(),
                second_road.command()
            )
            .into());
        };

        let next_move = said.to_lowercase();
        assert!(
            next_move.contains("stop") || next_move.contains("wait"),
            "the refusal has to name the next move — press Stop, or wait for the run that is \
             going — because a refusal without a way out leaves a person exactly where they were \
             (DESIGN §8). Coming from {} after {}, it said: {said:?}",
            second_road.command(),
            first_road.command()
        );
        assert!(
            said.split_whitespace().count() >= 6,
            "the refusal is too short to be a sentence a person can act on, and a name is not an \
             answer (invariant 14). It said: {said:?}"
        );
        assert!(
            !said.contains('_') && !said.contains("::"),
            "the refusal carries a name from the wire instead of an English sentence, and a wire \
             enum never reaches a screen (invariant 14). It said: {said:?}"
        );
        drop(first);
    }
    Ok(())
}

/// (c) Po odmowie Stop dalej dosięga PIERWSZEGO biegu — to jest cała treść tej naprawy.
#[tokio::test]
async fn stop_still_reaches_the_first_run() -> Result<(), Box<dyn Error>> {
    for (first_road, second_road) in PAIRS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let project = bench.project.path();
        let first = a_run_is_going(&state, project, first_road)?;

        // Token PIERWSZEGO biegu, wzięty przed drugim startem. To on jedzie w każdy krok
        // i w planistę, więc anulowanie, które go nie dotknie, nie dotknie żywego agenta.
        let first_token = first.control.cancel_token();
        // Wynik drugiego startu jest tu bez znaczenia i to jest świadome: o odmowę pyta
        // `no_second_start_takes_the_handle_from_a_live_run`, a to pytanie brzmi „kogo dosięga
        // Stop PO tym, jak ktoś nacisnął start drugi raz". Implementacja, która odmawia i mimo
        // to podmienia uchwyt, jest dla człowieka nie do odróżnienia od tej, która nie odmawia.
        let _ = second_road.take(&state, project);

        // Dokładnie to, co robi Stop: `stop_run` → `stop_run_inner` → `deps.control.stop()`,
        // a `deps` jest klonem żywego uchwytu wziętym w tej chwili (`AppState::deps`).
        state.deps().control.stop();

        assert!(
            first_token.is_cancelled(),
            "Stop pressed after a second start by {} did not reach the run that {} started: its \
             cancel token is still alive, so that agent keeps working and keeps paying while the \
             screen has nothing left to press. This is invariant 6 with nobody to ask for the \
             proof — the handle that knew about that run was overwritten",
            second_road.command(),
            first_road.command()
        );
        drop(first);
    }
    Ok(())
}

/// (d) Kiedy pierwszy bieg zszedł, drugi start przechodzi normalnie.
#[tokio::test]
async fn the_next_start_goes_through_once_the_first_run_is_down() -> Result<(), Box<dyn Error>> {
    for (first_road, second_road) in PAIRS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let project = bench.project.path();
        let first = a_run_is_going(&state, project, first_road)?;
        let _ = second_road.take(&state, project);

        // Bieg zszedł — tak samo, jak zapala to `run_workflow_inner`, kiedy naprawdę wróciło.
        first.control.settle();

        if let Err(said) = second_road.take(&state, project) {
            return Err(format!(
                "the run started by {first} is down and a start by {second} was still turned \
                 down: {said:?}. A latch that never opens is worse than the defect it guards \
                 against — it turns {second} into a command that works once per launch of the \
                 app, and nothing on the screen explains why",
                first = first_road.command(),
                second = second_road.command()
            )
            .into());
        }
    }
    Ok(())
}

/// (e) Kontrola przeciw pustemu przejściu: bez żywego biegu KAŻDA droga startuje.
#[tokio::test]
async fn every_road_starts_when_nothing_is_going() -> Result<(), Box<dyn Error>> {
    for road in ROADS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let deps = road.take(&state, bench.project.path()).map_err(|said| {
            format!(
                "{} refused to start with nothing going at all: {said:?}. Every assertion above \
                 would also hold for a road that refuses always, and that road is not a fix — it \
                 is an application in which no work can ever begin",
                road.command()
            )
        })?;

        assert!(
            !deps.control.is_working(),
            "{} handed out a handle that already reports it is leading a run. A handle that \
             reports working without having been begun is one that has already settled, and a \
             settled handle can never prove anything down again: Stop would come back instantly \
             while the agent keeps going",
            road.command()
        );
    }
    Ok(())
}

/// (f) Uchwyt wzięty jest czyjś JUŻ, a nie dopiero od pierwszej linii biegu.
///
/// # Co dokładnie rozstrzyga ten przypadek, czego nie rozstrzygają (a)–(e)
///
/// Wszystkie powyżej stawiają pytanie o bieg, który już wszedł do roboty: fikstura
/// [`a_run_is_going`] woła `begin()` sama, zaraz po wzięciu uchwytu. Odmowa oparta na „czy coś
/// PRACUJE" spełnia je więc wszystkie i zostawia otwartą szczelinę, w której nic jeszcze nie
/// pracuje, a uchwyt jest już podmieniony: skorupa komendy wraca z `begin_run` i dopiero POTEM
/// wchodzi do biegu, który jako pierwszą linię zapala „ruszyłem". Rust nie wykonuje tych dwóch
/// rzeczy jako jednej — `Cargo.toml` włącza `rt-multi-thread`, a Tauri wysyła każdą komendę
/// jako osobne zadanie tej puli, więc dwa Starty naprawdę stoją na dwóch wątkach i drugi
/// naprawdę trafia w tę szczelinę. Skutek jest ten sam co bez żadnej odmowy: pierwszy bieg
/// zostaje bez uchwytu, agent pracuje dalej i płaci dalej (niezmienniki 6 i 11).
///
/// Ten przypadek mierzy tę szczelinę BEZ dwóch wątków i bez zegara — po prostu nie woła
/// `begin()`, czyli zostawia stan dokładnie taki, jaki widzi drugi wątek. Wyścigu w czasie nie
/// odtwarza (o tym niżej, przy [`a_run_is_going`]), ale odróżnia obie implementacje na pewno,
/// zawsze i w tę samą stronę.
#[tokio::test]
async fn a_start_holds_the_handle_before_the_run_reaches_its_first_line()
-> Result<(), Box<dyn Error>> {
    for (first_road, second_road) in PAIRS {
        let bench = Bench::new()?;
        let state = bench.app_state()?;
        let project = bench.project.path();

        // Uchwyt wzięty i ANI JEDNEJ linii biegu — świadomie bez `begin()`: to jest stan,
        // w którym skorupa komendy wróciła z uchwytem i jeszcze nie weszła do biegu.
        let first = first_road.take(&state, project).map_err(|said| {
            format!(
                "the first start by {} was turned down with nothing going: {said:?}",
                first_road.command()
            )
        })?;
        let first_token = first.control.cancel_token();

        let Err(said) = second_road.take(&state, project) else {
            return Err(format!(
                "{second} handed out a handle while the run started by {first} was still on its \
                 way to its first line. Nothing reported working yet, and that is the whole \
                 point: the window is a few instructions wide, both commands run as separate \
                 tasks of the same thread pool, and a person pressing twice lands in it. What \
                 comes out of it is the orphan from the header — the first agent keeps working \
                 and keeps paying with nobody holding its handle. The pair was {first} then \
                 {second}",
                first = first_road.command(),
                second = second_road.command()
            )
            .into());
        };
        assert!(
            !said.trim().is_empty(),
            "{} turned the second start down with an empty answer, and silence in the place \
             where a person just asked for work is the same as no refusal at all (invariant 7)",
            second_road.command()
        );

        // Tak samo jak w `stop_still_reaches_the_first_run`: Stop dosięga tego, czyj token
        // trzyma żywy uchwyt. Token PIERWSZEGO startu jest tu całą treścią odpowiedzi.
        state.deps().control.stop();
        assert!(
            first_token.is_cancelled(),
            "Stop pressed after a second start by {second} did not reach the run that {first} \
             had just started: its cancel token is still alive. A refusal that leaves the handle \
             swapped is, for the person at the screen, the same thing as no refusal",
            first = first_road.command(),
            second = second_road.command()
        );
        drop(first);
    }
    Ok(())
}

/// Bieg, który idzie TERAZ — i uchwyt, który go prowadzi.
///
/// PRZESŁANKA, NIE ASERCJA KRYTERIUM: dwa zdania w środku pilnują samej fikstury. Uchwyt, który
/// nie melduje prowadzonego biegu, nie postawiłby pytania, o które chodzi — „drugi start przy
/// ŻYWYM pierwszym" — a wtedy każda asercja niżej byłaby zdaniem o stanie bezczynnym.
///
/// # CZEGO TA FIKSTURA NIE MIERZY — luka znana i zostawiona świadomie
///
/// `begin()` stoi tu SYNCHRONICZNIE, przed drugim startem, więc żaden przypadek w tym pliku nie
/// przeplata dwóch startów na dwóch wątkach naprawdę: mierzą stan, nie wyścig. Prawdziwe
/// przeplecenie („wątek B bierze zamek dokładnie w szczelinie wątku A") wymagałoby zatrzymania
/// jednego wątku w środku `begin_run`, czyli albo punktu wstrzyknięcia w kodzie produkcyjnym,
/// albo testu na zegarze — a test na zegarze bywa zielony na wolnej maszynie i czerwony na
/// zajętej, czyli przestaje być wyrocznią. Szczelinę samą sądzi bez zegara
/// [`a_start_holds_the_handle_before_the_run_reaches_its_first_line`]; czy dołożyć do tego
/// przypadek naprawdę współbieżny, jest decyzją człowieka, nie pisarza tej naprawy.
fn a_run_is_going<'a>(
    state: &'a AppState,
    project: &'a Path,
    road: Road,
) -> Result<RunDeps<'a>, Box<dyn Error>> {
    let first = road.take(state, project).map_err(|said| {
        format!(
            "the first start by {} was turned down with nothing going: {said:?}",
            road.command()
        )
    })?;
    assert!(
        !first.control.is_working(),
        "a new run has to get a FRESH handle, and {} handed out one that already reports working",
        road.command()
    );
    // To samo, co robi bieg, kiedy wchodzi do roboty (`run_workflow_with_slots`).
    first.control.begin();
    assert!(
        first.control.is_working(),
        "the handle handed to {} does not report the run it is leading, so nothing below can tell \
         whose handle is live",
        road.command()
    );
    Ok(first)
}

/// Biblioteka użytkownika i folder pracy — tyle, ile potrzebuje `AppState`.
#[derive(Debug)]
struct Bench {
    home: TempDir,
    project: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        fs::create_dir_all(home.path().join("workflows"))?;
        // `Store::open` zakłada plik bazy, ale nie katalog nad nim.
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self { home, project })
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }

    /// Stan aplikacji złożony tak, jak składa go `src-tauri/src/lib.rs`.
    ///
    /// ŚWIEŻY NA KAŻDĄ PARĘ: `live` jest polem, które te przypadki zmieniają, więc jeden stan
    /// na całą macierz znaczyłby, że para trzecia sądzi ślad po parze pierwszej.
    fn app_state(&self) -> Result<AppState, Box<dyn Error>> {
        let store = Store::open(&self.db())?;
        Ok(AppState::new(
            self.home.path().to_path_buf(),
            self.project.path().to_path_buf(),
            store,
            no_agents_needed(),
        ))
    }
}

/// Fabryka sterowników, o którą nikt tu nie zapyta.
///
/// Te przypadki nie odpalają ani jednego kroku — sądzą uchwyt, nie pracę — a `AppState::new`
/// wymaga fabryki, bo jest ona jedną z rzeczy, które umie zbudować wyłącznie powłoka okna.
/// `Absent` jest tu prawdziwym sterownikiem z drzewa, a nie atrapą na czterdzieści wierszy:
/// odmawia zdaniem nazywającym vendora, więc gdyby cokolwiek niżej naprawdę ruszyło krok,
/// zobaczylibyśmy o tym zdanie, a nie ciszę.
fn no_agents_needed() -> Drivers {
    let nobody: Arc<dyn AgentDriver> = Arc::new(Absent::new("nobody", "T-69"));
    Arc::new(move |_vendor| Arc::clone(&nobody))
}
