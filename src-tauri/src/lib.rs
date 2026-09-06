//! Powłoka aplikacji po stronie Rusta: dziennik, hak paniki, okno.
//!
//! Konfiguracja subskrybenta zostaje tutaj, a synchroniczny właściciel plików dziennika mieszka
//! w `logging.rs`. Silnik nadal nie zależy od pliku, który zna Tauri (niezmiennik 1).
//!
//! Kod platformowy też tu nie mieszka (niezmiennik 3). Przepis na czyste chrome jest
//! macOS-owy, ale jest zapisany jako DANE w `tauri.conf.json`, nie jako `cfg` w tym pliku.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tauri::Manager;

use crate::commands::Drivers;
use crate::engine::drivers::claude::ClaudeDriver;
use crate::engine::drivers::codex::CodexDriver;
use crate::engine::drivers::{
    AgentDriver, AgentHandle, DecodedEvent, DriverConfiguration, Probe, RunSpec, StepSettings,
    ValidatedImages,
};
use crate::engine::supervisor::AgentCliSearch;
use crate::library::agents::Vendor;
use crate::logging::{
    BoundedLogLayer, BoundedLogWriter, LOG_FILE, LOG_FILE_LIMIT, write_log_open_error,
};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::layer::SubscriberExt as _;

/// Czasowniki Loadouta dla agenta: most, ktorym agent siega po to, co nalezy do aplikacji.
///
/// Istnieje, bo vendor w trybie `-p` nie daje ANI JEDNEJ takiej drogi — zmierzone 2026-08-29,
/// powod w calosci stoi w naglowku modulu.
pub mod bridge;

/// Warstwa komend: funkcje `*_inner`, ktore nie znaja slowa „Tauri". Wypelnia T-15.
pub mod commands;

/// Prywatne dowody biegow i rozmow oraz bezpieczne manifesty ich wejscia. Wypelnia T-34.
pub mod evidence;

/// Silnik: graf, planista, nadzor procesow. Wypelnia T-02 i dalej.
pub mod engine;

/// Dziedziczenie wiedzy repo gospodarza: tekst, nigdy maszyneria. Wypelnia T-54.
pub mod inherit;

/// Import setupow repo do natywnych agentow, skilli, polaczen i workflow. Wypelnia T-75.
pub mod import;

/// Polaczenia narzedziowe zarzadzane przez Loadout. Wypelnia T-75.
pub mod connections;

/// Trwała publikacja plików będących prawdą: jeden rdzeń replace i create-if-absent.
pub mod durable_file;

/// Granica z oknem: pompa sklejajaca i kanal do webviewa. Wypelnia T-07.
pub mod ipc;

/// Lab: zestawy przypadkow, warianty i wynik przebiegu liczony z planu oraz `run.json`.
pub mod lab;
/// Ograniczony lokalny dziennik: jeden wlasciciel zapisow, najwyzej dwie generacje.
pub mod logging;

/// Biblioteka uzytkownika: agenci, umiejetnosci, pamiec. Wypelnia T-11 i dalej.
pub mod library;
/// Magazyn: schemat `SQLite`, jeden pisarz, migracje. Wypelnia T-06.
pub mod store;

/// Pamiec: pliki przekazan miedzy krokami (T-16) i notatki (T-17).
pub mod memory;

/// Odzyskiwanie po awarii: wykryj, sprzatnij po `pgid`, zapytaj. Wypelnia T-20.
pub mod recovery;

/// Umiejetnosci: jeden folder, dwa katalogi, szesciu vendorow. Wypelnia T-18 i T-19.
pub mod skills;

/// Format pliku workflow i walidacja przy zapisie. Wypelnia T-12.
pub mod workflow;

/// Rejestr workspace'ow: jeden folder — jedna karta, jedna wspolna pula miejsc. Wypelnia T-24.
pub mod workspace;

// 2026-08-15 — WARUNEK USUNIECIA DEKLARACJI TYMCZASOWEJ ZASZEDL, wiec jej tu nie ma.
//
// Stala tu para linii `#[path = "engine/supervisor.rs"] pub mod supervisor;`, bo `engine/mod.rs`
// nie mialo wtedy `pub mod supervisor;` — a jeden wiersz poza blokiem OWNS to pytanie do
// czlowieka (AGENTS.md §7), nie cichy dopisek. Czlowiek odpowiedzial commitem 687712a: linia
// stoi w `engine/mod.rs`, wiec jedyny poprawny adres modulu to `engine::supervisor`.
//
// Zostawienie obu naraz zbudowaloby ten sam plik dwa razy, jako dwa rozne moduly. To nie jest
// blad kompilacji — to dwa niezalezne typy `GroupProof`, ktorych kompilator nie zamieni jeden
// w drugi, wiec `stop()` z jednego modulu nie da sie porownac z dowodem z drugiego.

/// Etykieta jedynego okna. Ta sama wartość stoi w `app.windows[0].label` w `tauri.conf.json`
/// i w polu `windows` każdego pliku w `src-tauri/capabilities/`. Uprawnienia celujące w okno,
/// którego nie ma, nie dotyczą niczego i odmawiają KAŻDEGO wywołania z webviewa — a webview
/// dowiedziałby się o tym dopiero w T-07 i przeczytał to jako zepsute wywołanie.
const MAIN_WINDOW: &str = "main";

