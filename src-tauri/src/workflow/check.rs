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
use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use super::{Folder, Link, Step, WorkflowFile};
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

/// Wartości, których przelotka nie podnosi **niezależnie** od nazwy flagi.
///
/// Dial „co agent może zrobić z plikami" jest jedyną drogą do tych dwóch (ARCHITECTURE §6b
/// reguła 2, D6). Sama lista zarezerwowanych by nie wystarczyła: `--sandbox` nie jest na niej,
/// a `--sandbox danger-full-access` omija dial tak samo skutecznie jak `-s`.
pub const FORBIDDEN_ESCALATIONS: [&str; 2] = ["bypassPermissions", "danger-full-access"];

/// Zdanie z uruchomienia w T3 §5.2.
///
/// Mówi, co się stanie, a nie jak nazywa się algorytm, który to znalazł: `cycle detected in DAG`
/// jest zdaniem, z którym użytkownik nie może zrobić nic (niezmiennik 14).
const CIRCLE: &str = "These steps point back at each other in a circle. Work would never finish.";

/// Ile kopii jednego kroku naraz wolno zamówić [T3 §4.4]. Osiem jednoczesnych na prawdziwej
/// maszynie to już dużo.
const MOST_COPIES: u8 = 8;

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
}

/// Wszystko, co da się powiedzieć o pliku bez uruchamiania go.
///
/// Wołane przy **zapisie** (niezmiennik 12: odmowa pada tam, nie w trakcie biegu) i drugi raz
/// przy Run — to drugie dowodzi T-15.
#[must_use]
pub fn check(workflow: &WorkflowFile) -> Vec<Note> {
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

    // Kolejność reguł jest kolejnością, w jakiej użytkownik zobaczy uwagi, a `save()` odmawia
    // zdaniem PIERWSZEGO problemu — więc idzie od „ten plik nie trzyma się kupy" do „ten bieg
    // by nie wyszedł". Ostrzeżenia na końcu: nie blokują niczego.
    let mut notes = Vec::new();
    one_id_two_steps(&steps, &mut notes);
    arrows_into_nowhere(&workflow.links, &steps, &position, &mut notes);
    copies_out_of_range(&steps, &mut notes);
    the_passthrough(&steps, &mut notes);
    a_circle(&steps, &arrows, &mut notes);
    one_folder_two_steps(&steps, &arrows, &mut notes);
    islands(&steps, &arrows, &mut notes);
    notes
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
}

fn facts(step: &Step) -> Facts<'_> {
    match step {
        Step::Agent(agent) => Facts {
            id: &agent.id,
            name: &agent.name,
            copies: agent.copies,
            folder: Some(&agent.folder),
            passthrough: Some(&agent.vendor_options),
        },
        Step::Checkpoint(checkpoint) => Facts {
            id: &checkpoint.id,
            name: &checkpoint.name,
            copies: 1,
            folder: None,
            passthrough: None,
        },
    }
}

/// Uwaga, która blokuje Run i zapis.
fn problem(step_id: Option<&str>, message: String) -> Note {
    Note {
        level: Level::Problem,
        step_id: step_id.map(String::from),
        message,
    }
}

/// Uwaga, która nie blokuje niczego.
fn warning(step_id: Option<&str>, message: String) -> Note {
    Note {
        level: Level::Warning,
        step_id: step_id.map(String::from),
        message,
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

/// Dwa kroki, które **mogą biec równocześnie**, piszące po tych samych plikach.
///
/// „Mogą biec równocześnie" znaczy dokładnie jedno: nie istnieje ścieżka po strzałkach ani
/// stąd tam, ani stamtąd tu. Reguła bez tego zdania odmawia zwykłego łańcucha `plan → build`,
/// ktoś zgłasza to jako błąd, ktoś inny „naprawia" ją przez wyłączenie — i zostaje martwy kod
/// (niezmiennik 12).
fn one_folder_two_steps(steps: &[Facts<'_>], arrows: &[(usize, usize)], notes: &mut Vec<Note>) {
    for step in steps {
        // Krok w kilku kopiach biegnie równocześnie sam ze sobą — z definicji, bez żadnej
        // strzałki. T3 proponował tu podpowiedź; niezmiennik 12 mówi „odmowa przy zapisie",
        // a niezmiennik wygrywa nad raportem.
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
            notes.push(problem(
                Some(one.id),
                format!(
                    "\"{}\" and \"{}\" can run at the same time and {}. Give one of them a fresh \
                     copy.",
                    one.name,
                    other.name,
                    place(mine, theirs)
                ),
            ));
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
