//! Historia biegów **jednego projektu**: co tu już ruszyło i co z tego wyszło.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! # Po co to powstało (2026-08-23)
//!
//! Zamówienie właściciela: „powinna być opcja zapisu naszych sesji i wyboru z historii,
//! /history komenda np" oraz „pamiętaj że wszystko ma być per workspace ta historia". Ekran
//! pracy trzyma JEDNĄ żywą rozmowę na terminal (`src/sections/run/feed/live.ts`), a ta rozmowa
//! żyje w oknie i nie przeżywa jego przeładowania. Wszystko, co zostaje po biegu, leży na
//! dysku — i do dziś nie było ani jednej komendy, którą okno mogłoby o to zapytać. Pliki
//! powstawały, `store::rebuild` umiał je przeczytać na potrzeby indeksu, a człowiek nie widział
//! z nich ani jednej litery.
//!
//! # PER WORKSPACE ZNACZY: KATALOG TEGO PROJEKTU, NIGDY GLOBALNIE
//!
//! Biegi leżą pod `<projekt>/.loadout/runs/` (`docs/ARCHITECTURE.md` §8), więc „historia" jest
//! z konstrukcji własnością projektu — nie ma tu żadnej listy globalnej do przefiltrowania i to
//! jest właśnie ta własność, której nie wolno zgubić. Katalog dostajemy argumentem, tak samo jak
//! dostaje go `commands::diagnostics` (`ipc::copy_diagnostics`): zakres wybiera człowiek w oknie,
//! a warstwa, która wzięłaby go sobie sama z katalogu procesu, pokazywałaby historię sąsiedniego
//! projektu i nic by o tym nie mówiła.
//!
//! # Jeden nieczytelny bieg to JEDNA POZYCJA, nie zniknięcie i nie awaria listy
//!
//! Niezmiennik 5 postawiony w miejscu, w którym najłatwiej go złamać: `?` na `run.json` zamienia
//! jeden ręcznie edytowany plik w pustą historię całego projektu. Katalog biegu, którego opisu
//! nie da się przeczytać, dostaje więc wiersz z **uczciwym zdaniem** i tym jednym faktem, który
//! da się odczytać zawsze — chwilą, która stoi w nazwie katalogu (`commands::run::stamp`).
//!
//! # Czego ta warstwa świadomie NIE robi
//!
//! - **Nie wznawia biegu.** Odczyt i tylko odczyt; wznowienie jest osobną decyzją produktową
//!   i osobnym zadaniem.
//! - **Nie kuruje po swojemu.** Zapisany strumień kroku przechodzi przez `stream::decode`
//!   i `line::Curator`, czyli przez tę samą maszynę pięciu reguł, którą widzi żywy bieg
//!   (niezmiennik 15 i 23). Druga kuracja pokazywałaby przy tej samej linii inny podział na
//!   grupy, a nic na ekranie nie mówiłoby, który obraz jest prawdziwy.
//! - **Nie zagląda do `loadout.db`.** Pliki są prawdą, baza jest indeksem (niezmiennik 4);
//!   historia czytana z indeksu znikałaby po jego skasowaniu, czyli dokładnie wtedy, kiedy
//!   niezmiennik 4 obiecuje, że nic nie ginie.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::handoffs::{HandoffWire, handoffs_of_run, run_dirs};
use super::isolate;
use crate::engine::drivers::DecodedEvent;
use crate::engine::drivers::claude::ClaudeDecoder;
use crate::engine::drivers::codex::CodexDecoder;
use crate::engine::line::{Curator, Line, Seen, context_per_turn};
use crate::engine::stream::{Decoded, decode};
use crate::inherit::rewrite;

/// Opis biegu. Ta sama nazwa, którą składa `commands::run` — rozjazd znaczy pustą historię.
const RUN_FILE: &str = "run.json";

/// Surowe strumienie agentów, po jednym pliku na krok (`docs/ARCHITECTURE.md` §8).
const LOGS_DIR: &str = "logs";

/// Zdanie dla katalogu biegu, w którym opisu nie ma wcale.
///
/// Nazywa **fakt**, nie plik: człowiek nie ma czego zrobić z nazwą `run.json`, a ma co zrobić
/// z wiedzą, że po tym biegu został sam katalog. Zdanie mówi też, co Loadout mimo to wie,
/// żeby wiersz nie wyglądał na pusty (DESIGN §8).
const NOTHING_KEPT: &str = "Loadout kept no record of this one, so all it can say is when it ran.";

/// Zdanie dla katalogu biegu, którego opis jest, ale nie daje się przeczytać.
///
/// Osobne od [`NOTHING_KEPT`], bo to są dwie różne rzeczy do zrobienia: tam pliku nie ma
/// i nie będzie, tutaj plik leży i da się go obejrzeć.
const RECORD_UNREADABLE: &str =
    "Loadout could not read the record of this one, so all it can say is when it ran.";

/// Bieg tak, jak widzi go lista historii.
///
/// Czego tu nie ma: `workflow_snapshot`, `workflow_hash`, `boot_id`, `route_decisions` i sam
/// `id` biegu. Pole, którego nikt nie czyta, jest polem, które rozjedzie się pierwsze
/// (niezmiennik 21) — a adresem tego biegu jest [`RunWire::folder`], nie uuid: to nazwą katalogu
/// prosi się o niego z powrotem, i to ona jest widoczna w `ls`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunWire {
    /// Nazwa katalogu (`20260816-194804__<uuid>`) — adres, którym okno prosi o ten bieg.
    pub folder: String,
    /// Kiedy ruszył, do przeczytania: `2026-08-16 19:48` (UTC).
    ///
    /// Z NAZWY KATALOGU, nie z `created_at` w środku pliku, i to jest cała treść tego pola:
    /// nazwa jest jedyną rzeczą, która stoi po biegu, którego opisu nie da się przeczytać.
    /// Wiersz z datą i uczciwym zdaniem jest wierszem; wiersz z samym zdaniem jest listą,
    /// z której nie da się nic wybrać.
    pub when: String,
    /// Jak workflow nazywa SAM SIEBIE. Pusty, kiedy opisu nie dało się przeczytać.
    pub title: String,
    /// Nazwa DZISIEJSZEGO pliku workflow, albo pusta, kiedy nie ma go już w bibliotece.
    pub workflow_file: String,
    /// Słowo z drutu: `running`, `paused`, `succeeded`, `failed`, `cancelled`. Pusty, kiedy
    /// opisu nie dało się przeczytać.
    ///
    /// SUROWE, bo tłumaczy je okno (niezmiennik 14 zabrania enuma z drutu na ekranie, a tabela
    /// tłumaczeń mieszka po tamtej stronie granicy, obok pozostałych słów stanu —
    /// `src/sections/run/rail/card.ts`). Napis po angielsku złożony tutaj byłby drugą tabelą.
    pub state: String,
    /// Ile kroków miał ten bieg. Zero znaczy „nie wiadomo", i wtedy stoi obok [`RunWire::said`].
    pub steps: usize,
    /// Ile kosztował — suma kroków, które podały koszt. `None` znaczy „żaden nie podał",
    /// a to jest inna odpowiedź niż zero (niezmiennik 17).
    pub cost_usd: Option<f64>,
    /// Uczciwe zdanie, kiedy opisu biegu nie dało się przeczytać. `None` znaczy „przeczytany".
    pub said: Option<String>,
    /// Co prywatna tura Loadouta zrobiła z tym biegiem — ten sam rachunek, co w [`PastRunWire`].
    ///
    /// 2026-09 (Z-38) — POLE W WIERSZU LISTY, nie tylko w otwartym biegu, i to jest DROGA DANYCH
    /// DO SEKCJI KNOWLEDGE. Kolejka decyzji w Knowledge zapełnia się notatkami, które pisze tura
    /// po biegu; kiedy ta tura zeszła na cenie albo na czasie, kolejka jest pusta z POWODU,
    /// a jedyne miejsce, w którym ten powód istnieje, to `run.json` ostatniego biegu. Bez tego
    /// pola Knowledge musiałoby otworzyć każdy bieg z osobna (`read_run`), żeby dowiedzieć się
    /// czegoś o jednym.
    ///
    /// `None` dla biegu sprzed tego pola i dla biegu, którego opisu nie dało się przeczytać —
    /// tak samo jak w otwartym biegu, i z tego samego powodu (niezmiennik 17: nasza niewiedza
    /// nie jest faktem o biegu).
    pub reflection: Option<ReflectionWire>,
}

/// Otwarty bieg: to samo, co w wierszu listy, plus wszystko, co po nim zostało na dysku.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastRunWire {
    /// WF-19: zweryfikowany adres wejścia, wybierany w formularzu Lab bez ręcznych ID.
    pub saved_input: Option<SavedInputWire>,
    pub saved_input_said: Option<String>,
    /// Nazwa katalogu — ta sama, którą podał wołający.
    pub folder: String,
    /// Kiedy ruszył, do przeczytania.
    pub when: String,
    /// Jak workflow nazywa sam siebie.
    pub title: String,
    /// Słowo z drutu; tłumaczy je okno.
    pub state: String,
    /// Nazwa DZISIEJSZEGO pliku workflow, z którego ten bieg pochodzi — albo pusta, kiedy tego
    /// workflow nie ma już w bibliotece.
    ///
    /// 2026-08-23 — POLE POWSTAŁO Z DEFEKTU ZE ZRZUTU WŁAŚCICIELA: `/stop` odpowiedziało
    /// „Nothing is running." nad pracującym agentem. Bieg wznowiony z historii nie meldował się
    /// oknu, bo okno nie miało czym go nazwać — a „czy coś biegnie" to w całej aplikacji nazwa
    /// pliku (`state/run.ts`). Szukane po IDENTYFIKATORZE z `run.json`, nie po nazwie: nazwa
    /// pliku jest sluggiem tytułu i zmienia się razem z nim.
    pub workflow_file: String,
    /// Kroki w kolejności z `run.json`, czyli w kolejności z grafu.
    pub steps: Vec<PastStepWire>,
    /// Co kroki oddały sobie nawzajem — te same pliki, które pokazuje sekcja przekazań.
    pub handoffs: Vec<HandoffWire>,
    /// Zdanie, kiedy tych przekazań nie dało się przeczytać.
    pub handoffs_said: Option<String>,
    /// Gałęzie, które ten bieg zostawił w repozytorium projektu.
    ///
    /// Pusta lista dla biegu, po którym nie została ani jedna — i to jest zwykły stan: krok,
    /// który nic nie zmienił, gałęzi nie zostawia (`commands::isolate::finish`).
    pub branches: Vec<BranchWire>,
    /// Zachowane foldery mają własną ścieżkę; nieukończone wejście nie jest gotowym wynikiem.
    pub result_folders: Vec<ResultFolderWire>,
    /// Dokładne wyniki z `copy_results`, nie dzisiejsze gałęzie o podobnej nazwie.
    pub saved_results: Vec<Value>,
    /// Co prywatna tura Loadouta zrobiła z tym biegiem — albo `None`, kiedy opis o tym milczy.
    ///
    /// `None`, A NIE WYZEROWANY RACHUNEK, i to jest cała treść tego pola. Bieg zapisany zanim
    /// `run.json` niósł ten klucz nie jest biegiem, którego nie pytano: pierwsze jest naszą
    /// niewiedzą, drugie jest faktem o biegu. Struktura z samymi zerami przedstawiałaby jedno
    /// jako drugie, a te dwa stany mają na ekranie osobne zdania — po jednym w
    /// `src/sections/run/reflection/said.ts`.
    pub reflection: Option<ReflectionWire>,
    /// Uczciwe zdanie, kiedy opisu nie dało się przeczytać.
    pub said: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedInputWire {
    pub source_run_id: String,
    pub snapshot_id: String,
}

