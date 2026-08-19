//! `trait AgentDriver`, `trait AgentHandle` i typy, które **nie znają ani jednego vendora**
//! [T1 §8.2].
//!
//! Ten plik jest jedynym miejscem, w którym mieszka słownictwo Loadouta o agencie: polityka
//! po ludzku, zdarzenie neutralne, wynik kroku. Nazwy flag, kształt linii JSON i eskalacja
//! anulowania siedzą w `claude.rs` (i w `codex.rs`, kiedy powstanie w T-10) — a jeżeli T-10
//! będzie musiało tknąć ten plik albo `stream.rs`, to znaczy, że ten trait jest fikcją, a nie
//! abstrakcją, i to jest sygnał, nie porażka [PLAN §8, ryzyko 5].
//!
//! **Niezmiennik 23 czyta się tu dosłownie.** Polityka ma trzy warianty po ludzku
//! ([`Policy`]) i **jedną** tabelę tłumaczenia na flagi, w adapterze. Cicha wersja złamania
//! nie wygląda jak nowy trait — wygląda jak `if agent == "claude" { … }` w miejscu wywołania.
//!
//! # Stan tego pliku: KOMPLETNY (2026-08-15)
//!
//! Typy są tu w całości, bo to one są kontraktem, o który opierają się kryteria — a ten plik
//! ma być jedynym, który `CodexDriver` z T-10 przeczyta i **nie będzie musiał zmienić**.
//! Jedyna dziura w implementacji siedzi w `claude.rs`, w kolejnej turze tej samej sesji,
//! i jest opisana tam, przy [`AgentDriver::start`].

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::line::Tool;
use super::supervisor::{GroupId, GroupProof};

/// Vendor, ktory jest w typie, ale nie ma jeszcze adaptera. Fabryka `Drivers` jest funkcja
/// totalna, wiec musi czyms odpowiedziec takze dla `Vendor::Codex` -- do czasu T-10.
pub mod absent;
pub mod claude;
pub mod codex;

/// Wszystko, czego sterownik potrzebuje, żeby uruchomić jeden krok [T1 §8.2].
///
/// **Czego tu nie ma i dlaczego.** `max_turns` i `budget_usd` z T1 §8.2 **nie wchodzą**,
/// dopóki spike S-2 nie rozstrzygnie sprzeczności T1 vs T4 o istnieniu `--max-turns`
/// [`docs/ARCHITECTURE.md` §11]. Pole w strukturze, którego nikt nie umie przetłumaczyć na
/// flagę, jest kontrolką bez handlera (niezmiennik 16) — a sufit i tak egzekwuje limit czasu
/// ściennego z T-03, bo to on robi to, co użytkownik ma na myśli mówiąc „nie mielże
/// w nieskończoność" [T4 §3.3].
#[derive(Debug, Clone)]
pub struct RunSpec {
    /// Identyfikator biegu, wygenerowany przez nas **przed** startem procesu. Dla `claude`
    /// staje się `--session-id`, więc sesja jest znana zanim przyjdzie `system/init` i nie ma
    /// wyścigu o to, pod jakim numerem zapisać krok [T1 §4.6, T7 §6.2].
    pub run_id: Uuid,
    /// Katalog roboczy kroku. Przychodzi **argumentem**, nigdy stałą: ten katalog zna tylko
    /// warstwa wyżej, a literał ze ścieżką repo przewraca granicę z niezmiennika 1.
    pub cwd: PathBuf,
    /// Treść zadania dla agenta. Jedzie **wyłącznie stdinem** (niezmiennik 9) — nigdy w argv,
    /// bo argumenty widzi `ps` każdego użytkownika maszyny.
    pub prompt: String,
    /// Alias albo pełny identyfikator modelu. `None` znaczy „to, co vendor ma domyślnie".
    pub model: Option<String>,
    /// Dopisek do promptu systemowego. To jest **konfiguracja agenta**, nie treść zadania:
    /// treść zadania w tym polu byłaby niezmiennikiem 9 złamanym po cichu, bo stąd wchodzi
    /// do argv.
    pub system_append: Option<String>,
    /// Co agentowi wolno zrobić z plikami, po ludzku. Tłumaczenie na flagi jest jedną tabelą
    /// w adapterze (niezmiennik 23).
    pub policy: Policy,
    /// Katalogi poza `cwd`, do których krok ma mieć dostęp — w praktyce katalog przekazań
    /// [`docs/ARCHITECTURE.md` §8].
    pub extra_dirs: Vec<PathBuf>,
    /// Sesja do wznowienia. `None` przy pierwszej turze kroku.
    pub resume: Option<SessionRef>,
}