/// Dekorator, który zachowuje zamrożony świat wyszukiwania przez klonowanie sterownika per krok.
///
/// Run dokłada effort, budżet, ustawienia i dowody przez metody zwracające nowe sterowniki.
/// Samo ustawienie `PATH` w fabryce ginęłoby przy pierwszym takim klonie i bare-name fallback
/// ponownie czytałby środowisko procesu aplikacji. Każda metoda niżej wyłącznie deleguje i
/// ponownie zakłada ten sam dekorator; nie interpretuje żadnej polityki vendora.
struct SearchEnvironmentDriver {
    inner: Arc<dyn AgentDriver>,
    path: std::ffi::OsString,
}

impl SearchEnvironmentDriver {
    fn wrapped(&self, inner: Arc<dyn AgentDriver>) -> Arc<dyn AgentDriver> {
        Arc::new(Self {
            inner,
            path: self.path.clone(),
        })
    }

    fn with_search_path(&self, configuration: &DriverConfiguration) -> DriverConfiguration {
        let mut configuration = configuration.clone();
        if !configuration
            .environment
            .iter()
            .any(|(name, _)| name == "PATH")
        {
            configuration
                .environment
                .push(("PATH".to_owned(), self.path.clone()));
        }
        configuration
    }
}

#[async_trait::async_trait]
impl AgentDriver for SearchEnvironmentDriver {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    async fn probe(&self) -> anyhow::Result<Probe> {
        self.inner.probe().await
    }

    async fn start(
        &self,
        spec: RunSpec,
        tx: tokio::sync::mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.inner.start(spec, tx).await
    }

    async fn start_with_images(
        &self,
        spec: RunSpec,
        images: ValidatedImages,
        tx: tokio::sync::mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.inner.start_with_images(spec, images, tx).await
    }

    async fn start_conversation(
        &self,
        spec: RunSpec,
        images: ValidatedImages,
        tx: tokio::sync::mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.inner.start_conversation(spec, images, tx).await
    }

    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .inheriting(flags)
            .map(|inner| self.wrapped(inner))
    }

    fn with_evidence(
        &self,
        target: crate::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .with_evidence(target)
            .map(|inner| self.wrapped(inner))
    }

    fn with_settings(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        self.inner
            .with_settings(settings)
            .map(|result| result.map(|inner| self.wrapped(inner)))
    }

    fn configured(&self, configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .configured(&self.with_search_path(configuration))
            .map(|inner| self.wrapped(inner))
    }

    fn with_filesystem_fence(
        &self,
        fence: &engine::supervisor::FilesystemFence,
    ) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .with_filesystem_fence(fence)
            .map(|inner| self.wrapped(inner))
    }

    fn protected_readiness(&self) -> Option<anyhow::Result<()>> {
        self.inner.protected_readiness()
    }

    fn prepare_protected_step(
        &self,
        settings: &StepSettings,
    ) -> Option<anyhow::Result<engine::drivers::PreparedProtectedStep>> {
        self.inner.prepare_protected_step(settings).map(|result| {
            result.map(|mut prepared| {
                prepared.driver = self.wrapped(prepared.driver);
                prepared
            })
        })
    }

    fn with_budget(&self, dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        self.inner
            .with_budget(dollars)
            .map(|inner| self.wrapped(inner))
    }

    /// 2026-09 (Z-01d) — TEJ DELEGACJI TU NIE BYŁO i to była cała różnica między znacznikiem,
    /// który istnieje, a znacznikiem, który dojeżdża.
    ///
    /// Fabryka `agent_drivers_with_search` oddaje OBA produkcyjne sterowniki opakowane w ten
    /// dekorator, więc `configured_driver_for_agent` woła `for_step` **na nim**, nie na
    /// `ClaudeDriver` ani `CodexDriver`. Bez tej metody odpowiadał tu domyślny `None` z traitu,
    /// wołający brał `unwrap_or(driver)` — czyli sterownik bez znacznika — i każdy prawdziwy
    /// proces vendora szedł do `spawn_tagged` z `None`. Zielona bramka tego nie widziała, bo
    /// testy podstawiają własny sterownik i tego dekoratora nie mają wcale.
    fn for_step(&self, tag: &engine::supervisor::StepTag) -> Option<Arc<dyn AgentDriver>> {
        self.inner.for_step(tag).map(|inner| self.wrapped(inner))
    }

    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        self.inner.reflecting().map(|inner| self.wrapped(inner))
    }

    fn effort_argv(&self, level: &str) -> Vec<String> {
        self.inner.effort_argv(level)
    }

    fn carries_extra_dirs(&self) -> bool {
        self.inner.carries_extra_dirs()
    }

    fn narrows_its_tools(&self) -> bool {
        self.inner.narrows_its_tools()
    }
}

/// Zakłada na sterownik dekorator zamrożonego świata wyszukiwania.
///
/// **Jedyna droga, którą powstaje [`SearchEnvironmentDriver`]**, i dlatego jest publiczna:
/// bieg widzi sterowniki wyłącznie przez ten dekorator, więc każdy szew traitu, którego on nie
/// deleguje, jest w produkcji martwy — niezależnie od tego, jak kompletny jest adapter vendora
/// pod spodem. Kryterium, które chce dowieść, że coś dojeżdża do prawdziwego procesu, musi
/// przejść tędy; sterownik podstawiony obok tej funkcji sądzi inny kształt niż ten, który
/// biegnie u człowieka (2026-09, Z-01d — dokładnie tak zniknął znacznik biegu przy pierwszym
/// podejściu, przy zielonej bramce).
#[must_use]
pub fn driver_with_frozen_search(
    inner: Arc<dyn AgentDriver>,
    path: std::ffi::OsString,
) -> Arc<dyn AgentDriver> {
    Arc::new(SearchEnvironmentDriver { inner, path })
}

