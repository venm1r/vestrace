use std::net::SocketAddr;

use vestrace_infrastructure::{AppConfig, ConfigOverrides, LogFormat};

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
fn rejects_database_url_in_file_even_when_environment_overrides_it() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        "[database]\nurl = 'postgres://file-user:file-password@localhost/file-db'\n",
    )
    .unwrap();

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(TEST_DATABASE_URL), || {
        let result = AppConfig::load_from(Some(file.path()));

        assert!(
            result.is_err(),
            "database.url in a configuration file must be rejected even when the environment supplies a replacement"
        );
    });
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

#[test]
fn observability_format_accepts_typed_text_and_json_values() {
    for (value, expected) in [("text", LogFormat::Text), ("json", LogFormat::Json)] {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            file.path(),
            format!("[observability]\nformat = '{value}'\n"),
        )
        .unwrap();

        temp_env::with_var("VESTRACE_DATABASE__URL", Some(TEST_DATABASE_URL), || {
            let config = AppConfig::load_from(Some(file.path())).unwrap();
            assert_eq!(config.observability.format, expected);
        });
    }
}

#[test]
fn rejects_unknown_observability_file_keys() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "[observability]\ninclude_bodies = true\n").unwrap();

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(TEST_DATABASE_URL), || {
        assert!(AppConfig::load_from(Some(file.path())).is_err());
    });
}
