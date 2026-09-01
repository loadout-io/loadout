//! „Czy to da się uruchomić?" — raport, nie boolean [T3 §5.2].
//!
//! Frontend odpowiada na inne pytanie („czy da się narysować tę strzałkę?") i robi to przy
//! rysowaniu, jednym boolem. Rust jest tu autorytetem, bo plik na dysku bywa zmergowany gitem,
//! poprawiony ręcznie albo napisany przez inny build — **bieg nigdy nie ufa UI**.
//!
//! Reguła, która nie umie zaświecić, jest gorsza niż jej brak: zajmuje miejsce reguły, która by
//! zaświeciła. T3 §5.2 zmierzył dokładnie to — napisał wykrywanie „nieosiągalnych kroków",
//! uruchomił je i **nigdy nie wystrzeliło**, bo w grafie acyklicznym obchód z każdego wierzchołka
//! o stopniu wejściowym zero dociera zawsze wszędzie. Zamiast tego sprawdzamy **spójność**,
//! obchodem **ignorującym kierunek strzałek** — ten strzela.
//!
//! 2026-08-16 — cykli nie liczymy tu drugi raz. `engine::dag::Dag::new` odmawia cyklu przy
//! konstrukcji, na listach sąsiedztwa i bez `petgraph` (ARCHITECTURE §10), i zwraca kroki, które
//! na nim leżą. `check()` mapuje id na numery i woła tamto; drugi obchód w tym pliku byłby
//! dokładnie tym duplikatem, przed którym ostrzega TASK.md.
//!
//! Listy sąsiedztwa powstają tu mimo to — do osiągalności (AC-4) i do spójności (AC-5). To nie
//! jest ten sam duplikat: `Dag` nie wystawia ani jednego, ani drugiego, a zbudowanie wektora
//! wektorów z gotowej listy strzałek to cztery wiersze, nie drugi algorytm.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use super::{Condition, ConditionalLink, Folder, Link, Step, WorkflowFile};
use crate::engine::dag::{Dag, DagError};

/// Flagi, które Loadout ustawia sam dla `claude` — przelotka nie ma prawa ich podać.
///
/// 2026-08-16 — **to jest druga kopia listy** i tak jej nie zostawiamy. ARCHITECTURE §6b mówi
/// „lista zarezerwowanych jest jedna, w jednym miejscu, obok budowniczego komendy", a budowniczy
/// to `engine::drivers::claude` (`TRANSPORT` + `LEAN_CONTEXT` + `--session-id`, dziś prywatne).
/// Ten plik nie ma tamtego w swoim bloku OWNS, więc scalenie list jest pytaniem do człowieka
/// (AGENTS.md §7), a nie cichym dopiskiem w cudzym pliku.
///
/// 2026-08-23 (T-90) — `--effort` dochodzi jako ósma pozycja. Od T-91 ustawia ją sam Loadout
/// z pola „ile myśleć" (`library::agents::effort_level` → `AgentDriver::effort_argv`), więc
/// przelotka podająca ją drugi raz znaczy dwie strony piszące jedną rzecz. Do tego zadania
/// kolizja nie miała skutku, bo przelotka nie dojeżdżała do argv w ogóle; z chwilą, w której
/// dojeżdża, brak tej pozycji jest cichą wygraną jednej ze stron — dokładnie tym, czego
/// zakazuje D6. Zgłosił to pisarz T-91 zamiast dopisać linię w cudzym pliku.
///
/// 2026-08-24 (T-98) — **ZDANIE WYŻEJ O „BRAKU SKUTKU" JEST JUŻ NIEAKTUALNE I DLATEGO TA LISTA
/// UROSŁA Z OŚMIU DO DWUDZIESTU TRZECH.** Od T-90 przelotka dojeżdża do argv naprawdę
/// (`library::agents::vendor_argv` → `DriverConfiguration::arguments`), a `ClaudeDriver::command`
/// składa dwa razy tyle nazw, ile ta lista miała. Każda brakująca była więc cichą wygraną jednej
/// ze stron, i za każdą stoi konkretna strata: `--settings <własny plik>` podmienia nośnik,
/// którym T-92 wnosi przepisane reguły `deny` gospodarza; `--tools` rozszerza twardą listę
/// dostępności; `--model` przestawia model spod ręki człowieka, który wybrał inny w formularzu;
/// `--mcp-config` i `--plugin-dir` wskazują cudzy plik zamiast tego, który Loadout napisał
/// w katalogu biegu; `--max-budget-usd` odpowiada drugi raz na pytanie „ile to może wydać".
///
/// `--continue`, `--agents`, `--disallowedTools` i `--permission-prompt-tool` Loadout ustawia
/// sam dopiero potencjalnie — stoją tu dlatego, że przelotka może ich użyć **już teraz**,
/// a każda z nich odpowiada na pytanie, na które w tym produkcie odpowiada formularz albo dial.
///
/// Dopasowanie idzie po KLUCZU, nie po podciągu (`is_reserved` niżej): `--verbose` jest nasze,
/// a `--verbose-tool-output` jest inną flagą tej samej aplikacji i ma przechodzić. Filtr
/// pytający `starts_with` zabijałby flagę ogłoszoną dziś rano, czyli dokładnie to, po co
/// przelotka istnieje (D6).
pub const RESERVED_CLAUDE: [&str; 23] = [
    // ── czym JEST to wywołanie: transport i sesja ────────────────────────────────────────
    "-p",
    "--output-format",
    "--input-format",
    "--verbose",
    "--session-id",
    "--resume",
    "--continue",
    // ── izolacja kontekstu i nośniki, które piszemy sami w katalogu biegu ────────────────
    "--strict-mcp-config",
    "--setting-sources",
    "--settings",
    "--plugin-dir",
    "--mcp-config",
    "--agents",
    "--add-dir",
    // ── dial: co agent może zrobić z plikami i co ma pod ręką ───────────────────────────
    "--permission-mode",
    "--permission-prompt-tool",
    "--allowedTools",
    "--disallowedTools",
    "--tools",
    // ── to, co człowiek wybiera w formularzu agenta ─────────────────────────────────────
    "--model",
    "--effort",
    "--append-system-prompt",
    // ── pieniądze: kwotę zna wyłącznie księga biegu ─────────────────────────────────────
    "--max-budget-usd",
];

/// To samo dla `codex`: `-C` (katalog roboczy), `-s` (piaskownica), `--json` (strumień zdarzeń)
/// i `model_reasoning_effort` — powód czwartej pozycji stoi przy [`RESERVED_CLAUDE`].
///
/// 2026-08-24 (T-98) — **SZEŚĆ POZYCJI DOCHODZI I DWIE Z NICH SĄ PREFIKSAMI RODZIN.** Ten vendor
/// przyjmuje przelotką **całą swoją konfigurację** (`-c klucz=wartość`,
/// `library::agents::vendor_argv`), więc „dodatkowe ustawienie" bywa u niego dialem ustawionym
/// z boku. Zmierzone na trunku tego dnia — wszystkie przechodziły: `sandbox_mode=workspace-write`
/// (podniesienie z „look only"; filtr podniesień zna tylko literał `danger-full-access`, a to
/// jest inna wartość tego samego ustawienia), `sandbox_workspace_write.network_access=true`
/// (sieć z pominięciem pola, które ją włącza), `approval_policy=never`,
/// `mcp_servers.x.command=/bin/sh` (dowolny proces jako „serwer narzędziowy", obok listy
/// zatwierdzonych Connections) oraz `model_provider` i `model_providers.custom.base_url=…`
/// — czyli cały ruch, razem z promptem, pod cudzy adres.
///
/// **Kropka na końcu pozycji znaczy „cała rodzina"** (`is_reserved` niżej). `mcp_servers.*`
/// i `model_providers.*` mają w środku nazwę, którą wpisuje CZŁOWIEK, więc lista równościowa
/// musiałaby znać ją z góry — czyli nie istnieje. Pozostałe pozycje zostają równościowe, bo
/// `model_verbosity` i `model_reasoning_summary` są zwykłymi ustawieniami tego vendora i mają
/// przechodzić; `model_reasoning_effort` zostaje osobno, bo prefiks `model_provider` go nie łapie.
pub const RESERVED_CODEX: [&str; 10] = [
    "-C",
    "-s",
    "--json",
    "model_reasoning_effort",
    "sandbox_mode",
    "sandbox_workspace_write.network_access",
    "approval_policy",
    "model_provider",
    // Rodziny, nie nazwy: środek klucza wpisuje człowiek.
    "mcp_servers.",
    "model_providers.",
];

/// Podniesienia, których przelotka nie przepuszcza — **ani w nazwie flagi, ani w jej wartości**.
///
/// Dial „co agent może zrobić z plikami" jest jedyną drogą do nich (ARCHITECTURE §6b
/// reguła 2, D6). Sama lista zarezerwowanych by nie wystarczyła: `--sandbox` nie jest na niej,
/// a `--sandbox danger-full-access` omija dial tak samo skutecznie jak `-s`.
///
/// Czytają ją **dwie** przelotki: krok workflow (`the_passthrough` niżej) i definicja agenta
/// (`library::agents::vendor_args_filtered`). To jest cała polityka i jest jedna
/// (niezmiennik 23) — wpis dopisany tutaj zamyka obie naraz, a wpis dopisany po jednej stronie
/// jest dokładnie tą dziurą, przed którą ten komentarz stoi.
///
/// 2026-08-17 — `--dangerously-skip-permissions` dopisane po przeglądzie zewnętrznym (T-36).
/// Obie dotychczasowe pozycje były **wartościami**, więc główna flaga eskalacyjna Claude
/// Code — ta, która jest podniesieniem w samej NAZWIE i stoi z pustą wartością — przechodziła
/// obie przelotki: wiersz `"--dangerously-skip-permissions": ""` w `~/.loadout/agents/*.json`
/// omijał dial całkowicie, a ten sam wiersz na kroku workflow zapisywał się bez uwagi.
/// Obie reguły czytają `flag` i `value`, więc pozycja w kształcie nazwy działa bez zmiany w kodzie.
///
/// 2026-08-24 (T-98) — `--max-budget-usd` dochodzi jako czwarta pozycja i jest tu, a nie tylko
/// na liście zarezerwowanych, z jednego powodu: [`reserved`] jest **per vendor** i dla aplikacji,
/// o której Loadout jeszcze nie słyszał, oddaje pustą listę z rozmysłu (D6). Sufit wydatku
/// podany przelotką pod cudzą nazwą vendora przechodziłby więc obok wszystkiego. Ta lista jest
/// czytana dla KAŻDEJ nazwy aplikacji, więc pozycja tutaj to jedna reguła zamiast jednej kopii
/// na vendora (niezmiennik 23). Dług opisany w `docs/STATUS.md` po T-94.
///
/// Dopasowanie po stronie NAZWY idzie po kluczu (`escalation_in` niżej), a po stronie WARTOŚCI
/// zostaje podciągiem — i to nie jest niekonsekwencja, tylko dwie różne rzeczy. Nazwa flagi jest
/// nazwą: `--max-budget-usd-warning` zaczyna się od pozycji z tej listy i **nią nie jest**,
/// a odmowa mówiąca człowiekowi, że jego wiersz podnosi dial, kiedy nie podnosi, jest gorsza
/// od braku odmowy. Wartość jest treścią: `--sandbox danger-full-access` omija dial tak samo
/// skutecznie jak `-s`, a `--sandbox` nie jest i nie będzie na żadnej liście zarezerwowanych.
pub const FORBIDDEN_ESCALATIONS: [&str; 4] = [
    "bypassPermissions",
    "--dangerously-skip-permissions",
    "danger-full-access",
    "--max-budget-usd",
];