/// Dlaczego prywatna tura nie została poproszona.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotAsked {
    Stopped,
    TurnedOff,
    NoAgentWorked,
    NothingWasLeft,
    NothingCameBack,
    /// Tura poszła i zeszła na SWOIM suficie ceny, zanim zdążyła odpowiedzieć.
    ///
    /// 2026-09 (Z-38) — DO DZIŚ CZYTAŁO SIĘ JAK [`NotAsked::NothingCameBack`], czyli jak cisza
    /// modelu. Zmierzone na biegu meetnotes z 2026-09-04: dwie godziny, 57,52 USD, dziewięć
    /// przekazań, transkrypt refleksji kończy się `Reached maximum budget ($0.08)` po 22 s —
    /// a `run.json` mówił „nic nie wróciło". Te dwa fakty mają na ekranie osobne zdania, bo
    /// wymagają od człowieka czego innego: przy jednym nie ma czego szukać, przy drugim
    /// odpowiedź była w drodze i skończyły się pieniądze.
    RanOutOfBudget,
    /// To samo, tylko sufitem był czas ([`crate::commands::run::REFLECTION_MINUTES`]).
    RanOutOfTime,
    /// Aplikacji agenta nie było, nie wstała, albo nie wzięła tury Loadouta.
    ///
    /// 2026-09 (Z-38) — osobno od „nic nie wróciło", bo to jest jedyny z tych kodów, który
    /// mówi o TEJ MASZYNIE, a nie o biegu: nic w katalogu biegu tego nie naprawi.
    NoAgentApp,
    /// 2026-09 (Z-18): nowy kod z przyszłego pliku nie może unieważnić całej historii
    /// (niezmiennik 5), a ekran nie pokaże surowej wartości z drutu (niezmiennik 14).
    #[serde(other)]
    Unknown,
}

/// Rachunek prywatnej tury, tak jak leży w `run.json` i jak jedzie do okna.
///
/// # Dlaczego to nie jest `commands::run::ReflectionReceipt`
///
/// Bo tamten typ jest **pisarzem** i jest prywatny dla swojego modułu: niesie też cenę tury,
/// której dziś nie ma na żadnym ekranie, i ma prawo rosnąć razem z biegiem. Ten jest
/// **czytelnikiem** i czyta pliki, które powstały wcześniej — więc każde pole ma `#[serde(default)]`
/// (niezmiennik 5) i żadne z nich nie jest wymagane, żeby historia dała się otworzyć.
///
/// # KLUCZE W PLIKU SĄ MIESZANE i to nie jest przeoczenie do naprawienia tutaj
///
/// `ReflectionReceipt` serializuje `ran`, `kept`, `discardedAgain` (jawny `rename`)
/// i `dropped_without_reason` (bez renamu). Zmiana tamtej nazwy jest naprawą pisarza, a nie
/// czytelnika, i uczyniłaby nieczytelnym każdy `run.json` zapisany do dziś. Czytelnik przyjmuje
/// więc OBIE pisownie: `camelCase` z `rename_all` dla drutu do okna i `alias` na tę jedną
/// pisownię, którą pisarz naprawdę wypisuje.
/// 2026-09 (Z-38) — `Eq` ZESZŁO Z TEJ LISTY razem z `budget_usd`: kwota jest `f64`, a `f64`
/// nie jest `Eq`. Nikt tego typu nie porównywał na równość poza asercjami testów, którym
/// `PartialEq` wystarcza.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionWire {
    /// Czy tura naprawdę poszła i wróciła użyteczną odpowiedzią.
    #[serde(default)]
    pub ran: bool,
    /// Ile notatek z niej powstało — te czekają w Memory na decyzję człowieka.
    #[serde(default)]
    pub kept: usize,
    /// Ile wróciło takich, które człowiek już raz odrzucił.
    #[serde(default)]
    pub discarded_again: usize,
    /// Ile reguł przyszło bez uzasadnienia; takich nie zapisujemy [T6 §10.3].
    #[serde(default, alias = "dropped_without_reason")]
    pub dropped_without_reason: usize,
    /// Dlaczego prywatna tura nie poszła. Brak znaczy, że starszy plik tego nie zapisał.
    #[serde(default)]
    pub why: Option<NotAsked>,
    /// Sufit ceny, który tę turę obowiązywał — w dolarach.
    ///
    /// 2026-09 (Z-38) — BEZ TEJ LICZBY ZDANIE O ZEJŚCIU NA SUFICIE NIE MA CZYM SIĘ SKOŃCZYĆ.
    /// Sufit nie jest już stałą: skaluje się z tym, co bieg wydał na kroki
    /// ([`crate::commands::run::REFLECTION_BUDGET_USD`] jest jego podłogą), więc kwota
    /// przepisana do okna z jednej stałej byłaby prawdą wyłącznie dla najtańszego biegu.
    /// `alias`, bo pisarz wypisuje ten klucz w `snake_case` (powód wyżej, przy `run.json`).
    #[serde(default, alias = "budget_usd")]
    pub budget_usd: Option<f64>,
}

/// Jedna gałąź zostawiona przez bieg.
///
/// DWA POLA, BO CZŁOWIEK POTRZEBUJE OBU. Nazwa jest tym, co wpisze w gita; krok jest tym, po
/// czym pozna, o którą pracę chodzi — gałęzie jednego biegu różnią się ostatnim członem i czyta
/// się je jak jedną kolumnę tego samego napisu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchWire {
    /// Pełna nazwa: `loadout/<bieg>/<kafelek>`. Ta sama, którą składa `isolate::branch_for`.
    pub name: String,
    /// Nazwa kroku, który ją zostawił — ta z kafelka, nie klucz z pliku (niezmiennik 14).
    ///
    /// Pusta, kiedy `run.json` tego kroku już nie zna: gałąź zostaje wtedy nazwana samą sobą,
    /// bo istnieje naprawdę i człowiek ma prawo ją zdjąć.
    pub step: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultFolderWire {
    pub work_key: String,
    pub step: String,
    pub path: PathBuf,
    pub state: String,
}

