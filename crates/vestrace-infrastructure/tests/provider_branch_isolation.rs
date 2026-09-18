//! Two Connections in one workspace must stay two Connections.
//!
//! `provider_schema_contract::representative_composite_edges_reject_mixed_identity_inserts`
//! already proves the composite edges refuse mixed identities, but it builds
//! two `no_auth` Connections. The hazard this file is about is the credential
//! branch: which slot, guard and revision a dispatch may reach, when a second
//! Connection in the same workspace has a complete set of its own.

use std::str::FromStr;

use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

/// The provider-side model string both Connections advertise. Nothing in the
/// binding chain may key off it.
const SHARED_WIRE_MODEL: &str = "shared-wire-model-v1";

struct Branch {
    connection: Uuid,
    execution_guard: Uuid,
    slot: Uuid,
    revision: Uuid,
    connection_qualification: Uuid,
    model_revision: Uuid,
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as vestrace");
    let parsed = PgConnectOptions::from_str(&runtime_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .expect("runtime URL must contain a password");
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .expect("runtime must connect to the SQLx test database")
}

async fn scope(
    pool: &PgPool,
    workspace: Uuid,
    principal: Uuid,
) -> sqlx::Transaction<'_, sqlx::Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    for (setting, value) in [
        ("vestrace.workspace_id", workspace),
        ("vestrace.principal_id", principal),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1,$2,true)")
            .bind(setting)
            .bind(value.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    transaction
}

/// One workspace, one principal, one connector, and two Connections whose
/// credential machinery is complete and separate.
async fn two_branches(pool: &PgPool, workspace: Uuid, principal: Uuid) -> (Branch, Branch) {
    let connector = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id,slug) VALUES ($1,$2)")
        .bind(workspace)
        .bind(format!("branch-isolation-{workspace}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id,workspace_id,identifier) VALUES ($1,$2,$3)")
        .bind(principal)
        .bind(workspace)
        .bind(format!("branch-isolation-principal-{principal}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id,workspace_id,name,provider_type) VALUES ($1,$2,$3,'local')",
    )
    .bind(connector)
    .bind(workspace)
    .bind(format!("branch-isolation-connector-{connector}"))
    .execute(pool)
    .await
    .unwrap();

    let provider = Uuid::now_v7();
    sqlx::query("INSERT INTO providers (id,workspace_id,name,locality) VALUES ($1,$2,$3,'remote')")
        .bind(provider)
        .bind(workspace)
        .bind(format!("branch-isolation-provider-{provider}"))
        .execute(pool)
        .await
        .unwrap();

    let mut branches = Vec::new();
    for label in ["a", "b"] {
        let branch = Branch {
            connection: Uuid::now_v7(),
            execution_guard: Uuid::now_v7(),
            slot: Uuid::now_v7(),
            revision: Uuid::now_v7(),
            connection_qualification: Uuid::now_v7(),
            model_revision: Uuid::now_v7(),
        };
        sqlx::query(
            "INSERT INTO connections (id,connector_id,workspace_id,principal_id,name,status) \
             VALUES ($1,$2,$3,$4,$5,'active')",
        )
        .bind(branch.connection)
        .bind(connector)
        .bind(workspace)
        .bind(principal)
        .bind(format!("branch-isolation-{label}-{}", branch.connection))
        .execute(pool)
        .await
        .unwrap();

        let mut transaction = scope(pool, workspace, principal).await;
        sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
            .bind(branch.execution_guard)
            .bind(workspace)
            .bind(branch.connection)
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,'provider','primary')")
            .bind(branch.slot)
            .bind(workspace)
            .bind(branch.connection)
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
            .bind(Uuid::now_v7())
            .bind(workspace)
            .bind(branch.connection)
            .bind(branch.slot)
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        // The revision, its qualification and one model revision, all pinned to
        // this Connection. The model revision deliberately advertises the same
        // provider-side string as its sibling.
        let model = Uuid::now_v7();
        let qualification_job = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO models \
             (id,provider_id,workspace_id,model_name,context_window,\
              input_cost_per_mtoken,output_cost_per_mtoken) \
             VALUES ($1,$2,$3,$4,4096,0,0)",
        )
        .bind(model)
        .bind(provider)
        .bind(workspace)
        .bind(format!("branch-isolation-model-{label}"))
        .execute(pool)
        .await
        .unwrap();

        let mut guarded = scope(pool, workspace, principal).await;
        sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
            .execute(&mut *guarded)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO connection_revisions \
             (id,workspace_id,connection_id,execution_guard_id,kind,logical_base_url,\
              runtime_base_url,adapter_profile_revision,transport_policy,auth_mode,\
              credential_slot_id) \
             VALUES ($1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1',\
              'http://127.0.0.1:1234/v1','branch-isolation/v1','loopback_only','bearer',$5)",
        )
        .bind(branch.revision)
        .bind(workspace)
        .bind(branch.connection)
        .bind(branch.execution_guard)
        .bind(branch.slot)
        .execute(&mut *guarded)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO connection_revision_heads \
             (workspace_id,connection_id,current_revision_id,version) VALUES ($1,$2,$3,1)",
        )
        .bind(workspace)
        .bind(branch.connection)
        .bind(branch.revision)
        .execute(&mut *guarded)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO qualification_jobs \
             (id,workspace_id,connection_revision_id,profile_revision,state,completed_at) \
             VALUES ($1,$2,$3,'branch-isolation/v1','succeeded',NOW())",
        )
        .bind(qualification_job)
        .bind(workspace)
        .bind(branch.revision)
        .execute(&mut *guarded)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO connection_qualification_revisions \
             (id,workspace_id,connection_revision_id,qualification_job_id,profile_revision,\
              valid_until,capabilities) \
             VALUES ($1,$2,$3,$4,'branch-isolation/v1',NOW() + INTERVAL '1 hour',ARRAY['chat'])",
        )
        .bind(branch.connection_qualification)
        .bind(workspace)
        .bind(branch.revision)
        .bind(qualification_job)
        .execute(&mut *guarded)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO model_revisions \
             (id,workspace_id,model_id,connection_revision_id,wire_model_id,kind) \
             VALUES ($1,$2,$3,$4,$5,'chat')",
        )
        .bind(branch.model_revision)
        .bind(workspace)
        .bind(model)
        .bind(branch.revision)
        .bind(SHARED_WIRE_MODEL)
        .execute(&mut *guarded)
        .await
        .unwrap();
        guarded.commit().await.unwrap();

        branches.push(branch);
    }
    let second = branches.pop().unwrap();
    let first = branches.pop().unwrap();
    (first, second)
}

