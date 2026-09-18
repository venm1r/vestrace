use async_trait::async_trait;
use sqlx::{PgConnection, Row, postgres::PgRow};
use vestrace_application::{
    ApplicationError, EvaluationFactRecord, EvaluationRecord, EvaluationRepository,
    LearnedProjectionRecord, LearningProposalRecord, LearningRepository, RequestContext,
    WorkflowDefinitionRecord, WorkflowRepository, WorkflowRevisionRecord,
};
use vestrace_domain::{
    EvidenceRef,
    id::{
        EvaluationId, LearningProjectionId, LearningProposalId, WorkflowId, WorkflowRevisionId,
        WorkspaceId,
    },
};

use super::PgStore;

/// Workflow definitions and their revisions.
///
/// # Why this holds a store and not a pool
///
/// `create`, `save_revision`, `find_by_id` and `get_revision` all took a
/// `RequestContext` and ignored it. The two reads selected on an id alone, so a
/// `WorkflowId` from another tenant returned that tenant's definition, and the
/// revision read returned its definition body — which is the whole content of a
/// workflow, not merely its name.
pub struct PgWorkflowRepository {
    store: PgStore,
}

impl PgWorkflowRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

/// A record may not be written into a workspace other than the authenticated
/// one. See the note on the same check in the execution history adapter: the
/// context is the authority, not the `workspace_id` the record carries.
fn same_workspace(
    context: &RequestContext,
    record_workspace: WorkspaceId,
    what: &str,
) -> Result<(), ApplicationError> {
    if record_workspace == context.workspace_id {
        return Ok(());
    }
    Err(ApplicationError::Policy(format!(
        "{what} cannot be written into another workspace"
    )))
}

fn decode_evaluation_fact(row: &PgRow) -> Result<EvaluationFactRecord, ApplicationError> {
    let decode = |column: &str| -> Result<serde_json::Value, ApplicationError> {
        row.try_get(column).map_err(storage_error)
    };

    Ok(EvaluationFactRecord {
        id: vestrace_domain::id::EvaluationId::from_uuid(row.try_get("id").map_err(storage_error)?),
        workspace_id: WorkspaceId::from_uuid(row.try_get("workspace_id").map_err(storage_error)?),
        target: serde_json::from_value(decode("target")?).map_err(storage_error)?,
        evaluator: serde_json::from_value(decode("evaluator")?).map_err(storage_error)?,
        metric: serde_json::from_value(decode("metric")?).map_err(storage_error)?,
        result: serde_json::from_value(decode("result")?).map_err(storage_error)?,
        evidence_refs: serde_json::from_value(decode("evidence_refs")?).map_err(storage_error)?,
        authority: serde_json::from_value(decode("authority")?).map_err(storage_error)?,
        policy_version: row.try_get("policy_version").map_err(storage_error)?,
        created_at: row.try_get("created_at").map_err(storage_error)?,
    })
}

fn decode_learning_projection(row: &PgRow) -> Result<LearnedProjectionRecord, ApplicationError> {
    let decode = |column: &str| -> Result<serde_json::Value, ApplicationError> {
        row.try_get(column).map_err(storage_error)
    };

    Ok(LearnedProjectionRecord {
        id: LearningProjectionId::from_uuid(row.try_get("id").map_err(storage_error)?),
        workspace_id: WorkspaceId::from_uuid(row.try_get("workspace_id").map_err(storage_error)?),
        kind: serde_json::from_value(decode("kind")?).map_err(storage_error)?,
        target: serde_json::from_value(decode("target")?).map_err(storage_error)?,
        generator: serde_json::from_value(decode("generator")?).map_err(storage_error)?,
        authority: serde_json::from_value(decode("authority")?).map_err(storage_error)?,
        source_generation: row
            .try_get::<i32, _>("source_generation")
            .map_err(storage_error)? as u32,
        source_evaluation_fact_ids: serde_json::from_value(decode("source_evaluation_fact_ids")?)
            .map_err(storage_error)?,
        source_evidence_refs: serde_json::from_value(decode("source_evidence_refs")?)
            .map_err(storage_error)?,
        content: decode("content")?,
        created_at: row.try_get("created_at").map_err(storage_error)?,
    })
}

