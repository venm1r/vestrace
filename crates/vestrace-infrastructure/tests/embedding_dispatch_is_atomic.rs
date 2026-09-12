//! One dispatch authority, two callers.
//!
//! An embedding job reaches the provider through the same five guarded
//! functions P03's Run-step dispatch calls -- `vestrace_try_admit_provider_
//! dispatch`, `vestrace_issue_credential_dispatch_lease`,
//! `vestrace_consume_credential_dispatch_lease`,
//! `vestrace_record_provider_throttle` and `vestrace_release_provider_dispatch`.
//! A second admission, throttle, lease or recovery function existing for
//! embeddings would be the defect this suite is here to make visible.

use std::{str::FromStr, sync::Arc, time::Duration};

use chrono::Utc;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    AcceptEmbeddingJob, ApplicationError, EmbeddingJobAttemptRecovery, EmbeddingJobDispatchPlan,
    EmbeddingJobRepository, EmbeddingJobTerminationService, PolicyDecisionEngine,
    ProviderDispatchCause, ProviderDispatchFaultPoint, ProviderDispatchRepository, RequestContext,
};
use vestrace_domain::{
    AuditEvent, AuthorizationRequest, EmbeddingJobId, EmbeddingSpaceId, ExternalEffectIntent,
    ModelRequestEvidenceId, PolicyDecision, PolicyDecisionId, PolicyDecisionReason,
    PolicyDecisionResult, PolicyInputState, PrincipalId, WorkspaceId, embedding::EmbeddingJobKind,
    id::AuditEventId,
};
use vestrace_infrastructure::{PgEmbeddingJobRepository, PgStore};

#[path = "common/mod.rs"]
mod common;

use common::result_preparation_fixture::{
    OutputVaultFixture, acceptance_command, attach_source_to_evidence, live_source, outputs,
    reconcile_output_receipts,
};
use common::*;

/// A job this suite can actually dispatch.
///
/// Since migration 0194 `vestrace_lock_embedding_job_pre_dispatch_gate` refuses
/// any `requested`/`running` embedding job without a nonempty, complete, exact
/// receipted output set -- and it is called from three places: the routing
/// lock, `vestrace_try_admit_provider_dispatch`, and the recovery authority.
/// This suite was built on P03's bare accepted job, which has no output set at
/// all, so all three refused it and the refusals arrived under three different
/// mapped names.
///
/// The job and its outputs must be fixed in one transaction, because an
/// already accepted bare job cannot be backfilled with guessed identities. So
/// the fixture starts from the pre-acceptance boundary rather than from
/// `accept_embedding_job`.
async fn dispatchable_embedding_job(owner: &PgPool, runtime: &PgPool) -> AcceptedJob {
    dispatchable_embedding_job_of_branch(owner, runtime, false).await
}

/// The same, on the credential branch.
async fn dispatchable_credential_embedding_job(owner: &PgPool, runtime: &PgPool) -> AcceptedJob {
    dispatchable_embedding_job_of_branch(owner, runtime, true).await
}

async fn dispatchable_embedding_job_of_branch(
    owner: &PgPool,
    runtime: &PgPool,
    credential_backed: bool,
) -> AcceptedJob {
    let accepted = if credential_backed {
        prepare_delivery_embedding_job_with_pinned_credential(owner, runtime).await
    } else {
        prepare_delivery_embedding_job(owner, runtime).await
    };
    let sources = [
        live_source(runtime, &accepted).await,
        live_source(runtime, &accepted).await,
        live_source(runtime, &accepted).await,
    ];
    make_dispatchable(owner, runtime, &accepted).await;
    for (ordinal, source) in sources.into_iter().enumerate() {
        attach_source_to_evidence(owner, &accepted, source, 8 + ordinal as i64).await;
    }
    let output_set = outputs();
    let receipt_id = Uuid::now_v7();
    vestrace_application::EmbeddingOutputKeyRepository::accept_delivery_outputs(
        &vestrace_infrastructure::postgres::PgEmbeddingOutputKeyRepository::new(
            PgStore::from_pool(runtime.clone()),
        ),
        &accepted.context,
        acceptance_command(&accepted, receipt_id, output_set.clone()),
    )
    .await
    .expect("the acceptance boundary must fix the job and its outputs together");
    let vault = OutputVaultFixture::new();
    reconcile_output_receipts(runtime, &accepted, &output_set, &vault).await;
    // No delivery policy decision is recorded. The pre-dispatch gate does not
    // read one -- that is a result-preparation concern -- and recording it
    // needs the provisioner-installed database this suite does not use.
    accepted
}

/// A repository that implements only what the trait requires, so the two new
/// methods fall through to their defaults.
struct UnconfiguredDispatch;

#[async_trait::async_trait]
impl ProviderDispatchRepository for UnconfiguredDispatch {
    async fn prepare_dispatch(
        &self,
        _request: vestrace_application::ProviderDispatchRequest,
    ) -> Result<vestrace_application::ProviderDispatchOutcome, ApplicationError> {
        unreachable!("this fixture never dispatches")
    }

    async fn complete_post_network(
        &self,
        _context: &RequestContext,
        _completion: vestrace_application::ProviderPostNetworkCompletion,
    ) -> Result<(), ApplicationError> {
        unreachable!("this fixture never dispatches")
    }

    async fn release_after_provider_result_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn vestrace_application::UnitOfWork,
        _authority: &vestrace_application::ProviderDispatchAuthority,
        _receipt: &vestrace_domain::ExternalEffectReceipt,
    ) -> Result<(), ApplicationError> {
        unreachable!("this fixture never dispatches")
    }

    async fn recover_lost_post_network(
        &self,
        _context: &RequestContext,
        _authority: &vestrace_application::ProviderDispatchAuthority,
        _recovered_at: chrono::DateTime<Utc>,
    ) -> Result<vestrace_application::ProviderLostDispatchRecovery, ApplicationError> {
        unreachable!("this fixture never dispatches")
    }
}

struct AllowCancellationPolicy;

