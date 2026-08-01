use std::net::SocketAddr;

use vestrace_infrastructure::{AppConfig, ConfigOverrides};

const TEST_DATABASE_URL: &str = "postgres://test:test@localhost/vestrace_test";

#[test]
fn environment_overrides_file_value() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "[http]\nbind = '127.0.0.1:7000'\n").unwrap();

    temp_env::with_vars(
        [
            ("VESTRACE_DATABASE__URL", Some(TEST_DATABASE_URL)),
            ("VESTRACE_HTTP__BIND", Some("127.0.0.1:8000")),
        ],
        || {
            let config = AppConfig::load_from(Some(file.path())).unwrap();
            assert_eq!(config.http.bind.to_string(), "127.0.0.1:8000");
        },
    );
}

#[test]
fn typed_http_override_wins_over_environment() {
    temp_env::with_vars(
        [
            ("VESTRACE_DATABASE__URL", Some(TEST_DATABASE_URL)),
            ("VESTRACE_HTTP__BIND", Some("127.0.0.1:8000")),
        ],
        || {
            let config = AppConfig::load_from_with_overrides(
                None,
                ConfigOverrides {
                    http_bind: Some("127.0.0.1:9000".parse::<SocketAddr>().unwrap()),
                },
            )
            .unwrap();

            assert_eq!(config.http.bind.to_string(), "127.0.0.1:9000");
        },
    );
}

#[test]
fn debug_output_redacts_database_url() {
    temp_env::with_var("VESTRACE_DATABASE__URL", Some(TEST_DATABASE_URL), || {
        let output = format!("{:?}", AppConfig::load().unwrap());

        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains(TEST_DATABASE_URL));
        assert!(!output.contains("test:test"));
    });
}
