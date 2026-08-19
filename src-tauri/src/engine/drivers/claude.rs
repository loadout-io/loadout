//! `ClaudeDriver` — jeden długo żyjący proces, dwukierunkowy stdin, wiele tur w jednej sesji.
//!
//! Zweryfikowane end-to-end na tej maszynie: proces zostaje przy życiu między turami, oddaje
//! ten sam `session_id`, przyjmuje przerwanie w paśmie i wychodzi 0, kiedy zamkniemy mu stdin
//! [T1 §2, §4.6, 2026-08-15]. Wariant awaryjny — nowy proces na turę z `--resume` — jest
//! legalny i za tym samym traitem, ale płaci zimny start i odbudowę cache'u przy **każdej**
//! turze [T1 §8.1]. To jest ten koszt, którego to zadanie ma uniknąć.
//!
//! # Trzy rzeczy, które w tym pliku wychodzą cicho źle. Wszystkie zmierzone.
//!
//! **1. Brak izolacji kontekstu.** Bez `--strict-mcp-config --setting-sources ""` jeden bieg
//! ładuje 73 narzędzia z 9 serwerów i pali **36 870** tokenów tworzenia cache'u zamiast
//! **4 725** [T1 §3.3, korekta 4, 2026-08-15]. Nic nie pęka — jest tylko drożej i wolniej, na
//! każdym kroku, na zawsze. `--tools ""` **nie wystarcza**: pierwszy bieg podał ją i `init`
//! dalej wymieniał wszystkie narzędzia `mcp__`.
//!
//! **2. `--bare`.** Vendor sam ją poleca i zapowiada jako przyszłą domyślną dla `-p`
//! [T1 §3.3, docs] — a ona **nigdy nie czyta OAuth ani keychaina** i tutaj wywaliła bieg na
//! `Not logged in · Please run /login`, `terminal_reason:"api_error"` [T1 §3.3, ran].
//! Użytkownik subskrypcji nie może jej użyć. Dlatego izolacja idzie dwiema flagami wyżej,
//! a nie tą jedną.
//!
//! **3. `subtype`.** Ten sam nieudany bieg przyszedł z `"subtype":"success"` przy
//! `"is_error":true` [T1 §4.4, potwierdzone ponownie]. Sterownik czytający `subtype` melduje
//! sukces kroku, który nie zrobił nic, a stożek poniżej rusza na pustym przekazaniu. Czytamy
//! `is_error` i `terminal_reason`; wyjście procesu jest sygnałem **drugorzędnym** [T1 §8.5].
//!
//! # Co ten plik posiada, a czego nie
//!
//! Tu mieszka wire enum Claude i mapowanie **linia → [`AgentEvent`]**. Zapis surowego
//! `agent-<id>.jsonl` i kuracja `AgentEvent` → `Line` należą do T-05 i stoją w `stream.rs`;
//! tutaj jest pętla, która czyta stdout **żywego** procesu i jedno i drugie **woła**. Ten
//! podział jest jedynym, przy którym `CodexDriver` (T-10) powstaje bez dotykania `stream.rs`.
//!
//! # Transkrypt kroku: tee i rodzaj narzędzia (T-34, 2026-08-16)
//!
//! Podział wyżej mówi, **czyj jest kod**, a nie kto go woła — i przez to na wyładowanym trunku
//! nie wołał go nikt. Pętla czytająca czytała stdout i nie zapisywała ani bajtu, a
//! `store::rebuild` czyta `logs/agent-<id>.jsonl` od T-06: po prawdziwym biegu skasowanie
//! `loadout.db` zabierało wtedy **wszystkie** zdarzenia, czyli dokładnie to, przed czym stoi
//! niezmiennik 4. Druga połowa tej samej luki siedziała w kuracji: żywa droga podawała
//! `tool: None`, więc wiersze `Read`, `Edited` i `Ran` powstawały wyłącznie w testach, które
//! podają `tool` ręcznie.
//!
//! Obie połowy domyka [`Transcript`] — katalog biegu z `docs/ARCHITECTURE.md` §8, krok,
//! którego to strumień, i kanał wierszy. Z nim [`pump`] pisze każdą przeczytaną linię przez
//! [`stream::Recorder`] **przed** parsowaniem i podaje kuratorowi fakty o narzędziu, które
//! [`stream::decode`] wyjmuje z tej samej linii drutu; bez niego zachowuje się dokładnie tak
//! jak przedtem, bo sonda wersji nie ma katalogu biegu.
//!
//! **Wołającego produkcyjnego ta wartość wciąż nie ma i to jest pytanie do człowieka, nie
//! przeoczenie.** Jej miejsce jest w [`RunSpec`] — wtedy dostaje ją każdy sterownik, także
//! `CodexDriver` — ale `drivers/mod.rs` i `commands/run.rs` nie leżą w bloku OWNS T-34,
//! a jeden wiersz poza tym blokiem jest pytaniem, nie cichym dopiskiem (`AGENTS.md` §7).
//! Dopóki `commands::run` nie zawoła [`ClaudeDriver::with_transcript`], mechanizm jest
//! kompletny i nieużywany.
//!
//! # Kanał stdinu żyje tak długo jak sesja (2026-08-16)
//!
//! Proces startuje z `StdinPlan::Keep`, więc deskryptor wejściowy **nie zamyka się po pierwszej
//! kopercie** — wraca tutaj z `Supervised::stdin()` i zostaje polem uchwytu. Tym jednym kanałem
//! idą trzy rzeczy i wszystkie trzy są kryterium: koperta pierwszej tury, koperta każdej
//! następnej ([`AgentHandle::send`]) i `control_request`/`interrupt` pierwszego stopnia
//! eskalacji ([`AgentHandle::cancel`]). EOF jest osobnym czasownikiem
//! ([`AgentHandle::close`]) i znaczy „koniec sesji", nigdy „koniec tury".

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use uuid::Uuid;

use super::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Policy, Probe,
    RunSpec, SessionRef, ToAgent, Tokens, Voice,
};
use crate::engine::line::Line;
use crate::engine::stream::{self, Recorder};
use crate::engine::supervisor::{self, DEFAULT_GRACE, GroupId, GroupProof, StdinPlan, Supervised};

/// Etykieta tego vendora — ta sama w [`SessionRef::vendor`] i w [`AgentDriver::id`].
pub const VENDOR: &str = "claude";

/// Czym woła się CLI, kiedy nikt nie podał własnej ścieżki.
///
/// Gołe „claude", nie ścieżka bezwzględna: na tej maszynie to skrypt powłoki, który znajduje
/// się przez `PATH` — a `PATH` jest jedną z sześciu zmiennych, które supervisor przepuszcza
/// przez `env_clear()` [T-03, `PASSTHROUGH`].
const DEFAULT_BINARY: &str = "claude";

/// Podkatalog biegu, w którym leżą surowe strumienie agentów (`docs/ARCHITECTURE.md` §8).
///
/// Ta nazwa i format `agent-<krok>.jsonl` obok niej są **kontraktem z `store::rebuild`**
/// (T-06), a nie wyborem tego pliku: odbudowa składa dokładnie tę ścieżkę i czyta ją po
/// nazwie. Rozjazd nie wygląda jak błąd — plik powstaje, odbudowa go nie znajduje i nikt się
/// o tym nie dowiaduje aż do pierwszego skasowania `loadout.db`.
const LOGS_DIR: &str = "logs";

/// Nazwa pliku ustawień biegu wewnątrz katalogu, który poda wołający.
///
/// Nazwa jest **nasza**, nie vendora: `.claude/settings.json` to kształt repo gospodarza, a ten
/// plik ma się nie dać pomylić z tamtym ani na dysku, ani w `ps`. Katalogu sterownik sobie nie
/// wybiera — dostaje go argumentem, bo artefakty biegu leżą w katalogu biegu
/// (`docs/ARCHITECTURE.md` §8), a nie w `$TMPDIR`.
const RUN_SETTINGS_FILE: &str = "claude-settings.json";

/// Wiersz transportu: cztery flagi, które decydują, **czym** jest to wywołanie.
///
/// `--verbose` nie jest ozdobą — bez niej CLI odmawia startu, dosłownie:
/// `Error: When using --print, --output-format=stream-json requires --verbose` [T1 §3.1, ran].
/// `--input-format stream-json` jest tą jedną flagą, dzięki której proces zostaje żywy między
/// turami; bez niej każda tura płaci zimny start i odbudowę cache'u [T1 §4.6, ran].
const TRANSPORT: [&str; 6] = [
    "-p",
    "--output-format",
    "stream-json",
    "--input-format",
    "stream-json",
    "--verbose",
];

/// Izolacja kontekstu, dwie flagi i **argument o zerowej długości**.
///
/// 2026-08-15 — bieg bez nich załadował 73 narzędzia MCP z 9 serwerów i spalił **36 870**
/// tokenów tworzenia cache'u zamiast **4 725** [T1 §3.3, korekta 4, ran]. Nic nie pęka; jest
/// tylko drożej i wolniej, na każdym kroku, na zawsze. `--tools ""` **nie wystarcza**: bieg,
/// który ją podał, dalej wymieniał w `init` wszystkie narzędzia `mcp__` [T1 §3.3, ran].
///
/// Wartość `--setting-sources` ma **zero znaków** i to jest cała różnica: `"user,project"`
/// w tym miejscu przechodzi każde sprawdzenie pytające o obecność flagi i nie izoluje niczego.
const LEAN_CONTEXT: [&str; 3] = ["--strict-mcp-config", "--setting-sources", ""];

/// `subtype` linii `system`, która ogłasza sesję, model, narzędzia i zdolności [T1 §4.1].
const INIT: &str = "init";

/// `subtype` linii, która znaczy „model myśli" — i nic poza tym.
const THINKING_TOKENS: &str = "thinking_tokens";

/// `subtype` linii o ponowieniu zapytania do dostawcy [T1 §4.5, docs].
const API_RETRY: &str = "api_retry";

/// Po tym prefiksie `subtype` poznajemy, że linia `result` opisuje błąd — używane **wyłącznie**
/// wtedy, gdy vendor nie dosłał `is_error`.
const ERROR_PREFIX: &str = "error";

/// Po tym prefiksie poznajemy sufit: `error_max_turns` i cokolwiek, co vendor dołoży obok.
const CEILING_PREFIX: &str = "error_max";

/// `terminal_reason` tury zdjętej przerwaniem.
const CANCELLED: &str = "cancelled";

/// Zdolność, pod którą — i **tylko** pod którą — wolno wysłać przerwanie w paśmie.
///
/// Feature-detekcja idzie po tej liście z `system/init`, nigdy po numerze wersji [T1 §4.1,
/// §4.6]. Sam protokół `control_request` jest nieudokumentowany i zweryfikowany wyłącznie
/// eksperymentem, więc jedyną uczciwą przesłanką jest to, co CLI o sobie ogłosiło.
const INTERRUPT_CAPABILITY: &str = "interrupt_receipt_v1";

/// Ile czekamy, aż sesja poproszona o przerwanie skończy się **sama** [T1 §8.5, stopień 1].
///
/// Po tym oknie schodzimy na stopień drugi. Wysłanie przerwania tam, gdzie CLI go nie obsługuje,
/// kosztowałoby dokładnie te pięć sekund czekania na odpowiedź, która nie przyjdzie — dlatego
/// stopnia pierwszego w ogóle nie zaczynamy bez [`INTERRUPT_CAPABILITY`].
const INTERRUPT_WINDOW: Duration = Duration::from_secs(5);

/// Ile znaków wolno mieć jednolinijkowemu podsumowaniu, zanim zostanie przycięte. Pełne wyjście
/// i tak zostaje za kliknięciem — to jest linia w wierszu, nie dokument.
const SUMMARY_LIMIT: usize = 120;

/// Ile wyników tury mieści się w kanale między pętlą czytającą a [`AgentHandle::wait`].
///
/// Tura jest jedna naraz, więc jeden slot wystarczyłby — ale wynik, który nie ma gdzie wejść,
/// zatrzymuje pętlę czytającą, a zatrzymana pętla wygląda dokładnie jak zawieszony agent.
/// Zapas jest tańszy niż to rozróżnienie w zgłoszeniu błędu.
const TURNS_IN_FLIGHT: usize = 8;

/// Cała tabela tłumaczenia polityki na flagi vendora — **jedna, w adapterze** (niezmiennik 23).
///
/// Zwraca tryb uprawnień i listę dozwolonych narzędzi; `None` w drugim polu znaczy „nie wysyłaj
/// `--allowedTools` w ogóle".
///
/// **`Unrestricted` nie dostaje listy i to nie jest przeoczenie.** Lista dozwolonych narzędzi
/// nie ogranicza `bypassPermissions` — wszystko jest zatwierdzone niezależnie od niej
/// [T1 §5.2]. Wysłanie obu naraz to kłamstwo o tym, co jest ograniczone: w argv widać listę,
/// w rzeczywistości nie obowiązuje nic, a kto czyta `ps` albo dziennik, ten uwierzy liście.
///
/// Żaden wariant nie brzmi `default`: CLI 2.1.233 przyjmuje tę nazwę w czasie wykonania, ale
/// **nie wymienia jej** we własnym komunikacie odrzucenia (`acceptEdits, auto,
/// bypassPermissions, manual, dontAsk, plan`), a dokumentacja nazywa `manual` jej aliasem
/// [T1 korekta 10]. Opieranie się na nazwie, której własne CLI nie przyznaje, to jedna wersja
/// od cichego „unknown option".
///
/// Cicha wersja złamania niezmiennika 23 nie wygląda jak drugi adapter — wygląda jak
/// `if agent == "claude" { … }` w miejscu wywołania, i tak właśnie po cichu umarło skanowanie
/// sekretów w repo źródłowym [raport 05 §4].
const fn permission_flags(policy: Policy) -> (&'static str, Option<&'static str>) {
    match policy {
        Policy::ReadOnly => ("dontAsk", Some("Read,Grep,Glob")),
        // `Bash(git *)` to git i **tylko** git; gołe `Bash` byłoby każdą komendą na maszynie.
        Policy::EditInFolder => ("acceptEdits", Some("Read,Grep,Glob,Edit,Write,Bash(git *)")),
        Policy::Unrestricted => ("bypassPermissions", None),
    }
}

