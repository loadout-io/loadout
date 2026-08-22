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
pub const RESERVED_CLAUDE: [&str; 7] = [
    "--session-id",
    "--output-format",
    "--input-format",
    "--verbose",
    "--permission-mode",
    "--strict-mcp-config",
    "--setting-sources",
];

/// To samo dla `codex`: `-C` (katalog roboczy), `-s` (piaskownica), `--json` (strumień zdarzeń).
pub const RESERVED_CODEX: [&str; 3] = ["-C", "-s", "--json"];

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
pub const FORBIDDEN_ESCALATIONS: [&str; 3] = [
    "bypassPermissions",
    "--dangerously-skip-permissions",
    "danger-full-access",
];

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
    turns_out_of_range(&workflow.links, &steps, &position, &mut notes);
    loops_that_cross(&workflow.links, &steps, &position, &forward, &mut notes);
    a_step_without_an_agent(&steps, when, &mut notes);
    a_step_without_a_task(&steps, when, &mut notes);
    a_command_step_left_empty(&steps, &mut notes);
    conditional_routes(workflow, &mut notes);
    the_passthrough(&steps, &mut notes);
    a_circle(&steps, &forward, &mut notes);
    // STRZAŁKI BEZ POWROTÓW, nie wszystkie, i to jest ta sama lista, po której liczy się koło.
    // Powrót wchodzi do kroku dopiero w rundzie drugiej (`workflow::unroll`), więc krok, do
    // którego prowadzi WYŁĄCZNIE powrót, w rundzie pierwszej dalej nie ma nikogo przed sobą —
    // a to jest dokładnie ta runda, która ruszy jako pierwsza.
    nothing_before_it(&steps, &forward, &mut notes);
    one_folder_two_steps(&steps, &arrows, when, &mut notes);
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

/// Przelotka podnosząca flagę, którą Loadout ustawia sam.
///
/// Dwie granice, obie przy zapisie: kolizja z naszą flagą i próba podniesienia dialu „co agent
/// może zrobić z plikami". Druga jest **niezależna od listy** — `--sandbox` nie jest
/// zarezerwowane, a `--sandbox danger-full-access` omija dial dokładnie tak samo jak `-s`.
fn the_passthrough(steps: &[Facts<'_>], notes: &mut Vec<Note>) {
    for step in steps {
        let Some(options) = step.passthrough else {
            continue;
        };
        for (vendor, flags) in options {
            for (flag, value) in flags {
                if let Some(raise) = FORBIDDEN_ESCALATIONS
                    .iter()
                    .copied()
                    .find(|raise| flag.contains(raise) || value.contains(raise))
                {
                    notes.push(problem(
                        Some(step.id),
                        format!(
                            "\"{}\" tries to set {raise} through its {} options. What an agent \
                             may do with your files is set on the step itself.",
                            step.name,
                            vendor_name(vendor)
                        ),
                    ));
                } else if reserved(vendor).contains(&flag.as_str()) {
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
fn reserved(vendor: &str) -> &'static [&'static str] {
    match vendor {
        "claude" => &RESERVED_CLAUDE,
        "codex" => &RESERVED_CODEX,
        _ => &[],
    }
}

/// Nazwa vendora tak, jak nazywa go użytkownik. Klucz z pliku (`claude`) na ekran nie idzie.
fn vendor_name(vendor: &str) -> &str {
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
    for (first, one) in steps.iter().enumerate() {
        for (second, other) in steps.iter().enumerate().skip(first + 1) {
            if reach[first][second] || reach[second][first] {
                continue;
            }
            let (Some(mine), Some(theirs)) = (one.folder, other.folder) else {
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
                place(mine, theirs)
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

/// Czy dwa foldery to te same pliki.
fn the_same_files(one: &Folder, other: &Folder) -> bool {
    match (one, other) {
        (Folder::Project, Folder::Project) => true,
        (Folder::Pick { path: mine }, Folder::Pick { path: theirs }) => {
            // Po SEGMENTACH, nie po znakach: `/Users/x/api2` zaczyna się tak samo jak
            // `/Users/x/api`, a jest zupełnie innym folderem. `Path::starts_with` jest jedyną
            // wersją tego porównania, która o tym wie — `str::starts_with` wysyła użytkownika
            // do naprawiania czegoś, co nie jest zepsute.
            Path::new(mine).starts_with(theirs) || Path::new(theirs).starts_with(mine)
        }
        // `fresh-copy` nie koliduje z niczym — to jest cała obietnica izolacji z ARCHITECTURE
        // §2 punkt 4. `project` kontra `pick` też nie: 2026-08-16 — w pliku workflow nie ma
        // ścieżki projektu, bo projekt wybiera się przy uruchomieniu, więc porównanie ich tutaj
        // byłoby zgadywaniem. Tę parę widzi dopiero bieg (T-15), który zna oba katalogi.
        //
        // 2026-08-20 (T-56) — `same-copy` WPADA TU BEZ ODPOWIEDZI I TO JEST PRZYZNANE WPROST,
        // a nie przeoczone. Dwa kroki „to samo drzewo" schodzące z JEDNEGO poprzednika pracują
        // w jednym katalogu i mogą biec równocześnie: to jest kolizja z niezmiennika 12, której
        // ta funkcja nie zgłasza. Dwa schodzące z RÓŻNYCH drzew kolizją nie są. Rozróżnia je
        // wyłącznie graf, a tutaj wchodzi para folderów i nic więcej — więc `true` odmawiałoby
        // zwykłemu rozwidleniu, a `false` jest brakiem odpowiedzi, nie odpowiedzią „nie koliduje".
        // Żadne kryterium tego nie sądzi (TASK.md, „Świadomie poza zakresem"): to jest luka
        // w wyroczni, zgłoszona właścicielowi, a nie defekt do cichego załatania regułą, której
        // nikt nie mierzy.
        _ => false,
    }
}

/// Druga połowa zdania o kolizji: gdzie te dwa kroki się spotykają.
fn place(one: &Folder, other: &Folder) -> String {
    match (one, other) {
        (Folder::Pick { path: mine }, Folder::Pick { path: theirs }) if mine == theirs => {
            format!("both work in {mine}")
        }
        (Folder::Pick { .. }, Folder::Pick { .. }) => {
            "one of their folders is inside the other".to_owned()
        }
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
