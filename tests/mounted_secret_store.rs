use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use vestrace_domain::WorkspaceId;
use vestrace_domain::trust::{
    KeyProvider, KeyProviderError, KeyPurpose, KeyReference, SecretResolutionRequest,
};
use vestrace_infrastructure::crypto::{
    MOUNTED_SECRET_STORE_PROVIDER, MountedSecretStoreKeyProvider,
};

fn store_root(id: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vestrace-store-{id}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

/// Writes one key directory with its declarations, and one version in `state`.
fn write_key(root: &Path, key_id: &str, scope: &str, version: &str, state: &str) {
    let key_dir = root.join(key_id);
    fs::create_dir_all(&key_dir).unwrap();
    fs::write(key_dir.join("scope"), scope).unwrap();
    fs::write(key_dir.join("purpose"), "signing").unwrap();
    fs::write(key_dir.join("algorithm"), "ed25519").unwrap();
    write_version(root, key_id, version, state);
}

fn write_version(root: &Path, key_id: &str, version: &str, state: &str) {
    let version_dir = root.join(key_id).join(version);
    fs::create_dir_all(&version_dir).unwrap();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    fs::write(version_dir.join("state"), state).unwrap();
    fs::write(version_dir.join("private.pkcs8"), pkcs8.as_ref()).unwrap();
    fs::write(version_dir.join("public.bin"), pair.public_key().as_ref()).unwrap();
}

fn key_ref(key_id: &str, version: &str, scope: &str) -> KeyReference {
    KeyReference::new(
        MOUNTED_SECRET_STORE_PROVIDER,
        key_id,
        version,
        KeyPurpose::Signing,
        scope,
        "ed25519",
    )
    .unwrap()
}

fn request(purpose: &str) -> SecretResolutionRequest {
    SecretResolutionRequest::new(WorkspaceId::new(), purpose, "test://authorization")
}

/// The store's declaration is the one the caller cannot edit. A reference
/// claiming a scope the store never granted is refused even though the
/// request's own purpose matches the declaration — isolating the reference
/// check from the request-purpose check, which is proven separately by
/// `a_resolution_outside_the_declared_scope_is_refused`.
#[test]
fn a_scope_the_store_did_not_declare_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "export"),
            &request("release"),
        )
        .expect_err("a scope the store did not declare must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "expected Denied, got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// The lifecycle state belongs to the store. A revoked key is refused however
/// the caller describes it.
#[test]
fn a_revoked_version_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "revoked");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("release"),
        )
        .expect_err("a revoked version must be refused");

    assert!(
        matches!(error, KeyProviderError::NotUsable),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A key rotating out is still usable: during a rotation both halves have to
/// verify material signed by the other.
#[test]
fn a_rotating_version_still_resolves() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "rotating");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    assert!(
        provider
            .resolve(
                &key_ref("release-signing", "v1", "release"),
                &request("release")
            )
            .is_ok()
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn validated_public_resolution_accepts_active_and_rotating_signing_keys() {
    for state in ["active", "rotating"] {
        let root = store_root(&format!("{}-{state}", suffix()));
        write_key(&root, "release-signing", "release", "v1", state);
        let expected =
            fs::read(root.join("release-signing").join("v1").join("public.bin")).unwrap();
        let provider = MountedSecretStoreKeyProvider::new(&root);

        let actual = provider
            .resolve_public_key(
                &key_ref("release-signing", "v1", "release"),
                &request("release"),
            )
            .expect("usable signing key public material");

        assert_eq!(actual, expected, "state {state}");
        fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn validated_public_resolution_refuses_inactive_versions() {
    for state in ["revoked", "retired"] {
        let root = store_root(&format!("{}-{state}", suffix()));
        write_key(&root, "release-signing", "release", "v1", state);
        let provider = MountedSecretStoreKeyProvider::new(&root);

        let error = provider
            .resolve_public_key(
                &key_ref("release-signing", "v1", "release"),
                &request("release"),
            )
            .expect_err("inactive key public material must be refused");

        assert!(
            matches!(error, KeyProviderError::NotUsable),
            "state {state}: {error:?}"
        );
        fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn validated_public_resolution_refuses_declaration_mismatches() {
    for (field, value) in [
        ("purpose", "storage"),
        ("algorithm", "p256"),
        ("scope", "other-scope"),
    ] {
        let root = store_root(&format!("{}-{field}", suffix()));
        write_key(&root, "release-signing", "release", "v1", "active");
        fs::write(root.join("release-signing").join(field), value).unwrap();
        let provider = MountedSecretStoreKeyProvider::new(&root);

        let error = provider
            .resolve_public_key(
                &key_ref("release-signing", "v1", "release"),
                &request("release"),
            )
            .expect_err("mismatched declaration public material must be refused");

        assert!(
            matches!(error, KeyProviderError::Denied(_)),
            "{field}: {error:?}"
        );
        fs::remove_dir_all(&root).ok();
    }
}

/// The resolution's own purpose is checked against the declaration separately
/// from the reference's scope, because they are two different claims.
#[test]
fn a_resolution_outside_the_declared_scope_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("export"),
        )
        .expect_err("a resolution outside the declared scope must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A key id is an identifier, not a path.
#[test]
fn a_key_id_cannot_walk_out_of_the_store() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(&key_ref("../../etc", "v1", "release"), &request("release"))
        .expect_err("a traversing key id must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A drive-relative fragment (`C:keys`) is not a single path segment: on
/// Windows, `Path::new(root).join("C:keys")` discards `root` entirely and
/// names `C:keys` outright, so a string-contains check on `/`, `\` and `..`
/// alone would let this one through.
#[test]
fn a_drive_relative_key_id_cannot_replace_the_store_root() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(&key_ref("C:keys", "v1", "release"), &request("release"))
        .expect_err("a drive-relative key id must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// `.` names the store root itself, not a key within it.
#[test]
fn a_bare_dot_key_id_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(&key_ref(".", "v1", "release"), &request("release"))
        .expect_err("a bare '.' key id must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// An absolute path is not a single path segment either.
#[test]
fn an_absolute_path_key_id_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("/etc/passwd", "v1", "release"),
            &request("release"),
        )
        .expect_err("an absolute path key id must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// `declaration()` is called directly by callers that never go through
/// `resolve` (a probe reading what the store declares, for instance), so its
/// own segment check is the only guard on that path — not redundant with
/// `resolve`'s outer check, which this call never reaches.
#[test]
fn a_declaration_cannot_walk_out_of_the_store() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .declaration("../../etc")
        .expect_err("a traversing key id must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A key version is an identifier, not a path. `declaration()` never sees the
/// version, so only the segment check inside `resolve` itself can catch this.
#[test]
fn a_key_version_cannot_walk_out_of_the_store() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "../../etc", "release"),
            &request("release"),
        )
        .expect_err("a traversing key version must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unknown_key_is_unavailable() {
    let root = store_root(&suffix());
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(&key_ref("absent", "v1", "release"), &request("release"))
        .expect_err("an absent key must be refused");

    assert!(
        matches!(error, KeyProviderError::Unavailable(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unknown_version_is_unavailable() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v9", "release"),
            &request("release"),
        )
        .expect_err("an absent version must be refused");

    assert!(
        matches!(error, KeyProviderError::Unavailable(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A resolution nobody authorized is not a resolution.
#[test]
fn a_request_without_an_authorization_reference_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &SecretResolutionRequest::new(WorkspaceId::new(), "release", "   "),
        )
        .expect_err("an unauthorized request must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// The purpose a store declares is not overridden by the purpose a caller
/// names.
#[test]
fn a_purpose_the_store_did_not_declare_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    fs::write(root.join("release-signing").join("purpose"), "storage").unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("release"),
        )
        .expect_err("a purpose the store did not declare must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_algorithm_the_store_did_not_declare_is_refused() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    fs::write(root.join("release-signing").join("algorithm"), "p256").unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let error = provider
        .resolve(
            &key_ref("release-signing", "v1", "release"),
            &request("release"),
        )
        .expect_err("an algorithm the store did not declare must be refused");

    assert!(
        matches!(error, KeyProviderError::Denied(_)),
        "got {error:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// Nothing the adapter renders may carry the material it protects.
#[test]
fn no_refusal_discloses_key_material() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let private = fs::read(
        root.join("release-signing")
            .join("v1")
            .join("private.pkcs8"),
    )
    .unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let rendered = [
        provider
            .resolve(
                &key_ref("release-signing", "v1", "export"),
                &request("export"),
            )
            .err()
            .map(|error| format!("{error} {error:?}"))
            .unwrap_or_default(),
        provider
            .resolve(&key_ref("absent", "v1", "release"), &request("release"))
            .err()
            .map(|error| format!("{error} {error:?}"))
            .unwrap_or_default(),
    ]
    .join(" ");

    // clippy::format_collect: a `fold` would be faster, but this is a one-shot
    // needle for an assertion, not a hot path.
    #[allow(clippy::format_collect)]
    let needle: String = private.iter().map(|byte| format!("{byte:02x}")).collect();
    assert!(
        !rendered.contains(&needle),
        "a refusal disclosed key material"
    );
    assert!(
        !rendered
            .as_bytes()
            .windows(private.len())
            .any(|w| w == private),
        "a refusal disclosed key material"
    );

    fs::remove_dir_all(&root).ok();
}

/// `versions` is read directly by the next task's probe, without going
/// through `resolve`, so its sort order and its state pairing are load-bearing
/// on their own — a length check alone would pass against a method that
/// returned the right count of wrong things.
#[test]
fn the_store_lists_every_version_with_its_state() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "revoked");
    write_version(&root, "release-signing", "v2", "retired");
    write_version(&root, "release-signing", "v3", "active");
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let versions = provider.versions("release-signing").unwrap();

    assert_eq!(
        versions,
        vec![
            ("v1".to_string(), "revoked".to_string()),
            ("v2".to_string(), "retired".to_string()),
            ("v3".to_string(), "active".to_string()),
        ]
    );

    fs::remove_dir_all(&root).ok();
}

/// Kubernetes Secret projections add sidecar entries such as `..data`
/// alongside the version directories this adapter cares about. This
/// adapter's whole point is being fed by an orchestrator, so a sidecar entry
/// must be skipped rather than failing the whole listing.
#[test]
fn a_kubernetes_data_sidecar_does_not_break_the_version_listing() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    write_version(&root, "release-signing", "v2", "retired");
    fs::create_dir_all(root.join("release-signing").join("..data")).unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let versions = provider.versions("release-signing").unwrap();

    assert_eq!(
        versions,
        vec![
            ("v1".to_string(), "active".to_string()),
            ("v2".to_string(), "retired".to_string()),
        ]
    );

    fs::remove_dir_all(&root).ok();
}

/// `public_key` is read directly by the next task's probe to tell two
/// versions apart. A wrong path join could still return 32 plausible bytes —
/// only comparing against the exact bytes the fixture wrote catches that.
#[test]
fn the_store_reads_the_public_half_of_each_version() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    write_version(&root, "release-signing", "v2", "retired");
    let expected_v1 = fs::read(root.join("release-signing").join("v1").join("public.bin")).unwrap();
    let expected_v2 = fs::read(root.join("release-signing").join("v2").join("public.bin")).unwrap();
    let provider = MountedSecretStoreKeyProvider::new(&root);

    let actual_v1 = provider.public_key("release-signing", "v1").unwrap();
    let actual_v2 = provider.public_key("release-signing", "v2").unwrap();

    assert_eq!(actual_v1.len(), 32);
    assert_eq!(actual_v2.len(), 32);
    assert_ne!(actual_v1, actual_v2);
    assert_eq!(actual_v1, expected_v1);
    assert_eq!(actual_v2, expected_v2);

    fs::remove_dir_all(&root).ok();
}

use vestrace_application::{
    CryptoAdapterQualificationProbe, CryptoAdapterQualificationTarget, CryptoCustody,
    CryptoQualificationCheck,
};
use vestrace_infrastructure::crypto::{MountedStoreCryptoProbe, discloses};

fn target(key_id: &str, version: &str, scope: &str) -> CryptoAdapterQualificationTarget {
    CryptoAdapterQualificationTarget::new(
        MOUNTED_SECRET_STORE_PROVIDER,
        CryptoCustody::MountedSecretStore,
        key_id,
        version,
        "ed25519",
        KeyPurpose::Signing,
        scope,
    )
    .unwrap()
}

/// Two versions holding the same key are one key under two names. Counting
/// directories would call that a rotation; comparing the material does not.
#[test]
fn identical_material_under_two_versions_is_not_a_rotation() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");
    let key_dir = root.join("release-signing");
    fs::create_dir_all(key_dir.join("v2")).unwrap();
    fs::write(key_dir.join("v2").join("state"), "retired").unwrap();
    fs::copy(
        key_dir.join("v1").join("private.pkcs8"),
        key_dir.join("v2").join("private.pkcs8"),
    )
    .unwrap();
    fs::copy(
        key_dir.join("v1").join("public.bin"),
        key_dir.join("v2").join("public.bin"),
    )
    .unwrap();

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let evidence = probe
        .collect(&target("release-signing", "v1", "release"))
        .unwrap();

    assert!(
        !evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Rotation),
        "the same key under two version names is not a rotation"
    );

    fs::remove_dir_all(&root).ok();
}

/// A store with an active key, a revoked predecessor holding different
/// material, and readable public halves supports all six checks.
#[test]
fn a_complete_store_yields_every_check() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v2", "active");
    write_version(&root, "release-signing", "v1", "revoked");

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let evidence = probe
        .collect(&target("release-signing", "v2", "release"))
        .unwrap();

    for check in [
        CryptoQualificationCheck::Resolution,
        CryptoQualificationCheck::ScopeIsolation,
        CryptoQualificationCheck::CryptographicRoundTrip,
        CryptoQualificationCheck::Lifecycle,
        CryptoQualificationCheck::Rotation,
        CryptoQualificationCheck::SecretNonDisclosure,
    ] {
        assert!(
            evidence.passed_checks().contains(&check),
            "{check:?} was not collected"
        );
    }
    assert_eq!(evidence.evidence_refs().len(), 6);

    fs::remove_dir_all(&root).ok();
}

/// One version is not a rotation and no revocation is not a lifecycle.
#[test]
fn a_single_version_store_yields_neither_rotation_nor_lifecycle() {
    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v1", "active");

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let evidence = probe
        .collect(&target("release-signing", "v1", "release"))
        .unwrap();

    assert!(
        evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Resolution)
    );
    assert!(
        !evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Rotation)
    );
    assert!(
        !evidence
            .passed_checks()
            .contains(&CryptoQualificationCheck::Lifecycle)
    );

    fs::remove_dir_all(&root).ok();
}

/// The service's own verdict over a complete store, which is the point of all
/// of it.
#[test]
fn a_complete_store_passes_crypto_qualification() {
    use vestrace_application::CryptoAdapterQualificationService;

    let root = store_root(&suffix());
    write_key(&root, "release-signing", "release", "v2", "active");
    write_version(&root, "release-signing", "v1", "revoked");

    let probe = MountedStoreCryptoProbe::new(MountedSecretStoreKeyProvider::new(&root));
    let decision = CryptoAdapterQualificationService::evaluate_with_probe(
        &target("release-signing", "v2", "release"),
        &probe,
    )
    .unwrap();

    assert!(decision.is_passed(), "failures: {:?}", decision.failures());

    fs::remove_dir_all(&root).ok();
}

/// `discloses` is what the probe's SecretNonDisclosure check stands on, and
/// every fixture in this suite has the adapter refuse cleanly, so nothing
/// above pins the scan itself — a version that always answered "clean" would
/// pass every other case here unnoticed. Pinned directly instead, against a
/// synthetic string built to contain the secret: no adapter, no leaking code
/// path, so nothing leaky ships.
#[test]
fn discloses_finds_the_secret_bytes_and_a_clean_string_does_not() {
    let secret: &[u8] = b"super-secret-material-0123456789";
    let mut leaking = b"resolution denied: debug dump ".to_vec();
    leaking.extend_from_slice(secret);
    leaking.extend_from_slice(b" end of dump");
    let leaking = String::from_utf8(leaking).unwrap();

    assert!(
        discloses(&leaking, secret),
        "the secret bytes went undetected"
    );
    assert!(
        !discloses("resolution denied: outside declared scope", secret),
        "a clean refusal was reported as disclosing the secret"
    );
}

/// Raw contiguous bytes are the cheapest rendering to search for, and the
/// only one a lossy or unsafe formatting path could produce, but they are
/// also the shape safe `format!`/`Display`/`Debug` code essentially never
/// emits for binary key material — a derived `Debug` on `Vec<u8>` never puts
/// the secret's own bytes in a row. The other cases in this file pin the
/// shapes safe Rust actually renders it into.
#[test]
fn discloses_finds_lowercase_hex() {
    let secret: &[u8] = b"super-secret-material-0123456789";
    // clippy::format_collect: a one-shot needle for an assertion, not a hot path.
    #[allow(clippy::format_collect)]
    let hex: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
    let leaking = format!("resolution denied: debug dump {hex} end of dump");

    assert!(
        discloses(&leaking, secret),
        "lowercase hex of the secret went undetected"
    );
    assert!(
        !discloses("resolution denied: outside declared scope", secret),
        "a clean refusal was reported as disclosing the secret"
    );
}

#[test]
fn discloses_finds_uppercase_hex() {
    let secret: &[u8] = b"super-secret-material-0123456789";
    // clippy::format_collect: a one-shot needle for an assertion, not a hot path.
    #[allow(clippy::format_collect)]
    let hex: String = secret.iter().map(|byte| format!("{byte:02X}")).collect();
    let leaking = format!("resolution denied: debug dump {hex} end of dump");

    assert!(
        discloses(&leaking, secret),
        "uppercase hex of the secret went undetected"
    );
    assert!(
        !discloses("resolution denied: outside declared scope", secret),
        "a clean refusal was reported as disclosing the secret"
    );
}

/// A derived `Debug` on a byte slice — the accidental leak this whole change
/// exists to catch — renders as a decimal list. `{:?}` renders it compact,
/// `{:#?}` renders one byte per indented line; both must be caught by the
/// same check.
#[test]
fn discloses_finds_derived_debug_decimal_compact_and_pretty() {
    let secret: &[u8] = b"super-secret-material-0123456789";
    let compact_leak = format!("ResolvedKeyMaterial {{ bytes: {secret:?} }}");
    let pretty_leak = format!("ResolvedKeyMaterial {{\n    bytes: {secret:#?},\n}}");

    assert!(
        discloses(&compact_leak, secret),
        "the compact `{{:?}}` decimal rendering went undetected"
    );
    assert!(
        discloses(&pretty_leak, secret),
        "the pretty `{{:#?}}` decimal rendering went undetected"
    );
    assert!(
        !discloses("resolution denied: outside declared scope", secret),
        "a clean refusal was reported as disclosing the secret"
    );
}

#[test]
fn discloses_finds_base64() {
    let secret: &[u8] = b"super-secret-material-0123456789";
    let encoded = base64::engine::general_purpose::STANDARD.encode(secret);
    let leaking = format!("resolution denied: debug dump {encoded} end of dump");

    assert!(
        discloses(&leaking, secret),
        "base64 of the secret went undetected"
    );
    assert!(
        !discloses("resolution denied: outside declared scope", secret),
        "a clean refusal was reported as disclosing the secret"
    );
}
