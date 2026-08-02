use vestrace_domain::{
    id::*, release::*, now,
};

#[test]
fn test_release_manifest_creation() {
    let manifest_id = ReleaseManifestId::new();
    let at = now();

    let manifest = ReleaseManifest {
        id: manifest_id,
        release_version: "v0.2.0-final".into(),
        components: vec!["core".into(), "harness".into(), "console".into()],
        checksum: "sha256:manifest_hash_123".into(),
        created_at: at,
    };

    assert_eq!(manifest.release_version, "v0.2.0-final");
    assert_eq!(manifest.components.len(), 3);
}
