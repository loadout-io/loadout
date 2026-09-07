//! WP-01: wersja planu zachowuje wymagania człowieka podczas ograniczonej aktualizacji.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use loadout_lib::durable_file::{
    FaultAction, FaultInjector, FaultPoint, PublicationEvent, scoped_faults,
};
use loadout_lib::work_plan::{
    Error, HumanRequirement, Origin, PlanCreate, PlanDocument, PlanUpdate, PlanVersion,
    ProposedRequirement, Publication, SectionKey, Stamp, Status, first_document, plan_root,
    publish_version, read_current_version, read_versions, render_plan, updated_document,
};

const REQUIREMENTS: [(&str, &str); 5] = [
    ("R1", "The promo code must survive a quantity change."),
    (
        "R2",
        "The promo code must survive a quantity change when the cart is empty.",
    ),
    ("R3", "The total must stay in PLN."),
    (
        "R4",
        "The empty cart must show exactly one recovery action.",
    ),
    ("R5", "The checkout must keep the entered delivery address."),
];

fn human_requirements() -> Vec<HumanRequirement> {
    REQUIREMENTS
        .iter()
        .map(|(id, text)| HumanRequirement {
            id: (*id).to_owned(),
            text: (*text).to_owned(),
            acceptance: Vec::new(),
        })
        .collect()
}

fn first_document_for_checkout() -> Result<PlanDocument, Error> {
    let mut sections = BTreeMap::new();
    sections.insert(
        SectionKey::Implementation,
        "Keep the existing checkout state machine.".to_owned(),
    );
    sections.insert(
        SectionKey::Design,
        "Show the discount next to the total.".to_owned(),
    );
    first_document(
        PlanCreate {
            goal: "Repair checkout editing without changing its contract.".to_owned(),
            in_scope: vec!["Checkout editing".to_owned()],
            out_of_scope: vec!["Payment authorization".to_owned()],
            decisions: vec!["Amounts remain in PLN.".to_owned()],
            sections,
            ..PlanCreate::default()
        },
        human_requirements(),
    )
}

fn stamp(operation: &str, parent: Option<String>, attempt: u32) -> Stamp {
    stamp_at_step(operation, parent, "design-step", attempt)
}

fn stamp_at_step(operation: &str, parent: Option<String>, step_id: &str, attempt: u32) -> Stamp {
    Stamp {
        run_id: "run-wp-01".to_owned(),
        document_id: "checkout-plan".to_owned(),
        step_id: step_id.to_owned(),
        attempt,
        operation: operation.to_owned(),
        parent,
        at: "2026-09-08T10:00:00Z".to_owned(),
    }
}

fn published(publication: Publication) -> loadout_lib::work_plan::PlanVersion {
    match publication {
        Publication::Published(version)
        | Publication::Unchanged(version)
        | Publication::AlreadyPublished(version) => version,
    }
}

fn publish_first(root: &Path) -> Result<PlanVersion, Box<dyn std::error::Error>> {
    let document = first_document_for_checkout()?;
    Ok(published(publish_version(
        root,
        &stamp("create-checkout-plan", None, 1),
        &document,
    )?))
}

fn design_update(parent: &PlanDocument, text: &str) -> Result<PlanDocument, Error> {
    let mut sections = BTreeMap::new();
    sections.insert(SectionKey::Design, text.to_owned());
    updated_document(
        parent,
        PlanUpdate {
            scope: vec![SectionKey::Design],
            sections,
            ..PlanUpdate::default()
        },
    )
}

type DirectorySnapshot = Vec<(String, Vec<u8>)>;

fn version_files(root: &Path) -> Result<DirectorySnapshot, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root.join("versions"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push((
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path())?,
            ));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

