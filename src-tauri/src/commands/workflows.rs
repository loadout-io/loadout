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
use crate::library::definition::{Definition, Shelf, workflow_problem};
use crate::workflow::WorkflowFile;
use crate::workflow::check::Note;
use crate::workflow::file::{LoadError, SaveError};

/// Nazwa katalogu workflow — **jedyna w tym drzewie** od 2026-08-29 (T-164).
///
/// Do tego dnia ten napis stał w trzech plikach (`ipc.rs`, tutaj, `commands/chat.rs`) i każda
/// kopia niosła komentarz o obowiązku przeszukania całego `src-tauri/src` w dniu zmiany §8.
/// Trzy źródła jednej prawdy to trzy okazje do rozjazdu, a rozjazd wygląda tu nie na literówkę,
/// tylko na lidera, który „nie widzi" workflow leżącego na dysku. Pozostałe dwa importują
/// tę stałą (niezmiennik 23).
pub const WORKFLOWS_DIR: &str = "workflows";

/// Półka, z której przyszedł plik workflow.
///
/// # ROZSTRZYGNIĘCIE: DWA KORZENIE, A NIE POLE ZAKRESU W PLIKU (2026-08-29, T-164)
///
/// `~/.loadout/workflows/` zostaje **biblioteką** — bajt w bajt tam, gdzie leżą dziś pliki
/// człowieka. `<folder>/.loadout/workflows/` powstaje jako półka **projektu** i to tam ląduje
/// każdy nowy workflow. Powód, dla którego to katalog rozstrzyga, a nie pole w JSON-ie:
///
/// - Wzorzec z pamięci mówi mniej, niż się wydaje. Notatka trzyma we front-matterze, JAK DALEKO
///   sięga (`everywhere` / `this-project` / `this-agent`), a KTÓRY to projekt rozstrzyga korzeń:
///   `commands::memory::list_note_catalog_inner` bierze dwa korzenie. Katalog odpowiada „czyje",
///   front-matter „jak szeroko". Workflow ma jedną szerokość, więc zostaje sam katalog.
/// - Pole zakresu w pliku globalnym musiałoby nieść **bezwzględną ścieżkę folderu** wewnątrz
///   JSON-a, który człowiek commituje i kopiuje między maszynami. Taki napis psuje się cicho
///   przy pierwszym przeniesieniu projektu.
/// - Niezmiennik 25 i zdanie „stary Loadout ma je znaleźć" zakazują ruszania plików, które już
///   leżą w bibliotece. Dwa korzenie ich nie ruszają: zero migracji, zero przepisywania wierszy.
///   Schematu `SQLite` ta zmiana nie dotyka w ogóle.
/// - Niezmiennik 4 jest spełniony przez konstrukcję: półka wynika z tego, w którym katalogu leży
///   plik, więc `loadout.db` dalej wolno skasować.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowPlace {
    /// `~/.loadout/workflows/` — widzą go **wszystkie** workspace'y.
    Library,
    /// `<folder>/.loadout/workflows/` — widzi go wyłącznie ten jeden projekt.
    Project,
}

/// Rozstrzygnięta ścieżka pliku razem z półką, na której leży.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub place: WorkflowPlace,
    pub path: PathBuf,
}

/// Katalog workflow w bibliotece człowieka: `~/.loadout/workflows/` (`docs/ARCHITECTURE.md` §8).
#[must_use]
pub fn library_workflows(library: &Path) -> PathBuf {
    library.join(WORKFLOWS_DIR)
}

/// Katalog workflow jednego projektu: `<folder>/.loadout/workflows/` (§8 tamże).
#[must_use]
pub fn project_workflows(project: &Path) -> PathBuf {
    project.join(".loadout").join(WORKFLOWS_DIR)
}

/// Jedna pozycja listy: nazwa pliku, półka i to, co w pliku leży.
///
/// Lustro `WorkflowEntry` z `src/sections/workflows/list/store.ts`, pole w pole. `path` to
/// **sama nazwa pliku**, nigdy pełna ścieżka: katalog rozwiązuje ta warstwa, a front, który
/// doklejałby go sam, byłby drugim miejscem, w którym mieszka odpowiedź na pytanie „gdzie to
/// leży" [T3 §8.3]. `place` jedzie obok, bo nazwa pliku sama nie mówi, ile projektów zniknie
/// przy Delete.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkflowEntry {
    /// `ship-a-feature.json` — bez katalogu i bez `~`.
    pub path: String,
    /// Z której półki ten plik przyszedł.
    pub place: WorkflowPlace,
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

