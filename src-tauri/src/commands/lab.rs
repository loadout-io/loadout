//! Komendy sekcji Lab: zestawy, kandydatki, uruchomienie i odczyt wyników.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Ta warstwa nie zawiera ani jednej reguły o tym, czym jest zestaw: format i odmowy mieszkają
//! w `lab::file`, kształt grafu w `lab::plan`, liczenie wyniku w `lab::results`, a rozbiór
//! odpowiedzi agenta w `lab::cases`. Tu jest wyłącznie **składanie ścieżek, czytanie dysku
//! i jedna tura poza grafem** — czyli dokładnie to, czego tamte cztery moduły nie mogą robić,
//! jeśli mają dać się sprawdzić bez dysku i bez procesu.
//!
//! # Dwa wywołania agenta, dwie różne drogi i to jest treść tego pliku
//!
//! **Kandydatki** to JEDNA krótka tura poza grafem, tą samą maszynerią, którą draftuje się
//! umiejętność i porównuje kopie importu (`commands::skills::one_turn`). Jedna tura nie
//! potrzebuje planisty i nie ma czego zapisać do katalogu biegu.
//!
//! **Przebieg zestawu** to N×M kroków i idzie **przez silnik**, jako zwykły bieg. Skrót
//! „odpal sterownik wprost", którym idzie tura kandydatek, omija pulę miejsc, sufit wydatku
//! i dowód śmierci grupy — przy jednym pytaniu o projekt to jest w porządku, przy dwudziestu
//! siedmiu komórkach to jest zamrożony laptop i rachunek, którego nikt nie zamawiał.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engine::drivers::{DecodedEvent, Policy, RunSpec};
use crate::engine::supervisor::GroupProof;
use crate::lab::{
    self, Case, CaseStatus, EvalSet, Subject, Variant, cases, file, fix, plan, results, slugify,
};
use crate::library::agents::{Overrides, resolve};
use crate::memory::handoff;

use super::skills::{Ended, give_up_after, off_the_wire, one_turn, some_text, the_agent_saved_as};
use super::{Drivers, Part, RunRequest};

/// Ile zdarzeń mieści kanał jednej tury, zanim sterownik zacznie czekać.
const EVENT_QUEUE: usize = 256;

/// Ile kandydatek prosimy najwyżej za jednym razem.
///
/// Sufit, nie zamówienie — powód stoi przy `lab::cases::ask_for_cases`. Sześć, bo tyle mieści
/// się na ekranie bez przewijania, a lista, której człowiek nie przeczyta w całości, jest listą,
/// którą zaakceptuje w całości.
const AT_MOST: usize = 6;

/// Zdanie o drugim pytaniu zadanym w chwili, w której pierwsze jeszcze trwa.
const ALREADY_PROPOSING: &str =
    "An agent is already writing cases here. Wait for it, or stop it first.";

/// Zdanie o turze, która wróciła pusta.
const SAID_NOTHING: &str = "The agent finished without writing a single case.";

/// Zdanie o grupie, po której nie ma dowodu zejścia.
const MAY_STILL_BE_RUNNING: &str =
    "Loadout could not make sure that agent stopped, so it may still be running.";

/// Miejsce na JEDNO pisanie kandydatek naraz i uchwyt do tego, które trwa teraz.
///
/// **Osobne od `commands::import::Comparing` i od `commands::skills::Drafting`, mimo
/// identycznego kształtu.** Trzy różne pytania, zadawane z trzech różnych ekranów: Stop przy
/// kandydatkach w Labie nie ma prawa zatrzymać porównania kopii otwartego w Imporcie.
#[derive(Debug, Default)]
pub struct Proposing {
    /// `Some` znaczy „ktoś właśnie pisze" i niesie token **tej** tury.
    ///
    /// `std::sync::Mutex` i **nigdy trzymany przez `await`** (niezmiennik 8): każde wzięcie
    /// tego zamka mieści się w jednym wyrażeniu. Zamek trzymany przez turę zawiesiłby Stop na
    /// czas czytania przez model — czyli dokładnie wtedy, kiedy Stop jest potrzebny.
    working: Mutex<Option<CancellationToken>>,
}

impl Proposing {
    /// Miejsce, na którym nikt nie pisze.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bierze miejsce dla nowej tury — albo `None`, kiedy jest zajęte.
    fn claim(&self) -> Option<Claim<'_>> {
        let token = CancellationToken::new();
        let mut working = self.working.lock().unwrap_or_else(PoisonError::into_inner);
        if working.is_some() {
            return None;
        }
        *working = Some(token.clone());
        drop(working);
        Some(Claim {
            proposing: self,
            stop: token,
        })
    }

    /// Oddaje miejsce.
    fn release(&self) {
        let mut working = self.working.lock().unwrap_or_else(PoisonError::into_inner);
        *working = None;
    }

    /// „Stop" z okna: zatrzymuje turę, która trwa teraz. Bez tury nie robi nic.
    ///
    /// Zatruty zamek odplatamy zamiast panikować: `panic!` w agentowym runtime zabiera cały
    /// bieg (`AGENTS.md` §4), a uchwyt po panice jednej tury jest dalej poprawnym uchwytem.
    pub fn stop(&self) {
        let token = self
            .working
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(token) = token {
            token.cancel();
        }
    }
}

/// Zajęte miejsce, oddawane na **każdej** drodze wyjścia — także przy odmowie w środku.
struct Claim<'a> {
    proposing: &'a Proposing,
    /// Token TEJ tury — ten sam, który cofa [`Proposing::stop`].
    stop: CancellationToken,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.proposing.release();
    }
}

