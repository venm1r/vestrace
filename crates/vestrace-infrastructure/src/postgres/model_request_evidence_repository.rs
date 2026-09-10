use std::{collections::HashMap, sync::Arc};

use sqlx::{FromRow, Row};
use vestrace_application::{
    ApplicationError, CreateModelRequestEvidence, EffectiveChatMessage, EffectiveChatRole,
    EffectiveModelRequest, EffectiveRequestLimits, EffectiveSampling, EffectiveToolSchema,
    MaterialKeyVault, MissingModelRequestReference, ModelRequestEvidenceRepository,
    ModelRequestNodeKind, ModelRequestReconstruction, Q1AssistantToolCallReplay, Q1MreSource,
    Q1ProbeRequest, Q1SafeMessageLayout, Q1SafeResponseFormat, Q1SafeToolChoice, UnitOfWork,
    VaultError,
};
use vestrace_domain::{
    ContentMaterialId, MaterialKeyId, ModelRequestEvidenceId, QualificationJobId, WorkspaceId,
};
use zeroize::Zeroizing;

use crate::crypto::{ContentMaterialCodec, MAX_FRAMED_MATERIAL_BYTES};
use crate::openai_q1::OpenAiQ1Profile;

use super::PgScopedTransaction;

const MAX_MODEL_REQUEST_EVIDENCE_NODES: usize = 8200;
const MIN_FRAMED_MATERIAL_BYTES: usize = 4096;

pub struct PgModelRequestEvidenceRepository {
    vault: Arc<dyn MaterialKeyVault>,
    codec: ContentMaterialCodec,
}

impl PgModelRequestEvidenceRepository {
    pub fn new(vault: Arc<dyn MaterialKeyVault>) -> Self {
        Self {
            vault,
            codec: ContentMaterialCodec::new(),
        }
    }

    /// Reconstructs the one closed q1 adapter request from a Complete MRE.
    ///
    /// Q1 intentionally has no content-material nodes: its immutable target,
    /// pinned manifest and source tuple are the complete semantic request.
    /// This method reads and validates that tuple under the same evidence lock
    /// used by ordinary reconstruction, then returns the owned request that
    /// can be handed unchanged to `OpenAiCompatibleClient::execute_q1`.
    pub async fn reconstruct_q1_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        workspace_id: WorkspaceId,
        evidence_id: ModelRequestEvidenceId,
    ) -> Result<Q1ProbeRequest, ApplicationError> {
        let transaction = transaction(unit_of_work)?;
        sqlx::query("SELECT vestrace_lock_model_request_evidence_for_reconstruction($1,$2)")
            .bind(workspace_id.as_uuid())
            .bind(evidence_id.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(reconstruction_lock_error)?;
        let row = sqlx::query_as::<_, Q1MreRow>(
            "SELECT
                 target.qualification_job_id,
                 source.probe_ordinal,
                 source.message_layout,
                 source.tool_choice,
                 source.parallel_tool_calls,
                 source.response_format,
                 source.stream,
                 source.stream_include_usage,
                 source.assistant_tool_call_id,
                 chat.wire_model_id AS chat_wire_model_id,
                 embedding.wire_model_id AS embedding_wire_model_id
               FROM model_request_evidence_roots AS root
               JOIN qualification_q1_mre_sources AS source
                 ON source.workspace_id=root.workspace_id
                AND source.evidence_root_id=root.id
               JOIN qualification_target_bindings AS target
                 ON target.workspace_id=root.workspace_id
                AND target.id=root.qualification_target_binding_id
               JOIN model_revisions AS chat
                 ON chat.workspace_id=target.workspace_id
                AND chat.id=target.chat_model_revision_id
                AND chat.connection_revision_id=target.connection_revision_id
               JOIN model_revisions AS embedding
                 ON embedding.workspace_id=target.workspace_id
                AND embedding.id=target.embedding_model_revision_id
                AND embedding.connection_revision_id=target.connection_revision_id
              WHERE root.workspace_id=$1
                AND root.id=$2
                AND root.cause_kind='qualification_probe'
                AND EXISTS (
                    SELECT 1 FROM model_request_evidence_checks AS checked
                     WHERE checked.workspace_id=root.workspace_id
                       AND checked.evidence_root_id=root.id
                       AND checked.status='complete'
                       AND checked.missing_reference_count=0
                )",
        )
        .bind(workspace_id.as_uuid())
        .bind(evidence_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(q1_mre_incomplete)?;
        let assistant_tool_call = row
            .assistant_tool_call_id
            .map(Q1AssistantToolCallReplay::new)
            .transpose()
            .map_err(|_| q1_mre_incomplete())?;
        let source = Q1MreSource::new(
            row.probe_ordinal.as_str(),
            q1_message_layout_from_sql(&row.message_layout)?,
            q1_tool_choice_from_sql(&row.tool_choice)?,
            row.parallel_tool_calls,
            q1_response_format_from_sql(&row.response_format)?,
            row.stream,
            row.stream_include_usage,
            assistant_tool_call.clone(),
        )
        .map_err(|_| q1_mre_incomplete())?;
        let profile = OpenAiQ1Profile::from_pinned_manifest().map_err(|_| q1_mre_incomplete())?;
        let expected_source = profile
            .mre_source(row.probe_ordinal.as_str(), assistant_tool_call.clone())
            .map_err(|_| q1_mre_incomplete())?;
        if source != expected_source {
            return Err(q1_mre_incomplete());
        }
        profile
            .typed_request(
                QualificationJobId::from_uuid(row.qualification_job_id),
                row.probe_ordinal.as_str(),
                &row.chat_wire_model_id,
                &row.embedding_wire_model_id,
                assistant_tool_call,
            )
            .map_err(|_| q1_mre_incomplete())
    }
}

impl std::fmt::Debug for PgModelRequestEvidenceRepository {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PgModelRequestEvidenceRepository")
            .field("vault", &"host-owned")
            .field("codec", &"versioned-padded-aead")
            .finish()
    }
}

#[derive(FromRow)]
struct RootRow {
    external_effect_id: uuid::Uuid,
    request_kind: String,
    binding_snapshot_id: Option<uuid::Uuid>,
    qualification_target_binding_id: Option<uuid::Uuid>,
    cause_kind: String,
    cause_id: uuid::Uuid,
}

#[derive(Clone, FromRow)]
struct NodeRow {
    ordinal: i32,
    reference_kind: String,
    reference_id: uuid::Uuid,
    reference_version: Option<i64>,
    safe_ordinal: Option<String>,
}

#[derive(FromRow)]
struct RequestShapeRow {
    stream: bool,
    input_roles: Vec<String>,
}

#[derive(FromRow)]
struct LimitsRow {
    max_output_tokens: i32,
    max_inputs: i32,
    max_input_bytes: i32,
}

