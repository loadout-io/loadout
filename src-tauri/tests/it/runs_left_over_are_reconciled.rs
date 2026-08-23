//! Bieg, który zginął razem z aplikacją, przestaje kłamać przy otwarciu folderu.
//!
//! # Co było zepsute
//!
//! `run.json` biegu ubitego razem z oknem zostawał w `running` **na zawsze**. Zmierzone
//! u właściciela 2026-08-23: trzy takie biegi naraz, siedem grup procesów dawno martwych,
//! a historia pokazywała je jako pracę w toku.
//!
//! Odzyskiwanie ISTNIAŁO i nie miało jak ich zobaczyć, z dwóch niezależnych powodów: czytało
//! bazę BIBLIOTEKI (a biegi folderu mają własny indeks i własne pliki), a wynik zapisywało
//! WYŁĄCZNIE do bazy — podczas gdy historia i diagnostyka czytają `run.json`.
//!
//! # Słaba wersja tego kryterium
//!
//! „Po uzgodnieniu bieg nie stoi w `running`". Przechodzi ją funkcja, która przepisuje KAŻDY
//! bieg w folderze — a wtedy skończony bieg dostaje status przerwanego i człowiek traci historię
//! tego, co naprawdę się udało. Rozróżnia je drugi bieg w tej samej fikstrze: zamknięty,
//! porównywany BAJT W BAJT przed i po.
//!
//! Trzeci punkt pilnuje rzeczy, której nie widać, dopóki nie zaboli: `run.json` niesie migawkę
//! grafu i klucze, których ta wersja może nie znać. Uzgodnienie przepisujące plik przez typ tej
//! wersji skasowałoby wszystko, czego typ nie ma.

use std::error::Error;
use std::fs;
use std::path::Path;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::reconcile::with_reaper;
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::supervisor::machine_booted_at;
use loadout_lib::ipc::AppState;
use loadout_lib::recovery::ReapOutcome;
use loadout_lib::store::Store;
use serde_json::Value;

/// Grupa procesów, o którą fikstura każe zapytać. Nikt jej nie zabija — domykacz jest podstawiony.
const DEAD_GROUP: i32 = 33559;

/// Klucz, którego ta wersja nie zna. Ma przeżyć zapis.
const STRANGER: &str = "something-a-newer-build-wrote";

/* Ten sam bieg w obu kryteriach: identyfikator jest stala modulu, bo clippy
 * (`items_after_statements`) slusznie zauwaza, ze `const` w srodku ciala i tak istnieje
 * od poczatku zakresu, wiec udawanie, ze powstaje w tamtym miejscu, jest mylace. */
const OURS: &str = "20260823-114500__01a02c22-346e-73c2-9555-83670e3f93e4";

fn a_run(status: &str, step_status: &str, boot: &str) -> String {
    format!(
        r#"{{
  "id": "01a02c22-346e-73c2-9555-83670e3f93e3",
  "workflow_id": "deep-research.json",
  "workflow_hash": "abc",
  "workflow_snapshot": {{ "format": 1 }},
  "title": "Deep research",
  "status": "{status}",
  "concurrency": 3,
  "created_at": 1787446834286,
  "boot_id": "{boot}",
  "started_at": 1787446837880,
  "ended_at": null,
  "error": null,
  "{STRANGER}": {{ "kept": true }},
  "steps": [
    {{
      "id": "01a02c22-3474-74f1-b850-611803ce3144",
      "node_key": "s_1",
      "name": "Plan steps",
      "agent": "codex",
      "kind": "agent",
      "depends_on": [],
      "status": "{step_status}",
      "attempt": 0,
      "agent_session_id": "01a02c22-3474-74f1-b850-611803ce3144",
      "pid": {DEAD_GROUP},
      "pgid": {DEAD_GROUP},
      "started_at": 1787446837880,
      "ended_at": null,
      "error": null
    }}
  ]
}}
"#
    )
}

/// Bieg, który skończył się normalnie. Ani jeden jego bajt nie ma prawa się zmienić.
const FINISHED: &str = r#"{
  "id": "01a02c25-065d-7050-b8b0-3eed4e1ef2b5",
  "status": "succeeded",
  "ended_at": 1787447357700,
  "steps": [
    { "id": "s_only", "name": "Did it", "status": "succeeded", "ended_at": 1787447357700 }
  ]
}
"#;

fn put(project: &Path, folder: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let dir = project.join(".loadout").join("runs").join(folder);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("run.json"), text)?;
    Ok(())
}

fn read(project: &Path, folder: &str) -> Result<Value, Box<dyn Error>> {
    let text = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(folder)
            .join("run.json"),
    )?;
    Ok(serde_json::from_str(&text)?)
}

