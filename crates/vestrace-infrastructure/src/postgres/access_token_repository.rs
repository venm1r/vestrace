//! Postgres adapters for authentication credentials.
//!
//! Two adapters, matching the two ports, because they need different database
//! privileges:
//!
//! * [`PgAccessTokenAuthenticator`] runs the two `SECURITY DEFINER` functions
//!   from migration 0135. Resolving a credential cannot be workspace scoped —
//!   the workspace is what the credential tells us — so this is the one path
//!   that reads outside a scope, and it is confined to functions that return
//!   three identifiers and nothing else.
//!
//! * [`PgAccessTokenStore`] administers credentials inside a scoped
//!   transaction like every other store, because a caller minting or revoking
//!   one has already authenticated.

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{
    AccessTokenAuthenticator, AccessTokenStore, ApplicationError, AuthenticatedPrincipal,
    RequestContext, UnitOfWork,
};
use vestrace_domain::identity::AccessToken;
use vestrace_domain::{AccessTokenId, PrincipalId, Timestamp, WorkspaceId};

use super::{PgScopedTransaction, PgStore};

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

pub struct PgAccessTokenAuthenticator {
    pool: PgPool,
}

impl PgAccessTokenAuthenticator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl std::fmt::Debug for PgAccessTokenAuthenticator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Explicit rather than derived, so that adding a cached credential
        // here later cannot start printing one.
        formatter
            .debug_struct("PgAccessTokenAuthenticator")
            .finish()
    }
}

#[async_trait]
impl AccessTokenAuthenticator for PgAccessTokenAuthenticator {
    async fn resolve(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthenticatedPrincipal>, ApplicationError> {
        // A transaction because the visibility of the row depends on a
        // transaction-local setting. `access_tokens` forces row level security
        // and the authentication lookup cannot be workspace scoped — the
        // workspace is what the credential tells us — so migration 0136 grants
        // visibility of exactly the row whose hash the caller already knows.
        // The setting is local: it cannot leak to the next query on this
        // connection.
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind("vestrace.authenticating_token_hash")
            .bind(token_hash)
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage_error)?;

        // Expiry and revocation are applied by the function, so a dead
        // credential returns no row rather than a row this adapter has to
        // remember to filter.
        let row = sqlx::query(
            "SELECT token_id, workspace_id, principal_id FROM vestrace_resolve_access_token($1)",
        )
        .bind(token_hash)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;

        Ok(row.map(|row| AuthenticatedPrincipal {
            token_id: AccessTokenId::from_uuid(row.get("token_id")),
            workspace_id: WorkspaceId::from_uuid(row.get("workspace_id")),
            principal_id: PrincipalId::from_uuid(row.get("principal_id")),
        }))
    }

    async fn record_use(&self, token_id: AccessTokenId) -> Result<(), ApplicationError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind("vestrace.authenticating_token_id")
            .bind(token_id.as_uuid().to_string())
            .fetch_one(&mut *transaction)
            .await
            .map_err(storage_error)?;

        sqlx::query("SELECT vestrace_touch_access_token($1)")
            .bind(token_id.as_uuid())
            .execute(&mut *transaction)
            .await
            .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)
    }
}

pub struct PgAccessTokenStore {
    store: PgStore,
}

impl PgAccessTokenStore {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn row_to_token(row: &sqlx::postgres::PgRow) -> AccessToken {
    AccessToken {
        id: AccessTokenId::from_uuid(row.get("id")),
        workspace_id: WorkspaceId::from_uuid(row.get("workspace_id")),
        principal_id: PrincipalId::from_uuid(row.get("principal_id")),
        token_hash: row.get("token_hash"),
        label: row.get("label"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
        last_used_at: row.get("last_used_at"),
    }
}

const COLUMNS: &str = "id, workspace_id, principal_id, token_hash, label, created_at, \
                       expires_at, revoked_at, last_used_at";

fn transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected a PostgreSQL unit of work".to_owned()))
}

#[async_trait]
impl AccessTokenStore for PgAccessTokenStore {
    async fn put(
        &self,
        context: &RequestContext,
        token: &AccessToken,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        self.put_in(context, &mut scoped, token).await?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn put_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        token: &AccessToken,
    ) -> Result<(), ApplicationError> {
        // Refused rather than silently accepted: a credential minted into
        // another workspace would authenticate a caller the minting operator
        // has no authority over.
        if token.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "an access token cannot be created in another workspace".into(),
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO access_tokens
                (id, workspace_id, principal_id, token_hash, label, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(token.id.as_uuid())
        .bind(token.workspace_id.as_uuid())
        .bind(token.principal_id.as_uuid())
        .bind(&token.token_hash)
        .bind(&token.label)
        .bind(token.created_at)
        .bind(token.expires_at)
        .execute(transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;

        Ok(())
    }

    async fn list(&self, context: &RequestContext) -> Result<Vec<AccessToken>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // Revoked rows included on purpose: an access review needs to see what
        // was withdrawn and when, not a list that reads as though it never was.
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM access_tokens WHERE workspace_id = $1 ORDER BY created_at DESC"
        ))
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        let tokens = rows.iter().map(row_to_token).collect();
        scoped.commit().await.map_err(storage_error)?;
        Ok(tokens)
    }

    async fn find(
        &self,
        context: &RequestContext,
        id: AccessTokenId,
    ) -> Result<Option<AccessToken>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM access_tokens WHERE workspace_id = $1 AND id = $2"
        ))
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        let token = row.as_ref().map(row_to_token);
        scoped.commit().await.map_err(storage_error)?;
        Ok(token)
    }

    async fn revoke(
        &self,
        context: &RequestContext,
        id: AccessTokenId,
        at: Timestamp,
    ) -> Result<(), ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // `revoked_at IS NULL` in the predicate rather than a blind UPDATE:
        // re-revoking would move the timestamp and lose when the credential
        // actually stopped working.
        let affected = sqlx::query(
            r#"
            UPDATE access_tokens
            SET revoked_at = $3
            WHERE workspace_id = $1 AND id = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .bind(id.as_uuid())
        .bind(at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?
        .rows_affected();

        scoped.commit().await.map_err(storage_error)?;

        if affected == 0 {
            // One message for "no such token" and "already revoked": the
            // caller's next action is the same either way, and distinguishing
            // them tells an unauthorized caller which ids exist.
            // Reported as a conflict rather than a policy failure: the caller
            // is permitted to revoke here, the credential is simply not in a
            // state that can be revoked.
            return Err(ApplicationError::Conflict(
                "no live access token with that id exists in this workspace".into(),
            ));
        }
        Ok(())
    }
}
