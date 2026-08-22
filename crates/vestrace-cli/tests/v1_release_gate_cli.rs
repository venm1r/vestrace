//! The v1.0 release gate, asked from the outside.
//!
//! `V1ReleaseEvidenceService` decides whether an exact build in an exact
//! environment may be released, and until this command existed it was reachable
//! from no binary: the decision could be asserted in a document and checked by
//! nobody. These cases ask the gate the way an operator asks it, and the first
//! one pins the answer that matters most right now — which evidence sources
//! have no producer at all.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    FaultSuiteEvidenceRepository, QualificationBaselineRepository, RecoveryRepository,
};
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::{QualificationProfile, RequirementFamily, RequirementId};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation, evaluate_fault_suite};
use vestrace_domain::now;
use vestrace_domain::trust::{
    QualificationBaseline, QualificationBundle, TrustState, TrustStateRecord,
};
use vestrace_domain::{HealthScope, WorkspaceId};
use vestrace_infrastructure::{PgQualificationBaselineRepository, PgRecoveryRepository, PgStore};

fn test_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn manifest(profile: QualificationProfile) -> VestraceCapabilityManifest {
    VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "1.0.0",
        "source-revision",
        "sha256:build",
        "sha256:config",
        "environment://release",
        vec!["schema-1"],
        vec![profile],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-file"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["crypto custody is local-file"],
    )
    .unwrap()
}

fn evidence() -> Vec<HardGateEvidence> {
    vec![HardGateEvidence::pass(
        RequirementId::new(RequirementFamily::Arc, 1),
        "crates/vestrace-domain/src/execution/mod.rs",
        None,
        EvidenceOrigin::LocalExecutable,
    )]
}

