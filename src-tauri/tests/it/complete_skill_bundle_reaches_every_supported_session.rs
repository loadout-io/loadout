//! WF-13: prawdziwy runner ma przekazać cały wybrany katalog, nie samo SKILL.md.
//! Dubler zastępuje tylko proces vendora; czyta zasoby spod przekazanej mu ścieżki.

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use loadout_lib::commands::run::run_workflow_inner;
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunRequest};
use loadout_lib::engine::drivers::{
    AgentDriver, AgentEvent, AgentHandle, DecodedEvent, FinishReason, Outcome, Probe, RunSpec,
    SessionRef, Tokens,
};
use loadout_lib::engine::supervisor::{self, GroupProof};
use loadout_lib::ipc::{line_channel, spawn_pump};
use loadout_lib::skills::{Roots, StepSkills};
use loadout_lib::store::Store;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::mpsc;

const AGENT: &str = "01990000-0000-7000-8000-000000000013";
const SKILL: &str = "bundle-reader";

#[tokio::test]
async fn generated_native_skills_do_not_cross_fan_in_as_agent_work() -> Result<(), Box<dyn Error>> {
    let (bench, seen, driver) = provenance_bench(false)?;
    let report = bench.run_with(provenance_workflow(true), driver).await?;
    assert!(
        report
            .steps
            .iter()
            .all(|state| *state == loadout_lib::engine::step::StepState::Succeeded)
    );
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let consumer = seen
        .iter()
        .find(|entry| entry["role"] == "consume")
        .ok_or("consumer did not start")?;
    assert_eq!(
        consumer["nativeSkill"], false,
        "fan-in handed the consumer a private package it did not select"
    );
    assert_eq!(consumer["userSkill"], "updated user skill");
    assert_eq!(consumer["newSkill"], "agent authored this skill");
    assert_eq!(consumer["left"], "left product");
    assert_eq!(consumer["right"], "right product");
    Ok(())
}

#[tokio::test]
async fn generated_native_skills_are_not_the_result_folder_but_user_skills_are()
-> Result<(), Box<dyn Error>> {
    let (bench, _seen, driver) = provenance_bench(false)?;
    let report = bench.run_with(provenance_workflow(false), driver).await?;
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let copy = saved["copy_results"]
        .as_object()
        .ok_or("missing copy results")?
        .values()
        .next()
        .ok_or("no copy result")?;
    assert_eq!(
        copy["kind"], "folder",
        "real product files must be retained: {saved}"
    );
    let result = report
        .dir
        .join(copy["path"].as_str().ok_or("no result folder")?);
    assert!(
        !result.join(".agents/skills").join(SKILL).exists(),
        "the retained result includes a Loadout-generated skill package"
    );
    assert_eq!(
        fs::read_to_string(result.join(".agents/skills/user-owned/SKILL.md"))?,
        "updated user skill"
    );
    assert_eq!(
        fs::read_to_string(result.join(".agents/skills/authored/SKILL.md"))?,
        "agent authored this skill"
    );
    assert_eq!(fs::read_to_string(result.join("left.txt"))?, "left product");
    Ok(())
}

#[tokio::test]
async fn native_delivery_does_not_hide_user_skill_changes_from_the_saved_git_commit()
-> Result<(), Box<dyn Error>> {
    let (bench, _seen, driver) = provenance_bench(true)?;
    let report = bench.run_with(provenance_workflow(false), driver).await?;
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let copy = saved["copy_results"]
        .as_object()
        .ok_or("missing copy results")?
        .values()
        .next()
        .ok_or("no copy result")?;
    let oid = copy["oid"].as_str().ok_or("no saved Git result")?;
    assert_eq!(
        provenance_git(
            &bench.project(),
            &["show", &format!("{oid}:.agents/skills/user-owned/SKILL.md")]
        )?,
        "updated user skill",
        "the blanket native-shelf exclusion dropped tracked user work"
    );
    assert_eq!(
        provenance_git(
            &bench.project(),
            &["show", &format!("{oid}:.agents/skills/authored/SKILL.md")]
        )?,
        "agent authored this skill",
        "new agent-authored skills are work, not delivery machinery"
    );
    let files = provenance_git(&bench.project(), &["ls-tree", "-r", "--name-only", oid])?;
    assert!(
        !files
            .lines()
            .any(|path| path.starts_with(&format!(".agents/skills/{SKILL}/"))),
        "generated package reached the saved commit: {files}"
    );
    assert_eq!(
        provenance_git(&bench.project(), &["show", &format!("{oid}:left.txt")])?,
        "left product"
    );
    assert_eq!(
        fs::read_to_string(bench.project().join(".agents/skills/user-owned/SKILL.md"))?,
        "original user skill"
    );
    Ok(())
}

type ProvenanceObservations = Arc<Mutex<Vec<serde_json::Value>>>;