#[derive(sqlx::FromRow)]
struct AuditRollbackState {
    job_state: String,
    job_version: i64,
    receipt_count: i64,
    admission_released_at: Option<chrono::DateTime<Utc>>,
    wait_count: i64,
    admission_count: i64,
    cancellation_audit_count: i64,
    active_lease_count: i32,
    admission_state_version: i64,
    audit_mark_count: i64,
    watermark: i64,
    credential_lease_count: i64,
}

#[async_trait::async_trait]
impl PolicyDecisionEngine for AllowCancellationPolicy {
    async fn decide(
        &self,
        context: &RequestContext,
        request: AuthorizationRequest,
    ) -> Result<PolicyDecision, ApplicationError> {
        Ok(PolicyDecision {
            id: PolicyDecisionId::new(),
            policy_id: None,
            policy_version: "embedding-cancellation-test-v1".to_owned(),
            workspace_id: context.workspace_id,
            subject_id: context.principal_id,
            capability: request.capability.clone(),
            operation: request.operation.clone(),
            resource_scope: request.resource_scope.clone(),
            result: PolicyDecisionResult::Allow,
            reason: PolicyDecisionReason::ConfiguredAllowance,
            input_state: PolicyInputState::from_request(
                context.workspace_id,
                context.principal_id,
                &request,
            ),
            matched_grant_id: None,
            decided_at: Utc::now(),
        })
    }
}

fn termination_service(runtime: &PgPool) -> EmbeddingJobTerminationService {
    EmbeddingJobTerminationService::new(
        Arc::new(PgEmbeddingJobRepository::new(PgStore::from_pool(
            runtime.clone(),
        ))),
        Arc::new(AllowCancellationPolicy),
    )
}

async fn cancel(
    runtime: &PgPool,
    fixture: &AcceptedJob,
    idempotency_key: &str,
) -> Result<vestrace_application::EmbeddingJobTerminationReceipt, ApplicationError> {
    termination_service(runtime)
        .cancel(
            fixture.context.clone(),
            fixture.job_id,
            1,
            idempotency_key.to_owned(),
        )
        .await
}

async fn runtime_pool_single(source: &PgPool) -> PgPool {
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
        .expect("single-session runtime pool must connect to the SQLx test database")
}

/// The fail-closed default names `Unavailable`, not merely some error.
///
/// This is asserted on its own because a default that refused with the wrong
/// variant would still look red while the feature was unimplemented, and would
/// then look green for the wrong reason. `Unavailable` is what tells a worker
/// "this authority is not configured here"; a `Storage` or `Internal` in its
/// place would be read as a transient fault and retried.
#[tokio::test]
async fn the_new_dispatch_methods_fail_closed_as_unavailable() {
    let repository = UnconfiguredDispatch;
    let context = context();

    let plan = repository
        .load_embedding_dispatch_plan(&context, EmbeddingJobId::new())
        .await;
    assert!(
        matches!(
            &plan,
            Err(ApplicationError::Unavailable(message))
                if message == "governed embedding dispatch plan lookup is not configured"
        ),
        "unexpected result: {plan:?}"
    );

    let recovery = repository
        .recover_embedding_job_attempt(&context, EmbeddingJobId::new(), Utc::now())
        .await;
    assert!(
        matches!(
            &recovery,
            Err(ApplicationError::Unavailable(message))
                if message == "embedding job attempt recovery is not configured"
        ),
        "unexpected result: {recovery:?}"
    );
}

/// The two new methods sit on `ProviderDispatchRepository` and nowhere else.
///
/// Step 4 of this task asks a reviewer to confirm that embedding dispatch did
/// not grow a parallel trait. That is checkable in code rather than by reading:
/// these calls compile only through `ProviderDispatchRepository`, so a later
/// move to a separate trait breaks this file.
#[tokio::test]
async fn the_embedding_methods_are_reachable_only_through_the_shared_trait() {
    let repository: &dyn ProviderDispatchRepository = &UnconfiguredDispatch;
    let context = context();
    assert!(
        repository
            .load_embedding_dispatch_plan(&context, EmbeddingJobId::new())
            .await
            .is_err()
    );
    assert!(
        repository
            .recover_embedding_job_attempt(&context, EmbeddingJobId::new(), Utc::now())
            .await
            .is_err()
    );
}

/// A job accepted under the canonical lock order is rediscovered whole.
///
/// RED until the PostgreSQL implementation exists. The worker is told nothing:
/// it names the job id and the authority returns the pinned Connection, its
/// revision, the durable effect intent, and the auth branch the snapshot fixed
/// at acceptance.
#[sqlx::test(migrations = "../../migrations")]
async fn a_governed_embedding_job_is_rediscovered_by_the_dispatch_authority(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let repository = dispatch_repository(&runtime);

    let plan: EmbeddingJobDispatchPlan = repository
        .load_embedding_dispatch_plan(&fixture.context, fixture.job_id)
        .await
        .expect("an accepted embedding job has exactly one durable dispatch plan");

    assert_eq!(plan.attempt.job_id, fixture.job_id);
    assert_eq!(plan.attempt.workspace_id, fixture.context.workspace_id);
    assert_eq!(
        plan.attempt.model_binding_snapshot_id, fixture.snapshot_id,
        "the snapshot is the one pinned at acceptance, never re-resolved"
    );
    assert_eq!(
        plan.attempt.external_effect_id.as_uuid(),
        fixture.external_effect_id
    );
    assert_eq!(plan.connection_id.as_uuid(), fixture.connection_id);
    assert_eq!(
        plan.connection_revision_id.as_uuid(),
        fixture.connection_revision_id
    );
    assert_eq!(
        plan.intent.id().as_uuid(),
        fixture.external_effect_id,
        "the plan carries the persisted intent, not a freshly built equivalent"
    );
    assert!(
        plan.credential.is_none(),
        "a no-auth snapshot pins no credential, and an absent one is the branch rather than a lookup that failed"
    );
}

/// A job that has not reached the provider resumes rather than being adopted.
///
/// RED until the PostgreSQL implementation exists. Line 219: "Neither workers
/// nor recovery automatically retry a job after `Dispatching`" -- so recovery
/// classifies, and the classification for a job whose effect is still
/// `Requested` is that it may be resumed, not that a charge may have happened.
#[sqlx::test(migrations = "../../migrations")]
async fn recovery_of_an_undispatched_embedding_job_resumes_it(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_embedding_job(&pool, &runtime).await;
    let repository = dispatch_repository(&runtime);

    let recovery = repository
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect("an accepted job classifies rather than refusing");

    assert_eq!(recovery, EmbeddingJobAttemptRecovery::ResumeReserved);
}

