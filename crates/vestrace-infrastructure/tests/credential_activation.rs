use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use chrono::Duration;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, CandidateCredentialAbandonCommand, CandidateCredentialAbandonService,
    CredentialActivationCommand, CredentialActivationError, CredentialActivationRepository,
    CredentialRevocationCommand, CredentialRotationCommand, FenceReceipt, IdempotencyRecord,
    MaterialKeyVault, OutboxMessage, RequestContext, VaultError,
};
use vestrace_domain::{
    AuditEvent, ConnectionId, CredentialSlotId, ErasureReceipt, IntentNonce, MaterialKeyId,
    PrincipalId, VaultReceipt, WorkspaceId, ZeroizingDek, id::AuditEventId, time::now,
};
use vestrace_infrastructure::{
    PgCredentialActivationRepository, PgMaterialErasureRepository, PgStore,
};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

#[derive(Clone, Copy)]
struct Candidate {
    intent_id: Uuid,
    revision_id: Uuid,
    occupancy_id: Uuid,
}

struct Fixture {
    context: RequestContext,
    connection_id: ConnectionId,
    slot_id: CredentialSlotId,
    execution_guard_id: Uuid,
    activation_guard_id: Uuid,
    connection_revision_id: Uuid,
    qualification_revision_id: Uuid,
    first: Candidate,
}

#[derive(Default)]
struct CandidateAbandonVaultCounters {
    prepare_calls: AtomicUsize,
    erase_calls: AtomicUsize,
}

struct CandidateAbandonVault {
    counters: Arc<CandidateAbandonVaultCounters>,
}

impl MaterialKeyVault for CandidateAbandonVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

    fn prepare_erasure(&self, _key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        self.counters.prepare_calls.fetch_add(1, Ordering::SeqCst);
        Ok(FenceReceipt::from_uuid(Uuid::from_u128(0xC11)))
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        self.counters.erase_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ErasureReceipt::from_uuid(Uuid::from_u128(0xC12)))
    }
}

fn audit(context: &RequestContext, action: &str, resource_id: Uuid) -> AuditEvent {
    AuditEvent::new(
        AuditEventId::new(),
        context.workspace_id,
        context.principal_id,
        action,
        "credential",
        resource_id,
        serde_json::json!({"resource_id": resource_id}),
        now(),
    )
    .expect("activation test audit event is valid")
}

fn evidence(
    context: &RequestContext,
    action: &str,
    resource_id: Uuid,
) -> (IdempotencyRecord, OutboxMessage, AuditEvent) {
    let created_at = now();
    (
        IdempotencyRecord {
            idempotency_key: format!("credential-activation-{}", Uuid::now_v7()),
            workspace_id: context.workspace_id,
            request_hash: format!("credential-activation-hash-{}", Uuid::now_v7()),
            response_payload: Some(serde_json::json!({"credential_revision_id": resource_id})),
            status: "completed".to_owned(),
            created_at,
            expires_at: created_at + Duration::hours(1),
        },
        OutboxMessage::new(
            context.workspace_id,
            action,
            serde_json::json!({"credential_revision_id": resource_id}),
            created_at,
        ),
        audit(context, action, resource_id),
    )
}

fn activate_command(
    fixture: &Fixture,
    candidate: Candidate,
    expected_slot_version: u64,
) -> CredentialActivationCommand {
    let (idempotency, outbox, audit) = evidence(
        &fixture.context,
        "credential.activated",
        candidate.revision_id,
    );
    CredentialActivationCommand {
        connection_id: fixture.connection_id,
        credential_slot_id: fixture.slot_id,
        execution_guard_id: fixture.execution_guard_id,
        activation_guard_id: fixture.activation_guard_id,
        credential_revision_id: candidate.revision_id,
        credential_intent_id: candidate.intent_id,
        connection_qualification_revision_id: fixture.qualification_revision_id,
        expected_slot_version,
        idempotency: Some(idempotency),
        outbox: vec![outbox],
        audit,
    }
}

fn activate_command_for_qualification(
    fixture: &Fixture,
    candidate: Candidate,
    qualification_revision_id: Uuid,
    expected_slot_version: u64,
) -> CredentialActivationCommand {
    let mut command = activate_command(fixture, candidate, expected_slot_version);
    command.connection_qualification_revision_id = qualification_revision_id;
    command
}