#[tokio::test]
async fn same_bytes_on_a_foreign_inode_are_retained_not_cleaned_or_saved_as_a_result()
-> Result<(), Box<dyn Error>> {
    assert_changed_native_is_kept(NativeAlteration::ForeignInode).await
}

#[tokio::test]
async fn changed_native_bytes_are_retained_not_cleaned_or_saved_as_a_result()
-> Result<(), Box<dyn Error>> {
    assert_changed_native_is_kept(NativeAlteration::ChangedBytes).await
}

async fn assert_changed_native_is_kept(alteration: NativeAlteration) -> Result<(), Box<dyn Error>> {
    let (bench, seen, driver) = provenance_bench_with(false, alteration)?;
    let report = bench.run_with(provenance_workflow(false), driver).await?;
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    let copy = saved["copy_results"]
        .as_object()
        .ok_or("no copy results")?
        .values()
        .next()
        .ok_or("no copy result")?;
    assert_eq!(
        copy["kind"], "unavailable",
        "an altered delivery must not become an accepted result: {saved}"
    );
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let cwd = PathBuf::from(
        seen.first()
            .and_then(|one| one["cwd"].as_str())
            .ok_or("the producer did not start")?,
    );
    let expected = if matches!(alteration, NativeAlteration::ChangedBytes) {
        "foreign edited bytes"
    } else {
        &bench.marker
    };
    assert_eq!(
        fs::read_to_string(
            cwd.join(".agents/skills")
                .join(SKILL)
                .join("references/marker.txt")
        )?,
        expected,
        "cleanup removed or overwrote a changed object"
    );
    assert_eq!(
        fs::read_to_string(cwd.join("left.txt"))?,
        "left product",
        "uncertain delivery must preserve real work"
    );
    assert!(
        saved.to_string().contains("changed after publication"),
        "history did not explain why the result was withheld: {saved}"
    );
    Ok(())
}

