use chrono::{Duration, TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{ApplicationError, QualificationRepository};
use vestrace_domain::{
    QualificationBundle, QualificationBundleId, QualificationLifecycle,
    conformance::QualificationProfile,
};
use vestrace_infrastructure::{PgQualificationRepository, PgStore};

fn bundle(target_manifest: &str) -> QualificationBundle {
    let started_at = Utc.with_ymd_and_hms(2026, 8, 12, 0, 0, 0).single().unwrap();
    QualificationBundle::new(
        QualificationProfile::Core,
        target_manifest,
        "source-revision",
        "sha256:build",
        "sha256:configuration",
        "environment-manifest",
        "suite-v1",
        Vec::new(),
        vec!["deployment qualification remains open".to_owned()],
        started_at,
        Some(started_at + Duration::seconds(1)),
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_repository_round_trips_immutable_bundles(pool: PgPool) {
    let repository = PgQualificationRepository::new(PgStore::from_pool(pool));
    let bundle = bundle("target-manifest-a");

    repository.insert(&bundle).await.unwrap();
    repository.insert(&bundle).await.unwrap();

    assert_eq!(
        repository.find_by_id(bundle.id()).await.unwrap(),
        Some(bundle.clone())
    );
    assert_eq!(
        repository
            .find_latest(bundle.profile(), bundle.lifecycle(), bundle.target_digest(),)
            .await
            .unwrap(),
        Some(bundle.clone())
    );
    assert_eq!(
        repository
            .find_by_id(QualificationBundleId::new())
            .await
            .unwrap(),
        None
    );

    let mut conflicting_payload = serde_json::to_value(&bundle).unwrap();
    conflicting_payload["target_digest"] = serde_json::json!("sha256:tampered");
    let conflicting_bundle: QualificationBundle =
        serde_json::from_value(conflicting_payload).unwrap();

    assert!(matches!(
        repository.insert(&conflicting_bundle).await,
        Err(ApplicationError::Conflict(_))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_repository_latest_lookup_is_target_bound(pool: PgPool) {
    let repository = PgQualificationRepository::new(PgStore::from_pool(pool));
    let first = bundle("target-manifest-a");
    let second = bundle("target-manifest-b");

    repository.insert(&first).await.unwrap();
    repository.insert(&second).await.unwrap();

    assert_eq!(
        repository
            .find_latest(
                first.profile(),
                QualificationLifecycle::Release,
                second.target_digest(),
            )
            .await
            .unwrap(),
        Some(second)
    );
}
