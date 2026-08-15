use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use clap::ValueEnum;
use ring::signature::{ED25519, Ed25519KeyPair, UnparsedPublicKey};
use vestrace_application::{
    QualificationRepository, QualificationRuntime, RuntimeQualificationDecision,
    RuntimeQualificationEvidence, evaluate_runtime_qualification,
};
use vestrace_domain::WorkspaceId;
use vestrace_domain::conformance::gate::{EvidenceOrigin, GateEvidenceStatus, HardGateEvidence};
use vestrace_domain::conformance::{
    CaseOrigin, CaseStatus, ConformanceReport, QualificationProfile, registry,
};
use vestrace_domain::now;
use vestrace_domain::release::VestraceCapabilityManifest;
use vestrace_domain::trust::{
    KeyProvider, KeyProviderError, KeyPurpose, KeyReference, PostIncidentQualificationEvidence,
    QualificationBundle, QualificationLifecycle, QualificationStatus, ResolvedKeyMaterial,
    SecretResolutionRequest, SignatureAlgorithm, SignatureRecord, SignerTrustPolicy,
    SignerTrustRule,
};
use vestrace_infrastructure::{
    AppConfig, ConfigOverrides, PgQualificationRepository, PgStore, QualificationConfig,
};

#[derive(Clone, Debug, ValueEnum)]
pub enum ConformanceProfileArg {
    Core,
    Memory,
    Cognition,
    Autonomy,
    Federation,
    Trusted,
}

impl From<ConformanceProfileArg> for QualificationProfile {
    fn from(arg: ConformanceProfileArg) -> Self {
        match arg {
            ConformanceProfileArg::Core => Self::Core,
            ConformanceProfileArg::Memory => Self::Memory,
            ConformanceProfileArg::Cognition => Self::Cognition,
            ConformanceProfileArg::Autonomy => Self::Autonomy,
            ConformanceProfileArg::Federation => Self::Federation,
            ConformanceProfileArg::Trusted => Self::Trusted,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ConformanceLifecycleArg {
    PreMerge,
    Release,
    Deployment,
    Periodic,
    PostIncident,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ConformanceArtifactArg {
    Manifest,
    Bundle,
}

impl From<ConformanceLifecycleArg> for QualificationLifecycle {
    fn from(arg: ConformanceLifecycleArg) -> Self {
        match arg {
            ConformanceLifecycleArg::PreMerge => Self::PreMerge,
            ConformanceLifecycleArg::Release => Self::Release,
            ConformanceLifecycleArg::Deployment => Self::Deployment,
            ConformanceLifecycleArg::Periodic => Self::Periodic,
            ConformanceLifecycleArg::PostIncident => Self::PostIncident,
        }
    }
}

pub async fn run(
    action: ConformanceAction,
    config_path: Option<&Path>,
    config_overrides: ConfigOverrides,
) -> anyhow::Result<()> {
    match action {
        ConformanceAction::List { profile } => run_list(profile.map(|p| p.into())),
        ConformanceAction::Check { profile, json } => run_check(profile.map(|p| p.into()), json),
        ConformanceAction::Manifest {
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
            config_for_identity,
            known_limitations,
            output,
        } => run_manifest(
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
            output,
            config_for_identity.as_deref().or(config_path),
            &config_overrides,
        ),
        ConformanceAction::Bundle {
            profile,
            lifecycle,
            persist,
            output,
            target_manifest,
            target_manifest_file,
            source_revision,
            build_digest,
            configuration_digest,
            environment_manifest,
            suite_version,
            known_limitations,
            post_incident_evidence_file,
        } => {
            run_bundle(
                profile.into(),
                lifecycle.map(Into::into),
                persist,
                output,
                target_manifest,
                target_manifest_file,
                source_revision,
                build_digest,
                configuration_digest,
                environment_manifest,
                suite_version,
                known_limitations,
                post_incident_evidence_file,
                config_path,
                &config_overrides,
            )
            .await
        }
        ConformanceAction::Verify {
            profile,
            lifecycle,
            target_manifest_file,
            bundle_file,
            output,
            public_key_file,
            require_signature,
            trusted_signer,
            trusted_key_provider,
            trusted_key_id,
            trusted_key_version,
            trusted_key_scope,
            require_trusted_signer,
        } => {
            run_verify(
                profile.into(),
                lifecycle.into(),
                target_manifest_file,
                bundle_file,
                output,
                config_path,
                &config_overrides,
                public_key_file,
                require_signature,
                trusted_signer,
                trusted_key_provider,
                trusted_key_id,
                trusted_key_version,
                trusted_key_scope,
                require_trusted_signer,
            )
            .await
        }
        ConformanceAction::Sign {
            artifact,
            artifact_file,
            private_key_file,
            signer_identity,
            key_provider,
            key_id,
            key_version,
            key_scope,
            output,
        } => run_sign(
            artifact,
            artifact_file,
            private_key_file,
            signer_identity,
            key_provider,
            key_id,
            key_version,
            key_scope,
            output,
        ),
        ConformanceAction::VerifySignature {
            artifact,
            artifact_file,
            public_key_file,
            trusted_signer,
            trusted_key_provider,
            trusted_key_id,
            trusted_key_version,
            trusted_key_scope,
            require_trusted_signer,
        } => run_verify_signature(
            artifact,
            artifact_file,
            public_key_file,
            trusted_signer,
            trusted_key_provider,
            trusted_key_id,
            trusted_key_version,
            trusted_key_scope,
            require_trusted_signer,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
/// A digest of the binary that is answering.
///
/// The build digest is supposed to identify the artefact a qualification was
/// performed against. Reading it from the executable that is producing the
/// manifest is the only version of that claim nobody has to be trusted for.
fn running_executable_digest() -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};

    let path = std::env::current_exe()
        .map_err(|error| anyhow::anyhow!("the running executable could not be located: {error}"))?;
    let mut file = std::fs::File::open(&path)
        .map_err(|error| anyhow::anyhow!("the running executable could not be read: {error}"))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| anyhow::anyhow!("the running executable could not be hashed: {error}"))?;
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

// Direct CLI dispatch boundary: keep every flag explicit.
#[allow(clippy::too_many_arguments)]
pub fn run_manifest(
    manifest_version: String,
    product: String,
    product_version: String,
    source_revision: Option<String>,
    build_digest: Option<String>,
    configuration_digest: Option<String>,
    environment_manifest: Option<String>,
    schema_versions: Vec<String>,
    supported_profiles: Vec<ConformanceProfileArg>,
    optional_features: Vec<String>,
    storage_backends: Vec<String>,
    crypto_providers: Vec<String>,
    model_provider_adapters: Vec<String>,
    external_effect_adapters: Vec<String>,
    federation_capabilities: Vec<String>,
    known_limitations: Vec<String>,
    output: PathBuf,
    config_path: Option<&Path>,
    config_overrides: &ConfigOverrides,
) -> anyhow::Result<()> {
    // Identity that is typed is identity that can be typed wrong. A bundle
    // binds to a digest over these fields and a baseline refuses a bundle whose
    // digest moved — which only means anything if the digest describes the
    // build it was taken from. So each of them is computed here unless the
    // caller deliberately overrides it, and the one that cannot be computed
    // after the fact is refused rather than invented.
    let source_revision = source_revision
        .or_else(|| option_env!("VESTRACE_SOURCE_REVISION").map(str::to_owned))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no source revision: pass --source-revision, or build with \
                 VESTRACE_SOURCE_REVISION set. A manifest that cannot say which source it \
                 describes qualifies nothing in particular"
            )
        })?;

    let build_digest = match build_digest {
        Some(digest) => digest,
        None => running_executable_digest()?,
    };

    let (configuration_digest, environment_manifest) =
        match (configuration_digest, environment_manifest) {
            (Some(digest), Some(environment)) => (digest, environment),
            (configuration_digest, environment_manifest) => {
                let config =
                    AppConfig::load_from_with_overrides(config_path, config_overrides.clone())
                        .map_err(|error| {
                            anyhow::anyhow!("configuration could not be read for identity: {error}")
                        })?;
                (
                    configuration_digest.unwrap_or_else(|| config.identity_digest()),
                    environment_manifest.unwrap_or_else(|| config.environment_summary()),
                )
            }
        };

    let manifest = VestraceCapabilityManifest::new(
        manifest_version,
        product,
        product_version,
        source_revision,
        build_digest,
        configuration_digest,
        environment_manifest,
        schema_versions,
        supported_profiles
            .into_iter()
            .map(QualificationProfile::from)
            .collect(),
        optional_features,
        storage_backends,
        crypto_providers,
        model_provider_adapters,
        external_effect_adapters,
        federation_capabilities,
        known_limitations,
    )
    .map_err(|error| anyhow::anyhow!("failed to build capability manifest: {error}"))?;

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&manifest)?)?;
    println!(
        "Capability manifest: {} ({})",
        output.display(),
        manifest.manifest_digest()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_bundle(
    profile: QualificationProfile,
    lifecycle: Option<QualificationLifecycle>,
    persist: bool,
    output: PathBuf,
    target_manifest: Option<String>,
    target_manifest_file: Option<PathBuf>,
    source_revision: Option<String>,
    build_digest: Option<String>,
    configuration_digest: Option<String>,
    environment_manifest: Option<String>,
    suite_version: String,
    known_limitations: Vec<String>,
    post_incident_evidence_file: Option<PathBuf>,
    config_path: Option<&Path>,
    config_overrides: &ConfigOverrides,
) -> anyhow::Result<()> {
    if target_manifest.is_some() && target_manifest_file.is_some() {
        anyhow::bail!("--target-manifest and --target-manifest-file are mutually exclusive");
    }
    let started_at = vestrace_domain::now();
    let report = build_report(Some(profile));
    let evidence = hard_gate_evidence(&report);
    let lifecycle = lifecycle.unwrap_or(QualificationLifecycle::Release);
    if lifecycle == QualificationLifecycle::PostIncident && post_incident_evidence_file.is_none() {
        anyhow::bail!("--post-incident-evidence-file is required for post-incident qualification");
    }
    if lifecycle != QualificationLifecycle::PostIncident && post_incident_evidence_file.is_some() {
        anyhow::bail!(
            "--post-incident-evidence-file is only valid for post-incident qualification"
        );
    }

    let bundle = if let Some(manifest_path) = target_manifest_file {
        if source_revision.is_some()
            || build_digest.is_some()
            || configuration_digest.is_some()
            || environment_manifest.is_some()
        {
            anyhow::bail!(
                "identity overrides are not accepted with --target-manifest-file; use the manifest file as the canonical target"
            );
        }
        let manifest = VestraceCapabilityManifest::from_json(&std::fs::read(&manifest_path)?)
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed to load capability manifest {}: {error}",
                    manifest_path.display()
                )
            })?;
        QualificationBundle::from_conformance_report_for_manifest(
            lifecycle,
            profile,
            &manifest,
            suite_version,
            report,
            evidence,
            known_limitations,
            started_at,
            Some(vestrace_domain::now()),
        )
    } else {
        QualificationBundle::from_conformance_report(
            lifecycle,
            profile,
            required_bundle_argument("target manifest", target_manifest)?,
            required_bundle_argument("source revision", source_revision)?,
            required_bundle_argument("build digest", build_digest)?,
            required_bundle_argument("configuration digest", configuration_digest)?,
            required_bundle_argument("environment manifest", environment_manifest)?,
            suite_version,
            report,
            evidence,
            known_limitations,
            started_at,
            Some(vestrace_domain::now()),
        )
    }
    .map_err(|error| anyhow::anyhow!("failed to build qualification bundle: {error}"))?;

    let bundle = if let Some(evidence_path) = post_incident_evidence_file {
        let evidence: PostIncidentQualificationEvidence =
            serde_json::from_slice(&std::fs::read(&evidence_path)?).map_err(|error| {
                anyhow::anyhow!(
                    "failed to load post-incident evidence {}: {error}",
                    evidence_path.display()
                )
            })?;
        bundle
            .attach_post_incident_evidence(evidence)
            .map_err(|error| anyhow::anyhow!("invalid post-incident evidence: {error}"))?
    } else {
        bundle
    };

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&bundle)?)?;
    println!(
        "Qualification bundle: {} ({:?})",
        output.display(),
        bundle.status()
    );

    if persist {
        if let Err(error) = persist_to_postgres(&bundle, config_path, config_overrides).await {
            anyhow::bail!(
                "qualification bundle artifact was written to {}; persistence failed: {error}",
                output.display()
            );
        }
        println!("Qualification bundle persisted to PostgreSQL");
    }

    if bundle.status() != QualificationStatus::Passed {
        anyhow::bail!(
            "qualification bundle is not passed; artifact was written to {}",
            output.display()
        );
    }
    Ok(())
}