#[derive(FromRow)]
struct SamplingRow {
    temperature: f64,
    top_p: f64,
}

#[derive(FromRow)]
struct ToolRow {
    tool_name: String,
    description: Option<String>,
    parameters_schema: serde_json::Value,
}

#[derive(Clone, FromRow)]
struct MaterialRow {
    id: uuid::Uuid,
    intent_id: uuid::Uuid,
    material_key_id: uuid::Uuid,
    state: String,
}

#[derive(FromRow)]
struct ErasureRow {
    preparation_id: uuid::Uuid,
}

#[derive(FromRow)]
struct Q1MreRow {
    qualification_job_id: uuid::Uuid,
    probe_ordinal: String,
    message_layout: String,
    tool_choice: String,
    parallel_tool_calls: bool,
    response_format: String,
    stream: bool,
    stream_include_usage: bool,
    assistant_tool_call_id: Option<String>,
    chat_wire_model_id: String,
    embedding_wire_model_id: String,
}

struct HydratedSources {
    shape: RequestShapeRow,
    limits: LimitsRow,
    sampling: Option<SamplingRow>,
    wire_model_id: Option<String>,
    tools: Vec<ToolRow>,
}

#[async_trait::async_trait]
impl ModelRequestEvidenceRepository for PgModelRequestEvidenceRepository {
    async fn create_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        creation: &CreateModelRequestEvidence,
    ) -> Result<ModelRequestEvidenceId, ApplicationError> {
        let transaction = transaction(unit_of_work)?;
        let canonical = creation.canonical();
        let shape = &canonical.request_shape;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT vestrace_create_model_request_shape_revision($1,$2,$3,$4,$5,$6)",
        )
        .bind(shape.id)
        .bind(creation.workspace_id().as_uuid())
        .bind(to_i64(shape.version)?)
        .bind(shape.request_kind.as_str())
        .bind(shape.stream)
        .bind(
            shape
                .input_roles
                .iter()
                .map(|role| role.as_str())
                .collect::<Vec<_>>(),
        )
        .fetch_one(transaction.connection())
        .await
        .map_err(map_database_error)?;

        if let Some(sampling) = &canonical.sampling {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT vestrace_create_model_sampling_revision($1,$2,$3,$4,$5)",
            )
            .bind(sampling.id)
            .bind(creation.workspace_id().as_uuid())
            .bind(to_i64(sampling.version)?)
            .bind(sampling.sampling.temperature())
            .bind(sampling.sampling.top_p())
            .fetch_one(transaction.connection())
            .await
            .map_err(map_database_error)?;
        }
        let limits = &canonical.limits;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT vestrace_create_model_limits_revision($1,$2,$3,$4,$5,$6)",
        )
        .bind(limits.id)
        .bind(creation.workspace_id().as_uuid())
        .bind(to_i64(limits.version)?)
        .bind(to_i32(limits.limits.max_output_tokens())?)
        .bind(to_i32(limits.limits.max_inputs())?)
        .bind(to_i32(limits.limits.max_input_bytes())?)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_database_error)?;
        for tool in &canonical.tools {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT vestrace_create_model_tool_schema_revision($1,$2,$3,$4,$5,$6)",
            )
            .bind(tool.id)
            .bind(creation.workspace_id().as_uuid())
            .bind(to_i64(tool.version)?)
            .bind(&tool.name)
            .bind(&tool.description)
            .bind(&tool.parameters_schema)
            .fetch_one(transaction.connection())
            .await
            .map_err(map_database_error)?;
        }

        let node_kinds: Vec<&str> = creation
            .nodes()
            .iter()
            .map(|node| node.kind.as_str())
            .collect();
        let node_ids: Vec<uuid::Uuid> = creation
            .nodes()
            .iter()
            .map(|node| node.reference_id)
            .collect();
        let node_versions: Vec<Option<i64>> = creation
            .nodes()
            .iter()
            .map(|node| node.reference_version.map(to_i64).transpose())
            .collect::<Result<_, _>>()?;
        let node_safe_ordinals: Vec<Option<&str>> = creation
            .nodes()
            .iter()
            .map(|node| node.safe_ordinal.as_deref())
            .collect();
        let root_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT vestrace_create_model_request_evidence(
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12
            )",
        )
        .bind(creation.root_id())
        .bind(creation.workspace_id().as_uuid())
        .bind(creation.external_effect_id())
        .bind(shape.request_kind.as_str())
        .bind(creation.binding_snapshot_id())
        .bind(creation.qualification_target_binding_id())
        .bind(creation.cause_kind().as_str())
        .bind(creation.cause_id())
        .bind(node_kinds)
        .bind(node_ids)
        .bind(node_versions)
        .bind(node_safe_ordinals)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_database_error)?;
        if let Some(source) = creation.q1_source() {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT vestrace_create_qualification_q1_mre_source(
                    $1,$2,$3,$4,$5,$6,$7,$8,$9,$10
                )",
            )
            .bind(root_id)
            .bind(creation.workspace_id().as_uuid())
            .bind(source.ordinal())
            .bind(q1_message_layout(source.message_layout()))
            .bind(q1_tool_choice(source.tool_choice()))
            .bind(source.parallel_tool_calls())
            .bind(q1_response_format(source.response_format()))
            .bind(source.stream())
            .bind(source.stream_include_usage())
            .bind(source.assistant_tool_call().map(|replay| replay.call_id()))
            .fetch_one(transaction.connection())
            .await
            .map_err(map_database_error)?;
        }
        Ok(ModelRequestEvidenceId::from_uuid(root_id))
    }

    async fn reconstruct_current_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        workspace_id: WorkspaceId,
        evidence_id: ModelRequestEvidenceId,
    ) -> Result<ModelRequestReconstruction, ApplicationError> {
        let transaction = transaction(unit_of_work)?;
        sqlx::query("SELECT vestrace_lock_model_request_evidence_for_reconstruction($1,$2)")
            .bind(workspace_id.as_uuid())
            .bind(evidence_id.as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(reconstruction_lock_error)?;
        let root = sqlx::query_as::<_, RootRow>(
            "SELECT external_effect_id, request_kind, binding_snapshot_id,
                    qualification_target_binding_id, cause_kind, cause_id
               FROM model_request_evidence_roots
              WHERE id=$1 AND workspace_id=$2",
        )
        .bind(evidence_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?
        .ok_or_else(|| ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into()))?;
        if let Err(error) = lock_cause(transaction, workspace_id, &root).await {
            if is_incomplete_error(&error) {
                let missing = vec![missing_cause_reference(&root, evidence_id)];
                return self
                    .append_incomplete(transaction, workspace_id, evidence_id, missing)
                    .await;
            }
            return Err(error);
        }
        let nodes = sqlx::query_as::<_, NodeRow>(
            "SELECT ordinal, reference_kind, reference_id, reference_version, safe_ordinal
               FROM model_request_evidence_nodes
              WHERE evidence_root_id=$1 AND workspace_id=$2
              ORDER BY ordinal",
        )
        .bind(evidence_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        if nodes.len() > MAX_MODEL_REQUEST_EVIDENCE_NODES
            || nodes
                .iter()
                .enumerate()
                .any(|(index, node)| node.ordinal != index as i32)
            || !ordered_node_ordinals_are_valid(&nodes, "governed_input_material")
            || !ordered_node_ordinals_are_valid(&nodes, "tool_schema_revision")
            || !reconstruction_node_matrix_is_valid(&root, &nodes)
        {
            let missing = matrix_failure_reference(&root, evidence_id, &nodes);
            return self
                .append_incomplete(transaction, workspace_id, evidence_id, vec![missing])
                .await;
        }
        let missing_sources = missing_retained_sources(transaction, workspace_id, &nodes).await?;
        if !missing_sources.is_empty() {
            return self
                .append_incomplete(transaction, workspace_id, evidence_id, missing_sources)
                .await;
        }
        let sources = match lock_and_load_sources(transaction, workspace_id, &root, &nodes).await {
            Ok(sources) => sources,
            Err(error) if is_incomplete_error(&error) => {
                let missing = source_failure_references(evidence_id, &nodes);
                return self
                    .append_incomplete(transaction, workspace_id, evidence_id, missing)
                    .await;
            }
            Err(error) => return Err(error),
        };
        let material_nodes: Vec<&NodeRow> = nodes
            .iter()
            .filter(|node| node.reference_kind == "governed_input_material")
            .collect();
        let mut material_ids: Vec<uuid::Uuid> = material_nodes
            .iter()
            .map(|node| node.reference_id)
            .collect();
        material_ids.sort_unstable();
        let material_rows = lock_materials(transaction, workspace_id, &material_ids).await?;
        let material_by_id: HashMap<uuid::Uuid, MaterialRow> = material_rows
            .iter()
            .cloned()
            .map(|row| (row.id, row))
            .collect();
        let mut intent_ids: Vec<uuid::Uuid> =
            material_rows.iter().map(|row| row.intent_id).collect();
        intent_ids.sort_unstable();
        lock_material_intents(transaction, workspace_id, &intent_ids).await?;
        let byte_lengths = load_byte_lengths(transaction, workspace_id, &material_ids).await?;
        let aggregate_framed_bytes =
            byte_lengths
                .values()
                .flatten()
                .try_fold(0_usize, |total, length| {
                    usize::try_from(*length)
                        .ok()
                        .and_then(|length| total.checked_add(length))
                });
        let aggregate_framed_limit = material_nodes
            .len()
            .checked_mul(MIN_FRAMED_MATERIAL_BYTES)
            .and_then(|minimum_frames| {
                vestrace_application::MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES
                    .checked_mul(2)
                    .and_then(|decoded_allowance| minimum_frames.checked_add(decoded_allowance))
            })
            .filter(|_| material_nodes.len() <= vestrace_application::MAX_EFFECTIVE_REQUEST_ITEMS);
        if byte_lengths
            .values()
            .flatten()
            .any(|length| *length < 0 || *length as usize > MAX_FRAMED_MATERIAL_BYTES)
            || aggregate_framed_bytes
                .zip(aggregate_framed_limit)
                .is_none_or(|(bytes, limit)| bytes > limit)
        {
            let missing = material_nodes
                .iter()
                .map(|node| missing_reference(node.reference_id, None))
                .collect();
            return self
                .append_incomplete(transaction, workspace_id, evidence_id, missing)
                .await;
        }
        let byte_rows: Vec<(uuid::Uuid, Vec<u8>)> = sqlx::query_as(
            "SELECT material_id, ciphertext FROM content_material_bytes
              WHERE workspace_id=$1 AND material_id=ANY($2)
              ORDER BY material_id",
        )
        .bind(workspace_id.as_uuid())
        .bind(&material_ids)
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        let bytes_by_id: HashMap<uuid::Uuid, Vec<u8>> = byte_rows.into_iter().collect();

        let mut missing = Vec::new();
        let mut all_missing_are_expired = true;
        for node in &material_nodes {
            let Some(material) = material_by_id.get(&node.reference_id) else {
                missing.push(missing_reference(node.reference_id, None));
                all_missing_are_expired = false;
                continue;
            };
            if material.state != "live" {
                let erasure = finalized_erasure(transaction, workspace_id, material.id).await?;
                all_missing_are_expired &= erasure.is_some();
                missing.push(missing_reference(
                    material.id,
                    erasure.map(|row| row.preparation_id),
                ));
                continue;
            }
            let Some(frame) = bytes_by_id.get(&material.id) else {
                missing.push(missing_reference(material.id, None));
                all_missing_are_expired = false;
                continue;
            };
            if self.codec.validate_frame(frame).is_err() {
                missing.push(missing_reference(material.id, None));
                all_missing_are_expired = false;
            }
        }
        if !missing.is_empty() {
            if all_missing_are_expired {
                self.append_status(transaction, workspace_id, evidence_id, "expired", &missing)
                    .await?;
                return Ok(ModelRequestReconstruction::Expired);
            }
            return self
                .append_incomplete(transaction, workspace_id, evidence_id, missing)
                .await;
        }

        let mut decoded = Vec::with_capacity(material_nodes.len());
        let mut aggregate_bytes = 0_usize;
        let mut cryptographic_missing = Vec::new();
        for node in material_nodes {
            let material = material_by_id
                .get(&node.reference_id)
                .expect("material preflight retained the exact locked row");
            let frame = bytes_by_id
                .get(&material.id)
                .expect("material preflight retained the exact framed bytes");
            let material_id = ContentMaterialId::from_uuid(material.id);
            let material_key_id = MaterialKeyId::from_uuid(material.material_key_id);
            let mut opened = None;
            let unwrap_result = self.vault.unwrap(material_key_id, &mut |dek| {
                opened =
                    Some(
                        self.codec
                            .open(workspace_id, material_id, material_key_id, dek, frame),
                    );
            });
            match unwrap_result {
                Ok(()) => match opened {
                    Some(Ok(value)) => {
                        aggregate_bytes = aggregate_bytes.saturating_add(value.len());
                        if aggregate_bytes
                            > vestrace_application::MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES
                        {
                            cryptographic_missing.push(missing_reference(material.id, None));
                        } else {
                            decoded.push(value);
                        }
                    }
                    _ => {
                        cryptographic_missing.push(missing_reference(material.id, None));
                    }
                },
                Err(VaultError::Unavailable) => {
                    return Err(ApplicationError::Unavailable(
                        "material vault is unavailable".into(),
                    ));
                }
                Err(VaultError::NotFound | VaultError::Erased | VaultError::ErasurePrepared) => {
                    cryptographic_missing.push(missing_reference(material.id, None));
                }
                Err(_) => {
                    cryptographic_missing.push(missing_reference(material.id, None));
                }
            }
        }

        if !cryptographic_missing.is_empty() {
            return self
                .append_incomplete(
                    transaction,
                    workspace_id,
                    evidence_id,
                    cryptographic_missing,
                )
                .await;
        }
        let request = match build_effective_request(&root, sources, decoded) {
            Ok(request) => request,
            Err(error) if is_incomplete_error(&error) => {
                let missing = source_failure_references(evidence_id, &nodes);
                return self
                    .append_incomplete(transaction, workspace_id, evidence_id, missing)
                    .await;
            }
            Err(error) => return Err(error),
        };
        self.append_status(transaction, workspace_id, evidence_id, "complete", &[])
            .await?;
        Ok(ModelRequestReconstruction::Complete(request))
    }

    async fn reconstruct_q1_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        workspace_id: WorkspaceId,
        evidence_id: ModelRequestEvidenceId,
    ) -> Result<Q1ProbeRequest, ApplicationError> {
        Self::reconstruct_q1_in(self, unit_of_work, workspace_id, evidence_id).await
    }
}