fn decode_learning_proposal(row: &PgRow) -> Result<LearningProposalRecord, ApplicationError> {
    let decode = |column: &str| -> Result<serde_json::Value, ApplicationError> {
        row.try_get(column).map_err(storage_error)
    };

    Ok(LearningProposalRecord {
        id: LearningProposalId::from_uuid(row.try_get("id").map_err(storage_error)?),
        workspace_id: WorkspaceId::from_uuid(row.try_get("workspace_id").map_err(storage_error)?),
        source_projection_ids: serde_json::from_value(decode("source_projection_ids")?)
            .map_err(storage_error)?,
        source_evaluation_fact_ids: serde_json::from_value(decode("source_evaluation_fact_ids")?)
            .map_err(storage_error)?,
        target: serde_json::from_value(decode("target")?).map_err(storage_error)?,
        change: serde_json::from_value(decode("change")?).map_err(storage_error)?,
        expected_target_revision: row
            .try_get::<i32, _>("expected_target_revision")
            .map_err(storage_error)? as u32,
        rationale: row.try_get("rationale").map_err(storage_error)?,
        policy_version: row.try_get("policy_version").map_err(storage_error)?,
        created_by: vestrace_domain::id::PrincipalId::from_uuid(
            row.try_get("created_by").map_err(storage_error)?,
        ),
        status: serde_json::from_value(decode("status")?).map_err(storage_error)?,
        created_at: row.try_get("created_at").map_err(storage_error)?,
        updated_at: row.try_get("updated_at").map_err(storage_error)?,
    })
}

async fn ensure_evaluation_facts_are_workspace_scoped(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    evaluation_ids: &[EvaluationId],
) -> Result<(), ApplicationError> {
    let ids = serde_json::to_value(evaluation_ids).map_err(storage_error)?;
    let missing: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM jsonb_array_elements_text($1::jsonb) AS source(id)
        WHERE NOT EXISTS (
            SELECT 1
            FROM evaluation_facts
            WHERE evaluation_facts.id = source.id::uuid
              AND evaluation_facts.workspace_id = $2
        )
        "#,
    )
    .bind(ids)
    .bind(workspace_id.as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(storage_error)?;

    if missing > 0 {
        return Err(ApplicationError::Policy(
            "learning provenance references an evaluation fact outside the workspace or missing"
                .into(),
        ));
    }
    Ok(())
}

async fn ensure_projections_are_workspace_scoped(
    connection: &mut PgConnection,
    workspace_id: WorkspaceId,
    projection_ids: &[LearningProjectionId],
) -> Result<(), ApplicationError> {
    let ids = serde_json::to_value(projection_ids).map_err(storage_error)?;
    let missing: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM jsonb_array_elements_text($1::jsonb) AS source(id)
        WHERE NOT EXISTS (
            SELECT 1
            FROM learned_projections
            WHERE learned_projections.id = source.id::uuid
              AND learned_projections.workspace_id = $2
        )
        "#,
    )
    .bind(ids)
    .bind(workspace_id.as_uuid())
    .fetch_one(&mut *connection)
    .await
    .map_err(storage_error)?;

    if missing > 0 {
        return Err(ApplicationError::Policy(
            "learning provenance references a projection outside the workspace or missing".into(),
        ));
    }
    Ok(())
}