/// Co agentowi wolno zrobić z plikami — **po ludzku**, w trzech wariantach [T1 §9].
///
/// Na ekranie brzmią „Read only" / „Can edit this folder" / „No limits". Tłumaczenie na flagi
/// vendora jest **jedną tabelą w jednym adapterze** (niezmiennik 23): rozpisanie go w miejscu
/// wywołania jest dokładnie tym, jak w repo źródłowym po cichu umarło skanowanie sekretów
/// [raport 05 §4].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Czyta i szuka, nie zapisuje niczego.
    ReadOnly,
    /// Czyta, zapisuje i commituje w swoim katalogu.
    EditInFolder,
    /// Bez ograniczeń — i **żaden adapter nie ma prawa udawać**, że jakaś lista narzędzi
    /// jeszcze coś tu ogranicza [T1 §5.2].
    Unrestricted,
}

/// Zdarzenie z biegu agenta, neutralne wobec vendora i **świadomie stratne**: czego nie da się
/// tu wyrazić, tego nie ma na czystym transkrypcie [T1 §8.2].
///
/// Kuracja `AgentEvent` → `Line` należy do T-05. Ten enum jest granicą między „co się stało"
/// a „co widać".
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Sesja ruszyła. `capabilities` są tu, bo **na nich, a nie na numerze wersji**,
    /// feature-detektuje się protokół przerwania [T1 §4.1, §4.6]: czyta je eskalacja
    /// anulowania w adapterze, i to jest cały jej odbiorca (niezmiennik 21).
    Started {
        /// Sesja, którą wznowi kolejna tura.
        session: SessionRef,
        /// Model, którym vendor faktycznie odpowiada — bywa inny niż zamówiony.
        model: String,
        /// Narzędzia, które vendor naprawdę załadował. To jest jedyny uczciwy pomiar tego,
        /// czy izolacja kontekstu zadziałała [T1 §3.3].
        tools: Vec<String>,
        /// Zdolności protokołu ogłoszone przez CLI, np. `interrupt_receipt_v1`.
        capabilities: Vec<String>,
    },
    /// Agent myśli. **Nigdy nie niesie tekstu** i nigdy nie wchodzi do historii — jest stałym
    /// slotem na dole ekranu [`docs/ARCHITECTURE.md` §6, reguła 5].
    Thinking,
    /// Proza agenta, dosłownie.
    Said {
        /// Tekst bloku.
        text: String,
    },
    /// Agent zaczął czynność narzędziem.
    ToolStart {
        /// Identyfikator wywołania — po nim [`AgentEvent::ToolEnd`] trafia do swojej linii.
        id: String,
        /// Etykieta po ludzku, np. „Reading auth.rs". Vendor pisze ją sam w polu
        /// `description`, więc dostajemy ją za darmo [T1 §8.6].
        label: String,
    },
    /// Czynność się skończyła.
    ToolEnd {
        /// Identyfikator wywołania, ten sam co w [`AgentEvent::ToolStart`].
        id: String,
        /// Czy się udała.
        ok: bool,
        /// Jednolinijkowe podsumowanie; pełne wyjście zostaje za kliknięciem.
        summary: String,
    },
    /// Agent zmienił plik.
    FileEdit {
        /// Ścieżka zmienionego pliku.
        path: PathBuf,
    },
    /// Limit u dostawcy. **Osobny wariant, nie [`AgentEvent::Notice`]**, i to jest cała
    /// różnica między „widać banner" a „bieg umie się wznowić o właściwej godzinie": pola
    /// siedzą na drucie **zagnieżdżone** w `rate_limit_info`, a parser napisany pod płaski
    /// kształt po cichu nie widzi nic — deserializacja się udaje, zdarzenia nie ma, bieg nie
    /// pauzuje i dowiadujesz się o tym z rachunku [T1 korekta 3, 2026-08-15].
    RateLimit {
        /// Stan limitu z drutu, np. `allowed`.
        status: String,
        /// Kiedy limit wraca, w sekundach epoki uniksowej.
        resets_at: i64,
        /// Które okno limitu, np. `five_hour`.
        rate_limit_type: String,
        /// Czy bieg ma stanąć. Czyta to T-21 i **nikt poza nim** (niezmiennik 21); samą pauzę
        /// robi tamto zadanie, tu jest tylko fakt.
        pause_run: bool,
    },
    /// Jednorazowa uwaga: ponowienie zapytania, odmowa uprawnień, ostrzeżenie vendora.
    Notice {
        /// Zdanie po angielsku, gotowe na ekran.
        text: String,
    },
    /// Koniec tury. Dokładnie **jedno** takie zdarzenie na turę.
    Finished(Outcome),
}

