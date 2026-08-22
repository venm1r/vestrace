use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use vestrace_application::{ApplicationError, QualificationBaselineRepository};
use vestrace_domain::QualificationBaselineId;
use vestrace_domain::trust::QualificationBaseline;

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgQualificationBaselineRepository {
    pool: PgPool,
}

impl PgQualificationBaselineRepository {
    pub fn new(store: PgStore) -> Self {
        Self {
            pool: store.pool().clone(),
        }
    }
}

#[derive(Debug, FromRow)]
struct QualificationBaselineRow {
    id: uuid::Uuid,
    profile: String,
    target_digest: String,
    state: String,
    published_at: DateTime<Utc>,
    invalidation_reason: Option<String>,
    payload: Value,
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn conflict(message: impl Into<String>) -> ApplicationError {
    ApplicationError::Conflict(message.into())
}

fn enum_name<T: Serialize>(value: T) -> Result<String, ApplicationError> {
    serde_json::to_value(value)
        .map_err(storage_error)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| storage_error("qualification baseline enum did not serialize as a string"))
}

fn decode_row(row: QualificationBaselineRow) -> Result<QualificationBaseline, ApplicationError> {
    let baseline: QualificationBaseline =
        serde_json::from_value(row.payload).map_err(storage_error)?;
    let expected_profile = enum_name(baseline.profile())?;
    let expected_state = enum_name(baseline.state())?;
    let disagreement = [
        ("id", baseline.id().as_uuid() != row.id),
        ("profile", expected_profile != row.profile),
        (
            "target_digest",
            baseline.target_digest() != row.target_digest,
        ),
        ("state", expected_state != row.state),
        (
            "published_at",
            (baseline.published_at().timestamp_micros() - row.published_at.timestamp_micros())
                .abs()
                > 1,
        ),
        (
            "invalidation_reason",
            baseline.invalidation_reason() != row.invalidation_reason.as_deref(),
        ),
    ]
    .into_iter()
    .find_map(|(column, differs)| differs.then_some(column));

    if let Some(column) = disagreement {
        return Err(storage_error(format!(
            "qualification baseline column `{column}` does not match its payload"
        )));
    }
    Ok(baseline)
}

fn insert_error(error: sqlx::Error, target_digest: &str) -> ApplicationError {
    if error.as_database_error().is_some_and(|database_error| {
        database_error.code().as_deref() == Some("23505")
            && database_error.constraint()
                == Some("uq_qualification_baselines_target_digest_profile")
    }) {
        return conflict(format!(
            "qualification baseline already exists for target {target_digest}"
        ));
    }
    storage_error(error)
}

#[async_trait]
impl QualificationBaselineRepository for PgQualificationBaselineRepository {
    async fn insert(&self, baseline: &QualificationBaseline) -> Result<(), ApplicationError> {
        let profile = enum_name(baseline.profile())?;
        let state = enum_name(baseline.state())?;
        let payload = serde_json::to_value(baseline).map_err(storage_error)?;

        sqlx::query(
            "INSERT INTO qualification_baselines (
                 id, profile, target_digest, state, published_at,
                 invalidation_reason, payload
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(baseline.id().as_uuid())
        .bind(profile)
        .bind(baseline.target_digest())
        .bind(state)
        .bind(baseline.published_at())
        .bind(baseline.invalidation_reason())
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|error| insert_error(error, baseline.target_digest()))?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: QualificationBaselineId,
    ) -> Result<Option<QualificationBaseline>, ApplicationError> {
        let row = sqlx::query_as::<_, QualificationBaselineRow>(
            "SELECT id, profile, target_digest, state, published_at,
                    invalidation_reason, payload
             FROM qualification_baselines
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(decode_row).transpose()
    }

    async fn find_by_target_digest(
        &self,
        target_digest: &str,
    ) -> Result<Option<QualificationBaseline>, ApplicationError> {
        let row = sqlx::query_as::<_, QualificationBaselineRow>(
            "SELECT id, profile, target_digest, state, published_at,
                    invalidation_reason, payload
             FROM qualification_baselines
             WHERE target_digest = $1",
        )
        .bind(target_digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(decode_row).transpose()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use vestrace_application::ApplicationError;
    use vestrace_domain::conformance::QualificationProfile;
    use vestrace_domain::trust::{
        QualificationBaseline, QualificationBaselineState, QualificationBundle,
    };

    use super::{QualificationBaselineRow, decode_row};

    fn baseline() -> QualificationBaseline {
        let completed_at = Utc
            .with_ymd_and_hms(2026, 8, 22, 12, 0, 0)
            .single()
            .unwrap()
            + Duration::nanoseconds(999);
        let bundle = QualificationBundle::new(
            QualificationProfile::Core,
            "target-manifest",
            "source-revision",
            "sha256:build",
            "sha256:configuration",
            "environment-manifest",
            "suite-v1",
            Vec::new(),
            vec!["test fixture".to_owned()],
            completed_at,
            Some(completed_at),
        )
        .unwrap();
        QualificationBaseline::from_bundle(&bundle, completed_at).unwrap()
    }

    fn row(baseline: &QualificationBaseline) -> QualificationBaselineRow {
        QualificationBaselineRow {
            id: baseline.id().as_uuid(),
            profile: "core".to_owned(),
            target_digest: baseline.target_digest().to_owned(),
            state: "qualified".to_owned(),
            published_at: baseline.published_at(),
            invalidation_reason: None,
            payload: serde_json::to_value(baseline).unwrap(),
        }
    }

    #[test]
    fn every_projected_field_mismatch_is_refused_and_names_its_column() {
        let baseline = baseline();
        let cases = [
            (
                "id",
                QualificationBaselineRow {
                    id: uuid::Uuid::now_v7(),
                    ..row(&baseline)
                },
            ),
            (
                "profile",
                QualificationBaselineRow {
                    profile: "trusted".to_owned(),
                    ..row(&baseline)
                },
            ),
            (
                "target_digest",
                QualificationBaselineRow {
                    target_digest: "sha256:other-target".to_owned(),
                    ..row(&baseline)
                },
            ),
            (
                "state",
                QualificationBaselineRow {
                    state: "stale".to_owned(),
                    ..row(&baseline)
                },
            ),
            (
                "published_at",
                QualificationBaselineRow {
                    published_at: baseline.published_at() + Duration::microseconds(2),
                    ..row(&baseline)
                },
            ),
            (
                "invalidation_reason",
                QualificationBaselineRow {
                    invalidation_reason: Some("rewritten projection".to_owned()),
                    ..row(&baseline)
                },
            ),
        ];

        for (column, row) in cases {
            let error = decode_row(row).expect_err("projection mismatch must be refused");
            assert!(matches!(error, ApplicationError::Storage(_)));
            assert!(
                error.to_string().contains(&format!("`{column}`")),
                "storage error did not name {column}: {error}"
            );
        }
        assert_eq!(baseline.state(), QualificationBaselineState::Qualified);
    }
}