const LEFT_OVER: &str = "20260823-010034__01a02c22-346e-73c2-9555-83670e3f93e3";
const CLOSED: &str = "20260823-010339__01a02c25-065d-7050-b8b0-3eed4e1ef2b5";

#[test]
fn a_run_left_running_by_a_closed_window_is_written_off() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path();
    // TEN SAM rozruch maszyny, co teraz: tylko wtedy zapisany `pgid` opisuje cokolwiek
    // prawdziwego, a strażnik z `recovery::decide` w ogóle wypuszcza strzał.
    let boot = machine_booted_at().ok_or("this machine does not say when it booted")?;
    put(project, LEFT_OVER, &a_run("running", "running", &boot))?;
    put(project, CLOSED, FINISHED)?;
    let before = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(CLOSED)
            .join("run.json"),
    )?;

    let mut asked: Vec<i32> = Vec::new();
    let done = with_reaper(project, &mut |pgid| {
        asked.push(pgid);
        ReapOutcome::ProvenDead
    });

    assert_eq!(
        asked,
        vec![DEAD_GROUP],
        "the group of the step that was left running has to be asked about exactly once. Asking \
         about nothing leaves an orphan burning the provider's limit; asking about somebody \
         else's number is a signal sent to an innocent process"
    );
    assert_eq!(
        done.runs, 1,
        "exactly one run was left over; it said {done:?}"
    );

    let repaired = read(project, LEFT_OVER)?;
    assert_eq!(
        repaired["status"].as_str(),
        Some("interrupted"),
        "the run still reads as running, so the history keeps showing work in progress that \
         nobody is doing. It said: {:?}",
        repaired["status"]
    );
    assert_eq!(
        repaired["steps"][0]["status"].as_str(),
        Some("failed"),
        "the step inside it still reads as running"
    );
    assert!(
        !repaired["steps"][0]["error"].is_null(),
        "the step was cut off and says nothing about it - that is the empty red row the owner \
         spent a day looking at"
    );
    assert!(
        !repaired["ended_at"].is_null(),
        "a run that is over has to say when. Without it the history cannot even sort it"
    );

    /* TRZECI PUNKT: klucz, ktorego ta wersja nie zna, przezyl zapis. `run.json` niesie migawke
     * grafu i pola dolozone przez nowszy build; przepisanie pliku przez typ TEJ wersji skasowaloby
     * wszystko, czego typ nie ma — i nie zostawiloby po tym ani jednego komunikatu. */
    assert_eq!(
        repaired[STRANGER]["kept"].as_bool(),
        Some(true),
        "repairing the run threw away a key this build does not know. A newer build wrote it, \
         and one open in an older Loadout would silently eat it"
    );

    /* I CZWARTY, ktory odroznia to kryterium od slabej wersji: bieg zamkniety jest nietkniety
     * BAJT W BAJT. Uzgodnienie przepisujace kazdy plik zamienia historie tego, co sie udalo,
     * w historie przerwan. */
    let after = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(CLOSED)
            .join("run.json"),
    )?;
    assert_eq!(
        after, before,
        "a run that finished on its own was rewritten too. Reconciling is for runs nobody is \
         carrying any more - not for every file in the folder"
    );
    Ok(())
}

/// Sterownik, ktorego to kryterium nie wola. Musi istniec, bo [`AppState`] go trzyma.
fn no_drivers() -> Drivers {
    std::sync::Arc::new(|_vendor| -> std::sync::Arc<dyn AgentDriver> {
        unreachable!("asking a folder for its path never starts an agent")
    })
}

/* DRUGIE KRYTERIUM, I PILNUJE MIEJSCA WOLANIA, NIE MECHANIZMU.
 *
 * Mechanizm powyzej byl zielony i naprawa i tak nie dzialala: wpieta byla w
 * `workspace::Registry::open_store`, a `Registry` NIE MA ANI JEDNEGO WOLAJACEGO w calym drzewie.
 * Zmierzone u wlasciciela 2026-08-23: po restarcie aplikacji trzy zombie stalo dalej w `running`,
 * a linia dziennika nie padla ani razu. Kryterium na sam mechanizm nie odroznia kodu wpietego
 * od kodu, ktory tylko wyglada na wpiety.
 *
 * Dlatego to kryterium wola SZEW, przez ktory idzie kazda komenda dotykajaca projektu —
 * `AppState::project_for` (czternascie miejsc w `ipc.rs`) — i pyta o skutek uboczny. Przeniesienie
 * uzgodnienia gdzie indziej jest wolne; przeniesienie go tam, gdzie nikt nie zaglada, jest tu
 * czerwone.
 *
 * DRUGI PUNKT jest wazniejszy od pierwszego i opisuje ryzyko, ktore ta naprawa sama tworzy:
 * uzgodnienie ZABIJA grupy procesow. Wolane przy kazdej komendzie, przewrociloby biegi ZYWE —
 * te, ktore ta sesja wlasnie prowadzi. Stad „raz na folder": bieg ubity razem z oknem poznaje sie
 * po tym, ze zginal ZANIM to okno wstalo. Nizej stoi wlasnie to: plik, ktory pojawil sie PO
 * pierwszym dotknieciu folderu, jest po drugim dotknieciu nietkniety bajt w bajt.
 */