/// Produkcyjna para sterowników z binarkami i środowiskiem zamrożonymi przed pierwszym biegiem.
#[must_use]
pub fn agent_drivers_with_search(search: &AgentCliSearch) -> Drivers {
    let path = search.child_path().unwrap_or_default();
    let configuration = DriverConfiguration {
        environment: vec![("PATH".to_owned(), path.clone())],
        ..DriverConfiguration::default()
    };
    let claude = driver_with_frozen_search(
        Arc::new(
            ClaudeDriver::with_binary(search.resolve("claude"))
                .with_configuration(configuration.clone()),
        ),
        path.clone(),
    );
    let codex = driver_with_frozen_search(
        Arc::new(
            CodexDriver::with_binary(search.resolve("codex")).with_configuration(configuration),
        ),
        path,
    );
    Arc::new(move |vendor| match vendor {
        Vendor::ClaudeCode => Arc::clone(&claude),
        Vendor::Codex => Arc::clone(&codex),
    })
}

/// Wpina `tracing` w plik pod `dir` i zwraca ścieżkę tego pliku. Zdarzenia lecą jednocześnie
/// na wyjście diagnostyczne i do pliku, bo uruchomiona dwuklikiem aplikacja nie ma tego
/// pierwszego: `LaunchServices` je wyrzuca, więc release bez pliku jest niediagnozowalny.
///
/// Uchwyt pliku jest JEDEN na cały bieg i trzyma go [`logging::BoundedLogWriter`], nigdy
/// `try_clone()` na linijkę: w Murmurze to był `dup(2)` na linijkę i panika z wyczerpania
/// deskryptorów wewnątrz samego logowania [T8 §9, 2026-08-15].
///
/// [`logging::BoundedLogLayer`] formatuje wprost do ograniczonego wpisu i ten sam gotowy wpis
/// rozdziela na plik i na stderr — bez pośredniego, nieograniczonego `String` i bez drugiego
/// formattera. Odmowa otwarcia przechodzi przez `io::Error::other`, więc zdanie, które `run()`
/// wypisuje niżej, niesie już nazwę pliku i powód.
pub fn install_logging(dir: &Path) -> io::Result<PathBuf> {
    let path = dir.join(LOG_FILE);
    let to_file = BoundedLogWriter::open(dir, LOG_FILE_LIMIT).map_err(io::Error::other)?;

    // Bez RUST_LOG i tak chcemy dziennik: aplikacja odpalona dwuklikiem nie ma jak go dostać.
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy(std::env::var("RUST_LOG").unwrap_or_default());

    // Kody sterujące terminala w pliku, który czyta się rok później, to szum — nasza warstwa
    // nie pisze ich w ogóle, więc `with_ansi(false)` nie ma już czego wyłączać.
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(BoundedLogLayer::new(to_file).with_stderr());

    tracing::subscriber::set_global_default(subscriber).map_err(io::Error::other)?;

    Ok(path)
}

/// Wpina hak paniki, który najpierw loguje przez `tracing`, a potem **woła poprzedni hak**.
///
/// Łańcuchowanie, nie zastąpienie: tokio połyka paniki na granicy zadania, a domyślny hak pisze
/// wyłącznie na wyjście diagnostyczne, które `LaunchServices` wyrzuca — hak, który zastępuje
/// poprzedni, kasuje jedyny ślad po pierwszej panice w release [T8 §9, 2026-08-15].
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}");
        previous(info);
    }));
}

/// Katalog użytkownika Loadouta. Pliki są prawdą, a dziennik leży obok nich
/// (`docs/ARCHITECTURE.md` §8).
/// Katalog domowy CZŁOWIEKA — nie biblioteka Loadouta.
///
/// Import czyta stąd `~/.claude.json`, żeby serwery MCP zapisane `claude mcp add` w zakresie
/// lokalnym albo użytkownika też trafiły na listę połączeń. Osobna funkcja obok
/// [`loadout_dir`], bo pomylenie tych dwóch znaczy szukanie cudzej konfiguracji w naszym
/// katalogu albo pisanie naszych plików do czyjegoś.
fn your_home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn loadout_dir() -> PathBuf {
    // HOME zamiast osobnej zależności na katalogi: to jedyne miejsce w repo, które o to pyta.
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from(".loadout"),
        |home| PathBuf::from(home).join(".loadout"),
    )
}

/// Katalog, w którym pracują agenci tego okna.
///
/// ZMIERZONE 2026-08-17, przy pierwszym prawdziwym uruchomieniu. Pierwsza wersja brała po prostu
/// `current_dir()` i to jest ZŁE w obu przypadkach, w jakich ta aplikacja startuje: `npm run
/// tauri dev` uruchamia cargo z `src-tauri/`, więc bieg zakładałby `src-tauri/.loadout/runs/`
/// — w środku drzewa źródeł, gdzie `.gitignore` tego NIE łapie (ignoruje `/.loadout/runs/*`,
/// czyli tylko w korzeniu). Artefakty biegu wjechałyby do repozytorium i zapaliłyby
/// `quick-scope` przy pierwszym zadaniu. Kliknięcie w ikonę daje z kolei `/`.
///
/// Katalog projektu ma WYBIERAĆ CZŁOWIEK — to są karty workspace'ów z T-08 i `ARCHITECTURE.md`
/// §6a („karty odpowiadają na »w którym folderze«"). Ta droga nie dochodzi jeszcze do stanu,
/// więc do tego czasu: `LOADOUT_PROJECT`, jeśli ktoś go poda, a w przeciwnym razie DEDYKOWANY
/// katalog w bibliotece. Dedykowany, a nie `current_dir()`, bo cicha praca w nieoczekiwanym
/// miejscu jest gorsza niż praca w miejscu nudnym, ale nazwanym w dzienniku.
fn project_dir(home: &Path) -> PathBuf {
    std::env::var_os("LOADOUT_PROJECT").map_or_else(|| home.join("workspace"), PathBuf::from)
}