/// A bundle that genuinely satisfies the profile it names.
///
/// `bundle_for` carries one piece of evidence, which is enough for the tests
/// that exercise refusals and nowhere near enough for one that expects approval
/// to succeed: `ReleaseApprovalService` checks that the bundle passed and that
/// the profile's gate is satisfied, and a single ARC-001 result satisfies
/// neither. Kept separate so the refusal fixtures stay as small as they are.
fn fully_qualified_bundle_for(
    manifest: &VestraceCapabilityManifest,
    profile: QualificationProfile,
) -> QualificationBundle {
    let results = vestrace_domain::conformance::runner::profile_requirements(profile)
        .into_iter()
        .map(
            |requirement_id| vestrace_domain::conformance::ConformanceCaseResult {
                case_id: format!("release-gate-{requirement_id}"),
                requirement_ids: vec![requirement_id],
                status: vestrace_domain::conformance::CaseStatus::Pass,
                message: "passed".into(),
                evidence: Some(format!("test://{requirement_id}")),
                origin: vestrace_domain::conformance::CaseOrigin::Executed,
            },
        )
        .collect();

    QualificationBundle::from_conformance_report(
        vestrace_domain::trust::QualificationLifecycle::Release,
        profile,
        manifest.manifest_digest(),
        manifest.source_revision(),
        manifest.build_digest(),
        manifest.configuration_digest(),
        manifest.environment_manifest(),
        "suite-v1",
        vestrace_domain::conformance::ConformanceReport::from_results(Some(profile), results),
        // The trusted gate evaluates the governance/federation hard gate over
        // the bundle's evidence once its other four checks pass, so an empty
        // list fails it however complete the conformance report is.
        vestrace_domain::conformance::gate::GovernanceFederationGate::required_requirements(
            profile,
        )
        .into_iter()
        .map(|requirement_id| {
            HardGateEvidence::pass(
                requirement_id,
                format!("test://{requirement_id}"),
                // Some requirements demand a policy version and the gate
                // reports a missing one as a failure. Supplying it for all of
                // them is harmless: the check only looks where it is required.
                Some("policy-v1".to_owned()),
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect(),
        manifest.known_limitations().to_vec(),
        now(),
        Some(now()),
    )
    .unwrap()
}

fn bundle_for(
    manifest: &VestraceCapabilityManifest,
    profile: QualificationProfile,
) -> QualificationBundle {
    QualificationBundle::new(
        profile,
        manifest.manifest_digest(),
        manifest.source_revision(),
        manifest.build_digest(),
        manifest.configuration_digest(),
        manifest.environment_manifest(),
        "suite-v1",
        evidence(),
        manifest.known_limitations().to_vec(),
        now(),
        Some(now()),
    )
    .unwrap()
}

fn write_pair(
    id: &str,
    manifest: &VestraceCapabilityManifest,
    bundle: &QualificationBundle,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let manifest_path = std::env::temp_dir().join(format!("vestrace-release-manifest-{id}.json"));
    let bundle_path = std::env::temp_dir().join(format!("vestrace-release-bundle-{id}.json"));
    fs::write(&manifest_path, serde_json::to_vec_pretty(manifest).unwrap()).unwrap();
    fs::write(&bundle_path, serde_json::to_vec_pretty(bundle).unwrap()).unwrap();
    (manifest_path, bundle_path)
}

fn run_release(
    manifest_path: &std::path::Path,
    bundle_path: &std::path::Path,
    profile: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            profile,
            "--json",
        ])
        .output()
        .unwrap()
}

fn mounted_store_root(id: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vestrace-release-approval-store-{id}"));
    fs::create_dir_all(root.join("release-signing").join("v1")).unwrap();
    fs::write(root.join("release-signing").join("scope"), "release").unwrap();
    fs::write(root.join("release-signing").join("purpose"), "signing").unwrap();
    fs::write(root.join("release-signing").join("algorithm"), "ed25519").unwrap();
    fs::write(
        root.join("release-signing").join("v1").join("state"),
        "active",
    )
    .unwrap();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    fs::write(
        root.join("release-signing")
            .join("v1")
            .join("private.pkcs8"),
        pkcs8.as_ref(),
    )
    .unwrap();
    fs::write(
        root.join("release-signing").join("v1").join("public.bin"),
        pair.public_key().as_ref(),
    )
    .unwrap();
    root
}

fn sign_release_artifact(artifact: &str, input: &Path, output: &Path, store_root: &Path) {
    let signed = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "sign",
            "--artifact",
            artifact,
            "--artifact-file",
            input.to_str().unwrap(),
            "--key-provider",
            "mounted-secret-store",
            "--key-store-root",
            store_root.to_str().unwrap(),
            "--signer-identity",
            "issuer://release",
            "--key-id",
            "release-signing",
            "--key-version",
            "v1",
            "--key-scope",
            "release",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        signed.status.success(),
        "failed to sign {artifact}: {}",
        String::from_utf8_lossy(&signed.stderr)
    );
}

fn signed_release_pair(
    id: &str,
    manifest: &VestraceCapabilityManifest,
    bundle: &QualificationBundle,
    store_root: &Path,
) -> (PathBuf, PathBuf) {
    let (unsigned_manifest, unsigned_bundle) = write_pair(id, manifest, bundle);
    let signed_manifest =
        std::env::temp_dir().join(format!("vestrace-release-signed-manifest-{id}.json"));
    let signed_bundle =
        std::env::temp_dir().join(format!("vestrace-release-signed-bundle-{id}.json"));
    sign_release_artifact("manifest", &unsigned_manifest, &signed_manifest, store_root);
    sign_release_artifact("bundle", &unsigned_bundle, &signed_bundle, store_root);
    fs::remove_file(unsigned_manifest).ok();
    fs::remove_file(unsigned_bundle).ok();
    (signed_manifest, signed_bundle)
}

fn run_release_with_approval(
    manifest_path: &Path,
    bundle_path: &Path,
    store_root: &Path,
    workspace_id: WorkspaceId,
    trusted_signer: &str,
    database_url: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            "trusted",
            "--release-approval",
            "--key-store-root",
            store_root.to_str().unwrap(),
            "--key-id",
            "release-signing",
            "--trust-workspace-id",
            &workspace_id.to_string(),
            "--trusted-signer",
            trusted_signer,
            "--json",
        ])
        .env("VESTRACE_DATABASE__URL", database_url)
        .output()
        .unwrap()
}

