//! One dispatch authority, two callers.
//!
//! An embedding job reaches the provider through the same five guarded
//! functions P03's Run-step dispatch calls -- `vestrace_try_admit_provider_
//! dispatch`, `vestrace_issue_credential_dispatch_lease`,
//! `vestrace_consume_credential_dispatch_lease`,
//! `vestrace_record_provider_throttle` and `vestrace_release_provider_dispatch`.
//! A second admission, throttle, lease or recovery function existing for
//! embeddings would be the defect this suite is here to make visible.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    AcceptEmbeddingJob, ApplicationError, EmbeddingJobAttemptRecovery, EmbeddingJobDispatchPlan,
    EmbeddingJobRepository, ProviderDispatchCause, ProviderDispatchFaultPoint,
    ProviderDispatchRepository, RequestContext,
};
use vestrace_domain::{
    AuditEvent, EmbeddingJobId, EmbeddingSpaceId, ExternalEffectIntent, ModelRequestEvidenceId,
    PrincipalId, WorkspaceId, embedding::EmbeddingJobKind, id::AuditEventId,
};
use vestrace_infrastructure::{PgEmbeddingJobRepository, PgStore};

#[path = "common/mod.rs"]
mod common;

use common::*;

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
    let fixture = accept_embedding_job(&pool, &runtime).await;
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

/// An embedding job reaches the provider through the Run step's authority.
///
/// Nothing about this dispatch is embedding-specific except the cause: the same
/// `vestrace_try_admit_provider_dispatch`, the same routing lock, the same
/// admission and concurrency lease, the same `dispatching` lifecycle
/// transition. The cause row proves which owner asked, and it names the job.
#[sqlx::test(migrations = "../../migrations")]
async fn an_embedding_job_dispatches_through_the_shared_authority(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let fixture = accept_embedding_job(&pool, &runtime).await;
    make_dispatchable(&pool, &runtime, &fixture).await;

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
        let fixture = accept_embedding_job(&pool, &runtime).await;
        make_dispatchable(&pool, &runtime, &fixture).await;

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
