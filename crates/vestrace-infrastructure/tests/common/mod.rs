//! The governed embedding fixture both embedding suites build on.
//!
//! Rust test binaries share no code, so without this module the dispatch suite
//! and the recovery suite would each carry their own definition of what a
//! governed embedding job is. Two copies of that definition drift, and a drifted
//! fixture produces evidence about a system that does not exist.
//!
//! Everything here goes through a guarded function or an ordinary runtime
//! insert, except the qualification and binding-snapshot rows P03 has no
//! publisher for -- a known gap this package records rather than papers over.

#![allow(dead_code)]

use std::{str::FromStr, sync::Arc};

use chrono::Utc;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, CredentialDispatchLeaseRepository, ExternalEffectRepository,
    IdempotencyRecord, ModelRequestEvidenceRepository, OutboxMessage, ProviderDispatchCause,
    ProviderDispatchFaultInjector, ProviderDispatchFaultPoint, ProviderDispatchPolicyEvaluator,
    ProviderDispatchRequest, RequestContext,
};
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, Capability, ConnectionId, ConnectionRevisionId,
    DeliverySemantics, EffectPrecondition, EffectReversibility, EmbeddingJobId,
    ExternalEffectIntent, IdempotencyProfile, ModelRequestEvidenceId, PolicyDecision,
    PolicyDecisionId, PolicyDecisionReason, PolicyDecisionResult, PrincipalId, RiskCategory,
    WorkerId, WorkspaceId, id::AuditEventId, security::PolicyInputState,
};
use vestrace_infrastructure::{
    PgExternalEffectRepository, PgGovernedMutationRepository, PgInstallationMutationPermit,
    PgModelDataPolicyDecisionRepository, PgProviderDispatchRepository, PgStore,
};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

pub async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
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
        .max_connections(2)
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

/// One embedding effect intent, scoped to the workspace rather than a run.
///
/// `workspace://` is what a non-Run governed effect says here, and it is what
/// P03's qualification probe already says. The domain's closed vocabulary is
/// `run://`, `execution://` and `workspace://`; an embedding delivery job has no
/// run behind it, and the reference names an execution rather than an owner, so
/// inventing `embedding://` would widen a vocabulary to record something it does
/// not mean. The job row references the effect; the effect does not reference
/// the job.
pub fn workspace_scoped_intent(
    context: &RequestContext,
    snapshot_id: Uuid,
) -> ExternalEffectIntent {
    ExternalEffectIntent::new(
        "workspace://",
        context.workspace_id,
        context.principal_id,
        "openai-compatible",
        "embeddings",
        "http://127.0.0.1:1234/v1/embeddings",
        "sha256:arguments",
        "produce a governed embedding",
        vec![EffectPrecondition::new("model-snapshot", snapshot_id.to_string()).unwrap()],
        "sha256:preconditions",
        RiskCategory::Medium,
        EffectReversibility::Unknown,
        IdempotencyProfile::ProviderKey,
        DeliverySemantics::AtLeastOnce,
        Capability::ExportRead,
        None::<String>,
        None::<String>,
        Utc::now(),
    )
    .unwrap()
}

/// A fresh intent for a second job in an already-prepared fixture.
pub fn embedding_intent(fixture: &AcceptedJob, _job_id: EmbeddingJobId) -> ExternalEffectIntent {
    workspace_scoped_intent(&fixture.context, fixture.snapshot_id)
}

/// The durable identities one accepted embedding job is made of.
pub struct AcceptedJob {
    pub context: RequestContext,
    pub job_id: EmbeddingJobId,
    pub connection_id: Uuid,
    pub connection_revision_id: Uuid,
    pub connection_qualification_id: Uuid,
    pub model_revision_id: Uuid,
    pub space_registration_id: Uuid,
    pub model_qualification_id: Uuid,
    pub no_auth_binding_id: Uuid,
    pub snapshot_id: Uuid,
    pub external_effect_id: Uuid,
    pub evidence_id: Uuid,
    pub intent: ExternalEffectIntent,
}

