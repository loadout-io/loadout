//! Co zostawiły biegi, których Loadout **nie zamknął** — i jedyna droga, którą to schodzi.
//!
//! # Po co to istnieje
//!
//! Domykanie drzew przy otwarciu folderu (Z-9, [`super::reconcile::close_what_the_runs_left`])
//! chodzi po `<bieg>/.isolation/<klucz>`, czyli po notatce, którą bieg pisze o każdym katalogu,
//! jaki sobie otworzył. Bieg sprzed tej notatki nie ma jej wcale, więc jego katalog roboczy jest
//! dla tamtej pętli **niewidzialny**: nie zamyka go i — co kosztuje więcej — nie mówi o nim ani
//! słowa. Zmierzone u właściciela 2026-09-03 na `urc-monorepo`: dziennik zameldował „75 folder(s)
//! closed", a `git worktree list` dalej wymieniał dwanaście katalogów `work/s_*` po 264 MB. Gałęzi
//! `loadout/*` stało tam 99 przy czternastu biegach; w `meetnotes` siedem.
//!
//! # Dlaczego to NIE sprząta samo
//!
//! Bo nie wie, czyje to jest. Katalog bez notatki mógł powstać ręką człowieka, a gałąź, której
//! biegu już nie ma, może nieść jedyną kopię tego, co agent napisał. Ten moduł **liczy i mówi**,
//! a zdejmuje wyłącznie to, o co ktoś poprosił kliknięciem — i wyłącznie to, co nie niesie własnej
//! pracy: katalog z niezapisaną zmianą zostaje, gałąź z commitem spoza `HEAD` zostaje, a zdanie
//! nazywa oba po imieniu i ze ścieżką.
//!
//! # Granica
//!
//! Ten moduł nie zna ani okna, ani biegu: dostaje ścieżkę projektu i oddaje liczby oraz zdania po
//! angielsku (D5). Kasowanie oddaje TYM SAMYM funkcjom, którymi kończy się zwykły bieg
//! ([`isolate::remove_tree`], [`isolate::drop_branch`], [`super::history::forget_run_inner`]) —
//! druga polityka sprzątania obok byłaby tą, która kiedyś skasuje czyjąś pracę (niezmiennik 23).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::isolate;

/// Katalog biegów wewnątrz folderu człowieka. Ta sama ścieżka, co w [`super::reconcile`].
const RUNS_DIR: &str = ".loadout/runs";

/// Katalog, w którym bieg zakłada kroki. `<bieg>/work/<klucz pracy>`.
const WORK_DIR: &str = "work";

/// Przedrostek każdej gałęzi, którą zakłada bieg — składa go [`isolate::branch_for`].
///
/// Pusty bieg i pusty krok dają `loadout/`, więc „które gałęzie są nasze" ma jedną odpowiedź
/// i nie da się jej rozjechać z nazywaniem (niezmiennik 13).
fn ours() -> String {
    isolate::branch_for("", "").trim_end_matches('/').to_owned() + "/"
}

/// Co ten projekt mógłby zapomnieć — liczby dla obu kontrolek historii.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CouldForgetWire {
    /// Ile katalogów roboczych stoi po biegach, których Loadout nie zamknął.
    pub work_folders: usize,
    /// Ile gałęzi zostało po biegach, których katalogu już nie ma.
    pub branches: usize,
    /// Jedno zdanie o obu liczbach. **Pusty napis znaczy „nie ma o czym mówić"** — a wtedy nie
    /// ma też kontrolki (niezmiennik 16).
    pub said: String,
    /// Co zejdzie, kiedy człowiek każe zapomnieć biegi starsze niż tyle dni, ile podał.
    pub older: OlderWire,
}

/// Co zdejmie „forget runs older than N days" — policzone, zanim ktokolwiek naciśnie.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OlderWire {
    /// Ile katalogów biegów jest starszych niż podana liczba dni.
    pub runs: usize,
    /// Ile gałęzi zejdzie razem z nimi.
    pub branches: usize,
    /// Ile katalogów roboczych zejdzie razem z nimi.
    pub work_folders: usize,
    /// Jedno zdanie o tym, co zejdzie. Nigdy puste: „nic" też jest odpowiedzią na to pytanie.
    pub said: String,
}

