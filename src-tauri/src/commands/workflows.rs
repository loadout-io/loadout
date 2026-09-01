//! Komendy plików workflow: wczytaj, zapisz, sprawdź.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Format pliku, migracje w przód i odmowa przy zapisie mieszkają w `workflow::file` (T-12),
//! a wszystko, co da się o pliku powiedzieć bez uruchamiania go, w `workflow::check`. Te trzy
//! funkcje nie powtarzają ani jednej z tych reguł: składają ścieżkę i oddają to, co powiedział
//! walidator — **co do uwagi**.
//!
//! Ostatnie zdanie jest całym powodem, dla którego `check_workflow_inner` w ogóle istnieje jako
//! osobna komenda. Komenda, która gubi uwagi walidatora, jest gorsza niż jej brak: front
//! narysuje wtedy zielono plik, który Rust odrzuci przy Starcie, i człowiek dowie się o tym
//! dopiero od biegu, który nie ruszył.

use std::borrow::Borrow;
use std::io;
use std::path::{Path, PathBuf};

use crate::durable_file::{DurableFilePublisher, PublishError, revision_of};
use crate::engine::supervisor::{PublicationEntryKind, PublicationRoot};
use crate::library::definition::{Definition, Shelf, healthy_only, workflow_problem};
use crate::workflow::WorkflowFile;
use crate::workflow::check::Note;
use crate::workflow::file::{LoadError, SaveError};

/// Katalog workflow wewnątrz biblioteki: `~/.loadout/workflows/` (`docs/ARCHITECTURE.md` §8).
const WORKFLOWS_DIR: &str = "workflows";

/// Jedna pozycja listy: nazwa pliku i to, co w nim leży.
///
/// Lustro `WorkflowEntry` z `src/sections/workflows/list/store.ts`, pole w pole. `path` to
/// **sama nazwa pliku**, nigdy pełna ścieżka: katalog rozwiązuje ta warstwa, a front, który
/// doklejałby go sam, byłby drugim miejscem, w którym mieszka odpowiedź na pytanie „gdzie to
/// leży" [T3 §8.3].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowEntry {
    /// `ship-a-feature.json` — bez katalogu i bez `~`.
    pub path: String,
    pub workflow: WorkflowFile,
}

/// Otwarty plik razem z rewizją, na której okno go czyta.
///
/// 2026-08-28 — para, nie sam plik, i to jest cała treść tego typu. Zapis, który nie wie, co
/// czytał, nie ma jak odmówić spóźnionemu nadpisaniu; a rewizja policzona drugi raz, już po
/// odczycie, opisywałaby bajty, których nikt nie widział.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenWorkflow {
    pub workflow: WorkflowFile,
    pub revision: String,
}

/// Gdzie plik wylądował i jaką rewizję ma teraz.
///
/// Rewizja wraca, żeby okno mogło pisać dalej bez ponownego czytania pliku po każdej literze —
/// a każdy taki odczyt byłby nowym oknem na cudzą pracę.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    pub path: PathBuf,
    pub revision: String,
}

/// `home/workflows/<file_name>`, albo odmowa, kiedy `file_name` nie jest nazwą pliku.
///
/// Nazwa przychodzi z okna, więc jest wejściem, któremu nie ufamy (T3 §5.2). `Path::join`
/// z `../../.ssh/config` wychodzi poza bibliotekę bez jednego ostrzeżenia, a `join("/etc/x")`
/// **odrzuca cały prefiks** i zwraca `/etc/x` — czyli komenda „zapisz workflow" pisałaby
/// dokładnie tam, gdzie każe jej webview. Zapora jest tutaj, bo to jedyna warstwa, która wie,
/// że ten napis ma być nazwą, a nie ścieżką.
fn in_library(home: &Path, file_name: &str) -> Result<PathBuf, io::Error> {
    // `Path::file_name` zamiast ręcznego szukania separatorów: reguła „czym jest nazwa pliku"
    // należy do systemu plików, a nie do naszej listy zakazanych znaków. Nazwa, która nie jest
    // swoją własną nazwą pliku, niesie katalog — i o to właśnie pytamy.
    if Path::new(file_name)
        .file_name()
        .is_none_or(|name| name != file_name)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{file_name} is not the name of a file in the workflow folder"),
        ));
    }
    Ok(home.join(WORKFLOWS_DIR).join(file_name))
}