/// Krok otwartego biegu.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastStepWire {
    /// Identyfikator kroku z `run.json` — po nim nazywa się plik jego strumienia.
    ///
    /// Jest UNIKALNY W BIEGU i tylko w nim: nadaje go planista przy starcie. Do wskazania
    /// kafelka **nie służy** — od tego jest [`PastStepWire::tile`].
    pub id: String,
    /// Klucz kafelka Z PLIKU workflow — po nim wznawia się bieg od tego miejsca.
    ///
    /// 2026-08-23 — POLE POWSTAŁO Z DEFEKTU ZE ZRZUTU WŁAŚCICIELA. „Pick up here" podawał dalej
    /// `id`, czyli UUID nadany przy planowaniu, a wznowienie szuka kroku po kluczu z pliku —
    /// więc odmawiało zdaniem *„01a02b3c-… is not a step in that workflow any more"* o kroku,
    /// który stoi na płótnie i nigdzie się nie ruszył. Dwa identyfikatory jednego kroku muszą
    /// jechać jako DWA POLA: jedno pole robiące dwie rzeczy jest dokładnie tym, co ten defekt
    /// pokazał.
    ///
    /// Pusty znaczy „ten `run.json` nie mówi, z którego kafelka ten krok powstał" — wtedy nie ma
    /// czego wskazać i okno nie rysuje przycisku (`past/panel.tsx`).
    pub tile: String,
    /// Nazwa kafelka. Ta sama, którą człowiek widzi na płótnie i w podpisie każdej linii.
    pub name: String,
    /// Nazwa agenta, który go wykonał.
    pub agent: String,
    /// Słowo z drutu; tłumaczy je okno.
    pub state: String,
    /// `None` dla starych receipts: historia nie zgaduje wykonania ze statusu, czasu ani PID-u.
    pub executed: Option<bool>,
    /// Jedno zdanie, które ten krok po sobie zostawił. Puste, kiedy nie zostawił żadnego.
    pub summary: String,
    /// Powód, jeśli coś poszło nie tak. Pusty, kiedy poszło dobrze.
    pub error: String,
    /// Czyjego wyniku ten krok nie miał, choć pojechał dalej — po jednym zdaniu na poprzednika.
    ///
    /// 2026-09 (Z-39) — OSOBNO OD `error`, bo mówi o czym innym: tamto jest powodem, dla którego
    /// TEN krok nie przeszedł, a to jest zdaniem o materiale, którego nie dostał. Krok, który
    /// pojechał na `carry-on` i sam się udał, ma `error` puste i to zdanie niepuste — a bez tego
    /// rozdzielenia wyglądałby na krok, który po prostu tak odpowiedział.
    ///
    /// Pusta lista jest jawna także dla starych biegów, żeby granica TypeScript nie musiała
    /// zgadywać, czy pole zaginęło (ten sam wybór, co przy [`PastStepWire::memory`] niżej).
    pub ran_without: Vec<String>,
    /// Ile kosztował ten krok. `None` znaczy „nie podał", nie zero.
    pub cost_usd: Option<f64>,
    /// Zdanie o kontekście na wewnętrzną turę vendora. `None`, kiedy vendor nie podał tur
    /// albo liczniki są puste; historia nie zgaduje jednej tury Codeksa (2026-09, Z-48).
    pub context_per_turn: Option<String>,
    /// Zamrożony receipt wyłącznie TEGO fizycznego kroku. Pusta lista jest jawna także dla
    /// starych biegów, żeby granica TypeScript nie musiała zgadywać, czy pole zaginęło.
    pub memory: Vec<PastMemoryWire>,
    /// WF-12: dokładne tekstowe źródła ze zwalidowanego, zamrożonego pakietu.
    /// Nie jest to raport natywnego autoładowania aplikacji agenta.
    pub project_instructions: Vec<crate::inherit::instructions::InstructionSource>,
    /// Co aplikacja agenta wczytała z folderu tego kroku sama z siebie.
    ///
    /// `None` — a nie pusty rekord — dla każdego kroku, który tego nie ogłosił: kafelka
    /// kontrolnego, kroku „sprawdź", Codeksa i każdego biegu zapisanego przed 2026-09 (Z-16).
    /// Ekran ma te dwa stany rozróżniać, bo „nic stąd nie wczytał" i „nie wiemy" to dwa różne
    /// zdania (niezmiennik 17).
    pub loaded_by_the_app: Option<LoadedByTheAppWire>,
    /// Jedno zdanie o tym, co ten krok wczytał z folderu POZA tym, co włożył mu bieg.
    ///
    /// `None` znaczy „nie ma o czym mówić" i mówi to o trzech różnych stanach naraz: krok bez
    /// rekordu, rekord z pustymi listami i rekord, w którym stoi wyłącznie własność biegu.
    /// Dla patrzącego są jednym — z tego folderu nie przyszło nic, czego Loadout nie dał —
    /// a zdanie „przeczytał 0 rzeczy" byłoby wierszem bez treści przy każdym kroku każdego
    /// biegu (niezmiennik 17 i 16).
    ///
    /// LICZONE PRZY ODCZYCIE, nie zapisane w `run.json` (2026-09, Z-47): porównanie potrzebuje
    /// wyłącznie nazw, którymi bieg sam przypiął swoje rzeczy, więc bieg zapisany przed tą
    /// zmianą dostaje to zdanie tak samo jak dzisiejszy (niezmiennik 4).
    pub what_loadout_did_not_give: Option<String>,
    /// Zapisany strumień tego kroku, przepuszczony przez TĘ SAMĄ kurację, co żywy bieg.
    ///
    /// 2026-08-23 (T-95) — POPRAWIONY AKAPIT, BO POPRZEDNI BYŁ NIEPRAWDĄ. Stało tu, że
    /// „`commands::run` nie woła `ClaudeDriver::with_transcript`, więc `logs/agent-<krok>.jsonl`
    /// nie powstaje po żadnym prawdziwym biegu". Powstaje: od T-34 pisze go [`crate::evidence`],
    /// któremu bieg daje katalog i identyfikator kroku, i dzieje się to w KAŻDYM biegu — mówi
    /// to wprost nagłówek `commands/run.rs`. Ten sam zapis widziano na biegu właściciela
    /// `20260823-011240`, gdzie pliki kroków ważyły od 17 do 61 kB.
    ///
    /// Pusty jest więc dalej normalną odpowiedzią, ale z innego powodu: krok anulowany albo
    /// pominięty nie zdążył nic nadać, a po kroku bez agenta nie ma czego zapisywać. Zdanie
    /// odwrotne kosztowało tyle, ile kosztują wszystkie: uczyło następnego czytelnika szukać
    /// szwu, który już istnieje.
    pub lines: Vec<Line>,
}

/// Co aplikacja agenta dobrała sobie z folderu kroku — czytane z `run.json`, oddawane oknu.
///
/// # Jeden typ, dwie pisownie, i to nie jest sprytność
///
/// Na dysku klucze są `snake_case`, bo tak pisze je `commands::run::StepEntry` i tak czyta je
/// `store::rebuild` — rozjazd znaczy bieg, którego po skasowaniu bazy nie da się odtworzyć
/// (niezmiennik 4). Do okna jadą `camelCase`, jak cała reszta tego drutu. Drugi typ na tę samą
/// treść byłby drugim miejscem, w którym trzeba dopisać pole — a to jest dokładnie ta para,
/// która rozjeżdża się po cichu.
///
/// **Nazwy, nigdy ścieżki.** Powód w całości stoi przy `engine::drivers::LoadedFromTheFolder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct LoadedByTheAppWire {
    /// Nazwa katalogu, z którego to przyszło.
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub plugins: Vec<String>,
    #[serde(default)]
    pub slash_commands: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub memory_paths: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
}

/// Jedna zamrożona notatka przypięta do fizycznego kroku z `run.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastMemoryWire {
    pub reference: String,
    pub hash: String,
    pub bytes: usize,
    pub address: super::memory::NoteAddress,
    pub project: Option<String>,
    pub from: Option<String>,
    /// `true` znaczy, że ówczesny limit odłożył notatkę; nie wolno przedstawiać jej jako wiedzy.
    pub left_out: bool,
}

/// Czego okno nie dostało, bo nie dało się tego przeczytać.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// Nazwa, która nie jest nazwą jednego katalogu w `runs/`.
    ///
    /// Zapora na wędrówkę po ścieżkach, nie kosmetyka: `..` i ukośnik w tej nazwie czytałyby
    /// dowolny plik na dysku człowieka, bo nazwa przyjeżdża z okna, a okno rysuje ją z tego,
    /// co ktoś wpisał w wiersz wejścia.
    #[error("\"{asked}\" is not the name of one run in this folder.")]
    NotOneName { asked: String },

    /// Katalogu o tej nazwie w tym projekcie nie ma.
    #[error("There is no run called \"{asked}\" in this folder.")]
    NoSuchRun { asked: String },

    /// Ktoś na tej gałęzi w tej chwili pracuje.
    ///
    /// ODMOWA JEST CAŁOŚCIOWA i to jest treść tego wariantu: kiedy jedna z gałęzi biegu jest
    /// wyjęta do pracy, nie znika ani jedna. Połowa zdjęta i połowa nie byłaby stanem, o którym
    /// człowiek dowiaduje się dopiero z `git branch` — a zdjęcie komuś gałęzi spod ręki jest
    /// jedyną rzeczą, którą ta droga mogłaby zepsuć nieodwracalnie.
    #[error(
        "\"{branch}\" is checked out in another folder right now, so Loadout left every branch \
         of this run alone. Finish there, then try again."
    )]
    BranchIsOpen { branch: String },

    /// Git odmówił zdjęcia gałęzi z powodu, którego nie umiemy przewidzieć.
    ///
    /// Niesiemy jego własne zdanie, bo jest konkretniejsze niż nasze. Gałęzie zdjęte przed tą
    /// zniknęły naprawdę; lista w panelu zgadza się znowu po ponownym otwarciu biegu, bo to
    /// pliki są prawdą (niezmiennik 4).
    #[error("Loadout could not take \"{branch}\" away: {said}")]
    CouldNotForget { branch: String, said: String },

    /// Gałęzie zeszły, katalogu nie dało się zdjąć.
    ///
    /// **ZE ŚCIEŻKĄ**, z tego samego powodu, z którego niesie ją zdanie o nieposprzątanym drzewie
    /// (`isolate::could_not_tidy`): bez niej człowiek dowiaduje się, że coś zostało, i musi sam
    /// znaleźć jeden katalog wśród kilkudziesięciu innych.
    #[error("Loadout could not take the folder of this run away: {said}. It is still here: {path}")]
    CouldNotForgetRun { path: String, said: String },

    /// Zwykła retencja i Forget bez osobnej listy potwierdzeń nie usuwają jedynego wyniku.
    #[error("{said}")]
    ResultsAreKept { said: String },
    #[error("Loadout could not open this result folder: {said}")]
    CouldNotOpenResult { said: String },
}

/// Wszystkie biegi TEGO projektu, od najnowszego. Projekt bez `runs/` daje pustą listę.
///
/// **Nie oddaje `Result`**, i to jest decyzja, nie skrót. Jedynym powodem, dla którego ta
/// funkcja mogłaby się nie udać, jest nieczytelny pojedynczy bieg — a on ma być WIERSZEM
/// (patrz nagłówek modułu). Świeża maszyna bez ani jednego biegu jest stanem normalnym, nie
/// awarią dysku: czerwony pasek na świeżej instalacji uczy człowieka ignorować czerwone paski.
#[must_use]
pub fn list_runs_inner(project: &Path) -> Vec<RunWire> {
    /* 2026-09 (Z-27): indeks biblioteki powstaje RAZ na listę. `summary` wołane dla każdego
     * biegu nie może za każdym razem ponownie czytać całego katalogu workflow — przy 87 biegach
     * koszt wejścia na ekran rósłby do 87 pełnych skanów tych samych plików. */
    let workflows = workflow_files();
    run_dirs(project)
        .iter()
        .map(|dir| summary(dir, &workflows))
        .collect()
}

