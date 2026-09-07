//! Dokument planu konkretnego biegu: model, walidacja zmiany, wersje i render.

use std::fmt;
use std::io;

mod change;
mod document;
mod publish;
mod render;

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
pub use render::{RENDERER, render_plan};

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