/// Druga kolumna tej samej decyzji, co [`permission_flags`]: **co w ogóle jest w zestawie**.
///
/// Stoi obok tamtej tabeli, a nie zamiast niej, bo to są dwie różne flagi o dwóch różnych
/// znaczeniach. `--allowedTools` jest listą **auto-zatwierdzania**: narzędzie spoza niej dalej
/// jest w zestawie, tylko zapyta. `--tools` jest twardą listą **dostępności** — czego na niej
/// nie ma, tego proces nie ma pod ręką [zmierzone 2026-08-19].
///
/// # Dlaczego biała, a nie czarna [2026-08-19]
///
/// W biegu bez człowieka „zapyta" nie znaczy „nie zrobi". Zmierzone: agent Loadouta wywołał
/// **projektowego podagenta repo gospodarza**, ten wystartował jako osobny proces i spalił
/// **38–41 tys. tokenów** całkowicie poza widokiem i rozliczeniem Loadouta. Ani jednej
/// czerwieni, ani jednego wiersza na ekranie pracy, ani jednego dolara w podsumowaniu kroku.
///
/// Domyślna powierzchnia ma dziś **osiem ścieżek startu procesu** — `Task`, `Workflow`,
/// `SendMessage`, `CronCreate`, `RemoteTrigger`, `ScheduleWakeup`, `EnterWorktree`, `Monitor` —
/// a każda z nich startuje proces **poza naszą grupą**, czyli poza dowodem śmierci
/// z niezmiennika 6. Lista rzeczy zakazanych dostaje dziurę przy najbliższym wydaniu CLI, po
/// cichu, bo nikt nie czyta changelogu pod kątem „czy przybyło czasowników". Lista rzeczy
/// **dozwolonych** dziury nie dostaje: nowe narzędzie po prostu na nią nie wchodzi.
///
/// Żaden wariant nie ma prawa oddać ani pustej listy, ani `default`. To są dwa słowa vendora
/// o dwóch skrajnościach — `""` znaczy „żadnych narzędzi", `default` znaczy „wszystkie" —
/// i żadna polityka po ludzku nie znaczy żadnej z nich.
///
/// # Trzy szczeble, a każdy dokłada inny RODZAJ zasięgu
///
/// Nie trzy różne listy dobrane do smaku, tylko jedna drabina, w której każdy stopień jest
/// zdaniem, które ta polityka obiecuje na ekranie:
///
/// | Polityka | Na ekranie | Co dokłada ponad poprzedni szczebel |
/// |---|---|---|
/// | [`Policy::ReadOnly`] | „Read only" | czytanie i szukanie w repo |
/// | [`Policy::EditInFolder`] | „Can edit this folder" | zmienianie tego repo (`Edit`, `Write`, `Bash`) |
/// | [`Policy::Unrestricted`] | „No limits" | sięganie POZA repo (`WebFetch`, `WebSearch`) |
///
/// Zawierania są **ostre w obie strony i to jest asercja o zachowaniu**: agent obiecany jako
/// czytający nie ma prawa mieć pod ręką `Write` ani `Edit`, a agent bez ograniczeń nie ma prawa
/// mieć **mniej** niż ten, który edytuje folder. Adapter wypisujący jedną i tę samą listę
/// wszystkim trzem jest dokładnie tą pomyłką, którą T-04 nazwało już raz przy
/// `--permission-mode`.
///
/// `Bash` stoi tu gołe, a w [`permission_flags`] tej samej polityki stoi `Bash(git *)` — i to
/// nie jest rozjazd, tylko cały podział między tymi dwiema flagami. `--tools` mówi „narzędzie
/// jest w zestawie", `--allowedTools` mówi „ta jego część idzie bez pytania". Składnia zakresowa
/// należy do drugiej z nich; w pierwszej jest tylko nazwa.
///
/// # Dziesięć nazw, których tu nie ma, i ile kosztowała każda z nich [2026-08-19]
///
/// `Task`, `Workflow`, `SendMessage`, `CronCreate`, `RemoteTrigger`, `ScheduleWakeup`,
/// `EnterWorktree`, `Monitor` — każda z tych ośmiu startuje proces **poza naszą grupą**, czyli
/// poza dowodem śmierci z niezmiennika 6: dowód zostaje prawdziwy i przestaje cokolwiek znaczyć,
/// bo to nie ta grupa. `Agent` i `Skill` to ta sama czynność pod inną nazwą u tego samego
/// vendora. Zmierzone: jedno takie wywołanie — projektowy podagent repo gospodarza — spaliło
/// **38–41 tys. tokenów** całkowicie poza widokiem i rozliczeniem Loadouta. Ani jednej
/// czerwieni, ani jednego wiersza na ekranie pracy, ani jednego dolara w podsumowaniu kroku.
///
/// Ich nieobecność jest tu **skutkiem ubocznym**, nie regułą: lista zakazów dostałaby dziurę
/// przy najbliższym wydaniu CLI, po cichu, bo nikt nie czyta changelogu pod kątem „czy przybyło
/// czasowników". Na tę listę nowe narzędzie po prostu nie wchodzi.
#[must_use]
pub const fn tools_for(policy: Policy) -> &'static [&'static str] {
    match policy {
        Policy::ReadOnly => &["Read", "Grep", "Glob"],
        Policy::EditInFolder => &["Read", "Grep", "Glob", "Edit", "Write", "Bash"],
        Policy::Unrestricted => &[
            "Read",
            "Grep",
            "Glob",
            "Edit",
            "Write",
            "Bash",
            "WebFetch",
            "WebSearch",
        ],
    }
}

/// Dokąd idzie transkrypt kroku i kto dostaje jego wiersze.
///
/// Trzy fakty, których sterownik nie ma skąd wziąć sam, i ani jednego więcej: [`RunSpec`] nie
/// niesie żadnego z nich, a katalog biegu zna wyłącznie warstwa nad silnikiem.
///
/// **Ścieżkę pliku składa sterownik, nigdy wołający.** `logs/agent-<krok>.jsonl` to ta sama
/// nazwa, którą składa `store::rebuild` (T-06) — dwa miejsca składające ją osobno rozjadą się
/// po cichu, a wtedy plik powstaje, odbudowa go nie znajduje i nikt się o tym nie dowiaduje aż
/// do pierwszego skasowania `loadout.db`. Dlatego tu stoi katalog biegu i identyfikator kroku,
/// a nie gotowa ścieżka: kryterium ma czego pilnować, a wołający nie ma czego pomylić.
#[derive(Debug, Clone)]
pub struct Transcript {
    /// Katalog biegu z `docs/ARCHITECTURE.md` §8: `<repo>/.loadout/runs/<ts>__<id>/`.
    pub run_dir: PathBuf,
    /// Krok, którego to strumień — jego `id` z `run.json`, bo po nim nazywa się plik i po nim
    /// odbudowa wie, do którego kroku należą zdarzenia.
    pub step: String,
    /// Nazwa agenta, która wchodzi w każdy wiersz. Wchodzi też w klucz grupy sklejania, więc
    /// dwa agenty czytające pliki w tej samej sekundzie to dwa wiersze, nie jeden.
    pub agent: String,
    /// Wiersze na ekran. Ścieżka dysku nie gubi nigdy, ścieżka widoku wolno gubić [T7 §4.1] —
    /// zamknięty odbiornik nie ma prawa zatrzymać zapisu.
    pub lines: mpsc::Sender<Line>,
}

impl Transcript {
    /// Otwiera plik kroku i oddaje ujście, którym pojedzie strumień.
    ///
    /// Katalogu `logs/` **nie zakłada**: powstaje on razem z katalogiem biegu, w warstwie,
    /// która zna układ z `docs/ARCHITECTURE.md` §8. Sterownik ma tam dopisać plik, a nie
    /// wymyślać sobie własne miejsce — wymyślone byłoby miejscem, w którym odbudowa nie
    /// szuka, a to wygląda dokładnie tak samo jak brak zapisu.
    async fn open(&self) -> anyhow::Result<Recorder> {
        let path = self
            .run_dir
            .join(LOGS_DIR)
            .join(format!("agent-{}.jsonl", self.step));
        Recorder::create(&path, self.agent.clone(), self.lines.clone())
            .await
            .with_context(|| {
                format!(
                    "the step could not open its transcript at {}",
                    path.display()
                )
            })
    }
}

/// Plik ustawień, który piszemy **my** — i jedyny, jaki ten bieg w ogóle wczyta.
///
/// # Dlaczego nie wczytujemy pliku gospodarza [zmierzone 2026-08-19]
///
/// Z repo gospodarza dziedziczymy **tekst**, nigdy **maszynerię**. `--setting-sources ""`
/// odcina jego `.claude/settings.json` w całości i jest **jedyną** dźwignią, która gasi jego
/// haki: hak `PreToolUse` gospodarza startuje proces we **własnej** grupie, jego dziecko
/// dostaje `ppid=1` i **przeżywa wyjście `claude`** (jeden bieg zostawił 14 sierot,
/// eksperymenty łącznie 30). Krok się kończy, dowód śmierci grupy z niezmiennika 6 jest
/// prawdziwy — i nie dotyczy procesu, który nigdy nie był w naszej grupie. Nic nie pęka,
/// bramka jest zielona, a sierota pali limit w tle.
///
/// Odcięte zostaje jednak także to, co gospodarz naprawdę chciał **zabronić**. Wraca do nas
/// wyłącznie jako napis, przepisany do tego pliku: `--settings` **działa samodzielnie** przy
/// `--setting-sources ""` i egzekwuje przepisane `permissions.deny`. Sam z siebie izolacją
/// **nie jest** — sumuje się z projektowym i nie gasi hooków, nawet podany z pustą listą
/// `PreToolUse` — więc jest nośnikiem naszego `deny` i niczym więcej.
///
/// # Jeden klucz, i drugi jest nowym kryterium, nie łatką
///
/// Cztery pozostałe pola gospodarza **rozszerzyłyby** nas, nie ograniczyły:
/// `permissions.allow` to cudza polityka, `env` **nadpisuje** środowisko podane przez Loadouta
/// (czyli przewraca `env_clear()` z niezmiennika 9 od zewnątrz), `sandbox` z
/// `autoAllowBashIfSandboxed` przepuszcza **dowolną** komendę mimo białej listy narzędzi,
/// a `hooks` to ta grupa procesów, której nie zabijemy. Żadne z nich nie przechodzi przez
/// przepisanie — nigdy.
///
/// Czytelnik tego pliku jest dokładnie **jeden**: proces, który startujemy. Jeżeli powstaje,
/// a `--settings` nie stoi w argv z **jego** ścieżką, to jest śmieć w katalogu biegu
/// i jednocześnie cała izolacja, której nie ma (niezmiennik 21).
#[derive(Debug, Clone)]
pub struct RunSettings {
    /// Gdzie ten plik leży. Ta sama ścieżka, która ma stanąć obok `--settings` w argv.
    path: PathBuf,
}

/// Cały dokument, który idzie na dysk — i **jedyny kształt**, w jakim może iść.
///
/// Typ zamiast `serde_json::json!` jest tu decyzją, nie gustem: „jeden klucz" przestaje być
/// obietnicą w komentarzu i staje się faktem o typie. Do struktury z jednym polem nie da się
/// dopisać `env` ani `hooks` **przez pomyłkę** — a przepisanie hurtem cudzego obiektu
/// `permissions` jest właśnie taką pomyłką, która przechodzi każdy test pytający wyłącznie
/// o zawartość `deny`.
#[derive(Debug, Serialize)]
struct SettingsDocument<'a> {
    permissions: DenyOnly<'a>,
}

/// Jedyne pole gospodarza, które przechodzi przez granicę — i jedyne, które ta struktura zna.
#[derive(Debug, Serialize)]
struct DenyOnly<'a> {
    /// Reguły w **podanej kolejności**: `&[String]` serializuje się jako tablica JSON, więc
    /// kolejność jest tu tą samą kolejnością, którą podał wołający.
    deny: &'a [String],
}