/// Jedno zdarzenie razem z faktami, których ono samo nie niesie.
///
/// # Dlaczego to jest ładunek KANAŁU, a nie sam [`AgentEvent`] (2026-08-18)
///
/// Zmierzone, nie teoretyczne. `stream::decode` wyjmował z jednej linii drutu i zdarzenie,
/// i [`Tool`] — a kanał sterownika miał typ `mpsc::Sender<AgentEvent>`, więc **`Tool` ginął na
/// granicy sterownika**. Dalej było już tylko widać skutek: `commands::run::forward` musiał
/// podać kuratorowi `tool: None`, `Curator::tool_start` bez faktów oddaje `Vec::new()`, i wiersze
/// `read`, `search`, `edit` oraz `ran` **nie powstawały nigdy**. Widok pracy pokazywał wyłącznie
/// prozę agenta, choć agent czytał pliki i uruchamiał komendy.
///
/// Druga droga naprawy — druga tabela nazw narzędzi w `run.rs` — byłaby drugą implementacją
/// kuracji (niezmienniki 15 i 23), rozjeżdżającą się przy pierwszej zmianie u vendora i po cichu.
/// Dlatego szew jest tutaj: **jeden** typ, którym sterownik mówi o tym, co się stało.
///
/// Adres tego typu jest `drivers`, choć wypełnia go `stream::decode`, i to nie jest kaprys:
/// `AgentDriver::start` należy do tego pliku, a typ jego kanału mieszkający w module obok
/// znaczyłby, że `trait` jest zależny od pętli czytającej strumień, a nie odwrotnie.
/// `stream` re-eksportuje go pod dawnym adresem, żeby jedna nazwa nie miała dwóch ścieżek.
#[derive(Debug)]
pub struct DecodedEvent {
    /// Zdarzenie neutralne wobec vendora.
    pub event: AgentEvent,
    /// To, czego kuracja potrzebuje ponad zdarzenie. `None` dla zdarzeń bez narzędzia.
    pub tool: Option<Tool>,
}

impl From<AgentEvent> for DecodedEvent {
    /// Zdarzenie, które z narzędziem nie ma nic wspólnego — czyli większość.
    ///
    /// Istnieje po to, żeby miejsce wołania nie musiało pisać `tool: None` przy każdym
    /// `Notice`, `Thinking` i `Finished`: pole wypisane ręcznie w dwudziestu miejscach jest
    /// dwudziestoma okazjami, żeby raz wpisać tam `None` tam, gdzie fakt jednak był.
    fn from(event: AgentEvent) -> Self {
        Self { event, tool: None }
    }
}

/// Czym skończyła się tura [T1 §8.2].
///
/// Pola są tu dlatego, że ktoś je czyta (niezmiennik 21): koszt i tury czyta T-06 (zapis do
/// indeksu) i T-05 (linia `Done · 2 turns · 12s · $0.012`), a `session` czyta wznowienie
/// kolejnej tury.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Czy krok się udał. Liczone z `is_error`, **nigdy z `subtype`** — powód stoi przy
    /// [`FinishReason`].
    pub ok: bool,
    /// Dlaczego się skończyło.
    pub reason: FinishReason,
    /// Ostatnia wypowiedź agenta, czyli to, co krok przekazuje dalej.
    pub text: String,
    /// Koszt tury. `None`, kiedy vendor go nie podał — nie zero, bo zero jest liczbą i sumuje
    /// się w rachunek, którego nikt nie zamawiał.
    pub cost_usd: Option<f64>,
    /// Zużycie kontekstu.
    pub tokens: Tokens,
    /// Ile tur agent wykonał w tej wymianie.
    pub turns: u32,
    /// Ile to trwało, według vendora.
    pub took: Duration,
    /// Sesja, w której to się zdarzyło.
    pub session: SessionRef,
}

/// Uchwyt sesji vendora — to, czego potrzeba, żeby wrócić do tej samej rozmowy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    /// Który adapter ją wystawił, np. `claude`.
    pub vendor: &'static str,
    /// Identyfikator sesji u vendora.
    pub id: String,
}

