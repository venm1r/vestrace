use ring::digest;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const AGGREGATE_SHA256: &str = "762c25f3781f1ba30783e218db64aa4dc9536add490f61255056bfe8635d8594";
const PROVENANCE: &str = "operator-supplied snapshot at E:\\Junk\\AI\\vestrace-docss, registered from the approved spec on 2026-08-26";

#[derive(Clone, Copy)]
struct ExpectedEntry {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
    role: &'static str,
    decision: &'static str,
}

const EXPECTED_ENTRIES: [ExpectedEntry; 12] = [
    ExpectedEntry {
        name: "AMENDMENT-2026-08-19.md",
        bytes: 2987,
        sha256: "798dea48d277c545050e9637adc568a7a6c47aeff836749dfcdbc7c86b298df6",
        role: "proposed post-v1 contract",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "RFC-Model-Request-Reconstruction-and-Context-Surface-2026-08-19.md",
        bytes: 23397,
        sha256: "f90163409eae865c818e572936f936215f955537e56f5d4a5b4b440bdd4ef423",
        role: "proposed post-v1 contract",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-post-v1-gate-roadmap-2026-08-12.md",
        bytes: 62896,
        sha256: "b7a5b959ad1f055dea52b00a1dd9326a69d401199ced3c2a4d919188e018fdeb",
        role: "strategic post-v1 roadmap",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-post-v1-gate-roadmap-artifact-2026-08-12.json",
        bytes: 87553,
        sha256: "e430fa9c20519d43398f2504561d19a8b6fccfb3b10b595543b1658b86f12b0b",
        role: "derived roadmap data companion",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-post-v1-gate-roadmap-report-2026-08-12.html",
        bytes: 517604,
        sha256: "d48231290e0c6ffbb7c81e6e2af283af616e5ee4d68ab2e96bfc9de7876cdf99",
        role: "derived roadmap render companion",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-post-v1-gate-roadmap-source-notes-2026-08-12.md",
        bytes: 2533,
        sha256: "6e6622c914c9ecd20fbea3cd7d8d9b066934e74a1b10faac7c59c140928c5c3c",
        role: "roadmap source notes with missing-reference warning",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-prospective-technologies-artifact-2026-08-12.json",
        bytes: 90605,
        sha256: "62550dfc4e41a371b626166803a6d427a5d2a4330f645dfc2e69c261c25e070d",
        role: "derived research data companion",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-prospective-technologies-report-2026-08-12.html",
        bytes: 624448,
        sha256: "a9f2f93c9846654f8e0029c03880b26e248ce7e5eaa7943c063521e7a837f835",
        role: "derived research render companion",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-prospective-technologies-research-2026-08-12.md",
        bytes: 66180,
        sha256: "ca4c9e0f8dea834d4b5d430cac7fb77d1b17c22003c58254aeaabaf94bd103ff",
        role: "non-normative donor research",
        decision: "compatibility_seam",
    },
    ExpectedEntry {
        name: "Vestrace-reference-systems-artifact-2026-08-11.json",
        bytes: 72429,
        sha256: "0aa249d87edb176e8fba43a86933f438995259139c6a9017084fd2670db18de2",
        role: "derived research data companion",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-reference-systems-report-2026-08-11.html",
        bytes: 485361,
        sha256: "f1a58da57c079703f9c478e448682b73cd0cc53de93265e80170dc3d84a01ed9",
        role: "derived research render companion",
        decision: "defer_post_v1",
    },
    ExpectedEntry {
        name: "Vestrace-reference-systems-research-2026-08-11.md",
        bytes: 55105,
        sha256: "db2b404618a08720b4a8a7b89bc07998e72ddea4ea2119d23795949c86481c9f",
        role: "non-normative donor research",
        decision: "compatibility_seam",
    },
];

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/external-corpus/vestrace-docss-2026-08-19.manifest.json")
}

