//! Praca kilku kroków, zniesiona do JEDNEJ kopii — albo odmowa, kiedy dwa z nich napisały
//! w tym samym pliku co innego.
//!
//! # Dlaczego porównujemy PLIKI, a nie różnicę gita
//!
//! Bo plik, o którym git nie wie, nie ma w niej żadnej reprezentacji. Krok, który zakłada
//! `docs/added.txt` w katalogu, którego wcześniej nie było, zostawia po sobie robotę widoczną
//! wyłącznie na dysku — a to jest najczęstszy kształt pracy agenta, nie przypadek brzegowy.
//! Różnica liczona `git diff` przeniosłaby zmiany w plikach śledzonych i po cichu zgubiła całą
//! resztę, czyli dałaby krokowi poniżej kopię, która WYGLĄDA na złożoną.
//!
//! Bajty porównujemy z **nietkniętą** kopią, do której składamy. Ona jest wspólną bazą, bo
//! powstała tym samym przepisem, co kopie rodziców ([`super::isolate::make_from`]): ten sam
//! punkt startu i ta sama niescommitowana praca człowieka. Więc „ten plik różni się od bazy"
//! znaczy dokładnie „ten krok go zmienił", bez pytania kogokolwiek o deklarację.
//!
//! **Wznowienie jest tu świadomie poza zakresem** (2026-08-29). Kopia wznowionego kafelka odbija
//! się od gałęzi poprzedniego biegu (`commands::run::where_it_left_off`), więc baza kroku
//! składającego i baza jego rodzica mogą się wtedy rozjechać — i to jest osobna sprawa, nie
//! przeoczenie tej.
//!
//! # Cicha wygrana jednej strony jest gorsza od zatrzymanego kroku
//!
//! Kiedy dwa kroki napisały w jednym pliku różne bajty, każde rozstrzygnięcie po naszej stronie
//! jest zgadywaniem: „ostatni wygrywa" zależy od tego, który agent skończył szybciej, a to
//! zmienia się z biegu na bieg. Krok poniżej dostałby wtedy kod, którego nikt nie napisał,
//! i skończyłby się sukcesem. Dlatego niezgoda jest wartością zwracaną **przed** pierwszym
//! zapisem: do katalogu nie idzie ani jeden bajt, a obie kopie zostają tam, gdzie są, żeby
//! człowiek miał gdzie zajrzeć.
//!
//! Znacznik konfliktu w scalanym pliku byłby tą samą wadą o klasę gorzej: agent pracowałby na
//! nim jak na kodzie (ten sam powód stoi przy `git apply` w [`super::isolate`]).
//!
//! # Granica
//!
//! Ten moduł nie zna ani biegu, ani okna: dostaje katalog i listę kopii, oddaje fakt. Kto ma
//! zobaczyć zdanie o niezgodzie, rozstrzyga `commands::run`.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::isolate::NOT_COPIED;

/// Z której próby której pętli jest praca leżąca w kopii.
///
/// 2026-08-29 — ZNACZNIK POCHODZENIA WYNIKA Z `commands::run::node_key_for`, a nie stoi obok
/// niego. Tamten klucz nadaje każdej rundzie własny sufiks (`#N`) i każdej kopii własny (`~N`),
/// bo **klucze muszą się różnić między kopiami i między rundami**; rozłączne katalogi biorą się
/// z drugiej połowy tej samej decyzji. Rozłączny katalog mówi jednak wyłącznie „to nie jest ta
/// sama kopia" — nie mówi ani słowa o tym, którą rundę w sobie ma, a rundy dzielą folder
/// (`work_key_for`). Ta struktura jest tą brakującą połową, wyjętą z tego samego klucza.
///
/// [`Generation::loop_at`] jedzie razem z numerem próby, bo sam numer nie jest pokoleniem:
/// krok spoza pętli ma rundę zero, a sędzia pętli o trzech turach wychodzi rundą drugą — dwie
/// niezależne skale. Porównanie gołych numerów odmawiałoby grafu „zaplanuj raz, pętla obok,
/// potem ktoś to zbiera", czyli zwykłego dnia pracy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Generation {
    /// Która pętla biegu je liczy. Dwie różne pętle mają dwa niezależne liczniki rund.
    pub loop_at: usize,
    /// Która próba, licząc od jedynki — bo to jest zdanie dla człowieka, nie pole danych.
    pub which: u8,
    /// Ile prób ma ta pętla, żeby zdanie umiało powiedzieć „try 2 of 3".
    pub of: u8,
}

/// Kopia jednego kroku, którego praca ma wejść do kopii składanej.
#[derive(Debug, Clone, Copy)]
pub struct Parent<'a> {
    /// Nazwa z kafelka — jedyna rzecz, po której człowiek ten krok rozpozna (niezmiennik 14).
    pub name: &'a str,
    /// Katalog, w którym ten krok pracował.
    pub cwd: &'a Path,
    /// Którą próbę ta kopia w sobie ma. `None` znaczy „ta kopia nie należy do żadnej pętli
    /// albo nikt w niej jeszcze nie pracował" — czyli nie ma z czym się nie zgadzać.
    pub born: Option<Generation>,
}