fn run_release_approval_without_database(
    manifest_path: &Path,
    bundle_path: &Path,
    store_root: &Path,
    workspace_id: WorkspaceId,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            "trusted",
            "--release-approval",
            "--key-store-root",
            store_root.to_str().unwrap(),
            "--key-id",
            "release-signing",
            "--trust-workspace-id",
            &workspace_id.to_string(),
            "--trusted-signer",
            "issuer://release",
            "--json",
        ])
        .env_remove("DATABASE_URL")
        .env_remove("VESTRACE_DATABASE__URL")
        .output()
        .unwrap()
}

fn tamper_signature(path: &Path) {
    let mut artifact: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let signature = artifact["signature"]["signature"].as_str().unwrap();
    let replacement = if signature.starts_with('A') { 'B' } else { 'A' };
    artifact["signature"]["signature"] = Value::String(format!("{replacement}{}", &signature[1..]));
    fs::write(path, serde_json::to_vec_pretty(&artifact).unwrap()).unwrap();
}

async fn insert_release_approval_inputs(
    pool: &PgPool,
    bundle: &QualificationBundle,
    workspace_id: WorkspaceId,
    insert_baseline: bool,
    insert_trust: bool,
) {
    let store = PgStore::from_pool(pool.clone());
    if insert_baseline {
        PgQualificationBaselineRepository::new(store.clone())
            .insert(&QualificationBaseline::from_bundle(bundle, now()).unwrap())
            .await
            .unwrap();
    }
    if insert_trust {
        PgRecoveryRepository::new(store)
            .insert_trust_state(
                &TrustStateRecord::new(
                    HealthScope::workspace(workspace_id),
                    TrustState::Trusted,
                    "trusted for release approval test",
                    None,
                    now(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
}

fn run_release_collecting_runtime_evidence(
    manifest_path: &std::path::Path,
    bundle_path: &std::path::Path,
    database_url: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            "trusted",
            "--runtime-evidence",
            "--json",
        ])
        .env("VESTRACE_DATABASE__URL", database_url)
        .output()
        .unwrap()
}

fn run_release_with_fault_suite_evidence(
    manifest_path: &std::path::Path,
    bundle_path: &std::path::Path,
    evidence_id: uuid::Uuid,
    database_url: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            "trusted",
            "--fault-suite-evidence",
            &evidence_id.to_string(),
            "--json",
        ])
        .env("VESTRACE_DATABASE__URL", database_url)
        .output()
        .unwrap()
}

fn failures(output: &std::process::Output) -> Vec<String> {
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "release report is not JSON: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    report["failures"]
        .as_array()
        .expect("release report has no failures array")
        .iter()
        .map(|failure| failure.as_str().unwrap().to_owned())
        .collect()
}

/// The honest answer, machine-checked rather than written down: five evidence
/// sources are produced by nothing in this build, and a sixth is only collected
/// when the operator asks for it against a live database.
#[test]
fn the_release_gate_names_every_evidence_source_that_has_no_producer() {
    let id = test_suffix();
    let manifest = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&manifest, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &manifest, &bundle);

    let output = run_release(&manifest_path, &bundle_path, "trusted");

    assert!(
        !output.status.success(),
        "a release with no approval, no crypto and no fault evidence must not pass"
    );
    let mut observed = failures(&output);
    observed.sort();
    assert_eq!(
        observed,
        vec![
            "capability_restoration_missing".to_owned(),
            "crypto_qualification_missing".to_owned(),
            "fault_suite_missing".to_owned(),
            "recovery_qualification_missing".to_owned(),
            "release_approval_missing".to_owned(),
            "runtime_qualification_missing".to_owned(),
        ],
        "the gate must name exactly the evidence it does not have"
    );

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