/// Domyka biegi, które zginęły razem z poprzednim uruchomieniem aplikacji.
///
/// Trzy kroki, w tej kolejności i nie w innej: przeczytaj, ROZSTRZYGNIJ, dopiero potem działaj.
/// `recovery::decide` jest czystą funkcją i to ona trzyma wszystkie zasady — łącznie ze
/// strażnikiem czasu startu maszyny, bez którego `killpg` po zapisanym `pgid` trafiałby po
/// restarcie w niewinny proces (`kern.maxproc` = 16 000, PID-y przewijają się w godzinach).
///
/// Domykacz wstrzykujemy jako domknięcie, a nie wołamy w środku `apply`: dzięki temu kryterium
/// akceptacji może podstawić własny i sprawdzić, że NIC nie zostało zabite, bez zabijania
/// czegokolwiek na prawdziwej maszynie.
///
/// PUBLICZNA, i to nie jest ustępstwo na rzecz testu. To jest NAZWANA FAZA startu aplikacji,
/// a kryterium AC-2 z `tasks/T-35.md` żąda dowodu, że biegnie ona przez ścieżkę startową, a nie
/// że `decide()` da się zawołać wprost — bo `decide()` dawało się wołać od T-20 i przez cały ten
/// czas nie wołał go nikt. Prywatna funkcja byłaby niesprawdzalna dokładnie w tym jednym
/// wymiarze, o który tu chodzi.
pub async fn recover_from_last_time(
    store: &store::Store,
) -> Result<(usize, usize, recovery::RecoveryReport), Box<dyn std::error::Error>> {
    let rows = recovery::rows_to_judge(&store.reader()?)?;
    if rows.is_empty() {
        return Ok((0, 0, recovery::RecoveryReport::default()));
    }

    let machine = recovery::Machine {
        // Brak odpowiedzi z systemu zapisujemy jako pusty napis, a nie jako zgadniętą wartość:
        // pusty nie zrówna się z żadnym zapisanym znacznikiem, więc strażnik wstrzyma strzał.
        // Zgadnięta wartość mogłaby przypadkiem trafić i wtedy strażnik byłby ozdobą.
        boot_id: engine::supervisor::machine_booted_at().unwrap_or_default(),
        own_pgid: engine::supervisor::own_process_group(),
    };

    let plan = recovery::decide(&rows, &machine);
    /* 2026-09 (Z-01d) — POLITYKA STRZAŁU PRZYJEŻDŻA Z RDZENIA, nie stoi tutaj drugi raz.
     *
     * Do tego dnia były w tym miejscu dwa ramiona `match` nad `reap_group` — czyli własna kopia
     * decyzji o tym, kiedy wolno zabić zastaną grupę, bez pytania jej o znacznik biegu. Dwie
     * kopie znaczyły, że ta droga zabija cudze grupy, a droga folderu ich nie tyka: różnica
     * zależna wyłącznie od tego, gdzie mieszka bieg (niezmiennik 23).
     *
     * 2026-09 (Z-30) — I CZEKA NA WSZYSTKIE NARAZ. Wersja synchroniczna sprzątała grupy po
     * kolei, w wątku, który za chwilę ma pokazać okno: jedna sierota ignorująca SIGTERM to pełne
     * okno łaski plus dowód po dziewiątce, a pięć takich to pięć takich okien jedno za drugim. */
    let report = commands::reconcile::reap_what_is_ours_concurrently(&plan).await;

    // Zapis idzie JEDYNYM pisarzem (niezmiennik 2), a nie własnym połączeniem: drugie
    // połączenie zapisujące do tej bazy jest zakleszczeniem, nie „czasem wolniej", i
    // `checks/quick-boundary.sh` czyta konstruktory połączeń gerpem właśnie po to.
    let runs: Vec<(String, String)> = plan
        .run_status
        .iter()
        .map(|c| (c.run_id.clone(), c.status.clone()))
        .collect();
    let steps: Vec<(String, String, String)> = plan
        .step_status
        .iter()
        .map(|c| (c.step_id.clone(), c.status.clone(), c.reason.clone()))
        .collect();
    let counts = (runs.len(), steps.len());

    // `await`, nie `block_on`: TA funkcja nie ma prawa decydować, jak jej wołający mostkuje
    // sync z async. Zmierzone 2026-08-17 — `block_on` w środku panikuje zdaniem „Cannot start
    // a runtime from within a runtime", kiedy woła ją ktoś, kto już jest w runtime (a kryterium
    // akceptacji jest właśnie takim wołającym). Most stoi więc w `setup`, czyli w jedynym
    // miejscu, które naprawdę nie jest asynchroniczne.
    store.writer().recovered(runs, steps).await?;
    Ok((counts.0, counts.1, report))
}

