# P0 Stabilization Plan — Self-Review Corrections

These corrections are normative and supersede the corresponding details in `2026-08-04-p0-stabilization.md`.

## 1. Task 1 AG-UI route test

`/ag-ui/run` is registered as a POST route. The request in `ag_ui_is_explicitly_unavailable` must explicitly use POST:

```rust
let response = app()
    .oneshot(
        Request::builder()
            .method("POST")
            .uri("/ag-ui/run")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
```

A default GET request would correctly return `405 Method Not Allowed` and would not test the intended `501` handler.

## 2. Task 1 HTTP Cargo dependency placement

The stabilized shell does not use `serde_json` in production code. Do not add it to `[dependencies]` merely for `ApiError` serialization; Axum's `Json` requires `serde`, which is already present.

Add this only when HTTP tests decode response bodies:

```toml
[dev-dependencies]
serde_json.workspace = true
```

Continue to avoid `sqlx`, `chrono`, `futures-util`, and `tokio-stream` in the HTTP crate.

## 3. Task 3 infrastructure Cargo dependencies

`PgRunRepository` decodes PostgreSQL UUID and timestamp columns into concrete Rust types. Add these direct dependencies to `crates/vestrace-infrastructure/Cargo.toml` in Task 3:

```toml
[dependencies]
chrono.workspace = true
uuid.workspace = true
```

Also add `crates/vestrace-infrastructure/Cargo.toml` to Task 3's modified-file list and commit it with the repository implementation.

## 4. Task 4 HTTP domain dependency

`RunResponse` directly names `RunStatus` and `Timestamp`, and handlers parse `AgentRunId`. Add the domain crate as a direct HTTP dependency in Task 4:

```toml
[dependencies]
vestrace-domain = { path = "../vestrace-domain" }
```

Add `crates/vestrace-http/Cargo.toml` to Task 4's modified-file list and commit it with the HTTP run slice.

## 5. Corrected focused verification commands

After Task 3:

```bash
cargo check -p vestrace-infrastructure --all-features
cargo test -p vestrace-infrastructure --test run_repository
cargo clippy -p vestrace-infrastructure --all-targets --all-features -- -D warnings
```

After Task 4:

```bash
cargo check -p vestrace-http --all-features
cargo test -p vestrace-http --test router_contract --test run_routes
cargo clippy -p vestrace-http --all-targets --all-features -- -D warnings
```

## Self-review result

- Spec coverage: complete.
- Placeholder scan: no `TBD`, `TODO`, or unspecified implementation steps found.
- Type consistency: corrected for direct crate dependencies and POST route semantics.
- Scope: remains limited to P0 stabilization and one run vertical slice.