/// The evidence identity is read from the bundle and never from the manifest a
/// second time. Reading it twice from the same file would make this comparison
/// a tautology: every release would agree with itself about which build it
/// qualified, including one qualified against a different build entirely.
#[test]
fn a_bundle_qualified_against_another_build_is_refused() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let other_build = VestraceCapabilityManifest::new(
        "manifest-v1",
        "vestrace",
        "1.0.0",
        "another-source-revision",
        "sha256:another-build",
        "sha256:config",
        "environment://release",
        vec!["schema-1"],
        vec![QualificationProfile::Trusted],
        Vec::<String>::new(),
        vec!["postgres-17"],
        vec!["local-file"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
        vec!["crypto custody is local-file"],
    )
    .unwrap();
    let foreign_bundle = bundle_for(&other_build, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &foreign_bundle);

    let output = run_release(&manifest_path, &bundle_path, "trusted");

    assert!(
        failures(&output).contains(&"release_identity_mismatch".to_owned()),
        "a bundle that qualified another build must not release this one: {:?}",
        failures(&output)
    );

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

/// "Nobody collected it" and "we tried to collect it and could not" are
/// different facts about a release, and only the first one is `missing`.
///
/// Reporting an unreachable database as absent evidence would let a release
/// read as merely incomplete when the environment it claims to qualify could
/// not be reached at all — so an operator who asked for runtime evidence and
/// did not get it is told, rather than handed a report.
#[test]
fn runtime_evidence_that_was_asked_for_and_not_collected_is_an_error_not_a_silence() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &bundle);

    let output = run_release_collecting_runtime_evidence(
        &manifest_path,
        &bundle_path,
        "postgres://unreachable:unreachable@127.0.0.1:9/vestrace_unreachable",
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("runtime evidence could not be collected"),
        "the operator must be told the environment was unreachable, got stderr: {stderr}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "a release report that could not read the environment must not be published"
    );
    assert!(
        !stderr.contains("unreachable@"),
        "the failure must not leak database credentials: {stderr}"
    );

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

async fn ephemeral_database_url(pool: &PgPool) -> String {
    let base = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must name the PostgreSQL used by #[sqlx::test]");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    let (base, query) = match base.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (base.as_str(), None),
    };
    let (prefix, _) = base.rsplit_once('/').unwrap();
    match query {
        Some(query) => format!("{prefix}/{database}?{query}"),
        None => format!("{prefix}/{database}"),
    }
}

fn observation_payload(observation: &FaultObservation) -> Value {
    serde_json::json!({
        "point": observation.point,
        "status": observation.status,
        "retry_attempted": observation.retry_attempted,
        "reconciliation_started": observation.reconciliation_started,
        "receipt_persisted": observation.receipt_persisted,
    })
}

async fn insert_fault_evidence_directly(
    pool: &PgPool,
    target_digest: &str,
    stored_passed: bool,
    observations: &[FaultObservation],
) -> Uuid {
    let id = Uuid::now_v7();
    let created_at = now();
    let payload = serde_json::json!({
        "id": id,
        "target_digest": target_digest,
        "observations": observations.iter().map(observation_payload).collect::<Vec<_>>(),
        "passed": stored_passed,
        "failures": Vec::<String>::new(),
        "created_at": created_at,
    });
    sqlx::query(
        "INSERT INTO external_effect_fault_suite_evidence (
             id, target_digest, passed, payload, created_at
         ) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(target_digest)
    .bind(stored_passed)
    .bind(payload)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn required_observations() -> Vec<FaultObservation> {
    EffectFaultPoint::required_points()
        .into_iter()
        .map(FaultObservation::expected)
        .collect()
}

fn scenario_binary() -> std::path::PathBuf {
    let test_binary = std::env::current_exe().expect("current test binary path");
    let name = format!("vestrace-fault-scenario{}", std::env::consts::EXE_SUFFIX);
    let mut directory = test_binary.parent();
    while let Some(candidate) = directory {
        let path = candidate.join(&name);
        if path.is_file() {
            return path;
        }
        directory = candidate.parent();
    }
    panic!(
        "{name} was not found beside {}; build it first with `cargo build -p \
         vestrace-fault-scenario`",
        test_binary.display()
    );
}

#[test]
fn fault_evidence_that_was_named_but_cannot_be_loaded_is_an_error_not_a_silence() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &bundle);

    let output = run_release_with_fault_suite_evidence(
        &manifest_path,
        &bundle_path,
        uuid::Uuid::now_v7(),
        "postgres://fault-user:fault-secret@127.0.0.1:9/vestrace_unreachable",
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fault-suite evidence could not be loaded"),
        "{stderr}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "a release report must not be published when named evidence could not be read"
    );
    assert!(!stderr.contains("fault-user"), "{stderr}");
    assert!(!stderr.contains("fault-secret"), "{stderr}");

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