#[tokio::test]
async fn same_copy_successor_reads_the_frozen_native_bundle_again_after_cleanup()
-> Result<(), Box<dyn Error>> {
    let (bench, seen, driver) = provenance_bench_with(false, NativeAlteration::HostChanges)?;
    let mut document = provenance_workflow(false);
    document["steps"].as_array_mut().ok_or("no steps")?.push(json!({"kind":"agent","id":"repeat","name":"Read again in the same folder","agent":AGENT,
        "instructions":"WF13-NATIVE-REPEAT","folder":{"use":"same-copy"},"borrow":{"skills":[SKILL]},"at":{"x":0,"y":0}}));
    document["links"] = json!([{"from":"produce","to":"repeat"}]);
    let report = bench.run_with(document, driver).await?;
    assert!(
        report
            .steps
            .iter()
            .all(|state| *state == loadout_lib::engine::step::StepState::Succeeded)
    );
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0]["cwd"], seen[1]["cwd"],
        "the fixture must exercise the same physical folder"
    );
    assert_eq!(seen[1]["role"], "repeat");
    assert_eq!(seen[0]["nativeReference"], bench.marker);
    assert_eq!(
        seen[1]["nativeReference"], bench.marker,
        "the successor lost its native files or reread the changed host"
    );
    assert_eq!(
        fs::read_to_string(bench.skill().join("references/marker.txt"))?,
        "host changed after first start"
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum NativeAlteration {
    None,
    ForeignInode,
    ChangedBytes,
    HostChanges,
}

type ProvenanceBench = (Bench, ProvenanceObservations, Arc<dyn AgentDriver>);

fn provenance_bench(git: bool) -> Result<ProvenanceBench, Box<dyn Error>> {
    provenance_bench_with(git, NativeAlteration::None)
}

fn provenance_bench_with(
    git: bool,
    alteration: NativeAlteration,
) -> Result<ProvenanceBench, Box<dyn Error>> {
    let bench = Bench::new()?;
    fs::create_dir_all(bench.project().join(".agents/skills/user-owned"))?;
    fs::write(
        bench.project().join(".agents/skills/user-owned/SKILL.md"),
        "original user skill",
    )?;
    fs::write(bench.project().join(".gitignore"), ".loadout/\n")?;
    if git {
        provenance_git(&bench.project(), &["init", "--quiet"])?;
        provenance_git(&bench.project(), &["add", "-A"])?;
        provenance_git(
            &bench.project(),
            &["commit", "--quiet", "-m", "WF13 provenance input"],
        )?;
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let driver: Arc<dyn AgentDriver> = Arc::new(ProvenanceReader {
        seen: Arc::clone(&seen),
        alteration,
        host: bench.skill(),
    });
    Ok((bench, seen, driver))
}

fn provenance_git(root: &std::path::Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=WF13",
            "-c",
            "user.email=wf13@localhost",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn provenance_workflow(fan_in: bool) -> serde_json::Value {
    let mut steps = vec![
        json!({"kind":"agent","id":"produce","name":"Produce real files","agent":AGENT,
        "instructions":"WF13-PRODUCE","folder":{"use":"fresh-copy"},"borrow":{"skills":[SKILL]},"at":{"x":0,"y":0}}),
    ];
    let mut links = Vec::new();
    if fan_in {
        steps.push(
            json!({"kind":"agent","id":"right","name":"Produce other files","agent":AGENT,
            "instructions":"WF13-RIGHT","folder":{"use":"fresh-copy"},"at":{"x":0,"y":0}}),
        );
        steps.push(
            json!({"kind":"agent","id":"consume","name":"Read only produced files","agent":AGENT,
            "instructions":"WF13-CONSUME","folder":{"use":"same-copy"},"at":{"x":0,"y":0}}),
        );
        links = vec![
            json!({"from":"produce","to":"consume"}),
            json!({"from":"right","to":"consume"}),
        ];
    }
    json!({"format":1,"id":"wf13-provenance","name":"Keep delivery separate from work","steps":steps,"links":links})
}

#[derive(Clone)]
struct ProvenanceReader {
    seen: ProvenanceObservations,
    alteration: NativeAlteration,
    host: PathBuf,
}

#[async_trait]
impl AgentDriver for ProvenanceReader {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn reads_step_skills_from_its_folder(&self) -> bool {
        true
    }
    fn with_evidence(
        &self,
        _: loadout_lib::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let role = if spec.prompt.contains("WF13-PRODUCE") {
            "produce"
        } else if spec.prompt.contains("WF13-RIGHT") {
            "right"
        } else if spec.prompt.contains("WF13-NATIVE-REPEAT") {
            "repeat"
        } else {
            "consume"
        };
        if role == "produce" {
            anyhow::ensure!(
                spec.cwd
                    .join(".agents/skills")
                    .join(SKILL)
                    .join("references/marker.txt")
                    .is_file(),
                "native delivery did not reach the producer"
            );
            let reference = spec
                .cwd
                .join(".agents/skills")
                .join(SKILL)
                .join("references/marker.txt");
            match self.alteration {
                NativeAlteration::None => (),
                NativeAlteration::ForeignInode => {
                    let bytes = fs::read(&reference)?;
                    // Zachowany oryginał wyklucza ponowne użycie tego samego inode'u.
                    fs::rename(&reference, spec.cwd.join("original-delivery-inode"))?;
                    fs::write(&reference, bytes)?;
                }
                NativeAlteration::ChangedBytes => fs::write(&reference, "foreign edited bytes")?,
                NativeAlteration::HostChanges => fs::write(
                    self.host.join("references/marker.txt"),
                    "host changed after first start",
                )?,
            }
            fs::write(spec.cwd.join("left.txt"), "left product")?;
            fs::write(
                spec.cwd.join(".agents/skills/user-owned/SKILL.md"),
                "updated user skill",
            )?;
            fs::create_dir_all(spec.cwd.join(".agents/skills/authored"))?;
            fs::write(
                spec.cwd.join(".agents/skills/authored/SKILL.md"),
                "agent authored this skill",
            )?;
        } else if role == "right" {
            fs::write(spec.cwd.join("right.txt"), "right product")?;
        }
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).push(json!({
            "role":role,"cwd":spec.cwd,"nativeSkill":spec.cwd.join(".agents/skills").join(SKILL).exists(),
            "nativeReference":fs::read_to_string(spec.cwd.join(".agents/skills").join(SKILL).join("references/marker.txt")).ok(),
            "userSkill":fs::read_to_string(spec.cwd.join(".agents/skills/user-owned/SKILL.md")).ok(),
            "newSkill":fs::read_to_string(spec.cwd.join(".agents/skills/authored/SKILL.md")).ok(),
            "left":fs::read_to_string(spec.cwd.join("left.txt")).ok(),"right":fs::read_to_string(spec.cwd.join("right.txt")).ok()
        }));
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
                id: spec.run_id.to_string(),
            },
        }))
    }
}

#[tokio::test]
async fn codex_exec_reads_complete_frozen_bundles_from_its_real_fresh_copy()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let driver = bench.codex_exec_driver()?;
    let report = bench.run_with(codex_workflow("fresh-copy"), driver).await?;
    assert!(
        report
            .steps
            .iter()
            .all(|state| *state == loadout_lib::engine::step::StepState::Succeeded),
        "the real Codex exec adapter did not complete its reads"
    );
    let delivered = fs::read_to_string(bench.root.path().join("native-exec.jsonl"))?
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(delivered.len(), 2);
    assert_ne!(delivered[0]["cwd"], delivered[1]["cwd"]);
    for one in delivered {
        assert_eq!(one["reference"], bench.marker);
        assert_eq!(one["executable"], true);
        let cwd = PathBuf::from(one["cwd"].as_str().ok_or("missing actual cwd")?);
        assert!(cwd.starts_with(fs::canonicalize(&report.dir)?));
    }
    assert!(!bench.project().join(".agents").exists());
    assert!(!bench.project().join("HELPER-RAN").exists());
    Ok(())
}

