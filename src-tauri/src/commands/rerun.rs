//! Ponowne odpalenie **jednego kroku** skończonego biegu.
//!
//! # Po co to istnieje
//!
//! 2026-08-23, prośba właściciela po biegu, który trwał 48 minut i padł na ostatnim sprawdzeniu
//! z powodu środowiskowego („no dev server is reachable"). Do tego dnia jedynym sposobem
//! poprawienia jednego kroku było puszczenie całej dziesiątki od zera — z Planem, Researchem
//! i implementacją, które przeszły bez zarzutu i nie miały czego powtarzać.
//!
//! # Trzy rozstrzygnięcia, każde z powodem
//!
//! **To jest NOWY BIEG, nie dopisanie do starego.** `run.json` skończonego biegu jest historią
//! i nie ma prawa się zmienić dlatego, że ktoś powtórzył kafelek (niezmiennik 4: pliki są
//! prawdą). Powtórzenie zostawia więc własny katalog, własny `run.json` i własne przekazanie —
//! a stary bieg wygląda za tydzień dokładnie tak, jak wyglądał.
//!
//! **Wejściem są przekazania poprzedniego biegu.** Krok powtórzony sam jeden nie ma po czym iść,
//! a jego prompt składa się z instrukcji i indeksu przekazań poprzedników. Bez nich dostałby to
//! samo zadanie z pustym kontekstem i pracował od zera nad czymś, co reszta grafu już zrobiła.
//!
//! # Dwa czasowniki, nie jeden
//!
//! 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
//! [`again`] powtarza JEDEN kafelek ostatniego biegu tego workflow; [`onward`] wznawia
//! WSKAZANY bieg od wskazanego kroku i puszcza wszystko, co graf stawia po nim. Różnica jest
//! z życia, nie z symetrii: bieg, który padł na siódmym kroku z dziesięciu, ma sześć kroków
//! skończonych, których nikt nie chce powtarzać, i trzy, które nigdy nie ruszyły.
//!
//! [`again`] tego nie wyraża i wyrazić nie może — ona ZDEJMUJE strzałki, bo powtarzany kafelek
//! nie ma po czym iść (`commands::Part::Just`). Powód, dla którego strzałki wracają w drugim
//! czasowniku, stoi przy `commands::Part::Onward`.
//!
//! **Bierzemy DZISIEJSZY plik workflow, nie migawkę z `run.json`** — i mówimy o tym wprost, gdy
//! te dwa się różnią. Powód jest z życia: krok powtarza się zwykle po to, żeby zadziałała
//! poprawka, którą człowiek właśnie zrobił w agencie albo w kroku. Migawka dawałaby wierne
//! powtórzenie tamtej porażki, czyli dokładnie to, czego nikt nie chce. Kiedy pliku już nie ma,
//! odmawiamy zamiast zgadywać: bieg z wymyślonego grafu jest gorszy niż zdanie „nie ma z czego".

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::workflow::WorkflowFile;

use super::{Part, RunRequest};

/// `run.json` w tylu polach, ile potrzebuje powtórzenie — reszta pliku jest tu nieciekawa.
///
/// Własny, wąski kształt zamiast pełnego lustra: `run.json` rośnie z każdym zadaniem, a ta
/// funkcja pyta o dwie rzeczy i nie ma powodu przewracać się na trzeciej, której nie zna
/// (niezmiennik 5).
#[derive(Debug, Deserialize)]
struct Finished {
    /// Identyfikator workflow — po nim szukamy dzisiejszego pliku.
    workflow_id: String,
    /// Graf, jak biegł. Czytany wyłącznie po to, żeby powiedzieć, czy dziś jest inny.
    workflow_snapshot: WorkflowFile,
}

/// Czym powtórzenie umie odmówić. Każdy wariant naprawia się inaczej, więc każdy jest osobnym
/// zdaniem — ta sama reguła, co przy [`super::run::RunError`].
#[derive(Debug, thiserror::Error)]
pub enum Trouble {
    #[error("Loadout could not read that run: {0}")]
    Unreadable(String),
    #[error(
        "The workflow this run came from is no longer in your library, so there is nothing to \
         run this step from. Save it again in Workflows."
    )]
    NoWorkflow,
    #[error(
        "\"{0}\" is not a step in that workflow any more. Open the workflow and pick a step that \
         is still there."
    )]
    NoSuchStep(String),
    #[error("This workflow has not run in this workspace yet, so there is no step to run again.")]
    NoRun,
    #[error(
        "Loadout could not find that run in this workspace any more. Open the list again to see \
         what is there."
    )]
    NoSuchRun,
}