/// A bundle earns a profile by passing that profile's requirements. Accepting
/// one profile's evidence for another would let the smallest suite release the
/// largest claim.
#[test]
fn a_bundle_for_another_profile_is_refused() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let core_bundle = bundle_for(&released, QualificationProfile::Core);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &core_bundle);

    let output = run_release(&manifest_path, &bundle_path, "trusted");

    assert!(
        failures(&output).contains(&"profile_mismatch".to_owned()),
        "a core bundle must not release a trusted claim: {:?}",
        failures(&output)
    );

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_gate_rederives_a_directly_inserted_stale_passing_verdict(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &bundle);
    let database_url = ephemeral_database_url(&pool).await;
    let mut observations = required_observations();
    observations[0].retry_attempted = true;
    let evidence_id =
        insert_fault_evidence_directly(&pool, released.manifest_digest(), true, &observations)
            .await;

    let output = run_release_with_fault_suite_evidence(
        &manifest_path,
        &bundle_path,
        evidence_id,
        &database_url,
    );

    assert!(!output.status.success());
    let observed = failures(&output);
    assert!(
        observed.contains(&"fault_suite_failed".to_owned()),
        "{observed:?}"
    );
    assert!(
        !observed.contains(&"fault_suite_missing".to_owned()),
        "{observed:?}"
    );

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_gate_refuses_fault_evidence_bound_to_another_target(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &bundle);
    let database_url = ephemeral_database_url(&pool).await;
    let evidence_id = insert_fault_evidence_directly(
        &pool,
        "sha256:another-target",
        true,
        &required_observations(),
    )
    .await;

    let output = run_release_with_fault_suite_evidence(
        &manifest_path,
        &bundle_path,
        evidence_id,
        &database_url,
    );

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not match requested target"),
        "{stderr}"
    );
    assert!(stderr.contains("sha256:another-target"), "{stderr}");
    assert!(stderr.contains(released.manifest_digest()), "{stderr}");

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}

#[test]
fn release_approval_requires_mounted_key_store_coordinates_before_reporting() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &bundle);
    let workspace_id = WorkspaceId::new();

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            "trusted",
            "--release-approval",
            "--trust-workspace-id",
            &workspace_id.to_string(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "a refused request must not report a release"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--key-store-root"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_file(manifest_path).ok();
    fs::remove_file(bundle_path).ok();
}