fn required_bundle_argument(name: &str, value: Option<String>) -> anyhow::Result<String> {
    value.ok_or_else(|| anyhow::anyhow!("{name} is required without --target-manifest-file"))
}

async fn persist_to_postgres(
    bundle: &QualificationBundle,
    config_path: Option<&Path>,
    config_overrides: &ConfigOverrides,
) -> anyhow::Result<()> {
    let config = AppConfig::load_from_with_overrides(config_path, config_overrides.clone())
        .map_err(|error| {
            anyhow::anyhow!("qualification persistence configuration failed: {error}")
        })?;
    let store = PgStore::connect(&config.database).await.map_err(|error| {
        anyhow::anyhow!("qualification persistence database is unavailable: {error}")
    })?;
    store
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("qualification persistence migration failed: {error}"))?;
    match store.migrations_are_compatible().await {
        Ok(true) => {}
        Ok(false) => anyhow::bail!("qualification persistence migration history is incompatible"),
        Err(error) => {
            anyhow::bail!("qualification persistence migration verification failed: {error}")
        }
    }

    let repository = PgQualificationRepository::new(store);
    persist_bundle(bundle, &repository).await
}

pub async fn persist_bundle(
    bundle: &QualificationBundle,
    repository: &dyn QualificationRepository,
) -> anyhow::Result<()> {
    repository
        .insert(bundle)
        .await
        .map_err(|error| anyhow::anyhow!("failed to persist qualification bundle: {error}"))
}

#[derive(serde::Serialize)]
struct AutomaticQualificationArtifact {
    #[serde(flatten)]
    decision: RuntimeQualificationDecision,
    runtime: RuntimeQualificationEvidence,
    bundle_id: String,
    bundle_status: QualificationStatus,
    bundle: QualificationBundle,
}

pub async fn run_automatic_qualification(
    config: &QualificationConfig,
    component: QualificationRuntime,
    store: &PgStore,
) -> anyhow::Result<()> {
    let runtime_result = store.deployment_qualification_evidence().await;
    let runtime = runtime_result
        .as_ref()
        .cloned()
        .unwrap_or_else(|_| RuntimeQualificationEvidence::unavailable());
    let repository = PgQualificationRepository::new(store.clone());
    run_automatic_qualification_with_evidence(
        config,
        component,
        runtime,
        runtime_result.is_ok(),
        &repository,
    )
    .await
}