/// An inconsistent evidence tuple is refused, not classified.
///
/// The phase is derived, so a derivation defect would show up as a *wrong*
/// answer rather than an error, and a wrong answer here means telling a worker
/// that a provider charge is accounted for when nothing wrote it down. Every arm
/// therefore re-states the whole tuple it expects. This forces the mismatch the
/// arms exist to catch: a job whose state says the effect ended ambiguously,
/// with no dispatch transition to have been ambiguous about.
#[sqlx::test(migrations = "../../migrations")]
async fn a_job_claiming_unknown_without_a_dispatch_transition_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;

    let mut forced = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *forced)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *forced)
        .await
        .unwrap();
    sqlx::query("UPDATE embedding_jobs SET state='inconclusive_unknown' WHERE id=$1")
        .bind(fixture.job_id.as_uuid())
        .execute(&mut *forced)
        .await
        .unwrap();
    forced.commit().await.unwrap();

    let error = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&fixture.context, fixture.job_id, Utc::now())
        .await
        .expect_err("an unknown job with no dispatch transition is not recoverable");
    assert!(
        matches!(
            &error,
            ApplicationError::Conflict(message)
                if message == "the embedding job is not in a recoverable governed state"
        ),
        "unexpected error: {error:?}"
    );
}

/// A job is recoverable only by the workspace that owns it.
///
/// The guarded function compares its workspace argument against the transaction
/// GUC and refuses a mismatch, so a caller cannot recover another workspace's
/// job by naming its id. The refusal is a conflict rather than an absence: an
/// absence would tell the caller the job does not exist, which is itself an
/// answer about another workspace's data.
#[sqlx::test(migrations = "../../migrations")]
async fn a_job_cannot_be_recovered_from_another_workspace(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let intruder = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(intruder.workspace_id.as_uuid())
        .bind(format!("intruder-{}", intruder.workspace_id))
        .execute(&pool)
        .await
        .unwrap();

    let error = dispatch_repository(&runtime)
        .recover_embedding_job_attempt(&intruder, fixture.job_id, Utc::now())
        .await
        .expect_err("another workspace's job is not recoverable here");
    assert!(
        matches!(&error, ApplicationError::Conflict(_)),
        "unexpected error: {error:?}"
    );

    let plan = dispatch_repository(&runtime)
        .load_embedding_dispatch_plan(&intruder, fixture.job_id)
        .await;
    assert!(
        matches!(
            &plan,
            Err(ApplicationError::Conflict(message))
                if message == "the embedding job has no accepted attempt"
        ),
        "unexpected result: {plan:?}"
    );
}

/// The job row has no phase column, and that is the point.
///
/// An earlier draft of `EmbeddingJobState` carried `Dispatching`,
/// `ResultPrepared`, `Waiting` and `Authorized` alongside the six states spec
/// line 219 names. Those belong to the effect, the attempt's pre-dispatch phase,
/// and the result marker. Had they stayed, `embedding_jobs` would hold a second,
/// independently writable answer to whether the provider had been reached. This
/// asserts the column set that makes that impossible, so a later migration
/// cannot reintroduce it quietly.
#[sqlx::test(migrations = "../../migrations")]
async fn the_job_row_holds_no_dispatch_phase_of_its_own(pool: PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::TEXT FROM information_schema.columns \
          WHERE table_schema='public' AND table_name='embedding_jobs' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    for forbidden in [
        "phase",
        "dispatch_state",
        "dispatching_at",
        "result_prepared_at",
    ] {
        assert!(
            !columns.iter().any(|column| column == forbidden),
            "embedding_jobs must not carry `{forbidden}`: {columns:?}"
        );
    }
    assert!(columns.iter().any(|column| column == "state"));
    assert!(columns.iter().any(|column| column == "external_effect_id"));
}

