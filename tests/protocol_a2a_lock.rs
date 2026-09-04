use std::{fs, path::PathBuf};

fn root(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn a2a_pins_and_closed_advertised_profile_are_locked() {
    let workspace = fs::read_to_string(root("Cargo.toml")).expect("workspace Cargo.toml");
    let http =
        fs::read_to_string(root("crates/vestrace-http/Cargo.toml")).expect("http Cargo.toml");
    let infrastructure = fs::read_to_string(root("crates/vestrace-infrastructure/Cargo.toml"))
        .expect("infrastructure Cargo.toml");
    let lock = fs::read_to_string(root("Cargo.lock")).expect("Cargo.lock");
    assert!(workspace.contains("a2a = { package = \"a2a-lf\", version = \"=0.3.0\" }"));
    assert!(
        workspace.contains("a2a-client = { package = \"a2a-client-lf\", version = \"=0.2.1\" }")
    );
    assert!(
        workspace.contains("a2a-server = { package = \"a2a-server-lf\", version = \"=0.4.1\" }")
    );
    assert!(http.contains("a2a.workspace = true") && http.contains("a2a-server.workspace = true"));
    assert!(!http.contains("a2a-client.workspace = true"));
    assert!(
        infrastructure.contains("a2a.workspace = true")
            && infrastructure.contains("a2a-client.workspace = true")
    );
    assert!(!infrastructure.contains("a2a-server.workspace = true"));
    for (name, version, checksum) in [
        (
            "a2a-lf",
            "0.3.0",
            "7fb24275cca126dc3301d272eef07bd4cefd87f9a7dd5d6f27200fe87e8a83d0",
        ),
        (
            "a2a-client-lf",
            "0.2.1",
            "f68a06a40df172bb5ae0f25e3d49e922f0d33f8af49d0b1276307857dbd28ad2",
        ),
        (
            "a2a-server-lf",
            "0.4.1",
            "c4df08dff9607c4045c892b58f3824bb215262a37bce33f1ab42a72a5c9acd51",
        ),
    ] {
        assert!(
            lock.contains(&format!("name = \"{name}\"\nversion = \"{version}\""))
                && lock.contains(checksum)
        );
    }
    let profile: serde_json::Value = serde_json::from_slice(
        &fs::read(root("schemas/a2a/vestrace-v1-profile.json")).expect("A2A profile must exist"),
    )
    .expect("A2A profile JSON");
    assert_eq!(profile["specification"]["release"], "v1.0.1");
    assert_eq!(
        profile["specification"]["commit"],
        "3303592588e388e62e0f69f701af531d2f4e3991"
    );
    assert_eq!(
        profile["specification"]["repository"],
        "https://github.com/a2aproject/A2A.git"
    );
    assert_eq!(
        profile["specification"]["release_url"],
        "https://github.com/a2aproject/A2A/releases/tag/v1.0.1"
    );
    assert_eq!(profile["wire"]["header"], "A2A-Version: 1.0");
    assert_eq!(profile["wire"]["version"], "1.0");
    assert_eq!(
        profile["interfaces"],
        serde_json::json!([{"binding":"HTTP/SSE","protocol":"JSONRPC"}])
    );
    assert_eq!(
        profile["required_server_operations"],
        serde_json::json!([
            "SendMessage",
            "SendStreamingMessage",
            "GetTask",
            "ListTasks",
            "CancelTask",
            "SubscribeToTask"
        ])
    );
    assert_eq!(
        profile["unsupported"],
        serde_json::json!([
            "PushNotificationConfig",
            "GetExtendedAgentCard",
            "REST",
            "gRPC"
        ])
    );
}