impl PgModelRequestEvidenceRepository {
    async fn append_incomplete(
        &self,
        transaction: &mut PgScopedTransaction,
        workspace_id: WorkspaceId,
        evidence_id: ModelRequestEvidenceId,
        missing: Vec<MissingModelRequestReference>,
    ) -> Result<ModelRequestReconstruction, ApplicationError> {
        self.append_status(
            transaction,
            workspace_id,
            evidence_id,
            "incomplete",
            &missing,
        )
        .await?;
        Ok(ModelRequestReconstruction::Incomplete(missing))
    }

    async fn append_status(
        &self,
        transaction: &mut PgScopedTransaction,
        workspace_id: WorkspaceId,
        evidence_id: ModelRequestEvidenceId,
        status: &str,
        missing: &[MissingModelRequestReference],
    ) -> Result<(), ApplicationError> {
        let kinds: Vec<&str> = missing.iter().map(|item| item.kind.as_str()).collect();
        let ids: Vec<uuid::Uuid> = missing.iter().map(|item| item.reference_id).collect();
        let preparations: Vec<Option<uuid::Uuid>> = if status == "expired" {
            missing
                .iter()
                .map(|item| item.erasure_preparation_id)
                .collect()
        } else {
            vec![None; missing.len()]
        };
        let mut tombstones = Vec::with_capacity(missing.len());
        for item in missing {
            let tombstone = match (status, item.erasure_preparation_id) {
                ("expired", Some(preparation_id)) => sqlx::query_scalar::<_, uuid::Uuid>(
                    "SELECT id FROM material_erasure_audit_tombstones
                      WHERE workspace_id=$1 AND preparation_id=$2",
                )
                .bind(workspace_id.as_uuid())
                .bind(preparation_id)
                .fetch_optional(transaction.connection())
                .await
                .map_err(storage_error)?,
                _ => None,
            };
            tombstones.push(tombstone);
        }
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT vestrace_append_model_request_evidence_check(
                $1,$2,$3,$4,$5,$6,$7,$8
            )",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(workspace_id.as_uuid())
        .bind(evidence_id.as_uuid())
        .bind(status)
        .bind(kinds)
        .bind(ids)
        .bind(preparations)
        .bind(tombstones)
        .fetch_one(transaction.connection())
        .await
        .map_err(map_database_error)?;
        Ok(())
    }
}