/// Jeden bieg, otwarty do odczytu: jego kroki, ich strumienie i jego przekazania.
///
/// `run` jest nazwą katalogu z [`RunWire::folder`]. Sprawdzamy ją, zanim dotkniemy dysku
/// (patrz [`HistoryError::NotOneName`]), bo przyjeżdża z okna.
pub fn read_run_inner(project: &Path, run: &str) -> Result<PastRunWire, HistoryError> {
    let dir = one_run_dir(project, run)?;
    let (saved_input, saved_input_said) = saved_input_in(project, &dir);

    let workflows = workflow_files();
    let head = summary(&dir, &workflows);
    let described = read_description(&dir);
    let (instruction_package, instruction_problem) = if described
        .as_ref()
        .is_some_and(|file| file.project_instructions.is_some())
        || fs::symlink_metadata(dir.join("instructions/manifest.json")).is_ok()
    {
        match crate::inherit::instructions::read_snapshot(&dir) {
            Ok(package) => (Some(package), None),
            Err(error) => (
                None,
                Some(format!(
                    "The saved project instructions could not be verified: {error}"
                )),
            ),
        }
    } else {
        (None, None)
    };
    let steps = past_steps(
        project,
        &dir,
        described.as_ref(),
        instruction_package.as_ref(),
    );
    // PO KROKACH, bo gałąź nazywa się kluczem kafelka, a człowiek czyta nazwy. Przed budową
    // struktury, bo `steps` idzie do niej przez przeniesienie.
    let branches = described
        .as_ref()
        .map_or_else(Vec::new, |file| branches_of_run(project, &file.id, &steps));
    // PRZED `described.map(…)` niżej, bo tamto przenosi opis. Bieg, którego opisu nie dało się
    // przeczytać, oddaje tu `None` — i to jest ta sama odpowiedź, co dla pliku bez tego klucza:
    // w obu przypadkach po prostu nie wiemy, i tak ma to zabrzmieć na ekranie.
    let reflection = described.as_ref().and_then(|file| file.reflection);
    let saved_results = described
        .as_ref()
        .map(|file| super::result_restore::saved_results(project, &file.id))
        .transpose()
        .map_err(|said| HistoryError::ResultsAreKept { said })?
        .unwrap_or_default();
    let (handoffs, handoffs_said) = match handoffs_of_run(project, &dir) {
        Ok(handed) => (handed, None),
        Err(said) => (Vec::new(), Some(said)),
    };
    let (result_folders, result_problem) = match super::run::kept_folders_in(&dir) {
        Ok(folders) => (
            folders
                .into_iter()
                .map(|one| {
                    let step = steps
                        .iter()
                        .find(|step| step.tile == one.work_key)
                        .map_or_else(|| "Working copy".to_owned(), |step| step.name.clone());
                    let state = match one.state {
                        super::run::KeptFolderState::Changed => "changed",
                        super::run::KeptFolderState::Uncertain => "uncertain",
                        super::run::KeptFolderState::Incomplete => "incomplete",
                    }
                    .to_owned();
                    ResultFolderWire {
                        work_key: one.work_key,
                        step,
                        path: one.path,
                        state,
                    }
                })
                .collect(),
            None,
        ),
        Err(error) => (
            Vec::new(),
            Some(format!(
                "Loadout could not read the kept folders: {error}. Nothing was removed."
            )),
        ),
    };
    let result_problem = match (result_problem, instruction_problem) {
        (Some(one), Some(other)) => Some(format!("{one} {other}")),
        (one, other) => one.or(other),
    };
    let said = match (head.said, result_problem) {
        (Some(one), Some(other)) => Some(format!("{one} {other}")),
        (one, other) => one.or(other),
    };

    Ok(PastRunWire {
        saved_input,
        saved_input_said,
        folder: head.folder,
        when: head.when,
        title: head.title,
        state: head.state,
        workflow_file: head.workflow_file,
        steps,
        // Przekazania są prawdziwe niezależnie od `run.json`: to osobne pliki z własnym
        // front-matterem, więc bieg z zepsutym opisem nadal pokazuje, co jego kroki oddały.
        handoffs,
        // Osobne od `said`: tamto mówi wyłącznie o nieczytelnym `run.json`, a jedno pole na oba
        // fakty złamałoby zasadę jednego miejsca dla jednego faktu (niezmiennik 13).
        handoffs_said,
        branches,
        result_folders,
        saved_results,
        reflection,
        said,
    })
}

fn saved_input_in(project: &Path, dir: &Path) -> (Option<SavedInputWire>, Option<String>) {
    match verified_saved_input(project, dir) {
        Ok(input) => (Some(input), None),
        Err(detail) => (
            None,
            Some(format!(
                "Saved starting files are unavailable: {detail}. Choose another run."
            )),
        ),
    }
}

fn verified_saved_input(project: &Path, dir: &Path) -> Result<SavedInputWire, String> {
    // 2026-09-06: picker nie składa ID z własnej konwencji w JS. Ten sam resolver co replay
    // sprawdza unikalny katalog i zgodność ID w odczytanych no-follow bajtach run.json.
    let folder = file_name(dir);
    let (_, id) = folder
        .rsplit_once("__")
        .ok_or_else(|| "this older run has no saved input address".to_owned())?;
    let source = super::lead_history::source_for(project, id)?;
    if source.run_folder != folder {
        return Err("the saved run address no longer matches this folder".to_owned());
    }
    let held =
        crate::engine::supervisor::PublicationRoot::open(dir).map_err(|error| error.to_string())?;
    let bytes = super::lead_history::source_bytes(project, &source)?;
    let record: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "the saved description cannot be read".to_owned())?;
    let expected = record
        .pointer("/input_snapshot/id")
        .and_then(Value::as_str)
        .ok_or_else(|| "this run did not record its starting files".to_owned())?;
    if record
        .pointer("/input_snapshot/manifest")
        .and_then(Value::as_str)
        .is_some_and(|manifest| manifest != "input/manifest.json")
    {
        return Err("the saved description points to a different input folder".to_owned());
    }
    // Ten odczyt sprawdza pełny manifest i wszystkie pliki, nie sam istniejący wpis/UUID.
    let snapshot = super::input_snapshot::read(dir).map_err(|error| error.to_string())?;
    if snapshot.id() != expected {
        return Err("the starting files do not match this run's saved input".to_owned());
    }
    held.validate_path_identity(dir)
        .map_err(|error| error.to_string())?;
    Ok(SavedInputWire {
        source_run_id: source.run_id,
        snapshot_id: snapshot.id().to_owned(),
    })
}

/// WF-06: Finder dostaje wyłącznie folder wymieniony przez ten sam rdzeń co historia.
/// Nie przyjmujemy dowolnej ścieżki z okna i nie rozwiązujemy linku podstawionego pod kopię.
pub fn result_folder_inner(
    project: &Path,
    run: &str,
    work_key: &str,
) -> Result<PathBuf, HistoryError> {
    let dir = one_run_dir(project, run)?;
    super::run::kept_folders_in(&dir)
        .map_err(|error| HistoryError::CouldNotOpenResult {
            said: error.to_string(),
        })?
        .into_iter()
        .find(|one| one.work_key == work_key)
        .map(|one| one.path)
        .ok_or_else(|| HistoryError::CouldNotOpenResult {
            said: "that folder is not a saved result of this run".to_owned(),
        })
}

/// Zdejmuje gałęzie, które ten bieg zostawił — i **tylko** jego. Oddaje nazwy tych, których
/// już nie ma.
///
/// # 2026-08-23 (T-95) — druga połowa sprzątania po biegu
///
/// Katalog roboczy kroku znika zaraz po biegu, bo praca jest osiągalna z gałęzi
/// (`commands::isolate::finish`). Gałęzie zostawały natomiast na zawsze i nic nie umiało ich
/// zdjąć poza ręcznym `git branch -D` na każdą z osobna — a po tygodniu pracy `git branch`
/// przestaje być do przeczytania i gałąź niosąca coś ważnego ginie wśród kilkudziesięciu.
///
/// # PRZEDROSTEK JEST CAŁĄ ZAPORĄ, i składa go ta sama funkcja, która nadaje nazwy
///
/// `isolate::branch_for(id, "")` daje `loadout/<bieg>/`, więc „które gałęzie są tego biegu" ma
/// jedną odpowiedź i nie da się jej rozjechać z nazywaniem (niezmiennik 13). Napis sklejony tu
/// z palca byłby drugą regułą na to samo pytanie — a ta droga KASUJE, więc pomyłka w niej
/// zdejmuje cudzą gałąź.
///
/// # Bieg, którego opisu nie da się przeczytać, nie zdejmuje niczego
///
/// Bez `run.json` nie ma identyfikatora, a bez identyfikatora nie ma przedrostka. Zgadywanie
/// z nazwy katalogu byłoby drugim źródłem prawdy o tym, jak nazywa się gałąź tego biegu.
pub fn forget_run_branches_inner(project: &Path, run: &str) -> Result<Vec<String>, HistoryError> {
    let dir = one_run_dir(project, run)?;
    let _held = super::result_restore::removal_guard(&dir).map_err(|error| {
        HistoryError::ResultsAreKept {
            said: error.to_string(),
        }
    })?;
    forget_run_branches_while_locked(project, &dir)
}

fn forget_run_branches_while_locked(
    project: &Path,
    dir: &Path,
) -> Result<Vec<String>, HistoryError> {
    let Some(prefix) = read_description(dir).and_then(|file| run_prefix(&file.id)) else {
        return Ok(Vec::new());
    };

    let mine = isolate::branches_under(project, &prefix);
    // PYTAMY, ZANIM ZDEJMIEMY COKOLWIEK. Sprawdzanie po drodze zostawiłoby stan, w którym część
    // gałęzi zniknęła, a odmowa mówi o jednej — czyli człowiek czyta „nic nie ruszyłem" nad
    // repozytorium, w którym coś już zniknęło.
    let in_use = isolate::branches_in_use(project);
    if let Some(busy) = mine.iter().find(|name| in_use.contains(name)) {
        return Err(HistoryError::BranchIsOpen {
            branch: busy.clone(),
        });
    }

    let mut gone = Vec::new();
    for name in mine {
        isolate::drop_branch(project, &name).map_err(|said| HistoryError::CouldNotForget {
            branch: name.clone(),
            said,
        })?;
        gone.push(name);
    }
    Ok(gone)
}

