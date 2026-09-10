//! Właściwa publikacja odświeżenia, także gdy rodzice cofają poprzednią zmianę.
use loadout_lib::commands::{
    fan_in::{self, ApplyOutcome, MergePlan, Parent},
    input_snapshot::{self, Entry, InputSnapshot},
};
use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

struct Bench {
    _root: tempfile::TempDir,
    storage: PathBuf,
    project: PathBuf,
    parent: PathBuf,
    consumer: PathBuf,
    origin: InputSnapshot,
}
impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let storage = root.path().join("run");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("dir"))?;
        fs::create_dir_all(&storage)?;
        fs::write(project.join("value.txt"), "original")?;
        fs::write(project.join("dir/base.txt"), "original child")?;
        let origin = input_snapshot::capture(&project, &storage)?;
        let parent = root.path().join("parent");
        let consumer = root.path().join("consumer");
        origin.materialize(&parent)?;
        origin.materialize(&consumer)?;
        Ok(Self {
            _root: root,
            storage,
            project,
            parent,
            consumer,
            origin,
        })
    }
    fn plan(&self) -> Result<MergePlan, Box<dyn Error>> {
        Ok(fan_in::plan_frozen(
            &[Parent {
                name: "Parent",
                cwd: &self.parent,
                born: None,
            }],
            &self.origin,
            &self.project,
        )?)
    }
    fn apply(&self, plan: &MergePlan) -> Result<(), Box<dyn Error>> {
        assert_eq!(
            fan_in::stage_plan(&self.consumer, &self.storage, plan)?
                .apply(|_, _| Ok(()), || false)?,
            ApplyOutcome::Ready
        );
        Ok(())
    }
    fn first(&self) -> Result<BTreeMap<PathBuf, Option<Entry>>, Box<dyn Error>> {
        let plan = self.plan()?;
        self.apply(&plan)?;
        Ok(plan
            .changes
            .iter()
            .map(|one| (one.path.clone(), one.after.clone()))
            .collect())
    }
}

#[test]
fn a_later_input_updates_parent_files_without_losing_the_consumers_own_work()
-> Result<(), Box<dyn Error>> {
    let b = Bench::new()?;
    fs::write(b.parent.join("value.txt"), "one")?;
    let previous = b.first()?;
    fs::write(b.consumer.join("mine.txt"), "consumer work")?;
    fs::write(b.parent.join("value.txt"), "two")?;
    let refreshed = b
        .plan()?
        .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)?;
    b.apply(&refreshed)?;
    assert_eq!(fs::read_to_string(b.consumer.join("value.txt"))?, "two");
    assert_eq!(
        fs::read_to_string(b.consumer.join("mine.txt"))?,
        "consumer work"
    );
    Ok(())
}

#[test]
fn a_conflicting_own_edit_refuses_before_any_write() -> Result<(), Box<dyn Error>> {
    let b = Bench::new()?;
    fs::write(b.parent.join("value.txt"), "one")?;
    let previous = b.first()?;
    fs::write(b.consumer.join("value.txt"), "consumer edit")?;
    fs::write(b.parent.join("value.txt"), "parent edit")?;
    let before = input_snapshot::inspect(&b.consumer)?;
    let error = b
        .plan()?
        .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)
        .err()
        .ok_or("two different edits were silently resolved")?;
    assert!(error.to_string().contains("value.txt"));
    assert_eq!(input_snapshot::inspect(&b.consumer)?, before);
    Ok(())
}

#[test]
fn returning_to_origin_reverts_a_previous_import_instead_of_keeping_it_forever()
-> Result<(), Box<dyn Error>> {
    let b = Bench::new()?;
    fs::write(b.parent.join("value.txt"), "one")?;
    fs::write(b.parent.join("added.txt"), "transient")?;
    let previous = b.first()?;
    fs::write(b.parent.join("value.txt"), "original")?;
    fs::remove_file(b.parent.join("added.txt"))?;
    let refreshed = b
        .plan()?
        .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)?;
    b.apply(&refreshed)?;
    assert_eq!(
        fs::read_to_string(b.consumer.join("value.txt"))?,
        "original"
    );
    assert!(!b.consumer.join("added.txt").exists());
    Ok(())
}

#[test]
fn removing_a_parent_directory_does_not_remove_a_consumers_private_child()
-> Result<(), Box<dyn Error>> {
    let b = Bench::new()?;
    let previous = b.first()?;
    fs::write(b.consumer.join("dir/mine.txt"), "keep me")?;
    fs::remove_file(b.parent.join("dir/base.txt"))?;
    fs::remove_dir(b.parent.join("dir"))?;
    let before = input_snapshot::inspect(&b.consumer)?;
    assert!(
        b.plan()?
            .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)
            .is_err()
    );
    assert_eq!(input_snapshot::inspect(&b.consumer)?, before);
    Ok(())
}

#[test]
fn the_same_input_preserves_an_edit_and_equal_new_edits_agree() -> Result<(), Box<dyn Error>> {
    let b = Bench::new()?;
    fs::write(b.parent.join("value.txt"), "one")?;
    let previous = b.first()?;
    fs::write(b.consumer.join("value.txt"), "mine")?;
    let same = b
        .plan()?
        .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)?;
    b.apply(&same)?;
    assert_eq!(fs::read_to_string(b.consumer.join("value.txt"))?, "mine");
    fs::write(b.parent.join("value.txt"), "mine")?;
    let equal = b
        .plan()?
        .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)?;
    b.apply(&equal)?;
    assert_eq!(fs::read_to_string(b.consumer.join("value.txt"))?, "mine");
    Ok(())
}

#[test]
fn a_consumer_changed_after_planning_refuses_publication() -> Result<(), Box<dyn Error>> {
    let b = Bench::new()?;
    fs::write(b.parent.join("value.txt"), "one")?;
    let previous = b.first()?;
    fs::write(b.parent.join("value.txt"), "two")?;
    let refreshed = b
        .plan()?
        .refresh(&b.origin, &previous, &b.consumer, "Consumer", &b.project)?;
    fs::write(b.consumer.join("value.txt"), "changed during preparation")?;
    assert!(fan_in::stage_plan(&b.consumer, &b.storage, &refreshed).is_err());
    assert_eq!(
        fs::read_to_string(b.consumer.join("value.txt"))?,
        "changed during preparation"
    );
    Ok(())
}
