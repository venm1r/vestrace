# P06 (G1): Real Connections, Models, Agents, Runs, Settings and Model-Backed Execution — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a chat message typed into the console's AG-UI widget actually invoke a real model — through a real Connection, a real Model bound to it, and a workspace default that Run creation already knows how to pin — proven live for both an unauthenticated (LM Studio) and a credentialed (remote OpenAI-compatible) Connection, surviving a full restart.

**Architecture:** Two small, isolated backend additions close the wiring that the existing Run/Step/worker machinery already needs (a route to set/read the workspace's default chat model, and a real `run_agent` handler that becomes a thin call into the same `RunOrchestrator` the console's own "Create run" button already drives). The console's SDK client currently sends the wrong idempotency header and the wrong request shapes for every governed mutation; that is fixed once, centrally, before the two pages that depend on it are rewired to call the real governed endpoints instead of the legacy/dead ones.

**Tech Stack:** Rust (axum, sqlx/Postgres) for the backend; React + TypeScript for the console; Playwright (MCP) for browser evidence; Docker Compose for the restart-cycle evidence.

**Spec:** `docs/superpowers/specs/2026-09-18-vestrace-v1-p06-real-execution-design.md`

## Global Constraints

- `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`, always `127.0.0.1`, never `localhost`, for every Rust test run in this plan.
- Every new file under `docs/`, `migrations/`, or `crates/` must be added to `scripts/p05-scope.mjs` (sorted) and synced into `docs/development-evidence/v1-g0-05-preflight.json`'s `change_scope_paths` in the same step it is created. This plan creates no new migration; it touches existing crates and adds two new evidence files under `docs/development-evidence/v1-g0-05-gate/` in Task 8.
- Commit messages end with `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`.
- Never `git add`/`git commit`/`git stash` mid-task inside a dispatched subagent's own judgement about the wider repo state — this repo's working tree is intentionally kept in a specific dirty state across the whole v1 program; only commit the files this plan's tasks actually touch.
- Backend idempotency header is `idempotency-key` (see Task 3 — the console SDK currently sends `x-request-id` instead, which is why every existing governed-mutation call from the console fails against the real backend today).
- The two auth branches this package must prove are `ConnectionKind::LMStudioLocal` (no credential) and `ConnectionKind::OpenAiChatCompletionsV1` (Bearer credential) — see `crates/vestrace-domain/src/connection/revision.rs:53-67`.

---

### Task 1: Backend — read and write the workspace's default chat model

**Files:**
- Modify: `crates/vestrace-application/src/models/revisions.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`
- Modify: `crates/vestrace-http/src/api/models.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs:173-182` (route registration)
- Test: `crates/vestrace-http/src/api/models.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `ModelRevisionRepository` trait (existing), `state.model_revision_repository()` (existing, `crates/vestrace-http/src/router.rs:656`), `SetWorkspaceModelDefault` (existing, `crates/vestrace-application/src/models/revisions.rs:29`), `vestrace_application::LEGACY_RUN_MODEL_DEFAULT_PURPOSE` (existing constant, value `"chat"`).
- Produces: `ModelRevisionRepository::get_workspace_default(&self, context: &RequestContext, purpose: &str) -> Result<Option<WorkspaceModelDefaultProjection>, ApplicationError>` (new trait method with a default `Unavailable` impl, so no other implementor of this trait needs to change). `WorkspaceModelDefaultProjection { model_id: ModelId, version: u64 }` (new, public in `vestrace_application`). HTTP routes `POST /v1/models/{id}/default` and `GET /v1/models/default` (new). Task 7 (Settings tab) consumes the read route; Task 6 (Models page) consumes the write route.

- [ ] **Step 1: Add the projection type and the new trait method**

In `crates/vestrace-application/src/models/revisions.rs`, add after `SetWorkspaceModelDefault` (after line 39):

```rust
/// The workspace's current default Model for one purpose (e.g. `"chat"`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceModelDefaultProjection {
    pub model_id: ModelId,
    pub version: u64,
}
```

This needs `ModelId` in scope — it is already imported at the top of this file (`use vestrace_domain::{..., ModelId, ...}` — check the existing `use` block at the top of `revisions.rs` and add `ModelId` to it if it is not already there).

Then add a new method to the `ModelRevisionRepository` trait (after `set_workspace_default_governed`, before `list_safe_models`):

```rust
    /// The workspace's current default for `purpose`, or `None` if nothing has
    /// been set yet. A caller with no default configured is a normal, expected
    /// state (a fresh workspace), not an error.
    async fn get_workspace_default(
        &self,
        _context: &RequestContext,
        _purpose: &str,
    ) -> Result<Option<WorkspaceModelDefaultProjection>, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "workspace default model projection is not configured".to_owned(),
        ))
    }
```

- [ ] **Step 2: Implement it against Postgres**

In `crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`, add `WorkspaceModelDefaultProjection` to the `vestrace_application` import list at the top of the file, then add this method to the `impl ModelRevisionRepository for PgModelRevisionRepository` block (after `set_workspace_default_governed`, before `list_safe_models`):

```rust
    async fn get_workspace_default(
        &self,
        context: &RequestContext,
        purpose: &str,
    ) -> Result<Option<WorkspaceModelDefaultProjection>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT model_id, version FROM workspace_model_defaults \
             WHERE workspace_id = $1 AND purpose = $2",
        )
        .bind(context.workspace_id.as_uuid())
        .bind(purpose)
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;
        scoped.commit().await.map_err(storage_error)?;

        Ok(row.map(|row| WorkspaceModelDefaultProjection {
            model_id: ModelId::from_uuid(row.get::<uuid::Uuid, _>("model_id")),
            version: row.get::<i64, _>("version") as u64,
        }))
    }
```

This mirrors `list_safe_models`'s own `begin_scoped` / `fetch_all(scoped.connection())` / `scoped.commit()` pattern in the same file (lines ~80-201) — read that method first if anything above does not compile, since the exact `PgScopedTransaction` accessor names live there.

- [ ] **Step 3: Run the existing infra test suite to confirm nothing broke**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test model_binding_snapshot -- --test-threads=1`
Expected: PASS (this suite already exercises `workspace_model_defaults` and must be unaffected by the new read-only method).

- [ ] **Step 4: Add the HTTP request/response types and both handlers**

In `crates/vestrace-http/src/api/models.rs`, add `SetWorkspaceModelDefault` and `WorkspaceModelDefaultProjection` to the `vestrace_application` import list at the top of the file. Add these types and handlers after `create_model_revision` (after line 261, before `request_model_qualification`):

```rust
/// Names the workspace default this call is publishing. `purpose` travels in
/// the body even though this build only ever sends `"chat"`, because the
/// backend command already carries it generally.
#[derive(Debug, Deserialize)]
pub struct SetWorkspaceModelDefaultRequest {
    pub default_id: Uuid,
    pub model_id: Uuid,
    pub purpose: String,
    pub required_capabilities: Vec<String>,
    pub expected_version: u64,
}

pub async fn set_workspace_model_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<SetWorkspaceModelDefaultRequest>,
) -> Result<axum::response::Response, ApiError> {
    let context = request_context(&headers)?;
    if request.model_id != model_id {
        return Err(ApiError::bad_request(
            "the path model id and the request body disagree",
        ));
    }
    let idempotency_key = required_idempotency_key(&headers)?;
    let at = now();
    let evidence = serde_json::json!({
        "default_id": request.default_id,
        "purpose": request.purpose,
        "model_id": request.model_id,
    });
    let command = SetWorkspaceModelDefault {
        default_id: request.default_id,
        workspace_id: context.workspace_id,
        purpose: request.purpose,
        model_id: vestrace_domain::id::ModelId::from_uuid(request.model_id),
        required_capabilities: request.required_capabilities,
        expected_version: request.expected_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: idempotency_key.clone(),
            workspace_id: context.workspace_id,
            request_hash: request.default_id.to_string(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: at,
            expires_at: at + chrono::Duration::hours(24),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            "model.workspace_default.set",
            evidence.clone(),
            at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "model.workspace_default.set",
            "model",
            request.model_id,
            evidence,
            at,
        )
        .map_err(|error| ApiError::bad_request(error.to_string()))?,
    };
    let receipt = state
        .model_revision_repository()?
        .set_workspace_default_governed(context, command)
        .await
        .map_err(ApiError::from_application)?;
    Ok((
        StatusCode::CREATED,
        Json(GovernedMutationResponse::from(receipt)),
    )
        .into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct WorkspaceModelDefaultQuery {
    pub purpose: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceModelDefaultResponse {
    pub model_id: Option<Uuid>,
    pub purpose: String,
    pub version: u64,
}

pub async fn get_workspace_model_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<WorkspaceModelDefaultQuery>,
) -> Result<Json<WorkspaceModelDefaultResponse>, ApiError> {
    let context = request_context(&headers)?;
    let purpose = query
        .purpose
        .unwrap_or_else(|| vestrace_application::LEGACY_RUN_MODEL_DEFAULT_PURPOSE.to_owned());
    let projection = state
        .model_revision_repository()?
        .get_workspace_default(&context, &purpose)
        .await
        .map_err(ApiError::from_application)?;
    Ok(Json(WorkspaceModelDefaultResponse {
        model_id: projection.as_ref().map(|p| p.model_id.as_uuid()),
        version: projection.map(|p| p.version).unwrap_or(0),
        purpose,
    }))
}
```