/// Zapomina o biegu w całości: jego gałęzie i jego katalog. Oddaje nazwy zdjętych gałęzi.
///
/// # 2026-09 (Z-9) — TRZECIA POŁOWA SPRZĄTANIA, i pierwsza, która zdejmuje KATALOG
///
/// Do tego dnia dało się zdjąć gałęzie biegu ([`forget_run_branches_inner`]) i nie dało się zdjąć
/// jego katalogu **niczym**: ani po biegu, ani przy otwarciu folderu, ani przyciskiem. Katalog
/// biegu niesie strumienie agentów, przekazania i kopie notatek — u właściciela 2026-09-02 było
/// ich w jednym projekcie 87, na 3,8 GB, i jedyną drogą był `rm -rf` z terminala.
///
/// # GAŁĘZIE PIERWSZE, i kolejność jest tu całą treścią
///
/// Po dwa niezależne powody. **Ostrożność:** [`forget_run_branches_inner`] odmawia CAŁOŚCIOWO,
/// kiedy którakolwiek gałąź tego biegu jest w tej chwili wyjęta do pracy — a odmowa po skasowaniu
/// katalogu byłaby zdaniem „nic nie ruszyłem" nad projektem, w którym zniknęła już historia biegu.
/// **Adresowanie:** przedrostek gałęzi bierze się z `run.json` (`isolate::branch_for` po `id`
/// biegu), więc po skasowaniu katalogu nie ma z czego go policzyć i gałęzie zostałyby na zawsze,
/// bez niczego, co je jeszcze wymienia.
pub fn forget_run_inner(project: &Path, run: &str) -> Result<Vec<String>, HistoryError> {
    forget_run_with_results_inner(project, run, None)
}

/// Osobna zgoda zawiera dokładnie obecne ścieżki wyników, nie flagę „force”. Zmiana listy
/// między pokazaniem a kliknięciem odmawia całej operacji i wymaga świeżego potwierdzenia.
pub fn forget_run_with_results_inner(
    project: &Path,
    run: &str,
    confirmed: Option<&[PathBuf]>,
) -> Result<Vec<String>, HistoryError> {
    // Po nazwę katalogu pytamy TĘ SAMĄ funkcję, co wszystko inne w tym module: zapora na
    // wędrówkę po ścieżkach mieszka w jednym miejscu, a ta droga kasuje rekurencyjnie.
    let dir = one_run_dir(project, run)?;
    if let Some(said) = super::result_restore::removal_blocker(&dir).map_err(|error| {
        HistoryError::ResultsAreKept {
            said: error.to_string(),
        }
    })? {
        return Err(HistoryError::ResultsAreKept { said });
    }
    if let Some(said) =
        super::run::copy_lifetime_blocker(&dir).map_err(|error| HistoryError::ResultsAreKept {
            said: format!("Loadout cannot prove these result folders are safe to remove: {error}"),
        })?
    {
        return Err(HistoryError::ResultsAreKept { said });
    }
    let blocked =
        super::run::retention_blocker(&dir).map_err(|error| HistoryError::ResultsAreKept {
            said: format!("Loadout cannot prove these result folders are safe to remove: {error}"),
        })?;
    if blocked.is_some() || confirmed.is_some() {
        let said = blocked.unwrap_or_else(|| "The list of saved result folders changed. Open this run again before confirming their removal.".to_owned());
        if super::run::pending_copy_finalization(&dir).unwrap_or(true) {
            return Err(HistoryError::ResultsAreKept { said });
        }
        let kept =
            super::run::kept_folders_in(&dir).map_err(|error| HistoryError::ResultsAreKept {
                said: error.to_string(),
            })?;
        let mut expected: Vec<_> = kept.into_iter().map(|one| one.path).collect();
        expected.sort();
        let mut confirmed = confirmed.map(<[PathBuf]>::to_vec).unwrap_or_default();
        confirmed.sort();
        if confirmed != expected {
            return Err(HistoryError::ResultsAreKept { said });
        }
    }
    let _held = super::result_restore::removal_guard(&dir).map_err(|error| {
        HistoryError::ResultsAreKept {
            said: error.to_string(),
        }
    })?;
    let gone = forget_run_branches_while_locked(project, &dir)?;
    fs::remove_dir_all(&dir).map_err(|error| HistoryError::CouldNotForgetRun {
        path: dir.display().to_string(),
        said: error.to_string(),
    })?;
    Ok(gone)
}

/// Przedrostek gałęzi tego biegu, albo `None` dla biegu bez identyfikatora.
///
/// `None`, a nie `"loadout//"`: pusty człon środkowy dawałby wzorzec pasujący do gałęzi
/// KAŻDEGO biegu, czyli przycisk „zapomnij o gałęziach tego biegu" zdejmowałby wszystkie.
fn run_prefix(run_id: &str) -> Option<String> {
    (!run_id.trim().is_empty()).then(|| isolate::branch_for(run_id, ""))
}

/// Gałęzie tego biegu, nazwane krokiem, który je zostawił.
///
/// Krok bierzemy z kroków biegu, bo w nazwie gałęzi stoi KLUCZ kafelka, a klucza nie ma na
/// ekranie (niezmiennik 14). Pusty, kiedy `run.json` tego kroku już nie zna: gałąź istnieje
/// naprawdę i człowiek ma prawo ją zobaczyć także wtedy, gdy nie umiemy jej podpisać.
fn branches_of_run(project: &Path, run_id: &str, steps: &[PastStepWire]) -> Vec<BranchWire> {
    let Some(prefix) = run_prefix(run_id) else {
        return Vec::new();
    };
    isolate::branches_under(project, &prefix)
        .into_iter()
        .map(|name| {
            let tile = name.strip_prefix(&prefix).unwrap_or_default();
            let step = steps
                .iter()
                .find(|one| one.tile == tile)
                .map(|one| one.name.clone())
                .unwrap_or_default();
            BranchWire { name, step }
        })
        .collect()
}

/// Katalog JEDNEGO biegu tego projektu, po nazwie z okna.
///
/// Zapora na wędrówkę po ścieżkach stoi tutaj, w jednym miejscu dla obu wołających: nazwa
/// przyjeżdża z okna, a okno rysuje ją z tego, co ktoś wpisał w wiersz wejścia. Katalog bierzemy
/// z LISTY, nie ze sklejenia ścieżki — lista jest tym samym zbiorem, który widzi człowiek, więc
/// nie da się poprosić o katalog, którego nie było na ekranie. Sklejenie przechodziłoby także
/// dla katalogu, który biegiem nie jest.
fn one_run_dir(project: &Path, run: &str) -> Result<PathBuf, HistoryError> {
    let asked = run.trim();
    if !is_one_name(asked) {
        return Err(HistoryError::NotOneName {
            asked: asked.to_owned(),
        });
    }
    run_dirs(project)
        .into_iter()
        .find(|path| file_name(path) == asked)
        .ok_or_else(|| HistoryError::NoSuchRun {
            asked: asked.to_owned(),
        })
}

/// Opis biegu z `run.json` — dokładnie te pola, które ktoś czyta.
///
/// Nieznanych kluczy **nie odrzucamy** (niezmiennik 5): plik zapisany przez nowszego Loadouta
/// ma się dać przeczytać, a nie wywrócić historię. Każde pole poza `steps` jest opcjonalne
/// z tego samego powodu — plik po ręcznej edycji zostaje wierszem, a nie znika.
#[derive(Debug, Deserialize)]
struct Description {
    /// Identyfikator TEGO biegu. Z niego składa się przedrostek jego gałęzi.
    #[serde(default)]
    id: String,
    /// Identyfikator workflow, z którego ten bieg poszedł.
    #[serde(default)]
    workflow_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    steps: Vec<StepDescription>,
    /// Addytywny receipt T-130. Brak pola w starym pliku jest pustą listą, nie błędem historii.
    #[serde(default)]
    memory: Vec<MemoryDescription>,
    #[serde(default)]
    project_instructions: Option<serde_json::Value>,
    /// Rachunek prywatnej tury (T-165). Brak klucza znaczy „ten plik o tym nie mówi", i to jest
    /// inne zdanie niż rachunek zerowy — dlatego `Option`, a nie wartość domyślna struktury.
    #[serde(default)]
    reflection: Option<ReflectionWire>,
}

/// Tolerancyjny kształt rekordu z `run.json`.
///
/// Adres jest opcjonalny wyłącznie podczas deserializacji: stary wpis bez niego pozostaje
/// czytelny, lecz nie udaje notatki, którą da się bezpiecznie pokazać na ekranie.
#[derive(Debug, Deserialize)]
struct MemoryDescription {
    #[serde(default)]
    reference: String,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    bytes: usize,
    #[serde(default)]
    address: Option<super::memory::NoteAddress>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    recipients: Vec<String>,
    #[serde(default, rename = "leftOutFor")]
    left_out_for: Vec<String>,
}

fn memory_for_step(memory: &[MemoryDescription], step: &str) -> Vec<PastMemoryWire> {
    memory
        .iter()
        .filter_map(|record| {
            let delivered = record.recipients.iter().any(|recipient| recipient == step);
            let left_out = record
                .left_out_for
                .iter()
                .any(|recipient| recipient == step);
            if !delivered && !left_out {
                return None;
            }
            Some(PastMemoryWire {
                reference: record.reference.clone(),
                hash: record.hash.clone(),
                bytes: record.bytes,
                address: record.address.clone()?,
                project: record.project.clone(),
                from: record.from.clone(),
                // Uszkodzony przyszły rekord z UUID na obu listach nie może twierdzić, że
                // dostarczona notatka była wyłącznie pominięciem; dostarczenie wygrywa.
                left_out: !delivered && left_out,
            })
        })
        .collect()
}