/// Otwarty zestaw razem z rewizją, na której okno go czyta.
///
/// Para, nie sam plik: zapis, który nie wie, co czytał, nie ma jak odmówić spóźnionemu
/// nadpisaniu (ten sam powód stoi przy `commands::workflows::OpenWorkflow`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSet {
    /// Zestaw, jak leży na dysku.
    pub set: EvalSet,
    /// Rewizja bajtów, które okno właśnie przeczytało.
    pub revision: String,
}

/// Co wyszło z tury pisania kandydatek — dla okna.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedWire {
    /// Zestaw po dopisaniu kandydatek, razem z nową rewizją.
    pub set: OpenSet,
    /// Ile kandydatek doszło.
    pub written: usize,
    /// Ile odpadło bez pochodzenia.
    pub without_a_reason: usize,
    /// Ile odpadło niedokończonych.
    pub unfinished: usize,
}

/// Jeden przebieg zestawu, policzony.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PastEvalWire {
    /// Nazwa katalogu biegu — adres, którym okno prosi o szczegóły w Historii.
    pub folder: String,
    /// Kiedy ruszył, do przeczytania: `2026-08-31 09:14` (UTC).
    pub when: String,
    /// Słowo z drutu o całym biegu: `succeeded`, `failed`, `cancelled`, `running`.
    pub state: String,
    /// Ile komórek przeszło.
    pub passed: usize,
    /// Ile komórek zmierzono.
    pub judged: usize,
    /// Ile ten przebieg kosztował.
    pub cost_usd: Option<f64>,
    /// Komórki tego przebiegu.
    pub cells: Vec<CellWire>,
}

/// Jedna komórka macierzy na drucie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellWire {
    /// Identyfikator przypadku (wiersz).
    pub case: String,
    /// Identyfikator wariantu (kolumna).
    pub variant: String,
    /// `passed`, `did-not-pass` albo `not-judged`. Tłumaczy to okno (niezmiennik 14).
    pub outcome: String,
    /// Dlaczego tak, a nie inaczej. Puste przy przejściu.
    pub said: String,
    /// Ile ta komórka kosztowała.
    pub cost_usd: Option<f64>,
}

/// O ile ten przebieg różni się od poprzedniego.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementWire {
    /// Ile komórek zaczęło przechodzić.
    pub gained: usize,
    /// Ile przestało.
    pub lost: usize,
}

/// Wszystko, co okno rysuje na ekranie Lab dla jednego zestawu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardWire {
    /// Zestaw i jego rewizja.
    pub set: OpenSet,
    /// Przebiegi, od najnowszego. Pusta lista znaczy „ten zestaw jeszcze nie biegł".
    pub runs: Vec<PastEvalWire>,
    /// Różnica najnowszego wobec poprzedniego. `None`, kiedy nie ma z czym porównać.
    pub movement: Option<MovementWire>,
    /// Zdanie o tym, czego brakuje do uruchomienia. `None` znaczy „można".
    pub cannot_run: Option<String>,
}

/// Czego okno nie dostało i dlaczego.
#[derive(Debug, thiserror::Error)]
pub enum LabError {
    /// Zestawu o tym adresie tu nie ma.
    #[error("There is no set called \"{0}\" in this project.")]
    NoSuchSet(String),
    /// Nie dało się go wczytać.
    #[error("{0}")]
    Unreadable(String),
    /// Nie dało się go zapisać.
    #[error("{0}")]
    Unwritable(String),
    /// Da się wczytać, ale nie da się uruchomić.
    #[error("{0}")]
    NotReady(String),
    /// Dysk odmówił.
    #[error("This could not be done: {0}.")]
    Io(#[from] std::io::Error),
}

/// Wszystkie zestawy tego projektu.
#[must_use]
pub fn list_sets_inner(project: &Path) -> Vec<EvalSet> {
    file::list(project)
}

/// Jeden zestaw razem z rewizją, na której okno ma go czytać.
pub fn read_set_inner(project: &Path, id: &str) -> Result<OpenSet, LabError> {
    let path = file::path_for(project, id);
    let bytes = fs::read(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            LabError::NoSuchSet(id.to_owned())
        } else {
            LabError::Io(error)
        }
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let set = file::load_text(&text).map_err(|error| LabError::Unreadable(error.to_string()))?;
    Ok(OpenSet {
        set,
        revision: crate::durable_file::revision_of(&bytes),
    })
}

/// Zapisuje zestaw wobec rewizji, którą okno przeczytało.
pub fn save_set_inner(
    project: &Path,
    set: &EvalSet,
    expected: Option<&str>,
) -> Result<String, LabError> {
    let path = file::path_for(project, &set.id);
    file::save(set, &path, expected).map_err(|error| LabError::Unwritable(error.to_string()))
}

/// Zakłada nowy zestaw dla agenta albo umiejętności i oddaje go otwartego.
///
/// # Dlaczego nowy zestaw ma od razu kolumny
///
/// Bo zestaw bez ani jednej kolumny nie da się uruchomić, a człowiek, który właśnie kliknął
/// „Evaluate", chce zobaczyć pytanie, a nie formularz. Dla agenta jest to jedna kolumna: on
/// sam, nietknięty. Dla umiejętności są **dwie** — bez niej i z nią — bo to jest całe pytanie,
/// które zadaje się o umiejętność, a zestaw z jedną kolumną nie umie na nie odpowiedzieć.
pub fn create_set_inner(
    project: &Path,
    name: &str,
    subject: &Subject,
    agent: &str,
) -> Result<OpenSet, LabError> {
    let taken: BTreeSet<String> = file::list(project).into_iter().map(|set| set.id).collect();
    let mut id = slugify(name);
    if taken.contains(&id) {
        // Licznik od dwójki, tak samo jak przy kandydatkach: „review-2" obok „review" czyta się
        // jak drugi zestaw, a „review-1" obok „review" jak połowa pary.
        for suffix in 2..u32::MAX {
            let candidate = format!("{id}-{suffix}");
            if !taken.contains(&candidate) {
                id = candidate;
                break;
            }
        }
    }

    let set = EvalSet {
        format: lab::CURRENT,
        id,
        name: name.trim().to_owned(),
        subject: subject.clone(),
        cases: Vec::new(),
        variants: first_columns(subject, agent),
        extra: Map::new(),
    };
    let revision = save_set_inner(project, &set, None)?;
    Ok(OpenSet { set, revision })
}

/// Kolumny, z którymi zestaw się rodzi.
fn first_columns(subject: &Subject, agent: &str) -> Vec<Variant> {
    match subject {
        Subject::Agent { id } => vec![Variant {
            id: "as-it-is".to_owned(),
            name: "As it is".to_owned(),
            agent: id.clone(),
            overrides: Map::new(),
            extra: Map::new(),
        }],
        Subject::Skill { name } => vec![
            Variant {
                id: "without".to_owned(),
                name: "Without".to_owned(),
                agent: agent.to_owned(),
                // Pusta lista umiejętności, nie brak klucza: brak klucza znaczy „dziedzicz",
                // czyli agent zachowałby swoje własne umiejętności i obie kolumny miałyby tę
                // samą odpowiedź. Kolumna „bez", która ma tę rzecz, nie mierzy niczego.
                overrides: one_override("skills", Value::Array(Vec::new())),
                extra: Map::new(),
            },
            Variant {
                id: "with".to_owned(),
                name: "With".to_owned(),
                agent: agent.to_owned(),
                overrides: one_override("skills", Value::Array(vec![Value::String(name.clone())])),
                extra: Map::new(),
            },
        ],
    }
}

/// Patch RFC 7396 o jednym kluczu.
fn one_override(key: &str, value: Value) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert(key.to_owned(), value);
    patch
}

