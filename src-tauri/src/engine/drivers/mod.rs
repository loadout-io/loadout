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

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::line::Tool;
use super::supervisor::{GroupId, GroupProof};
use crate::evidence::EvidenceTarget;

/// Vendor, ktory jest w typie, ale nie ma jeszcze adaptera. Fabryka `Drivers` jest funkcja
/// totalna, wiec musi czyms odpowiedziec takze wtedy, gdy pytanie padnie o vendora bez adaptera.
///
/// 2026-08-24 — KOMENTARZ MOWIL „do czasu T-10" i przestal byc prawda w dniu, w ktorym T-10
/// wyladowalo: `Vendor::Codex` ma [`codex::CodexDriver`] i fabryka wydaje go naprawde. Ten modul
/// zostaje jako **odpowiedz dla trzeciego vendora**, ktory wejdzie w typ przed swoim adapterem —
/// czyli po to, po co powstal, tylko bez daty waznosci, ktora juz minela.
pub mod absent;
pub mod claude;
pub mod codex;
/// Krok „sprawdź": komendę odpala Loadout, werdykt wystawia Loadout, nigdy agent.
///
/// Sąsiad `claude.rs`, choć **nie implementuje** [`AgentDriver`] i nie ma go implementować —
/// to jest treść AC-4 z T-55, a nie pominięcie. Rodzaj sterownika, nie etap biegu:
/// niezmiennik 27 zakazuje warunku NAZYWAJĄCEGO etap, a nie ramienia mówiącego, **czym** jest
/// kafelek. Adres w `drivers/`, bo tu mieszka odpowiedź na pytanie „czym ten krok jedzie".
pub mod command;
/// Reguły `deny` repo gospodarza, przepisane do nas jako **tekst**, nigdy jako maszyneria.
/// Sąsiad `claude.rs`, nie część rdzenia: `.claude/settings.json` to kształt jednego vendora,
/// a ten plik nie zna ani jednego.
pub mod host;

