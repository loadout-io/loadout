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
//! [`RunSpec::resume`] jest jawnym transportem adaptera dla istniejącej sesji. Recovery tego
//! transportu nie konstruuje: wyłącznie sprząta procesy i oznacza faktycznie przerwane kroki.
//!
//! # Stan tego pliku: KOMPLETNY (2026-08-15)
//!
//! Typy są tu w całości, bo to one są kontraktem, o który opierają się kryteria — a ten plik
//! ma być jedynym, który `CodexDriver` z T-10 przeczyta i **nie będzie musiał zmienić**.
//! Jedyna dziura w implementacji siedzi w `claude.rs`, w kolejnej turze tej samej sesji,
//! i jest opisana tam, przy [`AgentDriver::start`].

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::line::Tool;
use super::supervisor::{self, GroupId, GroupProof};
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
/// Stawki modeli: tabela wbudowana i ta dopisana ręką w `~/.loadout/prices.json`.
///
/// Adres w `drivers/`, choć wczytuje ją bieg: to jest wiedza o vendorach, a nie o biegu, i to
/// sterownik jest jedynym, kto potrafi powiedzieć, czy da się z niej wycenić jego turę.
pub mod prices;
mod protected_state;

pub mod models;
/// Jeden rdzeń taniej sondy `--version` dla obu vendorów (niezmiennik 23). Prywatny, bo pytają
/// o wersję wyłącznie sterowniki, a granicę IPC obsługuje `commands::agent_apps`.
mod probe;

const SIGN_IN_AGAIN: &str =
    "The agent app is not signed in. Sign in to it outside Loadout, then try again.";
const CHECK_INSTALLATION: &str =
    "Loadout could not start the agent app. Check that it is installed, then try again.";
const AUTHENTICATION_SIGNS: [&str; 6] = [
    "failed to authenticate",
    "oauth session expired",
    "not logged in",
    "please run /login",
    "access token has been revoked",
    "401",
];
const MISSING_BINARY_SIGNS: [&str; 2] = ["command not found", "cannot execute binary file"];

/// Tłumaczy znaną, wykonalną przyczynę z całej zachowanej skargi na zdanie dla człowieka.
///
/// 2026-09-09 — `shell-init: ... getcwd ...` przyszło przed informacją o wygasłej sesji
/// Claude Code i pierwsza linia wysłała właściciela w stronę katalogów. Pierwsza linia nadal
/// wygrywa dla nieznanej skargi; wyjątkiem jest tylko powód, dla którego znamy następny ruch.
pub(crate) fn what_the_complaint_means(complaint: &str) -> Option<&'static str> {
    complaint.lines().find_map(what_one_line_means)
}

fn what_one_line_means(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    if AUTHENTICATION_SIGNS
        .iter()
        .any(|sign| line_contains_sign(&lower, sign))
    {
        return Some(SIGN_IN_AGAIN);
    }
    MISSING_BINARY_SIGNS
        .iter()
        .any(|sign| lower.contains(sign))
        .then_some(CHECK_INSTALLATION)
}