fn transaction(
    unit_of_work: &mut dyn UnitOfWork,
) -> Result<&mut PgScopedTransaction, ApplicationError> {
    unit_of_work
        .as_any_mut()
        .downcast_mut::<PgScopedTransaction>()
        .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".into()))
}

async fn lock_cause(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    root: &RootRow,
) -> Result<(), ApplicationError> {
    match (
        root.binding_snapshot_id,
        root.qualification_target_binding_id,
    ) {
        (Some(snapshot_id), None) => {
            sqlx::query(
                "SELECT id FROM model_binding_snapshots
                  WHERE workspace_id=$1 AND id=$2",
            )
            .bind(workspace_id.as_uuid())
            .bind(snapshot_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(source_database_error)?;
        }
        (None, Some(binding_id)) => {
            sqlx::query(
                "SELECT id FROM qualification_target_bindings
                  WHERE workspace_id=$1 AND id=$2",
            )
            .bind(workspace_id.as_uuid())
            .bind(binding_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(source_database_error)?;
        }
        _ => {
            return Err(ApplicationError::Policy(
                "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
            ));
        }
    }
    Ok(())
}

async fn lock_and_load_sources(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    root: &RootRow,
    nodes: &[NodeRow],
) -> Result<HydratedSources, ApplicationError> {
    for node in nodes {
        let table = match node.reference_kind.as_str() {
            "connection_revision" => Some("connection_revisions"),
            "connection_qualification_revision" => Some("connection_qualification_revisions"),
            "model_revision" => Some("model_revisions"),
            "model_qualification_revision" => Some("model_qualification_revisions"),
            "binding_snapshot" => Some("model_binding_snapshots"),
            "qualification_target" => Some("qualification_target_bindings"),
            "external_effect" => Some("external_effect_intents"),
            "request_shape_revision" => Some("model_request_shape_revisions"),
            "sampling_revision" => Some("model_sampling_revisions"),
            "limits_revision" => Some("model_limits_revisions"),
            "tool_schema_revision" => Some("model_tool_schema_revisions"),
            "qualification_probe" | "governed_input_material" => None,
            _ => {
                return Err(ApplicationError::Policy(
                    "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
                ));
            }
        };
        if let Some(table) = table {
            let query = format!("SELECT id FROM {table} WHERE workspace_id=$1 AND id=$2");
            sqlx::query(&query)
                .bind(workspace_id.as_uuid())
                .bind(node.reference_id)
                .fetch_one(transaction.connection())
                .await
                .map_err(source_database_error)?;
        }
    }
    let shape_node = one_node(nodes, "request_shape_revision")?;
    let shape = sqlx::query_as::<_, RequestShapeRow>(
        "SELECT stream, input_roles FROM model_request_shape_revisions
          WHERE workspace_id=$1 AND id=$2 AND version=$3",
    )
    .bind(workspace_id.as_uuid())
    .bind(shape_node.reference_id)
    .bind(shape_node.reference_version)
    .fetch_one(transaction.connection())
    .await
    .map_err(source_database_error)?;
    let limits_node = one_node(nodes, "limits_revision")?;
    let limits = sqlx::query_as::<_, LimitsRow>(
        "SELECT max_output_tokens, max_inputs, max_input_bytes FROM model_limits_revisions
          WHERE workspace_id=$1 AND id=$2 AND version=$3",
    )
    .bind(workspace_id.as_uuid())
    .bind(limits_node.reference_id)
    .bind(limits_node.reference_version)
    .fetch_one(transaction.connection())
    .await
    .map_err(source_database_error)?;
    let sampling = match optional_one_node(nodes, "sampling_revision")? {
        Some(node) => Some(
            sqlx::query_as::<_, SamplingRow>(
                "SELECT temperature, top_p FROM model_sampling_revisions
                  WHERE workspace_id=$1 AND id=$2 AND version=$3",
            )
            .bind(workspace_id.as_uuid())
            .bind(node.reference_id)
            .bind(node.reference_version)
            .fetch_one(transaction.connection())
            .await
            .map_err(source_database_error)?,
        ),
        None => None,
    };
    let wire_model_id = match optional_one_node(nodes, "model_revision")? {
        Some(node) => Some(
            sqlx::query_scalar(
                "SELECT wire_model_id FROM model_revisions
                  WHERE workspace_id=$1 AND id=$2",
            )
            .bind(workspace_id.as_uuid())
            .bind(node.reference_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(source_database_error)?,
        ),
        None => None,
    };
    let tool_nodes: Vec<&NodeRow> = nodes
        .iter()
        .filter(|node| node.reference_kind == "tool_schema_revision")
        .collect();
    let tool_ids: Vec<uuid::Uuid> = tool_nodes.iter().map(|node| node.reference_id).collect();
    let (tool_count, tool_bytes): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(
                    pg_column_size(parameters_schema)
                    + octet_length(tool_name)
                    + COALESCE(octet_length(description), 0)
                ), 0)
           FROM model_tool_schema_revisions
          WHERE workspace_id=$1 AND id=ANY($2)",
    )
    .bind(workspace_id.as_uuid())
    .bind(&tool_ids)
    .fetch_one(transaction.connection())
    .await
    .map_err(storage_error)?;
    if tool_count != tool_nodes.len() as i64
        || usize::try_from(tool_bytes)
            .ok()
            .is_none_or(|bytes| bytes > vestrace_application::MAX_AGGREGATE_EFFECTIVE_REQUEST_BYTES)
    {
        return Err(incomplete_error());
    }
    let mut tools = Vec::with_capacity(tool_nodes.len());
    for node in tool_nodes {
        tools.push(
            sqlx::query_as::<_, ToolRow>(
                "SELECT tool_name, description, parameters_schema
                   FROM model_tool_schema_revisions
                  WHERE workspace_id=$1 AND id=$2 AND version=$3",
            )
            .bind(workspace_id.as_uuid())
            .bind(node.reference_id)
            .bind(node.reference_version)
            .fetch_one(transaction.connection())
            .await
            .map_err(source_database_error)?,
        );
    }
    if root.request_kind == "chat_completions" && sampling.is_none() {
        return Err(ApplicationError::Policy(
            "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
        ));
    }
    Ok(HydratedSources {
        shape,
        limits,
        sampling,
        wire_model_id,
        tools,
    })
}

async fn missing_retained_sources(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    nodes: &[NodeRow],
) -> Result<Vec<MissingModelRequestReference>, ApplicationError> {
    let mut missing = Vec::new();
    for node in nodes {
        let table = match node.reference_kind.as_str() {
            "connection_revision" => Some("connection_revisions"),
            "connection_qualification_revision" => Some("connection_qualification_revisions"),
            "model_revision" => Some("model_revisions"),
            "model_qualification_revision" => Some("model_qualification_revisions"),
            "binding_snapshot" => Some("model_binding_snapshots"),
            "qualification_target" => Some("qualification_target_bindings"),
            "external_effect" => Some("external_effect_intents"),
            "request_shape_revision" => Some("model_request_shape_revisions"),
            "sampling_revision" => Some("model_sampling_revisions"),
            "limits_revision" => Some("model_limits_revisions"),
            "tool_schema_revision" => Some("model_tool_schema_revisions"),
            "qualification_probe" | "governed_input_material" => None,
            _ => return Err(incomplete_error()),
        };
        let Some(table) = table else {
            continue;
        };
        let versioned = matches!(
            node.reference_kind.as_str(),
            "request_shape_revision"
                | "sampling_revision"
                | "limits_revision"
                | "tool_schema_revision"
        );
        let exists: bool = if versioned {
            sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table}
                  WHERE workspace_id=$1 AND id=$2 AND version=$3)"
            ))
            .bind(workspace_id.as_uuid())
            .bind(node.reference_id)
            .bind(node.reference_version)
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?
        } else {
            sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table} WHERE workspace_id=$1 AND id=$2)"
            ))
            .bind(workspace_id.as_uuid())
            .bind(node.reference_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?
        };
        if !exists {
            let kind = parse_node_kind(&node.reference_kind).ok_or_else(incomplete_error)?;
            missing.push(missing_node_reference(kind, node.reference_id));
        }
    }
    Ok(missing)
}

