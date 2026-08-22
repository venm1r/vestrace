use chrono::{Duration, TimeZone, Utc};
use sqlx::PgPool;
use vestrace_application::{ApplicationError, QualificationBaselineRepository};
use vestrace_domain::conformance::{
    CaseOrigin, CaseStatus, ConformanceCaseResult, ConformanceReport, QualificationProfile,
};
use vestrace_domain::trust::{
    QualificationBaseline, QualificationBaselineState, QualificationBundle, QualificationLifecycle,
};
use vestrace_domain::{QualificationBaselineId, conformance::runner::profile_requirements};
use vestrace_infrastructure::{PgQualificationBaselineRepository, PgStore};

fn bundle_for_profile(target_manifest: &str, profile: QualificationProfile) -> QualificationBundle {
    let started_at = Utc
        .with_ymd_and_hms(2026, 8, 22, 10, 0, 0)
        .single()
        .unwrap();
    let results = profile_requirements(profile)
        .into_iter()
        .map(|requirement_id| ConformanceCaseResult {
            case_id: format!("baseline-{requirement_id}"),
            requirement_ids: vec![requirement_id],
            status: CaseStatus::Pass,
            message: "passed".into(),
            evidence: Some(format!("test://{requirement_id}")),
            origin: CaseOrigin::Executed,
        })
        .collect();

    QualificationBundle::from_conformance_report(
        QualificationLifecycle::Release,
        profile,
        target_manifest,
        "source-revision",
        "sha256:build",
        "sha256:configuration",
        "environment-manifest",
        "suite-v1",
        ConformanceReport::from_results(Some(profile), results),
        Vec::new(),
        vec!["test fixture".to_owned()],
        started_at,
        Some(started_at + Duration::seconds(1)),
    )
    .unwrap()
}

fn bundle(target_manifest: &str) -> QualificationBundle {
    bundle_for_profile(target_manifest, QualificationProfile::Core)
}

fn baseline(bundle: &QualificationBundle) -> QualificationBaseline {
    let published_at = Utc
        .with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
        .single()
        .unwrap()
        + Duration::nanoseconds(999);
    QualificationBaseline::from_bundle(bundle, published_at).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn published_baseline_round_trips_by_id_at_column_precision(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool.clone()));
    let baseline = baseline(&bundle("target-manifest-a"));

    repository.insert(&baseline).await.unwrap();

    let stored = repository
        .find_by_id(baseline.id())
        .await
        .unwrap()
        .expect("published baseline");
    let projected_published_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT published_at FROM qualification_baselines WHERE id = $1")
            .bind(baseline.id().as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(stored, baseline);
    assert_eq!(stored.state(), baseline.state());
    assert!(
        (stored.published_at().timestamp_micros() - projected_published_at.timestamp_micros())
            .abs()
            <= 1
    );
    assert_eq!(
        repository
            .find_by_id(QualificationBaselineId::new())
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn published_baseline_is_found_by_exact_target_digest_and_profile(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool));
    let baseline = baseline(&bundle("target-manifest-a"));

    repository.insert(&baseline).await.unwrap();

    assert_eq!(
        repository
            .find_by_target_digest_and_profile(baseline.target_digest(), baseline.profile())
            .await
            .unwrap(),
        Some(baseline)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn second_publication_for_a_target_is_refused_and_names_the_target(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool));
    let target = bundle_for_profile("target-manifest-a", QualificationProfile::Core);
    let first = baseline(&target);
    let again =
        QualificationBaseline::from_bundle(&target, first.published_at() + Duration::seconds(1))
            .unwrap();

    repository.insert(&first).await.unwrap();
    let error = repository.insert(&again).await.unwrap_err();

    assert!(matches!(error, ApplicationError::Conflict(_)));
    assert!(error.to_string().contains(first.target_digest()));
}

