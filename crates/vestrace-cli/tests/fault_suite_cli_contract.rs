use std::process::{Command, Output};

const CLI_MANIFEST: &str = include_str!("../Cargo.toml");

fn vestrace(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(args)
        .output()
        .expect("vestrace command should start")
}

#[test]
fn fault_suite_command_names_the_operator_supplied_qualification_boundary() {
    let output = vestrace(&["conformance", "fault-suite", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for required in ["--program", "--target-digest", "--isolation"] {
        assert!(
            stdout.contains(required),
            "missing {required} in:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("--database-url"),
        "database credentials must not be accepted through argv:\n{stdout}"
    );
}

#[test]
fn fault_suite_command_refuses_a_non_ephemeral_isolation_before_execution() {
    let output = vestrace(&[
        "conformance",
        "fault-suite",
        "--program",
        "fault-scenario",
        "--target-digest",
        "sha256:target",
        "--isolation",
        "designated-non-production",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("invalid value 'designated-non-production'"),
        "{stderr}"
    );
    assert!(stderr.contains("ephemeral"), "{stderr}");
}

#[test]
fn fault_suite_command_does_not_accept_a_database_url_argument() {
    let output = vestrace(&[
        "conformance",
        "fault-suite",
        "--program",
        "fault-scenario",
        "--target-digest",
        "sha256:target",
        "--isolation",
        "ephemeral",
        "--database-url",
        "postgres://operator:secret@localhost/vestrace",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("unexpected argument '--database-url'"),
        "{stderr}"
    );
}

#[test]
fn fault_suite_command_uses_configured_database_credentials_for_persistence() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "conformance",
            "fault-suite",
            "--program",
            "fault-scenario",
            "--target-digest",
            "sha256:target",
            "--isolation",
            "ephemeral",
        ])
        .env(
            "VESTRACE_DATABASE__URL",
            "postgres://fault-user:fault-secret@127.0.0.1:9/unreachable",
        )
        .output()
        .expect("vestrace command should start");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(
        stderr.contains("fault-suite evidence database is unavailable"),
        "{stderr}"
    );
    assert!(!stderr.contains("fault-user"), "{stderr}");
    assert!(!stderr.contains("fault-secret"), "{stderr}");
}

#[test]
fn cli_does_not_link_the_separately_built_fault_scenario_program() {
    assert!(
        !CLI_MANIFEST.contains("vestrace-fault-scenario"),
        "the CLI must invoke the operator-supplied program path, not link the destructive \
         scenario crate"
    );
}