fn rotation_command(
    fixture: &Fixture,
    previous: Candidate,
    activated: Candidate,
    expected_slot_version: u64,
) -> CredentialRotationCommand {
    let (idempotency, outbox, audit) = evidence(
        &fixture.context,
        "credential.rotated",
        activated.revision_id,
    );
    CredentialRotationCommand {
        connection_id: fixture.connection_id,
        credential_slot_id: fixture.slot_id,
        execution_guard_id: fixture.execution_guard_id,
        activation_guard_id: fixture.activation_guard_id,
        previous_credential_revision_id: previous.revision_id,
        activated_credential_revision_id: activated.revision_id,
        activated_credential_intent_id: activated.intent_id,
        connection_qualification_revision_id: fixture.qualification_revision_id,
        expected_slot_version,
        idempotency: Some(idempotency),
        outbox: vec![outbox],
        audit,
    }
}

fn revoke_command(
    fixture: &Fixture,
    candidate: Candidate,
    expected_slot_version: u64,
) -> CredentialRevocationCommand {
    let (idempotency, outbox, audit) = evidence(
        &fixture.context,
        "credential.revoked",
        candidate.revision_id,
    );
    CredentialRevocationCommand {
        connection_id: fixture.connection_id,
        credential_slot_id: fixture.slot_id,
        execution_guard_id: fixture.execution_guard_id,
        activation_guard_id: fixture.activation_guard_id,
        credential_revision_id: candidate.revision_id,
        credential_intent_id: candidate.intent_id,
        expected_slot_version,
        idempotency: Some(idempotency),
        outbox: vec![outbox],
        audit,
    }
}

fn candidate_abandon_command(
    fixture: &Fixture,
    candidate: Candidate,
    expected_association_version: u64,
) -> CandidateCredentialAbandonCommand {
    let (idempotency, outbox, audit) = evidence(
        &fixture.context,
        "credential.candidate_abandoned",
        candidate.intent_id,
    );
    CandidateCredentialAbandonCommand {
        credential_intent_id: candidate.intent_id,
        expected_association_version,
        idempotency,
        outbox: vec![outbox],
        audit,
    }
}

async fn scoped_transaction<'a>(
    pool: &'a PgPool,
    context: &RequestContext,
) -> sqlx::Transaction<'a, sqlx::Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    for (name, value) in [
        ("vestrace.workspace_id", context.workspace_id.to_string()),
        ("vestrace.principal_id", context.principal_id.to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(name)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    transaction
}

