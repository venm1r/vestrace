use std::process::{Command, Output};

fn trusted(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["conformance", "check", "trusted"])
        .args(args)
        .output()
        .expect("run vestrace conformance check trusted")
}

#[test]
fn trusted_json_reports_idw_010_as_build_verified_without_closing_the_gate() {
    let output = trusted(&["--json"]);
    assert!(
        !output.status.success(),
        "TRUSTED must remain open: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse TRUSTED report stdout as JSON");
    assert_eq!(report["summary"]["total"], 199);
    assert_eq!(report["summary"]["passed"], 196);
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["summary"]["skipped"], 3);
    assert_eq!(report["summary"]["not_applicable"], 0);
    assert_eq!(report["summary"]["passed_build_verified"], 1);

    let idw_010 = report["results"]
        .as_array()
        .expect("report results are an array")
        .iter()
        .find(|result| {
            result["requirement_ids"]
                .as_array()
                .is_some_and(|requirements| {
                    requirements.iter().any(|requirement| {
                        requirement["family"] == "idw" && requirement["number"] == 10
                    })
                })
        })
        .expect("IDW-010 result");
    assert_eq!(idw_010["status"], "pass");
    assert_eq!(idw_010["origin"], "build_verified");
    assert_eq!(
        idw_010["evidence"],
        "crates/vestrace-domain/src/enterprise/sharing.rs:SharedMemoryRef"
    );

    let mut skipped: Vec<_> = report["results"]
        .as_array()
        .expect("report results are an array")
        .iter()
        .filter(|result| result["status"] == "skip")
        .flat_map(|result| {
            result["requirement_ids"]
                .as_array()
                .expect("result requirement IDs are an array")
                .iter()
                .map(|requirement| {
                    format!(
                        "{}-{:03}",
                        requirement["family"]
                            .as_str()
                            .expect("serialized family")
                            .to_uppercase(),
                        requirement["number"].as_u64().expect("serialized number")
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    skipped.sort();
    assert_eq!(skipped, vec!["IDW-014", "QUAL-010", "REC-016"]);
}

#[test]
fn trusted_human_output_labels_build_verified_and_splits_pass_origins() {
    let output = trusted(&[]);
    assert!(
        !output.status.success(),
        "TRUSTED must remain open: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human output is UTF-8");
    assert!(stdout.contains("IDW-010 [build_verified]"));
    assert!(stdout.contains("196 passed ("));
    assert!(stdout.contains("executed:"));
    assert!(stdout.contains("build-verified: 1"));
    assert!(stdout.contains("attested:"));
}

#[test]
fn trusted_json_explains_build_verified_origin_as_compiler_proof() {
    let output = trusted(&["--json"]);
    assert!(
        !output.status.success(),
        "TRUSTED must remain open: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse TRUSTED report stdout as JSON");
    let qual_001 = report["results"]
        .as_array()
        .expect("report results are an array")
        .iter()
        .find(|result| {
            result["requirement_ids"]
                .as_array()
                .is_some_and(|requirements| {
                    requirements.iter().any(|requirement| {
                        requirement["family"] == "qual" && requirement["number"] == 1
                    })
                })
        })
        .expect("QUAL-001 result");
    let message = qual_001["message"].as_str().expect("QUAL-001 message");

    assert!(message.contains("BuildVerified"));
    assert!(message.contains("compiler proof"));
}
