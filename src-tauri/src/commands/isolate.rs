//! Własne drzewo robocze kroku: `git worktree`, kiedy projekt jest repozytorium, a kopia
//! systemu plików, kiedy nie jest.
//!
//! # Dlaczego nie kopiujemy plik po pliku
//!
//! Bo przegrywamy z systemem plików po kawałku. Zmierzone 2026-08-19 na `~/Projects/meetnotes`:
//! bieg odmówił na `.claude/worktrees/murmur-server`, czyli na **dowiązaniu do katalogu** —
//! `DirEntry::file_type` za dowiązaniem nie podąża, więc wpis wyglądał na „nie katalog", szedł
//! do `fs::copy`, a `fs::copy` za nim podążało i odmawiało. Kolejka FIFO w tym samym drzewie
//! jest jeszcze gorsza: `fs::copy` na niej nie odmawia, tylko **blokuje się na zawsze**, bo
//! otwarcie do odczytu czeka na piszącego. Takie wpisy robią same `pnpm`, `python -m venv`,
//! `git worktree` i worktree Claude Code.
//!
//! # Co daje drzewo, czego nie dała kopia
//!
//! **Drogę powrotną.** Do 2026-08-19 `copy_project_into` był jedynym transportem w całym
//! `commands::run` — cokolwiek agent napisał, zostawało w `.loadout/runs/<ts>/work/<krok>/`
//! i nie docierało do projektu nigdy. Drzewo stoi na GAŁĘZI, więc praca jest osiągalna z gita:
//! widać ją w `git log`, porównuje się `git diff`, scala normalnie.
//!
//! # Granica
//!
//! Ten moduł nie zna ani biegu, ani okna: dostaje dwie ścieżki i nazwę gałęzi, oddaje fakt.
//! Zdanie dla człowieka składa [`super::RunError`], bo tylko ono zna nazwę kroku.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Czego nie wnosimy do własnego drzewa, kiedy kopiujemy folder bez gita.
///
/// `.loadout` jest obowiązkowy, nie kosmetyczny: katalog biegu leży pod
/// `<projekt>/.loadout/runs/<…>/work/<krok>`, więc kopiowanie projektu do siebie samego
/// schodziłoby w nieskończoność. Pozostałe trzy są wyborem po stronie CZASU — `.git` dużego
/// repozytorium to gigabajty, `node_modules` i `target` odtwarza się jedną komendą.
///
/// Przy drzewie gita ta lista nie jest potrzebna: git sam nie niesie tego, czego nie śledzi.
const NOT_COPIED: [&str; 4] = [".git", ".loadout", "node_modules", "target"];

/// Nasz własny katalog w projekcie (`docs/ARCHITECTURE.md` §8).
///
/// Ukośnik na końcu jest treścią: bez niego wzorzec łapałby też plik o nazwie zaczynającej się
/// od `.loadout`, którego nie zostawiliśmy tam my.
const OURS: &str = ".loadout/";

/// Jak powstało drzewo tego kroku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum How {
    /// `git worktree` na własnej gałęzi — praca ma dokąd wrócić.
    Tree {
        /// Nazwa gałęzi, po której człowiek znajdzie tę pracę w `git branch`.
        branch: String,
    },
    /// Kopia plików. Folder, który repozytorium nie jest, innej drogi nie ma.
    Copy,
}

/// Gotowe drzewo kroku razem z tym, czego do niego nie weszło.
#[derive(Debug, Clone)]
pub struct Made {
    pub how: How,
    /// Pliki, o których git nie wie, więc drzewo ich nie niesie.
    ///
    /// **Lista, nie liczba, i nie pustka.** Plik, który po cichu nie dojechał do agenta, jest
    /// najgorszym kształtem tej funkcji: bieg wygląda na kompletny, a agentowi brakuje czegoś,
    /// co człowiek widzi u siebie na ekranie.
    pub left_behind: Vec<String>,
}

/// Dlaczego drzewa nie da się zrobić. Każdy wariant naprawia się inaczej, więc każdy jest
/// osobnym zdaniem — i każde zdanie mówi, CO Z TYM ZROBIĆ.
#[derive(Debug)]
pub enum Trouble {
    /// Repozytorium jest, commita nie ma, więc nie ma z czego odbić drzewa.
    NoCommitYet,
    /// Git odmówił. Niesiemy jego własne zdanie, bo ono jest konkretniejsze niż nasze.
    Git(String),
    /// Kopiowanie nie doszło do skutku.
    Copying(io::Error),
}