/// Zapisuje workflow pod `file_name` w bibliotece i oddaje ścieżkę, pod którą wylądował.
///
/// `file_name` to **sama nazwa pliku** (`ship-a-feature.json`), nigdy pełna ścieżka: katalog
/// rozwiązuje ta warstwa, po stronie Rusta. Front, który dokleja katalog sam, jest drugim
/// miejscem, w którym mieszka odpowiedź na pytanie „gdzie to leży" [T3 §8.3].
///
/// `impl Borrow<WorkflowFile>` zamiast `&WorkflowFile` — powód jest ten sam, co przy
/// `agents::save_agent_inner`: skorupa `#[tauri::command]` dostaje plik **wartością** (serde
/// musi go gdzieś zbudować) i nie ma go komu oddać. Wołający z pożyczką w ręku nie zauważa
/// różnicy.
///
/// `expected` przechodzi na wylot do `file::save`: `None` znaczy „tego pliku ma jeszcze nie
/// być", `Some(rewizja)` — „nadpisz dokładnie te bajty, które przeczytałem".
pub fn save_workflow_inner(
    home: &Path,
    file_name: &str,
    workflow: impl Borrow<WorkflowFile>,
    expected: Option<&str>,
) -> Result<Saved, SaveError> {
    let path = in_library(home, file_name).map_err(SaveError::Unwritable)?;
    // Katalog powstaje tutaj, bo `file::save` (T-12) pisze plik i nie zakłada katalogów —
    // celowo, bo tam jego brak jest awarią, a tutaj jest normalnym stanem biblioteki, w której
    // nikt jeszcze niczego nie zapisał.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SaveError::Unwritable)?;
    }

    // Odmowa walidatora, kolejność „sprawdź, potem dotknij dysku" i deterministyczny tekst
    // mieszkają w `file::save`. Ta funkcja nie powtarza ani jednej z tych decyzji.
    let revision = crate::workflow::file::save(workflow.borrow(), &path, expected)?;
    Ok(Saved { path, revision })
}

/// Wczytuje workflow spod `file_name` w bibliotece razem z jego rewizją.
pub fn load_workflow_inner(home: &Path, file_name: &str) -> Result<OpenWorkflow, LoadError> {
    let path = in_library(home, file_name).map_err(LoadError::Unreadable)?;
    let dir = home.join(WORKFLOWS_DIR);
    let publisher = DurableFilePublisher::new(&dir);
    let mut loaded = None;
    publisher
        .recover_with(|root| {
            loaded = Some(
                root.read_regular(Path::new(file_name), false)
                    .map_err(LoadError::Unreadable)
                    .and_then(|bytes| {
                        // Rewizja z TYCH bajtów, nie z ponownego odczytu: para „co pokazałem"
                        // i „co przy tym leżało na dysku" musi pochodzić z jednego spojrzenia.
                        let revision = revision_of(&bytes);
                        crate::workflow::file::load_snapshot(&path, &bytes)
                            .map(|workflow| OpenWorkflow { workflow, revision })
                    }),
            );
            Ok(())
        })
        .map_err(|error| LoadError::Unreadable(error.into_io()))?;
    loaded.ok_or_else(|| {
        LoadError::Unreadable(io::Error::other("the recovered workflow file was not read"))
    })?
}

/// Zdrowe workflowy dla Rustowych callerów, którzy nie renderują problemów biblioteki.
pub fn list_workflows_inner(home: &Path) -> Result<Vec<WorkflowEntry>, LoadError> {
    Ok(healthy_only(list_workflow_definitions_inner(home)?))
}