#[tokio::test]
async fn opening_a_folder_settles_what_the_last_window_left() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let project = root.path();
    // INNY rozruch maszyny niz ten: „maszyna wstala od nowa" znaczy „nie ma czego zabijac",
    // wiec kryterium przechodzi cala droge, nie wysylajac ani jednego sygnalu.
    put(
        project,
        LEFT_OVER,
        &a_run("running", "running", "a-boot-that-is-over"),
    )?;

    let state = AppState::new(
        home.path().to_path_buf(),
        project.to_path_buf(),
        Store::open(&home.path().join("index.db"))?,
        no_drivers(),
    );

    let asked = state
        .project_for(None)
        .map_err(|said| format!("the window could not even name its own folder: {said}"))?;
    assert_eq!(asked, project, "project_for handed back the wrong folder");

    let settled = read(project, LEFT_OVER)?;
    assert_eq!(
        settled["status"].as_str(),
        Some("interrupted"),
        "a run left behind by a window that is gone still reads as work in progress after the \
         new window has opened its folder. The mechanism for this exists and is proven by the \
         criterion above - what this one asks is whether anything ever calls it"
    );

    /* DRUGI PUNKT: bieg, ktory zaczal sie PO otwarciu folderu, jest bezpieczny. */
    put(
        project,
        OURS,
        &a_run("running", "running", "a-boot-that-is-over"),
    )?;
    let before = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(OURS)
            .join("run.json"),
    )?;

    let _ = state.project_for(None);
    let _ = state.project_for(Some(project.to_string_lossy().as_ref()));

    let after = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(OURS)
            .join("run.json"),
    )?;
    assert_eq!(
        after, before,
        "a run that this window started was written off as abandoned. Settling happens ONCE per \
         folder, at the first touch, because that is the only moment at which everything still \
         running was started by somebody else. Called on every command, this would shoot the run \
         the user is watching"
    );
    Ok(())
}

/* TRZECIE KRYTERIUM: folder, ktorego to okno NIE ma otwartego, tez zostaje posprzatany.
 *
 * Sierota w projekcie, do ktorego dzis nie zagladasz, pali limit dostawcy tak samo jak ta
 * w projekcie otwartym — a naprawa oparta wylacznie na pierwszym dotknieciu czeka z nia do dnia,
 * w ktorym czlowiek akurat kliknie ten workspace. Zmierzone u wlasciciela 2026-08-23:
 * uzgodnienie ruszylo przy starcie dla folderu otwartego (4 biegi, 20 krokow), a trzy zombie
 * w sasiednim projekcie staly dalej w `running`.
 *
 * DRUGI PUNKT pilnuje skutku ubocznego, bez ktorego ta naprawa jest gorsza od jej braku:
 * sprzatniety folder ma wejsc do zapadki. Inaczej bieg, ktory ta sesja uruchomi w folderze
 * nieotwartym od startu, trafi na uzgodnienie w trakcie WLASNEJ pracy — i zostanie spisany
 * na straty jako porzucony przez kogos innego.
 */
