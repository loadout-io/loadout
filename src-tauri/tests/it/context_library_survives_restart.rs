//! CT-01: zestaw tekstowy przeżywa restart, zmianę nazwy i spóźniony zapis.
//!
//! CZTERY RZECZY, KTÓRYCH TA FUNKCJA MA DOWIEŚĆ, i każda psuje się osobno:
//!
//! 1. **Materiał wraca z DYSKU, bez `loadout.db`.** Ten plik nie otwiera bazy ani razu i nie ma
//!    jak jej otworzyć — biblioteka Context nie zna `store::` (niezmiennik 4: pliki są prawdą).
//!    Odczyt idzie ŚWIEŻYM wywołaniem po nowym korzeniu, więc żadna pamięć procesu nie ma jak
//!    podstawić odpowiedzi zamiast plików; to jest cały restart, jaki da się napisać w teście.
//! 2. **Zmiana tytułu nie rusza tożsamości.** Słabą wersją jest `assert_eq!(set.id, id)` —
//!    przechodzi ją implementacja, która przenosi katalog i gubi wszystko, co ktoś zdążył
//!    w nim zapisać. Dlatego sądzimy NAZWĘ KATALOGU na dysku i treść, która w nim została.
//! 3. **Spóźniony zapis odbija się, a nowsze bajty zostają.** `is_err()` przechodzi także dla
//!    implementacji, która najpierw nadpisuje, a potem zwraca `Err`, więc drugą połową asercji
//!    jest odczyt pliku i porównanie go z bajtami zapisanymi po nas — ta sama para, co
//!    w `a_late_save_does_not_undo_newer_bytes`.
//! 4. **Niedokończona publikacja nie jest gotowym zestawem.** Przerwanie wchodzi produkcyjnym
//!    szwem `durable_file` (fault injector wybiera wyłącznie punkt przerwania i nie ma własnego
//!    algorytmu zapisu), a asercja pyta o KATALOG, który po tej awarii został: listowanie, które
//!    czyta nazwę zestawu z nazwy folderu, przechodzi tu na czerwono i o to chodzi.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use loadout_lib::context::files::{
    DraftEdit, create_set, library_root, list_sets, read_set, save_draft,
};
use loadout_lib::context::{ContextDraft, ContextSource, Error, SCHEMA, SourceKind};
use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, scoped_faults,
};

/// Chwila, w której człowiek kliknął. Podawana, bo biblioteka nie ma zegara i mieć nie będzie.
const CLICKED: &str = "2026-09-07T10:00:00Z";
const CLICKED_LATER: &str = "2026-09-07T10:04:00Z";

/// Materiał, który człowiek wpisał. Wielowierszowy z rozmysłem: porównanie „co do bajtu" ma
/// paść także dla implementacji, która przycina białe znaki albo skleja wiersze.
const TYPED: &str = "The checkout drops the promo code when the cart is edited.\n\n\
                     Repro: add SPRING20, change the quantity, look at the total.\n";

const HOW_TO_PREPARE: &str = "Keep the exact numbers and say which screen each one is from.";
const REQUIREMENT: &str = "The promo code must survive a quantity change.";

/// Zestaw, którego szuka człowiek. Nazwa z PLAN §1.
const TITLE: &str = "Checkout redesign";

/// Szkic z jednym źródłem tekstowym — dokładnie to, co ten etap zapisuje.
fn typed_draft() -> ContextDraft {
    ContextDraft {
        schema: SCHEMA,
        // 2026-09-07 (CT-02) — `..default()`, bo `ContextSource` dostało wtedy pola pliku,
        // przygotowania i szwu tekst–obraz. Ten zestaw jest tekstowy i żadnego z nich nie
        // niesie; wypisane z ręki zerowe wartości mówiłyby o nich coś, czego ten test nie bada.
        sources: vec![ContextSource {
            id: "s-notes".to_owned(),
            kind: SourceKind::Text,
            name: "Bug notes".to_owned(),
            description: "What QA wrote down while clicking through it.".to_owned(),
            text: TYPED.to_owned(),
            ..ContextSource::default()
        }],
        excluded: Vec::new(),
        how_to_prepare: HOW_TO_PREPARE.to_owned(),
        requirements: vec![REQUIREMENT.to_owned()],
    }
}