#[test]
fn release_approval_requires_an_independently_configured_trusted_signer_before_reporting() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    let workspace_id = WorkspaceId::new();

    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "release",
            "--manifest-file",
            manifest_path.to_str().unwrap(),
            "--bundle-file",
            bundle_path.to_str().unwrap(),
            "--profile",
            "trusted",
            "--release-approval",
            "--key-store-root",
            store_root.to_str().unwrap(),
            "--key-id",
            "release-signing",
            "--trust-workspace-id",
            &workspace_id.to_string(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "a refused request must not report a release"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--trusted-signer"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[test]
fn release_approval_refuses_a_revoked_mounted_public_key_before_database_access() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    fs::write(
        store_root.join("release-signing").join("v1").join("state"),
        "revoked",
    )
    .unwrap();

    let output = run_release_approval_without_database(
        &manifest_path,
        &bundle_path,
        &store_root,
        WorkspaceId::new(),
    );

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "a refused request must not report a release"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mounted signing public key"), "{stderr}");
    assert!(!stderr.contains("database.url"), "{stderr}");

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[test]
fn release_approval_verifies_with_public_material_when_private_pkcs8_is_absent() {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    fs::remove_file(
        store_root
            .join("release-signing")
            .join("v1")
            .join("private.pkcs8"),
    )
    .unwrap();

    let output = run_release_approval_without_database(
        &manifest_path,
        &bundle_path,
        &store_root,
        WorkspaceId::new(),
    );

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "a refused request must not report a release"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing configuration field \"database.url\""),
        "{stderr}"
    );
    assert!(!stderr.contains("private"), "{stderr}");
    assert!(!stderr.contains("key material is unreadable"), "{stderr}");

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_uses_signed_artifacts_and_persisted_matching_inputs(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = fully_qualified_bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &bundle, workspace_id, true, true).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://release",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    // The report carries `release_approval_reason`; printing only the failure
    // names would say "approval failed" about nineteen distinct checks and
    // leave whoever reads it no way to act.
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        !observed.contains(&"release_approval_missing".to_owned()),
        "{observed:?}\n{report}"
    );
    assert!(
        !observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}\n{report}"
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_marks_a_missing_baseline_as_failed_and_names_the_lookup(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &bundle, workspace_id, false, true).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://release",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    assert!(
        observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}"
    );
    assert!(
        !observed.contains(&"release_approval_missing".to_owned()),
        "{observed:?}"
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let reason = report["release_approval_reason"].as_str().unwrap();
    // The lookup is by the bundle's target digest, which is what
    // `QualificationBaseline::from_bundle` records — not by the manifest
    // digest, which is a different value and which no published baseline is
    // ever keyed on.
    assert!(reason.contains(bundle.target_digest()), "{reason}");
    assert!(reason.contains("trusted"), "{reason}");

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_does_not_use_another_profiles_baseline_for_the_same_target(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let trusted_bundle = bundle_for(&released, QualificationProfile::Trusted);
    let core_bundle = bundle_for(&released, QualificationProfile::Core);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) =
        signed_release_pair(&id, &released, &trusted_bundle, &store_root);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &core_bundle, workspace_id, true, true).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://release",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    assert!(
        observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}"
    );
    assert!(
        !observed.contains(&"release_approval_missing".to_owned()),
        "{observed:?}"
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["release_approval_reason"]
            .as_str()
            .unwrap()
            .contains("trusted")
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_marks_a_missing_trust_state_as_failed_and_names_the_scope(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &bundle, workspace_id, true, false).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://release",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    assert!(
        observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}"
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["release_approval_reason"]
            .as_str()
            .unwrap()
            .contains(&workspace_id.to_string())
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_uses_mounted_store_verification_for_a_manifest_signature(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    tamper_signature(&manifest_path);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &bundle, workspace_id, true, true).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://release",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    assert!(
        observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}"
    );
    assert!(
        !observed.contains(&"release_approval_missing".to_owned()),
        "{observed:?}"
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_uses_mounted_store_verification_for_a_bundle_signature(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    tamper_signature(&bundle_path);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &bundle, workspace_id, true, true).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://release",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    assert!(
        observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}"
    );
    assert!(
        !observed.contains(&"release_approval_missing".to_owned()),
        "{observed:?}"
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_rejects_a_valid_signature_from_an_untrusted_identity(pool: PgPool) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let store_root = mounted_store_root(&id);
    let (manifest_path, bundle_path) = signed_release_pair(&id, &released, &bundle, &store_root);
    let workspace_id = WorkspaceId::new();
    insert_release_approval_inputs(&pool, &bundle, workspace_id, true, true).await;

    let output = run_release_with_approval(
        &manifest_path,
        &bundle_path,
        &store_root,
        workspace_id,
        "issuer://untrusted",
        &ephemeral_database_url(&pool).await,
    );
    let observed = failures(&output);
    assert!(
        observed.contains(&"release_approval_failed".to_owned()),
        "{observed:?}"
    );
    assert!(
        !observed.contains(&"release_approval_missing".to_owned()),
        "{observed:?}"
    );

    for path in [manifest_path, bundle_path] {
        fs::remove_file(path).ok();
    }
    fs::remove_dir_all(store_root).ok();
}

