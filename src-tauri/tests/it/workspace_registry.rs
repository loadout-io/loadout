//! AC-3 dla T-24: ten sam folder nie zakłada drugiej karty — i nie otwiera drugiego magazynu.
//!
//! Kryterium ma dwie połowy i dopiero druga jest groźna.
//!
//! **Pierwsza połowa** jest o pasku kart: `open()` dwa razy pod rząd oddaje ten sam
//! `WorkspaceId`, a rejestr ma jedną pozycję. Słaba wersja porównuje surowe stringi ścieżek —
//! przechodzi dla dwóch identycznych wywołań i pęka na każdym z czterech sposobów nazwania
//! jednego folderu, które człowiek naprawdę wpisze albo wyklika. Dlatego te cztery stoją tu
//! jako osobne przypadki, a nie jako komentarz.
//!
//! **Druga połowa** jest o niezmienniku 2 i bez niej pierwsza nie chroni przed niczym.
//! `Store::open` nie ma żadnej obrony przed drugim otwarciem tej samej bazy i świadomie jej
//! nie dostaje: decyzja człowieka z 2026-08-16 brzmi, że gwarancja mieszka w rejestrze
//! workspace'ów. Skoro tutaj, to tutaj musi być udowodniona.
//!
//! Groźna słaba wersja tej połowy to porównanie `WorkspaceId`. Identyfikator jest **wyliczany
//! ze ścieżki**, więc zgadza się ZAWSZE — także w rejestrze, który pod spodem woła
//! `Store::open` drugi raz i uruchamia drugie zapisujące połączenie do tego samego pliku.
//! To jest dokładnie zakleszczenie, które niezmiennik 2 nazywa po imieniu [T7 ryzyko 7],
//! i przeszłoby całe kryterium wyżej bez jednego czerwonego testu. Rozróżniają je dwie rzeczy
//! naraz: `Arc::ptr_eq` na tym, co trzyma rejestr, oraz **kontrola dodatnia** — po drugim
//! otwarciu pierwszy uchwyt dalej pisze i zapis dochodzi. Sama tożsamość wskaźnika przeszłaby
//! jeszcze na implementacji, która oddaje ten sam obiekt po tym, jak go zamknęła.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, anyhow};
use loadout_lib::engine::limits::Limiter;
use loadout_lib::store::NewRun;
use loadout_lib::workspace::{RECENT_CAP, Registry, WorkspaceId};

/// Ile miejsc ma pula, kiedy to kryterium jej nie dotyczy.
const AT_ONCE: usize = 2;

/// Bieg, którym dowodzimy, że pierwszy uchwyt dalej pisze.
const RUN_ID: &str = "01996500-0000-7000-8000-00000000c001";

/// Ile folderów otwieramy, żeby zobaczyć sufit listy ostatnich. Więcej niż [`RECENT_CAP`],
/// bo lista przycięta do dziesięciu i lista, której nikt nie przycina, wyglądają identycznie
/// przy dziesięciu wpisach.
const FOLDERS: usize = 12;

