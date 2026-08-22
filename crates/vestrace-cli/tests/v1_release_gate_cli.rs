//! The v1.0 release gate, asked from the outside.
//!
//! `V1ReleaseEvidenceService` decides whether an exact build in an exact
//! environment may be released, and until this command existed it was reachable
//! from no binary: the decision could be asserted in a document and checked by
//! nobody. These cases ask the gate the way an operator asks it, and the first
//! one pins the answer that matters most right now — which evidence sources
//! have no producer at all.

use std::fs;
use std::process::Command;

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::FaultSuiteEvidenceRepository;
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::{QualificationProfile, RequirementFamily, RequirementId};
use vestrace_domain::external_effects::{EffectFaultPoint, FaultObservation, evaluate_fault_suite};
use vestrace_domain::now;
use vestrace_domain::trust::QualificationBundle;
use vestrace_infrastructure::PgStore;

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
