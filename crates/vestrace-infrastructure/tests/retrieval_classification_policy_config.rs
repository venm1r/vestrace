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
fn retrieval_policy_is_explicit_and_independent_from_embedding() {
    let path = config_file(
        "retrieval-classification-policy",
        "[policy.data]\nmemory_labels = ['internal', 'restricted']\n\n[policy.data.retrieval]\nadmissible_labels = ['internal']\nallow_unclassified = false\n\n[policy.data.embedding]\nmode = 'enforce'\nadmissible_labels = ['restricted']\nallow_unclassified = true\nclassification = 'restricted'\nmaximum_sensitivity = 'restricted'\nallowed_destinations = ['remote_provider']\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let config = AppConfig::load_from(Some(&path)).unwrap();
        let policy = config.retrieval_classification_policy().unwrap();

        assert!(policy.admits(Some("internal")));
        assert!(!policy.admits(Some("restricted")));
        assert!(!policy.admits(None));
    });
    let _ = std::fs::remove_file(path);
}

#[test]
fn retrieval_policy_refuses_labels_outside_the_memory_vocabulary() {
    let path = config_file(
        "retrieval-label-outside-vocabulary",
        "[policy.data]\nmemory_labels = ['internal']\n\n[policy.data.retrieval]\nadmissible_labels = ['restricted']\nallow_unclassified = false\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let message = AppConfig::load_from(Some(&path)).unwrap_err().to_string();
        assert!(message.contains("restricted"), "{message}");
        assert!(message.contains("policy.data.memory_labels"), "{message}");
    });
    let _ = std::fs::remove_file(path);
}

#[test]
fn retrieval_policy_changes_the_configuration_identity() {
    let internal = config_file(
        "retrieval-fingerprint-internal",
        "[policy.data]\nmemory_labels = ['internal', 'restricted']\n\n[policy.data.retrieval]\nadmissible_labels = ['internal']\nallow_unclassified = false\n",
    );
    let restricted = config_file(
        "retrieval-fingerprint-restricted",
        "[policy.data]\nmemory_labels = ['internal', 'restricted']\n\n[policy.data.retrieval]\nadmissible_labels = ['restricted']\nallow_unclassified = false\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let internal = AppConfig::load_from(Some(&internal)).unwrap();
        let restricted = AppConfig::load_from(Some(&restricted)).unwrap();

        assert_ne!(internal.fingerprint(), restricted.fingerprint());
    });
    let _ = std::fs::remove_file(internal);
    let _ = std::fs::remove_file(restricted);
}

#[test]
fn missing_retrieval_policy_is_not_consent() {
    let path = config_file(
        "retrieval-policy-absent",
        "[policy.data]\nmemory_labels = ['internal']\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let config = AppConfig::load_from(Some(&path)).unwrap();
        let message = config
            .retrieval_classification_policy()
            .unwrap_err()
            .to_string();

        assert!(
            message.contains("policy.data.retrieval.admissible_labels"),
            "{message}"
        );
        assert!(
            message.contains("policy.data.retrieval.allow_unclassified"),
            "{message}"
        );
    });
    let _ = std::fs::remove_file(path);
}

#[test]
fn blank_policy_version_is_refused_before_retrieval_can_start() {
    let path = config_file(
        "blank-retrieval-policy-version",
        "[policy]\nversion = '   '\n\n[policy.data]\nmemory_labels = ['internal']\n\n[policy.data.retrieval]\nadmissible_labels = ['internal']\nallow_unclassified = false\n",
    );

    temp_env::with_var("VESTRACE_DATABASE__URL", Some(DATABASE_URL), || {
        let message = AppConfig::load_from(Some(&path)).unwrap_err().to_string();
        assert!(message.contains("policy.version"), "{message}");
        assert!(message.contains("blank"), "{message}");
    });
    let _ = std::fs::remove_file(path);
}