/// Dlaczego pracy nie da się złożyć. Każdy wariant naprawia się inaczej, więc każdy jest osobnym
/// zdaniem — i każde zdanie mówi, CO Z TYM ZROBIĆ.
#[derive(Debug)]
pub enum Trouble {
    /// Dwie kopie mają dla jednej ścieżki różne bajty.
    TwoAnswers {
        /// Ścieżka względem katalogu kopii — tak, jak człowiek widzi ją w swoim projekcie.
        path: String,
        /// Nazwa kafelka, który napisał pierwszą wersję.
        one: String,
        /// I tego, który napisał drugą.
        other: String,
    },
    /// Dwie kopie tej samej pętli trzymają pracę z DWÓCH różnych prób.
    MixedTries {
        /// Nazwa kafelka, którego kopia stoi na wcześniejszej albo późniejszej próbie.
        one: String,
        /// Którą próbę ta kopia w sobie ma, licząc od jedynki.
        one_try: u8,
        /// Nazwa kafelka po drugiej stronie niezgody.
        other: String,
        /// I jego próba.
        other_try: u8,
        /// Ile prób ma ta pętla.
        of: u8,
    },
    /// Kopii nie dało się przeczytać albo zapisać.
    Reading(io::Error),
}

impl fmt::Display for Trouble {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Nazwy OBU kafelków i ścieżka pliku: bez nich człowiek dostaje zdanie o tym, że
            // coś się nie zgadza, i musi sam znaleźć, co i między kim.
            Self::TwoAnswers { path, one, other } => write!(
                formatter,
                "\"{one}\" and \"{other}\" both changed {path}, and they wrote different text. \
                 Loadout will not pick one of them for you, so this step was not started. \
                 Nothing was overwritten: both of them still have their own copy, so you can \
                 open each one and decide what {path} should say."
            ),
            // Nazwy OBU kafelków i OBIE próby: „these two do not match" zostawiałoby człowieka
            // z pytaniem, które dwie kopie i z których prób — a to jest cała treść tej odmowy.
            Self::MixedTries {
                one,
                one_try,
                other,
                other_try,
                of,
            } => write!(
                formatter,
                "\"{one}\" is holding try {one_try} of {of} and \"{other}\" is holding try \
                 {other_try} of {of}, so the two of them are not from the same round. Loadout \
                 will not fold work from two different tries into one folder, because the folder \
                 that came out would look exactly like work that went together, so this step was \
                 not started. Nothing was overwritten: both of them still have their own copy, \
                 so you can open each one and see what it holds."
            ),
            Self::Reading(error) => write!(
                formatter,
                "Loadout could not bring the work of the steps before this one into one folder: \
                 {error}"
            ),
        }
    }
}

impl std::error::Error for Trouble {}

/// Nakłada na `into` wszystko, co rodzice zmienili względem tego katalogu.
///
/// **Najpierw cała lista, dopiero potem pierwszy zapis.** Kolejność jest tu całą treścią:
/// niezgoda znaleziona w połowie nakładania zostawiłaby katalog w stanie, którego nie opisuje
/// ani jedna kopia — czyli w dokładnie tym, przed czym ta funkcja stoi.
///
/// **Pokolenie pytamy przed pierwszym ODCZYTEM**, nie tylko przed pierwszym zapisem: kopie
/// z dwóch różnych prób nie mają o czym rozmawiać, więc chodzenie po ich drzewach byłoby
/// liczeniem różnicy, której i tak nikt nie użyje.
pub fn fold_the_copies<'a>(into: &Path, parents: &[Parent<'a>]) -> Result<(), Trouble> {
    if let Some(trouble) = a_copy_from_another_try(parents) {
        return Err(trouble);
    }
    let mut changed: BTreeMap<PathBuf, Change<'a>> = BTreeMap::new();
    for parent in parents {
        what_changed(*parent, parent.cwd, Path::new(""), into, &mut changed)?;
    }
    for (path, change) in &changed {
        let to = into.join(path);
        // Katalog, którego w bazie nie było: praca agenta to zwykle nowy plik w nowym miejscu,
        // a `fs::write` sam katalogu nie zakłada.
        if let Some(folder) = to.parent() {
            fs::create_dir_all(folder).map_err(Trouble::Reading)?;
        }
        fs::write(&to, &change.bytes).map_err(Trouble::Reading)?;
    }
    Ok(())
}

