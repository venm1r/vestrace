use crate::trust::SignatureRecord;
use crate::{
    conformance::QualificationProfile, error::DomainError, id::ReleaseManifestId, time::Timestamp,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReleaseManifest {
    pub id: ReleaseManifestId,
    pub release_version: String,
    pub components: Vec<String>,
    pub checksum: String,
    pub created_at: Timestamp,
}

/// Machine-readable deployment/build assertion used as the identity anchor for qualification.
///
/// The manifest declares capabilities; it does not prove that the running deployment actually
/// provides them. Runtime qualification and an optional signature remain separate gates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VestraceCapabilityManifest {
    manifest_version: String,
    product: String,
    product_version: String,
    source_revision: String,
    build_digest: String,
    configuration_digest: String,
    environment_manifest: String,
    schema_versions: Vec<String>,
    supported_profiles: Vec<QualificationProfile>,
    optional_features: Vec<String>,
    storage_backends: Vec<String>,
    crypto_providers: Vec<String>,
    model_provider_adapters: Vec<String>,
    external_effect_adapters: Vec<String>,
    federation_capabilities: Vec<String>,
    known_limitations: Vec<String>,
    manifest_digest: String,
    #[serde(default)]
    signature: Option<SignatureRecord>,
}

impl VestraceCapabilityManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manifest_version: impl Into<String>,
        product: impl Into<String>,
        product_version: impl Into<String>,
        source_revision: impl Into<String>,
        build_digest: impl Into<String>,
        configuration_digest: impl Into<String>,
        environment_manifest: impl Into<String>,
        schema_versions: Vec<impl Into<String>>,
        supported_profiles: Vec<QualificationProfile>,
        optional_features: Vec<impl Into<String>>,
        storage_backends: Vec<impl Into<String>>,
        crypto_providers: Vec<impl Into<String>>,
        model_provider_adapters: Vec<impl Into<String>>,
        external_effect_adapters: Vec<impl Into<String>>,
        federation_capabilities: Vec<impl Into<String>>,
        known_limitations: Vec<impl Into<String>>,
    ) -> Result<Self, DomainError> {
        let manifest_version = required_manifest_text("manifest version", manifest_version)?;
        let product = required_manifest_text("product", product)?;
        let product_version = required_manifest_text("product version", product_version)?;
        let source_revision = required_manifest_text("source revision", source_revision)?;
        let build_digest = required_manifest_text("build digest", build_digest)?;
        let configuration_digest =
            required_manifest_text("configuration digest", configuration_digest)?;
        let environment_manifest =
            required_manifest_text("environment manifest", environment_manifest)?;
        let schema_versions = normalized_manifest_values("schema versions", schema_versions, true)?;
        let optional_features =
            normalized_manifest_values("optional features", optional_features, false)?;
        let storage_backends =
            normalized_manifest_values("storage backends", storage_backends, false)?;
        let crypto_providers =
            normalized_manifest_values("crypto providers", crypto_providers, false)?;
        let model_provider_adapters =
            normalized_manifest_values("model provider adapters", model_provider_adapters, false)?;
        let external_effect_adapters = normalized_manifest_values(
            "external effect adapters",
            external_effect_adapters,
            false,
        )?;
        let federation_capabilities =
            normalized_manifest_values("federation capabilities", federation_capabilities, false)?;
        let known_limitations =
            normalized_manifest_values("known limitations", known_limitations, false)?;

        if supported_profiles.is_empty() {
            return Err(DomainError::InvalidArgument(
                "supported profiles requires at least one value".into(),
            ));
        }
        let mut supported_profiles = supported_profiles;
        supported_profiles.sort_by_key(|profile| profile.to_string());
        supported_profiles.dedup();

        let mut manifest = Self {
            manifest_version,
            product,
            product_version,
            source_revision,
            build_digest,
            configuration_digest,
            environment_manifest,
            schema_versions,
            supported_profiles,
            optional_features,
            storage_backends,
            crypto_providers,
            model_provider_adapters,
            external_effect_adapters,
            federation_capabilities,
            known_limitations,
            manifest_digest: String::new(),
            signature: None,
        };
        manifest.manifest_digest = manifest.calculate_digest();
        Ok(manifest)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, DomainError> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|error| {
            DomainError::InvalidArgument(format!("capability manifest JSON is invalid: {error}"))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        required_manifest_text("manifest version", self.manifest_version.clone())?;
        required_manifest_text("product", self.product.clone())?;
        required_manifest_text("product version", self.product_version.clone())?;
        required_manifest_text("source revision", self.source_revision.clone())?;
        required_manifest_text("build digest", self.build_digest.clone())?;
        required_manifest_text("configuration digest", self.configuration_digest.clone())?;
        required_manifest_text("environment manifest", self.environment_manifest.clone())?;

        validate_manifest_values("schema versions", &self.schema_versions, true)?;
        validate_manifest_values("optional features", &self.optional_features, false)?;
        validate_manifest_values("storage backends", &self.storage_backends, false)?;
        validate_manifest_values("crypto providers", &self.crypto_providers, false)?;
        validate_manifest_values(
            "model provider adapters",
            &self.model_provider_adapters,
            false,
        )?;
        validate_manifest_values(
            "external effect adapters",
            &self.external_effect_adapters,
            false,
        )?;
        validate_manifest_values(
            "federation capabilities",
            &self.federation_capabilities,
            false,
        )?;
        validate_manifest_values("known limitations", &self.known_limitations, false)?;

        if self.supported_profiles.is_empty() {
            return Err(DomainError::InvalidArgument(
                "supported profiles requires at least one value".into(),
            ));
        }
        let mut normalized_profiles = self.supported_profiles.clone();
        normalized_profiles.sort_by_key(|profile| profile.to_string());
        normalized_profiles.dedup();
        if normalized_profiles != self.supported_profiles {
            return Err(DomainError::InvalidArgument(
                "supported profiles must be normalized".into(),
            ));
        }

        if self.manifest_digest != self.calculate_digest() {
            return Err(DomainError::InvalidArgument(
                "capability manifest digest does not match its assertion".into(),
            ));
        }
        if let Some(signature) = &self.signature {
            signature.validate_for(&self.unsigned_signing_digest()?)?;
        }
        Ok(())
    }

    pub fn manifest_version(&self) -> &str {
        &self.manifest_version
    }

    pub fn product(&self) -> &str {
        &self.product
    }

    pub fn product_version(&self) -> &str {
        &self.product_version
    }

    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }

    pub fn build_digest(&self) -> &str {
        &self.build_digest
    }

    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }

    pub fn environment_manifest(&self) -> &str {
        &self.environment_manifest
    }

    pub fn schema_versions(&self) -> &[String] {
        &self.schema_versions
    }

    pub fn supported_profiles(&self) -> &[QualificationProfile] {
        &self.supported_profiles
    }

    pub fn optional_features(&self) -> &[String] {
        &self.optional_features
    }

    pub fn storage_backends(&self) -> &[String] {
        &self.storage_backends
    }

    pub fn crypto_providers(&self) -> &[String] {
        &self.crypto_providers
    }

    pub fn model_provider_adapters(&self) -> &[String] {
        &self.model_provider_adapters
    }

    pub fn external_effect_adapters(&self) -> &[String] {
        &self.external_effect_adapters
    }

    pub fn federation_capabilities(&self) -> &[String] {
        &self.federation_capabilities
    }

    pub fn known_limitations(&self) -> &[String] {
        &self.known_limitations
    }

    pub fn manifest_digest(&self) -> &str {
        &self.manifest_digest
    }

    pub fn signature(&self) -> Option<&SignatureRecord> {
        self.signature.as_ref()
    }

    pub fn unsigned_signing_payload(&self) -> Result<Vec<u8>, DomainError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        serde_json::to_vec(&unsigned).map_err(|error| {
            DomainError::InvalidArgument(format!("manifest signing payload is invalid: {error}"))
        })
    }

    pub fn unsigned_signing_digest(&self) -> Result<String, DomainError> {
        let digest = Sha256::digest(self.unsigned_signing_payload()?);
        Ok(format!("sha256:{digest:x}"))
    }

    pub fn signing_payload_for(&self, signature: &SignatureRecord) -> Result<Vec<u8>, DomainError> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        signature.payload_for(&unsigned)
    }

    pub fn attach_signature(mut self, signature: SignatureRecord) -> Result<Self, DomainError> {
        if self.signature.is_some() {
            return Err(DomainError::PolicyViolation(
                "capability manifest already has a signature".into(),
            ));
        }
        signature.validate_for(&self.unsigned_signing_digest()?)?;
        self.signature = Some(signature);
        Ok(self)
    }

    pub fn validate_signature(&self) -> Result<(), DomainError> {
        let Some(signature) = self.signature.as_ref() else {
            return Err(DomainError::PolicyViolation(
                "capability manifest has no signature".into(),
            ));
        };
        signature.validate_for(&self.unsigned_signing_digest()?)
    }

    fn calculate_digest(&self) -> String {
        let payload = (
            &self.manifest_version,
            &self.product,
            &self.product_version,
            &self.source_revision,
            &self.build_digest,
            &self.configuration_digest,
            &self.environment_manifest,
            &self.schema_versions,
            &self.supported_profiles,
            &self.optional_features,
            &self.storage_backends,
            &self.crypto_providers,
            &self.model_provider_adapters,
            &self.external_effect_adapters,
            &self.federation_capabilities,
            &self.known_limitations,
        );
        let bytes = serde_json::to_vec(&payload).expect("manifest digest payload is serializable");
        let digest = Sha256::digest(bytes);
        format!("sha256:{digest:x}")
    }
}