fn line_contains_sign(line: &str, sign: &str) -> bool {
    if sign != "401" {
        return line.contains(sign);
    }
    line.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word == sign)
}

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
    /// Jawny adapterowy start istniejącej sesji. `None` wybiera pierwszą turę kroku.
    ///
    /// 2026-09 (Z-33) — to pole nie ma dziś producenta: recovery wyłącznie sprząta i oznacza
    /// przerwane kroki, a wznowienie historii buduje nowy bieg. Gdyby zaczęło je wypełniać bez
    /// osobnego transportu katalogu stanu i starego worktree, vendor szukałby identyfikatora
    /// rozmowy w świeżych katalogach i kończył `No conversation found`.
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
    /// Co aplikacja agenta dobrała sobie z folderu, w którym ją postawiono.
    ///
    /// 2026-09 (Z-16) — OSOBNY WARIANT, NIE POLA W [`AgentEvent::Started`], i powód jest
    /// mierzalny: tamten wariant konstruuje 96 miejsc w 95 plikach, a to jest fakt, którego
    /// żaden dubler nie ma i mieć nie musi. Wariant obok kosztuje dwa ramiona i ani jednej
    /// fikstury.
    ///
    /// **To NIE JEST to samo pytanie, co [`AgentEvent::Started::tools`].** Tamto pole mierzy
    /// narzędzia; to zdarzenie mierzy TEKST i pozostałą powierzchnię folderu — umiejętności,
    /// polecenia z ukośnikiem, pluginy, podagentów, katalog pamięci.
    ///
    /// **Powodem jest to, że odpowiedź na to pytanie zmieniła się po cichu.** Na `claude` 2.1.251
    /// `CLAUDE.md` gospodarza docierał do kroku mimo `--setting-sources ""` i sześć kroków biegu
    /// `20260823-145648` zapisało przez to pliki wyników wbrew temu, co kazał im Loadout. Na
    /// 2.1.260 (zmierzone 2026-09-04) już nie dociera. Changelog o tej zmianie milczał, więc
    /// jedynym zapisem, po którym da się zauważyć następną, jest to, co CLI **samo o sobie
    /// ogłosiło** w `system/init` — i to niesie ten wariant, dosłownie, bez wniosków.
    LoadedFromTheFolder(LoadedFromTheFolder),
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
    /// Czynność wciąż trwa i vendor mówi, ile już.
    ///
    /// 2026-09 (Z-36) — OSOBNY WARIANT, i powstał z siedmiu minut ciszy. Lider siedział tyle
    /// w jednym wywołaniu Basha, CLI słało co 30 s
    /// `{"type":"tool_progress","elapsed_time_seconds":N,"heartbeat":true}`, a ekran nie pokazał
    /// NIC: wiersz o komendzie powstawał dopiero z pary `tool_use`+`tool_result`. Właściciel
    /// opisał to jako „lider się zawiesza i nie odpisuje".
    ///
    /// **Oba pola są `Option`, bo zmierzony heartbeat ma dokładnie trzy klucze i `id` nie jest
    /// jednym z nich** (niezmiennik 5). Wywołanie, którego on nie nazywa, rozstrzyga kurator —
    /// tam, gdzie stoi lista komend czekających na swój wynik.
    ToolProgress {
        /// Wywołanie, o którym mowa — kiedy vendor je nazwał.
        id: Option<String>,
        /// Ile ta czynność trwa, w sekundach, wedle vendora.
        elapsed_seconds: Option<u64>,
    },
    /// Czynność została puszczona w tło i nikt na nią nie czeka.
    ///
    /// 2026-09 (Z-36) — `system/task_started` z `is_backgrounded: true`. Bez tego wariantu
    /// komenda puszczona w tło zostawałaby na ekranie jako praca w toku i tykała do końca
    /// biegu, choć nikt na nią nie czeka.
    ToolBackgrounded {
        /// Wywołanie, które poszło w tło — kiedy vendor je nazwał.
        id: Option<String>,
        /// Opis, który model napisał sobie sam. Zapasowy podmiot wiersza: kiedy zapowiedzi tej
        /// czynności nie widzieliśmy, jest to jedyne, czym da się ją nazwać uczciwie.
        description: Option<String>,
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
    /// Ile ta tura kosztuje **do tej chwili**, wedle tabeli cen.
    ///
    /// 2026-09 (Z-13b) — OSOBNY WARIANT, NIE POLE W [`AgentEvent::Finished`], i to jest cała
    /// jego treść: liczniki docierały do biegu wyłącznie na końcu tury, czyli wtedy, gdy jest
    /// ona już opłacona. Codex nie ma po stronie CLI flagi sufitu (Claude ma
    /// `--max-budget-usd`), więc to jedyna liczba, którą da się zatrzymać jego turę od środka.
    ///
    /// **Nigdy nie zostaje wierszem** (`engine::line`): kwota nie jest historią, a zdanie dla
    /// człowieka powstaje dopiero przy przerwaniu.
    Spending {
        /// Szacunek dla tokenów zużytych od początku TEJ tury — nie rachunek vendora.
        estimate_usd: f64,
    },
    /// Koniec tury. Dokładnie **jedno** takie zdarzenie na turę.
    Finished(Outcome),
}

/// Ładunek [`AgentEvent::LoadedFromTheFolder`]: skąd, i co stamtąd weszło.
///
/// Wszystkie listy są **nazwami**, nigdy ścieżkami: drut podaje pluginy i serwery narzędzi jako
/// obiekty ze ścieżką w katalogu domowym człowieka, a `memory_paths` jako obiekt, którego
/// wartością jest taka ścieżka. Nazwa mówi tyle samo o tym, co weszło do kroku, i przeżywa
/// zapis do pliku, który zostaje po biegu — ta sama reguła, którą `evidence::validate_manifest`
/// stawia przed manifestem wejścia (2026-09, Z-16).
#[derive(Debug, Clone, Default)]
pub struct LoadedFromTheFolder {
    /// Katalog, który CLI podało jako swój. **Pełna ścieżka**, bo wołający pyta jeszcze dysk
    /// o to, co w niej leży; do pliku biegu idzie z niej sama nazwa katalogu.
    pub folder: PathBuf,
    /// Katalogi pluginów, po nazwie.
    pub plugins: Vec<String>,
    /// Polecenia z ukośnikiem, które ta sesja zna.
    pub slash_commands: Vec<String>,
    /// Umiejętności, które ta sesja zna.
    pub skills: Vec<String>,
    /// Serwery narzędzi, po nazwie.
    pub mcp_servers: Vec<String>,
    /// Rodzaje pamięci, po kluczu (`auto`) — nigdy po ścieżce, którą ten klucz wskazuje.
    pub memory_paths: Vec<String>,
    /// Podagenci, których ta sesja może zawołać.
    pub agents: Vec<String>,
}