/// Builds one no-auth embedding job the way a deployment would, and stops.
///
/// Everything here goes through a guarded function or an ordinary runtime
/// insert; nothing is written as `vestrace_guarded_owner` except the
/// qualification and snapshot rows P03 has no publisher for yet, which is a
/// known gap recorded in this package's evidence rather than something this
/// fixture may pretend away.
pub async fn accept_embedding_job(owner: &PgPool, runtime: &PgPool) -> AcceptedJob {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    let context = RequestContext::new(workspace_id, principal_id);
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let connection_revision_id = Uuid::now_v7();
    let no_auth_id = Uuid::now_v7();
    let provider_id = Uuid::now_v7();
    let model_id = Uuid::now_v7();
    let model_revision_id = Uuid::now_v7();
    let qualification_job_id = Uuid::now_v7();
    let connection_qualification_id = Uuid::now_v7();
    let model_qualification_id = Uuid::now_v7();
    let snapshot_id = Uuid::now_v7();
    let space_registration_id = Uuid::now_v7();
    let job_id = EmbeddingJobId::new();
    let evidence_id = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-{workspace_id}"))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("embedding-{principal_id}"))
        .execute(owner)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors(id,workspace_id,name,provider_type) VALUES($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(format!("embedding-{connector_id}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections(id,connector_id,workspace_id,principal_id,name,status) \
         VALUES($1,$2,$3,$4,$5,'active')",
    )
    .bind(connection_id)
    .bind(connector_id)
    .bind(workspace_id.as_uuid())
    .bind(principal_id.as_uuid())
    .bind(format!("embedding-{connection_id}"))
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO providers(id,workspace_id,name,locality) VALUES($1,$2,'provider','local')",
    )
    .bind(provider_id)
    .bind(workspace_id.as_uuid())
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO models(id,provider_id,workspace_id,model_name,context_window,\
         input_cost_per_mtoken,output_cost_per_mtoken) VALUES($1,$2,$3,'embedding-model',4096,0,0)",
    )
    .bind(model_id)
    .bind(provider_id)
    .bind(workspace_id.as_uuid())
    .execute(owner)
    .await
    .unwrap();

    let mut governed = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(guard_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .execute(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(\
           $1,$2,$3,$4,'lm_studio_local','http://127.0.0.1:1234/v1',\
           'http://127.0.0.1:1234/v1','lm-studio-local/v1','loopback_only','none',NULL,0)",
    )
    .bind(connection_revision_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_id)
    .bind(guard_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_no_auth_binding_revision($1,$2,$3,$4)")
        .bind(no_auth_id)
        .bind(workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .fetch_one(&mut *governed)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_revision_and_advance_head(\
           $1,$2,$3,$4,$5,$6,'embedding-model','embedding',NULL,NULL,NULL,NULL,NULL,NULL,0)",
    )
    .bind(model_revision_id)
    .bind(workspace_id.as_uuid())
    .bind(model_id)
    .bind(connection_id)
    .bind(guard_id)
    .bind(connection_revision_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    // The space is authorized by name, model and dimensions, not minted as a
    // side effect of the first vector written into it.
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_register_embedding_space($1,$2,$3,'nomic-768',\
           'text-embedding-nomic-embed-text-v1.5',768)",
    )
    .bind(space_registration_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_id)
    .fetch_one(&mut *governed)
    .await
    .unwrap();
    governed.commit().await.unwrap();

    let intent = workspace_scoped_intent(&context, snapshot_id);
    let external_effect_id = intent.id().as_uuid();
    PgExternalEffectRepository::new(PgStore::from_pool(runtime.clone()))
        .insert_intent(&context, &intent)
        .await
        .unwrap();

    // Qualification and the binding snapshot have no governed publisher yet.
    // P03 left both to be written as the guarded owner, and P04's Task 4
    // deliberately fixed only the admission policy, so this is written the way
    // P03's own suites write it and the evidence says so.
    let mut seeded = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *seeded)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *seeded)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO qualification_jobs(id,workspace_id,connection_revision_id,profile_revision,\
         state,completed_at) VALUES($1,$2,$3,'q1','succeeded',NOW())",
    )
    .bind(qualification_job_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_revision_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connection_qualification_revisions(id,workspace_id,connection_revision_id,\
         qualification_job_id,profile_revision,valid_until,capabilities) \
         VALUES($1,$2,$3,$4,'q1',NOW()+INTERVAL '1 hour',ARRAY['embedding']::TEXT[])",
    )
    .bind(connection_qualification_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_revision_id)
    .bind(qualification_job_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_qualification_revisions(id,workspace_id,model_revision_id,\
         connection_revision_id,connection_qualification_revision_id,qualification_job_id,\
         capabilities,valid_until) \
         VALUES($1,$2,$3,$4,$5,$6,ARRAY['embedding']::TEXT[],NOW()+INTERVAL '1 hour')",
    )
    .bind(model_qualification_id)
    .bind(workspace_id.as_uuid())
    .bind(model_revision_id)
    .bind(connection_revision_id)
    .bind(connection_qualification_id)
    .bind(qualification_job_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_binding_snapshots(id,workspace_id,connection_id,connection_revision_id,\
         connection_qualification_revision_id,model_revision_id,model_qualification_revision_id,\
         branch,no_auth_binding_revision_id) VALUES($1,$2,$3,$4,$5,$6,$7,'no_auth',$8)",
    )
    .bind(snapshot_id)
    .bind(workspace_id.as_uuid())
    .bind(connection_id)
    .bind(connection_revision_id)
    .bind(connection_qualification_id)
    .bind(model_revision_id)
    .bind(model_qualification_id)
    .bind(no_auth_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    // P04 Task 6 makes snapshot scope a positive, deferred invariant.  This
    // fixture is ordinary work, so it must write its immutable ordinary mark in
    // the same transaction as the snapshot it seeds.
    sqlx::query(
        "INSERT INTO model_binding_snapshot_scopes(\
         workspace_id,snapshot_id,scope,transition_plan_id) VALUES($1,$2,'ordinary',NULL)",
    )
    .bind(workspace_id.as_uuid())
    .bind(snapshot_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    seeded.commit().await.unwrap();

    // Acceptance itself is guarded and runs as the runtime role, in the
    // canonical lock order.
    let mut accept = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *accept)
        .await
        .unwrap();
    let accepted = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_accept_embedding_job($1,$2,$3,'delivery',$4,$5,$6,NULL,NULL::BIGINT)",
    )
    .bind(job_id.as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(space_registration_id)
    .bind(snapshot_id)
    .bind(external_effect_id)
    .bind(evidence_id)
    .fetch_one(&mut *accept)
    .await
    .expect("the runtime role accepts an embedding job through its guarded function");
    assert_eq!(accepted, job_id.as_uuid());
    accept.commit().await.unwrap();

    AcceptedJob {
        context,
        job_id,
        connection_id,
        connection_revision_id,
        connection_qualification_id,
        model_revision_id,
        space_registration_id,
        model_qualification_id,
        no_auth_binding_id: no_auth_id,
        snapshot_id,
        external_effect_id,
        evidence_id,
        intent,
    }
}