async fn lock_materials(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    material_ids: &[uuid::Uuid],
) -> Result<Vec<MaterialRow>, ApplicationError> {
    sqlx::query_as::<_, MaterialRow>(
        "SELECT id, intent_id, material_key_id, state FROM content_materials
          WHERE workspace_id=$1 AND id=ANY($2) ORDER BY id",
    )
    .bind(workspace_id.as_uuid())
    .bind(material_ids)
    .fetch_all(transaction.connection())
    .await
    .map_err(storage_error)
}

async fn lock_material_intents(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    intent_ids: &[uuid::Uuid],
) -> Result<(), ApplicationError> {
    sqlx::query(
        "SELECT id FROM material_key_creation_intents
          WHERE workspace_id=$1 AND id=ANY($2) ORDER BY id",
    )
    .bind(workspace_id.as_uuid())
    .bind(intent_ids)
    .fetch_all(transaction.connection())
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn load_byte_lengths(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    material_ids: &[uuid::Uuid],
) -> Result<HashMap<uuid::Uuid, Option<i32>>, ApplicationError> {
    let rows = sqlx::query(
        "SELECT material.id, octet_length(bytes.ciphertext) AS framed_size
           FROM content_materials AS material
           LEFT JOIN content_material_bytes AS bytes ON bytes.material_id=material.id
          WHERE material.workspace_id=$1 AND material.id=ANY($2)
          ORDER BY material.id",
    )
    .bind(workspace_id.as_uuid())
    .bind(material_ids)
    .fetch_all(transaction.connection())
    .await
    .map_err(storage_error)?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("id"), row.get("framed_size")))
        .collect())
}

