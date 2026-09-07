//! Zestawy na dysku: gdzie leżą, jak się je czyta i jak się je publikuje.
//!
//! DWA PLIKI, JEDNA PUBLIKACJA. `draft.json` i `manifest.json` jadą w jednej partii
//! `DurableFilePublisher::with_publication`, czyli pod jednym uchwytem katalogu i jednym
//! guardem — między nimi nie ma okna, w które mogłoby wejść recovery albo drugie okno Loadouta.
//!
//! KOLEJNOŚĆ W PARTII JEST TREŚCIĄ, nie porządkiem alfabetycznym:
//!
//! - **szkic pierwszy**, bo to on niesie pracę człowieka i to on ma warunkowy zapis. Spóźniony
//!   zapis odbija się, ZANIM cokolwiek dotknie manifestu, więc odmowa zostawia na dysku komplet
//!   nowszych bajtów, a nie nowszy szkic pod starym tytułem.
//! - **manifest ostatni**, bo to jego obecność czyni katalog GOTOWYM zestawem ([`list_sets`]
//!   czyta nazwę stamtąd, nigdy z nazwy folderu). Publikacja przerwana w połowie zostawia więc
//!   katalog, którego lista nie pokaże — a nie zestaw bez treści.
//!
//! Nazwy zestawu nie ma w nazwie katalogu w sensie tożsamości: slug jest wyłącznie po to, żeby
//! człowiek rozpoznał folder w Finderze (PLAN §4). Wyszukanie zestawu po `id` czyta manifesty,
//! bo folder wolno komuś przemianować ręcznie, a tożsamość ma wtedy zostać.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::durable_file::{
    DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PublishError, revision_of,
};

use super::{ContextDraft, ContextSet, ContextSetRead, Error, LIBRARY_DIR, SCHEMA};

/// Manifest zestawu — jego obecność znaczy „ten katalog jest gotowym zestawem".
const MANIFEST: &str = "manifest.json";

/// Szkic zestawu: źródła, instrukcje budowania i wymagania człowieka.
const DRAFT: &str = "draft.json";

/// Przedrostek katalogu zestawu. Pełna nazwa to `context-<slug>-<id>`.
const FOLDER_PREFIX: &str = "context-";

/// Jedna edycja zestawu, tak jak przyjeżdża z okna.
///
/// Struktura, nie sześć argumentów: to jest JEDEN ładunek `save_context_draft`, więc tytuł
/// i szkic jadą razem albo wcale — zapis, który wziąłby tytuł bez szkicu, zapisałby pół decyzji
/// człowieka i rozjechałby rewizję, którą okno trzyma.
#[derive(Clone, Debug)]
pub struct DraftEdit {
    pub id: String,
    pub title: String,
    pub description: String,
    pub draft: ContextDraft,
    /// Rewizja `draft.json`, którą okno PRZECZYTAŁO. `None` znaczy „tego pliku ma jeszcze nie być".
    pub expected_revision: Option<String>,
    /// Chwila, w której człowiek kliknął — podana przez wołającego, bo to jest fakt o nim,
    /// a nie o momencie, w którym bajty dotarły na dysk (ta sama umowa, co w `memory::notes`).
    pub at: String,
}

/// Rewizje obu plików zestawu z JEDNEGO odczytu — to, na czym stoi ta edycja.
///
/// Para, nie dwa argumenty obok siebie: rewizja szkicu i rewizja manifestu opisują ten sam
/// moment, a podane osobno dałyby się wołać z dwóch różnych odczytów i wtedy jeden z dwóch
/// plików byłby publikowany na bajtach, których nikt nie widział.
#[derive(Clone, Copy, Debug, Default)]
struct Expected<'a> {
    draft: Option<&'a str>,
    manifest: Option<&'a str>,
}

/// Katalog biblioteki. Bierze się z `AppState.home`, nigdy z odczytanego samodzielnie `$HOME` —
/// inaczej test i druga instancja aplikacji pisałyby do katalogu człowieka.
#[must_use]
pub fn library_root(home: &Path) -> PathBuf {
    home.join(LIBRARY_DIR)
}