/// Wszystko, czego sterownik potrzebuje, żeby uruchomić jeden krok [T1 §8.2].
///
/// **Czego tu nie ma i dlaczego.** `max_turns` i `budget_usd` z T1 §8.2 **nie wchodzą**,
/// dopóki spike S-2 nie rozstrzygnie sprzeczności T1 vs T4 o istnieniu `--max-turns`
/// [`docs/ARCHITECTURE.md` §11]. Pole w strukturze, którego nikt nie umie przetłumaczyć na
/// flagę, jest kontrolką bez handlera (niezmiennik 16) — a sufit i tak egzekwuje limit czasu
/// ściennego z T-03, bo to on robi to, co użytkownik ma na myśli mówiąc „nie mielże
/// w nieskończoność" [T4 §3.3].
#[derive(Clone)]
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
    /// Czy ten krok może sięgnąć do internetu.
    ///
    /// **Osobno od [`RunSpec::policy`], i to jest cała treść tego pola.** Dial mówi o PLIKACH
    /// („look only" znaczy „nie zmienia plików"), a nie o tym, czy agent widzi świat — więc sieć
    /// wpuszczona w dial dawałaby wybór między „widzi świat i może zepsuć pliki" a „nie zepsuje
    /// niczego i nie widzi nic". To jest dokładnie ta sama granica, którą postawiło T-63.
    ///
    /// **Jedno pole dla obu vendorów**, choć każdy realizuje je czym innym: Claude dwoma
    /// czasownikami w `--tools`/`--allowedTools`, Codex ustawieniem piaskownicy
    /// (`sandbox_workspace_write.network_access`). Nazwa narzędzia w tym miejscu byłaby faktem,
    /// którego jeden z dwóch adapterów nie umie wypowiedzieć.
    ///
    /// 2026-08-23 — z pytania właściciela „czemu dostępu do neta nie mają?". Do tego dnia dla
    /// Codeksa nie było ŻADNEJ drogi: `network_access` nie wychodziło z tej skrzyni ani razu.
    pub reaches_the_web: bool,
    /// Które narzędzia ten krok ma mieć pod ręką — albo `None`, czyli „tyle, ile daje polityka".
    ///
    /// 2026-08-20 (T-63) — DO DZIŚ TEGO POLA NIE BYŁO, a `Agent.tools` (`library::agents::Tools`)
    /// jest polem formularza agenta od T-11: człowiek je ustawia, ekran je pokazuje, dysk je
    /// zapisuje i **nic go nie czyta**. To jest martwa kontrolka (niezmiennik 16) schowana
    /// o warstwę głębiej — nie da się jej zobaczyć, klikając, bo „agent nie użył narzędzia" jest
    /// nieodróżnialne od „agent uznał, że nie warto".
    ///
    /// **Nazwy, nie wariant `Tools`**, i to nie jest kwestia gustu. Ten plik jest granicą, za którą
    /// nie ma ani jednego vendora — i nie ma też definicji agenta: dial `FileAccess` przechodzi
    /// tędy jako [`Policy`], tłumaczony jedną tabelą w warstwie, która zna jedno i drugie
    /// (`commands::run::policy_of`). Wariant biblioteki w tym polu odwróciłby tę strzałkę:
    /// `engine/` zależałby od `library/`, a `library/` zależy już od `workflow/`, które zależy od
    /// `engine::dag`. Zamknięte koło Rust skompiluje i nikt go nie zauważy, dopóki ktoś nie zapyta,
    /// co jest pod czym.
    ///
    /// `None` znaczy „nie zawężaj", czyli DOKŁADNIE dzisiejsze argv: sufit polityki
    /// z `claude::tools_for`. Nie pusta lista — `--tools ""` znaczy u vendora „żadnych narzędzi"
    /// i wygląda jak zawieszony agent, więc lista, która wyszła pusta, jest odmową przy budowie
    /// zadania, nie wartością tego pola [`claude::ToolsRefused::NothingChosen`].
    pub tools: Option<Vec<String>>,
    /// Katalogi poza `cwd`, do których krok ma mieć dostęp — w praktyce katalog przekazań
    /// [`docs/ARCHITECTURE.md` §8].
    pub extra_dirs: Vec<PathBuf>,
    /// Sesja do wznowienia. `None` przy pierwszej turze kroku.
    pub resume: Option<SessionRef>,
}

impl std::fmt::Debug for RunSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /* Prompt, dopisek systemowy, model i absolutny cwd sa danymi prywatnymi. Ten Debug
         * trafia do bledow spawn/transport, wiec redakcja musi byc w typie, nie w kazdym
         * wołajacym, ktory akurat pamietal o niezmienniku 9. */
        formatter
            .debug_struct("RunSpec")
            .field("run_id", &self.run_id)
            .field("cwd", &"<private workspace path>")
            .field("prompt_bytes", &self.prompt.len())
            .field(
                "system_append_bytes",
                &self.system_append.as_ref().map(String::len),
            )
            .field("model", &self.model.as_ref().map(|_| "<configured>"))
            .field("policy", &self.policy)
            .field("reaches_the_web", &self.reaches_the_web)
            .field("tools", &self.tools.as_ref().map(Vec::len))
            .field("extra_dirs", &self.extra_dirs.len())
            .field("resuming", &self.resume.is_some())
            .finish()
    }
}

