//! P-02: instancja testowa nie pisze do danych człowieka.
//!
//! Prawdziwy `Processes`, prawdziwy proces, prawdziwe środowisko. Program testowy wypisuje
//! to, co dostał, więc kryterium sądzi ŚRODOWISKO, które naprawdę dojechało do dziecka —
//! nie wartość policzoną w funkcji, której nikt nie woła.

#![allow(clippy::unwrap_used)]
#![allow(clippy::panic)]
#![allow(clippy::expect_used, clippy::too_many_lines, clippy::similar_names)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use std::os::unix::fs::PermissionsExt as _;

use loadout_lib::commands::processes::{Processes, ServiceOwner, ServiceRef};
use loadout_lib::engine::supervisor::StepTag;
use loadout_lib::workflow::{
    LaunchDescription, ReadinessSpec, ServiceEndpointSpec, ServiceLifetime, TargetKind,
};
use tokio_util::sync::CancellationToken;

/// Podmiana katalogu domowego jest odmową, nie izolacją.
///
/// Odziedziczona zieleń, trzymana tutaj z rozmysłu: sama odmowa mieszka w walidatorze opisu
/// uruchomienia, a to kryterium jest jej strażnikiem od strony STARTU — czyli tej, na której
/// instancja testowa naprawdę mogłaby napisać do katalogu człowieka.
#[tokio::test]
async fn a_replaced_home_folder_is_refused() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let mut environment = std::collections::BTreeMap::new();
    environment.insert("HOME".to_owned(), "/tmp/somewhere".to_owned());
    let why = bench
        .start(LaunchDescription {
            command: "true".to_owned(),
            kind: TargetKind::Web,
            test_data_env: None,
            subdirectory: String::new(),
            environment,
            required_env: Vec::new(),
            endpoints: Vec::new(),
            readiness: None,
        })
        .await
        .expect_err("a replaced home folder started anyway");
    assert!(
        why.contains("cannot replace the environment variable HOME"),
        "a test instance was allowed to run with a home folder nobody described: {why:?}"
    );
    Ok(())
}

/// Aplikacja natywna bez własnej podmiany katalogu danych nie startuje — i zdanie mówi,
/// że brakuje USTAWIENIA W APLIKACJI, a nie że coś jest z nią nie tak.
#[tokio::test]
async fn a_native_app_without_a_test_data_setting_does_not_start() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let why = bench
        .start(LaunchDescription {
            command: "true".to_owned(),
            kind: TargetKind::Native,
            test_data_env: None,
            subdirectory: String::new(),
            environment: std::collections::BTreeMap::new(),
            required_env: Vec::new(),
            endpoints: Vec::new(),
            readiness: None,
        })
        .await
        .expect_err("a native app with nowhere to put test data started anyway");
    assert!(
        why.contains("no way to keep test data apart from yours")
            && why.contains("missing setting in the app, not a result about it"),
        "the refusal reads like a verdict about the application: {why:?}"
    );
    Ok(())
}

/// Cel webowy bez własnych danych zostaje zwykłym celem webowym.
#[tokio::test]
async fn a_web_target_without_one_still_starts() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench
        .start(LaunchDescription {
            command: "true".to_owned(),
            kind: TargetKind::Web,
            test_data_env: None,
            subdirectory: String::new(),
            environment: std::collections::BTreeMap::new(),
            required_env: Vec::new(),
            endpoints: Vec::new(),
            readiness: None,
        })
        .await?;
    Ok(())
}

/// Podana zmienna naprawdę dojeżdża do procesu i wskazuje katalog PRZY BIEGU, nie w kopii
/// roboczej: kopia jest porównywana z bazą, więc dane aplikacji w jej środku wyglądałyby
/// jak praca agenta.
#[tokio::test]
async fn the_named_setting_reaches_the_process_and_points_beside_the_run()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let said = bench
        .start_and_read(LaunchDescription {
            command: "printf '%s\\n' \"$MURMUR_TEST_DATA\"".to_owned(),
            kind: TargetKind::Native,
            test_data_env: Some("MURMUR_TEST_DATA".to_owned()),
            subdirectory: String::new(),
            environment: std::collections::BTreeMap::new(),
            required_env: Vec::new(),
            endpoints: Vec::new(),
            readiness: None,
        })
        .await?;
    let at = said.trim();
    assert!(
        !at.is_empty(),
        "the app was started without the test data folder it was promised"
    );
    assert!(
        std::path::Path::new(at).is_dir(),
        "the app was handed a folder that does not exist: {at:?}"
    );
    assert!(
        at.contains("/app-data/"),
        "the test data folder is not the one Loadout makes for this run: {at:?}"
    );
    assert!(
        !at.contains("/work/"),
        "test data lands inside the working copy, where it reads as the agent's own work: {at:?}"
    );
    Ok(())
}