/// Co naprawdę zeszło, i co zostało — razem ze zdaniem, które czyta człowiek.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgottenWire {
    /// Ile katalogów roboczych zdjęto.
    pub work_folders: usize,
    /// Ile gałęzi zdjęto.
    pub branches: usize,
    /// Ile biegów zapomniano w całości. Zero po zamiataniu i to jest o nim prawda: zamiatacz
    /// nie zapomina biegów, tylko to, co po nich zostało.
    pub runs: usize,
    /// Co się stało i co **nie** zeszło — po imieniu i ze ścieżką.
    pub said: String,
}

/// Liczby dla obu kontrolek historii, policzone jednym przejściem po projekcie.
///
/// JEDNA KOMENDA NA DWA PYTANIA, bo panel zadaje je w tej samej chwili i o tym samym folderze
/// (niezmiennik 13). Dwa wywołania dałyby dwie odpowiedzi z dwóch chwil, a między nimi mieści się
/// bieg, który właśnie się skończył.
#[must_use]
pub fn what_this_folder_could_forget(project: &Path, older_than_days: u32) -> CouldForgetWire {
    let left = what_the_old_runs_left(project);
    let older = what_goes_with_the_old_runs(project, older_than_days);
    CouldForgetWire {
        work_folders: left.work_folders.len(),
        branches: left.branches.len(),
        said: how_much_is_left(left.work_folders.len(), left.branches.len()),
        older,
    }
}

/// Zdejmuje to, co zostawiły biegi, których Loadout nie zamknął — i **tylko** to.
///
/// Dwa strażniki, oba w stronę „zostaw", bo pomyłka w drugą stronę kasuje czyjąś pracę:
///
/// - **katalog z niezapisaną zmianą zostaje.** To jedyna kopia tego, co ktoś w nim napisał;
///   gałąź jej nie niesie, dopóki nikt tego nie zacommitował.
/// - **gałąź z commitem, którego nie ma `HEAD` projektu, zostaje.** Praca zapisana ręką agenta
///   żyje wyłącznie na niej — po `branch -D` nie sięga do niej nic poza `git fsck`.
///
/// Katalogi idą PRZED gałęziami i to nie jest kosmetyka: `branch -D` odmawia gałęzi wyjętej do
/// pracy w drzewie, więc kolejność odwrotna zostawiałaby gałąź, którą trzymał katalog zdjęty
/// sekundę później.
#[must_use]
pub fn forget_what_the_old_runs_left(project: &Path) -> ForgottenWire {
    let left = what_the_old_runs_left(project);
    let mut done = ForgottenWire::default();
    let mut stayed: Vec<String> = Vec::new();

    for folder in &left.work_folders {
        if isolate::holds_unsaved_work(folder) {
            stayed.push(format!(
                "It left the folder {} where it is, because it still holds changes nobody saved.",
                folder.display()
            ));
            continue;
        }
        match isolate::remove_tree(project, folder) {
            Ok(()) => done.work_folders += 1,
            Err(said) => stayed.push(format!(
                "It could not take the folder {} away: {}",
                folder.display(),
                first_line(&said)
            )),
        }
    }

    for branch in &left.branches {
        if isolate::carries_its_own_work(project, branch) {
            stayed.push(format!(
                "It left the branch {branch} alone, because it carries a commit this project does \
                 not have anywhere else."
            ));
            continue;
        }
        match isolate::drop_branch(project, branch) {
            Ok(()) => done.branches += 1,
            Err(said) => stayed.push(format!(
                "It could not take the branch {branch} away: {}",
                first_line(&said)
            )),
        }
    }

    done.said = one_paragraph(
        &format!(
            "Loadout took {} and {} away.",
            folders_read(done.work_folders),
            branches_read(done.branches)
        ),
        &stayed,
    );
    done
}

