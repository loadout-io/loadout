//! WF-18: prawdziwe `ClaudeDriver`, `CodexDriver` i `CommandDriver`; tylko binarka CLI jest dublem.
//! Każdy proces próbuje write/rename/chmod, odczytu obcej komórki i ucieczki przez potomka.
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::drivers::codex::CodexDriver;
use loadout_lib::engine::drivers::command::{CheckHow, CheckSpec, CommandDriver};
use loadout_lib::engine::drivers::{
    AgentDriver, DriverConfiguration, Policy, RunSpec, StepSettings,
};
use loadout_lib::engine::supervisor::{self, FilesystemFence, GroupProof};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Bench {
    _root: tempfile::TempDir,
    root: PathBuf,
    cell: PathBuf,
    binary: PathBuf,
    fence: FilesystemFence,
}

fn readiness_driver(vendor: &str, home: &Path, native: &Path) -> Arc<dyn AgentDriver> {
    let configuration = DriverConfiguration {
        environment: vec![
            ("HOME".to_owned(), home.as_os_str().to_os_string()),
            ("CODEX_HOME".to_owned(), native.as_os_str().to_os_string()),
        ],
        ..DriverConfiguration::default()
    };
    let driver: Arc<dyn AgentDriver> = if vendor == "codex" {
        Arc::new(
            CodexDriver::with_binary(PathBuf::from("/usr/bin/false"))
                .with_configuration(configuration),
        )
    } else {
        Arc::new(
            ClaudeDriver::with_binary(PathBuf::from("/usr/bin/false"))
                .with_configuration(configuration),
        )
    };
    loadout_lib::driver_with_frozen_search(driver, "/usr/bin:/bin".into())
}

fn names_in(path: &Path) -> Result<Vec<std::ffi::OsString>, Box<dyn Error>> {
    let mut names = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}

#[tokio::test]
async fn protected_readiness_is_read_only_through_the_real_codex_wrapper()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let native = root.path().join("native");
    fs::create_dir(&native)?;
    let auth = "synthetic sign-in bytes, never used for a request\n";
    fs::write(native.join("auth.json"), auth)?;
    let before_root = names_in(root.path())?;
    let before_native = names_in(&native)?;
    let driver = readiness_driver("codex", root.path(), &native);
    let first = driver.protected_readiness();
    let second = driver.protected_readiness();
    // Probe korzysta z dokładnie tego samego nadzoru co Start i nie wymaga run-dir.
    FilesystemFence::new(vec![], vec![], vec![])?
        .prove_available()
        .await?;
    assert_eq!(names_in(root.path())?, before_root);
    assert_eq!(names_in(&native)?, before_native);
    assert_eq!(fs::read_to_string(native.join("auth.json"))?, auth);
    assert!(
        matches!(first, Some(Ok(()))),
        "Codex readiness did not reach its actual adapter: {first:?}"
    );
    assert!(
        matches!(second, Some(Ok(()))),
        "a repeated preview must remain read-only: {second:?}"
    );
    Ok(())
}

#[test]
fn protected_readiness_refuses_missing_or_linked_codex_sign_in_without_preparing_state()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let native = root.path().join("native");
    fs::create_dir(&native)?;
    let driver = readiness_driver("codex", root.path(), &native);
    let missing = driver.protected_readiness();
    assert_eq!(
        names_in(root.path())?,
        vec![std::ffi::OsString::from("native")]
    );
    assert!(names_in(&native)?.is_empty());
    assert!(
        matches!(missing, Some(Err(_))),
        "missing sign-in must be an explicit read-only refusal: {missing:?}"
    );
    let foreign = root.path().join("foreign-auth");
    fs::write(&foreign, "synthetic bytes\n")?;
    supervisor::link(&foreign, &native.join("auth.json"))?;
    let linked = driver.protected_readiness();
    assert!(
        matches!(linked, Some(Err(_))),
        "preview followed a foreign sign-in alias: {linked:?}"
    );
    assert_eq!(fs::read_to_string(&foreign)?, "synthetic bytes\n");
    assert_eq!(fs::read_link(native.join("auth.json"))?, foreign);
    assert_eq!(
        names_in(&native)?,
        vec![std::ffi::OsString::from("auth.json")]
    );
    Ok(())
}

