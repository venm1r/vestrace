use crate::DomainError;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    MemoryRead,
    MemoryWrite,
    MemoryPurge,
    EventRead,
    EventWrite,
    ContextRetrieve,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::MemoryRead => "memory.read",
            Self::MemoryWrite => "memory.write",
            Self::MemoryPurge => "memory.purge",
            Self::EventRead => "event.read",
            Self::EventWrite => "event.write",
            Self::ContextRetrieve => "context.retrieve",
        };
        write!(f, "{}", name)
    }
}

impl FromStr for Capability {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "memory.read" => Ok(Self::MemoryRead),
            "memory.write" => Ok(Self::MemoryWrite),
            "memory.purge" => Ok(Self::MemoryPurge),
            "event.read" => Ok(Self::EventRead),
            "event.write" => Ok(Self::EventWrite),
            "context.retrieve" => Ok(Self::ContextRetrieve),
            _ => Err(DomainError::InvalidArgument(format!("unknown capability {}", s))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}