/// The production dispatch authority, composed with the collaborators the two
/// methods under test never reach.
///
/// They are `unreachable!` rather than permissive doubles on purpose: plan
/// rediscovery and recovery must not evaluate policy, mint evidence, or take a
/// credential lease, and a double that quietly answered would hide it if they
/// did.
pub fn dispatch_repository(runtime: &PgPool) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    PgProviderDispatchRepository::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        Arc::new(UnusedEvidence),
        Arc::new(PgExternalEffectRepository::new(store.clone())),
        Arc::new(UnusedCredentialLeases),
        Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone())),
        Arc::new(PgGovernedMutationRepository::new(store)),
        Arc::new(UnusedPolicy),
    )
}

pub struct UnusedEvidence;

#[async_trait::async_trait]
impl ModelRequestEvidenceRepository for UnusedEvidence {
    async fn create_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _creation: &vestrace_application::CreateModelRequestEvidence,
    ) -> Result<vestrace_domain::ModelRequestEvidenceId, ApplicationError> {
        unreachable!("rediscovery and recovery author no evidence")
    }

    async fn reconstruct_current_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _workspace: WorkspaceId,
        _evidence: vestrace_domain::ModelRequestEvidenceId,
    ) -> Result<vestrace_application::ModelRequestReconstruction, ApplicationError> {
        unreachable!("rediscovery and recovery reconstruct no request")
    }
}

