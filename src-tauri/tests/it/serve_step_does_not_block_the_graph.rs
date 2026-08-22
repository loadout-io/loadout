//! Kafelek „uruchom i zostaw" kończy się, gdy proces WSTAŁ — i zostawia go żywym.
//!
//! # Co to mierzy
//!
//! 2026-08-23, prośba właściciela wprost („uruchom i zostaw"), po jego biegu na `urc-monorepo`,
//! w którym sprawdzenie frontu nie miało jak podnieść serwera dev. Zderzają się tam dwie
//! POPRAWNE reguły: proces poboczny nie ma prawa przeżyć swojego kroku (niezmiennik 6), a
//! pomiar żywej aplikacji wymaga, żeby przeżył. Do tego dnia jedynym kafelkiem, który cokolwiek
//! uruchamiał, był „sprawdź" — a ten CZEKA na koniec komendy, więc `npm run dev` wisiał w nim
//! do limitu i meldował porażkę na kroku, który nie miał czego sprawdzić.
//!
//! # SŁABĄ WERSJĄ jest „bieg się skończył"
//!
//! Przechodzi ją implementacja, która komendy nie uruchamia w ogóle — i wygląda przy tym na
//! szybką. Dlatego niżej stoją naraz TRZY rzeczy, których żadna pojedynczo nie rozstrzyga:
//!
//! * (a) bieg wraca, choć komenda nie zeszła i nie zejdzie sama z siebie,
//! * (b) plik, który tworzy TA komenda, istnieje — więc naprawdę pobiegła,
//! * (c) krok PO niej ruszył i zdążył skończyć, choć ona dalej żyje.
//!
//! # I czwarta, w drugą stronę
//!
//! (d) proces jest w rejestrze i ma grupę — bo „uruchom i zostaw" bez wpisu w rejestrze jest
//! rzeczą, której nikt nie umie ubić, czyli osieroconym `npm run dev` trzymającym port po
//! zamknięciu okna (niezmiennik 6). Bez tej asercji przechodzi implementacja, która startuje
//! komendę i puszcza uchwyt.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::step::StepState;
use loadout_lib::ipc::{QUEUE_CAP, line_channel, spawn_pump};
use loadout_lib::store::Store;
use tauri::ipc::Channel;
use tempfile::TempDir;

/// Sufit cierpliwości jednego biegu.
///
/// Jest KRÓTSZY niż sen komendy niżej i to jest treść kryterium (a), nie oszczędność: gdyby graf
/// czekał na koniec tej komendy, ten bieg nie wróciłby w tym oknie i test padłby na limicie.
const PATIENCE: Duration = Duration::from_secs(20);

/// Komenda, która NIE SCHODZI. Zostawia po sobie ślad i śpi dłużej niż cała cierpliwość testu.
///
/// `touch` przed `sleep`, nie po: dowód (b) ma powstać w chwili startu, bo po końcu snu nikt tu
/// już nie patrzy.
const STAYS: &str = r#"#!/bin/sh
touch "$1"
sleep 600
"#;

/// Krok PO nim: zwykłe sprawdzenie, które schodzi natychmiast.
const AFTER: &str = r#"#!/bin/sh
echo "1 passed"
exit 0
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_graph_walks_past_a_command_that_never_finishes() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let mark = bench.project.path().join("it-started");
    let stays = bench.script("stays.sh", STAYS)?;
    let after = bench.script("after.sh", AFTER)?;
    let workflow = bench.workflow("serve-then-check", &serve_then_check(&stays, &mark, &after))?;
    let store = Store::open(&bench.db())?;
    let processes = Arc::new(Processes::new());

    let deps = RunDeps {
        home: bench.home.path(),
        project: bench.project.path(),
        store: &store,
        drivers: no_drivers(),
        processes: Arc::clone(&processes),
        control: RunControl::new(),
    };
    let request = RunRequest {
        workflow,
        how_many_at_once: 2,
        task: None,
        only: None,
        handoffs_from: None,
    };

    // ── (a) BIEG WRACA, CHOĆ KOMENDA DALEJ ŚPI ────────────────────────────────────────────
    let report = one_run(&deps, &request).await??;

    // ── (b) I KOMENDA NAPRAWDĘ POBIEGŁA ───────────────────────────────────────────────────
    // Bez tej asercji kryterium przechodzi dla implementacji, która kafelek POMIJA — a taka
    // wygląda dokładnie tak samo: bieg wraca szybko i wszystko jest zielone.
    assert!(
        mark.exists(),
        "the command left no trace, so nothing started it. A tile that quietly does nothing is \
         the cheapest way to pass this test and the worst thing this tile could do"
    );

    // ── (c) I KROK PO NIM ZDĄŻYŁ SKOŃCZYĆ ─────────────────────────────────────────────────
    assert_eq!(
        report.steps,
        vec![StepState::Succeeded, StepState::Succeeded],
        "both steps have to be finished: the tile in the moment its process stood up, and the \
         check that runs after it. A run in which the second step never got its turn is the \
         owner's original defect with a different tile drawn over it"
    );

    // ── (d) I PROCES ZOSTAŁ ŻYWY, W REJESTRZE ─────────────────────────────────────────────
    // To jest druga połowa nazwy tego kafelka. Bez wpisu w rejestrze nikt nie umie tej rzeczy
    // ubić — a wtedy „zostaw" znaczy „osieroć" (niezmiennik 6).
    let held = processes.list();
    assert_eq!(
        held.len(),
        1,
        "the registry holds {} things. A command started and let go of is a thing nobody can \
         stop: it survives the window, goes to PID 1 and keeps the port",
        held.len()
    );
    assert!(
        held[0].alive,
        "the thing is in the registry and already down. `sleep 600` cannot have finished inside \
         a {PATIENCE:?} run, so this says the step waited for it after all — or killed it"
    );

    // Sprzątamy po sobie: to jest ta sama droga, którą idzie zamknięcie okna (`lib.rs`).
    let _ = processes.close().await;
    Ok(())
}

