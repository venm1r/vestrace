use async_trait::async_trait;
use sqlx::{PgPool, Row};
use vestrace_application::{ApplicationError, DiagnosticsRepository, RequestContext};
use vestrace_domain::diagnostics::DiagnosticFinding;

pub struct PgDiagnosticsRepository {
    pool: PgPool,
}

impl PgDiagnosticsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait]
impl DiagnosticsRepository for PgDiagnosticsRepository {
    async fn check_migrations(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT
                (SELECT count(*) FROM _sqlx_migrations WHERE success = false) AS failed_count,
                (SELECT count(*) FROM _sqlx_migrations) AS applied_count
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut findings = Vec::new();

        let failed_count: i64 = row.get("failed_count");
        if failed_count > 0 {
            findings.push(DiagnosticFinding::error(
                "MIGRATION_FAILED",
                None,
                &format!("{failed_count} migrations failed to apply"),
                "inspect _sqlx_migrations table and re-run `vestrace migrate`",
            ));
        }

        Ok(findings)
    }

    async fn check_extensions(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT extname FROM pg_extension WHERE extname IN ('pgcrypto', 'vector', 'pg_trgm')
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let installed: Vec<String> = rows
            .into_iter()
            .map(|r| r.get::<String, _>("extname"))
            .collect();
        let mut findings = Vec::new();

        for required in &["pgcrypto", "vector", "pg_trgm"] {
            if !installed.iter().any(|e| e == *required) {
                findings.push(DiagnosticFinding::error(
                    "EXTENSION_MISSING",
                    None,
                    &format!("required extension '{required}' is not installed"),
                    &format!("run: CREATE EXTENSION IF NOT EXISTS \"{required}\";"),
                ));
            }
        }

        Ok(findings)
    }

    async fn check_broken_references(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let mut findings = Vec::new();

        let rows = sqlx::query(
            r#"
            SELECT m.id AS memory_id
            FROM memories m
            LEFT JOIN memory_sources ms ON ms.memory_id = m.id
            WHERE m.workspace_id = $1
              AND m.status = 'active'
              AND ms.id IS NULL
            LIMIT 20
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        for row in rows {
            let id: uuid::Uuid = row.get("memory_id");
            findings.push(DiagnosticFinding::warning(
                "BROKEN_REFERENCE",
                Some(vestrace_domain::diagnostics::KnowledgeRef::Memory(
                    vestrace_domain::id::MemoryId::from_uuid(id),
                )),
                "active memory has no provenance source record",
                "inspect the memory and re-link a source or mark as archived",
            ));
        }

        let rows = sqlx::query(
            r#"
            SELECT m.id AS memory_id
            FROM memories m
            LEFT JOIN search_documents sd ON sd.memory_id = m.id
            WHERE m.workspace_id = $1
              AND m.status = 'active'
              AND sd.id IS NULL
            LIMIT 20
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        for row in rows {
            let id: uuid::Uuid = row.get("memory_id");
            findings.push(DiagnosticFinding::warning(
                "BROKEN_REFERENCE",
                Some(vestrace_domain::diagnostics::KnowledgeRef::Memory(
                    vestrace_domain::id::MemoryId::from_uuid(id),
                )),
                "active memory has no search document",
                "run `vestrace rebuild search-documents`",
            ));
        }

        Ok(findings)
    }

    async fn check_outbox_lag(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT count(*) AS pending_count
            FROM outbox
            WHERE workspace_id = $1
              AND processed_at IS NULL
              AND created_at < NOW() - INTERVAL '60 seconds'
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut findings = Vec::new();
        let pending_count: i64 = row.get("pending_count");
        if pending_count > 0 {
            findings.push(DiagnosticFinding::warning(
                "OUTBOX_LAG",
                None,
                &format!("{pending_count} outbox messages pending for more than 60 seconds"),
                "process the outbox queue or check worker health",
            ));
        }

        Ok(findings)
    }

    async fn check_missing_search_documents(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT count(*) AS missing_count
            FROM memories m
            LEFT JOIN search_documents sd ON sd.memory_id = m.id
            WHERE m.workspace_id = $1
              AND m.status = 'active'
              AND sd.id IS NULL
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut findings = Vec::new();
        let missing_count: i64 = row.get("missing_count");
        if missing_count > 0 {
            findings.push(DiagnosticFinding::warning(
                "MISSING_SEARCH_DOCUMENT",
                None,
                &format!("{missing_count} active memories have no search document"),
                "run `vestrace rebuild search-documents`",
            ));
        }

        Ok(findings)
    }

    async fn check_stale_model_health(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT count(*) AS stale_count
            FROM model_executions
            WHERE workspace_id = $1
              AND status = 'failed'
              AND created_at > NOW() - INTERVAL '24 hours'
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut findings = Vec::new();
        let stale_count: i64 = row.get("stale_count");
        if stale_count > 5 {
            findings.push(DiagnosticFinding::warning(
                "STALE_MODEL_HEALTH",
                None,
                &format!("{stale_count} model execution failures in the last 24 hours"),
                "review model provider configuration and health",
            ));
        }

        Ok(findings)
    }

    async fn check_expired_leases(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT count(*) AS expired_count
            FROM run_leases
            WHERE workspace_id = $1
              AND expires_at < NOW()
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut findings = Vec::new();
        let expired_count: i64 = row.get("expired_count");
        if expired_count > 0 {
            findings.push(DiagnosticFinding::warning(
                "EXPIRED_LEASE",
                None,
                &format!("{expired_count} run leases have expired"),
                "renew or clean up stale run leases",
            ));
        }

        Ok(findings)
    }

    async fn check_dead_letter_jobs(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<DiagnosticFinding>, ApplicationError> {
        let row = sqlx::query(
            r#"
            SELECT count(*) AS dead_letter_count
            FROM jobs
            WHERE workspace_id = $1
              AND state = 'dead_lettered'
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut findings = Vec::new();
        let dead_letter_count: i64 = row.get("dead_letter_count");
        if dead_letter_count > 0 {
            findings.push(DiagnosticFinding::warning(
                "DEAD_LETTER_JOB",
                None,
                &format!("{dead_letter_count} jobs in dead-letter state"),
                "inspect and retry or discard dead-lettered jobs",
            ));
        }

        Ok(findings)
    }
}