#[tokio::test]
async fn codex_exec_refuses_project_delivery_before_spawn_instead_of_installing_into_the_host()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let driver = bench.codex_exec_driver()?;
    let error = bench
        .run_with(codex_workflow("project"), driver)
        .await
        .err()
        .ok_or("the project-folder step started without a native private skill route")?
        .to_string();
    assert!(
        error.contains(SKILL) && error.contains("Read the selected bundle"),
        "unnamed refusal: {error}"
    );
    assert!(
        !bench.root.path().join("native-exec.jsonl").exists(),
        "the CLI started despite refusing delivery"
    );
    assert!(!bench.project().join(".agents").exists());
    assert_eq!(
        fs::read_to_string(bench.skill().join("references/marker.txt"))?,
        bench.marker
    );
    Ok(())
}

#[tokio::test]
async fn codex_exec_borrow_uses_the_same_complete_owned_native_shelf() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let driver = bench.codex_exec_driver()?;
    let agent = bench.home().join("agents/reader.md");
    fs::write(
        &agent,
        fs::read_to_string(&agent)?.replace(&format!("skills: [{SKILL}]"), "skills: []"),
    )?;
    let mut document = codex_workflow("fresh-copy");
    document["steps"][0]["borrow"] = json!({"skills":[SKILL]});
    let report = bench.run_with(document, driver).await;
    assert!(
        report.is_ok(),
        "Borrow refused despite the native shelf in an owned copy: {report:?}"
    );
    let report = report?;
    assert!(
        report
            .steps
            .iter()
            .all(|state| *state == loadout_lib::engine::step::StepState::Succeeded)
    );
    let delivered = fs::read_to_string(bench.root.path().join("native-exec.jsonl"))?
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(delivered.len(), 2);
    assert!(
        delivered
            .iter()
            .all(|item| item["reference"] == bench.marker && item["executable"] == true)
    );
    assert!(!bench.project().join(".agents").exists());
    Ok(())
}

fn codex_workflow(folder: &str) -> serde_json::Value {
    json!({"format":1,"id":"wf13-native","name":"Native bundle reader","links":[],"steps":[{
        "kind":"agent","id":"reader","name":"Read the selected bundle","agent":AGENT,"copies":if folder == "project" {1} else {2},
        "instructions":"Read only","folder":{"use":folder},"at":{"x":0,"y":0}
    }]})
}

#[tokio::test]
async fn codex_borrow_cannot_install_its_native_shelf_into_the_human_project()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let driver = bench.codex_exec_driver()?;
    let agent = bench.home().join("agents/reader.md");
    fs::write(
        &agent,
        fs::read_to_string(&agent)?.replace(&format!("skills: [{SKILL}]"), "skills: []"),
    )?;
    let mut document = codex_workflow("project");
    document["steps"][0]["borrow"] = json!({"skills":[SKILL]});
    let error = bench
        .run_with(document, driver)
        .await
        .err()
        .ok_or("Borrow installed native skills into the project")?
        .to_string();
    assert!(
        error.contains(SKILL) && error.contains("Read the selected bundle"),
        "unnamed refusal: {error}"
    );
    assert!(!bench.root.path().join("native-exec.jsonl").exists());
    assert!(!bench.project().join(".agents").exists());
    assert_eq!(
        fs::read_to_string(bench.skill().join("references/marker.txt"))?,
        bench.marker
    );
    Ok(())
}

const NATIVE_EXEC: &str = r#"#!/bin/sh
here="$(dirname "$0")"
if [ "$1" = "--version" ]; then
  printf '%s\n' 'codex-cli 0.153.4'
  exit 0
fi
cat >/dev/null
skill=".agents/skills/bundle-reader"
reference="$(cat "$skill/references/marker.txt")"
executable=false
if [ -x "$skill/scripts/helper.sh" ]; then executable=true; fi
printf '{"cwd":"%s","reference":"%s","executable":%s}\n' "$(pwd -P)" "$reference" "$executable" >> "$here/native-exec.jsonl"
printf '%s' 'CHANGED-AFTER-FIRST-EXEC' > "$here/project/.claude/skills/bundle-reader/references/marker.txt"
printf '%s\n' '{"type":"thread.started","thread_id":"skill-exec"}'
printf '%s\n' '{"type":"item.completed","item":{"id":"one","type":"agent_message","text":"Read the selected bundle."}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'
"#;

