//! WF-19: picker dostaje adres tylko po weryfikacji prawdziwego `input_snapshot`.
use loadout_lib::commands::{history::read_run_inner, input_snapshot};
use serde_json::{Value, json};
use std::{error::Error, fs, path::PathBuf};

struct Saved {
    _root: tempfile::TempDir,
    project: PathBuf,
    dir: PathBuf,
    folder: String,
    id: String,
    snapshot: input_snapshot::InputSnapshot,
}
impl Saved {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let project = root.path().join("project");
        fs::create_dir_all(&project)?;
        fs::write(project.join("seed.txt"), "the original input")?;
        let id = uuid::Uuid::now_v7().to_string();
        let folder = format!("20260906-020000__{id}");
        let dir = project.join(".loadout/runs").join(&folder);
        fs::create_dir_all(&dir)?;
        let snapshot = input_snapshot::capture(&project, &dir)?;
        let saved = Self {
            _root: root,
            project,
            dir,
            folder,
            id,
            snapshot,
        };
        saved.description(Some(saved.snapshot.id()))?;
        Ok(saved)
    }
    fn description(&self, input: Option<&str>) -> Result<(), Box<dyn Error>> {
        let mut file = json!({"id":self.id,"title":"Saved starting point","workflow_id":"subject",
            "status":"succeeded","steps":[]});
        if let Some(input) = input {
            file["input_snapshot"] = json!({"id":input,"manifest":"input/manifest.json"});
        }
        fs::write(self.dir.join("run.json"), serde_json::to_vec(&file)?)?;
        Ok(())
    }
    fn read(&self) -> Result<Value, Box<dyn Error>> {
        Ok(serde_json::to_value(read_run_inner(
            &self.project,
            &self.folder,
        )?)?)
    }
    fn unavailable(&self) -> Result<(), Box<dyn Error>> {
        let shown = self.read()?;
        assert!(
            shown["savedInput"].is_null(),
            "unverified input became selectable: {shown}"
        );
        assert!(
            shown["savedInputSaid"]
                .as_str()
                .is_some_and(|said| !said.is_empty()),
            "the picker has no reason to show for unavailable starting files: {shown}"
        );
        Ok(())
    }
}

#[test]
fn the_real_history_reader_returns_the_exact_verified_starting_input() -> Result<(), Box<dyn Error>>
{
    let saved = Saved::new()?;
    fs::write(
        saved.project.join("seed.txt"),
        "today is not the saved input",
    )?;
    let shown = saved.read()?;
    assert_eq!(
        shown["savedInput"],
        json!({"sourceRunId":saved.id,"snapshotId":saved.snapshot.id()})
    );
    assert!(shown["savedInputSaid"].is_null());
    Ok(())
}

#[test]
fn corrupted_starting_files_remain_unselectable() -> Result<(), Box<dyn Error>> {
    let saved = Saved::new()?;
    fs::write(
        saved.snapshot.files().join("seed.txt"),
        "changed after capture",
    )?;
    saved.unavailable()
}

#[test]
fn a_kept_receipt_with_missing_input_is_not_a_saved_starting_point() -> Result<(), Box<dyn Error>> {
    let saved = Saved::new()?;
    fs::remove_file(saved.dir.join("input/manifest.json"))?;
    saved.unavailable()
}

#[test]
fn an_input_bound_to_a_different_run_record_is_not_selectable() -> Result<(), Box<dyn Error>> {
    let saved = Saved::new()?;
    saved.description(Some(&uuid::Uuid::now_v7().to_string()))?;
    saved.unavailable()
}

#[test]
fn legacy_history_does_not_adopt_an_unbound_input_folder() -> Result<(), Box<dyn Error>> {
    let saved = Saved::new()?;
    saved.description(None)?;
    saved.unavailable()
}