/// Vendorowe argumenty i jawnie rozwiązane środowisko zatwierdzonych Connections.
/// Własny `Debug` celowo nie pokazuje wartości sekretów.
#[derive(Clone, Default)]
pub struct DriverConfiguration {
    pub arguments: Vec<String>,
    pub environment: Vec<(String, OsString)>,
    /// Nazwy serwerów, które ten krok naprawdę dostał.
    ///
    /// 2026-08-22 — POLE JEST NOWE i bez niego zatwierdzone połączenie nie da się użyć.
    /// Zmierzone na biegu właściciela: serwer `figma` zameldował się jako `connected`, CLI
    /// zarejestrowało **32** jego narzędzia, agent zawołał `get_design_context` i dostał
    /// `permission_denied` — bo `--allowedTools` niesie wyłącznie czasowniki plikowe z dialu,
    /// a `--permission-mode dontAsk` odrzuca resztę bez pytania. Połączenie, które się łączy
    /// i którego nie wolno użyć, jest kontrolką bez skutku (niezmiennik 16).
    ///
    /// Same nazwy, nie narzędzia: konkretne `mcp__<serwer>__<narzędzie>` poznaje się dopiero
    /// po połączeniu, a argv powstaje wcześniej. Adapter składa z nich wzorzec zakresowy.
    pub servers: Vec<String>,
}

impl std::fmt::Debug for DriverConfiguration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DriverConfiguration")
            .field("arguments", &self.arguments)
            .field(
                "environment_names",
                &self
                    .environment
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .field("servers", &self.servers)
            .finish()
    }
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
    ///
    /// Zdanie wyżej zostaje prawdziwe o liście **auto-zatwierdzania** i przestaje być
    /// prawdziwe o liście **dostępności**: pierwsza rzeczywiście nie wiąże
    /// `bypassPermissions`, druga jest twarda i wyjmuje narzędzie z zestawu niezależnie od
    /// trybu uprawnień, więc także tutaj [zmierzone 2026-08-19].
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

const UNKNOWN_PRICE_OPENS: &str = "The price for ";
const UNKNOWN_PRICE_CLOSES: &str = " is not known.";

pub(crate) fn unknown_price_notice(model: &str) -> String {
    format!("{UNKNOWN_PRICE_OPENS}{model}{UNKNOWN_PRICE_CLOSES}")
}

