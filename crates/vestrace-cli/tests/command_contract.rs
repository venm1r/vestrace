use std::process::{Command, Output};

const UNAVAILABLE_DATABASE_URL: &str =
    "postgres://p0-user:p0-secret-password@127.0.0.1:9/vestrace_p0_unavailable";

fn run(command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .arg(command)
        .env("VESTRACE_DATABASE__URL", UNAVAILABLE_DATABASE_URL)
        .output()
        .expect("vestrace command should start")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn worker_fails_explicitly_instead_of_reporting_false_success() {
    let output = run("worker");

    assert!(!output.status.success(), "worker unexpectedly succeeded");
    assert!(
        stderr(&output).contains("worker command is not implemented"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn placeholder_commands_fail_explicitly() {
    for (command, expected) in [
        ("mcp", "mcp command is not implemented"),
        ("doctor", "doctor command is not implemented"),
        ("rebuild", "rebuild command is not implemented"),
    ] {
        let output = run(command);

        assert!(!output.status.success(), "{command} unexpectedly succeeded");
        assert!(
            stderr(&output).contains(expected),
            "unexpected stderr for {command}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn migrate_fails_when_database_is_unavailable_and_redacts_secrets() {
    let output = run("migrate");
    let stderr = stderr(&output);

    assert!(!output.status.success(), "migrate unexpectedly succeeded");
    assert!(stderr.contains("database is unavailable"), "{stderr}");
    assert!(!stderr.contains("p0-secret-password"), "{stderr}");
    assert!(!stderr.contains(UNAVAILABLE_DATABASE_URL), "{stderr}");
}