/// Co powtórzenie ma uruchomić — i czy plik zdążył się od tamtego biegu zmienić.
#[derive(Debug)]
pub struct Again {
    /// Żądanie gotowe dla [`super::run`].
    pub request: RunRequest,
    /// Zdanie dla człowieka, kiedy dzisiejszy plik różni się od tego, który wtedy biegł.
    /// `None`, kiedy graf jest ten sam co do pola.
    pub said: Option<String>,
}

/// Składa żądanie, które powtórzy jeden krok skończonego biegu.
///
/// `run_dir` to katalog tamtego biegu, `step` — klucz kafelka (`id` kroku z pliku workflow).
pub fn again(
    home: &Path,
    project: &Path,
    file_name: &str,
    step: &str,
    how_many_at_once: usize,
) -> Result<Again, Trouble> {
    let path = home.join("workflows").join(file_name);
    let today: WorkflowFile = fs::read(&path)
        .map_err(|error| Trouble::Unreadable(error.to_string()))
        .and_then(|bytes| {
            serde_json::from_slice(&bytes).map_err(|error| Trouble::Unreadable(error.to_string()))
        })
        .map_err(|_| Trouble::NoWorkflow)?;

    /* KATALOG BIEGU ZNAJDUJEMY TUTAJ, a nie w oknie, i to jest wybór na rzecz uczciwości okna:
     * katalog biegu powstaje w środku planowania, więc okno nigdy go nie poznaje. Prosząc je
     * o ścieżkę, prosilibyśmy o rzecz, której nie ma — a jedyne, co człowiek naprawdę wskazał,
     * to kafelek w ostatnim biegu tego workflow. */
    let Some(run_dir) = newest_run_of(project, &today.id) else {
        return Err(Trouble::NoRun);
    };
    let bytes = fs::read(run_dir.join("run.json"))
        .map_err(|error| Trouble::Unreadable(error.to_string()))?;
    let finished: Finished =
        serde_json::from_slice(&bytes).map_err(|error| Trouble::Unreadable(error.to_string()))?;
    if !today.steps.iter().any(|one| one.id() == step) {
        return Err(Trouble::NoSuchStep(step.to_owned()));
    }

    // RÓŻNICĘ MÓWIMY, NIE UKRYWAMY. Powtórzenie na zmienionym pliku jest tym, o co człowiek
    // zwykle prosi — ale „to samo jeszcze raz" i „to samo z twoją poprawką" są dwiema różnymi
    // rzeczami i nie mogą wyglądać identycznie (niezmiennik 29).
    let said = (today != finished.workflow_snapshot).then(|| {
        format!(
            "\"{}\" runs again from the workflow as it is now, and that is not the same file the \
             first run used.",
            today
                .steps
                .iter()
                .find(|one| one.id() == step)
                .map_or(step, |one| one.name()),
        )
    });

    Ok(Again {
        request: RunRequest {
            workflow: path,
            how_many_at_once,
            // Zadanie biegu przychodzi z przekazań, nie stąd: powtarzamy krok, a nie polecenie.
            task: None,
            part: Some(Part::Just(vec![step.to_owned()])),
            handoffs_from: Some(run_dir),
        },
        said,
    })
}