/// Counts every durable trace one dispatch leaves behind.
///
/// The tuple is the point: an injected fault must leave all of these at zero,
/// and a successful dispatch must leave all of them present. Counting one row
/// and calling the transaction atomic would miss exactly the failure this suite
/// exists to catch -- a leg that committed while its siblings rolled back.
async fn dispatch_traces(pool: &PgPool, fixture: &AcceptedJob) -> (i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM provider_dispatch_causes WHERE external_effect_id=$1), \
            (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
            (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1), \
            (SELECT COUNT(*) FROM external_effect_lifecycle_transitions \
              WHERE effect_id=$1 AND status='dispatching'), \
            (SELECT COUNT(*) FROM external_effect_authorizations WHERE effect_id=$1), \
            (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$2 \
              AND action='provider.dispatch.prepared')",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn admit_embedding_without_dispatch(runtime: &PgPool, fixture: &AcceptedJob) -> Uuid {
    type AdmissionDecision = (
        String,
        Option<i32>,
        Option<Uuid>,
        Option<chrono::DateTime<Utc>>,
        Option<chrono::DateTime<Utc>>,
    );

    let lease_id = Uuid::now_v7();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let decision: AdmissionDecision = sqlx::query_as(
        "SELECT decision,retry_after_seconds,concurrency_lease_id,wait_deadline_at,dispatch_expires_at \
           FROM public.vestrace_try_admit_provider_dispatch( \
             $1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(lease_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.connection_revision_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.evidence_id)
    .bind(fixture.snapshot_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    assert_eq!(decision.0, "admitted");
    assert_eq!(decision.2, Some(lease_id));
    assert!(decision.1.is_none() && decision.3.is_none() && decision.4.is_some());
    transaction.commit().await.unwrap();
    lease_id
}

async fn authorize_embedding_effect_for_credential_lease(
    owner: &PgPool,
    fixture: &AcceptedJob,
) -> Uuid {
    let authorization_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO external_effect_authorizations(\
         id,effect_id,workspace_id,policy_version,subject_id,capability,operation,resource_scope,\
         result,reason,input_state,decided_at,payload) \
         VALUES($1,$2,$3,'embedding-credential-fixture-v1',$4,'provider.dispatch','dispatch',\
         'effect','allow','configured_allowance','{}'::jsonb,NOW(),'{}'::jsonb)",
    )
    .bind(authorization_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.context.principal_id.as_uuid())
    .execute(owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions(\
         effect_id,workspace_id,status,cause,cause_ref,recorded_at) \
         VALUES($1,$2,'authorized','authorization_recorded',$3,NOW())",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(authorization_id.to_string())
    .execute(owner)
    .await
    .unwrap();
    authorization_id
}

async fn issue_unconsumed_pinned_credential_lease(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: &AcceptedJob,
) -> Uuid {
    let credential = fixture
        .credential
        .as_ref()
        .expect("the credential-branch fixture must retain its exact pinned tuple");
    let authorization_id = authorize_embedding_effect_for_credential_lease(owner, fixture).await;
    let lease_id = Uuid::now_v7();
    let mut issue = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *issue)
        .await
        .unwrap();
    let issued: Uuid = sqlx::query_scalar(
        "SELECT vestrace_issue_credential_dispatch_lease(\
         $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(lease_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.external_effect_id)
    .bind(authorization_id)
    .bind(credential.slot_id)
    .bind(credential.revision_id)
    .bind(credential.activation_guard_id)
    .bind("api.example.test")
    .bind("bearer")
    .bind(Utc::now() + chrono::Duration::minutes(5))
    .fetch_one(&mut *issue)
    .await
    .expect("the runtime role must issue the credential lease through its guarded function");
    assert_eq!(issued, lease_id);
    issue.commit().await.unwrap();
    lease_id
}

async fn wait_for_blocker(pool: &PgPool, waiting_pid: i32, blocker_pid: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blockers: Vec<i32> = sqlx::query_scalar("SELECT unnest(pg_blocking_pids($1))")
                .bind(waiting_pid)
                .fetch_all(pool)
                .await
                .unwrap();
            if blockers.contains(&blocker_pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the competing admission never reached the observed termination lock");
}

/// An embedding job reaches the provider through the Run step's authority.
///
/// Nothing about this dispatch is embedding-specific except the cause: the same
/// `vestrace_try_admit_provider_dispatch`, the same routing lock, the same
/// admission and concurrency lease, the same `dispatching` lifecycle
/// transition. The cause row proves which owner asked, and it names the job.
#[sqlx::test(migrations = "../../migrations")]
async fn an_embedding_job_dispatches_through_the_shared_authority(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_embedding_job(&pool, &runtime).await;

    let outcome = dispatching_repository(&runtime, None)
        .prepare_dispatch(embedding_dispatch_request(&fixture))
        .await
        .expect("an accepted, evidenced embedding job is dispatchable");

    let vestrace_application::ProviderDispatchOutcome::Prepared {
        authority, request, ..
    } = outcome
    else {
        panic!("an admitted embedding dispatch must be prepared");
    };
    assert_eq!(authority.effect_id.as_uuid(), fixture.external_effect_id);
    // The reconstructed request is the embedding one. A chat request reaching
    // here would mean the shared transaction had composed the wrong wire shape
    // for this owner.
    assert!(matches!(
        request,
        vestrace_application::EffectiveModelRequest::Embeddings(_)
    ));
    let (causes, admissions, leases, dispatching, authorizations, audits) =
        dispatch_traces(&pool, &fixture).await;
    assert_eq!(
        (
            causes,
            admissions,
            leases,
            dispatching,
            authorizations,
            audits
        ),
        (1, 1, 1, 1, 1, 1),
        "every leg of one dispatch must be present"
    );

    let cause: (String, Option<Uuid>, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT cause_kind, embedding_job_id, run_id, qualification_job_id \
           FROM provider_dispatch_causes WHERE external_effect_id=$1",
    )
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        cause,
        (
            "embedding_job".to_owned(),
            Some(fixture.job_id.as_uuid()),
            None,
            None
        ),
        "the cause row must name the job and nothing else"
    );

    // The five guarded functions are the Run step's five. A sixth admission or
    // throttle function existing for embeddings is the defect this asserts
    // against, and the database is where it would be visible.
    let admission_functions: Vec<String> = sqlx::query_scalar(
        "SELECT proname::TEXT FROM pg_proc \
          WHERE proname LIKE 'vestrace_%admit%' OR proname LIKE 'vestrace_%throttle%' \
          ORDER BY proname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        admission_functions,
        [
            "vestrace_record_provider_throttle",
            "vestrace_try_admit_provider_dispatch"
        ],
        "embedding dispatch must not have grown its own admission or throttle authority"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn dispatching_first_refuses_cancellation_without_a_terminal_receipt(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_embedding_job(&pool, &runtime).await;
    dispatching_repository(&runtime, None)
        .prepare_dispatch(embedding_dispatch_request(&fixture))
        .await
        .expect("the first actor must durably enter Dispatching");

    let refusal = cancel(&runtime, &fixture, "dispatching-first-cancellation").await;
    assert!(
        matches!(
            refusal,
            Err(ApplicationError::Conflict(_)) | Err(ApplicationError::Policy(_))
        ),
        "Dispatching must refuse cancellation rather than terminalize it: {refusal:?}"
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 0, "Dispatching must leave no terminal receipt");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admitted_but_not_dispatched_embedding_job_cancels_and_releases_its_lease(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_embedding_job(&pool, &runtime).await;
    let lease_id = admit_embedding_without_dispatch(&runtime, &fixture).await;

    let receipt = cancel(&runtime, &fixture, "admitted-cancellation")
        .await
        .expect("an admitted but undispatched embedding job remains cancellable");
    assert_eq!(receipt.job_id, fixture.job_id);
    let persisted: (
        String,
        i64,
        Option<chrono::DateTime<Utc>>,
        Option<Uuid>,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT (SELECT state FROM embedding_jobs WHERE id=$1), \
                (SELECT version FROM embedding_jobs WHERE id=$1), \
                (SELECT released_at FROM provider_concurrency_leases WHERE id=$2), \
                (SELECT released_termination_id FROM provider_concurrency_leases WHERE id=$2), \
                (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$3), \
                (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$3)",
    )
    .bind(fixture.job_id.as_uuid())
    .bind(lease_id)
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted.0, "cancelled");
    assert_eq!(persisted.1, 2);
    assert!(persisted.2.is_some());
    assert_eq!(persisted.3, Some(receipt.receipt_id));
    assert_eq!((persisted.4, persisted.5), (0, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancellation_revokes_an_unconsumed_pinned_credential_lease(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job_with_pinned_credential(&pool, &runtime).await;
    let credential = fixture
        .credential
        .as_ref()
        .expect("the helper must produce a credential binding snapshot");
    let lease_id = issue_unconsumed_pinned_credential_lease(&pool, &runtime, &fixture).await;

    let before: (
        String,
        Uuid,
        Uuid,
        Uuid,
        Option<chrono::DateTime<Utc>>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT snapshot.branch,lease.credential_slot_id,lease.credential_revision_id,\
                    lease.credential_activation_guard_id,lease.consumed_at,lease.terminal_state \
               FROM model_binding_snapshots AS snapshot \
               JOIN credential_dispatch_leases AS lease \
                 ON lease.workspace_id=snapshot.workspace_id \
              WHERE snapshot.id=$1 AND lease.id=$2",
    )
    .bind(fixture.snapshot_id)
    .bind(lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before.0, "credential");
    assert_eq!(
        (before.1, before.2, before.3),
        (
            credential.slot_id,
            credential.revision_id,
            credential.activation_guard_id
        )
    );
    assert!(before.4.is_none() && before.5.is_none());

    cancel(&runtime, &fixture, "credential-lease-cancellation")
        .await
        .expect("an undispatched credential-backed embedding job remains cancellable");

    let after: (Option<chrono::DateTime<Utc>>, Option<String>) = sqlx::query_as(
        "SELECT consumed_at,terminal_state FROM credential_dispatch_leases WHERE id=$1",
    )
    .bind(lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        after.0.is_none(),
        "cancellation must not consume the credential"
    );
    assert_eq!(after.1.as_deref(), Some("revoked"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancellation_audit_failure_rolls_back_terminalization_and_lease_release(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_credential_embedding_job(&pool, &runtime).await;
    let lease_id = admit_embedding_without_dispatch(&runtime, &fixture).await;
    let credential_lease_id =
        issue_unconsumed_pinned_credential_lease(&pool, &runtime, &fixture).await;
    let baseline: (i32, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT active_lease_count FROM connection_admission_states \
                  WHERE workspace_id=$1 AND connection_id=$2), \
                (SELECT version FROM connection_admission_states \
                  WHERE workspace_id=$1 AND connection_id=$2), \
                (SELECT COUNT(*) FROM governed_mutation_audit_marks WHERE workspace_id=$1), \
                (SELECT watermark FROM installation_mutation_watermark WHERE singleton), \
                (SELECT COUNT(*) FROM credential_dispatch_leases WHERE external_effect_id=$3)",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(
        "CREATE FUNCTION public.reject_embedding_cancellation_audit() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
           IF NEW.action='embedding.job.cancelled' THEN \
             RAISE EXCEPTION 'injected cancellation audit refusal' USING ERRCODE='23514'; \
           END IF; \
           RETURN NEW; \
         END $$; \
         CREATE TRIGGER reject_embedding_cancellation_audit \
         BEFORE INSERT ON public.audit_events FOR EACH ROW \
         EXECUTE FUNCTION public.reject_embedding_cancellation_audit();",
    )
    .execute(&pool)
    .await
    .unwrap();

    let error = cancel(&runtime, &fixture, "audit-rollback-cancellation")
        .await
        .expect_err("the injected audit trigger must reject cancellation");
    assert!(
        format!("{error:?}").contains("injected cancellation audit refusal"),
        "the real termination service must surface the injected audit error: {error:?}"
    );
    let persisted: AuditRollbackState = sqlx::query_as(
        "SELECT (SELECT state FROM embedding_jobs WHERE id=$1) AS job_state, \
                (SELECT version FROM embedding_jobs WHERE id=$1) AS job_version, \
                (SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE job_id=$1) AS receipt_count, \
                (SELECT released_at FROM provider_concurrency_leases WHERE id=$2) AS admission_released_at, \
                (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$3) AS wait_count, \
                (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$3) AS admission_count, \
                (SELECT COUNT(*) FROM audit_events WHERE workspace_id=$4 AND action='embedding.job.cancelled') AS cancellation_audit_count, \
                (SELECT active_lease_count FROM connection_admission_states WHERE workspace_id=$4 AND connection_id=$5) AS active_lease_count, \
                (SELECT version FROM connection_admission_states WHERE workspace_id=$4 AND connection_id=$5) AS admission_state_version, \
                (SELECT COUNT(*) FROM governed_mutation_audit_marks WHERE workspace_id=$4) AS audit_mark_count, \
                (SELECT watermark FROM installation_mutation_watermark WHERE singleton) AS watermark, \
                (SELECT COUNT(*) FROM credential_dispatch_leases WHERE external_effect_id=$3) AS credential_lease_count",
    )
    .bind(fixture.job_id.as_uuid())
    .bind(lease_id)
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted.job_state, "requested");
    assert_eq!(persisted.job_version, 1);
    assert_eq!(persisted.receipt_count, 0);
    assert!(
        persisted.admission_released_at.is_none(),
        "the existing admission lease must remain live"
    );
    assert_eq!(
        (
            persisted.wait_count,
            persisted.admission_count,
            persisted.cancellation_audit_count
        ),
        (0, 1, 0)
    );
    assert_eq!(
        (
            persisted.active_lease_count,
            persisted.admission_state_version,
            persisted.audit_mark_count,
            persisted.watermark,
            persisted.credential_lease_count
        ),
        baseline
    );
    let credential_state: (Option<String>, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT terminal_state,consumed_at FROM credential_dispatch_leases WHERE id=$1",
    )
    .bind(credential_lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(credential_state, (None, None));
}

#[sqlx::test(migrations = "../../migrations")]
async fn termination_first_blocks_shared_admission_then_leaves_no_admission_trace(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    make_dispatchable(&pool, &runtime, &fixture).await;

    let mut termination = runtime.begin().await.unwrap();
    let termination_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *termination)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *termination)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.context.principal_id.to_string())
        .fetch_one(&mut *termination)
        .await
        .unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "SELECT receipt_id FROM public.vestrace_terminate_embedding_job_pre_dispatch( \
          $1,$2,$3,$4,1,$5,'cancelled','cancellation_authorization',$6, \
          'embedding-cancellation-test-v1','execution.write','embedding.job.cancel','workspace://','low')",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.context.principal_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind("termination-first-race")
    .bind(Uuid::now_v7())
    .fetch_one(&mut *termination)
    .await
    .expect("the first transaction must terminalize while retaining its canonical locks");

    let competing_runtime = runtime_pool(&pool).await;
    let competing_fixture = fixture.context.clone();
    let connection_id = fixture.connection_id;
    let connection_revision_id = fixture.connection_revision_id;
    let effect_id = fixture.external_effect_id;
    let evidence_id = fixture.evidence_id;
    let snapshot_id = fixture.snapshot_id;
    let (started, waiting_pid) = tokio::sync::oneshot::channel();
    let competing = tokio::spawn(async move {
        let mut transaction = competing_runtime.begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
            .bind(competing_fixture.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        started.send(pid).unwrap();
        let result = sqlx::query_as::<_, (String,)>(
            "SELECT decision FROM public.vestrace_try_admit_provider_dispatch( \
              $1,$2,$3,$4,$5,$6,$7,$8,'embedding_job',NULL,NULL,$9,NULL,NULL,NULL,60)",
        )
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(Uuid::now_v7())
        .bind(competing_fixture.workspace_id.as_uuid())
        .bind(connection_id)
        .bind(connection_revision_id)
        .bind(effect_id)
        .bind(evidence_id)
        .bind(snapshot_id)
        .fetch_one(&mut *transaction)
        .await;
        if result.is_ok() {
            transaction.commit().await.unwrap();
        } else {
            transaction.rollback().await.unwrap();
        }
        competing_runtime.close().await;
        result
    });
    let waiting_pid = waiting_pid.await.unwrap();
    wait_for_blocker(&pool, waiting_pid, termination_pid).await;
    termination.commit().await.unwrap();
    let refusal = tokio::time::timeout(Duration::from_secs(5), competing)
        .await
        .expect("the competing admission did not resume after termination committed")
        .unwrap()
        .expect_err("a terminated job must refuse the resumed admission");
    assert_eq!(
        refusal
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    let traces: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM connection_dispatch_admissions WHERE external_effect_id=$1), \
                (SELECT COUNT(*) FROM provider_admission_waits WHERE external_effect_id=$1), \
                (SELECT COUNT(*) FROM provider_concurrency_leases WHERE external_effect_id=$1)",
    )
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(traces, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn dispatching_first_blocks_cancellation_then_refuses_without_terminal_receipt(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_embedding_job(&pool, &runtime).await;
    let lease_id = admit_embedding_without_dispatch(&runtime, &fixture).await;
    let dispatch_expires_at: chrono::DateTime<Utc> = sqlx::query_scalar(
        "SELECT dispatch_expires_at FROM connection_dispatch_admissions WHERE external_effect_id=$1",
    )
    .bind(fixture.external_effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let dispatching_runtime = runtime_pool(&pool).await;
    let mut dispatching = dispatching_runtime.begin().await.unwrap();
    let dispatching_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *dispatching)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *dispatching)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_lifecycle_transitions( \
          effect_id,workspace_id,status,cause,cause_ref,recorded_at,dispatch_owner,dispatch_expires_at) \
         VALUES($1,$2,'dispatching','dispatch_started',$3,NOW(),$4,$5)",
    )
    .bind(fixture.external_effect_id)
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(lease_id.to_string())
    .bind(Uuid::now_v7())
    .bind(dispatch_expires_at)
    .execute(&mut *dispatching)
    .await
    .expect("the admitted embedding job must enter its real Dispatching transition");

    let cancellation_runtime = runtime_pool_single(&pool).await;
    let cancellation_fixture = fixture.context.clone();
    let job_id = fixture.job_id;
    let (started, waiting_pid) = tokio::sync::oneshot::channel();
    let cancellation = tokio::spawn(async move {
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&cancellation_runtime)
            .await
            .unwrap();
        started.send(pid).unwrap();
        let service = termination_service(&cancellation_runtime);
        let result = service
            .cancel(
                cancellation_fixture,
                job_id,
                1,
                "dispatching-first-blocked-cancellation".to_owned(),
            )
            .await;
        cancellation_runtime.close().await;
        result
    });
    let waiting_pid = waiting_pid.await.unwrap();
    wait_for_blocker(&pool, waiting_pid, dispatching_pid).await;
    dispatching.commit().await.unwrap();
    dispatching_runtime.close().await;
    let refusal = tokio::time::timeout(Duration::from_secs(5), cancellation)
        .await
        .expect("cancellation did not resume after Dispatching committed")
        .unwrap();
    assert!(
        matches!(
            refusal,
            Err(ApplicationError::Conflict(_)) | Err(ApplicationError::Policy(_))
        ),
        "committed Dispatching must refuse the delayed cancellation: {refusal:?}"
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM embedding_job_termination_receipts WHERE workspace_id=$1 AND job_id=$2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn preexisting_cancellation_transaction_releases_a_later_admission_at_or_after_issuance(
    pool: PgPool,
) {
    let runtime = runtime_pool(&pool).await;
    let fixture = dispatchable_embedding_job(&pool, &runtime).await;

    let cancellation_runtime = runtime_pool_single(&pool).await;
    let mut cancellation = cancellation_runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *cancellation)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.principal_id',$1,true)")
        .bind(fixture.context.principal_id.to_string())
        .fetch_one(&mut *cancellation)
        .await
        .unwrap();
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&mut *cancellation)
        .await
        .unwrap();

    let admission_runtime = runtime_pool(&pool).await;
    let lease_id = admit_embedding_without_dispatch(&admission_runtime, &fixture).await;
    admission_runtime.close().await;

    sqlx::query_scalar::<_, Uuid>(
        "SELECT receipt_id FROM public.vestrace_terminate_embedding_job_pre_dispatch( \
          $1,$2,$3,$4,1,$5,'cancelled','cancellation_authorization',$6, \
          'embedding-cancellation-test-v1','execution.write','embedding.job.cancel','workspace://','low')",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.context.principal_id.as_uuid())
    .bind(fixture.job_id.as_uuid())
    .bind("older-transaction-later-admission")
    .bind(Uuid::now_v7())
    .fetch_one(&mut *cancellation)
    .await
    .expect("the older transaction must see and release the committed admission");
    cancellation.commit().await.unwrap();
    cancellation_runtime.close().await;

    let timestamps: (chrono::DateTime<Utc>, chrono::DateTime<Utc>) =
        sqlx::query_as("SELECT issued_at,released_at FROM provider_concurrency_leases WHERE id=$1")
            .bind(lease_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        timestamps.1 >= timestamps.0,
        "termination must use a post-lock timestamp: issued_at={:?}, released_at={:?}",
        timestamps.0,
        timestamps.1
    );
}

/// Every leg commits or rolls back together.
///
/// A boundary is injected at each named point of one dispatch and the whole
/// trace tuple is required to be empty afterwards. A partial commit here is the
/// worst outcome this system can produce: an admission or a `dispatching`
/// transition that survives without its siblings is a possible provider charge
/// nobody can account for.
#[sqlx::test(migrations = "../../migrations")]
async fn injected_write_boundaries_roll_every_embedding_dispatch_leg_back(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    for point in [
        ProviderDispatchFaultPoint::BeforeAdmission,
        ProviderDispatchFaultPoint::AfterAdmission,
        ProviderDispatchFaultPoint::BeforeIntent,
        ProviderDispatchFaultPoint::AfterIntent,
        ProviderDispatchFaultPoint::BeforeAuthorization,
        ProviderDispatchFaultPoint::AfterAuthorization,
        ProviderDispatchFaultPoint::BeforeDispatching,
        ProviderDispatchFaultPoint::AfterDispatching,
        ProviderDispatchFaultPoint::BeforeGovernedCommit,
    ] {
        let fixture = dispatchable_embedding_job(&pool, &runtime).await;

        let error = dispatching_repository(&runtime, Some(point))
            .prepare_dispatch(embedding_dispatch_request(&fixture))
            .await
            .err()
            .unwrap_or_else(|| panic!("{point:?} did not fail the dispatch"));
        assert!(
            matches!(&error, ApplicationError::Internal(message)
                if message.contains(&format!("{point:?}"))),
            "{point:?} produced an unexpected error: {error:?}"
        );

        let traces = dispatch_traces(&pool, &fixture).await;
        assert_eq!(
            traces,
            (0, 0, 0, 0, 0, 0),
            "{point:?} left a committed dispatch leg behind"
        );
    }
}

/// The model-data-policy leg is not on this path, and that is deliberate.
///
/// `BeforePolicyRecord` and `AfterPolicyRecord` fire only inside
/// `if let Some(record) = evaluation.model_data_policy`, and an embedding job
/// supplies `None`: its disclosure decision belongs to `EmbeddingDataPolicyGate`
/// and is taken before acceptance, while the model data policy record names a
/// run and a step it does not have.
///
/// This is asserted rather than left implicit because the atomicity test above
/// cannot cover a leg that never runs, and a future change that started
/// recording a model data policy for embeddings would otherwise add an
/// uncovered write to the dispatch transaction in silence.
#[sqlx::test(migrations = "../../migrations")]
async fn the_model_data_policy_leg_is_absent_from_an_embedding_dispatch(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let mut workspaces = Vec::new();

    // Each iteration dispatches for real, so every point needs its own job:
    // reusing one would replay the same effect and conflict for a reason that
    // has nothing to do with the property under test.
    for point in [
        ProviderDispatchFaultPoint::BeforePolicyRecord,
        ProviderDispatchFaultPoint::AfterPolicyRecord,
    ] {
        let fixture = accept_embedding_job(&pool, &runtime).await;
        make_dispatchable(&pool, &runtime, &fixture).await;
        assert!(
            dispatching_repository(&runtime, Some(point))
                .prepare_dispatch(embedding_dispatch_request(&fixture))
                .await
                .is_ok(),
            "{point:?} is on the embedding dispatch path; the atomicity test must cover it"
        );
        workspaces.push(fixture.context.workspace_id.as_uuid());
    }
    assert_eq!(workspaces.len(), 2);

    // The table is keyed by run and step and has no workspace column at all,
    // which is the same fact from the schema's side: a model data policy
    // decision is structurally a Run-step artifact. The per-test database is
    // fresh, so any row here would be one these dispatches wrote.
    let decisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_data_policy_decisions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        decisions, 0,
        "an embedding dispatch must record no model data policy decision"
    );
}

/// A dispatch may not name a snapshot the job did not pin.
///
/// Line 219: an ordinary job "owns its snapshot resolved from the current tuple
/// at acceptance and never changed thereafter". A worker that could pass a
/// different snapshot would be routing, and the refusal has to come from the
/// database rather than from the caller's own care.
#[sqlx::test(migrations = "../../migrations")]
async fn a_dispatch_naming_another_snapshot_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    make_dispatchable(&pool, &runtime, &fixture).await;

    let mut request = embedding_dispatch_request(&fixture);
    request.cause = ProviderDispatchCause::EmbeddingJob {
        job_id: fixture.job_id,
        snapshot_id: Uuid::now_v7(),
    };
    let error = dispatching_repository(&runtime, None)
        .prepare_dispatch(request)
        .await
        .err()
        .expect("a snapshot the job never pinned must be refused");
    assert!(
        matches!(
            &error,
            ApplicationError::Conflict(_) | ApplicationError::Policy(_)
        ),
        "unexpected error: {error:?}"
    );
    assert_eq!(dispatch_traces(&pool, &fixture).await, (0, 0, 0, 0, 0, 0));
}

/// Acceptance writes the job and the effect it owns, or neither.
///
/// The effect intent has to be durable before the job, because every later
/// dispatch converges on that exact effect id. Both are inside one governed
/// mutation, so a job without its effect -- which no dispatch could ever find --
/// is not a state this path can produce.
#[sqlx::test(migrations = "../../migrations")]
async fn acceptance_writes_the_job_and_its_effect_together(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let job_id = EmbeddingJobId::new();
    let intent = embedding_intent(&fixture, job_id);
    let effect_id = intent.id().as_uuid();

    repository
        .accept_governed(
            fixture.context.clone(),
            accept_command(&fixture, job_id, intent),
        )
        .await
        .expect("a second job in an authorized space is acceptable");

    let (jobs, intents): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE id=$2)",
    )
    .bind(job_id.as_uuid())
    .bind(effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((jobs, intents), (1, 1));

    let stored: (String, String, Uuid, Option<Uuid>) = sqlx::query_as(
        "SELECT kind, state, external_effect_id, retries_unknown_embedding_job_id \
           FROM embedding_jobs WHERE id=$1",
    )
    .bind(job_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        (
            "delivery".to_owned(),
            "requested".to_owned(),
            effect_id,
            None
        ),
        "an accepted ordinary job starts Requested and retries nothing"
    );
}

/// A job for a space nobody authorized is refused, and leaves no effect behind.
///
/// The space registration is the `(workspace, EmbeddingSpaceKey)` guard, and
/// acceptance takes it in the canonical lock order. Refusing here is what stops
/// the lazy path this package replaces: a space that came into existence
/// because something needed to write a vector into it.
#[sqlx::test(migrations = "../../migrations")]
async fn acceptance_into_an_unauthorized_space_leaves_nothing(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let job_id = EmbeddingJobId::new();
    let intent = embedding_intent(&fixture, job_id);
    let effect_id = intent.id().as_uuid();
    let mut command = accept_command(&fixture, job_id, intent);
    command.space_registration_id = EmbeddingSpaceId::new();

    let error = repository
        .accept_governed(fixture.context.clone(), command)
        .await
        .expect_err("an unregistered space accepts no job");
    assert!(
        matches!(
            &error,
            ApplicationError::Policy(message) if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
        ),
        "unexpected error: {error:?}"
    );

    let (jobs, intents, audits): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM embedding_jobs WHERE id=$1), \
                (SELECT COUNT(*) FROM external_effect_intents WHERE id=$2), \
                (SELECT COUNT(*) FROM audit_events WHERE action='embedding.job.accepted')",
    )
    .bind(job_id.as_uuid())
    .bind(effect_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (jobs, intents, audits),
        (0, 0, 0),
        "a refused acceptance leaves no job, no effect and no audit"
    );
}