/// Wszystkie GOTOWE zestawy biblioteki, w kolejności katalogów.
///
/// Katalog bez czytelnego manifestu jest pomijany i to jest cała odpowiedź na „niedokończona
/// publikacja": zestaw, którego manifest nie powstał, nie ma jak udawać gotowego, bo nie ma
/// skąd wziąć ani tytułu, ani identyfikatora.
pub fn list_sets(library: &Path) -> Result<Vec<ContextSet>, Error> {
    let mut out = Vec::new();
    for folder in set_folders(library)? {
        match read_manifest(&folder) {
            Ok((set, _revision)) => out.push(set),
            // Niezmiennik 5: jeden nieczytelny katalog nie zabiera ze sobą całej biblioteki.
            Err(error) => {
                tracing::debug!("{} is not a ready context set: {error}", folder.display());
            }
        }
    }
    Ok(out)
}

/// Jeden zestaw w całości: manifest, szkic i rewizja szkicu.
///
/// Jeden odczyt, nie dwa wywołania: manifest i szkic pobrane osobno mogą pochodzić z dwóch
/// różnych chwil, a wtedy rewizja opisuje bajty inne niż te, które człowiek widzi na ekranie.
pub fn read_set(library: &Path, id: &str) -> Result<ContextSetRead, Error> {
    let folder = folder_holding(library, id)?;
    let (set, _revision) = read_manifest(&folder)?;
    let (draft, revision) = read_draft(&folder)?;
    Ok(ContextSetRead {
        set,
        draft,
        revision,
    })
}

/// Nowy zestaw: własny katalog, pusty szkic i manifest, który czyni go gotowym.
pub fn create_set(library: &Path, title: &str, at: &str) -> Result<ContextSetRead, Error> {
    let id = crate::commands::mint::new_id_inner().to_string();
    let folder = library.join(format!(
        "{FOLDER_PREFIX}{}-{id}",
        // Ta sama zasada nazwy pliku, co przy workflow, umiejętności i zestawie Lab. Czwarta
        // kopia slugifikacji byłaby czwartą odpowiedzią na jedno pytanie (niezmiennik 13).
        crate::lab::slugify(title)
    ));
    fs::create_dir_all(&folder)?;

    let set = ContextSet {
        schema: SCHEMA,
        id,
        title: title.to_owned(),
        description: String::new(),
        archived: false,
        draft_revision: 0,
        latest_ready_revision: None,
        created_at: at.to_owned(),
        changed_at: at.to_owned(),
    };
    let draft = ContextDraft {
        schema: SCHEMA,
        ..ContextDraft::default()
    };
    // Obu plików ma tam jeszcze NIE BYĆ: katalog nosi świeżo wybity uuid v7, więc cokolwiek pod
    // tą ścieżką znaczyłoby, że ktoś nas uprzedził — i publikacja ma wtedy odmówić.
    let revision = publish(&folder, &set, &draft, Expected::default())?;
    Ok(ContextSetRead {
        set,
        draft,
        revision,
    })
}

/// Zapisuje tytuł, opis i szkic — albo odmawia, kiedy na dysku leży nowsza praca.
///
/// Katalog i `id` zostają nietknięte, także przy zmianie tytułu: przeniesienie folderu przy
/// każdej poprawionej literze zrywałoby każdą ścieżkę, którą ktoś zdążył sobie zapisać.
pub fn save_draft(library: &Path, edit: &DraftEdit) -> Result<ContextSetRead, Error> {
    let folder = folder_holding(library, &edit.id)?;
    let (known, manifest_revision) = read_manifest(&folder)?;
    let set = ContextSet {
        title: edit.title.clone(),
        description: edit.description.clone(),
        // Licznik dla CZŁOWIEKA („ile razy to zapisałem"). Warunkowy zapis rozstrzyga się
        // o bajty `draft.json`, nie o tę liczbę — skrót ma kolizje, a bajty nie.
        draft_revision: known.draft_revision.saturating_add(1),
        changed_at: edit.at.clone(),
        ..known
    };
    let revision = publish(
        &folder,
        &set,
        &edit.draft,
        Expected {
            draft: edit.expected_revision.as_deref(),
            manifest: Some(&manifest_revision),
        },
    )?;
    Ok(ContextSetRead {
        set,
        draft: edit.draft.clone(),
        revision,
    })
}

