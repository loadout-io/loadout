//! Cienki czasownik planu nad tym samym ograniczonym czytnikiem co Context.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{Answer, Call, host::Answers, verbs::Verb};
use crate::{
    context::access::ContextAccess,
    work_plan::{PlanVersion, render_plan},
};

const NAME: &str = "read_plan";

#[derive(Debug)]
pub struct PlanDesk {
    version_id: String,
    access: ContextAccess,
    recorder: Option<crate::work_plan::PlanReadRecorder>,
}

impl PlanDesk {
    #[must_use]
    pub fn new(
        holder: impl Into<String>,
        version: &PlanVersion,
        expires: CancellationToken,
    ) -> Self {
        let version_id = version.version_id.clone();
        Self {
            access: ContextAccess::one_text(
                holder,
                "workflow-plan",
                version_id.clone(),
                version_id.clone(),
                format!("Plan version {}", version.version),
                render_plan(&version.document),
                expires,
            ),
            version_id,
            recorder: None,
        }
    }

    /// 2026-09-08 (WP-06): historia odczytu należy do fizycznego odbiorcy, a nie do wersji;
    /// dwa równoległe kroki nie współdzielą licznika ani zamka (niezmienniki 8 i 11).
    #[must_use]
    pub(crate) fn recording(
        holder: impl Into<String>,
        version: &PlanVersion,
        expires: CancellationToken,
        recorder: crate::work_plan::PlanReadRecorder,
    ) -> Self {
        let mut desk = Self::new(holder, version, expires);
        desk.recorder = Some(recorder);
        desk
    }

    #[must_use]
    pub fn tools(&self) -> Value {
        Value::Array(vec![
            Verb {
                name: NAME,
                describe: "Read the complete plan version pinned to this step, at most 16 KiB at a time. Pass the returned cursor unchanged to continue.",
                schema: json!({"type":"object","properties":{"versionId":{"type":"string"},"cursor":{"type":"string"}},"required":["versionId"],"additionalProperties":false}),
                read_only: true,
            }
            .listed(),
        ])
    }

    fn dispatch(&self, call: &Call) -> Result<Answer, String> {
        if call.call != NAME {
            return Err("This plan action is not available to this agent.".to_owned());
        }
        let input: ReadPlan = serde_json::from_value(call.input.clone()).map_err(|_error| {
            "Name the pinned plan version and pass only its exact reading cursor.".to_owned()
        })?;
        if input.version_id != self.version_id {
            return Err("That plan version was not given to this step.".to_owned());
        }
        let answer = self
            .access
            .read(&self.version_id, input.cursor.as_deref())
            .map_err(|error| error.to_string())?;
        let bytes = answer.text.len();
        let value = serde_json::to_value(answer)
            .map_err(|_error| "Loadout could not shape this plan answer safely.".to_owned())?;
        if let Some(recorder) = &self.recorder {
            recorder
                .opened(&self.version_id, bytes)
                .map_err(|error| error.to_string())?;
        }
        Ok(Answer::Ok(value))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadPlan {
    version_id: String,
    #[serde(default)]
    cursor: Option<String>,
}

#[async_trait]
impl Answers for PlanDesk {
    async fn answer(&self, call: Call) -> Answer {
        self.dispatch(&call).unwrap_or_else(Answer::Refused)
    }
}