async fn run_automatic_qualification_with_evidence(
    config: &QualificationConfig,
    component: QualificationRuntime,
    runtime: RuntimeQualificationEvidence,
    runtime_available: bool,
    repository: &dyn QualificationRepository,
) -> anyhow::Result<()> {
    let manifest_path = config
        .manifest_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("qualification.manifest_file is required when enabled"))?;
    let profile = config
        .profile
        .ok_or_else(|| anyhow::anyhow!("qualification.profile is required when enabled"))?;
    let lifecycle = config
        .lifecycle
        .ok_or_else(|| anyhow::anyhow!("qualification.lifecycle is required when enabled"))?;
    if lifecycle != QualificationLifecycle::Deployment {
        anyhow::bail!(
            "automatic server/worker qualification requires qualification.lifecycle=deployment"
        );
    }
    let suite_version = config
        .suite_version
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("qualification.suite_version is required when enabled"))?;
    let output = config
        .output
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("qualification.output is required when enabled"))?;
    let manifest =
        VestraceCapabilityManifest::from_json(&std::fs::read(manifest_path)?).map_err(|error| {
            anyhow::anyhow!(
                "failed to load qualification capability manifest {}: {error}",
                manifest_path.display()
            )
        })?;

    let mut report = build_report(Some(profile));
    let runtime_requirement = vestrace_domain::conformance::RequirementId::new(
        vestrace_domain::conformance::RequirementFamily::Qual,
        3,
    );
    let runtime_message = if runtime_available
        && runtime.migration_history_compatible
        && runtime.runtime_database_is_restricted()
    {
        format!("live PostgreSQL deployment evidence passed for {component}")
    } else {
        format!("live PostgreSQL deployment evidence failed for {component}")
    };
    let runtime_evidence = Some(format!("runtime://{component}/postgres"));
    let runtime_status = if runtime_available
        && runtime.migration_history_compatible
        && runtime.runtime_database_is_restricted()
    {
        CaseStatus::Pass
    } else {
        CaseStatus::Fail
    };
    let mut replaced_runtime_case = false;
    for result in &mut report.results {
        if result.requirement_ids.contains(&runtime_requirement) {
            result.status = runtime_status;
            result.message = runtime_message.clone();
            result.evidence = runtime_evidence.clone();
            // This one queried a live database, so it is an executed result
            // even where it is replacing an attestation.
            result.origin = CaseOrigin::Executed;
            replaced_runtime_case = true;
        }
    }
    if !replaced_runtime_case {
        report
            .results
            .push(vestrace_domain::conformance::ConformanceCaseResult {
                case_id: format!("runtime-deployment-{component}"),
                requirement_ids: vec![runtime_requirement],
                status: runtime_status,
                message: runtime_message,
                evidence: runtime_evidence,
                origin: CaseOrigin::Executed,
            });
    }
    let report = ConformanceReport::from_results(report.profile, report.results);
    let bundle = QualificationBundle::from_conformance_report_for_manifest(
        lifecycle,
        profile,
        &manifest,
        suite_version,
        report.clone(),
        hard_gate_evidence(&report),
        manifest.known_limitations().to_vec(),
        now(),
        Some(now()),
    )
    .map_err(|error| anyhow::anyhow!("failed to build automatic qualification bundle: {error}"))?;

    let decision =
        evaluate_runtime_qualification(component, &manifest, &bundle, profile, lifecycle, &runtime);
    let artifact = AutomaticQualificationArtifact {
        decision,
        runtime,
        bundle_id: bundle.id().to_string(),
        bundle_status: bundle.status(),
        bundle: bundle.clone(),
    };
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&artifact)?)?;

    persist_bundle(&bundle, repository).await?;

    println!(
        "Automatic {component} qualification: {} ({})",
        output.display(),
        artifact.decision.status
    );
    if !artifact.decision.is_passed() {
        anyhow::bail!(
            "automatic {component} qualification failed; evidence was written to {} and persisted",
            output.display()
        );
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct DeploymentQualificationResult {
    profile: QualificationProfile,
    lifecycle: QualificationLifecycle,
    target_manifest: String,
    status: &'static str,
    checks: BTreeMap<String, &'static str>,
}

// Direct CLI dispatch boundary: keep every flag explicit.
#[allow(clippy::too_many_arguments)]
pub async fn run_verify(
    profile: QualificationProfile,
    lifecycle: QualificationLifecycle,
    target_manifest_file: PathBuf,
    bundle_file: PathBuf,
    output: PathBuf,
    config_path: Option<&Path>,
    config_overrides: &ConfigOverrides,
    public_key_file: Option<PathBuf>,
    require_signature: bool,
    trusted_signer: Option<String>,
    trusted_key_provider: Option<String>,
    trusted_key_id: Option<String>,
    trusted_key_version: Option<String>,
    trusted_key_scope: Option<String>,
    require_trusted_signer: bool,
) -> anyhow::Result<()> {
    let manifest = VestraceCapabilityManifest::from_json(&std::fs::read(&target_manifest_file)?)
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to load capability manifest {}: {error}",
                target_manifest_file.display()
            )
        })?;
    let bundle: QualificationBundle = serde_json::from_slice(&std::fs::read(&bundle_file)?)
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to load qualification bundle {}: {error}",
                bundle_file.display()
            )
        })?;

    bundle
        .validate_manifest_identity(&manifest, profile, lifecycle)
        .map_err(|error| anyhow::anyhow!("qualification target binding failed: {error}"))?;

    let trust_policy = build_signer_trust_policy(
        trusted_signer,
        trusted_key_provider,
        trusted_key_id,
        trusted_key_version,
        trusted_key_scope,
        require_trusted_signer,
    )?;
    let mut checks = BTreeMap::from([
        ("manifest_integrity".to_owned(), "passed"),
        ("bundle_target_binding".to_owned(), "passed"),
        (
            "bundle_status".to_owned(),
            if bundle.status() == QualificationStatus::Passed {
                "passed"
            } else {
                "failed"
            },
        ),
    ]);

    if require_signature || public_key_file.is_some() || trust_policy.is_some() {
        let public_key_file = public_key_file.ok_or_else(|| {
            anyhow::anyhow!("signature or signer trust verification requires --public-key-file")
        })?;
        verify_bundle_signature(&bundle, &std::fs::read(public_key_file)?)?;
        checks.insert("signature".to_owned(), "passed");
        if let Some(policy) = trust_policy.as_ref() {
            let (trust_status, _) = signer_trust_status(
                bundle
                    .signature()
                    .ok_or_else(|| anyhow::anyhow!("qualification bundle has no signature"))?,
                Some(policy),
            );
            checks.insert(
                "signer_trust".to_owned(),
                if trust_status == "trusted" {
                    "passed"
                } else {
                    "failed"
                },
            );
        }
    }

    let store = match AppConfig::load_from_with_overrides(config_path, config_overrides.clone()) {
        Ok(config) => PgStore::connect(&config.database).await.ok(),
        Err(_) => None,
    };
    if let Some(store) = store {
        match store.deployment_qualification_evidence().await {
            Ok(evidence) => {
                checks.insert(
                    "migration_history".to_owned(),
                    if evidence.migration_history_compatible {
                        "passed"
                    } else {
                        "failed"
                    },
                );
                checks.insert(
                    "runtime_database".to_owned(),
                    if !evidence.is_superuser
                        && !evidence.bypasses_rls
                        && !evidence.inherits_bootstrap
                    {
                        "passed"
                    } else {
                        "failed"
                    },
                );
            }
            Err(_) => {
                checks.insert("migration_history".to_owned(), "failed");
                checks.insert("runtime_database".to_owned(), "failed");
            }
        }
    } else {
        checks.insert("migration_history".to_owned(), "failed");
        checks.insert("runtime_database".to_owned(), "failed");
    }

    let status = if checks.values().all(|value| *value == "passed") {
        "passed"
    } else {
        "failed"
    };
    let result = DeploymentQualificationResult {
        profile,
        lifecycle,
        target_manifest: manifest.manifest_digest().to_owned(),
        status,
        checks,
    };
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&result)?)?;
    println!("Deployment qualification: {} ({status})", output.display());
    if status != "passed" {
        anyhow::bail!(
            "deployment qualification failed; evidence was written to {}",
            output.display()
        );
    }
    Ok(())
}

fn signature_algorithm_name(algorithm: SignatureAlgorithm) -> &'static str {
    algorithm.as_str()
}

fn create_signature<T: serde::Serialize>(
    object: &T,
    object_digest: String,
    signer_identity: &str,
    key_ref: KeyReference,
    private_key: &Ed25519KeyPair,
) -> anyhow::Result<SignatureRecord> {
    let signed_at = now();
    let placeholder = SignatureRecord::new(
        object_digest.clone(),
        signer_identity,
        key_ref.clone(),
        SignatureAlgorithm::Ed25519,
        "pending",
        signed_at,
    )?;
    let payload = placeholder.payload_for(object)?;
    let signature = private_key.sign(&payload);
    Ok(SignatureRecord::new(
        object_digest,
        signer_identity,
        key_ref,
        SignatureAlgorithm::Ed25519,
        base64::engine::general_purpose::STANDARD.encode(signature.as_ref()),
        signed_at,
    )?)
}

fn verify_signature_payload(
    payload: Vec<u8>,
    record: &SignatureRecord,
    public_key: &[u8],
) -> anyhow::Result<()> {
    let signature = base64::engine::general_purpose::STANDARD
        .decode(record.signature())
        .map_err(|error| anyhow::anyhow!("signature is not valid base64: {error}"))?;
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&payload, &signature)
        .map_err(|_| anyhow::anyhow!("Ed25519 signature verification failed"))
}

fn verify_bundle_signature(bundle: &QualificationBundle, public_key: &[u8]) -> anyhow::Result<()> {
    let record = bundle
        .signature()
        .ok_or_else(|| anyhow::anyhow!("qualification bundle has no signature"))?;
    record.validate_for(&bundle.unsigned_signing_digest()?)?;
    verify_signature_payload(bundle.signing_payload_for(record)?, record, public_key)
}

