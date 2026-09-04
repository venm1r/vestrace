use crate::WorkspaceId;

/// Why a space key exists at all.
///
/// Before this package, a space was created by `ensure_space(context, name,
/// model, dimensions)` at the moment something first needed one, so a space
/// could appear as a side effect of its own first write. There was no point at
/// which anyone authorized it, and nothing recorded which model a stored vector
/// had actually come from beyond a name.
///
/// The key is the whole tuple. Two spaces that differ only in their model are
/// different spaces, and a caller holding only a display name holds nothing.
/// It deliberately derives neither `serde::Deserialize` nor `From<&str>`: no
/// HTTP body, configuration file, or protocol payload may mint one.
///
/// ```compile_fail
/// fn requires_deserialize<T: serde::de::DeserializeOwned>() {}
/// requires_deserialize::<vestrace_domain::embedding::EmbeddingSpaceKey>();
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EmbeddingSpaceKey {
    workspace_id: WorkspaceId,
    name: String,
    model: String,
    dimensions: u32,
}

/// The refusals a malformed space key produces.
///
/// Each names the missing component rather than reporting "invalid", because a
/// caller that cannot tell which half of the tuple it failed to supply will
/// guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddingSpaceKeyError {
    NameIsBlank,
    ModelIsBlank,
    DimensionsAreZero,
}

impl std::fmt::Display for EmbeddingSpaceKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NameIsBlank => "an embedding space key requires a non-blank space name",
            Self::ModelIsBlank => "an embedding space key requires the exact model it was built by",
            Self::DimensionsAreZero => "an embedding space key requires a positive dimension count",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for EmbeddingSpaceKeyError {}

impl EmbeddingSpaceKey {
    pub fn new(
        workspace_id: WorkspaceId,
        name: impl Into<String>,
        model: impl Into<String>,
        dimensions: u32,
    ) -> Result<Self, EmbeddingSpaceKeyError> {
        let name = name.into();
        let model = model.into();
        if name.trim().is_empty() {
            return Err(EmbeddingSpaceKeyError::NameIsBlank);
        }
        if model.trim().is_empty() {
            return Err(EmbeddingSpaceKeyError::ModelIsBlank);
        }
        if dimensions == 0 {
            return Err(EmbeddingSpaceKeyError::DimensionsAreZero);
        }
        Ok(Self {
            workspace_id,
            name,
            model,
            dimensions,
        })
    }

    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub const fn dimensions(&self) -> u32 {
        self.dimensions
    }
}