pub(crate) fn is_unknown_price_notice(text: &str) -> bool {
    text.starts_with(UNKNOWN_PRICE_OPENS)
        && text.ends_with(UNKNOWN_PRICE_CLOSES)
        && text.len() > UNKNOWN_PRICE_OPENS.len() + UNKNOWN_PRICE_CLOSES.len()
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

/// Jeden z czterech formatow obrazu, ktore oba wspierane vendory przyjmuja natywnie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageMime {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageMime {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// Prywatne bajty jednego obrazu. Debug celowo nie wypisuje zawartosci.
#[derive(Clone)]
pub struct ImageInput {
    mime: ImageMime,
    bytes: Arc<[u8]>,
}

impl ImageInput {
    /// Buduje prywatny obraz z drutu webviewa, odmawiajac MIME spoza zamknietej listy.
    ///
    /// Szkielet istnieje przed implementacja, zeby SVG i dowolny napis padaly podczas
    /// wykonania acceptance testu, a nie byly niewyrazalne w jego typach.
    pub fn from_wire(mime: &str, bytes: impl Into<Arc<[u8]>>) -> Result<Self, ImageError> {
        let mime = match mime {
            "image/png" => ImageMime::Png,
            "image/jpeg" => ImageMime::Jpeg,
            "image/gif" => ImageMime::Gif,
            "image/webp" => ImageMime::Webp,
            _ => return Err(ImageError::Unsupported),
        };
        Ok(Self::new(mime, bytes))
    }

    #[must_use]
    pub fn new(mime: ImageMime, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            mime,
            bytes: bytes.into(),
        }
    }

    #[must_use]
    pub const fn mime(&self) -> ImageMime {
        self.mime
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl std::fmt::Debug for ImageInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageInput")
            .field("mime", &self.mime)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// Obrazy, ktore przeszly wspolna walidacje przed startem jakiegokolwiek procesu.
#[derive(Clone, Debug, Default)]
pub struct ValidatedImages(Vec<ImageInput>);

impl ValidatedImages {
    /// Jedyna walidacja MIME, magic bytes i limitow dla obu adapterow.
    pub fn validate(images: Vec<ImageInput>) -> Result<Self, ImageError> {
        const MAX_IMAGES: usize = 4;
        const MAX_ONE: usize = 5 * 1024 * 1024;
        const MAX_ALL: usize = 12 * 1024 * 1024;

        if images.len() > MAX_IMAGES {
            return Err(ImageError::TooMany);
        }
        let mut total = 0_usize;
        for image in &images {
            if image.bytes().len() > MAX_ONE {
                return Err(ImageError::OneTooLarge);
            }
            total = total.saturating_add(image.bytes().len());
            if total > MAX_ALL {
                return Err(ImageError::AllTooLarge);
            }
            if !magic_matches(image.mime(), image.bytes()) {
                return Err(ImageError::WrongMagic);
            }
        }
        Ok(Self(images))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ImageInput] {
        &self.0
    }
}

fn magic_matches(mime: ImageMime, bytes: &[u8]) -> bool {
    match mime {
        ImageMime::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ImageMime::Jpeg => bytes.starts_with(b"\xff\xd8\xff"),
        ImageMime::Gif => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        ImageMime::Webp => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
    }
}

/// Nazwane odmowy wspolnej walidacji obrazow.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("Attach no more than 4 images at once.")]
    TooMany,
    #[error("Each image must be no larger than 5 MiB.")]
    OneTooLarge,
    #[error("The attached images must be no larger than 12 MiB together.")]
    AllTooLarge,
    #[error("Attach a PNG, JPEG, GIF or WebP image.")]
    Unsupported,
    #[error("The image contents do not match their file type.")]
    WrongMagic,
}

/// Czego ten JEDEN krok potrzebuje w swoim pliku ustawień — opis, nie dokument.
///
/// 2026-08-23 (T-92). Trzy pola, bo tyle wystarcza, żeby ten typ nie znał ani jednego vendora
/// (nagłówek modułu): mówi, gdzie plik ma powstać, dokąd ma iść auto-pamięć tego kroku i czego
/// gospodarz zabronił. Nazwy kluczy, liczba kluczy i nazwa flagi zostają w adapterze
/// (niezmiennik 23) — inaczej `permissions.deny` i `autoMemoryDirectory` stałyby wypisane
/// w dwóch plikach, a rozjazd między nimi widać dopiero na rachunku za bieg.
///
/// # Dlaczego auto-pamięć w ogóle tu jest
///
/// Zmierzone 2026-08-23 w `system/init` każdego kroku Claude'a: `memory_paths.auto` wskazuje
/// `~/.claude/projects/<projekt>/memory/` — czyli katalog, który człowiek **dzieli ze swoimi
/// sesjami interaktywnymi**. Krok Loadouta pisze tam bez pytania i bez śladu w biegu: nikt tego
/// nie widzi, nikt tego nie kuruje, a zdanie napisane przez agenta w cudzym biegu wraca potem
/// do promptu człowieka jako jego własna notatka. [T6 §10.4] nazywa przekierowanie tego katalogu
/// per bieg „najlepszym leverem znalezionym w researchu" i ma na myśli dokładnie te dwa pola.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepSettings {
    /// Katalog, w którym plik ma powstać — katalog **biegu** (`docs/ARCHITECTURE.md` §8).
    ///
    /// Podaje go warstwa, która zna układ katalogów; sterownik miejsca sobie nie wybiera, bo
    /// wymyślone miejsce jest `$TMPDIR`, czyli artefaktem biegu poza biegiem.
    pub dir: PathBuf,
    /// Dokąd ma iść auto-pamięć tego kroku: `<katalog biegu>/mem/<krok>`.
    ///
    /// Per KROK, nie per bieg: dwa kroki jednego biegu bywają dwoma różnymi agentami, a wtedy
    /// jeden katalog na oba daje notatkę, o której nie wiadomo, czyja jest.
    pub memory: PathBuf,
    /// Reguły `deny` przepisane z repo gospodarza (`super::host::deny_rules`), w jego kolejności.
    ///
    /// Jadą tym samym plikiem, bo plik jest jeden: `--settings` wskazuje jeden dokument, więc
    /// drugi nośnik odmów po prostu nie istnieje.
    pub deny: Vec<String>,
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

    /// Uruchamia pierwsza ture z natywnymi obrazami, po wspolnej walidacji.
    ///
    /// Pusta lista zachowuje dotychczasowa droge tekstowa. Niepusta lista jest nazwana odmowa
    /// dla adaptera bez natywnego transportu, nigdy panika w silniku (AGENTS.md §2a).
    async fn start_with_images(
        &self,
        spec: RunSpec,
        images: ValidatedImages,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        if images.is_empty() {
            return self.start(spec, tx).await;
        }
        Err(anyhow::anyhow!(
            "this agent app does not accept images in a conversation"
        ))
    }

    /// Uruchamia interaktywna rozmowe Lead, ktora moze miec inny transport niz krok grafu.
    ///
    /// Codex jest pierwszym przypadkiem: workflow tekstowy zachowuje `codex exec`, ale Lead
    /// musi uzyc jednego `app-server` po stdio, zeby obrazy nie trafily ani do pliku, ani do
    /// trwalej sesji vendora. Domyslne cialo zachowuje dotychczasowe duble i Claude'a; adapter
    /// Codeksa nadpisuje caly czas zycia rozmowy.
    async fn start_conversation(
        &self,
        spec: RunSpec,
        images: ValidatedImages,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        self.start_with_images(spec, images, tx).await
    }

    /// Ten sam sterownik, tylko niosący **gotowy** fragment argv przyniesiony przez warstwę
    /// wyżej — albo `None`, kiedy ten vendor takiego szwu nie ma.
    ///
    /// # Po co to istnieje na TRAICIE, a nie na typie
    ///
    /// Fragment powstaje w `inherit::wire` (katalog pluginu z umiejętnościami gospodarza), a
    /// bieg trzyma sterownik jako `Arc<dyn AgentDriver>`: fabryka z `lib.rs` wydaje go raz,
    /// więc w `commands::run` konkretny typ jest już zgubiony. Budowniczy żyjący wyłącznie na
    /// [`claude::ClaudeDriver`] jest przez to nieosiągalny z biegu — to jest ta sama dziura,
    /// którą T-53 opisało przy `ClaudeDriver::with_settings` i której nie miało jak zamknąć.
    ///
    /// `Option`, a nie ciche „przyjąłem", i to jest cała treść tego typu zwrotnego. Fragment
    /// niesie nazwę flagi konkretnego vendora, więc vendor, który jej nie zna, **nie może** jej
    /// dostać — a wołający, który dostanie `None` przy niepustym fragmencie, ma o tym powiedzieć
    /// głośno. Sterownik, który po cichu ignoruje przyniesiony fragment, daje bieg, w którym
    /// człowiek zaznaczył umiejętności, agent nie dostał żadnej i nic tego nie mówi: „agent nie
    /// zna umiejętności" jest z zewnątrz nieodróżnialne od „model nie uznał, że warto jej użyć".
    ///
    /// Domyślnie `None`, żeby ten trait dalej dał się zaimplementować bez wiedzy o dziedziczeniu
    /// (niezmiennik 23): `CodexDriver` i atrapy testów nie zmieniają ani jednej linii, a to jest
    /// warunek, pod którym ten plik zostaje „jedynym, którego T-10 nie musi zmienić".
    fn inheriting(&self, _flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Ten sam sterownik z prywatnym targetem dowodow tej logicznej sesji.
    ///
    /// `Option` uniemozliwia produkcji ciche uruchomienie vendora bez dowodow. Implementacje
    /// wejda w Phase 2; domyslne `None` utrzymuje istniejace duble kompilowalne w honest-red.
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Ten sam sterownik, tylko z **własnym plikiem ustawień** tego kroku — albo `None`, kiedy
    /// ten vendor takiego pliku nie ma.
    ///
    /// # Po co to istnieje na TRAICIE, a nie na typie (2026-08-23, T-92)
    ///
    /// `claude::ClaudeDriver::with_settings` istnieje od T-53 i **nigdy nie miało wołającego**:
    /// budowniczy żyje na konkretnym typie, a bieg trzyma `Arc<dyn AgentDriver>`, bo fabryka
    /// z `lib.rs` wydaje sterownik raz na aplikację. Komentarz przy tamtym budowniczym opisuje
    /// tę dziurę i mówi wprost, że jej zamknięcie wymaga „albo fabryki wołanej per bieg, albo
    /// tej samej odpowiedzi, której T-34 nie dostało dla transkryptu". Ta odpowiedź jest tutaj
    /// i jest dokładnie tą samą, którą dostały [`AgentDriver::inheriting`]
    /// i [`AgentDriver::with_evidence`]: metoda na traicie z domyślnym `None`.
    ///
    /// **Argument opisuje potrzebę, nie plik.** [`StepSettings`] nie wie, jak nazywa się flaga
    /// vendora, ile kluczy ma dokument ani gdzie w nim stoją — wie tylko, gdzie ten krok pracuje
    /// i czego mu nie wolno. Gdyby wjechał tu gotowy `claude::RunSettings`, ten plik znałby
    /// vendora, a to jest jedyna rzecz, której nagłówek tego modułu zabrania.
    ///
    /// `Option`, a nie ciche „przyjąłem", z tego samego powodu co przy [`AgentDriver::inheriting`]:
    /// vendor, który nie umie wczytać naszego pliku, **nie może** dostać jego ścieżki, a wołający
    /// ma o tym wiedzieć. `None` znaczy „ten vendor nie ma gdzie tego przyjąć" — Codex zwraca
    /// właśnie to i nie dostaje nic.
    ///
    /// # Dlaczego `Option<Result<…>>`, a nie samo `Option` (2026-08-23, T-92, druga runda)
    ///
    /// Bo to są DWIE różne odpowiedzi i wołający robi po nich dwie różne rzeczy:
    ///
    /// - `None` — „nie mam gdzie tego przyjąć". Krok rusza bez pliku, bo ten vendor i tak by go
    ///   nie wczytał. Tak odpowiada Codex, tak odpowiada domyślna implementacja i tak odpowiada
    ///   każda atrapa, która o tym szwie nic nie wie.
    /// - `Some(Err(…))` — „biorę i **nie udało się**". Krok NIE rusza. Bez tego pliku pisze to,
    ///   czego się uczy, do katalogu, który człowiek dzieli ze swoimi sesjami [T6 §10.4],
    ///   i zabrania sobie mniej, niż ten projekt kazał (`host::deny_rules`) — czyli cicho traci
    ///   dokładnie to, po co ten szew powstał.
    ///
    /// **Pierwsza runda tego zadania spłaszczyła te dwie odpowiedzi do jednego `None`** i musiała
    /// je z powrotem rozdzielić po nazwie vendora: `None if driver.id() == "claude"` znaczyło
    /// „skoro to Claude, to `None` może być tylko awarią zapisu". Zmierzone: to zdanie odmawia
    /// startu każdemu dublerowi, który podaje się za `"claude"` i o tym szwie nie wie — a takich
    /// jest w drzewie trzy. Jeden z nich (`product_path_end_to_end`) sądzi całą drogę produktu
    /// i poszedł przez to na czerwono przy zielonych kryteriach. Rozróżnienie w TYPIE nie da się
    /// tak pomylić i nie kosztuje żadnej atrapy ani jednej linii.
    fn with_settings(
        &self,
        _settings: &StepSettings,
    ) -> Option<anyhow::Result<Arc<dyn AgentDriver>>> {
        None
    }

    /// Klon sterownika skonfigurowany dla zatwierdzonych Connections tego jednego kroku.
    fn configured(&self, _configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Ten sam sterownik z sufitem ceny należącym do tego klona.
    ///
    /// Domyślne `None` jest honest-red szkieletem T-126: wołający musi odmówić przed `start`,
    /// zamiast uruchomić płatną turę bez twardego limitu. Konkretna flaga pozostaje własnością
    /// adaptera (niezmiennik 23).
    fn with_budget(&self, _dollars: f64) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Ten sam sterownik, kiedy wolno mu wziąć turę **Loadouta** — albo `None`, kiedy ten vendor
    /// takiej tury nie bierze.
    ///
    /// Tura Loadouta to jedyna tura biegu, o którą nie prosi żaden kafelek grafu: po
    /// `close_the_book` pytamy raz, czego ten bieg nauczył, i z odpowiedzi zostają kandydatki
    /// do pamięci (`commands::run::what_this_run_taught_us`, T6 §5.3). Krok jej nie zlecił,
    /// człowiek jej nie narysował — więc nie ma jej po co przepuszczać przez tę samą drogę,
    /// którą jadą kroki.
    ///
    /// # Dlaczego to jest OSOBNY szew, a nie po prostu sterownik z fabryki (2026-08-23, T-92)
    ///
    /// **Zmierzone, nie przewidziane.** Pierwsza wersja tego mechanizmu brała sterownik prosto
    /// z fabryki `commands::Drivers` — tej samej, którą podstawia KAŻDY test integracyjny. Bieg
    /// dostawał wtedy jedno wywołanie sterownika więcej, niż zlecił graf, i **26 zielonych
    /// specyfikacji poszło na czerwono**: te, które liczą sesje („the driver closed 4 window(s)
    /// out of 2"), te, które enumerują prompty, i te, w których dubel trzyma jedno pole na `spec`,
    /// więc tura refleksji nadpisywała to, co zapisał krok. Żadna z nich nie była wadą produktu
    /// i żadnej nie wolno było poprawić: one pilnują, żeby bieg nie uruchomił więcej procesów,
    /// niż miał — czyli klasy błędu, która pali pieniądze.
    ///
    /// Domyślne `None` załatwia to strukturalnie, a nie umową: dubel, który tej metody nie
    /// implementuje, nie ma jak zobaczyć tury, o którą nie prosił. To jest dokładnie ta sama
    /// odpowiedź, co przy [`AgentDriver::inheriting`], [`AgentDriver::with_evidence`]
    /// i [`AgentDriver::with_settings`], i z tego samego powodu (niezmiennik 23).
    ///
    /// **Cena jest nazwana i pilnowana kryterium.** Szew z domyślnym `None`, którego produkcja
    /// nie podaje, to funkcja wyglądająca na gotową i niebiegnąca nigdy — czyli ten sam kształt
    /// awarii, który T-92 naprawia po stronie pamięci. Dlatego AC-1 dowodzi obu połów: że przy
    /// podanym szwie kandydatki powstają, i że `ClaudeDriver` ten szew podaje.
    fn reflecting(&self) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Jak TEN vendor nazywa w argv poziom wysiłku — pusto, kiedy takiej flagi nie zna.
    ///
    /// # Co tu jest polityką, a co adapterem (niezmiennik 23)
    ///
    /// Polityką jest sam POZIOM i mieszka w jednej tabeli przy szczeblu
    /// (`library::agents::effort_level`): cztery szczeble z formularza → `low | medium | high |
    /// xhigh`. Adapterem jest wyłącznie SPOSÓB podania, bo tylko on różni vendorów — Claude Code
    /// bierze `--effort <poziom>`, Codex `-c model_reasoning_effort=<poziom>` jako opcję
    /// GLOBALNĄ, czyli przed podkomendą.
    ///
    /// `&str`, nie wariant z `library/`: ten plik jest granicą, za którą nie ma ani jednego
    /// vendora i nie ma też definicji agenta. Enum biblioteki w tym podpisie odwróciłby strzałkę
    /// zależności dokładnie tak, jak opisuje to komentarz przy [`RunSpec::tools`].
    ///
    /// Pusto domyślnie, a nie `todo!()`: trait ma dalej dać się zaimplementować bez wiedzy
    /// o wysiłku, więc atrapy silnika i `absent` nie zmieniają ani jednej linii. Wołający czyta
    /// pustkę jako „ten vendor nie ma czym tego przyjąć" i wtedy nie dokłada niczego do argv —
    /// flaga z pustą wartością połknęłaby następny argument jako swój.
    fn effort_argv(&self, _level: &str) -> Vec<String> {
        Vec::new()
    }

    /// Czy TEN vendor przenosi [`RunSpec::extra_dirs`] do katalogu pracy kroku.
    ///
    /// Domyślne `true` zachowuje dotychczasowy transport Claude'a i atrap, które nie mają
    /// powodu z niego rezygnować. Adapter bez takiej zdolności musi odmówić jawnie.
    fn carries_extra_dirs(&self) -> bool {
        true
    }

    /// Czy TEN vendor w ogóle umie zawęzić agentowi listę narzędzi.
    ///
    /// # Po co to stoi na traicie (2026-08-24, T-97)
    ///
    /// Do tego dnia sufit listy narzędzi był **stałą jednego adaptera**: `commands::run`
    /// przepuszczało `Tools::Only([…])` każdego agenta przez `claude::tool_surface`, bo innego
    /// sufitu nie było. Dla Claude'a to jest poprawne i ma zostać — jego lista naprawdę wybiera
    /// spośród tego, co daje dial, i przekroczenie sufitu naprawdę jest odmową
    /// (`DECISIONS-LOCKED.md` D6). Dla Codeksa nie: `CAPABILITIES` mówi o tym polu
    /// `Unavailable`, adapter listy nie czyta ani razu — a mimo to potrafiła ona **zabrać cały
    /// bieg**, o ustawienie, które dla tego vendora nie robi nic.
    ///
    /// To jest niezmiennik 23 w jednym zdaniu: polityka („lista wybiera spośród diala, nigdy
    /// ponad") zostaje w rdzeniu, a adapter odpowiada wyłącznie na pytanie **o siebie**. Druga
    /// tabela nazw narzędzi per vendor jest tym, czego ten niezmiennik zabrania.
    ///
    /// `true` domyślnie, i to jest wybór w stronę odmowy: vendor, o którym nic nie wiadomo,
    /// jest sądzony jak dziś. Domyślne `false` znaczyłoby, że każda atrapa i każdy adapter
    /// dopisany w przyszłości po cichu przepuszcza listę ponad dialem bezpieczeństwa — czyli
    /// że pole `tools` staje się drugą drogą do uprawnień w chwili, w której ktoś zapomni
    /// nadpisać jedną metodę.
    fn narrows_its_tools(&self) -> bool {
        true
    }
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
#[derive(Clone)]
pub enum ToAgent {
    /// Kolejna tura: to, co napisał człowiek albo bieg.
    Turn(String),
    /// Przerwanie w paśmie, z identyfikatorem prośby.
    Interrupt(String),
}

impl std::fmt::Debug for ToAgent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Turn(text) => formatter
                .debug_struct("Turn")
                .field("text", &format_args!("<private; {} bytes>", text.len()))
                .finish(),
            Self::Interrupt(_) => formatter.write_str("Interrupt(<private request id>)"),
        }
    }
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

    /// Kolejna tura z natywnymi obrazami, przez ten sam logiczny watek.
    async fn send_with_images(
        &mut self,
        text: String,
        images: ValidatedImages,
    ) -> anyhow::Result<()> {
        if images.is_empty() {
            return self.send(text).await;
        }
        Err(anyhow::anyhow!(
            "this agent app does not accept images in a follow-up turn"
        ))
    }

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
