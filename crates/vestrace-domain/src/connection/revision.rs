use std::fmt;

use crate::{ConnectionId, CredentialSlotId, DomainError, Timestamp, WorkspaceId};

macro_rules! revision_id {
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
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

revision_id!(ConnectionRevisionId);
revision_id!(NoAuthBindingRevisionId);

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    LMStudioLocal,
    OpenAiChatCompletionsV1,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAuthMode {
    None,
    Bearer,
    ApiKey,
    XApiKey,
}

#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
#[serde(transparent)]
pub struct NormalizedBaseUrl(String);

impl NormalizedBaseUrl {
    pub fn parse(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let is_http = value.starts_with("http://") || value.starts_with("https://");
        if value.trim() != value
            || !is_http
            || value.contains(['?', '#', '@'])
            || value.ends_with('/')
        {
            return Err(DomainError::InvalidArgument("base URL must be normalized HTTP(S), without userinfo, query, fragment, or trailing slash".into()));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(
    Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ConnectionTransportPolicy {
    LoopbackOnly,
    RemoteHttps,
}

impl ConnectionTransportPolicy {
    fn for_runtime_url(url: &NormalizedBaseUrl) -> Result<Self, DomainError> {
        if url
            .as_str()
            .strip_prefix("http://")
            .and_then(runtime_authority_host)
            .is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host.eq_ignore_ascii_case("localhost.")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
            })
        {
            Ok(Self::LoopbackOnly)
        } else if url.as_str().starts_with("https://") {
            Ok(Self::RemoteHttps)
        } else {
            Err(DomainError::InvalidArgument(
                "non-loopback runtime URL requires HTTPS".into(),
            ))
        }
    }
}

fn runtime_authority_host(authority_and_path: &str) -> Option<&str> {
    let authority = authority_and_path.split('/').next()?;
    if let Some(bracketed) = authority.strip_prefix('[') {
        return bracketed.split_once(']').map(|(host, _)| host);
    }
    Some(
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _port)| host),
    )
}

/// ```compile_fail
/// let _ = serde_json::from_str::<vestrace_domain::ConnectionRevision>("{}");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct ConnectionRevision {
    id: ConnectionRevisionId,
    workspace_id: WorkspaceId,
    connection_id: ConnectionId,
    kind: ConnectionKind,
    logical_base_url: NormalizedBaseUrl,
    runtime_base_url: NormalizedBaseUrl,
    adapter_profile_revision: String,
    transport_policy: ConnectionTransportPolicy,
    auth_mode: ConnectionAuthMode,
    credential_slot_id: Option<CredentialSlotId>,
    created_at: Timestamp,
}

