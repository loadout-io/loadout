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
/// Czy w tym repozytorium jest coś, co ta nazwa wskazuje.
///
/// Pytanie zadane PRZED `git worktree add`: nieistniejący punkt startu odmawia całego biegu,
/// a gałąź po skasowanym biegu znika w normalnym trybie pracy. `^{commit}` żąda commitu, więc
/// nazwa wskazująca na drzewo albo na tag adnotowany nie przejdzie tu jako punkt startu.
#[must_use]
pub fn names_a_commit(project: &Path, name: &str) -> bool {
    git(
        project,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{name}^{{commit}}"),
        ],
    )
    .is_ok()
}

pub fn make(project: &Path, dest: &Path, branch: &str) -> Result<Made, Trouble> {
    make_from(project, dest, branch, "HEAD")
}

/// To samo, ale drzewo odbija się od WSKAZANEGO punktu, nie od `HEAD`.
///
/// # Po co to istnieje
///
/// 2026-08-23, zmierzone na biegu właściciela. Wznowienie z historii niesie przekazania
/// poprzedniego biegu, a **nie niosło jego pracy**: świeża kopia powstawała z `HEAD`, więc krok
/// „Front" dostawał czysty checkout i przepisywał od zera 164 pliki, które poprzedni bieg
/// zacommitował na swojej gałęzi — a sędzia obok orzekał na pustym drzewie i pisał „nie mam czego
/// porównywać". Praca poprzedniego biegu leży na `loadout/<tamten bieg>/<kafelek>` i to jest
/// punkt, od którego wznowienie ma zacząć.
///
/// # Niescommitowana praca człowieka DALEJ jedzie z nim, i dalej liczona od `HEAD`
///
/// Bo to jest różnica względem tego, co człowiek ma u siebie, a nie względem cudzej gałęzi.
/// Kiedy jego zmiany dotykają tych samych plików, co poprzedni bieg, `git apply` ODMAWIA —
/// i to jest poprawne zachowanie: cicha trójstronna scalanka zostawiłaby w drzewie znaczniki
/// konfliktu, na których agent pracowałby jak na kodzie. Odmowa jest głośna i zatrzymuje bieg
/// przed pierwszym procesem.
pub fn make_from(project: &Path, dest: &Path, branch: &str, from: &str) -> Result<Made, Trouble> {
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
            from,
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
    /// Praca jest, siedzi na tej gałęzi — i tylko na niej. Katalog, w którym powstała, jest
    /// sprzątnięty.
    OnABranch(String),
    /// Pracy nie dało się zapisać na gałąź, więc katalog z nią **zostaje**.
    ///
    /// Zdanie jest tu, a nie u wołającego, bo tylko ten moduł wie, co dokładnie odmówiło —
    /// i tylko on zna ścieżkę, pod którą ta praca leży. Wołający ma je zapisać tam, gdzie
    /// człowiek czyta o biegu.
    LeftInPlace {
        /// Gałąź, na którą ta praca miała trafić.
        branch: String,
        /// Jedno zdanie dla człowieka: co się nie udało i gdzie w takim razie leży jego praca.
        why: String,
    },
    /// Krok nic nie zmienił, więc nie zostało nic.
    Nothing,
}

