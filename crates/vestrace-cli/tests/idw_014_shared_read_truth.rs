use std::process::{Command, Output};

fn trusted(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["conformance", "check", "trusted"])
        .args(args)
        .output()
        .expect("run vestrace conformance check trusted")
}

#[test]
fn trusted_json_reports_idw_014_as_executed_pass() {
    let output = trusted(&["--json"]);
    assert!(
        output.status.success(),
        "TRUSTED must now pass completely: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse TRUSTED report stdout as JSON");
    assert_eq!(report["summary"]["total"], 199);
    assert_eq!(report["summary"]["passed"], 199);
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["summary"]["skipped"], 0);
    assert_eq!(report["summary"]["not_applicable"], 0);
    assert_eq!(report["summary"]["passed_build_verified"], 1);
    assert_eq!(report["summary"]["passed_executed"], 190);

    let idw_014 = report["results"]
        .as_array()
        .expect("report results are an array")
        .iter()
        .find(|result| {
            result["requirement_ids"]
                .as_array()
                .is_some_and(|requirements| {
                    requirements.iter().any(|requirement| {
                        requirement["family"] == "idw" && requirement["number"] == 14
                    })
                })
        })
        .expect("IDW-014 result");
    assert_eq!(idw_014["status"], "pass");
    assert_eq!(idw_014["origin"], "executed");
    let message = idw_014["message"].as_str().unwrap();
    assert!(message.contains("exact permit-bound read"));
    assert!(message.contains("workspace"));
    assert_eq!(
        idw_014["evidence"],
        "crates/vestrace-application/src/memory/shared_read.rs"
    );

    let skipped: Vec<String> = report["results"]
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
    assert!(skipped.is_empty(), "expected 0 skips, got: {:?}", skipped);
}