/// Najnowszy katalog biegu tego workflow w tym projekcie.
///
/// Nazwy katalogów zaczynają się od znacznika czasu (`<ts>__<id>`), więc **sortowanie bajtowe
/// jest sortowaniem chronologicznym** — i to jest jedyny powód, dla którego ten katalog nazywa
/// się tak, jak się nazywa (`commands::run::run_directory`). Czytamy `run.json`, bo nazwa
/// katalogu nie mówi, z którego workflow bieg pochodzi.
fn newest_run_of(project: &Path, workflow_id: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(project.join(".loadout/runs"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs.into_iter().rev().find(|dir| {
        fs::read(dir.join("run.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Finished>(&bytes).ok())
            .is_some_and(|one| one.workflow_id == workflow_id)
    })
}

/// Wznawia wskazany bieg od wskazanego kroku: on i **wszystko, co graf stawia po nim**.
///
/// `run` to nazwa katalogu biegu — dokładnie to, czym historia nazywa wiersz na ekranie
/// (`commands::history::RunWire::folder`). Nazwa, nie ścieżka: okno nie ma prawa podać ścieżki
/// spoza `.loadout/runs`, bo bieg pisze wyłącznie do swojego katalogu (`ARCHITECTURE` §8),
/// a katalog wzięty z zewnątrz byłby drogą do czytania cudzych przekazań.
///
/// **Plik workflow znajdujemy PO BIEGU, nie po nazwie z okna.** Wiersz historii mówi, co
/// biegło; nie mówi, w którym pliku ten graf dziś leży, bo plik można było przemianować.
/// Idziemy więc przez `workflow_id` z `run.json` do dzisiejszej biblioteki — a kiedy tego
/// workflow już w niej nie ma, odmawiamy zamiast zgadywać.
pub fn onward(
    home: &Path,
    project: &Path,
    run: &str,
    step: &str,
    how_many_at_once: usize,
) -> Result<Again, Trouble> {
    let run_dir = one_run_named(project, run).ok_or(Trouble::NoSuchRun)?;
    let bytes = fs::read(run_dir.join("run.json"))
        .map_err(|error| Trouble::Unreadable(error.to_string()))?;
    let finished: Finished =
        serde_json::from_slice(&bytes).map_err(|error| Trouble::Unreadable(error.to_string()))?;
    let (path, today) = in_the_library(home, &finished.workflow_id).ok_or(Trouble::NoWorkflow)?;
    let Some(named) = today.steps.iter().find(|one| one.id() == step) else {
        return Err(Trouble::NoSuchStep(step.to_owned()));
    };

    // RÓŻNICĘ MÓWIMY, NIE UKRYWAMY — ten sam powód, co przy [`again`], i o tyle mocniejszy, że
    // wznowienie dotyczy WIĘKSZEJ części grafu: człowiek ma wiedzieć, że ruszy dzisiejszy plik.
    let said = (today != finished.workflow_snapshot).then(|| {
        format!(
            "This picks up from \"{}\" using the workflow as it is now, and that is not the same \
             file the first run used.",
            named.name(),
        )
    });

    Ok(Again {
        request: RunRequest {
            workflow: path,
            how_many_at_once,
            // Zadanie przychodzi z przekazań poprzedniego biegu, nie stąd: wznawiamy pracę,
            // a nie zaczynamy nowej.
            task: None,
            part: Some(Part::Onward(step.to_owned())),
            handoffs_from: Some(run_dir),
        },
        said,
    })
}

/// Katalog biegu o tej nazwie — **tylko** wtedy, gdy leży tam, gdzie leżą biegi.
///
/// `file_name()`, nie sklejenie ścieżek: `..` w nazwie przysłanej z okna wyprowadziłoby ten
/// odczyt poza projekt.
fn one_run_named(project: &Path, run: &str) -> Option<PathBuf> {
    if Path::new(run).file_name().is_none_or(|one| one != run) {
        return None;
    }
    let dir = project.join(".loadout/runs").join(run);
    dir.is_dir().then_some(dir)
}

/// Dzisiejszy plik tego workflow w bibliotece — ścieżka i treść.
///
/// Po identyfikatorze, nie po nazwie pliku: nazwa jest sluggiem tytułu i zmienia się razem
/// z nim, a identyfikator jest tym, czym bieg zapamiętał, skąd przyszedł.
fn in_the_library(home: &Path, workflow_id: &str) -> Option<(PathBuf, WorkflowFile)> {
    let mut names: Vec<PathBuf> = fs::read_dir(home.join("workflows"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|one| one == "json"))
        .collect();
    // Porządek jest ustalony, żeby dwa pliki o jednym identyfikatorze (plik i jego kopia obok)
    // dawały ZA KAŻDYM RAZEM ten sam wynik — `read_dir` nie obiecuje kolejności.
    names.sort();
    names.into_iter().find_map(|path| {
        let file: WorkflowFile = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())?;
        (file.id == workflow_id).then_some((path, file))
    })
}