/// Usuwa zestaw. Przebiegi zostają — są historią biegów, nie własnością zestawu.
pub fn delete_set_inner(project: &Path, id: &str) -> Result<(), LabError> {
    let path = file::path_for(project, id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(LabError::NoSuchSet(id.to_owned()))
        }
        Err(error) => Err(LabError::Io(error)),
    }
}

/// Plan jednego przebiegu: złożony graf zapisany na dysk plus żądanie, którym się go puszcza.
#[derive(Debug, Clone)]
pub struct Planned {
    /// Żądanie dla `commands::run`.
    pub request: RunRequest,
    /// Gdzie plan wylądował — do pokazania człowiekowi, gdy zechce go otworzyć.
    pub path: PathBuf,
}

/// Składa plan przebiegu i zapisuje go obok zestawu.
///
/// Nazwa pliku niesie **identyfikator zestawu i chwilę**, więc dwa przebiegi tego samego
/// zestawu nie nadpisują się nawzajem, a katalog daje się przeczytać oczami. Identyfikator
/// workflow w planie jest tym, po czym [`read_board_inner`] rozpoznaje **swoje** biegi
/// w historii projektu.
pub fn plan_a_run_inner(
    project: &Path,
    set_id: &str,
    how_many_at_once: usize,
) -> Result<Planned, LabError> {
    let open = read_set_inner(project, set_id)?;
    if let Some(said) = open.set.why_it_cannot_run() {
        return Err(LabError::NotReady(said));
    }

    // Uuid v7, nie znacznik czasu: jest sortowalny po czasie, jest unikalny bez oglądania się
    // na sąsiada, i nie dokłada trzeciej kopii algorytmu dni→data, która stoi w tym drzewie już
    // dwa razy (`commands::run::stamp`, `memory::handoff`). Datę tego przebiegu człowiek czyta
    // z nazwy katalogu biegu, a nie z nazwy planu — plan jest plikiem dla maszyny.
    let plan = plan::compose(
        &open.set,
        workflow_id_for(set_id),
        format!("{} · Lab", open.set.name),
    );
    let path = lab::project_plans(project).join(format!("{set_id}__{}.json", Uuid::now_v7()));
    let root = path.parent().ok_or_else(|| {
        LabError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "an eval plan path has no controlled parent",
        ))
    })?;
    fs::create_dir_all(root)?;
    crate::workflow::file::save(&plan, &path, None)
        .map_err(|error| LabError::Unwritable(error.to_string()))?;

    Ok(Planned {
        request: RunRequest {
            workflow: path.clone(),
            how_many_at_once,
            // Każdy przypadek niesie własne zadanie, więc wspólnego nie ma. Puste zdanie
            // wspólne dopisałoby do każdego promptu nagłówek nad niczym.
            task: None,
            part: None::<Part>,
            handoffs_from: None,
        },
        path,
    })
}

/// Identyfikator workflow, którym plan mówi, do którego zestawu należy.
///
/// Przedrostek jest częścią kontraktu: `read_board_inner` filtruje po nim biegi projektu,
/// a zwykły workflow człowieka ma tam swój własny identyfikator i nigdy się nie trafi.
#[must_use]
pub fn workflow_id_for(set_id: &str) -> String {
    format!("eval:{set_id}")
}