impl ConnectionRevision {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        kind: ConnectionKind,
        logical_base_url: impl Into<String>,
        runtime_base_url: impl Into<String>,
        adapter_profile_revision: impl Into<String>,
        auth_mode: ConnectionAuthMode,
        credential_slot_id: Option<CredentialSlotId>,
    ) -> Result<Self, DomainError> {
        let logical_base_url = NormalizedBaseUrl::parse(logical_base_url)?;
        let runtime_base_url = NormalizedBaseUrl::parse(runtime_base_url)?;
        let transport_policy = ConnectionTransportPolicy::for_runtime_url(&runtime_base_url)?;
        Self::build(
            ConnectionRevisionId::new(),
            workspace_id,
            connection_id,
            kind,
            logical_base_url,
            runtime_base_url,
            adapter_profile_revision,
            transport_policy,
            auth_mode,
            credential_slot_id,
            crate::now(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_persisted(
        id: ConnectionRevisionId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        kind: ConnectionKind,
        logical_base_url: impl Into<String>,
        runtime_base_url: impl Into<String>,
        adapter_profile_revision: impl Into<String>,
        auth_mode: ConnectionAuthMode,
        credential_slot_id: Option<CredentialSlotId>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let logical_base_url = NormalizedBaseUrl::parse(logical_base_url)?;
        let runtime_base_url = NormalizedBaseUrl::parse(runtime_base_url)?;
        let transport_policy = ConnectionTransportPolicy::for_runtime_url(&runtime_base_url)?;
        Self::build(
            id,
            workspace_id,
            connection_id,
            kind,
            logical_base_url,
            runtime_base_url,
            adapter_profile_revision,
            transport_policy,
            auth_mode,
            credential_slot_id,
            created_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        id: ConnectionRevisionId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        kind: ConnectionKind,
        logical_base_url: NormalizedBaseUrl,
        runtime_base_url: NormalizedBaseUrl,
        adapter_profile_revision: impl Into<String>,
        transport_policy: ConnectionTransportPolicy,
        auth_mode: ConnectionAuthMode,
        credential_slot_id: Option<CredentialSlotId>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let adapter_profile_revision = adapter_profile_revision.into();
        if adapter_profile_revision.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "adapter profile revision is required".into(),
            ));
        }
        match (auth_mode, credential_slot_id) {
            (ConnectionAuthMode::None, None)
            | (
                ConnectionAuthMode::Bearer
                | ConnectionAuthMode::ApiKey
                | ConnectionAuthMode::XApiKey,
                Some(_),
            ) => Ok(Self {
                id,
                workspace_id,
                connection_id,
                kind,
                logical_base_url,
                runtime_base_url,
                adapter_profile_revision,
                transport_policy,
                auth_mode,
                credential_slot_id,
                created_at,
            }),
            (ConnectionAuthMode::None, Some(_)) => Err(DomainError::InvalidArgument(
                "no-auth connection revision cannot name a credential slot".into(),
            )),
            (_, None) => Err(DomainError::InvalidArgument(
                "credential auth requires an exact credential slot".into(),
            )),
        }
    }

    pub fn new_no_auth(
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        kind: ConnectionKind,
        logical_base_url: impl Into<String>,
        runtime_base_url: impl Into<String>,
        adapter_profile_revision: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Self::new(
            workspace_id,
            connection_id,
            kind,
            logical_base_url,
            runtime_base_url,
            adapter_profile_revision,
            ConnectionAuthMode::None,
            None,
        )
    }

    pub const fn id(&self) -> ConnectionRevisionId {
        self.id
    }
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }
    pub const fn kind(&self) -> ConnectionKind {
        self.kind
    }
    pub fn logical_base_url(&self) -> &NormalizedBaseUrl {
        &self.logical_base_url
    }
    pub fn runtime_base_url(&self) -> &NormalizedBaseUrl {
        &self.runtime_base_url
    }
    pub fn adapter_profile_revision(&self) -> &str {
        &self.adapter_profile_revision
    }
    pub fn transport_policy(&self) -> &ConnectionTransportPolicy {
        &self.transport_policy
    }
    pub const fn auth_mode(&self) -> ConnectionAuthMode {
        self.auth_mode
    }
    pub const fn credential_slot_id(&self) -> Option<CredentialSlotId> {
        self.credential_slot_id
    }
    pub const fn created_at(&self) -> Timestamp {
        self.created_at
    }
}

/// ```compile_fail
/// let _ = serde_json::from_str::<vestrace_domain::NoAuthBindingRevision>("{}");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, schemars::JsonSchema, serde::Serialize)]
pub struct NoAuthBindingRevision {
    id: NoAuthBindingRevisionId,
    workspace_id: WorkspaceId,
    connection_id: ConnectionId,
    connection_revision_id: ConnectionRevisionId,
    auth_mode: ConnectionAuthMode,
}

impl NoAuthBindingRevision {
    pub fn new(
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        connection_revision_id: ConnectionRevisionId,
        auth_mode: ConnectionAuthMode,
    ) -> Result<Self, DomainError> {
        Self::build(
            NoAuthBindingRevisionId::new(),
            workspace_id,
            connection_id,
            connection_revision_id,
            auth_mode,
        )
    }

    pub fn from_persisted(
        id: NoAuthBindingRevisionId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        connection_revision_id: ConnectionRevisionId,
        auth_mode: ConnectionAuthMode,
    ) -> Result<Self, DomainError> {
        Self::build(
            id,
            workspace_id,
            connection_id,
            connection_revision_id,
            auth_mode,
        )
    }