impl RunSettings {
    /// Zapisuje plik ustawień biegu w **podanym** katalogu i oddaje uchwyt do niego.
    ///
    /// Reguły przychodzą gotowe — przepisane z gospodarza przez [`super::host::deny_rules`] —
    /// i wchodzą do dokumentu **w podanej kolejności**, bo lista odmów czytana przez człowieka
    /// przetasowana po drodze jest listą, której nikt nie potrafi zweryfikować.
    ///
    /// Katalogu **nie zakłada** i miejsca sobie **nie wybiera**, dokładnie jak
    /// [`Transcript::open`]: katalog biegu powstaje w warstwie, która zna układ
    /// z `docs/ARCHITECTURE.md` §8, a sterownik ma tam dopisać plik. Wymyślone miejsce byłoby
    /// `$TMPDIR`, czyli artefaktem biegu poza biegiem.
    pub fn write(dir: &Path, deny: &[String]) -> anyhow::Result<Self> {
        let path = dir.join(RUN_SETTINGS_FILE);

        let document = SettingsDocument {
            permissions: DenyOnly { deny },
        };
        let text = serde_json::to_string_pretty(&document)
            .context("the rewritten deny rules could not be turned into a settings document")?;

        std::fs::write(&path, text).with_context(|| {
            format!(
                "the run settings file could not be written at {}",
                path.display()
            )
        })?;

        Ok(Self { path })
    }

    /// Ścieżka zapisanego pliku — ta sama, którą dostaje `--settings`.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Sterownik `claude`.
///
/// Ścieżka do binarki jest **polem**, nie stałą, i to jest jedyny szew, przez który kryteria
/// AC-6 i AC-7 wpuszczają skrypt-atrapę zamiast prawdziwego CLI. Atrapa loguje **obok
/// siebie**, nigdy przez zmienną środowiskową: supervisor robi `env_clear()`, więc fikstura
/// sterowana envem po cichu przestałaby działać.
#[derive(Debug, Clone)]
pub struct ClaudeDriver {
    /// Co uruchamiamy.
    binary: PathBuf,
    /// Dokąd zapisać surowy strumień i komu oddać wiersze. `None` znaczy „tego biegu nikt nie
    /// zapisuje": sonda wersji nie ma katalogu biegu, a kryteria samego sterownika (T-04) pytają
    /// o zdarzenia, nie o transkrypt.
    transcript: Option<Transcript>,
    /// Plik ustawień tego biegu, czyli jedyna droga, którą reguła gospodarza do nas wraca.
    /// `None` znaczy „ten bieg go nie ma": sonda wersji nie ma katalogu biegu, więc nie ma
    /// gdzie go położyć, a `--settings` bez pliku pod podaną ścieżką zabiłoby CLI.
    settings: Option<RunSettings>,
    /// Gotowy fragment argv przyniesiony przez warstwę wyżej — nic więcej.
    ///
    /// `Vec<String>`, nie `Option<PathBuf>` i nie żaden typ mówiący „umiejętność": ten plik nie
    /// ma prawa wiedzieć, czym jest dziedziczenie ani kiedy flagę wolno postawić (niezmiennik
    /// 23). Puste znaczy „nie było czego odziedziczyć" i rozstrzygnął to `inherit::wire`, nie my.
    inherited: Vec<String>,
}

impl Default for ClaudeDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeDriver {
    /// Sterownik wołający `claude` z `PATH`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            binary: PathBuf::from(DEFAULT_BINARY),
            transcript: None,
            settings: None,
            inherited: Vec::new(),
        }
    }

    /// Sterownik wołający konkretny plik. Szew dla kryteriów, które uruchamiają prawdziwy
    /// proces — i dla użytkownika, który trzyma CLI poza `PATH`.
    #[must_use]
    pub fn with_binary(binary: PathBuf) -> Self {
        Self {
            binary,
            transcript: None,
            settings: None,
            inherited: Vec::new(),
        }
    }

    /// Sterownik, który **zapisuje** surowy strumień kroku i oddaje jego wiersze.
    ///
    /// Bez tego bieg nie tee'uje w ogóle: zmierzone na wyładowanym trunku 2026-08-16 —
    /// `logs/agent-<id>.jsonl` nie powstaje po żadnym biegu, więc `store::rebuild` czyta plik,
    /// którego nikt nie pisze (niezmiennik 21 czytany od drugiej strony), a skasowanie
    /// `loadout.db` kosztuje wszystkie zdarzenia (niezmiennik 4).
    ///
    /// Wartość, nie mutacja, i to nie jest kwestia gustu: transkrypt jest **per krok**, a
    /// sterownik bywa jeden na vendora, więc jedyny bezpieczny kształt to tani klon z własnym
    /// ujściem. Nadpisanie pola we współdzielonym sterowniku przepięłoby transkrypt biegu,
    /// który akurat trwa, i wyglądałoby to jak zgubione linie, a nie jak wyścig.
    #[must_use]
    pub fn with_transcript(mut self, transcript: Transcript) -> Self {
        self.transcript = Some(transcript);
        self
    }

    /// Sterownik, który każe procesowi wczytać **nasz** plik ustawień — i żaden inny.
    ///
    /// Budowniczy przez wartość, dokładnie jak [`ClaudeDriver::with_transcript`], i z tego
    /// samego powodu: plik ustawień jest **per bieg**, a sterownik bywa jeden na vendora, więc
    /// jedyny bezpieczny kształt to tani klon z własną ścieżką. To jest też powód, dla którego
    /// nie ma tu nowego pola w [`RunSpec`]: tę strukturę konstruuje literałem trzynaście plików
    /// spoza tego zadania, a nowe pole wywróciłoby kompilację ich wszystkich.
    ///
    /// # Wołacza produkcyjnego ten szew nie ma i to jest pytanie do człowieka (T-53)
    ///
    /// Kształt jest ten sam co przy [`ClaudeDriver::with_transcript`] z T-34: mechanizm jest
    /// kompletny i **nieużywany**, dopóki człowiek go nie zepnie. Wołaczem jest `commands/run.rs`,
    /// a on **nie leży w bloku OWNS T-53** — jeden wiersz poza tym blokiem jest pytaniem, nie
    /// cichym dopiskiem (`AGENTS.md` §7).
    ///
    /// Proponowane miejsce: `plan_agent` w `commands/run.rs`. Tam, i tylko tam, znane są
    /// **naraz** obie ścieżki — katalog roboczy kroku (`cwd` policzone przez `workspace`; dla
    /// `Folder::Project` to `setup.project`, a dla `fresh-copy` kopia pod katalogiem biegu) oraz
    /// katalog biegu (`setup.dir`, ten sam, który dostaje `lay_out_the_run_dir`). Trzy wiersze,
    /// w tej kolejności:
    ///
    /// 1. `let deny = super::host::deny_rules(&cwd);`
    /// 2. `let settings = RunSettings::write(setup.dir, &deny)?;` — katalog biegu, nigdy
    ///    `$TMPDIR` (`docs/ARCHITECTURE.md` §8); zapis idzie **przed** startem procesu, bo
    ///    `--settings` bez pliku zabija CLI dopiero w produkcji (niezmiennik 21).
    /// 3. `.with_settings(settings)` na sterowniku, **zanim** ktokolwiek zawoła
    ///    [`ClaudeDriver::command`] — flaga wchodzi do argv wyłącznie stąd.
    ///
    /// Otwarte pytanie tej propozycji i drugi powód, dla którego decyzja należy do człowieka:
    /// fabryka z `lib.rs` oddaje `Arc<dyn AgentDriver>` raz na aplikację, więc w `plan_agent`
    /// konkretny typ jest już zgubiony, a ten budowniczy żyje na [`ClaudeDriver`], nie na
    /// traicie. Wpięcie potrzebuje **albo** fabryki wołanej per bieg, **albo** tej samej
    /// odpowiedzi, której T-34 nie dostało dla transkryptu. Obie zmieniają plik spoza OWNS.
    #[must_use]
    pub fn with_settings(mut self, settings: RunSettings) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Sterownik, który dopisuje do argv **gotowy** fragment przyniesiony przez warstwę wyżej.
    ///
    /// LISTA FLAG, NIE WIEDZA O DZIEDZICZENIU (niezmiennik 23). Ten plik nie ma prawa wiedzieć,
    /// skąd ten fragment się wziął, czym jest „umiejętność gospodarza" ani kiedy `--plugin-dir`
    /// wolno postawić: to rozstrzyga `inherit::wire` i rozstrzyga **raz**. Adapter, który zna
    /// drugą połowę tej reguły, jest dokładnie tym drugim zestawem reguł, przez który w repo
    /// źródłowym po cichu umarło skanowanie sekretów [raport 05 §4].
    ///
    /// Pusty fragment to **brak flagi**, nie flaga z pustą wartością. Rozróżnienie jest
    /// zmierzone i kosztowne w obie strony: `--setting-sources ""` w tym samym argv jest flagą,
    /// której pusty argument jest poprawny, a `--plugin-dir` bez wartości połknąłby następną
    /// flagę sterownika jako swój argument. Dlatego stąd nie wychodzi ani jedna decyzja o tym,
    /// co znaczy „nie ma czego odziedziczyć" — wychodzi tylko to, co przyszło.
    ///
    /// Budowniczy przez wartość, dokładnie jak [`ClaudeDriver::with_settings`] i z tego samego
    /// powodu: dziedziczenie jest **per bieg**, a sterownik bywa jeden na vendora, więc jedyny
    /// bezpieczny kształt to tani klon z własnym fragmentem.
    #[must_use]
    pub fn with_inherited(mut self, flags: Vec<String>) -> Self {
        self.inherited = flags;
        self
    }

    /// Buduje komendę jednej tury. **Promptu w niej nie ma i nigdy nie będzie**
    /// (niezmiennik 9): treść zadania jedzie kopertą na stdin, bo argumenty widzi `ps`
    /// każdego użytkownika maszyny.
    ///
    /// Linia argv w wersji wiążącej [T1 §8.3, `docs/ARCHITECTURE.md` §4]:
    ///
    /// | Fragment | Dlaczego dokładnie tak |
    /// |---|---|
    /// | `-p` | brama do wszystkiego poniżej |
    /// | `--output-format stream-json` | zdarzenia, nie bajty terminala |
    /// | `--input-format stream-json` | dwukierunkowy stdin: proces zostaje na wiele tur |
    /// | `--verbose` | bez niej CLI **odmawia**: `Error: When using --print, --output-format=stream-json requires --verbose` [T1 §3.1] |
    /// | `--session-id <run_id>` \| `--resume <id>` | dokładnie jedno z dwóch, nigdy oba |
    /// | `--strict-mcp-config` | 73 narzędzia z 9 serwerów zostają za drzwiami [T1 korekta 4] |
    /// | `--setting-sources ""` | argument o **zerowej długości**; `"user,project"` w tym miejscu to izolacja, która nie działa |
    /// | `--settings <ścieżka>` | [`RunSettings`], jeśli ten bieg go ma: nośnik przepisanego `deny`, **nie** izolacja — sumuje się z projektowym i nie gasi hooków [2026-08-19] |
    /// | `--permission-mode` + `--allowedTools` | z [`super::Policy`], jedną tabelą (niezmiennik 23) |
    /// | `--tools <lista>` | twarda biała lista **dostępności** z [`tools_for`]: czego na niej nie ma, tego proces nie ma pod ręką [2026-08-19] |
    ///
    /// Czego tu **nie ma**: `--bare` (wywala subskrypcję [T1 §3.3]), `--max-turns`
    /// i `--max-budget-usd` (spike S-2 nierozstrzygnięty [`docs/ARCHITECTURE.md` §11]).
    #[must_use]
    pub fn command(&self, spec: &RunSpec) -> Command {
        let mut command = Command::new(&self.binary);

        // Katalog roboczy przychodzi ARGUMENTEM, nigdy stałą: literał ze ścieżką repo w pliku
        // pod `engine/` przewraca granicę z niezmiennika 1, bo `checks/quick-boundary.sh`
        // gerpuje `-i tauri` po niekomentowanych liniach, a każda nasza ścieżka zaczyna się
        // od `src-tauri/`.
        command.current_dir(&spec.cwd);

        command.args(TRANSPORT);

        // Dokładnie jedno z dwóch, nigdy oba: to są dwie różne sesje, a CLI musiałoby zgadnąć,
        // która wygrywa. Sesję świeżego biegu nadajemy MY, zanim proces wystartuje — dopiero
        // to znosi wyścig o to, pod jakim numerem zapisać krok [T1 §4.6, T7 §6.2].
        match &spec.resume {
            None => {
                command.arg("--session-id").arg(spec.run_id.to_string());
            }
            Some(session) => {
                command.arg("--resume").arg(&session.id);
            }
        }

        command.args(LEAN_CONTEXT);

        // NOŚNIK NASZEGO `deny` I NIC POZA TYM. `--settings` **sumuje się** z ustawieniami
        // projektowymi i **nie gasi** hooków, nawet podany z pustą listą `PreToolUse`
        // [zmierzone 2026-08-19] — izolacją jest wyłącznie `--setting-sources` o zerowej
        // długości argumentu, dwie linie wyżej. Dlatego ta flaga stoi TUŻ ZA tamtą i nigdzie
        // indziej: kto ją zobaczy, ma najpierw przeczytać, że tamta zostaje jedna.
        //
        // Cicho łamie się to jednym dopiskiem: ktoś stawia `--settings` i „dla pewności, żeby
        // się wczytał" dokłada drugie `--setting-sources project`. Wtedy wraca CAŁY plik
        // gospodarza — jego hak `PreToolUse` startuje proces we własnej grupie, dziecko dostaje
        // `ppid=1` i przeżywa wyjście `claude` (30 sierot w eksperymentach), a każde sprawdzenie
        // pytające o OBECNOŚĆ flagi zostaje zielone.
        //
        // Ścieżka, nigdy JSON w argumencie: `--settings` przyjmuje jedno i drugie, a treść
        // w argv widzi `ps` każdego użytkownika maszyny (niezmiennik 9 także wtedy, gdy nie
        // chodzi o prompt). `None` znaczy „ten bieg nie ma katalogu, w którym mógłby ten plik
        // leżeć" — tak wygląda sonda wersji — i wtedy flagi nie ma w ogóle, bo `--settings`
        // wskazujące nieistniejący plik zabija CLI przy starcie.
        if let Some(settings) = &self.settings {
            command.arg("--settings").arg(settings.path());
        }

        // FRAGMENT PRZYSZEDŁ GOTOWY I WCHODZI GOTOWY. Ani jednego warunku nad nim: „czy jest co
        // odziedziczyć" rozstrzyga `inherit::wire` i rozstrzyga raz (niezmiennik 23). Pusty
        // fragment to po prostu zero argumentów — nie flaga z pustą wartością, bo `--plugin-dir`
        // bez wartości połknąłby następną flagę jako swój argument. Kształt „pusty argument jest
        // poprawny" stoi w tym samym argv dwie linie wyżej (`--setting-sources ""`) i pomylenie
        // tych dwóch nie wygląda jak błąd: proces startuje, tylko z wyjedzoną flagą.
        //
        // Stoi TUŻ ZA `--settings`, bo to jedna rodzina: oba wskazują coś, co napisaliśmy sami
        // w katalogu tego biegu. Z repo gospodarza jedzie tu wyłącznie ŚCIEŻKA — jego treść
        // (`## Recurring patterns`, ciało podagenta) jedzie promptem i nigdy argv, bo argumenty
        // widzi `ps` każdego użytkownika maszyny (niezmiennik 9).
        command.args(&self.inherited);

        // Jedna tabela, jedno miejsce (niezmiennik 23). `None` znaczy „nie wysyłaj listy",
        // a nie „wyślij pustą": pusta lista i brak listy to dla CLI dwie różne rzeczy.
        let (mode, tools) = permission_flags(spec.policy);
        command.arg("--permission-mode").arg(mode);
        if let Some(tools) = tools {
            command.arg("--allowedTools").arg(tools);
        }

        // Druga kolumna tej samej decyzji, nie druga decyzja (niezmiennik 23): wyżej stoi to,
        // co idzie bez pytania, tutaj to, co w ogóle jest w zestawie. Jedno wystąpienie flagi
        // i jeden argument z przecinkami — tak samo jak `--allowedTools`, tak samo jak mówi
        // `claude --help`. Bez tej linii cała tabela wyżej jest napisem: `--allowedTools`
        // to lista AUTO-ZATWIERDZANIA, a narzędzie spoza niej dalej jest pod ręką, tylko
        // zapyta — i w biegu bez człowieka „zapyta" nie znaczy „nie zrobi" [2026-08-19].
        command.arg("--tools").arg(tools_for(spec.policy).join(","));

        if let Some(model) = &spec.model {
            command.arg("--model").arg(model);
        }

        // KONFIGURACJA agenta, nie treść zadania. Treść zadania w tym polu byłaby
        // niezmiennikiem 9 złamanym po cichu: stąd wchodzi do argv, a argv widzi `ps` każdego
        // użytkownika maszyny.
        if let Some(append) = &spec.system_append {
            command.arg("--append-system-prompt").arg(append);
        }

        for dir in &spec.extra_dirs {
            command.arg("--add-dir").arg(dir);
        }

        // Promptu tu nie ma i nigdy nie będzie (niezmiennik 9). Jedzie kopertą na stdin.
        //
        // Nie ma tu też `--bare` (nigdy nie czyta OAuth ani keychaina; na tej maszynie wywaliła
        // bieg na `Not logged in · Please run /login` z `terminal_reason:"api_error"`
        // [T1 §3.3, ran]), ani `--max-turns` / `--max-budget-usd` — spike S-2 nie rozstrzygnął
        // sprzeczności T1 vs T4, a sufit i tak egzekwuje limit czasu ściennego z T-03
        // [`docs/ARCHITECTURE.md` §11].
        command
    }
}