/// Początki, po których poznaje się klucz wydany przez konkretnego dostawcę.
///
/// 2026-08-28 (T-157) — definicje workflowów i agentów są **zwykłymi plikami**: idą do gita, do
/// kopii i do wyników biegu. Sekret ma w tym produkcie jedną drogę — env dziecka (niezmiennik 9)
/// — a przelotka i pole „co uruchomić" są dwiema szparami, przez które literał może się do pliku
/// wcisnąć. Ta lista jest pierwszą z trzech reguł [`secret_shaped`] i jedyną, która wie
/// **cokolwiek** o dostawcach: pozostałe dwie pytają wyłącznie o kształt.
///
/// Dopasowanie jest WRAŻLIWE NA WIELKOŚĆ LITER i to jest treść, nie przeoczenie. `AKIA`, `ASIA`
/// i `AIza` mają po cztery znaki i po zignorowaniu wielkości liter zaczynałyby zwykłe angielskie
/// wyrazy (`aiza…`, `asia…`) — czyli fałszywą odmowę na wartości, która niczego nie niesie.
/// Prawdziwe klucze tych rodzin mają wielkie litery zawsze, bo dostawca je tak wydaje.
///
/// `sk-ant-` stoi PRZED `sk-`, choć wynik jest ten sam: kolejność czyta człowiek, nie kod, i para
/// „rodzina, potem rodzic" mówi wprost, że drugie nie jest literówką pierwszego.
const SECRET_PREFIXES: [&str; 15] = [
    "sk-ant-",
    "sk-",
    "ghp_",
    "gho_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "lin_api_",
    "AKIA",
    "ASIA",
    "AIza",
    "npm_",
    "hf_",
    "dop_v1_",
];

/// Nazwy parametrów adresu, które niosą sekret **w samej nazwie**.
///
/// 2026-08-28 (T-157) — porównanie idzie po nazwie sprowadzonej do małych liter z `-` zamienionym
/// na `_`, więc `api-key`, `Api_Key` i `APIKEY` pytają o tę samą pozycję. To jedyna reguła w tym
/// pliku, która patrzy na NAZWĘ, i wolno jej na to dlatego, że nazwa parametru adresu jest częścią
/// wartości wpisu, a nie polem, które wypełnia człowiek: adres z `?token=…` niesie sekret
/// niezależnie od tego, jak nazywa się flaga, w której stoi.
const SECRET_PARAMETERS: [&str; 11] = [
    "token",
    "key",
    "api_key",
    "apikey",
    "secret",
    "access_token",
    "password",
    "passwd",
    "pwd",
    "auth",
    "signature",
];

/// Ile znaków musi mieć ogon za znanym prefiksem, żeby uznać go za klucz.
///
/// 2026-08-28 (T-157) — próg jest tu, żeby `sk-` nie zabijało `sk-test`, `sk-1` i każdej innej
/// krótkiej wartości, która przypadkiem tak się zaczyna. Najkrótszy prawdziwy klucz z tych rodzin
/// (`hf_` u Hugging Face) ma 37 znaków, `AKIA…` dokładnie 20, więc szesnaście jest z zapasem pod
/// nimi i z zapasem nad wszystkim, co jest zwykłym ustawieniem.
const A_KNOWN_KEY: usize = 16;

/// Ile znaków musi mieć wartość parametru adresu, żeby uznać ją za sekret.
///
/// 2026-08-28 (T-157) — `?key=1` i `?auth=none` są zwykłymi parametrami i mają przechodzić;
/// klucz podpisujący nie ma dwunastu znaków nigdy.
const A_PARAMETER_VALUE: usize = 12;

/// Ile znaków musi mieć zbity ciąg, żeby jego sam kształt był powodem odmowy.
///
/// 2026-08-28 (T-157) — TO JEST PRÓG, KTÓRY MOŻE ZABLOKOWAĆ PRACĘ, więc jest zmierzony na
/// wartościach, które MAJĄ przechodzić, a nie wybrany na wyczucie. Fałszywa odmowa jest tu gorsza
/// niż brak sprawdzenia: człowiek nie ma jak jej obejść, bo nie może zmienić wartości, którą musi
/// podać. Dlatego trzy warunki naraz, i każdy z nich odsiewa konkretną wartość z trunku:
///
/// - **32 znaki** — `workspace-write` (15), `sandbox_mode` (12), `xhigh` (5) i każde inne
///   ustawienie vendora są krótsze o rząd wielkości;
/// - **trzy klasy znaków naraz** (mała litera, wielka litera, cyfra) — SHA gita (40 znaków) i UUID
///   (36) są długie i mają po DWIE klasy, więc nie wpadają. To one przewracają wersję pytającą
///   samą długością;
/// - **ciąg cięty na kropce i ukośniku** — `sandbox_workspace_write.network_access` ma 38 znaków
///   i rozpada się na 23 i 14, a każda ścieżka bezwzględna rozpada się na segmenty. Bez tego cięcia
///   `/Users/kto/Projects/loadout-h-p8-t157` (38 znaków, trzy klasy) byłby odmową, a jest zwykłą
///   ścieżką wpisaną w pole „gdzie".
///
/// Cena tego cięcia: sekret zakodowany base64, w którym akurat trafił się ukośnik, może rozpaść się
/// na dwa krótsze ciągi i tą regułą przejść. Zostaje wtedy reguła prefiksów i reguła adresu. To jest
/// wybór świadomy — heurystyka, która blokuje pracę, przestaje istnieć po pierwszym tygodniu.
const A_PACKED_RUN: usize = 32;

/// Zdanie z uruchomienia w T3 §5.2.
///
/// Mówi, co się stanie, a nie jak nazywa się algorytm, który to znalazł: `cycle detected in DAG`
/// jest zdaniem, z którym użytkownik nie może zrobić nic (niezmiennik 14).
const CIRCLE: &str = "These steps point back at each other in a circle. Work would never finish.";

/// Ile kopii jednego kroku naraz wolno zamówić [T3 §4.4]. Osiem jednoczesnych na prawdziwej
/// maszynie to już dużo.
const MOST_COPIES: u8 = 8;

/// Sufit rund pętli. Dziesięć rund dwóch agentów to już długa noc bez nadzoru i prawdziwy
/// rachunek — ta liczba jest tym samym rodzajem zapory, co [`MOST_COPIES`], i z tego samego
/// powodu stoi w schemacie, a nie w głowie użytkownika.
const MOST_TURNS: u8 = 10;

/// Klucz katalogu pracy jednej kopii kroku.
///
/// 2026-08-24 (T-114) — planista biegu i walidator kolizji muszą kodować kopię identycznie.
/// Jedna funkcja zapobiega sytuacji, w której Start rezerwuje inny ref niż ten sprawdzony tutaj.
#[must_use]
pub(crate) fn work_key_for(tile_key: &str, copy: u8) -> String {
    if copy == 0 {
        return tile_key.to_owned();
    }
    format!("{tile_key}~{}", copy + 1)
}

/// Ogon refa Gita dla klucza pracy.
///
/// `~` rozróżnia kopie w katalogu biegu, ale Git nie dopuszcza go w nazwie refa. Zamiana jest
/// jawna i wspólna dla walidatora oraz adaptera Gita, żeby oba odpowiadały na to samo pytanie.
#[must_use]
pub(crate) fn work_branch_tail(work_key: &str) -> String {
    work_key.replace('~', "-")
}

/// Waga uwagi. `Problem` blokuje Run i zapis, `Warning` nie blokuje niczego.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Problem,
    Warning,
}

/// Jedna uwaga o jednym defekcie.
///
/// `message` idzie **wprost na ekran** (T3 §5.3), więc jest gotowym angielskim zdaniem — bez
/// kodów, bez kluczy i18n i bez żargonu (niezmiennik 14). `cycle detected in DAG`, `orphan node`
/// i `in-degree` są tu zakazane tak samo, jak w komponencie Reacta.
///
/// `step_id` jest tym, na czym ląduje kropka na kafelku i co dostaje `fitView` po kliknięciu
/// uwagi — więc musi nazywać krok, **który istnieje**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub level: Level,
    pub step_id: Option<String>,
    pub message: String,
    /// Naprawa, którą Loadout umie wykonać sam — o ile jest jednoznaczna.
    ///
    /// 2026-08-22 — POLE JEST NOWE i niesie cały auto-fix. `None` znaczy „tę naprawę wybiera
    /// człowiek", i tak zostaje dla wszystkiego, co liczy `check` z samego pliku: kształt grafu
    /// naprawia się przeciągnięciem strzałki, a nie przyciskiem. Wypełnia je `workflow::roster`,
    /// który jako jedyny wie, co dokładnie trzeba przestawić.
    ///
    /// `skip_serializing_if`: uwaga bez naprawy ma jechać na drut w tym samym kształcie, co
    /// przed dołożeniem tego pola.
    /// `Box`, nie wartość wprost: `Note` jedzie w `RunError::Refused`, a `clippy::result_large_err`
    /// (deny w bramce) mierzy rozmiar wariantu błędu. Naprawa jest tu rzadkością — pole wskaźnika
    /// kosztuje osiem bajtów zawsze, treść tylko wtedy, gdy naprawa istnieje.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Box<super::roster::Fix>>,
}

/// Wszystko, co da się powiedzieć o pliku bez uruchamiania go.
///
/// Wołane przy **zapisie** (niezmiennik 12: odmowa pada tam, nie w trakcie biegu) i drugi raz
/// przy Run — to drugie dowodzi T-15.
#[must_use]
pub fn check(workflow: &WorkflowFile) -> Vec<Note> {
    notes(workflow, When::Saving)
}

/// To samo, ale sądzone tak, jak sądzi się plik, który ma **ruszyć** za sekundę.
///
/// JEDNA reguła zmienia wagę i to jest cała różnica: krok bez agenta. Przy zapisie jest
/// ostrzeżeniem, bo szkic w połowie zbudowany ma się **zapisać** — kafelek dodany przed
/// wybraniem agenta jest normalnym stanem pracy, a zapis, który go odrzuca, kasuje pracę
/// człowieka w chwili, gdy ten pracuje. Przy Run jest problemem, bo krok, który nie nazywa
/// agenta, nie ma czym ruszyć i lepiej powiedzieć to **przed** biegiem, zdaniem o agencie,
/// niż w trakcie, zdaniem systemu plików.
///
/// Dwa wejścia, nie argument: `check` ma trzech wołających (zapis, `check_workflow` dla okna,
/// bieg) i tylko jeden z nich sądzi bieg. Argument w sygnaturze zmuszałby dwóch pozostałych
/// do wybierania wartości, o którą nie pytają.
#[must_use]
pub fn check_to_run(workflow: &WorkflowFile) -> Vec<Note> {
    notes(workflow, When::Running)
}

/// Po co pytamy — jedyna rzecz, która zmienia wagę uwagi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    /// Zapis pliku: szkic w połowie zbudowany jest poprawnym plikiem.
    Saving,
    /// Naciśnięty Run: plik ma za sekundę uruchomić procesy.
    Running,
}