#[tokio::test]
async fn two_lead_conversations_do_not_replace_each_others_frozen_resources()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let agent = bench.home().join("agents/reader.md");
    fs::write(
        &agent,
        fs::read_to_string(&agent)?.replace("skills: []", &format!("skills: [{SKILL}]")),
    )?;
    let driver: Arc<dyn AgentDriver> = Arc::new(Reader {
        seen: Arc::clone(&bench.seen),
        flags: Vec::new(),
        host: bench.skill(),
    });
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let store = Store::open(&bench.project().join(".loadout/loadout.db"))?;
    let state = loadout_lib::ipc::AppState::new(bench.home(), bench.project(), store, drivers);
    for terminal in ["wf13-first", "wf13-second"] {
        let (sink, _source) = line_channel(256);
        state.watching_the_lead(terminal, None, sink).await?;
        let result = loadout_lib::ipc::say_to_orchestrator_from_window(
            &state,
            terminal,
            None,
            Some(AGENT),
            "Read the selected marker.",
            Vec::new(),
        )
        .await;
        if result.is_err() {
            state.close_everything_down().await;
            result?;
        }
    }
    let paths = bench
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    let first_bytes = fs::read_to_string(paths[0].join("references/marker.txt"));
    state.close_everything_down().await;
    assert_eq!(paths.len(), 2);
    assert_ne!(
        paths[0], paths[1],
        "two live conversations share one mutable skill folder"
    );
    assert_eq!(
        first_bytes?, bench.marker,
        "the second conversation replaced the first one's bundle"
    );
    let mut recorded = Vec::new();
    for entry in fs::read_dir(bench.project().join(".loadout/conversations"))? {
        let entry = entry?;
        let input: serde_json::Value =
            serde_json::from_slice(&fs::read(entry.path().join("input.json"))?)?;
        for source in input["context"].as_array().ok_or("missing context list")? {
            if source["kind"] == "inheritedSkill" {
                recorded.push(
                    bench
                        .project()
                        .join(source["reference"].as_str().ok_or("missing reference")?),
                );
            }
        }
    }
    /* PO MIEJSCU, nie po pisowni: wskaźnik w historii rozmowy jest WZGLĘDNY, a fikstura skleja
    go z surową ścieżką katalogu tymczasowego. Okno rozwiązuje korzeń projektu na wejściu
    (`ipc::project_folder`), żeby cała aplikacja miała jedną pisownię, a `/var` na macOS jest
    skrótem do `/private/var`. Pytanie tej asercji zostaje to samo: czy każdy dostarczony
    pakiet ma trwały wskaźnik w historii swojej rozmowy. */
    let same_place = |one: &std::path::Path| -> PathBuf {
        std::fs::canonicalize(one).unwrap_or_else(|_error| one.to_path_buf())
    };
    let recorded: Vec<PathBuf> = recorded.iter().map(|one| same_place(one)).collect();
    assert!(
        paths
            .iter()
            .all(|path| recorded.contains(&same_place(&path.join("SKILL.md")))),
        "a delivered bundle has no durable link to its conversation history"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_lead_receives_a_native_skill_input_pointing_at_the_complete_private_bundle()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let agent = bench.home().join("agents/reader.md");
    fs::write(
        &agent,
        fs::read_to_string(&agent)?
            .replace("runsWith: claude-code", "runsWith: codex")
            .replace("skills: []", &format!("skills: [{SKILL}]")),
    )?;
    let binary = bench.root.path().join("codex-fixture");
    fs::write(&binary, NATIVE_APP_SERVER)?;
    supervisor::set_executable_file(&std::fs::File::open(&binary)?, true)?;
    let driver: Arc<dyn AgentDriver> =
        Arc::new(loadout_lib::engine::drivers::codex::CodexDriver::with_binary(binary));
    let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
    let store = Store::open(&bench.project().join(".loadout/loadout.db"))?;
    let state = loadout_lib::ipc::AppState::new(bench.home(), bench.project(), store, drivers);
    let (sink, _source) = line_channel(256);
    state.watching_the_lead("wf13-native", None, sink).await?;
    let reply = loadout_lib::ipc::say_to_orchestrator_from_window(
        &state,
        "wf13-native",
        None,
        Some(AGENT),
        "Read the selected marker.",
        Vec::new(),
    )
    .await;
    state.close_everything_down().await;
    reply?;
    let calls = fs::read_to_string(bench.root.path().join("native-input.jsonl"))?
        .lines()
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()?;
    let skill = calls
        .iter()
        .find(|call| call["method"] == "turn/start")
        .and_then(|call| call["params"]["input"].as_array())
        .and_then(|input| input.iter().find(|item| item["type"] == "skill"));
    assert!(
        skill.is_some(),
        "the real Codex Lead IPC did not deliver a native skill input"
    );
    let skill = skill.ok_or("missing native skill input")?;
    assert_eq!(skill["name"], SKILL);
    let path = PathBuf::from(skill["path"].as_str().ok_or("missing skill path")?);
    let folder = path.parent().ok_or("missing skill folder")?;
    assert_ne!(
        folder,
        bench.skill(),
        "the session reads mutable host resources"
    );
    assert_eq!(
        fs::read_to_string(folder.join("references/marker.txt"))?,
        bench.marker
    );
    assert!(supervisor::executable_bits(&fs::metadata(
        folder.join("scripts/helper.sh")
    )?));
    assert!(!bench.project().join(".agents").exists());
    Ok(())
}