#[tokio::test]
async fn a_folder_this_window_never_opened_is_settled_too() -> Result<(), Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let open = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    put(
        elsewhere.path(),
        LEFT_OVER,
        &a_run("running", "running", "a-boot-that-is-over"),
    )?;
    fs::write(
        home.path().join("workspaces.json"),
        serde_json::to_string(&serde_json::json!([{
            "id": elsewhere.path().to_string_lossy(),
            "name": "The one nobody clicked",
            "folder": elsewhere.path().to_string_lossy(),
        }]))?,
    )?;

    let state = AppState::new(
        home.path().to_path_buf(),
        open.path().to_path_buf(),
        Store::open(&home.path().join("index.db"))?,
        no_drivers(),
    );
    state.settle_everything_left_behind(home.path());

    assert_eq!(
        read(elsewhere.path(), LEFT_OVER)?["status"].as_str(),
        Some("interrupted"),
        "the window opened on one project and left a run in another one reading as work in \
         progress. Nobody is carrying it, and its agents are still counted against the limit \
         of the account that pays for them"
    );

    /* DRUGI PUNKT: zapadka zasiana, wiec bieg tej sesji w tamtym folderze jest bezpieczny. */
    put(
        elsewhere.path(),
        OURS,
        &a_run("running", "running", "a-boot-that-is-over"),
    )?;
    let before = read(elsewhere.path(), OURS)?;
    let _ = state.project_for(Some(elsewhere.path().to_string_lossy().as_ref()));

    assert_eq!(
        read(elsewhere.path(), OURS)?,
        before,
        "settling at start-up did not put that folder in the latch, so the first command naming \
         it settled it a SECOND time - by then this session was running there, and its own run \
         was written off as somebody else's leftovers"
    );
    Ok(())
}

/* CZWARTE KRYTERIUM: bieg zaparkowany na PYTANIU tez jest porzucony.
 *
 * Zmierzone u wlasciciela 2026-08-23: bieg `20260819-160548` stal w `paused` **czwarty dzien**,
 * przez kilkanascie restartow aplikacji, i zadne sprzatanie go nie dotykalo. Przebieg dla biegow
 * uciętych w pracy nie ma jak go zobaczyc: pyta o kroki stojace w `running`, zeby miec co dobic,
 * a bieg czekajacy na czlowieka nie ma ani jednego takiego kroku.
 *
 * DLACZEGO TO JEST PORZUCENIE, A NIE CIERPLIWOSC: pytanie punktu kontrolnego zyje wylacznie
 * w zywym strumieniu okna, a `continue_run` nie bierze identyfikatora biegu — wiec po zniknieciu
 * okna nie ma ZADNEJ drogi, zeby na ten bieg odpowiedziec. Nazywanie tego pauza jest obietnica,
 * ktorej nie ma jak dotrzymac.
 *
 * FOLDER Z SAMA PAUZA, i to nie jest wymyslony przypadek — to jest dokladnie folder wlasciciela.
 * Ten uklad zlapal prawdziwy blad w pierwszej wersji naprawy: „nie ma czego dobijac" konczylo
 * caly przebieg wczesniej, wiec bieg zaparkowany wychodzil z niego nietkniety. Fikstura bez ani
 * jednego kroku w `running` jest jedyna, ktora te roznice widzi.
 */
#[test]
fn a_run_left_standing_on_a_question_is_written_off_too() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let project = root.path();
    const PARKED: &str = "20260819-160548__01a01ac5-8a29-7f02-adef-2f12a67416a1";

    put(
        project,
        PARKED,
        &a_run("paused", "pending", "a-boot-that-is-over"),
    )?;
    put(project, CLOSED, FINISHED)?;
    let before = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(CLOSED)
            .join("run.json"),
    )?;

    let mut asked: Vec<i32> = Vec::new();
    let done = with_reaper(project, &mut |pgid| {
        asked.push(pgid);
        ReapOutcome::ProvenDead
    });

    assert!(
        asked.is_empty(),
        "a signal was sent while settling a run in which nothing was working. Every number here \
         belongs to some process on this machine, and none of them belongs to this run: {asked:?}"
    );

    let settled = read(project, PARKED)?;
    assert_eq!(
        settled["status"].as_str(),
        Some("interrupted"),
        "a run that was waiting for an answer still says it is paused. Nothing is working in it \
         and the question it was standing on went away with the window that drew it, so there is \
         no way left to answer it - it would sit there for ever"
    );
    let said = settled["error"].as_str().unwrap_or_default();
    assert!(
        said.contains("waiting for your answer"),
        "the run says it was interrupted and does not say what happened to it. \"Interrupted\" \
         alone reads like a crash; this one was waiting for a person. It said: {said}"
    );
    assert_eq!(
        done.runs, 1,
        "the tally is wrong, so the log line under it would misreport what was left over"
    );

    /* I TA SAMA POPRZECZKA, CO WYZEJ: bieg zamkniety normalnie jest nietkniety bajt w bajt.
     * Przebieg, ktory przepisuje kazdy plik, zamienia historie tego, co sie udalo, w historie
     * przerwan — a ten przebieg czyta KAZDY katalog w folderze, wiec pyta o to od nowa. */
    let after = fs::read_to_string(
        project
            .join(".loadout")
            .join("runs")
            .join(CLOSED)
            .join("run.json"),
    )?;
    assert_eq!(
        after, before,
        "a run that finished on its own was rewritten by the pass that settles parked runs"
    );
    Ok(())
}
