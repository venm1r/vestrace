//! Authentication credentials that name a real principal.
//!
//! # What this replaces
//!
//! One shared bearer token compared against a value in the process environment,
//! mapped to one hardcoded `(workspace, principal)` pair. Every authenticated
//! request was therefore the same administrator: audit rows, run events and
//! approvals all recorded an identity that told you nothing about who acted.
//! Attribution did not exist, and no capability, delegation or governance
//! requirement can be satisfied without it.
//!
//! # Why a hash and not a password KDF
//!
//! A token here is 256 bits from a CSPRNG, not a phrase a person chose. Argon2
//! and bcrypt exist to make *guessable* secrets expensive to guess; against a
//! uniformly random 256-bit value there is nothing to guess, so a slow KDF buys
//! no security and costs latency on every single request. It would also make
//! the lookup impossible: a per-row salt means the database cannot find the row
//! from the presented value without scanning and verifying every credential in
//! the table.
//!
//! SHA-256 is used for exactly the property needed — a one-way, collision
//! resistant, *deterministic* index into the credential table. If the token
//! format ever changes to something a human picks, this reasoning stops holding
//! and the choice must change with it.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::{AccessTokenId, DomainError, PrincipalId, Timestamp, WorkspaceId};

/// The prefix every Vestrace credential carries.
///
/// Present so a leaked string is recognisable as a credential in a log, a paste
/// or a scanning tool, and so a caller that sends the wrong kind of secret gets
/// a clear refusal rather than a generic 401.
pub const TOKEN_PREFIX: &str = "vst_";

/// How many random bytes back a token.
const TOKEN_ENTROPY_BYTES: usize = 32;

/// A freshly minted credential, in the only moment it exists in plaintext.
///
/// Deliberately lacks `Debug`, `Display`, `Clone` and `Serialize`: the one thing
/// that must not happen to this value is being copied somewhere by accident. It
/// zeroizes on drop. The caller has exactly one chance to hand it to whoever
/// asked; there is no path that reads it back afterwards, by construction —
/// only the hash is stored.
pub struct IssuedToken {
    token: String,
    record: AccessToken,
}

impl IssuedToken {
    /// The plaintext credential. Consumes the value: there is no second read.
    pub fn into_token(mut self) -> String {
        std::mem::take(&mut self.token)
    }

    pub fn record(&self) -> &AccessToken {
        &self.record
    }
}

impl Drop for IssuedToken {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

/// The stored half of a credential. Contains no secret.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccessToken {
    pub id: AccessTokenId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    /// SHA-256 of the presented token, lowercase hex.
    pub token_hash: String,
    /// What an operator revokes by. Never a secret.
    pub label: String,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
    pub last_used_at: Option<Timestamp>,
}

impl AccessToken {
    /// Mint a credential from caller-supplied entropy.
    ///
    /// The randomness is a parameter rather than drawn here so the domain stays
    /// free of an RNG dependency and so a test can pin the value. Callers in
    /// production must pass bytes from a CSPRNG; anything else produces a
    /// guessable credential, which is why the length is enforced.
    pub fn issue(
        id: AccessTokenId,
        workspace_id: WorkspaceId,
        principal_id: PrincipalId,
        label: impl Into<String>,
        entropy: &[u8],
        expires_at: Option<Timestamp>,
        at: Timestamp,
    ) -> Result<IssuedToken, DomainError> {
        if entropy.len() != TOKEN_ENTROPY_BYTES {
            return Err(DomainError::InvalidArgument(format!(
                "an access token must be backed by {TOKEN_ENTROPY_BYTES} bytes of entropy"
            )));
        }

        let label = label.into();
        if label.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "an access token must carry a label so it can be identified and revoked".into(),
            ));
        }

        // An already-expired credential is a configuration mistake, not a
        // credential: minting one would hand the caller a value that never
        // works and report success.
        if matches!(expires_at, Some(expiry) if expiry <= at) {
            return Err(DomainError::InvalidArgument(
                "an access token cannot expire at or before the moment it is issued".into(),
            ));
        }

        let token = format!("{TOKEN_PREFIX}{}", hex_encode(entropy));
        let token_hash = hash_token(&token);

        Ok(IssuedToken {
            record: AccessToken {
                id,
                workspace_id,
                principal_id,
                token_hash,
                label: label.trim().to_string(),
                created_at: at,
                expires_at,
                revoked_at: None,
                last_used_at: None,
            },
            token,
        })
    }

    /// Whether this credential may authenticate a request at `at`.
    ///
    /// The database enforces the same two conditions inside
    /// `vestrace_resolve_access_token`, on purpose: a caller that forgets to
    /// check here still cannot authenticate with a dead credential, and a
    /// caller reading a row directly still gets the right answer. Neither layer
    /// is load-bearing alone.
    pub fn is_live_at(&self, at: Timestamp) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        match self.expires_at {
            Some(expiry) => expiry > at,
            None => true,
        }
    }

    /// Withdraw the credential without destroying the record that it existed.
    ///
    /// Deleting the row would remove the evidence an investigation needs — when
    /// it was created, by what label, and when it was last used.
    pub fn revoke(mut self, at: Timestamp) -> Result<Self, DomainError> {
        if let Some(already) = self.revoked_at {
            return Err(DomainError::PolicyViolation(format!(
                "access token was already revoked at {already}"
            )));
        }
        self.revoked_at = Some(at);
        Ok(self)
    }
}