const NATIVE_APP_SERVER: &str = r#"#!/bin/sh
here="$(dirname "$0")"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/native-input.jsonl"
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"id":%s,"result":{}}\n' "${id:-1}"
      ;;
    *'"method":"config/read"'*)
      printf '{"id":%s,"result":{"config":{"mcp_servers":{}},"origins":{}}}\n' "${id:-2}"
      ;;
    *'"method":"thread/start"'*)
      printf '{"id":%s,"result":{"thread":{"id":"skill-thread","ephemeral":true,"path":null}}}\n' "${id:-3}"
      ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"skill-turn","status":"inProgress"}}}\n' "${id:-4}"
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"skill-thread","turn":{"id":"skill-turn","status":"completed"}}}'
      ;;
    *'interrupt'*)
      printf '{"id":%s,"result":{}}\n' "${id:-5}"
      ;;
  esac
done
"#;

#[tokio::test]
async fn saved_bundles_are_read_without_the_original_source_and_refuse_missing_resources()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    bench.run().await?;
    let delivered = bench.seen.lock().unwrap_or_else(PoisonError::into_inner)[0]
        .path
        .clone();
    let plugin = delivered
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("missing plugin root")?;
    fs::rename(
        bench.skill(),
        bench.project().join("removed-original-skill"),
    )?;
    let saved = loadout_lib::skills::bundle::read_delivered(plugin)?;
    assert_eq!(saved.len(), 1);
    let repeated = bench.root.path().join("repeated");
    saved[0].bundle.materialize(&repeated)?;
    assert_eq!(
        fs::read_to_string(repeated.join("references/marker.txt"))?,
        bench.marker
    );
    fs::rename(
        delivered.join("references/marker.txt"),
        delivered.join("references/missing-marker.txt"),
    )?;
    assert!(
        loadout_lib::skills::bundle::read_delivered(plugin).is_err(),
        "saved bundle corruption was accepted"
    );
    Ok(())
}

#[tokio::test]
async fn borrowed_bundle_keeps_references_helpers_and_the_frozen_source()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let report = bench.run().await?;
    assert!(
        report
            .steps
            .iter()
            .all(|state| { *state == loadout_lib::engine::step::StepState::Succeeded })
    );
    let seen = bench.seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(seen.len(), 2, "both actual copies must reach the driver");
    for delivered in seen.iter() {
        assert_eq!(
            delivered.reference.as_deref(),
            Some(bench.marker.as_str()),
            "Borrow lost its selected references/marker.txt at the real driver boundary"
        );
        assert_eq!(
            delivered.helper.as_deref(),
            Some("#!/bin/sh\nexit 0\n"),
            "Borrow lost the helper without reporting incomplete delivery"
        );
        assert!(delivered.executable, "the helper lost its executable bit");
        assert!(
            delivered.path.starts_with(&report.dir),
            "the run borrowed a mutable host path"
        );
    }
    assert!(!bench.project().join("HELPER-RAN").exists());
    assert!(
        !bench.project().join(".agents").exists(),
        "delivery wrote a vendor shelf into the host"
    );
    Ok(())
}

#[test]
fn different_bundles_with_the_same_name_require_a_source_choice() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    let other = bench.home().join("skills").join(SKILL);
    fs::create_dir_all(&other)?;
    fs::write(other.join("SKILL.md"), skill_text())?;
    fs::write(
        other.join("different.txt"),
        "a different bundle, even with identical SKILL.md",
    )?;
    let roots = Roots {
        home: bench.root.path().to_path_buf(),
        project: Some(bench.project()),
        data: bench.home(),
    };
    let result = StepSkills::wherever_they_lie(&roots, &[SKILL.to_owned()], None, "Reader");
    let said = match result {
        Err(refused) => refused.to_string(),
        Ok(found) => {
            return Err(format!(
                "two different copies silently selected the first folder: {found:?}"
            )
            .into());
        }
    };
    // Odmowa prosi o wybór, więc musi powiedzieć MIĘDZY CZYM. Bez obu katalogów w zdaniu człowiek
    // zostaje z pracą, którą Loadout właśnie wykonał: obejść półki i porównać kopie ręcznie.
    // Samo `is_err()` przepuszczało też pusty komunikat.
    for folder in [bench.skill(), other] {
        assert!(
            said.contains(&folder.display().to_string()),
            "the refusal asked for a choice without naming {}: {said}",
            folder.display()
        );
    }
    Ok(())
}

#[test]
fn escaping_link_is_refused_by_borrow_and_import_instead_of_being_omitted()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    fs::write(bench.root.path().join("private.txt"), "must not be copied")?;
    supervisor::link(
        &bench.root.path().join("private.txt"),
        &bench.skill().join("references/outside.txt"),
    )?;
    let borrowed = loadout_lib::inherit::rewrite::plugin_dir(
        &bench.project(),
        &[SKILL.to_owned()],
        &bench.root.path().join("borrowed"),
    );
    let imported = loadout_lib::skills::ingest::from_folder(&bench.skill());
    assert!(
        borrowed.is_err(),
        "Borrow called an incomplete escaping-link bundle delivered"
    );
    assert!(
        imported.is_err(),
        "Import silently removed part of the selected bundle"
    );
    Ok(())
}