async fn finalized_erasure(
    transaction: &mut PgScopedTransaction,
    workspace_id: WorkspaceId,
    material_id: uuid::Uuid,
) -> Result<Option<ErasureRow>, ApplicationError> {
    sqlx::query_as::<_, ErasureRow>(
        "SELECT preparation.id AS preparation_id
           FROM material_erasure_preparations AS preparation
           JOIN material_erasure_audit_tombstones AS tombstone
             ON tombstone.preparation_id=preparation.id
            AND tombstone.workspace_id=preparation.workspace_id
          WHERE preparation.workspace_id=$1
            AND preparation.content_material_id=$2
            AND preparation.finalized_at IS NOT NULL
            AND preparation.erasure_receipt IS NOT NULL",
    )
    .bind(workspace_id.as_uuid())
    .bind(material_id)
    .fetch_optional(transaction.connection())
    .await
    .map_err(storage_error)
}

fn build_effective_request(
    root: &RootRow,
    sources: HydratedSources,
    decoded: Vec<Zeroizing<Vec<u8>>>,
) -> Result<EffectiveModelRequest, ApplicationError> {
    let limits = EffectiveRequestLimits::new(
        to_u32(sources.limits.max_output_tokens)?,
        to_u32(sources.limits.max_inputs)?,
        to_u32(sources.limits.max_input_bytes)?,
    )
    .map_err(provider_error)?;
    match root.request_kind.as_str() {
        "models_list" => Ok(EffectiveModelRequest::models_list()),
        "chat_completions" => {
            let roles = sources
                .shape
                .input_roles
                .iter()
                .map(|role| parse_role(role))
                .collect::<Result<Vec<_>, _>>()?;
            if roles.len() != decoded.len() {
                return Err(incomplete_error());
            }
            let messages = decoded
                .into_iter()
                .zip(roles)
                .map(|(bytes, role)| {
                    EffectiveChatMessage::from_zeroizing_content(role, zeroizing_utf8(bytes)?)
                        .map_err(provider_error)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let sampling = sources.sampling.ok_or_else(|| {
                ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into())
            })?;
            let sampling = EffectiveSampling::new(sampling.temperature, sampling.top_p)
                .map_err(provider_error)?;
            let tools = sources
                .tools
                .into_iter()
                .map(|tool| {
                    EffectiveToolSchema::new(
                        tool.tool_name,
                        tool.description,
                        tool.parameters_schema,
                    )
                    .map_err(provider_error)
                })
                .collect::<Result<Vec<_>, _>>()?;
            EffectiveModelRequest::chat_completions(
                sources.wire_model_id.ok_or_else(|| {
                    ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into())
                })?,
                messages,
                sampling,
                limits,
                tools,
                sources.shape.stream,
            )
            .map_err(provider_error)
        }
        "embeddings" => {
            let inputs = decoded
                .into_iter()
                .map(zeroizing_utf8)
                .collect::<Result<Vec<_>, _>>()?;
            EffectiveModelRequest::embeddings_from_zeroizing_inputs(
                sources.wire_model_id.ok_or_else(|| {
                    ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into())
                })?,
                inputs,
                limits,
            )
            .map_err(provider_error)
        }
        _ => Err(ApplicationError::Policy(
            "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
        )),
    }
}

fn zeroizing_utf8(mut bytes: Zeroizing<Vec<u8>>) -> Result<Zeroizing<String>, ApplicationError> {
    match String::from_utf8(std::mem::take(&mut *bytes)) {
        Ok(value) => Ok(Zeroizing::new(value)),
        Err(error) => {
            let invalid_bytes = Zeroizing::new(error.into_bytes());
            drop(invalid_bytes);
            Err(ApplicationError::Policy(
                "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
            ))
        }
    }
}

fn parse_role(value: &str) -> Result<EffectiveChatRole, ApplicationError> {
    match value {
        "system" => Ok(EffectiveChatRole::System),
        "user" => Ok(EffectiveChatRole::User),
        "assistant" => Ok(EffectiveChatRole::Assistant),
        "tool" => Ok(EffectiveChatRole::Tool),
        _ => Err(ApplicationError::Policy(
            "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
        )),
    }
}

fn one_node<'a>(nodes: &'a [NodeRow], kind: &str) -> Result<&'a NodeRow, ApplicationError> {
    optional_one_node(nodes, kind)?
        .ok_or_else(|| ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into()))
}

fn optional_one_node<'a>(
    nodes: &'a [NodeRow],
    kind: &str,
) -> Result<Option<&'a NodeRow>, ApplicationError> {
    let mut matching = nodes.iter().filter(|node| node.reference_kind == kind);
    let first = matching.next();
    if matching.next().is_some() {
        return Err(ApplicationError::Policy(
            "MODEL_REQUEST_EVIDENCE_INCOMPLETE".into(),
        ));
    }
    Ok(first)
}

fn ordered_node_ordinals_are_valid(nodes: &[NodeRow], kind: &str) -> bool {
    nodes
        .iter()
        .filter(|node| node.reference_kind == kind)
        .enumerate()
        .all(|(expected, node)| {
            node.safe_ordinal
                .as_deref()
                .and_then(|value| value.parse::<usize>().ok())
                == Some(expected)
        })
}

