//! P-01: zależność od SĄSIEDNIEGO repozytorium, widziana zanim ruszy pierwszy proces.
//!
//! # Po co to istnieje
//!
//! Zmierzone 2026-09-06 (incydent I-08, bieg `20260906-165702`): krok pracował we własnej kopii
//! projektu, a `Cargo.toml` tego projektu zależał od `../murmur-server`. Własna kopia leży
//! w `.loadout/runs/<bieg>/work/<krok>`, więc `../murmur-server` wskazuje tam katalog, którego
//! nie ma i nie będzie. Agent dostał błąd Cargo, którego treść nie mówi ani słowa o kopii,
//! i przez kilka tur ratował środowisko ręcznie zamiast robić swoją robotę.
//!
//! # Czego to świadomie NIE robi
//!
//! Nie wciąga sąsiada do kopii. Dołączenie cudzego prywatnego katalogu jest decyzją człowieka
//! o zakresie odczytu, a `additionalInputs` — jedyne zatwierdzone wejście wspólnego obrazu —
//! z premedytacją odmawia ścieżek z `..` (`workflow::validate_additional_inputs`). Ten moduł
//! nazywa brak i drogi wyjścia; nie tworzy symlinków i nie kopiuje niczego spoza projektu.

use std::fs;
use std::path::Path;

/// Jedna zależność wskazująca poza folder projektu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbour {
    /// Manifest, w którym to stoi — ścieżka względem folderu projektu.
    pub manifest: String,
    /// Ścieżka, dokładnie jak zapisał ją człowiek.
    pub path: String,
}

/// Ile plików manifestu przeglądamy, zanim przestaniemy. Sufit jest tu po to, żeby skan przed
/// startem nie zamienił się w chodzenie po całym dysku w repo z tysiącem podpakietów.
const MOST_MANIFESTS: usize = 200;

/// Katalogi, w które nie wchodzimy: wyniki budowania i cudze zależności to nie jest projekt.
const SKIPPED: [&str; 5] = ["target", "node_modules", ".git", "dist", ".loadout"];

/// Zależności ścieżkowe projektu, które wychodzą poza jego folder.
///
/// Kolejność jest kolejnością obchodu, deterministyczną: dwa wywołania na tym samym drzewie
/// dają tę samą listę, więc zdanie dla człowieka nie zmienia się między biegami.
#[must_use]
pub fn outside_the_project(project: &Path) -> Vec<Neighbour> {
    let mut found = Vec::new();
    let mut seen = 0_usize;
    walk(project, project, &mut found, &mut seen, 0);
    found
}

fn walk(project: &Path, here: &Path, found: &mut Vec<Neighbour>, seen: &mut usize, depth: usize) {
    if depth > 6 || *seen >= MOST_MANIFESTS {
        return;
    }
    let Ok(entries) = fs::read_dir(here) else {
        return;
    };
    let mut names: Vec<_> = entries
        .flatten()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    names.sort();
    let mut directories = Vec::new();
    for name in names {
        let path = here.join(&name);
        let Ok(kind) = fs::symlink_metadata(&path) else {
            continue;
        };
        // Dowiązania omijamy: skan przed startem nie ma prawa wyjść poza projekt nawet
        // czytaniem, a to jest jedyna droga, którą mógłby to zrobić.
        if kind.file_type().is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !SKIPPED.contains(&name.to_string_lossy().as_ref()) {
                directories.push(path);
            }
            continue;
        }
        let manifest = match name.to_string_lossy().as_ref() {
            "Cargo.toml" => Kind::Cargo,
            "package.json" => Kind::Npm,
            _ => continue,
        };
        *seen += 1;
        if *seen > MOST_MANIFESTS {
            return;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let shown = path
            .strip_prefix(project)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        for escaping in escaping_paths(&text, manifest) {
            found.push(Neighbour {
                manifest: shown.clone(),
                path: escaping,
            });
        }
    }
    for directory in directories {
        walk(project, &directory, found, seen, depth + 1);
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Cargo,
    Npm,
}

fn escaping_paths(text: &str, kind: Kind) -> Vec<String> {
    let mut out = Vec::new();
    match kind {
        Kind::Cargo => {
            /* Skan po wierszach, nie pełny rozbiór TOML-a, i to jest świadome: pytanie brzmi
             * „czy ktoś tu wskazał katalog wyżej", a nie „jaki jest dokładny graf zależności".
             * Rozbiór wymagałby skrzyni, której to drzewo nie ma, a odpowiedź byłaby ta sama. */
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with('#') {
                    continue;
                }
                let mut rest = line;
                while let Some(at) = rest.find("path") {
                    rest = &rest[at + "path".len()..];
                    let value = rest.trim_start();
                    let Some(value) = value.strip_prefix('=') else {
                        continue;
                    };
                    let value = value.trim_start();
                    let Some(value) = value.strip_prefix('"') else {
                        continue;
                    };
                    let Some(end) = value.find('"') else { continue };
                    let candidate = &value[..end];
                    if leaves_the_project(candidate) {
                        out.push(candidate.to_owned());
                    }
                }
            }
        }
        Kind::Npm => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
                return out;
            };
            for section in ["dependencies", "devDependencies", "optionalDependencies"] {
                let Some(map) = value.get(section).and_then(serde_json::Value::as_object) else {
                    continue;
                };
                for asked in map.values().filter_map(serde_json::Value::as_str) {
                    let candidate = asked
                        .strip_prefix("file:")
                        .or_else(|| asked.strip_prefix("link:"))
                        .unwrap_or("");
                    if leaves_the_project(candidate) {
                        out.push(candidate.to_owned());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Czy ta ścieżka wychodzi poza folder projektu.
///
/// Bezwzględna wychodzi zawsze; względna wtedy, kiedy `..` wyprowadza wyżej, niż zeszła.
fn leaves_the_project(candidate: &str) -> bool {
    if candidate.is_empty() {
        return false;
    }
    if Path::new(candidate).is_absolute() {
        return true;
    }
    let mut depth = 0_i32;
    for part in candidate.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            _ => depth += 1,
        }
    }
    false
}

/// Ile nazw wchodzi do jednego zdania.
const NAMES_IN_A_SENTENCE: usize = 2;

/// Zdanie dla człowieka o kroku, który pracuje we własnej kopii.
///
/// Nazywa brak i obie drogi wyjścia. Żadna z nich nie jest czymś, co Loadout wykona sam:
/// wciągnięcie cudzego katalogu jest decyzją o zakresie odczytu, a przestawienie kroku
/// na folder projektu zdejmuje ochronę przed kolizją, którą własna kopia daje.
#[must_use]
pub fn said(step: &str, neighbours: &[Neighbour]) -> String {
    let shown = neighbours
        .iter()
        .take(NAMES_IN_A_SENTENCE)
        .map(|one| format!("{} needs {}", one.manifest, one.path))
        .collect::<Vec<_>>()
        .join(", ");
    let rest = match neighbours.len().saturating_sub(NAMES_IN_A_SENTENCE) {
        0 => String::new(),
        more => format!(" and {more} more like it"),
    };
    format!(
        "\"{step}\" works in its own copy of the project, and {shown}{rest} — a folder that is \
         not inside the project and will not be inside the copy. Either run this step in the \
         project folder, or bring that dependency into the project before the run."
    )
}
