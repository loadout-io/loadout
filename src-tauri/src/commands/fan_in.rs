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
//! 2026-09-05 (WF-01/02): podstawą porównania jest wspólny, zamrożony obraz wejścia, nie
//! dzisiejszy HEAD ani katalog konsumenta. Suma ścieżek obrazu i rodzica obejmuje także
//! usunięcia. Manifest tego samego resolvera zachowuje linki i bit wykonywalności; bajty
//! plików są czytane strumieniowo, nie przechowywane w pamięci całego planu.
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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::input_snapshot::{self, Entry, InputSnapshot};
use crate::engine::supervisor;
use crate::engine::supervisor::{PublicationEntryKind, PublicationIdentity, PublicationRoot};

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
    /// Dwie kopie proponują niezgodne stany ścieżki albo jej przodka.
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
                "\"{one}\" and \"{other}\" both changed {path}, and left different results there. \
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

/// Jedna uzgodniona operacja. Brak `after` oznacza usunięcie; rename to dwie operacje.
#[derive(Debug, Clone)]
pub struct Change {
    /// Względna ścieżka pochodząca wyłącznie ze wspólnego skanera, nigdy z promptu.
    pub path: PathBuf,
    /// Stan bazowy operacji: origin przy składaniu, bieżący konsument przy odświeżeniu.
    pub before: Option<Entry>,
    /// Stan, na który zgodzili się zmienieni rodzice.
    pub after: Option<Entry>,
    /// Nazwy kroków, które zaproponowały identyczną zmianę.
    pub authors: Vec<String>,
    /// Katalog stabilnego źródła bajtów. Względną ścieżką pliku jest `path`.
    source_root: PathBuf,
}

/// Kompletny plan bez bajtów repo w RAM. Samo przygotowanie nie pisze do celu.
#[derive(Debug, Clone)]
pub struct MergePlan {
    /// Zmiany w stabilnej kolejności ścieżek, z pełnymi operacjami plikowymi.
    pub changes: Vec<Change>,
    /// Manifesty mają czytelnika w apply: rodzic zmieniony po planie nie jest starym wynikiem.
    sources: Vec<(PathBuf, BTreeMap<PathBuf, Entry>)>,
}

impl MergePlan {
    /// Odświeża wejście następnej rundy, nie nadpisując własnej pracy konsumenta.
    pub fn refresh(
        &self,
        origin: &InputSnapshot,
        previous: &BTreeMap<PathBuf, Option<Entry>>,
        consumer: &Path,
        consumer_name: &str,
    ) -> Result<Self, Trouble> {
        // 2026-09-06: reset do nowego wejścia niszczyłby pracę konsumenta, a zapadka po
        // folderze zostawiała w każdej rundzie wynik pierwszej. Pamiętamy tylko importowaną
        // deltę: trzy stany pozwalają odświeżyć wejście bez zgadywania właściciela zmian.
        let current = input_snapshot::inspect(consumer).map_err(Trouble::Reading)?;
        let new: BTreeMap<_, _> = self.changes.iter().map(|one| (&one.path, one)).collect();
        let paths: BTreeSet<_> = origin
            .entries()
            .keys()
            .chain(previous.keys())
            .chain(current.keys())
            .chain(self.changes.iter().map(|one| &one.path))
            .collect();
        let mut result = current.clone();
        let mut changed = BTreeMap::new();
        for path in paths {
            let base = origin.entries().get(path);
            let old = previous.get(path).map_or(base, Option::as_ref);
            let incoming = new.get(path).map_or(base, |one| one.after.as_ref());
            let mine = current.get(path);
            if incoming == old || incoming == mine {
                continue;
            }
            let author = new
                .get(path)
                .and_then(|one| one.authors.first())
                .map_or("Previous steps", String::as_str);
            if mine != old {
                return Err(conflict(path, consumer_name, author));
            }
            if let Some(entry) = incoming {
                result.insert(path.clone(), entry.clone());
            } else {
                result.remove(path);
            }
            changed.insert(
                path.clone(),
                Change {
                    path: path.clone(),
                    before: mine.cloned(),
                    after: incoming.cloned(),
                    authors: vec![author.to_owned()],
                    source_root: new
                        .get(path)
                        .map_or_else(|| origin.files().clone(), |one| one.source_root.clone()),
                },
            );
        }
        // Sprawdzamy także zachowane, własne dzieci konsumenta. Sama lista operacji nie
        // zobaczy pliku pozostającego pod katalogiem, który rodzic właśnie usunął.
        for path in result.keys() {
            for ancestor in path
                .ancestors()
                .skip(1)
                .filter(|one| !one.as_os_str().is_empty())
            {
                if !matches!(result.get(ancestor), Some(Entry::Directory)) {
                    let author = changed
                        .get(ancestor)
                        .and_then(|one| one.authors.first())
                        .map_or("Previous steps", String::as_str);
                    return Err(conflict(path, consumer_name, author));
                }
            }
        }
        let mut sources = self.sources.clone();
        sources.push((consumer.to_path_buf(), current));
        Ok(Self {
            changes: changed.into_values().collect(),
            sources,
        })
    }