/// The activation guard is the edge that ties a slot to its Connection, and it
/// is why the binding-snapshot chain cannot be crossed. Proved directly rather
/// than assumed, because the test below leans on it.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_activation_guard_refuses_another_connections_slot(pool: PgPool) {
    let workspace = Uuid::now_v7();
    let principal = Uuid::now_v7();
    let (first, second) = two_branches(&pool, workspace, principal).await;

    let mut transaction = scope(&pool, workspace, principal).await;
    let crossed = sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
        .bind(Uuid::now_v7())
        .bind(workspace)
        .bind(first.connection)
        .bind(second.slot)
        .execute(&mut *transaction)
        .await
        .expect_err("one connection must not guard another connection's credential slot");
    let database = crossed
        .as_database_error()
        .expect("a crossed activation guard must be a database refusal");
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert_eq!(
        database.message(),
        "credential activation guard requires its credential slot"
    );
}

/// A credential-auth Connection revision must not name another Connection's
/// slot, and exactly one thing in the database stops it.
///
/// Not the foreign key: `connection_revisions_credential_slot_fkey` references
/// `credential_slots(workspace_id, id)` and omits `connection_id`, so the slot
/// need only exist in the workspace. Not the guarded creator either:
/// `vestrace_create_connection_revision_and_advance_head` validates the
/// execution guard and the head version, then passes `target_credential_slot_id`
/// straight into the INSERT. The refusal comes from the row trigger
/// `vestrace_validate_connection_revision_identity`, and from nowhere else.
///
/// Three callers therefore rest on one trigger: the repository's own
/// activation-guard precondition, the binding snapshot's composite edges, and
/// this function. Deleting the trigger would leave the schema's own foreign
/// keys satisfied by a revision pointing at a foreign Connection's slot, so
/// this test names the trigger's exact message rather than only its SQLSTATE —
/// a bare `23514` would not say which of the checks above spoke.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_connection_revision_pinning_another_connections_slot_is_refused_by_the_database(
    pool: PgPool,
) {
    let workspace = Uuid::now_v7();
    let principal = Uuid::now_v7();
    let (first, second) = two_branches(&pool, workspace, principal).await;

    let runtime = runtime_pool(&pool).await;
    let mut transaction = runtime.begin().await.unwrap();
    for (setting, value) in [
        ("vestrace.workspace_id", workspace),
        ("vestrace.principal_id", principal),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1,$2,true)")
            .bind(setting)
            .bind(value.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    // Head version 1: the fixture already published one revision per Connection,
    // so this is a lawful successor in every respect except the slot it names.
    let crossed = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(\
         $1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1','http://127.0.0.1:1234/v1',\
         'branch-isolation/v1','loopback_only','bearer',$5,1)",
    )
    .bind(Uuid::now_v7())
    .bind(workspace)
    .bind(first.connection)
    .bind(first.execution_guard)
    .bind(second.slot)
    .fetch_one(&mut *transaction)
    .await;

    // The pool holds one connection, so the transaction must be finished before
    // the pool is closed; a refusal leaves it aborted rather than closed.
    let persisted = match &crossed {
        Ok(revision) => {
            transaction.commit().await.unwrap();
            Some(*revision)
        }
        Err(_) => {
            transaction.rollback().await.unwrap();
            None
        }
    };
    runtime.close().await;

    match crossed {
        Err(error) => {
            let database = error
                .as_database_error()
                .expect("a crossed revision must be a database refusal");
            assert_eq!(
                database.code().as_deref(),
                Some("23514"),
                "got {}: {}",
                database.code().as_deref().unwrap_or("no SQLSTATE"),
                database.message()
            );
            assert_eq!(
                database.message(),
                "connection revision credential slot identity does not match",
                "the refusal must come from the revision identity trigger, not from a \
                 different check that happens to share its SQLSTATE"
            );
        }
        Ok(revision) => {
            // Name the reachable state rather than a suspicion.
            let row = sqlx::query(
                "SELECT connection_id, credential_slot_id FROM connection_revisions WHERE id = $1",
            )
            .bind(persisted.expect("a committed revision must be readable"))
            .fetch_one(&pool)
            .await
            .unwrap();
            panic!(
                "the guarded creator accepted a foreign credential slot: revision {revision} has \
                 connection_id={} and credential_slot_id={}, while that slot belongs to \
                 connection {}",
                row.get::<Uuid, _>("connection_id"),
                row.get::<Uuid, _>("credential_slot_id"),
                second.connection
            );
        }
    }
}