/// Zestaw, jego przebiegi i różnica między dwoma ostatnimi — wszystko, co rysuje ekran.
///
/// `how_many` przycina listę przebiegów: tabela pokazuje ostatni, a trend kilka poprzednich.
/// Czytanie wszystkich biegów projektu przy każdym otwarciu ekranu byłoby kosztem, który rośnie
/// z wiekiem projektu i którego nikt nie zamawiał.
pub fn read_board_inner(
    project: &Path,
    set_id: &str,
    how_many: usize,
) -> Result<BoardWire, LabError> {
    let open = read_set_inner(project, set_id)?;
    let wanted = workflow_id_for(set_id);

    let mut runs: Vec<PastEvalWire> = runs_of(project, &wanted, how_many)
        .into_iter()
        .map(|past| score_one(&open.set, past))
        .collect();
    // Od najnowszego: człowiek pyta „jak jest teraz", a dopiero potem „czy było lepiej".
    runs.reverse();

    let movement = match runs.as_slice() {
        [newest, before, ..] => Some(MovementWire::from(results::moved(
            &scored_of(newest),
            &scored_of(before),
        ))),
        _ => None,
    };

    Ok(BoardWire {
        cannot_run: open.set.why_it_cannot_run(),
        set: open,
        runs,
        movement,
    })
}

impl From<results::Movement> for MovementWire {
    fn from(movement: results::Movement) -> Self {
        Self {
            gained: movement.gained,
            lost: movement.lost,
        }
    }
}

/// Odtwarza [`results::Scored`] z tego, co już policzył [`score_one`].
///
/// Istnieje, żeby różnica między przebiegami liczyła się **na tych samych komórkach**, które
/// widzi człowiek, a nie na drugim, świeżo policzonym zbiorze. Drugie liczenie umiałoby dać
/// inny wynik po zmianie zestawu w międzyczasie — a wtedy „+2" na ekranie nie odpowiadałoby
/// żadnej parze komórek nad nim.
fn scored_of(past: &PastEvalWire) -> results::Scored {
    results::Scored {
        cells: past
            .cells
            .iter()
            .map(|cell| results::CellResult {
                case: cell.case.clone(),
                variant: cell.variant.clone(),
                outcome: match cell.outcome.as_str() {
                    "passed" => results::Outcome::Passed,
                    "did-not-pass" => results::Outcome::DidNotPass,
                    _ => results::Outcome::NotJudged,
                },
                said: cell.said.clone(),
                cost_usd: cell.cost_usd,
            })
            .collect(),
        passed: past.passed,
        judged: past.judged,
        cost_usd: past.cost_usd,
    }
}

/// Bieg projektu, sprowadzony do tego, czego potrzebuje liczenie wyniku.
#[derive(Debug, Clone)]
struct PastRun {
    folder: String,
    when: String,
    state: String,
    steps: Vec<results::Finished>,
}

/// Biegi tego zestawu, od najstarszego, najwyżej `how_many`.
///
/// # Dlaczego to nie woła `commands::history::read_run_inner`
///
/// Bo tamta funkcja odpowiada na inne pytanie i płaci za nie inną cenę: dla każdego kroku
/// dekoduje **cały** zapisany strumień i przepuszcza go przez kurację, żeby człowiek mógł
/// przeczytać transkrypt. Tabela wyników nie potrzebuje ani jednej linii transkryptu, a ekran
/// Lab czyta kilkanaście biegów naraz — więc tamta droga zamieniłaby otwarcie zestawu
/// w sekundy pracy nad tekstem, którego nikt nie zobaczy. Czytamy więc `run.json` po swoje
/// pięć pól i przekazania po ciała, i ani bajtu więcej.
fn runs_of(project: &Path, workflow_id: &str, how_many: usize) -> Vec<PastRun> {
    let root = project.join(".loadout").join("runs");
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    // Nazwa katalogu zaczyna się od znacznika czasu w UTC, więc sortowanie po nazwie jest
    // sortowaniem po czasie (`commands::run::stamp`). `read_dir` nie obiecuje żadnej kolejności.
    dirs.sort();

    let mut out: Vec<PastRun> = Vec::new();
    for dir in dirs {
        let Some(past) = one_run(&dir, workflow_id) else {
            continue;
        };
        out.push(past);
        if out.len() > how_many {
            out.remove(0);
        }
    }
    out
}

/// `run.json` jednego biegu, tak jak leży na dysku — i wyłącznie te pola, których tu trzeba.
///
/// Nieznanych kluczy nie odrzucamy i każde pole ma wartość domyślną (niezmiennik 5): plik po
/// ręcznej edycji albo od nowszego Loadouta ma zostać pominiętym wierszem, a nie awarią całego
/// ekranu.
#[derive(Debug, Deserialize)]
struct Description {
    #[serde(default)]
    workflow_id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    steps: Vec<StepDescription>,
}

#[derive(Debug, Deserialize)]
struct StepDescription {
    /// Klucz kafelka z pliku planu — po nim poznaje się komórkę.
    #[serde(default)]
    node_key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    cost_usd: Option<f64>,
}