fn reconstruction_node_matrix_is_valid(root: &RootRow, nodes: &[NodeRow]) -> bool {
    if nodes.iter().any(|node| {
        let Some(kind) = parse_node_kind(&node.reference_kind) else {
            return true;
        };
        let versioned = matches!(
            kind,
            ModelRequestNodeKind::RequestShapeRevision
                | ModelRequestNodeKind::SamplingRevision
                | ModelRequestNodeKind::LimitsRevision
                | ModelRequestNodeKind::ToolSchemaRevision
        );
        let version_is_valid = if versioned {
            node.reference_version.is_some_and(|version| version >= 1)
        } else {
            node.reference_version.is_none()
        };
        let safe_ordinal_is_valid = match kind {
            ModelRequestNodeKind::GovernedInputMaterial
            | ModelRequestNodeKind::ToolSchemaRevision => node.safe_ordinal.is_some(),
            ModelRequestNodeKind::QualificationProbe => {
                node.safe_ordinal.as_deref().is_some_and(|ordinal| {
                    matches!(
                        ordinal,
                        "00" | "10"
                            | "15"
                            | "20"
                            | "30"
                            | "35"
                            | "40"
                            | "50"
                            | "60"
                            | "70"
                            | "80"
                            | "90"
                    )
                })
            }
            _ => node.safe_ordinal.is_none(),
        };
        !version_is_valid || !safe_ordinal_is_valid
    }) {
        return false;
    }
    if node_count(nodes, "external_effect") != 1
        || one_node_with_id(nodes, "external_effect", root.external_effect_id).is_none()
        || node_count(nodes, "connection_revision") != 1
        || node_count(nodes, "request_shape_revision") != 1
        || node_count(nodes, "limits_revision") != 1
    {
        return false;
    }
    match root.request_kind.as_str() {
        "models_list" => {
            if root.cause_kind == "run_step"
                || node_count(nodes, "model_revision") != 0
                || node_count(nodes, "sampling_revision") != 0
                || node_count(nodes, "tool_schema_revision") != 0
                || node_count(nodes, "governed_input_material") != 0
            {
                return false;
            }
        }
        "chat_completions" => {
            if node_count(nodes, "model_revision") != 1
                || node_count(nodes, "sampling_revision") != 1
                || node_count(nodes, "governed_input_material") == 0
            {
                return false;
            }
        }
        "embeddings" => {
            if node_count(nodes, "model_revision") != 1
                || node_count(nodes, "governed_input_material") == 0
                || node_count(nodes, "sampling_revision") != 0
                || node_count(nodes, "tool_schema_revision") != 0
            {
                return false;
            }
        }
        _ => return false,
    }
    match root.cause_kind.as_str() {
        // An embedding job is bound exactly as a run step is -- one binding
        // snapshot, no qualification target -- and differs only in carrying no
        // sampling revision, which the request-kind arm above already enforced.
        "embedding_job" => {
            let Some(snapshot_id) = root.binding_snapshot_id else {
                return false;
            };
            root.qualification_target_binding_id.is_none()
                && one_node_with_id(nodes, "binding_snapshot", snapshot_id).is_some()
                && node_count(nodes, "binding_snapshot") == 1
                && node_count(nodes, "connection_qualification_revision") == 1
                && node_count(nodes, "model_qualification_revision") == 1
                && node_count(nodes, "qualification_target") == 0
                && node_count(nodes, "qualification_probe") == 0
        }
        "run_step" => {
            let Some(snapshot_id) = root.binding_snapshot_id else {
                return false;
            };
            root.qualification_target_binding_id.is_none()
                && one_node_with_id(nodes, "binding_snapshot", snapshot_id).is_some()
                && node_count(nodes, "binding_snapshot") == 1
                && node_count(nodes, "connection_qualification_revision") == 1
                && node_count(nodes, "model_qualification_revision") == 1
                && node_count(nodes, "qualification_target") == 0
                && node_count(nodes, "qualification_probe") == 0
        }
        "qualification_probe" => {
            let Some(target_id) = root.qualification_target_binding_id else {
                return false;
            };
            root.binding_snapshot_id.is_none()
                && one_node_with_id(nodes, "qualification_target", target_id).is_some()
                && one_node_with_id(nodes, "qualification_probe", root.cause_id).is_some()
                && node_count(nodes, "qualification_target") == 1
                && node_count(nodes, "qualification_probe") == 1
                && node_count(nodes, "binding_snapshot") == 0
                && node_count(nodes, "connection_qualification_revision") == 0
                && node_count(nodes, "model_qualification_revision") == 0
        }
        _ => false,
    }
}

fn node_count(nodes: &[NodeRow], kind: &str) -> usize {
    nodes
        .iter()
        .filter(|node| node.reference_kind == kind)
        .count()
}

fn one_node_with_id<'a>(nodes: &'a [NodeRow], kind: &str, id: uuid::Uuid) -> Option<&'a NodeRow> {
    nodes
        .iter()
        .find(|node| node.reference_kind == kind && node.reference_id == id)
}

fn parse_node_kind(value: &str) -> Option<ModelRequestNodeKind> {
    match value {
        "connection_revision" => Some(ModelRequestNodeKind::ConnectionRevision),
        "connection_qualification_revision" => {
            Some(ModelRequestNodeKind::ConnectionQualificationRevision)
        }
        "model_revision" => Some(ModelRequestNodeKind::ModelRevision),
        "model_qualification_revision" => Some(ModelRequestNodeKind::ModelQualificationRevision),
        "binding_snapshot" => Some(ModelRequestNodeKind::BindingSnapshot),
        "qualification_target" => Some(ModelRequestNodeKind::QualificationTarget),
        "external_effect" => Some(ModelRequestNodeKind::ExternalEffect),
        "qualification_probe" => Some(ModelRequestNodeKind::QualificationProbe),
        "governed_input_material" => Some(ModelRequestNodeKind::GovernedInputMaterial),
        "tool_schema_revision" => Some(ModelRequestNodeKind::ToolSchemaRevision),
        "sampling_revision" => Some(ModelRequestNodeKind::SamplingRevision),
        "limits_revision" => Some(ModelRequestNodeKind::LimitsRevision),
        "request_shape_revision" => Some(ModelRequestNodeKind::RequestShapeRevision),
        _ => None,
    }
}

fn matrix_failure_reference(
    root: &RootRow,
    evidence_id: ModelRequestEvidenceId,
    nodes: &[NodeRow],
) -> MissingModelRequestReference {
    let required = [
        (
            ModelRequestNodeKind::ExternalEffect,
            root.external_effect_id,
        ),
        (
            ModelRequestNodeKind::ConnectionRevision,
            evidence_id.as_uuid(),
        ),
        (
            ModelRequestNodeKind::RequestShapeRevision,
            evidence_id.as_uuid(),
        ),
        (ModelRequestNodeKind::LimitsRevision, evidence_id.as_uuid()),
    ];
    for (kind, fallback_id) in required {
        if node_count(nodes, kind.as_str()) != 1 {
            return existing_node_reference(nodes, kind)
                .unwrap_or_else(|| missing_node_reference(kind, fallback_id));
        }
    }
    if root.request_kind == "chat_completions" && node_count(nodes, "sampling_revision") != 1 {
        return existing_node_reference(nodes, ModelRequestNodeKind::SamplingRevision)
            .unwrap_or_else(|| {
                missing_node_reference(
                    ModelRequestNodeKind::SamplingRevision,
                    evidence_id.as_uuid(),
                )
            });
    }
    if matches!(
        root.request_kind.as_str(),
        "chat_completions" | "embeddings"
    ) && node_count(nodes, "governed_input_material") == 0
    {
        return existing_node_reference(nodes, ModelRequestNodeKind::GovernedInputMaterial)
            .unwrap_or_else(|| {
                missing_node_reference(
                    ModelRequestNodeKind::GovernedInputMaterial,
                    evidence_id.as_uuid(),
                )
            });
    }
    if let Some(node) = nodes.iter().find(|node| {
        parse_node_kind(&node.reference_kind).is_none()
            || node.reference_version.is_some_and(|version| version < 1)
    }) {
        return missing_node_reference(
            parse_node_kind(&node.reference_kind)
                .unwrap_or(ModelRequestNodeKind::RequestShapeRevision),
            node.reference_id,
        );
    }
    existing_node_reference(nodes, ModelRequestNodeKind::RequestShapeRevision).unwrap_or_else(
        || {
            missing_node_reference(
                ModelRequestNodeKind::RequestShapeRevision,
                evidence_id.as_uuid(),
            )
        },
    )
}