/// W którym miejscu drogi wyjścia stoi ta aplikacja.
///
/// # 2026-09 (Z-6) — TRZY STANY, NIE DWA, i ta różnica jest cała treścią niezmiennika 6
///
/// Pierwsze podejście miało tu jednorazowy `AtomicBool` i przepuszczało KAŻDE kolejne żądanie
/// wyjścia. To nie był kompromis, tylko wada: drugie ⌘Q wciśnięte w trakcie sprzątania kończyło
/// proces natychmiast — w środku eskalacji zabijania, przed dowodami śmierci grup i przed
/// domknięciem indeksu. Czyli dokładnie ta sierota, przed którą stoi cała ta droga.
///
/// „Zamykanie trwa" i „posprzątane" muszą więc być osobnymi odpowiedziami, bo wołający robi na
/// nich dwie różne rzeczy: pod pierwszą wstrzymuje wyjście i NIE odpala drugiego sprzątania, pod
/// drugą nie wstrzymuje już nic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WayOut {
    /// Nikt jeszcze nie prosił o wyjście.
    Open,
    /// Sprzątanie idzie TERAZ: biegi schodzą i każdy ma jeszcze oddać dowód śmierci swojej grupy.
    ClosingDown,
    /// Sprzątanie się skończyło. Od tej chwili nie ma już czego chronić.
    Done,
}

/// Zapadka drogi wyjścia, wspólna dla obu zdarzeń okna.
///
/// Jedna na proces i **wspólna z premedytacją**: czerwony guzik i ⌘Q to dwa różne zdarzenia Tauri
/// nad jedną robotą, więc dwie osobne zapadki znaczyłyby dwa sprzątania naraz nad tymi samymi
/// procesami.
///
/// To NIE jest globalny bool anulowania z niezmiennika 7: nie odpowiada na pytanie „czy ta
/// operacja jest anulowana", nie przecieka między operacjami i nigdy nie wraca do `Open`.
/// Odpowiada na jedno pytanie całego procesu — jak daleko zaszła droga wyjścia.
///
/// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8): każde wzięcie tego zamka
/// mieści się w jednym wyrażeniu, a sprzątanie czeka na dowody już bez niego.
#[derive(Debug)]
struct TheWayOut {
    phase: Mutex<WayOut>,
}

impl TheWayOut {
    fn new() -> Self {
        Self {
            phase: Mutex::new(WayOut::Open),
        }
    }

    /// Czy TEN wołający ma odpalić sprzątanie. Odpowiada „tak" dokładnie raz na proces.
    ///
    /// Drugie i trzecie żądanie dostają `false` i mają wyłącznie wstrzymać wyjście: sprzątanie
    /// jest już w drodze, a druga jego kopia biłaby się z pierwszą o te same procesy i mogłaby
    /// dojść do końca szybciej, czyli wyjść przed dowodami, na które tamta jeszcze czeka.
    fn mine_to_close(&self) -> bool {
        let mut phase = self.phase.lock().unwrap_or_else(PoisonError::into_inner);
        if *phase == WayOut::Open {
            *phase = WayOut::ClosingDown;
            return true;
        }
        false
    }

    /// Czy sprzątanie się już skończyło.
    ///
    /// Wołający przestaje wtedy wstrzymywać cokolwiek — i to jest jednocześnie jedyne wyjście
    /// awaryjne tej zapadki: gdyby `app.exit(0)` gdzieś przepadło, ⌘Q dalej zamyka aplikację.
    fn finished(&self) -> bool {
        *self.phase.lock().unwrap_or_else(PoisonError::into_inner) == WayOut::Done
    }
}

/// Przestawia zapadkę na [`WayOut::Done`], KTÓRYMKOLWIEK wyjściem ze sprzątania.
///
/// 2026-09 (Z-6) — `Drop`, a nie linia na końcu zadania, i to jest różnica między aplikacją, którą
/// da się zamknąć zawsze, a taką, która po panice w sprzątaniu przestaje odpowiadać na ⌘Q na
/// zawsze: dopóki stoi `ClosingDown`, każde żądanie wyjścia jest wstrzymywane. Tokio połyka paniki
/// na granicy zadania, więc bez tej gwardii ten jeden przypadek zostawiałby okno bez wyjścia.
#[derive(Debug)]
struct ClosedWhenDropped(Arc<TheWayOut>);

impl Drop for ClosedWhenDropped {
    fn drop(&mut self) {
        *self.0.phase.lock().unwrap_or_else(PoisonError::into_inner) = WayOut::Done;
    }
}

/// Sprzątnij — najwyżej raz — i wyjdź. **Wspólne ciało obu dróg wyjścia.**
///
/// # 2026-09 (Z-6) — dlaczego jedno ciało, a nie dwa domknięcia
///
/// Bo czerwony guzik i ⌘Q to dwa RÓŻNE zdarzenia Tauri nad jedną robotą, a dwie kopie tej samej
/// odpowiedzi rozjeżdżają się po cichu: pierwsze podejście miało tu dwie i to właśnie w jednej
/// z nich drugie żądanie kończyło proces w środku eskalacji zabijania. Tutaj zostaje wyłącznie
/// transport — całą politykę trzyma [`ipc::AppState::close_everything_down`] (niezmiennik 23),
/// a każde ze zdarzeń ma po pięć linii adaptera: „nie wstrzymuj po sprzątaniu, wstrzymaj
/// w trakcie, zawołaj to".
///
/// `exit(0)`, nie `destroy()`: zniszczenie ostatniego okna każe Tauri wysłać
/// `ExitRequested { code: None }`, czyli to samo zdarzenie, co ⌘Q — więc obsługa musiałaby ten
/// jeden przypadek przepuszczać, a przepuszczając go, przepuszczałaby też drugie ⌘Q wciśnięte
/// w trakcie sprzątania. Nasze własne wyjście jedzie z kodem, którego tamta obsługa nie tyka.
fn leave_when_everything_is_down(app: &tauri::AppHandle, way_out: &Arc<TheWayOut>) {
    // Sprzątanie odpala się najwyżej raz na proces: druga jego kopia biłaby się z pierwszą o te
    // same procesy i mogłaby dojść do `exit` przed dowodami, na które tamta jeszcze czeka.
    if !way_out.mine_to_close() {
        return;
    }
    let app = app.clone();
    let way_out = Arc::clone(way_out);
    tauri::async_runtime::spawn(async move {
        let cleaned_up = ClosedWhenDropped(way_out);
        app.state::<ipc::AppState>().close_everything_down().await;
        // Zapadka przechodzi w `Done` TU, przed `exit(0)`: od tej chwili każde żądanie wyjścia
        // jest prawdziwe i nikt go już nie wstrzymuje.
        drop(cleaned_up);
        app.exit(0);
    });
}