/// Zapomina biegi starsze niż `days` dni — razem z ich gałęziami i katalogami roboczymi.
///
/// TĄ SAMĄ DROGĄ, CO PRZYCISK PRZY JEDNYM BIEGU (niezmiennik 23): [`super::history::forget_run_inner`]
/// zdejmuje najpierw gałęzie, a katalog dopiero po nich, i odmawia w całości, kiedy którakolwiek
/// gałąź jest w tej chwili wyjęta do pracy. Własne `remove_dir_all` tutaj byłoby drugą polityką
/// kasowania — tą, która pominie ostrożność.
///
/// Odmowa jednego biegu **nie zatrzymuje pozostałych**, ale — inaczej niż przy retencji
/// z ustawień — nie ginie w dzienniku: człowiek nacisnął to sam i czeka na odpowiedź.
#[must_use]
pub fn forget_runs_older_than(project: &Path, days: u32) -> ForgottenWire {
    let mut done = ForgottenWire::default();
    let (going, mut stayed) = old_runs_that_would_go(project, days);
    for one in going {
        /* DRZEWA SCHODZĄ PIERWSZE, I BEZ TEGO TA KONTROLKA BYŁABY MARTWA (2026-09, Z-46). Katalog
         * roboczy trzyma gałąź swojego kroku wyjętą do pracy, a `forget_run_inner` odmawia CAŁEGO
         * biegu, kiedy którakolwiek jego gałąź jest wyjęta — czyli odmawiałby dokładnie tym
         * biegom, po których coś zostało, i tylko im. Zdejmuje je ta sama funkcja, co po zwykłym
         * biegu (niezmiennik 23); nieudane zdjęcie mówi o sobie samo, bo wtedy odmawia zdanie
         * niżej i niesie powód od gita. */
        let mut folders_gone = 0;
        for folder in &one.work_folders {
            if isolate::remove_tree(project, folder).is_ok() {
                folders_gone += 1;
            }
        }
        match super::history::forget_run_inner(project, &one.folder) {
            Ok(gone) => {
                done.runs += 1;
                done.branches += gone.len();
                done.work_folders += folders_gone;
            }
            Err(error) => stayed.push(format!(
                "It could not forget {}: {}",
                one.folder,
                first_line(&error.to_string())
            )),
        }
    }
    done.said = one_paragraph(
        &format!(
            "Loadout forgot {}, and took {} and {} with them.",
            runs_read(done.runs),
            branches_read(done.branches),
            folders_read(done.work_folders)
        ),
        &stayed,
    );
    done
}

/// Katalogi robocze, których ten bieg nie zamknął — para `(katalog biegu, katalog kroku)`.
///
/// Wołane przez [`super::reconcile`] przy otwarciu folderu, żeby zdanie o każdym z nich trafiło
/// tam, gdzie człowiek o biegu czyta (niezmiennik 29).
#[must_use]
pub fn folders_the_runs_did_not_close(project: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut left = Vec::new();
    let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) else {
        return left;
    };
    let registered = registered_here(project);
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    // Kolejność katalogu jest dowolna, a z tych zdań składa się jedno pole tekstowe: bez
    // sortowania ten sam folder czytałby się przy każdym otwarciu inaczej.
    dirs.sort();
    for dir in dirs {
        if !super::reconcile::run_is_over(&dir) {
            continue;
        }
        for folder in work_folders_in(&dir, &registered) {
            left.push((dir.clone(), folder));
        }
    }
    left
}

/// Zdanie o katalogu roboczym, którego Loadout nie zamknął — **ze ścieżką**.
///
/// Ścieżka jest treścią, nie ozdobą, i to jest ta sama zasada, co przy
/// [`isolate::could_not_tidy`]: bez niej człowiek dowiaduje się, że coś zostało, i musi sam
/// znaleźć jeden katalog wśród kilkudziesięciu innych.
#[must_use]
pub fn a_folder_we_did_not_close(work: &Path) -> String {
    format!(
        "Loadout did not close the folder this step worked in, because this run is older than the \
         note Loadout now writes down for every folder it opens. It is still here: {}. History can \
         forget what runs like this one left behind.",
        work.display()
    )
}

// ── co gdzie leży ─────────────────────────────────────────────────────────────────────────

/// Co stoi w projekcie po biegach, których Loadout nie zamknął.
#[derive(Debug, Default)]
struct LeftOver {
    /// Katalogi robocze, które git wciąż zna, a bieg, który je otworzył, dawno zszedł.
    work_folders: Vec<PathBuf>,
    /// Gałęzie `loadout/*` biegów, których katalogu już nie ma.
    branches: Vec<String>,
}

