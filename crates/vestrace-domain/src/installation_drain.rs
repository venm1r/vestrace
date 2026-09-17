//! DrainMutationPermit / Quiescing: an installation-wide request that new
//! MaterialKeyCreationIntent/CredentialKeyCreationIntent work stop, and that
//! every intent already in flight reach a state that can survive
//! indefinitely -- using only the resume/abort logic those intents already
//! have. This module adds no new per-intent transition.

use uuid::Uuid;

use crate::{CredentialKeyCreationIntentState, MaterialKeyCreationIntentState};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct InstallationDrainRequestId(Uuid);

impl InstallationDrainRequestId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for InstallationDrainRequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// `Draining` while intents drained-by this request remain pre-Quiescing;
/// `Frozen` once none do. Reconciliation, not this type, decides which --
/// this type only carries the identity and the two states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallationDrainRequest {
    Draining(InstallationDrainRequestId),
    Frozen(InstallationDrainRequestId),
}

impl InstallationDrainRequest {
    pub const fn request(id: InstallationDrainRequestId) -> Self {
        Self::Draining(id)
    }

    pub const fn id(self) -> InstallationDrainRequestId {
        match self {
            Self::Draining(id) | Self::Frozen(id) => id,
        }
    }

    pub const fn is_frozen(self) -> bool {
        matches!(self, Self::Frozen(_))
    }

    pub const fn complete(self) -> Self {
        Self::Frozen(self.id())
    }
}

/// Exhaustive by construction: a state added to `MaterialKeyCreationIntentState`
/// later fails to compile here rather than being silently treated as
/// not-pre-Quiescing.
pub const fn is_material_pre_quiescing(state: MaterialKeyCreationIntentState) -> bool {
    use MaterialKeyCreationIntentState::*;
    match state {
        Reserved | ProvisionalCreated | ProvisionalReceipted | ContentPrepared | ResultPrepared => {
            true
        }
        ContentAbandonPrepared
        | PrePreparedAbandonPrepared
        | Bound
        | Live
        | ErasurePrepared
        | Tombstoned
        | Abandoned => false,
    }
}

pub const fn is_credential_pre_quiescing(state: CredentialKeyCreationIntentState) -> bool {
    use CredentialKeyCreationIntentState::*;
    match state {
        Reserved | ProvisionalCreated | ProvisionalReceipted | CredentialPrepared => true,
        CredentialAbandonPrepared | Bound | Candidate | ErasurePrepared | Destroyed | Abandoned => {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_request_is_draining() {
        assert!(!InstallationDrainRequest::request(InstallationDrainRequestId::new()).is_frozen());
    }
}
