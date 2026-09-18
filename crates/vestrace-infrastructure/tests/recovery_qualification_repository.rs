use sqlx::PgPool;
use vestrace_application::{
    RecoveryQualificationEvidence, RecoveryQualificationEvidenceRepository, StartupRecoveryOutcome,
    StartupRecoveryRecord,
};
use vestrace_domain::{
    AgentRunId, RecoveryClassification, RecoveryTarget, evaluate_recovery_qualification, now,
};
use vestrace_infrastructure::{PgRecoveryQualificationEvidenceRepository, PgStore};

fn observation(
    target: RecoveryTarget,
    classification: RecoveryClassification,
    outcome: StartupRecoveryOutcome,
) -> RecoveryQualificationEvidence {
    RecoveryQualificationEvidence::from_recovery_record(
        &StartupRecoveryRecord {
            run_id: AgentRunId::new(),
            target,
            classification,
            outcome,
        },
        now(),
    )
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn recovery_qualification_observations_round_trip_without_deduplication(pool: PgPool) {
    let repository = PgRecoveryQualificationEvidenceRepository::new(PgStore::from_pool(pool));
    let first = observation(
        RecoveryTarget::RunningExecution,
        RecoveryClassification::SafeToResume,
        StartupRecoveryOutcome::Restored,
    );
    let second = observation(
        RecoveryTarget::RunningExecution,
        RecoveryClassification::SafeToResume,
        StartupRecoveryOutcome::Aborted,
    );

    repository.insert(&first).await.unwrap();
    repository.insert(&second).await.unwrap();

    let stored = repository.list().await.unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.contains(&first));
    assert!(stored.contains(&second));
    let observations = stored
        .iter()
        .map(RecoveryQualificationEvidence::to_observation)
        .collect::<Vec<_>>();
    let decision = evaluate_recovery_qualification(&observations);
    assert!(
        decision.failures().contains(
            &"recovery target RunningExecution must have exactly one observation".to_owned()
        ),
        "unexpected failures: {:?}",
        decision.failures()
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn recovery_qualification_observations_cannot_be_edited_or_deleted(pool: PgPool) {
    let repository =
        PgRecoveryQualificationEvidenceRepository::new(PgStore::from_pool(pool.clone()));
    let evidence = observation(
        RecoveryTarget::RunningExecution,
        RecoveryClassification::SafeToResume,
        StartupRecoveryOutcome::Restored,
    );
    repository.insert(&evidence).await.unwrap();

    let update = sqlx::query(
        "UPDATE recovery_qualification_observations
         SET action = 'abort'
         WHERE id = $1",
    )
    .bind(evidence.id())
    .execute(&pool)
    .await;
    assert!(update.is_err(), "persisted observation was editable");

    let delete = sqlx::query("DELETE FROM recovery_qualification_observations WHERE id = $1")
        .bind(evidence.id())
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "persisted observation was deletable");
    assert_eq!(repository.list().await.unwrap(), vec![evidence]);
}