/// A successor may only be created against a terminal ambiguity head.
///
/// Line 257 makes `InconclusiveUnknown` the sole state from which the authorized
/// duplicate-charge acknowledgement may create a successor. The check is in the
/// guarded function rather than in the caller, because the caller that wanted to
/// retry a running job is exactly the caller that would skip it.
#[sqlx::test(migrations = "../../migrations")]
async fn a_successor_of_a_live_predecessor_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    let repository = PgEmbeddingJobRepository::new(PgStore::from_pool(runtime.clone()));
    let job_id = EmbeddingJobId::new();
    let mut command = accept_command(&fixture, job_id, embedding_intent(&fixture, job_id));
    command.retries_unknown_embedding_job_id = Some(fixture.job_id);
    command.expected_predecessor_version = Some(1);

    let error = repository
        .accept_governed(fixture.context.clone(), command)
        .await
        .expect_err("a Requested predecessor is not an ambiguity head");
    assert!(
        matches!(
            &error,
            ApplicationError::Policy(message) if message == "EMBEDDING_JOB_ACCEPTANCE_REFUSED"
        ),
        "unexpected error: {error:?}"
    );

    // The predecessor is untouched: line 257 requires an acknowledgement to
    // leave it exactly as it was.
    let predecessor_state: String =
        sqlx::query_scalar("SELECT state FROM embedding_jobs WHERE id=$1")
            .bind(fixture.job_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(predecessor_state, "requested");
}

