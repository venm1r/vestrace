const SERVER_SOURCE: &str = include_str!("../src/commands/server.rs");
const WORKER_SOURCE: &str = include_str!("../src/commands/worker.rs");
const HTTP_ROUTER_SOURCE: &str = include_str!("../../vestrace-http/src/router.rs");

/// The old factory is process-configured and has no immutable Connection /
/// Model / qualification tuple.  It must not survive in either production
/// composition root after governed execution is wired.
#[test]
fn production_composition_has_no_legacy_provider_factory_or_config_routing() {
    for (name, source) in [("server", SERVER_SOURCE), ("worker", WORKER_SOURCE)] {
        assert!(
            !source.contains("SecretBackedProviderFactory"),
            "{name} still composes the legacy secret-backed provider factory"
        );
        assert!(
            !source.contains("ProviderStepModelExecutor"),
            "{name} still composes name/config model execution"
        );
        assert!(
            !source.contains("config.model.base_url"),
            "{name} still routes provider calls from config.model"
        );
        assert!(
            !source.contains(".migrate()"),
            "{name} performs nested migration instead of failing closed"
        );
    }
}

#[test]
fn long_running_roots_verify_schema_before_constructing_governed_authorities() {
    for (name, source) in [("server", SERVER_SOURCE), ("worker", WORKER_SOURCE)] {
        assert!(
            source.contains("migrations_are_compatible"),
            "{name} does not verify the administrative schema before startup"
        );
        // One graph, not two that happen to agree today. A root that built its
        // own dispatch repository could drift from the other, and the symptom
        // would be a Run the surface accepted and the worker refused.
        assert!(
            source.contains("GovernedProviderRuntime::new"),
            "{name} does not compose the shared governed provider runtime"
        );
        assert!(
            !source.contains("PgProviderDispatchRepository::new"),
            "{name} constructs a parallel provider dispatch graph beside the shared one"
        );
        assert!(
            !source.contains("PgProviderResultRepository::new"),
            "{name} constructs a parallel provider result graph beside the shared one"
        );
        assert!(
            source.contains("build_material_vault"),
            "{name} does not compose the host material vault"
        );
        assert!(
            source.contains("build_model_data_policy_settings"),
            "{name} does not compose the declared model disclosure boundary"
        );
    }
}

/// The worker is the only root that executes a Run step, and it must reach the
/// provider through the governed executor rather than any surviving legacy
/// path. The server must not acquire one: a surface that executed steps would
/// bypass the lease that decides which worker owns an attempt.
#[test]
fn only_the_worker_registers_the_governed_step_executor() {
    assert!(
        WORKER_SOURCE.contains("with_model_executor(governed.step_executor("),
        "worker does not register the governed step executor"
    );
    assert!(
        !SERVER_SOURCE.contains("step_executor("),
        "server must not execute Run steps"
    );
    assert!(
        !WORKER_SOURCE.contains("governed provider execution is unavailable"),
        "worker still announces that governed execution is uncomposed"
    );
}

#[test]
fn server_injects_safe_projection_ports_from_the_runtime_store_only() {
    let compact_server_source: String = SERVER_SOURCE
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    assert!(
        SERVER_SOURCE.contains("PgConnectionRevisionRepository"),
        "server does not compose the governed Connection projection repository"
    );
    assert!(
        SERVER_SOURCE.contains("PgModelRevisionRepository"),
        "server does not compose the governed Model projection repository"
    );
    assert!(
        SERVER_SOURCE.contains("let store_for_governed_projections = store.clone();"),
        "server must derive both read projections from the established runtime store"
    );
    assert!(
        compact_server_source.contains(
            "PgConnectionRevisionRepository::new(store_for_governed_projections.clone(),)"
        ),
        "connection projection must use the shared runtime-scoped store"
    );
    assert!(
        compact_server_source
            .contains("PgModelRevisionRepository::new(store_for_governed_projections)"),
        "model projection must use the shared runtime-scoped store"
    );
    assert!(
        SERVER_SOURCE
            .contains(".with_connection_revision_repository(connection_revision_repository)"),
        "server must inject the governed Connection projection port"
    );
    assert!(
        SERVER_SOURCE.contains(".with_model_revision_repository(model_revision_repository)"),
        "server must inject the governed Model projection port"
    );
    assert!(
        !HTTP_ROUTER_SOURCE.contains("    provider_repository: SharedProviderRepository,"),
        "AppState must not retain the legacy provider repository after GET moves to governed projections"
    );
    assert!(
        HTTP_ROUTER_SOURCE.contains("_provider_repository: SharedProviderRepository"),
        "public AppState constructors retain call compatibility while discarding legacy provider injection"
    );
}

/// The embedding acceptance authority is composed from the same governed graph
/// as Run-step dispatch, and dispatch itself is not duplicated for it.
///
/// Task 4 asks a reviewer to confirm that embedding dispatch did not grow a
/// parallel authority. This checks the composition rather than the reviewer's
/// memory: `GovernedProviderRuntime` exposes exactly one dispatch repository and
/// one embedding acceptance repository, and there is no second
/// `PgProviderDispatchRepository` construction anywhere in that file.
#[test]
fn the_governed_runtime_composes_one_dispatch_authority_for_both_callers() {
    const RUNTIME_SOURCE: &str = include_str!("../../vestrace-infrastructure/src/postgres/mod.rs");

    assert_eq!(
        RUNTIME_SOURCE
            .matches("PgProviderDispatchRepository::new(")
            .count(),
        1,
        "the governed runtime must construct exactly one dispatch repository"
    );
    assert!(
        RUNTIME_SOURCE.contains("PgEmbeddingJobRepository::new(store.clone())"),
        "the governed runtime must compose embedding acceptance from the shared store"
    );
    assert!(
        RUNTIME_SOURCE.contains("pub fn embedding_jobs(&self)"),
        "the governed runtime must expose the embedding acceptance authority"
    );
    // Acceptance is a separate port; dispatch is not. A method here returning a
    // dispatch repository built for embeddings would be the defect.
    assert!(
        !RUNTIME_SOURCE.contains("embedding_dispatch"),
        "embedding work must reach the provider through the shared dispatch authority"
    );
}