/// Dlaczego tura się skończyła [T1 §8.5].
///
/// **Anulowanie jest wariantem wartości, nigdy błędem** (niezmiennik 7): `Err(Cancelled)`
/// zmusza każdego wołającego do rozróżniania „to się nie udało" od „to zatrzymał człowiek",
/// a rozróżnienie zgubione raz jest zgubione wszędzie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// Skończyło się samo, bez błędu.
    Completed,
    /// Zatrzymał to człowiek.
    Cancelled,
    /// Agent uderzył w sufit — tur, budżetu albo czasu.
    LimitReached,
    /// Cokolwiek innego; niesie powód gotowy na ekran.
    Failed(String),
}

/// Zużycie kontekstu w jednej turze.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Świeże wejście.
    pub input: u64,
    /// Wyjście modelu.
    pub output: u64,
    /// Wejście przeczytane z cache'u. To ta liczba pokazuje, czy izolacja kontekstu działa:
    /// bieg bez niej płacił 36 870 zamiast 4 725 [T1 §3.3, korekta 4].
    pub cached: u64,
}

/// Co wiadomo o CLI vendora **przed** pierwszym biegiem. Napędza ekran ustawień (T-01/T-11).
///
/// **Czego tu nie ma.** T1 §8.2 rysuje jeszcze pole `signed_in`. Nie wchodzi, bo nic nie umie
/// go wypełnić uczciwie: `claude --version` odpowiada tak samo wylogowanemu i zalogowanemu,
/// a jedyny znany sygnał — `Not logged in · Please run /login` — przychodzi dopiero
/// z prawdziwej, płatnej tury [T1 §3.3, 2026-08-15]. Pole, które zawsze mówi „nie", jest
/// gorsze niż jego brak: ekran ustawień pokazałby wtedy fałszywy alarm każdemu zalogowanemu
/// użytkownikowi. Kiedy pojawi się tani sposób na tę odpowiedź, to jest jeden wiersz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// Czy binarka w ogóle jest.
    pub found: bool,
    /// Wersja, jeśli binarka odpowiedziała. Vendorzy dokładają i zabierają flagi co tydzień,
    /// więc to jest liczba, którą chcemy widzieć w zgłoszeniu błędu [T1 ryzyko 2].
    pub version: Option<String>,
}

/// Sterownik jednego vendora. Dwie implementacje od pierwszego dnia (decyzja D3): ta jest
/// pierwszą z dwóch, `CodexDriver` z T-10 jest testem, czy ten trait jest abstrakcją.
#[async_trait]
pub trait AgentDriver: Send + Sync {
    /// Etykieta vendora — ta sama, która ląduje w [`SessionRef::vendor`] i którą T-06 zapisuje
    /// przy kroku, żeby wznowienie wiedziało, do kogo wrócić.
    fn id(&self) -> &'static str;

    /// Czy CLI jest i w jakiej wersji. Biegnie przy starcie aplikacji i **nigdy nie zwraca
    /// błędu z powodu braku binarki**: brak CLI to ekran ustawień, a nie awaria startu.
    async fn probe(&self) -> anyhow::Result<Probe>;

    /// Uruchamia krok. Zdarzenia płyną na `tx` aż do dokładnie jednego
    /// [`AgentEvent::Finished`] na turę.
    ///
    /// Ładunkiem kanału jest [`DecodedEvent`], a nie sam [`AgentEvent`], i powód stoi przy tym
    /// typie: bez faktów o narzędziu wołający nie ma z czego zbudować ani jednego wiersza
    /// `read`, `search`, `edit` czy `ran`.
    async fn start(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>>;
}

/// Żywa sesja jednego agenta.
///
/// Tam, gdzie vendor to potrafi, wszystkie tury idą przez **jeden proces** — a tam, gdzie nie
/// potrafi, adapter odpala świeży proces z `--resume` i wołający nie widzi różnicy [T1 §8.1].
/// Różnica jest w rachunku, nie w typie: wariant z procesem na turę płaci zimny start
/// i odbudowę cache'u za każdym razem.
/// Co da się powiedzieć ŻYWEJ sesji agenta.
///
/// Dwa warianty, bo dwie rzeczy jadą tym samym potokiem i muszą jechać jednym kanałem: kolejna
/// tura i przerwanie w paśmie. Dwa kanały nad jednym `stdin` to wyścig, w którym koperta tury
/// wchodzi w środek prośby o przerwanie — a CLI czyta stdin **linia po linii**, więc rozjechana
/// linia jest turą zgubioną po drugiej stronie.
#[derive(Debug, Clone)]
pub enum ToAgent {
    /// Kolejna tura: to, co napisał człowiek albo bieg.
    Turn(String),
    /// Przerwanie w paśmie, z identyfikatorem prośby.
    Interrupt(String),
}

/// Uchwyt do mówienia do sesji — **klonowalny i bez `&mut`**.
///
/// 2026-08-18 — PO CO TO ISTNIEJE, zgłoszone przez właściciela: „dalej nie działa pisanie do
/// agenta przez terminal". Przyczyna nie była w wierszu wejścia, a tutaj: `stdin` był polem
/// uchwytu, [`AgentHandle::send`] brał `&mut self`, a `one_turn` trzymał ten uchwyt pożyczony
/// mutowalnie przez CAŁĄ turę (`handle.wait()` w `tokio::select!`). Cokolwiek z zewnątrz — okno,
/// komenda, cokolwiek — nie miało jak dosięgnąć żywej sesji, dopóki tura się nie skończy.
/// A wtedy sesji już nie ma, bo `close()` porzuca `stdin`, co JEST jej końcem.
///
/// Głos rozwiązuje to u przyczyny: `stdin` przechodzi na własność jednego zadania-pisarza,
/// a wszyscy pozostali dostają nadajnik. Kolejność linii zostaje zachowana, bo kanał jest jeden
/// i czyta go jeden odbiorca.
pub type Voice = mpsc::Sender<ToAgent>;

#[async_trait]
pub trait AgentHandle: Send {
    /// Sesja tej rozmowy.
    fn session(&self) -> SessionRef;