// ── Wire enum Claude ──────────────────────────────────────────────────────────────────────
//
// Kształt z drutu mieszka WYŁĄCZNIE tutaj. Powyżej tej linii nie ma ani jednego `serde`, poniżej
// nie ma ani jednego [`AgentEvent`] — to jest ten sam podział, dzięki któremu `CodexDriver`
// (T-10) powstaje bez dotykania `stream.rs` [PLAN §8, ryzyko 5].

/// Pole, którego kształt vendor może zmienić bez uprzedzenia.
///
/// Cokolwiek nie pasuje, znika jako `None` — zamiast wywalić **całą linię** do licznika śmieci.
/// To jest niezmiennik 5 w miejscu, w którym naprawdę się łamie: `#[serde(other)]` ratuje
/// nieznany `type`, ale nie ratuje znanego typu, któremu vendor zmienił kształt pola
/// zagnieżdżonego — a wtedy tracimy linię, która w 95% była dla nas czytelna.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// Jedna linia strumienia `stream-json` [T1 §8.5].
///
/// `#[serde(other)] Unknown` jest nienegocjowalny: vendorzy dokładają typy zdarzeń co tydzień,
/// po cichu, i bieg nie ma prawa na tym paść (niezmiennik 5). Sam ten atrybut jednak **nie
/// wystarcza** — decyduje to, że [`ClaudeDecoder::push`] nie zwraca `Result`, więc nie ma czego
/// przepuścić przez `?` w pętli czytającej.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeLine {
    /// `init`, `thinking_tokens`, `api_retry`, haki — rozróżniane po `subtype`.
    System(SystemLine),
    /// Proza, myślenie i wywołania narzędzi.
    Assistant(TurnLine),
    /// Wyniki narzędzi wracające do modelu (i nasze koperty, gdyby ktoś włączył ich echo).
    User(TurnLine),
    /// Limit u dostawcy. Pola siedzą **zagnieżdżone** [T1 korekta 3].
    RateLimitEvent(RateLimitLine),
    /// Koniec tury. Dokładnie jedna na turę [T1 §4.4].
    Result(Box<ResultLine>),
    /// Wszystko, czego jeszcze nie znamy. Linia jest **rozpoznana**, tylko nic nie znaczy.
    #[serde(other)]
    Unknown,
}

/// Linia `system/*`. Każde pole opcjonalne, bo `init` z 2.1.233 ma ich dwadzieścia kilka,
/// a `hook_response` — pięć zupełnie innych [T1 §4.1, korekta 5].
#[derive(Debug, Deserialize)]
struct SystemLine {
    subtype: Option<String>,
    session_id: Option<String>,
    model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    tools: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient")]
    capabilities: Option<Vec<String>>,
    attempt: Option<u32>,
    max_retries: Option<u32>,
}

/// Linia `assistant` albo `user`: obie niosą wiadomość z blokami treści [T1 §4.2, §4.3].
#[derive(Debug, Deserialize)]
struct TurnLine {
    message: Option<TurnMessage>,
}

/// Wiadomość jednej strony rozmowy.
#[derive(Debug, Deserialize)]
struct TurnMessage {
    /// **Surowe** wartości, nie od razu `Vec<Block>`, i to jest różnica z pomiarem za sobą:
    /// jeden blok o nieoczekiwanym kształcie kosztowałby nas **wszystkie** bloki tej
    /// wiadomości, bo `Vec<T>` jest albo cały, albo wcale. Każdy blok czytamy z osobna
    /// w [`ClaudeDecoder::blocks`] (niezmiennik 5).
    #[serde(default, deserialize_with = "lenient")]
    content: Option<Vec<Value>>,
}

/// Blok treści wewnątrz wiadomości.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    /// Model myśli. Treści myślenia **nie czytamy** — nie wchodzi na transkrypt
    /// [`docs/ARCHITECTURE.md` §6, reguła 5].
    Thinking {},
    /// Proza, dosłownie.
    Text { text: Option<String> },
    /// Czynność narzędziem.
    ToolUse {
        id: Option<String>,
        name: Option<String>,
        #[serde(default, deserialize_with = "lenient")]
        input: Option<ToolInput>,
    },
    /// Wynik czynności.
    ToolResult {
        tool_use_id: Option<String>,
        content: Option<Value>,
        is_error: Option<bool>,
    },
    /// Blok, którego nie znamy.
    #[serde(other)]
    Unknown,
}

/// To, co nas interesuje w argumentach narzędzia.
#[derive(Debug, Deserialize)]
struct ToolInput {
    /// Etykieta po ludzku, **napisana przez sam model**. To jest prezent: dostajemy zdanie
    /// gotowe na ekran, za darmo i bez zgadywania [T1 §8.6, ran].
    description: Option<String>,
    file_path: Option<String>,
}

/// Linia `rate_limit_event`.
#[derive(Debug, Deserialize)]
struct RateLimitLine {
    /// Koperta, której raport T1 §4.5 **nie miał** — i to jest cała pułapka tego zdarzenia
    /// [T1 korekta 3]. Parser napisany pod kształt płaski deserializuje się bez błędu, nie
    /// widzi nic, banner się nie pokazuje i dowiadujesz się o tym z rachunku.
    #[serde(default, deserialize_with = "lenient")]
    rate_limit_info: Option<RateLimitInfo>,
}

/// Wnętrze koperty limitu. Klucze są tu `camelCase`, w odróżnieniu od reszty strumienia —
/// tak je wypisało CLI 2.1.233 i tak zostaje.
#[derive(Debug, Deserialize)]
struct RateLimitInfo {
    status: Option<String>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
    #[serde(rename = "rateLimitType")]
    rate_limit_type: Option<String>,
}

/// Linia `result` — jedyna, która kończy turę [T1 §4.4].
#[derive(Debug, Deserialize)]
struct ResultLine {
    /// **Nigdy nie rozstrzyga o powodzeniu.** Nieudany bieg przyszedł z `"subtype":"success"`
    /// przy `"is_error":true` [T1 §4.4, ran, potwierdzone ponownie]. Czytamy go wyłącznie po to,
    /// żeby odróżnić sufit tur (`error_max_*`) od reszty.
    subtype: Option<String>,
    /// To pole, a nie `subtype`, mówi, czy krok się udał.
    is_error: Option<bool>,
    terminal_reason: Option<String>,
    session_id: Option<String>,
    num_turns: Option<u32>,
    total_cost_usd: Option<f64>,
    duration_ms: Option<u64>,
    /// Ostatnia wypowiedź agenta — to, co krok przekazuje dalej.
    result: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    usage: Option<Usage>,
}

/// Zużycie kontekstu z drutu. Trzy pola z kilkunastu: reszta to statystyki, których nikt nie
/// czyta, a pole bez czytelnika jest zakazane (niezmiennik 21).
#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(rename = "input_tokens")]
    input: Option<u64>,
    #[serde(rename = "output_tokens")]
    output: Option<u64>,
    /// Ta liczba, i tylko ta, mówi, czy izolacja kontekstu w ogóle działa [T1 §3.3].
    #[serde(rename = "cache_read_input_tokens")]
    cached: Option<u64>,
}

/// Dekoder jednego strumienia: linia tekstu → zero lub więcej [`AgentEvent`].
///
/// **`push` nie zwraca `Result` i to jest cały niezmiennik 5 w jednej sygnaturze.** Cicha
/// wersja złamania nie siedzi w typie — enum z `#[serde(other)]` ma wariant `Unknown` i to
/// nie pomaga — tylko w **pętli**: `let ev = serde_json::from_str(&line)?;` kończy krok na
/// pierwszej linii, która nie jest JSON-em, a vendorzy dokładają typy zdarzeń co tydzień, po
/// cichu [niezmiennik 5, T7 ryzyko 4]. Nieznaną linię logujemy i porzucamy; skoro nie da się
/// jej zwrócić jako błąd, nie da się na niej wywalić biegu.
///
/// Kształt wire enuma, który tu wejdzie [T1 §8.5]: `#[serde(tag = "type")]` z wariantem
/// `#[serde(other)] Unknown` i `Option<T>` na **każdym** polu, które nie jest niezbędne.
#[derive(Debug, Default)]
pub struct ClaudeDecoder {
    /// Ile linii nie dało się w ogóle sparsować. Rośnie tylko dla śmieci — linia z poprawnym
    /// JSON-em i nieznanym `type` jest **rozpoznana**, tylko nic nie znaczy.
    unparsed: usize,
    /// Sesja, którą CLI ogłosiło w `init` albo powtórzyło w `result`. Trzymamy ją, żeby
    /// zdarzenie końca miało czym się podpisać także wtedy, gdy strumień urwał się bez `result`.
    session: Option<String>,
    /// Czy któraś linia `result` już zamknęła turę. Po tym poznaje [`Self::end_of_stream`],
    /// że nie ma czego domykać.
    ended: bool,
    /// Wywołania narzędzi, które zapowiedziały zmianę pliku, czekające na swój wynik.
    ///
    /// [`AgentEvent::FileEdit`] mówi „agent **zmienił** plik" w czasie przeszłym, więc wolno go
    /// wypuścić dopiero, kiedy narzędzie się udało. Wpis znika przy wyniku niezależnie od tego,
    /// czy zmiana doszła do skutku — inaczej mapa rosłaby przez cały bieg.
    edits: HashMap<String, PathBuf>,
}