/// Krok w `run.json`. Nazwy pól są tymi, które pisze `commands::run::StepEntry`.
#[derive(Debug, Deserialize)]
struct StepDescription {
    #[serde(default)]
    id: String,
    /// Klucz węzła: klucz kafelka z pliku, a dla dalszych rund pętli z sufiksem `#N`.
    #[serde(default)]
    node_key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    status: String,
    /// Addytywny fakt Z-33. Brak zachowuje znaczenie każdego pliku sprzed tego pola.
    #[serde(default)]
    not_run_because: Option<String>,
    /// Werdykt rundy jest osobnym faktem od stanu potrzebnego planiście. Napis, nie enum:
    /// przyszła wartość z drutu nie może unieważnić całej historii (niezmiennik 5).
    #[serde(default)]
    round_outcome: Option<String>,
    #[serde(default)]
    executed: Option<bool>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    error: Option<String>,
    /// Czyjego wyniku ten krok nie miał, choć pojechał dalej. Addytywny fakt Z-39; klucza nie ma
    /// w żadnym `run.json` zapisanym wcześniej, a `default` znaczy tam dokładnie to samo, co
    /// znaczył brak: ten krok dostał wszystko, po co przyszedł (niezmiennik 5 na granicy pliku).
    #[serde(default)]
    ran_without: Vec<String>,
    #[serde(default)]
    cost_usd: Option<f64>,
    /// Nowy słownik Z-48. Na dysku ma `snake_case`; aliasy przyjmują te same fakty z kopii,
    /// która przeszła przez drut JSON zamiast bezpośrednio przez pisarza `run.json`.
    #[serde(default, alias = "uncachedInput")]
    uncached_input: Option<u64>,
    #[serde(default, alias = "cacheRead")]
    cache_read: Option<u64>,
    #[serde(default, alias = "vendorTurns")]
    vendor_turns: Option<u32>,
    /// Stare klucze zostają wyłącznie czytnikiem zgodności. Dla Codeksa `input_tokens`
    /// zawierał cache, a `turns` znaczył turę Loadouta, więc oba wymagają mapowania zamiast
    /// prostego przepisania (2026-09, Z-48; niezmiennik 4).
    #[serde(default, alias = "inputTokens")]
    input_tokens: Option<u64>,
    #[serde(default, alias = "cachedTokens")]
    cached_tokens: Option<u64>,
    #[serde(default)]
    turns: Option<u32>,
    /// Migawka agenta, z której bierzemy JEDNO pole: czym ten krok był prowadzony.
    ///
    /// `None` dla kroków bez agenta (kafelek kontrolny, „sprawdź", „uruchom i zostaw") i dla
    /// plików sprzed wprowadzenia migawki. Oba znaczą „czytaj jak dotąd", czyli Claude'em.
    #[serde(default)]
    effective: Option<EffectiveAgent>,
    /// Co aplikacja agenta wczytała z folderu tego kroku. Klucza nie ma w żadnym `run.json`
    /// zapisanym przed 2026-09 (Z-16), więc `default` jest tu jedyną drogą do tego, żeby stare
    /// biegi dalej dawały się otworzyć (niezmiennik 5 na granicy pliku).
    #[serde(default)]
    loaded_by_the_app: Option<LoadedByTheAppWire>,
}

/// To samo zdanie, które [`crate::engine::line::done_line`] wkłada do widocznego wiersza.
fn past_steps(
    project: &Path,
    dir: &Path,
    described: Option<&Description>,
    instruction_package: Option<&crate::inherit::instructions::InstructionSnapshot>,
) -> Vec<PastStepWire> {
    match described {
        Some(file) => file
            .steps
            .iter()
            .map(|step| PastStepWire {
                id: step.id.clone(),
                /* Rundy pętli mają wspólny kafelek i różne klucze węzła (`build#2`), więc sufiks
                 * zdejmuje ta sama warstwa, która go nadała. */
                tile: crate::commands::run::tile_key_of(&step.node_key).to_owned(),
                name: step.name.clone(),
                agent: step.agent.clone(),
                state: history_state(step),
                executed: step.executed,
                summary: step.summary.clone().unwrap_or_default(),
                error: step.error.clone().unwrap_or_default(),
                ran_without: step.ran_without.clone(),
                cost_usd: step.cost_usd,
                context_per_turn: recorded_context_per_turn(step),
                memory: memory_for_step(&file.memory, &step.id),
                project_instructions: instruction_package
                    .filter(|package| {
                        step.executed == Some(true)
                            && package.for_step(crate::commands::run::tile_key_of(&step.node_key))
                    })
                    .map_or_else(
                        Vec::new,
                        crate::inherit::instructions::InstructionSnapshot::sources,
                    ),
                what_loadout_did_not_give: what_loadout_did_not_give(
                    step.loaded_by_the_app.as_ref(),
                ),
                loaded_by_the_app: step.loaded_by_the_app.clone(),
                lines: recorded_lines(
                    dir,
                    &step.id,
                    &step.name,
                    step.effective
                        .as_ref()
                        .map_or("", |one| one.runs_with.as_str()),
                    Some((project, &file.id, &step.node_key)),
                ),
            })
            .collect(),
        None => Vec::new(),
    }
}

fn recorded_context_per_turn(step: &StepDescription) -> Option<String> {
    let effective_vendor = step
        .effective
        .as_ref()
        .map(|one| one.runs_with.as_str())
        .filter(|one| !one.trim().is_empty());
    let mut vendor = effective_vendor
        .unwrap_or(step.agent.as_str())
        .trim()
        .to_ascii_lowercase();
    // Pliki starsze od migawki `effective` uruchamiały wyłącznie Claude'a; UUID agenta
    // nie może zostać błędnie uznany za nowego vendora (2026-09, niezmiennik 4).
    if effective_vendor.is_none() && !matches!(vendor.as_str(), "codex" | "claude" | "claude-code")
    {
        "claude-code".clone_into(&mut vendor);
    }
    let cache_read = step.cache_read.or(step.cached_tokens).unwrap_or_default();
    let uncached_input = step.uncached_input.unwrap_or_else(|| {
        let old = step.input_tokens.unwrap_or_default();
        if vendor == "codex" {
            old.saturating_sub(cache_read)
        } else {
            old
        }
    });
    let turns = step.vendor_turns.or_else(|| {
        matches!(vendor.as_str(), "claude" | "claude-code" | "claudecode")
            .then_some(step.turns)
            .flatten()
    })?;
    context_per_turn(uncached_input, cache_read, turns)
}

/// Pluginy, które zakłada krokowi SAM BIEG — po nazwie, którą przypina ich manifest.
///
/// Czytane ze stałych warstwy, która te nazwy nadaje (`inherit::rewrite::pin_the_name`), a nie
/// przepisane: kopia rozjechałaby się przy pierwszej zmianie nazwy i krok zacząłby liczyć
/// własność biegu jako cudzą (niezmiennik 13).
const OUR_PLUGINS: [&str; 2] = [rewrite::LIBRARY_PLUGIN, rewrite::INHERITED_PLUGIN];

/// Klucz, którym aplikacja agenta nazywa katalog pamięci przekierowany przez bieg.
///
/// Zmierzone 2026-08-23 w `system/init` każdego kroku: `memory_paths.auto` wskazywał katalog
/// dzielony z sesjami człowieka, dopóki nie zaczął go podstawiać `StepDocument`
/// (`engine::drivers::claude`). Każdy inny klucz tej mapy jest pamięcią, której Loadout temu
/// krokowi nie dał.
const AUTO_MEMORY: &str = "auto";

/// Zdanie o tym, co ten krok wczytał z folderu POZA tym, co włożył mu bieg — albo `None`.
///
/// # Po co ono jest (2026-09, Z-47)
///
/// Rekord Z-16 mówi, co aplikacja agenta ogłosiła o sobie; nikt nie porównywał tego z tym, co
/// Loadout WŁOŻYŁ, a to jest całe pytanie, dla którego tamten rekord powstał. `ARCHITECTURE` §4:
/// na 2.1.251 plik instrukcji gospodarza docierał do kroku mimo `--setting-sources ""`, na
/// 2.1.260 już nie, i vendor odwrócił to bez linijki w zmianach. Następny taki obrót zobaczy
/// wyłącznie ktoś, kto otworzy panel i porówna listy ręcznie, pozycja po pozycji.
///
/// # Liczone przy odczycie, bez ani jednego nowego klucza w `run.json`
///
/// Rzeczy biegu są rozpoznawalne po nazwach, którymi bieg sam je przypiął: pluginy z
/// [`OUR_PLUGINS`], umiejętności z nich (wracają jako `<plugin>:<nazwa>` [S1 §2]) i przekierowany
/// katalog pamięci pod kluczem [`AUTO_MEMORY`]. Zdanie powstaje więc przy każdym otwarciu biegu,
/// także tego zapisanego przed tą zmianą (niezmiennik 4).
///
/// # Trzy listy z sześciu, i to jest wybór
///
/// `slash_commands`, `mcp_servers` i `agents` nie mają po naszej stronie nazwy, po której dałoby
/// się poznać, że przyszły od nas — Loadout nie wkłada krokowi ani jednej z tych trzech rzeczy,
/// więc każda pozycja byłaby „spoza biegu" i zdanie mówiłoby to samo o każdym kroku, który w ogóle
/// się przedstawił. Zostaje to, co bieg naprawdę wkłada i co naprawdę da się porównać.
fn what_loadout_did_not_give(loaded: Option<&LoadedByTheAppWire>) -> Option<String> {
    let loaded = loaded?;
    let skills = loaded
        .skills
        .iter()
        .filter(|name| !from_our_plugin(name))
        .count();
    let plugins = loaded
        .plugins
        .iter()
        .filter(|name| !OUR_PLUGINS.contains(&name.as_str()))
        .count();
    let memory = loaded
        .memory_paths
        .iter()
        .filter(|key| key.as_str() != AUTO_MEMORY)
        .count();

    // Kolejność jest kolejnością zdania i nie jest przypadkowa: umiejętności są tym, czego z
    // cudzego folderu przychodzi najwięcej, a katalog pamięci tym, o czym najtrudniej się
    // dowiedzieć skądkolwiek indziej.
    let parts: Vec<String> = [
        a_few(skills, "1 skill", "skills"),
        a_few(plugins, "a plugin", "plugins"),
        a_few(memory, "a memory folder", "memory folders"),
    ]
    .into_iter()
    .flatten()
    .collect();

    let what = match parts.as_slice() {
        // Krok bez rekordu, rekord z pustymi listami i rekord z samą własnością biegu są dla
        // patrzącego jednym: z tego folderu nie przyszło nic, czego Loadout nie dał. Zdanie
        // „przeczytał 0 rzeczy" byłoby wierszem bez treści przy niemal każdym kroku
        // (niezmienniki 16 i 17).
        [] => return None,
        [alone] => alone.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };

    let folder = loaded.folder.as_str();
    // Folder bez nazwy zdarza się tylko przy uszkodzonym rekordzie. Zdanie zostaje prawdziwe bez
    // niego; „from " zakończone niczym wyglądałoby jak napis, który się nie dorysował.
    Some(if folder.is_empty() {
        format!("This step also read {what} that Loadout did not give it")
    } else {
        format!("This step also read {what} from {folder} that Loadout did not give it")
    })
}