/// Zamyka drzewo kroku: commit i sprzątanie, kiedy jest co zapisać, samo sprzątanie, kiedy nie ma.
///
/// **Krok, który nic nie zmienił, nie ma prawa zostawić gałęzi.** Po tygodniu biegów `git
/// branch` byłby nie do przeczytania, a gałęzie niosące pracę ginęłyby wśród pustych.
///
/// # 2026-08-23 (T-95) — KATALOG ZNIKA TAKŻE PO KROKU, KTÓRY COŚ ZMIENIŁ
///
/// Do tego dnia drzewo z pracą zostawało na dysku razem z pełnym checkoutem repozytorium.
/// Zmierzone u właściciela: dziesięć biegów na jednym monorepo zostawiło kilkadziesiąt
/// katalogów `work/s_*`, każdy z osobną kopią całego drzewa, dla zadania, które tego
/// repozytorium nie dotykało — bo „look only" nie znaczy „nie zapisze zrzutu ekranu", a jeden
/// nowy plik to już zmiana.
///
/// Obietnica z T-52 brzmi: praca jest po biegu **osiągalna z gita**, i gałąź spełnia ją
/// w całości. Katalog nie dokłada do niej nic poza miejscem na dysku i wpisem w rejestrze gita.
/// Wznowienie też na tym nie traci: punkt startu bierze się z GAŁĘZI, nie z katalogu
/// (`commands::run::where_it_left_off`).
///
/// **Sprzątamy dopiero PO udanym zapisie i nigdy przed nim.** Kolejność jest tu całą treścią:
/// katalog skasowany przed commitem, który się nie uda, jest jedyną operacją w tym module,
/// która umie stracić czyjąś robotę.
pub fn finish(project: &Path, dest: &Path, branch: &str, message: &str) -> Kept {
    let dirty = git(dest, &["status", "--porcelain"]).is_ok_and(|said| !said.trim().is_empty());

    if dirty {
        if let Err(said) = save(dest, message) {
            // Commit się nie udał: drzewo zostaje na dysku razem z pracą, a bieg ma o tym
            // powiedzieć. Cicha strata jest tu najgorszym możliwym kształtem: bieg wygląda na
            // udany, a jedyna kopia pracy leży poza gitem, w katalogu, którego nikt nie szuka.
            tracing::warn!(
                branch,
                said,
                "the step's work could not be saved on its branch; leaving the folder in place"
            );
            return Kept::LeftInPlace {
                branch: branch.to_owned(),
                why: could_not_save(branch, dest, &said),
            };
        }
        // Praca jest na gałęzi, więc w katalogu nie ma już niczego, czego git nie zna.
        // Zdejmujemy go RĘKAMI GITA, nie `remove_dir_all`: samo skasowanie plików zostawia wpis
        // w rejestrze drzew, a taki wpis odmawia potem założenia drzewa pod tą samą ścieżką.
        // Błąd tylko logujemy — praca jest już bezpieczna, a nieusunięty katalog nie jest
        // powodem, żeby zepsuć wynik biegu.
        if let Err(said) = remove_tree(project, dest) {
            tracing::debug!(
                said,
                "the work folder could not be removed after the commit"
            );
        }
        return Kept::OnABranch(branch.to_owned());
    }

    // Nic się nie zmieniło — zdejmujemy drzewo i gałąź. Błędy tu tylko logujemy: bieg jest już
    // po wszystkim, a nieusunięty katalog nie jest powodem, żeby zepsuć jego wynik.
    if let Err(said) = remove_tree(project, dest) {
        tracing::debug!(said, "the empty work folder could not be removed");
    }
    if let Err(said) = git(project, &["branch", "-D", branch]) {
        tracing::debug!(said, "the empty branch could not be removed");
    }
    Kept::Nothing
}

/// Zapisuje wszystko, co jest w drzewie, jako commit na jego gałęzi.
///
/// `add -A` bierze też pliki nowe: praca agenta to zwykle nowy plik, a commit bez nich byłby
/// gałęzią, która niczego nie niesie.
fn save(dest: &Path, message: &str) -> Result<(), String> {
    git(dest, &["add", "-A"])?;
    git(dest, &["commit", "--quiet", "--no-verify", "-m", message])?;
    Ok(())
}

/// Zdejmuje drzewo robocze razem z jego wpisem w rejestrze.
///
/// `--force`, bo katalog kroku niesie też to, czego git nie śledzi — wynik builda, cache
/// pakietów — a bez tej flagi `worktree remove` odmawia na pierwszym takim pliku. Praca, o którą
/// tu chodzi, jest w tym momencie już na gałęzi; reszta jest odtwarzalna jedną komendą.
fn remove_tree(project: &Path, dest: &Path) -> Result<(), String> {
    git(
        project,
        &["worktree", "remove", "--force", &dest.display().to_string()],
    )
    .map(|_| ())
}

/// Zdanie o pracy, która nie doszła na gałąź — **ze ścieżką**.
///
/// Ścieżka jest treścią, nie ozdobą: bez niej człowiek dowiaduje się, że coś poszło nie tak,
/// i musi sam znaleźć katalog wśród kilkudziesięciu innych. Powód od gita bierzemy pierwszym
/// wierszem, bo `git` odpowiada tu akapitem, a reszta akapitu mówi to samo dłużej.
fn could_not_save(branch: &str, dest: &Path, said: &str) -> String {
    let first = said.lines().next().unwrap_or("").trim();
    format!(
        "Loadout could not put this step's work on the branch \"{branch}\" ({first}), so the \
         folder it worked in was left exactly as it is: {}",
        dest.display()
    )
}