impl fmt::Display for Trouble {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCommitYet => formatter.write_str(
                "this project is a git repository with no commits yet, so there is nothing to \
                 branch a work tree from. Make the first commit, or set this step to the \
                 project folder",
            ),
            Self::Git(said) => write!(formatter, "git could not make a work tree here: {said}"),
            Self::Copying(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Trouble {}

/// Czy `project` jest korzeniem repozytorium — nie „czy leży w jakimś".
///
/// `--show-toplevel`, nie `--is-inside-work-tree`: katalog biegu leży POD projektem, więc
/// drugie pytanie odpowiada „tak" także o cudzym drzewie i cała ta funkcja kłamałaby o folderze
/// wewnątrz czyjegoś repo.
#[must_use]
pub fn is_a_repo(project: &Path) -> bool {
    let Some(top) = git(project, &["rev-parse", "--show-toplevel"]).ok() else {
        return false;
    };
    match (fs::canonicalize(top.trim()), fs::canonicalize(project)) {
        (Ok(top), Ok(here)) => top == here,
        _ => false,
    }
}

/// Robi krokowi własne drzewo w `dest`.
///
/// `dest` jeszcze nie istnieje — `git worktree add` wymaga, żeby nie istniał, a kopia i tak
/// zakłada go sama.
pub fn make(project: &Path, dest: &Path, branch: &str) -> Result<Made, Trouble> {
    if !is_a_repo(project) {
        copy_tree(project, dest).map_err(Trouble::Copying)?;
        return Ok(Made {
            how: How::Copy,
            left_behind: Vec::new(),
        });
    }

    // Repozytorium bez commita nie ma `HEAD`, więc nie ma z czego odbić drzewa. Sprawdzamy to
    // ZANIM cokolwiek powstanie: odmowa po założeniu katalogu zostawia śmieć po nieudanym biegu.
    if git(project, &["rev-parse", "--verify", "HEAD"]).is_err() {
        return Err(Trouble::NoCommitYet);
    }

    git(
        project,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            branch,
            &dest.display().to_string(),
            "HEAD",
        ],
    )
    .map_err(Trouble::Git)?;

    // NIESCOMMITOWANA PRACA JEDZIE Z CZŁOWIEKIEM. Drzewo z samego `HEAD` pokazuje agentowi stan
    // sprzed jego zmian, więc agent pisze przeciwko wersji, której już nie ma — a konflikt widać
    // dopiero przy scalaniu. `--binary`, bo różnica bez tego gubi pliki nietekstowe po cichu.
    let diff = git(project, &["diff", "--binary", "HEAD"]).map_err(Trouble::Git)?;
    if !diff.trim().is_empty() {
        apply(dest, &diff).map_err(Trouble::Git)?;
    }

    // Plików nieśledzonych git nie zna, więc drzewo ich nie niesie. Wołający ma o nich
    // POWIEDZIEĆ — patrz [`Made::left_behind`].
    let left_behind = git(project, &["ls-files", "--others", "--exclude-standard"])
        .map_err(Trouble::Git)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        // NASZYCH WŁASNYCH PLIKÓW NIE MELDUJEMY CZŁOWIEKOWI JAKO JEGO BRAKÓW. Zmierzone
        // 2026-08-19 na `~/Projects/meetnotes`: `.loadout/` nie jest tam w `.gitignore`, więc
        // ze 188 plików nieśledzonych **171 to zawartość katalogu poprzedniego biegu**. Bez
        // tego wiersza filtru pierwsze, co człowiek czyta po naciśnięciu Start, to pięć nazw
        // z `work/s_1` sprzed godziny — a lista rośnie z każdym biegiem, bo każdy zostawia
        // swoje drzewo w tym samym miejscu.
        .filter(|line| !line.starts_with(OURS))
        .map(str::to_owned)
        .collect();

    Ok(Made {
        how: How::Tree {
            branch: branch.to_owned(),
        },
        left_behind,
    })
}

/// Co zostało po kroku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    /// Praca jest i siedzi na tej gałęzi.
    OnABranch(String),
    /// Krok nic nie zmienił, więc nie zostało nic.
    Nothing,
}

