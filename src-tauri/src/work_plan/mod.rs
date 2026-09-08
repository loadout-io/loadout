//! Dokument planu konkretnego biegu: model, walidacja zmiany, wersje i render.

use std::fmt;
use std::io;

mod candidate;
mod change;
mod document;
mod publish;
mod recorded;
mod render;
mod review;

pub use crate::commands::context_inputs::{Composed, ContextBlock, compose};
pub use candidate::WorkPlanCore;
pub use candidate::{Configuration, DOCUMENT_ID, MAX_CANDIDATE_BYTES, Mode, Prepared};
pub use change::{
    HumanRequirement, PlanCreate, PlanUpdate, ProposedRequirement, first_document, updated_document,
};
pub use document::{
    AcceptanceCriterion, Origin, PlanDocument, Requirement, SectionKey, SourceRef, Status,
};
pub use publish::{
    PlanVersion, Publication, Stamp, plan_root, publish_version, read_current_version,
    read_versions,
};
pub(crate) use recorded::Recorder as PlanReadRecorder;
pub(crate) use recorded::SavedNode as RecordedPlanNode;
pub(crate) use recorded::recorder as plan_read_recorder;
pub(crate) use recorded::removal_guard as recorded_removal_guard;
pub use recorded::{Binding as RecordedBinding, Snapshot as RecordedSnapshot};
pub(crate) use recorded::{binding as recorded_binding, hold as hold_recorded};
pub(crate) use recorded::{is_held as recorded_is_held, read_bound as read_recorded};
pub use render::{RENDERER, render_plan};
pub use render::{detail_index, render_core, render_details};
pub use review::{
    Basis, StillOpen, about_other_work, approved, carry, conflicts, frozen_criteria,
    not_this_product, told_what_is_still_open,
};

/// Pierwszy i jedyny kształt pliku, dopóki naprawdę nie pojawi się potrzeba migracji.
pub const SCHEMA: u32 = 1;
pub const LATEST_SCHEMA: u32 = SCHEMA;

/// Nazwane odmowy planu; każda jest jednym zdaniem, które może później pokazać UI.
#[derive(Debug)]
pub enum Error {
    Stale,
    OutOfScope,
    WouldWeakenHumanRequirement,
    RepeatedRequirement,
    NoSuchPlan,
    NotAllowed,
    NotDelivered(String),
    Malformed(String),
    Unwritable(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale => formatter.write_str(
                "This plan was not published because its parent is no longer current, so nothing was overwritten.",
            ),
            Self::OutOfScope => formatter.write_str(
                "This plan was not published because it changes a section outside the entrusted scope, so nothing was overwritten.",
            ),
            Self::WouldWeakenHumanRequirement => formatter.write_str(
                "This plan was not published because it would remove or change a human requirement, so nothing was overwritten.",
            ),
            Self::RepeatedRequirement => formatter.write_str(
                "This plan was not published because a requirement identifier is repeated, so nothing was overwritten.",
            ),
            Self::NoSuchPlan => {
                formatter.write_str("Loadout has no published plan under that identity.")
            }
            Self::NotAllowed => formatter.write_str(
                "This step cannot publish a plan because its Plan setting does not allow changes.",
            ),
            Self::NotDelivered(reason) => write!(
                formatter,
                "This step did not deliver a valid plan document: {reason}. No plan was published."
            ),
            Self::Malformed(reason) => write!(
                formatter,
                "This plan candidate was refused because its shape is not valid: {reason}."
            ),
            Self::Unwritable(error) => {
                write!(formatter, "This plan could not be published: {error}.")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Unwritable(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Malformed(error.to_string())
    }
}
