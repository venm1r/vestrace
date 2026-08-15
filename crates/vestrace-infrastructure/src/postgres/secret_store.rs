use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::{PgConnection, Row};
use vestrace_application::{
    ApplicationError, RequestContext, SecretDescriptor, SecretMaterial, SecretStore,
};
use vestrace_domain::trust::{SecretLease, SecretRef};

use super::PgStore;
use crate::crypto::{
    ALGORITHM_SUITE, EnvelopeCipher, KEY_LENGTH, LOCAL_FILE_PROVIDER, MasterKey, Sealed,
    data_key_associated_data, secret_associated_data,
};

/// The alias of the data key each workspace's secrets are encrypted under.
///
/// One per workspace for now. The column is part of the uniqueness constraint
/// so a second alias can be introduced later without a migration.
const DEFAULT_KEY_ALIAS: &str = "workspace-default";

/// Envelope-encrypted secret storage on PostgreSQL.
///
/// See [`crate::crypto`] for the construction and for what this custody model
/// does and does not protect against.
pub struct PgSecretStore {
    store: PgStore,
    master: Arc<MasterKey>,
    cipher: EnvelopeCipher,
}

impl std::fmt::Debug for PgSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PgSecretStore")
            .field("key_version", &self.master.version())
            .finish()
    }
}

impl PgSecretStore {
    pub fn new(store: PgStore, master: Arc<MasterKey>) -> Self {
        Self {
            store,
            master,
            cipher: EnvelopeCipher::new(),
        }
    }

    /// Fetch the workspace data key, provisioning one on first use.
    ///
    /// Returns the key row id and the unwrapped key. The unwrapped key is
    /// returned by value and dropped by the caller as soon as one operation is
    /// done with it, rather than cached on `self` — a long-lived plaintext key
    /// in process memory is the thing an envelope is supposed to avoid.
    async fn data_key(
        &self,
        connection: &mut PgConnection,
        workspace_id: uuid::Uuid,
    ) -> Result<(uuid::Uuid, DataKey), ApplicationError> {
        let existing = sqlx::query(
            r#"
            SELECT id, wrapped_dek, wrap_nonce, kek_version
            FROM workspace_keks
            WHERE workspace_id = $1 AND key_alias = $2
            "#,
        )
        .bind(workspace_id)
        .bind(DEFAULT_KEY_ALIAS)
        .fetch_optional(&mut *connection)
        .await
        .map_err(storage_error)?;

        if let Some(row) = existing {
            let id: uuid::Uuid = row.try_get("id").map_err(storage_error)?;
            let wrapped: Option<Vec<u8>> = row.try_get("wrapped_dek").map_err(storage_error)?;
            let nonce: Option<Vec<u8>> = row.try_get("wrap_nonce").map_err(storage_error)?;
            let version: Option<String> = row.try_get("kek_version").map_err(storage_error)?;

            // Migration 0087 created this table without key material, so a row
            // may predate the envelope. Treat it as unprovisioned and fill it
            // rather than failing, but never treat a *partial* row as usable.
            if let (Some(wrapped), Some(nonce), Some(version)) = (wrapped, nonce, version) {
                let unwrapped = self.unwrap_data_key(workspace_id, &wrapped, &nonce, &version)?;
                return Ok((id, unwrapped));
            }
            let (wrapped, nonce, key) = self.wrap_fresh_data_key(workspace_id)?;
            sqlx::query(
                r#"
                UPDATE workspace_keks
                SET wrapped_dek = $2, wrap_nonce = $3, kek_version = $4, algorithm = $5
                WHERE id = $1
                "#,
            )
            .bind(id)
            .bind(&wrapped)
            .bind(&nonce)
            .bind(self.master.version())
            .bind(ALGORITHM_SUITE)
            .execute(&mut *connection)
            .await
            .map_err(storage_error)?;
            return Ok((id, key));
        }

        let id = uuid::Uuid::now_v7();
        let (wrapped, nonce, key) = self.wrap_fresh_data_key(workspace_id)?;
        sqlx::query(
            r#"
            INSERT INTO workspace_keks
                (id, workspace_id, key_alias, algorithm, wrapped_dek, wrap_nonce, kek_version)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(DEFAULT_KEY_ALIAS)
        .bind(ALGORITHM_SUITE)
        .bind(&wrapped)
        .bind(&nonce)
        .bind(self.master.version())
        .execute(&mut *connection)
        .await
        .map_err(storage_error)?;
        Ok((id, key))
    }

    fn wrap_fresh_data_key(
        &self,
        workspace_id: uuid::Uuid,
    ) -> Result<(Vec<u8>, Vec<u8>, DataKey), ApplicationError> {
        let key = self.cipher.generate_data_key().map_err(crypto_error)?;
        let associated = data_key_associated_data(workspace_id, self.master.version());
        let sealed = self
            .cipher
            .seal(self.master.material(), &associated, &key)
            .map_err(crypto_error)?;
        Ok((sealed.ciphertext, sealed.nonce.to_vec(), DataKey::new(key)))
    }

    fn unwrap_data_key(
        &self,
        workspace_id: uuid::Uuid,
        wrapped: &[u8],
        nonce: &[u8],
        version: &str,
    ) -> Result<DataKey, ApplicationError> {
        let nonce: [u8; ring::aead::NONCE_LEN] = nonce.try_into().map_err(|_| {
            ApplicationError::Storage("stored data key nonce has the wrong length".into())
        })?;
        // The version the key was wrapped under, not the current one: after a
        // master-key rotation, keys not yet rewrapped must still open.
        let associated = data_key_associated_data(workspace_id, version);
        let opened = self
            .cipher
            .open(
                self.master.material(),
                &associated,
                &Sealed {
                    nonce,
                    ciphertext: wrapped.to_vec(),
                },
            )
            .map_err(crypto_error)?;
        let key: [u8; KEY_LENGTH] = opened.as_slice().try_into().map_err(|_| {
            ApplicationError::Internal("unwrapped data key has the wrong length".into())
        })?;
        Ok(DataKey::new(key))
    }

    fn reference_for(
        &self,
        id: vestrace_domain::SecretRefId,
        workspace_id: vestrace_domain::WorkspaceId,
        name: &str,
        purpose: &str,
    ) -> Result<SecretRef, ApplicationError> {
        let mut metadata = BTreeMap::new();
        metadata.insert("name".to_string(), name.to_string());
        metadata.insert("algorithm".to_string(), ALGORITHM_SUITE.to_string());
        SecretRef::rehydrate(
            id,
            // Opaque by construction: it names where the secret lives, never
            // any part of its value.
            format!("secret://workspace/{workspace_id}/{purpose}/{name}"),
            LOCAL_FILE_PROVIDER,
            workspace_id,
            purpose,
            metadata,
            Some(self.master.version().to_string()),
        )
        .map_err(ApplicationError::Domain)
    }
}

/// An unwrapped data key, wiped when it goes out of scope.
struct DataKey {
    bytes: [u8; KEY_LENGTH],
}

impl DataKey {
    fn new(bytes: [u8; KEY_LENGTH]) -> Self {
        Self { bytes }
    }