/// The unconfigured acceptance authority refuses rather than reporting success.
#[tokio::test]
async fn unconfigured_acceptance_is_unavailable() {
    struct Unconfigured;
    impl EmbeddingJobRepository for Unconfigured {}

    let error = Unconfigured
        .accept_governed(context(), unreachable_command())
        .await
        .expect_err("an unconfigured authority accepts nothing");
    assert!(
        matches!(
            &error,
            ApplicationError::Unavailable(message)
                if message == "governed embedding job acceptance is not configured"
        ),
        "unexpected error: {error:?}"
    );
}

fn accept_command(
    fixture: &AcceptedJob,
    job_id: EmbeddingJobId,
    intent: ExternalEffectIntent,
) -> AcceptEmbeddingJob {
    let at = Utc::now();
    AcceptEmbeddingJob {
        job_id,
        space_registration_id: EmbeddingSpaceId::from_uuid(fixture.space_registration_id),
        kind: EmbeddingJobKind::Delivery,
        model_binding_snapshot_id: fixture.snapshot_id,
        intent,
        model_request_evidence_id: ModelRequestEvidenceId::new(),
        retries_unknown_embedding_job_id: None,
        expected_predecessor_version: None,
        idempotency: None,
        outbox: Vec::new(),
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "embedding.job.accepted",
            "embedding_job",
            job_id.as_uuid(),
            serde_json::json!({"embedding_job_id": job_id}),
            at,
        )
        .unwrap(),
    }
}

/// The command the unconfigured-authority test never gets to use.
fn unreachable_command() -> AcceptEmbeddingJob {
    let context = context();
    let at = Utc::now();
    let job_id = EmbeddingJobId::new();
    AcceptEmbeddingJob {
        job_id,
        space_registration_id: EmbeddingSpaceId::new(),
        kind: EmbeddingJobKind::RetrievalQuery,
        model_binding_snapshot_id: Uuid::now_v7(),
        intent: workspace_scoped_intent(&context, Uuid::now_v7()),
        model_request_evidence_id: ModelRequestEvidenceId::new(),
        retries_unknown_embedding_job_id: None,
        expected_predecessor_version: None,
        idempotency: None,
        outbox: Vec::new(),
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "embedding.job.accepted",
            "embedding_job",
            job_id.as_uuid(),
            serde_json::json!({}),
            at,
        )
        .unwrap(),
    }
}