pub struct UnusedCredentialLeases;

#[async_trait::async_trait]
impl CredentialDispatchLeaseRepository for UnusedCredentialLeases {
    async fn issue(
        &self,
        _context: &RequestContext,
        _request: vestrace_application::CredentialDispatchLeaseRequest,
    ) -> Result<vestrace_application::CredentialDispatchLease, ApplicationError> {
        unreachable!("rediscovery and recovery take no credential lease")
    }

    async fn issue_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _request: vestrace_application::CredentialDispatchLeaseRequest,
    ) -> Result<vestrace_application::CredentialDispatchLease, ApplicationError> {
        unreachable!("rediscovery and recovery take no credential lease")
    }

    async fn consume_for_dispatch(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _lease: &vestrace_application::CredentialDispatchLease,
    ) -> Result<vestrace_application::ConnectionAuth, ApplicationError> {
        unreachable!("rediscovery and recovery consume no credential lease")
    }
}

pub struct UnusedPolicy;

#[async_trait::async_trait]
impl ProviderDispatchPolicyEvaluator for UnusedPolicy {
    async fn evaluate(
        &self,
        _context: &RequestContext,
        _intent: &vestrace_domain::ExternalEffectIntent,
        _cause: &vestrace_application::ProviderDispatchCause,
        _target: &vestrace_application::ProviderDispatchTarget,
        _request: &vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::ProviderDispatchPolicyEvaluation, ApplicationError> {
        unreachable!("rediscovery and recovery evaluate no policy")
    }
}

/// Completes an accepted job into a dispatchable one.
///
/// Acceptance reserves the evidence identity; the root, its nodes and its
/// completeness check are authored afterwards, exactly as a Run step's are. The
/// admission policy is published through the guarded compare-and-swap this
/// package added, as the runtime role, so nothing here writes a row a
/// deployment could not.
pub async fn make_dispatchable(owner: &PgPool, runtime: &PgPool, fixture: &AcceptedJob) {
    let mut published = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *published)
        .await
        .unwrap();
    let shape_id = Uuid::now_v7();
    let limits_id = Uuid::now_v7();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_model_request_shape_revision(\
           $1,$2,1,'embeddings',false,ARRAY[]::TEXT[])",
    )
    .bind(shape_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(&mut *published)
    .await
    .unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_create_model_limits_revision($1,$2,1,8,1,2048)")
        .bind(limits_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .fetch_one(&mut *published)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i64>(
        "SELECT vestrace_publish_connection_admission_policy(\
           $1,$2,$3,0::BIGINT,1::SMALLINT,60000,30,900)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .fetch_one(&mut *published)
    .await
    .unwrap();
    published.commit().await.unwrap();

    let mut evidence = owner.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *evidence)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *evidence)
        .await
        .unwrap();
    // `cause_kind='embedding_job'` and `cause_id` the job: the third cause this
    // package added to the evidence vocabulary, with the same snapshot-rooted
    // shape as a Run step's.
    sqlx::query(
        "INSERT INTO model_request_evidence_roots\
         (id,workspace_id,external_effect_id,request_kind,binding_snapshot_id,cause_kind,cause_id) \
         VALUES ($1,$2,$3,'embeddings',$4,'embedding_job',$5)",
    )
    .bind(fixture.evidence_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.external_effect_id)
    .bind(fixture.snapshot_id)
    .bind(fixture.job_id.as_uuid())
    .execute(&mut *evidence)
    .await
    .unwrap();
    let kinds = [
        "external_effect",
        "binding_snapshot",
        "connection_revision",
        "connection_qualification_revision",
        "model_revision",
        "model_qualification_revision",
        "request_shape_revision",
        "limits_revision",
    ];
    let references = [
        fixture.external_effect_id,
        fixture.snapshot_id,
        fixture.connection_revision_id,
        fixture.connection_qualification_id,
        fixture.model_revision_id,
        fixture.model_qualification_id,
        shape_id,
        limits_id,
    ];
    for (ordinal, (kind, reference)) in kinds.iter().zip(references).enumerate() {
        let version =
            matches!(*kind, "request_shape_revision" | "limits_revision").then_some(1_i64);
        sqlx::query(
            "INSERT INTO model_request_evidence_nodes\
             (id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id,\
              reference_version) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.evidence_id)
        .bind(ordinal as i32)
        .bind(kind)
        .bind(reference)
        .bind(version)
        .execute(&mut *evidence)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO model_request_evidence_checks\
         (id,workspace_id,evidence_root_id,status,missing_reference_count) \
         VALUES ($1,$2,$3,'complete',0)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.evidence_id)
    .execute(&mut *evidence)
    .await
    .unwrap();
    evidence.commit().await.unwrap();
}

pub fn embedding_dispatch_request(fixture: &AcceptedJob) -> ProviderDispatchRequest {
    let now = Utc::now();
    ProviderDispatchRequest {
        context: fixture.context.clone(),
        intent: fixture.intent.clone(),
        model_request_evidence_id: ModelRequestEvidenceId::from_uuid(fixture.evidence_id),
        connection_id: ConnectionId::from_uuid(fixture.connection_id),
        connection_revision_id: ConnectionRevisionId::from_uuid(fixture.connection_revision_id),
        cause: ProviderDispatchCause::EmbeddingJob {
            job_id: fixture.job_id,
            snapshot_id: fixture.snapshot_id,
        },
        admission_id: Uuid::now_v7(),
        wait_id: Uuid::now_v7(),
        concurrency_lease_id: Uuid::now_v7(),
        dispatch_ttl_seconds: 60,
        worker_id: WorkerId::new(),
        credential: None,
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "provider.dispatch.prepared",
            "external_effect",
            fixture.external_effect_id,
            serde_json::json!({"phase": "pre_dispatch"}),
            now,
        )
        .unwrap(),
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("embedding-dispatch:{}", fixture.external_effect_id),
            workspace_id: fixture.context.workspace_id,
            request_hash: "safe-embedding-dispatch-tuple-v1".to_owned(),
            response_payload: None,
            status: "completed".to_owned(),
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
        }),
        outbox: vec![OutboxMessage::new(
            fixture.context.workspace_id,
            "provider.dispatch.prepared",
            serde_json::json!({"effect_id": fixture.external_effect_id}),
            now,
        )],
    }
}