/// Kafelek „uruchom i zostaw", a po nim sprawdzenie.
fn serve_then_check(stays: &Path, mark: &Path, after: &Path) -> String {
    // Ścieżki bezwzględne wprost w komendzie: środowisko dziecka jest czyszczone, więc przez
    // zmienną nie przejdą.
    format!(
        r#"{{
  "format": 1,
  "id": "wf_serve_then_check",
  "name": "Start the app, then look at it",
  "steps": [
    {{
      "kind": "serve",
      "id": "s_serve",
      "name": "Start the app",
      "command": "{stays} {mark}",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 24, "y": 24 }}
    }},
    {{
      "kind": "check",
      "id": "s_after",
      "name": "Look at it",
      "command": "{after}",
      "proof": "(\\d+) passed",
      "folder": {{ "use": "project" }},
      "at": {{ "x": 24, "y": 168 }}
    }}
  ],
  "links": [{{ "from": "s_serve", "to": "s_after" }}]
}}"#,
        stays = stays.display(),
        mark = mark.display(),
        after = after.display(),
    )
}

/// Jeden bieg z limitem cierpliwości. Zewnętrzny `Result` mówi „bieg wrócił", wewnętrzny — czym.
async fn one_run(
    deps: &RunDeps<'_>,
    request: &RunRequest,
) -> Result<Result<RunReport, loadout_lib::commands::RunError>, Box<dyn Error>> {
    let (sink, source) = line_channel(QUEUE_CAP);
    let pump = spawn_pump(source, Channel::new(|_| Ok(())));
    let drain = async move {
        let _ = pump.await;
    };

    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(run_workflow_inner(deps, request, sink), drain)
    })
    .await
    .map_err(|_| format!("the run did not come back within {PATIENCE:?}"))?;
    Ok(both.0)
}

/// Fabryka, która PANIKUJE. Żaden krok w tym pliku nie ma vendora, więc nikt nie ma powodu prosić
/// o sterownik — a prośba jest wtedy defektem, nie szczegółem.
fn no_drivers() -> Drivers {
    Arc::new(|_| -> Arc<dyn AgentDriver> {
        panic!("no step in this workflow names a vendor, so nothing may ask for an agent driver")
    })
}

/// Biblioteka użytkownika i projekt na czas jednego kryterium.
struct Bench {
    home: TempDir,
    project: TempDir,
    scripts: TempDir,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let project = TempDir::new()?;
        let scripts = TempDir::new()?;
        fs::create_dir_all(home.path().join("workflows"))?;
        fs::create_dir_all(project.path().join(".loadout"))?;
        Ok(Self {
            home,
            project,
            scripts,
        })
    }

    fn script(&self, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self.scripts.path().join(name);
        fs::write(&path, body)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    fn workflow(&self, slug: &str, text: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = self
            .home
            .path()
            .join("workflows")
            .join(format!("{slug}.json"));
        fs::write(&path, text)?;
        Ok(path)
    }

    fn db(&self) -> PathBuf {
        self.project.path().join(".loadout").join("loadout.db")
    }
}