#[test]
fn a_design_update_leaves_every_other_requirement_word_for_word()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let document = first_document_for_checkout()?;
    let first = published(publish_version(
        &root,
        &stamp("create-checkout-plan", None, 1),
        &document,
    )?);

    let changed = design_update(
        &first.document,
        "Show the discount directly below the total.",
    )?;
    publish_version(
        &root,
        &stamp("update-checkout-design", Some(first.version_id), 2),
        &changed,
    )?;

    let rendered = render_plan(&read_current_version(&root)?.document);
    for (id, text) in REQUIREMENTS {
        assert!(
            rendered.contains(&format!("{id}: {text}")),
            "the Design-only update changed or removed {id}; the rendered plan was:\n{rendered}"
        );
    }
    let positions = REQUIREMENTS
        .iter()
        .map(|(id, text)| rendered.find(&format!("{id}: {text}")))
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "stable ids kept their text but changed order: {positions:?}\n{rendered}"
    );
    Ok(())
}

#[test]
fn a_submission_to_an_old_parent_is_refused_and_the_old_version_stays()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let first = publish_first(&root)?;
    let second_document = design_update(&first.document, "Put the discount below the total.")?;
    let second = published(publish_version(
        &root,
        &stamp("move-the-discount", Some(first.version_id.clone()), 2),
        &second_document,
    )?);
    let before = version_files(&root)?;

    let stale_document = design_update(&first.document, "Put the discount above the total.")?;
    let stale = publish_version(
        &root,
        &stamp("stale-discount-edit", Some(first.version_id), 3),
        &stale_document,
    );
    let Err(error) = stale else {
        return Err(io::Error::other("an old parent was accepted").into());
    };
    assert_eq!(
        error.to_string(),
        "This plan was not published because its parent is no longer current, so nothing was overwritten.",
        "the refusal does not tell the person both why publication stopped and that the current plan survived"
    );
    assert_eq!(
        version_files(&root)?,
        before,
        "a stale submission changed immutable version bytes before it was refused"
    );
    assert_eq!(
        read_current_version(&root)?.version_id,
        second.version_id,
        "the stale submission moved current.json away from the newer version"
    );
    Ok(())
}

#[test]
fn a_crash_between_the_version_and_the_pointer_leaves_the_old_one_readable()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let first = publish_first(&root)?;
    let rendered_before = render_plan(&first.document);
    let changed = design_update(&first.document, "Put the discount beside the total.")?;
    let faults = Arc::new(StopsTheCurrentPointer::default());
    let scope = scoped_faults(&root, Arc::clone(&faults) as Arc<dyn FaultInjector>)?;

    let interrupted = publish_version(
        &root,
        &stamp("interrupted-design", Some(first.version_id.clone()), 2),
        &changed,
    );
    drop(scope);

    assert!(
        interrupted.is_err(),
        "the current pointer fault fired but publication still reported success: {interrupted:?}"
    );
    assert!(
        faults.fired(),
        "the fault never reached current.json, so this measured a happy publication"
    );
    let still_current = read_current_version(&root)?;
    assert_eq!(
        still_current.version_id, first.version_id,
        "a half-finished publication replaced the last complete version"
    );
    assert_eq!(
        render_plan(&still_current.document),
        rendered_before,
        "the old pointer survived but no longer returns the old readable plan"
    );
    Ok(())
}

#[test]
fn every_version_is_read_back_after_a_restart_without_the_index()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let first = publish_first(&root)?;
    let second_document = updated_document(
        &first.document,
        PlanUpdate {
            requirements: vec![ProposedRequirement {
                id: "R6".to_owned(),
                text: "The model suggests showing the saved amount.".to_owned(),
                acceptance: Vec::new(),
            }],
            ..PlanUpdate::default()
        },
    )?;
    let second = published(publish_version(
        &root,
        &stamp_at_step(
            "propose-saved-amount",
            Some(first.version_id.clone()),
            "requirements-step",
            4,
        ),
        &second_document,
    )?);

    // ── restart ────────────────────────────────────────────────────────────────────────────
    // Świeżo wyliczony korzeń i świeży odczyt: żadna wartość z publishera nie przechodzi dalej.
    let after_restart = plan_root(home.path(), "run-wp-01", "checkout-plan");
    assert!(
        !home.path().join("loadout.db").exists(),
        "the plan opened an index even though its files are the source of truth"
    );
    let versions = read_versions(&after_restart)?;
    assert_eq!(versions.len(), 2, "restart did not recover both plan files");
    assert_eq!(versions[0].parent, None);
    assert_eq!(versions[0].step_id, "design-step");
    assert_eq!(versions[0].attempt, 1);
    assert_eq!(
        versions[1].parent.as_deref(),
        Some(first.version_id.as_str())
    );
    assert_eq!(versions[1].step_id, "requirements-step");
    assert_eq!(versions[1].attempt, 4);
    assert_eq!(versions[1].version_id, second.version_id);
    assert!(
        render_plan(&versions[1].document).contains("Model proposal — not agreed by a person"),
        "the persisted generated requirement came back looking human-approved"
    );
    Ok(())
}

