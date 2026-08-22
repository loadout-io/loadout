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

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::handoffs::{HandoffWire, handoffs_of_run, run_dirs};
use crate::engine::drivers::claude::ClaudeDecoder;
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
    /// Kroki w kolejności z `run.json`, czyli w kolejności z grafu.
    pub steps: Vec<PastStepWire>,
    /// Co kroki oddały sobie nawzajem — te same pliki, które pokazuje sekcja przekazań.
    pub handoffs: Vec<HandoffWire>,
    /// Uczciwe zdanie, kiedy opisu nie dało się przeczytać.
    pub said: Option<String>,
}

/// Krok otwartego biegu.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastStepWire {
    /// Identyfikator kroku z `run.json` — po nim nazywa się plik jego strumienia.
    pub id: String,
    /// Nazwa kafelka. Ta sama, którą człowiek widzi na płótnie i w podpisie każdej linii.
    pub name: String,
    /// Nazwa agenta, który go wykonał.
    pub agent: String,
    /// Słowo z drutu; tłumaczy je okno.
    pub state: String,
    /// Jedno zdanie, które ten krok po sobie zostawił. Puste, kiedy nie zostawił żadnego.
    pub summary: String,
    /// Powód, jeśli coś poszło nie tak. Pusty, kiedy poszło dobrze.
    pub error: String,
    /// Ile kosztował ten krok. `None` znaczy „nie podał", nie zero.
    pub cost_usd: Option<f64>,
    /// Zapisany strumień tego kroku, przepuszczony przez TĘ SAMĄ kurację, co żywy bieg.
    ///
    /// Pusty jest dziś stanem normalnym i **zapisanym długiem**, nie usterką tego pliku:
    /// `commands::run` nie woła `ClaudeDriver::with_transcript`, więc `logs/agent-<krok>.jsonl`
    /// nie powstaje po żadnym prawdziwym biegu (zapisane w nagłówku `commands/run.rs`, akapit
    /// „Czego ta warstwa świadomie NIE robi"). Odczyt jest napisany i sprawdzony, żeby w dniu,
    /// w którym ktoś ten szew zepnie, historia zaczęła nieść transkrypt bez ani jednej zmiany
    /// tutaj — a do tego dnia widok mówi wprost, że zapisu nie ma.
    pub lines: Vec<Line>,
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
    let asked = run.trim();
    if !is_one_name(asked) {
        return Err(HistoryError::NotOneName {
            asked: asked.to_owned(),
        });
    }

    // Katalog bierzemy z LISTY, nie ze sklejenia ścieżki: lista jest tym samym zbiorem, który
    // widzi człowiek, więc nie da się poprosić o katalog, którego nie było na ekranie. Sklejenie
    // przechodziłoby także dla katalogu, który nie jest biegiem.
    let dir = run_dirs(project)
        .into_iter()
        .find(|path| file_name(path) == asked)
        .ok_or_else(|| HistoryError::NoSuchRun {
            asked: asked.to_owned(),
        })?;

    let head = summary(&dir);
    let steps = match read_description(&dir) {
        Some(file) => file
            .steps
            .iter()
            .map(|step| PastStepWire {
                id: step.id.clone(),
                name: step.name.clone(),
                agent: step.agent.clone(),
                state: step.status.clone(),
                summary: step.summary.clone().unwrap_or_default(),
                error: step.error.clone().unwrap_or_default(),
                cost_usd: step.cost_usd,
                lines: recorded_lines(&dir, &step.id, &step.name),
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(PastRunWire {
        folder: head.folder,
        when: head.when,
        title: head.title,
        state: head.state,
        steps,
        // Przekazania są prawdziwe niezależnie od `run.json`: to osobne pliki z własnym
        // front-matterem, więc bieg z zepsutym opisem nadal pokazuje, co jego kroki oddały.
        handoffs: handoffs_of_run(project, &dir),
        said: head.said,
    })
}

/// Opis biegu z `run.json` — dokładnie te pola, które ktoś czyta.
///
/// Nieznanych kluczy **nie odrzucamy** (niezmiennik 5): plik zapisany przez nowszego Loadouta
/// ma się dać przeczytać, a nie wywrócić historię. Każde pole poza `steps` jest opcjonalne
/// z tego samego powodu — plik po ręcznej edycji zostaje wierszem, a nie znika.
#[derive(Debug, Deserialize)]
struct Description {
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    steps: Vec<StepDescription>,
}

/// Krok w `run.json`. Nazwy pól są tymi, które pisze `commands::run::StepEntry`.
#[derive(Debug, Deserialize)]
struct StepDescription {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    cost_usd: Option<f64>,
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
/// Dekoder jest dziś jeden, Claude'owy, i to jest **zapisane ograniczenie**: `stream::decode`
/// zna kształt strumienia Claude'a, więc transkrypt kroku prowadzonego Codexem przejdzie tędy
/// jako zero wierszy. To ta sama granica, którą ma dziś żywa pompa (`engine::stream::pump`) —
/// nie druga, gorsza.
fn recorded_lines(run_dir: &Path, step: &str, agent: &str) -> Vec<Line> {
    let path = run_dir.join(LOGS_DIR).join(format!("agent-{step}.jsonl"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    let mut decoder = ClaudeDecoder::new();
    let mut curator = Curator::new();
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Linia, której nie da się przeczytać, jest jedną linią mniej — nigdy końcem odczytu
        // (niezmiennik 5). Vendorzy dokładają typy zdarzeń co tydzień, po cichu.
        if let Decoded::Events(events) = decode(&mut decoder, line) {
            for one in events {
                out.extend(curator.observe(Seen {
                    agent,
                    at_ms: 0,
                    event: &one.event,
                    tool: one.tool.as_ref(),
                }));
            }
        }
    }
    // Ostatnia grupa sklejania nie wyszłaby bez tego nigdy — czyli człowiek zobaczyłby o wiersz
    // mniej, niż się wydarzyło. Najgorszy rodzaj zgubienia, bo cichy.
    out.extend(curator.flush());
    out
}
