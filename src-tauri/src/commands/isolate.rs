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

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Czego nie wnosimy do własnego drzewa, kiedy kopiujemy folder bez gita.
///
/// `.loadout` jest obowiązkowy, nie kosmetyczny: katalog biegu leży pod
/// `<projekt>/.loadout/runs/<…>/work/<krok>`, więc kopiowanie projektu do siebie samego
/// schodziłoby w nieskończoność. Pozostałe trzy są wyborem po stronie CZASU — `.git` dużego
/// repozytorium to gigabajty, `node_modules` i `target` odtwarza się jedną komendą.
///
/// Przy drzewie gita ta lista nie jest potrzebna: git sam nie niesie tego, czego nie śledzi.
///
/// 2026-08-29 — WIDOCZNA DLA MODUŁU OBOK, bo składanie kopii ([`super::fan_in`]) obchodzi
/// dokładnie te same drzewa i musi pomijać dokładnie te same nazwy (niezmiennik 13). Druga lista
/// tam znaczyłaby, że `.git` drzewa roboczego — a jest tam PLIKIEM ze ścieżką do rejestru, więc
/// w każdej kopii innym — czyta się jako plik, na którym dwa kroki się nie zgadzają.
pub(super) const NOT_COPIED: [&str; 4] = [".git", ".loadout", "node_modules", "target"];

/// Nasz własny katalog w projekcie (`docs/ARCHITECTURE.md` §8).
///
/// Ukośnik na końcu jest treścią: bez niego wzorzec łapałby też plik o nazwie zaczynającej się
/// od `.loadout`, którego nie zostawiliśmy tam my.
const OURS: &str = ".loadout/";

/// Ile nowych plików może wnieść commit kroku, zanim największe katalogi zostaną poza nim.
const HOW_MUCH_UNTRACKED_FITS_IN_A_COMMIT: u64 = 50 * MEBIBYTE;
const MEBIBYTE: u64 = 1024 * 1024;

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