/// Hash a presented credential into its stored form.
///
/// Rejects anything without the Vestrace prefix before hashing, so a caller
/// that sends a provider key or a session cookie by mistake is refused as a
/// malformed credential rather than silently hashed into a value that will
/// never match.
pub fn hash_presented_token(presented: &str) -> Result<String, DomainError> {
    if !presented.starts_with(TOKEN_PREFIX) {
        return Err(DomainError::InvalidArgument(
            "an access token must begin with the Vestrace credential prefix".into(),
        ));
    }
    if presented.len() != TOKEN_PREFIX.len() + TOKEN_ENTROPY_BYTES * 2 {
        return Err(DomainError::InvalidArgument(
            "an access token has an unexpected length".into(),
        ));
    }
    Ok(hash_token(presented))
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    // A fixed two-character encoding per byte; `format!("{:x}")` on the whole
    // digest would drop leading zeros and produce a hash the database's
    // `^[0-9a-f]{64}$` constraint rejects only sometimes.
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::now;

    fn entropy() -> Vec<u8> {
        (0..TOKEN_ENTROPY_BYTES).map(|i| i as u8).collect()
    }

    fn issue(expires_at: Option<Timestamp>, at: Timestamp) -> Result<IssuedToken, DomainError> {
        AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "console",
            &entropy(),
            expires_at,
            at,
        )
    }

    #[test]
    fn the_stored_record_never_contains_the_token() {
        let at = now();
        let issued = issue(None, at).unwrap();
        let record = issued.record().clone();
        let token = issued.into_token();

        let rendered = format!("{record:?}");
        assert!(
            !rendered.contains(&token),
            "the stored record rendered the credential"
        );
        // The hash is not a credential: it does not carry the prefix, so it
        // cannot be presented as one.
        assert!(!record.token_hash.starts_with(TOKEN_PREFIX));
        assert_eq!(record.token_hash.len(), 64);
    }

    #[test]
    fn presenting_the_token_reproduces_the_stored_hash() {
        let at = now();
        let issued = issue(None, at).unwrap();
        let expected = issued.record().token_hash.clone();
        let token = issued.into_token();

        assert_eq!(hash_presented_token(&token).unwrap(), expected);
    }

    #[test]
    fn a_hash_is_lowercase_hex_of_the_full_digest() {
        // The database constrains this column to `^[0-9a-f]{64}$`; a digest
        // rendered with a formatter that drops leading zeros would violate it
        // for roughly one token in 256.
        let at = now();
        let issued = issue(None, at).unwrap();
        let hash = issued.record().token_hash.clone();
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }

    #[test]
    fn a_credential_without_the_prefix_is_refused_before_hashing() {
        // A provider key or a session cookie sent by mistake must be a clear
        // refusal, not a hash that silently never matches.
        assert!(hash_presented_token("sk-not-a-vestrace-token").is_err());
        assert!(hash_presented_token("").is_err());
    }

    #[test]
    fn a_truncated_token_is_refused() {
        let at = now();
        let issued = issue(None, at).unwrap();
        let token = issued.into_token();
        let truncated = &token[..token.len() - 1];
        assert!(hash_presented_token(truncated).is_err());
    }

    #[test]
    fn weak_entropy_is_refused() {
        let at = now();
        let result = AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "console",
            b"too short",
            None,
            at,
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_blank_label_is_refused() {
        let at = now();
        let result = AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "   ",
            &entropy(),
            None,
            at,
        );
        // A credential nobody can name is a credential nobody will revoke.
        assert!(result.is_err());
    }

    #[test]
    fn a_token_cannot_be_issued_already_expired() {
        let at = now();
        let result = issue(Some(at - chrono::Duration::seconds(1)), at);
        assert!(result.is_err());
        assert!(issue(Some(at), at).is_err(), "expiry equal to issue time");
        assert!(issue(Some(at + chrono::Duration::hours(1)), at).is_ok());
    }

    #[test]
    fn expiry_and_revocation_both_end_a_credential() {
        let at = now();
        let live = issue(None, at).unwrap().record().clone();
        assert!(live.is_live_at(at));

        let revoked = live.clone().revoke(at).unwrap();
        assert!(!revoked.is_live_at(at));
        // Revocation keeps the record rather than erasing it.
        assert_eq!(revoked.id, live.id);
        assert_eq!(revoked.label, live.label);

        let expiring = issue(Some(at + chrono::Duration::hours(1)), at)
            .unwrap()
            .record()
            .clone();
        assert!(expiring.is_live_at(at));
        assert!(!expiring.is_live_at(at + chrono::Duration::hours(2)));
    }

    #[test]
    fn revoking_twice_is_refused_rather_than_moving_the_timestamp() {
        // Overwriting would lose when the credential actually stopped working.
        let at = now();
        let revoked = issue(None, at)
            .unwrap()
            .record()
            .clone()
            .revoke(at)
            .unwrap();
        assert!(revoked.revoke(at + chrono::Duration::hours(1)).is_err());
    }

    #[test]
    fn two_tokens_from_different_entropy_do_not_collide() {
        let at = now();
        let first = AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "a",
            &[7u8; TOKEN_ENTROPY_BYTES],
            None,
            at,
        )
        .unwrap();
        let second = AccessToken::issue(
            AccessTokenId::new(),
            WorkspaceId::new(),
            PrincipalId::new(),
            "b",
            &[9u8; TOKEN_ENTROPY_BYTES],
            None,
            at,
        )
        .unwrap();
        assert_ne!(first.record().token_hash, second.record().token_hash);
    }
}
