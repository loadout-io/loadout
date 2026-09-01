//! Odczyt, zapis i lista zestawów — z odmową-w-przód i z odmową ZANIM dysk zostanie tknięty.
//!
//! Trzy własności przeniesione wprost z `workflow::file`, bo plik zestawu jest tym samym
//! rodzajem rzeczy co plik workflow: człowiek może go zmergować gitem, poprawić ręcznie
//! i otworzyć raz nowszym buildem, raz starszym.
//!
//! - **Odmowa zamiast zgadywania w przód.** Plik z `format` większym niż [`super::CURRENT`]
//!   nie jest wczytywany ani dotykany.
//! - **Sprawdź, dopiero potem pisz.** Implementacja, która zapisuje i sprawdza po zapisie,
//!   niszczy poprzednią wersję dokładnie w tym momencie, w którym sprawdzenie miało jej bronić.
//! - **Zapis wobec rewizji, którą wołający przeczytał.** Bez tego dwie karty otwarte na jednym
//!   zestawie po cichu kasują sobie nawzajem pracę.
//!
//! Czego tu **nie ma**: migracji. Jedna wersja, dopóki nie ma drugiej (niezmiennik 25).
//! Tablica migracji „na przyszłość" jest w tym repo zakazana, a pusta tablica z pętlą, która
//! nigdy się nie kręci, jest kłamstwem o tym, ile wersji tego pliku naprawdę było.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::durable_file::{
    DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PublishError, revision_of,
};

use super::{CURRENT, Case, CaseStatus, EvalSet, project_evals};