async fn prepare_candidate(pool: &PgPool, fixture: &Fixture) -> Candidate {
    let candidate = Candidate {
        intent_id: Uuid::now_v7(),
        revision_id: Uuid::now_v7(),
        occupancy_id: Uuid::now_v7(),
    };
    let mut transaction = scoped_transaction(pool, &fixture.context).await;
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
        .bind(candidate.occupancy_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.connection_id.as_uuid())
        .bind(fixture.slot_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "SELECT vestrace_reserve_credential_key_creation_intent(\
         $1, $2, $3, $4, $5, $6, $7, $8, 'credential_v2')",
    )
    .bind(candidate.intent_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id.as_uuid())
    .bind(fixture.slot_id.as_uuid())
    .bind(candidate.occupancy_id)
    .bind(candidate.revision_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
        .bind(candidate.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
        .bind(candidate.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
        .bind(candidate.intent_id)
        .bind(Uuid::now_v7())
        .bind(vec![0xA5_u8; 32])
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
        .bind(candidate.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
        .bind(candidate.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    candidate
}

async fn fixture(pool: &PgPool) -> Fixture {
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let connection_id = ConnectionId::new();
    let slot_id = CredentialSlotId::new();
    let execution_guard_id = Uuid::now_v7();
    let activation_guard_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("credential-activation-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!(
            "credential-activation-principal-{}",
            context.principal_id
        ))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, 'local')",
    )
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(format!("credential-activation-connector-{connector_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(connection_id.as_uuid())
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .bind(format!("credential-activation-connection-{connection_id}"))
    .execute(pool)
    .await
    .unwrap();

    let mut transaction = scoped_transaction(pool, &context).await;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(execution_guard_id)
        .bind(context.workspace_id.as_uuid())
        .bind(connection_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, 'provider', 'primary')")
        .bind(slot_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(connection_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
        .bind(activation_guard_id)
        .bind(context.workspace_id.as_uuid())
        .bind(connection_id.as_uuid())
        .bind(slot_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    let mut fixture = Fixture {
        context,
        connection_id,
        slot_id,
        execution_guard_id,
        activation_guard_id,
        connection_revision_id: Uuid::nil(),
        qualification_revision_id: Uuid::now_v7(),
        first: Candidate {
            intent_id: Uuid::nil(),
            revision_id: Uuid::nil(),
            occupancy_id: Uuid::nil(),
        },
    };
    fixture.first = prepare_candidate(pool, &fixture).await;

    let connection_revision_id = Uuid::now_v7();
    fixture.connection_revision_id = connection_revision_id;
    let qualification_job_id = Uuid::now_v7();
    let mut guarded = scoped_transaction(pool, &fixture.context).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *guarded)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connection_revisions (\
         id, workspace_id, connection_id, execution_guard_id, kind, logical_base_url, \
         runtime_base_url, adapter_profile_revision, transport_policy, auth_mode, credential_slot_id\
         ) VALUES ($1, $2, $3, $4, 'lm_studio_local', 'http://127.0.0.1:1234/v1', \
         'http://127.0.0.1:1234/v1', 'credential-activation/v1', 'loopback_only', 'bearer', $5)",
    )
    .bind(connection_revision_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id.as_uuid())
    .bind(fixture.execution_guard_id)
    .bind(fixture.slot_id.as_uuid())
    .execute(&mut *guarded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_revision_heads (workspace_id, connection_id, current_revision_id, version) \
         VALUES ($1, $2, $3, 1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id.as_uuid())
    .bind(connection_revision_id)
    .execute(&mut *guarded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs (\
         id, workspace_id, connection_revision_id, profile_revision, state, completed_at\
         ) VALUES ($1, $2, $3, 'credential-activation/v1', 'succeeded', NOW())",
    )
    .bind(qualification_job_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(connection_revision_id)
    .execute(&mut *guarded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions (\
         id, workspace_id, connection_revision_id, qualification_job_id, profile_revision, valid_until, capabilities\
         ) VALUES ($1, $2, $3, $4, 'credential-activation/v1', NOW() + INTERVAL '1 hour', ARRAY['chat'])",
    )
    .bind(fixture.qualification_revision_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(connection_revision_id)
    .bind(qualification_job_id)
    .execute(&mut *guarded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_heads (\
         workspace_id, connection_revision_id, current_qualification_revision_id, version\
         ) VALUES ($1, $2, $3, 1)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(connection_revision_id)
    .bind(fixture.qualification_revision_id)
    .execute(&mut *guarded)
    .await
    .unwrap();
    guarded.commit().await.unwrap();
    fixture
}

async fn pending_candidate_qualification(
    pool: &PgPool,
    fixture: &Fixture,
    candidate: Candidate,
) -> (Uuid, Uuid, Uuid) {
    let provider_id = Uuid::now_v7();
    let chat_model_id = Uuid::now_v7();
    let embedding_model_id = Uuid::now_v7();
    let chat_revision_id = Uuid::now_v7();
    let embedding_revision_id = Uuid::now_v7();
    let job_id = Uuid::now_v7();
    let target_id = Uuid::now_v7();
    let connection_qualification_id = Uuid::now_v7();
    let chat_qualification_id = Uuid::now_v7();
    let embedding_qualification_id = Uuid::now_v7();
    let mut transaction = scoped_transaction(pool, &fixture.context).await;
    sqlx::query(
        "INSERT INTO providers (id, workspace_id, name, locality) VALUES ($1,$2,$3,'local')",
    )
    .bind(provider_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(format!("q1-pending-provider-{provider_id}"))
    .execute(&mut *transaction)
    .await
    .unwrap();
    for (model_id, name) in [
        (chat_model_id, "q1-chat"),
        (embedding_model_id, "q1-embedding"),
    ] {
        sqlx::query(
            "INSERT INTO models (id, provider_id, workspace_id, model_name, context_window, input_cost_per_mtoken, output_cost_per_mtoken) \
             VALUES ($1,$2,$3,$4,4096,0,0)",
        )
        .bind(model_id)
        .bind(provider_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(name)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    for (revision_id, model_id, wire_model_id, kind) in [
        (chat_revision_id, chat_model_id, "q1-chat", "chat"),
        (
            embedding_revision_id,
            embedding_model_id,
            "q1-embedding",
            "embedding",
        ),
    ] {
        sqlx::query(
            "INSERT INTO model_revisions (id, workspace_id, model_id, connection_revision_id, wire_model_id, kind) \
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(revision_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(model_id)
        .bind(fixture.connection_revision_id)
        .bind(wire_model_id)
        .bind(kind)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO qualification_jobs (id, workspace_id, connection_revision_id, profile_revision, state, completed_at) \
         VALUES ($1,$2,$3,'openai-chat-completions-v1/q1','succeeded',NOW())",
    )
    .bind(job_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO qualification_target_bindings (\
             id, workspace_id, qualification_job_id, connection_id, connection_revision_id, branch, \
             credential_revision_id, credential_slot_id, credential_activation_guard_id, expected_slot_version, \
             chat_model_revision_id, embedding_model_revision_id\
         ) VALUES ($1,$2,$3,$4,$5,'credential',$6,$7,$8,0,$9,$10)",
    )
    .bind(target_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(job_id)
    .bind(fixture.connection_id.as_uuid())
    .bind(fixture.connection_revision_id)
    .bind(candidate.revision_id)
    .bind(fixture.slot_id.as_uuid())
    .bind(fixture.activation_guard_id)
    .bind(chat_revision_id)
    .bind(embedding_revision_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions (\
             id, workspace_id, connection_revision_id, qualification_job_id, profile_revision, valid_until, capabilities\
         ) VALUES ($1,$2,$3,$4,'openai-chat-completions-v1/q1',NOW() + INTERVAL '1 hour',ARRAY['q1'])",
    )
    .bind(connection_qualification_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_revision_id)
    .bind(job_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    for (model_qualification_id, model_revision_id, capability) in [
        (chat_qualification_id, chat_revision_id, "chat"),
        (
            embedding_qualification_id,
            embedding_revision_id,
            "embeddings",
        ),
    ] {
        sqlx::query(
            "INSERT INTO model_qualification_revisions (\
                id, workspace_id, model_revision_id, connection_revision_id, connection_qualification_revision_id, \
                qualification_job_id, capabilities, valid_until\
             ) VALUES ($1,$2,$3,$4,$5,$6,ARRAY[$7]::TEXT[],NOW() + INTERVAL '1 hour')",
        )
        .bind(model_qualification_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(model_revision_id)
        .bind(fixture.connection_revision_id)
        .bind(connection_qualification_id)
        .bind(job_id)
        .bind(capability)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    transaction.commit().await.unwrap();
    (
        connection_qualification_id,
        chat_qualification_id,
        embedding_qualification_id,
    )
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_database_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must supply restricted runtime credentials");
    let authority = runtime_database_url
        .split_once("://")
        .and_then(|(_, value)| value.rsplit_once('@'))
        .expect("VESTRACE_RUNTIME_DATABASE_URL must contain credentials");
    let (username, password) = authority
        .0
        .split_once(':')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must contain a password");
    let options = source
        .connect_options()
        .as_ref()
        .clone()
        .username(username)
        .password(password);
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let role = sqlx::query(
        "SELECT current_user::text AS role, rolsuper, rolbypassrls \
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(role.get::<String, _>("role"), "vestrace");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
    runtime
}

fn assert_conflict(
    result: Result<vestrace_application::GovernedMutationReceipt, CredentialActivationError>,
) {
    assert!(matches!(
        result,
        Err(CredentialActivationError::Application(ApplicationError::Conflict(code)))
            if code == "CREDENTIAL_SLOT_VERSION_CONFLICT"
    ));
}

fn assert_state_refusal(
    result: Result<vestrace_application::GovernedMutationReceipt, CredentialActivationError>,
) {
    assert!(matches!(
        result,
        Err(CredentialActivationError::Application(ApplicationError::Policy(code)))
            if code == "CREDENTIAL_STATE_REFUSED"
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_abandon_replay_returns_its_original_erasure_preparation(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let association_version: i64 = sqlx::query_scalar(
        "SELECT association_version FROM credential_guard_occupancies WHERE id=$1",
    )
    .bind(fixture.first.occupancy_id)
    .fetch_one(&pool)
    .await
    .expect("Candidate fixture must expose its durable association version");
    let mut first = scoped_transaction(&pool, &fixture.context).await;
    let prepared: (Uuid, Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(fixture.first.intent_id)
    .bind(association_version)
    .fetch_one(&mut *first)
    .await
    .expect("the exact Candidate association version must start one erasure lifecycle");
    first.commit().await.unwrap();

    let mut replay = scoped_transaction(&pool, &fixture.context).await;
    let replayed: (Uuid, Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(fixture.first.intent_id)
    .bind(association_version)
    .fetch_one(&mut *replay)
    .await
    .expect("Candidate-abandon restart must discover the same prepared erasure lifecycle");
    replay.commit().await.unwrap();

    assert_eq!(replayed, prepared);
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_abandon_refuses_stale_or_unequal_association_versions(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let association_version: i64 = sqlx::query_scalar(
        "SELECT association_version FROM credential_guard_occupancies WHERE id=$1",
    )
    .bind(fixture.first.occupancy_id)
    .fetch_one(&pool)
    .await
    .expect("Candidate fixture must expose its durable association version");

    let mut unequal = scoped_transaction(&pool, &fixture.context).await;
    let error = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
        "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(fixture.first.intent_id)
    .bind(association_version + 1)
    .fetch_one(&mut *unequal)
    .await
    .expect_err("a mismatched Candidate association version must not create preparation");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref(),
        Some("23514")
    );
    unequal.rollback().await.unwrap();

    let mut first = scoped_transaction(&pool, &fixture.context).await;
    sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
        "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(fixture.first.intent_id)
    .bind(association_version)
    .fetch_one(&mut *first)
    .await
    .expect("the original Candidate association version must prepare exactly once");
    first.commit().await.unwrap();

    let mut stale = scoped_transaction(&pool, &fixture.context).await;
    let error = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
        "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(fixture.first.intent_id)
    .bind(association_version + 1)
    .fetch_one(&mut *stale)
    .await
    .expect_err("the resulting version must not masquerade as the original replay key");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref(),
        Some("23514")
    );
    stale.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_abandon_runtime_acl_executes_only_the_guarded_function(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let association_version: i64 = sqlx::query_scalar(
        "SELECT association_version FROM credential_guard_occupancies WHERE id=$1",
    )
    .bind(fixture.first.occupancy_id)
    .fetch_one(&pool)
    .await
    .expect("Candidate fixture must expose its durable association version");
    let runtime = runtime_pool(&pool).await;

    let owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(proowner) FROM pg_proc \
         WHERE oid = 'vestrace_prepare_candidate_abandon_and_erasure(UUID, BIGINT)'::REGPROCEDURE",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let direct_insert: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('vestrace', 'material_erasure_preparations', 'INSERT')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, "vestrace_guarded_owner");
    assert!(
        !direct_insert,
        "runtime must not gain direct erasure preparation writes"
    );

    let mut transaction = scoped_transaction(&runtime, &fixture.context).await;
    let prepared: (Uuid, Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT preparation_id, material_key_id, finalized_erasure_receipt \
         FROM vestrace_prepare_candidate_abandon_and_erasure($1, $2)",
    )
    .bind(fixture.first.intent_id)
    .bind(association_version)
    .fetch_one(&mut *transaction)
    .await
    .expect("runtime may execute the one guarded Candidate-abandon function");
    transaction.commit().await.unwrap();
    assert!(prepared.2.is_none());
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_abandon_recovers_guarded_prepare_through_vault_fence_and_finalize(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let association_version: i64 = sqlx::query_scalar(
        "SELECT association_version FROM credential_guard_occupancies WHERE id=$1",
    )
    .bind(fixture.first.occupancy_id)
    .fetch_one(&pool)
    .await
    .expect("Candidate fixture must expose its durable association version");
    let command = candidate_abandon_command(&fixture, fixture.first, association_version as u64);

    // This is the durable phase committed before a crash can occur.  Recovery
    // must reuse its audit/idempotency/outbox and exactly one preparation.
    let phase_one = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));
    let prepared = phase_one
        .prepare_candidate_abandon(fixture.context.clone(), command.clone())
        .await
        .expect("guarded Candidate abandonment must commit before the vault call");
    assert!(prepared.finalized_receipt().is_none());

    let counters = Arc::new(CandidateAbandonVaultCounters::default());
    let service = CandidateCredentialAbandonService::new(
        PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone())),
        PgMaterialErasureRepository::new(PgStore::from_pool(pool.clone())),
        CandidateAbandonVault {
            counters: Arc::clone(&counters),
        },
    );
    let first = service
        .abandon(&fixture.context, command.clone())
        .await
        .expect("restart must fence, erase, and finalize the original preparation");
    let replay = service
        .abandon(&fixture.context, command.clone())
        .await
        .expect("finalized Candidate abandonment must replay without another vault call");

    assert_eq!(first, replay);
    assert_eq!(counters.prepare_calls.load(Ordering::SeqCst), 1);
    assert_eq!(counters.erase_calls.load(Ordering::SeqCst), 1);
    let state: String =
        sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
            .bind(fixture.first.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let lifecycle_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM material_erasure_preparations WHERE credential_intent_id = $1",
    )
    .bind(fixture.first.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE id = $1")
        .bind(command.audit.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let idempotency_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(&command.idempotency.idempotency_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "destroyed");
    assert_eq!(lifecycle_count, 1);
    assert_eq!(audit_count, 1);
    assert_eq!(idempotency_count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_abandon_rejects_same_request_key_with_a_different_hash(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let association_version: i64 = sqlx::query_scalar(
        "SELECT association_version FROM credential_guard_occupancies WHERE id=$1",
    )
    .bind(fixture.first.occupancy_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let command = candidate_abandon_command(&fixture, fixture.first, association_version as u64);
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .prepare_candidate_abandon(fixture.context.clone(), command.clone())
        .await
        .expect("the original request key/hash must commit Candidate phase one");

    let mut conflicting = command.clone();
    conflicting.idempotency.request_hash.push_str("-different");
    let result = repository
        .prepare_candidate_abandon(fixture.context.clone(), conflicting)
        .await;
    assert!(matches!(
        result,
        Err(CredentialActivationError::Application(ApplicationError::Conflict(code)))
            if code == "IDEMPOTENCY_KEY_REUSE_CONFLICT"
    ));
    let cancellations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_association_events \
         WHERE intent_id=$1 AND event_kind='credential_association_cancelled'",
    )
    .bind(fixture.first.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE id=$1")
        .bind(command.audit.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cancellations, 1);
    assert_eq!(audits, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_activation_moves_slot_version_and_closes_occupancy(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));

    repository
        .activate_first(
            fixture.context.clone(),
            activate_command(&fixture, fixture.first, 0),
        )
        .await
        .unwrap();

    let slot: (Option<Uuid>, i64, Option<i64>) = sqlx::query_as(
        "SELECT current_revision_id, current_revision_version, tombstone_version \
         FROM credential_slots WHERE id = $1",
    )
    .bind(fixture.slot_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let state: String =
        sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
            .bind(fixture.first.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let occupancy: String =
        sqlx::query_scalar("SELECT state FROM credential_guard_occupancies WHERE id = $1")
            .bind(fixture.first.occupancy_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(slot, (Some(fixture.first.revision_id), 1, None));
    assert_eq!(state, "active");
    assert_eq!(occupancy, "activated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_q1_activation_atomically_advances_connection_and_both_model_heads(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let (connection_qualification_id, chat_qualification_id, embedding_qualification_id) =
        pending_candidate_qualification(&pool, &fixture, fixture.first).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));

    repository
        .activate_first(
            fixture.context.clone(),
            activate_command_for_qualification(
                &fixture,
                fixture.first,
                connection_qualification_id,
                0,
            ),
        )
        .await
        .unwrap();

    let connection_head: Uuid = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM connection_qualification_heads \
         WHERE workspace_id=$1 AND connection_revision_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let model_heads: Vec<Uuid> = sqlx::query_scalar(
        "SELECT current_qualification_revision_id FROM model_qualification_heads \
         WHERE workspace_id=$1 ORDER BY current_qualification_revision_id",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(connection_head, connection_qualification_id);
    assert_eq!(model_heads, {
        let mut expected = vec![chat_qualification_id, embedding_qualification_id];
        expected.sort_unstable();
        expected
    });
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_activation_is_a_typed_conflict_and_leaves_no_evidence(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));

    assert_conflict(
        repository
            .activate_first(
                fixture.context.clone(),
                activate_command(&fixture, fixture.first, 1),
            )
            .await,
    );

    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
         (SELECT COUNT(*) FROM credential_activation_events WHERE credential_intent_id = $1), \
         (SELECT COUNT(*) FROM audit_events WHERE resource_id = $2), \
         (SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id = $3), \
         (SELECT COUNT(*) FROM outbox WHERE workspace_id = $3)",
    )
    .bind(fixture.first.intent_id)
    .bind(fixture.first.revision_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn activating_an_already_active_slot_is_refused(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .activate_first(
            fixture.context.clone(),
            activate_command(&fixture, fixture.first, 0),
        )
        .await
        .unwrap();

    assert_state_refusal(
        repository
            .activate_first(
                fixture.context.clone(),
                activate_command(&fixture, fixture.first, 1),
            )
            .await,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rotation_retires_outgoing_intent_and_leaves_one_active_intent(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .activate_first(
            fixture.context.clone(),
            activate_command(&fixture, fixture.first, 0),
        )
        .await
        .unwrap();
    let successor = prepare_candidate(&pool, &fixture).await;

    repository
        .rotate(
            fixture.context.clone(),
            rotation_command(&fixture, fixture.first, successor, 1),
        )
        .await
        .unwrap();

    let rotation_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_rotation_events \
         WHERE previous_credential_revision_id = $1 AND activated_credential_revision_id = $2",
    )
    .bind(fixture.first.revision_id)
    .bind(successor.revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let outgoing_state: String =
        sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
            .bind(fixture.first.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_key_creation_intents WHERE credential_slot_id = $1 AND state = 'active'",
    )
    .bind(fixture.slot_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(rotation_count, 1);
    assert_eq!(outgoing_state, "retired");
    assert_eq!(active_count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rotation_from_a_non_current_revision_is_refused(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .activate_first(
            fixture.context.clone(),
            activate_command(&fixture, fixture.first, 0),
        )
        .await
        .unwrap();
    let successor = prepare_candidate(&pool, &fixture).await;

    assert_state_refusal(
        repository
            .rotate(
                fixture.context.clone(),
                rotation_command(
                    &fixture,
                    Candidate {
                        revision_id: Uuid::now_v7(),
                        ..fixture.first
                    },
                    successor,
                    1,
                ),
            )
            .await,
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoke_is_immediate_and_blocks_later_resolution(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgCredentialActivationRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .activate_first(
            fixture.context.clone(),
            activate_command(&fixture, fixture.first, 0),
        )
        .await
        .unwrap();

    repository
        .revoke(
            fixture.context.clone(),
            revoke_command(&fixture, fixture.first, 1),
        )
        .await
        .unwrap();

    let slot: (Option<Uuid>, i64, Option<i64>, bool) = sqlx::query_as(
        "SELECT current_revision_id, current_revision_version, tombstone_version, tombstoned_at IS NOT NULL \
         FROM credential_slots WHERE id = $1",
    )
    .bind(fixture.slot_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let intent_state: String =
        sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
            .bind(fixture.first.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let revoked_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_activation_events \
         WHERE credential_revision_id = $1 AND event_kind = 'revoked'",
    )
    .bind(fixture.first.revision_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(slot, (None, 2, Some(2), true));
    assert_eq!(intent_state, "retired");
    assert_eq!(revoked_events, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_cannot_insert_activation_or_rotation_events(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO credential_activation_events (\
         id, workspace_id, connection_id, credential_slot_id, credential_revision_id, credential_intent_id, \
         event_kind, expected_slot_version, resulting_slot_version, audit_event_id\
         ) VALUES ($1, $2, $3, $4, $5, $6, 'active', 0, 1, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    let error = result.expect_err("runtime role must not insert activation events directly");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref(),
        Some("42501")
    );

    let result = sqlx::query(
        "INSERT INTO credential_rotation_events (\
         id, workspace_id, connection_id, credential_slot_id, previous_credential_revision_id, \
         activated_credential_revision_id, expected_slot_version, resulting_slot_version, audit_event_id\
         ) VALUES ($1, $2, $3, $4, $5, $6, 1, 2, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&runtime)
    .await;
    let error = result.expect_err("runtime role must not insert rotation events directly");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref(),
        Some("42501")
    );
    runtime.close().await;
}