/// Nazwy katalogów, które biblioteka naprawdę utworzyła. Posortowane: kolejność, w jakiej system
/// plików oddaje wpisy, nie jest niczyją obietnicą.
fn folders_in(library: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(library) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn text_saved_now_is_read_back_after_a_restart_without_the_index()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let library = library_root(home.path());

    let made = create_set(&library, TITLE, CLICKED)?;
    save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: TITLE.to_owned(),
            description: "Everything the redesign has to keep working.".to_owned(),
            draft: typed_draft(),
            expected_revision: Some(made.revision.clone()),
            at: CLICKED.to_owned(),
        },
    )?;

    // ── restart ────────────────────────────────────────────────────────────────────────────
    // Nic z powyższego nie żyje dalej: ŚWIEŻY korzeń liczony drugi raz z tego samego `home`,
    // czyli dokładnie to, co robi aplikacja, która właśnie wstała. Bazy nie ma i nie było —
    // gdyby zestaw jej potrzebował, ta linia oddawałaby pustą listę.
    let after_restart = library_root(home.path());
    assert!(
        !home.path().join("loadout.db").exists(),
        "this test must not touch the index at all; a context set that needs it breaks the rule \
         that files are the truth"
    );

    let listed = list_sets(&after_restart)?;
    assert_eq!(
        listed
            .iter()
            .map(|set| set.title.clone())
            .collect::<Vec<_>>(),
        vec![TITLE.to_owned()],
        "the set a person made is gone after a restart, or the library invented another one"
    );

    let read = read_set(&after_restart, &made.set.id)?;
    assert_eq!(
        read.draft.sources.first().map(|source| source.text.clone()),
        Some(TYPED.to_owned()),
        "the material came back changed. Every byte of it is a person's work, so a save that \
         reformats, trims or re-wraps it hands back something they did not write"
    );
    assert_eq!(
        read.draft.how_to_prepare, HOW_TO_PREPARE,
        "the answer to \"How should this context be prepared?\" did not survive the restart"
    );
    assert_eq!(
        read.draft.requirements,
        vec![REQUIREMENT.to_owned()],
        "the requirements a person wrote have to come back word for word"
    );
    assert_eq!(
        read.set.latest_ready_revision, None,
        "nothing has been built yet, so claiming a ready version would be a fact nobody measured"
    );
    Ok(())
}

#[test]
fn a_renamed_set_keeps_its_folder_and_its_id() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let library = library_root(home.path());

    let made = create_set(&library, TITLE, CLICKED)?;
    let saved = save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: TITLE.to_owned(),
            description: String::new(),
            draft: typed_draft(),
            expected_revision: Some(made.revision.clone()),
            at: CLICKED.to_owned(),
        },
    )?;

    let folders_before = folders_in(&library);
    assert_eq!(
        folders_before.len(),
        1,
        "one set has to leave exactly one folder behind: {folders_before:?}"
    );

    let renamed = save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: "Checkout redesign, second pass".to_owned(),
            description: String::new(),
            draft: typed_draft(),
            expected_revision: Some(saved.revision.clone()),
            at: CLICKED_LATER.to_owned(),
        },
    )?;

    assert_eq!(
        renamed.set.id, made.set.id,
        "renaming a set is renaming a title, not making another set"
    );
    assert_eq!(
        folders_in(&library),
        folders_before,
        "the rename moved the folder. The slug in the name is there so a person recognises it in \
         Finder; the identity is the id, and a folder that walks takes every path anybody saved \
         with it"
    );

    let read = read_set(&library, &made.set.id)?;
    assert_eq!(
        read.set.title, "Checkout redesign, second pass",
        "the new title has to be the one that comes back"
    );
    assert_eq!(
        read.draft.sources.first().map(|source| source.text.clone()),
        Some(TYPED.to_owned()),
        "the material went missing across a rename, which is the one thing a rename may not touch"
    );
    Ok(())
}