/// Dlaczego zestawu nie da się wczytać. Każdy wariant naprawia się inaczej, więc każdy ma
/// własne zdanie — jedno wspólne „nie udało się" zostawia człowieka z otwarciem pliku
/// w edytorze jako jedyną drogą dalej.
#[derive(Debug)]
pub enum LoadError {
    /// `format` większy niż [`super::CURRENT`]. Plik zostaje nietknięty.
    TooNew,
    /// Brak klucza `format`. Osobno od [`LoadError::Malformed`], bo plik bez wersji równie
    /// dobrze może być czymś, co zestawem nie jest.
    NoFormat,
    /// `format` mniejszy niż bieżący, a nie ma czym go podnieść.
    TooOld,
    /// Pliku nie dało się przeczytać.
    Unreadable(io::Error),
    /// Bajty są, ale to nie jest ten format.
    Malformed(serde_json::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooNew => formatter
                .write_str("This set was saved by a newer Loadout. Update Loadout to open it."),
            Self::TooOld => formatter.write_str(
                "This set is from a Loadout too old for this one to open. Open it with the \
                 version that wrote it.",
            ),
            Self::NoFormat => formatter.write_str(
                "This file does not say which Loadout wrote it, so it was \
                     left alone.",
            ),
            Self::Unreadable(error) => write!(formatter, "This set could not be read: {error}."),
            Self::Malformed(error) => {
                write!(formatter, "This set could not be understood: {error}.")
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// Dlaczego zestawu nie da się zapisać.
#[derive(Debug)]
pub enum SaveError {
    /// Coś w zestawie sprawia, że zapis byłby zapisem czegoś, czego nie da się uruchomić.
    /// Niesie gotowe zdanie dla człowieka.
    Refused(String),
    /// Plik zmienił się na dysku po tym, jak wołający go przeczytał.
    Changed,
    /// Dysk odmówił.
    Unwritable(io::Error),
    /// Struktury nie dało się zamienić w tekst.
    Malformed(serde_json::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(said) => formatter.write_str(said),
            // Trzy fakty naraz, bo człowiek potrzebuje wszystkich trzech: że jego zmiana NIE
            // weszła, dlaczego, i że nic cudzego nie zostało zniszczone.
            Self::Changed => formatter.write_str(
                "This set was not saved: it changed on disk after you opened it, so nothing was \
                 overwritten. Open it again to see the newer one.",
            ),
            Self::Unwritable(error) => write!(formatter, "This set could not be saved: {error}."),
            Self::Malformed(error) => {
                write!(
                    formatter,
                    "This set could not be turned into a file: {error}."
                )
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// Ścieżka pliku zestawu w tym projekcie.
#[must_use]
pub fn path_for(project: &Path, id: &str) -> PathBuf {
    project_evals(project).join(format!("{id}.json"))
}

/// Wczytuje zestaw: `format` czytany z surowego dokumentu **przed** deserializacją reszty.
pub fn load(path: &Path) -> Result<EvalSet, LoadError> {
    let text = fs::read_to_string(path).map_err(LoadError::Unreadable)?;
    load_text(&text)
}

/// Ta sama droga dla bajtów, które wołający już ma.
pub fn load_text(text: &str) -> Result<EvalSet, LoadError> {
    let document: Value = serde_json::from_str(text).map_err(LoadError::Malformed)?;
    let format = document
        .get("format")
        .and_then(Value::as_u64)
        .ok_or(LoadError::NoFormat)?;
    if format > u64::from(CURRENT) {
        return Err(LoadError::TooNew);
    }
    if format < u64::from(CURRENT) {
        return Err(LoadError::TooOld);
    }
    serde_json::from_value(document).map_err(LoadError::Malformed)
}

/// Wszystkie zestawy tego projektu, w kolejności nazw plików.
///
/// **Jeden nieczytelny plik to jedna brakująca pozycja, nigdy pusta lista** (niezmiennik 5):
/// `?` na pojedynczym pliku zamieniłby jedną ręczną poprawkę w zniknięcie całej sekcji.
/// Kolejność bierze się z posortowanych nazw, bo `read_dir` nie obiecuje żadnej i na innym
/// systemie plików oddałby inną — a lista, która przestawia się sama, jest listą, w której
/// człowiek szuka od nowa za każdym razem.
#[must_use]
pub fn list(project: &Path) -> Vec<EvalSet> {
    let root = project_evals(project);
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .collect();
    paths.sort();
    paths.iter().filter_map(|path| load(path).ok()).collect()
}

/// Zapisuje zestaw — **jeżeli** [`why_it_would_not_hold`] nie ma nic do powiedzenia.
///
/// `expected` jest rewizją, którą wołający przeczytał; `None` znaczy „tego pliku ma jeszcze
/// nie być". Zwrócona rewizja opisuje bajty, które właśnie wylądowały.
pub fn save(set: &EvalSet, path: &Path, expected: Option<&str>) -> Result<String, SaveError> {
    if let Some(refusal) = why_it_would_not_hold(set) {
        return Err(SaveError::Refused(refusal));
    }

    let mut text = serde_json::to_string_pretty(set).map_err(SaveError::Malformed)?;
    // Znak nowej linii na końcu: bez niego każda zmiana ostatniego wiersza niesie w zmianach
    // dodatkowe „\ No newline at end of file", a plik przestaje być zwykłym plikiem tekstowym.
    text.push('\n');

    let root = path.parent().ok_or_else(|| {
        SaveError::Unwritable(io::Error::new(
            io::ErrorKind::InvalidInput,
            "an eval set path has no controlled parent",
        ))
    })?;
    // Katalog powstaje tutaj, a nie u wołającego: publikacja wymaga korzenia, który istnieje,
    // a wołający, który musiałby o tym pamiętać, jest wołającym, który kiedyś zapomni.
    fs::create_dir_all(root).map_err(SaveError::Unwritable)?;
    DurableFilePublisher::new(root)
        .publish_definition(
            path,
            text.as_bytes(),
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
            expected,
        )
        .map_err(|error| match error {
            // Spóźniony zapis ma WŁASNE zdanie: to jedyny wariant, po którym człowiek ma coś
            // do zrobienia.
            PublishError::Changed { .. } | PublishError::Conflict { .. } => SaveError::Changed,
            other => SaveError::Unwritable(other.into_io()),
        })?;
    Ok(revision_of(text.as_bytes()))
}

/// Co w tym zestawie sprawia, że zapisu nie wolno przyjąć — albo `None`.
///
/// # Dlaczego odmowa pada TUTAJ, a nie przed uruchomieniem
///
/// Bo to są rzeczy, których żaden bieg nie naprawi, a każda z nich gubi wynik **po cichu**.
/// Dwa przypadki o jednym identyfikatorze dają dwa kroki o jednym kluczu w planie, więc wynik
/// jednego z nich znika bez śladu; przypadek `in-use`, którego nie ma czym osądzić, przechodzi
/// zawsze i podnosi wynik zestawu, nie mierząc niczego. Niezmiennik 12 mówi wprost, kiedy taka
/// odmowa ma padać: najpóźniej przy zapisie, nigdy w trakcie biegu.
///
/// Kandydatki (`suggested`) **nie są sądzone** poza identyfikatorem: propozycja bez komendy
/// jest normalnym stanem rzeczy, którą człowiek dopiero przeczyta i uzupełni. Odmowa zapisu
/// kandydatki kasowałaby całą turę, która ją wypracowała.
#[must_use]
pub fn why_it_would_not_hold(set: &EvalSet) -> Option<String> {
    if set.id.trim().is_empty() {
        return Some("This set has no name to save it under.".to_owned());
    }

    let mut names: BTreeSet<String> = BTreeSet::new();
    for case in &set.cases {
        // NAZWA JEST KLUCZEM ZŁĄCZENIA, nie tylko podpisem w tabeli. Przekazanie zna krok,
        // który je zostawił, wyłącznie po nazwie (`memory::handoff::Meta::from`), a nazwa kroku
        // składa się z nazwy przypadku i nazwy kolumny (`plan::work_name`). Dwa przypadki
        // o jednym podpisie dałyby dwa kroki o jednej nazwie i odpowiedź jednego z nich
        // czytałaby się jako odpowiedź drugiego — czyli zielony wiersz nad cudzą pracą.
        if !names.insert(case.name.trim().to_lowercase()) {
            return Some(format!(
                "Two cases here are both named \"{}\". Loadout tells their answers apart by that \
                 name, so one would be read as the other.",
                case.name.trim()
            ));
        }
    }

    let mut headings: BTreeSet<String> = BTreeSet::new();
    for variant in &set.variants {
        if !headings.insert(variant.name.trim().to_lowercase()) {
            return Some(format!(
                "Two columns here are both named \"{}\". Loadout tells their answers apart by \
                 that name, so one would be read as the other.",
                variant.name.trim()
            ));
        }
    }

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for case in &set.cases {
        if case.id.trim().is_empty() {
            return Some(format!(
                "The case named \"{}\" has no id, so its results would have nowhere to land.",
                case.name.trim()
            ));
        }
        if let Some(said) = why_this_id_would_be_ambiguous(&case.id, "case") {
            return Some(said);
        }
        if !seen.insert(case.id.as_str()) {
            return Some(format!(
                "Two cases here are both called \"{}\". One of them would lose its results \
                 without a word.",
                case.id.trim()
            ));
        }
    }

    let mut columns: BTreeSet<&str> = BTreeSet::new();
    for variant in &set.variants {
        if variant.id.trim().is_empty() {
            return Some(format!(
                "The column named \"{}\" has no id, so its results would have nowhere to land.",
                variant.name.trim()
            ));
        }
        if let Some(said) = why_this_id_would_be_ambiguous(&variant.id, "column") {
            return Some(said);
        }
        if !columns.insert(variant.id.as_str()) {
            return Some(format!(
                "Two columns here are both called \"{}\". One of them would lose its results \
                 without a word.",
                variant.id.trim()
            ));
        }
        if variant.agent.trim().is_empty() {
            return Some(format!(
                "The column \"{}\" does not say which agent does the work.",
                variant.name.trim()
            ));
        }
    }

    set.cases
        .iter()
        .filter(|case| case.status == CaseStatus::InUse)
        .find_map(why_this_case_cannot_judge)
}

/// Zdanie o identyfikatorze, którego nie da się jednoznacznie odczytać z powrotem.
///
/// Identyfikator przypadku i wariantu wchodzą do identyfikatora kroku w planie, rozdzielone
/// `plan::APART`. Identyfikator, który sam ten rozdzielacz zawiera, rozbiera się na cztery
/// człony zamiast trzech — a `plan::cell_of` oddaje wtedy `None` i wynik komórki znika
/// z tabeli bez jednego zdania. Odmowa przy zapisie jest jedynym momentem, w którym da się
/// to powiedzieć człowiekowi, zanim zapłaci za bieg.
fn why_this_id_would_be_ambiguous(id: &str, what: &str) -> Option<String> {
    if !id.contains(super::plan::APART) {
        return None;
    }
    Some(format!(
        "The {what} id \"{id}\" has two underscores in a row, and Loadout uses those to tell \
         rows from columns in a run. Rename it and its results will land in the right place."
    ))
}

/// Zdanie o przypadku, który jest w użyciu i którego nie ma czym osądzić.
///
/// Osobno od pętli wyżej, bo mówi o czymś innym: tamta broni przed utratą wyniku, ta przed
/// wynikiem, który nic nie znaczy. Przypadek bez komendy i bez pól przechodzi **zawsze** — a
/// zielony wiersz nad niczym jest dokładnie tą wadą, dla której to repo powstało.
fn why_this_case_cannot_judge(case: &Case) -> Option<String> {
    if case.has_something_to_judge_it() {
        // Komenda bez wzorca spadłaby na sam kod wyjścia, a suita, która nie uruchomiła ani
        // jednego testu, kończy się zerem (niezmiennik 19). Wzorzec bez komendy jest za to
        // nieszkodliwy: nie ma czego dopasować, więc nic się nie dzieje.
        if !case.command.trim().is_empty() && case.proof.trim().is_empty() {
            return Some(format!(
                "The case \"{}\" runs a command but does not say what proves it worked, so a \
                 command that ran nothing would still look green.",
                case.name.trim()
            ));
        }
        return None;
    }
    Some(format!(
        "The case \"{}\" has nothing that could judge it: no command and no field to hand back. \
         It would pass every time without measuring anything.",
        case.name.trim()
    ))
}