/// This is the qualification path operators run deliberately: it needs a real
/// PostgreSQL 17 deployment and a separately built program that aborts five
/// child processes. Its expected failed verdict is evidence, not a test defect.
#[sqlx::test(migrations = "../../migrations")]
#[ignore = "needs PostgreSQL 17 and the separately built fault-scenario program"]
async fn real_fault_suite_evidence_changes_missing_to_failed_without_changing_the_verdict(
    pool: PgPool,
) {
    let id = test_suffix();
    let released = manifest(QualificationProfile::Trusted);
    let bundle = bundle_for(&released, QualificationProfile::Trusted);
    let (manifest_path, bundle_path) = write_pair(&id, &released, &bundle);
    let database_url = ephemeral_database_url(&pool).await;
    let scenario = scenario_binary();

    let produced = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "fault-suite",
            "--program",
            scenario.to_str().unwrap(),
            "--target-digest",
            released.manifest_digest(),
            "--isolation",
            "ephemeral",
        ])
        .env("VESTRACE_DATABASE__URL", &database_url)
        .output()
        .unwrap();
    assert!(
        produced.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&produced.stdout),
        String::from_utf8_lossy(&produced.stderr)
    );
    let produced_stdout = String::from_utf8(produced.stdout).unwrap();
    let evidence_id = produced_stdout
        .split_whitespace()
        .last()
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("producer must print the persisted evidence id");
    let evidence = vestrace_infrastructure::PgFaultSuiteEvidenceRepository::new(
        PgStore::from_pool(pool.clone()),
    )
    .find_by_id(evidence_id)
    .await
    .unwrap()
    .expect("printed evidence id must name the inserted row");
    assert_eq!(evidence.target_digest(), released.manifest_digest());
    assert_eq!(evidence.observations().len(), 5);
    assert!(!evidence.is_passed());
    assert_eq!(evidence.failures().len(), 2);

    let observations = evidence
        .observations()
        .iter()
        .map(FaultObservation::from)
        .collect::<Vec<_>>();
    let expected = required_observations();
    for index in [0, 1, 3, 4] {
        assert_eq!(observations[index], expected[index]);
    }
    assert_ne!(observations[2].status, expected[2].status);
    assert_ne!(
        observations[2].reconciliation_started,
        expected[2].reconciliation_started
    );
    assert_eq!(observations[2].retry_attempted, expected[2].retry_attempted);
    assert_eq!(
        observations[2].receipt_persisted,
        expected[2].receipt_persisted
    );
    let current_decision = evaluate_fault_suite(&observations);
    assert!(!current_decision.is_passed());
    assert_eq!(current_decision.failures().len(), 2);

    let with_evidence = run_release_with_fault_suite_evidence(
        &manifest_path,
        &bundle_path,
        evidence_id,
        &database_url,
    );
    let without_evidence = run_release(&manifest_path, &bundle_path, "trusted");
    println!(
        "FAULT_SUITE_EVIDENCE\n{}",
        serde_json::to_string_pretty(&evidence).unwrap()
    );
    println!(
        "RELEASE_WITH_FAULT_SUITE_EVIDENCE\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&with_evidence.stdout),
        String::from_utf8_lossy(&with_evidence.stderr)
    );
    println!(
        "RELEASE_WITHOUT_FAULT_SUITE_EVIDENCE\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&without_evidence.stdout),
        String::from_utf8_lossy(&without_evidence.stderr)
    );

    let with_failures = failures(&with_evidence);
    assert!(with_failures.contains(&"fault_suite_failed".to_owned()));
    assert!(!with_failures.contains(&"fault_suite_missing".to_owned()));
    let without_failures = failures(&without_evidence);
    assert!(without_failures.contains(&"fault_suite_missing".to_owned()));
    assert!(!without_failures.contains(&"fault_suite_failed".to_owned()));

    fs::remove_file(&manifest_path).ok();
    fs::remove_file(&bundle_path).ok();
}
