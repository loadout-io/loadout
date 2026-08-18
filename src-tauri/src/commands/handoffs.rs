//! Komenda przekazań: co jeden krok oddał następnemu, odczytane z plików.
//!
//! **Ani jednego `use tauri::`** — jak w całym tym katalogu (`docs/ARCHITECTURE.md` §3).
//!
//! Cała wiedza o formacie pliku mieszka w `memory::handoff` i zostaje tam (niezmiennik 23):
//! to on wie, że pliki leżą w `<bieg>/handoffs/`, że biorą się wyłącznie te z rozszerzeniem
//! `.md`, że prefiks `NN` jest numerem kroku i że **jeden nieczytelny plik nie zabiera całej
//! listy** (niezmiennik 5). Ta warstwa robi dokładnie dwie rzeczy: wypisuje katalogi biegów
//! i przekłada [`Handoff`] na kształt, który rozumie okno.
//!
//! # Dlaczego to w ogóle powstało (2026-08-18)
//!
//! Przekazania są **jedyną** drogą, którą wynik jednego kroku dochodzi do promptu
//! następnego (`docs/ARCHITECTURE.md` §8, D6 punkt 5) — i do tego dnia nie było ani jednej
//! komendy, którą okno mogłoby o nie zapytać. Pliki powstawały, `memory::handoff::scan_run_dir`
//! umiało je przeczytać, a człowiek nie widział z tego ani jednej litery: sekcja, która ma
//! pokazać, co krok komu oddał, nie miała skąd wziąć danych i rysowała pustkę.
//!
//! # Katalog, którego nie ma, to PUSTA LISTA
//!
//! Na świeżej maszynie `<projekt>/.loadout/runs/` nie istnieje, bo nikt jeszcze nic nie
//! uruchomił. To jest stan normalny, nie awaria dysku — dokładnie ten sam wybór, co
//! w `commands::skills::list_skills_inner` i `memory::notes::scan_notes`. Czerwony pasek na
//! świeżej instalacji uczy człowieka ignorować czerwone paski.
//!
//! # Kolejność
//!
//! Katalogi biegów **od najnowszego**, bo ich nazwa otwiera się znacznikiem czasu UTC
//! (`commands::run::stamp`: `20260816-194804__<uuid>`), więc porządek leksykograficzny odwrotny
//! **jest** porządkiem od najświeższego biegu. Wewnątrz jednego biegu zostaje kolejność, którą
//! oddaje [`scan_run_dir`] — czyli numer kroku rosnąco, ten sam, który widać w `ls handoffs/`.
//! Kolejność „jak zdąży system plików" byłaby listą, która przy każdym wejściu na sekcję
//! wygląda inaczej.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::memory::Result;
use crate::memory::handoff::{Handoff, scan_run_dir};

/// Katalog Loadouta w projekcie. Ta sama nazwa, co w `commands::run` i `workspace` — i to jest
/// trzecia kopia tego napisu w drzewie, wypisana świadomie: tamte dwie są prywatne w plikach,
/// których ta warstwa nie ma prawa zmieniać po cichu.
const PROJECT_DIR: &str = ".loadout";

/// Katalog biegów pod [`PROJECT_DIR`] (`docs/ARCHITECTURE.md` §8).
const RUNS_DIR: &str = "runs";

/// Przekazanie tak, jak widzi je okno.
///
/// Osobny typ od [`Handoff`], z tego samego powodu, co [`crate::commands::memory::NoteWire`]:
/// tamten nie jest `Serialize` i nie ma nim być — `#[derive(Serialize)]` dopisany w
/// `memory/handoff.rs` zamroziłby JEGO pola jako kontrakt drutu przy okazji, bez ani jednego
/// kryterium, które by tego pilnowało.
///
/// Czego tu nie ma: `body` (okno nie renderuje treści przekazania — ona jest dla agenta),
/// `reads`, `supersedes`, `est_tokens` i `extra`. Pole, którego nikt nie czyta, jest polem,
/// które rozjedzie się pierwsze (niezmiennik 21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffWire {
    /// Tożsamość przekazania z jego front-mattera.
    pub id: String,
    /// Bieg, w którym powstało.
    pub run: String,
    /// Nazwa kafelka, który je oddał — ta sama, którą człowiek widzi na płótnie.
    pub from: String,
    /// Nazwy kafelków, które je dostają. Pusta lista znaczy „nikt po tym kroku nie idzie".
    pub to: Vec<String>,
    /// `findings`, `plan`, `review`… — słowo z pliku, nie z ekranu.
    pub kind: String,
    /// O co poproszono ten krok, w jednym wierszu.
    pub title: String,
    /// `current` albo `superseded`.
    pub status: String,
    /// Kiedy powstało, ISO 8601 UTC.
    pub created: String,
    /// Gdzie leży, **licząc od katalogu projektu**.
    ///
    /// Nie bezwzględnie: `/Users/<ktoś>/Projects/…` na ekranie jest szumem, a ścieżka
    /// bezwzględna wpisana w widok przestaje cokolwiek znaczyć po pierwszym przeniesieniu
    /// katalogu biegu (niezmiennik 4 — plik przeżywa `cp -r`).
    pub path: String,
    /// Ile waży ciało, **zmierzone przy odczycie**.
    ///
    /// [`Handoff::actual_bytes`], nie `meta.bytes`: to drugie jest **deklaracją z pliku** i przy
    /// cudzym albo uciętym zapisie bywa nieprawdą ([`Handoff::bytes_mismatch`]). Liczba na
    /// ekranie ma odpowiadać na pytanie „ile tego jest", a na nie odpowiada tylko pomiar.
    pub bytes: usize,
}