/// Czy w tym drzewie **cokolwiek się wydarzyło** — niezacommitowana zmiana albo commit ponad bazą.
///
/// 2026-08-22 — POWSTAŁO DLA PĘTLI, na prośbę właściciela: „jak backend nie ma czego
/// implementować, to żeby bez sensu się nie odbijać". Sędzia pętli, który uczciwie mówi „nie ma
/// czego sprawdzać", nie ma dziś jak tego powiedzieć — jedynym wyjściem z pętli jest werdykt
/// `pass`, więc odbija się tyle razy, ile ma tur, i pada. Kara za uczciwość, płacona prawdziwymi
/// procesami i prawdziwymi tokenami.
///
/// **Pytamy gita, nie agenta**, i to jest cały wybór tej funkcji. Deklaracja „nic nie zmieniłem"
/// jest tym, co agent powiedział; diff jest tym, co się stało — a na tej różnicy stoi cały ten
/// produkt (`docs/research/projects/00-SYNTHESIS.md` §2.1). Modelowi nie da się tego ograć.
///
/// **Dwa pytania, nie jedno.** Sama `status --porcelain` wystarcza tylko dopóki krok niczego nie
/// zacommitował — a implementer, który commituje swoją pracę na własną gałąź, zostawia drzewo
/// czyste i pracę zrobioną. Zmierzone na biegu właściciela: `Front` zacommitował `605fa3e5`
/// i `status` był po nim pusty.
///
/// **Wątpliwość znaczy „wydarzyło się".** Kiedy git nie odpowiada, oddajemy `true`, bo pominięta
/// weryfikacja jest droższa od jednej niepotrzebnej rundy: pierwsze przepuszcza pracę, której
/// nikt nie sprawdził, drugie kosztuje minutę.
#[must_use]
pub fn touched(project: &Path, dest: &Path) -> bool {
    if git(dest, &["status", "--porcelain"]).is_ok_and(|said| !said.trim().is_empty()) {
        return true;
    }
    let Ok(base) = git(project, &["rev-parse", "HEAD"]) else {
        return true;
    };
    let range = format!("{}..HEAD", base.trim());
    git(dest, &["rev-list", "--count", &range]).map_or(true, |said| said.trim() != "0")
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

/// Gałęzie, których nazwa zaczyna się od tego przedrostka.
///
/// 2026-08-23 (T-95) — PO CO TO ISTNIEJE. Po sprzątaniu katalogów po biegu zostaje sama gałąź,
/// i to jest dobra strona umowy. Zła jest taka, że gałęzie zostają na ZAWSZE: nic ich nie
/// listuje, nic nie umie ich zdjąć poza ręcznym `git branch -D` na każdą z osobna. Historia
/// biegu pyta tędy o swoje własne.
///
/// `--format=%(refname:short)`, a nie gołe `git branch --list`: to drugie maluje gwiazdkę przy
/// gałęzi wyjętej do pracy i wcina resztę, więc czytanie jego wyjścia zaczyna się od zdejmowania
/// ozdób, których nie ma w żadnej nazwie.
///
/// Pusta lista, kiedy git nie odpowiada — pytanie „co ten bieg zostawił" nie ma prawa przewrócić
/// odczytu historii (niezmiennik 5).
#[must_use]
pub fn branches_under(project: &Path, prefix: &str) -> Vec<String> {
    let pattern = format!("{prefix}*");
    git(
        project,
        &["branch", "--list", &pattern, "--format=%(refname:short)"],
    )
    .map(|said| {
        said.lines()
            .map(str::trim)
            // Wzorzec gita jest globem, a nasz przedrostek napisem: warunek sprawdzamy jeszcze
            // raz u siebie, żeby ta funkcja oddawała dokładnie to, co obiecuje jej nazwa.
            .filter(|line| line.starts_with(prefix))
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

/// Gałęzie wyjęte W TEJ CHWILI do pracy w jakimkolwiek drzewie tego repozytorium.
///
/// Czytane z `--porcelain`, bo tam każdy fakt stoi w osobnym wierszu o stałym kształcie
/// (`branch refs/heads/<nazwa>`). Zwykłe `worktree list` skleja ścieżkę, skrót commita i nazwę
/// gałęzi w jeden wiersz do czytania okiem.
///
/// Pusta lista, kiedy git nie odpowiada, i tu jest to wybór ostrożny w złą stronę — dlatego
/// wołający ma tę odpowiedź traktować jako „nie wiem o nikim", a nie jako zgodę.
#[must_use]
pub fn branches_in_use(project: &Path) -> Vec<String> {
    git(project, &["worktree", "list", "--porcelain"])
        .map(|said| {
            said.lines()
                .filter_map(|line| line.trim().strip_prefix("branch refs/heads/"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Zdejmuje gałąź. Niesie zdanie gita, bo ono jest konkretniejsze niż nasze.
///
/// `-D`, nie `-d`: gałąź biegu nigdy nie jest wmergowana nigdzie, więc `-d` odmawiałby każdej
/// i przycisk „zapomnij o nich" nie zdejmowałby ani jednej. Kto tego naciska, wie, że praca
/// zniknie — i po to nacisnął.
pub fn drop_branch(project: &Path, branch: &str) -> Result<(), String> {
    git(project, &["branch", "-D", branch]).map(|_| ())
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
