//! Storage and resolution of workspace secrets.
//!
//! The port deals in ciphertext-at-rest and hands back plaintext only through
//! [`SecretMaterial`], which is deliberately awkward to keep around: it does not
//! implement `Debug`, `Display`, `Clone` or `Serialize`, and it zeroes its
//! buffer on drop. A secret that can be formatted is a secret that reaches logs.

use async_trait::async_trait;
use vestrace_domain::trust::{SecretLease, SecretRef};

use crate::{ApplicationError, RequestContext};

/// Plaintext secret material, held for as long as it takes to use it.
///
/// The absence of `Debug`/`Display`/`Serialize` is the point — those are the
/// three ways secrets escape into logs, error bodies and traces.
pub struct SecretMaterial {
    bytes: Vec<u8>,
}

impl SecretMaterial {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// Interpret the material as UTF-8, for secrets that are textual such as an
    /// API key. Errors do not quote the material.
    pub fn expose_str(&self) -> Result<&str, ApplicationError> {
        std::str::from_utf8(&self.bytes)
            .map_err(|_| ApplicationError::Internal("secret material is not valid UTF-8".into()))
    }
}

impl Drop for SecretMaterial {
    fn drop(&mut self) {
        // Overwrite before the allocation returns to the allocator, so the
        // plaintext does not linger in freed memory to be picked up by a later
        // allocation or a core dump. `zeroize` performs the volatile write and
        // the compiler fence; this crate forbids `unsafe`, and hand-rolling it
        // is exactly the kind of code that gets optimised away silently.
        use zeroize::Zeroize as _;
        self.bytes.zeroize();
    }
}

/// A secret as it is described to a caller: never its value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretDescriptor {
    pub id: vestrace_domain::SecretRefId,
    pub name: String,
    pub purpose: String,
    /// The key version the material is currently encrypted under. Reported so
    /// an operator can tell what a rotation has and has not covered yet.
    pub key_version: String,
    pub created_at: vestrace_domain::time::Timestamp,
    pub updated_at: vestrace_domain::time::Timestamp,
}

#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Encrypt and store `material`, replacing any secret with the same name
    /// and purpose in this workspace.
    async fn put(
        &self,
        context: &RequestContext,
        name: &str,
        purpose: &str,
        material: SecretMaterial,
    ) -> Result<SecretRef, ApplicationError>;

    /// Describe the stored secrets. Returns descriptors, never material.
    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<SecretDescriptor>, ApplicationError>;

    /// Find a secret's reference by name and purpose.
    ///
    /// Returns the reference, never material: a caller then has to obtain a
    /// lease from it, which is where the workspace and purpose are checked.
    /// `Ok(None)` means no such secret, which a caller may legitimately treat
    /// as "not configured" rather than as a failure.
    async fn find(
        &self,
        context: &RequestContext,
        name: &str,
        purpose: &str,
    ) -> Result<Option<SecretRef>, ApplicationError>;

    /// Resolve a secret to its plaintext.
    ///
    /// The lease is produced by [`SecretRef::authorize_resolution`], so a caller
    /// cannot reach material without having matched the secret's workspace and
    /// purpose and named an authorization reference.
    async fn resolve(
        &self,
        context: &RequestContext,
        lease: &SecretLease,
    ) -> Result<SecretMaterial, ApplicationError>;

    /// Delete a secret. Idempotent: deleting an absent secret is not an error,
    /// because the caller's intent — that it no longer exist — is satisfied.
    async fn delete(
        &self,
        context: &RequestContext,
        id: vestrace_domain::SecretRefId,
    ) -> Result<(), ApplicationError>;
}

pub type SharedSecretStore = std::sync::Arc<dyn SecretStore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_exposes_exactly_what_it_was_given() {
        let material = SecretMaterial::new(b"sk-provider-key".to_vec());
        assert_eq!(material.expose(), b"sk-provider-key");
        assert_eq!(material.expose_str().unwrap(), "sk-provider-key");
    }

    #[test]
    fn a_non_utf8_secret_error_does_not_quote_the_secret() {
        let material = SecretMaterial::new(vec![0xff, 0xfe, 0xfd]);
        let error = material.expose_str().unwrap_err().to_string();
        assert!(!error.contains("\u{fffd}"));
        assert!(error.contains("not valid UTF-8"));
    }
}