#[test]
fn a_stale_revision_is_refused_and_the_newer_text_stays() -> Result<(), Box<dyn std::error::Error>>
{
    let home = tempfile::tempdir()?;
    let library = library_root(home.path());

    let made = create_set(&library, TITLE, CLICKED)?;
    // Rewizja, którą przeczytało okno otwarte dawno temu.
    let read_by_the_window = save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: TITLE.to_owned(),
            description: String::new(),
            draft: typed_draft(),
            expected_revision: Some(made.revision.clone()),
            at: CLICKED.to_owned(),
        },
    )?;

    // Drugie okno zapisuje w międzyczasie i to ONO ma na dysku ostatnie słowo.
    let mut newer_draft = typed_draft();
    newer_draft.how_to_prepare = "Somebody else got here first.".to_owned();
    save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: TITLE.to_owned(),
            description: String::new(),
            draft: newer_draft.clone(),
            expected_revision: Some(read_by_the_window.revision.clone()),
            at: CLICKED_LATER.to_owned(),
        },
    )?;
    let path = draft_path(&library, &made.set.id);
    assert!(
        path.is_file(),
        "the second window's save left no draft.json at {}, so there are no newer bytes for the \
         late save to threaten and the comparison below would pass on nothing",
        path.display()
    );
    let newer_bytes = fs::read(&path)?;

    let mut late_draft = typed_draft();
    late_draft.how_to_prepare = "The stale window had this open the whole time.".to_owned();
    let late = save_draft(
        &library,
        &DraftEdit {
            id: made.set.id.clone(),
            title: TITLE.to_owned(),
            description: String::new(),
            draft: late_draft,
            // Rewizja SPRZED zapisu drugiego okna — dokładnie to, co niesie spóźnione okno.
            expected_revision: Some(read_by_the_window.revision.clone()),
            at: CLICKED_LATER.to_owned(),
        },
    );
    let described = format!("{late:?}");
    assert!(
        late.is_err(),
        "save_draft took a revision that is no longer what the file says. A window left open for \
         five minutes then writes over work saved one minute ago, and says nothing about it; it \
         answered: {described}"
    );
    let refused = late.err().ok_or("save_draft accepted a stale revision")?;
    let described = format!("{refused:?}");
    assert!(
        matches!(refused, Error::Changed),
        "a late save has its own refusal, because it is the only one after which a person has \
         something to do; got: {described}"
    );
    let said = refused.to_string();
    assert!(
        said.contains("nothing was overwritten"),
        "the refusal has to say that nothing was destroyed, or it reads like a save that did \
         half the work; it says: {said}"
    );
    assert_eq!(
        fs::read(draft_path(&library, &made.set.id))?,
        newer_bytes,
        "the bytes on disk are somebody's newer work, and newer work never disappears without a \
         word — an implementation that writes first and compares afterwards destroys it here"
    );
    Ok(())
}

#[test]
fn a_half_written_set_is_not_listed_as_ready() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let library = library_root(home.path());
    fs::create_dir_all(&library)?;

    // Przerwanie wchodzi produkcyjnym szwem: injector wybiera PUNKT, w którym publikacja pada,
    // i nic poza tym. Po `AfterPartialWrite` w pliku tymczasowym leży połowa manifestu, a pod
    // docelową nazwą nie ma jeszcze nic — czyli dokładnie stan, w jakim zostawia to awaria.
    let faults = Arc::new(StopsTheManifest::default());
    let scope = scoped_faults(&library, Arc::clone(&faults) as Arc<dyn FaultInjector>)?;

    let made = create_set(&library, TITLE, CLICKED);
    drop(scope);

    let described = format!("{made:?}");
    assert!(
        made.is_err(),
        "the manifest never landed and create_set reported a set anyway: {described}. A set that \
         exists only in the answer is one a person will look for tomorrow and not find"
    );
    assert!(
        faults.fired(),
        "the fault never fired, so nothing was interrupted and this case is measuring a happy \
         write; it saw: {described}"
    );

    // Katalog po przerwanej publikacji ZOSTAJE — awaria zasilania zostawia go tak samo, więc
    // listowanie ma sobie z nim radzić, a nie liczyć na sprzątanie po błędzie.
    let left_behind = folders_in(&library);
    assert_eq!(
        left_behind.len(),
        1,
        "the interrupted publication left {left_behind:?} behind, so the assertion below would \
         have nothing to skip and would pass on an empty folder"
    );

    assert_eq!(
        list_sets(&library)?,
        Vec::new(),
        "a folder whose manifest never landed is showing up as a ready set. Listing has to read \
         the name out of the manifest — a listing that reads it out of the folder name shows a \
         set that was never finished, under a title nobody saved"
    );
    Ok(())
}

/// Ścieżka `draft.json` tego zestawu, znaleziona po katalogu, który go trzyma.
///
/// Test celowo nie SKLEJA jej ze sluga: nazwa katalogu jest szczegółem implementacji, a pytanie
/// brzmi „czy bajty na dysku zostały nietknięte", nie „czy zgadłem nazwę folderu".
fn draft_path(library: &Path, id: &str) -> PathBuf {
    let folder = folders_in(library)
        .into_iter()
        .find(|name| name.ends_with(id))
        .unwrap_or_else(|| format!("no folder in the library ends with {id}"));
    library.join(folder).join("draft.json")
}

/// Przerywa publikację `manifest.json` raz, w połowie zapisu.
#[derive(Default)]
struct StopsTheManifest {
    fired: Mutex<bool>,
}

impl StopsTheManifest {
    fn fired(&self) -> bool {
        *lock(&self.fired)
    }
}

impl FaultInjector for StopsTheManifest {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        let manifest = event
            .target
            .file_name()
            .is_some_and(|name| name == "manifest.json");
        if manifest && event.point == FaultPoint::AfterPartialWrite {
            *lock(&self.fired) = true;
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
