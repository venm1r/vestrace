use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use crate::AppState;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list_runs).post(create_run))
        .route("/runs/:id", get(get_run))
        .route("/runs/:id/approve", post(approve_run))
        .route("/artifacts", get(list_artifacts))
        .route("/agents", get(list_agents))
        .route("/workflows", get(list_workflows))
        .route("/triggers", get(list_triggers))
        .route("/connections", get(list_connections))
        .route("/models", get(list_models))
        .route("/evaluations", get(list_evaluations))
        .route("/audit", get(list_audit_events))
        .route("/metrics/summary", get(get_metrics_summary))
        .route("/system/health", get(get_system_health))
        .route("/profile", get(get_profile))
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceQuery {
    pub workspace_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRunPayload {
    pub prompt: String,
    pub agent_id: Option<String>,
    pub workflow_id: Option<String>,
    pub autonomy_level: Option<String>,
    pub budget_limit: Option<f64>,
}

async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    let default_workspace_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let default_principal_id = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

    let pool = match state.health_repository().as_any().downcast_ref::<sqlx::Pool<sqlx::Postgres>>() {
        Some(p) => p,
        None => return mock_runs_fallback(),
    };

    // Ensure seed workspace & principal exist for live DB queries
    let _ = sqlx::query(
        "INSERT INTO workspaces (id, name, slug) VALUES ($1, 'Default Workspace', 'default') ON CONFLICT DO NOTHING"
    )
    .bind(default_workspace_id)
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "INSERT INTO principals (id, workspace_id, name, principal_type) VALUES ($1, $2, 'Operator Admin', 'user') ON CONFLICT DO NOTHING"
    )
    .bind(default_principal_id)
    .bind(default_workspace_id)
    .execute(pool)
    .await;

    // Set RLS session variable
    let _ = sqlx::query("SELECT set_config('vestrace.workspace_id', $1, false)")
        .bind(default_workspace_id.to_string())
        .execute(pool)
        .await;

    let rows = sqlx::query(
        "SELECT id, title, status, run_version, created_at FROM agent_runs ORDER BY created_at DESC LIMIT 50"
    )
    .fetch_all(pool)
    .await;

    match rows {
        Ok(records) if !records.is_empty() => {
            let runs: Vec<serde_json::Value> = records
                .into_iter()
                .map(|r| {
                    let id: Uuid = r.get("id");
                    let title: String = r.get("title");
                    let status: String = r.get("status");
                    let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
                    json!({
                        "id": id.to_string(),
                        "name": title,
                        "agent": "Compliance-Bot v2",
                        "status": if status == "created" { "Running" } else { &status },
                        "progress": 65,
                        "tokens_used": 14200,
                        "cost": "$0.042",
                        "duration": "1m 24s",
                        "created_at": created_at.to_rfc3339()
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!(runs)))
        }
        _ => mock_runs_fallback(),
    }
}

fn mock_runs_fallback() -> (StatusCode, Json<serde_json::Value>) {
    let mock_runs = json!([
        {
            "id": "0194f4a0-7b3c-7000-8000-000000000001",
            "name": "Audit Data Retention Policy Enforcement",
            "agent": "Compliance-Bot v2",
            "status": "Running",
            "progress": 65,
            "tokens_used": 14200,
            "cost": "$0.042",
            "duration": "1m 24s",
            "created_at": "2026-08-03T10:14:22Z"
        },
        {
            "id": "0194f4a0-7b3c-7000-8000-000000000002",
            "name": "Postgres Index Optimization & Vacuum",
            "agent": "DB-Optimizer",
            "status": "WaitingApproval",
            "progress": 40,
            "tokens_used": 8900,
            "cost": "$0.018",
            "duration": "45s",
            "created_at": "2026-08-03T09:55:00Z"
        },
        {
            "id": "0194f4a0-7b3c-7000-8000-000000000003",
            "name": "Daily Knowledge Graph Extraction",
            "agent": "Memory-Extractor",
            "status": "Completed",
            "progress": 100,
            "tokens_used": 54100,
            "cost": "$0.162",
            "duration": "4m 12s",
            "created_at": "2026-08-03T08:30:11Z"
        }
    ]);
    (StatusCode::OK, Json(mock_runs))
}

async fn create_run(
    State(state): State<AppState>,
    Json(payload): Json<CreateRunPayload>,
) -> impl IntoResponse {
    let run_id = Uuid::now_v7();
    let default_workspace_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let default_principal_id = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

    if let Some(pool) = state.health_repository().as_any().downcast_ref::<sqlx::Pool<sqlx::Postgres>>() {
        let _ = sqlx::query("SELECT set_config('vestrace.workspace_id', $1, false)")
            .bind(default_workspace_id.to_string())
            .execute(pool)
            .await;

        let _ = sqlx::query(
            "INSERT INTO agent_runs (id, workspace_id, principal_id, title, status) VALUES ($1, $2, $3, $4, 'created')"
        )
        .bind(run_id)
        .bind(default_workspace_id)
        .bind(default_principal_id)
        .bind(&payload.prompt)
        .execute(pool)
        .await;
    }

    let new_run = json!({
        "id": run_id.to_string(),
        "name": payload.prompt,
        "agent": payload.agent_id.unwrap_or_else(|| "Default-Agent".into()),
        "status": "Running",
        "progress": 0,
        "tokens_used": 0,
        "cost": "$0.000",
        "duration": "0s",
        "created_at": chrono::Utc::now().to_rfc3339()
    });
    (StatusCode::CREATED, Json(new_run))
}

async fn get_run(Path(id): Path<String>) -> impl IntoResponse {
    let run = json!({
        "id": id,
        "name": "Detailed Task Run Execution",
        "agent": "Compliance-Bot v2",
        "status": "Running",
        "progress": 65,
        "tokens_used": 14200,
        "cost": "$0.042",
        "duration": "1m 24s",
        "created_at": "2026-08-03T10:14:22Z",
        "steps": [
            { "step": 1, "name": "Initialize Context", "status": "Completed" },
            { "step": 2, "name": "Query Database Schema", "status": "Completed" },
            { "step": 3, "name": "Evaluate Compliance Policy", "status": "Running" }
        ]
    });
    (StatusCode::OK, Json(run))
}

async fn approve_run(Path(id): Path<String>) -> impl IntoResponse {
    let result = json!({
        "id": id,
        "status": "Approved",
        "message": "Run approved successfully."
    });
    (StatusCode::OK, Json(result))
}

async fn list_artifacts() -> impl IntoResponse {
    let artifacts = json!([
        {
            "id": "art_01",
            "name": "compliance_audit_2026_08_03.json",
            "kind": "Report",
            "size": "452 KB",
            "created_at": "2026-08-03T10:15:00Z",
            "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        },
        {
            "id": "art_02",
            "name": "optimized_indexes.sql",
            "kind": "SQL Script",
            "size": "12 KB",
            "created_at": "2026-08-03T09:55:30Z",
            "checksum": "sha256:8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4"
        }
    ]);
    (StatusCode::OK, Json(artifacts))
}

async fn list_agents() -> impl IntoResponse {
    let agents = json!([
        { "id": "ag_01", "name": "Compliance-Bot v2", "model": "gpt-4o", "status": "Active", "runs_count": 142 },
        { "id": "ag_02", "name": "DB-Optimizer", "model": "claude-3-5-sonnet", "status": "Active", "runs_count": 89 },
        { "id": "ag_03", "name": "Memory-Extractor", "model": "gpt-4o-mini", "status": "Idle", "runs_count": 512 }
    ]);
    (StatusCode::OK, Json(agents))
}

async fn list_workflows() -> impl IntoResponse {
    let workflows = json!([
        { "id": "wf_01", "name": "Daily Compliance & Cleanup", "steps_count": 5, "last_run": "10 mins ago", "status": "Active" },
        { "id": "wf_02", "name": "Database Health Inspection", "steps_count": 3, "last_run": "1 hour ago", "status": "Active" }
    ]);
    (StatusCode::OK, Json(workflows))
}

async fn list_triggers() -> impl IntoResponse {
    let triggers = json!([
        { "id": "tr_01", "name": "Cron Daily Midnight UTC", "type": "Schedule", "target": "Daily Compliance & Cleanup", "status": "Enabled" },
        { "id": "tr_02", "name": "Webhook GitHub Push Main", "type": "Webhook", "target": "Database Health Inspection", "status": "Enabled" }
    ]);
    (StatusCode::OK, Json(triggers))
}

async fn list_connections() -> impl IntoResponse {
    let connections = json!([
        { "id": "conn_01", "name": "PostgreSQL Primary Pool", "type": "Database", "status": "Healthy", "latency": "2ms" },
        { "id": "conn_02", "name": "OpenAI Provider Gateway", "type": "AI Provider", "status": "Healthy", "latency": "140ms" },
        { "id": "conn_03", "name": "Anthropic Provider Gateway", "type": "AI Provider", "status": "Healthy", "latency": "165ms" }
    ]);
    (StatusCode::OK, Json(connections))
}

async fn list_models() -> impl IntoResponse {
    let models = json!([
        { "id": "mod_01", "provider": "OpenAI", "model_name": "gpt-4o", "context_window": "128k tokens", "cost_input": "$2.50/M", "cost_output": "$10.00/M" },
        { "id": "mod_02", "provider": "Anthropic", "model_name": "claude-3-5-sonnet", "context_window": "200k tokens", "cost_input": "$3.00/M", "cost_output": "$15.00/M" },
        { "id": "mod_03", "provider": "OpenAI", "model_name": "text-embedding-3-small", "context_window": "8k tokens", "cost_input": "$0.02/M", "cost_output": "$0.00/M" }
    ]);
    (StatusCode::OK, Json(models))
}

async fn list_evaluations() -> impl IntoResponse {
    let evals = json!([
        { "id": "ev_01", "suite": "Safety & Alignment Benchmark", "score": "99.4%", "last_evaluated": "2 hours ago", "status": "Passed" },
        { "id": "ev_02", "suite": "JSON Schema Output Compliance", "score": "100.0%", "last_evaluated": "5 hours ago", "status": "Passed" }
    ]);
    (StatusCode::OK, Json(evals))
}

async fn list_audit_events() -> impl IntoResponse {
    let events = json!([
        { "id": "evt_01", "timestamp": "2026-08-03T10:14:22Z", "actor": "admin@vestrace.io", "action": "run.create", "resource": "0194f4a0-7b3c-7000-8000-000000000001" },
        { "id": "evt_02", "timestamp": "2026-08-03T09:55:00Z", "actor": "system.worker", "action": "approval.requested", "resource": "0194f4a0-7b3c-7000-8000-000000000002" }
    ]);
    (StatusCode::OK, Json(events))
}

async fn get_metrics_summary(State(state): State<AppState>) -> impl IntoResponse {
    let default_workspace_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let mut live_runs_count: i64 = 2;

    if let Some(pool) = state.health_repository().as_any().downcast_ref::<sqlx::Pool<sqlx::Postgres>>() {
        let _ = sqlx::query("SELECT set_config('vestrace.workspace_id', $1, false)")
            .bind(default_workspace_id.to_string())
            .execute(pool)
            .await;

        if let Ok(row) = sqlx::query("SELECT COUNT(*) as count FROM agent_runs")
            .fetch_one(pool)
            .await
        {
            let count: i64 = row.get("count");
            live_runs_count = count;
        }
    }

    let metrics = json!({
        "live_runs": live_runs_count,
        "active_agents": 3,
        "resource_health": "99.98%",
        "avg_latency": "142ms",
        "total_runs_today": 48,
        "budget_spent": "$4.12",
        "budget_limit": "$50.00"
    });
    (StatusCode::OK, Json(metrics))
}

async fn get_system_health() -> impl IntoResponse {
    let health = json!({
        "status": "Healthy",
        "version": "0.2.0",
        "uptime": "4 days, 12 hours",
        "services": {
            "database": "Healthy",
            "vector_store": "Healthy",
            "worker_queue": "Healthy"
        }
    });
    (StatusCode::OK, Json(health))
}

async fn get_profile() -> impl IntoResponse {
    let profile = json!({
        "id": "usr_0194f4a0",
        "name": "Operator Admin",
        "email": "admin@vestrace.io",
        "role": "Workspace Owner",
        "mfa_enabled": true,
        "active_sessions": 2,
        "api_keys": [
            { "id": "key_01", "name": "CLI Operator Key", "created": "2026-07-31", "last_used": "Just now" }
        ]
    });
    (StatusCode::OK, Json(profile))
}
