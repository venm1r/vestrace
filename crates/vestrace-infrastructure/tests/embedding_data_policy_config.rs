use vestrace_infrastructure::AppConfig;

const DATABASE_URL: &str = "postgres://test:test@localhost/vestrace_test";

fn config_file(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "vestrace-{name}-{}-{}.toml",
        std::process::id(),
        uuid::Uuid::now_v7()
    ));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn enabled_embeddings_require_their_own_complete_data_policy() {
    let path = config_file(
        "embedding-with-completion-policy-only",
        "[embedding]\nenabled = true\nbase_url = 'http://localhost:12345/v1'\nmodel_name = 'text-embedding-nomic-embed-text-v1.5'\nspace_name = 'nomic-768'\n\n[policy.data]\nmode = 'enforce'\nclassification = 'confidential'\nmaximum_sensitivity = 'confidential'\nallowed_destinations = ['local_model']\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let message = AppConfig::load_from(Some(&path)).unwrap_err().to_string();
        for key in [
            "policy.data.embedding.mode",
            "policy.data.embedding.admissible_labels",
            "policy.data.embedding.allow_unclassified",
            "policy.data.embedding.classification",
            "policy.data.embedding.maximum_sensitivity",
            "policy.data.embedding.allowed_destinations",
        ] {
            assert!(message.contains(key), "{key} missing from {message:?}");
        }
    });
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_embedding_policy_can_be_declared_without_completion_consent() {
    let path = config_file(
        "embedding-policy-only",
        "[embedding]\nenabled = true\nbase_url = 'http://localhost:12345/v1'\nmodel_name = 'text-embedding-nomic-embed-text-v1.5'\nspace_name = 'nomic-768'\n\n[policy.data.embedding]\nmode = 'enforce'\nadmissible_labels = ['internal']\nallow_unclassified = true\nclassification = 'confidential'\nmaximum_sensitivity = 'confidential'\nallowed_destinations = ['local_model']\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let config = AppConfig::load_from(Some(&path)).unwrap();
        let embedding = config.policy.data.unwrap().embedding.unwrap();
        assert_eq!(
            embedding.admissible_labels.into_iter().collect::<Vec<_>>(),
            vec!["internal"]
        );
        assert!(embedding.allow_unclassified);
    });
    let _ = std::fs::remove_file(path);
}

#[test]
fn compose_style_environment_lists_build_the_embedding_policy() {
    let path = config_file(
        "embedding-policy-environment",
        "[database]\nmax_connections = 5\n",
    );

    temp_env::with_vars(
        [
            ("VESTRACE_DATABASE__URL", Some(DATABASE_URL)),
            ("VESTRACE_EMBEDDING__ENABLED", Some("true")),
            (
                "VESTRACE_EMBEDDING__BASE_URL",
                Some("http://host.docker.internal:12345/v1"),
            ),
            (
                "VESTRACE_EMBEDDING__MODEL_NAME",
                Some("text-embedding-nomic-embed-text-v1.5"),
            ),
            ("VESTRACE_EMBEDDING__SPACE_NAME", Some("nomic-768")),
            ("VESTRACE_POLICY__DATA__EMBEDDING__MODE", Some("enforce")),
            (
                "VESTRACE_POLICY__DATA__EMBEDDING__ADMISSIBLE_LABELS",
                Some("internal,confidential"),
            ),
            (
                "VESTRACE_POLICY__DATA__EMBEDDING__ALLOW_UNCLASSIFIED",
                Some("true"),
            ),
            (
                "VESTRACE_POLICY__DATA__EMBEDDING__CLASSIFICATION",
                Some("restricted"),
            ),
            (
                "VESTRACE_POLICY__DATA__EMBEDDING__MAXIMUM_SENSITIVITY",
                Some("restricted"),
            ),
            (
                "VESTRACE_POLICY__DATA__EMBEDDING__ALLOWED_DESTINATIONS",
                Some("remote_provider"),
            ),
        ],
        || {
            let config = AppConfig::load_from(Some(&path)).unwrap();
            let policy = config.policy.data.unwrap().embedding.unwrap();
            assert_eq!(
                policy.admissible_labels.into_iter().collect::<Vec<_>>(),
                vec!["confidential", "internal"]
            );
            assert_eq!(
                policy.allowed_destinations.into_iter().collect::<Vec<_>>(),
                vec![vestrace_domain::DataDestination::RemoteProvider]
            );
        },
    );
    let _ = std::fs::remove_file(path);
}
