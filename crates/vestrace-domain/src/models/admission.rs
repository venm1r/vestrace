use std::fmt;

use crate::{ConnectionId, DomainError, WorkspaceId};

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
pub struct ProviderAdmissionLeaseId(uuid::Uuid);
impl ProviderAdmissionLeaseId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
}
impl Default for ProviderAdmissionLeaseId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for ProviderAdmissionLeaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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
pub struct ConnectionAdmissionPolicyId(uuid::Uuid);
impl ConnectionAdmissionPolicyId {
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
impl Default for ConnectionAdmissionPolicyId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for ConnectionAdmissionPolicyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
pub struct ConnectionAdmissionLimits {
    max_in_flight: u8,
    requests_per_60_seconds: u32,
    queue_wait_timeout_seconds: u16,
    provider_throttle_cap_seconds: u16,
}
impl ConnectionAdmissionLimits {
    pub fn new(
        max_in_flight: u8,
        requests_per_60_seconds: u32,
        queue_wait_timeout_seconds: u16,
        provider_throttle_cap_seconds: u16,
    ) -> Result<Self, DomainError> {
        if !(1..=64).contains(&max_in_flight)
            || !(1..=60_000).contains(&requests_per_60_seconds)
            || !(1..=300).contains(&queue_wait_timeout_seconds)
            || !(1..=900).contains(&provider_throttle_cap_seconds)
        {
            return Err(DomainError::InvalidArgument(
                "connection admission policy is outside the v1 bounds".into(),
            ));
        }
        Ok(Self {
            max_in_flight,
            requests_per_60_seconds,
            queue_wait_timeout_seconds,
            provider_throttle_cap_seconds,
        })
    }
    pub const fn max_in_flight(self) -> u8 {
        self.max_in_flight
    }
    pub const fn requests_per_60_seconds(self) -> u32 {
        self.requests_per_60_seconds
    }
    pub const fn queue_wait_timeout_seconds(self) -> u16 {
        self.queue_wait_timeout_seconds
    }
    pub const fn provider_throttle_cap_seconds(self) -> u16 {
        self.provider_throttle_cap_seconds
    }
}

/// ```compile_fail
/// let _ = serde_json::from_str::<vestrace_domain::ConnectionAdmissionPolicy>("{}");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct ConnectionAdmissionPolicy {
    id: ConnectionAdmissionPolicyId,
    workspace_id: WorkspaceId,
    connection_id: ConnectionId,
    version: u64,
    #[schemars(flatten)]
    #[serde(flatten)]
    limits: ConnectionAdmissionLimits,
}
impl ConnectionAdmissionPolicy {
    pub const DEFAULT_MAX_IN_FLIGHT: u8 = 4;
    pub const DEFAULT_REQUESTS_PER_60_SECONDS: u32 = 60;
    pub const DEFAULT_QUEUE_WAIT_TIMEOUT_SECONDS: u16 = 30;
    pub const DEFAULT_PROVIDER_THROTTLE_CAP_SECONDS: u16 = 900;

    pub fn new(
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        version: u64,
        limits: ConnectionAdmissionLimits,
    ) -> Result<Self, DomainError> {
        Self::build(
            ConnectionAdmissionPolicyId::new(),
            workspace_id,
            connection_id,
            version,
            limits,
        )
    }
    pub fn from_persisted(
        id: ConnectionAdmissionPolicyId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        version: u64,
        limits: ConnectionAdmissionLimits,
    ) -> Result<Self, DomainError> {
        Self::build(id, workspace_id, connection_id, version, limits)
    }
    fn build(
        id: ConnectionAdmissionPolicyId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        version: u64,
        limits: ConnectionAdmissionLimits,
    ) -> Result<Self, DomainError> {
        if version == 0 {
            return Err(DomainError::InvalidArgument(
                "connection admission policy is outside the v1 bounds".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            connection_id,
            version,
            limits,
        })
    }
    pub fn default_for(workspace_id: WorkspaceId, connection_id: ConnectionId) -> Self {
        Self::new(
            workspace_id,
            connection_id,
            1,
            ConnectionAdmissionLimits::new(
                Self::DEFAULT_MAX_IN_FLIGHT,
                Self::DEFAULT_REQUESTS_PER_60_SECONDS,
                Self::DEFAULT_QUEUE_WAIT_TIMEOUT_SECONDS,
                Self::DEFAULT_PROVIDER_THROTTLE_CAP_SECONDS,
            )
            .expect("v1 admission defaults are valid"),
        )
        .expect("v1 admission defaults are valid")
    }
    pub const fn max_in_flight(&self) -> u8 {
        self.limits.max_in_flight()
    }
    pub const fn requests_per_60_seconds(&self) -> u32 {
        self.limits.requests_per_60_seconds()
    }
    pub const fn queue_wait_timeout_seconds(&self) -> u16 {
        self.limits.queue_wait_timeout_seconds()
    }
    pub const fn provider_throttle_cap_seconds(&self) -> u16 {
        self.limits.provider_throttle_cap_seconds()
    }

    pub const fn limits(&self) -> ConnectionAdmissionLimits {
        self.limits
    }

    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn id(&self) -> ConnectionAdmissionPolicyId {
        self.id
    }
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RollingWindowDecision {
    Admitted { lease_id: ProviderAdmissionLeaseId },
    Throttled { retry_after_seconds: u32 },
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAdmissionError {
    Throttle { retry_after_seconds: u32 },
    Conflict,
}

#[cfg(test)]
mod tests {
    use super::ConnectionAdmissionLimits;
    use crate::{ConnectionAdmissionPolicy, ConnectionId, WorkspaceId};

    #[test]
    fn admission_policy_requires_a_positive_rolling_limit() {
        let result = ConnectionAdmissionLimits::new(0, 1, 30, 900);

        assert!(result.is_err());
    }

    #[test]
    fn admission_policy_uses_the_exact_v1_fields_bounds_and_defaults() {
        let policy =
            ConnectionAdmissionPolicy::default_for(WorkspaceId::new(), ConnectionId::new());

        assert_eq!(policy.max_in_flight(), 4);
        assert_eq!(policy.requests_per_60_seconds(), 60);
        assert_eq!(policy.queue_wait_timeout_seconds(), 30);
        assert_eq!(policy.provider_throttle_cap_seconds(), 900);
        assert!(ConnectionAdmissionLimits::new(65, 60, 30, 900).is_err());
    }

    #[test]
    fn persisted_policy_preserves_identity_and_connection_scope() {
        let id = crate::ConnectionAdmissionPolicyId::from_uuid(uuid::Uuid::now_v7());
        let workspace_id = WorkspaceId::new();
        let connection_id = ConnectionId::new();
        let policy = ConnectionAdmissionPolicy::from_persisted(
            id,
            workspace_id,
            connection_id,
            7,
            ConnectionAdmissionLimits::new(4, 60, 30, 900)
                .expect("persisted admission limits remain valid"),
        )
        .expect("persisted policy remains valid");

        assert_eq!(policy.id(), id);
        assert_eq!(policy.workspace_id(), workspace_id);
        assert_eq!(policy.connection_id(), connection_id);
    }
}