/// Publikuje oba pliki zestawu w jednej partii i oddaje rewizję świeżego `draft.json`.
///
/// OBA IDĄ PRZEZ `publish_definition`, bo `durable_file` nazywa je jedynym wejściem dla plików
/// definicji: publikuje dopiero po sprawdzeniu, CO leży na dysku. `None` znaczy „tego pliku ma
/// tam jeszcze nie być" i tak wchodzi świeży zestaw; przy zapisie każdy z dwóch plików niesie
/// rewizję z TEGO odczytu, więc zmiana pod nami odmawia zamiast nadpisywać.
fn publish(
    folder: &Path,
    set: &ContextSet,
    draft: &ContextDraft,
    expected: Expected<'_>,
) -> Result<String, Error> {
    let draft_text = as_file(draft)?;
    let manifest_text = as_file(set)?;
    let mode = ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE);

    DurableFilePublisher::new(folder)
        .with_publication(|batch| {
            // Szkic pierwszy: on niesie pracę człowieka, więc spóźniony zapis odbija się, ZANIM
            // manifest zdąży dostać nowy tytuł. Powód w nagłówku pliku.
            batch.publish_definition(
                &folder.join(DRAFT),
                draft_text.as_bytes(),
                mode,
                expected.draft,
            )?;
            // Manifest ostatni: to jego obecność czyni katalog gotowym zestawem, więc publikacja
            // przerwana w połowie zostawia katalog, którego lista nie pokaże.
            batch.publish_definition(
                &folder.join(MANIFEST),
                manifest_text.as_bytes(),
                mode,
                expected.manifest,
            )
        })
        .map_err(|error| match error {
            // Odmowa spóźnionego zapisu ma WŁASNE zdanie, nie techniczny powód opakowany
            // w „could not be saved": to jedyny wariant, po którym człowiek ma coś do zrobienia.
            PublishError::Changed { .. } | PublishError::Conflict { .. } => Error::Changed,
            other => Error::Unwritable(other.into_io()),
        })?;
    Ok(revision_of(draft_text.as_bytes()))
}

/// Tekst pliku definicji: dwie spacje wcięcia i znak nowej linii na końcu.
///
/// Deterministycznie i z `\n`, tak samo jak workflow: bez niego każda zmiana ostatniego wiersza
/// niesie w diffie „\ No newline at end of file", a plik przestaje być zwykłym plikiem tekstowym.
fn as_file<T: Serialize>(value: &T) -> Result<String, Error> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

/// Katalogi biblioteki, posortowane. Korzeń, w którym nikt nic nie zapisał, ma zero zestawów —
/// pusta biblioteka jest prawdą, a odmowa nad nowym profilem nie jest.
fn set_folders(library: &Path) -> Result<Vec<PathBuf>, Error> {
    let entries = match fs::read_dir(library) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    // Sortujemy ścieżki, a nie wynik: kolejność, w jakiej system plików oddaje wpisy, nie jest
    // niczyją obietnicą, a lista zestawów ma wyglądać tak samo przy każdym wejściu do sekcji.
    folders.sort();
    Ok(folders)
}

/// Katalog, w którym leży zestaw o tym `id`.
///
/// Czyta MANIFESTY, a nie nazwy katalogów, i to jest cała treść tej funkcji: nazwa folderu ma
/// slug tytułu z chwili utworzenia, więc po zmianie tytułu przestaje być prawdą o zestawie —
/// a ręcznie przemianowany folder nie ma prawa zgubić zestawu, który w nim stoi.
fn folder_holding(library: &Path, id: &str) -> Result<PathBuf, Error> {
    for folder in set_folders(library)? {
        if read_manifest(&folder).is_ok_and(|(set, _revision)| set.id == id) {
            return Ok(folder);
        }
    }
    Err(Error::NoSuchSet)
}

/// Manifest i rewizja jego DOKŁADNYCH bajtów — tej samej, którą zapis odda z powrotem.
fn read_manifest(folder: &Path) -> Result<(ContextSet, String), Error> {
    let bytes = fs::read(folder.join(MANIFEST))?;
    let set = serde_json::from_slice(&bytes)?;
    Ok((set, revision_of(&bytes)))
}

/// Szkic i rewizja jego DOKŁADNYCH bajtów — tej samej, którą okno odda przy następnym zapisie.
fn read_draft(folder: &Path) -> Result<(ContextDraft, String), Error> {
    let bytes = fs::read(folder.join(DRAFT))?;
    let draft = serde_json::from_slice(&bytes)?;
    Ok((draft, revision_of(&bytes)))
}