impl ClaudeDecoder {
    /// Świeży dekoder, przed pierwszą linią.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wpuszcza jedną linię strumienia i oddaje zdarzenia, które z niej wynikają.
    ///
    /// Pusty wektor jest **normalną odpowiedzią**, nie sygnałem błędu: tak wyglądają
    /// `thinking_tokens`, hooki `SessionStart` i każdy typ zdarzenia, którego jeszcze nie
    /// znamy.
    pub fn push(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            // Pusta linia nie jest śmieciem: NDJSON kończy się nią przy każdym normalnym
            // wyjściu, a licznik śmieci ma zostać liczbą, którą warto czytać.
            return Vec::new();
        }

        // Całe mapowanie linia → zdarzenia stoi w JEDNYM match, razem z gałęzią śmiecia: to jest
        // ta lista, którą czyta się, pytając „co ten sterownik w ogóle rozumie".
        match serde_json::from_str::<ClaudeLine>(line) {
            Err(error) => {
                self.unparsed += 1;
                // Treści linii tu nie ma, i to jest świadome: surowy strumień leży już na dysku
                // (tee z T-05), a dziennik aplikacji czyta się w zgłoszeniu błędu — nie ma
                // powodu, żeby druga kopia cudzego tekstu jechała jeszcze tędy.
                tracing::debug!(
                    bytes = line.len(),
                    %error,
                    "a line of the agent stream could not be read; dropping it"
                );
                Vec::new()
            }
            Ok(ClaudeLine::System(line)) => self.system(&line),
            Ok(ClaudeLine::Assistant(line) | ClaudeLine::User(line)) => self.blocks(line.message),
            Ok(ClaudeLine::RateLimitEvent(line)) => Self::rate_limit(&line),
            Ok(ClaudeLine::Result(line)) => vec![self.finish(&line)],
            // Nieznany typ jest ROZPOZNANY — linia się wczytała, tylko nic dla nas nie znaczy.
            // Liczenie jej jako śmiecia zasłoniłoby linie, które naprawdę były śmieciem.
            Ok(ClaudeLine::Unknown) => Vec::new(),
        }
    }

    /// `system/*` → zdarzenie albo cisza.
    ///
    /// Haki `SessionStart` są celowo niczym: pojawiają się nawet bez `--include-hook-events`
    /// i znikają pod `--setting-sources ""`, a użytkownikowi nie mówią nic [T1 §4.5, ran].
    fn system(&mut self, line: &SystemLine) -> Vec<AgentEvent> {
        match line.subtype.as_deref() {
            Some(INIT) => {
                if let Some(id) = &line.session_id {
                    self.session = Some(id.clone());
                }
                vec![AgentEvent::Started {
                    session: self.session_ref(line.session_id.as_deref()),
                    model: line.model.clone().unwrap_or_default(),
                    tools: line.tools.clone().unwrap_or_default(),
                    // Na TEJ liście, a nie na numerze wersji, feature-detektuje się przerwanie
                    // w paśmie [T1 §4.1, §4.6].
                    capabilities: line.capabilities.clone().unwrap_or_default(),
                }]
            }
            // Nigdy nie niesie tekstu: to jest stały slot na dole ekranu, nie wpis w historii
            // [`docs/ARCHITECTURE.md` §6, reguła 5].
            Some(THINKING_TOKENS) => vec![AgentEvent::Thinking],
            // 2026-08-15 — kształt tej linii jest [docs], nie [ran]: nie ma jej w fiksturze,
            // więc mapowanie zostaje możliwie głupie. Zdanie po angielsku, nigdy `api_retry`
            // na ekranie (niezmiennik 14).
            Some(API_RETRY) => vec![AgentEvent::Notice {
                text: retry_sentence(line.attempt, line.max_retries),
            }],
            _ => Vec::new(),
        }
    }

    /// Bloki treści jednej wiadomości → zdarzenia.
    ///
    /// Każdy blok czytany **osobno**: jeden blok o nieznanym kształcie nie ma prawa kosztować
    /// nas pozostałych (niezmiennik 5).
    fn blocks(&mut self, message: Option<TurnMessage>) -> Vec<AgentEvent> {
        let Some(blocks) = message.and_then(|message| message.content) else {
            return Vec::new();
        };

        let mut events = Vec::new();
        for raw in blocks {
            match serde_json::from_value::<Block>(raw).unwrap_or(Block::Unknown) {
                Block::Thinking {} => events.push(AgentEvent::Thinking),
                Block::Text { text } => {
                    let text = text.unwrap_or_default();
                    if !text.trim().is_empty() {
                        events.push(AgentEvent::Said { text });
                    }
                }
                Block::ToolUse { id, name, input } => {
                    let id = id.unwrap_or_default();
                    let name = name.unwrap_or_default();
                    events.push(AgentEvent::ToolStart {
                        id: id.clone(),
                        label: tool_label(&name, input.as_ref()),
                    });
                    if let Some(path) = editing_path(&name, input) {
                        self.edits.insert(id, path);
                    }
                }
                Block::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let id = tool_use_id.unwrap_or_default();
                    let ok = !is_error.unwrap_or(false);
                    let edited = self.edits.remove(&id);
                    events.push(AgentEvent::ToolEnd {
                        id,
                        ok,
                        summary: summarise(content.as_ref()),
                    });
                    if ok && let Some(path) = edited {
                        events.push(AgentEvent::FileEdit { path });
                    }
                }
                Block::Unknown => {}
            }
        }
        events
    }

    /// `rate_limit_event` → zdarzenie limitu, albo nic.
    ///
    /// **Nic, kiedy brakuje którejkolwiek z trzech wartości** — i to jest cały sens tego
    /// kryterium. Zdarzenie z `resets_at == 0` mówi „limit wraca o 01:00 czasu uniksowego 1970",
    /// czyli wygląda jak odpowiedź; brak bannera przynajmniej nie kłamie [T1 korekta 3].
    fn rate_limit(line: &RateLimitLine) -> Vec<AgentEvent> {
        let Some(info) = &line.rate_limit_info else {
            tracing::debug!("a rate limit line arrived without its envelope; dropping it");
            return Vec::new();
        };
        let (Some(status), Some(resets_at), Some(window)) = (
            info.status.as_deref(),
            info.resets_at,
            info.rate_limit_type.as_deref(),
        ) else {
            tracing::debug!("a rate limit line arrived half-filled; dropping it");
            return Vec::new();
        };

        vec![AgentEvent::RateLimit {
            status: status.to_owned(),
            resets_at,
            rate_limit_type: window.to_owned(),
            // Co jest zgodą, rozstrzyga `engine::limits` i tylko on (niezmiennik 23) — tutaj
            // stała była trzecią kopią tej samej reguły. Samą pauzę robi T-21; ten sterownik
            // mówi wyłącznie, czy dostawca zostawił coś do wysłania.
            pause_run: !crate::engine::limits::is_allowed(status),
        }]
    }

    /// Linia `result` → koniec tury.
    ///
    /// **`subtype` nie rozstrzyga o niczym poza sufitem tur.** Nieudany bieg przyszedł
    /// z `"subtype":"success"` przy `"is_error":true` i `"terminal_reason":"api_error"`
    /// [T1 §4.4, ran]. Sterownik czytający `subtype` melduje sukces kroku, który nie zrobił nic,
    /// a stożek poniżej rusza na pustym przekazaniu.
    fn finish(&mut self, line: &ResultLine) -> AgentEvent {
        self.ended = true;
        if let Some(id) = &line.session_id {
            self.session = Some(id.clone());
        }

        // Brak `is_error` nie jest obietnicą sukcesu: kiedy vendor go nie dosłał, pytamy
        // `subtype`, bo to jedyne, co zostało. Kiedy dosłał — `subtype` nie ma tu głosu.
        let failed = line.is_error.unwrap_or_else(|| {
            line.subtype
                .as_deref()
                .is_some_and(|subtype| subtype.starts_with(ERROR_PREFIX))
        });

        let reason = if !failed {
            FinishReason::Completed
        } else if line.terminal_reason.as_deref() == Some(CANCELLED) {
            // Anulowanie jest wartością, nie błędem (niezmiennik 7): krok, który ktoś zatrzymał
            // celowo, nie ma prawa czytać się tak samo jak krok, który się zepsuł.
            FinishReason::Cancelled
        } else if line
            .subtype
            .as_deref()
            .is_some_and(|subtype| subtype.starts_with(CEILING_PREFIX))
        {
            FinishReason::LimitReached
        } else {
            FinishReason::Failed(failure_sentence(line))
        };

        let usage = line.usage.as_ref();
        AgentEvent::Finished(Outcome {
            ok: !failed,
            reason,
            text: line.result.clone().unwrap_or_default(),
            // `None`, nie zero: zero jest liczbą i sumuje się w rachunek, którego nikt nie
            // zamawiał.
            cost_usd: line.total_cost_usd,
            tokens: Tokens {
                input: usage.and_then(|usage| usage.input).unwrap_or_default(),
                output: usage.and_then(|usage| usage.output).unwrap_or_default(),
                cached: usage.and_then(|usage| usage.cached).unwrap_or_default(),
            },
            turns: line.num_turns.unwrap_or_default(),
            took: Duration::from_millis(line.duration_ms.unwrap_or_default()),
            session: self.session_ref(line.session_id.as_deref()),
        })
    }

    /// Sesja tej rozmowy: to, co powiedziała linia, a w drugiej kolejności to, co pamiętamy.
    fn session_ref(&self, from_line: Option<&str>) -> SessionRef {
        SessionRef {
            vendor: VENDOR,
            id: from_line
                .map(str::to_owned)
                .or_else(|| self.session.clone())
                .unwrap_or_default(),
        }
    }

    /// Ile linii dekoder porzucił jako niesparsowalne. To jest licznik do pliku debug
    /// i do zgłoszenia błędu, a nie powód, żeby zatrzymać bieg.
    #[must_use]
    pub fn unparsed(&self) -> usize {
        self.unparsed
    }

    /// Domyka turę, kiedy strumień się skończył. `exit_code` jest sygnałem **drugorzędnym**
    /// [T1 §8.5].
    ///
    /// Zwraca [`AgentEvent::Finished`] tylko wtedy, gdy linia `result` **nie przyszła** —
    /// bo wtedy nikt inny go nie wypuści, a krok bez zdarzenia końca wisiałby w `running` do
    /// końca biegu. Strumień zakończony kodem 0 bez `result` jest **niepowodzeniem**, nie
    /// sukcesem: proces, który wyszedł czysto i nie powiedział, co zrobił, nie ma czego
    /// przekazać dalej.
    pub fn end_of_stream(&mut self, exit_code: Option<i32>, complaint: &str) -> Option<AgentEvent> {
        if self.ended {
            return None;
        }
        self.ended = true;

        // Kod wyjścia jest w tym zdaniu opisem, nie dowodem: proces, który wyszedł czysto i nie
        // powiedział, co zrobił, nie ma czego przekazać dalej [T1 §8.5].
        let mut why = match exit_code {
            Some(code) => format!("The agent exited with code {code} and never sent its result."),
            None => "The agent stopped without ever sending its result.".to_owned(),
        };

        // 2026-08-18 — TO, CO AGENT POWIEDZIAŁ NA STRUMIENIU SKARG, DOKLEJAMY DO ZDANIA.
        //
        // Bez tego zdanie wyżej było jedyną rzeczą, jaką dostawał człowiek, i nie mówiło ani
        // słowa o przyczynie — a przyczyna niemal zawsze była wypisana, tylko na potoku, którego
        // `Supervised` nie dawał odebrać. Zmierzone na tej maszynie: `which claude` wskazuje
        // wrapper, który przy braku binarki pisze na stderr i wychodzi 127; z okna wyglądało to
        // identycznie jak agent, który wystartował i zamilkł.
        //
        // Jedna linia, nie cały potok: to jest zdanie na ekran, a nie dziennik. Pierwsza
        // niepusta linia skargi odpowiada na pytanie „dlaczego" w praktycznie każdym realnym
        // przypadku, a reszta jest już śladem stosu, który należy do pliku, nie do wiersza.
        if let Some(first) = complaint
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
        {
            why.push(' ');
            why.push_str(&first_line(first));
        }

        Some(AgentEvent::Finished(Outcome {
            ok: false,
            reason: FinishReason::Failed(why),
            text: String::new(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 0,
            took: Duration::ZERO,
            session: self.session_ref(None),
        }))
    }
}

/// Zdanie, które ląduje w [`FinishReason::Failed`], czyli **na ekranie**.
///
/// Najpierw własna wypowiedź agenta: to ona odpowiada na pytanie „dlaczego", które ktoś zaraz
/// zada. Dopiero kiedy jej nie ma, tłumaczymy enum z drutu na angielskie zdanie — `api_error`
/// samo w sobie nie ma prawa dojechać na ekran (niezmiennik 14).
fn failure_sentence(line: &ResultLine) -> String {
    if let Some(text) = line.result.as_deref()
        && !text.trim().is_empty()
    {
        return first_line(text);
    }
    match line.terminal_reason.as_deref() {
        Some("api_error") => "The model provider returned an error.".to_owned(),
        Some("timeout") => "The agent ran out of time.".to_owned(),
        _ => "The agent stopped before it finished.".to_owned(),
    }
}