/// Jedno przejście po projekcie: co po tych biegach zostało.
fn what_the_old_runs_left(project: &Path) -> LeftOver {
    LeftOver {
        work_folders: folders_the_runs_did_not_close(project)
            .into_iter()
            .map(|(_, folder)| folder)
            .collect(),
        branches: branches_without_a_run(project),
    }
}

/// Gałęzie `loadout/*`, których biegu nie ma już w `runs/`.
///
/// KTO JESZCZE TU JEST, LICZYMY NA DWA SPOSOBY, i oba są ostrożne w tę samą stronę: identyfikator
/// z `run.json` **oraz** ogon nazwy katalogu za `__`. Bieg, którego opisu nie da się przeczytać,
/// ma zostać biegiem, który wciąż tu jest — inaczej jedna ręczna edycja pliku zamienia wszystkie
/// jego gałęzie w leżaki proponowane do zdjęcia.
///
/// Gałąź, z której nazwy nie da się wyczytać biegu, zostaje z tego samego powodu: nie wiemy o niej
/// dość, żeby ją komukolwiek przypisać.
fn branches_without_a_run(project: &Path) -> Vec<String> {
    let mut still_here: BTreeSet<String> = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(project.join(RUNS_DIR)) {
        for dir in entries.flatten().map(|entry| entry.path()) {
            if let Some(id) = super::reconcile::run_named_by(&dir) {
                still_here.insert(id);
            }
            if let Some((_, tail)) = dir
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(|name| name.split_once("__"))
            {
                still_here.insert(tail.to_owned());
            }
        }
    }
    let ours = ours();
    isolate::branches_under(project, &ours)
        .into_iter()
        .filter(|name| {
            name.strip_prefix(&ours)
                .and_then(|rest| rest.split('/').next())
                .is_some_and(|run| !still_here.contains(run))
        })
        .collect()
}

/// Katalogi biegów starsze niż `days` dni, **od najstarszego**.
///
/// Od najstarszego, bo w tej kolejności czyta się odmowa: kiedy git odmówi w środku listy,
/// człowiek ma zdjęte to, co najdawniejsze, a nie losową połowę.
fn runs_older_than(project: &Path, days: u32) -> Vec<String> {
    let mut older: Vec<String> = super::handoffs::run_dirs(project)
        .iter()
        .filter_map(|dir| {
            dir.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_owned)
        })
        .filter(|folder| how_many_days_ago(folder).is_some_and(|ago| ago > u64::from(days)))
        .collect();
    // `run_dirs` oddaje od najnowszego, a nazwa katalogu otwiera się znacznikiem czasu UTC.
    older.reverse();
    older
}

/// Ile pełnych dni temu ruszył bieg o tej nazwie katalogu. `None`, kiedy nazwa daty nie niesie.
///
/// Z NAZWY, NIE Z DATY PLIKU, i to jest jedna odpowiedź na jedno pytanie (niezmiennik 13): wiersz
/// historii pokazuje dokładnie ten napis (`history::when_of` czyta tę samą nazwę). Data
/// modyfikacji katalogu zmienia się przy każdym dopisaniu do biegu, więc „starszy niż 30 dni"
/// znaczyłoby wtedy co innego na ekranie i co innego przy kasowaniu.
///
/// Bieg z przyszłości (zegar przestawiony, katalog przeniesiony ręcznie) daje `None`, czyli
/// „nie schodzi": jedyna bezpieczna odpowiedź dla liczby, na której stoi kasowanie.
fn how_many_days_ago(folder: &str) -> Option<u64> {
    let started = day_of(folder)?;
    let today = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs()
            / 86_400,
    )
    .ok()?;
    u64::try_from(today - started).ok()
}

/// Dzień od epoki, który niesie nazwa katalogu biegu (`20260901-120000__…`).
fn day_of(folder: &str) -> Option<i64> {
    let stamp = folder.split("__").next().unwrap_or(folder);
    let day = stamp.split('-').next()?;
    if day.len() != 8 || !day.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year: i64 = day.get(0..4)?.parse().ok()?;
    let month: i64 = day.get(4..6)?.parse().ok()?;
    let of_month: i64 = day.get(6..8)?.parse().ok()?;
    Some(days_from_civil(year, month, of_month))
}

