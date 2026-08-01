use std::process::{Command, Output};

const TEST_DATABASE_URL: &str = "postgres://test:test@localhost/vestrace_test";

fn vestrace(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(args)
        .env("VESTRACE_DATABASE__URL", TEST_DATABASE_URL)
        .output()
        .unwrap()
}

#[test]
fn help_lists_all_supported_subcommands() {
    let output = vestrace(&["--help"]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for subcommand in ["server", "worker", "mcp", "migrate", "doctor", "rebuild"] {
        assert!(
            stdout.contains(subcommand),
            "missing {subcommand} in:\n{stdout}"
        );
    }
}

#[test]
fn database_url_is_not_accepted_as_a_process_argument() {
    let output = vestrace(&["--database-url", TEST_DATABASE_URL, "server"]);
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(
        stderr.contains("unexpected argument '--database-url'"),
        "{stderr}"
    );
}

#[test]
fn typed_http_override_reaches_the_server_handler() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["--http-bind", "127.0.0.1:9000", "server"])
        .env("VESTRACE_DATABASE__URL", TEST_DATABASE_URL)
        .env("VESTRACE_HTTP__BIND", "not-a-socket")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(
        stderr.contains("server command is not implemented"),
        "{stderr}"
    );
}