/// Jeden katalog biegu → to, czego potrzebuje liczenie. `None`, kiedy to nie jest bieg tego
/// zestawu albo kiedy opisu nie da się przeczytać.
fn one_run(dir: &Path, workflow_id: &str) -> Option<PastRun> {
    let text = fs::read_to_string(dir.join("run.json")).ok()?;
    let described: Description = serde_json::from_str(&text).ok()?;
    if described.workflow_id != workflow_id {
        return None;
    }

    // Ciała przekazań, po nazwie kroku, który je zostawił. Nazwa jest kluczem złączenia
    // i dlatego zapis zestawu odmawia dwóch przypadków o jednej nazwie (`lab::plan::work_name`).
    let bodies: BTreeMap<String, String> = handoff::scan_run_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .map(|one| (one.meta.from.clone(), one.body))
        .collect();

    let folder = dir.file_name()?.to_string_lossy().into_owned();
    Some(PastRun {
        when: when_of(&folder),
        state: described.status,
        steps: described
            .steps
            .into_iter()
            .map(|step| results::Finished {
                // Ciało przekazania, kiedy jest; podsumowanie, kiedy go nie ma. Powód stoi przy
                // `results::Finished::said`: podsumowanie jest jednym zdaniem przyciętym do
                // limitu, a oczekiwane pole bywa dziesiątym wierszem odpowiedzi.
                said: bodies
                    .get(&step.name)
                    .cloned()
                    .or(step.summary)
                    .unwrap_or_default(),
                tile: step.node_key,
                state: step.status,
                cost_usd: step.cost_usd,
                error: step.error.unwrap_or_default(),
            })
            .collect(),
        folder,
    })
}

/// `20260831-091412__<uuid>` → `2026-08-31 09:14`.
///
/// Z NAZWY KATALOGU, nie ze środka pliku: nazwa jest jedyną rzeczą, która stoi po biegu,
/// którego opisu nie da się przeczytać (ten sam wybór stoi w `commands::history::RunWire`).
/// Nazwa, której nie da się rozłożyć, wraca w całości — wiersz z dziwną datą jest wierszem,
/// a wiersz bez daty jest luką.
fn when_of(folder: &str) -> String {
    let stamp = folder.split("__").next().unwrap_or(folder);
    let (day, time) = match stamp.split_once('-') {
        Some(halves) if halves.0.len() == 8 && halves.1.len() >= 4 => halves,
        _ => return folder.to_owned(),
    };
    let (year, rest) = day.split_at(4);
    let (month, date) = rest.split_at(2);
    let (hour, minute) = time.split_at(2);
    format!("{year}-{month}-{date} {hour}:{}", &minute[..2])
}

/// Liczy jeden przebieg wobec DZISIEJSZEGO zestawu.
///
/// Dzisiejszego, a nie tego sprzed przebiegu, i to jest wybór: tabela ma tyle wierszy, ile ma
/// zestaw teraz, więc przebieg sprzed dopisania wiersza pokazuje w nim „nie zmierzono", a nie
/// znika. Człowiek widzi wtedy prawdę — ten wiersz jest nowszy niż tamten przebieg.
fn score_one(set: &EvalSet, past: PastRun) -> PastEvalWire {
    let scored = results::score(set, &past.steps);
    PastEvalWire {
        folder: past.folder,
        when: past.when,
        state: past.state,
        passed: scored.passed,
        judged: scored.judged,
        cost_usd: scored.cost_usd,
        cells: scored
            .cells
            .into_iter()
            .map(|cell| CellWire {
                case: cell.case,
                variant: cell.variant,
                outcome: cell.outcome.name().to_owned(),
                said: cell.said,
                cost_usd: cell.cost_usd,
            })
            .collect(),
    }
}