    /// Odcisk semantycznych operacji, nie ścieżki losowego katalogu staging.
    pub fn digest(&self) -> io::Result<String> {
        let operations: Vec<_> = self
            .changes
            .iter()
            .map(|one| (&one.path, &one.before, &one.after, &one.authors))
            .collect();
        digest_of(&operations)
    }

    /// Rodzice zapisani w metadanych kopii mają własny odcisk pełnego wyniku.
    pub fn parents(&self) -> io::Result<Vec<(PathBuf, String)>> {
        self.sources
            .iter()
            .map(|(root, entries)| Ok((root.clone(), digest_of(entries)?)))
            .collect()
    }
}

fn digest_of(value: &impl serde::Serialize) -> io::Result<String> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Najpierw wszystkie konflikty, dopiero później [`apply_plan`].
pub fn plan_frozen(parents: &[Parent<'_>], origin: &InputSnapshot) -> Result<MergePlan, Trouble> {
    plan_entries(origin.entries(), parents)
}

fn plan_entries(
    origin: &BTreeMap<PathBuf, Entry>,
    parents: &[Parent<'_>],
) -> Result<MergePlan, Trouble> {
    if let Some(trouble) = a_copy_from_another_try(parents) {
        return Err(trouble);
    }
    let mut changed: BTreeMap<PathBuf, Change> = BTreeMap::new();
    let mut sources = Vec::with_capacity(parents.len());
    for parent in parents {
        let entries = input_snapshot::inspect(parent.cwd).map_err(Trouble::Reading)?;
        let paths: BTreeSet<&PathBuf> = origin.keys().chain(entries.keys()).collect();
        for path in paths {
            let before = origin.get(path);
            let after = entries.get(path);
            if before == after {
                // Niezmieniony rodzic nie głosuje przeciw zmianie drugiego.
                continue;
            }
            match changed.get_mut(path) {
                Some(previous) if previous.after.as_ref() == after => {
                    previous.authors.push(parent.name.to_owned());
                }
                Some(previous) => {
                    return Err(conflict(path, &previous.authors[0], parent.name));
                }
                None => {
                    changed.insert(
                        path.clone(),
                        Change {
                            path: path.clone(),
                            before: before.cloned(),
                            after: after.cloned(),
                            authors: vec![parent.name.to_owned()],
                            source_root: parent.cwd.to_path_buf(),
                        },
                    );
                }
            }
        }
        sources.push((parent.cwd.to_path_buf(), entries));
    }
    check_ancestors(&changed)?;
    Ok(MergePlan {
        changes: changed.into_values().collect(),
        sources,
    })
}

fn conflict(path: &Path, one: &str, other: &str) -> Trouble {
    Trouble::TwoAnswers {
        path: path.display().to_string(),
        one: one.to_owned(),
        other: other.to_owned(),
    }
}

/// Katalog + nowe dzieci jest zgodą. Usunięcie albo plik/link nad żywym dzieckiem nie jest.
fn check_ancestors(changed: &BTreeMap<PathBuf, Change>) -> Result<(), Trouble> {
    for (path, child) in changed {
        if child.after.is_none() {
            // Usunięcie katalogu naturalnie usuwa też jego dzieci; nie jest konfliktem ze sobą
            // ani z drugim rodzicem, który zgadza się usunąć jedno z tych dzieci.
            continue;
        }
        for ancestor in path.ancestors().skip(1) {
            if let Some(parent) = changed.get(ancestor)
                && !matches!(parent.after, Some(Entry::Directory))
            {
                return Err(conflict(ancestor, &parent.authors[0], &child.authors[0]));
            }
        }
    }
    Ok(())
}

type Identity = (PublicationEntryKind, PublicationIdentity);

/// Anulowanie nie jest błędem I/O ani nieudaną pracą agenta (niezmiennik 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Ready,
    Cancelled,
}

/// Wszystkie bajty/linki są już sprawdzone i należą do biegu. Deskryptor celu oraz odciski
/// jego ścieżek powstają PRZED stagingiem, żeby podmiana podczas odczytu nie dawała uprawnień.
#[derive(Debug)]
pub struct StagedPlan {
    plan: MergePlan,
    into: PathBuf,
    destination: PublicationRoot,
    expected: BTreeMap<PathBuf, Option<Identity>>,
    storage: PublicationRoot,
    staging_name: PathBuf,
    staging_identity: Identity,
    staging: PublicationRoot,
    staged: BTreeMap<PathBuf, Identity>,
}

/// Nie mutuje celu. Odmowa lub przerwanie zachowuje prywatny staging do diagnostyki;
/// nigdy nie wywołuje rekurencyjnego cleanupu na nazwie, którą mógł podmienić ktoś inny.
pub fn stage_plan(into: &Path, storage: &Path, plan: &MergePlan) -> Result<StagedPlan, Trouble> {
    stage_checked(into, storage, plan).map_err(Trouble::Reading)
}

fn stage_checked(into: &Path, storage: &Path, plan: &MergePlan) -> io::Result<StagedPlan> {
    let destination = PublicationRoot::open(into)?;
    let mut expected = BTreeMap::new();
    for change in &plan.changes {
        for path in change
            .path
            .ancestors()
            .filter(|path| !path.as_os_str().is_empty())
        {
            expected
                .entry(path.to_path_buf())
                .or_insert(identity_at(&destination, path)?);
        }
    }
    for (root, expected) in &plan.sources {
        if input_snapshot::inspect(root)? != *expected {
            return Err(io::Error::other(format!(
                "the source folder {} changed after its result was read",
                root.display()
            )));
        }
    }
    let storage_path = storage.to_path_buf();
    let storage = PublicationRoot::open(storage)?;
    let staging_name = PathBuf::from(format!(".fan-in-{}", uuid::Uuid::now_v7()));
    if storage.entry_identity(&staging_name)?.is_some() {
        return Err(io::Error::other(
            "the reserved staging folder already exists",
        ));
    }
    storage.ensure_directory(&staging_name, 0o700)?;
    let staging_identity = storage
        .entry_identity(&staging_name)?
        .ok_or_else(|| io::Error::other("the staging folder disappeared"))?;
    storage.validate_path_identity(&storage_path)?;
    let staging = PublicationRoot::open(&storage_path.join(&staging_name))?;
    let mut staged = BTreeMap::new();
    for (number, change) in plan.changes.iter().enumerate() {
        let name = PathBuf::from(number.to_string());
        match &change.after {
            None | Some(Entry::Directory) => {}
            Some(Entry::Symlink { target: link }) => {
                staging.create_link(&name, link)?;
                let identity = staging
                    .entry_identity(&name)?
                    .ok_or_else(|| io::Error::other("the staged link disappeared"))?;
                staged.insert(name, identity);
            }
            Some(entry @ Entry::File { .. }) => {
                let source = PublicationRoot::open(&change.source_root)?;
                let identity = copy_verified(&source, &change.path, &staging, &name, entry)?;
                source.validate_path_identity(&change.source_root)?;
                staged.insert(name, identity);
            }
        }
    }
    destination.validate_path_identity(into)?;
    Ok(StagedPlan {
        plan: plan.clone(),
        into: into.to_path_buf(),
        destination,
        expected,
        storage,
        staging_name,
        staging_identity,
        staging,
        staged,
    })
}

impl StagedPlan {
    pub fn apply(
        mut self,
        after: impl Fn(&Path, usize) -> io::Result<()>,
        cancelled: impl Fn() -> bool,
    ) -> Result<ApplyOutcome, Trouble> {
        self.apply_checked(after, cancelled)
            .map_err(Trouble::Reading)
    }

    fn apply_checked(
        &mut self,
        after: impl Fn(&Path, usize) -> io::Result<()>,
        cancelled: impl Fn() -> bool,
    ) -> io::Result<ApplyOutcome> {
        if cancelled() {
            return Ok(ApplyOutcome::Cancelled);
        }
        after(&self.into, 0)?;
        self.validate_all()?;
        // Ta sama inode może mieć nowe bajty. Ostatnie sprawdzenie manifestów musi
        // poprzedzać pierwszy zapis; po nim cel naturalnie przestaje być starym wejściem.
        for (root, entries) in &self.plan.sources {
            if input_snapshot::inspect(root)? != *entries {
                return Err(io::Error::other(
                    "a working folder changed after input preparation",
                ));
            }
        }
        let mut removing: Vec<PathBuf> = self
            .plan
            .changes
            .iter()
            .map(|one| one.path.clone())
            .collect();
        removing.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for path in removing {
            if cancelled() {
                return Ok(ApplyOutcome::Cancelled);
            }
            self.validate_path(&path)?;
            if let Some(identity) = self.expected.get(&path).copied().flatten()
                && !self.destination.remove_entry_if_identity(&path, identity)?
            {
                return Err(changed_target(&path));
            }
            self.expected.insert(path, None);
        }
        for number in 0..self.plan.changes.len() {
            if cancelled() {
                return Ok(ApplyOutcome::Cancelled);
            }
            let change = &self.plan.changes[number];
            let name = PathBuf::from(number.to_string());
            self.validate_path(&change.path)?;
            let identity = match &change.after {
                None => None,
                Some(Entry::Directory) => {
                    self.destination.ensure_directory(&change.path, 0o700)?;
                    self.destination.entry_identity(&change.path)?
                }
                Some(Entry::Symlink { target }) => {
                    if self.staging.entry_identity(&name)? != self.staged.get(&name).copied()
                        || self.staging.read_link(&name)? != *target
                    {
                        return Err(io::Error::other("a staged link changed before publication"));
                    }
                    self.destination.create_link(&change.path, target)?;
                    self.destination.entry_identity(&change.path)?
                }
                Some(entry @ Entry::File { .. }) => {
                    if self.staging.entry_identity(&name)? != self.staged.get(&name).copied() {
                        return Err(io::Error::other("a staged file changed before publication"));
                    }
                    Some(copy_verified(
                        &self.staging,
                        &name,
                        &self.destination,
                        &change.path,
                        entry,
                    )?)
                }
            };
            self.expected.insert(change.path.clone(), identity);
            after(&self.into, number + 1)?;
        }
        if cancelled() {
            return Ok(ApplyOutcome::Cancelled);
        }
        self.validate_all()?;
        // Tylko nasze nadal-identyczne obiekty. Obcy inode lub dodatkowy plik pozostawia
        // staging na dysku; brak możliwości cleanupu nie zmienia kompletności wejścia.
        if self.cleanup_staging().is_err() {
            tracing::debug!(path = %self.staging_name.display(), "the completed staging folder remains for inspection");
        }
        Ok(ApplyOutcome::Ready)
    }

    fn validate_path(&self, path: &Path) -> io::Result<()> {
        self.destination.validate_path_identity(&self.into)?;
        for one in path.ancestors().filter(|path| !path.as_os_str().is_empty()) {
            if identity_at(&self.destination, one)? != self.expected.get(one).copied().flatten() {
                return Err(changed_target(one));
            }
        }
        Ok(())
    }

    fn validate_all(&self) -> io::Result<()> {
        for path in self.expected.keys() {
            self.validate_path(path)?;
        }
        self.destination.validate_path_identity(&self.into)
    }

    fn cleanup_staging(&self) -> io::Result<()> {
        for (name, identity) in &self.staged {
            if !self.staging.remove_entry_if_identity(name, *identity)? {
                return Err(io::Error::other(
                    "a staged object was replaced; it was not removed",
                ));
            }
        }
        if !self
            .storage
            .remove_entry_if_identity(&self.staging_name, self.staging_identity)?
        {
            return Err(io::Error::other(
                "the staging directory was replaced; it was not removed",
            ));
        }
        Ok(())
    }
}

fn changed_target(path: &Path) -> io::Error {
    io::Error::other(format!(
        "{} was replaced while its input was being prepared; nothing else was overwritten",
        path.display()
    ))
}

fn identity_at(root: &PublicationRoot, path: &Path) -> io::Result<Option<Identity>> {
    match root.entry_identity(path) {
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        other => other,
    }
}

/// Otwarty plik źródłowy nie może zamienić się w symlink między sprawdzeniem a odczytem.
fn copy_verified(
    from: &PublicationRoot,
    source_path: &Path,
    to: &PublicationRoot,
    target_path: &Path,
    entry: &Entry,
) -> io::Result<Identity> {
    let Entry::File {
        digest: expected_digest,
        bytes: expected_bytes,
        executable,
    } = entry
    else {
        return Err(io::Error::other("only a regular file has bytes to copy"));
    };
    let mut source = from.open_regular_file(source_path)?;
    let metadata = source.metadata()?;
    if metadata.len() != *expected_bytes || supervisor::executable_bits(&metadata) != *executable {
        return Err(io::Error::other(format!(
            "{} changed before copying",
            source_path.display()
        )));
    }
    let mut output = to.create_regular(target_path)?;
    let identity = (
        PublicationEntryKind::Regular,
        supervisor::publication_identity(&output)?,
    );
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let chunk = &buffer[..count];
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("file size overflow"))?;
        if copied > *expected_bytes {
            return Err(io::Error::other(format!(
                "{} grew while copying",
                source_path.display()
            )));
        }
        digest.update(chunk);
        output.write_all(chunk)?;
    }
    if copied != *expected_bytes || format!("{:x}", digest.finalize()) != *expected_digest {
        return Err(io::Error::other(format!(
            "{} changed while copying",
            source_path.display()
        )));
    }
    output.flush()?;
    supervisor::set_executable_file(&output, *executable)?;
    output.sync_all()?;
    if to.entry_identity(target_path)? != Some(identity) {
        return Err(changed_target(target_path));
    }
    Ok(identity)
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