    fn build(
        id: NoAuthBindingRevisionId,
        workspace_id: WorkspaceId,
        connection_id: ConnectionId,
        connection_revision_id: ConnectionRevisionId,
        auth_mode: ConnectionAuthMode,
    ) -> Result<Self, DomainError> {
        if auth_mode != ConnectionAuthMode::None {
            return Err(DomainError::InvalidArgument(
                "no-auth binding requires ConnectionAuthMode::None".into(),
            ));
        }
        Ok(Self {
            id,
            workspace_id,
            connection_id,
            connection_revision_id,
            auth_mode,
        })
    }
    pub const fn id(&self) -> NoAuthBindingRevisionId {
        self.id
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub const fn connection_revision_id(&self) -> ConnectionRevisionId {
        self.connection_revision_id
    }

    pub const fn auth_mode(&self) -> ConnectionAuthMode {
        self.auth_mode
    }
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_not_impl_any;

    use crate::{
        ConnectionAuthMode, ConnectionId, ConnectionKind, ConnectionRevision, CredentialSlotId,
        NoAuthBindingRevision, WorkspaceId,
    };

    #[test]
    fn invariant_bearing_connection_records_cannot_be_deserialized_unchecked() {
        assert_not_impl_any!(NoAuthBindingRevision: From<CredentialSlotId>, AsRef<CredentialSlotId>);
    }

    #[test]
    fn no_auth_revision_has_no_credential_shape() {
        let revision = ConnectionRevision::new_no_auth(
            WorkspaceId::new(),
            ConnectionId::new(),
            ConnectionKind::LMStudioLocal,
            "http://127.0.0.1:1234/v1",
            "http://127.0.0.1:1234/v1",
            "lm-studio-local/v1",
        )
        .expect("no-auth connection revision is valid");

        assert_eq!(revision.auth_mode(), ConnectionAuthMode::None);
        assert!(revision.credential_slot_id().is_none());
    }

    #[test]
    fn credential_auth_requires_an_exact_slot() {
        let result = ConnectionRevision::new(
            WorkspaceId::new(),
            ConnectionId::new(),
            ConnectionKind::OpenAiChatCompletionsV1,
            "https://provider.example/v1",
            "https://provider.example/v1",
            "openai-chat-completions-v1/q1",
            ConnectionAuthMode::Bearer,
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn deceptive_loopback_prefixes_are_not_local_runtime_urls() {
        for runtime_url in [
            "http://localhost.evil:12345/v1",
            "http://127.example:12345/v1",
        ] {
            let result = ConnectionRevision::new_no_auth(
                WorkspaceId::new(),
                ConnectionId::new(),
                ConnectionKind::LMStudioLocal,
                runtime_url,
                runtime_url,
                "lm-studio-local/v1",
            );

            assert!(
                result.is_err(),
                "deceptive hostname {runtime_url:?} was accepted as loopback"
            );
        }
    }

    #[test]
    fn connection_revision_exposes_the_complete_safe_record() {
        let workspace_id = WorkspaceId::new();
        let connection_id = ConnectionId::new();
        let revision = ConnectionRevision::new_no_auth(
            workspace_id,
            connection_id,
            ConnectionKind::LMStudioLocal,
            "http://127.0.0.1:1234/v1",
            "http://127.0.0.1:1234/v1",
            "lm-studio-local/v1",
        )
        .expect("no-auth revision is valid");

        assert_eq!(revision.workspace_id(), workspace_id);
        assert_eq!(revision.connection_id(), connection_id);
        assert_eq!(revision.kind(), ConnectionKind::LMStudioLocal);
        assert_eq!(
            revision.logical_base_url().as_str(),
            "http://127.0.0.1:1234/v1"
        );
        assert_eq!(
            revision.runtime_base_url().as_str(),
            "http://127.0.0.1:1234/v1"
        );
        assert_eq!(revision.adapter_profile_revision(), "lm-studio-local/v1");
    }

    #[test]
    fn persisted_connection_revision_preserves_its_identity_and_timestamp() {
        let id = crate::ConnectionRevisionId::from_uuid(uuid::Uuid::now_v7());
        let created_at = crate::now();
        let revision = ConnectionRevision::from_persisted(
            id,
            WorkspaceId::new(),
            ConnectionId::new(),
            ConnectionKind::LMStudioLocal,
            "http://127.0.0.1:1234/v1",
            "http://127.0.0.1:1234/v1",
            "lm-studio-local/v1",
            ConnectionAuthMode::None,
            None,
            created_at,
        )
        .expect("persisted no-auth revision remains valid");

        assert_eq!(revision.id(), id);
        assert_eq!(revision.created_at(), created_at);
    }
}