#[test]
fn protected_readiness_reaches_claude_without_writing_settings_or_private_state()
-> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let driver = readiness_driver("claude", root.path(), &root.path().join("unused"));
    let readiness = driver.protected_readiness();
    assert!(names_in(root.path())?.is_empty());
    assert!(
        matches!(readiness, Some(Ok(()))),
        "Claude capability did not reach its adapter: {readiness:?}"
    );
    Ok(())
}

impl Bench {
    fn new(vendor: &str) -> Result<Self, Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let root = fs::canonicalize(directory.path())?;
        let measurement = root.join("measurement");
        let cell = measurement.join("cell-one");
        let foreign = measurement.join("cell-two");
        let oracle = measurement.join("examiner");
        fs::create_dir_all(&cell)?;
        fs::create_dir_all(&foreign)?;
        fs::create_dir_all(&oracle)?;
        for file in ["write", "rename", "chmod", "child"] {
            fs::write(oracle.join(file), "original examiner\n")?;
        }
        fs::write(foreign.join("answer"), "private answer from another cell\n")?;
        let probe = cell.join("probe.sh");
        fs::write(
            &probe,
            format!(
                r#"#!/bin/sh
oracle={oracle}
foreign={foreign}
printf 'own output\n' > own-output
printf 'own_allowed\n' > facts
if /bin/sh -c 'printf changed > "$1"' sh "$oracle/write" 2>/dev/null; then echo write_escaped; else echo write_refused; fi >> facts
printf replacement > replacement
if /bin/mv replacement "$oracle/rename" 2>/dev/null; then echo rename_escaped; else echo rename_refused; fi >> facts
if /bin/chmod 600 "$oracle/chmod" 2>/dev/null; then echo chmod_escaped; else echo chmod_refused; fi >> facts
if /bin/cat "$foreign/answer" >/dev/null 2>&1; then echo foreign_escaped; else echo foreign_refused; fi >> facts
if /bin/sh -c '/bin/sh -c '\''printf changed > "$1"'\'' sh "$1"' sh "$oracle/child" 2>/dev/null; then echo child_write_escaped; else echo child_write_refused; fi >> facts
if /bin/sh -c '/bin/sh -c '\''/bin/cat "$1" > /dev/null'\'' sh "$1"' sh "$foreign/answer" 2>/dev/null; then echo child_read_escaped; else echo child_read_refused; fi >> facts
/bin/ln -s "$foreign/answer" foreign-alias
if /bin/cat foreign-alias >/dev/null 2>&1; then echo alias_escaped; else echo alias_refused; fi >> facts
printf '1 passed\n'
"#,
                oracle = quote(&oracle),
                foreign = quote(&foreign)
            ),
        )?;
        let binary = root.join("fake-cli");
        let (read, reply) = if vendor == "claude" {
            (
                "IFS= read -r prompt",
                r#"printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","session_id":"wf18-session","num_turns":1,"total_cost_usd":0,"duration_ms":1,"result":"The file operations ran."}'"#,
            )
        } else {
            (
                "/bin/cat > /dev/null",
                r#"printf '%s\n' '{"type":"thread.started","thread_id":"wf18-thread"}' '{"type":"item.completed","item":{"id":"message","type":"agent_message","text":"The file operations ran."}}' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'"#,
            )
        };
        fs::write(
            &binary,
            format!(
                "#!/bin/sh\n{read}\n/bin/sh {} >/dev/null\n{reply}\n",
                quote(&probe)
            ),
        )?;
        supervisor::set_executable_file(&std::fs::File::open(&binary)?, true)?;
        // Szeroki readable ancestor celowo NIE może otworzyć ukrytego pomiaru.
        let fence = FilesystemFence::new(
            vec![cell.clone()],
            vec![root.clone(), cell.clone()],
            vec![measurement],
        )?;
        Ok(Self {
            _root: directory,
            root,
            cell,
            binary,
            fence,
        })
    }

    fn assert_protected(&self) -> Result<(), Box<dyn Error>> {
        let facts = fs::read_to_string(self.cell.join("facts"))?;
        assert_eq!(
            facts,
            "own_allowed\nwrite_refused\nrename_refused\nchmod_refused\nforeign_refused\nchild_write_refused\nchild_read_refused\nalias_refused\n",
            "a real process escaped its file boundary: {facts}"
        );
        assert_eq!(
            fs::read_to_string(self.cell.join("own-output"))?,
            "own output\n"
        );
        for file in ["write", "rename", "chmod", "child"] {
            assert_eq!(
                fs::read_to_string(self.root.join("measurement/examiner").join(file))?,
                "original examiner\n"
            );
        }
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claude_and_its_shell_children_cannot_change_the_examiner_or_read_another_cell()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("claude")?;
    agent(
        &bench,
        Arc::new(ClaudeDriver::with_binary(bench.binary.clone())),
    )
    .await?;
    bench.assert_protected()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn codex_and_its_shell_children_cannot_change_the_examiner_or_read_another_cell()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("codex")?;
    agent(
        &bench,
        Arc::new(CodexDriver::with_binary(bench.binary.clone())),
    )
    .await?;
    bench.assert_protected()
}

