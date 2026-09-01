//! Podpowiedzi ścieżek dla `@` w polach Loadouta.
//!
//! # Po co to istnieje
//!
//! Właściciel 2026-09-01: „chce aby jak pisze @ miec opcje wyboru lokacji […] cos jak w claude
//! code taki expirience ze jak daje malpke to ladnie podopwiada nam dany path". Do tego dnia
//! każde miejsce, w którym człowiek wskazuje agentowi folder, było gołym polem tekstowym: literówka
//! w ścieżce nie odbijała się aż do biegu, a katalogu, którego nazwy się nie pamięta, nie dało się
//! znaleźć bez wyjścia do Findera.
//!
//! # Dlaczego to NIE jest zwykłe `read_dir`
//!
//! Napis przychodzi od człowieka i trafia do systemu plików, więc `..` w środku wyprowadza
//! listowanie poza projekt — a wtedy podpowiadamy nazwy plików spoza folderu, który człowiek
//! wybrał. To jest ta sama klasa wady, którą po stronie zapisu sądzi
//! `results_are_written_where_asked`. Korzeń kanonikalizujemy RAZ i sprawdzamy, że wynik nadal
//! w nim siedzi — po rozwiązaniu dowiązań, bo `link -> /etc` nie wygląda z zewnątrz na wyjście.

use std::cmp::Ordering;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// Ile podpowiedzi maksymalnie. Lista dłuższa niż ekran jest listą, której nikt nie czyta,
/// a katalog `node_modules` potrafi mieć ich dziesiątki tysięcy.
pub const MOST: usize = 40;

/// Katalogi, których nie podpowiadamy nigdy. Ta sama lista, którą kopiowanie kroku pomija
/// (`isolate::NOT_COPIED`) — agent i tak nie ma tam czego szukać.
const NEVER: [&str; 4] = [".git", ".loadout", "node_modules", "target"];

/// Jedna podpowiedź: ścieżka względem folderu projektu i to, czy da się w nią wejść.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Zawsze ze `/` na końcu dla katalogu, żeby dopisanie kolejnego członu było jednym znakiem.
    pub path: String,
    /// Czy to katalog. Ekran rysuje z tego ikonę i decyduje, czy wejść, czy wstawić.
    pub folder: bool,
}

/// Czym odmawia listowanie.
#[derive(Debug, thiserror::Error)]
pub enum Trouble {
    /// Wskazany korzeń nie istnieje albo nie jest folderem.
    #[error("{} is not a folder, so nothing can be suggested from it", .0.display())]
    NoSuchRoot(PathBuf),
    /// To, co człowiek wpisał, wyprowadza poza folder projektu.
    #[error("that path leads out of the project folder")]
    OutOfBounds,
}

/// Czy `inside` naprawdę siedzi pod `root` — po rozwiązaniu dowiązań.
fn within(root: &Path, inside: &Path) -> bool {
    inside.starts_with(root)
}

/// Ścieżka bez `..`, żeby `src/../..` nie wyszło poza korzeń zanim dotknie dysku.
fn tidy(typed: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in Path::new(typed).components() {
        match part {
            Component::Normal(name) => out.push(name),
            Component::CurDir => {}
            // `..` i każdy korzeń są odmową, nie milczącym przycięciem: człowiek, który je
            // napisał, ma zobaczyć, że tędy nie wolno, a nie dostać listing czegoś innego.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

/// Podpowiedzi dla tego, co człowiek wpisał po `@`.
///
/// `typed` jest ścieżką WZGLĘDNĄ wobec folderu projektu; jej ostatni człon jest przedrostkiem
/// nazwy, a nie katalogiem — `src/sec` listuje `src/` i zostawia to, co zaczyna się na `sec`.
pub fn suggest(root: &Path, typed: &str, most: usize) -> Result<Vec<Suggestion>, Trouble> {
    let root = root
        .canonicalize()
        .map_err(|_| Trouble::NoSuchRoot(root.to_path_buf()))?;
    if !root.is_dir() {
        return Err(Trouble::NoSuchRoot(root));
    }

    let (folder_part, prefix) = match typed.rsplit_once('/') {
        Some((before, after)) => (before, after),
        None => ("", typed),
    };
    let relative = tidy(folder_part).ok_or(Trouble::OutOfBounds)?;
    let looking_in = root.join(&relative);
    let looking_in = looking_in
        .canonicalize()
        .map_err(|_| Trouble::OutOfBounds)?;
    if !within(&root, &looking_in) {
        return Err(Trouble::OutOfBounds);
    }

    let Ok(entries) = fs::read_dir(&looking_in) else {
        return Ok(Vec::new());
    };

    let lowered = prefix.to_lowercase();
    let mut found: Vec<Suggestion> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            if NEVER.contains(&name.as_str()) {
                return None;
            }
            // Kropka na początku chowa wpis, DOPÓKI człowiek sam jej nie napisze. Inaczej pierwsze
            // naciśnięcie `@` w każdym repo pokazuje `.github` i `.vscode` zamiast kodu.
            if name.starts_with('.') && !lowered.starts_with('.') {
                return None;
            }
            if !name.to_lowercase().starts_with(&lowered) {
                return None;
            }
            let folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
            let shown = if relative.as_os_str().is_empty() {
                name
            } else {
                format!("{}/{name}", relative.to_string_lossy())
            };
            Some(Suggestion {
                path: if folder { format!("{shown}/") } else { shown },
                folder,
            })
        })
        .collect();

    // Katalogi przed plikami, bo `@` służy przede wszystkim do wskazania MIEJSCA; w obrębie
    // rodzaju alfabetycznie, bo kolejność z `read_dir` jest kolejnością systemu plików i zmienia
    // się między maszynami.
    found.sort_by(|left, right| match right.folder.cmp(&left.folder) {
        Ordering::Equal => left.path.to_lowercase().cmp(&right.path.to_lowercase()),
        other => other,
    });
    found.truncate(most);
    Ok(found)
}
