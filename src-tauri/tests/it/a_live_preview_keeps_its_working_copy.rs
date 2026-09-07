//! WF-25: prawdziwa usługa czyta własne pliki także po końcu grafu.
//!
//! 2026-09-05: dotychczasowe kryterium Serve używało Project, więc nie widziało
//! usuwania worktree spod żywego procesu. Bramka synchronizuje rzeczywisty start;
//! późniejszy odczyt wykonuje sam proces, nie asercja nad polem `alive`.

#![allow(clippy::needless_pass_by_value)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::processes::Processes;
use loadout_lib::commands::run::{run_workflow_inner, stop_run_inner};
use loadout_lib::commands::{Drivers, RunControl, RunDeps, RunReport, RunRequest};
use loadout_lib::engine::supervisor::{self, GroupProof};
use loadout_lib::ipc::line_channel;
use loadout_lib::store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;

const PATIENCE: Duration = Duration::from_secs(25);
const POLL: Duration = Duration::from_millis(10);
const INITIAL: &str = "the original project input\n";
const PREPARED: &str = "the prepared preview input\n";

// Sterowanie i odpowiedź leżą POZA projektem/kopią. Usunięcie cwd nie zabiera
// bariery ani dowodu braku pliku. Żadne pole stdout nie udaje odczytu aplikacji.
const SERVICE: &str = r#"#!/bin/sh
control="$1"
name="$2"
pwd -P > "$control/cwd-$name"
while [ ! -f "$control/exit-$name" ]; do
  if [ -f "$control/read-$name" ]; then
    if [ -f value.txt ]; then
      cp value.txt "$control/value-$name"
    else
      printf 'the working file disappeared\n' > "$control/value-$name"
    fi
    : > "$control/read-done-$name"
    rm "$control/read-$name"
  fi
  sleep 0.02
done
"#;

const GATE: &str = r#"#!/bin/sh
control="$1"
count="$2"
while [ ! -f "$control/cwd-a" ]; do sleep 0.02; done
if [ "$count" = 2 ]; then
  while [ ! -f "$control/cwd-b" ]; do sleep 0.02; done
fi
if [ "$3" = stop ]; then
  : > "$control/gate-is-running"
  sleep 600
fi
printf '1 passed\n'
"#;

