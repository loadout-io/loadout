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

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::handoffs::{HandoffWire, handoffs_of_run, run_dirs};
use super::isolate;
use crate::engine::drivers::DecodedEvent;
use crate::engine::drivers::claude::ClaudeDecoder;
use crate::engine::drivers::codex::CodexDecoder;
use crate::engine::line::{Curator, Line, Seen};
use crate::engine::stream::{Decoded, decode};

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
}

/// Otwarty bieg: to samo, co w wierszu listy, plus wszystko, co po nim zostało na dysku.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastRunWire {
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
    /// Gałęzie, które ten bieg zostawił w repozytorium projektu.
    ///
    /// Pusta lista dla biegu, po którym nie została ani jedna — i to jest zwykły stan: krok,
    /// który nic nie zmienił, gałęzi nie zostawia (`commands::isolate::finish`).
    pub branches: Vec<BranchWire>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Ile kosztował ten krok. `None` znaczy „nie podał", nie zero.
    pub cost_usd: Option<f64>,
    /// Zamrożony receipt wyłącznie TEGO fizycznego kroku. Pusta lista jest jawna także dla
    /// starych biegów, żeby granica TypeScript nie musiała zgadywać, czy pole zaginęło.
    pub memory: Vec<PastMemoryWire>,
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
}

/// Wszystkie biegi TEGO projektu, od najnowszego. Projekt bez `runs/` daje pustą listę.
///
/// **Nie oddaje `Result`**, i to jest decyzja, nie skrót. Jedynym powodem, dla którego ta
/// funkcja mogłaby się nie udać, jest nieczytelny pojedynczy bieg — a on ma być WIERSZEM
/// (patrz nagłówek modułu). Świeża maszyna bez ani jednego biegu jest stanem normalnym, nie
/// awarią dysku: czerwony pasek na świeżej instalacji uczy człowieka ignorować czerwone paski.
#[must_use]
pub fn list_runs_inner(project: &Path) -> Vec<RunWire> {
    run_dirs(project).iter().map(|dir| summary(dir)).collect()
}

/// Jeden bieg, otwarty do odczytu: jego kroki, ich strumienie i jego przekazania.
///
/// `run` jest nazwą katalogu z [`RunWire::folder`]. Sprawdzamy ją, zanim dotkniemy dysku
/// (patrz [`HistoryError::NotOneName`]), bo przyjeżdża z okna.
pub fn read_run_inner(project: &Path, run: &str) -> Result<PastRunWire, HistoryError> {
    let dir = one_run_dir(project, run)?;

    let head = summary(&dir);
    let described = read_description(&dir);
    let steps = match &described {
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
                state: step.status.clone(),
                executed: step.executed,
                summary: step.summary.clone().unwrap_or_default(),
                error: step.error.clone().unwrap_or_default(),
                cost_usd: step.cost_usd,
                memory: memory_for_step(&file.memory, &step.id),
                lines: recorded_lines(
                    &dir,
                    &step.id,
                    &step.name,
                    step.effective
                        .as_ref()
                        .map_or("", |one| one.runs_with.as_str()),
                ),
            })
            .collect(),
        None => Vec::new(),
    };
    // PO KROKACH, bo gałąź nazywa się kluczem kafelka, a człowiek czyta nazwy. Przed budową
    // struktury, bo `steps` idzie do niej przez przeniesienie.
    let branches = described
        .as_ref()
        .map_or_else(Vec::new, |file| branches_of_run(project, &file.id, &steps));
    // PRZED `described.map(…)` niżej, bo tamto przenosi opis. Bieg, którego opisu nie dało się
    // przeczytać, oddaje tu `None` — i to jest ta sama odpowiedź, co dla pliku bez tego klucza:
    // w obu przypadkach po prostu nie wiemy, i tak ma to zabrzmieć na ekranie.
    let reflection = described.as_ref().and_then(|file| file.reflection);

    Ok(PastRunWire {
        folder: head.folder,
        when: head.when,
        title: head.title,
        state: head.state,
        workflow_file: described
            .map(|file| file.workflow_id)
            .and_then(|id| file_named(&id))
            .unwrap_or_default(),
        steps,
        // Przekazania są prawdziwe niezależnie od `run.json`: to osobne pliki z własnym
        // front-matterem, więc bieg z zepsutym opisem nadal pokazuje, co jego kroki oddały.
        handoffs: handoffs_of_run(project, &dir),
        branches,
        reflection,
        said: head.said,
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
    let Some(prefix) = read_description(&dir).and_then(|file| run_prefix(&file.id)) else {
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
    #[serde(default)]
    executed: Option<bool>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    cost_usd: Option<f64>,
    /// Migawka agenta, z której bierzemy JEDNO pole: czym ten krok był prowadzony.
    ///
    /// `None` dla kroków bez agenta (kafelek kontrolny, „sprawdź", „uruchom i zostaw") i dla
    /// plików sprzed wprowadzenia migawki. Oba znaczą „czytaj jak dotąd", czyli Claude'em.
    #[serde(default)]
    effective: Option<EffectiveAgent>,
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
fn summary(dir: &Path) -> RunWire {
    let folder = file_name(dir);
    let when = when_of(&folder);

    let Some(file) = read_description(dir) else {
        return RunWire {
            folder,
            when,
            title: String::new(),
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

    RunWire {
        folder,
        when,
        title: file.title,
        state: file.status,
        steps: file.steps.len(),
        cost_usd,
        said: None,
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
fn recorded_lines(run_dir: &Path, step: &str, agent: &str, vendor: &str) -> Vec<Line> {
    let path = run_dir.join(LOGS_DIR).join(format!("agent-{step}.jsonl"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

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
    out
}

/// Nazwa pliku, pod którą ten workflow leży dziś w bibliotece.
///
/// Po identyfikatorze, nie po nazwie: nazwa pliku jest sluggiem tytułu i zmienia się razem z nim,
/// a identyfikator jest tym, czym bieg zapamiętał, skąd przyszedł. Porządek jest ustalony, żeby
/// dwa pliki o jednym identyfikatorze dawały za każdym razem ten sam wynik — `read_dir` nie
/// obiecuje kolejności.
fn file_named(workflow_id: &str) -> Option<String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(crate::loadout_dir().join("workflows"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|one| one == "json"))
        .collect();
    paths.sort();
    paths.into_iter().find_map(|path| {
        let named: Value = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())?;
        (named.get("id")?.as_str()? == workflow_id)
            .then(|| path.file_name()?.to_str().map(str::to_owned))
            .flatten()
    })
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