    /// Głos do tej sesji, jeśli ją da się jeszcze zagadać.
    ///
    /// `None` znaczy „ta sesja nie przyjmuje już nic": po [`AgentHandle::close`] albo w dublerze,
    /// który nie ma procesu. Domyślnie `None`, żeby sterownik bez dwukierunkowego stdinu nie
    /// musiał udawać, że go ma — a wołający dostał odpowiedź „nie da się", nie ciszę.
    fn voice(&self) -> Option<Voice> {
        None
    }

    /// Grupa procesów tej sesji, dopóki żyje. Czyta to T-06 (zapisuje `pid` i `pgid` przy
    /// kroku, zanim popłynie cokolwiek ze stdout [T7 §6.2]) i T-20 (sprzątanie po awarii
    /// aplikacji). `None`, kiedy między turami nie ma żadnego procesu.
    fn group(&self) -> Option<GroupId>;

    /// Kolejna tura w tej samej sesji.
    async fn send(&mut self, text: String) -> anyhow::Result<()>;

    /// Czeka na koniec bieżącej tury.
    async fn wait(&mut self) -> anyhow::Result<Outcome>;

    /// Anuluje turę i **dowodzi**, że po grupie nic nie zostało.
    ///
    /// Zwraca [`GroupProof`], a nie `anyhow::Result<()>`, i to nie jest kwestia gustu:
    /// niezmiennik 6 mówi, że dopóki `kill(-pgid, 0)` nie dał `ESRCH`, grupa jest żywa —
    /// więc `Ok(())` znaczyłoby „wysłałem sygnał", a wołający przeczytałby „nie żyje".
    /// Zmierzone w tym samym kształcie: `A after kill: total=2 orphaned=2` przy statusie
    /// dziecka mówiącym „zabity" [T7 §3.1]. Osierocony agent pali limit w tle; to jest błąd
    /// finansowy, nie higieniczny.
    ///
    /// Eskalacja jest trzystopniowa i **nie wolno jej skracać** [T1 §8.5]: przerwanie w paśmie
    /// tylko pod ogłoszoną zdolnością, potem SIGTERM, potem SIGKILL na grupę. Sterownik, który
    /// od razu strzela dziewiątką, traci wznawialność sesji, dosypanie transkryptu i hooki
    /// `SessionEnd` [T1 §4.6].
    async fn cancel(&mut self) -> GroupProof;

    /// Zamyka wejście sesji i czeka, aż proces wyjdzie **sam**.
    ///
    /// To jest normalne zakończenie kroku, nie anulowanie: `claude` z otwartym stdinem czeka
    /// w nieskończoność, więc bez tego każdy skończony krok zostawiałby żywy proces
    /// [T1 §2, §4.6]. Zwraca kod wyjścia; `None`, kiedy vendor nie trzyma jednego procesu na
    /// sesję albo kiedy proces zginął od sygnału i kodu po prostu nie ma.
    ///
    /// Wolne czytanie stdoutu potrafi opóźnić to wyjście do 30 s — to jest udokumentowane
    /// zachowanie, nie zawieszenie [T1 „Worth adding"].
    async fn close(&mut self) -> anyhow::Result<Option<i32>>;
}