/// Jedna tura poza grafem: agent czyta PROJEKT i pisze kandydatki, które czekają na człowieka.
///
/// `agent` jest identyfikatorem zapisanego agenta: model, prompt systemowy i limit czasu biorą
/// się z jego definicji przez `library::agents::resolve`. Dial bezpieczeństwa jest jedynym
/// polem, którego z definicji **nie** bierzemy — wolno go tylko obniżyć (D6), a tu jest
/// obniżony do „czyta, nie zapisuje".
///
/// Kandydatki lądują na dysku **od razu**, ze statusem `suggested`. Zwrócenie ich samym oknem
/// znaczyłoby, że przeładowanie okna kasuje turę, za którą człowiek już zapłacił.
pub async fn propose_cases_inner(
    library: &Path,
    drivers: &Drivers,
    proposing: &Proposing,
    project: &Path,
    set_id: &str,
    agent: &str,
) -> Result<ProposedWire, String> {
    // JEDNA NARAZ, i odmowa PRZED czymkolwiek, co dotyka dysku albo sterownika: drugie pytanie
    // ma zostawić pierwsze nietknięte, a jedynym sposobem, żeby to była prawda, jest nie zaczynać.
    let Some(claim) = proposing.claim() else {
        return Err(ALREADY_PROPOSING.to_owned());
    };

    let open = read_set_inner(project, set_id).map_err(|error| error.to_string())?;
    let saved = the_agent_saved_as(library, agent).map_err(|error| error.to_string())?;
    let effective = resolve(&saved, &Overrides::default())
        .map_err(|error| error.to_string())?
        .agent;

    let run = Uuid::now_v7();
    let spec = RunSpec {
        run_id: run,
        // KORZEŃ PROJEKTU, i to jest inna obietnica niż przy imporcie. Tam tura pracuje w pustym
        // katalogu, bo pytanie dotyczy dwóch plików, które jadą w jego treści; tutaj pytanie
        // brzmi „przeczytaj ten projekt", więc projekt musi być tym, co agent widzi. Ekran mówi
        // to wprost, zanim człowiek kliknie.
        cwd: project.to_path_buf(),
        prompt: cases::ask_for_cases(&open.set.subject, AT_MOST),
        model: some_text(&effective.model),
        system_append: some_text(&effective.instructions),
        // DIAL WOLNO TYLKO OBNIŻYĆ (D6). Pisanie kandydatek nie wymaga zapisu, a dial skopiowany
        // z definicji wygląda poprawnie do chwili, w której człowiek każe pisać kandydatki
        // swojemu najmocniejszemu agentowi.
        policy: Policy::ReadOnly,
        // Materiał ma pochodzić z tego projektu, nie z sieci: kandydatka wzięta z internetu nie
        // jest ugruntowana w niczym, co człowiek może otworzyć.
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };

    // Odbiór staje PRZED startem sterownika: vendor ma prawo powiedzieć pierwsze zdarzenia
    // jeszcze w `start`, a kanał bez odbiorcy zatrzymałby go na pierwszym pełnym buforze.
    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
    let drain = tokio::spawn(off_the_wire(inbox));

    let said = match (drivers)(effective.runs_with).start(spec, events).await {
        Err(error) => Err(error.to_string()),
        Ok(mut handle) => {
            let limit = give_up_after(effective.give_up_after_minutes);
            let ended = one_turn(&mut *handle, &claim.stop, limit).await;
            what_came_of_it(&mut *handle, ended, limit).await
        }
    };

    // Uchwyt zszedł razem z gałęzią wyżej, więc kanał jest zamknięty i drenaż kończy się sam.
    let _ = drain.await;
    let said = said?;

    let taken: BTreeSet<String> = open.set.cases.iter().map(|case| case.id.clone()).collect();
    let proposed = cases::read(&said, &taken);
    if proposed.cases.is_empty() && proposed.without_a_reason == 0 && proposed.unfinished == 0 {
        return Err(SAID_NOTHING.to_owned());
    }

    let written = proposed.cases.len();
    let mut set = open.set;
    set.cases.extend(proposed.cases);
    let revision =
        save_set_inner(project, &set, Some(&open.revision)).map_err(|error| error.to_string())?;

    Ok(ProposedWire {
        set: OpenSet { set, revision },
        written,
        without_a_reason: proposed.without_a_reason,
        unfinished: proposed.unfinished,
    })
}

/// Co z tury wynikło: tekst agenta, anulowanie jako odmowa z własnym zdaniem, albo powód.
async fn what_came_of_it(
    handle: &mut dyn crate::engine::drivers::AgentHandle,
    ended: Ended,
    limit: Duration,
) -> Result<String, String> {
    match ended {
        // PRZEKROCZONY LIMIT IDZIE TĄ SAMĄ DROGĄ, CO STOP: przez sterownik, po dowód. Powód
        // nazywa limit czasu i liczbę, którą trzeba zmienić — inaczej człowiek szuka wady
        // w agencie, którego nikt nie zepsuł.
        Ended::Overdue => {
            let minutes = limit.as_secs() / 60;
            Err(match handle.cancel().await {
                GroupProof::Alive { .. } => format!(
                    "Writing cases ran longer than its {minutes} minute limit, and Loadout could \
                     not make sure the agent stopped, so it may still be running."
                ),
                GroupProof::Dead { .. } => format!(
                    "Writing cases ran longer than its {minutes} minute limit, so Loadout \
                     stopped it. Give that agent more minutes."
                ),
            })
        }
        // ANULOWANIE IDZIE PRZEZ STEROWNIK, nie przez zdjęcie zadania Rusta (niezmienniki 6 i 10).
        Ended::Stopped => Err(match handle.cancel().await {
            GroupProof::Dead { .. } => "You stopped this before it wrote anything.".to_owned(),
            GroupProof::Alive { .. } => MAY_STILL_BE_RUNNING.to_owned(),
        }),
        Ended::Turn(Err(error)) => Err(error.to_string()),
        Ended::Turn(Ok(turn)) => {
            // Normalne zakończenie idzie przez `close`: `claude` z otwartym stdinem czeka
            // w nieskończoność, więc tura bez tego zostawia żywy proces [T1 §2, §4.6].
            let code = handle.close().await.ok().flatten();
            // Sukces to zero **i** `ok` (niezmiennik 19). Agent, który wypisał „nie dam rady"
            // i wyszedł czysto, nie napisał ani jednej kandydatki.
            if !turn.ok || !matches!(code, None | Some(0)) {
                return Err(SAID_NOTHING.to_owned());
            }
            let said = turn.text.trim();
            if said.is_empty() {
                return Err(SAID_NOTHING.to_owned());
            }
            Ok(said.to_owned())
        }
    }
}

/// Zmienia status jednego przypadku — to jest całe „accept" i całe „discard" po stronie Rusta.
///
/// Awansuje wyłącznie człowiek (`lab::CaseStatus`), więc ta funkcja jest jedynym miejscem,
/// z którego kandydatka może stać się przypadkiem w użyciu. Odrzucenie **kasuje** ją z pliku,
/// zamiast zostawiać trzeci stan: notatka ma trzy stany, bo jej odrzucenie niesie wiedzę
/// („tego już nie proponuj"); przypadek odrzucony nie niesie żadnej, a lista zestawu ma zostać
/// listą tego, co mierzymy.
pub fn decide_case_inner(
    project: &Path,
    set_id: &str,
    case_id: &str,
    keep: bool,
    expected: Option<&str>,
) -> Result<OpenSet, LabError> {
    let open = read_set_inner(project, set_id)?;
    let mut set = open.set;
    let found = set.cases.iter_mut().find(|case| case.id == case_id);
    let Some(case) = found else {
        return Err(LabError::NoSuchSet(format!("{set_id}/{case_id}")));
    };
    if keep {
        case.status = CaseStatus::InUse;
    } else {
        set.cases.retain(|case| case.id != case_id);
    }
    let expected = expected.or(Some(open.revision.as_str()));
    let revision = save_set_inner(project, &set, expected)?;
    Ok(OpenSet { set, revision })
}