async fn agent(bench: &Bench, driver: Arc<dyn AgentDriver>) -> Result<(), Box<dyn Error>> {
    let driver = driver
        .with_filesystem_fence(&bench.fence)
        .ok_or("the real driver did not carry the file boundary")?;
    let (events, _inbox) = mpsc::channel(128);
    let mut handle = driver
        .start(
            RunSpec {
                run_id: uuid::Uuid::now_v7(),
                cwd: bench.cell.clone(),
                prompt: "Exercise the file boundary.".to_owned(),
                model: None,
                system_append: None,
                policy: Policy::Unrestricted,
                reaches_the_web: false,
                tools: None,
                extra_dirs: Vec::new(),
                resume: None,
            },
            events,
        )
        .await?;
    let group = handle
        .group()
        .ok_or("real wrapper did not start a process")?;
    let outcome = tokio::time::timeout(Duration::from_secs(5), handle.wait()).await;
    let proof = handle.cancel().await;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "fixture cleanup: {proof:?}"
    );
    assert!(supervisor::group_is_empty(group.pgid));
    let outcome = outcome??;
    assert!(
        outcome.ok,
        "the protocol double failed instead of exercising the boundary: {outcome:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_ordinary_check_and_its_children_have_the_same_file_boundary()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("check")?;
    let end = CommandDriver::new()
        .with_filesystem_fence(&bench.fence)
        .run(
            &CheckSpec {
                command: "/bin/sh probe.sh".to_owned(),
                proof: r"(\d+) passed".to_owned(),
                cwd: bench.cell.clone(),
                required_tests: Vec::new(),
            },
            &CancellationToken::new(),
        )
        .await?;
    assert!(supervisor::group_is_empty(end.group.pgid));
    assert!(
        matches!(end.how,CheckHow::Ran(ref result) if result.passed),
        "fixture failed before measuring permissions: {end:?}"
    );
    bench.assert_protected()
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exact_private_files_and_path_traversal_do_not_open_sibling_content()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new("codex")?;
    let private = bench.root.join("measurement/runtime/codex/s_one");
    fs::create_dir_all(&private)?;
    let instructions = private.join("instructions.txt");
    fs::write(&instructions, "the selected instructions\n")?;
    fs::write(private.join("another-agent.txt"), "not selected\n")?;
    let fence = FilesystemFence::new(
        vec![bench.cell.clone()],
        vec![bench.cell.clone()],
        vec![bench.root.join("measurement")],
    )?
    .reading_files(vec![instructions.clone()])?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command.args([
        "--exact",
        "processes_cannot_rewrite_their_examiner::native_path_canonicalization_probe",
        "--ignored",
        "--nocapture",
    ]);
    let mut process = supervisor::spawn_tagged_with_fence(
        command,
        supervisor::StdinPlan::Null,
        &[(
            "LOADOUT_FENCE_PATH_FIXTURE".to_owned(),
            instructions.into_os_string(),
        )],
        None,
        Some(&fence),
    )?;
    let result = tokio::time::timeout(Duration::from_secs(5), process.wait()).await;
    let proof = process.stop(Duration::from_millis(250)).await;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "fixture cleanup: {proof:?}"
    );
    assert!(
        result??.success(),
        "the private exact file or its native canonical path was unavailable, or a sibling became readable"
    );
    Ok(())
}