/// One build can answer more than one question about itself.
///
/// This asserted that a second profile for the same target was refused, because
/// the plan asked for uniqueness on the target alone. That was wrong: the
/// profiles are a ladder, and a build qualified as `core` and later as `trusted`
/// has two true and different facts. Uniqueness is on the pair, so the earlier
/// baseline does not have to be invalidated to publish the later one.
#[sqlx::test(migrations = "../../migrations")]
async fn a_second_profile_for_one_target_is_published_beside_the_first(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool));
    let core = bundle_for_profile("target-manifest-a", QualificationProfile::Core);
    let trusted = bundle_for_profile("target-manifest-a", QualificationProfile::Trusted);
    assert_eq!(core.target_digest(), trusted.target_digest());
    assert_ne!(core.profile(), trusted.profile());

    let first = baseline(&core);
    let second =
        QualificationBaseline::from_bundle(&trusted, first.published_at() + Duration::seconds(1))
            .unwrap();

    repository.insert(&first).await.unwrap();
    repository.insert(&second).await.unwrap();

    assert_eq!(
        repository.find_by_id(first.id()).await.unwrap(),
        Some(first)
    );
    assert_eq!(
        repository.find_by_id(second.id()).await.unwrap(),
        Some(second)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_refuses_rewriting_each_published_fact(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool.clone()));
    let baseline = baseline(&bundle("target-manifest-a"));
    repository.insert(&baseline).await.unwrap();

    for (column, statement) in [
        (
            "profile",
            "UPDATE qualification_baselines SET profile = 'trusted' WHERE id = $1",
        ),
        (
            "target_digest",
            "UPDATE qualification_baselines SET target_digest = 'sha256:rewritten' WHERE id = $1",
        ),
        (
            "published_at",
            "UPDATE qualification_baselines SET published_at = published_at + INTERVAL '1 second' WHERE id = $1",
        ),
    ] {
        let error = sqlx::query(statement)
            .bind(baseline.id().as_uuid())
            .execute(&pool)
            .await
            .expect_err("published fact must be immutable");
        assert!(
            error.to_string().contains(column),
            "error for {column} did not name the immutable column: {error}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_refuses_rewriting_each_published_fact_inside_the_payload(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool.clone()));
    let baseline = baseline(&bundle("target-manifest-a"));
    repository.insert(&baseline).await.unwrap();

    for (column, statement) in [
        (
            "profile",
            "UPDATE qualification_baselines
             SET payload = jsonb_set(payload, '{profile}', '\"trusted\"'::jsonb)
             WHERE id = $1",
        ),
        (
            "target_digest",
            "UPDATE qualification_baselines
             SET payload = jsonb_set(payload, '{target_digest}', '\"sha256:rewritten\"'::jsonb)
             WHERE id = $1",
        ),
        (
            "published_at",
            "UPDATE qualification_baselines
             SET payload = jsonb_set(
                 payload,
                 '{published_at}',
                 '\"2027-08-22T12:00:00Z\"'::jsonb
             )
             WHERE id = $1",
        ),
    ] {
        let error = sqlx::query(statement)
            .bind(baseline.id().as_uuid())
            .execute(&pool)
            .await
            .expect_err("authoritative published fact must be immutable");
        assert!(
            error.to_string().contains(column),
            "payload error for {column} did not name the immutable field: {error}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_allows_state_and_invalidation_reason_to_move_with_the_payload(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool.clone()));
    let baseline = baseline(&bundle("target-manifest-a"));
    repository.insert(&baseline).await.unwrap();
    let reason = "superseded by a later operator publication";
    let mut stale_payload = serde_json::to_value(&baseline).unwrap();
    stale_payload["state"] = serde_json::json!("stale");
    stale_payload["invalidation_reason"] = serde_json::json!(reason);

    sqlx::query(
        "UPDATE qualification_baselines
         SET state = 'stale', invalidation_reason = $2, payload = $3
         WHERE id = $1",
    )
    .bind(baseline.id().as_uuid())
    .bind(reason)
    .bind(stale_payload)
    .execute(&pool)
    .await
    .unwrap();

    let stored = repository
        .find_by_id(baseline.id())
        .await
        .unwrap()
        .expect("updated baseline");
    assert_eq!(stored.state(), QualificationBaselineState::Stale);
    assert_eq!(stored.invalidation_reason(), Some(reason));
}

#[sqlx::test(migrations = "../../migrations")]
async fn stored_baseline_for_bundle_a_does_not_match_bundle_b(pool: PgPool) {
    let repository = PgQualificationBaselineRepository::new(PgStore::from_pool(pool));
    let bundle_a = bundle("target-manifest-a");
    let bundle_b = bundle("target-manifest-b");
    let baseline = baseline(&bundle_a);
    repository.insert(&baseline).await.unwrap();

    let stored = repository
        .find_by_target_digest_and_profile(bundle_a.target_digest(), bundle_a.profile())
        .await
        .unwrap()
        .expect("published baseline for bundle A");

    assert!(!stored.matches_bundle(&bundle_b));
}