    fn material(&self) -> &[u8; KEY_LENGTH] {
        &self.bytes
    }
}

impl Drop for DataKey {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        self.bytes.zeroize();
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

/// Crypto failures are reported without detail. Which of "wrong key",
/// "tampered", or "moved row" occurred is exactly what an attacker probing the
/// store would want to learn.
fn crypto_error(error: crate::crypto::CryptoError) -> ApplicationError {
    ApplicationError::Internal(error.to_string())
}

#[async_trait]
impl SecretStore for PgSecretStore {
    async fn put(
        &self,
        context: &RequestContext,
        name: &str,
        purpose: &str,
        material: SecretMaterial,
    ) -> Result<SecretRef, ApplicationError> {
        let workspace_uuid = context.workspace_id.as_uuid();
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let (kek_id, data_key) = self
            .data_key(transaction.connection(), workspace_uuid)
            .await?;

        // Reuse the existing id when replacing, because the id is bound into
        // the associated data and is what any issued lease names.
        let existing: Option<uuid::Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM workspace_secrets
            WHERE workspace_id = $1 AND purpose = $2 AND name = $3
            "#,
        )
        .bind(workspace_uuid)
        .bind(purpose)
        .bind(name)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        let secret_id = existing.unwrap_or_else(uuid::Uuid::now_v7);

        let associated = secret_associated_data(workspace_uuid, secret_id, purpose);
        let sealed = self
            .cipher
            .seal(data_key.material(), &associated, material.expose())
            .map_err(crypto_error)?;
        drop(material);
        drop(data_key);