/// P-02 punkt 6 / P-03b: dla celu NATYWNEGO gotowy port jest połową prawdy.
///
/// Scenariusz, który ma się odbyć w oknie, potrzebuje okna. `ng serve` odpowiadający na porcie
/// spełnia wyłącznie cel webowy — a incydent I-06 to właśnie dopuszczał jako potwierdzenie
/// zachowania widocznego wyłącznie w interfejsie.
#[tokio::test]
async fn a_native_target_without_a_window_is_not_ready_yet() -> Result<(), Box<dyn Error>> {
    // System, który odpowiada „zero okien": aplikacja jeszcze go nie pokazała.
    let bench = Bench::answering("echo 0")?;
    let why = bench
        .start(native_with_readiness())
        .await
        .expect_err("a native target with no window was announced as ready");
    assert!(
        why.contains("did not become ready"),
        "the wait ended for some other reason than the missing window: {why:?}"
    );
    Ok(())
}

/// Brak ZGODY na pytanie kończy czekanie od razu i mówi, czego brakuje. Powtarzanie pytania,
/// na które system nie pozwoli odpowiedzieć, jest wyłącznie czekaniem.
#[tokio::test]
async fn a_refused_permission_ends_the_wait_with_its_own_sentence() -> Result<(), Box<dyn Error>> {
    let bench = Bench::answering(
        "echo 'execution error: System Events — błąd: Nie masz zgody. (-1743)' >&2; exit 1",
    )?;
    let why = bench
        .start(native_with_readiness())
        .await
        .expect_err("a refused permission was treated as a window that will still appear");
    assert!(
        why.contains("permission on this machine, not a result about the application"),
        "the refusal reads like a verdict about the application: {why:?}"
    );
    Ok(())
}

/// Okno jest — cel natywny może zostać ogłoszony gotowym.
#[tokio::test]
async fn a_native_target_with_a_window_becomes_ready() -> Result<(), Box<dyn Error>> {
    let bench = Bench::answering("echo 1")?;
    bench.start(native_with_readiness()).await?;
    Ok(())
}

/// Cel WEBOWY nie jest o okno pytany w ogóle: interpreter, który zawsze odmawia, nie ma prawa
/// zatrzymać serwera, którego gotowość jest kompletna bez okna.
#[tokio::test]
async fn a_web_target_is_never_asked_about_a_window() -> Result<(), Box<dyn Error>> {
    let bench = Bench::answering("echo 'never asked' >&2; exit 1")?;
    let mut description = native_with_readiness();
    description.kind = TargetKind::Web;
    description.test_data_env = None;
    bench.start(description).await?;
    Ok(())
}

/// Aplikacja, która stoi i słucha na swoim porcie.
fn native_with_readiness() -> LaunchDescription {
    LaunchDescription {
        /* Prawdziwy nasłuch na przydzielonym porcie, trzymany do końca testu. `nc -l` odpada:
         * na macOS kończy się natychmiast, a wtedy kryterium mówiłoby o wyjściu procesu
         * zamiast o oknie. */
        command: "python3 -c \"import os,socket,time; s=socket.socket(); \
                  s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); \
                  s.bind(('127.0.0.1', int(os.environ['APP_PORT']))); s.listen(8); \
                  time.sleep(20)\""
            .to_owned(),
        kind: TargetKind::Native,
        test_data_env: Some("MURMUR_TEST_DATA".to_owned()),
        subdirectory: String::new(),
        environment: std::collections::BTreeMap::new(),
        required_env: Vec::new(),
        endpoints: vec![ServiceEndpointSpec {
            name: "app".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 0,
            port_env: Some("APP_PORT".to_owned()),
        }],
        readiness: Some(ReadinessSpec {
            kind: loadout_lib::workflow::ReadinessKind::Tcp,
            endpoint: "app".to_owned(),
            path: "/".to_owned(),
            timeout_seconds: 3,
            expected_status: 200,
        }),
    }
}

