use crate::DomainError;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Confidence(f32);

impl Confidence {
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidArgument(
                "confidence score must be between 0.0 and 1.0".into(),
            ))
        }
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Importance(f32);

impl Importance {
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidArgument(
                "importance score must be between 0.0 and 1.0".into(),
            ))
        }
    }

    pub const fn value(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Preference,
    Constraint,
    Decision,
    Task,
    Procedure,
    Observation,
    Outcome,
    Summary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Candidate,
    Active,
    Superseded,
    Rejected,
    Expired,
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_bounds() {
        assert!(Confidence::new(0.0).is_ok());
        assert!(Confidence::new(1.0).is_ok());
        assert!(Confidence::new(0.75).is_ok());
        assert!(Confidence::new(-0.01).is_err());
        assert!(Confidence::new(1.01).is_err());
        assert!(Confidence::new(f32::NAN).is_err());
    }

    #[test]
    fn importance_bounds() {
        assert!(Importance::new(0.0).is_ok());
        assert!(Importance::new(1.0).is_ok());
        assert!(Importance::new(-0.1).is_err());
        assert!(Importance::new(1.1).is_err());
    }
}
