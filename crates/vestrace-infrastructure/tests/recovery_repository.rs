use chrono::{Duration, TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{ApplicationError, RecoveryRepository};
use vestrace_domain::trust::{
    ContainmentAction, ContainmentActionKind, Incident, IncidentSeverity, IncidentType,
    IntegrityStatus, RecoveryPoint, RevalidationCheck, RevalidationLevel, RevalidationResult,
    RevalidationRun, TrustStateRecord,
};
use vestrace_domain::{HealthFindingId, HealthScope, PrincipalId, WorkspaceId, now};
use vestrace_infrastructure::{PgRecoveryRepository, PgStore};

fn workspace_scope(workspace_id: WorkspaceId) -> HealthScope {
    HealthScope::workspace(workspace_id)
}

fn incident(scope: HealthScope, at: chrono::DateTime<Utc>) -> Incident {
    let mut incident = Incident::open(
        IncidentType::RecoveryRequired,
        IncidentSeverity::High,
        scope,
        vec![HealthFindingId::new()],
        vec!["run:recovery".into()],
        vec!["evidence:incident".into()],
        at,
    )
    .unwrap();
    incident
        .begin_containment(
            ContainmentAction::new(
                ContainmentActionKind::PauseWorkers,
                "run:recovery",
                PrincipalId::new(),
                "contain recovery target",
                "policy-v1",
                at,
            )
            .unwrap(),
        )
        .unwrap();
    incident.mark_contained(at).unwrap();
    incident
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn recovery_repository_round_trips_and_retries_control_records(pool: PgPool) {
    let repository = PgRecoveryRepository::new(PgStore::from_pool(pool));
    let workspace_id = WorkspaceId::new();
    let scope = workspace_scope(workspace_id);
    let at = Utc.with_ymd_and_hms(2026, 8, 12, 1, 0, 0).single().unwrap();
    let incident = incident(scope.clone(), at);
    let revalidation = RevalidationRun::complete(
        Some(incident.id()),
        scope.clone(),
        RevalidationLevel::Workspace,
        "state:workspace:recovered",
        vec![RevalidationCheck::passing(
            "canonical-history",
            vec!["evidence:history".into()],
        )],
        vec!["evidence:revalidation".into()],
        RevalidationResult::Passed,
        at + Duration::seconds(1),
    )
    .unwrap();
    let mut trust =
        TrustStateRecord::from_incident(scope.clone(), incident.id(), IncidentSeverity::High, at);
    trust.begin_revalidation(revalidation.id(), at).unwrap();
    trust.apply_revalidation(&revalidation).unwrap();
    let recovery_point = RecoveryPoint::new(
        "state:workspace:recovered",
        42,
        "workspace",
        IntegrityStatus::Valid,
        "checkpoint:42",
        at,
    )
    .unwrap();

    repository.insert_incident(&incident).await.unwrap();
    repository.insert_incident(&incident).await.unwrap();
    repository
        .insert_revalidation_run(&revalidation)
        .await
        .unwrap();
    repository.insert_trust_state(&trust).await.unwrap();
    repository
        .insert_recovery_point(&recovery_point)
        .await
        .unwrap();

    assert_eq!(
        repository.find_incident(incident.id()).await.unwrap(),
        Some(incident)
    );
    assert_eq!(
        repository
            .find_revalidation_run(revalidation.id())
            .await
            .unwrap(),
        Some(revalidation)
    );
    assert_eq!(
        repository.find_latest_trust_state(&scope).await.unwrap(),
        Some(trust)
    );
    assert_eq!(
        repository
            .find_recovery_point(recovery_point.id())
            .await
            .unwrap(),
        Some(recovery_point)
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn recovery_repository_rejects_conflicting_reuse_of_immutable_ids(pool: PgPool) {
    let repository = PgRecoveryRepository::new(PgStore::from_pool(pool));
    let at = now();
    let first = incident(HealthScope::workspace(WorkspaceId::new()), at);
    let mut payload = serde_json::to_value(&first).unwrap();
    payload["status"] = serde_json::json!("closed");
    let conflicting: Incident = serde_json::from_value(payload).unwrap();

    repository.insert_incident(&first).await.unwrap();
    let outcome = repository.insert_incident(&conflicting).await;
    assert!(
        matches!(outcome, Err(ApplicationError::Conflict(_))),
        "unexpected outcome: {outcome:?}"
    );
}

/// `now()` carries nanoseconds; a PostgreSQL `TIMESTAMPTZ` column carries
/// microseconds. The indexed column is a truncated index of the payload value,
/// so read-back must compare it at the column's precision instead of rejecting
/// every record written with a real clock reading.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn recovery_records_survive_sub_microsecond_timestamps(pool: PgPool) {
    let repository = PgRecoveryRepository::new(PgStore::from_pool(pool));
    let at = now() + Duration::nanoseconds(1);
    let incident = incident(HealthScope::workspace(WorkspaceId::new()), at);

    repository.insert_incident(&incident).await.unwrap();

    let found = repository.find_incident(incident.id()).await.unwrap();
    assert_eq!(found, Some(incident));
}