fn notes(workflow: &WorkflowFile, when: When) -> Vec<Note> {
    let steps: Vec<Facts<'_>> = workflow.steps.iter().map(facts).collect();

    // Pusty plik kończy sprawdzanie. Każda następna reguła mówiłaby o krokach, których nie ma,
    // a użytkownik ma tu dokładnie jedną rzecz do zrobienia i chce usłyszeć o niej raz.
    if steps.is_empty() {
        return vec![problem(None, "There are no steps yet.".to_owned())];
    }

    // Numer kroku to jego pozycja w pliku. Przy powtórzonym id wygrywa PIERWSZY — to samo
    // rozstrzygnięcie, o którym mówi uwaga o powtórzeniu, więc strzałka nie celuje raz w jeden
    // krok, raz w drugi, zależnie od reguły, która akurat pyta.
    let mut position: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, step) in steps.iter().enumerate() {
        position.entry(step.id).or_insert(index);
    }

    // Strzałki, których OBA końce istnieją. Strzałka w nieistniejący krok jest osobną uwagą
    // i nie ma prawa przewrócić ani obchodu, ani liczenia cyklu.
    let arrows: Vec<(usize, usize)> = workflow
        .links
        .iter()
        .filter_map(|link| {
            Some((
                *position.get(link.from.as_str())?,
                *position.get(link.to.as_str())?,
            ))
        })
        .collect();

    // Strzałki BEZ POWROTÓW — to na nich liczy się koło. Powrót (`max_turns`) domyka koło
    // z rozmysłu i jest całą treścią pętli; koło zamknięte czymkolwiek innym jest pomyłką,
    // najczęściej strzałką pociągniętą w złą stronę, i ma zostać odmową. Reguła w jednym
    // zdaniu: po usunięciu powrotów graf musi być bez cykli.
    let forward: Vec<(usize, usize)> = workflow
        .links
        .iter()
        .filter(|link| !link.is_a_way_back())
        .filter_map(|link| {
            Some((
                *position.get(link.from.as_str())?,
                *position.get(link.to.as_str())?,
            ))
        })
        .collect();

    // Kolejność reguł jest kolejnością, w jakiej użytkownik zobaczy uwagi, a `save()` odmawia
    // zdaniem PIERWSZEGO problemu — więc idzie od „ten plik nie trzyma się kupy" do „ten bieg
    // by nie wyszedł". Ostrzeżenia na końcu: nie blokują niczego.
    let mut notes = Vec::new();
    one_id_two_steps(&steps, &mut notes);
    arrows_into_nowhere(&workflow.links, &steps, &position, &mut notes);
    copies_out_of_range(&steps, &mut notes);
    colliding_work_branches(&steps, when, &mut notes);
    turns_out_of_range(&workflow.links, &steps, &position, &mut notes);
    loop_judges_run_once(&workflow.links, &steps, &position, &mut notes);
    loops_that_cross(&workflow.links, &steps, &position, &forward, &mut notes);
    a_step_without_an_agent(&steps, when, &mut notes);
    a_step_without_a_task(&steps, when, &mut notes);
    a_command_step_left_empty(&steps, &mut notes);
    a_command_carrying_a_secret(&steps, &mut notes);
    conditional_routes(workflow, &mut notes);
    the_passthrough(&steps, &mut notes);
    a_circle(&steps, &forward, &mut notes);
    // STRZAŁKI BEZ POWROTÓW, nie wszystkie, i to jest ta sama lista, po której liczy się koło.
    // Powrót wchodzi do kroku dopiero w rundzie drugiej (`workflow::unroll`), więc krok, do
    // którego prowadzi WYŁĄCZNIE powrót, w rundzie pierwszej dalej nie ma nikogo przed sobą —
    // a to jest dokładnie ta runda, która ruszy jako pierwsza.
    nothing_before_it(&steps, &forward, &mut notes);
    one_folder_two_steps(&steps, &arrows, &forward, when, &mut notes);
    islands(&steps, &arrows, &mut notes);
    notes
}

fn conditional_routes(workflow: &WorkflowFile, notes: &mut Vec<Note>) {
    let Some(value) = workflow.extra.get("linkConditions") else {
        return;
    };
    let Ok(routes) = serde_json::from_value::<Vec<ConditionalLink>>(value.clone()) else {
        notes.push(problem(
            None,
            "The saved route conditions cannot be read. Remove them or open this workflow in a newer Loadout."
                .to_owned(),
        ));
        return;
    };
    for route in &routes {
        let connected = workflow
            .links
            .iter()
            .any(|link| link.from == route.from && link.to == route.to && !link.is_a_way_back());
        if !connected {
            notes.push(problem(
                Some(&route.from),
                "A route condition points to a connection that is not in this workflow.".to_owned(),
            ));
            continue;
        }
        let source = workflow.steps.iter().find(|step| match step {
            Step::Agent(step) => step.id == route.from,
            Step::Checkpoint(step) => step.id == route.from,
            Step::Check(step) => step.id == route.from,
            Step::Serve(step) => step.id == route.from,
        });
        let compatible = matches!(
            (source, &route.when),
            (Some(Step::Check(_)), Condition::Check { .. })
                | (Some(Step::Checkpoint(_)), Condition::Checkpoint { .. })
                | (Some(Step::Agent(_)), Condition::Handoff { .. })
        );
        if !compatible {
            notes.push(problem(
                Some(&route.from),
                "This route asks for a result that its step cannot produce.".to_owned(),
            ));
        }
    }

    let sources: BTreeSet<&str> = routes.iter().map(|route| route.from.as_str()).collect();
    for source in sources {
        for link in workflow
            .links
            .iter()
            .filter(|link| link.from == source && !link.is_a_way_back())
        {
            let count = routes
                .iter()
                .filter(|route| route.from == link.from && route.to == link.to)
                .count();
            if count != 1 {
                notes.push(problem(
                    Some(source),
                    "Every next step after a conditional result needs exactly one visible condition."
                        .to_owned(),
                ));
                return;
            }
        }
    }
}

/// To, co reguły czytają z kroku, niezależnie od jego rodzaju.
///
/// Kafelek kontrolny nie pisze po plikach i nie woła vendora, więc `folder` i `passthrough` są
/// dla niego `None` — a reguła, która ich dotyczy, po prostu go pomija. To jest tańsze i mniej
/// kłamliwe niż udawanie, że checkpoint ma folder projektu.
#[derive(Debug, Clone, Copy)]
struct Facts<'a> {
    id: &'a str,
    /// Nazwa z kafelka. To ona pada w uwagach: `s_lonely` nie jest niczym, co użytkownik widzi.
    name: &'a str,
    copies: u8,
    folder: Option<&'a Folder>,
    passthrough: Option<&'a BTreeMap<String, BTreeMap<String, String>>>,
    /// Treść zadania kroku. `None` dla kafelka kontrolnego — on pyta człowieka, nie agenta.
    ///
    /// 2026-08-18 — DOŁOŻONE PO PIERWSZYM PRAWDZIWYM BIEGU. Właściciel uruchomił workflow, którego
    /// oba kroki miały `"instructions": ""`, i agent odpowiedział mu w strumieniu zdaniem
    /// „both have empty `instructions` — so the task description is blank there too. What would
    /// you like me to implement?". Czyli: zapłacone wywołanie vendora, trzy tury, i pytanie
    /// zamiast pracy. Loadout wiedział o tym PRZED startem i nie powiedział ani słowa.
    instructions: Option<&'a str>,
    /// Id agenta, którego krok nazywa. `None` dla kafelka kontrolnego — on nie woła vendora.
    ///
    /// 2026-08-18 — TEGO POLA TU NIE BYŁO i to była najdroższa luka walidatora. Żadna z siedmiu
    /// reguł nie czytała `agent`, więc plik z krokiem, który nie nazywa żadnego agenta,
    /// przechodził jako **bezproblemowy**: panel „things to fix" był pusty, `Run` aktywny,
    /// a odmowa padała kilka ekranów dalej komunikatem systemu plików bez słowa „agent"
    /// (`commands::run::find_agent` robiło `fs::read_dir` po nieistniejącym katalogu
    /// biblioteki). Zmierzone na dwóch plikach właściciela: oba miały `"agent": ""`.
    agent: Option<&'a str>,
    /// Komenda kroku „sprawdź". `None` dla kroków, które żadnej nie uruchamiają.
    command: Option<&'a str>,
    /// Wzorzec dowodu kroku „sprawdź". `None` jak wyżej.
    ///
    /// Osobne pole od [`Facts::command`], choć jedna reguła czyta oba: krok bez komendy i krok
    /// bez dowodu to dwa różne stany i naprawia się je w dwóch różnych polach kafelka.
    proof: Option<&'a str>,
}

fn facts(step: &Step) -> Facts<'_> {
    match step {
        Step::Agent(agent) => Facts {
            id: &agent.id,
            name: &agent.name,
            copies: agent.copies,
            folder: Some(&agent.folder),
            passthrough: Some(&agent.vendor_options),
            instructions: Some(&agent.instructions),
            agent: Some(&agent.agent),
            command: None,
            proof: None,
        },
        Step::Checkpoint(checkpoint) => Facts {
            id: &checkpoint.id,
            name: &checkpoint.name,
            copies: 1,
            folder: None,
            passthrough: None,
            instructions: None,
            agent: None,
            command: None,
            proof: None,
        },
        // `folder: Some(…)`, i to jest asercja (d) z AC-1 zapisana w kodzie. „To tylko
        // sprawdzenie, więc folder go nie dotyczy" jest nieprawdą — `cargo test` pisze po
        // `target/`, `npm test` po `node_modules/.cache` — a `folder: None` tutaj znaczy, że
        // `one_folder_two_steps` POMIJA ten krok całkowicie
        // (`let (Some(mine), Some(theirs)) = … else continue`) i dwa równoległe sprawdzenia
        // budujące w jednym katalogu zapisują się bez słowa (niezmiennik 12).
        //
        // `passthrough`, `instructions` i `agent` zostają `None`, bo krok „sprawdź" nie woła
        // żadnego vendora: reguła o pustym agencie i reguła o pustym zadaniu mają go pomijać,
        // a nie żądać od niego pól, których nie ma.
        Step::Check(check) => Facts {
            id: &check.id,
            name: &check.name,
            copies: 1,
            folder: Some(&check.folder),
            passthrough: None,
            instructions: None,
            agent: None,
            command: Some(&check.command),
            proof: Some(&check.proof),
        },
        /* `proof: None` I TO JEST TREŚĆ, nie przeoczenie. Krok „sprawdź" bez dowodu jest odmową
         * (niezmiennik 19: kod wyjścia to nie dowód), bo jego zadaniem jest ORZEC. Ten kafelek
         * niczego nie orzeka — ma coś podnieść i zostawić żywe — a żądanie od niego wzorca
         * dowodu byłoby polem, którego nie da się sensownie wypełnić. */
        Step::Serve(serve) => Facts {
            id: &serve.id,
            name: &serve.name,
            copies: 1,
            folder: Some(&serve.folder),
            passthrough: None,
            instructions: None,
            agent: None,
            command: Some(&serve.command),
            proof: None,
        },
    }
}

/// Uwaga, która blokuje Run i zapis.
fn problem(step_id: Option<&str>, message: String) -> Note {
    Note {
        level: Level::Problem,
        step_id: step_id.map(String::from),
        message,
        fix: None,
    }
}

/// Uwaga, która nie blokuje niczego.
fn warning(step_id: Option<&str>, message: String) -> Note {
    Note {
        level: Level::Warning,
        step_id: step_id.map(String::from),
        message,
        fix: None,
    }
}

/// Dwa kroki o jednym id: każda strzałka celująca w to id znaczy wtedy dwie rzeczy naraz.
fn one_id_two_steps(steps: &[Facts<'_>], notes: &mut Vec<Note>) {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for step in steps {
        let times = seen.entry(step.id).or_default();
        *times += 1;
        // Uwaga pada przy DRUGIM wystąpieniu i tylko przy nim: trzy kroki o jednym id to
        // wciąż jedna rzecz do naprawienia.
        if *times == 2 {
            notes.push(problem(
                Some(step.id),
                format!(
                    "Two steps have the same id ({}). Loadout cannot tell which one an arrow \
                     points at.",
                    step.id
                ),
            ));
        }
    }
}