/// Gdzie leży — albo gdzie ma powstać — plik o tej nazwie. **Jedyna reguła rozwiązania nazwy.**
///
/// Trzy zdania i ani jednego więcej:
///
/// 1. plik projektu wygrywa z plikiem biblioteki o tej samej nazwie — otwarty projekt jest
///    bliżej człowieka niż wspólna półka;
/// 2. plik, którego w projekcie nie ma, a jest w bibliotece, przyjeżdża z biblioteki — i to
///    jest zdanie, na którym stoi „workflow zapisany przed tą zmianą nadal się otwiera";
/// 3. nazwa nieznana nigdzie jest **nowym plikiem projektu** (a biblioteki, kiedy żaden projekt
///    nie jest otwarty).
///
/// Dzięki temu `file_name` zostaje JEDNYM kluczem: ani trigger, ani powtórzenie kroku, ani
/// historia nie potrzebują adresu — o półce wie wyłącznie lista i ekran.
///
/// Nazwa przychodzi z okna, więc jest wejściem, któremu nie ufamy (T3 §5.2). `Path::join`
/// z `../../.ssh/config` wychodzi poza bibliotekę bez jednego ostrzeżenia, a `join("/etc/x")`
/// **odrzuca cały prefiks** i zwraca `/etc/x` — czyli komenda „zapisz workflow" pisałaby
/// dokładnie tam, gdzie każe jej webview. Zapora jest tutaj, bo to jedyna warstwa, która wie,
/// że ten napis ma być nazwą, a nie ścieżką — i od 2026-08-29 jest jej JEDYNA kopia w drzewie
/// (druga stała w `ipc::run_request` z komentarzem czekającym na zadanie posiadające oba pliki).
pub fn where_it_lives(
    library: &Path,
    project: Option<&Path>,
    file_name: &str,
) -> Result<Placed, io::Error> {
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

    let mine = project.map(|project| project_workflows(project).join(file_name));
    if let Some(path) = mine.clone().filter(|path| path.is_file()) {
        return Ok(Placed {
            place: WorkflowPlace::Project,
            path,
        });
    }
    let shared = library_workflows(library).join(file_name);
    if shared.is_file() {
        return Ok(Placed {
            place: WorkflowPlace::Library,
            path: shared,
        });
    }
    // Nowy plik ląduje w projekcie, kiedy jakiś jest otwarty. Bez otwartego projektu zostaje
    // biblioteka — i to jest jedyny stan, w którym Loadout dalej pisze tam, gdzie pisał zawsze.
    Ok(match mine {
        Some(path) => Placed {
            place: WorkflowPlace::Project,
            path,
        },
        None => Placed {
            place: WorkflowPlace::Library,
            path: shared,
        },
    })
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
    project: Option<&Path>,
    file_name: &str,
    workflow: impl Borrow<WorkflowFile>,
    expected: Option<&str>,
) -> Result<Saved, SaveError> {
    // WRACA TAM, SKĄD PRZYSZEDŁ. Zapis pliku bibliotecznego, który forkowałby go do projektu,
    // zostawiałby człowieka z dwiema kopiami jednego workflow i bez ani jednego zdania o tym
    // (2026-08-29, T-164) — a jedna z nich byłaby od tej chwili niewidoczna dla sąsiedniego
    // projektu, choć człowiek właśnie ją poprawiał.
    let path = where_it_lives(home, project, file_name)
        .map_err(SaveError::Unwritable)?
        .path;
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

/// Wczytuje workflow spod `file_name` — z projektu, a kiedy go tam nie ma, z biblioteki.
pub fn load_workflow_inner(
    home: &Path,
    project: Option<&Path>,
    file_name: &str,
) -> Result<OpenWorkflow, LoadError> {
    let path = where_it_lives(home, project, file_name)
        .map_err(LoadError::Unreadable)?
        .path;
    // Katalog bierzemy Z ROZSTRZYGNIĘTEJ ŚCIEŻKI, a nie składamy drugi raz: publikator ma
    // odzyskiwać dokładnie ten katalog, z którego zaraz czytamy bajty.
    let dir = path
        .parent()
        .map_or_else(|| home.to_path_buf(), Path::to_path_buf);
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

/// Katalog obu półek: najpierw projekt, potem to, czego projekt nie przesłonił.
///
/// Ten sam kształt, co `commands::memory::list_note_catalog_inner`: jeden czytnik, dwa korzenie,
/// półka dopięta dopiero w adapterze katalogu. Nazwa zajęta przez projekt **nie wchodzi**
/// z biblioteki drugi raz — inaczej lista pokazywałaby dwa wiersze nad jednym kluczem, a Start
/// uruchamiałby ten, który wybrało `where_it_lives`, czyli nie zawsze ten, w który kliknięto.
///
/// Błąd jednego pliku jest jednym wpisem problemu; błąd całego kontrolowanego katalogu nadal
/// odmawia operacji.
pub fn list_workflow_definitions_inner(
    home: &Path,
    project: Option<&Path>,
) -> Result<Vec<Definition<WorkflowEntry>>, LoadError> {
    let mut catalog = match project {
        Some(project) => shelf(&project_workflows(project), WorkflowPlace::Project)?,
        None => Vec::new(),
    };
    let taken: std::collections::BTreeSet<String> = catalog.iter().map(named).collect();
    catalog.extend(
        shelf(&library_workflows(home), WorkflowPlace::Library)?
            .into_iter()
            // APFS jest domyślnie NIEwrażliwy na wielkość liter, więc `Ship.JSON` i `ship.json`
            // są tam jednym plikiem i muszą być jednym wierszem także tutaj.
            .filter(|one| !taken.contains(&named(one).to_lowercase())),
    );
    Ok(catalog)
}

/// Nazwa workflow **do wpisania** — ta, którą człowiek pisze po `/run`.
///
/// # Jeden fakt, dwie strony granicy
///
/// Ta sama reguła żyje po stronie okna (`src/sections/run/run-command.ts`, bo wiersz wejścia musi
/// znormalizować to, co człowiek NAPISAŁ, zanim cokolwiek pojedzie na drut) i tutaj (bo czasownik
/// `list_workflows` oddaje liderowi nazwy, którymi ma się posłużyć). Dwóch implementacji nie da
/// się uniknąć — jedna strona potrzebuje funkcji, nie wartości.
///
/// Da się natomiast uniemożliwić ich cichy rozjazd, i to robi wspólna wyrocznia:
/// `docs/patterns/fixtures/typable-names.json` czytają kryteria po OBU stronach. Bez niej
/// rozjazd wygląda tak: lider proponuje nazwę, Enter jej nie zna, a człowiek widzi workflow,
/// „którego nie ma".
///
/// Rozszerzenie `.json` odpada niezależnie od wielkości liter, bo `ship-a-feature.json`,
/// `Ship a feature` i `ship-a-feature` mają prowadzić do JEDNEGO workflow, a nie do trzech
/// odpowiedzi na jedno pytanie.
#[must_use]
pub fn typable(name: &str) -> String {
    let lowered = name.to_lowercase();
    let stem = lowered.strip_suffix(".json").unwrap_or(&lowered);

    let mut out = String::with_capacity(stem.len());
    let mut pending_break = false;
    for letter in stem.chars() {
        if letter.is_ascii_alphanumeric() {
            /* Łącznik dopisujemy dopiero PRZED następną literą, nie po poprzedniej. Dzięki temu
             * ogon nigdy nie zostaje z łącznikiem i nie trzeba go potem obcinać — a obcinanie
             * po fakcie jest tym krokiem, który w drugiej implementacji zwykle wypada. */
            if pending_break && !out.is_empty() {
                out.push('-');
            }
            pending_break = false;
            out.push(letter);
        } else {
            pending_break = true;
        }
    }
    out
}

/// Nazwa pliku tej pozycji, małymi literami — klucz przesłaniania.
fn named(definition: &Definition<WorkflowEntry>) -> String {
    match definition {
        Definition::Healthy { value, .. } => value.path.to_lowercase(),
        Definition::DefinitionProblem { file_name, .. } => file_name.to_lowercase(),
    }
}

/// Jeden korzeń, cały jego katalog.
fn shelf(dir: &Path, place: WorkflowPlace) -> Result<Vec<Definition<WorkflowEntry>>, LoadError> {
    let publisher = DurableFilePublisher::new(dir);
    let mut listed = None;
    match publisher.recover_with(|root| {
        listed = Some(list_workflow_definitions_from_root(root, dir, place));
        Ok(())
    }) {
        Ok(()) => listed.ok_or_else(|| {
            LoadError::Unreadable(io::Error::other(
                "the recovered workflow library was not listed",
            ))
        })?,
        // Półka bez katalogu pozostaje legalnie pusta — a od 2026-08-29 jest to stan CODZIENNY,
        // nie brzegowy: projekt, w którym nikt jeszcze nie zapisał workflow, nie ma tego katalogu
        // wcale. Recovery rozróżnia ten przypadek od symlinka lub pliku podstawionego pod nazwę
        // kontrolowanego katalogu.
        Err(PublishError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(LoadError::Unreadable(error.into_io())),
    }
}

fn list_workflow_definitions_from_root(
    root: &PublicationRoot,
    dir: &Path,
    place: WorkflowPlace,
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
                        place,
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

/// Usuwa plik workflow z tej półki, z której go widać.
pub fn delete_workflow_inner(
    home: &Path,
    project: Option<&Path>,
    file_name: &str,
) -> Result<(), io::Error> {
    std::fs::remove_file(where_it_lives(home, project, file_name)?.path)
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