/// Zdanie o ponowieniu zapytania. Liczby wchodzą tylko wtedy, gdy vendor je podał.
fn retry_sentence(attempt: Option<u32>, max_retries: Option<u32>) -> String {
    match (attempt, max_retries) {
        (Some(attempt), Some(max)) => format!("Retrying — try {attempt} of {max}."),
        (Some(attempt), None) => format!("Retrying — try {attempt}."),
        _ => "Retrying.".to_owned(),
    }
}

/// Etykieta czynności, gotowa na ekran.
///
/// Pierwszy wybór to zawsze `description`: model pisze ją sam, po ludzku, i to jest najlepszy
/// tekst, jaki tu w ogóle może być [T1 §8.6, ran]. Reszta to zapasowe trzy czasowniki — a że
/// **kuracja należy do T-05**, nie zgadujemy tu niczego więcej.
fn tool_label(name: &str, input: Option<&ToolInput>) -> String {
    if let Some(description) = input.and_then(|input| input.description.as_deref())
        && !description.trim().is_empty()
    {
        return first_line(description);
    }

    let target = input
        .and_then(|input| input.file_path.as_deref())
        .map(file_name);
    match (verb_for(name), target) {
        (verb, Some(target)) => format!("{verb} {target}"),
        (verb, None) => verb.to_owned(),
    }
}

/// Czasownik dla rodziny narzędzi [T1 §8.6].
fn verb_for(name: &str) -> &'static str {
    match name {
        "Read" | "Grep" | "Glob" | "NotebookRead" => "Reading",
        "Edit" | "Write" | "NotebookEdit" => "Editing",
        "Bash" | "BashOutput" => "Running a command",
        // Narzędzia, których nie znamy — a jest ich siedemdziesiąt kilka i przybywa co tydzień.
        // Nazwa własna narzędzia jest tu jedyną prawdą, jaką mamy.
        _ => "Working",
    }
}

/// Ścieżka, którą to wywołanie zmieni — o ile w ogóle coś zmienia.
fn editing_path(name: &str, input: Option<ToolInput>) -> Option<PathBuf> {
    if verb_for(name) != "Editing" {
        return None;
    }
    input
        .and_then(|input| input.file_path)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

/// Sama nazwa pliku: pełna ścieżka w etykiecie to trzy czwarte linii zjedzone przez katalogi,
/// których użytkownik nie wybierał.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Jednolinijkowe podsumowanie wyniku narzędzia. Pełne wyjście zostaje za kliknięciem (T-05).
fn summarise(content: Option<&Value>) -> String {
    let text = match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };
    first_line(&text)
}

/// Pierwsza niepusta linia, przycięta do długości, która mieści się w wierszu.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.chars().count() > SUMMARY_LIMIT {
        line.chars().take(SUMMARY_LIMIT).collect::<String>() + "…"
    } else {
        line.to_owned()
    }
}

// ── Koperta wiadomości ────────────────────────────────────────────────────────────────────

/// Jedna linia stdinu: `{"type":"user","message":{"role":"user","content":[{"type":"text",…}]}}`
/// [T1 §4.6, ran].
///
/// Tędy — i **wyłącznie tędy** — jedzie treść zadania (niezmiennik 9). Cicha wersja złamania nie
/// wygląda jak prompt w argv: wygląda jak `--append-system-prompt` z wklejoną treścią zadania,
/// a argumenty widzi `ps` każdego użytkownika maszyny.
#[derive(Debug, Serialize)]
struct Envelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    message: EnvelopeMessage<'a>,
}

/// Wiadomość w kopercie.
#[derive(Debug, Serialize)]
struct EnvelopeMessage<'a> {
    role: &'static str,
    content: [EnvelopeBlock<'a>; 1],
}

/// Jedyny blok treści koperty.
#[derive(Debug, Serialize)]
struct EnvelopeBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
}

/// Buduje kopertę jednej tury — **jedna linia**, bo CLI czyta stdin linia po linii.
///
/// Serializujemy zamiast sklejać stringi: prompt z cudzysłowem albo znakiem nowej linii,
/// wklejony ręcznie, rozjeżdża linię JSON i cała tura ginie na parsowaniu po drugiej stronie.
fn user_envelope(text: &str) -> serde_json::Result<String> {
    serde_json::to_string(&Envelope {
        kind: "user",
        message: EnvelopeMessage {
            role: "user",
            content: [EnvelopeBlock { kind: "text", text }],
        },
    })
}

/// Prośba sterująca — ta sama droga co koperta użytkownika, inny kształt [T1 §4.6, ran]:
/// `{"type":"control_request","request_id":"req_…","request":{"subtype":"interrupt"}}`
///
/// Protokół jest **nieudokumentowany** i zweryfikowany wyłącznie eksperymentem, dlatego jedzie
/// tylko pod ogłoszoną zdolnością [`INTERRUPT_CAPABILITY`].
#[derive(Debug, Serialize)]
struct ControlRequest<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    /// Wraca w `control_response` i po nim, a nie po kolejności, poznaje się odpowiedź na
    /// **tę** prośbę.
    request_id: &'a str,
    request: ControlBody,
}

/// Treść prośby sterującej. Dziś jeden podtyp; lista jest niepubliczna [T1 §11].
#[derive(Debug, Serialize)]
struct ControlBody {
    subtype: &'static str,
}

/// Buduje przerwanie w paśmie — jedna linia, tak jak koperta tury.
fn interrupt_request(request_id: &str) -> serde_json::Result<String> {
    serde_json::to_string(&ControlRequest {
        kind: "control_request",
        request_id,
        request: ControlBody {
            subtype: "interrupt",
        },
    })
}

// ── Pętla czytająca ───────────────────────────────────────────────────────────────────────

/// Ile linii do agenta mieści się w kolejce, zanim nadawca zaczeka.
///
/// Mała z rozmysłem: to jest kanał sterowania, nie strumień danych. Głęboka kolejka znaczyłaby,
/// że tekst wpisany przez człowieka czeka w niej po tym, jak sesja już się skończyła — a wtedy
/// nadawca dowiaduje się o tym dopiero z ciszy.
const SAY_QUEUE: usize = 8;

/// JEDYNY pisarz do `stdin` sesji: koperty tur i przerwania, w kolejności nadania.
///
/// 2026-08-18 — POWSTAŁO, ŻEBY DAŁO SIĘ NAPISAĆ DO ŻYWEGO AGENTA. Potok był polem uchwytu, więc
/// pisanie wymagało `&mut self`, a uchwyt jest pożyczony mutowalnie przez całą turę. Powód
/// w całości stoi przy polu [`ClaudeHandle::voice`].
///
/// Potok GINIE RAZEM Z TYM ZADANIEM i to jest jego druga robota: porzucenie `ChildStdin` jest
/// tym, po czym CLI dostaje EOF i wychodzi zerem [T1 §2]. Dlatego [`AgentHandle::close`] czeka
/// na to zadanie, zamiast tylko zamknąć kanał.
///
/// Bez `?` i bez `unwrap` (niezmiennik 5): linia, której nie dało się zapisać, kończy pętlę —
/// dalsze pisanie do zamkniętego potoku dawałoby błąd na każdej następnej i tyle samo wierszy
/// w dzienniku.
///
/// # Dlaczego osobna prośba o zamknięcie, a nie „koniec, gdy zniknie ostatni nadajnik"
///
/// 2026-08-18, zmierzone: pierwsza wersja kończyła pisanie wyłącznie na zamkniętym kanale, więc
/// [`AgentHandle::close`] czekał, aż zniknie **każdy** klon głosu. Głos jest jednak klonowalny
/// z definicji i produkcja trzyma jego kopię przez całą turę (`commands::run::RunControl.voices`,
/// żeby dało się napisać do agenta, który pracuje). Jedna taka kopia — trzymana przez okno, przez
/// test, przez rejestr, który jeszcze nie posprzątał — znaczyła: pisarz nie kończy, `stdin` nie
/// ginie, CLI nie dostaje EOF, `close()` nie wraca **nigdy**. Sesja `claude` przeżyła w tym
/// kształcie 15 minut przy dwóch turach po 3 s i zeszła dopiero z sygnału.
///
/// Dlatego zamknięcie jest teraz JAWNE i wygrywa niezależnie od tego, kto jeszcze trzyma głos.
/// Ma to drugi, celowy skutek: kiedy to zadanie się kończy, ginie z nim odbiornik kanału, więc
/// każde późniejsze `send` na starym klonie oddaje `Err` — czyli głos po zamknięciu odpowiada
/// „ta sesja już nic nie przyjmuje", a nie ciszą (to samo obiecuje [`AgentHandle::voice`]).
async fn talk(
    mut pipe: ChildStdin,
    mut inbox: mpsc::Receiver<ToAgent>,
    session: String,
    mut hush: oneshot::Receiver<()>,
) {
    loop {
        // `biased`, bo prośba o zamknięcie ma wygrywać z linią, która właśnie przyszła: sesja
        // zamykana ma się zamknąć, a nie dopisać jeszcze jedną turę i zamknąć się później.
        let said = tokio::select! {
            biased;
            _ = &mut hush => break,
            said = inbox.recv() => match said {
                Some(said) => said,
                // Kanał bez ani jednego nadajnika też kończy pisanie. To jest droga dublerów
                // i testów, które nie wołają `close()`; produkcja przychodzi przez `hush`.
                None => break,
            },
        };
        let line = match &said {
            ToAgent::Turn(text) => user_envelope(text),
            ToAgent::Interrupt(id) => interrupt_request(id),
        };
        let Ok(line) = line else {
            tracing::debug!(%session, "a line to the agent could not be built; dropping it");
            continue;
        };
        // Znak nowej linii jest częścią protokołu, nie formatowaniem: CLI czyta stdin linia po
        // linii i bez niego koperta nigdy się nie kończy.
        if pipe.write_all(line.as_bytes()).await.is_err()
            || pipe.write_all(b"\n").await.is_err()
            || pipe.flush().await.is_err()
        {
            tracing::debug!(%session, "the agent stopped reading its input");
            break;
        }
    }
}

/// Ile bajtów skargi trzymamy. Pierwsze, nie ostatnie.
///
/// Pierwsza linia stderr jest tą, która mówi, co się stało („command not found", „not logged
/// in", „permission denied"); ostatnia jest zwykle ogonem śladu stosu. Bufor bez limitu byłby
/// za to trzecim miejscem, w którym gadatliwy agent może zjeść pamięć okna.
const COMPLAINT_KEPT: usize = 4 * 1024;

/// Opróżnia strumień skarg do EOF i zapamiętuje początek tego, co powiedział.
///
/// **Opróżnia**, a nie „czyta, jeśli ktoś zapyta", i to jest cały powód, dla którego to zadanie
/// istnieje osobno: potok o pojemności ~64 KB, którego nikt nie odbiera, zatrzymuje dziecko na
/// `write` — czyli agent gadatliwy na stderr wisi, a z okna wygląda to jak agent, który myśli.
///
/// Bez `?` i bez `unwrap` (niezmiennik 5): błąd odczytu skargi nie ma prawa zabrać tury.
/// Zamek brany i oddany w jednym wyrażeniu, nigdy przez `await` (niezmiennik 8).
async fn drain_complaints(stderr: ChildStderr, into: Arc<Mutex<String>>) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let mut held = into.lock().unwrap_or_else(PoisonError::into_inner);
        if held.len() < COMPLAINT_KEPT {
            held.push_str(&line);
        }
        // Bez `break` po przekroczeniu limitu: pętla musi dalej OPRÓŻNIAĆ potok, nawet gdy nic
        // już nie zapamiętuje. Wyjście tutaj przywróciłoby dokładnie tę blokadę, przed którą
        // to zadanie stoi.
    }
}