/// Dni od 1970-01-01 dla daty z kalendarza gregoriańskiego — era 400-letnia, bez zależności.
///
/// ODWROTNOŚĆ rachunku z [`super::now_utc`], nie jego czwarta kopia: tamten idzie z sekund na
/// datę, ten z daty na dni. `chrono`/`time` odpadają z tego samego powodu, co tam —
/// `src-tauri/Cargo.toml` nie należy do tego zadania (AGENTS.md §7).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // Marzec zaczyna rok, więc dzień przestępny wypada na jego końcu i nie trzeba go nigdzie
    // wtrącać — na tym stoi cały ten rachunek.
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = if shifted >= 0 { shifted } else { shifted - 399 } / 400;
    let year_of_era = shifted - era * 400;
    let month_of_year = (month + 9) % 12;
    let day_of_year = (153 * month_of_year + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Katalogi robocze pod tym biegiem, które git zna jako miejsca pracy.
///
/// Z DYSKU, A NIE Z WYJŚCIA GITA, i to jest wybór o jednym konkretnym skutku: git oddaje ścieżkę
/// rozwiązaną do końca (`/private/var/…` na macOS), a wołający składa zdanie o katalogu, którego
/// nazwę człowiek widzi w opisie biegu (`/var/…`). Dwie pisownie jednej ścieżki w dwóch zdaniach
/// o tej samej rzeczy czytają się jak dwa różne katalogi. Porównujemy więc przez [`same_place`],
/// a oddajemy ścieżkę złożoną z katalogu biegu.
fn work_folders_in(run_dir: &Path, registered: &BTreeSet<PathBuf>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(run_dir.join(WORK_DIR)) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| registered.contains(&same_place(path)))
        .collect();
    folders.sort();
    folders
}

/// Miejsca pracy, które git zna w tym repozytorium — po ścieżkach rozwiązanych do końca.
fn registered_here(project: &Path) -> BTreeSet<PathBuf> {
    isolate::trees_registered(project)
        .iter()
        .map(|path| same_place(path))
        .collect()
}