const PREPARE: &str = r"#!/bin/sh
printf 'the prepared preview input\n' > value.txt
printf '1 passed\n'
";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_window_service_keeps_its_fresh_git_copy_after_the_graph() -> Result<(), Box<dyn Error>> {
    assert_window_copy(true, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_window_service_keeps_its_fresh_non_git_copy_after_the_graph()
-> Result<(), Box<dyn Error>> {
    assert_window_copy(false, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_window_service_keeps_the_prepared_same_copy_after_the_graph()
-> Result<(), Box<dyn Error>> {
    assert_window_copy(true, true).await
}

async fn assert_window_copy(git: bool, prepared: bool) -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(git)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run(&processes, prepared, 1, "window").await?;
    let pgid = process_group(&report, "s_preview_a")?;
    let cwd = bench.cwd("a")?;
    let was_alive = !supervisor::group_is_empty(pgid);
    let result = bench.read_from_service("a").await;
    let proofs = processes.close().await;
    let removed = wait_until(|| !cwd.exists()).await;

    assert!(
        was_alive,
        "the service did not survive the graph, so its later file read proves nothing"
    );
    assert_dead(&proofs, &[pgid]);
    assert_eq!(
        result?,
        if prepared { PREPARED } else { INITIAL },
        "a live preview lost its real working file after graph finalization"
    );
    assert!(
        removed,
        "the confirmed last service death never released normal copy cleanup"
    );
    if prepared {
        assert_one_result_commit(&bench, &report)?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_the_last_of_two_services_releases_the_shared_worktree() -> Result<(), Box<dyn Error>>
{
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run(&processes, true, 2, "window").await?;
    let a = process_group(&report, "s_preview_a")?;
    let b = process_group(&report, "s_preview_b")?;
    let cwd_a = bench.cwd("a")?;
    let cwd_b = bench.cwd("b")?;
    let proof_a = processes.stop(a).await;
    let b_alive = !supervisor::group_is_empty(b);
    let after_one_death = bench.read_from_service("b").await;
    let proof_b = processes.stop(b).await;
    let remainder = processes.close().await;
    let removed = wait_until(|| !cwd_b.exists()).await;

    assert_eq!(
        cwd_a, cwd_b,
        "the two services did not actually share one working copy"
    );
    assert!(matches!(proof_a, Some(GroupProof::Dead { .. })));
    assert!(
        b_alive,
        "stopping the first service stopped a different service too"
    );
    assert_eq!(
        after_one_death?, PREPARED,
        "the first death released the other service's copy"
    );
    assert!(matches!(proof_b, Some(GroupProof::Dead { .. })));
    assert_dead(&remainder, &[a, b]);
    assert!(removed, "the last death never released the shared copy");
    assert_one_result_commit(&bench, &report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_natural_service_end_releases_deferred_finalization() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run(&processes, true, 1, "window").await?;
    let pgid = process_group(&report, "s_preview_a")?;
    let cwd = bench.cwd("a")?;
    let result = bench.read_from_service("a").await;
    fs::write(bench.control.path().join("exit-a"), b"exit")?;
    let natural_death = wait_until(|| supervisor::group_is_empty(pgid)).await;
    let removed_naturally = wait_until(|| !cwd.exists()).await;
    // Dopiero po pomiarze sprzątamy awaryjnie: close nie może zaliczyć naturalnego końca.
    let proofs = processes.close().await;

    assert_dead(&proofs, &[pgid]);
    assert_eq!(
        result?, PREPARED,
        "finalization ran while its service still needed the copy"
    );
    assert!(
        natural_death,
        "the natural-end path did not prove the group dead"
    );
    assert!(
        removed_naturally,
        "natural EOF/death did not trigger deferred finalization"
    );
    assert_one_result_commit(&bench, &report)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_deferred_result_survives_a_temporarily_missing_run_file() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run(&processes, true, 1, "window").await?;
    let observed: Result<_, Box<dyn Error>> = async {
        let pgid = process_group(&report, "s_preview_a")?;
        let cwd = bench.cwd("a")?;
        let old_run = bench.control.path().join("run-before-finalization.json");
        fs::rename(report.dir.join("run.json"), &old_run)?;
        let proofs = processes.close().await;
        assert_dead(&proofs, &[pgid]);
        let receipt = report.dir.join(".results/s_prepare.json");
        let settled = wait_until(|| !cwd.exists()).await;
        let recorded = fs::read(&receipt);
        // Przywracamy także na RED. To rzeczywisty starszy pending, nie odtworzony run.json.
        fs::rename(old_run, report.dir.join("run.json"))?;
        let branch = loadout_lib::commands::isolate::branch_for(&report.id, "s_prepare");
        let oid = git(bench.project.path(), &["rev-parse", &branch])?
            .trim()
            .to_owned();
        let _ = loadout_lib::commands::reconcile::reconcile_runs(bench.project.path());
        let saved: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
        let run_name = report
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("run folder has no name")?;
        let history =
            loadout_lib::commands::history::read_run_inner(bench.project.path(), run_name);
        Ok((settled, recorded, oid, saved, history))
    }
    .await;
    let proofs = processes.close().await;
    assert_dead(&proofs, &[]);
    let (settled, recorded, oid, saved, history) = observed?;
    assert!(
        settled,
        "the last confirmed death never attempted finalization"
    );
    assert!(
        recorded.is_ok(),
        "the finished copy had no durable receipt before cleanup: {recorded:?}"
    );
    let receipt: Value = serde_json::from_slice(&recorded?)?;
    assert_eq!(receipt["schema"], 1);
    assert_eq!(receipt["run_id"], report.id);
    assert_eq!(receipt["work_key"], "s_prepare");
    assert_eq!(
        receipt["saved"],
        json!({ "kind": "git", "oid": oid }),
        "the durable receipt lost the exact Git result"
    );
    assert_eq!(
        saved["copy_results"]["s_prepare"],
        json!({ "kind": "git", "oid": oid }),
        "recovery did not import the exact result saved before cleanup"
    );
    assert!(
        !saved["pending_finalization"]
            .as_array()
            .is_some_and(|items| items.contains(&json!("s_prepare"))),
        "the recovered result remains pending forever"
    );
    assert!(history.is_ok(), "history could not read the recovered run");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_result_receipt_keeps_the_only_working_copy() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run(&processes, true, 1, "window").await?;
    let observed: Result<_, Box<dyn Error>> = async {
        let pgid = process_group(&report, "s_preview_a")?;
        let cwd = bench.cwd("a")?;
        let results = report.dir.join(".results");
        if results.exists() {
            fs::rename(&results, bench.control.path().join("earlier-results"))?;
        }
        fs::write(&results, b"not a directory")?;
        let proofs = processes.close().await;
        assert_dead(&proofs, &[pgid]);
        // Zakończenie callbacku poznajemy po rezultacie albo zdjęciu cwd, nie po arbitralnym
        // śnie. Obie implementacje muszą zapisać końcową informację w księdze biegu.
        let settled = wait_until(|| {
            !cwd.exists()
                || fs::read(report.dir.join("run.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .is_some_and(|run| !run["copy_results"]["s_prepare"].is_null())
        })
        .await;
        Ok((settled, fs::read_to_string(cwd.join("value.txt"))))
    }
    .await;
    let proofs = processes.close().await;
    assert_dead(&proofs, &[]);
    let (settled, kept) = observed?;
    assert!(
        settled,
        "the failed result publication never finished its callback"
    );
    assert_eq!(
        kept?, PREPARED,
        "publication refusal removed the only working copy anyway"
    );
    let results = report.dir.join(".results");
    assert!(fs::symlink_metadata(&results)?.file_type().is_file());
    assert_eq!(fs::read(&results)?, b"not a directory");
    // Wyłącznie przeszkoda, którą ten test postawił: obcy katalog/link nie jest zgodą.
    fs::remove_file(&results)?;
    let _ = loadout_lib::commands::reconcile::reconcile_runs(bench.project.path());
    let branch = loadout_lib::commands::isolate::branch_for(&report.id, "s_prepare");
    let oid = git(bench.project.path(), &["rev-parse", &branch])?
        .trim()
        .to_owned();
    let saved: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    assert_eq!(
        saved["copy_results"]["s_prepare"],
        json!({ "kind": "git", "oid": oid }),
        "recovery before the first receipt did not publish the exact result: {saved}"
    );
    assert!(
        !saved["pending_finalization"]
            .as_array()
            .is_some_and(|items| items.contains(&json!("s_prepare")))
    );
    assert!(
        !bench.cwd("a")?.exists(),
        "the recovered result never released the working folder"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_lifetime_ends_its_service_before_the_run_finishes() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run(&processes, false, 1, "run").await?;
    let pgid = process_group(&report, "s_preview_a")?;
    let cwd = bench.cwd("a")?;
    let dead_at_return = supervisor::group_is_empty(pgid);
    let copies_closed_at_return = !cwd.exists();
    let still_listed = processes.list().iter().any(|item| item.pgid == pgid);
    let proofs = processes.close().await;

    assert_dead(&proofs, &[pgid]);
    assert!(
        dead_at_return,
        "run-owned service was still alive when the run finished"
    );
    assert!(
        !still_listed,
        "the finished run still lists its run-owned service as running"
    );
    assert!(
        copies_closed_at_return,
        "run-owned service death did not release ordinary cleanup"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_a_run_ends_its_service_but_not_an_unrelated_window_process()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let unrelated = processes.start(
        &loadout_lib::engine::drivers::command::StartSpec {
            command: "sleep 600".to_owned(),
            cwd: bench.control.path().to_path_buf(),
        },
        None,
    )?;
    // Nawet Ok(report) może zawierać failed Serve. Zbieramy pytania przed cleanupem,
    // ale każde `?` wychodzi z bloku, nigdy przed zamknięciem własnego sleep.
    let observed: Result<_, Box<dyn Error>> = async {
        let report = bench.run_mode(&processes, false, 1, "run", true).await?;
        let owned = process_group(&report, "s_preview_a")?;
        Ok((
            report,
            owned,
            supervisor::group_is_empty(owned),
            !supervisor::group_is_empty(unrelated.pgid),
        ))
    }
    .await;
    let proofs = processes.close().await;
    assert_dead(&proofs, &[unrelated.pgid]);
    let (report, owned, owned_dead, unrelated_alive) = observed?;

    assert_dead(&proofs, &[owned, unrelated.pgid]);
    assert_eq!(report.outcome, loadout_lib::commands::Outcome::Cancelled);
    assert!(
        owned_dead,
        "Stop returned while its own run-lifetime service was alive"
    );
    assert!(
        unrelated_alive,
        "Stop iterated globally over a different window-owned process"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_a_run_leaves_its_window_service_and_real_copy_available()
-> Result<(), Box<dyn Error>> {
    let bench = Bench::new(true)?;
    let processes = Arc::new(Processes::new());
    let report = bench.run_mode(&processes, false, 1, "window", true).await?;
    let owned = process_group(&report, "s_preview_a")?;
    let still_alive = !supervisor::group_is_empty(owned);
    let result = bench.read_from_service("a").await;
    let proofs = processes.close().await;

    assert_dead(&proofs, &[owned]);
    assert_eq!(report.outcome, loadout_lib::commands::Outcome::Cancelled);
    assert!(
        still_alive,
        "Stop silently treated a window-lifetime preview as run-lifetime"
    );
    assert_eq!(
        result?, INITIAL,
        "Stop left the window service alive but destroyed its copy"
    );
    Ok(())
}

fn assert_dead(proofs: &[GroupProof], groups: &[i32]) {
    assert!(
        proofs
            .iter()
            .all(|proof| matches!(proof, GroupProof::Dead { .. })),
        "test cleanup did not obtain every death proof: {proofs:?}"
    );
    assert!(
        groups.iter().all(|&pgid| supervisor::group_is_empty(pgid)),
        "test cleanup left an owned process group alive: {groups:?}"
    );
}

fn assert_one_result_commit(bench: &Bench, report: &RunReport) -> Result<(), Box<dyn Error>> {
    let branch = loadout_lib::commands::isolate::branch_for(&report.id, "s_prepare");
    assert_eq!(
        git(
            bench.project.path(),
            &["rev-list", "--count", &format!("HEAD..{branch}")]
        )?
        .trim(),
        "1",
        "deferred finalization did not save exactly one result commit"
    );
    assert_eq!(
        git(
            bench.project.path(),
            &["show", &format!("{branch}:value.txt")]
        )?,
        PREPARED
    );
    Ok(())
}

fn process_group(report: &RunReport, node: &str) -> Result<i32, Box<dyn Error>> {
    let saved: Value = serde_json::from_slice(&fs::read(report.dir.join("run.json"))?)?;
    saved["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["node_key"] == node))
        .and_then(|step| step["pgid"].as_i64())
        .and_then(|pgid| i32::try_from(pgid).ok())
        .ok_or_else(|| {
            format!("the executed service {node} has no recorded process group: {saved}").into()
        })
}

async fn wait_until(condition: impl Fn() -> bool) -> bool {
    tokio::time::timeout(PATIENCE, async {
        while !condition() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .is_ok()
}

struct Bench {
    catalog: TempDir,
    project: TempDir,
    control: TempDir,
}

impl Bench {
    fn new(with_git: bool) -> Result<Self, Box<dyn Error>> {
        let bench = Self {
            catalog: TempDir::new()?,
            project: TempDir::new()?,
            control: TempDir::new()?,
        };
        fs::create_dir_all(bench.catalog.path().join("workflows"))?;
        fs::write(bench.project.path().join(".gitignore"), ".loadout/\n")?;
        fs::write(bench.project.path().join("value.txt"), INITIAL)?;
        fs::write(bench.project.path().join("service.sh"), SERVICE)?;
        fs::write(bench.project.path().join("gate.sh"), GATE)?;
        fs::write(bench.project.path().join("prepare.sh"), PREPARE)?;
        if with_git {
            git(bench.project.path(), &["init", "--quiet"])?;
            git(bench.project.path(), &["add", "-A"])?;
            git(
                bench.project.path(),
                &["commit", "--quiet", "-m", "preview input"],
            )?;
        }
        Ok(bench)
    }

    fn cwd(&self, name: &str) -> Result<PathBuf, Box<dyn Error>> {
        Ok(PathBuf::from(
            fs::read_to_string(self.control.path().join(format!("cwd-{name}")))?.trim(),
        ))
    }

    async fn read_from_service(&self, name: &str) -> Result<String, Box<dyn Error>> {
        fs::write(
            self.control.path().join(format!("read-{name}")),
            b"read now",
        )?;
        if !wait_until(|| {
            self.control
                .path()
                .join(format!("read-done-{name}"))
                .exists()
        })
        .await
        {
            return Err(
                format!("service {name} never answered a read after the graph finished").into(),
            );
        }
        Ok(fs::read_to_string(
            self.control.path().join(format!("value-{name}")),
        )?)
    }

    async fn run(
        &self,
        processes: &Arc<Processes>,
        prepared: bool,
        count: usize,
        lifetime: &str,
    ) -> Result<RunReport, Box<dyn Error>> {
        self.run_mode(processes, prepared, count, lifetime, false)
            .await
    }

    async fn run_mode(
        &self,
        processes: &Arc<Processes>,
        prepared: bool,
        count: usize,
        lifetime: &str,
        stop: bool,
    ) -> Result<RunReport, Box<dyn Error>> {
        let workflow = self.catalog.path().join("workflows/live-copy.json");
        fs::write(
            &workflow,
            serde_json::to_vec(&self.workflow(prepared, count, lifetime, stop))?,
        )?;
        fs::create_dir_all(self.project.path().join(".loadout"))?;
        let store = Store::open(&self.project.path().join(".loadout/loadout.db"))?;
        let drivers: Drivers = Arc::new(|_| unreachable!("the service fixture has no model nodes"));
        let deps = RunDeps {
            home: self.catalog.path(),
            project: self.project.path(),
            store: &store,
            drivers,
            processes: Arc::clone(processes),
            control: RunControl::new(),
        };
        let request = RunRequest {
            workflow,
            how_many_at_once: 3,
            task: None,
            part: None,
            handoffs_from: None,
        };
        let (sink, mut source) = line_channel(4096);
        let running = run_workflow_inner(&deps, &request, sink);
        let stopping = async {
            if stop {
                if !wait_until(|| self.control.path().join("gate-is-running").exists()).await {
                    return Err("the stop fixture never reached its running check".to_owned());
                }
                stop_run_inner(&deps).await.map_err(|why| why.to_string())?;
            }
            Ok(())
        };
        let ran = tokio::time::timeout(PATIENCE, async {
            tokio::pin!(running);
            // Failed Serve potrafi zakończyć graf przed barierą Stop. Nie czekamy wtedy
            // 25 sekund na plik, którego nie ma już kto utworzyć.
            if stop {
                tokio::select! {
                    ran = &mut running => (ran, Ok(())),
                    stopped = stopping => (running.await, stopped),
                }
            } else {
                (running.await, Ok(()))
            }
        })
        .await;
        while source.try_next().is_some() {}
        match ran {
            Ok((Ok(report), Ok(()))) => {
                let checked: Result<(), Box<dyn Error>> = (|| {
                    process_group(&report, "s_preview_a")?;
                    self.cwd("a")?;
                    if count == 2 {
                        process_group(&report, "s_preview_b")?;
                        self.cwd("b")?;
                    }
                    Ok(())
                })();
                if let Err(error) = checked {
                    let proofs = processes.close().await;
                    assert_dead(&proofs, &[]);
                    return Err(error);
                }
                Ok(report)
            }
            Ok((Err(error), _)) => {
                let _ = processes.close().await;
                Err(error.into())
            }
            Ok((_, Err(error))) => {
                let _ = processes.close().await;
                Err(error.into())
            }
            Err(error) => {
                // Timeout future nie zabija procesu checka (niezmiennik 10).
                let _ = stop_run_inner(&deps).await;
                let _ = processes.close().await;
                Err(error.into())
            }
        }
    }

    fn workflow(&self, prepared: bool, count: usize, lifetime: &str, stop: bool) -> Value {
        let mut steps = Vec::new();
        let mut links = Vec::new();
        if prepared {
            steps.push(check(
                "s_prepare",
                "Prepare the copy",
                "sh prepare.sh".to_owned(),
                "fresh-copy",
            ));
            links.push(json!({"from": "s_prepare", "to": "s_preview_a"}));
        }
        for name in ["a", "b"].into_iter().take(count) {
            let node = format!("s_preview_{name}");
            steps.push(json!({
                "kind": "serve", "id": node, "name": format!("Preview {name}"),
                "command": format!("sh service.sh '{}' '{name}'", self.control.path().display()),
                "folder": {"use": if prepared || name == "b" { "same-copy" } else { "fresh-copy" }},
                "lifetime": lifetime, "at": {"x": 0, "y": 0}
            }));
            if name == "b" {
                links.push(json!({"from": "s_preview_a", "to": node}));
            }
        }
        let last = if count == 2 {
            "s_preview_b"
        } else {
            "s_preview_a"
        };
        steps.push(check(
            "s_gate",
            "Wait for the real service",
            format!(
                "sh gate.sh '{}' {count} {}",
                self.control.path().display(),
                if stop { "stop" } else { "finish" }
            ),
            "project",
        ));
        links.push(json!({"from": last, "to": "s_gate"}));
        json!({"format": 1, "id": "wf_live_preview_copy", "name": "Live preview owns its copy", "steps": steps, "links": links})
    }
}

fn check(id: &str, name: &str, command: String, folder: &str) -> Value {
    json!({"kind": "check", "id": id, "name": name, "command": command,
        "proof": "(\\d+) passed", "whenItFails": "stop",
        "folder": {"use": folder}, "at": {"x": 0, "y": 0}})
}

fn git(at: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(at)
        .args([
            "-c",
            "user.name=Loadout Test",
            "-c",
            "user.email=test@loadout.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