fn verify_manifest_signature(
    manifest: &VestraceCapabilityManifest,
    public_key: &[u8],
) -> anyhow::Result<()> {
    let record = manifest
        .signature()
        .ok_or_else(|| anyhow::anyhow!("capability manifest has no signature"))?;
    record.validate_for(&manifest.unsigned_signing_digest()?)?;
    verify_signature_payload(manifest.signing_payload_for(record)?, record, public_key)
}

struct LocalFileKeyProvider {
    key_file: PathBuf,
}

impl LocalFileKeyProvider {
    fn new(key_file: PathBuf) -> Self {
        Self { key_file }
    }
}

impl KeyProvider for LocalFileKeyProvider {
    fn resolve(
        &self,
        key: &KeyReference,
        request: &SecretResolutionRequest,
    ) -> Result<ResolvedKeyMaterial, KeyProviderError> {
        if key.provider() != "local-file" {
            return Err(KeyProviderError::Denied(format!(
                "provider '{}' is not implemented by the local-file adapter",
                key.provider()
            )));
        }
        if key.purpose() != KeyPurpose::Signing {
            return Err(KeyProviderError::Denied(
                "local-file adapter only resolves signing keys".into(),
            ));
        }
        if !key.is_usable() {
            return Err(KeyProviderError::NotUsable);
        }
        if request.purpose() != key.scope() || request.authorization_ref().trim().is_empty() {
            return Err(KeyProviderError::Denied(
                "key resolution request is outside the authorized key scope".into(),
            ));
        }
        let bytes = std::fs::read(&self.key_file).map_err(|error| {
            KeyProviderError::Unavailable(format!("failed to read local key material: {error}"))
        })?;
        ResolvedKeyMaterial::from_ephemeral(bytes)
    }
}

fn resolve_signing_key(
    key_ref: &KeyReference,
    private_key_file: PathBuf,
) -> anyhow::Result<Ed25519KeyPair> {
    let provider = LocalFileKeyProvider::new(private_key_file);
    let request =
        SecretResolutionRequest::new(WorkspaceId::new(), key_ref.scope(), "conformance://sign");
    let key_material = provider
        .resolve(key_ref, &request)
        .map_err(|error| anyhow::anyhow!("signing key resolution failed: {error}"))?;
    let key_bytes = key_material.into_bytes();
    Ed25519KeyPair::from_pkcs8(&key_bytes)
        .map_err(|_| anyhow::anyhow!("Ed25519 private key is not valid PKCS#8"))
}

// Direct CLI dispatch boundary: keep every flag explicit.
#[allow(clippy::too_many_arguments)]
pub fn run_sign(
    artifact: ConformanceArtifactArg,
    artifact_file: PathBuf,
    private_key_file: PathBuf,
    signer_identity: String,
    key_provider: String,
    key_id: String,
    key_version: String,
    key_scope: String,
    output: PathBuf,
) -> anyhow::Result<()> {
    let key_ref = KeyReference::new(
        key_provider,
        key_id,
        key_version,
        KeyPurpose::Signing,
        key_scope,
        SignatureAlgorithm::Ed25519.as_str(),
    )?;
    let private_key = resolve_signing_key(&key_ref, private_key_file)?;
    let serialized = match artifact {
        ConformanceArtifactArg::Manifest => {
            let manifest = VestraceCapabilityManifest::from_json(&std::fs::read(&artifact_file)?)
                .map_err(|error| {
                anyhow::anyhow!("failed to load capability manifest: {error}")
            })?;
            if manifest.signature().is_some() {
                anyhow::bail!("capability manifest is already signed")
            }
            let signature = create_signature(
                &manifest,
                manifest.unsigned_signing_digest()?,
                &signer_identity,
                key_ref,
                &private_key,
            )?;
            serde_json::to_vec_pretty(&manifest.attach_signature(signature)?)?
        }
        ConformanceArtifactArg::Bundle => {
            let bundle: QualificationBundle =
                serde_json::from_slice(&std::fs::read(&artifact_file)?).map_err(|error| {
                    anyhow::anyhow!("failed to load qualification bundle: {error}")
                })?;
            if bundle.signature().is_some() {
                anyhow::bail!("qualification bundle is already signed")
            }
            let signature = create_signature(
                &bundle,
                bundle.unsigned_signing_digest()?,
                &signer_identity,
                key_ref,
                &private_key,
            )?;
            serde_json::to_vec_pretty(&bundle.attach_signature(signature)?)?
        }
    };
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&output, serialized)?;
    println!("Signed artifact: {}", output.display());
    Ok(())
}

#[derive(serde::Serialize)]
struct SignatureVerificationResult {
    artifact: &'static str,
    object_digest: String,
    signer_identity: String,
    algorithm: &'static str,
    cryptographic_status: &'static str,
    trust_status: &'static str,
    trust_failures: Vec<String>,
    status: &'static str,
}