- [ ] **Step 5: Register both routes**

In `crates/vestrace-http/src/api/mod.rs`, add after the `create_model_revision` mount (after line 177, before the `/v1/models/{id}/qualifications` mount):

```rust
    let router = mount(
        router,
        route_descriptor(&Method::POST, "/v1/models/{id}/default"),
        post(models::set_workspace_model_default),
    );
    let router = mount(
        router,
        route_descriptor(&Method::GET, "/v1/models/default"),
        get(models::get_workspace_model_default),
    );
```

- [ ] **Step 6: Write the failing tests**

In `crates/vestrace-http/src/api/models.rs`'s existing `#[cfg(test)] mod tests` block, extend `StubProjection` (it already implements `ModelRevisionRepository`) with a recording variant. Add this alongside the existing `StubProjection` (it already has a `Vec<GovernedModelProjection>` field — add a new struct rather than changing that one, to keep the existing tests untouched):

```rust
    #[derive(Default)]
    struct SpyDefaults {
        set: std::sync::Mutex<Option<vestrace_application::SetWorkspaceModelDefault>>,
        stored: std::sync::Mutex<Option<vestrace_application::WorkspaceModelDefaultProjection>>,
    }

    #[async_trait::async_trait]
    impl ModelRevisionRepository for SpyDefaults {
        async fn create_governed(
            &self,
            _context: RequestContext,
            _command: CreateModelRevision,
        ) -> Result<vestrace_application::GovernedMutationReceipt, ApplicationError> {
            unreachable!("this suite exercises the default pointer only")
        }

        async fn set_workspace_default_governed(
            &self,
            _context: RequestContext,
            command: vestrace_application::SetWorkspaceModelDefault,
        ) -> Result<vestrace_application::GovernedMutationReceipt, ApplicationError> {
            *self.stored.lock().unwrap() = Some(vestrace_application::WorkspaceModelDefaultProjection {
                model_id: command.model_id,
                version: command.expected_version + 1,
            });
            *self.set.lock().unwrap() = Some(command);
            Ok(vestrace_application::GovernedMutationReceipt {
                audit_event_id: vestrace_domain::id::AuditEventId::new(),
                idempotency_key: None,
                outbox_message_ids: vec![],
            })
        }

        async fn get_workspace_default(
            &self,
            _context: &RequestContext,
            _purpose: &str,
        ) -> Result<Option<vestrace_application::WorkspaceModelDefaultProjection>, ApplicationError> {
            Ok(*self.stored.lock().unwrap())
        }

        async fn list_safe_models(
            &self,
            _context: &RequestContext,
        ) -> Result<Vec<GovernedModelProjection>, ApplicationError> {
            Ok(Vec::new())
        }
    }

    fn default_request(uri: &str, idempotency_key: Option<&str>, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-workspace-id", Uuid::now_v7().to_string())
            .header("x-principal-id", Uuid::now_v7().to_string());
        if let Some(key) = idempotency_key {
            builder = builder.header("idempotency-key", key);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    #[tokio::test]
    async fn setting_the_workspace_default_reaches_the_application_and_a_later_read_sees_it() {
        use std::sync::Arc;
        let spy = Arc::new(SpyDefaults::default());
        let model_id = Uuid::now_v7();
        let app = build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_model_revision_repository(spy.clone()),
        );

        let body = serde_json::json!({
            "default_id": Uuid::now_v7(),
            "model_id": model_id,
            "purpose": "chat",
            "required_capabilities": ["chat.completions"],
            "expected_version": 0,
        })
        .to_string();

        let response = app
            .clone()
            .oneshot(default_request(
                &format!("/v1/models/{model_id}/default"),
                Some(&Uuid::now_v7().to_string()),
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let recorded = spy.set.lock().unwrap().take().expect("the set reached the application");
        assert_eq!(recorded.model_id.as_uuid(), model_id);
        assert_eq!(recorded.purpose, "chat");

        let read = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/models/default?purpose=chat")
                    .header("x-workspace-id", Uuid::now_v7().to_string())
                    .header("x-principal-id", Uuid::now_v7().to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK);
        let read_body = to_bytes(read.into_body(), 4096).await.unwrap();
        let read_body: serde_json::Value = serde_json::from_slice(&read_body).unwrap();
        assert_eq!(read_body["model_id"], model_id.to_string());
    }

    #[tokio::test]
    async fn a_default_read_with_nothing_configured_answers_a_null_model_id_not_an_error() {
        use std::sync::Arc;
        let app = build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_model_revision_repository(Arc::new(SpyDefaults::default())),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/models/default?purpose=chat")
                    .header("x-workspace-id", Uuid::now_v7().to_string())
                    .header("x-principal-id", Uuid::now_v7().to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["model_id"].is_null());
    }

    #[tokio::test]
    async fn a_default_write_whose_path_and_body_disagree_is_refused() {
        use std::sync::Arc;
        let app = build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_model_revision_repository(Arc::new(SpyDefaults::default())),
        );
        let body = serde_json::json!({
            "default_id": Uuid::now_v7(),
            "model_id": Uuid::now_v7(),
            "purpose": "chat",
            "required_capabilities": ["chat.completions"],
            "expected_version": 0,
        })
        .to_string();

        let response = app
            .oneshot(default_request(
                &format!("/v1/models/{}/default", Uuid::now_v7()),
                Some(&Uuid::now_v7().to_string()),
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
```

This needs `AppState::with_model_revision_repository` — check it exists in `crates/vestrace-http/src/router.rs` near the other `with_*` builders (e.g. next to `with_run_orchestrator`); it almost certainly already does, since `model_revision_repository` is already a settable field consumed by `state.model_revision_repository()?`. If it is named differently, use the name the file actually declares.

- [ ] **Step 7: Run the new tests and confirm they fail for the right reason, then pass**

Run: `cargo test -p vestrace-http --lib api::models::tests -- --test-threads=1`
Expected before Steps 1-5: does not compile (the types/handlers do not exist yet). After Steps 1-5: PASS, 3 new tests green, all pre-existing tests in this file still green.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-application/src/models/revisions.rs \
        crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs \
        crates/vestrace-http/src/api/models.rs \
        crates/vestrace-http/src/api/mod.rs
git commit -m "feat(p06): expose read/write routes for the workspace's default chat model

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 2: Backend — implement real `run_agent`

**Files:**
- Modify: `crates/vestrace-http/src/api/ag_ui.rs`
- Modify: `crates/vestrace-http/tests/router_contract.rs:389-411` (rewrite the now-inaccurate test)

**Interfaces:**
- Consumes: `state.run_orchestrator()` (existing, returns `Result<&SharedRunOrchestrator, ApiError>`), `state.run_use_cases()` (existing, returns `&dyn RunUseCases`, infallible), `CreateRun`/`AddRunSteps`/`NewRunStepDto`/`NewRunStepInput`/`ConfidentialRunInput` (existing, `vestrace_application::run`), `RunActorRef`/`RunExecutionMode` (existing, `vestrace_domain::run`), `AgentRuntimeSnapshotId`/`RunStepId`/`AgentRunId` (existing, `vestrace_domain::id`).
- Produces: `run_agent` now performs a real mutation. Its response shape (`RunAgentResponse { run_id, status, message }`) is unchanged, so `apps/console/src/sdk/agUiClient.ts` and `CompactChat.tsx` need no changes.

- [ ] **Step 1: Write the failing tests in `ag_ui.rs`'s own test module**