        sqlx::query(
            r#"
            INSERT INTO workspace_secrets
                (id, workspace_id, kek_id, name, purpose, nonce, ciphertext)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (workspace_id, purpose, name) DO UPDATE
            SET nonce = EXCLUDED.nonce,
                ciphertext = EXCLUDED.ciphertext,
                kek_id = EXCLUDED.kek_id,
                updated_at = NOW()
            "#,
        )
        .bind(secret_id)
        .bind(workspace_uuid)
        .bind(kek_id)
        .bind(name)
        .bind(purpose)
        .bind(sealed.nonce.as_slice())
        .bind(&sealed.ciphertext)
        .execute(transaction.connection())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;
        self.reference_for(
            vestrace_domain::SecretRefId::from_uuid(secret_id),
            context.workspace_id,
            name,
            purpose,
        )
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<SecretDescriptor>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // Selects no ciphertext: a listing has no reason to load it, and not
        // loading it means it cannot be accidentally serialised into a response.
        let rows = sqlx::query(
            r#"
            SELECT s.id, s.name, s.purpose, s.created_at, s.updated_at,
                   k.kek_version
            FROM workspace_secrets AS s
            JOIN workspace_keks AS k ON k.id = s.kek_id
            WHERE s.workspace_id = $1
            ORDER BY s.purpose, s.name
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(SecretDescriptor {
                    id: vestrace_domain::SecretRefId::from_uuid(
                        row.try_get("id").map_err(storage_error)?,
                    ),
                    name: row.try_get("name").map_err(storage_error)?,
                    purpose: row.try_get("purpose").map_err(storage_error)?,
                    key_version: row
                        .try_get::<Option<String>, _>("kek_version")
                        .map_err(storage_error)?
                        .unwrap_or_else(|| "unprovisioned".to_string()),
                    created_at: row.try_get("created_at").map_err(storage_error)?,
                    updated_at: row.try_get("updated_at").map_err(storage_error)?,
                })
            })
            .collect()
    }

    async fn find(
        &self,
        context: &RequestContext,
        name: &str,
        purpose: &str,
    ) -> Result<Option<SecretRef>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        // Selects the id only: a lookup has no reason to load ciphertext.
        let id: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM workspace_secrets WHERE workspace_id = $1 AND purpose = $2 AND name = $3",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(purpose)
        .bind(name)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        id.map(|id| {
            self.reference_for(
                vestrace_domain::SecretRefId::from_uuid(id),
                context.workspace_id,
                name,
                purpose,
            )
        })
        .transpose()
    }

    async fn resolve(
        &self,
        context: &RequestContext,
        lease: &SecretLease,
    ) -> Result<SecretMaterial, ApplicationError> {
        let workspace_uuid = context.workspace_id.as_uuid();
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT s.purpose, s.nonce, s.ciphertext,
                   k.wrapped_dek, k.wrap_nonce, k.kek_version
            FROM workspace_secrets AS s
            JOIN workspace_keks AS k ON k.id = s.kek_id
            WHERE s.id = $1 AND s.workspace_id = $2
            "#,
        )
        .bind(lease.secret_ref_id().as_uuid())
        .bind(workspace_uuid)
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;

        let row = row.ok_or_else(|| {
            // Says the lease does not resolve, not whether the id exists in
            // another workspace.
            ApplicationError::Storage("no secret resolves for this lease".into())
        })?;

        let wrapped: Vec<u8> = row.try_get("wrapped_dek").map_err(storage_error)?;
        let wrap_nonce: Vec<u8> = row.try_get("wrap_nonce").map_err(storage_error)?;
        let key_version: String = row.try_get("kek_version").map_err(storage_error)?;
        let data_key = self.unwrap_data_key(workspace_uuid, &wrapped, &wrap_nonce, &key_version)?;

        let purpose: String = row.try_get("purpose").map_err(storage_error)?;
        let nonce: Vec<u8> = row.try_get("nonce").map_err(storage_error)?;
        let ciphertext: Vec<u8> = row.try_get("ciphertext").map_err(storage_error)?;
        let nonce: [u8; ring::aead::NONCE_LEN] = nonce.as_slice().try_into().map_err(|_| {
            ApplicationError::Storage("stored secret nonce has the wrong length".into())
        })?;

        let associated =
            secret_associated_data(workspace_uuid, lease.secret_ref_id().as_uuid(), &purpose);
        let plaintext = self
            .cipher
            .open(
                data_key.material(),
                &associated,
                &Sealed { nonce, ciphertext },
            )
            .map_err(crypto_error)?;
        Ok(SecretMaterial::new(plaintext))
    }

    async fn delete(
        &self,
        context: &RequestContext,
        id: vestrace_domain::SecretRefId,
    ) -> Result<(), ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        sqlx::query("DELETE FROM workspace_secrets WHERE id = $1 AND workspace_id = $2")
            .bind(id.as_uuid())
            .bind(context.workspace_id.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }
}
