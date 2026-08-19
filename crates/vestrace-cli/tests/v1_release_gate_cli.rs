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
use vestrace_domain::VestraceCapabilityManifest;
use vestrace_domain::conformance::gate::{EvidenceOrigin, HardGateEvidence};
use vestrace_domain::conformance::{QualificationProfile, RequirementFamily, RequirementId};
use vestrace_domain::now;
use vestrace_domain::trust::QualificationBundle;

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