/// Czy ta umiejętność przyjechała z pluginu, który założył bieg.
///
/// Po przedrostku `<plugin>:`, bo tak wymienia je `system/init` [S1 §2] — samo `starts_with`
/// nazwą pluginu łapałoby też `loadout-skills-of-somebody-else`, czyli cudzą rzecz uznaną
/// za naszą.
fn from_our_plugin(skill: &str) -> bool {
    OUR_PLUGINS.iter().any(|plugin| {
        skill
            .strip_prefix(plugin)
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

/// Człon zdania o jednej liście: jedna sztuka nazwana słowem, więcej — liczbą.
///
/// `None` przy zerze, żeby wołający nie musiał pytać dwa razy o to samo.
fn a_few(count: usize, one: &str, many: &str) -> Option<String> {
    match count {
        0 => None,
        1 => Some(one.to_owned()),
        _ => Some(format!("{count} {many}")),
    }
}

/// Stan jednego fizycznego kroku, który ma przeczytać człowiek.
///
/// 2026-09 (Z-33) — `Succeeded` nieostatniej rundy jest wyłącznie sygnałem dla planisty, żeby
/// odblokował powrót pętli. Historia najpierw pyta o trwałe fakty rundy; bez nich starszy plik
/// zachowuje dokładnie dotychczasowe znaczenie.
fn history_state(step: &StepDescription) -> String {
    if step.not_run_because.is_some() {
        return "not_run".to_owned();
    }
    if step.round_outcome.as_deref() == Some("fail") {
        return "failed".to_owned();
    }
    step.status.clone()
}

/// To jedno pole migawki agenta, którego potrzebuje odczyt transkryptu.
///
/// Osobna, minimalna struktura zamiast `library::agents::Agent`: tamten typ ma kilkanaście pól
/// i własną ewolucję, a tutaj interesuje nas wyłącznie, którym dekoderem czytać plik. Czytanie
/// całego agenta wiązałoby historię ze zmianami w bibliotece, które jej nie dotyczą.
#[derive(Debug, Deserialize)]
struct EffectiveAgent {
    /// `claude-code` albo `codex` — nazwa z `library::agents::Vendor`, w camelCase jak reszta
    /// migawki.
    #[serde(default, rename = "runsWith")]
    runs_with: String,
}

/// Wiersz listy dla jednego katalogu biegu.
fn summary(dir: &Path, workflows: &HashMap<String, String>) -> RunWire {
    let folder = file_name(dir);
    let when = when_of(&folder);

    let Some(file) = read_description(dir) else {
        return RunWire {
            folder,
            when,
            title: String::new(),
            workflow_file: String::new(),
            state: String::new(),
            steps: 0,
            cost_usd: None,
            said: Some(
                if dir.join(RUN_FILE).exists() {
                    RECORD_UNREADABLE
                } else {
                    NOTHING_KEPT
                }
                .to_owned(),
            ),
            // Biegu, którego opisu nie dało się przeczytać, nie pytamy o prywatną turę: to nie
            // jest „tury nie było", tylko „nie wiemy" (niezmiennik 17).
            reflection: None,
        };
    };

    // Suma po krokach, które koszt PODAŁY. `None` przy wszystkich `None` jest inną odpowiedzią
    // niż `0.0`: „nikt nie zmierzył" i „nie kosztowało nic" to dwa różne zdania na ekranie.
    let costs: Vec<f64> = file.steps.iter().filter_map(|step| step.cost_usd).collect();
    let cost_usd = if costs.is_empty() {
        None
    } else {
        Some(costs.iter().sum())
    };
    let workflow_file = workflows
        .get(&file.workflow_id)
        .cloned()
        .unwrap_or_default();

    RunWire {
        folder,
        when,
        title: file.title,
        workflow_file,
        state: file.status,
        steps: file.steps.len(),
        cost_usd,
        said: None,
        reflection: file.reflection,
    }
}

/// Opis biegu, albo `None` — kiedy pliku nie ma, nie da się go otworzyć, albo nie jest JSON-em.
///
/// Trzy powody i jedna odpowiedź, bo wołający robi z nimi to samo: stawia wiersz z uczciwym
/// zdaniem. Rozróżnienie „nie ma" od „nie da się przeczytać" wraca w [`summary`], z pliku.
fn read_description(dir: &Path) -> Option<Description> {
    let text = std::fs::read_to_string(dir.join(RUN_FILE)).ok()?;
    match serde_json::from_str(&text) {
        Ok(file) => Some(file),
        Err(error) => {
            tracing::warn!(
                run = %dir.display(),
                %error,
                "this run's description could not be read, so it stands on the list with a sentence instead"
            );
            None
        }
    }
}

/// Ostatni człon ścieżki jako napis. Pusty tylko dla ścieżki, która go nie ma.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Czy to jest nazwa JEDNEGO katalogu — bez ukośników, bez `..`, niepusta.
fn is_one_name(asked: &str) -> bool {
    !asked.is_empty()
        && asked != "."
        && asked != ".."
        && !asked.contains('/')
        && !asked.contains('\\')
        && !asked.contains('\0')
}

/// `20260816-194804__<uuid>` → `2026-08-16 19:48`.
///
/// Z nazwy katalogu, bo ona stoi zawsze — także po biegu, którego opisu nie da się przeczytać.
/// Nazwę składa `commands::run::stamp` i to jest kontrakt między tamtą funkcją a tą; nazwa,
/// która do niego nie pasuje (katalog przeniesiony ręcznie, cudzy), wraca **sobą samą**:
/// napis, którego nie umiemy przeczytać jako daty, dalej nazywa ten jeden katalog, a data
/// zgadnięta byłaby zmyśleniem (niezmiennik 17).
fn when_of(folder: &str) -> String {
    let stamp = folder.split("__").next().unwrap_or(folder);
    let Some((day, time)) = stamp.split_once('-') else {
        return folder.to_owned();
    };
    if day.len() != 8
        || time.len() != 6
        || !day.bytes().chain(time.bytes()).all(|b| b.is_ascii_digit())
    {
        return folder.to_owned();
    }
    format!(
        "{}-{}-{} {}:{}",
        &day[0..4],
        &day[4..6],
        &day[6..8],
        &time[0..2],
        &time[2..4]
    )
}

/// Zapisany strumień kroku → wiersze, TĄ SAMĄ kuracją, którą widzi żywy bieg.
///
/// # Dlaczego wszystkie zdarzenia dostają `at_ms: 0`
///
/// Bo w pliku nie ma czasu i nie ma skąd go wziąć. `logs/agent-<krok>.jsonl` to linie wprost
/// od vendora, a te znaczników czasu nie niosą — mówi to wprost `store::rebuild`, akapit
/// o `events.ts`, i z tego samego powodu odmawia tam dosypywania numeru linii: „wyglądałoby
/// dokładniej i byłoby zmyśleniem". Skutek jest widoczny i zapisany: reguła 4 skleja sąsiednie
/// czynności tego samego rodzaju w oknie 2 s, więc przy jednym znaczniku dla całego pliku
/// odczyty sąsiadujące ze sobą czytają się jako JEDEN wiersz z licznikiem („Read 12 files").
/// Licznik jest prawdziwy, a podział na grupy jest zgrubny — i to jest uczciwa cena za brak
/// zegara. Wersja z zegarem wymyślonym tutaj wyglądałaby dokładniej i mówiłaby nieprawdę.
///
/// Pliku, którego nie ma, nie ma i tyle: krok anulowany albo pominięty nie zdążył nic nadać.
///
/// Dekoder dobrany do vendora — bo strumienie są dwa i nie mają wspólnego kształtu.
///
/// Claude nadaje linie `system` / `assistant` / `result`, Codex `thread.started` /
/// `item.completed`. Żaden z tych zbiorów nie zawiera drugiego, więc dekoder użyty do cudzego
/// pliku nie myli się po trochu — oddaje ZERO wierszy, czyli ekran, który wygląda jak brak
/// danych.
///
/// Enum, a nie obiekt traitu: warianty są dwa i zamknięte, a `stream::decode` przyjmuje
/// `&mut ClaudeDecoder` konkretnie, więc nie ma czego opakować.
enum Transcript {
    Claude(ClaudeDecoder),
    Codex(CodexDecoder),
}

impl Transcript {
    /// Nazwy są tymi z `library::agents::Vendor`, jak w migawce kroku.
    ///
    /// Cokolwiek innego — pusty napis, krok bez agenta, plik sprzed migawki, vendor dołożony
    /// kiedyś w przyszłości — czyta się Claude'em, czyli dokładnie tak, jak czytało się do
    /// 2026-08-23. Nowy vendor bez wpisu tutaj jest więc regresją WIDOCZNĄ (pusty transkrypt),
    /// a nie cichą zmianą treści.
    fn for_vendor(vendor: &str) -> Self {
        if vendor == "codex" {
            return Self::Codex(CodexDecoder::new());
        }
        Self::Claude(ClaudeDecoder::new())
    }

    /// Zdarzenia z jednej linii. Pusty wektor jest normalną odpowiedzią, nie awarią.
    ///
    /// Linia nieczytelna znika po cichu i to jest ta sama umowa, co przy `Decoded::Unrecognised`
    /// (niezmiennik 5): vendorzy dokładają typy zdarzeń co tydzień, a jedna nieznana linia ma
    /// kosztować jeden wiersz, nie cały transkrypt.
    fn read(&mut self, line: &str) -> Vec<DecodedEvent> {
        match self {
            Self::Claude(one) => match decode(one, line) {
                Decoded::Events(events) => events,
                Decoded::Unrecognised => Vec::new(),
            },
            // `From<AgentEvent>` dokłada `tool: None` — Codex niesie narzędzie w samym
            // zdarzeniu, więc kuracja nie potrzebuje tu nic ponad nie.
            Self::Codex(one) => one.push(line).into_iter().map(DecodedEvent::from).collect(),
        }
    }
}

/// 2026-08-23 — DEKODER DOBIERANY DO VENDORA. Do dziś stało tu na sztywno `ClaudeDecoder`
/// i było to opisane jako „zapisane ograniczenie": transkrypt kroku prowadzonego Codexem
/// przechodził tędy jako **zero wierszy**.
///
/// Ograniczenie było prawdziwe i przestało być potrzebne: `CodexDecoder` istnieje od T-10
/// i ma dokładnie ten szew — `push(&str) -> Vec<AgentEvent>`. Nikt go tu po prostu nie
/// podłączył.
///
/// ZMIERZONE NA BIEGU WŁAŚCICIELA `20260823-011240`: siedem kroków codeksa pokazywało w historii
/// „Nothing of what this step said was kept on disk", podczas gdy ich transkrypty leżały na
/// dysku i ważyły od 17 do 61 kB. Dziewięć kroków Claude'a z tego samego biegu wyświetlało się
/// w całości. Wzór był bez jednego wyjątku, więc połowa historii tego biegu była niewidoczna.
///
/// Nieznany albo pusty vendor czyta się Claude'em — dokładnie tak, jak czytał się do dziś.
fn recorded_lines(
    run_dir: &Path,
    step: &str,
    agent: &str,
    vendor: &str,
    messages: Option<(&Path, &str, &str)>,
) -> Vec<Line> {
    let path = run_dir.join(LOGS_DIR).join(format!("agent-{step}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap_or_default();

    let mut decoder = Transcript::for_vendor(vendor);
    let mut curator = Curator::new();
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Linia, której nie da się przeczytać, jest jedną linią mniej — nigdy końcem odczytu
        // (niezmiennik 5). Vendorzy dokładają typy zdarzeń co tydzień, po cichu.
        for one in decoder.read(line) {
            out.extend(curator.observe(Seen {
                agent,
                at_ms: 0,
                event: &one.event,
                tool: one.tool.as_ref(),
            }));
        }
    }
    // Ostatnia grupa sklejania nie wyszłaby bez tego nigdy — czyli człowiek zobaczyłby o wiersz
    // mniej, niż się wydarzyło. Najgorszy rodzaj zgubienia, bo cichy.
    out.extend(curator.flush());
    if let Some((project, run_id, node_key)) = messages {
        match crate::bridge::messages::history(project, run_dir, run_id, node_key) {
            Ok(lines) => out.extend(lines),
            Err(error) => out.push(Line::Problem {
                agent: agent.to_owned(),
                text: format!("The saved messages could not be read safely: {error}"),
                resets_at: None,
            }),
        }
    }
    out
}

/// Indeks identyfikator → nazwa pliku dla dzisiejszej biblioteki workflow.
///
/// Po identyfikatorze, nie po nazwie: nazwa pliku jest sluggiem tytułu i zmienia się razem z nim,
/// a identyfikator jest tym, czym bieg zapamiętał, skąd przyszedł. Porządek jest ustalony, żeby
/// dwa pliki o jednym identyfikatorze dawały za każdym razem ten sam wynik — `read_dir` nie
/// obiecuje kolejności.
fn workflow_files() -> HashMap<String, String> {
    let Ok(entries) = fs::read_dir(crate::loadout_dir().join("workflows")) else {
        return HashMap::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|one| one == "json"))
        .collect();
    paths.sort();
    let mut files = HashMap::new();
    for path in paths {
        let Some(named) = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            continue;
        };
        let Some(id) = named.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        /* Posortowana pierwsza nazwa wygrywa. `or_insert` zachowuje tę decyzję także wtedy,
         * gdy dwa ręcznie edytowane pliki niosą ten sam identyfikator. */
        files
            .entry(id.to_owned())
            .or_insert_with(|| file.to_owned());
    }
    files
}

#[cfg(test)]
mod tests {
    //! Zdejmowanie gałęzi biegu — jedyna droga w tym module, która KASUJE.
    //!
    //! # Dlaczego kryterium stoi TUTAJ, a nie w `tests/it/`
    //!
    //! Bo kryterium tego zadania sądzi ekran (`branches-can-be-dropped.test.tsx`), a granica
    //! jest po tamtej stronie atrapą: ani jedna asercja tam nie dotyka prawdziwego
    //! repozytorium. Reguła „tylko gałęzie TEGO biegu" i całościowa odmowa są natomiast
    //! jedynymi rzeczami, które ta droga potrafi zepsuć nieodwracalnie. Wzorzec „kryterium przy
    //! regule" jest w repo (`workflow::check`, `commands::run`, `memory::handoff`).
    //!
    //! # Słaba wersja
    //!
    //! `assert_eq!(gone.len(), 2)`. Przechodzi ją implementacja zdejmująca każdą gałąź
    //! `loadout/*`, czyli kasująca pracę cudzych biegów jednym naciśnięciem. Rozstrzyga to, że
    //! fikstura ma gałąź drugiego biegu i gałąź spoza Loadouta, a obie mają przeżyć.

    use std::error::Error;
    use std::path::Path;
    use std::process::Command;

    use serde_json::json;
    use tempfile::TempDir;

    use super::{HistoryError, RUN_FILE, forget_run_branches_inner, fs};

    const RUN: &str = "0198a1f2-3b4c-7d5e-8f60-000000000004";
    const OTHER_RUN: &str = "0198a1f2-3b4c-7d5e-8f60-000000000009";
    const MINE: &str = "a-branch-of-my-own";

    fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(at)
            .args(["-c", "user.name=Loadout Test"])
            .args(["-c", "user.email=test@loadout.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()?;
        if !out.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )
            .into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Nazwa katalogu tego biegu — ta sama, którą składa `commands::run::stamp`.
    fn folder_of(run: &str) -> String {
        format!("20260823-011240__{run}")
    }

    /// Repozytorium z czterema gałęziami: dwie tego biegu, jedna cudzego, jedna człowieka.
    fn a_project_with_branches() -> Result<TempDir, Box<dyn Error>> {
        let project = TempDir::new()?;
        let at = project.path();
        git(at, &["init", "--quiet"])?;
        fs::write(at.join("README.md"), "one\n")?;
        git(at, &["add", "-A"])?;
        git(at, &["commit", "--quiet", "-m", "the first commit"])?;
        for branch in [
            format!("loadout/{RUN}/s_build"),
            format!("loadout/{RUN}/s_docs"),
            format!("loadout/{OTHER_RUN}/s_build"),
            MINE.to_owned(),
        ] {
            git(at, &["branch", &branch, "HEAD"])?;
        }
        let dir = at.join(".loadout").join("runs").join(folder_of(RUN));
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join(RUN_FILE),
            serde_json::to_string(&json!({ "id": RUN, "steps": [] }))?,
        )?;
        Ok(project)
    }

    fn branches(at: &Path) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(git(at, &["branch", "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    }

    #[test]
    fn it_takes_away_only_the_branches_of_that_run() -> Result<(), Box<dyn Error>> {
        let project = a_project_with_branches()?;
        let at = project.path();

        let gone = forget_run_branches_inner(at, &folder_of(RUN))?;
        assert_eq!(
            gone,
            vec![
                format!("loadout/{RUN}/s_build"),
                format!("loadout/{RUN}/s_docs")
            ],
            "the answer has to name what is no longer there, because that is the only way the \
             screen can say what happened without asking again"
        );

        let left = branches(at)?;
        assert!(
            !left
                .iter()
                .any(|name| name.starts_with(&format!("loadout/{RUN}/"))),
            "the branches of this run are still there, so pressing the control did nothing at \
             all. Left: {left:?}"
        );
        assert!(
            left.contains(&format!("loadout/{OTHER_RUN}/s_build")),
            "another run's branch went away with this one. One press would then take the work \
             of every run in this folder. Left: {left:?}"
        );
        assert!(
            left.contains(&MINE.to_owned()),
            "a branch nobody made with Loadout went away. Left: {left:?}"
        );
        Ok(())
    }

    #[test]
    fn one_branch_open_somewhere_leaves_every_branch_alone() -> Result<(), Box<dyn Error>> {
        let project = a_project_with_branches()?;
        let at = project.path();
        let busy = format!("loadout/{RUN}/s_docs");
        let side = at.join("side");
        git(
            at,
            &[
                "worktree",
                "add",
                "--quiet",
                &side.display().to_string(),
                &busy,
            ],
        )?;

        let refused = forget_run_branches_inner(at, &folder_of(RUN));
        let Err(HistoryError::BranchIsOpen { branch }) = refused else {
            return Err(format!(
                "taking away a branch somebody is working on is the one move here that cannot be \
                 undone, and it was not turned down: {refused:?}"
            )
            .into());
        };
        assert_eq!(
            branch, busy,
            "the refusal has to name the branch that is open"
        );

        let left = branches(at)?;
        assert!(
            left.contains(&format!("loadout/{RUN}/s_build")),
            "the refusal was not whole: one branch of this run went away before the open one \
             stopped it. Half gone and half not is a state a person only finds out about from \
             `git branch`. Left: {left:?}"
        );
        Ok(())
    }

    #[test]
    fn a_run_with_no_record_takes_nothing_away() -> Result<(), Box<dyn Error>> {
        let project = a_project_with_branches()?;
        let at = project.path();
        fs::remove_file(
            at.join(".loadout")
                .join("runs")
                .join(folder_of(RUN))
                .join(RUN_FILE),
        )?;

        let gone = forget_run_branches_inner(at, &folder_of(RUN))?;
        assert!(
            gone.is_empty(),
            "without the run's record there is no run identifier, and without it there is no way \
             to tell this run's branches from anybody else's. Guessing from the folder name \
             would be a second answer to how a branch of this run is named. Took: {gone:?}"
        );
        assert_eq!(
            branches(at)?.len(),
            5,
            "nothing may go away over a run whose record could not be read"
        );
        Ok(())
    }
}
