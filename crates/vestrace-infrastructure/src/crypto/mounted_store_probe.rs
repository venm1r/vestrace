//! Crypto qualification evidence, collected by doing rather than by claiming.
//!
//! Each check below is recorded only when its attempt produced the outcome the
//! check is about. A probe that reports six checks unconditionally is a probe
//! that checks nothing, so every recording here is downstream of an operation
//! that could have gone the other way.

use ring::signature::{ED25519, Ed25519KeyPair, UnparsedPublicKey};
use vestrace_application::{
    ApplicationError, CryptoAdapterQualificationEvidence, CryptoAdapterQualificationProbe,
    CryptoAdapterQualificationTarget, CryptoQualificationCheck,
};
use vestrace_domain::WorkspaceId;
use vestrace_domain::trust::{
    KeyProvider, KeyProviderError, KeyReference, SecretResolutionRequest,
};

use super::mounted_secret_store::{MOUNTED_SECRET_STORE_PROVIDER, MountedSecretStoreKeyProvider};

const PROBE_PAYLOAD: &[u8] = b"vestrace crypto qualification probe";

/// Whether `rendered` contains `secret` as a contiguous byte run.
///
/// Pulled out of the probe so it can be pinned directly: every fixture the
/// shipped suite exercises has the adapter refuse cleanly, so nothing in that
/// suite can tell this scan apart from one that always answers "clean" unless
/// the scan itself is tested against a string built to contain the secret.
pub fn discloses(rendered: &str, secret: &[u8]) -> bool {
    rendered
        .as_bytes()
        .windows(secret.len().max(1))
        .any(|window| window == secret)
}

pub struct MountedStoreCryptoProbe {
    provider: MountedSecretStoreKeyProvider,
}

impl MountedStoreCryptoProbe {
    pub fn new(provider: MountedSecretStoreKeyProvider) -> Self {
        Self { provider }
    }

    fn request(purpose: &str) -> SecretResolutionRequest {
        SecretResolutionRequest::new(WorkspaceId::new(), purpose, "conformance://crypto-qualify")
    }
}