struct Bench {
    _project: tempfile::TempDir,
    workspace: std::path::PathBuf,
    processes: Arc<Processes>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        Self::answering("echo 1")
    }

    /// Ten sam świat, z podstawionym interpreterem odpowiedzi systemu — bez tego kryterium
    /// sądziłoby ZGODY tej maszyny zamiast tego kodu.
    fn answering(body: &str) -> Result<Self, Box<dyn Error>> {
        let project = tempfile::tempdir()?;
        let workspace = std::fs::canonicalize(project.path())?;
        let pretending = workspace.join("osascript");
        std::fs::create_dir_all(&workspace)?;
        std::fs::write(&pretending, format!("#!/bin/sh\n{body}\n"))?;
        std::fs::set_permissions(&pretending, std::fs::Permissions::from_mode(0o755))?;
        Ok(Self {
            _project: project,
            workspace,
            processes: Arc::new(Processes::confirming_windows_with(pretending)),
        })
    }

    /// Konfiguruje i startuje jedną usługę; oddaje zdanie odmowy albo nic.
    async fn start(&self, description: LaunchDescription) -> Result<(), String> {
        self.started(description).await.map(|_| ())
    }

    /// To samo, ale czeka na wyjście programu i oddaje to, co wypisał.
    async fn start_and_read(&self, description: LaunchDescription) -> Result<String, String> {
        let reference = self.started(description).await?;
        for _ in 0..100 {
            let page = self
                .processes
                .service_logs(&reference, 0, 4096, None)
                .map_err(|why| why.to_string())?;
            let said = page["text"].as_str().unwrap_or_default().to_owned();
            if !said.trim().is_empty() {
                return Ok(said);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err("the app never said anything".to_owned())
    }

    async fn started(&self, description: LaunchDescription) -> Result<ServiceRef, String> {
        // Bieg i usługa są identyfikowane UUID-em: rejestr usług odmawia wszystkiego innego.
        let run = "01a07000-0000-7000-8000-000000000001";
        let node = "s_app";
        let run_dir = self.workspace.join(".loadout/runs").join(run);
        let cwd = run_dir.join("work").join(node);
        std::fs::create_dir_all(&cwd).map_err(|why| why.to_string())?;
        let reference = ServiceRef {
            workspace: self.workspace.clone(),
            run_id: run.to_owned(),
            node_key: node.to_owned(),
            service_id: "01a07000-0000-7000-8000-0000000000a1".to_owned(),
            generation: 1,
        };
        self.processes
            .configure_description(
                &description,
                StepTag::new(run, node),
                ServiceOwner {
                    reference: reference.clone(),
                    run_dir,
                    cwd,
                    lifetime: ServiceLifetime::Window,
                },
            )
            .map_err(|why| why.to_string())?;
        self.processes
            .start_service(&reference, &CancellationToken::new())
            .await
            .map_err(|why| why.to_string())?;
        Ok(reference)
    }
}

/// Kafelek „uruchom i zostaw" umie POWIEDZIEĆ, że startuje aplikację natywną — i przenieść to
/// przez plik bez utraty ani jednego pola.
///
/// `targetKind`, nie `kind`: `Step` jest enumem tagowanym kluczem `kind`, więc pole o tej nazwie
/// w środku wariantu zderzyłoby się ze znacznikiem rodzaju kafelka. Kryterium sądzi ZAPIS
/// I ODCZYT, bo kolizja tagu nie jest widoczna w typie — dopiero w tym, co wyjdzie na dysk.
#[test]
fn a_serve_tile_says_what_it_starts_and_survives_the_file() -> Result<(), Box<dyn Error>> {
    let written = serde_json::json!({
        "kind": "serve", "id": "s_app", "name": "The app",
        "command": "npm run app",
        "targetKind": "native",
        "testDataEnv": "MURMUR_TEST_DATA",
        "folder": {"use": "same-copy"}, "at": {"x": 0, "y": 0}
    });
    let step: loadout_lib::workflow::Step = serde_json::from_value(written.clone())?;
    let loadout_lib::workflow::Step::Serve(serve) = &step else {
        return Err("the tile stopped being a serve step".into());
    };
    assert_eq!(serve.target_kind, TargetKind::Native);
    assert_eq!(serve.test_data_env.as_deref(), Some("MURMUR_TEST_DATA"));

    let again = serde_json::to_value(&step)?;
    assert_eq!(
        again.get("kind").and_then(serde_json::Value::as_str),
        Some("serve"),
        "the tile's own kind was overwritten by what it starts"
    );
    assert_eq!(
        again.get("targetKind").and_then(serde_json::Value::as_str),
        Some("native"),
        "saving the workflow dropped what this tile starts"
    );
    assert_eq!(
        again.get("testDataEnv").and_then(serde_json::Value::as_str),
        Some("MURMUR_TEST_DATA")
    );

    // Plik BEZ tych pól ma wyglądać dokładnie tak, jak wyglądał przed ich dołożeniem.
    let plain = serde_json::json!({
        "kind": "serve", "id": "s_web", "name": "Dev server", "command": "npm run dev",
        "folder": {"use": "project"}, "at": {"x": 0, "y": 0}
    });
    let web: loadout_lib::workflow::Step = serde_json::from_value(plain)?;
    let round = serde_json::to_value(&web)?;
    assert!(
        round.get("targetKind").is_none() && round.get("testDataEnv").is_none(),
        "a workflow that never mentioned these fields grew them on save: {round}"
    );
    Ok(())
}

/// P-02, DZIURA ZNALEZIONA WE WŁASNEJ PRACY (2026-09-07): `testDataEnv` omijało regułę
/// zastrzeżonych zmiennych.
///
/// `isolated_data` ustawia tę zmienną na procesie potomnym, a walidator nie oglądał jej nigdy —
/// więc `"HOME"` podmieniał aplikacji katalog domowy, `"PATH"` jej wyszukiwanie programów,
/// a `"DYLD_INSERT_LIBRARIES"` wstrzykiwał do niej bibliotekę. Dokładnie te trzy są od dawna
/// zabronione dla zmiennych wpisanych do `environment` (`a_replaced_home_folder_is_refused`
/// wyżej). Jedna reguła z dwiema furtkami to reguła, której nie ma.
///
/// Sądzone od strony STARTU, tak jak jej starsza siostra: to jest ta strona, po której instancja
/// testowa naprawdę mogłaby napisać do katalogu człowieka.
#[tokio::test]
async fn a_test_data_setting_cannot_replace_a_reserved_variable() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    for name in [
        "HOME",
        "PATH",
        "DYLD_INSERT_LIBRARIES",
        "LOADOUT_RUN_ID",
        "SHELL",
    ] {
        let why = bench
            .start(LaunchDescription {
                command: "true".to_owned(),
                kind: TargetKind::Native,
                test_data_env: Some(name.to_owned()),
                subdirectory: String::new(),
                environment: std::collections::BTreeMap::new(),
                required_env: Vec::new(),
                endpoints: Vec::new(),
                readiness: None,
            })
            .await
            .unwrap_err();
        assert!(
            why.contains(&format!("cannot replace the environment variable {name}")),
            "a test instance was allowed to replace {name} through its test data setting: {why:?}"
        );
    }
    Ok(())
}

/// Ta sama odmowa na PŁÓTNIE, zanim ktokolwiek uruchomi bieg.
///
/// Odmowa dochodząca dopiero przy starcie każe się jej dowiedzieć z biegu, który zapłacił już
/// za kroki przed tym (niezmiennik 29).
#[test]
fn a_tile_asking_for_a_reserved_variable_is_named_on_the_canvas() {
    let file: loadout_lib::workflow::WorkflowFile = serde_json::from_value(serde_json::json!({
        "format": 1, "id": "wf", "name": "W",
        "steps": [{
            "kind": "serve", "id": "s_app", "name": "The app", "command": "./app",
            "targetKind": "native", "testDataEnv": "HOME",
            "folder": {"use": "project"}, "at": {"x": 0, "y": 0}
        }],
        "links": []
    }))
    .expect("the tile does not load");
    let said = loadout_lib::workflow::check::check(&file);
    assert!(
        said.iter().any(|note| note.message.contains("HOME")),
        "the canvas says nothing about a tile that replaces the app's home folder: {said:?}"
    );
}