/// Strzałka, której koniec nie istnieje.
///
/// Uwaga ląduje na tym końcu, który **istnieje**: kliknięcie uwagi przesuwa płótno na kafelek,
/// więc wskazanie kroku, którego nie ma, zamienia ją w martwy odnośnik.
fn arrows_into_nowhere(
    links: &[Link],
    steps: &[Facts<'_>],
    position: &BTreeMap<&str, usize>,
    notes: &mut Vec<Note>,
) {
    let named = |id: &str| {
        position
            .get(id)
            .and_then(|&index| steps.get(index))
            .copied()
    };

    for link in links {
        if named(&link.to).is_none() {
            let source = named(&link.from);
            notes.push(problem(
                source.map(|step| step.id),
                source.map_or_else(
                    || {
                        format!(
                            "An arrow points at a step that is not in this workflow ({}).",
                            link.to
                        )
                    },
                    |step| {
                        format!(
                            "\"{}\" points at a step that is not in this workflow ({}).",
                            step.name, link.to
                        )
                    },
                ),
            ));
        }
        if named(&link.from).is_none() {
            let target = named(&link.to);
            notes.push(problem(
                target.map(|step| step.id),
                target.map_or_else(
                    || {
                        format!(
                            "An arrow comes from a step that is not in this workflow ({}).",
                            link.from
                        )
                    },
                    |step| {
                        format!(
                            "\"{}\" waits for a step that is not in this workflow ({}).",
                            step.name, link.from
                        )
                    },
                ),
            ));
        }
    }
}

/// Krok, który nie nazywa żadnego agenta.
///
/// Waga zależy od tego, po co pytamy — powód stoi przy [`check_to_run`]. Zdanie jest to samo
/// w obu przypadkach i mówi, **co zrobić**, a nie tylko czego brakuje (DESIGN §8): nazwa
/// kafelka, potem dwie drogi wyjścia, w kolejności od tańszej.
fn a_step_without_an_agent(steps: &[Facts<'_>], when: When, notes: &mut Vec<Note>) {
    for step in steps {
        // `Some("")`, nie `None`: kafelek kontrolny agenta nie ma i nie ma mieć, a krok agenta
        // z pustym polem to krok, którego nikt jeszcze nie przypisał. Rozróżnienie po rodzaju,
        // bo pusty napis niesie tu informację, a brak pola nie niesie żadnej.
        let Some(agent) = step.agent else { continue };
        if !agent.trim().is_empty() {
            continue;
        }
        let message = format!(
            "\"{}\" does not have an agent yet, so it has nothing to run. Pick an agent on \
             the step, or create one in Agents first.",
            step.name
        );
        notes.push(match when {
            When::Saving => warning(Some(step.id), message),
            When::Running => problem(Some(step.id), message),
        });
    }
}

/// Krok, który nie mówi, co ma zrobić.
///
/// Waga zależy od tego, po co pytamy — dokładnie jak przy [`a_step_without_an_agent`]: szkic
/// w połowie zbudowany ma się ZAPISAĆ, a Run ma odmówić. Powód, dla którego ta reguła w ogóle
/// istnieje, stoi przy polu [`Facts::instructions`] i jest zmierzony na prawdziwym biegu.
///
/// Zdanie mówi, gdzie to wpisać, a nie tylko czego brakuje (DESIGN §8). „What to do" jest
/// etykietą TEGO pola w panelu kroku, więc człowiek czyta nazwę, którą widzi na ekranie.
fn a_step_without_a_task(steps: &[Facts<'_>], when: When, notes: &mut Vec<Note>) {
    for step in steps {
        // `Some("")`, nie `None`: kafelek kontrolny zadania nie ma i nie ma mieć.
        let Some(task) = step.instructions else {
            continue;
        };
        if !task.trim().is_empty() {
            continue;
        }
        let message = format!(
            "\"{}\" does not say what to do, so the agent would have to guess. Write it in \
             \"What to do\" on the step.",
            step.name
        );
        notes.push(match when {
            When::Saving => warning(Some(step.id), message),
            When::Running => problem(Some(step.id), message),
        });
    }
}

/// Kafelek z komendą, którego pola stoją puste — „sprawdź" albo „uruchom i zostaw".
///
/// PROBLEM, NIE OSTRZEŻENIE, i to jest inaczej niż przy [`a_step_without_an_agent`]. Różnica jest
/// realna: kafelek bez agenta czeka na wybór z listy, którą człowiek zaraz zobaczy, a krok
/// sprawdzający bez dowodu **jest gotowy i kłamie** — uruchomi się i orzeknie na samym kodzie
/// wyjścia. Suita, która nie uruchomiła ani jednego testu, wychodzi zerem (niezmiennik 19).
/// Ostrzeżenie tutaj nie blokowałoby `save()`, więc plik, który miał być odrzucony, wylądowałby
/// na dysku i pobiegł.
///
/// DWIE UWAGI, NIE JEDNA, kiedy brakuje obu rzeczy. Krok bez komendy i krok bez dowodu to dwa
/// różne stany i naprawia się je w dwóch różnych polach kafelka — zdanie mówiące o obu naraz
/// wysyłałoby człowieka do jednego z nich, a drugie zostawiałoby na następny raz. Kolejność jest
/// kolejnością pracy: najpierw wpisuje się, co uruchomić, potem po czym poznać, że ruszyło.
///
/// Zdania nazywają POLA TAK, JAK BRZMIĄ NA EKRANIE („Command to run", „Proof that it ran"),
/// żeby człowiek szukał tego, co widzi, a nie nazwy z pliku (niezmiennik 13). Ani jedno nie
/// niesie słowa „regex" ani nazwy kodu wyjścia: to zdanie czyta ktoś, kto właśnie dodał kafelek,
/// a nie ktoś, kto zna nasz schemat (niezmiennik 14).
fn a_command_step_left_empty(steps: &[Facts<'_>], notes: &mut Vec<Note>) {
    for step in steps {
        // `Some("")`, nie `None`: krok agenta i kafelek kontrolny komendy nie mają i nie mają
        // mieć, a krok sprawdzający z pustym polem to krok, którego nikt jeszcze nie wypełnił.
        //
        // 2026-08-23 — DWA WARUNKI, NIE JEDNA PARA. Do tego dnia obie połowy stały za wspólnym
        // `let (Some(command), Some(proof)) = … else continue`, więc kafelek „uruchom i zostaw"
        // — który komendę ma, a dowodu CELOWO nie ma (`facts`) — wypadał z tej reguły w całości.
        // Pusty kafelek zapisywał się bez słowa i odmawiał dopiero w środku biegu, po tym jak
        // człowiek odczekał swoje na krokach przed nim.
        if step
            .command
            .is_some_and(|command| command.trim().is_empty())
        {
            notes.push(problem(
                Some(step.id),
                format!(
                    "\"{}\" does not say what to run, so there would be nothing to start. Write \
                     it in \"Command to run\" on the step.",
                    step.name
                ),
            ));
        }
        if step.proof.is_some_and(|proof| proof.trim().is_empty()) {
            notes.push(problem(
                Some(step.id),
                format!(
                    "\"{}\" does not say how to tell that the work really ran, so it would call a \
                     command that did nothing at all a success. Write what its output has to say \
                     in \"Proof that it ran\" on the step.",
                    step.name
                ),
            ));
        }
    }
}

/// Komenda kroku, która niesie sekret literalnie.
///
/// 2026-08-28 (T-157) — [`Facts::command`] mają dwa kafelki, „sprawdź" i „uruchom i zostaw", i to
/// jedyne miejsca w definicji workflowu, gdzie mieszka ADRES. Wiersz `curl https://ci:…@…` wjeżdża
/// do pliku, który idzie do gita, do kopii i do wyników biegu — a sekret ma w tym produkcie jedną
/// drogę, env dziecka (niezmiennik 9).
///
/// PROBLEM NIEZALEŻNIE OD [`When`], tak samo jak przy [`a_command_step_left_empty`]: to nie jest
/// szkic w połowie zbudowany, który dokończy się za chwilę. Bajty z sekretem albo trafiają na dysk,
/// albo nie, a ostrzeżenie tutaj nie zablokowałoby `save()` — czyli plik, który miał być odrzucony,
/// leżałby już w `~/.loadout/workflows/` i w każdej kopii, którą ktoś zdążył zrobić.
///
/// Zdanie nazywa kafelek i POLE, nigdy samą wartość: człowiek ma wiedzieć, gdzie patrzeć, a uwaga
/// jedzie na ekran i do wyników biegu, więc wartość wpisana do niej żyłaby drugi raz.
fn a_command_carrying_a_secret(steps: &[Facts<'_>], notes: &mut Vec<Note>) {
    for step in steps {
        let Some(command) = step.command else {
            continue;
        };
        let Some(what) = secret_shaped(command) else {
            continue;
        };
        notes.push(problem(
            Some(step.id),
            format!(
                "\"{}\" has what looks like {what} in the command it runs. Loadout will not write \
                 that into a workflow file — a workflow file goes into git and into copies. Hand \
                 it to the agent as an environment variable instead.",
                step.name
            ),
        ));
    }
}

/// Dwie pętle, które dzielą choć jeden krok.
///
/// ODMOWA, NIE DOMYSŁ, i to jest granica przyznana wprost — tylko że od 2026-08-22 biegnie tam,
/// gdzie naprawdę leży. Do tego dnia odmawialiśmy KAŻDEGO drugiego powrotu, bo `workflow::unroll`
/// rozwijał jedną pętlę. Kosztowało to kształt, o który poprosił właściciel i który jest zwykłym
/// dniem pracy: jeden plan, dwie gałęzie (front i backend), każda ze swoim sprawdzeniem i swoją
/// poprawką. Te dwie pętle nie mają ze sobą nic wspólnego, rozwijają się niezależnie i odmowa
/// kazała wyrzucić jedną z gałęzi.
///
/// Czego dalej nie umiemy i dlatego odmawiamy: pętli ZAGNIEŻDŻONYCH i PRZECINAJĄCYCH SIĘ. Dla
/// kroku należącego do dwóch pętli naraz nie wiadomo ani ile razy ma się powtórzyć, ani która
/// jego runda wychodzi na zewnątrz. Gdyby tej reguły nie było, `unroll` musiałby jedną z pętli
/// po cichu porzucić, a bieg wyglądałby na udany, robiąc coś innego, niż narysował człowiek.
/// Cicha zmiana znaczenia grafu jest gorsza od odmowy, która mówi, czego jeszcze nie umiemy.
fn loops_that_cross(
    links: &[Link],
    steps: &[Facts<'_>],
    position: &BTreeMap<&str, usize>,
    forward: &[(usize, usize)],
    notes: &mut Vec<Note>,
) {
    // Ciało liczy `workflow::unroll`, ta sama funkcja, która potem rozwija bieg. Druga definicja
    // słowa „wspólny krok" rozjechałaby się przy pierwszej poprawce, a rozjazd znaczyłby, że ta
    // reguła wpuszcza plik, którego rozwinięcie nie rozumie.
    let bodies: Vec<(usize, BTreeSet<usize>)> = links
        .iter()
        .filter_map(|link| {
            link.max_turns?;
            let judge = *position.get(link.from.as_str())?;
            let entry = *position.get(link.to.as_str())?;
            Some((
                judge,
                crate::workflow::unroll::body_of(judge, entry, forward),
            ))
        })
        .collect();

    // Krok po nazwie, nie po identyfikatorze: `s_c` nie jest niczym, co użytkownik widzi.
    let name_of = |judge: usize| steps.get(judge).map_or("a step", |step| step.name);

    for (at, (judge, body)) in bodies.iter().enumerate() {
        for (earlier, other) in bodies.iter().take(at) {
            if body.is_disjoint(other) {
                continue;
            }
            // JEDNA uwaga, nie po jednej na parę: człowiek ma tu do zrobienia jedną rzecz —
            // rozdzielić te pętle — a trzy zdania o tym samym czyta się jak trzy usterki.
            notes.push(problem(
                steps.get(*judge).map(|step| step.id),
                format!(
                    "\"{}\" and \"{}\" send the work back over the same steps. Loadout runs loops \
                     side by side, never one inside another. Keep one of them, or move them apart.",
                    name_of(*judge),
                    name_of(*earlier)
                ),
            ));
            return;
        }
    }
}

/// Liczba rund powrotu poza zakresem 1–[`MOST_TURNS`].
///
/// `0` i `11` są dwoma różnymi rodzajami nonsensu i oba muszą paść. Zero znaczy „pętla, która
/// nie wykonuje się ani razu" — czyli narysowana strzałka bez skutku, niezmiennik 16 wpisany do
/// pliku. Powyżej sufitu to noc bez nadzoru i rachunek, którego nikt się nie spodziewa.
///
/// Uwaga nazywa krok, z którego powrót WYCHODZI: to on jest sędzią pętli i to jego kafelek
/// człowiek otworzy, żeby zmienić tę liczbę.
fn turns_out_of_range(
    links: &[Link],
    steps: &[Facts<'_>],
    position: &BTreeMap<&str, usize>,
    notes: &mut Vec<Note>,
) {
    for link in links {
        let Some(turns) = link.max_turns else {
            continue;
        };
        if (1..=MOST_TURNS).contains(&turns) {
            continue;
        }
        // Krok po nazwie, nie po identyfikatorze: `s_test` nie jest niczym, co użytkownik widzi.
        let named = position
            .get(link.from.as_str())
            .and_then(|&index| steps.get(index));
        let name = named.map_or(link.from.as_str(), |step| step.name);
        notes.push(problem(
            named.map(|step| step.id),
            format!(
                "\"{name}\" would send the work back {turns} times. Pick a number from 1 to \
                 {MOST_TURNS}."
            ),
        ));
    }
}

/// Źródło strzałki powrotnej wydaje jeden werdykt, więc nie może biec w kilku kopiach.
///
/// 2026-08-24 (T-114) — sędzią jest `link.from`, który zamyka pętlę; `link.to` jest jej
/// wejściem i może legalnie mieć kilka kopii. Zbiór po id źródła sprawia, że dwie strzałki
/// powrotne od tego samego sędziego dają człowiekowi jedno zdanie, nie dwa.
fn loop_judges_run_once(
    links: &[Link],
    steps: &[Facts<'_>],
    position: &BTreeMap<&str, usize>,
    notes: &mut Vec<Note>,
) {
    let mut judged: BTreeSet<&str> = BTreeSet::new();
    for link in links.iter().filter(|link| link.is_a_way_back()) {
        if !judged.insert(link.from.as_str()) {
            continue;
        }
        let Some(step) = position
            .get(link.from.as_str())
            .and_then(|at| steps.get(*at))
        else {
            continue;
        };
        if step.copies > 1 {
            notes.push(problem(
                Some(step.id),
                format!(
                    "\"{}\" closes a loop, so it can only run once at a time.",
                    step.name
                ),
            ));
        }
    }
}

/// Liczba kopii poza zakresem 1–[`MOST_COPIES`].
fn copies_out_of_range(steps: &[Facts<'_>], notes: &mut Vec<Note>) {
    for step in steps {
        if step.copies == 0 {
            notes.push(problem(
                Some(step.id),
                format!(
                    "\"{}\" is set to run zero times, so it would never start. Pick a number \
                     from 1 to {MOST_COPIES}.",
                    step.name
                ),
            ));
        } else if step.copies > MOST_COPIES {
            notes.push(problem(
                Some(step.id),
                format!(
                    "\"{}\" would run {} copies at the same time. Pick a number from 1 to \
                     {MOST_COPIES}.",
                    step.name, step.copies
                ),
            ));
        }
    }
}

/// Dwie planowane własne kopie, które po kodowaniu wybrałyby ten sam ref Gita.
///
/// 2026-08-24 (T-114) — katalogi `s_2~2` i `s_2-2` są różne, ale oba kodują się jako ref
/// `s_2-2`. Przy zapisie to ostrzeżenie, bo luźny szkic ma pozostać zapisywalny; przy Starcie
/// ten sam fakt jest problemem, zanim powstanie katalog biegu albo pierwszy proces.
fn colliding_work_branches(steps: &[Facts<'_>], when: When, notes: &mut Vec<Note>) {
    let mut reserved: BTreeMap<String, usize> = BTreeMap::new();
    let mut reported: BTreeSet<(usize, usize, String)> = BTreeSet::new();
    for (index, step) in steps.iter().enumerate() {
        if !step.folder.is_some_and(Folder::is_own_copy) {
            continue;
        }
        for copy in 0..step.copies {
            let branch = work_branch_tail(&work_key_for(step.id, copy));
            let Some(&other) = reserved.get(&branch) else {
                reserved.insert(branch, index);
                continue;
            };
            if other == index || !reported.insert((other, index, branch.clone())) {
                continue;
            }
            let Some(first) = steps.get(other) else {
                continue;
            };
            let message = format!(
                "\"{}\" and \"{}\" would use the same work branch \"{branch}\". Rename one of them before starting.",
                first.name, step.name
            );
            notes.push(match when {
                When::Saving => warning(Some(first.id), message),
                When::Running => problem(Some(first.id), message),
            });
        }
    }
}

/// Przelotka podnosząca flagę, którą Loadout ustawia sam.
///
/// Trzy granice, wszystkie przy zapisie: kolizja z naszą flagą, próba podniesienia dialu „co agent
/// może zrobić z plikami" i literalny sekret. Druga jest **niezależna od listy** — `--sandbox` nie
/// jest zarezerwowane, a `--sandbox danger-full-access` omija dial dokładnie tak samo jak `-s`.
///
/// 2026-08-28 (T-157) — sekret stoi PRZED kolizją nazw, bo jest cięższy: wiersz kolidujący
/// z naszą flagą jest do skasowania i tyle, a wiersz z kluczem trafiłby do pliku, który idzie do
/// gita i do kopii. Kiedy wpis odpada z obu powodów, człowiek ma przeczytać ten drugi.
fn the_passthrough(steps: &[Facts<'_>], notes: &mut Vec<Note>) {
    for step in steps {
        let Some(options) = step.passthrough else {
            continue;
        };
        for (vendor, flags) in options {
            for (flag, value) in flags {
                if let Some(raise) = escalation_in(flag, value) {
                    notes.push(problem(
                        Some(step.id),
                        format!(
                            "\"{}\" tries to set {raise} through its {} options. What an agent \
                             may do with your files is set on the step itself.",
                            step.name,
                            vendor_name(vendor)
                        ),
                    ));
                } else if let Some(what) = literal_secret_in(flag, value) {
                    // Zdanie nazywa WIERSZ i nigdy nie cytuje wartości: uwaga jedzie na ekran,
                    // do wyników biegu i do wszystkiego, co je kopiuje, więc sekret wpisany do
                    // niej żyłby drugi raz w miejscu, którego nikt nie sprząta.
                    notes.push(problem(
                        Some(step.id),
                        format!(
                            "\"{}\" has what looks like {what} in its {} options, in the line \
                             {flag}. Loadout will not write that into a workflow file — a \
                             workflow file goes into git and into copies. Hand it to the agent as \
                             an environment variable instead.",
                            step.name,
                            vendor_name(vendor)
                        ),
                    ));
                } else if is_reserved(vendor, flag) {
                    notes.push(problem(
                        Some(step.id),
                        format!(
                            "Loadout sets {flag} itself, so \"{}\" cannot set it too. Remove it \
                             from this step's {} options.",
                            step.name,
                            vendor_name(vendor)
                        ),
                    ));
                }
            }
        }
    }
}

/// Flagi zarezerwowane dla tego vendora. Vendor spoza listy nie ma żadnych — przelotka istnieje
/// właśnie po to, żeby nowy vendor nie wymagał wydania Loadouta.
///
/// `pub` od 2026-08-23 (T-90), bo pyta o to samo także przelotka DEFINICJI AGENTA
/// (`library::agents::passthrough_refused`). Krok workflow i plik agenta to dwa nośniki tej
/// samej przelotki, a lista jest jedna i mieszka tutaj (niezmiennik 23): druga kopia po tamtej
/// stronie rozjechałaby się w dniu, w którym ktoś dopisze pozycję tylko do jednej z nich.
#[must_use]
pub fn reserved(vendor: &str) -> &'static [&'static str] {
    match vendor {
        "claude" => &RESERVED_CLAUDE,
        "codex" => &RESERVED_CODEX,
        _ => &[],
    }
}

/// Klucz tego wpisu przelotki — czyli to, co stoi **przed** pierwszym `=`.
///
/// 2026-08-24 (T-98) — `--dangerously-skip-permissions=true` to ten sam wiersz, co
/// `"--dangerously-skip-permissions": ""`, tylko zapisany inaczej, a człowiek, któremu raz
/// odmówiono, pisze go drugi raz właśnie tak. Do tego dnia zamykał tę furtkę przypadkiem
/// `contains`; odkąd dopasowanie idzie po nazwie, musi ją zamykać ktoś z premedytacją.
///
/// Ten sam kształt jest u drugiego vendora **normą, a nie wyjątkiem**: cała jego konfiguracja
/// jedzie jako `-c klucz=wartość` (`library::agents::vendor_argv`).
fn key_of(flag: &str) -> &str {
    flag.split_once('=').map_or(flag, |(key, _)| key)
}

/// Czy ta pozycja listy zamyka ten klucz. Pozycja z kropką na końcu jest **prefiksem rodziny**,
/// każda inna jest nazwą i porównuje się przez równość.
///
/// To rozróżnienie jest całą treścią „po kluczu, nie po podciągu". Rodzin jest dziś dwie
/// (`mcp_servers.`, `model_providers.`) i obie mają w środku nazwę wpisywaną przez człowieka,
/// więc listy równościowej dla nich nie da się napisać. Wszystko poza nimi zostaje nazwą, bo
/// `--verbose` jest nasze, a `--verbose-tool-output` jest inną flagą tej samej aplikacji.
fn covers(rule: &str, key: &str) -> bool {
    if rule.ends_with('.') {
        key.starts_with(rule)
    } else {
        key == rule
    }
}

/// Czy ten wpis przelotki koliduje z czymś, co Loadout ustawia temu vendorowi sam.
///
/// Jedna reguła dla obu nośników przelotki — kroku workflow ([`the_passthrough`]) i definicji
/// agenta (`library::agents::passthrough_refused`). Druga kopia reguły po tamtej stronie
/// rozjechałaby się w dniu, w którym ktoś zmieni jedną z nich (niezmiennik 23), a rozjazd
/// widać dopiero z zachowania procesu.
#[must_use]
pub(crate) fn is_reserved(vendor: &str, flag: &str) -> bool {
    let key = key_of(flag);
    reserved(vendor)
        .iter()
        .copied()
        .any(|rule| covers(rule, key))
}

/// Podniesienie, przez które ten wpis przelotki odpada — albo `None`, kiedy nic nie podnosi.
///
/// **Niezależne od vendora**, bo dial jest jeden (D6) i bo [`reserved`] dla nieznanej nazwy
/// aplikacji oddaje pustą listę z rozmysłu: gdyby ta reguła też była per vendor, wpis schowany
/// pod nazwą aplikacji, której jeszcze nie wspieramy, przechodziłby obok wszystkiego.
///
/// Dwie połowy i obie są konieczne. Sama nazwa przepuszcza `--sandbox danger-full-access`
/// (`--sandbox` nie jest zarezerwowane, a dial omija tak samo skutecznie jak `-s`); sama
/// wartość przepuszcza flagę, która JEST podniesieniem i stoi z pustą wartością.
#[must_use]
pub(crate) fn escalation_in(flag: &str, value: &str) -> Option<&'static str> {
    let key = key_of(flag);
    FORBIDDEN_ESCALATIONS
        .iter()
        .copied()
        .find(|raise| key == *raise || value.contains(raise))
}

/// Sekret podany LITERALNIE w tym wpisie przelotki — albo `None`, kiedy wpis niczego nie niesie.
///
/// **Niezależne od vendora i od nazwy wpisu**, dokładnie jak [`escalation_in`]: plik definicji
/// jedzie do gita niezależnie od tego, która aplikacja miała dostać tę flagę, a nazwa flagi jest
/// tym, co człowiek wymyśla — `--auth-header` nie ma w sobie ani „key", ani „token", a niesie
/// klucz tak samo skutecznie jak `--api-key`. Rozróżnienie należy więc do KSZTAŁTU wartości.
///
/// Prawa strona `klucz=wartość` jest czytana tak samo jak sama wartość, i z tego samego powodu,
/// dla którego [`key_of`] czyta lewą: u drugiego vendora cała konfiguracja jedzie tym zapisem
/// (`library::agents::vendor_argv`), więc sekret schowany za znakiem równości byłby tą samą
/// dziurą o jeden znak dalej.
///
/// Czytają to **trzy** miejsca i to jest cała polityka, jedna (niezmiennik 23): przelotka kroku
/// ([`the_passthrough`]), przelotka definicji agenta (`library::agents::passthrough_refused`)
/// oraz brama zapisu pliku agenta (`library::agents::write_agent_file`).
#[must_use]
pub(crate) fn literal_secret_in(flag: &str, value: &str) -> Option<&'static str> {
    secret_shaped(value).or_else(|| {
        flag.split_once('=')
            .and_then(|(_, written)| secret_shaped(written))
    })
}

/// Czym ten tekst wygląda na sekret — albo `None`, kiedy nie wygląda na nic.
///
/// Trzy reguły, wszystkie o KSZTAŁCIE, w kolejności od najwięcej mówiącej człowiekowi: adres
/// z hasłem nazywa MIEJSCE, w którym sekret siedzi, i to jest zdanie, po którym wiadomo, gdzie
/// patrzeć; „a key" z prefiksu albo z samego kształtu jest zdaniem słabszym i stoi niżej.
///
/// `pub(crate)`, bo pyta o to samo także reguła o komendzie kroku ([`a_command_carrying_a_secret`])
/// i nie ma tam wpisu przelotki, który dałoby się podać do [`literal_secret_in`].
#[must_use]
pub(crate) fn secret_shaped(text: &str) -> Option<&'static str> {
    a_web_address_carrying_one(text)
        .or_else(|| a_known_key(text))
        .or_else(|| a_packed_run(text))
}

/// Ciągi, z których składają się klucze: litery, cyfry, `_` i `-`.
fn key_runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|letter: char| !(letter.is_ascii_alphanumeric() || matches!(letter, '_' | '-')))
}

/// Reguła pierwsza: znany prefiks dostawcy z dość długim ogonem za nim.
fn a_known_key(text: &str) -> Option<&'static str> {
    key_runs(text)
        .any(|run| {
            SECRET_PREFIXES.iter().any(|prefix| {
                run.strip_prefix(prefix)
                    .is_some_and(|tail| tail.len() >= A_KNOWN_KEY)
            })
        })
        .then_some("a key")
}

/// Reguła druga: adres z sekretem w środku.
///
/// Pętla po KAŻDYM `://` w tekście, bo pole „co uruchomić" jest wierszem powłoki i bywa w nim
/// więcej niż jeden adres — a odmowa, która zna tylko pierwszy, jest odmową do obejścia
/// przestawieniem argumentów.
fn a_web_address_carrying_one(text: &str) -> Option<&'static str> {
    let mut rest = text;
    while let Some((_, after)) = rest.split_once("://") {
        // Adres kończy się na pierwszym białym znaku: po nim w wierszu powłoki stoją argumenty.
        if let Some(found) = inside_the_address(after.split_whitespace().next().unwrap_or_default())
        {
            return Some(found);
        }
        // `after` jest krótsze od `rest` o prefiks i o same trzy znaki, więc pętla ma koniec.
        rest = after;
    }
    None
}

/// Co ten jeden adres niesie w części przed `@` i w swoich parametrach.
fn inside_the_address(address: &str) -> Option<&'static str> {
    let authority = address
        .split_once(['/', '?', '#'])
        .map_or(address, |(host, _)| host);
    // Po OSTATNIM `@`, nie po pierwszym: hasło wolno mieć w sobie ten znak, nazwa serwera nie.
    if let Some((who, _)) = authority.rsplit_once('@')
        && who
            .split_once(':')
            .is_some_and(|(_, password)| !password.is_empty())
    {
        return Some("a password in a web address");
    }
    let query = address.split_once('?').map_or("", |(_, rest)| rest);
    query
        .split('#')
        .next()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .any(|(name, value)| {
            value.len() >= A_PARAMETER_VALUE
                && SECRET_PARAMETERS.contains(&name.to_ascii_lowercase().replace('-', "_").as_str())
        })
        .then_some("a key")
}

/// Reguła trzecia: długi zbity ciąg o trzech klasach znaków naraz. Próg i jego pomiar stoją
/// przy [`A_PACKED_RUN`] — to ta reguła może zablokować pracę, więc to ona ma tam uzasadnienie.
fn a_packed_run(text: &str) -> Option<&'static str> {
    text.split(|letter: char| {
        !(letter.is_ascii_alphanumeric() || matches!(letter, '_' | '-' | '+' | '='))
    })
    .filter(|run| run.len() >= A_PACKED_RUN)
    .any(|run| {
        run.chars().any(char::is_lowercase)
            && run.chars().any(char::is_uppercase)
            && run.chars().any(|letter| letter.is_ascii_digit())
    })
    .then_some("a key")
}

/// Nazwa vendora tak, jak nazywa go użytkownik. Klucz z pliku (`claude`) na ekran nie idzie.
///
/// `pub(crate)`, bo tę samą odpowiedź musi znać każda odmowa nazywająca program po imieniu,
/// a jest ich dziś troje i wszystkie siedzą w tej skrzyni: zapis kroku z przelotką, start biegu
/// z definicji agenta (T-90) oraz krok, który pożyczył umiejętność programowi nieumiejącemu
/// przyjąć katalogu pluginu (`commands::run`, T-93). Druga tabela nazw obok tej rozjechałaby się
/// przy pierwszym nowym vendorze i nikt by tego nie zauważył, bo dziś odpowiadają tak samo
/// (niezmienniki 13 i 14).
#[must_use]
pub(crate) fn vendor_name(vendor: &str) -> &str {
    match vendor {
        "claude" => "Claude Code",
        "codex" => "Codex",
        other => other,
    }
}

/// Koło.
///
/// 2026-08-16 — liczy je `engine::dag`, który odmawia cyklu przy konstrukcji, na listach
/// sąsiedztwa i bez `petgraph` (ARCHITECTURE §10), i oddaje kroki, które na nim leżą. Drugi
/// obchód w tym pliku byłby dokładnie tym duplikatem, przed którym ostrzega zadanie.
fn a_circle(steps: &[Facts<'_>], arrows: &[(usize, usize)], notes: &mut Vec<Note>) {
    // `UnknownNode` tędy nie przechodzi: `arrows` ma już tylko strzałki o istniejących końcach.
    if let Err(DagError::Cycle { nodes }) = Dag::new(steps.len(), arrows) {
        // Jedno koło to jedna rzecz do naprawienia — trzy uwagi o jednej pomyłce czytają się
        // jak trzy pomyłki. Kropka ląduje na pierwszym kroku, który na nim utknął.
        let named = nodes
            .first()
            .and_then(|&index| steps.get(index))
            .map(|step| step.id);
        notes.push(problem(named, CIRCLE.to_owned()));
    }
}

/// Zdanie o kroku, który ma pracować tam, gdzie krok przed nim, a przed nim nie ma nikogo.
///
/// `pub`, bo mówią je **dwa miejsca** i ma być jednym zdaniem (niezmiennik 13): walidator poniżej
/// oraz planista biegu (`commands::run`), który dochodzi do tego samego braku od drugiej strony —
/// przy rozwiązywaniu katalogu roboczego, już po rozwinięciu pętli. Dwie kopie tego samego zdania
/// rozjechałyby się przy pierwszej poprawce jednej z nich.
///
/// Krok nazwany jest **nazwą z kafelka**: `s_head` jest kluczem w pliku i nie ma go na ekranie
/// (niezmiennik 14). Zdanie mówi też, co z tym zrobić, i wymienia obie drogi wyjścia (DESIGN §8) —
/// pociągnąć strzałkę albo dać krokowi własną kopię.
#[must_use]
pub fn nothing_before(name: &str) -> String {
    format!(
        "\"{name}\" is set to work in the same folder as the step before it, and nothing comes \
         before it. Draw an arrow into it, or give it a fresh copy."
    )
}

/// Krok „to samo drzewo, w którym pracował krok przede mną", przed którym nic nie stoi.
///
/// PROBLEM, NIE OSTRZEŻENIE, także przy zapisie — inaczej niż para kolidujących kroków
/// z [`one_folder_two_steps`], i różnica jest ta sama, co przy kroku o zerowej liczbie kopii:
/// tamta para to stan przejściowy, który jest poprawnym plikiem, dopóki nikt nie naciśnie Run,
/// a ten krok wskazuje katalog, którego **nie ma jak wyliczyć** — plik niesie ustawienie bez
/// znaczenia i żaden bieg z niego nie ruszy. Autosave nie ma tu czego zablokować w połowie gestu:
/// przełącznik folderu w panelu kroku przestawia wyłącznie między folderem projektu a własną
/// kopią (`src/sections/workflows/step-panel/panel.tsx`), więc ta wartość bierze się dziś
/// wyłącznie z pliku napisanego ręcznie albo przez inny build.
fn nothing_before_it(steps: &[Facts<'_>], forward: &[(usize, usize)], notes: &mut Vec<Note>) {
    for (index, step) in steps.iter().enumerate() {
        // Kafelek kontrolny folderu nie ma i mieć nie może, więc `Some` jest tu częścią pytania,
        // a nie zabezpieczeniem przed `None`.
        if !step
            .folder
            .is_some_and(|folder| matches!(folder, Folder::SameCopy))
        {
            continue;
        }
        if forward.iter().any(|&(_, to)| to == index) {
            continue;
        }
        notes.push(problem(Some(step.id), nothing_before(step.name)));
    }
}

/// Dwa kroki, które **mogą biec równocześnie**, piszące po tych samych plikach.
///
/// „Mogą biec równocześnie" znaczy dokładnie jedno: nie istnieje ścieżka po strzałkach ani
/// stąd tam, ani stamtąd tu. Reguła bez tego zdania odmawia zwykłego łańcucha `plan → build`,
/// ktoś zgłasza to jako błąd, ktoś inny „naprawia" ją przez wyłączenie — i zostaje martwy kod
/// (niezmiennik 12).
/// Dwa kroki, które mogą biec równocześnie, celujące w te same pliki.
///
/// WAGA ZALEŻY OD TEGO, PO CO PYTAMY, i to jest rozstrzygnięcie właściciela z 2026-08-19.
/// Para bez strzałki jest **ostrzeżeniem przy zapisie** i **problemem przy Run** — tym samym
/// wzorcem, którym stoją [`a_step_without_an_agent`] i [`a_step_without_a_task`].
///
/// Powód jest mierzony na edytorze, nie estetyczny. Kafelki dokłada się na płótno luzem
/// i dopiero potem łączy strzałkami — to jest cały gest budowania workflow, w tym takiego,
/// gdzie trzy gałęzie wchodzą do jednego kroku. Dopóki ta reguła odmawiała przy zapisie, DRUGI
/// dołożony kafelek robił z dokumentu plik niezapisywalny: autosave dostawał odmowę, na ekranie
/// stało „this workflow was not saved", a praca człowieka żyła wyłącznie w pamięci okna.
/// Wymuszało to strzałkę doklejaną automatycznie do ostatniego kroku — czyli edytor, w którym
/// nie da się zbudować niczego poza łańcuchem.
///
/// Niezmiennik 12 na tym nie traci ANI JEDNEGO biegu: `check_to_run` woła się w
/// `commands::run` **przed** uruchomieniem czegokolwiek, więc odmowa dalej pada, zanim
/// pierwszy agent dotknie pliku. Zdanie niezmiennika przeciwstawia się odkrywaniu kolizji
/// wtedy, gdy agenci już po sobie nadpisują — a nie Startowi.
fn one_folder_two_steps(
    steps: &[Facts<'_>],
    arrows: &[(usize, usize)],
    forward: &[(usize, usize)],
    when: When,
    notes: &mut Vec<Note>,
) {
    for step in steps {
        // Krok w kilku kopiach biegnie równocześnie sam ze sobą — z definicji, bez żadnej
        // strzałki. To JEDYNA gałąź tej reguły, która zostaje problemem także przy zapisie,
        // i różnica jest realna: para bez strzałki to stan przejściowy, który człowiek naprawia
        // gestem na płótnie, a krok kolidujący sam ze sobą nie ma strzałki, którą dałoby się go
        // naprawić — wyjściem jest wyłącznie zmiana pola, więc nie ma czego czekać na Run.
        if step.copies > 1 && step.folder.is_some_and(|folder| !folder.is_own_copy()) {
            notes.push(problem(
                Some(step.id),
                format!(
                    "\"{}\" runs {} copies at the same time and they would all work in the same \
                     folder. Give it a fresh copy.",
                    step.name, step.copies
                ),
            ));
        }
    }

    let reach = reachable(steps.len(), arrows);
    // Gdzie każdy krok pracuje, policzone RAZ. Odpowiedź dla `same-copy` wymaga obchodu grafu
    // wstecz, a pytanie o nią stoi w pętli po parach — czyli w miejscu, w którym liczy się je
    // kwadratowo razy.
    let spots: Vec<Option<Spot<'_>>> = (0..steps.len())
        .map(|index| spot_of(index, steps, forward))
        .collect();
    for (first, one) in steps.iter().enumerate() {
        for (second, other) in steps.iter().enumerate().skip(first + 1) {
            if reach[first][second] || reach[second][first] {
                continue;
            }
            let (Some(mine), Some(theirs)) = (spots[first], spots[second]) else {
                continue;
            };
            if !the_same_files(mine, theirs) {
                continue;
            }
            let message = format!(
                "\"{}\" and \"{}\" can run at the same time and {}. Give one of them a fresh \
                 copy.",
                one.name,
                other.name,
                place(mine, theirs, steps)
            );
            // Zdanie jest to samo w obu wagach: człowiek ma przeczytać przy zapisie dokładnie
            // to, co zatrzyma mu Start, a nie dwa opisy jednej kolizji.
            notes.push(match when {
                When::Saving => warning(Some(one.id), message),
                When::Running => problem(Some(one.id), message),
            });
        }
    }
}

/// Gdzie krok NAPRAWDĘ pracuje — po rozwiązaniu „to samo drzewo, co krok przede mną".
///
/// 2026-08-23 (T-95) — TEN TYP JEST CAŁĄ NAPRAWĄ. Do tego dnia reguła o kolizji porównywała
/// [`Folder`], a `Folder::SameCopy` folderu nie nazywa: „to samo drzewo, co krok przede mną"
/// jest zdaniem o GRAFIE. Para dwóch takich kroków wpadała więc do reguły i wychodziła z niej
/// bez uwagi — a to jest dokładnie kolizja z niezmiennika 12, tylko widoczna o jeden obchód
/// dalej: dwa kroki po jednej `fresh-copy`, bez strzałki między sobą, dostają JEDEN katalog
/// i piszą po sobie nawzajem, oba kończąc się sukcesem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spot<'a> {
    /// Folder projektu, w którym biegnie workflow.
    Project,
    /// Folder wskazany ręcznie.
    Picked(&'a str),
    /// Kopia, którą bieg zakłada dla kroku o tej pozycji. Dwa kroki są w niej razem wtedy
    /// i tylko wtedy, gdy schodzą do TEGO SAMEGO kroku, który ją nazwał.
    OwnCopy(usize),
}

/// Folder, który krok nazywa SAM — albo `None`, kiedy nie nazywa go wcale.
///
/// `None` dla `same-copy` i to jest ta sama odpowiedź, którą daje `commands::run::workspace`:
/// tutaj wchodzi jeden krok, a odpowiedź wymaga strzałek.
fn named_spot(folder: &Folder, index: usize) -> Option<Spot<'_>> {
    match folder {
        Folder::Project => Some(Spot::Project),
        Folder::Pick { path } => Some(Spot::Picked(path)),
        Folder::FreshCopy => Some(Spot::OwnCopy(index)),
        Folder::SameCopy => None,
    }
}

/// Gdzie pracuje krok o tej pozycji — z odpowiedzią także dla tego, który sam jej nie zna.
///
/// TĄ SAMĄ REGUŁĄ, CO BIEG (`commands::run::trees_before`), i to jest jedyny powód, dla którego
/// wolno ją tu napisać drugi raz: druga reguła na to samo pytanie rozjechałaby się przy pierwszej
/// poprawce jednej z nich, a rozjazd znaczy walidator mówiący co innego niż bieg, który za
/// sekundę ruszy.
///
/// `None`, kiedy odpowiedzi nie ma: kafelek kontrolny nie pracuje w żadnym folderze, a krok
/// „to samo drzewo", przed którym nie stoi nikt, nie ma z czego jej wyliczyć. Ta sytuacja ma już
/// własną odmowę ([`nothing_before_it`]), więc tutaj jest milczeniem, a nie zgadywaniem.
///
/// 2026-08-29 — KROK, PRZED KTÓRYM PRACUJĄ RÓŻNE MIEJSCA, MA OD TERAZ ODPOWIEDŹ: własną kopię,
/// do której bieg znosi pracę wszystkich poprzedników (`commands::run::where_it_works`). Do tego
/// dnia było to `None` i miało tam odmowę — a bez tego lustra walidator przestałby widzieć
/// kolizję dwóch kroków stojących za jednym składaniem, bo obydwa czytałyby się jako miejsca
/// nieznane.
///
/// Wzajemna rekurencja z [`spots_before`] idzie WYŁĄCZNIE po strzałkach wstecz, więc kończy się:
/// graf bez cykli, a każdy krok obchodu cofa się o co najmniej jedną strzałkę.
fn spot_of<'a>(index: usize, steps: &[Facts<'a>], forward: &[(usize, usize)]) -> Option<Spot<'a>> {
    let folder = steps.get(index)?.folder?;
    if let Some(named) = named_spot(folder, index) {
        return Some(named);
    }
    let mut before = spots_before(index, steps, forward);
    match before.len() {
        0 => None,
        1 => Some(before.remove(0)),
        _ => Some(Spot::OwnCopy(index)),
    }
}