impl CryptoAdapterQualificationProbe for MountedStoreCryptoProbe {
    fn collect(
        &self,
        target: &CryptoAdapterQualificationTarget,
    ) -> Result<CryptoAdapterQualificationEvidence, ApplicationError> {
        let declaration = self
            .provider
            .declaration(target.key_id())
            .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;

        // The observed key describes what the store says, not what the target
        // asked for. The qualification service compares the two, which it can
        // only do if they are gathered from different places.
        let observed = KeyReference::new(
            MOUNTED_SECRET_STORE_PROVIDER,
            target.key_id(),
            target.key_version(),
            target.purpose(),
            declaration.scope.clone(),
            declaration.algorithm.clone(),
        )
        .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;

        let mut checks = Vec::new();
        let mut refs = Vec::new();

        let active = self
            .provider
            .resolve(&observed, &Self::request(&declaration.scope));
        let (material, resolved_debug) = match active {
            Ok(material) => {
                checks.push(CryptoQualificationCheck::Resolution);
                refs.push(format!(
                    "evidence:mounted-store/resolution/{}/{}",
                    target.key_id(),
                    target.key_version()
                ));
                // The only rendering that can actually hold the material: the
                // Ok value of a successful resolution, taken before it is
                // reduced to raw bytes below. Every other rendering the
                // non-disclosure scan gathers is of a refusal, whose text
                // structurally cannot contain key bytes.
                let debug = format!("{material:?}");
                (Some(material.into_bytes()), Some(debug))
            }
            Err(_) => (None, None),
        };

        // Scope isolation: a resolution for something the store did not declare
        // must be refused. Attempted, not assumed.
        let foreign_scope = format!("{}-not-this", declaration.scope);
        let foreign_scope_result = self
            .provider
            .resolve(&observed, &Self::request(&foreign_scope));
        if matches!(foreign_scope_result, Err(KeyProviderError::Denied(_))) {
            checks.push(CryptoQualificationCheck::ScopeIsolation);
            refs.push(format!(
                "evidence:mounted-store/scope-denial/{}",
                target.key_id()
            ));
        }

        if let Some(bytes) = &material {
            if let Ok(pair) = Ed25519KeyPair::from_pkcs8(bytes) {
                let signature = pair.sign(PROBE_PAYLOAD);
                if let Ok(public) = self
                    .provider
                    .public_key(target.key_id(), target.key_version())
                {
                    if UnparsedPublicKey::new(&ED25519, public)
                        .verify(PROBE_PAYLOAD, signature.as_ref())
                        .is_ok()
                    {
                        checks.push(CryptoQualificationCheck::CryptographicRoundTrip);
                        refs.push(format!(
                            "evidence:mounted-store/sign-verify/{}/{}",
                            target.key_id(),
                            target.key_version()
                        ));
                    }
                }
            }
        }

        let versions = self
            .provider
            .versions(target.key_id())
            .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;

        // Lifecycle: a version the store has revoked is refused. Built once
        // and kept, since the non-disclosure scan below reuses the same
        // refusal rather than provoking a different one.
        let revoked_ref = match versions.iter().find(|(_, state)| state == "revoked") {
            Some((version, _)) => Some(
                KeyReference::new(
                    MOUNTED_SECRET_STORE_PROVIDER,
                    target.key_id(),
                    version,
                    target.purpose(),
                    declaration.scope.clone(),
                    declaration.algorithm.clone(),
                )
                .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?,
            ),
            None => None,
        };
        if let Some(revoked) = &revoked_ref {
            if matches!(
                self.provider
                    .resolve(revoked, &Self::request(&declaration.scope)),
                Err(KeyProviderError::NotUsable)
            ) {
                checks.push(CryptoQualificationCheck::Lifecycle);
                refs.push(format!(
                    "evidence:mounted-store/lifecycle/{}/{}",
                    target.key_id(),
                    revoked.version()
                ));
            }
        }

        // Rotation: a version other than the active one is marked retired or
        // revoked by the store, and its public key differs from the active
        // version's. The key comparison is the one that matters — the same
        // material under two version names would satisfy a check that only
        // looked at the state label.
        let active_public = self
            .provider
            .public_key(target.key_id(), target.key_version())
            .ok();
        if let Some(active_public) = active_public {
            let rotated = versions.iter().find(|(version, state)| {
                version != target.key_version() && (state == "retired" || state == "revoked")
            });
            if let Some((version, _)) = rotated {
                let superseded = self.provider.public_key(target.key_id(), version).ok();
                if superseded.is_some_and(|public| public != active_public) {
                    checks.push(CryptoQualificationCheck::Rotation);
                    refs.push(format!(
                        "evidence:mounted-store/rotation/{}/{}→{}",
                        target.key_id(),
                        version,
                        target.key_version()
                    ));
                }
            }
        }

        // Non-disclosure is a property of formatting code, which no type
        // prevents from regressing, so it is searched for rather than
        // asserted — and searched for across several renderings, not one.
        // The foreign-scope refusal used below interpolates only
        // `request.purpose()` and `declaration.scope`, never key material, so
        // by itself it carries no information about whether material leaks:
        // an adapter that dumped bytes into a `NotUsable` or `Unavailable`
        // error, or that reverted `ResolvedKeyMaterial`'s redacted `Debug`,
        // would still pass a scan of that one rendering alone. The Ok value
        // of the successful resolution is the one rendering that can
        // actually hold the material, so it is included unconditionally.
        if let Some(bytes) = &material {
            let mut rendered = String::new();
            if let Err(error) = &foreign_scope_result {
                rendered.push_str(&format!("{error} {error:?} "));
            }
            let unknown_version = KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                target.key_id(),
                format!("{}-does-not-exist", target.key_version()),
                target.purpose(),
                declaration.scope.clone(),
                declaration.algorithm.clone(),
            )
            .map_err(|error| ApplicationError::InvalidConfiguration(format!("{error}")))?;
            if let Err(error) = self
                .provider
                .resolve(&unknown_version, &Self::request(&declaration.scope))
            {
                rendered.push_str(&format!("{error} {error:?} "));
            }
            if let Some(revoked) = &revoked_ref {
                if let Err(error) = self
                    .provider
                    .resolve(revoked, &Self::request(&declaration.scope))
                {
                    rendered.push_str(&format!("{error} {error:?} "));
                }
            }
            if let Some(debug) = &resolved_debug {
                rendered.push_str(debug);
            }
            let leaked = discloses(&rendered, bytes);
            if !rendered.is_empty() && !leaked {
                checks.push(CryptoQualificationCheck::SecretNonDisclosure);
                refs.push(format!(
                    "evidence:mounted-store/non-disclosure/{}",
                    target.key_id()
                ));
            }
        }

        Ok(CryptoAdapterQualificationEvidence::new(
            observed, checks, refs,
        ))
    }
}