#[test]
fn plugin_publication_does_not_follow_a_replaced_manifest_directory() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new()?;
    let into = bench.root.path().join("private-plugin");
    let foreign = bench.root.path().join("foreign-plugin");
    fs::create_dir_all(&into)?;
    fs::create_dir_all(&foreign)?;
    let manifest = foreign.join("plugin.json");
    fs::write(&manifest, "foreign bytes must survive")?;
    supervisor::link(&foreign, &into.join(".claude-plugin"))?;
    let delivered =
        loadout_lib::inherit::rewrite::plugin_dir(&bench.project(), &[SKILL.to_owned()], &into);
    assert_eq!(
        fs::read_to_string(manifest)?,
        "foreign bytes must survive",
        "publication escaped its private bundle through a replaced manifest directory"
    );
    assert!(
        delivered.is_err(),
        "an unsafe native plugin was called delivered"
    );
    Ok(())
}

#[test]
fn imported_resources_are_frozen_and_contained_links_are_installed() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;
    supervisor::link(
        std::path::Path::new("marker.txt"),
        &bench.skill().join("references/alias.txt"),
    )?;
    let imported = loadout_lib::skills::ingest::from_folder(&bench.skill())?;
    fs::write(
        bench.skill().join("references/marker.txt"),
        "changed after import was reviewed",
    )?;
    let roots = Roots {
        home: bench.root.path().join("install-home"),
        project: None,
        data: bench.home(),
    };
    let plan = loadout_lib::skills::place::plan(
        &imported.skill,
        loadout_lib::skills::Scope::Global,
        &roots,
    )?;
    loadout_lib::skills::place::apply(&plan, &imported.skill)?;
    for folder in plan.writes {
        assert_eq!(
            fs::read_to_string(folder.join("references/marker.txt"))?,
            bench.marker,
            "installation read source bytes changed after the import was reviewed"
        );
        assert_eq!(
            fs::read_link(folder.join("references/alias.txt"))?,
            PathBuf::from("marker.txt"),
            "installation omitted a contained resource link"
        );
        assert!(supervisor::executable_bits(&fs::metadata(
            folder.join("scripts/helper.sh")
        )?));
    }
    assert!(!bench.project().join("HELPER-RAN").exists());
    Ok(())
}

fn skill_text() -> String {
    format!(
        "---\nname: {SKILL}\ndescription: Read the selected local marker.\n---\nRead references/marker.txt. The optional helper is scripts/helper.sh.\n"
    )
}

struct Bench {
    root: TempDir,
    marker: String,
    seen: Arc<Mutex<Vec<Delivered>>>,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            root: tempfile::tempdir()?,
            marker: uuid::Uuid::now_v7().to_string(),
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        fs::create_dir_all(bench.home().join("agents"))?;
        fs::create_dir_all(bench.home().join("workflows"))?;
        fs::create_dir_all(bench.project().join(".loadout"))?;
        fs::create_dir_all(bench.skill().join("references"))?;
        fs::create_dir_all(bench.skill().join("scripts"))?;
        fs::write(bench.skill().join("SKILL.md"), skill_text())?;
        fs::write(bench.skill().join("references/marker.txt"), &bench.marker)?;
        fs::write(
            bench.skill().join("scripts/helper.sh"),
            "#!/bin/sh\nexit 0\n",
        )?;
        supervisor::set_executable_file(
            &std::fs::File::open(bench.skill().join("scripts/helper.sh"))?,
            true,
        )?;
        fs::write(
            bench.home().join("agents/reader.md"),
            format!(
                "---\nschema: 1\nid: {AGENT}\nname: Reader\nsummary: Reads a bundle\ncolor: slate\nrunsWith: claude-code\nmodel: opus\nthinking: balanced\nfileAccess: look-only\ngiveUpAfterMinutes: 20\nwriteResultsTo: \"\"\ntools: everything\nskills: []\nconnections: []\n---\nRead only.\n"
            ),
        )?;
        Ok(bench)
    }
    fn home(&self) -> PathBuf {
        self.project().join(".loadout")
    }
    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }
    fn skill(&self) -> PathBuf {
        self.project().join(".claude/skills").join(SKILL)
    }
    fn codex_exec_driver(&self) -> Result<Arc<dyn AgentDriver>, Box<dyn Error>> {
        let agent = self.home().join("agents/reader.md");
        fs::write(
            &agent,
            fs::read_to_string(&agent)?
                .replace("runsWith: claude-code", "runsWith: codex")
                .replace("skills: []", &format!("skills: [{SKILL}]")),
        )?;
        let binary = self.root.path().join("codex-exec-fixture");
        fs::write(&binary, NATIVE_EXEC)?;
        supervisor::set_executable_file(&std::fs::File::open(&binary)?, true)?;
        Ok(Arc::new(
            loadout_lib::engine::drivers::codex::CodexDriver::with_binary(binary),
        ))
    }
    async fn run(&self) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
        let document = json!({"format":1,"id":"wf13","name":"Bundle reader","links":[],"steps":[{
            "kind":"agent","id":"reader","name":"Read the selected bundle","agent":AGENT,"copies":2,
            "instructions":"Read only","folder":{"use":"fresh-copy"},"borrow":{"skills":[SKILL]},"at":{"x":0,"y":0}
        }]});
        let driver: Arc<dyn AgentDriver> = Arc::new(Reader {
            seen: Arc::clone(&self.seen),
            flags: Vec::new(),
            host: self.skill(),
        });
        self.run_with(document, driver).await
    }
    async fn run_with(
        &self,
        document: serde_json::Value,
        driver: Arc<dyn AgentDriver>,
    ) -> Result<loadout_lib::commands::RunReport, Box<dyn Error>> {
        let workflow = self.home().join("workflows/bundle.json");
        fs::write(&workflow, document.to_string())?;
        let store = Store::open(&self.project().join(".loadout/loadout.db"))?;
        let drivers: Drivers = Arc::new(move |_| Arc::clone(&driver));
        let deps = RunDeps {
            home: &self.home(),
            library: self.home().clone(),
            project: &self.project(),
            store: &store,
            drivers,
            processes: Arc::new(loadout_lib::commands::processes::Processes::new()),
            control: RunControl::new(),
        };
        let (sink, source) = line_channel(256);
        let pump = spawn_pump(source, tauri::ipc::Channel::new(|_| Ok(())));
        let report = tokio::time::timeout(
            Duration::from_secs(30),
            run_workflow_inner(
                &deps,
                &RunRequest {
                    workflow,
                    how_many_at_once: 1,
                    task: None,
                    part: None,
                    handoffs_from: None,
                },
                sink,
            ),
        )
        .await?;
        tokio::time::timeout(Duration::from_secs(30), pump).await??;
        Ok(report?)
    }
}

