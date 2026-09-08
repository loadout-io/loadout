//! Kandydat jest wynikiem roboczym agenta; dopiero host przypina rodzica i publikuje dokument.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    Error, HumanRequirement, PlanCreate, PlanDocument, PlanUpdate, PlanVersion, Publication,
    SectionKey, Stamp, detail_index, first_document, publish_version, read_current_version,
    render_core, render_details, updated_document,
};

pub const DOCUMENT_ID: &str = "workflow-plan";
pub const MAX_CANDIDATE_BYTES: usize = 256 * 1024;

const CREATE_SHAPE: &str = r#"{"goal":"...","inScope":["..."],"outOfScope":["..."],"requirements":[{"id":"R2","text":"...","acceptance":[{"text":"...","verification":"..."}]}],"acceptance":[{"text":"...","verification":"..."}],"decisions":["..."],"sections":{"Implementation":"...","Design":"...","Validation":"..."},"proposals":["..."],"assumptions":["..."],"questions":["..."],"conflicts":["..."],"sources":[{"name":"...","locator":"...","version":"..."}]}"#;
const UPDATE_CHANGES: &str = r#"{"sections":{"Design":"..."},"requirements":[{"id":"R2","text":"...","acceptance":[]}],"proposals":[],"questions":[],"conflicts":[]}"#;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Off,
    Create,
    Update,
    Use,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Configuration {
    pub mode: Mode,
    pub can_update: Vec<SectionKey>,
    pub focus_on: Vec<SectionKey>,
    pub same_plan_as: Option<String>,
}

impl Configuration {
    pub fn from_step(value: Option<&Value>) -> Result<Self, Error> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let configuration: Self = serde_json::from_value(value.clone())?;
        if configuration.mode == Mode::Unknown {
            return Err(Error::Malformed(
                "this build does not know that Plan setting".to_owned(),
            ));
        }
        if configuration.mode != Mode::Update && !configuration.can_update.is_empty() {
            return Err(Error::Malformed(
                "only a Plan Update step may name sections it can change".to_owned(),
            ));
        }
        if configuration.mode != Mode::Use && !configuration.focus_on.is_empty() {
            return Err(Error::Malformed(
                "only a Plan Use step may name sections to focus on".to_owned(),
            ));
        }
        if !matches!(configuration.mode, Mode::Update | Mode::Use)
            && configuration.same_plan_as.is_some()
        {
            return Err(Error::Malformed(
                "only a Plan Update or Use step may choose a plan source".to_owned(),
            ));
        }
        Ok(configuration)
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub fn writes_candidate(&self) -> bool {
        matches!(self.mode, Mode::Create | Mode::Update)
    }

    pub fn prepare(
        &self,
        root: PathBuf,
        candidate: PathBuf,
        human_requirements: Vec<HumanRequirement>,
    ) -> Result<Prepared, Error> {
        let current = match self.mode {
            Mode::Create => match read_current_version(&root) {
                Ok(_version) => return Err(Error::Stale),
                Err(Error::NoSuchPlan) => None,
                Err(error) => return Err(error),
            },
            Mode::Update | Mode::Use => Some(read_current_version(&root)?),
            Mode::Off | Mode::Unknown => None,
        };
        self.prepare_pinned(root, candidate, human_requirements, current)
    }

    pub(crate) fn prepare_pinned(
        &self,
        root: PathBuf,
        candidate: PathBuf,
        human_requirements: Vec<HumanRequirement>,
        current: Option<PlanVersion>,
    ) -> Result<Prepared, Error> {
        if matches!(self.mode, Mode::Update | Mode::Use) && current.is_none() {
            return Err(Error::NoSuchPlan);
        }
        if self.mode == Mode::Create && current.is_some() {
            return Err(Error::Stale);
        }
        let can_update = if self.mode == Mode::Update && self.can_update.is_empty() {
            current
                .as_ref()
                .map(|version| version.document.sections.keys().cloned().collect())
                .unwrap_or_default()
        } else {
            self.can_update.clone()
        };
        Ok(Prepared {
            mode: self.mode,
            root,
            candidate,
            current,
            can_update,
            focus_on: self.focus_on.clone(),
            human_requirements,
        })
    }
}

/// Zamrożone uprawnienie jednego fizycznego kroku i jednej próby.
#[derive(Clone, Debug)]
pub struct Prepared {
    mode: Mode,
    root: PathBuf,
    candidate: PathBuf,
    current: Option<PlanVersion>,
    can_update: Vec<SectionKey>,
    focus_on: Vec<SectionKey>,
    human_requirements: Vec<HumanRequirement>,
}

#[derive(Clone, Debug)]
pub struct WorkPlanCore {
    pub version: u64,
    pub version_id: String,
    pub required: String,
    pub details: String,
    pub index: Vec<(String, String)>,
}

impl Prepared {
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub fn writes_candidate(&self) -> bool {
        matches!(self.mode, Mode::Create | Mode::Update)
    }

    #[must_use]
    pub fn candidate(&self) -> &Path {
        &self.candidate
    }

    #[must_use]
    pub fn current(&self) -> Option<&PlanVersion> {
        self.current.as_ref()
    }