/// Wszystko, co leży w katalogu workflow. Błąd jednego pliku jest jednym wpisem problemu;
/// błąd całego kontrolowanego katalogu nadal odmawia operacji.
pub fn list_workflow_definitions_inner(
    home: &Path,
) -> Result<Vec<Definition<WorkflowEntry>>, LoadError> {
    let dir = home.join(WORKFLOWS_DIR);
    let publisher = DurableFilePublisher::new(&dir);
    let mut listed = None;
    match publisher.recover_with(|root| {
        listed = Some(list_workflow_definitions_from_root(root, &dir));
        Ok(())
    }) {
        Ok(()) => listed.ok_or_else(|| {
            LoadError::Unreadable(io::Error::other(
                "the recovered workflow library was not listed",
            ))
        })?,
        // Biblioteka bez katalogu pozostaje legalnie pusta. Recovery rozróżnia ten przypadek
        // od symlinka lub pliku podstawionego pod nazwę kontrolowanego katalogu.
        Err(PublishError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(LoadError::Unreadable(error.into_io())),
    }
}

fn list_workflow_definitions_from_root(
    root: &PublicationRoot,
    dir: &Path,
) -> Result<Vec<Definition<WorkflowEntry>>, LoadError> {
    // Wyłącznie regularne `.json`, więc symlink oraz `.json.bak` nie stają się definicją.
    let mut names = root
        .list_directory(Path::new(""))
        .map_err(LoadError::Unreadable)?
        .into_iter()
        .filter(|entry| {
            // 2026-08-28: APFS domyślnie zderza `New-Workflow.JSON` z kanonicznym
            // `new-workflow.json`. Problem musi wejść do zajętych nazw przed Create/Duplicate.
            entry.kind == PublicationEntryKind::Regular
                && Path::new(&entry.name)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
    names.sort();

    let mut out = Vec::with_capacity(names.len());
    for file_name in names {
        let path = dir.join(&file_name);
        let name = file_name.to_str().ok_or_else(|| {
            LoadError::Unreadable(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a name Loadout can read", path.display()),
            ))
        })?;
        let value = match root.read_regular(Path::new(&file_name), false) {
            Ok(bytes) => match crate::workflow::file::load_snapshot(&path, &bytes) {
                Ok(workflow) => Definition::Healthy {
                    // Rewizja z bajtów, które ten spacer i tak przeczytał — drugi odczyt tego
                    // samego pliku opisywałby inną chwilę niż wypisana definicja.
                    revision: revision_of(&bytes),
                    value: WorkflowEntry {
                        path: name.to_owned(),
                        workflow,
                    },
                },
                Err(error) => Definition::DefinitionProblem {
                    shelf: Shelf::Workflows,
                    file_name: name.to_owned(),
                    problem: workflow_problem(&error),
                },
            },
            Err(_error) => Definition::DefinitionProblem {
                shelf: Shelf::Workflows,
                file_name: name.to_owned(),
                problem: crate::library::definition::DefinitionProblemKind::Unreadable,
            },
        };
        out.push(value);
    }
    Ok(out)
}

/// Usuwa plik workflow z biblioteki.
pub fn delete_workflow_inner(home: &Path, file_name: &str) -> Result<(), io::Error> {
    std::fs::remove_file(in_library(home, file_name)?)
}

/// Uwagi walidatora o tym workflow — **te same**, które padają przy zapisie i przed Startem.
///
/// `impl Borrow<WorkflowFile>` z tego samego powodu, co przy [`save_workflow_inner`].
#[must_use]
pub fn check_workflow_inner(home: &Path, workflow: impl Borrow<WorkflowFile>) -> Vec<Note> {
    let file = workflow.borrow();
    // Przelotka i nic więcej. Drugi walidator reguł o SAMYM PLIKU, dopisany tutaj „bo front
    // potrzebuje jeszcze jednej uwagi", byłby drugim miejscem, w którym mieszka odpowiedź na
    // pytanie „co jest nie tak z tym plikiem", i jedno z dwóch zawsze byłoby nieaktualne
    // (niezmiennik 13).
    let mut notes = crate::workflow::check::check(file);

    /* DRUGA LISTA, o BIBLIOTECE, i dlatego doklejana tutaj, a nie w `check`.
     *
     * 2026-08-22 — powód stoi w całości w nagłówku `workflow::roster`: trzy odmowy pod rząd,
     * wszystkie po naciśnięciu Start, wszystkie policzalne wcześniej. `check` musi zostać czystą
     * funkcją nad plikiem, bo woła ją zapis, a plik ma się zapisać także wtedy, gdy ktoś właśnie
     * skasował agenta. Sklejenie obu list należy więc do komendy okna — jednego miejsca, które
     * ma i plik, i bibliotekę.
     *
     * Biblioteka nie do odczytania NIE ZABIERA uwag o pliku: człowiek dostaje wtedy to, co
     * dało się policzyć, zamiast pustej listy sugerującej, że wszystko jest w porządku. */
    let agents = match crate::commands::agents::list_agents_inner(home) {
        Ok(agents) => agents,
        Err(_error) => {
            notes.push(Note {
                level: crate::workflow::check::Level::Problem,
                step_id: None,
                message: "Loadout could not read your saved agents, so it could not check this \
                          workflow's roles."
                    .to_owned(),
                fix: None,
            });
            Vec::new()
        }
    };
    let connections =
        crate::connections::runtime::all(&home.join("connections")).unwrap_or_default();
    let skills = saved_skill_names(home);
    notes.extend(crate::workflow::roster::check_the_roster(
        file,
        &agents,
        &connections,
        &skills,
    ));
    notes
}

/// Nazwy katalogów w `~/.loadout/skills` — tyle, ile walidator obsady potrzebuje.
///
/// Sama nazwa, bez czytania `SKILL.md`: „czy ta nazwa w ogóle coś znaczy" jest innym pytaniem niż
/// „czy ten plik jest umiejętnością", a na to drugie odpowiada `skills::place::validate_usable`
/// w chwili, w której krok po nią sięga.
fn saved_skill_names(home: &Path) -> Vec<String> {
    let Ok(listing) = std::fs::read_dir(home.join("skills")) else {
        return Vec::new();
    };
    listing
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry.file_type().ok()?.is_dir().then_some(())?;
            entry.file_name().into_string().ok()
        })
        .collect()
}
