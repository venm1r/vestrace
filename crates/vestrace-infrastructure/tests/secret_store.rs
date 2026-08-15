//! Database-backed proof that secrets survive a round trip *and* that what
//! lands in the table is unreadable.
//!
//! Unit tests over the cipher prove the construction; these prove the storage
//! path does not undo it — by reading the raw bytes back out of PostgreSQL and
//! asserting the plaintext is not among them.

use std::sync::Arc;

use sqlx::{PgPool, Row};
use vestrace_application::{RequestContext, SecretMaterial, SecretStore};
use vestrace_domain::trust::SecretResolutionRequest;
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::crypto::MasterKey;
use vestrace_infrastructure::{PgSecretStore, PgStore};

const PLAINTEXT: &str = "sk-provider-0123456789abcdef";

fn master_key(version: &str) -> Arc<MasterKey> {
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode([42u8; 32]);
    Arc::new(MasterKey::from_base64(&encoded, version).unwrap())
}

async fn seed_workspace(pool: &PgPool, workspace_id: WorkspaceId) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("secrets-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
}

fn context(workspace_id: WorkspaceId) -> RequestContext {
    RequestContext::new(workspace_id, PrincipalId::new())
}

fn lease(
    reference: &vestrace_domain::trust::SecretRef,
    workspace: WorkspaceId,
    purpose: &str,
) -> vestrace_domain::trust::SecretLease {
    reference
        .authorize_resolution(&SecretResolutionRequest::new(
            workspace,
            purpose,
            "test-authorization",
        ))
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stored_secret_resolves_to_exactly_what_was_stored(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool), master_key("v1"));
    let context = context(workspace);

    let reference = store
        .put(
            &context,
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    let resolved = store
        .resolve(&context, &lease(&reference, workspace, "provider-api-key"))
        .await
        .unwrap();

    assert_eq!(resolved.expose_str().unwrap(), PLAINTEXT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_plaintext_is_nowhere_in_the_database(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));

    store
        .put(
            &context(workspace),
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    // Read the row directly, bypassing the store entirely — this is what an
    // operator with a database dump sees.
    let row = sqlx::query("SELECT name, purpose, nonce, ciphertext FROM workspace_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    let ciphertext: Vec<u8> = row.try_get("ciphertext").unwrap();
    let nonce: Vec<u8> = row.try_get("nonce").unwrap();

    assert!(
        !ciphertext
            .windows(PLAINTEXT.len())
            .any(|window| window == PLAINTEXT.as_bytes()),
        "plaintext found in stored ciphertext"
    );
    assert_eq!(nonce.len(), 12, "AES-GCM nonce must be 96 bits");
    assert_eq!(
        ciphertext.len(),
        PLAINTEXT.len() + 16,
        "ciphertext must carry a 16-byte authentication tag"
    );
    // The name is intentionally readable — it is not a secret, and an operator
    // needs it to know what exists.
    assert_eq!(row.try_get::<String, _>("name").unwrap(), "openai");
}

#[sqlx::test(migrations = "../../migrations")]
async fn replacing_a_secret_keeps_its_identity_and_changes_its_ciphertext(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));
    let context = context(workspace);

    let first = store
        .put(
            &context,
            "openai",
            "provider-api-key",
            SecretMaterial::new(b"old-value".to_vec()),
        )
        .await
        .unwrap();
    let before: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM workspace_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();

    let second = store
        .put(
            &context,
            "openai",
            "provider-api-key",
            SecretMaterial::new(b"new-value".to_vec()),
        )
        .await
        .unwrap();

    // The id is stable, so a lease taken before the rotation still names the
    // same secret rather than dangling.
    assert_eq!(first.id(), second.id());
    let after: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM workspace_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(before, after);

    let resolved = store
        .resolve(&context, &lease(&second, workspace, "provider-api-key"))
        .await
        .unwrap();
    assert_eq!(resolved.expose_str().unwrap(), "new-value");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_secret_does_not_resolve_from_another_workspace(pool: PgPool) {
    let owner = WorkspaceId::new();
    let intruder = WorkspaceId::new();
    seed_workspace(&pool, owner).await;
    seed_workspace(&pool, intruder).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool), master_key("v1"));

    let reference = store
        .put(
            &context(owner),
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    // The lease is valid — it was issued by the owning workspace's reference —
    // but it is presented under another workspace's context. Row-level security
    // must not return the row, and even if it did the associated data would not
    // match.
    let leaked = lease(&reference, owner, "provider-api-key");
    let result = store.resolve(&context(intruder), &leaked).await;

    assert!(
        result.is_err(),
        "a secret resolved across a workspace boundary"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_domain_refuses_a_lease_for_the_wrong_purpose(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool), master_key("v1"));

    let reference = store
        .put(
            &context(workspace),
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    // Authorization happens before any storage access: a caller asking for the
    // export-signing key cannot be handed the provider key.
    let denied = reference.authorize_resolution(&SecretResolutionRequest::new(
        workspace,
        "export-signing",
        "test-authorization",
    ));

    assert!(denied.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_different_master_key_cannot_read_stored_secrets(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let reference = {
        let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));
        store
            .put(
                &context(workspace),
                "openai",
                "provider-api-key",
                SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
            )
            .await
            .unwrap()
    };

    // A process started with the wrong master key sees the rows and can read
    // nothing — which is the whole point of keeping the key out of the database.
    use base64::Engine as _;
    let other = Arc::new(
        MasterKey::from_base64(
            &base64::engine::general_purpose::STANDARD.encode([9u8; 32]),
            "v1",
        )
        .unwrap(),
    );
    let store = PgSecretStore::new(PgStore::from_pool(pool), other);

    let result = store
        .resolve(
            &context(workspace),
            &lease(&reference, workspace, "provider-api-key"),
        )
        .await;

    // Matched rather than unwrapped because `SecretMaterial` deliberately has
    // no `Debug`, which is what stops it being printed anywhere — including by
    // `unwrap_err`.
    let message = match result {
        Ok(_) => panic!("a secret decrypted under the wrong master key"),
        Err(error) => error.to_string(),
    };
    // The error must not quote the material, and must not distinguish a wrong
    // key from a missing row.
    assert!(!message.contains(PLAINTEXT));
}

#[sqlx::test(migrations = "../../migrations")]
async fn tampering_with_the_stored_row_makes_it_unreadable_rather_than_wrong(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));
    let context = context(workspace);

    let reference = store
        .put(
            &context,
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    // Someone with write access to the database relabels the secret's purpose,
    // hoping a caller authorized for a different purpose will be handed it.
    sqlx::query("UPDATE workspace_secrets SET purpose = $1 WHERE id = $2")
        .bind("export-signing")
        .bind(reference.id().as_uuid())
        .execute(&pool)
        .await
        .unwrap();

    let result = store
        .resolve(&context, &lease(&reference, workspace, "provider-api-key"))
        .await;

    assert!(
        result.is_err(),
        "a relabelled secret decrypted into its new context"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn listing_reports_what_exists_and_never_the_material(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool), master_key("v3"));
    let context = context(workspace);

    store
        .put(
            &context,
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    let listed = store.list(&context).await.unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "openai");
    assert_eq!(listed[0].purpose, "provider-api-key");
    assert_eq!(listed[0].key_version, "v3");
    // A descriptor has no field that could carry material; this asserts the
    // rendered form stays clean if one is ever added.
    assert!(!format!("{:?}", listed[0]).contains(PLAINTEXT));
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_is_idempotent_and_removes_the_ciphertext(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));
    let context = context(workspace);

    let reference = store
        .put(
            &context,
            "openai",
            "provider-api-key",
            SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
        )
        .await
        .unwrap();

    store.delete(&context, reference.id()).await.unwrap();
    // Deleting again satisfies the same intent and is not an error.
    store.delete(&context, reference.id()).await.unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspace_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_workspace_gets_its_own_data_key(pool: PgPool) {
    let first = WorkspaceId::new();
    let second = WorkspaceId::new();
    seed_workspace(&pool, first).await;
    seed_workspace(&pool, second).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));

    for workspace in [first, second] {
        store
            .put(
                &context(workspace),
                "openai",
                "provider-api-key",
                SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
            )
            .await
            .unwrap();
    }

    let wrapped: Vec<Vec<u8>> = sqlx::query_scalar("SELECT wrapped_dek FROM workspace_keks")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(wrapped.len(), 2, "expected one data key per workspace");
    assert_ne!(
        wrapped[0], wrapped[1],
        "workspaces must not share a data key"
    );
    assert!(
        wrapped.iter().all(|key| key.len() == 48),
        "a wrapped 256-bit key is 32 bytes plus a 16-byte tag"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn storing_two_secrets_reuses_the_workspace_data_key(pool: PgPool) {
    let workspace = WorkspaceId::new();
    seed_workspace(&pool, workspace).await;
    let store = PgSecretStore::new(PgStore::from_pool(pool.clone()), master_key("v1"));
    let context = context(workspace);

    for name in ["openai", "anthropic"] {
        store
            .put(
                &context,
                name,
                "provider-api-key",
                SecretMaterial::new(PLAINTEXT.as_bytes().to_vec()),
            )
            .await
            .unwrap();
    }

    let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspace_keks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(keys, 1, "a second secret must not mint a second data key");

    // Two secrets under one key must still use distinct nonces.
    let nonces: Vec<Vec<u8>> = sqlx::query_scalar("SELECT nonce FROM workspace_secrets")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(nonces.len(), 2);
    assert_ne!(nonces[0], nonces[1]);
}