#[async_trait]
impl WorkflowRepository for PgWorkflowRepository {
    async fn create(
        &self,
        context: &RequestContext,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), ApplicationError> {
        same_workspace(context, record.workspace_id, "a workflow definition")?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO workflow_definitions (id, workspace_id, name, current_revision, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(&record.name)
        .bind(record.current_revision as i32)
        .bind(record.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<WorkflowDefinitionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, name, current_revision, created_at
            FROM workflow_definitions
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| WorkflowDefinitionRecord {
                id: WorkflowId::from_uuid(r.get("id")),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                name: r.get("name"),
                current_revision: r.get::<i32, _>("current_revision") as u32,
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: WorkflowId,
    ) -> Result<Option<WorkflowDefinitionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, name, current_revision, created_at
            FROM workflow_definitions
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        Ok(row.map(|r| WorkflowDefinitionRecord {
            id: WorkflowId::from_uuid(r.get("id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            name: r.get("name"),
            current_revision: r.get::<i32, _>("current_revision") as u32,
            created_at: r.get("created_at"),
        }))
    }

    async fn save_revision(
        &self,
        context: &RequestContext,
        record: &WorkflowRevisionRecord,
    ) -> Result<(), ApplicationError> {
        same_workspace(context, record.workspace_id, "a workflow revision")?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO workflow_revisions (revision_id, workflow_id, workspace_id, revision_number, definition, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(record.revision_id.as_uuid())
        .bind(record.workflow_id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.revision_number as i32)
        .bind(&record.definition)
        .bind(record.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn get_revision(
        &self,
        context: &RequestContext,
        workflow_id: WorkflowId,
        revision_number: u32,
    ) -> Result<Option<WorkflowRevisionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT revision_id, workflow_id, workspace_id, revision_number, definition, created_at
            FROM workflow_revisions
            WHERE workflow_id = $1 AND revision_number = $2 AND workspace_id = $3
            "#,
        )
        .bind(workflow_id.as_uuid())
        .bind(revision_number as i32)
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        Ok(row.map(|r| WorkflowRevisionRecord {
            revision_id: WorkflowRevisionId::from_uuid(r.get("revision_id")),
            workflow_id: WorkflowId::from_uuid(r.get("workflow_id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            revision_number: r.get::<i32, _>("revision_number") as u32,
            definition: r.get("definition"),
            created_at: r.get("created_at"),
        }))
    }
}

/// Evaluations, raw evaluation facts, learned projections and learning
/// proposals.
///
/// # Why this holds a store and not a pool
///
/// This adapter is where the two halves of the boundary were furthest apart,
/// and in the *opposite* direction from the rest.
///
/// Migration `0124` forced row level security on `learned_projections` and
/// `learning_proposals`. This adapter — their only writer — used a bare pool, so
/// `vestrace.workspace_id` was never set and `vestrace_current_workspace_id()`
/// returned NULL for every statement it issued. A forced policy comparing
/// `workspace_id` against NULL admits nothing: **every write to those two
/// tables has been refused, and every read has returned empty, since 0124**.
/// `POST /v1/learning/projections` answered `500 storage_failure` in any
/// deployment running under the runtime role, which is to say all of them.
///
/// Elsewhere the same split made policies inert; here it made a surface dead.
/// Both come from an adapter and its tables being changed independently, and
/// neither was caught because every database test connects as a superuser,
/// which bypasses row level security in both directions.
///
/// The provenance checks now run on the same scoped connection as the write
/// they guard, so a fact cannot be verified in one transaction and referenced
/// from another.
pub struct PgEvaluationRepository {
    store: PgStore,
}

impl PgEvaluationRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl EvaluationRepository for PgEvaluationRepository {
    async fn create(
        &self,
        context: &RequestContext,
        record: &EvaluationRecord,
    ) -> Result<(), ApplicationError> {
        same_workspace(context, record.workspace_id, "an evaluation")?;

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO evaluations (id, workspace_id, model_id, name, status, score, summary, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(record.id.as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.model_id.map(|m| m.as_uuid()))
        .bind(&record.name)
        .bind(&record.status)
        .bind(record.score)
        .bind(&record.summary)
        .bind(record.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn list(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<EvaluationRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, model_id, name, status, score, summary, created_at
            FROM evaluations
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|r| EvaluationRecord {
                id: EvaluationId::from_uuid(r.get("id")),
                workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
                model_id: r
                    .get::<Option<uuid::Uuid>, _>("model_id")
                    .map(vestrace_domain::id::ModelId::from_uuid),
                name: r.get("name"),
                status: r.get("status"),
                score: r.get::<Option<f64>, _>("score"),
                summary: r.get("summary"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: EvaluationId,
    ) -> Result<Option<EvaluationRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, model_id, name, status, score, summary, created_at
            FROM evaluations
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        Ok(row.map(|r| EvaluationRecord {
            id: EvaluationId::from_uuid(r.get("id")),
            workspace_id: WorkspaceId::from_uuid(r.get("workspace_id")),
            model_id: r
                .get::<Option<uuid::Uuid>, _>("model_id")
                .map(vestrace_domain::id::ModelId::from_uuid),
            name: r.get("name"),
            status: r.get("status"),
            score: r.get::<Option<f64>, _>("score"),
            summary: r.get("summary"),
            created_at: r.get("created_at"),
        }))
    }

    async fn create_fact(
        &self,
        context: &RequestContext,
        fact: &EvaluationFactRecord,
    ) -> Result<(), ApplicationError> {
        if fact.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "evaluation fact workspace does not match request context".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        sqlx::query(
            r#"
            INSERT INTO evaluation_facts
                (id, workspace_id, target, evaluator, metric, result, evidence_refs, authority, policy_version, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(fact.id.as_uuid())
        .bind(fact.workspace_id.as_uuid())
        .bind(serde_json::to_value(&fact.target).map_err(storage_error)?)
        .bind(serde_json::to_value(&fact.evaluator).map_err(storage_error)?)
        .bind(serde_json::to_value(&fact.metric).map_err(storage_error)?)
        .bind(serde_json::to_value(fact.result).map_err(storage_error)?)
        .bind(serde_json::to_value(&fact.evidence_refs).map_err(storage_error)?)
        .bind(serde_json::to_value(fact.authority).map_err(storage_error)?)
        .bind(&fact.policy_version)
        .bind(fact.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn list_facts(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<EvaluationFactRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, target, evaluator, metric, result,
                   evidence_refs, authority, policy_version, created_at
            FROM evaluation_facts
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.iter().map(decode_evaluation_fact).collect()
    }

    async fn find_fact(
        &self,
        context: &RequestContext,
        id: vestrace_domain::id::EvaluationId,
    ) -> Result<Option<EvaluationFactRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, target, evaluator, metric, result,
                   evidence_refs, authority, policy_version, created_at
            FROM evaluation_facts
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        row.as_ref().map(decode_evaluation_fact).transpose()
    }
}

#[async_trait]
impl LearningRepository for PgEvaluationRepository {
    async fn create_projection(
        &self,
        context: &RequestContext,
        projection: &LearnedProjectionRecord,
    ) -> Result<(), ApplicationError> {
        if projection.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "learned projection workspace does not match request context".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        ensure_evaluation_facts_are_workspace_scoped(
            scoped.connection(),
            context.workspace_id,
            &projection.source_evaluation_fact_ids,
        )
        .await?;
        let evidence_fact_ids: Vec<EvaluationId> = projection
            .source_evidence_refs
            .iter()
            .filter_map(|evidence| match evidence {
                EvidenceRef::EvaluationRef { evaluation_id } => Some(*evaluation_id),
                _ => None,
            })
            .collect();
        ensure_evaluation_facts_are_workspace_scoped(
            scoped.connection(),
            context.workspace_id,
            &evidence_fact_ids,
        )
        .await?;

        sqlx::query(
            r#"
            INSERT INTO learned_projections
                (id, workspace_id, kind, target, generator, authority, source_generation,
                 source_evaluation_fact_ids, source_evidence_refs, content, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(projection.id.as_uuid())
        .bind(projection.workspace_id.as_uuid())
        .bind(serde_json::to_value(projection.kind).map_err(storage_error)?)
        .bind(serde_json::to_value(&projection.target).map_err(storage_error)?)
        .bind(serde_json::to_value(&projection.generator).map_err(storage_error)?)
        .bind(serde_json::to_value(projection.authority).map_err(storage_error)?)
        .bind(projection.source_generation as i32)
        .bind(serde_json::to_value(&projection.source_evaluation_fact_ids).map_err(storage_error)?)
        .bind(serde_json::to_value(&projection.source_evidence_refs).map_err(storage_error)?)
        .bind(&projection.content)
        .bind(projection.created_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn list_projections(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<LearnedProjectionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, kind, target, generator, authority, source_generation,
                   source_evaluation_fact_ids, source_evidence_refs, content, created_at
            FROM learned_projections
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.iter().map(decode_learning_projection).collect()
    }

    async fn find_projection(
        &self,
        context: &RequestContext,
        id: LearningProjectionId,
    ) -> Result<Option<LearnedProjectionRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, kind, target, generator, authority, source_generation,
                   source_evaluation_fact_ids, source_evidence_refs, content, created_at
            FROM learned_projections
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        row.as_ref().map(decode_learning_projection).transpose()
    }

    async fn create_proposal(
        &self,
        context: &RequestContext,
        proposal: &LearningProposalRecord,
    ) -> Result<(), ApplicationError> {
        if proposal.workspace_id != context.workspace_id {
            return Err(ApplicationError::Policy(
                "learning proposal workspace does not match request context".into(),
            ));
        }

        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        ensure_projections_are_workspace_scoped(
            scoped.connection(),
            context.workspace_id,
            &proposal.source_projection_ids,
        )
        .await?;
        ensure_evaluation_facts_are_workspace_scoped(
            scoped.connection(),
            context.workspace_id,
            &proposal.source_evaluation_fact_ids,
        )
        .await?;

        sqlx::query(
            r#"
            INSERT INTO learning_proposals
                (id, workspace_id, source_projection_ids, source_evaluation_fact_ids, target,
                 change, expected_target_revision, rationale, policy_version, created_by, status,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(proposal.id.as_uuid())
        .bind(proposal.workspace_id.as_uuid())
        .bind(serde_json::to_value(&proposal.source_projection_ids).map_err(storage_error)?)
        .bind(serde_json::to_value(&proposal.source_evaluation_fact_ids).map_err(storage_error)?)
        .bind(serde_json::to_value(&proposal.target).map_err(storage_error)?)
        .bind(serde_json::to_value(&proposal.change).map_err(storage_error)?)
        .bind(proposal.expected_target_revision as i32)
        .bind(&proposal.rationale)
        .bind(&proposal.policy_version)
        .bind(proposal.created_by.as_uuid())
        .bind(serde_json::to_value(proposal.status).map_err(storage_error)?)
        .bind(proposal.created_at)
        .bind(proposal.updated_at)
        .execute(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)
    }

    async fn list_proposals(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<LearningProposalRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, source_projection_ids, source_evaluation_fact_ids, target,
                   change, expected_target_revision, rationale, policy_version, created_by,
                   status, created_at, updated_at
            FROM learning_proposals
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .bind(context.workspace_id.as_uuid())
        .fetch_all(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        rows.iter().map(decode_learning_proposal).collect()
    }

    async fn find_proposal(
        &self,
        context: &RequestContext,
        id: LearningProposalId,
    ) -> Result<Option<LearningProposalRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, source_projection_ids, source_evaluation_fact_ids, target,
                   change, expected_target_revision, rationale, policy_version, created_by,
                   status, created_at, updated_at
            FROM learning_proposals
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        row.as_ref().map(decode_learning_proposal).transpose()
    }

    async fn submit_proposal(
        &self,
        context: &RequestContext,
        id: LearningProposalId,
        at: vestrace_domain::time::Timestamp,
    ) -> Result<Option<LearningProposalRecord>, ApplicationError> {
        let mut scoped = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;

        // The submitted row is returned from the same statement that wrote it.
        // Reading it back afterwards would leave a window in which another
        // writer could change the status between the update and the read, and
        // report a proposal this call did not produce.
        let updated = sqlx::query(
            r#"
            UPDATE learning_proposals
            SET status = $1, updated_at = $2
            WHERE id = $3
              AND workspace_id = $4
              AND status = '"draft"'::jsonb
            RETURNING id, workspace_id, source_projection_ids, source_evaluation_fact_ids, target,
                      change, expected_target_revision, rationale, policy_version, created_by,
                      status, created_at, updated_at
            "#,
        )
        .bind(serde_json::json!("submitted"))
        .bind(at)
        .bind(id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .fetch_optional(scoped.connection())
        .await
        .map_err(storage_error)?;

        scoped.commit().await.map_err(storage_error)?;

        updated.as_ref().map(decode_learning_proposal).transpose()
    }
}