#[test]
#[ignore = "subprocess fixture invoked only with its exact private path"]
fn native_path_canonicalization_probe() -> Result<(), Box<dyn Error>> {
    let path =
        PathBuf::from(std::env::var_os("LOADOUT_FENCE_PATH_FIXTURE").ok_or("not a fixture child")?);
    assert_eq!(fs::canonicalize(&path)?, path);
    assert_eq!(fs::read_to_string(&path)?, "the selected instructions\n");
    let parent = path.parent().ok_or("fixture path has no parent")?;
    assert!(
        fs::read_dir(parent).is_err(),
        "path traversal exposed the directory listing"
    );
    assert!(
        fs::read_to_string(parent.join("another-agent.txt")).is_err(),
        "the exact file exception exposed another agent's file"
    );
    assert!(
        fs::write(&path, "changed").is_err(),
        "a readable instruction became writable"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn prepared_codex_state_can_write_only_its_own_runtime_and_never_copy_auth()
-> Result<(), Box<dyn Error>> {
    let mut bench = Bench::new("codex")?;
    let native_home = bench.root.join("native-home");
    let native_auth = native_home.join(".codex/auth.json");
    fs::create_dir_all(native_auth.parent().ok_or("no auth parent")?)?;
    fs::write(
        &native_auth,
        "{\"OPENAI_API_KEY\":\"unit-test-not-a-real-key\"}\n",
    )?;
    let settings = StepSettings {
        dir: bench.root.join("measurement/run"),
        work_key: "s_one".to_owned(),
        memory: bench.root.join("measurement/run/mem/s_one"),
        deny: Vec::new(),
    };
    fs::create_dir_all(&settings.memory)?;
    let driver = CodexDriver::with_binary(bench.binary.clone())
        .configured(&DriverConfiguration {
            arguments: Vec::new(),
            environment: vec![("HOME".to_owned(), native_home.as_os_str().to_os_string())],
            servers: Vec::new(),
        })
        .ok_or("fixture driver configuration missing")?;
    let prepared = driver
        .prepare_protected_step(&settings)
        .ok_or("Codex has no protected private state")??;
    let private = settings.dir.join("codex/s_one");
    // Sprawdzenie PRZED procesem: pomyłka wyboru auth nigdy nie czyta prawdziwego sekretu.
    assert_eq!(fs::read_link(private.join("auth.json"))?, native_auth);
    let script = r#"#!/bin/sh
/bin/cat >/dev/null
printf '%s\n' "$CODEX_HOME" > seen-codex-home
printf state > "$CODEX_HOME/sessions/own-state"
printf temp > "$TMPDIR/own-temp"
if /bin/sh -c 'printf changed > "$1/auth.json.tmp"' sh "$CODEX_HOME" 2>/dev/null; then echo auth_temp_escaped; else echo auth_temp_refused; fi > state-facts
if /bin/rm "$CODEX_HOME/auth.json" 2>/dev/null; then echo auth_unlink_escaped; else echo auth_unlink_refused; fi >> state-facts
printf replacement > replacement-auth
if /bin/mv replacement-auth "$CODEX_HOME/auth.json" 2>/dev/null; then echo auth_replace_escaped; else echo auth_replace_refused; fi >> state-facts
if /bin/sh -c 'printf changed > "$1/outside"' sh "$HOME" 2>/dev/null; then echo home_escaped; else echo home_refused; fi >> state-facts
printf '%s\n' '{"type":"thread.started","thread_id":"private-state-thread"}' '{"type":"item.completed","item":{"id":"message","type":"agent_message","text":"The private state was exercised."}}' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'
"#;
    fs::write(&bench.binary, script)?;
    let mut writable = prepared.writable_roots;
    writable.push(bench.cell.clone());
    let mut readable = prepared.readable_roots;
    readable.push(bench.cell.clone());
    bench.fence = FilesystemFence::new(writable, readable, vec![bench.root.join("measurement")])?
        .reading_files(prepared.readable_files)?;
    agent(&bench, prepared.driver).await?;
    assert_eq!(
        fs::read_to_string(bench.cell.join("seen-codex-home"))?,
        format!("{}\n", private.display())
    );
    assert_eq!(
        fs::read_to_string(private.join("sessions/own-state"))?,
        "state"
    );
    assert_eq!(
        fs::read_to_string(bench.cell.join("state-facts"))?,
        "auth_temp_refused\nauth_unlink_refused\nauth_replace_refused\nhome_refused\n"
    );
    assert_eq!(fs::read_link(private.join("auth.json"))?, native_auth);
    assert!(!private.join("auth.json.tmp").exists());
    Ok(())
}

/// Niepłatny proof lokalnie zainstalowanego CLI. Nie czyta rzeczywistego auth użytkownika,
/// nie wywołuje modelu ani odświeżenia poświadczeń. Root uruchamia jawnie z --ignored.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the installed native Codex CLI; no model or network call"]
async fn native_codex_reads_a_read_only_auth_link_with_private_state() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new("codex")?;
    let native = bench.root.join("native-auth");
    let private = bench.root.join("measurement/runtime/codex/s_one");
    fs::create_dir_all(&native)?;
    fs::create_dir_all(&private)?;
    let auth = "{\"OPENAI_API_KEY\":\"sk-test-only-not-a-real-key-123456789\"}\n";
    fs::write(native.join("auth.json"), auth)?;
    supervisor::link(&native.join("auth.json"), &private.join("auth.json"))?;
    let mut writable = vec![bench.cell.clone()];
    for name in [
        ".tmp",
        "log",
        "sessions",
        "shell_snapshots",
        "sqlite",
        "skills",
    ] {
        let directory = private.join(name);
        fs::create_dir_all(&directory)?;
        writable.push(directory);
    }
    let fence = FilesystemFence::new(
        writable,
        vec![private.clone(), bench.cell.clone()],
        vec![bench.root.join("measurement")],
    )?;
    let mut command = tokio::process::Command::new("codex");
    command.current_dir(&bench.cell);
    command.args([
        "-c",
        &format!("log_dir={}", serde_json::to_string(&private.join("log"))?),
        "-c",
        &format!(
            "sqlite_home={}",
            serde_json::to_string(&private.join("sqlite"))?
        ),
        "login",
        "status",
    ]);
    let mut process = supervisor::spawn_tagged_with_fence(
        command,
        supervisor::StdinPlan::Null,
        &[
            ("CODEX_HOME".to_owned(), private.as_os_str().to_os_string()),
            ("TMPDIR".to_owned(), private.join(".tmp").into_os_string()),
        ],
        None,
        Some(&fence),
    )?;
    let mut stderr = process.stderr().ok_or("native fixture lost stderr")?;
    let complaints = tokio::spawn(async move {
        use tokio::io::AsyncReadExt as _;
        let mut text = String::new();
        stderr.read_to_string(&mut text).await.map(|_| text)
    });
    let waited = tokio::time::timeout(Duration::from_secs(5), process.wait()).await;
    let proof = process.stop(Duration::from_millis(250)).await;
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "native probe did not stop: {proof:?}"
    );
    let complaints = complaints.await??;
    assert!(
        waited??.success(),
        "native Codex could not read its synthetic auth through the protected private state: {complaints}"
    );
    assert_eq!(
        fs::read_link(private.join("auth.json"))?,
        native.join("auth.json")
    );
    assert_eq!(fs::read_to_string(native.join("auth.json"))?, auth);
    Ok(())
}
