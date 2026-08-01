use std::{fmt, str::FromStr};

macro_rules! domain_id {
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

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

domain_id!(WorkspaceId);
domain_id!(PrincipalId);
domain_id!(OperationId);
domain_id!(RequestId);
domain_id!(CorrelationId);
domain_id!(SessionId);
domain_id!(EventId);
domain_id!(MemoryId);
domain_id!(MemoryRevisionId);
domain_id!(MemorySourceId);
domain_id!(DerivationId);
domain_id!(RelationId);
domain_id!(JobId);
domain_id!(OutboxId);
domain_id!(EmbeddingSpaceId);
domain_id!(RetrievalRunId);
domain_id!(ContextPackId);