/// W jakich miejscach pracują kroki PRZED tym — bez powtórzeń.
///
/// Obchód idzie po strzałkach wstecz, ze zbiorem odwiedzonych: fan-in bywa diamentem, więc bez
/// niego ten sam krok liczyłby się dwa razy i zwykłe rozwidlenie wyglądałoby jak dwa różne
/// miejsca. Iteracyjny, nie rekurencyjny — łańcuch dwudziestu kroków nie ma prawa przepełnić
/// stosu (ta sama zasada, co przy [`reachable`]).
///
/// Mija po drodze dwa rodzaje kroków, które miejsca nie wyznaczają: kafelek kontrolny (nie
/// dotyka plików) i krok „to samo drzewo", który sam niczego nie składa (jego odpowiedź jest tym
/// samym pytaniem, zadanym dalej). Stąd „najbliższy poprzednik, jakiegokolwiek rodzaju jest".
///
/// Krok, który SKŁADA, obchód zatrzymuje i melduje swoją własną kopię: praca jego rodziców jest
/// od 2026-08-29 właśnie w niej, więc schodzenie do dziadków pokazywałoby dwa miejsca tam, gdzie
/// naprawdę jest jedno. To jest to samo zdanie, co `commands::run::works_in`.
///
/// STRZAŁKI BEZ POWROTÓW, bo powrót wchodzi do kroku dopiero w rundzie drugiej
/// (`workflow::unroll`) — ta sama lista, na której [`nothing_before_it`] rozstrzyga, czy przed
/// krokiem ktokolwiek stoi. Dwie różne listy dawałyby krok, przed którym „nikogo nie ma"
/// i który jednocześnie ma z czego wyliczyć swoje drzewo.
fn spots_before<'a>(node: usize, steps: &[Facts<'a>], forward: &[(usize, usize)]) -> Vec<Spot<'a>> {
    let mut seen = vec![false; steps.len()];
    // Ten krok od razu jako odwiedzony: strzałka do siebie samego jest kształtem, którego bieg
    // odmawia, ale obchód nie ma prawa się o nią zapętlić, gdyby jednak w pliku stała.
    if let Some(mine) = seen.get_mut(node) {
        *mine = true;
    }
    let mut stack = vec![node];
    let mut found: Vec<Spot<'a>> = Vec::new();
    while let Some(at) = stack.pop() {
        for &(from, to) in forward {
            if to != at {
                continue;
            }
            let Some(first_time) = seen.get_mut(from).filter(|been| !**been) else {
                continue;
            };
            *first_time = true;
            match spot_of(from, steps, forward) {
                Some(spot) if !found.contains(&spot) => found.push(spot),
                Some(_) => {}
                // Kafelek kontrolny albo `same-copy` bez własnej kopii: to samo pytanie, tylko
                // o krok dalej wstecz.
                None => stack.push(from),
            }
        }
    }
    found
}

