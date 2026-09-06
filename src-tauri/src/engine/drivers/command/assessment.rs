//! WF-18: tylko własny stdout zaufanego egzaminatora jest protokołem wyniku.

use serde::{Deserialize, Serialize};

pub const MAX_BYTES: usize = 64 * 1024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofMode {
    #[default]
    OutputPattern,
    ExternalAssessmentV1,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Completed,
    SubjectCannotLoad,
    InfrastructureFailed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Receipt {
    pub format: u32,
    pub status: Status,
    pub passed: u32,
    pub failed: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Passed,
    DidNotPass,
    NotJudged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assessment {
    pub outcome: Outcome,
    pub receipt: Option<Receipt>,
    pub reason: String,
}

/// Nie szukamy JSON w dowolnym tekście: dodatkowy komunikat/odpowiedź unieważnia protokół.
#[must_use]
pub fn assess(bytes: &[u8], exit: Option<i32>, read_failed: bool) -> Assessment {
    let receipt = if bytes.len() <= MAX_BYTES && !read_failed {
        serde_json::from_slice::<Receipt>(bytes)
            .ok()
            .filter(|one| one.format == 1)
    } else {
        None
    };
    let Some(one) = receipt else {
        return Assessment { outcome:Outcome::NotJudged, receipt:None,
            reason:"The independent examiner did not return one complete, valid result within 64 KiB. This was not measured.".to_owned() };
    };
    let outcome = if exit == Some(0) {
        match one.status {
            Status::Completed if one.failed > 0 => Outcome::DidNotPass,
            Status::Completed if one.passed > 0 => Outcome::Passed,
            Status::SubjectCannotLoad => Outcome::DidNotPass,
            Status::Completed | Status::InfrastructureFailed | Status::Unknown => {
                Outcome::NotJudged
            }
        }
    } else {
        Outcome::NotJudged
    };
    let reason = if exit != Some(0) {
        "The independent examiner did not finish normally. This was not measured.".to_owned()
    } else if !one.reason.trim().is_empty() {
        one.reason.clone()
    } else {
        match outcome {
            Outcome::Passed => format!("Independent checks passed ({} passed).", one.passed),
            Outcome::DidNotPass => "The independent checks found that the result did not meet the case.".to_owned(),
            Outcome::NotJudged => "The independent examiner did not report any completed assertions. This was not measured.".to_owned(),
        }
    };
    Assessment {
        outcome,
        receipt: Some(one),
        reason,
    }
}
