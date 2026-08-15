//! Database-backed tests for durable health findings.
//!
//! # What these are for
//!
//! Before findings were stored, every run built a new one: `occurrence_count`
//! was always zero, recurrence and flapping had nothing to assess, and a
//! disposition had nothing to attach to. The properties below are the ones that
//! only exist once a finding survives the run that produced it, so none of them
//! could have been written against the in-memory path.
//!
//! `sqlx::test` connects as the database owner, so row level security is not
//! exercised here; the workspace separation shown is the query's own predicate.

use sqlx::PgPool;
use vestrace_application::{
    HealthFindingRepository, HealthInspectionService, HealthMonitorService, InvariantObservation,
    InvariantObserver, RequestContext, standard_invariants,
};
use vestrace_domain::health::{FindingLifecycleStatus, HealthScope, HealthState};
use vestrace_domain::{PrincipalId, WorkspaceId, time::now};
use vestrace_infrastructure::{PgHealthFindingRepository, PgStore};

use std::sync::Arc;
use std::sync::Mutex;

/// An observer that reports whatever the test tells it to.
struct ScriptedObserver {
    observations: Mutex<Vec<Vec<InvariantObservation>>>,
}

impl ScriptedObserver {
    fn new(runs: Vec<Vec<InvariantObservation>>) -> Self {
        Self {
            observations: Mutex::new(runs.into_iter().rev().collect()),
        }
    }
}

#[async_trait::async_trait]
impl InvariantObserver for ScriptedObserver {
    async fn observe(
        &self,
        _context: &RequestContext,
    ) -> Result<Vec<InvariantObservation>, vestrace_application::ApplicationError> {
        Ok(self
            .observations
            .lock()
            .expect("observer script")
            .pop()
            .unwrap_or_default())
    }
}

async fn seed_workspace(pool: &PgPool) -> RequestContext {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();

    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("ws-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .expect("workspace");
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind("tester")
        .execute(pool)
        .await
        .expect("principal");

    RequestContext::new(workspace_id, principal_id)
}

fn observation(context: &RequestContext, state: HealthState, detail: &str) -> InvariantObservation {
    InvariantObservation {
        invariant_id: "outbox.backlog_within_budget".to_string(),
        scope: HealthScope::workspace(context.workspace_id),
        fingerprint: format!("outbox.backlog_within_budget:{}", context.workspace_id),
        state,
        detail: detail.to_string(),
        evidence_refs: vec!["table:outbox".to_string()],
    }
}

fn monitor(pool: &PgPool, runs: Vec<Vec<InvariantObservation>>) -> HealthMonitorService {
    HealthMonitorService::new(
        Arc::new(ScriptedObserver::new(runs)),
        Arc::new(PgHealthFindingRepository::new(PgStore::from_pool(
            pool.clone(),
        ))),
        HealthInspectionService::new(standard_invariants()),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_same_problem_seen_twice_is_one_finding_with_two_occurrences(pool: PgPool) {
    // The property the whole slice exists for. Two runs observing the same
    // violation used to produce two findings, each with a count of zero.
    let context = seed_workspace(&pool).await;
    let monitor = monitor(
        &pool,
        vec![
            vec![observation(&context, HealthState::Degraded, "7 pending")],
            vec![observation(&context, HealthState::Degraded, "9 pending")],
        ],
    );

    let first = monitor.inspect(&context, now()).await.expect("first run");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].finding.occurrence_count(), 1);

    let second = monitor.inspect(&context, now()).await.expect("second run");
    assert_eq!(
        second.len(),
        1,
        "a second observation of the same violation produced {} findings",
        second.len()
    );
    assert_eq!(
        second[0].finding.occurrence_count(),
        2,
        "the finding did not accumulate its second occurrence"
    );
    assert_eq!(
        second[0].finding.id(),
        first[0].finding.id(),
        "the second run replaced the finding rather than adding to it"
    );
    assert_eq!(second[0].detail, "9 pending");

    let repository = PgHealthFindingRepository::new(PgStore::from_pool(pool.clone()));
    let occurrences = repository
        .occurrences(&context, second[0].finding.id())
        .await
        .expect("occurrences");
    assert_eq!(occurrences.len(), 2);
    assert_eq!(occurrences[0].sequence(), 1);
    assert_eq!(occurrences[1].sequence(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_finding_not_observed_again_stays_open(pool: PgPool) {
    // Resolving a finding by forgetting it is the failure mode a stateless
    // check cannot even have. A problem stops being a problem when something
    // verifies it away, not when a run happens not to look.
    let context = seed_workspace(&pool).await;
    let monitor = monitor(
        &pool,
        vec![
            vec![observation(&context, HealthState::Degraded, "7 pending")],
            Vec::new(),
        ],
    );

    monitor.inspect(&context, now()).await.expect("first run");
    let second = monitor.inspect(&context, now()).await.expect("second run");

    assert_eq!(second.len(), 1, "the finding disappeared when unobserved");
    assert!(!second[0].observed_on_this_run);
    assert_eq!(second[0].status, FindingLifecycleStatus::Open);
    assert_eq!(
        second[0].finding.occurrence_count(),
        1,
        "a run that did not observe the problem still counted an occurrence"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn findings_do_not_cross_a_workspace_boundary(pool: PgPool) {
    let mine = seed_workspace(&pool).await;
    let theirs = seed_workspace(&pool).await;

    let monitor = monitor(
        &pool,
        vec![vec![observation(&mine, HealthState::Degraded, "7 pending")]],
    );
    monitor.inspect(&mine, now()).await.expect("run");

    let repository = PgHealthFindingRepository::new(PgStore::from_pool(pool.clone()));
    assert_eq!(repository.list(&mine).await.expect("mine").len(), 1);
    assert!(
        repository.list(&theirs).await.expect("theirs").is_empty(),
        "another workspace could read this workspace's findings"
    );
    assert!(
        repository
            .find_by_fingerprint(
                &theirs,
                "outbox.backlog_within_budget",
                &format!("outbox.backlog_within_budget:{}", mine.workspace_id)
            )
            .await
            .expect("lookup")
            .is_none()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_oscillating_finding_records_its_transitions(pool: PgPool) {
    // Flapping detection reads occurrences. Until they were stored there was
    // nothing for `assess_recurrence` to read, so HLT-009 held in the domain
    // and could not hold in the deployment.
    let context = seed_workspace(&pool).await;
    let monitor = monitor(
        &pool,
        vec![
            vec![observation(&context, HealthState::Unhealthy, "bad")],
            vec![observation(&context, HealthState::Degraded, "better")],
            vec![observation(&context, HealthState::Unhealthy, "bad again")],
            vec![observation(&context, HealthState::Degraded, "better again")],
        ],
    );

    for _ in 0..4 {
        monitor.inspect(&context, now()).await.expect("run");
    }

    let repository = PgHealthFindingRepository::new(PgStore::from_pool(pool.clone()));
    let findings = repository.list(&context).await.expect("findings");
    assert_eq!(findings.len(), 1);

    let occurrences = repository
        .occurrences(&context, findings[0].id())
        .await
        .expect("occurrences");
    assert_eq!(occurrences.len(), 4);

    let assessment = vestrace_domain::health::assess_recurrence(&occurrences, 4);
    assert!(assessment.is_recurrent());
    assert!(
        assessment.is_flapping(),
        "an oscillating history was not detected as flapping: {} transitions",
        assessment.state_transitions()
    );
}