/// Czyta stdout linia po linii, kładzie **bajty** w transkrypcie kroku i sypie zdarzeniami,
/// aż do końca strumienia.
///
/// **Nie ma tu `?` i to nie jest przeoczenie** (niezmiennik 5): jedyny sposób, żeby nieznana
/// linia zabiła bieg, to zwrócić z tej pętli błąd. Dekoder oddaje pusty wektor, a pętla leci
/// dalej. Ta sama zasada obowiązuje zapis: dysk, który odmówił jednej linii, jest wart
/// wiersza w dzienniku, a nie urwanego biegu.
///
/// `transcript` na `None` znaczy „tego biegu nikt nie zapisuje" — sonda wersji i kryteria
/// samego sterownika pytają o zdarzenia, nie o plik.
async fn pump(
    stdout: ChildStdout,
    capabilities: Arc<OnceLock<Vec<String>>>,
    events: mpsc::Sender<DecodedEvent>,
    outcomes: mpsc::Sender<Outcome>,
    mut transcript: Option<Recorder>,
    complaint: Arc<Mutex<String>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut decoder = ClaudeDecoder::new();
    let mut buffer: Vec<u8> = Vec::with_capacity(8 * 1024);

    loop {
        buffer.clear();
        // `read_until`, nie `lines()`. `lines()` zjada `\r`, gubi to, czy linia w ogóle miała
        // znak końca, i przewraca się na bajtach nie-UTF-8 — a każda z tych trzech rzeczy
        // czyni bajtową identyczność transkryptu nie do spełnienia (`ARCHITECTURE` §4).
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(%error, "the agent output stream broke off");
                break;
            }
        }

        // TEE PRZED PARSOWANIEM. Linia, której nikt nie zrozumie, jest w pliku tak samo jak
        // każda inna — a to właśnie ona jest potrzebna w zgłoszeniu błędu, i to ona przybywa
        // u vendora co tydzień, po cichu.
        if let Some(recorder) = transcript.as_mut()
            && let Err(error) = recorder.raw(&buffer).await
        {
            tracing::warn!(%error, "the step transcript would not take a line of the stream");
        }

        let Ok(text) = std::str::from_utf8(&buffer) else {
            // Bajty nie-UTF-8 są już w transkrypcie i tam zostają; dla dekodera to linia nie do
            // przeczytania, a nie powód, żeby przestać czytać strumień.
            continue;
        };

        let text = text.trim();
        if text.is_empty() {
            // Pusta linia nie jest uszkodzeniem: NDJSON kończy się nią przy każdym normalnym
            // wyjściu.
            continue;
        }

        // Zdarzenia i fakty o narzędziu z JEDNEJ linii i JEDNYM wywołaniem. `stream::decode`
        // pyta dekoder o zdarzenia neutralne wobec vendora i z tej samej linii dokłada
        // [`Tool`] — rodzinę czynności, pełną ścieżkę i pełne wyjście, czyli to wszystko,
        // co `AgentEvent` świadomie gubi [T1 §8.2]. Druga tabela nazw narzędzi tutaj byłaby
        // drugą implementacją tej samej polityki (niezmiennik 23), a parowanie zdarzenia
        // z faktem po czymkolwiek innym niż wspólna linia rozjeżdża się na pierwszym
        // strumieniu, w którym jedna wiadomość niesie dwa bloki.
        let stream::Decoded::Events(from_line) = stream::decode(&mut decoder, text) else {
            continue;
        };

        for stream::DecodedEvent { event, tool } in from_line {
            // Zapis PRZED wysyłką i tylko tutaj: kto zobaczył `Started`, ten ma prawo
            // zakładać, że eskalacja anulowania wie już, o co pytać. Odwrotna kolejność
            // jest wyścigiem, który przechodzi na tej maszynie i przewraca się na
            // wolniejszej. `set` na drugim `init` przepada celowo — zdolności ogłasza
            // ten proces, a on jest jeden na sesję.
            if let AgentEvent::Started {
                capabilities: announced,
                ..
            } = &event
            {
                let _ = capabilities.set(announced.clone());
            }
            if let Some(recorder) = transcript.as_mut() {
                recorder.curate(&event, tool.as_ref()).await;
            }
            // 2026-08-18 — FAKT O NARZĘDZIU JEDZIE DALEJ. Do tego dnia szło tu samo `event`,
            // a `tool` kończyło życie w transkrypcie: wołający dostawał zdarzenie bez rodziny
            // czynności, więc `Curator::tool_start` oddawał `Vec::new()` i wiersze `read`,
            // `search`, `edit`, `ran` nie powstawały nigdy (powód w całości przy
            // [`DecodedEvent`]).
            emit(DecodedEvent { event, tool }, &events, &outcomes).await;
        }
    }

    // Kod wyjścia jest sygnałem drugorzędnym i tu go nie mamy: uchwyt procesu został przy
    // wołającym, a strumień skończył się przed nim. Zdarzenie końca musi paść mimo to, inaczej
    // krok wisi w `running` do końca biegu [T1 §8.5].
    // Skargę czytamy DOPIERO TERAZ, po EOF na wyjściu: proces, który się przewrócił, pisze na
    // stderr zanim zamknie stdout, więc w tej chwili buforek ma już to, co miał do powiedzenia.
    // Zamek brany i oddany w JEDNYM wyrażeniu — między nim a jakimkolwiek `await` nie ma ani
    // jednej instrukcji (niezmiennik 8).
    let said = complaint
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    // Kodu wyjścia tu nie ma i nie da się go tu mieć: uchwyt procesu został przy wołającym,
    // a ta pętla kończy się na EOF wyjścia, czyli ZANIM proces zdąży zostać zebrany. Zdanie
    // niesie więc skargę, nie numer — i to jest ta połowa, która odpowiada na „dlaczego".
    // Zgłoszone: gałąź `Some(code)` niżej ma dziś jednego wołającego w testach dekodera.
    if let Some(event) = decoder.end_of_stream(None, &said) {
        // Bez narzędzia i to nie jest brak: koniec strumienia jest faktem o turze, nie
        // o czynności. Wiersz z niego powstaje w kuratorze tak samo jak z linii `result`.
        if let Some(recorder) = transcript.as_mut() {
            recorder.curate(&event, None).await;
        }
        emit(event.into(), &events, &outcomes).await;
    }

    if let Some(recorder) = transcript.take()
        && let Err(error) = recorder.close().await
    {
        tracing::warn!(%error, "the step transcript would not close cleanly");
    }

    // Oba nadajniki giną RAZEM Z TĄ PĘTLĄ i to jest ich druga robota: zamknięty kanał wyników
    // jest jedynym sygnałem, po którym `wait()` wie, że nic już nie przyjdzie. Bez tego czekanie
    // na turę, która nigdy się nie skończy, jest nieodróżnialne od czekania na turę, która trwa.
    drop(events);
    drop(outcomes);
}

/// Wypuszcza jedno zdarzenie — **najpierw** do [`AgentHandle::wait`], potem na ekran.
///
/// Ta kolejność jest jedyną obroną przed wolnym konsumentem: kanał zdarzeń z pełnym buforem
/// zatrzymuje wysyłkę, a wynik tury, który utknął za nim, wygląda jak zawieszony agent.
/// Odwrotna kolejność kosztowałaby dokładnie to [T1 „Worth adding": wolny konsument opóźnia
/// wyjście do 30 s].
async fn emit(
    decoded: DecodedEvent,
    events: &mpsc::Sender<DecodedEvent>,
    outcomes: &mpsc::Sender<Outcome>,
) {
    if let AgentEvent::Finished(outcome) = &decoded.event {
        let _ = outcomes.send(outcome.clone()).await;
    }
    // Zamknięty kanał zdarzeń nie kończy pętli: nikt już nie patrzy na ekran, ale wynik tury
    // nadal ma dojść tam, gdzie ktoś na niego czeka.
    let _ = events.send(decoded).await;
}

/// Żywa sesja `claude` — jeden proces, wiele tur.
#[derive(Debug)]
pub struct ClaudeHandle {
    /// Sesja, którą sami nadaliśmy przed startem [T7 §6.2].
    session: SessionRef,
    /// Proces sesji, razem z całą eskalacją zabijania i dowodem z T-03. Grupa procesów jest
    /// jego polem, a nie kopią tutaj: dwie kopie tego samego faktu rozjeżdżają się dokładnie
    /// w chwili, w której zaczyna on być ciekawy.
    process: Supervised,
    /// Nadajnik do sesji. Tędy jedzie koperta każdej kolejnej tury i przerwanie w paśmie.
    ///
    /// 2026-08-18 — TU BYŁ `Option<ChildStdin>` I TO BYŁA PRZYCZYNA, dla której nie dało się
    /// napisać do żywego agenta. Potok był polem uchwytu, więc każdy pisarz potrzebował
    /// `&mut self` — a `commands::run::one_turn` trzyma uchwyt pożyczony mutowalnie przez CAŁĄ
    /// turę (`handle.wait()` w `tokio::select!`). Okno nie miało jak dosięgnąć sesji, dopóki tura
    /// trwa; po turze `close()` porzuca potok, co JEST końcem sesji. Czyli: nigdy.
    ///
    /// Potok przechodzi teraz na własność [`talk`], a tu zostaje nadajnik — klonowalny, bez
    /// `&mut`. Kolejność linii jest dalej zachowana, bo kanał jest jeden i czyta go jeden
    /// odbiorca; dwa kanały nad jednym `stdin` przeplotłyby kopertę tury z prośbą o przerwanie,
    /// a CLI czyta stdin linia po linii.
    ///
    /// `None` dopiero po [`AgentHandle::close`].
    voice: Option<Voice>,
    /// Zadanie, które trzyma potok. Czekamy na nie w [`AgentHandle::close`]: dopiero jego koniec
    /// porzuca `stdin`, a porzucenie potoku jest tym, po czym CLI wychodzi zerem [T1 §2].
    writer: Option<tokio::task::JoinHandle<()>>,
    /// Prośba do pisarza: „koniec, porzuć potok".
    ///
    /// Istnieje, bo czekanie na zniknięcie ostatniego klonu głosu jest zawieszeniem, nie
    /// zamknięciem — cały powód stoi przy [`talk`]. `None` po [`AgentHandle::close`], bo
    /// zamknięcie ma się dziać raz.
    hush: Option<oneshot::Sender<()>>,
    /// Zdolności protokołu ogłoszone w `system/init`, wpisane tu przez pętlę czytającą.
    ///
    /// Dzielone, bo ogłasza je strumień, a czyta eskalacja anulowania — i to jest jej **jedyny**
    /// odbiorca (niezmiennik 21). `OnceLock`, bo ta lista przychodzi raz i nigdy się nie zmienia,
    /// a zamek trzymany przez `await` byłby złamaniem niezmiennika 8 w miejscu, w którym nic by
    /// za to nie dał. Puste, dopóki `init` nie przyszedł — anulowanie przed startem sesji nie ma
    /// czego feature-detektować i schodzi wprost na sygnały.
    capabilities: Arc<OnceLock<Vec<String>>>,
    /// Wyniki tur, w kolejności, w jakiej padły. Osobno od kanału zdarzeń, bo `wait()` musi je
    /// dostać także wtedy, gdy nikt nie czyta ekranu.
    outcomes: mpsc::Receiver<Outcome>,
}

impl ClaudeHandle {
    /// Czy to CLI samo powiedziało, że rozumie przerwanie w paśmie.
    ///
    /// Po **liście z `init`**, nigdy po numerze wersji [T1 §4.1]. `false`, dopóki `init` nie
    /// przyszedł: anulowanie wcześniej nie ma czego pytać, a pytanie wysłane w ciemno kosztuje
    /// pełne okno czekania na odpowiedź, której nie będzie.
    fn announces_interrupt(&self) -> bool {
        self.capabilities
            .get()
            .is_some_and(|announced| announced.iter().any(|name| name == INTERRUPT_CAPABILITY))
    }

    /// Wysyła jedno przerwanie w paśmie. `false`, kiedy nie było czym albo zapis się nie udał —
    /// wołający schodzi wtedy na sygnały, zamiast czekać na odpowiedź, która nie wyjechała.
    ///
    /// Dokładnie **jedna** prośba na anulowanie: powtórzone pytanie, kiedy odpowiedź jest już
    /// w drodze, jest nieodróżnialne od dwóch anulowań i tak samo wygląda w dzienniku CLI.
    async fn ask_to_stop(&mut self) -> bool {
        let Some(voice) = self.voice.as_ref() else {
            return false;
        };
        // Prośba idzie TYM SAMYM kanałem, co koperty tur (powód przy polu `voice`): inaczej
        // dwa pisarze nad jednym potokiem przeplotłyby linie i CLI dostałoby połowę każdej.
        if voice
            .send(ToAgent::Interrupt(format!(
                "req_{}",
                Uuid::now_v7().simple()
            )))
            .await
            .is_err()
        {
            tracing::debug!(
                session = %self.session.id,
                "the interrupt could not be written; falling through to signals"
            );
            return false;
        }
        true
    }
}