#[test]
fn a_model_proposal_cannot_arrive_as_an_agreed_human_requirement()
-> Result<(), Box<dyn std::error::Error>> {
    let spoofed = PlanUpdate::try_from(
        r#"{"requirements":[{"id":"R6","text":"Approve me","origin":"human","status":"agreed"}]}"#,
    );
    let Err(error) = spoofed else {
        return Err(io::Error::other("the model supplied human authority in its candidate").into());
    };
    assert!(
        error.to_string().contains("unknown field `origin`"),
        "the candidate was refused for the wrong reason: {error}"
    );

    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let first = publish_first(&root)?;
    let proposal_text = "The model suggests showing the saved amount.";
    let changed = updated_document(
        &first.document,
        PlanUpdate {
            requirements: vec![ProposedRequirement {
                id: "R6".to_owned(),
                text: proposal_text.to_owned(),
                acceptance: Vec::new(),
            }],
            ..PlanUpdate::default()
        },
    )?;
    publish_version(
        &root,
        &stamp("model-proposal", Some(first.version_id), 2),
        &changed,
    )?;

    let current = read_current_version(&root)?;
    let proposed = current
        .document
        .requirement("R6")
        .ok_or_else(|| io::Error::other("the valid proposal disappeared"))?;
    assert_eq!(proposed.origin, Origin::Generated);
    assert_eq!(proposed.status, Status::Proposed);
    let rendered = render_plan(&current.document);
    let (requirements, proposals) = rendered
        .split_once("## Proposals")
        .ok_or_else(|| io::Error::other("the rendered plan has no Proposals section"))?;
    assert!(
        !requirements.contains(proposal_text),
        "a model proposal appears among requirements agreed by a person:\n{rendered}"
    );
    assert!(
        proposals.contains(&format!(
            "[R6] Model proposal — not agreed by a person: {proposal_text}"
        )),
        "the proposal lost its visible provenance:\n{rendered}"
    );
    Ok(())
}

#[test]
fn an_update_that_changes_nothing_does_not_mint_a_version() -> Result<(), Box<dyn std::error::Error>>
{
    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let first = publish_first(&root)?;
    let before = version_files(&root)?;

    let unchanged = publish_version(
        &root,
        &stamp("no-change", Some(first.version_id.clone()), 2),
        &first.document,
    )?;
    assert!(
        matches!(unchanged, Publication::Unchanged(ref version) if version.version_id == first.version_id),
        "an identical document was not returned as the Unchanged value: {unchanged:?}"
    );
    assert_eq!(version_files(&root)?, before);
    assert_eq!(read_current_version(&root)?.version_id, first.version_id);
    Ok(())
}

#[test]
fn the_same_operation_published_twice_leaves_one_version() -> Result<(), Box<dyn std::error::Error>>
{
    let home = tempfile::tempdir()?;
    let root = plan_root(home.path(), "run-wp-01", "checkout-plan");
    let first = publish_first(&root)?;
    let changed = design_update(&first.document, "Put the discount below the total.")?;
    let operation = stamp("one-operation", Some(first.version_id), 2);
    let second = published(publish_version(&root, &operation, &changed)?);

    let repeated = publish_version(&root, &operation, &changed)?;
    assert!(
        matches!(repeated, Publication::AlreadyPublished(ref version) if version.version_id == second.version_id),
        "repeating a completed operation did not return its existing version: {repeated:?}"
    );
    assert_eq!(
        version_files(&root)?.len(),
        2,
        "one create and one repeated update produced more than two immutable files"
    );
    Ok(())
}