/// Ta sama ścieżka, zapisana tak, żeby dało się ją porównać z cudzą.
///
/// `canonicalize` rozwija dowiązania — a katalog tymczasowy na macOS jest dowiązaniem
/// (`/var` → `/private/var`), więc bez tego kroku ta sama ścieżka od gita i od nas nigdy nie jest
/// równa. Ścieżka, której nie da się rozwiązać, wraca sobą samą: nie wiemy o niej nic więcej.
fn same_place(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// ── zdania dla człowieka (D5) ─────────────────────────────────────────────────────────────

/// „This project has 2 work folders and 5 branches from runs Loadout did not close."
///
/// Pusty napis, kiedy nie ma ani jednego z nich: zdanie „0 work folders and 0 branches" jest
/// wierszem, który nic nie mówi, przy każdym otwarciu każdego folderu (niezmiennik 16).
fn how_much_is_left(work_folders: usize, branches: usize) -> String {
    if work_folders == 0 && branches == 0 {
        return String::new();
    }
    format!(
        "This project has {} and {} from runs Loadout did not close.",
        folders_read(work_folders),
        branches_read(branches)
    )
}

/// Zdanie o tym, co zejdzie razem z biegami starszymi niż `days` dni.
///
/// `staying` to biegi, które są starsze i mimo to zostają, bo niosą niezapisaną pracę. Bez nich
/// zdanie „nic tu nie jest starsze niż 30 dni" byłoby po prostu nieprawdą nad folderem, w którym
/// taki bieg stoi — a to jest ta sama klasa rozjazdu między zdaniem a skutkiem, dla której ta
/// funkcja bierze dziś liczby ze wspólnej kwalifikacji (2026-09, Z-46).
fn what_would_go(
    runs: usize,
    branches: usize,
    work_folders: usize,
    days: u32,
    staying: usize,
) -> String {
    let goes = if runs == 0 {
        if staying == 0 {
            return format!("Nothing here is older than {}.", days_read(days));
        }
        format!("Nothing older than {} can go from here.", days_read(days))
    } else {
        format!(
            "Forgetting them takes {} away, and {} and {} with them.",
            runs_read(runs),
            branches_read(branches),
            folders_read(work_folders)
        )
    };
    if staying == 0 {
        return goes;
    }
    let stays = if staying == 1 {
        "1 older run holds work nobody saved, so it stays.".to_owned()
    } else {
        format!("{staying} older runs hold work nobody saved, so they stay.")
    };
    format!("{goes} {stays}")
}

/// Co zejdzie po dacie — policzone, zanim ktokolwiek naciśnie.
fn what_goes_with_the_old_runs(project: &Path, days: u32) -> OlderWire {
    // TA SAMA KWALIFIKACJA, CO PRZY KASOWANIU, i to jest cały powód, dla którego stoi ona
    // w osobnej funkcji (2026-09, Z-46): podgląd liczący „wszystko starsze niż N dni" obiecywał
    // biegi, których `forget_runs_older_than` nie tykało.
    let (going, stayed) = old_runs_that_would_go(project, days);
    let mut done = OlderWire {
        runs: going.len(),
        ..OlderWire::default()
    };
    for one in &going {
        done.branches += one.branches.len();
        done.work_folders += one.work_folders.len();
    }
    done.said = what_would_go(
        done.runs,
        done.branches,
        done.work_folders,
        days,
        stayed.len(),
    );
    done
}

/// Bieg starszy niż podana liczba dni, razem z tym, co po nim stoi.
#[derive(Debug)]
struct OldRun {
    /// Nazwa katalogu — adres, którym prosi się o ten bieg (`history::forget_run_inner`).
    folder: String,
    /// Gałęzie, które zejdą razem z nim.
    branches: Vec<String>,
    /// Katalogi robocze, które zejdą razem z nim.
    work_folders: Vec<PathBuf>,
}

/// Biegi starsze niż `days` dni, **które naprawdę zejdą**, i po zdaniu o każdym, który zostaje.
///
/// # 2026-09 (Z-46) — JEDNA KWALIFIKACJA, DWA WOŁANIA (niezmiennik 23)
///
/// Do tego dnia stały tu dwie: podgląd liczył wszystko, co starsze niż podana liczba dni,
/// a kasowanie pomijało bieg, w którego katalogu roboczym leży zmiana, której nikt nie zapisał.
/// Człowiek czytał więc „zejdą dwa", naciskał i zostawał z jednym — bez niczego, co by mu
/// powiedziało, która z dwóch liczb jest prawdziwa. Zdanie nad kontrolką, która kasuje, ma mówić
/// o skutku tej kontrolki, a nie o zbiorze, z którego skutek się liczy.
///
/// # DWA STRAŻNIKI, TE SAME, CO PRZY „FORGET THEM"
///
/// **Niezapisana zmiana zatrzymuje CAŁY bieg**, nie tylko swój katalog: `forget_run_inner` kasuje
/// katalog biegu rekurencyjnie, więc zeszłaby razem z nim. Katalog jest jedyną kopią tego, co ktoś
/// w nim napisał.
///
/// **Gałąź z commitem spoza `HEAD` zatrzymuje bieg tak samo**, i to jest droga, która ginęła
/// między dwiema kontrolkami: zamiatacz o nią pytał, kasowanie po dacie nie. Cena jest realna
/// i świadoma — folder, w którym każdy stary bieg zostawił niewlaną gałąź, nie zdejmie po dacie
/// ani jednego biegu i powie o każdym z nich osobnym zdaniem. To jest właściwa strona pomyłki:
/// w drugą kasujemy jedyną kopię pracy agenta.
fn old_runs_that_would_go(project: &Path, days: u32) -> (Vec<OldRun>, Vec<String>) {
    let registered = registered_here(project);
    let mut going = Vec::new();
    let mut stayed = Vec::new();
    for folder in runs_older_than(project, days) {
        let dir = project.join(RUNS_DIR).join(&folder);
        let work_folders = work_folders_in(&dir, &registered);
        if let Some(unsaved) = work_folders
            .iter()
            .find(|one| isolate::holds_unsaved_work(one))
        {
            stayed.push(format!(
                "It left {folder} alone, because the folder {} still holds changes nobody saved.",
                unsaved.display()
            ));
            continue;
        }
        let branches = branches_of_the_run_in(project, &dir);
        /* DATA NIE JEST ZGODĄ NA SKASOWANIE CZYJEJŚ PRACY (2026-09, Z-46). Ten sam strażnik, co
         * w zamiataczu wyżej, i tu jest równie potrzebny: `forget_run_inner` zdejmuje KAŻDĄ gałąź
         * tego biegu przez `git branch -D`, więc bez tego pytania czysta gałąź z commitem spoza
         * `HEAD` — czyli praca, którą krok zapisał sam — znikała bez słowa, a po `branch -D` nie
         * sięga do niej nic poza `git fsck`. Kwalifikacja po samym brudnym katalogu roboczym tej
         * drogi nie widziała: drzewo dawno zeszło, a commit został.
         *
         * ZATRZYMUJE CAŁY BIEG, nie samą gałąź: odmowa częściowa zostawiłaby gałąź bez `run.json`,
         * z którego liczy się jej przedrostek — czyli gałąź, której nic już nie umie nazwać. */
        if let Some(carries) = branches
            .iter()
            .find(|one| isolate::carries_its_own_work(project, one))
        {
            stayed.push(format!(
                "It left {folder} alone, because the branch {carries} carries a commit this \
                 project does not have anywhere else."
            ));
            continue;
        }
        going.push(OldRun {
            folder,
            branches,
            work_folders,
        });
    }
    (going, stayed)
}

/// Gałęzie tego jednego biegu, po przedrostku złożonym z jego identyfikatora.
///
/// Przedrostek składa [`isolate::branch_for`], czyli ta sama funkcja, która nadaje nazwy — tak
/// samo, jak robi to `history::forget_run_branches_inner`. Napis sklejony tu z palca byłby drugą
/// regułą na to samo pytanie, a ta liczba stoi nad kontrolką, która KASUJE.
fn branches_of_the_run_in(project: &Path, run_dir: &Path) -> Vec<String> {
    let Some(id) = super::reconcile::run_named_by(run_dir) else {
        return Vec::new();
    };
    isolate::branches_under(project, &isolate::branch_for(&id, ""))
}

/// „1 work folder" albo „3 work folders" — zdanie, nie liczba obok słowa.
fn folders_read(how_many: usize) -> String {
    if how_many == 1 {
        "1 work folder".to_owned()
    } else {
        format!("{how_many} work folders")
    }
}

/// „1 branch" albo „5 branches".
fn branches_read(how_many: usize) -> String {
    if how_many == 1 {
        "1 branch".to_owned()
    } else {
        format!("{how_many} branches")
    }
}

/// „1 run" albo „3 runs".
fn runs_read(how_many: usize) -> String {
    if how_many == 1 {
        "1 run".to_owned()
    } else {
        format!("{how_many} runs")
    }
}

/// „1 day" albo „30 days".
fn days_read(how_many: u32) -> String {
    if how_many == 1 {
        "1 day".to_owned()
    } else {
        format!("{how_many} days")
    }
}

/// Zdanie o skutku, a za nim po jednym zdaniu o każdym leżaku, który został.
fn one_paragraph(done: &str, stayed: &[String]) -> String {
    if stayed.is_empty() {
        return done.to_owned();
    }
    format!("{done} {}", stayed.join(" "))
}

/// Pierwszy wiersz cudzej odpowiedzi, **zakończony jedną kropką**.
///
/// Pierwszy wiersz, bo git odpowiada akapitem, a reszta akapitu mówi to samo dłużej — ta sama
/// zasada, co przy [`isolate::could_not_tidy`]. Kropka dokładana warunkowo, bo zdania odmowy
/// z `history::HistoryError` już ją mają, a „try again.." czyta się jak usterka renderu.
fn first_line(said: &str) -> String {
    let first = said.lines().next().unwrap_or("").trim();
    if first.ends_with(['.', '!', '?']) {
        first.to_owned()
    } else {
        format!("{first}.")
    }
}