struct Delivered {
    path: PathBuf,
    reference: Option<String>,
    helper: Option<String>,
    executable: bool,
}

#[derive(Clone)]
struct Reader {
    seen: Arc<Mutex<Vec<Delivered>>>,
    flags: Vec<String>,
    host: PathBuf,
}

#[async_trait]
impl AgentDriver for Reader {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn inheriting(&self, flags: &[String]) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(Self {
            flags: flags.to_vec(),
            ..self.clone()
        }))
    }
    fn with_evidence(
        &self,
        _: loadout_lib::evidence::EvidenceTarget,
    ) -> Option<Arc<dyn AgentDriver>> {
        Some(Arc::new(self.clone()))
    }
    async fn probe(&self) -> anyhow::Result<Probe> {
        Ok(Probe {
            found: true,
            version: None,
        })
    }
    async fn start(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<DecodedEvent>,
    ) -> anyhow::Result<Box<dyn AgentHandle>> {
        let path = self
            .flags
            .windows(2)
            .find(|pair| pair[0] == "--plugin-dir")
            .map_or_else(
                || spec.cwd.join("missing-delivery"),
                |pair| PathBuf::from(&pair[1]).join("skills").join(SKILL),
            );
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Delivered {
                reference: fs::read_to_string(path.join("references/marker.txt")).ok(),
                helper: fs::read_to_string(path.join("scripts/helper.sh")).ok(),
                executable: fs::metadata(path.join("scripts/helper.sh"))
                    .is_ok_and(|meta| supervisor::executable_bits(&meta)),
                path,
            });
        fs::write(
            self.host.join("references/marker.txt"),
            "CHANGED-AFTER-FIRST-START",
        )?;
        Ok(Box::new(Turn {
            events,
            session: SessionRef {
                vendor: self.id(),
                id: spec.run_id.to_string(),
            },
        }))
    }
}

struct Turn {
    events: mpsc::Sender<DecodedEvent>,
    session: SessionRef,
}

#[async_trait]
impl AgentHandle for Turn {
    fn session(&self) -> SessionRef {
        self.session.clone()
    }
    fn group(&self) -> Option<supervisor::GroupId> {
        None
    }
    async fn send(&mut self, _: String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn wait(&mut self) -> anyhow::Result<Outcome> {
        let outcome = Outcome {
            ok: true,
            reason: FinishReason::Completed,
            text: "Read the selected bundle.".to_owned(),
            cost_usd: None,
            tokens: Tokens::default(),
            turns: 1,
            took: Duration::from_millis(1),
            session: self.session.clone(),
        };
        self.events
            .send(AgentEvent::Finished(outcome.clone()).into())
            .await?;
        Ok(outcome)
    }
    async fn cancel(&mut self) -> GroupProof {
        GroupProof::Dead { status: None }
    }
    async fn close(&mut self) -> anyhow::Result<Option<i32>> {
        Ok(Some(0))
    }
}