/// Dopisuje albo poprawia jeden przypadek napisany ręcznie.
///
/// Osobno od [`save_set_inner`], choć zapisuje ten sam plik: okno edytuje **jeden wiersz**,
/// a odesłanie całego zestawu z okna znaczyłoby, że okno jest autorytetem o wszystkich
/// pozostałych wierszach — także tych, których człowiek w tej chwili nie widzi.
pub fn put_case_inner(
    project: &Path,
    set_id: &str,
    case: Case,
    expected: Option<&str>,
) -> Result<OpenSet, LabError> {
    let open = read_set_inner(project, set_id)?;
    let mut set = open.set;
    match set.cases.iter_mut().find(|one| one.id == case.id) {
        Some(existing) => *existing = case,
        None => set.cases.push(case),
    }
    let expected = expected.or(Some(open.revision.as_str()));
    let revision = save_set_inner(project, &set, expected)?;
    Ok(OpenSet { set, revision })
}

/// To samo dla kolumny.
pub fn put_variant_inner(
    project: &Path,
    set_id: &str,
    variant: Variant,
    expected: Option<&str>,
) -> Result<OpenSet, LabError> {
    let open = read_set_inner(project, set_id)?;
    let mut set = open.set;
    match set.variants.iter_mut().find(|one| one.id == variant.id) {
        Some(existing) => *existing = variant,
        None => set.variants.push(variant),
    }
    let expected = expected.or(Some(open.revision.as_str()));
    let revision = save_set_inner(project, &set, expected)?;
    Ok(OpenSet { set, revision })
}

/// Zdejmuje kolumnę. Przebiegi zostają — komórka po zdjętej kolumnie po prostu nie ma wiersza.
pub fn drop_variant_inner(
    project: &Path,
    set_id: &str,
    variant_id: &str,
    expected: Option<&str>,
) -> Result<OpenSet, LabError> {
    let open = read_set_inner(project, set_id)?;
    let mut set = open.set;
    set.variants.retain(|one| one.id != variant_id);
    let expected = expected.or(Some(open.revision.as_str()));
    let revision = save_set_inner(project, &set, expected)?;
    Ok(OpenSet { set, revision })
}

/// Poprawka, którą agent proponuje po przebiegu — **zanim** ktokolwiek ją zastosuje.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixWire {
    /// Identyfikator agenta, którego to dotyczy — ten sam, który przyjdzie z powrotem w Apply.
    pub agent: String,
    /// Nazwa agenta, żeby karta mówiła, czyj tekst człowiek właśnie czyta.
    pub name: String,
    /// Dlaczego. Człowiek czyta to PRZED tekstem.
    pub because: String,
    /// Cały nowy tekst instrukcji.
    pub instructions: String,
    /// Instrukcje, które ten agent ma **teraz** — żeby dało się przeczytać obie strony.
    pub instead_of: String,
    /// Rewizja pliku agenta **w chwili propozycji**.
    ///
    /// Wraca z Apply i tam jest oczekiwaniem: definicja zmieniona między przeczytaniem
    /// poprawki a jej zastosowaniem ma zapis ODRZUCIĆ, a nie cofnąć. Rewizja pobrana dopiero
    /// przy Apply opisywałaby inną chwilę niż tekst, który człowiek właśnie przeczytał — czyli
    /// zamykałaby okno o zero sekund za późno.
    pub revision: Option<String>,
}

/// Zdanie o poprawce, o którą poproszono przed pierwszym przebiegiem.
const NOTHING_TO_FIX: &str =
    "Nothing here has come back wrong yet, so there is nothing to fix. Press Run first.";

/// Zdanie o poprawce dla umiejętności.
///
/// Nie przycisk, który odmawia, tylko przycisk, którego nie ma, i to zdanie obok. Powód
/// w całości stoi w nagłówku `lab::fix`: tekst umiejętności napisany przez model jest tak samo
/// nieufny jak wklejony z linku, więc jedyną drogą do `SKILL.md` jest ta, która przechodzi
/// przez skaner.
pub const A_SKILL_GOES_THROUGH_SKILLS: &str = "A change to a skill goes through the same check as one pasted from a link, so it is \
     written over in Skills.";