/// Reconstructs the one embedding request this suite dispatches.
///
/// It is a fixed value rather than a real reconstruction because the property
/// under test is the transaction, not the request: what must hold is that every
/// leg of one dispatch commits or rolls back together.
pub struct FixedEmbeddingEvidence;

#[async_trait::async_trait]
impl ModelRequestEvidenceRepository for FixedEmbeddingEvidence {
    async fn create_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        creation: &vestrace_application::CreateModelRequestEvidence,
    ) -> Result<ModelRequestEvidenceId, ApplicationError> {
        Ok(ModelRequestEvidenceId::from_uuid(creation.root_id()))
    }

    async fn reconstruct_current_in(
        &self,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _workspace: WorkspaceId,
        _evidence: ModelRequestEvidenceId,
    ) -> Result<vestrace_application::ModelRequestReconstruction, ApplicationError> {
        Ok(vestrace_application::ModelRequestReconstruction::Complete(
            vestrace_application::EffectiveModelRequest::embeddings(
                "text-embedding-nomic-embed-text-v1.5",
                vec!["a governed embedding input".to_owned()],
                vestrace_application::EffectiveRequestLimits::new(8, 1, 2048).unwrap(),
            )
            .unwrap(),
        ))
    }
}

pub struct AllowEmbeddingPolicy;