/// Zakłada folder pod `root` i oddaje jego ścieżkę.
fn folder(root: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let path = root.join(name);
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Bieg w kształcie, w jakim wchodzi do bazy. Treść jest nieistotna — istotne jest, czy zapis
/// przez PIERWSZY uchwyt dochodzi po tym, jak ktoś otworzył ten sam folder drugi raz.
fn a_run() -> NewRun {
    NewRun {
        id: RUN_ID.to_owned(),
        workflow_id: "ship-a-feature".to_owned(),
        workflow_snapshot: r#"{"nodes":[],"edges":[]}"#.to_owned(),
        title: "Fix the CSV parser".to_owned(),
        status: "running".to_owned(),
        concurrency: 3,
        created_at: 1_755_300_000_000,
        started_at: Some(1_755_300_001_000),
        ended_at: None,
        boot_id: None,
        error: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_one_folder_twice_leaves_one_tab() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let meetnotes = folder(root.path(), "meetnotes")?;
    let registry = Registry::new(Limiter::new(AT_ONCE));

    let first = registry.open(&meetnotes)?;
    let again = registry.open(&meetnotes)?;

    assert_eq!(
        first, again,
        "opening the same folder twice has to hand back the same workspace, not a second one. \
         Two runs in one directory collide on the files themselves, and the per-step copy \
         protects steps from each other — never runs from each other (ARCHITECTURE §6a rule 1)"
    );
    assert_eq!(
        registry.tabs(),
        vec![first],
        "and the tab bar has to hold exactly one tab afterwards. Handing back the same id while \
         still adding a row is the version a person sees: two tabs with the same name, both \
         claiming the same folder"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn four_ways_of_naming_one_folder_are_one_workspace() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let meetnotes = folder(root.path(), "meetnotes")?;

    // Dowiązanie symboliczne jest najczęstszym z tych czterech wejść, nie najrzadszym:
    // `~/work -> ~/Projects` jest normalnym układem katalogu domowego, a wtedy ten sam folder
    // ma dwie nazwy, których żadne porównanie tekstu nie sklei.
    //
    // `std::os::unix` bez `cfg`, bo Loadout jest aplikacją macOS-ową (DECISIONS-LOCKED, D1),
    // a `checks/quick-boundary.sh` trzyma niezmiennik 3 na `src-tauri/src/` — plik testowy
    // jest z tej reguły wyłączony PO ŚCIEŻCE i to jest jedyne miejsce, w którym wolno to
    // napisać wprost.
    let link = root.path().join("work-link");
    std::os::unix::fs::symlink(&meetnotes, &link)?;

    // Cztery nazwy tego samego katalogu, każda z innego wejścia:
    //   plain    to, co odda okno wyboru folderu,
    //   trailing to, co wpisze człowiek z uzupełnianiem w powłoce,
    //   dotted   to, co przyjdzie ze ścieżki sklejonej względem czegoś innego,
    //   link     to, co zobaczysz, kiedy katalog domowy ma dowiązania.
    let named: [(&str, PathBuf); 4] = [
        ("plain", meetnotes.clone()),
        (
            "trailing slash",
            PathBuf::from(format!("{}/", meetnotes.display())),
        ),
        ("dot in the middle", root.path().join(".").join("meetnotes")),
        ("symlink", link),
    ];

    let registry = Registry::new(Limiter::new(AT_ONCE));
    let expected = registry.open(&meetnotes)?;

    for (how, path) in named {
        let opened = registry
            .open(&path)
            .with_context(|| format!("opening the {how} spelling of the folder"))?;
        assert_eq!(
            opened, expected,
            "{how} names the very same directory, so it has to land on the very same workspace. \
             Comparing the raw strings passes for two identical calls and breaks on every one of \
             these four"
        );
    }

    assert_eq!(
        registry.tabs().len(),
        1,
        "and all five openings together may leave exactly one tab on the bar; the bar holds {:?}",
        registry.tabs()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_opening_hands_back_the_store_that_is_already_working() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let meetnotes = folder(root.path(), "meetnotes")?;
    let registry = Registry::new(Limiter::new(AT_ONCE));

    let first = registry.open(&meetnotes)?;
    // Uchwyt wzięty PRZED drugim otwarciem. To jest cała konstrukcja tego przypadku: chodzi
    // o to, co się stanie z magazynem, który już pracuje, kiedy ktoś otworzy ten sam folder.
    let working = registry
        .store(&first)
        .ok_or_else(|| anyhow!("the registry opened {first} and then had no store for it"))?;

    let again = registry.open(&meetnotes)?;
    let handed_back = registry
        .store(&again)
        .ok_or_else(|| anyhow!("the second opening of {again} came back without a store"))?;

    assert!(
        Arc::ptr_eq(&working, &handed_back),
        "the second opening has to hand back THE SAME store object, not an equivalent one. \
         Comparing WorkspaceId instead would pass here no matter what: the id is computed from \
         the path, so it agrees even when a second Store::open ran underneath and a second \
         WRITING connection to one file is live — which is the deadlock invariant 2 names by \
         name [T7 risk 7]"
    );

    // ── Kontrola dodatnia ─────────────────────────────────────────────────────────────────
    // Sama tożsamość wskaźnika przechodzi jeszcze na implementacji, która oddaje ten sam
    // obiekt po tym, jak go zamknęła albo podmieniła mu połączenie. Zapis PIERWSZYM uchwytem,
    // po drugim otwarciu, jest jedynym pytaniem, które to rozróżnia.
    working
        .writer()
        .insert_run(a_run())
        .await
        .context("the handle taken before the second opening refused to write after it")?;

    let landed: i64 = handed_back.reader()?.query_row(
        "SELECT count(*) FROM runs WHERE id = ?1",
        [RUN_ID],
        |row| row.get(0),
    )?;
    assert_eq!(
        landed, 1,
        "the write went through the first handle and has to be readable afterwards. A second \
         opening may not close, replace or fence off a store that is already working — the run \
         it carries did not stop just because somebody picked the folder again"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_recent_folders_are_newest_first_and_ten_long() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let registry = Registry::new(Limiter::new(AT_ONCE));

    let mut opened: Vec<WorkspaceId> = Vec::with_capacity(FOLDERS);
    for n in 0..FOLDERS {
        opened.push(registry.open(&folder(root.path(), &format!("project-{n:02}"))?)?);
    }

    let recent = registry.recent();
    assert_eq!(
        recent.len(),
        RECENT_CAP,
        "{FOLDERS} folders were opened and the recent list holds {}. A menu that keeps every \
         folder forever stops being a shortcut somewhere around the twentieth entry",
        recent.len()
    );

    let newest_first: Vec<WorkspaceId> = opened.iter().rev().take(RECENT_CAP).cloned().collect();
    assert_eq!(
        recent, newest_first,
        "the recent list is ordered by when the folder was last used, newest first. Insertion \
         order reads the same for the first ten openings and then never changes again, which is \
         a menu whose top entry is the folder you have not touched in a week"
    );

    // Ponowne otwarcie jest UŻYCIEM, więc folder wraca na górę — także taki, który z listy
    // zdążył już wypaść.
    let long_ago = opened
        .first()
        .ok_or_else(|| anyhow!("no folder was opened, so there is nothing to reopen"))?;
    let back = registry.open(long_ago.as_path())?;
    let recent = registry.recent();
    assert_eq!(
        recent.first(),
        Some(&back),
        "reopening a folder is using it, so it has to come back to the top of the list. It \
         stands at {:?}",
        recent.first()
    );
    assert_eq!(
        recent.len(),
        RECENT_CAP,
        "and the ceiling holds while it does so"
    );
    Ok(())
}