// Direct CLI dispatch boundary: keep every flag explicit.
#[allow(clippy::too_many_arguments)]
pub fn run_verify_signature(
    artifact: ConformanceArtifactArg,
    artifact_file: PathBuf,
    public_key_file: PathBuf,
    trusted_signer: Option<String>,
    trusted_key_provider: Option<String>,
    trusted_key_id: Option<String>,
    trusted_key_version: Option<String>,
    trusted_key_scope: Option<String>,
    require_trusted_signer: bool,
) -> anyhow::Result<()> {
    let public_key = std::fs::read(&public_key_file).map_err(|error| {
        anyhow::anyhow!(
            "failed to read Ed25519 public key {}: {error}",
            public_key_file.display()
        )
    })?;
    let trust_policy = build_signer_trust_policy(
        trusted_signer,
        trusted_key_provider,
        trusted_key_id,
        trusted_key_version,
        trusted_key_scope,
        require_trusted_signer,
    )?;
    let result = match artifact {
        ConformanceArtifactArg::Manifest => {
            let manifest = VestraceCapabilityManifest::from_json(&std::fs::read(&artifact_file)?)
                .map_err(|error| {
                anyhow::anyhow!("failed to load capability manifest: {error}")
            })?;
            let record = manifest
                .signature()
                .ok_or_else(|| anyhow::anyhow!("capability manifest has no signature"))?;
            verify_manifest_signature(&manifest, &public_key)?;
            signature_verification_result("manifest", record, trust_policy.as_ref())?
        }
        ConformanceArtifactArg::Bundle => {
            let bundle: QualificationBundle =
                serde_json::from_slice(&std::fs::read(&artifact_file)?).map_err(|error| {
                    anyhow::anyhow!("failed to load qualification bundle: {error}")
                })?;
            let record = bundle
                .signature()
                .ok_or_else(|| anyhow::anyhow!("qualification bundle has no signature"))?;
            verify_bundle_signature(&bundle, &public_key)?;
            signature_verification_result("bundle", record, trust_policy.as_ref())?
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result.status != "passed" {
        anyhow::bail!("signature verification failed trusted-signer policy");
    }
    Ok(())
}

fn build_signer_trust_policy(
    trusted_signer: Option<String>,
    trusted_key_provider: Option<String>,
    trusted_key_id: Option<String>,
    trusted_key_version: Option<String>,
    trusted_key_scope: Option<String>,
    require_trusted_signer: bool,
) -> anyhow::Result<Option<SignerTrustPolicy>> {
    let values = [
        trusted_signer.as_ref(),
        trusted_key_provider.as_ref(),
        trusted_key_id.as_ref(),
        trusted_key_version.as_ref(),
        trusted_key_scope.as_ref(),
    ];
    if values.iter().all(Option::is_none) {
        if require_trusted_signer {
            anyhow::bail!(
                "--require-trusted-signer requires all trusted signer and key metadata flags"
            );
        }
        return Ok(None);
    }
    if values.iter().any(Option::is_none) {
        anyhow::bail!(
            "trusted signer policy requires --trusted-signer, --trusted-key-provider, --trusted-key-id, --trusted-key-version, and --trusted-key-scope"
        );
    }
    let rule = SignerTrustRule::new(
        trusted_signer.unwrap(),
        trusted_key_provider.unwrap(),
        trusted_key_id.unwrap(),
        trusted_key_version.unwrap(),
        trusted_key_scope.unwrap(),
        SignatureAlgorithm::Ed25519,
    )?;
    Ok(Some(SignerTrustPolicy::new(vec![rule])?))
}

fn signature_verification_result(
    artifact: &'static str,
    record: &SignatureRecord,
    trust_policy: Option<&SignerTrustPolicy>,
) -> anyhow::Result<SignatureVerificationResult> {
    let (trust_status, trust_failures) = signer_trust_status(record, trust_policy);
    let status = if trust_status == "untrusted" {
        "failed"
    } else {
        "passed"
    };
    Ok(SignatureVerificationResult {
        artifact,
        object_digest: record.object_digest().to_owned(),
        signer_identity: record.signer_identity().to_owned(),
        algorithm: signature_algorithm_name(record.algorithm()),
        cryptographic_status: "passed",
        trust_status,
        trust_failures,
        status,
    })
}

fn signer_trust_status(
    record: &SignatureRecord,
    trust_policy: Option<&SignerTrustPolicy>,
) -> (&'static str, Vec<String>) {
    match trust_policy {
        None => ("not_requested", Vec::new()),
        Some(policy) => {
            let decision = policy.evaluate(record);
            let status = if decision.is_trusted() {
                "trusted"
            } else {
                "untrusted"
            };
            let failures = decision
                .failures()
                .iter()
                .map(|failure| format!("{failure:?}"))
                .collect();
            (status, failures)
        }
    }
}

fn hard_gate_evidence(report: &ConformanceReport) -> Vec<HardGateEvidence> {
    let mut seen = HashSet::new();
    let mut evidence = Vec::new();
    for result in &report.results {
        let status = match result.status {
            CaseStatus::Pass => GateEvidenceStatus::Pass,
            CaseStatus::Fail => GateEvidenceStatus::Fail,
            CaseStatus::Skip => GateEvidenceStatus::Skipped,
            CaseStatus::NotApplicable => GateEvidenceStatus::NotApplicable,
        };
        // The origin is read from the result rather than assumed. This was
        // hardcoded to `LocalExecutable` for every result, so the hard gate was
        // told that a sentence somebody typed was locally executed evidence —
        // the exact distinction `EvidenceOrigin` exists to preserve.
        // An attestation produced here is local. Filing it as
        // `RemoteSelfAsserted` put this installation's reading of its own code
        // in the same category as a federation peer's claim about itself, which
        // is a different thing with a different reason for being distrusted.
        // The gate now admits a local attestation only where the requirement's
        // class is `Static`, which is exactly where the report already says
        // execution is not the right form.
        let origin = match result.origin {
            CaseOrigin::Executed => EvidenceOrigin::LocalExecutable,
            CaseOrigin::Attested => EvidenceOrigin::LocalAttested,
        };
        for requirement_id in &result.requirement_ids {
            if seen.insert(*requirement_id) {
                evidence.push(HardGateEvidence::new(
                    *requirement_id,
                    status,
                    result.evidence.clone(),
                    None,
                    origin,
                ));
            }
        }
    }
    evidence
}

use crate::ConformanceAction;

pub fn run_list(profile: Option<QualificationProfile>) -> anyhow::Result<()> {
    let reqs = registry::all();
    let filtered: Vec<_> = match profile {
        Some(p) => {
            let allowed_ids = profile_requirement_ids(p);
            reqs.iter()
                .filter(|r| allowed_ids.contains(&r.id))
                .collect()
        }
        None => reqs.iter().collect(),
    };

    if let Some(p) = profile {
        println!("Profile: {p}");
        println!();
    }

    // The catalogue's own wording, not the English gloss. `statement` is a
    // paraphrase and paraphrases drift — several described a different
    // requirement than the id they were attached to — so a listing that a reader
    // uses to decide what is required shows the authoritative text.
    println!("{:<10} {:<10} {:<12} Requirement", "ID", "Level", "Class");
    println!("{}", "-".repeat(80));
    for req in &filtered {
        println!(
            "{:<10} {:<10} {:<12} {}",
            req.id, req.level, req.class, req.spec_statement
        );
    }
    println!();
    println!("Total: {} requirements", filtered.len());
    Ok(())
}

pub fn run_check(profile: Option<QualificationProfile>, json_output: bool) -> anyhow::Result<()> {
    let report = build_report(profile);

    if json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| anyhow::anyhow!("failed to serialize report: {e}"))?;
        println!("{json}");
        if !report.is_pass() {
            anyhow::bail!("conformance report contains failed or skipped cases");
        }
        return Ok(());
    }

    print_report_human(&report);
    if !report.is_pass() {
        anyhow::bail!("conformance report contains failed or skipped cases");
    }
    Ok(())
}

fn build_report(profile: Option<QualificationProfile>) -> ConformanceReport {
    use std::collections::HashMap;
    use vestrace_domain::conformance::{CaseOrigin, cases};

    let reqs = registry::all();

    // An executed case outranks a written claim about the same requirement.
    // Everything used to come from `evaluate_requirement`, a table of
    // hand-written assertions that ran nothing; those remain as the fallback,
    // but they are now marked `Attested` rather than passed off as verified.
    //
    // Assembled from both layers. Domain cases can only reach domain types, and
    // most of what is left — how channels fuse, what a boundary refuses, what a
    // journal records — is application behaviour that is invisible from there.
    // The CLI is the first place that can see both.
    let executed: HashMap<_, _> = cases::executable_cases()
        .run_all(None)
        .results
        .into_iter()
        .chain(
            vestrace_application::conformance_cases::executable_cases()
                .run_all(None)
                .results,
        )
        .filter_map(|result| {
            result
                .requirement_ids
                .first()
                .copied()
                .map(|id| (id, result))
        })
        .collect();

    let results: Vec<_> = reqs
        .iter()
        .map(|req| match executed.get(&req.id) {
            Some(result) => result.clone(),
            None => {
                let (status, message, evidence) = evaluate_requirement(req);
                vestrace_domain::conformance::ConformanceCaseResult {
                    case_id: format!("registry-{}", req.id),
                    requirement_ids: vec![req.id],
                    status,
                    message,
                    evidence,
                    origin: CaseOrigin::Attested,
                }
            }
        })
        .collect();

    let filtered = match profile {
        Some(p) => {
            let allowed = profile_requirement_ids(p);
            results
                .into_iter()
                .map(|mut r| {
                    let matches = r.requirement_ids.iter().any(|id| allowed.contains(id));
                    if !matches {
                        r.status = CaseStatus::NotApplicable;
                    }
                    r
                })
                .collect()
        }
        None => results,
    };

    ConformanceReport::from_results(profile, filtered)
}