/// Jedna tura poza grafem: agent czyta to, co nie przeszło, i proponuje nowy tekst instrukcji.
///
/// **Nie stosuje niczego.** Powód stoi w nagłówku `lab::fix`: instrukcja agenta jest tym, co on
/// robi w każdym biegu, także poza Labem, a pętla przepisująca ją bez człowieka zmieniałaby
/// zachowanie agenta w nocy.
pub async fn propose_fix_inner(
    library: &Path,
    drivers: &Drivers,
    proposing: &Proposing,
    project: &Path,
    set_id: &str,
    writer: &str,
) -> Result<FixWire, String> {
    let Some(claim) = proposing.claim() else {
        return Err(ALREADY_PROPOSING.to_owned());
    };

    let board = read_board_inner(project, set_id, 1).map_err(|error| error.to_string())?;
    let Subject::Agent { id: subject } = &board.set.set.subject else {
        return Err(A_SKILL_GOES_THROUGH_SKILLS.to_owned());
    };

    let failures = what_did_not_pass(&board);
    if failures.is_empty() {
        return Err(NOTHING_TO_FIX.to_owned());
    }

    let mended = the_agent_saved_as(library, subject).map_err(|error| error.to_string())?;
    let saved = the_agent_saved_as(library, writer).map_err(|error| error.to_string())?;
    let effective = resolve(&saved, &Overrides::default())
        .map_err(|error| error.to_string())?
        .agent;

    let spec = RunSpec {
        run_id: Uuid::now_v7(),
        // Korzeń projektu: poprawka ma prawo zajrzeć w kod, o który potknęła się praca.
        cwd: project.to_path_buf(),
        prompt: fix::ask_for_a_fix(&mended.name, &mended.instructions, &failures),
        model: some_text(&effective.model),
        system_append: some_text(&effective.instructions),
        // DIAL WOLNO TYLKO OBNIŻYĆ (D6). Poprawka jest tekstem, nie zapisem: agent, który ją
        // pisze, nie ma powodu dotykać ani jednego pliku.
        policy: Policy::ReadOnly,
        reaches_the_web: false,
        tools: None,
        extra_dirs: Vec::new(),
        resume: None,
    };

    let (events, inbox) = mpsc::channel::<DecodedEvent>(EVENT_QUEUE);
    let drain = tokio::spawn(off_the_wire(inbox));
    let said = match (drivers)(effective.runs_with).start(spec, events).await {
        Err(error) => Err(error.to_string()),
        Ok(mut handle) => {
            let limit = give_up_after(effective.give_up_after_minutes);
            let ended = one_turn(&mut *handle, &claim.stop, limit).await;
            what_came_of_it(&mut *handle, ended, limit).await
        }
    };
    let _ = drain.await;
    let said = said?;

    let Some(fixed) = fix::read_fix(&said) else {
        // Pusta poprawka z przyciskiem Apply skasowałaby instrukcję agenta jednym kliknięciem,
        // wyglądając dokładnie jak poprawka prawdziwa.
        return Err("The agent answered without a new set of instructions in it.".to_owned());
    };

    Ok(FixWire {
        revision: revision_of_agent(library, subject),
        agent: mended.id.to_string(),
        name: mended.name.clone(),
        because: fixed.because,
        instructions: fixed.instructions,
        instead_of: mended.instructions,
    })
}

/// Rewizja pliku tego agenta, albo `None`, kiedy biblioteki nie da się przeczytać.
///
/// `None` znaczy „nie wiem", a Apply czyta to jako brak oczekiwania — czyli zapis, który
/// niczego nie broni. To jest gorsze niż odmowa, ale lepsze niż oczekiwanie ZMYŚLONE: rewizja
/// wymyślona odrzucałaby każdy zapis, a wtedy poprawka nie miałaby jak wejść nigdy.
fn revision_of_agent(library: &Path, id: &str) -> Option<String> {
    crate::commands::agents::list_agent_definitions_inner(library)
        .ok()?
        .into_iter()
        .find_map(|definition| match definition {
            crate::library::definition::Definition::Healthy { value, revision }
                if value.id.to_string() == id =>
            {
                Some(revision)
            }
            _ => None,
        })
}

/// Zdania z komórek, które nie przeszły w NAJNOWSZYM przebiegu — po jednym na komórkę.
///
/// Tylko najnowszy: poprawka odpowiada na to, jak jest teraz. Zdania ze starszych przebiegów
/// opisują pracę sprzed poprzedniej poprawki i uczyłyby model naprawiać rzeczy naprawione.
fn what_did_not_pass(board: &BoardWire) -> Vec<String> {
    let Some(newest) = board.runs.first() else {
        return Vec::new();
    };
    newest
        .cells
        .iter()
        .filter(|cell| cell.outcome == results::Outcome::DidNotPass.name())
        .map(|cell| {
            let row = board
                .set
                .set
                .cases
                .iter()
                .find(|one| one.id == cell.case)
                .map_or(cell.case.as_str(), |one| one.name.as_str());
            let column = board
                .set
                .set
                .variants
                .iter()
                .find(|one| one.id == cell.variant)
                .map_or(cell.variant.as_str(), |one| one.name.as_str());
            format!("{row} ({column}): {}", cell.said)
        })
        .collect()
}

/// Stosuje poprawkę: zapisuje nowy tekst instrukcji agenta.
///
/// `expected` jest rewizją pliku agenta, którą okno przeczytało — bez niej Apply skasowałby
/// cudzą, nowszą zmianę tego samego agenta bez jednego zdania.
///
/// Zmienia **jedno pole**. Reszta definicji jedzie z dysku nietknięta: poprawka opisuje tekst,
/// a nie model, dial ani limit czasu, i implementacja składająca całego agenta z tego, co
/// przyszło z okna, oddawałaby oknu władzę nad polami, o których ta karta nic nie mówi.
pub fn apply_fix_inner(
    library: &Path,
    agent: &str,
    instructions: String,
    expected: Option<&str>,
) -> Result<String, LabError> {
    let mut saved = the_agent_saved_as(library, agent)
        .map_err(|error| LabError::Unreadable(error.to_string()))?;
    saved.instructions = instructions;
    crate::commands::agents::save_agent_inner(library, &saved, expected)
        .map(|written| written.revision)
        .map_err(|error| LabError::Unwritable(error.to_string()))
}
