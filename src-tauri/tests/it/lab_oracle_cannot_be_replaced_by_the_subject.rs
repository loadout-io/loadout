//! WF-18: rzeczywisty pomiar nie pozwala procesowi subjectu przepisać wyroczni.
use loadout_lib::{
    commands::{RunControl, RunDeps},
    store::Store,
};
use serde_json::{Value, json};
use std::{error::Error, fs, sync::Arc};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_protected_comparison_runs_real_checks_without_exposing_its_host_or_other_scopes()
-> Result<(), Box<dyn Error>> {
    exercise(None).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_protected_claude_step_and_ordinary_check_reach_the_frozen_examiner()
-> Result<(), Box<dyn Error>> {
    exercise(Some("claude")).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_protected_codex_step_and_ordinary_check_reach_the_frozen_examiner()
-> Result<(), Box<dyn Error>> {
    exercise(Some("codex")).await
}

async fn exercise(vendor: Option<&str>) -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("library");
    fs::create_dir_all(project.join(".loadout/workflows"))?;
    fs::create_dir_all(project.join(".loadout/evals"))?;
    fs::create_dir_all(&home)?;
    fs::write(project.join("oracle.txt"), "DO NOT CHANGE")?;
    let subject = temp.path().join("subject.py");
    fs::write(
        &subject,
        r#"import os,pathlib,subprocess,sys
host=pathlib.Path(sys.argv[1]); work=pathlib.Path.cwd(); run=work.parent.parent
refused=0
for action in [lambda:(host/'oracle.txt').write_text('changed'),
               lambda:os.rename(host/'oracle.txt',host/'renamed.txt'),
               lambda:os.chmod(host/'oracle.txt',0o600),
               lambda:(run/'run.json').read_bytes(),
               lambda:list((run/'work').iterdir()),
               lambda:(host/'.git/HEAD').read_bytes()]:
    try: action()
    except PermissionError: refused+=1
    except FileNotFoundError: pass
child=subprocess.run([sys.executable,'-c',"import pathlib,sys;pathlib.Path(sys.argv[1]).write_text('child')",str(host/'oracle.txt')],capture_output=True)
if child.returncode!=0 and b'PermissionError' in child.stderr: refused+=1
(work/'result.txt').write_text(str(refused))
print('1 passed' if refused==7 else '0 passed')
"#,
    )?;
    // Prawdziwa wspólna baza Gita: ochrona nie może zostawić dostępu przez jej objects/HEAD.
    let initialized = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&project)
        .status()?;
    assert!(initialized.success());
    let committed = std::process::Command::new("git")
        .current_dir(&project)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "base",
        ])
        .status()?;
    assert!(committed.success());
    let command = format!(
        "/usr/bin/python3 '{}' '{}'",
        subject.display(),
        project.display()
    );
    let mut workflow = json!({"format":1,"id":"subject","name":"Subject","steps":[{
        "kind":"check","id":"out","name":"Subject behavior","command":command,"proof":"(\\d+) passed",
        "folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}],"links":[]});
    let mut native: Option<Arc<dyn loadout_lib::engine::drivers::AgentDriver>> = None;
    if let Some(vendor) = vendor {
        use loadout_lib::{
            engine::drivers::{DriverConfiguration, claude::ClaudeDriver, codex::CodexDriver},
            library::agents::{Agent, Vendor, write_agent_file},
        };
        let mut agent = Agent::example();
        agent.runs_with = if vendor == "claude" {
            Vendor::ClaudeCode
        } else {
            Vendor::Codex
        };
        agent.reaches_the_web = false;
        write_agent_file(&home.join("agents"), &agent, None)?;
        let cli = temp.path().join("model-cli");
        let (read, reply) = if vendor == "claude" {
            (
                "IFS= read -r prompt",
                r#"printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","session_id":"wf18-session","num_turns":1,"total_cost_usd":0,"duration_ms":1,"result":"The file operations ran."}'"#,
            )
        } else {
            (
                "/bin/cat >/dev/null",
                r#"printf '%s\n' '{"type":"thread.started","thread_id":"wf18-thread"}' '{"type":"item.completed","item":{"id":"message","type":"agent_message","text":"The file operations ran."}}' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'"#,
            )
        };
        fs::write(
            &cli,
            format!("#!/bin/sh\n{read}\n{command} >/dev/null\n{reply}\n"),
        )?;
        loadout_lib::engine::supervisor::set_executable_file(&std::fs::File::open(&cli)?, true)?;
        let driver: Arc<dyn loadout_lib::engine::drivers::AgentDriver> = if vendor == "claude" {
            Arc::new(ClaudeDriver::with_binary(cli))
        } else {
            let native_home = temp.path().join("synthetic-auth");
            fs::create_dir_all(&native_home)?;
            fs::write(
                native_home.join("auth.json"),
                r#"{"OPENAI_API_KEY":"synthetic-not-a-real-credential"}"#,
            )?;
            let driver: Arc<dyn loadout_lib::engine::drivers::AgentDriver> =
                Arc::new(CodexDriver::with_binary(cli));
            driver
                .configured(&DriverConfiguration {
                    environment: vec![("CODEX_HOME".to_owned(), native_home.into_os_string())],
                    ..DriverConfiguration::default()
                })
                .ok_or("Codex configuration is missing")?
        };
        native = Some(driver);
        workflow["steps"] = json!([
            {"kind":"agent","id":"writer","name":"Subject behavior","agent":agent.id,"instructions":"Exercise file access.","overrides":{},"folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}},
            {"kind":"check","id":"out","name":"Check the actual file","command":"/usr/bin/python3 -c 'import pathlib; assert pathlib.Path(\"result.txt\").read_text()==\"7\"; print(\"1 passed\")'","proof":"(\\d+) passed","folder":{"use":"same-copy"},"at":{"x":200,"y":0}}
        ]);
        workflow["links"] = json!([{"from":"writer","to":"out"}]);
    }
    fs::write(
        project.join(".loadout/workflows/subject.json"),
        serde_json::to_vec(&workflow)?,
    )?;
    let examiner_source = r"import json,pathlib,sys
data=json.load(sys.stdin)
root=pathlib.Path(data['results'][0]['files']['root'])
ok=(root/'result.txt').read_text()=='7'
print(json.dumps(dict(format=1,status='completed',passed=int(ok),failed=int(not ok),reason='' if ok else 'File boundaries failed.')))
";
    let examiner = temp.path().join("examiner.py");
    fs::write(&examiner, examiner_source)?;
    let set = json!({"format":2,"id":"protected","name":"Protected comparison","subject":{"kind":"workflow","id":"subject"},
        "protected":true,"cases":[{"id":"case","name":"Case","task":"Do the work","status":"in-use",
            "command":format!("/usr/bin/python3 '{}'",examiner.display()),"proof":"unused","proofMode":"external-assessment-v1",
            "examiner":{"kind":"python","program":"/usr/bin/python3","source":examiner_source}}],
        "variants":[{"id":"variant","name":"Variant","workflow":{"id":"subject","outputStep":"out"}}]});
    fs::write(
        project.join(".loadout/evals/protected.json"),
        serde_json::to_vec(&set)?,
    )?;
    let planned =
        loadout_lib::commands::lab::plan_a_run_with_library(&home, &project, "protected", 2)?;
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &home,
        project: &project,
        store: &store,
        drivers: Arc::new(move |_| {
            Arc::clone(
                native
                    .as_ref()
                    .expect("the Check-only fixture must never request a model"),
            )
        }),
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    let (sink, _) = loadout_lib::ipc::line_channel(4096);
    let report =
        loadout_lib::commands::run::run_workflow_inner(&deps, &planned.request, sink).await?;
    let book: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert!(
        book["steps"]
            .as_array()
            .ok_or("missing steps")?
            .iter()
            .all(|one| one["status"] == "succeeded" && one["death_proof"] == true),
        "a protected comparison must actually execute and enforce its boundary, not refuse every process: {}",
        book["steps"]
    );
    assert_eq!(
        fs::read_to_string(project.join("oracle.txt"))?,
        "DO NOT CHANGE"
    );
    assert!(!project.join("renamed.txt").exists());
    let board = loadout_lib::commands::lab::read_board_inner(&project, "protected", 1)?;
    assert_eq!((board.runs[0].passed, board.runs[0].judged), (1, 1));
    Ok(())
}

#[test]
fn a_frozen_external_examiner_does_not_need_a_fake_shell_command_or_pass_pattern()
-> Result<(), Box<dyn Error>> {
    let graph: loadout_lib::workflow::WorkflowFile = serde_json::from_value(
        json!({"format":1,"id":"source","name":"Source","steps":[{
        "kind":"check","id":"out","name":"Work","command":"printf '1 passed\\n'","proof":"(\\d+) passed","folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}],"links":[]}),
    )?;
    let set: loadout_lib::lab::EvalSet = serde_json::from_value(
        json!({"format":2,"id":"trusted","name":"Trusted","subject":{"kind":"workflow","id":"source"},"protected":true,
        "cases":[{"id":"case","name":"Case","task":"Work","status":"in-use","command":"","proof":"","proofMode":"external-assessment-v1","examiner":{"kind":"python","program":"/usr/bin/python3","source":"print('trusted assertions')"}}],
        "variants":[{"id":"one","name":"One","workflow":{"id":"source","outputStep":"out"}}]}),
    )?;
    let compiled = loadout_lib::lab::workflow_plan::compose(
        &[graph],
        &set,
        "measure".into(),
        "Measure".into(),
    )
    .expect("a frozen external examiner must not require an unused shell command or pattern");
    let grader = compiled.steps.last().ok_or("missing examiner")?;
    let serialized = serde_json::to_value(grader)?;
    assert_eq!(serialized["command"], "");
    assert_eq!(serialized["proof"], "");
    let notes = loadout_lib::workflow::check::check_to_run(&compiled);
    assert!(
        !notes
            .iter()
            .any(|note| note.level == loadout_lib::workflow::check::Level::Problem),
        "the actual Start validator rejected its executable examiner: {notes:?}"
    );
    let temporary = tempfile::tempdir()?;
    loadout_lib::lab::file::save(&set, &temporary.path().join("set.json"), None)
        .expect("the accepted examiner must also survive the actual set writer");
    Ok(())
}