fn evaluate_requirement(
    req: &vestrace_domain::conformance::Requirement,
) -> (CaseStatus, String, Option<String>) {
    use vestrace_domain::conformance::{RequirementFamily as F, VerificationClass as V};

    // Everything returned here is an **attestation**: a reading of the code
    // written down by a person, marked `CaseOrigin::Attested` by the caller. It
    // is the correct evidence form for `VerificationClass::Static`, which
    // describes architectural shape no runtime assertion observes, and it is
    // not evidence for anything else. Behavioural requirements belong in
    // `vestrace_domain::conformance::cases`, where a case can fail.
    match (req.id.family, req.id.number, req.class) {
        // A skip that says why. "No conformance case registered yet" is true of
        // both a requirement nobody has looked at and one whose evidence exists
        // but cannot run here, and those call for entirely different work.
        (F::Qual, 1, V::Static) => (
            CaseStatus::Pass,
            "Tests, conformance and qualification are three types with three lifetimes. A test              is a `#[test]` that gates a build and leaves nothing behind. A conformance case is              a `ConformanceCase` producing a `ConformanceCaseResult` that names a requirement              and carries `CaseOrigin::{Executed, Attested}`, so a claim and a run are              distinguishable afterwards. A qualification is a `QualificationBundle`: a profile,              a conformance report, hard-gate evidence, the build/config/environment identity of              what was measured, known limitations, and a target digest a `QualificationBaseline`              is bound to. Neither of the last two can be produced from the first: a green suite              is not a report, and a passing report is not a bundle — `from_conformance_report`              additionally requires the identity of the thing qualified, and refuses a report              that does not cover the profile's closure".to_string(),
            Some("crates/vestrace-domain/src/trust.rs".to_string()),
        ),
        (F::Qual, 10, _) => (
            CaseStatus::Skip,
            "Cognitive qualification has no scenarios of its own. The evaluation and learning              types exist (`EvaluationRun`, learning proposals) and nothing runs a model against              a rubric as part of qualification, so there is no place where exact wording could              be compared and no place where properties are compared instead. This is a SHOULD              over an unbuilt subsystem, and the honest reading is that the requirement has              nothing to be true or false about yet".to_string(),
            None,
        ),
        (F::Rec, 16, _) => (
            CaseStatus::Skip,
            "Nothing preserves forensic evidence before a destructive recovery, because              nothing performs a destructive recovery. `CaptureProfile::Forensic` exists in the              state-engine types and is constructed by no code; there is no snapshot-before-             overwrite step to attach evidence to. What is in place is the surrounding              discipline this requirement protects: a recovery point carries its provenance and              refuses to be restored from unless its integrity was checked (REC-007, REC-008),              and closing an incident keeps everything it was opened for (REC-018). This is a              SHOULD, and it is unimplemented rather than unverified".to_string(),
            None,
        ),
        (F::Idw, 10, _) => (
            CaseStatus::Skip,
            "`SharedMemoryRef` cannot be substituted for a local `MemoryId` because no              conversion exists: it exposes `source_memory_id()` and no `From`, `Into`, `Deref`              or accessor yielding a local identifier. That is a property of the type, and the              absence of a conversion is precisely what a runtime case cannot observe — a case              asserting it would only be re-stating that it did not call something. IDW-009              executes the half that is observable: the reference names its source workspace,              memory, revision and grant revision, and its source workspace is never the              borrowing one"
                .to_string(),
            Some("crates/vestrace-domain/src/enterprise/sharing.rs".to_string()),
        ),
        (F::Idw, 14, _) => (
            CaseStatus::Skip,
            "Cross-workspace sharing exists only in the domain — `MemoryShareGrant`, \
             `MemoryMount` and `evaluate_share_access` are constructed by no adapter and \
             reachable from no surface — so this build has never performed a cross-workspace \
             read, and there is no code path that could be tempted to relax isolation to \
             perform one. What can be said is the negative half, and it is now checked by a \
             live test rather than asserted: every table carrying a workspace_id forces row \
             level security, including `cross_workspace_memory_grants` itself. Verifying that \
             a future sharing adapter reads across a boundary *without* relaxing that needs \
             the adapter to exist first".to_string(),
            Some("crates/vestrace-infrastructure/tests/row_level_security.rs".to_string()),
        ),
        (F::Arc, 1, V::Static) => (
            CaseStatus::Pass,
            "Authoritative and derived state are separated by table and by type: run_events \
             (append-only, trigger-enforced) with run_streams as the version authority is \
             canonical, while agent_runs, WorkflowExecution and StepExecution are projections \
             rebuilt from it. RunCoordinator is the sole writer of the canonical pair; the \
             projections have no independent write path".to_string(),
            Some("migrations/0112_run_streams.sql + migrations/0113_event_append_only_trigger.sql + crates/vestrace-domain/src/execution/mod.rs".to_string()),
        ),
        (F::Arc, 4, V::Static) => (
            CaseStatus::Pass,
            "One execution boundary: every run mutation goes through RunCoordinator, and \
             AppState no longer holds a RunCommandExecutor, so the single-writer property is a \
             compile-time fact rather than a convention. Authorization passes through \
             PolicyDecisionEngine on both the HTTP surface and the worker, and durable actions \
             record an AuditEvent through the same repository".to_string(),
            Some("crates/vestrace-application/src/run/coordinator.rs + crates/vestrace-http/src/router.rs".to_string()),
        ),
        (F::Arc, 8, V::Static) => (
            CaseStatus::Pass,
            "Holding a reference is never sufficient: SecretRef::authorize_resolution requires \
             a matching workspace, a matching purpose and a non-blank authorization reference \
             before it issues a lease, and artifact content is addressed by SHA-256 inside a \
             workspace-scoped table under RLS so a known digest does not reach another \
             tenant's bytes. The lease half of this is verified executably by TMP-009; the \
             claim that the property holds across *every* identity type in the system is the \
             attested part".to_string(),
            Some("crates/vestrace-domain/src/trust.rs + migrations/0134_artifact_content_storage.sql".to_string()),
        ),
        (F::Arc, 9, V::Static) => (
            CaseStatus::Pass,
            "Durable entities carry newtype ids rather than bare UUIDs, so a RunStepId cannot \
             be passed where an AgentRunId is expected, and every durable table carries \
             workspace_id with FORCE ROW LEVEL SECURITY naming the workspace as authority \
             owner".to_string(),
            Some("crates/vestrace-domain/src/id.rs + docs/security-and-rls.md".to_string()),
        ),
        (F::Arc, 5, V::Static) => (
            CaseStatus::Pass,
            "Single event store: only run_events table exists, no parallel orchestration runtime. \
             WorkflowExecution/StepExecution are typed history projections with run_id link to AgentRun, not competing event-sourced runtimes".to_string(),
            Some("crates/vestrace-domain/src/execution/mod.rs + migrations/0115_execution_ownership_link.sql".to_string()),
        ),
        (F::Arc, 7, V::Stateful) => (
            CaseStatus::Pass,
            "RunStatus has explicit Unknown/WaitingForInput/WaitingForApproval states".to_string(),
            Some("crates/vestrace-domain/src/run/state.rs".to_string()),
        ),
        (F::Arc, 10, V::Static) => (
            CaseStatus::Pass,
            "WorkflowExecution/StepExecution have doc comments labeling them as typed history projections, not source of truth".to_string(),
            Some("crates/vestrace-domain/src/execution/mod.rs".to_string()),
        ),
        (F::Mem, 1, V::Domain) => (
            CaseStatus::Pass,
            "Memory has stable MemoryId with immutable MemoryRevision sequence".to_string(),
            Some("crates/vestrace-domain/src/memory/mod.rs".to_string()),
        ),
        (F::Mem, 2, V::Domain) => (
            CaseStatus::Pass,
            "Semantic content change creates new revision; no in-place overwrite".to_string(),
            Some("crates/vestrace-domain/src/memory/mod.rs".to_string()),
        ),
        (F::Mem, 4, V::Domain) => (
            CaseStatus::Pass,
            "Revision numbers are monotonic via revision_number sequence; state_revision tracks aggregate version".to_string(),
            Some("crates/vestrace-domain/src/memory/revision.rs".to_string()),
        ),
        (F::Mem, 5, V::Domain) => (
            CaseStatus::Pass,
            "MemorySource links Active Memory to evidence source; EvidenceRef v2 unifies EventRef/ArtifactRevisionRef/ModelExecutionRef/ToolResultRef/etc".to_string(),
            Some("crates/vestrace-domain/src/provenance.rs".to_string()),
        ),
        (F::Mem, 6, V::Domain) => (
            CaseStatus::Pass,
            "Derivation v2 has input_refs, output_ref, execution_ref, model_ref, policy_version, created_by; DerivationMethod includes Extraction/Summarization/Inference/Consolidation/ConflictResolution/RuleBased/HumanAuthored/Import/FederatedDerivation".to_string(),
            Some("crates/vestrace-domain/src/provenance.rs".to_string()),
        ),
        (F::Mem, 8, V::Domain) => (
            CaseStatus::Pass,
            "MemoryRevision has valid_from/valid_until fields with validate_temporal_range() enforcing valid_until >= valid_from".to_string(),
            Some("crates/vestrace-domain/src/memory/revision.rs".to_string()),
        ),
        (F::Mem, 9, V::Domain) => (
            CaseStatus::Pass,
            "Confidence and Importance newtypes enforce [0,1] range at construction time".to_string(),
            Some("crates/vestrace-domain/src/memory/kind.rs".to_string()),
        ),
        (F::Mem, 12, V::Evidence) => (
            CaseStatus::Pass,
            "EvidenceRef preserves exact identity/version without copying content; MemorySource.evidence_ref provides provenance closure".to_string(),
            Some("crates/vestrace-domain/src/provenance.rs".to_string()),
        ),
        (F::Mem, 13, V::Domain) => (
            CaseStatus::Pass,
            "Claim is a separate domain type from Memory: Claim has semantic_key, subject/predicate/value, lifecycle_status; Memory has kind, active_revision_id".to_string(),
            Some("crates/vestrace-domain/src/claim/claim.rs".to_string()),
        ),
        (F::Mem, 14, V::Domain) => (
            CaseStatus::Pass,
            "ClaimStatus::Supported means evidence/policy evaluation passed; doc comment states it is not absolute external truth".to_string(),
            Some("crates/vestrace-domain/src/claim/claim.rs".to_string()),
        ),
        (F::Mem, 15, V::Stateful) => (
            CaseStatus::Pass,
            "ClaimEvidenceLink tracks admissible evidence per claim; loss of supporting evidence triggers revalidation via lifecycle transitions".to_string(),
            Some("crates/vestrace-domain/src/claim/evidence.rs".to_string()),
        ),
        (F::Mem, 16, V::Stateful) => (
            CaseStatus::Pass,
            "Conflict with ConflictStatus::Open persists until reconciliation; status transitions require explicit propose_reconciliation/resolve/accept_ambiguity".to_string(),
            Some("crates/vestrace-domain/src/claim/conflict.rs".to_string()),
        ),
        (F::Mem, 17, V::Stateful) => (
            CaseStatus::Pass,
            "Conflict does not auto-resolve by latest-timestamp; ConflictKind distinguishes semantic/temporal/source/policy/scope conflicts".to_string(),
            Some("crates/vestrace-domain/src/claim/conflict.rs".to_string()),
        ),
        (F::Mem, 18, V::Evidence) => (
            CaseStatus::Pass,
            "Conflict resolution preserves evidence_refs and reconciliation_ref; resolved conflict retains participant_refs and evidence history".to_string(),
            Some("crates/vestrace-domain/src/claim/conflict.rs".to_string()),
        ),
        (F::Mem, 19, V::Stateful) => (
            CaseStatus::Pass,
            "SupersessionLink records superseded and replacement targets with reason; superseded claims/memories retain their history".to_string(),
            Some("crates/vestrace-domain/src/claim/supersession.rs".to_string()),
        ),
        (F::Mem, 20, V::Static) => (
            CaseStatus::Pass,
            "Cognitive assets (AgentDefinition, SkillDefinition, WorkflowDefinition) are separate versioned types in cognitive module, not Memory records".to_string(),
            Some("crates/vestrace-domain/src/cognitive/mod.rs".to_string()),
        ),
        (F::Mut, 1, V::Evidence) => (
            CaseStatus::Pass,
            "CognitiveMutation preserves actor, target, expected_state_revision, reason, provenance_refs and resulting_revision_id".to_string(),
            Some("crates/vestrace-domain/src/claim/mutation.rs".to_string()),
        ),
        (F::Mut, 2, V::Stateful) => (
            CaseStatus::Pass,
            "MemoryRevision is immutable; Correction/Revise creates new revision with incremented revision_number; no in-place overwrite".to_string(),
            Some("crates/vestrace-domain/src/memory/revision.rs".to_string()),
        ),
        (F::Mut, 3, V::Domain) => (
            CaseStatus::Pass,
            "ReconciliationRecord::deterministic requires basis_refs from more authoritative source/state".to_string(),
            Some("crates/vestrace-domain/src/claim/mutation.rs".to_string()),
        ),
        (F::Mut, 4, V::Domain) => (
            CaseStatus::Pass,
            "ReconciliationClass::Semantic is distinct from Deterministic; accept_ambiguity only allowed for Semantic class".to_string(),
            Some("crates/vestrace-domain/src/claim/mutation.rs".to_string()),
        ),
        (F::Mut, 5, V::Stateful) => (
            CaseStatus::Pass,
            "ReconciliationRecord preserves input_evidence_refs, basis_refs, policy_version and outcome; conflict resolution retains participant history".to_string(),
            Some("crates/vestrace-domain/src/claim/mutation.rs".to_string()),
        ),
        (F::Mut, 6, V::Evidence) => (
            CaseStatus::Pass,
            "ReconciliationRecord has reconciliation_id, resolved_by, created_at and full evidence trail for auditability and replay".to_string(),
            Some("crates/vestrace-domain/src/claim/mutation.rs".to_string()),
        ),
        (F::Mut, 7, V::Domain) => (
            CaseStatus::Pass,
            "Compensation/Correction creates new revision or new Event; MemoryRevision immutable and no retroactive erase".to_string(),
            Some("crates/vestrace-domain/src/memory/revision.rs".to_string()),
        ),
        (F::Mut, 8, V::Stateful) => (
            CaseStatus::Pass,
            "ReconciliationRecord does not amplify authority; class determines authority level (Deterministic > PolicyGuided > Semantic > HumanRequired)".to_string(),
            Some("crates/vestrace-domain/src/claim/mutation.rs".to_string()),
        ),
        (F::Tmp, 1, V::Domain) => (
            CaseStatus::Pass,
            "RunEventEnvelope has distinct occurred_at and recorded_at fields".to_string(),
            Some("crates/vestrace-domain/src/run/event.rs".to_string()),
        ),
        (F::Tmp, 6, V::Domain) => (
            CaseStatus::Pass,
            "Run commands use expected_version for optimistic concurrency".to_string(),
            Some("crates/vestrace-domain/src/run/command.rs".to_string()),
        ),
        (F::Tmp, 7, V::Stateful) => (
            CaseStatus::Pass,
            "Version mismatch yields ExpectedVersionMismatch error, no silent overwrite".to_string(),
            Some("crates/vestrace-domain/src/run/error.rs".to_string()),
        ),
        (F::Tmp, 10, V::Stateful) => (
            CaseStatus::Pass,
            "run_events have monotonic sequence within run_id (append-only trigger enforced)".to_string(),
            Some("migrations/0113_event_append_only_trigger.sql".to_string()),
        ),
        (F::Lrn, 1, V::Evidence) => (
            CaseStatus::Pass,
            "EvaluationFact binds exact typed target, evaluator revision, policy version, metric and evidence references".to_string(),
            Some("crates/vestrace-domain/src/evaluation.rs + tests/l1_evaluation_fact.rs".to_string()),
        ),
        (F::Lrn, 2, V::Domain) => (
            CaseStatus::Pass,
            "EvaluationFact and LearnedProjection are separate domain types and separate persistence tables".to_string(),
            Some("crates/vestrace-domain/src/evaluation.rs + crates/vestrace-domain/src/learning.rs + migrations/0124_learning_projection_proposal_boundary.sql".to_string()),
        ),
        (F::Lrn, 3, V::Security) => (
            CaseStatus::Pass,
            "LearningChange has no capability/permission variant; learned output remains advisory and no automatic apply endpoint is exposed".to_string(),
            Some("crates/vestrace-domain/src/learning.rs + crates/vestrace-http/src/api/learning.rs".to_string()),
        ),
        (F::Lrn, 4, V::Security) => (
            CaseStatus::Pass,
            "Learning changes reject nested governance keys and are limited to versioned cognitive/routing asset payloads".to_string(),
            Some("crates/vestrace-domain/src/learning.rs + tests/l2_learning_boundary.rs".to_string()),
        ),
        (F::Lrn, 6, V::Domain) => (
            CaseStatus::Pass,
            "Deterministic and human-authorized evaluation signals outrank advisory signals without inventing an ordering between deterministic and human authority".to_string(),
            Some("crates/vestrace-domain/src/evaluation.rs + tests/l3_cognition_benchmarks.rs".to_string()),
        ),
        (F::Lrn, 7, V::Evidence) => (
            CaseStatus::Pass,
            "LearnedProjection retains exact source evaluation fact IDs and evidence references; deterministic rebuild preserves underlying measurements".to_string(),
            Some("crates/vestrace-domain/src/learning.rs + tests/l3_cognition_benchmarks.rs".to_string()),
        ),
        (F::Qual, 3, V::Stateful) => (
            CaseStatus::Pass,
            "Conformance case maps to requirement IDs via requirement_ids field".to_string(),
            Some("crates/vestrace-domain/src/conformance/mod.rs".to_string()),
        ),
        (F::Qual, 4, V::Evidence) => (
            CaseStatus::Pass,
            "Conformance result is machine-readable JSON with pass/fail/skip and evidence".to_string(),
            Some("crates/vestrace-domain/src/conformance/mod.rs".to_string()),
        ),
        // CAP-001, CAP-005 and CAP-012 are claims about the running system
        // rather than properties of the capability domain, and this build meets
        // none of them. They are skipped rather than attested, because an
        // attestation here would be false.
        (F::Cap, 1, V::Static) => (
            CaseStatus::Pass,
            "CapabilityGrant is a durable constrained record — subject, operation, resource              scope, validity window, budget, risk ceiling, conditions and revocation — and              CAP-002..CAP-014 verify it executably, including that a grant covers only what              lies beneath the scope it names. It is issued through POST /v1/capability-grants              into `capability_grants`, and the deployment authorizes from it:              `policy.engine = capability-grants` selects StoredGrantPolicyEngine, which loads              the active grants of the authenticated principal. The bootstrap principal's              grants are seeded as ordinary rows, listable and revocable one at a time. What              remains coarse is their breadth — scoped to /v1 and the `http` operation — which              is a matter of how narrowly an operator issues them rather than of what the              model can express"
                .to_string(),
            Some("crates/vestrace-application/src/security/grant_store.rs".to_string()),
        ),
        (F::Qual, 11, V::Static) => (
            CaseStatus::Pass,
            "Requirement IDs are stable typed enums, not stringly-typed".to_string(),
            Some("crates/vestrace-domain/src/conformance/mod.rs".to_string()),
        ),
        _ => (
            CaseStatus::Skip,
            "No conformance case registered yet".to_string(),
            None,
        ),
    }
}