/// Czy `project` leży WEWNĄTRZ repozytorium, ale nie jest jego korzeniem.
///
/// Dokładna negacja [`is_a_repo`] w obrębie tego jednego pytania, i to jest cały powód, dla
/// którego stoi tu, obok tamtej, a nie u wołającego: oba pytania czytają to samo wyjście gita,
/// a rozjazd między nimi znaczyłby folder, o którym Loadout mówi „nie repozytorium" i zaraz potem
/// „podfolder repozytorium".
///
/// 2026-09 (Z-9) — POWSTAŁO DLA JEDNEGO ZDANIA PRZY STARCIE. Folder wewnątrz cudzego
/// repozytorium przechodzi przez [`make_from_after_add`] gałęzią kopii plikowej, bo `is_a_repo`
/// odpowiada o nim „nie" — a to znaczy, że praca kroku nie ląduje na żadnej gałęzi i nie ma
/// drogi powrotnej do gita. Do dziś nic tego nie mówiło: człowiek wybierał podkatalog swojego
/// monorepo i dostawał bieg, który wygląda dokładnie jak bieg w korzeniu.
#[must_use]
pub fn inside_a_repo_but_not_its_root(project: &Path) -> bool {
    let Ok(top) = git(project, &["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    match (fs::canonicalize(top.trim()), fs::canonicalize(project)) {
        (Ok(top), Ok(here)) => top != here,
        // „Nie wiem" znaczy tu „nie mów nic". Zdanie o podfolderze postawione nad folderem,
        // którego ścieżki nie da się rozwiązać, jest zdaniem o czymś, czego nie sprawdziliśmy.
        _ => false,
    }
}

/// Zdejmuje z rejestru wpisy o drzewach, których katalogów już nie ma.
///
/// 2026-09 (Z-9) — DRUGA POŁOWA SPRZĄTANIA, KTÓREJ NIE ROBIŁO NIC. `worktree remove` zdejmuje
/// katalog razem z jego wpisem, ale katalog skasowany czyjąkolwiek inną ręką — `rm -rf` człowieka,
/// przerwany bieg, kopia projektu przeniesiona na inny dysk — zostawia wpis, którego nie widać
/// nigdzie poza `git worktree list`. Taki wpis ODMAWIA założenia drzewa pod tą samą ścieżką, więc
/// jest nie tylko śmieciem: jest odmową następnego biegu tego kroku. Zmierzone u właściciela
/// 2026-09-02 na `urc-monorepo`: 89 wpisów przy 87 istniejących katalogach.
///
/// `prune` sam z siebie nie tyka drzewa, które stoi na dysku — decyduje o tym git, a nie my.
pub fn prune_trees(project: &Path) -> Result<(), String> {
    git(project, &["worktree", "prune"]).map(|_| ())
}

/// Dopisuje `.loadout/` do listy, którą czyta wyłącznie git tego projektu — idempotentnie.
///
/// # Dlaczego `.git/info/exclude`, a NIE `.gitignore`
///
/// 2026-09 (Z-9). `.gitignore` jest plikiem CZŁOWIEKA i jest w jego commicie: dopisanie tam
/// czegokolwiek jest zmianą, której nie zamawiał, i wyjeżdża w jego następnym commicie do jego
/// współpracowników. `info/exclude` jest listą prywatną tego jednego klona — dokładnie to samo
/// wyciszenie, zero śladu w historii.
///
/// # Po co to w ogóle jest
///
/// Bo bez tego wszystko, co Loadout zostawia w projekcie, czyta się jako nieśledzona praca
/// człowieka. Zmierzone 2026-08-19 na `~/Projects/meetnotes`: ze 188 plików nieśledzonych
/// **171 było zawartością katalogu poprzedniego biegu**. `make_from_after_add` filtruje je
/// u siebie po [`OURS`], więc do promptu nie jadą — ale `git status`, który człowiek uruchamia
/// sam u siebie, nie zna żadnego naszego filtru.
///
/// Wspólny katalog gita, nie `.git`: w drzewie roboczym `.git` jest PLIKIEM ze wskaźnikiem,
/// a lista wyciszeń jest jedna na całe repozytorium.
pub fn exclude_our_folder(project: &Path) -> Result<(), String> {
    let said = git(project, &["rev-parse", "--git-common-dir"])?;
    let common = Path::new(said.trim());
    // Git oddaje tę ścieżkę WZGLĘDNĄ (zwykle samo `.git`), liczoną od katalogu, w którym go
    // zawołano — a zawołaliśmy go w `project`.
    let common = if common.is_absolute() {
        common.to_path_buf()
    } else {
        project.join(common)
    };
    let info = common.join("info");
    let exclude = info.join("exclude");
    let text = match fs::read_to_string(&exclude) {
        Ok(text) => text,
        // Świeży klon nie ma tego pliku i to jest stan normalny, nie awaria dysku.
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    if text.lines().any(|line| line.trim() == OURS) {
        return Ok(());
    }
    let mut next = text;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(WHY_OURS_IS_HERE);
    next.push_str(OURS);
    next.push('\n');
    fs::create_dir_all(&info).map_err(|error| error.to_string())?;
    replace_in_place(&info, &exclude, &next)
}

/// Zdanie, które człowiek czyta w swoim `info/exclude`, kiedy się w niego zajrzy.
///
/// Po angielsku (D5), bo to jest plik w jego repozytorium, a nie nasza dokumentacja. Wiersz bez
/// wyjaśnienia w cudzym pliku konfiguracji jest zagadką, którą ktoś kiedyś skasuje.
const WHY_OURS_IS_HERE: &str =
    "# Loadout keeps this project's runs here; only this clone sees this line.\n";

/// Zapisuje plik przez plik tymczasowy i `rename`, w tym samym katalogu.
///
/// TĘDY, A NIE `fs::write`, i to jest wymóg, nie ostrożność na zapas: piszemy do CUDZEGO pliku
/// konfiguracji gita. `fs::write` obcina cel przed pierwszym bajtem, więc przerwany zapis
/// zostawia człowiekowi pustą listę wyciszeń zamiast jego własnej. `rename` w obrębie jednego
/// katalogu jest atomowe — czytelnik widzi albo poprzednią treść w całości, albo nową.
fn replace_in_place(dir: &Path, path: &Path, text: &str) -> Result<(), String> {
    let writing = dir.join("exclude.loadout-writing");
    fs::write(&writing, text).map_err(|error| error.to_string())?;
    fs::rename(&writing, path).map_err(|error| {
        // Nieudany `rename` zostawia plik tymczasowy w cudzym `.git/info/`. Zdejmujemy go, bo
        // to jedyny moment, w którym ktokolwiek jeszcze o nim wie.
        let _ = fs::remove_file(&writing);
        error.to_string()
    })
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
    make_from_after_add(project, dest, branch, from, |_| Ok(()))
}

/// Jak [`make_from`], ale oddaje ownership nowego drzewa natychmiast po `git worktree add`.
///
/// 2026-08-28 (T-152): nakładanie WIP i liczenie pominiętych plików nadal może odmówić po
/// utworzeniu drzewa. Callback stoi dokładnie na tej granicy, żeby wołający zdążył zapisać
/// gałąź i katalog w provisional guardzie przed jakimkolwiek następnym fallible krokiem.
pub fn make_from_after_add(
    project: &Path,
    dest: &Path,
    branch: &str,
    from: &str,
    after_add: impl FnOnce(&str) -> io::Result<()>,
) -> Result<Made, Trouble> {
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

    // 2026-08-28 (T-152 review): guard nie może później pytać, na czym *teraz* stoi branch.
    // Rozwiązujemy punkt startu przed pierwszym skutkiem, zakładamy drzewo z dokładnego OID i
    // przekazujemy ten sam OID właścicielowi natychmiast po `add`. To zamyka okno, w którym
    // przesunięty ref mógłby dostać uprawnienie do skasowania cudzego drzewa.
    let commitish = format!("{from}^{{commit}}");
    let head = git(project, &["rev-parse", "--verify", &commitish])
        .map_err(Trouble::Git)?
        .trim()
        .to_owned();
    git(
        project,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            branch,
            &dest.display().to_string(),
            &head,
        ],
    )
    .map_err(Trouble::Git)?;
    after_add(&head).map_err(Trouble::Copying)?;

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

/// Co zostało po kroku i czego przy okazji NIE dało się sprzątnąć albo zapisać.
///
/// Trzy pola, bo to są trzy niezależne fakty o jednym zamknięciu: gdzie jest praca, co po niej
/// zostało wbrew nam i czego świadomie nie zapisaliśmy. Jedno pole musiałoby zgubić któryś z nich.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// Gdzie jest praca tego kroku.
    pub kept: Kept,
    /// Jedno zdanie dla człowieka, kiedy katalog albo gałąź zostały wbrew nam — **ze ścieżką**.
    ///
    /// `None` znaczy „sprzątnięte", nie „nie wiem": obie komendy sprzątające albo się udały,
    /// albo złożyły tu swoje zdanie. Wołający ma je zapisać tam, gdzie człowiek czyta o biegu.
    pub tidied: Option<String>,
    /// Jedno zdanie dla człowieka o ciężkich, nieśledzonych artefaktach poza commitem kroku.
    pub left_behind: Option<String>,
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
///
/// # 2026-09-03 (Z-7) — CZYSTE DRZEWO NIE ZNACZY „NIC SIĘ NIE STAŁO"
///
/// Do tego dnia całe pytanie brzmiało `status --porcelain`, a pusta odpowiedź prowadziła prosto
/// do `branch -D`. Agent, który sam zacommitował swoją pracę na gałąź kroku — a to jest normalny
/// tryb pracy implementera — zostawia dokładnie takie drzewo: czyste, bo wszystko już zapisał.
/// Kasowaliśmy mu wtedy gałąź razem z commitami, czyli **jedyną kopię jego pracy**: po `branch -D`
/// nie ma jej w `git log`, nie ma w `git branch` i nie sięga do niej nic poza `git fsck`.
///
/// Pyta się więc dwoma pytaniami, tymi samymi, co [`touched`]: czy jest co commitować i czy nad
/// punktem startu stoi commit. Krok, który zacommitował sam, dostaje `Kept::OnABranch` **bez
/// nowego commita na wierzchu** — pracę ma już zapisaną tak, jak chciał ją zapisać.
///
/// **Wątpliwość znaczy „zostaw gałąź".** `base` nieznany (brak markera, nieczytelny marker) albo
/// git, który nie odpowiada, dają tu ten sam wybór co commit: gałąź zostaje. Odwrotny wybór to ta
/// sama cicha strata, przed którą stoi cały ten moduł — a jedna zbędna gałąź kosztuje wiersz
/// w `git branch`.
pub fn finish(
    project: &Path,
    dest: &Path,
    branch: &str,
    message: &str,
    base: Option<&str>,
) -> Closed {
    let dirty = git(dest, &["status", "--porcelain"]).is_ok_and(|said| !said.trim().is_empty());

    if dirty {
        let left_behind = match save(dest, message) {
            Ok(left_behind) => left_behind,
            Err(said) => {
                // Commit się nie udał: drzewo zostaje na dysku razem z pracą, a bieg ma o tym
                // powiedzieć. Cicha strata jest tu najgorszym możliwym kształtem: bieg wygląda na
                // udany, a jedyna kopia pracy leży poza gitem, w katalogu, którego nikt nie szuka.
                tracing::warn!(
                    branch,
                    said,
                    "the step's work could not be saved on its branch; leaving the folder in place"
                );
                return Closed {
                    kept: Kept::LeftInPlace {
                        branch: branch.to_owned(),
                        why: could_not_save(branch, dest, &said),
                    },
                    tidied: None,
                    left_behind: None,
                };
            }
        };
        // 2026-09 (Z-8): półka albo ciężki katalog mogą być jedyną zmianą. Po ich wyłączeniu
        // nie wolno ani robić pustego commita, ani zostawiać pustej gałęzi jako rzekomej pracy.
        if !committed_over(dest, base) {
            return close_empty_tree(project, dest, branch, left_behind);
        }
        return Closed {
            kept: Kept::OnABranch(branch.to_owned()),
            tidied: tidy_away(project, dest, branch),
            left_behind,
        };
    }

    // Czyste drzewo z commitem ponad punktem startu to praca zapisana ręką agenta. Katalog
    // schodzi jak po naszym własnym commicie, gałąź zostaje nietknięta.
    if committed_over(dest, base) {
        return Closed {
            kept: Kept::OnABranch(branch.to_owned()),
            tidied: tidy_away(project, dest, branch),
            left_behind: None,
        };
    }

    close_empty_tree(project, dest, branch, None)
}

/// Zdejmuje drzewo i gałąź, kiedy po wyłączeniu własnych albo ciężkich plików nie ma pracy.
fn close_empty_tree(
    project: &Path,
    dest: &Path,
    branch: &str,
    left_behind: Option<String>,
) -> Closed {
    let mut left: Vec<String> = tidy_away(project, dest, branch).into_iter().collect();
    if let Err(said) = git(project, &["branch", "-D", branch]) {
        tracing::warn!(branch, said, "the empty branch could not be removed");
        left.push(could_not_tidy(branch, dest, &said));
    }
    Closed {
        kept: Kept::Nothing,
        tidied: (!left.is_empty()).then(|| left.join(" ")),
        left_behind,
    }
}

/// Czy na tej gałęzi stoi commit, którego nie było w punkcie startu drzewa.
///
/// Drugie z dwóch pytań [`touched`], zadane tu z tego samego powodu: agent, który commituje sam,
/// zostawia czyste drzewo i zrobioną pracę. Punkt startu bierze się z markera TEGO kroku, a nie
/// z `HEAD` projektu — krok wznowiony odbija się od gałęzi poprzedniego biegu
/// (`commands::run::where_it_left_off`), więc `HEAD` odpowiadałby tu na inne pytanie i liczył
/// cudze commity jako jego.
///
/// **Wątpliwość znaczy „tak".** Powód stoi przy [`finish`]: pomyłka w tę stronę zostawia jedną
/// gałąź za dużo, w drugą — kasuje czyjąś pracę.
fn committed_over(dest: &Path, base: Option<&str>) -> bool {
    let Some(base) = base else {
        return true;
    };
    let range = format!("{base}..HEAD");
    git(dest, &["rev-list", "--count", &range]).map_or(true, |said| said.trim() != "0")
}

/// Zdejmuje katalog kroku po tym, jak praca jest już bezpieczna — i oddaje ZDANIE, kiedy się nie
/// da.
///
/// Zdejmujemy go RĘKAMI GITA, nie `remove_dir_all`: samo skasowanie plików zostawia wpis
/// w rejestrze drzew, a taki wpis odmawia potem założenia drzewa pod tą samą ścieżką.
///
/// 2026-09-03 (Z-7) — WARN I ZDANIE, NIE `debug!`. Nieusunięty katalog nadal nie jest powodem,
/// żeby zepsuć wynik biegu — ale jest powodem, żeby o nim powiedzieć: niesie pełny checkout
/// repozytorium i blokuje następny bieg pod tą samą ścieżką, a dziennika debugowego nie czyta
/// nikt. Do dziś człowiek czytał zielony bieg nad katalogiem, o którym nic mu nie powiedziało.
fn tidy_away(project: &Path, dest: &Path, branch: &str) -> Option<String> {
    let said = remove_tree(project, dest).err()?;
    tracing::warn!(
        branch,
        said,
        "the work folder could not be removed after the step"
    );
    Some(could_not_tidy(branch, dest, &said))
}

/// Zapisuje pracę z drzewa jako commit na jego gałęzi.
///
/// `add -A` bierze też pliki nowe, ale dwa rodzaje nie należą do wyniku: półka umiejętności,
/// którą położył Loadout, oraz największe wpisy pierwszego poziomu po przekroczeniu 50 MiB.
/// Oba wyłączenia powstają tutaj, bo drugi zestaw reguł obok adaptera byłby pierwszym, który
/// przestanie obowiązywać (niezmiennik 23).
fn save(dest: &Path, message: &str) -> Result<Option<String>, String> {
    let (heavy, left_behind) = what_does_not_belong_in_the_commit(dest)?;
    let mut add = vec![
        "add".to_owned(),
        "-A".to_owned(),
        "--".to_owned(),
        ".".to_owned(),
        format!(
            ":(exclude,literal){}",
            crate::skills::SHELF_THE_OTHER_FIVE_READ
        ),
    ];
    add.extend(
        heavy
            .iter()
            .map(|entry| format!(":(exclude,literal){}", entry.name)),
    );
    let args: Vec<&str> = add.iter().map(String::as_str).collect();
    git(dest, &args)?;

    // 2026-09 (Z-8): kiedy jedyną zmianą była półka albo duży katalog, `git commit` odmawia
    // zdaniem „nothing to commit". To nie jest awaria zapisu pracy, bo pracy do zapisu nie ma.
    if git(dest, &["diff", "--cached", "--name-only"])?
        .trim()
        .is_empty()
    {
        return Ok(left_behind);
    }
    git(dest, &["commit", "--quiet", "--no-verify", "-m", message])?;
    Ok(left_behind)
}

#[derive(Debug)]
struct UntrackedTop {
    name: String,
    bytes: u64,
    is_dir: bool,
}

/// Największe nowe wpisy pierwszego poziomu, które sprowadzą resztę pod limit, i zdanie o nich.
fn what_does_not_belong_in_the_commit(
    dest: &Path,
) -> Result<(Vec<UntrackedTop>, Option<String>), String> {
    let shelf = Path::new(crate::skills::SHELF_THE_OTHER_FIVE_READ);
    let mut by_top: BTreeMap<String, (u64, bool)> = BTreeMap::new();
    let mut total = 0_u64;

    // `-z`, bo nazwa pliku może zawierać znak nowej linii. Dzielenie po wierszach zmieniłoby
    // wtedy jeden plik w dwa nieistniejące i policzyło inny zbiór niż późniejsze `git add`.
    for name in git(dest, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split('\0')
        .filter(|name| !name.is_empty())
    {
        let relative = Path::new(name);
        if relative.starts_with(shelf) {
            // Półka jest własnym plikiem Loadouta, nie pracą ani artefaktem agenta. Zawsze ma
            // osobny pathspek i nie może sama przepchnąć prawdziwej pracy ponad limit.
            continue;
        }
        let metadata = fs::symlink_metadata(dest.join(relative))
            .map_err(|error| format!("could not measure {name}: {error}"))?;
        let Some(top) = relative.components().next() else {
            continue;
        };
        let top = top.as_os_str().to_string_lossy().into_owned();
        let bytes = metadata.len();
        total = total.saturating_add(bytes);
        let entry = by_top
            .entry(top.clone())
            .or_insert_with(|| (0, dest.join(&top).is_dir()));
        entry.0 = entry.0.saturating_add(bytes);
    }

    if total <= HOW_MUCH_UNTRACKED_FITS_IN_A_COMMIT {
        return Ok((Vec::new(), None));
    }

    let mut biggest: Vec<UntrackedTop> = by_top
        .into_iter()
        .map(|(name, (bytes, is_dir))| UntrackedTop {
            name,
            bytes,
            is_dir,
        })
        .collect();
    biggest.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.name.cmp(&right.name))
    });

    let mut kept = total;
    let mut heavy = Vec::new();
    for entry in biggest {
        if kept <= HOW_MUCH_UNTRACKED_FITS_IN_A_COMMIT {
            break;
        }
        kept = kept.saturating_sub(entry.bytes);
        heavy.push(entry);
    }

    let bytes = heavy
        .iter()
        .fold(0_u64, |sum, entry| sum.saturating_add(entry.bytes));
    let names = heavy
        .iter()
        .map(|entry| {
            if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let megabytes = bytes.saturating_add(MEBIBYTE - 1) / MEBIBYTE;
    let limit = HOW_MUCH_UNTRACKED_FITS_IN_A_COMMIT / MEBIBYTE;
    let left_behind = format!(
        "Loadout left {megabytes} MB from {names} out of this step's commit because a step branch \
         does not carry more than {limit} MB of new files."
    );
    Ok((heavy, Some(left_behind)))
}

/// Zdejmuje drzewo robocze razem z jego wpisem w rejestrze.
///
/// `--force`, bo katalog kroku niesie też to, czego git nie śledzi — wynik builda, cache
/// pakietów — a bez tej flagi `worktree remove` odmawia na pierwszym takim pliku. Praca, o którą
/// tu chodzi, jest w tym momencie już na gałęzi; reszta jest odtwarzalna jedną komendą.
///
/// 2026-09 (Z-46) — PUBLICZNE, bo zamiatacz starych biegów ([`super::sweep`]) zdejmuje katalogi
/// dokładnie tak samo. Własne `remove_dir_all` tam byłoby drugą polityką kasowania (niezmiennik
/// 23), a katalog skasowany bez gita zostawia wpis, który odmawia potem założenia drzewa pod tą
/// samą ścieżką.
pub fn remove_tree(project: &Path, dest: &Path) -> Result<(), String> {
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

/// Zdanie o tym, czego po kroku nie dało się sprzątnąć — **też ze ścieżką**.
///
/// Ta sama zasada, co przy [`could_not_save`], i ten sam powód: bez ścieżki człowiek dowiaduje
/// się, że coś zostało, i musi sam znaleźć jeden katalog wśród kilkudziesięciu innych. Zdanie
/// wymienia oba możliwe leżaki — katalog i gałąź — bo składają je dwie różne komendy, każda ze
/// swoim własnym powodem od gita w nawiasie.
fn could_not_tidy(branch: &str, dest: &Path, said: &str) -> String {
    let first = said.lines().next().unwrap_or("").trim();
    format!(
        "Loadout could not tidy up after this step ({first}), so the folder it worked in, or the \
         branch \"{branch}\" it stands on, is still here: {}",
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
/// produkt (`docs/FOUNDATIONS.md` §2.1). Modelowi nie da się tego ograć.
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

/// Katalogi, które to repozytorium zna jako miejsca pracy — **wszystkie**, razem z korzeniem.
///
/// 2026-09 (Z-46) — POWSTAŁO DLA ZAMIATACZA STARYCH BIEGÓW. Bieg sprzed markera izolacji nie
/// zostawia po sobie ani jednego pliku, po którym dałoby się poznać, że otworzył sobie katalog;
/// jedynym miejscem, w którym ten fakt stoi, jest rejestr gita. Zmierzone u właściciela
/// 2026-09-03 na `urc-monorepo`: dwanaście takich wpisów przy dzienniku meldującym, że wszystko
/// zostało zamknięte.
///
/// Bliźniak [`branches_in_use`] czyta to samo wyjście i pyta o drugą połowę wiersza — tam
/// o gałąź, tu o katalog. Pusta lista, kiedy git nie odpowiada: „nie wiem o żadnym" jest tu
/// odpowiedzią ostrożną, bo wołający na jej podstawie **kasuje**.
#[must_use]
pub fn trees_registered(project: &Path) -> Vec<PathBuf> {
    git(project, &["worktree", "list", "--porcelain"])
        .map(|said| {
            said.lines()
                .filter_map(|line| line.trim().strip_prefix("worktree "))
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Czy w tym katalogu leży zmiana, której nikt nie zapisał. **Wątpliwość znaczy „tak".**
///
/// 2026-09 (Z-46) — TO SAMO PYTANIE, CO PIERWSZA LINIA [`finish`], I ODWROTNA OSTROŻNOŚĆ, bo to
/// jest inne pytanie. Tam odpowiedź brzmi „czy jest co commitować" i git, który nie odpowiada,
/// prowadzi do sprzątania pustego drzewa. Tutaj odpowiedź brzmi „czy wolno ten katalog skasować",
/// a jedyną kopią tego, co ktoś w nim napisał, jest on sam — więc cisza gita ma go zostawić.
#[must_use]
pub fn holds_unsaved_work(dest: &Path) -> bool {
    git(dest, &["status", "--porcelain"]).map_or(true, |said| !said.trim().is_empty())
}

/// Czy na tej gałęzi stoi commit, którego nie ma `HEAD` projektu. **Wątpliwość znaczy „tak".**
///
/// 2026-09 (Z-46) — pytanie zadawane przed `branch -D` na gałęzi po biegu, którego katalogu już
/// nie ma. Praca zapisana ręką agenta żyje wyłącznie na niej: po `branch -D` nie ma jej ani
/// w `git log`, ani w `git branch`, a sięga do niej wyłącznie `git fsck`. Ten sam wybór, co przy
/// [`committed_over`] — pomyłka w jedną stronę zostawia wiersz w `git branch`, w drugą kasuje
/// czyjąś pracę.
#[must_use]
pub fn carries_its_own_work(project: &Path, branch: &str) -> bool {
    let range = format!("HEAD..{branch}");
    git(project, &["rev-list", "--count", &range]).map_or(true, |said| said.trim() != "0")
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

/// Nazwa gałęzi dla katalogu pracy: `loadout/<bieg>/<krok>`.
///
/// Wymienia i bieg, i krok, bo człowiek czytający `git branch` dzień później nie ma z czego
/// odtworzyć ani jednego, ani drugiego.
///
/// 2026-08-24 (T-114) — klucz drugiej kopii ma wewnętrzny separator `~`, którego Git nie
/// dopuszcza w refie. Ref koduje ten sam numer separatorem `-`: `s_2~2` staje się `s_2-2`,
/// a pierwsza kopia `s_2` zostaje co do bajta bez zmian.
#[must_use]
pub fn branch_for(run: &str, work_key: &str) -> String {
    format!(
        "loadout/{run}/{}",
        crate::workflow::check::work_branch_tail(work_key)
    )
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