/// Zamyka drzewo kroku: commit, kiedy jest co zapisać, sprzątanie, kiedy nie ma.
///
/// **Krok, który nic nie zmienił, nie ma prawa zostawić gałęzi.** Po tygodniu biegów `git
/// branch` byłby nie do przeczytania, a gałęzie niosące pracę ginęłyby wśród pustych.
pub fn finish(project: &Path, dest: &Path, branch: &str, message: &str) -> Kept {
    let dirty = git(dest, &["status", "--porcelain"]).is_ok_and(|said| !said.trim().is_empty());

    if dirty {
        // `add -A` bierze też pliki nowe: praca agenta to zwykle nowy plik, a commit bez nich
        // byłby gałęzią, która niczego nie niesie.
        if git(dest, &["add", "-A"]).is_ok()
            && git(dest, &["commit", "--quiet", "--no-verify", "-m", message]).is_ok()
        {
            return Kept::OnABranch(branch.to_owned());
        }
        // Commit się nie udał: drzewo zostaje na dysku razem z pracą. Sprzątanie w tym miejscu
        // byłoby jedyną operacją w tym module, która umie stracić czyjąś robotę.
        tracing::warn!(
            branch,
            "the step's work could not be committed; leaving the tree in place"
        );
        return Kept::OnABranch(branch.to_owned());
    }

    // Nic się nie zmieniło — zdejmujemy drzewo i gałąź. Błędy tu tylko logujemy: bieg jest już
    // po wszystkim, a nieusunięty katalog nie jest powodem, żeby zepsuć jego wynik.
    if let Err(said) = git(
        project,
        &["worktree", "remove", "--force", &dest.display().to_string()],
    ) {
        tracing::debug!(said, "the empty work tree could not be removed");
    }
    if let Err(said) = git(project, &["branch", "-D", branch]) {
        tracing::debug!(said, "the empty branch could not be removed");
    }
    Kept::Nothing
}

/// Kopiuje drzewo projektu do katalogu roboczego kroku — **każdy kształt pliku przeżywa**.
///
/// Trzy reguły i każda ma zmierzony powód:
///
/// - **Decyzja po `symlink_metadata`, nie po `metadata`.** Ta druga podąża za dowiązaniem, więc
///   dowiązanie do katalogu wpadałoby do gałęzi katalogu i wciągało cudze drzewo do kopii.
/// - **Dowiązanie odtwarzamy jako dowiązanie.** Kopiowanie jego celu wciąga do każdej kopii
///   każdego kroku cały katalog po drugiej stronie — w zmierzonym przypadku drugie repozytorium.
/// - **Czego nie da się skopiować, tego nie tykamy.** Kolejka FIFO, gniazdo i urządzenie nie są
///   danymi projektu, a `fs::copy` na kolejce blokuje się na zawsze.
pub fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if NOT_COPIED.iter().any(|skip| name == *skip) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        // `file_type()` z `DirEntry` NIE podąża za dowiązaniem i o to tu chodzi.
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let target = fs::read_link(&from)?;
            // Jedyne wołanie platformowe w tym module mieszka w `engine::supervisor`
            // (niezmiennik 3), więc tutaj jest już tylko decyzja: dowiązanie zostaje
            // dowiązaniem.
            crate::engine::supervisor::link(&target, &to)?;
        } else if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to)?;
        } else {
            // Kolejka, gniazdo, urządzenie. Pomijamy w ciszy wobec biegu i głośno wobec
            // dziennika: to nie są pliki projektu, a odmowa na nich zatrzymywała każdy bieg
            // w folderze, w którym stały.
            tracing::debug!(path = %from.display(), "this is not a file, a folder or a link; the step's copy does not carry it");
        }
    }
    Ok(())
}

/// Nazwa gałęzi dla kroku: `loadout/<bieg>/<krok>`.
///
/// Wymienia i bieg, i krok, bo człowiek czytający `git branch` dzień później nie ma z czego
/// odtworzyć ani jednego, ani drugiego.
#[must_use]
pub fn branch_for(run: &str, step: &str) -> String {
    format!("loadout/{run}/{step}")
}

/// Wołanie gita z tożsamością podaną na miejscu.
///
/// Tożsamość jest tu, bo commit kroku nie ma prawa zależeć od tego, czy ktoś ustawił
/// `user.email` na tej maszynie — a `commit.gpgsign` wyłączamy, bo podpisywanie czeka na hasło
/// i wieszałoby bieg bez jednego słowa na ekranie.
fn git(at: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["-c", "user.name=Loadout"])
        .args(["-c", "user.email=loadout@localhost"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `git apply` z różnicą na stdin.
///
/// Różnica jedzie **kopertą**, nie argumentem: plik pośredni w katalogu biegu byłby czwartą
/// rzeczą do posprzątania, a argv widzi każdy `ps` na maszynie.
fn apply(at: &Path, diff: &str) -> Result<(), String> {
    use std::io::Write;

    let mut child = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["apply", "--binary", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "git apply took no input".to_owned())?
        .write_all(diff.as_bytes())
        .map_err(|error| error.to_string())?;
    let out = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(())
}