#[async_trait::async_trait]
impl ProviderDispatchPolicyEvaluator for AllowEmbeddingPolicy {
    async fn evaluate(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        cause: &vestrace_application::ProviderDispatchCause,
        _target: &vestrace_application::ProviderDispatchTarget,
        _request: &vestrace_application::EffectiveModelRequest,
    ) -> Result<vestrace_application::ProviderDispatchPolicyEvaluation, ApplicationError> {
        // The cause reaching policy evaluation must be the embedding one. A
        // Run-step cause here would mean the dispatch had been composed for
        // another owner, and this double would otherwise allow it silently.
        assert!(
            matches!(
                cause,
                vestrace_application::ProviderDispatchCause::EmbeddingJob { .. }
            ),
            "policy evaluation received {cause:?}"
        );
        let request = AuthorizationRequest::new(
            intent.required_capability(),
            intent.operation(),
            intent.target(),
            intent.risk(),
        );
        Ok(vestrace_application::ProviderDispatchPolicyEvaluation {
            authorization: PolicyDecision {
                id: PolicyDecisionId::new(),
                policy_id: None,
                policy_version: "embedding-dispatch-v1".into(),
                workspace_id: context.workspace_id,
                subject_id: context.principal_id,
                capability: intent.required_capability(),
                operation: intent.operation().into(),
                resource_scope: intent.target().into(),
                result: PolicyDecisionResult::Allow,
                reason: PolicyDecisionReason::ConfiguredAllowance,
                input_state: PolicyInputState::from_request(
                    context.workspace_id,
                    context.principal_id,
                    &request,
                ),
                matched_grant_id: None,
                decided_at: Utc::now(),
            },
            // An embedding job's disclosure decision belongs to the embedding
            // data policy gate, not to the model data policy record, which names
            // a run and a step.
            model_data_policy: None,
        })
    }
}

pub fn dispatching_repository(
    runtime: &PgPool,
    fault: Option<ProviderDispatchFaultPoint>,
) -> PgProviderDispatchRepository {
    let store = PgStore::from_pool(runtime.clone());
    let permit = Arc::new(PgInstallationMutationPermit::new(store.clone()));
    let evidence = Arc::new(FixedEmbeddingEvidence);
    let effects = Arc::new(PgExternalEffectRepository::new(store.clone()));
    let credential = Arc::new(UnusedCredentialLeases);
    let data_policy = Arc::new(PgModelDataPolicyDecisionRepository::new(store.clone()));
    let governed = Arc::new(PgGovernedMutationRepository::new(store));
    let policy = Arc::new(AllowEmbeddingPolicy);
    match fault {
        Some(point) => PgProviderDispatchRepository::with_faults(
            permit,
            evidence,
            effects,
            credential,
            data_policy,
            governed,
            policy,
            Arc::new(FailAtDispatchPoint(point)),
        ),
        None => PgProviderDispatchRepository::new(
            permit,
            evidence,
            effects,
            credential,
            data_policy,
            governed,
            policy,
        ),
    }
}

/// Fails once at one named boundary, so the transaction is torn at exactly the
/// leg under test and nowhere else.
pub struct FailAtDispatchPoint(ProviderDispatchFaultPoint);

impl ProviderDispatchFaultInjector for FailAtDispatchPoint {
    fn check(&self, point: ProviderDispatchFaultPoint) -> Result<(), ApplicationError> {
        if point == self.0 {
            return Err(ApplicationError::Internal(format!(
                "injected embedding dispatch fault at {point:?}"
            )));
        }
        Ok(())
    }
}

pub fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}