#[test]
fn registered_external_corpus_is_complete_and_non_runtime() {
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(manifest_path()).expect("P01 repo-local external corpus manifest must exist"),
    )
    .expect("external corpus manifest must be JSON");

    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["aggregate_sha256"], AGGREGATE_SHA256);
    assert_eq!(manifest["source_path_is_runtime_dependency"], false);
    let entries = manifest["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), EXPECTED_ENTRIES.len());
    let mut aggregate_lines = Vec::new();
    for (entry, expected) in entries.iter().zip(EXPECTED_ENTRIES) {
        assert_eq!(entry["name"], expected.name);
        assert_eq!(entry["bytes"], expected.bytes);
        assert_eq!(entry["sha256"], expected.sha256);
        assert_eq!(entry["role"], expected.role);
        assert_eq!(entry["decision"], expected.decision);
        assert_eq!(entry["provenance"], PROVENANCE);
        assert!(matches!(
            entry["decision"].as_str(),
            Some("adopt_now" | "compatibility_seam" | "defer_post_v1" | "reject")
        ));
        aggregate_lines.extend_from_slice(
            format!(
                "{}\t{}\t{}\n",
                expected.name, expected.bytes, expected.sha256
            )
            .as_bytes(),
        );
    }
    let actual_aggregate = digest::digest(&digest::SHA256, &aggregate_lines)
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            std::fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"))
                .expect("writing to String cannot fail");
            hex
        });
    assert_eq!(actual_aggregate, AGGREGATE_SHA256);

    for name in [
        "Vestrace-post-v1-gate-roadmap-artifact-2026-08-12.json",
        "Vestrace-post-v1-gate-roadmap-report-2026-08-12.html",
        "Vestrace-prospective-technologies-artifact-2026-08-12.json",
        "Vestrace-prospective-technologies-report-2026-08-12.html",
        "Vestrace-reference-systems-artifact-2026-08-11.json",
        "Vestrace-reference-systems-report-2026-08-11.html",
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry["name"] == name)
            .expect("derived entry");
        assert_eq!(entry["independent_authority"], false);
        assert!(entry["derived_from"].is_string());
    }
    let donor = entries
        .iter()
        .find(|entry| entry["name"] == "Vestrace-prospective-technologies-research-2026-08-12.md")
        .expect("prospective donor");
    assert_eq!(
        donor["borrowed_invariants"],
        serde_json::json!([
            "canonical truth remains distinct from projections",
            "single-writer/CAS/lease/fencing may preserve declared ownership"
        ])
    );
    assert_eq!(
        donor["rejected_patterns"],
        serde_json::json!([
            "second durable runtime",
            "message-bus or graph truth",
            "cloud/microservice requirement",
            "owner assertion as release evidence"
        ])
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry["name"] == "AMENDMENT-2026-08-19.md")
            .unwrap()["decision_note"],
        "post-v1 architecture-contract reconciliation is required"
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry["name"] == "Vestrace-post-v1-gate-roadmap-2026-08-12.md")
            .unwrap()["decision_note"],
        "its v1/TRUSTED-complete premise is unverified and supplies no evidence"
    );
    assert_eq!(
        manifest["missing_references"],
        serde_json::json!([
            {"contents_inferred": false, "name": "vestrace-brain-face-organ-system-model.md", "present": false},
            {"contents_inferred": false, "name": "ADR-0011 — Brain–Face–Organ System Decomposition", "present": false}
        ])
    );
}

#[test]
fn offline_check_needs_only_the_registered_manifest() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp = std::env::temp_dir().join(format!(
        "vestrace-p01-manifest-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(temp.join("docs/external-corpus")).unwrap();
    fs::copy(
        manifest_path(),
        temp.join("docs/external-corpus/vestrace-docss-2026-08-19.manifest.json"),
    )
    .unwrap();
    let output = Command::new("node")
        .arg(root.join("scripts/external-corpus-manifest.mjs"))
        .arg("--check")
        .arg(&temp)
        .output()
        .expect("Node must run external corpus manifest checker");
    let _ = fs::remove_dir_all(&temp);
    assert!(
        output.status.success(),
        "offline check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
