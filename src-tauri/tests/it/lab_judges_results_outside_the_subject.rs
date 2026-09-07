//! WF-18: wyrocznia wykonuje własne asercje, a obcy stdout pozostaje danymi.

#![allow(clippy::too_many_lines)]

use loadout_lib::commands::{Drivers, RunControl, RunDeps};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use std::{error::Error, fs, sync::Arc};

#[tokio::test]
async fn external_assessment_does_not_treat_subject_stdout_or_a_broken_examiner_as_a_pass()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let project = temp.path().join("project");
    let home = temp.path().join("home");
    fs::create_dir_all(project.join(".loadout"))?;
    fs::create_dir_all(&home)?;
    let drivers: Drivers = Arc::new(|_| unreachable!("a Check must not request a paid agent"));
    let store = Store::open(&project.join(".loadout/loadout.db"))?;
    let deps = RunDeps {
        home: &home,
        project: &project,
        store: &store,
        drivers,
        control: RunControl::new(),
        processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
    };
    for (name, subject, examiner, expected, cause) in [
        ("good", "print('42')", "normal", "succeeded", "completed"),
        (
            "lie",
            "import os; print('100 passed', flush=True); os._exit(0)",
            "normal",
            "failed",
            "task-failed",
        ),
        ("syntax", "def broken(:", "normal", "failed", "task-failed"),
        (
            "missing-library",
            "print('42')",
            "missing",
            "failed",
            "infrastructure-failed",
        ),
        (
            "zero-tests",
            "print('42')",
            "zero",
            "failed",
            "infrastructure-failed",
        ),
        (
            "oversized",
            "print('42')",
            "oversized",
            "failed",
            "infrastructure-failed",
        ),
        (
            "two-replies",
            "print('42')",
            "duplicate",
            "failed",
            "infrastructure-failed",
        ),
        (
            "stderr-lie",
            "print('42')",
            "stderr",
            "failed",
            "infrastructure-failed",
        ),
    ] {
        let subject_path = project.join(format!("{name}.py"));
        fs::write(&subject_path, subject)?;
        let examiner_path = temp.path().join(format!("examiner-{name}.py"));
        let python = match examiner {
            "missing" => "import loadout_nonexistent_examiner_dependency\n".to_owned(),
            "zero" => "print('{\"format\":1,\"status\":\"completed\",\"passed\":0,\"failed\":0,\"reason\":\"no tests\"}')\n".to_owned(),
            "oversized" => "print('x' * 65537)\n".to_owned(),
            "duplicate" => "print('{\"format\":1,\"status\":\"completed\",\"passed\":1,\"failed\":0,\"reason\":\"\"}' * 2)\n".to_owned(),
            "stderr" => "import sys\nprint('{\"format\":1,\"status\":\"completed\",\"passed\":1,\"failed\":0,\"reason\":\"\"}', file=sys.stderr)\n".to_owned(),
            _ => format!(r"import json, subprocess, sys
child = subprocess.run([sys.executable, {}], capture_output=True, text=True)
if child.returncode != 0:
    answer = dict(format=1, status='subject-cannot-load', passed=0, failed=1, reason='The program could not be loaded.')
else:
    correct = child.stdout.strip() == '42'
    answer = dict(format=1, status='completed', passed=int(correct), failed=int(not correct), reason='' if correct else 'The program did not produce 42.')
print(json.dumps(answer))
", serde_json::to_string(&subject_path.to_string_lossy())?),
        };
        fs::write(&examiner_path, python)?;
        let command = format!(
            "/usr/bin/python3 '{}'",
            examiner_path.to_string_lossy().replace('\'', "'\\''")
        );
        let workflow = json!({"format":1,"id":format!("assessment-{name}"),"name":"Independent assessment",
            "steps":[{"kind":"check","id":"check","name":"Independent assessment","command":command,
                "proof":"(\\d+) passed","proofMode":"external-assessment-v1", "folder":{"use":"project"},
                "at":{"x":0,"y":0}}],"links":[]});
        let path = project.join(format!("{name}.json"));
        fs::write(&path, serde_json::to_vec(&workflow)?)?;
        let request = loadout_lib::commands::RunRequest {
            workflow: path,
            how_many_at_once: 1,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, _source) = line_channel(1024);
        let report = loadout_lib::commands::run::run_workflow_inner(&deps, &request, sink).await?;
        let saved: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        assert_eq!(
            saved["steps"][0]["status"], expected,
            "{name}: the Check did not use the independent examiner protocol: {}",
            saved["steps"][0]
        );
        assert_eq!(
            saved["steps"][0]["end_cause"], cause,
            "{name}: no guess from stderr or failed/skipped alone"
        );
        assert_eq!(saved["steps"][0]["death_proof"], true);
    }
    Ok(())
}
