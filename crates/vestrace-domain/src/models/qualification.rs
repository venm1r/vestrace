use std::fmt;

macro_rules! qualification_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Eq,
            Hash,
            PartialEq,
            schemars::JsonSchema,
            serde::Deserialize,
            serde::Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }
            pub const fn from_uuid(value: uuid::Uuid) -> Self {
                Self(value)
            }
            pub const fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

qualification_id!(QualificationJobId);
qualification_id!(ConnectionQualificationRevisionId);
qualification_id!(ModelQualificationRevisionId);

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum QualificationJobState {
    Requested,
    Running,
    Succeeded,
    FailedDefinite,
    InconclusiveUnknown,
    Cancelled,
}

impl QualificationJobState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::FailedDefinite | Self::InconclusiveUnknown | Self::Cancelled
        )
    }
    pub const fn can_publish_qualification(self) -> bool {
        matches!(self, Self::Succeeded)
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum QualificationProbeResult {
    Pass,
    UnsupportedDefinite,
    FailedDefinite,
    InconclusiveUnknown,
    SkippedPrerequisite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualificationJobStatus(QualificationJobState);
impl QualificationJobStatus {
    pub const fn new(state: QualificationJobState) -> Self {
        Self(state)
    }
    pub const fn is_terminal(self) -> bool {
        self.0.is_terminal()
    }
    pub const fn can_publish_qualification(self) -> bool {
        self.0.can_publish_qualification()
    }
}

#[cfg(test)]
mod tests {
    use crate::{QualificationJobState, QualificationJobStatus};

    #[test]
    fn terminal_unknown_cannot_publish_qualification() {
        let status = QualificationJobStatus::new(QualificationJobState::InconclusiveUnknown);

        assert!(status.is_terminal());
        assert!(!status.can_publish_qualification());
    }
}
