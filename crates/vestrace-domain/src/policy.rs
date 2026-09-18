use crate::{Confidence, MemoryKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryWritePolicy {
    Manual,
    Assisted,
    Automatic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationDecision {
    AutoActivate,
    RequireApproval,
    DiscardCandidate,
}

impl MemoryWritePolicy {
    pub fn evaluate(self, kind: MemoryKind, confidence: Confidence) -> ActivationDecision {
        match self {
            Self::Manual => ActivationDecision::RequireApproval,
            Self::Automatic => {
                if confidence.value() >= 0.7 {
                    ActivationDecision::AutoActivate
                } else if confidence.value() >= 0.4 {
                    ActivationDecision::RequireApproval
                } else {
                    ActivationDecision::DiscardCandidate
                }
            }
            Self::Assisted => {
                if matches!(kind, MemoryKind::Fact | MemoryKind::Preference)
                    && confidence.value() >= 0.85
                {
                    ActivationDecision::AutoActivate
                } else {
                    ActivationDecision::RequireApproval
                }
            }
        }
    }
}