impl LoadedFromTheFolder {
    /// Czy ta linia powiedziała cokolwiek o folderze.
    ///
    /// Zdarzenie bez ani jednego faktu jest ciszą, a nie odpowiedzią — i wypuszczone mimo to
    /// dokładałoby klucz „nic nie wczytano" do każdego kroku każdego biegu w historii.
    #[must_use]
    pub fn says_anything(&self) -> bool {
        !self.folder.as_os_str().is_empty()
            || [
                &self.plugins,
                &self.slash_commands,
                &self.skills,
                &self.mcp_servers,
                &self.memory_paths,
                &self.agents,
            ]
            .iter()
            .any(|list| !list.is_empty())
    }
}

const UNKNOWN_PRICE_OPENS: &str = "The price for ";
const UNKNOWN_PRICE_CLOSES: &str = " is not known.";

/// Jak nazywa się model, kiedy krok nie powiedział, czym jedzie.
///
/// Stała, a nie literał w jednym miejscu (2026-09, Z-44): tym samym zwrotem posługuje się odmowa
/// startu pod sufitem (`commands::run`), a dwa zdania o tym samym braku, napisane osobno,
/// rozjeżdżają się przy pierwszej zmianie brzmienia jednego z nich (niezmiennik 13).
pub(crate) const THE_MODEL_WITH_NO_NAME: &str = "the model this step used";

pub(crate) fn unknown_price_notice(model: Option<&str>) -> String {
    let model = model.unwrap_or(THE_MODEL_WITH_NO_NAME);
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
    /// Ile wewnętrznych tur zgłosił vendor. Zero jest zgodnościowym zapisem braku dla
    /// adaptera, którego protokół tej liczby nie podaje; na trwałą granicę wychodzi przez
    /// [`Outcome::vendor_turns`] jako `None` (2026-09, Z-48).
    pub turns: u32,
    /// Ile to trwało, według vendora.
    pub took: Duration,
    /// Sesja, w której to się zdarzyło.
    pub session: SessionRef,
}

impl Outcome {
    /// Wewnętrzne tury vendora, tylko kiedy jego protokół naprawdę je zgłasza.
    #[must_use]
    pub fn vendor_turns(&self) -> Option<u32> {
        // 2026-09 (Z-48) — adapter bez takiego licznika zapisuje zgodnościowe zero. Rdzeń
        // nie rozpoznaje vendora po nazwie i nie powiela jego polityki (niezmiennik 23).
        (self.turns > 0).then_some(self.turns)
    }
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

/// Zużycie kontekstu w jednej turze, w jednym słowniku dla obu vendorów (2026-09, Z-48).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Świeże wejście, bez tego przeczytanego z cache'u.
    pub uncached_input: u64,
    /// Wejście przeczytane z cache'u. To ta liczba pokazuje, czy izolacja kontekstu działa:
    /// bieg bez niej płacił 36 870 zamiast 4 725 [T1 §3.3, korekta 4].
    pub cache_read: u64,
    /// Wejście zapisane do cache'u, kiedy vendor je rozróżnia.
    pub cache_write: u64,
    /// Wyjście modelu.
    pub output: u64,
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

/// Rozpoznawalna kategoria odmowy przygotowania sterownika, niezależna od vendora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DriverSetupFailure {
    /// Proces nie ma bezpiecznego, prywatnego miejsca na swój stan.
    PrivateState,
}

/// Zachowuje źródłowy błąd IO, a wołającemu pozwala wybrać jedno bezpieczne zdanie publiczne.
#[derive(Debug)]
pub(crate) struct DriverSetupError {
    failure: DriverSetupFailure,
    source: io::Error,
}

impl DriverSetupError {
    pub(crate) fn private_state(source: io::Error) -> Self {
        Self {
            failure: DriverSetupFailure::PrivateState,
            source,
        }
    }

    #[must_use]
    pub(crate) fn failure(&self) -> DriverSetupFailure {
        self.failure
    }
}

impl std::fmt::Display for DriverSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.failure {
            DriverSetupFailure::PrivateState => {
                formatter.write_str("the private process state directory could not be created")
            }
        }
    }
}

impl std::error::Error for DriverSetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Czego ten JEDEN krok potrzebuje w swoim pliku ustawień — opis, nie dokument.
///
/// 2026-08-26 (T-127). Cztery pola wystarczają, żeby ten typ nie znał ani jednego vendora
/// (nagłówek modułu): mówi, gdzie plik ma powstać, jak nazywa się fizyczna praca, dokąd ma iść
/// auto-pamięć tego kroku i czego gospodarz zabronił. Nazwy kluczy dokumentu i flag zostają
/// w adapterze
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
    /// Vendor-neutralny klucz fizycznej pracy w tym biegu.
    ///
    /// Szkielet T-127: adapter Claude'a zacznie używać go dopiero w fazie implementacji.
    pub work_key: String,
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