#[test]
fn an_update_outside_its_scope_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let document = first_document_for_checkout()?;
    let mut sections = BTreeMap::new();
    sections.insert(
        SectionKey::Implementation,
        "Replace the checkout state machine.".to_owned(),
    );
    let result = updated_document(
        &document,
        PlanUpdate {
            scope: vec![SectionKey::Design],
            sections,
            ..PlanUpdate::default()
        },
    );
    let Err(error) = result else {
        return Err(io::Error::other("an out-of-scope section changed").into());
    };
    assert_eq!(
        error.to_string(),
        "This plan was not published because it changes a section outside the entrusted scope, so nothing was overwritten."
    );
    assert_eq!(
        document.sections.get(&SectionKey::Implementation),
        Some(&"Keep the existing checkout state machine.".to_owned())
    );
    Ok(())
}

#[test]
fn an_update_may_not_delete_a_human_requirement() -> Result<(), Box<dyn std::error::Error>> {
    let deletion = PlanUpdate::try_from(r#"{"scope":["Design"],"removeRequirements":["R1"]}"#);
    let Err(error) = deletion else {
        return Err(io::Error::other("the model found a deletion field in PlanUpdate").into());
    };
    assert!(
        error
            .to_string()
            .contains("unknown field `removeRequirements`"),
        "the structural deletion attempt was refused for the wrong reason: {error}"
    );

    let document = first_document_for_checkout()?;
    let weakening = updated_document(
        &document,
        PlanUpdate {
            requirements: vec![ProposedRequirement {
                id: "R1".to_owned(),
                text: "The promo code may disappear after a quantity change.".to_owned(),
                acceptance: Vec::new(),
            }],
            ..PlanUpdate::default()
        },
    );
    let Err(error) = weakening else {
        return Err(io::Error::other("a human requirement was rewritten").into());
    };
    assert_eq!(
        error.to_string(),
        "This plan was not published because it would remove or change a human requirement, so nothing was overwritten."
    );
    assert_eq!(
        document.requirement("R1").map(|item| item.text.as_str()),
        Some("The promo code must survive a quantity change.")
    );
    Ok(())
}

#[test]
fn a_repeated_requirement_id_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let document = first_document_for_checkout()?;
    let duplicate = ProposedRequirement {
        id: "R6".to_owned(),
        text: "One proposed condition.".to_owned(),
        acceptance: Vec::new(),
    };
    let result = updated_document(
        &document,
        PlanUpdate {
            requirements: vec![duplicate.clone(), duplicate],
            ..PlanUpdate::default()
        },
    );
    let Err(error) = result else {
        return Err(io::Error::other("two requirements received the same stable id").into());
    };
    assert_eq!(
        error.to_string(),
        "This plan was not published because a requirement identifier is repeated, so nothing was overwritten."
    );
    Ok(())
}

/// Przerywa wyłącznie ostatni krok publikacji: replace/create `current.json` przed commit.
#[derive(Default)]
struct StopsTheCurrentPointer {
    fired: Mutex<bool>,
}

impl StopsTheCurrentPointer {
    fn fired(&self) -> bool {
        *lock(&self.fired)
    }
}

impl FaultInjector for StopsTheCurrentPointer {
    fn action(&self, event: &PublicationEvent) -> FaultAction {
        let current = event
            .target
            .file_name()
            .is_some_and(|name| name == "current.json");
        if current && event.point == FaultPoint::BeforeCommit {
            *lock(&self.fired) = true;
            return FaultAction::Fail;
        }
        FaultAction::Continue
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