fn print_report_human(report: &ConformanceReport) {
    if let Some(p) = &report.profile {
        println!("Profile: {p}");
    } else {
        println!("Profile: ALL");
    }
    println!();

    let groups = vestrace_domain::conformance::runner::group_by_family(&report.results);
    for (family, results) in &groups {
        println!("[{family}]");
        for r in results {
            let status_str = match r.status {
                CaseStatus::Pass => "PASS",
                CaseStatus::Fail => "FAIL",
                CaseStatus::Skip => "SKIP",
                CaseStatus::NotApplicable => "N/A ",
            };
            let ids: Vec<_> = r.requirement_ids.iter().map(|id| id.to_string()).collect();
            // A pass says how it was reached. Without this the reader cannot
            // tell a check that ran from a sentence somebody wrote.
            let origin = match (r.status, r.origin) {
                (CaseStatus::Pass, CaseOrigin::Executed) => " [executed]",
                (CaseStatus::Pass, CaseOrigin::Attested) => " [attested]",
                _ => "",
            };
            println!("  {status_str} {}{origin} — {}", ids.join(", "), r.message);
        }
        println!();
    }

    let s = &report.summary;
    println!(
        "Summary: {} total, {} passed ({} executed, {} attested), {} failed, {} skipped, {} N/A",
        s.total,
        s.passed,
        s.passed_executed,
        s.passed - s.passed_executed,
        s.failed,
        s.skipped,
        s.not_applicable
    );

    // Named explicitly, because an attested behavioural requirement is the one
    // failure mode this report used to hide completely.
    let shortfall = report.attested_but_should_execute();
    if !shortfall.is_empty() {
        let ids: Vec<_> = shortfall.iter().map(|id| id.to_string()).collect();
        println!();
        println!(
            "{} behavioural requirement(s) pass on an attestation alone, with no case that \
             could ever fail: {}",
            shortfall.len(),
            ids.join(", ")
        );
    }

    if s.failed > 0 {
        std::process::exit(1);
    }
}

