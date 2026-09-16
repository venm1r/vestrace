//! Database-backed tests for the capability grant store and the engine that
//! reads it.
//!
//! # Why these matter more than the domain cases
//!
//! CAP-002..CAP-014 verify the domain kernel, and did so while nothing could
//! issue a grant. These are about the half that was missing: that a grant
//! survives, that authority is scoped to the subject it was issued to, and that
//! revocation reaches the *next decision* rather than the next restart.
//!
//! `sqlx::test` connects as the database owner, so row level security is not
//! exercised here; the workspace separation shown rests on the query predicate.

use sqlx::PgPool;
use std::sync::Arc;
use vestrace_application::{
    CapabilityGrantRepository, PolicyDecisionEngine, RequestContext, StoredGrantPolicyEngine,
};
use vestrace_domain::security::{
    AuthorizationRequest, BudgetConstraint, CapabilityGrant, CapabilityGrantSpec,
    PolicyDecisionReason,
};
use vestrace_domain::{
    Capability, CapabilityGrantId, PrincipalId, RiskCategory, WorkspaceId, time::now,
};
use vestrace_infrastructure::{PgCapabilityGrantRepository, PgStore};

const OPERATION: &str = "memory.write";
const SCOPE: &str = "memory://project-alpha";

async fn seed(pool: &PgPool) -> (RequestContext, PrincipalId) {
    let workspace_id = WorkspaceId::new();
    let subject = PrincipalId::new();

    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("ws-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .expect("workspace");
    for (id, identifier) in [(subject, "subject")] {
        sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
            .bind(id.as_uuid())
            .bind(workspace_id.as_uuid())
            .bind(identifier)
            .execute(pool)
            .await
            .expect("principal");
    }

    (RequestContext::new(workspace_id, subject), subject)
}

fn grant_for(context: &RequestContext, subject: PrincipalId) -> CapabilityGrant {
    let at = now();
    CapabilityGrant::issue(
        CapabilityGrantSpec {
            id: CapabilityGrantId::new(),
            workspace_id: context.workspace_id,
            subject_id: subject,
            issuer_id: context.principal_id,
            capability: Capability::MemoryWrite,
            operation: OPERATION.to_string(),
            resource_scope: SCOPE.to_string(),
            valid_from: at,
            valid_until: None,
            budget: Some(BudgetConstraint::new(100)),
            risk_ceiling: RiskCategory::Medium,
            conditions: Vec::new(),
        },
        at,
    )
    .expect("a well-formed grant")
}

fn request() -> AuthorizationRequest {
    AuthorizationRequest::new(Capability::MemoryWrite, OPERATION, SCOPE, RiskCategory::Low)
}

fn engine(pool: &PgPool) -> StoredGrantPolicyEngine {
    StoredGrantPolicyEngine::new(
        Arc::new(PgCapabilityGrantRepository::new(PgStore::from_pool(
            pool.clone(),
        ))),
        "policy-v1",
    )
    .expect("a well-formed engine")
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_stored_grant_authorizes_and_a_missing_one_denies(pool: PgPool) {
    let (context, subject) = seed(&pool).await;
    let engine = engine(&pool);

    // Before anything is issued: denied, and denied as a default rather than as
    // some near-miss.
    let denied = engine
        .decide(&context, request())
        .await
        .expect("a decision with no grants");
    assert!(!denied.is_allowed());
    assert_eq!(denied.reason, PolicyDecisionReason::DefaultDeny);

    let repository = PgCapabilityGrantRepository::new(PgStore::from_pool(pool.clone()));
    let grant = grant_for(&context, subject);
    repository
        .insert(&context, &grant)
        .await
        .expect("issuing a grant");

    let allowed = engine
        .decide(&context, request())
        .await
        .expect("a decision with a grant");
    assert!(allowed.is_allowed());
    assert_eq!(allowed.reason, PolicyDecisionReason::GrantMatched);
    assert_eq!(
        allowed.matched_grant_id,
        Some(grant.id),
        "the decision did not name the stored grant that permitted it"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn revocation_reaches_the_next_decision(pool: PgPool) {
    // CAP-010 in the deployment rather than in the domain: no restart, no new
    // grant cycle, no cache to invalidate.
    let (context, subject) = seed(&pool).await;
    let engine = engine(&pool);
    let repository = PgCapabilityGrantRepository::new(PgStore::from_pool(pool.clone()));

    let mut grant = grant_for(&context, subject);
    repository
        .insert(&context, &grant)
        .await
        .expect("issuing a grant");
    assert!(
        engine
            .decide(&context, request())
            .await
            .expect("decision")
            .is_allowed()
    );

    grant.revoke(now()).expect("revoking an active grant");
    repository
        .save_revocation(&context, &grant)
        .await
        .expect("persisting the revocation");

    let after = engine.decide(&context, request()).await.expect("decision");
    assert!(
        !after.is_allowed(),
        "a revoked grant still authorized the next request"
    );
    assert_eq!(after.reason, PolicyDecisionReason::DefaultDeny);

    // A second revocation must not move the recorded time.
    let mut again = grant.clone();
    assert!(
        repository.save_revocation(&context, &again).await.is_err(),
        "an already-revoked grant was revoked again in storage"
    );
    assert!(again.revoke(now()).is_err());
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_grant_issued_to_someone_else_does_not_authorize_me(pool: PgPool) {
    // The subject is the whole difference between this and the configured static
    // list, which gave every principal the same authority.
    let (context, _subject) = seed(&pool).await;
    let repository = PgCapabilityGrantRepository::new(PgStore::from_pool(pool.clone()));

    let other = PrincipalId::new();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(other.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind("somebody-else")
        .execute(&pool)
        .await
        .expect("principal");

    repository
        .insert(&context, &grant_for(&context, other))
        .await
        .expect("issuing a grant to somebody else");

    let decision = engine(&pool)
        .decide(&context, request())
        .await
        .expect("decision");
    assert!(
        !decision.is_allowed(),
        "a grant issued to another principal authorized this one"
    );

    // And the loaded set is genuinely subject-scoped, not filtered afterwards.
    assert!(
        repository
            .active_for_subject(&context, context.principal_id)
            .await
            .expect("lookup")
            .is_empty()
    );
    assert_eq!(
        repository
            .active_for_subject(&context, other)
            .await
            .expect("lookup")
            .len(),
        1
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn a_grant_does_not_cross_a_workspace_boundary(pool: PgPool) {
    let (mine, subject) = seed(&pool).await;
    let (theirs, _) = seed(&pool).await;
    let repository = PgCapabilityGrantRepository::new(PgStore::from_pool(pool.clone()));

    repository
        .insert(&mine, &grant_for(&mine, subject))
        .await
        .expect("issuing a grant");

    assert_eq!(repository.list(&mine).await.expect("mine").len(), 1);
    assert!(
        repository.list(&theirs).await.expect("theirs").is_empty(),
        "another workspace could read this workspace's grants"
    );

    // A grant claiming a different workspace than the request context is
    // refused by name rather than written into the wrong tenant.
    let foreign = grant_for(&mine, subject);
    assert!(repository.insert(&theirs, &foreign).await.is_err());
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn an_expired_grant_stops_authorizing_without_being_revoked(pool: PgPool) {
    use chrono::Duration;

    let (context, subject) = seed(&pool).await;
    let repository = PgCapabilityGrantRepository::new(PgStore::from_pool(pool.clone()));
    let at = now();

    let expiring = CapabilityGrant::issue(
        CapabilityGrantSpec {
            id: CapabilityGrantId::new(),
            workspace_id: context.workspace_id,
            subject_id: subject,
            issuer_id: context.principal_id,
            capability: Capability::MemoryWrite,
            operation: OPERATION.to_string(),
            resource_scope: SCOPE.to_string(),
            valid_from: at - Duration::hours(2),
            valid_until: Some(at - Duration::hours(1)),
            budget: Some(BudgetConstraint::new(10)),
            risk_ceiling: RiskCategory::Medium,
            conditions: Vec::new(),
        },
        at - Duration::hours(2),
    )
    .expect("a grant that has already expired");

    repository
        .insert(&context, &expiring)
        .await
        .expect("issuing an expired grant");

    // It is still `active` in the store — expiry is not revocation, and the row
    // records what was issued rather than a status somebody has to sweep.
    assert_eq!(
        repository
            .active_for_subject(&context, subject)
            .await
            .expect("lookup")
            .len(),
        1
    );

    let decision = engine(&pool)
        .decide(&context, request())
        .await
        .expect("decision");
    assert!(
        !decision.is_allowed(),
        "an expired grant authorized the request"
    );
    assert_eq!(decision.reason, PolicyDecisionReason::Expired);
}
