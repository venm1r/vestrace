use std::sync::Arc;

use anyhow::{Context, anyhow};
use secrecy::ExposeSecret;
use sqlx::Row;
use vestrace_application::{DoctorService, RequestContext};
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{AppConfig, PgDiagnosticsRepository, PgStore};

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum RebuildTarget {
    SearchDocuments,
    Embeddings,
    All,
}

pub async fn run(config: &AppConfig, target: &RebuildTarget) -> anyhow::Result<()> {
    let url = config.database.url.expose_secret();
    let redacted = redact_url(url);

    print!("Connecting to database at {redacted} ... ");
    let store = PgStore::connect(&config.database)
        .await
        .context("database is unavailable")?;
    println!("OK");

    let pool = store.pool().clone();

    match target {
        RebuildTarget::SearchDocuments | RebuildTarget::All => {
            print!("Rebuilding search documents ... ");
            let count = rebuild_search_documents(&pool).await?;
            println!("done ({count} documents rebuilt)");
        }
        _ => {}
    }

    match target {
        RebuildTarget::Embeddings | RebuildTarget::All => {
            print!("Rebuilding embeddings ... ");
            let count = rebuild_embeddings(&pool).await?;
            println!("done ({count} embeddings rebuilt)");
        }
        _ => {}
    }

    println!();
    println!("Running post-rebuild verification ... ");

    let repo = Arc::new(PgDiagnosticsRepository::new(pool));
    let doctor = DoctorService::new(repo);

    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let ctx = RequestContext::new(workspace_id, principal_id);

    let report = doctor
        .run_checks(&ctx)
        .await
        .context("post-rebuild verification failed")?;

    if report.is_clean() {
        println!("Verification: all checks passed");
        Ok(())
    } else if report.has_errors() {
        for finding in report.errors() {
            println!("[ERROR] {}: {}", finding.code, finding.message);
        }
        Err(anyhow!("verification found errors after rebuild"))
    } else {
        for finding in report.warnings() {
            println!("[WARN]  {}: {}", finding.code, finding.message);
        }
        println!("Verification: warnings found, no errors");
        Ok(())
    }
}

async fn rebuild_search_documents(pool: &sqlx::PgPool) -> Result<i64, anyhow::Error> {
    let row = sqlx::query(
        r#"
        INSERT INTO search_documents (id, memory_id, workspace_id, content, search_tsv, projection_version)
        SELECT
            gen_random_uuid(),
            m.id,
            m.workspace_id,
            m.content_text,
            to_tsvector('english', m.content_text),
            1
        FROM memories m
        LEFT JOIN search_documents sd ON sd.memory_id = m.id
        WHERE m.status = 'active' AND sd.id IS NULL
        ON CONFLICT DO NOTHING
        RETURNING 1
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(row.len() as i64)
}

async fn rebuild_embeddings(pool: &sqlx::PgPool) -> Result<i64, anyhow::Error> {
    let row = sqlx::query(
        r#"
        SELECT count(*) AS missing_count
        FROM memories m
        LEFT JOIN search_documents sd ON sd.memory_id = m.id
        WHERE m.status = 'active' AND sd.id IS NULL
        "#,
    )
    .fetch_one(pool)
    .await?;

    let missing: i64 = row.get("missing_count");
    if missing > 0 {
        return Err(anyhow!(
            "{missing} memories still missing search documents — rebuild search-documents first"
        ));
    }

    Ok(0)
}

fn redact_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(scheme_end) = url.find("://") {
            let scheme = &url[..scheme_end + 3];
            let host = &url[at_pos + 1..];
            return format!("{scheme}***@{host}");
        }
    }
    url.to_string()
}