/// Both Connections advertise the same provider-side model string, and the
/// qualification earned on one must not reach the other.
///
/// This is the ambiguity the identity chain has to survive: read a wire model
/// id off the provider and there is nothing to tell the two apart. The chain
/// answers by never carrying that string in a key —
/// `model_qualification_revisions` is pinned by
/// `(workspace, model_revision, connection_revision)` and by
/// `(workspace, connection_revision, connection_qualification_revision)`, so a
/// qualification claiming one Connection's model against the other Connection's
/// evidence has nowhere to attach.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn one_wire_model_id_on_two_connections_cannot_share_a_qualification(pool: PgPool) {
    let workspace = Uuid::now_v7();
    let principal = Uuid::now_v7();
    let (first, second) = two_branches(&pool, workspace, principal).await;

    // The premise: without it the test below would pass for the wrong reason.
    let advertised: Vec<String> = sqlx::query_scalar(
        "SELECT wire_model_id FROM model_revisions WHERE id = ANY($1) ORDER BY id",
    )
    .bind(vec![first.model_revision, second.model_revision])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        advertised,
        vec![SHARED_WIRE_MODEL.to_owned(), SHARED_WIRE_MODEL.to_owned()],
        "both Connections must advertise the identical provider-side model string"
    );

    let mut guarded = scope(&pool, workspace, principal).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    let crossed = sqlx::query(
        "INSERT INTO model_qualification_revisions \
         (id,workspace_id,model_revision_id,connection_revision_id,\
          connection_qualification_revision_id,qualification_job_id,capabilities,valid_until) \
         SELECT $1,$2,$3,$4,$5,job.id,ARRAY['chat'],NOW() + INTERVAL '1 hour' \
           FROM qualification_jobs AS job \
          WHERE job.connection_revision_id = $4",
    )
    .bind(Uuid::now_v7())
    .bind(workspace)
    .bind(first.model_revision)
    .bind(second.revision)
    .bind(second.connection_qualification)
    .execute(&mut *guarded)
    .await
    .expect_err("one Connection's model must not be qualified by the other's evidence");

    let database = crossed
        .as_database_error()
        .expect("a crossed qualification must be a database refusal");
    assert_eq!(
        database.code().as_deref(),
        Some("23503"),
        "got {}: {}",
        database.code().as_deref().unwrap_or("no SQLSTATE"),
        database.message()
    );
    assert_eq!(
        database.constraint(),
        Some("model_qualification_revisions_model_revision_fkey"),
        "the composite edge that pins a model revision to its Connection revision \
         must be the one that refuses, not an incidental key"
    );
}