#[async_trait]
impl AgentHandle for ClaudeHandle {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }

    fn group(&self) -> Option<GroupId> {
        // Zawsze `Some`: ten sterownik trzyma proces przez całą sesję, więc nie ma chwili
        // „między turami", w której nie byłoby czego zabić. Czyta to T-06 (zapis `pid`/`pgid`
        // przy kroku) i T-20 (sprzątanie po awarii aplikacji).
        Some(self.process.group())
    }

    /// Kolejna tura **tym samym procesem**: koperta na stdin, stdin zostaje otwarty.
    ///
    /// Koperta, jedna linia JSON [T1 §4.6]:
    /// `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"…"}]}}`
    ///
    /// Znak nowej linii jest częścią protokołu, nie formatowaniem: CLI czyta stdin **linia po
    /// linii** i bez niego czeka na resztę koperty w nieskończoność. Serializujemy zamiast
    /// sklejać stringi — prompt z cudzysłowem rozjechałby linię i tura zginęłaby po drugiej
    /// stronie.
    ///
    /// `flush()` po zapisie, bo tura, która utknęła w buforze, wygląda dokładnie tak samo jak
    /// agent, który nie odpowiada.
    async fn send(&mut self, text: String) -> anyhow::Result<()> {
        let voice = self.voice.as_ref().ok_or_else(|| {
            anyhow!(
                "a follow-up turn of {} bytes has nowhere to go: session {} was already closed, \
                 and closing the input is how a session ends",
                text.len(),
                self.session.id
            )
        })?;
        voice
            .send(ToAgent::Turn(text))
            .await
            .map_err(|_| anyhow!("session {} stopped listening", self.session.id))
    }

    fn voice(&self) -> Option<Voice> {
        self.voice.clone()
    }

    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        self.outcomes.recv().await.ok_or_else(|| {
            anyhow!(
                "session {} ended without ever saying how the turn went",
                self.session.id
            )
        })
    }

    /// Trzy stopnie, w tej kolejności i nigdy krócej [T1 §8.5].
    ///
    /// 1. **Tylko** jeśli `init` ogłosił `interrupt_receipt_v1`: `control_request` z podtypem
    ///    `interrupt` na stdin i czekanie ≤5 s. Sesja zostaje wznawialna [T1 §4.6]. Wysłanie
    ///    go tam, gdzie CLI go nie obsługuje, kończy się pięcioma sekundami czekania na
    ///    odpowiedź, która nie przyjdzie — dlatego zdolność, a nie numer wersji [T1 §4.1].
    /// 2. Inaczej, albo po upływie tego okna: SIGTERM na **grupę**. `claude` dosypuje wtedy
    ///    transkrypt, zwalnia zamek sesji i odpala hooki `SessionEnd`, wychodząc 143.
    /// 3. Po oknie łaski: SIGKILL na grupę i **pętla dowodowa**, aż `kill(-pgid, 0)` odpowie
    ///    `ESRCH`. Oba ostatnie kroki to gotowa ścieżka z T-03 — ten plik nie ma prawa znać
    ///    ani jednej stałej sygnału (niezmiennik 3).
    ///
    /// Kiedy proces wyszedł **sam** po przerwaniu, status w dowodzie jest jego własnym kodem
    /// wyjścia, nie sygnałem. To jest jedyny obserwowalny ślad różnicy między wznawialną
    /// sesją a zabitą.
    ///
    /// Stopnie dwa i trzy są w całości z T-03 i wołane są **zawsze**, także po udanym
    /// przerwaniu: `stop()` na grupie, po której nic nie zostało, kosztuje jedno pytanie do
    /// jądra i oddaje ten sam dowód, którego wymaga niezmiennik 6. Skrót „przerwanie się udało,
    /// więc nie pytamy" byłby dokładnie tym `Ok(())`, przed którym stoi `GroupProof`.
    async fn cancel(&mut self) -> GroupProof {
        // Stopień pierwszy — TYLKO pod ogłoszoną zdolnością. Bez tego warunku ta sama linia
        // wisi pięć sekund tam, gdzie CLI o `control_request` nigdy nie słyszało.
        if self.announces_interrupt() && self.ask_to_stop().await {
            // Odpowiedzią jest wyjście sesji: `control_response` przychodzi tuż przed `result`,
            // a proces wychodzi sam. Upłynięcie okna nie jest błędem, tylko zejściem na
            // stopień drugi.
            let _timed_out = timeout(INTERRUPT_WINDOW, self.process.wait()).await;
        }

        self.process.stop(DEFAULT_GRACE).await
    }

    /// Koniec **sesji**, nie tury: dziecko dostaje EOF i wychodzi samo.
    ///
    /// Bez tego czasownika każdy skończony krok zostawiałby żywy proces — `claude` z otwartym
    /// stdinem czeka w nieskończoność [T1 §2].
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        // Porzucenie potoku JEST tym zamknięciem: dziecko dostaje EOF i wychodzi 0. Musi paść
        // przed czekaniem, bo inaczej czekamy na proces, któremu sami nie powiedzieliśmy, że to
        // koniec — i jest to zawieszenie nie do odróżnienia od agenta, który myśli.
        //
        // DWA KROKI, NIE JEDEN, odkąd potok należy do [`talk`]: najpierw ginie nadajnik (pętla
        // pisarza kończy się na zamkniętym kanale), potem czekamy na samo zadanie — i dopiero
        // jego koniec porzuca `ChildStdin`. Pominięcie tego czekania zostawiałoby potok żywy
        // dokładnie tak długo, jak długo zadanie jeszcze nie zdążyło się zejść, czyli dawałoby
        // zawieszenie zależne od pogody.
        drop(self.voice.take());
        /* PROŚBA, NIE CZEKANIE NA CUDZE KLONY. Głos jest klonowalny i produkcja trzyma jego
         * kopię przez całą turę, więc samo porzucenie NASZEGO nadajnika nie kończy pisarza —
         * a wtedy `stdin` żyje, CLI nie dostaje EOF i to czekanie nie wraca nigdy. Zmierzone
         * 2026-08-18 na żywej sesji: 15 minut przy dwóch turach po trzy sekundy. Powód w całości
         * przy `talk`. */
        if let Some(hush) = self.hush.take() {
            // Odbiornik już mógł zniknąć — pisarz kończy się też sam, kiedy CLI przestanie
            // czytać. Wtedy nie ma komu tego powiedzieć i nie ma o czym mówić.
            let _ = hush.send(());
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.await;
        }

        // `None` znaczy „proces zginął od sygnału i kodu po prostu nie ma" — to jest ta sama
        // różnica, którą mierzy dowód z `cancel()`.
        Ok(self.process.wait().await?.code())
    }
}

#[async_trait]
impl AgentDriver for ClaudeDriver {
    fn id(&self) -> &'static str {
        VENDOR
    }

    /// Pyta binarkę o wersję. **Brak pliku to `Ok(Probe { found: false, .. })`, nigdy `Err`**:
    /// nieobecne CLI jest ekranem ustawień, a nie awarią startu aplikacji.
    ///
    /// Nieudany start jest tu odpowiedzią w **każdej** postaci, nie tylko przy braku pliku:
    /// binarka bez prawa wykonania i binarka, której nie ma, znaczą dla użytkownika dokładnie
    /// to samo zdanie („zainstaluj to"), a `Err` z tego miejsca wywala Loadouta, zanim
    /// ktokolwiek zobaczy, co jest do naprawienia.
    async fn probe(&self) -> anyhow::Result<Probe> {
        let mut command = Command::new(&self.binary);
        command.arg("--version");

        // Przez ten sam spawn co bieg, a nie własną komendą obok: `env_clear()` plus jawna lista
        // przepuszczanych zmiennych mieszka w jednym rdzeniu (niezmiennik 23), a `/dev/null` na
        // stdinie oszczędza tu 3 s ostrzeżenia `no stdin data received` [T1 §4.6].
        let mut process = match supervisor::spawn(command, StdinPlan::Null) {
            Ok(process) => process,
            Err(error) => {
                tracing::debug!(
                    binary = %self.binary.display(),
                    %error,
                    "the agent CLI could not be started, so the setup screen has its answer"
                );
                return Ok(Probe {
                    found: false,
                    version: None,
                });
            }
        };

        let mut version = None;
        if let Some(stdout) = process.stdout() {
            version = first_answer(stdout).await;
        }

        // Zebranie procesu jest częścią jego uruchomienia, nie sprzątaniem po nim: zombie nadal
        // odpowiada na sygnał zerowy, więc niezebrany `--version` zostawiłby grupę, której nikt
        // nigdy nie udowodni martwej (niezmiennik 6).
        let _ = process.wait().await;

        Ok(Probe {
            found: true,
            version,
        })
    }

    /// Startuje sesję i zaczyna sypać zdarzeniami na `tx`.
    ///
    /// Kolejność jest wymuszona przez odzyskiwanie po awarii: sesję nadajemy **przed**
    /// startem, `pid` i `pgid` są znane **zanim** cokolwiek zostanie przeczytane ze stdout
    /// [T7 §6.2]. Prompt wchodzi pierwszą kopertą na stdin — nigdy w argv (niezmiennik 9).
    ///
    /// # Stdin zostaje otwarty i to jest cała różnica (2026-08-16)
    ///
    /// `StdinPlan::Keep`, a nie `Write`: deskryptor przeżywa pierwszą kopertę i wraca tu jako
    /// pole uchwytu. Zamknięcie go jest osobnym czasownikiem ([`AgentHandle::close`]), bo znaczy
    /// „koniec sesji", a nie „koniec tury" — i to jest ta jedna rzecz, którą różni się jeden
    /// proces na sesję od wariantu awaryjnego B z T1 §8.1 (nowy proces na turę z `--resume`),
    /// płacącego zimny start i odbudowę cache'u przy **każdej** turze.
    ///
    /// Proces startuje przez `engine::supervisor::spawn` i tylko przez nie: własna grupa
    /// procesów, `env_clear()` i cała eskalacja zabijania mieszkają tam (niezmienniki 3 i 23).
    /// Ten plik nie zna ani jednej stałej sygnału i nie startuje niczego obok nadzorcy.
    async fn start(
        &self,
        spec: RunSpec,
        tx: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        // Plik transkryptu powstaje TUTAJ, przed pierwszym procesem, i błąd jego otwarcia
        // przewraca start kroku (T-34, 2026-08-16). Sterownik poproszony o transkrypt nie ma
        // prawa udawać, że go zrobił: cicha wersja tej porażki jest dokładnie tym, co zmierzył
        // przegląd zewnętrzny — bieg wygląda normalnie, plik nie powstaje, a dowiadujesz się
        // o tym dopiero po skasowaniu `loadout.db`, czyli wtedy, kiedy tych zdarzeń nie ma
        // już nigdzie.
        let transcript = match &self.transcript {
            Some(transcript) => Some(transcript.open().await?),
            None => None,
        };

        // Sesję nadajemy PRZED startem procesu: dopiero to znosi wyścig o to, pod jakim numerem
        // zapisać krok, i dopiero to czyni odzyskiwanie po awarii możliwym [T7 §6.2].
        let session = SessionRef {
            vendor: VENDOR,
            id: spec
                .resume
                .as_ref()
                .map_or_else(|| spec.run_id.to_string(), |session| session.id.clone()),
        };

        let envelope = user_envelope(&spec.prompt)?;
        let mut process = supervisor::spawn(
            self.command(&spec),
            // Prompt wyłącznie tędy (niezmiennik 9). Znak nowej linii jest częścią protokołu:
            // CLI czyta stdin linia po linii i bez niego czekałoby na resztę koperty. `Keep`,
            // bo po tej kopercie przyjdą następne — i przerwanie w paśmie.
            StdinPlan::Keep(format!("{envelope}\n")),
        )?;

        let stdout = process
            .stdout()
            .ok_or_else(|| anyhow!("the agent started without an output stream to read"))?;

        // SKARGI ODBIERAMY I OPRÓŻNIAMY, i to jest jedna z dwóch rzeczy, bez których krok pada
        // zdaniem bez przyczyny (druga to `end_of_stream`, które to zdanie składa). Potok był
        // ustawiany od pierwszego dnia (`supervisor::spawn`, `Stdio::piped()`) i **nie dawał się
        // odebrać**, więc nikt go nie czytał: przyczyna awarii szła do bufora o pojemności
        // ~64 KB, a przy pełnym buforze dziecko blokuje się na `write`.
        //
        // Brak potoku NIE jest tu awarią startu: sonda wersji i część kryteriów sterownika
        // uruchamiają proces bez niego, a agent bez strumienia skarg jest agentem, który po
        // prostu nie ma na co narzekać.
        let complaint = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = process.stderr() {
            let into = Arc::clone(&complaint);
            let _drain = tokio::spawn(drain_complaints(stderr, into));
        }

        let (finished, outcomes) = mpsc::channel(TURNS_IN_FLIGHT);
        // Zdolności ogłasza `init`, czyli strumień — a potrzebuje ich uchwyt, w eskalacji
        // anulowania. Dlatego jedno pudełko, wypełniane przez pętlę czytającą.
        let capabilities = Arc::new(OnceLock::new());
        // Pętla czytająca żyje własnym zadaniem: uchwyt ma zostać responsywny na `cancel()`
        // także wtedy, gdy nikt nie woła `wait()`. Startuje PRZED odebraniem stdinu, bo
        // odebranie stdinu czeka na koniec pierwszego zapisu — a agent, który zaczyna mówić
        // w trakcie, ma mieć kto czytać.
        let _reader = tokio::spawn(pump(
            stdout,
            Arc::clone(&capabilities),
            tx,
            finished,
            transcript,
            complaint,
        ));

        // Ten potok zostaje otwarty aż do `close()`. Bez niego sesja ma dokładnie jedną turę
        // i nie ma czym wysłać przerwania.
        let stdin = process.stdin().await.ok_or_else(|| {
            anyhow!("the agent started without an input channel for the turns that follow")
        })?;

        /* POTOK PRZECHODZI NA WŁASNOŚĆ ZADANIA, a uchwyt dostaje nadajnik. To jest cała zmiana,
         * po której da się napisać do agenta, który właśnie pracuje — powód w całości przy polu
         * `ClaudeHandle::voice` i przy `talk`. */
        let (voice, inbox) = mpsc::channel(SAY_QUEUE);
        let (hush, hushed) = oneshot::channel();
        let writer = tokio::spawn(talk(stdin, inbox, session.id.clone(), hushed));

        Ok(Box::new(ClaudeHandle {
            session,
            process,
            voice: Some(voice),
            writer: Some(writer),
            hush: Some(hush),
            capabilities,
            outcomes,
        }))
    }
}

/// Pierwsza niepusta linia, jaką powiedziała binarka. Tyle wystarczy na pytanie o wersję.
async fn first_answer(stdout: ChildStdout) -> Option<String> {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if !line.is_empty() {
            return Some(line.to_owned());
        }
    }
    None
}