fn existing_node_reference(
    nodes: &[NodeRow],
    preferred: ModelRequestNodeKind,
) -> Option<MissingModelRequestReference> {
    nodes
        .iter()
        .find(|node| node.reference_kind == preferred.as_str())
        .or_else(|| {
            nodes
                .iter()
                .find(|node| node.reference_kind == "request_shape_revision")
        })
        .or_else(|| {
            nodes
                .iter()
                .find(|node| parse_node_kind(&node.reference_kind).is_some())
        })
        .and_then(|node| {
            parse_node_kind(&node.reference_kind)
                .map(|kind| missing_node_reference(kind, node.reference_id))
        })
}

fn missing_cause_reference(
    root: &RootRow,
    evidence_id: ModelRequestEvidenceId,
) -> MissingModelRequestReference {
    match (
        root.binding_snapshot_id,
        root.qualification_target_binding_id,
    ) {
        (Some(id), None) => missing_node_reference(ModelRequestNodeKind::BindingSnapshot, id),
        (None, Some(id)) => missing_node_reference(ModelRequestNodeKind::QualificationTarget, id),
        _ => missing_node_reference(
            ModelRequestNodeKind::RequestShapeRevision,
            evidence_id.as_uuid(),
        ),
    }
}

fn source_failure_references(
    evidence_id: ModelRequestEvidenceId,
    nodes: &[NodeRow],
) -> Vec<MissingModelRequestReference> {
    let mut missing: Vec<_> = nodes
        .iter()
        .filter_map(|node| {
            let kind = parse_node_kind(&node.reference_kind)?;
            matches!(
                kind,
                ModelRequestNodeKind::RequestShapeRevision
                    | ModelRequestNodeKind::SamplingRevision
                    | ModelRequestNodeKind::LimitsRevision
                    | ModelRequestNodeKind::ToolSchemaRevision
                    | ModelRequestNodeKind::ModelRevision
                    | ModelRequestNodeKind::GovernedInputMaterial
            )
            .then(|| missing_node_reference(kind, node.reference_id))
        })
        .collect();
    if missing.is_empty() {
        missing.push(
            existing_node_reference(nodes, ModelRequestNodeKind::RequestShapeRevision)
                .unwrap_or_else(|| {
                    missing_node_reference(
                        ModelRequestNodeKind::RequestShapeRevision,
                        evidence_id.as_uuid(),
                    )
                }),
        );
    }
    missing
}

fn missing_node_reference(
    kind: ModelRequestNodeKind,
    reference_id: uuid::Uuid,
) -> MissingModelRequestReference {
    MissingModelRequestReference {
        kind,
        reference_id,
        erasure_preparation_id: None,
    }
}

fn missing_reference(
    reference_id: uuid::Uuid,
    erasure_preparation_id: Option<uuid::Uuid>,
) -> MissingModelRequestReference {
    MissingModelRequestReference {
        kind: ModelRequestNodeKind::GovernedInputMaterial,
        reference_id,
        erasure_preparation_id,
    }
}

fn incomplete_error() -> ApplicationError {
    ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into())
}

fn is_incomplete_error(error: &ApplicationError) -> bool {
    matches!(error, ApplicationError::Policy(message) if message == "MODEL_REQUEST_EVIDENCE_INCOMPLETE")
}

fn source_database_error(error: sqlx::Error) -> ApplicationError {
    if matches!(error, sqlx::Error::RowNotFound) {
        incomplete_error()
    } else {
        storage_error(error)
    }
}

fn reconstruction_lock_error(error: sqlx::Error) -> ApplicationError {
    match error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
    {
        Some("23514") => incomplete_error(),
        Some("42501") => {
            ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_RECONSTRUCTION_LOCK_REFUSED".into())
        }
        _ => storage_error(error),
    }
}

fn q1_message_layout(value: Q1SafeMessageLayout) -> &'static str {
    match value {
        Q1SafeMessageLayout::PlainText => "plain_text",
        Q1SafeMessageLayout::MultipartImageMarker => "multipart_image_marker",
        Q1SafeMessageLayout::AssistantToolCallReplay => "assistant_tool_call_replay",
    }
}

fn q1_message_layout_from_sql(value: &str) -> Result<Q1SafeMessageLayout, ApplicationError> {
    match value {
        "plain_text" => Ok(Q1SafeMessageLayout::PlainText),
        "multipart_image_marker" => Ok(Q1SafeMessageLayout::MultipartImageMarker),
        "assistant_tool_call_replay" => Ok(Q1SafeMessageLayout::AssistantToolCallReplay),
        _ => Err(q1_mre_incomplete()),
    }
}

fn q1_tool_choice(value: Q1SafeToolChoice) -> &'static str {
    match value {
        Q1SafeToolChoice::None => "none",
        Q1SafeToolChoice::NamedProbe => "named_probe",
        Q1SafeToolChoice::Required => "required",
    }
}

fn q1_tool_choice_from_sql(value: &str) -> Result<Q1SafeToolChoice, ApplicationError> {
    match value {
        "none" => Ok(Q1SafeToolChoice::None),
        "named_probe" => Ok(Q1SafeToolChoice::NamedProbe),
        "required" => Ok(Q1SafeToolChoice::Required),
        _ => Err(q1_mre_incomplete()),
    }
}

fn q1_response_format(value: Q1SafeResponseFormat) -> &'static str {
    match value {
        Q1SafeResponseFormat::None => "none",
        Q1SafeResponseFormat::StrictNonceJsonSchema => "strict_nonce_json_schema",
    }
}

fn q1_response_format_from_sql(value: &str) -> Result<Q1SafeResponseFormat, ApplicationError> {
    match value {
        "none" => Ok(Q1SafeResponseFormat::None),
        "strict_nonce_json_schema" => Ok(Q1SafeResponseFormat::StrictNonceJsonSchema),
        _ => Err(q1_mre_incomplete()),
    }
}

fn q1_mre_incomplete() -> ApplicationError {
    ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into())
}

fn to_i64(value: u64) -> Result<i64, ApplicationError> {
    i64::try_from(value).map_err(|_| {
        ApplicationError::Policy("model request evidence version is out of range".into())
    })
}

fn to_i32(value: u32) -> Result<i32, ApplicationError> {
    i32::try_from(value).map_err(|_| {
        ApplicationError::Policy("model request evidence limit is out of range".into())
    })
}

fn to_u32(value: i32) -> Result<u32, ApplicationError> {
    u32::try_from(value)
        .map_err(|_| ApplicationError::Policy("MODEL_REQUEST_EVIDENCE_INCOMPLETE".into()))
}

fn provider_error(_error: vestrace_application::ProviderError) -> ApplicationError {
    incomplete_error()
}

fn map_database_error(error: sqlx::Error) -> ApplicationError {
    if error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
        == Some("40001")
    {
        ApplicationError::Conflict("MODEL_REQUEST_EVIDENCE_CONFLICT".into())
    } else {
        storage_error(error)
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