/// WF-13: plik wejściowy już zweryfikowanego, prywatnie zamrożonego bundle.
/// Ten opis nie rozszerza uprawnień i nie wykonuje dołączonych helperów.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSkill {
    pub name: String,
    pub path: PathBuf,
}

/// Przygotowanie stanu vendora nie przyznaje mu uprawnień. Host dopiero z tych potrzeb
/// i własnego scope składa `FilesystemFence`, przed startem pierwszego procesu modelu.
pub struct PreparedProtectedStep {
    pub driver: Arc<dyn AgentDriver>,
    pub writable_roots: Vec<PathBuf>,
    pub readable_roots: Vec<PathBuf>,
    pub readable_files: Vec<PathBuf>,
}

impl fmt::Debug for PreparedProtectedStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProtectedStep")
            .field("vendor", &self.driver.id())
            .field("writable_roots", &self.writable_roots.len())
            .field("readable_roots", &self.readable_roots.len())
            .field("readable_files", &self.readable_files.len())
            .finish()
    }
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

    /// Adaptery produkcyjne pobierają katalog z tej samej instalacji CLI co start.
    /// Brak implementacji jest jawny; duble bez CLI zachowują dotychczasowy kontrakt.
    async fn model_catalog(&self) -> anyhow::Result<Option<models::ModelCatalog>> {
        Ok(None)
    }

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

    /// Natywny transport konkretnego skilla w rozmowie, odrębny od CLI plugin argv.
    /// Pusta lista wyłącznie pyta o wsparcie; None wymaga jawnej informacji przed Start.
    fn with_conversation_skills(
        &self,
        _skills: &[ConversationSkill],
    ) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Natywna półka skilli w katalogu roboczym kroku. Rdzeń zapisuje ją wyłącznie we własnej
    /// kopii; ta capability nie zezwala na instalację w projekcie człowieka.
    fn reads_step_skills_from_its_folder(&self) -> bool {
        false
    }

    /// Ten sam sterownik z prywatnym targetem dowodow tej logicznej sesji.
    ///
    /// `Option` uniemozliwia produkcji ciche uruchomienie vendora bez dowodow. Implementacje
    /// wejda w Phase 2; domyslne `None` utrzymuje istniejace duble kompilowalne w honest-red.
    fn with_evidence(&self, _target: EvidenceTarget) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Ten sam sterownik, tylko wiedzący, **komu oddać grupę, której nie umiał dowieść**.
    ///
    /// # Po co to istnieje (2026-09, Z-4)
    ///
    /// Bo start vendora potrafi paść nad żywą grupą, a wtedy nikt na zewnątrz nie ma uchwytu:
    /// `start` oddaje `Err`, a nadzorowany proces ginie razem z ramką sterownika. `Drop` posyła mu
    /// wtedy dziewiątkę, ale **nie dowodzi `ESRCH`** — czyli zostaje grupa, o którą nikt nie może
    /// zapytać (niezmiennik 6). Ten szew jest jedyną drogą, którą ten właściciel wychodzi ze
    /// sterownika żywy.
    ///
    /// Metoda na TRAICIE z domyślnym `None`, dokładnie jak [`AgentDriver::with_evidence`]
    /// i [`AgentDriver::inheriting`], i z tego samego zmierzonego powodu: bieg trzyma sterownik
    /// jako `Arc<dyn AgentDriver>`, więc budowniczy żyjący na konkretnym typie jest z niego
    /// nieosiągalny. `None` znaczy „ten vendor nie zostawia po sobie właściciela, którego dałoby
    /// się oddać" — tak odpowiada domyślna implementacja i tak odpowiada każda atrapa, która
    /// o tym szwie nic nie wie, więc ani jeden dubel w tym drzewie nie zmienia się o linię
    /// (niezmiennik 23).
    ///
    /// Argument jest [`supervisor::KeepsLeftovers`], nie `&Processes`: rejestr mieszka
    /// w `commands/`, a ten plik nie ma prawa o nim wiedzieć (niezmiennik 1).
    fn leaving_leftovers_with(
        &self,
        _keeper: Arc<dyn supervisor::KeepsLeftovers>,
    ) -> Option<Arc<dyn AgentDriver>> {
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
    /// vendora, ile kluczy ma dokument ani gdzie w nim stoją — wie tylko, gdzie ten krok pracuje,
    /// jak nazywa się jego fizyczna praca i czego mu nie wolno. Gdyby wjechał tu gotowy
    /// `claude::RunSettings`, ten plik znałby
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

    /// Podgląd sprawdza możliwości bez tworzenia prywatnego stanu. `None` oznacza
    /// brak obsługi, nie zgodę; dostępność procesu pod granicą sądzi osobno supervisor.
    fn protected_readiness(&self) -> Option<anyhow::Result<()>> {
        None
    }

    fn prepare_protected_step(
        &self,
        _settings: &StepSettings,
    ) -> Option<anyhow::Result<PreparedProtectedStep>> {
        None
    }

    /// Klon sterownika skonfigurowany dla zatwierdzonych Connections tego jednego kroku.
    fn configured(&self, _configuration: &DriverConfiguration) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Host wybiera granicę raz; adapter wyłącznie przekazuje ją supervisorowi.
    fn with_filesystem_fence(
        &self,
        _fence: &supervisor::FilesystemFence,
    ) -> Option<Arc<dyn AgentDriver>> {
        None
    }

    /// Ten sam sterownik, tylko wiedzący, **czyj bieg i czyj krok** uruchamia.
    ///
    /// # Po co to istnieje (2026-09, Z-01d)
    ///
    /// Bo po awarii aplikacji z całego biegu zostają pliki i garść numerów grup, a numery
    /// procesów przewijają się na macOS w godzinach. Znacznik ([`supervisor::StepTag`]) jedzie do
    /// środowiska każdego procesu kroku i jest jedyną rzeczą, po której odzyskiwanie odróżni
    /// własną sierotę od cudzego, niewinnego procesu pod tym samym `pgid`.
    ///
    /// Metoda na TRAICIE z domyślnym `None`, dokładnie jak [`AgentDriver::with_evidence`]
    /// i z tego samego zmierzonego powodu: bieg trzyma sterownik jako `Arc<dyn AgentDriver>`,
    /// więc budowniczy żyjący na konkretnym typie jest z niego nieosiągalny. `None` znaczy „ten
    /// vendor nie startuje własnego procesu, więc nie ma czego znaczyć" — tak odpowiada każda
    /// atrapa, która o tym szwie nic nie wie, i ani jeden dubel w tym drzewie nie zmienia się
    /// o linię (niezmiennik 23).
    ///
    /// Argument jest **znacznikiem gotowym**, nie parą napisów: wartość, którą odzyskiwanie
    /// porównuje co do bajta, ma jeden konstruktor i jedno źródło (powód przy [`supervisor::TAG_RUN`]).
    fn for_step(&self, _tag: &supervisor::StepTag) -> Option<Arc<dyn AgentDriver>> {
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

    /// Czy TEN vendor umie powiedzieć, ile kosztowała tura tego modelu — **zanim** ją zamówi.
    ///
    /// # Po co to istnieje (2026-09, Z-44)
    ///
    /// Bo sufit wydatku biegu jest wart tyle, ile wart jest najsłabszy krok pod nim. Krok, którego
    /// tury nikt nie umie wycenić, nie dokłada do sumy ani centa (Z-13b) — więc bieg z sufitem
    /// 275 USD potrafił go przekroczyć, nie łamiąc ani jednej linii kodu. Wołający pyta o to
    /// PRZED startem, bo po turze jest już tylko rachunek.
    ///
    /// Domyślne `true`, i to jest wybór na korzyść vendora, który cenę tury oddaje sam: `claude`
    /// podaje ją z drutu i zna ją zawsze, tak samo każda atrapa silnika. Domyślne `false`
    /// zatrzymywałoby pod sufitem kroki, które nie mają z tym problemem — a to jest awaria
    /// głośniejsza niż ta, przed którą ten szew stoi.
    fn can_price_a_turn(&self, _model: Option<&str>, _prices: &prices::Prices) -> bool {
        true
    }

    /// Ten sam sterownik z tabelą stawek, którą ten bieg wczytał — albo `None`, kiedy ten vendor
    /// żadnej nie czyta.
    ///
    /// Metoda na TRAICIE z domyślnym `None`, dokładnie jak [`AgentDriver::with_evidence`]
    /// i [`AgentDriver::for_step`] obok, i z tego samego zmierzonego powodu: bieg trzyma sterownik
    /// jako `Arc<dyn AgentDriver>`, więc budowniczy żyjący na konkretnym typie jest z niego
    /// nieosiągalny. `None` znaczy „ten vendor cen nie liczy" — tak odpowiada każda atrapa, która
    /// o tym szwie nic nie wie, więc ani jeden dubel w tym drzewie nie zmienia się o linię
    /// (niezmiennik 23).
    fn priced_from(&self, _prices: &prices::Prices) -> Option<Arc<dyn AgentDriver>> {
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

/// Klamka, którą przerywa się TURĘ — z zewnątrz sesji, bez uchwytu i bez `&mut`.
///
/// # Po co to istnieje (2026-09, Z-40)
///
/// Bo przerwanie w paśmie miało w tym drzewie dokładnie jednego wołającego: [`AgentHandle::cancel`],
/// czyli czasownik, który KOŃCZY rozmowę. Człowiek patrzący na lidera siedzącego siódmą minutę
/// w jednym wywołaniu Basha mógł więc tylko czekać albo zamknąć rozmowę razem z jej kontekstem —
/// a to są dwa złe wyjścia z sytuacji, w której CLI umie stanąć i wrócić do rozmowy.
///
/// **Klon głosu, nigdy pożyczka uchwytu**, i to jest ten sam pomiar, z którego wziął się
/// [`Voice`]: uchwyt sesji należy do actora rozmowy, a actor stoi w tej chwili w `handle.wait()`.
/// Klamka wisząca na `&mut self` byłaby więc osiągalna dokładnie wtedy, kiedy nie ma czego
/// przerywać.
///
/// **Zdolność czytana W CHWILI PYTANIA, nie przy wydaniu klamki.** Lista z `system/init`
/// przychodzi po starcie sesji, więc klamka, która przepisałaby ją sobie przy powstaniu, znałaby
/// wyłącznie pustkę — i mówiłaby „ten agent tego nie umie" o agencie, który umie.
#[derive(Debug, Clone)]
pub struct TurnBreak {
    /// Ten sam kanał, którym jadą tury: jeden pisarz nad jednym `stdin` (powód przy [`ToAgent`]).
    voice: Voice,
    /// Zdolności, które CLI ogłosiło o sobie. Puste, dopóki `init` nie przyszedł.
    announced: Arc<OnceLock<Vec<String>>>,
    /// Nazwa zdolności, której ten vendor wymaga do przerwania w paśmie. Zna ją **wyłącznie
    /// adapter** (niezmiennik 23) — ten plik nie ma prawa znać ani jednej nazwy z drutu.
    capability: &'static str,
    /// Aplikacja agenta, którą ta sesja prowadzi. Niesie ją odmowa, bo zdanie dla człowieka ma
    /// nazwać TĘ aplikację: Codex nie ma dostać zdania o Claude.
    agent_app: &'static str,
}

impl TurnBreak {
    /// Klamka do żywej sesji. Buduje ją adapter, bo tylko on zna nazwę zdolności i swoją własną.
    #[must_use]
    pub fn new(
        voice: Voice,
        announced: Arc<OnceLock<Vec<String>>>,
        capability: &'static str,
        agent_app: &'static str,
    ) -> Self {
        Self {
            voice,
            announced,
            capability,
            agent_app,
        }
    }

    /// Prosi turę, żeby stanęła — **dokładnie jedną linią i dokładnie raz**.
    ///
    /// Powtórzone pytanie, kiedy odpowiedź jest już w drodze, jest nieodróżnialne od dwóch
    /// przerwań i tak samo wygląda w dzienniku CLI.
    pub async fn ask(&self) -> Interrupted {
        // BEZ OGŁOSZONEJ ZDOLNOŚCI NIE WYSYŁAMY NICZEGO. Ta sama linia posłana tam, gdzie CLI
        // o `control_request` nie słyszało, kosztuje pełne okno czekania na odpowiedź, której
        // nie będzie — a człowiek widzi wtedy pięć sekund ciszy zamiast zdania [T1 §4.1].
        if !self.announces_interrupt() {
            return Interrupted::NotAnnounced {
                agent_app: self.agent_app,
            };
        }
        if self
            .voice
            .send(ToAgent::Interrupt(format!(
                "req_{}",
                Uuid::now_v7().simple()
            )))
            .await
            .is_err()
        {
            // Kanał bez odbiornika znaczy, że pisarz tej sesji już zszedł — czyli że nie ma
            // czego przerywać. To jest odpowiedź, nie awaria.
            return Interrupted::NoLongerListening;
        }
        Interrupted::Sent
    }

    /// Czy to CLI samo powiedziało, że rozumie przerwanie w paśmie.
    ///
    /// Po **liście z `init`**, nigdy po numerze wersji [T1 §4.1]. `false`, dopóki `init` nie
    /// przyszedł: przerwanie przed startem sesji nie ma czego feature-detektować.
    fn announces_interrupt(&self) -> bool {
        self.announced
            .get()
            .is_some_and(|announced| announced.iter().any(|name| name == self.capability))
    }
}

/// Co się stało z prośbą o przerwanie tury [T1 §8.5].
///
/// Trzy warianty, bo człowiek czyta trzy różne zdania — a **odmowa nie ma prawa udawać, że coś
/// pojechało**: droga, w której „nie umiem" wygląda jak „wysłałem", zostawia go przed ekranem,
/// na którym nic się nie dzieje i nic tego nie tłumaczy (niezmiennik 29).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupted {
    /// Prośba pojechała w paśmie, tym samym potokiem, co tura.
    Sent,
    /// To CLI nie ogłosiło zdolności przerwania, więc **nie wysłaliśmy niczego**. Niesie nazwę
    /// aplikacji agenta, bo zdanie odmowy ma nazwać tę jedną, którą ta rozmowa prowadzi.
    NotAnnounced { agent_app: &'static str },
    /// Nie ma czego przerywać: rozmowy nie ma albo jej proces przestał czytać wejście.
    NoLongerListening,
}

/// Agent nie wyszedł sam po zamknięciu wejścia i wymagał eskalacji supervisora.
///
/// Osobny typ pozwala rdzeniowi odróżnić kontrolowane zejście przez dowód od awarii transportu:
/// pierwsze nie psuje prywatnych dowodów kroku, choć nadal odbiera mu prawo do sukcesu
/// (2026-09, niezmienniki 6 i 29).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DidNotLetGo;

impl std::fmt::Display for DidNotLetGo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the agent kept going after Loadout closed its input")
    }
}

impl std::error::Error for DidNotLetGo {}

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

    /// Klamka do przerwania TEJ tury — albo `None`, kiedy tej sesji nie da się przerwać.
    ///
    /// 2026-09 (Z-40) — metoda na TRAICIE z domyślnym `None`, dokładnie jak [`AgentHandle::voice`]
    /// obok i z tego samego powodu: rozmowa trzyma uchwyt jako `Box<dyn AgentHandle>`, więc
    /// klamka żyjąca na konkretnym typie jest z niej nieosiągalna. `None` znaczy „tej sesji nie
    /// ma jak poprosić, żeby stanęła" — tak odpowiada `absent`, tak odpowiada Codex i tak
    /// odpowiada każdy dubel, który o tym szwie nic nie wie, więc ani jeden z nich nie zmienia
    /// się o linię (niezmiennik 23).
    fn turn_break(&self) -> Option<TurnBreak> {
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
    ///
    /// Wołający, który tego nie robi, ma to POWIEDZIEĆ przez
    /// [`AgentHandle::turn_endings_are_read_from_the_stream`] — powód w całości stoi tam.
    async fn wait(&mut self) -> anyhow::Result<Outcome>;

    /// Właściciel tej sesji rozlicza zakończenia tur z KANAŁU ZDARZEŃ, więc [`AgentHandle::wait`]
    /// nie ma tu wołającego i druga kopia wyniku nie ma po co powstawać.
    ///
    /// # Incydent 2026-09-07 — dziewiąta tura rozmowy z liderem
    ///
    /// Sterownik Claude'a odkładał każdy [`AgentEvent::Finished`] w DWA miejsca: raz do kanału
    /// zdarzeń wołającego, raz do ograniczonej kolejki, którą opróżnia wyłącznie `wait()`.
    /// Rozmowa prowadzona [`Voice`] nie woła `wait()` ani razu — wyniki czyta jej pętla zdarzeń —
    /// więc po ośmiu turach kolejka była pełna, a dziewiąte `send().await` zatrzymało pętlę
    /// czytającą stdout PRZED wysłaniem dziewiątego zdarzenia. Z zewnątrz wyglądało to dokładnie
    /// jak agent, który przestał odpowiadać: pisarz stdin działał dalej, proces żył, kolejne
    /// polecenia człowieka nadal docierały do CLI, a na ekranie nie pojawiało się już nic.
    ///
    /// **Nieodebrany kanał nie ma prawa blokować odczytu stdoutu.** Sterownik, który tę drugą
    /// kopię odkłada, ma tu porzucić jej odbiornik: porzucony odbiornik kończy każdą wysyłkę
    /// natychmiast, także tę, która już czeka na miejsce — więc ta sama linia leczy zarówno
    /// sesję, która dopiero ruszyła, jak i tę, która już stanęła. Po niej `wait()` w tej sesji
    /// jest NAZWANYM błędem, nigdy wiecznie oczekującym future: cisza w tym miejscu byłaby tą
    /// samą wadą, tylko przesuniętą o jedno wywołanie.
    ///
    /// Domyślnie nic nie robi i to nie jest przeoczenie: adapter, który drugiej kopii nie
    /// odkłada, nie ma czego porzucić — a wtedy ani jeden dubel testowy nie zmienia się o linię
    /// (niezmiennik 23). Wołać wolno **raz**, zanim ruszy druga tura.
    fn turn_endings_are_read_from_the_stream(&mut self) {}

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

    /// Zamyka wejście sesji i czeka z sufitem, aż proces wyjdzie **sam**.
    ///
    /// To jest normalne zakończenie kroku, nie anulowanie: `claude` z otwartym stdinem czeka
    /// w nieskończoność, więc bez tego każdy skończony krok zostawiałby żywy proces
    /// [T1 §2, §4.6]. Zwraca kod wyjścia; `None`, kiedy vendor nie trzyma jednego procesu na
    /// sesję albo kiedy proces zginął od sygnału i kodu po prostu nie ma.
    ///
    /// Po przekroczeniu wspólnego sufitu sterownik eskaluje przez supervisor i zwraca
    /// [`DidNotLetGo`]. Wolne czytanie stdoutu potrafi opóźnić wyjście — dlatego sufit jest
    /// wielokrotnością okna łaski, a nie natychmiastowym anulowaniem [T1 „Worth adding"].
    async fn close(&mut self) -> anyhow::Result<Option<i32>>;

    /// **Dowód**, że po grupie tej sesji nie zostało nic — na ścieżce UDANEJ.
    ///
    /// 2026-08-28 — bliźniak [`AgentHandle::cancel`] dla kroku, który skończył pracę sam.
    /// Do tego dnia takiego czasownika nie było i ścieżka udana nie produkowała [`GroupProof`]
    /// ani razu: [`AgentHandle::close`] zamyka wejście i zbiera **lidera**, a wnuka nie widzi
    /// żaden nasz `wait()` — bo wnuk nie jest naszym dzieckiem [T7 §3.1]. Krok meldował się więc
    /// człowiekowi jako zrobiony nad grupą, o którą nikt nie zapytał jądra, czyli dokładnie tym
    /// `Ok(())`, przed którym stoi [`GroupProof`] (niezmiennik 6).
    ///
    /// **Nie `cancel()`**, i to nie jest kwestia gustu: tamta prowadzi przerwaniem w paśmie
    /// [T1 §8.5] i czeka na odpowiedź, której proces po `close()` już nie wyśle — udana tura
    /// płaciłaby całym oknem przerwania za pytanie zadane nieboszczykowi. Tutaj zostaje sama
    /// eskalacja z `engine::supervisor` i sam dowód.
    ///
    /// Domyślne `Dead { status: None }` mówi prawdę o sesji, która **nie ma procesu**: `absent`
    /// i dublery bez grupy nie mają czego zabijać ani czego dowodzić, a `Alive` posłałoby
    /// wołającego po grupę, której nie ma. Ten sam wybór stoi w `codex::CodexHandle::cancel`.
    /// Statusu nie ma, bo nie było czyjego odebrać.
    async fn proof_of_death(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }

    /// **Każda** grupa procesów, którą ta sesja uruchomiła — nie tylko grupa lidera.
    ///
    /// 2026-09 (Z-01d) — czyta to księga biegu i zapisuje jako `pgids` przy kroku, na każdej
    /// drodze zejścia. Bez tego z kroku zostawał w `run.json` sam `pgid` lidera, a wszystko, co
    /// krok odpalił we własnej grupie, nie miało po awarii aplikacji kogo poprosić o sprzątnięcie
    /// (niezmiennik 6). `group()` jest zdaniem o JEDNEJ grupie i takie zostaje: to jest adres
    /// sesji, a nie spis tego, co po niej biegnie.
    ///
    /// `&mut self`, bo odpowiedź wymaga świeżego przeglądu drzewa procesów i uchwyt go zapamiętuje
    /// ([`supervisor::Supervised::descendant_groups`]).
    ///
    /// Domyślnie pusto: sesja bez procesu — `absent`, dublery — nie ma czego wymienić, a pusta
    /// lista znaczy dokładnie to, co mówi, i niczego w księdze nie kasuje (zapis jest addytywny).
    ///
    /// Synchronicznie, tak samo jak przegląd drzewa w `Supervised::stop`: `ps` idzie przez
    /// `std::process::Command`, więc nie ma tu na co czekać.
    fn descendant_groups(&mut self) -> Vec<i32> {
        Vec::new()
    }
}

/// Uchwyt sesji agenta widziany wyłącznie jako **to, co po niej może zostać**.
///
/// 2026-09 (Z-4) — rejestr ocalałych przestał tego dnia znać `AgentHandle` i zna zamiast tego
/// [`supervisor::Leftover`], czyli jeden czasownik zamiast dziewięciu. Sesja agenta wchodzi tam
/// tędy, a `Checking` i `Supervised` wchodzą własnymi `impl`-ami — bez tego adaptera rejestr
/// musiałby trzymać trzy różne pola albo enum z ramieniem na każdy rodzaj procesu.
///
/// `proof_of_death`, nie `cancel`: sesja, która tu trafia, pracę już oddała, a `cancel` prowadzi
/// przerwaniem w paśmie i czeka na odpowiedź, której ten proces już nie wyśle (powód w całości
/// przy [`AgentHandle::proof_of_death`]).
pub struct SessionLeftover(Box<dyn AgentHandle>);

impl SessionLeftover {
    /// Przejmuje jedynego właściciela sesji.
    #[must_use]
    pub fn new(handle: Box<dyn AgentHandle>) -> Self {
        Self(handle)
    }
}

impl fmt::Debug for SessionLeftover {
    /// Ręcznie, bo uchwytu sesji nie da się pokazać sensownie, a `missing_debug_implementations`
    /// jest w tej skrzyni ostrzeżeniem, czyli pod `-D warnings` odmową.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionLeftover")
            .field("group", &self.0.group())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl supervisor::Leftover for SessionLeftover {
    async fn ask_again(&mut self) -> GroupProof {
        self.0.proof_of_death().await
    }

    fn address(&self) -> Option<GroupId> {
        self.0.group()
    }
}