Replace the existing `#[cfg(test)] mod tests` block in `crates/vestrace-http/src/api/ag_ui.rs` (currently just the three tests at the bottom of the file) with the two existing tests kept as-is, plus this fake orchestrator and three new tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use vestrace_application::run::{AddRunSteps, CreateRun, RunOrchestrator, RunSnapshot};
    use vestrace_application::{ApplicationError, RequestContext};

    #[test]
    fn thread_and_run_identifiers_are_accepted_without_being_echoed() {
        let request: RunAgentRequest =
            serde_json::from_str(r#"{"message":"do the thing","thread_id":"t-1","run_id":"r-1"}"#)
                .unwrap();
        assert_eq!(request.message, "do the thing");
        assert_eq!(request.thread_id.as_deref(), Some("t-1"));

        let response = RunAgentResponse {
            run_id: uuid::Uuid::nil(),
            status: "created".into(),
            message: "created run".into(),
        };
        let rendered = serde_json::to_string(&response).unwrap();
        assert!(!rendered.contains("t-1"));
        assert!(!rendered.contains("thread"));
    }

    #[test]
    fn the_stream_query_defaults_to_the_whole_workspace() {
        let query: StreamQuery = serde_urlencoded::from_str("").unwrap();
        assert!(query.run_id.is_none());
    }

    /// Records what the transport asked the durable coordinator to do, exactly
    /// like `api::runs::tests::RecordingOrchestrator` — redefined here because
    /// that one is private to `runs.rs`'s own test module.
    #[derive(Default, Clone)]
    struct RecordingOrchestrator {
        created: std::sync::Arc<std::sync::Mutex<Option<CreateRun>>>,
        added: std::sync::Arc<std::sync::Mutex<Option<AddRunSteps>>>,
    }

    fn snapshot(context: &RequestContext, objective: String) -> RunSnapshot {
        let at = vestrace_domain::time::now();
        let run_id = vestrace_domain::id::AgentRunId::new();
        RunSnapshot {
            run: vestrace_domain::run::AgentRun {
                id: run_id,
                workspace_id: context.workspace_id,
                objective,
                coordinator_snapshot_id: vestrace_domain::id::AgentRuntimeSnapshotId::new(),
                active_plan_revision_id: None,
                execution_mode: vestrace_domain::run::RunExecutionMode::Supervised,
                status: vestrace_domain::run::RunStatus::Created,
                current_step_id: None,
                checkpoint_id: None,
                parent: None,
                root_run_id: run_id,
                budget_snapshot_id: None,
                resource_usage_snapshot_id: None,
                version: vestrace_domain::run::RunVersion::INITIAL,
                result: None,
                created_at: at,
                updated_at: at,
                finished_at: None,
            },
            steps: vec![],
            checkpoint: None,
        }
    }

    #[async_trait::async_trait]
    impl RunOrchestrator for RecordingOrchestrator {
        async fn create_run(
            &self,
            context: &RequestContext,
            command: CreateRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            let objective = command.objective.clone();
            *self.created.lock().unwrap() = Some(command);
            Ok(snapshot(context, objective))
        }

        async fn add_steps(
            &self,
            context: &RequestContext,
            command: AddRunSteps,
        ) -> Result<RunSnapshot, ApplicationError> {
            *self.added.lock().unwrap() = Some(command);
            Ok(snapshot(context, "stepped".to_string()))
        }

        async fn pause_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::PauseRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "paused".to_string()))
        }

        async fn resume_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::ResumeRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "resumed".to_string()))
        }

        async fn cancel_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::CancelRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "cancelled".to_string()))
        }

        async fn approve_run(
            &self,
            context: &RequestContext,
            _command: vestrace_application::run::ApproveRun,
        ) -> Result<RunSnapshot, ApplicationError> {
            Ok(snapshot(context, "approved".to_string()))
        }
    }

    fn post_run(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/ag-ui/run")
            .header("content-type", "application/json")
            .header("x-workspace-id", uuid::Uuid::now_v7().to_string())
            .header("x-principal-id", uuid::Uuid::now_v7().to_string())
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn a_message_with_no_run_id_creates_a_run_and_adds_an_agent_step() {
        use std::sync::Arc;
        let orchestrator = RecordingOrchestrator::default();
        let app = crate::build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_run_orchestrator(Arc::new(orchestrator.clone())),
        );

        let response = app
            .oneshot(post_run(r#"{"message":"hello there"}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let created = orchestrator.created.lock().unwrap().take();
        assert!(created.is_some(), "run_agent must create a run when no run_id is given");
        let added = orchestrator.added.lock().unwrap().take();
        let added = added.expect("run_agent must add a step to the run it created");
        assert_eq!(added.steps.len(), 1);
        assert!(matches!(
            added.steps[0].assigned_actor,
            vestrace_domain::run::RunActorRef::AgentSnapshot(_)
        ));

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["run_id"].is_string());
    }

    #[tokio::test]
    async fn a_blank_message_is_refused_before_anything_is_created() {
        use std::sync::Arc;
        let orchestrator = RecordingOrchestrator::default();
        let app = crate::build_router(
            crate::api::runs::tests::test_state()
                .with_policy(Arc::new(crate::api::runs::tests::TestAllowPolicy))
                .with_run_orchestrator(Arc::new(orchestrator.clone())),
        );

        let response = app.oneshot(post_run(r#"{"message":"   "}"#)).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(orchestrator.created.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn no_orchestrator_configured_answers_not_implemented_rather_than_a_stub_refusal() {
        let response = crate::build_router(
            crate::api::runs::tests::test_state()
                .with_policy(std::sync::Arc::new(crate::api::runs::tests::TestAllowPolicy)),
        )
        .oneshot(post_run(r#"{"message":"hello there"}"#))
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}
```

- [ ] **Step 2: Run the new tests to confirm they fail**

Run: `cargo test -p vestrace-http --lib api::ag_ui::tests -- --test-threads=1`
Expected: FAIL to compile or fail — `run_agent` is still the stub, so no run/step is ever created and the response is a 403, not 200.

- [ ] **Step 3: Implement `run_agent`**

Replace the existing `run_agent` function (lines 124-134) with:

```rust
async fn run_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RunAgentRequest>,
) -> Result<Json<RunAgentResponse>, ApiError> {
    let context = request_context(&headers)?;
    let confidential_input = vestrace_application::run::ConfidentialRunInput::parse(request.message)
        .map_err(ApiError::from_application)?;

    let (run_id, expected_version) = match request.run_id {
        Some(ref raw) => {
            let run_id = raw
                .parse::<vestrace_domain::id::AgentRunId>()
                .map_err(|_| ApiError::bad_request("run id must be a UUID"))?;
            let run = state
                .run_use_cases()
                .get_run(&context, run_id)
                .await
                .map_err(ApiError::from_application)?
                .ok_or_else(|| ApiError::not_found("run"))?;
            (run_id, run.version)
        }
        None => {
            let created = state
                .run_orchestrator()?
                .create_run(
                    &context,
                    vestrace_application::run::CreateRun {
                        objective: "AG-UI run".to_owned(),
                        coordinator_snapshot_id: vestrace_domain::id::AgentRuntimeSnapshotId::new(),
                        execution_mode: vestrace_domain::run::RunExecutionMode::Supervised,
                        parent: None,
                        correlation_id: None,
                        idempotency_key: uuid::Uuid::now_v7().to_string(),
                    },
                )
                .await
                .map_err(ApiError::from_application)?;
            (created.run.id, created.run.version)
        }
    };

    let result = state
        .run_orchestrator()?
        .add_steps(
            &context,
            vestrace_application::run::AddRunSteps {
                run_id,
                expected_version,
                correlation_id: None,
                steps: vec![vestrace_application::run::NewRunStepDto {
                    id: vestrace_domain::id::RunStepId::new(),
                    plan_step_reference: None,
                    assigned_actor: vestrace_domain::run::RunActorRef::AgentSnapshot(
                        vestrace_domain::id::AgentRuntimeSnapshotId::new(),
                    ),
                    input_references: vec![],
                    input: vestrace_application::run::NewRunStepInput::Confidential(
                        confidential_input,
                    ),
                }],
                actor: vestrace_domain::run::RunActorRef::Principal(context.principal_id),
                idempotency_key: uuid::Uuid::now_v7().to_string(),
            },
        )
        .await
        .map_err(ApiError::from_application)?;

    Ok(Json(RunAgentResponse {
        run_id: result.run.id.as_uuid(),
        status: "accepted".to_owned(),
        message: "run accepted".to_owned(),
    }))
}
```

Also update the module doc comment at the top of the file (lines 1-6), which currently says the route "is deliberately closed" — replace it with a short accurate note that it now creates or extends a Run through the same `RunOrchestrator` the console's own Run detail page uses, single-shot (one message, one step, no thread continuation).

- [ ] **Step 4: Run the tests again**

Run: `cargo test -p vestrace-http --lib api::ag_ui::tests -- --test-threads=1`
Expected: PASS, all 5 tests (2 pre-existing + 3 new) green.

- [ ] **Step 5: Rewrite the now-inaccurate router_contract test**

In `crates/vestrace-http/tests/router_contract.rs`, replace the test at lines 389-411 (`ag_ui_run_reports_unavailable_when_no_orchestrator_is_configured`) — its premise ("no orchestrator is configured" as the reason for refusal) is no longer how the route behaves; the route now genuinely tries to reach an orchestrator, and `app()` in this file still does not configure one, so the correct current behavior is `501 not_implemented`, not `403 governed_run_input_required`:

```rust
#[tokio::test]
/// AG-UI creates or extends a Run through the same orchestrator `/v1/runs`
/// uses. This test's router has none configured, so the honest answer is "not
/// implemented", not a stub refusal that never looked.
async fn ag_ui_run_answers_not_implemented_when_no_orchestrator_is_configured() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ag-ui/run")
                .header("content-type", "application/json")
                .header("x-workspace-id", "00000000-0000-0000-0000-000000000001")
                .header("x-principal-id", "00000000-0000-0000-0000-000000000002")
                .body(Body::from(r#"{"message":"do the thing"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "not_implemented");
    assert!(!body.to_string().contains("do the thing"));
}
```

- [ ] **Step 6: Run the whole `vestrace-http` test suite**

Run: `cargo test -p vestrace-http -- --test-threads=1`
Expected: PASS. Run it as its own dedicated invocation, not combined with other `--test` suites in one command (a combined run's positional stdout is not trustworthy for per-suite counts).

- [ ] **Step 7: Commit**

```bash
git add crates/vestrace-http/src/api/ag_ui.rs crates/vestrace-http/tests/router_contract.rs
git commit -m "feat(p06): implement real AG-UI run execution over the existing RunOrchestrator

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 3: Console SDK — fix the governed-mutation header and the Connections/Models payload shapes

**Files:**
- Modify: `apps/console/src/sdk/client.ts`

**Interfaces:**
- Consumes: nothing new from earlier tasks.
- Produces: `vestraceClient.createConnection`, `.createConnectionRevision`, `.requestConnectionQualification`, `.createConnectionCredential`, `.activateConnectionCredential`, `.createModelRevision`, `.setWorkspaceModelDefault` (new), `.getWorkspaceModelDefault` (new) — all with payload types matching the real backend request bodies exactly (`ConnectionRevisionRequest`, `QualificationRequest`, `CredentialActivationRequest`, `ModelRevisionRequest`, `SetWorkspaceModelDefaultRequest` from Task 1). Task 5 and Task 6 consume these.

This is the task that discovered (during this plan's own research, by reading `crates/vestrace-http/src/api/mod.rs:60-79` next to `crates/vestrace-http/src/api/connections.rs`'s test fixtures) that **every existing governed-mutation call from the console currently fails against the real backend**: `governedPost` sends header `x-request-id`, but the backend's `required_idempotency_key` reads `idempotency-key` — a different header the backend's own doc comment explains was chosen specifically because `x-request-id` is server-generated and cannot be used for deduplication. Separately, the existing `CreateConnectionPayload` / `CreateConnectionRevisionPayload` / `QualificationRequestPayload` / `CreateConnectionCredentialPayload` / `CreateModelRevisionPayload` types are drastically simpler than the backend's real `ConnectionRevisionRequest` / `QualificationRequest` / `CredentialActivationRequest` / `ModelRevisionRequest`, so they would fail deserialization even with the header fixed.

- [ ] **Step 1: Fix the idempotency header**

In `apps/console/src/sdk/client.ts`, change `governedPost` (around line 334-344):

```typescript
function governedPost<T>(
  endpoint: string,
  payload: unknown | undefined,
  options: GovernedMutationOptions,
): Promise<T> {
  return request<T>(endpoint, {
    method: 'POST',
    headers: { 'idempotency-key': options.requestId },
    ...(payload === undefined ? {} : { body: JSON.stringify(payload) }),
  });
}
```

(Only the header name changes: `'x-request-id'` becomes `'idempotency-key'`. `GovernedMutationOptions.requestId` keeps its name — it is the console's own field name for "the value the caller is deduplicating on", which is exactly what the backend calls the idempotency key.)

- [ ] **Step 2: Replace the Connection payload types**

Replace `CreateConnectionPayload` and `CreateConnectionRevisionPayload` (lines 394-404) with types matching `ConnectionRevisionRequest` (`crates/vestrace-http/src/api/connections.rs:73-91`):

```typescript
/** Mirrors `vestrace_domain::connection::revision::ConnectionKind`. serde's
 * snake_case rule inserts `_` before every uppercase letter, not just at word
 * boundaries — `LMStudioLocal` becomes `l_m_studio_local`, not
 * `lm_studio_local` (verified against a live `serde_json::to_string` of the
 * real enum; this was wrong in an earlier draft of this plan and caught by
 * Task 3's own review). */
export type ConnectionKind = 'l_m_studio_local' | 'open_ai_chat_completions_v1';

/** Mirrors `vestrace_domain::connection::revision::ConnectionAuthMode` (serde snake_case). */
export type ConnectionAuthMode = 'none' | 'bearer' | 'api_key' | 'x_api_key';

/** Mirrors `vestrace_domain::connection::revision::ConnectionTransportPolicy` (serde tagged, snake_case). */
export type ConnectionTransportPolicy =
  | { kind: 'loopback_only' }
  | { kind: 'remote_https' };

/** Mirrors `api::connections::ConnectionRevisionRequest`. Every id here is minted
 * by the caller (the console), never by the server: the backend does not
 * default, infer or look any of them up. */
export interface CreateConnectionPayload {
  connection_id: string;
  connector_id: string;
  name: string;
  revision_id: string;
  execution_guard_id: string;
  kind: ConnectionKind;
  logical_base_url: string;
  runtime_base_url: string;
  adapter_profile_revision: string;
  transport_policy: ConnectionTransportPolicy;
  auth_mode: ConnectionAuthMode;
  credential_slot_id: string | null;
  expected_head_version: number;
}

/** A later revision on an existing Connection carries every field
 * `CreateConnectionPayload` does; only `expected_head_version` changes meaning
 * (it must equal the Connection's current head, not zero). */
export type CreateConnectionRevisionPayload = CreateConnectionPayload;
```

- [ ] **Step 3: Replace the qualification and credential payload types**

Replace `QualificationRequestPayload` (line 406-408) and `CreateConnectionCredentialPayload` (line 410-412) with types matching `QualificationRequest` and `CredentialActivationRequest` (`crates/vestrace-http/src/api/connections.rs:401-449` and `:513-523`):

```typescript
/** Mirrors `api::connections::QualificationTargetRequest` (serde internally tagged on `branch`). */
export type QualificationTargetPayload =
  | {
      branch: 'credential';
      revision_id: string;
      slot_id: string;
      activation_guard_id: string;
      expected_slot_version: number;
    }
  | { branch: 'no_auth'; binding_revision_id: string };

/** Mirrors `api::connections::QualificationRequest`. Qualifies one exact
 * Connection revision against one chat and one embedding Model revision. */
export interface QualificationRequestPayload {
  job_id: string;
  target_binding_id: string;
  connection_id: string;
  connection_revision_id: string;
  target: QualificationTargetPayload;
  chat_model_revision_id: string;
  embedding_model_revision_id: string;
}

/** Mirrors `api::connections::CredentialActivationRequest` — the first
 * publication of a credential into an empty slot. The actual secret bytes
 * are not part of this request: `credential_intent_id` names material that
 * must already exist (see Task 5's notes on this gap). */
export interface CreateConnectionCredentialPayload {
  connection_id: string;
  credential_slot_id: string;
  execution_guard_id: string;
  activation_guard_id: string;
  credential_revision_id: string;
  credential_intent_id: string;
  connection_qualification_revision_id: string;
  expected_slot_version: number;
}
```

- [ ] **Step 4: Replace the Model revision payload type and add the default-model payloads**

Replace `CreateModelRevisionPayload` (lines 414-418) with a type matching `ModelRevisionRequest` (`crates/vestrace-http/src/api/models.rs:160-175`), and add the two new payload/response types for Task 1's routes:

```typescript
/** Mirrors `vestrace_domain::models::binding::ModelKind`. */
export type ModelKind = 'chat' | 'embedding';

/** Mirrors `api::models::ModelRevisionRequest`. Publishes the compatibility
 * Model row and its immutable revision together, bound to one exact
 * Connection revision. */
export interface CreateModelRevisionPayload {
  model_id: string;
  provider_id: string;
  model_name: string;
  context_window: number;
  input_cost_per_mtoken: number;
  output_cost_per_mtoken: number;
  revision_id: string;
  connection_id: string;
  connection_revision_id: string;
  wire_model_id: string;
  kind: ModelKind;
  execution_guard_id: string;
  expected_head_version: number;
}

/** Mirrors `api::models::SetWorkspaceModelDefaultRequest`. */
export interface SetWorkspaceModelDefaultPayload {
  default_id: string;
  model_id: string;
  purpose: string;
  required_capabilities: string[];
  expected_version: number;
}

/** Mirrors `api::models::WorkspaceModelDefaultResponse`. */
export interface WorkspaceModelDefaultItem {
  model_id: string | null;
  purpose: string;
  version: number;
}
```

- [ ] **Step 5: Fix the SDK methods that use these payloads and add the two new ones**

Replace `createConnection` and `createConnectionRevision` (lines 473-487) — their bodies do not change, only the payload type they now reference changes automatically since the interfaces were redefined above; verify their signatures still read:

```typescript
  createConnection: (
    payload: CreateConnectionPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedConnectionItem> =>
    governedPost<GovernedConnectionItem>('/connections', payload, options),
  createConnectionRevision: (
    connectionId: string,
    payload: CreateConnectionRevisionPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedConnectionItem> =>
    governedPost<GovernedConnectionItem>(
      `/connections/${encodeURIComponent(connectionId)}/revisions`,
      payload,
      options,
    ),
```

Every governed-mutation handler on the backend — `create_connection`, `revise_connection`, `request_connection_qualification`, `activate_credential`, `rotate_credential`, `revoke_credential`, `create_model_revision`, and this task's own new `set_workspace_model_default` — returns the same `GovernedMutationResponse` shape (`crates/vestrace-http/src/api/connections.rs:95-100`: `{ audit_event_id, idempotency_key, outbox_message_ids }`), not a projection of the resource. Add this type once and use it as the return type for the new method (the pre-existing methods' `Promise<GovernedConnectionItem>`/`Promise<QualificationItem>`/`Promise<CredentialLifecycleItem>`/`Promise<GovernedModelItem>` return types are already wrong the same way, but fixing those is outside this task's scope — callers of those methods today don't read their return values, so leave them for a later pass rather than widen this task).

Add, near the other response-shaped interfaces (e.g. right before `GovernedMutationOptions`):

```typescript
/** Mirrors `api::connections::GovernedMutationResponse`, returned by every
 * governed-mutation route on this backend (Connections, Models, credentials,
 * qualifications) — never a projection of the mutated resource. */
export interface GovernedMutationResponse {
  audit_event_id: string;
  idempotency_key: string | null;
  outbox_message_ids: string[];
}
```

Then, after `createModelRevision` (around line 549-553), add:

```typescript
  setWorkspaceModelDefault: (
    modelId: string,
    payload: SetWorkspaceModelDefaultPayload,
    options: GovernedMutationOptions,
  ): Promise<GovernedMutationResponse> =>
    governedPost<GovernedMutationResponse>(
      `/models/${encodeURIComponent(modelId)}/default`,
      payload,
      options,
    ),
  getWorkspaceModelDefault: (purpose = 'chat'): Promise<WorkspaceModelDefaultItem> =>
    request<WorkspaceModelDefaultItem>(`/models/default?purpose=${encodeURIComponent(purpose)}`),
```

- [ ] **Step 6: Type-check the console**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS. (`ConnectionsPage.tsx` and `ModelsPage.tsx` are not yet rewired — Tasks 5 and 6 do that — so this step only proves `client.ts` itself is internally consistent.)

- [ ] **Step 7: Commit**

```bash
git add apps/console/src/sdk/client.ts
git commit -m "fix(p06): correct the console SDK's governed-mutation header and payload shapes

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 4: Console — shared qualification-state polling hook

**Files:**
- Create: `apps/console/src/sdk/useQualificationPolling.ts`

**Interfaces:**
- Consumes: nothing new.
- Produces: `usePolledQualification<T extends { id: string; qualification_state: string | null }>(list: () => Promise<T[]>, targetId: string | null) -> { item: T | null, polling: boolean }`. Task 5 uses this for the Connections page's "Test Connection" status; Task 6 does not need it (the Models page only reads `qualification_state` from the list it already loads, with no active "test" trigger of its own).

**A design correction made while researching this task, before writing any code:** the spec's original wording ("poll the returned qualification job") assumed a per-job status route exists. It does not — `POST /v1/connections/{id}/qualifications` and `POST /v1/models/{id}/qualifications` both return a `QualificationJobResponse { id, target_binding_id, state }` **once, at creation**, and nothing in `crates/vestrace-http/src/api/connections.rs` or `models.rs` reads a job by id afterward. What *does* observably change over time is the owning Connection/Model's own list projection: `GovernedConnectionProjection.qualification_state` (`crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs:190-196`) is computed fresh on every `GET /v1/connections` call from the connection revision's `qualification_valid_until` timestamp, taking the value `"qualified"`, `"expired"`, or `"missing"`. So "polling a qualification" here means re-fetching the list endpoint and watching one row's `qualification_state` field change — not polling the job.

- [ ] **Step 1: Write the hook**

```typescript
import { useEffect, useRef, useState } from 'react';

const POLL_INTERVAL_MS = 2000;
const MAX_POLLS = 30;

export interface QualifiableItem {
  id: string;
  qualification_state: string | null;
}

/**
 * Re-fetches `list()` every `POLL_INTERVAL_MS` while `targetId` is set,
 * watching for that item's `qualification_state` to change from whatever it
 * was when polling started. Stops when it changes, after `MAX_POLLS`
 * attempts (one minute), when `targetId` becomes `null`, or on unmount.
 *
 * There is no per-qualification-job status route in this backend (see this
 * task's own notes) — the thing that changes observably over time is the
 * owning Connection/Model's list projection, once a qualification job
 * finishes and records a result against it.
 */
export function usePolledQualification<T extends QualifiableItem>(
  list: () => Promise<T[]>,
  targetId: string | null,
): { item: T | null; polling: boolean } {
  const [item, setItem] = useState<T | null>(null);
  const [polling, setPolling] = useState(false);
  const listRef = useRef(list);
  listRef.current = list;

  useEffect(() => {
    setItem(null);
    if (!targetId) {
      setPolling(false);
      return;
    }

    let active = true;
    let attempts = 0;
    let startingState: string | null | undefined;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const poll = () => {
      setPolling(true);
      listRef
        .current()
        .then((items) => {
          if (!active) return;
          const found = items.find((candidate) => candidate.id === targetId) ?? null;
          setItem(found);
          if (startingState === undefined) startingState = found?.qualification_state ?? null;
          attempts += 1;
          const changed = found !== null && found.qualification_state !== startingState;
          if (changed || attempts >= MAX_POLLS) {
            setPolling(false);
          } else {
            timer = setTimeout(poll, POLL_INTERVAL_MS);
          }
        })
        .catch(() => {
          if (!active) return;
          attempts += 1;
          if (attempts >= MAX_POLLS) {
            setPolling(false);
          } else {
            // A transient read failure does not stop the poll; the next tick
            // tries again rather than freezing the UI on one bad response.
            timer = setTimeout(poll, POLL_INTERVAL_MS);
          }
        });
    };
    poll();

    return () => {
      active = false;
      if (timer) clearTimeout(timer);
    };
  }, [targetId]);

  return { item, polling };
}
```

- [ ] **Step 2: Type-check**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 3: Add the new file to scope**

Add `"apps/console/src/sdk/useQualificationPolling.ts"` to `scripts/p05-scope.mjs`'s `changeScopePaths` array in sorted order (it sorts alphabetically between other `apps/console/src/sdk/*` entries — check the existing entries in that range and place it correctly), add the same path to `docs/development-evidence/v1-g0-05-preflight.json`'s `change_scope_paths` array at the matching position, bump `tests/p05_scope.test.mjs`'s pinned `changeScopePaths.length` assertion by 1, and add a `scope_amendments` entry to the preflight JSON following the exact pattern the Task 8 entry below uses (see Task 8, Step 4, for the amendment-entry shape — write this one with today's date and a reason naming this task).

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/console/src/sdk/useQualificationPolling.ts scripts/p05-scope.mjs \
        docs/development-evidence/v1-g0-05-preflight.json tests/p05_scope.test.mjs
git commit -m "feat(p06): add a shared qualification-state polling hook for the console

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 5: Console — Connections page real creation and qualification

**Files:**
- Modify: `apps/console/src/routes/ConnectionsPage.tsx`

**Interfaces:**
- Consumes: `vestraceClient.createConnection`, `.requestConnectionQualification`, `.createConnectionCredential`, `.activateConnectionCredential` (fixed in Task 3), `usePolledQualification` (Task 4).
- Produces: nothing new consumed by later tasks — Connections and Models are read independently by both the console and by Task 8's evidence run.

**Two gaps discovered while researching this plan, which this task must not attempt to solve:**

1. `QualificationRequest` (`crates/vestrace-http/src/api/connections.rs:441-449`) names a `chat_model_revision_id` and an `embedding_model_revision_id` — a Connection is qualified against two *specific, already-published* Model revisions, not qualified in the abstract. That means qualification cannot usefully happen right after a bare Connection is created; a chat Model revision (and an embedding one) must already exist against it first (Task 6 publishes chat Model revisions; embedding Model publication is out of this package's scope entirely). Rather than block this task on embedding-model UI that does not exist, "Test Connection" asks the operator to paste both revision ids directly — clunky, but honest about what the backend actually requires, and sufficient for Task 8's evidence run (which will have real ids from having just created them).
2. `CredentialActivationRequest` (and the SDK's `CreateConnectionCredentialPayload` from Task 3) names a `credential_intent_id`, but no HTTP route in this codebase creates one — the actual secret bytes behind a credential are provisioned through the security-material subsystem (`crates/vestrace-domain/src/security`, P02), which has no console-facing surface in this build. This task therefore implements the **no-auth branch fully** (LM Studio, `auth_mode: 'none'`, no credential step at all) and implements the **credential-branch UI** (the form fields, the `activate_credential` call) but the actual `credential_intent_id`/`credential_revision_id` values used against a real backend will need to come from wherever Task 8's evidence-gathering finds real material is seeded (see Task 8's own notes) — do not invent a secret-submission form here; that is out of this package's scope per the spec's Non-goals.

- [ ] **Step 1: Replace the "configured via env vars" modal with a real creation form**

Replace the entire `<Modal>` block in `ConnectionsPage.tsx` (lines 40-64) and the component's state (lines 16-21) with:

```tsx
export const ConnectionsPage: React.FC = () => {
  const { data: connections, error, loading, reload } = useApiResource(vestraceClient.listConnections);
  const { notice, notify, dismiss } = useNotice();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [name, setName] = useState('');
  const [preset, setPreset] = useState<'lm_studio' | 'openai_compatible'>('lm_studio');
  const [runtimeBaseUrl, setRuntimeBaseUrl] = useState('http://host.docker.internal:12345/v1');
  const [submitting, setSubmitting] = useState(false);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [chatRevisionInput, setChatRevisionInput] = useState('');
  const [embeddingRevisionInput, setEmbeddingRevisionInput] = useState('');
  const { item: testedConnection, polling: testingQualification } = usePolledQualification(
    vestraceClient.listConnections,
    testingId,
  );

  const items = connections ?? [];

  const handleCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!name.trim()) {
      notify('warning', 'Connection name is required.');
      return;
    }
    setSubmitting(true);
    try {
      const kind: ConnectionKind = preset === 'lm_studio' ? 'l_m_studio_local' : 'open_ai_chat_completions_v1';
      const authMode: ConnectionAuthMode = preset === 'lm_studio' ? 'none' : 'bearer';
      const transportPolicy: ConnectionTransportPolicy =
        preset === 'lm_studio' ? { kind: 'loopback_only' } : { kind: 'remote_https' };
      const credentialSlotId = authMode === 'none' ? null : crypto.randomUUID();

      await vestraceClient.createConnection(
        {
          connection_id: crypto.randomUUID(),
          connector_id: crypto.randomUUID(),
          name: name.trim(),
          revision_id: crypto.randomUUID(),
          execution_guard_id: crypto.randomUUID(),
          kind,
          logical_base_url: runtimeBaseUrl,
          runtime_base_url: runtimeBaseUrl,
          adapter_profile_revision: 'openai-chat-completions/v1',
          transport_policy: transportPolicy,
          auth_mode: authMode,
          credential_slot_id: credentialSlotId,
          expected_head_version: 0,
        },
        { requestId: crypto.randomUUID() },
      );

      reload();
      setIsModalOpen(false);
      setName('');
      notify('success', `Connection "${name.trim()}" was created.`);
    } catch (err: unknown) {
      const described = describeError(err, 'connection creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  const handleTest = async (connectionId: string) => {
    const connection = items.find((c) => c.id === connectionId);
    if (!connection?.revision_id) {
      notify('warning', 'This connection has no published revision yet.');
      return;
    }
    if (!chatRevisionInput.trim() || !embeddingRevisionInput.trim()) {
      notify('warning', 'Paste a chat and an embedding Model revision id to qualify against.');
      return;
    }
    try {
      await vestraceClient.requestConnectionQualification(
        connectionId,
        {
          job_id: crypto.randomUUID(),
          target_binding_id: crypto.randomUUID(),
          connection_id: connectionId,
          connection_revision_id: connection.revision_id,
          target: { branch: 'no_auth', binding_revision_id: crypto.randomUUID() },
          chat_model_revision_id: chatRevisionInput.trim(),
          embedding_model_revision_id: embeddingRevisionInput.trim(),
        },
        { requestId: crypto.randomUUID() },
      );
      setTestingId(connectionId);
      notify('info', `Qualification requested for ${connectionId}.`);
    } catch (err: unknown) {
      const described = describeError(err, 'connection qualification');
      notify('error', `${described.title}: ${described.detail}`);
    }
  };
```

(The remaining JSX — `<PageShell>`, `<PageHeader>`, `<NoticeBanner>` — stays as-is; only the `<Modal>` contents and the per-connection "Test Connection" area change, in the next two steps. The `target: { branch: 'no_auth', ... }` above qualifies the no-auth branch; the credentialed branch's evidence run in Task 8 passes `{ branch: 'credential', revision_id, slot_id, activation_guard_id, expected_slot_version }` instead, using the real ids from whatever credential material Task 8 seeds.)

- [ ] **Step 2: Replace the modal body**

Replace lines 45-63 (the modal's inner `<div>`) with a real form:

```tsx
        <form onSubmit={handleCreate} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <label htmlFor="connection-name" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Name *
            </label>
            <input
              id="connection-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. LM Studio (local)"
              className="field-control"
              style={{ width: '100%', boxSizing: 'border-box' }}
            />
          </div>
          <div>
            <label htmlFor="connection-preset" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Provider *
            </label>
            <select
              id="connection-preset"
              value={preset}
              onChange={(e) => {
                const next = e.target.value as typeof preset;
                setPreset(next);
                setRuntimeBaseUrl(
                  next === 'lm_studio' ? 'http://host.docker.internal:12345/v1' : 'https://api.example.com/v1',
                );
              }}
              className="field-control"
              style={{ width: '100%' }}
            >
              <option value="lm_studio">LM Studio (local, no credential)</option>
              <option value="openai_compatible">Remote OpenAI-compatible (bearer credential)</option>
            </select>
          </div>
          <div>
            <label htmlFor="connection-url" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Runtime base URL *
            </label>
            <input
              id="connection-url"
              type="text"
              required
              value={runtimeBaseUrl}
              onChange={(e) => setRuntimeBaseUrl(e.target.value)}
              className="field-control"
              style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'var(--font-mono)' }}
            />
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px', marginTop: '8px' }}>
            <Button variant="secondary" onClick={() => setIsModalOpen(false)} disabled={submitting}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={submitting} icon="add">
              {submitting ? 'Creating...' : 'Create'}
            </Button>
          </div>
        </form>
```

Add the needed imports at the top of the file: `import { ConnectionAuthMode, ConnectionKind, ConnectionTransportPolicy, vestraceClient } from '../sdk/client';` and `import { usePolledQualification } from '../sdk/useQualificationPolling';`.

- [ ] **Step 3: Replace the "Test Connection" button with the paste-ids form and status**

Replace the `<ActionButton>` at lines 149-160 with:

```tsx
              <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                {testingId === connection.id ? (
                  <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>
                    Qualification:{' '}
                    <strong style={{ color: 'var(--text-primary)' }}>
                      {testingQualification
                        ? 'checking...'
                        : (testedConnection?.qualification_state ?? 'unknown')}
                    </strong>
                  </span>
                ) : (
                  <>
                    <input
                      type="text"
                      value={chatRevisionInput}
                      onChange={(e) => setChatRevisionInput(e.target.value)}
                      placeholder="Chat Model revision id"
                      className="field-control"
                      style={{ fontSize: '12px', fontFamily: 'var(--font-mono)' }}
                    />
                    <input
                      type="text"
                      value={embeddingRevisionInput}
                      onChange={(e) => setEmbeddingRevisionInput(e.target.value)}
                      placeholder="Embedding Model revision id"
                      className="field-control"
                      style={{ fontSize: '12px', fontFamily: 'var(--font-mono)' }}
                    />
                  </>
                )}
                <ActionButton
                  variant="quiet"
                  style={{ padding: '8px 14px', fontSize: '13px' }}
                  onClick={() => void handleTest(connection.id)}
                >
                  Test Connection
                </ActionButton>
              </div>
```

- [ ] **Step 4: Type-check and manually verify against the running stack**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS.

Then start the dev stack (`docker compose up -d`, per this session's established restart-safe recipe) and, through Playwright, navigate to the Connections page, create an LM Studio connection with the form, and confirm it appears in the list with a real revision id (not "none"). This is manual verification for this task only — the full qualification-through-to-real-run browser evidence is Task 8's job, done once, after every page is wired.

- [ ] **Step 5: Commit**

```bash
git add apps/console/src/routes/ConnectionsPage.tsx
git commit -m "feat(p06): real Connection creation and qualification on the Connections page

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 6: Console — Models page bound to a real Connection, with a "set as chat default" action

**Files:**
- Modify: `apps/console/src/routes/ModelsPage.tsx`

**Interfaces:**
- Consumes: `vestraceClient.listConnections`, `.createModelRevision`, `.setWorkspaceModelDefault`, `.getWorkspaceModelDefault` (Task 3).
- Produces: nothing new consumed by later tasks.

- [ ] **Step 1: Replace the provider-based state and form with a Connection-based one**

Replace the component's state block (lines 26-38) and `handleCreateModel` (lines 43-86):

```tsx
export const ModelsPage: React.FC = () => {
  const { data: initialModels, error, loading, reload } = useApiResource(vestraceClient.listModels);
  const { data: connections } = useApiResource(vestraceClient.listConnections);
  const { data: defaultModel, reload: reloadDefault } = useApiResource(() =>
    vestraceClient.getWorkspaceModelDefault('chat'),
  );
  const [models, setModels] = useState<GovernedModelItem[] | null>(null);
  const { notice, notify, dismiss } = useNotice();

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modelName, setModelName] = useState('');
  const [wireModelId, setWireModelId] = useState('');
  const [selectedConnectionId, setSelectedConnectionId] = useState('');
  const [contextWindow, setContextWindow] = useState('128000');
  const [inputCost, setInputCost] = useState('0.15');
  const [outputCost, setOutputCost] = useState('0.60');
  const [submitting, setSubmitting] = useState(false);
  const [settingDefaultId, setSettingDefaultId] = useState<string | null>(null);

  const connectionList = connections ?? [];
  const items = models ?? initialModels ?? [];

  const handleCreateModel = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!modelName.trim() || !wireModelId.trim()) {
      notify('warning', 'Model name and wire model id are required.');
      return;
    }
    if (!selectedConnectionId) {
      notify('warning', 'Select a Connection this Model runs against.');
      return;
    }
    const connection = connectionList.find((c) => c.id === selectedConnectionId);
    if (!connection || !connection.revision_id) {
      notify('warning', 'The selected Connection has no published revision yet.');
      return;
    }

    setSubmitting(true);
    try {
      const modelId = crypto.randomUUID();
      await vestraceClient.createModelRevision(
        modelId,
        {
          model_id: modelId,
          provider_id: crypto.randomUUID(),
          model_name: modelName.trim(),
          context_window: parseInt(contextWindow, 10) || 128000,
          input_cost_per_mtoken: parseFloat(inputCost) || 0,
          output_cost_per_mtoken: parseFloat(outputCost) || 0,
          revision_id: crypto.randomUUID(),
          connection_id: connection.id,
          connection_revision_id: connection.revision_id,
          wire_model_id: wireModelId.trim(),
          kind: 'chat',
          execution_guard_id: crypto.randomUUID(),
          expected_head_version: 0,
        },
        { requestId: crypto.randomUUID() },
      );

      setModels(null);
      reload();
      setIsModalOpen(false);
      setModelName('');
      setWireModelId('');
      notify('success', `Model "${modelName.trim()}" was published.`);
    } catch (err: unknown) {
      const described = describeError(err, 'model publication');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  const handleSetDefault = async (modelId: string) => {
    setSettingDefaultId(modelId);
    try {
      await vestraceClient.setWorkspaceModelDefault(
        modelId,
        {
          default_id: crypto.randomUUID(),
          model_id: modelId,
          purpose: 'chat',
          required_capabilities: ['chat.completions'],
          expected_version: defaultModel?.version ?? 0,
        },
        { requestId: crypto.randomUUID() },
      );
      reloadDefault();
      notify('success', `Model ${modelId} is now the workspace's chat default.`);
    } catch (err: unknown) {
      const described = describeError(err, 'setting the workspace default');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSettingDefaultId(null);
    }
  };
```

- [ ] **Step 2: Replace the provider `<select>` with a Connection `<select>`, and drop the "new provider" block**

In the modal's form (around old lines 116-240), replace the `model-name` label ("Model Name *" stays as-is) then replace the whole provider `<select>` block and the "new provider" conditional block (old lines 144-240) with:

```tsx
          <div>
            <label htmlFor="model-wire-id" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Wire Model Id *
            </label>
            <input
              id="model-wire-id"
              type="text"
              required
              value={wireModelId}
              onChange={(e) => setWireModelId(e.target.value)}
              placeholder="e.g. ternary-bonsai-27b"
              className="field-control"
              style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'var(--font-mono)' }}
            />
          </div>

          <div>
            <label htmlFor="model-connection" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Connection *
            </label>
            <select
              id="model-connection"
              value={selectedConnectionId}
              onChange={(e) => setSelectedConnectionId(e.target.value)}
              className="field-control"
              style={{ width: '100%' }}
            >
              <option value="">Select a Connection...</option>
              {connectionList.map((c) => (
                <option key={c.id} value={c.id} disabled={!c.revision_id}>
                  {c.id} ({c.qualification_state ?? 'unqualified'})
                </option>
              ))}
            </select>
          </div>
```

- [ ] **Step 3: Show the current default and add "Set as default" per row**

Add, right after `<PageHeader ... />` (old line 106):

```tsx
      <Panel>
        <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
          Current chat default:{' '}
          <strong style={{ color: 'var(--text-primary)', fontFamily: 'var(--font-mono)' }}>
            {defaultModel?.model_id ?? 'none configured'}
          </strong>
        </div>
      </Panel>
```

And add a sixth column to the table (after the `Blockers` `<Th>` and its cell, old lines 362 and 397-403):

```tsx
                  <Th style={{ padding: '12px 16px' }}>Default</Th>
```

```tsx
                    <td style={{ padding: '16px' }}>
                      {defaultModel?.model_id === model.id ? (
                        <span style={{ color: 'var(--color-success)' }}>chat default</span>
                      ) : (
                        <ActionButton
                          variant="quiet"
                          style={{ padding: '4px 10px', fontSize: '12px' }}
                          disabled={settingDefaultId === model.id}
                          onClick={() => handleSetDefault(model.id)}
                        >
                          {settingDefaultId === model.id ? 'Setting...' : 'Set as default'}
                        </ActionButton>
                      )}
                    </td>
```

- [ ] **Step 4: Type-check and manually verify**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS.

Through Playwright against the running stack: create a Model bound to the LM Studio connection created in Task 5, set it as the chat default, and confirm the "Current chat default" line updates.

- [ ] **Step 5: Commit**

```bash
git add apps/console/src/routes/ModelsPage.tsx
git commit -m "feat(p06): bind Model publication to a real Connection and expose the chat default

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 7: Console — Settings "Models" tab

**Files:**
- Modify: `apps/console/src/routes/SettingsPage.tsx`

**Interfaces:**
- Consumes: `vestraceClient.getWorkspaceModelDefault` (Task 3).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add a fourth tab**

In `SettingsPage.tsx`, change `TabId` and `TABS` (lines 20-26):

```typescript
type TabId = 'kernel' | 'models' | 'telemetry' | 'environment';

const TABS: Array<{ id: TabId; label: string; icon: string }> = [
  { id: 'kernel', label: 'Kernel Constraints', icon: 'memory' },
  { id: 'models', label: 'Models', icon: 'model_training' },
  { id: 'telemetry', label: 'Telemetry', icon: 'monitoring' },
  { id: 'environment', label: 'Environment', icon: 'shield' },
];
```

Add a data load alongside the existing `useApiResource(vestraceClient.getSettings)` call (line 142):

```typescript
  const { data: defaultModel } = useApiResource(() => vestraceClient.getWorkspaceModelDefault('chat'));
```

- [ ] **Step 2: Add the tab panel**

Add, after the `kernel` tab panel closes and before the `telemetry` panel starts (between old lines 305 and 307):

```tsx
          {activeTab === 'models' && (
            <div role="tabpanel" id="settings-panel-models" aria-labelledby="settings-tab-models" style={cardStyle}>
              <h2 style={headingStyle}>Chat default</h2>
              <ObservedRow
                label="Default Model"
                value={defaultModel?.model_id ?? 'none configured'}
                hint="The Model every Run's agent step uses, unless it is created through a path that names one explicitly. Change it on the Models page."
              />
            </div>
          )}
```

- [ ] **Step 3: Type-check and manually verify**

Run: `cd apps/console && npx tsc --noEmit`
Expected: PASS.

Through Playwright: open Settings, switch to the Models tab, confirm it shows the default set in Task 6.

- [ ] **Step 4: Commit**

```bash
git add apps/console/src/routes/SettingsPage.tsx
git commit -m "feat(p06): surface the workspace's chat default model in Settings

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

### Task 8: Evidence — browser and restart proof for both auth branches

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt` (or similarly named — see Step 6)
- Create: `docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt`
- Modify: `scripts/p05-scope.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`, `tests/p05_scope.test.mjs` (scope admission for the two new evidence files, following the exact pattern already used for every prior P05 evidence package in this repo)

**Interfaces:**
- Consumes: everything from Tasks 1-7, running together.

This task is evidence-gathering, not code-writing: it drives the real stack through a real browser and records what happened. Two things discovered during this plan's research are load-bearing for how it must be run, and are recorded here rather than left for the person running it to rediscover:

1. **LM Studio reachability from inside the Docker stack.** The server, worker, and console all run as Docker containers per `docker-compose.yml`; LM Studio runs on the host at `http://localhost:12345/v1`. From inside a container that is `http://host.docker.internal:12345/v1`. Before trusting the Connections form's default in Task 5, confirm this resolves from inside the `vestrace-server` container (`docker compose exec vestrace-server curl -s http://host.docker.internal:12345/v1/models`) — if it does not, Docker Desktop's `host.docker.internal` mapping may need an explicit `extra_hosts` entry added to `docker-compose.yml` for the affected services, which is a one-line addition, not a design change.

2. **`ConnectionTransportPolicy` vs. `ConnectionKind` validation.** Before relying on `transport_policy: { kind: 'loopback_only' }` for the LM Studio connection, read `crates/vestrace-domain/src/connection/revision.rs`'s validation around `ConnectionKind`/`ConnectionTransportPolicy`/`NormalizedBaseUrl::for_runtime_url` to confirm `host.docker.internal` (not literally `localhost`) is accepted for a loopback-policy connection — the domain may compute the transport policy from the URL itself rather than trust the caller's stated value. If `host.docker.internal` is rejected, the pragmatic fix for evidence purposes is to add a `127.0.0.1 lmstudio.local` style loopback alias, or run this one evidence step from a shell with host networking — record whichever was actually needed in the evidence doc; do not silently paper over a real validation failure.

3. **Credential material for the credentialed branch.** As discovered in Task 5, there is no HTTP surface to submit a credential's actual secret bytes in this build — `credential_intent_id` names material that must already exist. Locate how existing tests construct this (start with `crates/vestrace-infrastructure/tests/common/mod.rs`, which already builds fixture credential/material rows for other suites) and use the same mechanism — a direct SQL insert against the disposable evidence database, or an existing internal helper — to seed one real credential before driving the browser through `activate_credential`. Record the exact mechanism used in the evidence doc; this is real setup, not a shortcut around the feature.

- [ ] **Step 1: No-auth branch — Playwright walkthrough**

Bring up the stack (`docker compose up -d`, rebuilding any stale images first — `docker compose build vestrace-server vestrace-worker console` if their source changed since the last build, per this session's established recipe). Through Playwright:
1. Navigate to Connections, create the LM Studio connection (Task 5's form, `lm_studio` preset).
2. Click "Test Connection", wait for the qualification job to reach a terminal state; record whether it reaches `qualified`.
3. Navigate to Models, create a Model bound to that Connection with `wire_model_id: ternary-bonsai-27b`, set it as the chat default.
4. Navigate to a Run (or use the AG-UI chat widget directly), send a message, and confirm a real completion streams back over `/ag-ui/events/stream`.

Save the Playwright transcript/screenshots and the final AG-UI event payload (proving real model output, not a placeholder) to `docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt`.

- [ ] **Step 2: Credentialed branch — Playwright walkthrough**

Repeat Step 1's flow with the `openai_compatible` preset against a real (or realistic local stand-in, per the spec's own open question) OpenAI-compatible endpoint requiring a Bearer credential, seeding the credential material per this task's note 3 above. Save the transcript to `docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt`.

- [ ] **Step 3: Restart evidence**

With both Connections/Models/default already configured from Steps 1-2, run `docker compose down` (preserving the `vestrace_postgres-data` volume — do not add `-v`) followed by `docker compose up -d`. Once the server reports healthy, repeat one AG-UI message against the already-configured default model (no reconfiguration) and confirm it still produces a real completion. Append this result to `p06-no-auth-run-test.txt`.

- [ ] **Step 4: Write the P06 evidence document**

Create `docs/development-evidence/v1-g0-06-real-execution.md` summarizing what Tasks 1-8 proved, citing the exact two evidence files by path and sha256 (compute with `sha256sum` or Node's `crypto` module), following the citation style already used in `docs/development-evidence/v1-g0-05j-backup-restore-closure.md`.

- [ ] **Step 5: Admit the new evidence files and evidence doc into scope**

Add `docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt`, `docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt`, and `docs/development-evidence/v1-g0-06-real-execution.md` to `scripts/p05-scope.mjs`'s `changeScopePaths` array (sorted), the same three paths to `docs/development-evidence/v1-g0-05-preflight.json`'s `change_scope_paths` array (same sorted position), bump `tests/p05_scope.test.mjs`'s pinned `changeScopePaths.length` assertion accordingly, and add a `scope_amendments` entry:

```json
{
  "authorized_at_utc": "<today, ISO-8601, one second after the previous latest entry>",
  "authorized_by": "user standing authorization",
  "reason": "P06 Task 8 closes real model-backed Run execution evidence for both auth branches (LM Studio no-auth, remote OpenAI-compatible credentialed) plus a restart cycle, per docs/superpowers/plans/2026-09-18-vestrace-v1-p06-real-execution.md.",
  "paths": [
    "docs/superpowers/plans/2026-09-18-vestrace-v1-p06-real-execution.md",
    "docs/development-evidence/v1-g0-06-real-execution.md",
    "docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt",
    "docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt"
  ]
}
```

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS.

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: silent (no output), confirming the working tree's diff from baseline stays within the declared scope.

- [ ] **Step 6: Commit**

```bash
git add docs/development-evidence/v1-g0-06-real-execution.md \
        docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt \
        docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt \
        scripts/p05-scope.mjs docs/development-evidence/v1-g0-05-preflight.json tests/p05_scope.test.mjs
git commit -m "docs(p06): record browser and restart evidence for both auth branches

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Push

Per this repository's standing rule, push after every completed task using the temp-index technique (never the real index/HEAD, so the program's intentionally-dirty working tree survives):

```bash
export GIT_INDEX_FILE=/tmp/vestrace-temp-index-$$
rm -f "$GIT_INDEX_FILE"
git add -A
SHA=$(git commit-tree $(git write-tree) -p $(git rev-parse origin/main) -m "...")
git push origin "$SHA:refs/heads/main"
rm -f "$GIT_INDEX_FILE"
unset GIT_INDEX_FILE
```

Always fetch and use the current `origin/main` tip as `-p`, not a remembered SHA — this plan's own Task 0 push discovered a stale-parent rejection when the recorded HEAD had drifted from the real remote tip.