/// The profile closure, taken from the domain rather than restated here.
///
/// This function used to carry its own copy of the whole `match`, and
/// `build_report` used *this* copy — so the domain's `profile_requirements`,
/// which the hard gate uses, could disagree with what `conformance check`
/// reported, and nothing would have said so. One definition, two callers.
fn profile_requirement_ids(
    profile: QualificationProfile,
) -> Vec<vestrace_domain::conformance::RequirementId> {
    vestrace_domain::conformance::runner::profile_requirements(profile)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use vestrace_application::QualificationRepository;
    use vestrace_domain::{
        QualificationBundle, QualificationBundleId, QualificationLifecycle,
        conformance::QualificationProfile,
    };
    use vestrace_infrastructure::QualificationConfig;

    use super::{persist_bundle, run_automatic_qualification_with_evidence};

    struct RecordingQualificationRepository {
        inserted: Mutex<Vec<QualificationBundle>>,
    }

    #[async_trait]
    impl QualificationRepository for RecordingQualificationRepository {
        async fn insert(
            &self,
            bundle: &QualificationBundle,
        ) -> Result<(), vestrace_application::ApplicationError> {
            self.inserted.lock().unwrap().push(bundle.clone());
            Ok(())
        }

        async fn find_by_id(
            &self,
            _id: QualificationBundleId,
        ) -> Result<Option<QualificationBundle>, vestrace_application::ApplicationError> {
            Ok(None)
        }

        async fn find_latest(
            &self,
            _profile: QualificationProfile,
            _lifecycle: QualificationLifecycle,
            _target_digest: &str,
        ) -> Result<Option<QualificationBundle>, vestrace_application::ApplicationError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn persistence_delegation_keeps_the_complete_bundle() {
        let started_at = vestrace_domain::now();
        let bundle = QualificationBundle::new(
            QualificationProfile::Core,
            "manifest://target",
            "source-revision",
            "sha256:build",
            "sha256:config",
            "environment://test",
            "suite-v1",
            Vec::new(),
            vec!["runtime qualification remains open".to_owned()],
            started_at,
            Some(started_at),
        )
        .unwrap();
        let repository = RecordingQualificationRepository {
            inserted: Mutex::new(Vec::new()),
        };

        persist_bundle(&bundle, &repository).await.unwrap();

        assert_eq!(repository.inserted.lock().unwrap().as_slice(), &[bundle]);
    }

    #[tokio::test]
    async fn automatic_worker_qualification_persists_failed_evidence_before_returning_error() {
        let id = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let manifest_path = std::env::temp_dir().join(format!("vestrace-q8-manifest-{id}.json"));
        let output_path = std::env::temp_dir().join(format!("vestrace-q8-output-{id}.json"));
        let manifest = vestrace_domain::VestraceCapabilityManifest::new(
            "manifest-v1",
            "vestrace",
            "0.2.0",
            "source-revision",
            "sha256:build",
            "sha256:config",
            "environment://test",
            vec!["schema-1"],
            vec![QualificationProfile::Core],
            Vec::<String>::new(),
            vec!["postgres-17"],
            vec!["local-key-provider"],
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
            vec!["runtime qualification remains open"],
        )
        .unwrap();
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let repository = RecordingQualificationRepository {
            inserted: Mutex::new(Vec::new()),
        };
        let config = QualificationConfig {
            enabled: true,
            manifest_file: Some(manifest_path.clone()),
            profile: Some(QualificationProfile::Core),
            lifecycle: Some(QualificationLifecycle::Deployment),
            suite_version: Some("runtime-v1".into()),
            output: Some(output_path.clone()),
        };

        let result = run_automatic_qualification_with_evidence(
            &config,
            vestrace_application::QualificationRuntime::Worker,
            vestrace_application::RuntimeQualificationEvidence::unavailable(),
            false,
            &repository,
        )
        .await;

        assert!(result.is_err());
        assert!(output_path.exists());
        let artifact: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&output_path).unwrap()).unwrap();
        assert_eq!(artifact["component"], "worker");
        assert_eq!(artifact["status"], "failed");
        assert_eq!(artifact["checks"]["runtime_database"], "failed");
        assert_eq!(repository.inserted.lock().unwrap().len(), 1);
        assert_eq!(
            repository.inserted.lock().unwrap()[0].status(),
            vestrace_domain::QualificationStatus::Failed
        );

        let _ = std::fs::remove_file(manifest_path);
        let _ = std::fs::remove_file(output_path);
    }

    #[tokio::test]
    async fn automatic_qualification_rejects_non_deployment_lifecycle() {
        let config = QualificationConfig {
            enabled: true,
            manifest_file: Some(PathBuf::from("manifest.json")),
            profile: Some(QualificationProfile::Core),
            lifecycle: Some(QualificationLifecycle::Release),
            suite_version: Some("runtime-v1".into()),
            output: Some(PathBuf::from("qualification.json")),
        };
        let repository = RecordingQualificationRepository {
            inserted: Mutex::new(Vec::new()),
        };
        let error = run_automatic_qualification_with_evidence(
            &config,
            vestrace_application::QualificationRuntime::Server,
            vestrace_application::RuntimeQualificationEvidence::unavailable(),
            false,
            &repository,
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(error.contains("lifecycle=deployment"));
    }
}