fn required_manifest_text(field: &str, value: impl Into<String>) -> Result<String, DomainError> {
    let value = value.into().trim().to_owned();
    if value.is_empty() {
        return Err(DomainError::InvalidArgument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn normalized_manifest_values<I, V>(
    field: &str,
    values: I,
    required: bool,
) -> Result<Vec<String>, DomainError>
where
    I: IntoIterator<Item = V>,
    V: Into<String>,
{
    let mut values = values
        .into_iter()
        .map(|value| value.into().trim().to_owned())
        .collect::<Vec<_>>();
    if values.iter().any(String::is_empty) {
        return Err(DomainError::InvalidArgument(format!(
            "{field} must not contain empty values"
        )));
    }
    if required && values.is_empty() {
        return Err(DomainError::InvalidArgument(format!(
            "{field} requires at least one value"
        )));
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn validate_manifest_values(
    field: &str,
    values: &[String],
    required: bool,
) -> Result<(), DomainError> {
    let normalized = normalized_manifest_values(field, values.to_vec(), required)?;
    if normalized != values {
        return Err(DomainError::InvalidArgument(format!(
            "{field} must be normalized"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::VestraceCapabilityManifest;
    use crate::DomainError;
    use crate::conformance::QualificationProfile;
    use crate::trust::{KeyPurpose, KeyReference, SignatureAlgorithm, SignatureRecord};

    fn manifest(
        source_revision: &str,
        optional_features: Vec<&str>,
    ) -> Result<VestraceCapabilityManifest, DomainError> {
        VestraceCapabilityManifest::new(
            "manifest-v1",
            "vestrace",
            "0.2.0",
            source_revision,
            "sha256:build",
            "sha256:config",
            "environment://test",
            vec!["schema-2", "schema-1", "schema-2"],
            vec![QualificationProfile::Trusted, QualificationProfile::Core],
            optional_features,
            vec!["postgres-17"],
            vec!["local-key-provider"],
            vec!["openai"],
            vec!["http-effects"],
            vec!["federation-disabled"],
            vec!["deployment qualification remains open"],
        )
    }

    #[test]
    fn capability_manifest_rejects_blank_identity() {
        let error = VestraceCapabilityManifest::new(
            "manifest-v1",
            "vestrace",
            "0.2.0",
            " ",
            "sha256:build",
            "sha256:config",
            "environment://test",
            vec!["schema-1"],
            vec![QualificationProfile::Core],
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
        )
        .expect_err("blank source revision must be rejected");

        assert!(
            matches!(error, DomainError::InvalidArgument(message) if message.contains("source revision"))
        );

        let list_error = manifest("revision-a", vec![" "])
            .expect_err("blank capability declaration must be rejected");
        assert!(
            matches!(list_error, DomainError::InvalidArgument(message) if message.contains("optional features"))
        );
    }

    #[test]
    fn capability_manifest_normalizes_declarations_and_has_stable_digest() {
        let first = manifest("revision-a", vec!["zeta", " alpha ", "zeta"]).unwrap();
        let second = manifest("revision-a", vec!["zeta", "alpha"]).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.schema_versions(), &["schema-1", "schema-2"]);
        assert_eq!(first.optional_features(), &["alpha", "zeta"]);
        assert_eq!(
            first.supported_profiles(),
            &[QualificationProfile::Core, QualificationProfile::Trusted]
        );
        assert!(first.manifest_digest().starts_with("sha256:"));
        assert_eq!(first.manifest_digest(), second.manifest_digest());
    }

    #[test]
    fn capability_manifest_digest_changes_with_assertion() {
        let first = manifest("revision-a", vec!["feature-a"]).unwrap();
        let second = manifest("revision-b", vec!["feature-a"]).unwrap();

        assert_ne!(first.manifest_digest(), second.manifest_digest());
    }

    #[test]
    fn capability_manifest_json_rejects_tampered_digest_and_non_normalized_values() {
        let manifest = manifest("revision-a", vec!["feature-a"]).unwrap();
        let mut tampered = serde_json::to_value(&manifest).unwrap();
        tampered["manifest_digest"] = serde_json::Value::String("sha256:tampered".into());
        let error = VestraceCapabilityManifest::from_json(&serde_json::to_vec(&tampered).unwrap())
            .expect_err("tampered digest must be rejected");
        assert!(error.to_string().contains("digest"));

        let mut non_normalized = serde_json::to_value(&manifest).unwrap();
        non_normalized["optional_features"] = serde_json::json!(["zeta", "feature-a"]);
        let error =
            VestraceCapabilityManifest::from_json(&serde_json::to_vec(&non_normalized).unwrap())
                .expect_err("non-normalized declaration must be rejected");
        assert!(error.to_string().contains("normalized"));
    }

    #[test]
    fn signed_manifest_preserves_identity_digest_and_supports_unsigned_json() {
        let manifest = manifest("revision-a", vec!["feature-a"]).unwrap();
        let key = KeyReference::new(
            "local-file",
            "release-signing",
            "v1",
            KeyPurpose::Signing,
            "release",
            "ed25519",
        )
        .unwrap();
        let record = SignatureRecord::new(
            manifest.unsigned_signing_digest().unwrap(),
            "issuer://release",
            key,
            SignatureAlgorithm::Ed25519,
            "base64-signature",
            crate::now(),
        )
        .unwrap();

        let signed = manifest.clone().attach_signature(record).unwrap();
        assert!(signed.validate_signature().is_ok());
        assert_eq!(signed.manifest_digest(), manifest.manifest_digest());

        let mut unsigned_json = serde_json::to_value(&manifest).unwrap();
        unsigned_json.as_object_mut().unwrap().remove("signature");
        let decoded =
            VestraceCapabilityManifest::from_json(&serde_json::to_vec(&unsigned_json).unwrap())
                .unwrap();
        assert!(decoded.signature().is_none());
    }
}