/// Czy dwa miejsca to te same pliki.
fn the_same_files(one: Spot<'_>, other: Spot<'_>) -> bool {
    match (one, other) {
        (Spot::Project, Spot::Project) => true,
        (Spot::Picked(mine), Spot::Picked(theirs)) => {
            // Po SEGMENTACH, nie po znakach: `/Users/x/api2` zaczyna się tak samo jak
            // `/Users/x/api`, a jest zupełnie innym folderem. `Path::starts_with` jest jedyną
            // wersją tego porównania, która o tym wie — `str::starts_with` wysyła użytkownika
            // do naprawiania czegoś, co nie jest zepsute.
            Path::new(mine).starts_with(theirs) || Path::new(theirs).starts_with(mine)
        }
        // Dwie RÓŻNE własne kopie nie kolidują z niczym — to jest cała obietnica izolacji
        // z ARCHITECTURE §2 punkt 4. Jedna i ta sama kopia koliduje, i to jest jedyna rzecz,
        // która się tu 2026-08-23 zmieniła.
        (Spot::OwnCopy(mine), Spot::OwnCopy(theirs)) => mine == theirs,
        // `project` kontra `pick` nie koliduje: 2026-08-16 — w pliku workflow nie ma ścieżki
        // projektu, bo projekt wybiera się przy uruchomieniu, więc porównanie ich tutaj byłoby
        // zgadywaniem. Tę parę widzi dopiero bieg (T-15), który zna oba katalogi.
        _ => false,
    }
}