    #[must_use]
    pub fn core_for_prompt(&self) -> Option<WorkPlanCore> {
        let current = self.current.as_ref()?;
        Some(WorkPlanCore {
            version: current.version,
            version_id: current.version_id.clone(),
            required: render_core(current),
            details: render_details(&current.document, &self.focus_on),
            index: detail_index(&current.document),
        })
    }

    #[must_use]
    pub fn instruction(&self) -> Option<String> {
        match self.mode {
            Mode::Create => Some(format!(
                "\nPLAN DOCUMENT\nCreate a complete plan candidate as JSON. This candidate is separate from your step result: write the JSON only to the path chosen by Loadout below, then still answer this step normally. Do not choose another path and do not add origin or status to requirements. Use exactly this shape (empty arrays are allowed):\n{CREATE_SHAPE}\nLOADOUT_PLAN_CANDIDATE_PATH={}\n",
                self.candidate.display(),
            )),
            Mode::Update => {
                let current = self.current.as_ref()?;
                let sections = self
                    .can_update
                    .iter()
                    .map(SectionKey::label)
                    .collect::<Vec<_>>()
                    .join(", ");
                Some(format!(
                    "\nPLAN DOCUMENT\nUpdate the pinned plan version {} ({}) by writing one JSON candidate to the path chosen by Loadout below. This candidate is separate from your step result: still answer this step normally. Read the complete pinned plan with read_plan when needed. Use {{\"baseVersion\":{},\"result\":\"unchanged\"}} when nothing changed, or {{\"baseVersion\":{},\"result\":\"changes\",\"changes\":{UPDATE_CHANGES}}} (empty arrays are allowed). You may change only these sections: {}. Do not choose another path.\nLOADOUT_PLAN_CANDIDATE_PATH={}\n",
                    current.version,
                    current.version_id,
                    current.version,
                    current.version,
                    sections,
                    self.candidate.display()
                ))
            }
            Mode::Use => {
                let current = self.current.as_ref()?;
                let focus = if self.focus_on.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Give extra attention to these detailed sections: {}. The complete shared core and every human requirement still apply.",
                        self.focus_on
                            .iter()
                            .map(SectionKey::label)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                Some(format!(
                    "\nPLAN DOCUMENT\nUse the complete pinned plan version {} ({}). Read it with read_plan when needed. This step cannot publish a plan.{}\n",
                    current.version, current.version_id, focus
                ))
            }
            Mode::Off | Mode::Unknown => None,
        }
    }

    pub fn publish(&self, stamp: &Stamp, bytes: &[u8]) -> Result<Publication, Error> {
        let document = self.document(bytes)?;
        publish_version(&self.root, stamp, &document)
    }

    /// 2026-09-08 — sprawdza bez publikacji, aby ta sama sesja mogła dostać kolejną turę.
    pub(crate) fn validate(&self, bytes: &[u8]) -> Result<(), Error> {
        self.document(bytes).map(|_document| ())
    }

    fn document(&self, bytes: &[u8]) -> Result<PlanDocument, Error> {
        if !self.writes_candidate() {
            return Err(Error::NotAllowed);
        }
        if bytes.len() > MAX_CANDIDATE_BYTES {
            return Err(Error::NotDelivered(format!(
                "the candidate is larger than {MAX_CANDIDATE_BYTES} bytes"
            )));
        }
        let candidate = std::str::from_utf8(bytes)
            .map_err(|_error| Error::NotDelivered("the candidate is not UTF-8 JSON".to_owned()))?;
        match self.mode {
            Mode::Create => {
                let create = PlanCreate::try_from(candidate)?;
                if create.goal.trim().is_empty() {
                    return Err(Error::NotDelivered("the candidate has no goal".to_owned()));
                }
                first_document(create, self.human_requirements.clone())
            }
            Mode::Update => {
                let current = self.current.as_ref().ok_or(Error::NoSuchPlan)?;
                let submission: UpdateCandidate = serde_json::from_str(candidate)?;
                if submission.base_version() != current.version {
                    return Err(Error::Stale);
                }
                let document = match submission {
                    UpdateCandidate::Unchanged { .. } => current.document.clone(),
                    UpdateCandidate::Changes { mut changes, .. } => {
                        // 2026-09-08 — zakres pochodzi z hosta, bo agent nie może poszerzyć
                        // własnego uprawnienia przez pole zapisane w kandydacie.
                        changes.scope.clone_from(&self.can_update);
                        updated_document(&current.document, changes)?
                    }
                };
                Ok(document)
            }
            Mode::Off | Mode::Use | Mode::Unknown => Err(Error::NotAllowed),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "result", rename_all = "lowercase", deny_unknown_fields)]
enum UpdateCandidate {
    Unchanged {
        #[serde(rename = "baseVersion")]
        base_version: u64,
    },
    Changes {
        #[serde(rename = "baseVersion")]
        base_version: u64,
        changes: PlanUpdate,
    },
}

impl UpdateCandidate {
    fn base_version(&self) -> u64 {
        match self {
            Self::Unchanged { base_version } | Self::Changes { base_version, .. } => *base_version,
        }
    }
}