/// Pierwsza para kopii, które liczy ta sama pętla, a które trzymają różne próby.
///
/// Porównanie jest PARAMI, a nie „wszystkie równe pierwszej": lista rodziców bywa dłuższa niż
/// dwa, a zdanie dla człowieka ma wymienić dokładnie tę parę, która się nie zgadza.
///
/// Kopia bez pokolenia mija się z każdą: krok spoza pętli biegnie raz i jego praca nie należy
/// do żadnej próby, więc odmowa na nim zatrzymywałaby zwykłe „zaplanuj, potem dwie gałęzie".
fn a_copy_from_another_try(parents: &[Parent<'_>]) -> Option<Trouble> {
    for (at, one) in parents.iter().enumerate() {
        for other in parents.iter().skip(at + 1) {
            let (Some(mine), Some(theirs)) = (one.born, other.born) else {
                continue;
            };
            // Dwie RÓŻNE pętle mają dwa niezależne liczniki prób, więc ich numery nie są
            // porównywalne — powód stoi w całości przy [`Generation`].
            if mine.loop_at != theirs.loop_at || mine.which == theirs.which {
                continue;
            }
            return Some(Trouble::MixedTries {
                one: one.name.to_owned(),
                one_try: mine.which,
                other: other.name.to_owned(),
                other_try: theirs.which,
                of: mine.of,
            });
        }
    }
    None
}

/// Jedna zmiana, którą ktoś proponuje dla jednej ścieżki.
struct Change<'a> {
    /// Kto ją napisał. Trzymane po to, żeby zdanie o niezgodzie umiało wymienić OBU.
    who: &'a str,
    /// Bajty, które mają stanąć w kopii składanej.
    bytes: Vec<u8>,
}

/// Obchodzi jedną kopię i dopisuje do listy wszystko, co różni się od bazy.
///
/// Rekurencja, jak w [`super::isolate::copy_tree`] obok i z tego samego powodu: drzewo projektu
/// jest głębokie na kilkanaście poziomów, a nie na tysiące.
fn what_changed<'a>(
    parent: Parent<'a>,
    at: &Path,
    rel: &Path,
    base: &Path,
    changed: &mut BTreeMap<PathBuf, Change<'a>>,
) -> Result<(), Trouble> {
    for entry in fs::read_dir(at).map_err(Trouble::Reading)? {
        let entry = entry.map_err(Trouble::Reading)?;
        let name = entry.file_name();
        // JEDNA LISTA POMIJANYCH NAZW dla kopiowania i dla składania (niezmiennik 13). `.git`
        // jest tu obowiązkowy, nie kosmetyczny: w drzewie roboczym gita jest PLIKIEM ze ścieżką
        // do rejestru, więc każda kopia ma tam co innego i każde dwie „nie zgadzałyby się".
        if NOT_COPIED.iter().any(|skip| name == *skip) {
            continue;
        }
        let from = entry.path();
        let here = rel.join(&name);
        // `file_type()` z `DirEntry` NIE podąża za dowiązaniem i o to tu chodzi — ta sama
        // decyzja, co w `copy_tree`.
        let kind = entry.file_type().map_err(Trouble::Reading)?;
        if kind.is_dir() {
            what_changed(parent, &from, &here, base, changed)?;
            continue;
        }
        if !kind.is_file() {
            // Dowiązanie, kolejka, gniazdo, urządzenie. Pomijamy w ciszy wobec biegu i głośno
            // wobec dziennika: dowiązania i bit wykonywalności są świadomie poza zakresem tej
            // zmiany (2026-08-29), a odmowa na nich zatrzymywałaby każdy bieg w folderze,
            // w którym stoją.
            tracing::debug!(path = %from.display(), "this is not a file or a folder; the folded copy does not carry it");
            continue;
        }
        let Some(bytes) = what_it_says_now(&from, &base.join(&here))? else {
            continue;
        };
        match changed.get(&here) {
            // Ta sama ścieżka, te same bajty: dwa kroki, które napisały to samo, nie są
            // niezgodą — są jedną zmianą powiedzianą dwa razy.
            Some(before) if before.bytes == bytes => {}
            Some(before) => {
                return Err(Trouble::TwoAnswers {
                    path: here.display().to_string(),
                    one: before.who.to_owned(),
                    other: parent.name.to_owned(),
                });
            }
            None => {
                changed.insert(
                    here,
                    Change {
                        who: parent.name,
                        bytes,
                    },
                );
            }
        }
    }
    Ok(())
}

/// Bajty pliku, JEŚLI różni się od tego, co stoi pod tą samą ścieżką w bazie.
///
/// `None` znaczy „ten krok tego pliku nie tknął", i to jest odpowiedź, a nie brak odpowiedzi:
/// kopia rodzica niesie cały projekt, więc bez tego pytania każdy plik byłby „zmianą" i dwie
/// kopie nie zgadzałyby się na wszystkim, czego człowiek nie ma w commicie.
fn what_it_says_now(from: &Path, base: &Path) -> Result<Option<Vec<u8>>, Trouble> {
    let bytes = fs::read(from).map_err(Trouble::Reading)?;
    match fs::read(base) {
        Ok(before) if before == bytes => Ok(None),
        Ok(_) => Ok(Some(bytes)),
        // Pliku nie było w bazie, więc ten krok go ZAŁOŻYŁ — plik nieśledzony przez gita
        // wchodzi tędy i tylko tędy.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Some(bytes)),
        Err(error) => Err(Trouble::Reading(error)),
    }
}