/// Druga połowa zdania o kolizji: gdzie te dwa kroki się spotykają.
fn place(one: Spot<'_>, other: Spot<'_>, steps: &[Facts<'_>]) -> String {
    match (one, other) {
        (Spot::Picked(mine), Spot::Picked(theirs)) if mine == theirs => {
            format!("both work in {mine}")
        }
        (Spot::Picked(..), Spot::Picked(..)) => {
            "one of their folders is inside the other".to_owned()
        }
        // NAZWĄ KROKU, KTÓRY TĘ KOPIĘ NAZWAŁ, nie kluczem z pliku (niezmiennik 14): człowiek
        // widzi na płótnie nazwy, a klucza `s_make` nie ma tam ani razu. Zdanie „both work in
        // the project folder" byłoby tu nieprawdą — ci dwaj pracują w kopii, którą założył bieg,
        // i wysłanie człowieka do plików projektu każe mu szukać nie tam.
        (Spot::OwnCopy(owner), _) => match steps.get(owner) {
            Some(step) => format!("both work in the copy made for \"{}\"", step.name),
            None => "both work in the same copy".to_owned(),
        },
        _ => "both work in the project folder".to_owned(),
    }
}

/// Które kroki da się osiągnąć po strzałkach z którego.
///
/// Obchód iteracyjny, ze zbiorem odwiedzonych: plik z kołem ma się skończyć tak samo jak każdy
/// inny, a łańcuch dwudziestu kroków nie ma prawa przepełnić stosu.
fn reachable(count: usize, arrows: &[(usize, usize)]) -> Vec<Vec<bool>> {
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(from, to) in arrows {
        next[from].push(to);
    }

    let mut reach = vec![vec![false; count]; count];
    let mut stack: Vec<usize> = Vec::new();
    // Wiersz bierzemy z iteratora, a nie przez `reach[start]`: to ten sam obchód, tylko bez
    // indeksowania tablicy zmienną pętli, którego pełna bramka nie przepuszcza.
    for (start, from_here) in reach.iter_mut().enumerate() {
        stack.push(start);
        while let Some(step) = stack.pop() {
            for &after in &next[step] {
                if !from_here[after] {
                    from_here[after] = true;
                    stack.push(after);
                }
            }
        }
    }
    reach
}

/// Kroki, których nikt nie podłączył do reszty.
///
/// Obchód **ignoruje kierunek strzałek**. T3 §5.2 napisał wersję skierowaną, uruchomił ją
/// i nigdy nie wystrzeliła: w grafie bez kół obchód z każdego kroku bez wejść dociera zawsze
/// wszędzie. Wersja skierowana przepuszcza całą wyspę — dwa kroki połączone tylko ze sobą mają
/// po jednej strzałce, więc licznik strzałek też ich nie widzi.
///
/// Poziom to `Warning`, nie `Problem`: taki workflow wolno uruchomić, a wyspa bywa świadoma —
/// ktoś odłączył krok na chwilę i wróci do niego.
fn islands(steps: &[Facts<'_>], arrows: &[(usize, usize)], notes: &mut Vec<Note>) {
    let groups = groups(steps.len(), arrows);
    // Główny kawałek to największy, a przy remisie ten, który zaczyna się wcześniej w pliku.
    let Some((main, _)) = groups
        .iter()
        .enumerate()
        .max_by_key(|(position, members)| (members.len(), Reverse(*position)))
    else {
        return;
    };

    for (position, members) in groups.iter().enumerate() {
        if position == main {
            continue;
        }
        let names: Vec<&str> = members
            .iter()
            .filter_map(|&index| steps.get(index))
            .map(|step| step.name)
            .collect();
        let (Some(first), Some((leader, others))) = (members.first(), names.split_first()) else {
            continue;
        };
        notes.push(warning(
            steps.get(*first).map(|step| step.id),
            not_connected(leader, others),
        ));
    }
}

/// Kroki pogrupowane w kawałki połączone strzałkami, bez patrzenia na ich kierunek.
fn groups(count: usize, arrows: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(from, to) in arrows {
        neighbours[from].push(to);
        neighbours[to].push(from);
    }

    let mut group_of: Vec<Option<usize>> = vec![None; count];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..count {
        if group_of[start].is_some() {
            continue;
        }
        let number = groups.len();
        let mut members: Vec<usize> = Vec::new();
        group_of[start] = Some(number);
        stack.push(start);
        while let Some(step) = stack.pop() {
            members.push(step);
            for &neighbour in &neighbours[step] {
                if group_of[neighbour].is_none() {
                    group_of[neighbour] = Some(number);
                    stack.push(neighbour);
                }
            }
        }
        // Kolejnością w kawałku jest kolejność w pliku: uwaga ma nazwać ten krok, który
        // użytkownik zobaczy na płótnie pierwszy.
        members.sort_unstable();
        groups.push(members);
    }
    groups
}

/// Zdanie o kawałku, którego nikt nie podłączył. Nazywa krok jego **nazwą**, nie identyfikatorem.
fn not_connected(first: &str, others: &[&str]) -> String {
    match others {
        [] => format!("\"{first}\" is not connected to the rest of the workflow."),
        [second] => {
            format!("\"{first}\" and \"{second}\" are not connected to the rest of the workflow.")
        }
        more => format!(
            "\"{first}\" and {} more steps are not connected to the rest of the workflow.",
            more.len()
        ),
    }
}

#[cfg(test)]
mod tests {
    //! Granica pętli przyznana wprost: pętle ROZŁĄCZNE wolno, pętle o wspólnym kroku nie.
    //!
    //! # Dlaczego to jest odmowa, a nie ostrzeżenie
    //!
    //! `workflow::unroll` rozwija pętle rozłączne — każdy krok należy do najwyżej jednej z nich,
    //! więc „która to runda" ma jedną odpowiedź. Dla kroku wspólnego dwóm pętlom takiej odpowiedzi
    //! nie ma: nie wiadomo ani ile razy ma się powtórzyć, ani która jego runda wychodzi na
    //! zewnątrz. Bez tej reguły `unroll` musiałby jedną z pętli po cichu porzucić i bieg
    //! wyglądałby na udany, robiąc coś innego, niż narysował człowiek. Cicha zmiana znaczenia
    //! grafu jest gorsza od odmowy, która mówi, czego jeszcze nie umiemy.
    //!
    //! # Co się zmieniło 2026-08-22
    //!
    //! Do tego dnia odmawiany był KAŻDY drugi powrót. Kosztowało to kształt, o który poprosił
    //! właściciel i który jest zwykłym dniem pracy: jeden plan, dwie gałęzie, każda ze swoim
    //! sprawdzeniem i swoją poprawką. Kryterium `two_loops_side_by_side_are_fine` niżej pilnuje,
    //! żeby ta odmowa nie wróciła — bez niego regres wygląda dokładnie jak ostrożność.
    //!
    //! # Dlaczego kryterium stoi TUTAJ, a nie w `tests/it/`
    //!
    //! `checks/quick-scope.sh` przy ręcznym biegu bez `TASK.md` nie wpuszcza zapisu do
    //! `src-tauri/tests/`, a kryterium ma powstać razem z regułą, nie po niej. Wzorzec jest
    //! w repo (`ipc.rs`, `commands/run.rs`, `memory/handoff.rs`).
    //!
    //! # Słaba wersja
    //!
    //! Sprawdzenie „są dwa powroty, więc jest jakiś problem" przechodzi dla pliku, w którym problem
    //! zgłasza REGUŁA KOŁA — a wtedy kryterium świeci nad kodem, którego nie ma. Asercja stoi więc
    //! na treści zdania, i osobno na tym, że JEDEN powrót nie zgłasza niczego.

    use serde_json::{Value, json};

    use super::{Level, check_to_run};
    use crate::workflow::WorkflowFile;

    fn step(id: &str) -> Value {
        json!({
            "kind": "agent",
            "id": id,
            "name": id,
            "agent": "a",
            "instructions": "Do it.",
            "folder": { "use": "fresh-copy" }
        })
    }

    /// `Result`, nie `expect`: powód ten sam, co w `workflow::unroll::tests` — pełne clippy
    /// biegnie `-D warnings`, a `expect_used` i `panic` są w restrykcjach.
    fn file(links: &[Value]) -> Result<WorkflowFile, serde_json::Error> {
        serde_json::from_value(json!({
            "format": 1,
            "id": "wf",
            "name": "Test",
            "steps": [step("s_a"), step("s_b"), step("s_c")],
            "links": links
        }))
    }

    /// Zdania wagi problemu, w kolejności zgłoszenia.
    fn problems(file: &WorkflowFile) -> Vec<String> {
        check_to_run(file)
            .into_iter()
            .filter(|note| note.level == Level::Problem)
            .map(|note| note.message)
            .collect()
    }

    /// Plik z jednym krokiem — takim, jaki podaje wołający.
    fn one(only: &Value) -> Result<WorkflowFile, serde_json::Error> {
        serde_json::from_value(json!({
            "format": 1,
            "id": "wf",
            "name": "Test",
            "steps": [only.clone()],
            "links": []
        }))
    }

    /* 2026-08-23 — KAFELEK „URUCHOM I ZOSTAW" Z PUSTYM POLEM.
     *
     * Do tego dnia obie połowy [`a_command_step_left_empty`] stały za wspólnym
     * `let (Some(command), Some(proof)) = … else continue`, a ten kafelek dowodu CELOWO nie ma
     * (`facts`) — więc wypadał z reguły w całości. Pusty zapisywał się bez słowa i odmawiał
     * dopiero w środku biegu, po tym jak człowiek odczekał swoje na krokach przed nim.
     *
     * SŁABĄ WERSJĄ jest sam pierwszy przypadek: przechodzi go implementacja, która żąda od tego
     * kafelka także DOWODU — czyli pola, którego nie da się na nim sensownie wypełnić i którego
     * panel nie ma. Rozstrzyga drugi przypadek, w drugą stronę. */
    #[test]
    fn a_started_command_left_empty_is_refused_by_name() -> Result<(), serde_json::Error> {
        let empty = one(&json!({
            "kind": "serve",
            "id": "s_serve",
            "name": "Start the app",
            "command": "   ",
            "folder": { "use": "project" }
        }))?;

        assert_eq!(
            problems(&empty),
            vec![
                "\"Start the app\" does not say what to run, so there would be nothing to start. \
                 Write it in \"Command to run\" on the step."
                    .to_owned()
            ],
            "an empty tile is one sentence naming the field the person has to fill, and it has to \
             arrive at Save — not in the middle of a run that already cost twenty minutes"
        );
        Ok(())
    }

    #[test]
    fn a_started_command_is_never_asked_for_a_proof() -> Result<(), serde_json::Error> {
        let filled = one(&json!({
            "kind": "serve",
            "id": "s_serve",
            "name": "Start the app",
            "command": "npm run dev",
            "folder": { "use": "project" }
        }))?;

        assert!(
            problems(&filled).is_empty(),
            "this tile does not judge anything — it starts something and walks on — so there is \
             no output for a proof to match and no field on its panel to write one in. Got: {:?}",
            problems(&filled)
        );
        Ok(())
    }

    #[test]
    fn one_way_back_is_fine() -> Result<(), serde_json::Error> {
        let one = file(&[
            json!({ "from": "s_a", "to": "s_b" }),
            json!({ "from": "s_b", "to": "s_c" }),
            json!({ "from": "s_b", "to": "s_a", "max_turns": 3 }),
        ])?;

        assert!(
            problems(&one).is_empty(),
            "one loop is the whole feature; refusing it here would mean nobody can use it. \
             Got: {:?}",
            problems(&one)
        );
        Ok(())
    }

    #[test]
    fn two_loops_over_the_same_step_are_refused_by_name() -> Result<(), serde_json::Error> {
        // `s_b → s_a` powtarza a i b; `s_c → s_b` powtarza b i c. Wspólne jest b.
        let crossing = file(&[
            json!({ "from": "s_a", "to": "s_b" }),
            json!({ "from": "s_b", "to": "s_c" }),
            json!({ "from": "s_b", "to": "s_a", "max_turns": 3 }),
            json!({ "from": "s_c", "to": "s_b", "max_turns": 2 }),
        ])?;

        let said = problems(&crossing);

        assert!(
            said.iter()
                .any(|one| one.contains("send the work back over the same steps")),
            "the refusal has to say WHAT is wrong and what to do about it. A note about a circle \
             here would mean this rule is not running at all and the criterion is passing over \
             nothing. Got: {said:?}"
        );
        assert!(
            said.iter()
                .any(|one| one.contains("s_c") && one.contains("s_b")),
            "and it has to name BOTH steps the work goes back from — one name leaves the reader \
             hunting for the other half of the pair. Got: {said:?}"
        );
        Ok(())
    }

    #[test]
    fn two_loops_side_by_side_are_fine() -> Result<(), serde_json::Error> {
        /* Kształt z ekranu właściciela: jeden plan, dwie gałęzie, każda ze swoim sprawdzeniem
         * i swoją drogą powrotną. Ani jeden krok nie należy do obu pętli. */
        let branches: WorkflowFile = serde_json::from_value(json!({
            "format": 1,
            "id": "wf",
            "name": "Test",
            "steps": [
                step("s_plan"), step("s_front"), step("s_design"),
                step("s_back"), step("s_checked")
            ],
            "links": [
                { "from": "s_plan", "to": "s_front" },
                { "from": "s_front", "to": "s_design" },
                { "from": "s_plan", "to": "s_back" },
                { "from": "s_back", "to": "s_checked" },
                { "from": "s_design", "to": "s_front", "max_turns": 3 },
                { "from": "s_checked", "to": "s_back", "max_turns": 2 }
            ]
        }))?;

        assert!(
            problems(&branches).is_empty(),
            "two loops that share no step unroll on their own, and refusing them made the owner \
             throw away one of the two branches. Got: {:?}",
            problems(&branches)
        );
        Ok(())
    }
}