/// Otwiera okno. Cała powłoka po stronie Rusta zaczyna się tutaj i tutaj kończy.
pub fn run() {
    match install_logging(&loadout_dir()) {
        Ok(path) => tracing::info!("this run writes to {}", path.display()),
        Err(error) => {
            let mut stderr = io::stderr().lock();
            // 2026-08-31: jeśli stderr też odmawia, nie ma drugiego bezpiecznego sinka;
            // opisanie tej awarii przez `tracing` wróciłoby do nieotwartego dziennika.
            let _diagnostic_error = write_log_open_error(&mut stderr, &error);
        }
    }
    install_panic_hook();

    /* STAN APLIKACJI — podłączony 2026-08-17, po tym jak okno stanęło i nie umiało nic zapisać.
     *
     * `ipc.rs` sam to zgłosił w komentarzu: „nikt jej jeszcze nie oddaje builderowi… trzy komendy
     * biegu są zarejestrowane i odmawiają wywołania zdaniem »state not managed«". Pisarz T-30 nie
     * mógł tego dopisać — `lib.rs` nie był w jego OWNS — i słusznie zostawił to człowiekowi
     * (AGENTS.md §7). To jest ta decyzja.
     *
     * Bez `.manage(…)` KAŻDA komenda biorąca `State<'_, AppState>` pada pod palcem, a
     * `generate_handler!` tego nie widzi: rejestracja i stan to dwie różne rzeczy, więc lista
     * komend jest kompletna i aplikacja i tak nie działa.
     *
     * PROJEKT to katalog, w którym stoi proces. Karty workspace'ów (T-08) wybiorą go per karta,
     * ale ta droga jeszcze nie dochodzi do stanu; do tego czasu jedno okno pracuje nad jednym
     * katalogiem i mówi o tym wprost w dzienniku, zamiast po cichu pisać nie tam, gdzie myślisz. */
    let home = loadout_dir();
    let project = project_dir(&home);
    tracing::info!(
        "library at {}, project at {}",
        home.display(),
        project.display()
    );

    // Jedna zapadka na obie drogi wyjścia; powód, dla którego jest wspólna, stoi przy [`TheWayOut`].
    let way_out = Arc::new(TheWayOut::new());

    let outcome = tauri::Builder::default()
        .setup(move |app| {
            /* Baza otwiera się WEWNĄTRZ runtime'u Tauri, i to nie jest ozdoba składniowa.
             *
             * ZMIERZONE 2026-08-17: `setup` biegnie na wątku głównym i **nie jest** kontekstem
             * tokio, więc `Store::open` — który pyta `Handle::try_current()` — nie znajduje
             * runtime'u i cała aplikacja pada zdaniem „a store can only be opened from inside
             * a tokio runtime". Panika w `setup` jest nieodwracalna (`panic in a function that
             * cannot unwind`), więc okno nie zdąża się nawet pokazać.
             *
             * Poprzednia wersja tego komentarza twierdziła, że `setup` runtime MA. Twierdzenie
             * było nieprawdziwe i kosztowało jedno uruchomienie; zostaje zapisane, bo następny
             * czytelnik zada dokładnie to samo pytanie.
             *
             * `block_on` z runtime'u Tauri, a nie własny `Runtime::new()`: druga pętla zdarzeń
             * w tym procesie to drugi zestaw wątków i drugie miejsce, w którym żyją zadania
             * biegu. */
            let store = tauri::async_runtime::block_on(async {
                store::Store::open(&home.join("loadout.db"))
            })?;

            /* Fabryka sterowników. Funkcja, nie mapa — trzeci vendor ma wejść bez wydania
             * Loadouta (`commands/mod.rs`). Każde ramię oddaje własny adapter, bo
             * `SessionRef::vendor` zapisuje tę odpowiedź do bazy i podstawienie innego
             * sterownika skłamałoby o tym, kto wykonał krok.
             *
             * Oba istniejące adaptery muszą być żywe także dla analizy importu: atrapą Codeksa
             * aplikacja pokazywała wybór, który zawsze odmawiał. */
            let drivers = agent_drivers_with_search(&AgentCliSearch::for_process());

            /* ODZYSKIWANIE PO AWARII — wpięte 2026-08-17 (T-35 AC-2), i do tego dnia było
             * STRUKTURALNIE MARTWE. `recovery::decide()` i `apply()` istniały od T-20, miały
             * własne kryteria i **nikt ich nie wołał**; do tego nikt nie zapisywał czasu startu
             * maszyny, więc gdyby je wtedy wpiąć, każdy wiersz padłby na `NO_BOOT_TIME` i nic
             * by nie posprzątało. Mechanizm był zielony w testach i nie mógł zadziałać.
             *
             * Biegnie TUTAJ, przed oddaniem stanu oknu: bieg, który zginął razem z aplikacją,
             * ma być oznaczony, ZANIM człowiek zobaczy listę. Ekran pokazujący `running` dla
             * czegoś, czego nikt już nie prowadzi, jest gorszy niż pusta lista — bo wygląda
             * na pracę w toku.
             *
             * PORAŻKA ODZYSKIWANIA NIE ZABIERA OKNA. Aplikacja, która nie wstaje, bo nie udało
             * się posprzątać po poprzednim uruchomieniu, zamyka człowieka poza jego własnymi
             * plikami. Zdanie idzie do dziennika i idziemy dalej. */
            // `block_on` TUTAJ, bo `setup` Tauri nie jest kontekstem async, a pisarz magazynu
            // jest zadaniem tokio. To jest jedyne miejsce w tym łańcuchu, które musi mostkować.
            match tauri::async_runtime::block_on(recover_from_last_time(&store)) {
                Ok((runs, steps, report)) if runs + steps > 0 => tracing::info!(
                    "recovery: {runs} run(s) and {steps} step(s) marked interrupted; \
                     {} group(s) proven dead, {} still alive, {} belong to someone else",
                    report.reaped.len(),
                    report.unproven.len(),
                    report.foreign.len()
                ),
                Ok(_) => tracing::debug!("recovery: nothing was left running"),
                Err(error) => {
                    tracing::error!(
                        "recovery could not finish, opening the window anyway: {error}"
                    );
                }
            }

            let state = ipc::AppState::new(home.clone(), project.clone(), store, drivers);
            /* Drugie sprzątanie, i NIE jest to powtórka tego wyżej. Tamto czyta bazę biblioteki
             * i pisze do bazy; biegi mieszkają w plikach, w katalogu KAŻDEGO projektu z osobna,
             * i tamta droga nie widziała ich nigdy — zmierzone 2026-08-23: biblioteka miała 19
             * biegów i ani jednego `running`, a trzy zombie właściciela nie były w niej wcale.
             *
             * Kolejność jest wiążąca: przed `manage`, czyli zanim okno zdąży cokolwiek zamówić.
             * Powód stoi przy `settle_everything_left_behind` — to jedyny moment, w którym
             * „biegnie" na pewno znaczy „po kimś innym". */
            /* Drugi most przez `block_on`, z tego samego powodu co ten wyżej: `setup` Tauri jest
             * synchroniczne, a od 2026-09 (Z-10) uzgodnienie folderu oddaje pracę gita puli
             * blokującej, więc jest `async`. Blokujemy tu świadomie i bez kosztu dla człowieka:
             * okno jeszcze nie istnieje, a `manage` niżej i tak musi na to poczekać. */
            tauri::async_runtime::block_on(state.settle_everything_left_behind(&home));
            app.manage(state);
            Ok(())
        })
        // Pierwsza w kolejności i tak ma zostać: druga kopia Loadouta to drugi zestaw agentów
        // pod tymi samymi plikami. Zamiast otwierać kolejne okno, podnosimy to, które jest.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.set_focus();
            }
        }))
        /* ZAMKNIĘCIE OKNA ZATRZYMUJE BIEG, i to jest transport, nie polityka: całą decyzję
         * podejmuje `ipc::AppState::close_everything_down` (niezmiennik 1 i 23), a tutaj zostaje
         * wyłącznie „wstrzymaj zamknięcie, zawołaj, potem zamknij".
         *
         * `prevent_close` PRZED czymkolwiek innym: bez tego okno znika w tej samej chwili, proces
         * kończy się razem z nim, a zadanie zatrzymujące bieg nie ma już gdzie działać — czyli
         * agenci zostają żywi, dokładnie tak, jak było do 2026-08-19.
         *
         * `prevent_close` BEZWARUNKOWO, a sprzątanie najwyżej raz: drugie kliknięcie w czerwony
         * guzik ma tylko wstrzymać zamknięcie, nigdy odpalić drugą kopię sprzątania nad tymi
         * samymi procesami. Reszta — łącznie z tym, czym się kończy — stoi przy
         * [`leave_when_everything_is_down`]. */
        .on_window_event({
            let way_out = Arc::clone(&way_out);
            move |window, event| {
                let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                    return;
                };
                // Po posprzątaniu okno wolno zamknąć: nie ma już czego chronić, a to jest
                // jednocześnie wyjście awaryjne, gdyby `exit(0)` gdzieś przepadło.
                if way_out.finished() {
                    return;
                }
                api.prevent_close();
                leave_when_everything_is_down(window.app_handle(), &way_out);
            }
        })
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(ipc::command_handler())
        .build(tauri::generate_context!());

    let app = match outcome {
        Ok(app) => app,
        Err(error) => {
            tracing::error!("Loadout could not open its window: {error}");
            std::process::exit(1);
        }
    };

    /* ⌘Q I QUIT Z MENU NIE ZAMYKAJĄ OKNA — kończą aplikację, i to jest cała treść tej obsługi.
     *
     * Z AUDYTU 2026-09-02 (L-6): `CloseRequested` dostaje wyłącznie czerwony guzik i ⌘W. Wyjście
     * z menu wysyła `RunEvent::ExitRequested` i do tego dnia nie łapał go nikt, więc proces
     * znikał, zanim cokolwiek zdążyło zejść — agenci przechodzili pod PID 1 i palili limit
     * dalej (niezmiennik 6), a `loadout.db-wal` zostawał z dziesiątkami megabajtów.
     *
     * `code: None` to prośba człowieka; `code: Some(_)` niesie nasze własne `exit(0)` — z obu
     * dróg — i **nie wolno** go wstrzymywać. Aplikacja, która odmawia własnemu wyjściu, jest
     * niezamykalna, a to jest gorsze niż wszystko, przed czym ta obsługa stoi.
     *
     * 2026-09 (Z-6) — WSTRZYMUJEMY KAŻDĄ PROŚBĘ, DOPÓKI SPRZĄTANIE TRWA, a nie tylko pierwszą.
     * Pierwsze podejście przepuszczało drugą i kończyło proces w środku eskalacji zabijania,
     * przed dowodami śmierci grup i przed domknięciem indeksu — czyli łamało niezmiennik 6
     * dokładnie tam, gdzie miało go dowieść. Sprzątanie odpala się przy tym najwyżej raz
     * ([`TheWayOut::mine_to_close`]), bo druga jego kopia biłaby się z pierwszą o te same procesy.
     *
     * Że to się nie zamienia w okno bez wyjścia, stoi na dwóch rzeczach, obie mierzalne: każdy
     * krok sprzątania ma własny sufit czasu (`AppState::close_everything_down`), a zapadka
     * przechodzi w `Done` na `Drop`, więc także po panice. */
    app.run(move |app, event| {
        let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        else {
            return;
        };
        // Posprzątane: nie ma czego wstrzymywać. Wstrzymanie tutaj zamieniłoby zgubione `exit(0)`
        // w aplikację, której nie da się zamknąć.
        if way_out.finished() {
            return;
        }
        api.prevent_exit();
        leave_when_everything_is_down(app, &way_out);
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ClosedWhenDropped, TheWayOut};

    /// Co robi powłoka okna z odpowiedziami zapadki — te same dwa pytania, w tej samej kolejności,
    /// co w obu domknięciach w [`super::run`].
    ///
    /// Zdarzeń Tauri nie da się w teście podstawić: `RunEvent` i `ExitRequestApi` przychodzą
    /// z pętli zdarzeń, której bez okna nie ma. Sądzimy więc DECYZJĘ, bo to ona była wadą —
    /// transport nad nią to dwa `if`-y, jeden na zdarzenie.
    fn what_the_shell_would_do(way_out: &TheWayOut) -> (bool, bool) {
        if way_out.finished() {
            return (false, false);
        }
        (true, way_out.mine_to_close())
    }

    #[test]
    fn a_second_way_out_is_held_back_and_never_starts_a_second_cleanup() {
        let way_out = Arc::new(TheWayOut::new());

        // Pierwsza prośba: wstrzymaj i sprzątaj.
        let (held, mine) = what_the_shell_would_do(&way_out);
        assert!(
            held,
            "the first request for the way out was not held back at all"
        );
        assert!(
            mine,
            "nobody was told to do the cleanup, so the first request would leave the runs going"
        );

        // TU PADAŁO PIERWSZE PODEJŚCIE. Zapadka była jednorazowym boolem i drugą prośbę
        // PRZEPUSZCZAŁA, czyli kończyła proces w środku eskalacji zabijania — przed dowodami
        // śmierci grup i przed domknięciem indeksu (niezmiennik 6).
        let cleaned_up = ClosedWhenDropped(Arc::clone(&way_out));
        for which in ["second", "third"] {
            let (held, mine) = what_the_shell_would_do(&way_out);
            assert!(
                held,
                "the {which} request for the way out was let through while the cleanup was still \
                 going. It ends the process in the middle of the kill escalation: the groups never \
                 answer for their death and the index is left open — which is the orphan this \
                 whole path exists to prevent"
            );
            assert!(
                !mine,
                "the {which} request was told to do the cleanup as well. Two cleanups race over \
                 the same processes, and the one that finishes first leaves while the other is \
                 still waiting for proof"
            );
        }

        // Sprzątanie się skończyło: od tej chwili prośba jest prawdziwa i nikt jej nie wstrzymuje.
        drop(cleaned_up);
        let (held, mine) = what_the_shell_would_do(&way_out);
        assert!(
            !held,
            "a request for the way out is still held back after the cleanup finished. Nothing is \
             left to protect, and holding it back is how a lost exit turns into an app that \
             cannot be closed at all"
        );
        assert!(
            !mine,
            "the cleanup was started a second time after it had already finished"
        );
    }

    #[test]
    fn a_panicking_cleanup_still_leaves_a_way_out() {
        let way_out = Arc::new(TheWayOut::new());
        assert!(way_out.mine_to_close(), "the cleanup never started");

        /* Tokio połyka paniki na granicy zadania, więc zadanie sprzątające po prostu SIĘ KOŃCZY
         * i żadna linia po panice już nie biegnie. Gdyby `Done` ustawiała taka linia, zapadka
         * stałaby w `ClosingDown` do końca życia procesu, a wtedy każde ⌘Q byłoby wstrzymywane —
         * czyli okno bez wyjścia (2026-09). Zwinięcie stosu przez panikę porzuca gwardię
         * dokładnie tak, jak porzuca ją tutaj `drop`. */
        drop(ClosedWhenDropped(Arc::clone(&way_out)));

        assert!(
            way_out.finished(),
            "the way out stayed shut after the cleanup task ended without reaching its last \
             line. Every request to leave would be held back from now until the process dies, \
             and there would be no way to make it die"
        );
    }
}