impl HandoffWire {
    /// Przekazanie z dysku → kształt dla okna, ze ścieżką liczoną od katalogu projektu.
    fn from(handoff: &Handoff, project: &Path) -> Self {
        // Deklaracja z pliku i pomiar rozeszły się: to jest fakt do zaraportowania, nie do
        // wygładzenia. Dziennik jest tu jedynym właściwym miejscem — okno dostaje pomiar, a nikt
        // nie zobaczyłby po samej liczbie, że plik kłamie o własnej długości.
        if handoff.bytes_mismatch() {
            tracing::warn!(
                file = %handoff.path.display(),
                said = handoff.meta.bytes,
                measured = handoff.actual_bytes,
                "this handoff declares a body length it does not have"
            );
        }
        Self {
            id: handoff.meta.id.clone(),
            run: handoff.meta.run.clone(),
            from: handoff.meta.from.clone(),
            to: handoff.meta.to.clone(),
            kind: handoff.meta.kind.name().to_owned(),
            title: handoff.meta.title.clone(),
            status: handoff.meta.status.name().to_owned(),
            created: handoff.meta.created.clone(),
            path: handoff
                .path
                .strip_prefix(project)
                .unwrap_or(&handoff.path)
                .display()
                .to_string(),
            bytes: handoff.actual_bytes,
        }
    }
}

/// Wszystkie przekazania wszystkich biegów tego projektu, od najnowszego biegu.
///
/// Bez argumentu zakresu: okno pyta „co ten projekt przekazywał", a nie „co przekazał bieg
/// numer trzy". Filtrowanie po biegu jest wyborem widoku i mieszka po tamtej stronie granicy —
/// drugi argument tutaj byłby drugim miejscem, w którym mieszka odpowiedź na pytanie, które
/// przekazania pokazać.
pub fn list_handoffs_inner(project: &Path) -> Result<Vec<HandoffWire>> {
    let mut out = Vec::new();
    for dir in run_dirs(project) {
        // Błąd jednego katalogu biegu **nie zabiera pozostałych** (niezmiennik 5): bieg
        // z katalogiem `handoffs/`, do którego nie mamy prawa czytać, jest jednym biegiem
        // mniej na liście, a nie pustą sekcją. `scan_run_dir` sam już milczy o katalogu,
        // którego nie ma.
        match scan_run_dir(&dir) {
            Ok(handed) => out.extend(handed.iter().map(|one| HandoffWire::from(one, project))),
            Err(error) => tracing::warn!(
                run = %dir.display(),
                %error,
                "this run's handoffs could not be read, so they are not on the list"
            ),
        }
    }
    Ok(out)
}

/// Katalogi biegów tego projektu, **od najnowszego**. Katalog, którego nie ma, daje pustą listę.
fn run_dirs(project: &Path) -> Vec<PathBuf> {
    let runs = project.join(PROJECT_DIR).join(RUNS_DIR);
    let Ok(entries) = std::fs::read_dir(&runs) else {
        // Świeża maszyna: nikt jeszcze nic nie uruchomił. Zero biegów, nie awaria — powód
        // stoi w nagłówku modułu.
        return Vec::new();
    };

    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    // Nazwa katalogu otwiera się znacznikiem czasu UTC bez separatorów, więc porządek
    // leksykograficzny odwrotny JEST porządkiem od najświeższego biegu.
    dirs.sort_unstable();
    dirs.reverse();
    dirs
}
