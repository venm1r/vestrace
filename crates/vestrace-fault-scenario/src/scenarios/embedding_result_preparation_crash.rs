//! A process-abort proof for delivery result preparation.
//!
//! The child performs one loopback provider call, seals the governed response,
//! commits the ResultPrepared tuple, announces the durable identities, then
//! aborts before it can return. The parent reads every asserted fact back from
//! PostgreSQL and asks the normal embedding recovery authority for its phase.

use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::{future::Future, pin::Pin, time::Duration};

use sqlx::{
    PgPool,
    types::chrono::{DateTime, Utc},
};
use uuid::Uuid;
use vestrace_application::{
    AcceptDeliveryOutputs, AcceptEmbeddingJob, ApplicationError, DeliveryOutputIdentity,
    EmbeddingDataPolicyDecisionRecord, EmbeddingDataPolicyDecisionRepository,
    EmbeddingDataPolicyMode, EmbeddingOutputKeyProgress, EmbeddingOutputKeyRepository,
    EmbeddingOutputKeyService, EmbeddingResultDispatchAuthority, EmbeddingResultPreparationId,
    EmbeddingResultPreparationIdentities, EmbeddingResultPreparationOutcome,
    EmbeddingResultPreparationService, GovernedEmbeddingVector, GovernedEmbeddingsResponse,
    IdempotencyRecord, OutboxMessage, ProviderDispatchAuthority, ProviderDispatchOutcome,
    ProviderDispatchRepository, ProviderDispatchRequest, ProviderLostDispatchRecovery,
    ProviderPostNetworkCompletion, RequestContext, UnitOfWork,
};
use vestrace_domain::embedding::EmbeddingJobKind;
use vestrace_domain::id::{AuditEventId, OutboxId, PolicyDecisionId};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    AuditEvent, ContentMaterialId, DataDestination, EmbeddingJobId, EmbeddingSpaceId,
    ExternalEffectId, ExternalEffectLifecycleTransitionId, ExternalEffectReceipt,
    ExternalEffectReceiptId, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyId,
    ModelRequestEvidenceId, PreparedMaterialAttachmentId, PrincipalId, Sensitivity, WorkspaceId,
};
use vestrace_infrastructure::crypto::{
    ContentMaterialCodec, HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER,
};
use vestrace_infrastructure::postgres::{
    PgEmbeddingDataPolicyDecisionRepository, PgEmbeddingOutputKeyRepository,
    PgEmbeddingResultRepository, PgScopedTransaction, PgStore,
};

use crate::{ScenarioSettings, embedding_dispatch_crash as dispatch, intent_support};

const MARKER: &str = "vestrace-fault-scenario: embedding-result-preparation";
const BOOTSTRAP_KEY_ID: &str = "material-vault-bootstrap";
const BOOTSTRAP_SCOPE: &str = "material-vault-bootstrap";
const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";
const RESPONSE_MODEL: &str = "text-embedding-nomic-embed-text-v1.5";
const RESPONSE_DIMENSIONS: usize = 768;

type PersistedResultPreparationCounts = (
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    Option<String>,
    i64,
    i64,
);

/// The result repository needs only this embedding-specific completion lock.
/// It invokes the same guarded SQL routine as the production adapter; the other
/// dispatch methods deliberately remain unavailable in this focused scenario.
/// Shared with the worker-completion scenario, which drives a second physical
/// job through the same preparation path. It is a stub because no fault
/// scenario dispatches through this port: the provider is reached over the
/// loopback listener instead, where a party outside the process can count it.
pub(super) struct ResultPreparationDispatch;

impl ProviderDispatchRepository for ResultPreparationDispatch {
    fn prepare_dispatch<'life0, 'async_trait>(
        &'life0 self,
        _request: ProviderDispatchRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ProviderDispatchOutcome, ApplicationError>>
                + Send
                + 'async_trait,
        >,
    >
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        Box::pin(async move {
            Err(ApplicationError::Unavailable(
                "fault scenario does not dispatch through this port".into(),
            ))
        })
    }

    fn complete_post_network<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _context: &'life1 RequestContext,
        _completion: ProviderPostNetworkCompletion,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApplicationError>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move {
            Err(ApplicationError::Unavailable(
                "fault scenario does not complete generic dispatch".into(),
            ))
        })
    }

    fn release_after_provider_result_in<'life0, 'life1, 'life2, 'life3, 'life4, 'async_trait>(
        &'life0 self,
        _context: &'life1 RequestContext,
        _unit_of_work: &'life2 mut dyn UnitOfWork,
        _authority: &'life3 ProviderDispatchAuthority,
        _receipt: &'life4 ExternalEffectReceipt,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApplicationError>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
        'life4: 'async_trait,
    {
        Box::pin(async move {
            Err(ApplicationError::Unavailable(
                "result preparation releases through its guarded SQL command".into(),
            ))
        })
    }

    fn lock_embedding_result_completion_authority_in<'life0, 'life1, 'life2, 'life3, 'async_trait>(
        &'life0 self,
        context: &'life1 RequestContext,
        unit_of_work: &'life2 mut dyn UnitOfWork,
        authority: &'life3 ProviderDispatchAuthority,
        job_id: EmbeddingJobId,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApplicationError>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
    {
        Box::pin(async move {
            let transaction = unit_of_work
                .as_any_mut()
                .downcast_mut::<PgScopedTransaction>()
                .ok_or_else(|| {
                    ApplicationError::Internal(
                        "fault scenario requires PostgreSQL transaction".into(),
                    )
                })?;
            sqlx::query_scalar::<_, Uuid>(
                "SELECT vestrace_lock_embedding_result_completion_authority($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(context.workspace_id.as_uuid())
            .bind(job_id.as_uuid())
            .bind(authority.effect_id.as_uuid())
            .bind(authority.connection_id.as_uuid())
            .bind(authority.connection_revision_id.as_uuid())
            .bind(authority.dispatch_transition_id.as_uuid())
            .bind(authority.concurrency_lease_id)
            .fetch_one(transaction.connection())
            .await
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            Ok(())
        })
    }

    fn recover_lost_post_network<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        _context: &'life1 RequestContext,
        _authority: &'life2 ProviderDispatchAuthority,
        _recovered_at: DateTime<Utc>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<ProviderLostDispatchRecovery, ApplicationError>>
                + Send
                + 'async_trait,
        >,
    >
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move {
            Err(ApplicationError::Unavailable(
                "fault scenario does not recover generic dispatch".into(),
            ))
        })
    }
}

pub async fn run_child(settings: &ScenarioSettings) -> ! {
    settings.embedding_result_preparation_point();
    let owner = dispatch::connect_owner(settings)
        .await
        .unwrap_or_else(|error| die(error));
    let runtime = dispatch::connect_runtime(&owner)
        .await
        .unwrap_or_else(|error| die(error));
    let fixture = dispatch::build_result_preparation_fixture(&owner, &runtime)
        .await
        .unwrap_or_else(|error| die(format!("result-preparation fixture build failed: {error}")));
    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    );
    let (vault, outputs) = prepare_dispatch_outputs(&owner, &runtime, &fixture, &context)
        .await
        .unwrap_or_else(|error| die(format!("guarded output setup failed: {error}")));

    // The pre-network authority is durable before the loopback recipient is
    // touched. This is also what makes the later lease release observable.
    dispatch::drive(&runtime, &fixture, None)
        .await
        .unwrap_or_else(|error| die(format!("pre-network dispatch setup failed: {error}")));
    let provider_url = std::env::var("VESTRACE_RESULT_PREPARATION_DISPATCH_URL")
        .unwrap_or_else(|_| die("result-preparation child has no loopback URL"));
    reqwest::Client::new()
        .post(provider_url)
        .body("one governed embedding response")
        .send()
        .await
        .map_err(|error| format!("loopback provider call failed: {error}"))
        .unwrap_or_else(|error| die(error));

    let authority = read_dispatch_authority(&runtime, &fixture)
        .await
        .unwrap_or_else(|error| die(format!("dispatch authority read failed: {error}")));
    let repository = Arc::new(PgEmbeddingResultRepository::new(
        PgStore::from_pool(runtime.clone()),
        Arc::new(ResultPreparationDispatch),
    ));
    let service = EmbeddingResultPreparationService::new(
        repository,
        vault,
        Arc::new(ContentMaterialCodec::new()),
    );
    let preparation_id = EmbeddingResultPreparationId::new();
    let receipt_id = ExternalEffectReceiptId::new();
    let response = GovernedEmbeddingsResponse::new(
        RESPONSE_MODEL,
        RESPONSE_MODEL.into(),
        outputs
            .iter()
            .enumerate()
            .map(|(ordinal, _)| {
                GovernedEmbeddingVector::from_provider_components(
                    ordinal,
                    vec![0.25_f32; RESPONSE_DIMENSIONS],
                )
                .unwrap_or_else(|error| die(format!("result response is invalid: {error}")))
            })
            .collect(),
        outputs.len(),
    )
    .unwrap_or_else(|error| die(format!("result response is invalid: {error}")));
    let outcome = service
        .prepare(
            context,
            EmbeddingResultDispatchAuthority {
                job_id: EmbeddingJobId::from_uuid(fixture.job_id),
                effect_id: ExternalEffectId::from_uuid(fixture.effect_id),
                dispatch: authority,
            },
            EmbeddingResultPreparationIdentities {
                preparation_id,
                receipt_id,
                attachments: outputs
                    .iter()
                    .map(
                        |output| vestrace_application::EmbeddingResultPreparedAttachment {
                            output_ordinal: output.output_ordinal,
                            intent_id: output.intent_id,
                            attachment_id: PreparedMaterialAttachmentId::new(),
                        },
                    )
                    .collect(),
            },
            response,
        )
        .await
        .unwrap_or_else(|error| die(format!("result preparation was refused: {error}")));
    if outcome != (EmbeddingResultPreparationOutcome::Prepared { preparation_id }) {
        die("result preparation did not create this child's marker");
    }
    announce(&fixture, preparation_id, receipt_id);
    // A panic or injected error would retain process-local state. Only an abort
    // proves that the parent can recover from the exact durable commit alone.
    std::process::abort();
}

pub async fn run_parent(settings: &ScenarioSettings) -> Result<String, String> {
    settings.embedding_result_preparation_point();
    let listener = LoopbackCounter::start().await?;
    let crashed = run_same_child(settings, listener.url()).await?;
    if crashed.status.success() {
        return Err("result-preparation child exited successfully instead of aborting".into());
    }
    let marker = parse_marker(&String::from_utf8_lossy(&crashed.stderr))?;
    let owner = dispatch::connect_owner(settings).await?;
    let persisted = read_persisted(&owner, &marker).await?;
    let runtime = dispatch::connect_runtime(&owner).await?;
    let recovery_phase = recover(&runtime, &marker).await?;
    if recovery_phase != "result_prepared" {
        return Err(format!(
            "recovery did not derive result_prepared: {recovery_phase}"
        ));
    }
    if listener.count() != 1 {
        return Err(format!(
            "loopback provider was called {} times instead of once",
            listener.count()
        ));
    }
    Ok(serde_json::json!({
        "scenario": "embedding_result_preparation_crash",
        "point": "after_result_prepared_before_return",
        "proved": true,
        "loopback_requests": listener.count(),
        "recovery_phase": recovery_phase,
        "persisted": persisted,
    })
    .to_string())
}

async fn create_live_source(
    runtime: &PgPool,
    fixture: &dispatch::Fixture,
) -> Result<ContentMaterialId, String> {
    let material_id = ContentMaterialId::new();
    let intent_id = MaterialKeyCreationIntentId::new();
    let key_id = MaterialKeyId::new();
    let mut framed_ciphertext = vec![0x51_u8; 4096];
    framed_ciphertext[..5].copy_from_slice(b"VMRF\x01");
    let mut transaction = runtime.begin().await.map_err(dispatch::sql)?;
    dispatch::set_workspace(&mut transaction, fixture.workspace_id).await?;
    sqlx::query("SELECT vestrace_reserve_material_key_creation_intent($1,$2,$3,$4,$5,'model_request_input',$6,0)")
        .bind(intent_id.as_uuid()).bind(fixture.workspace_id).bind(material_id.as_uuid())
        .bind(key_id.as_uuid()).bind(Uuid::now_v7()).bind(fixture.job_id)
        .execute(&mut *transaction).await.map_err(dispatch::sql)?;
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    sqlx::query("SELECT vestrace_prepare_content_material($1,$2,$3,4096)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .bind(framed_ciphertext)
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1,$2)")
        .bind(intent_id.as_uuid())
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(intent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    transaction.commit().await.map_err(dispatch::sql)?;
    Ok(material_id)
}

async fn attach_source_to_evidence(
    owner: &PgPool,
    fixture: &dispatch::Fixture,
    source: ContentMaterialId,
) -> Result<(), String> {
    let mut transaction = owner.begin().await.map_err(dispatch::sql)?;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .map_err(dispatch::sql)?;
    dispatch::set_workspace(&mut transaction, fixture.workspace_id).await?;
    sqlx::query("INSERT INTO model_request_evidence_nodes(id,workspace_id,evidence_root_id,ordinal,reference_kind,reference_id) VALUES($1,$2,$3,8,'governed_input_material',$4)")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(fixture.evidence_id).bind(source.as_uuid())
        .execute(&mut *transaction).await.map_err(dispatch::sql)?;
    transaction.commit().await.map_err(dispatch::sql)
}

/// Shared pre-dispatch fixture: acceptance owns the output identities and real
/// host reconciliation supplies every receipt required by the admission gate.
pub(super) async fn prepare_dispatch_outputs(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: &dispatch::Fixture,
    context: &RequestContext,
) -> Result<(Arc<HostMaterialKeyVault>, Vec<DeliveryOutputIdentity>), String> {
    prepare_dispatch_outputs_of_kind(owner, runtime, fixture, context, EmbeddingJobKind::Delivery)
        .await
}

/// The same preparation for a rebuild. Since migration 0207 the two are not
/// interchangeable: a rebuild is refused unless a transition plan already
/// names its space, so a caller asking for one must have planned first.
pub(super) async fn prepare_dispatch_outputs_of_kind(
    owner: &PgPool,
    runtime: &PgPool,
    fixture: &dispatch::Fixture,
    context: &RequestContext,
    kind: EmbeddingJobKind,
) -> Result<(Arc<HostMaterialKeyVault>, Vec<DeliveryOutputIdentity>), String> {
    let source = create_live_source(runtime, fixture).await?;
    attach_source_to_evidence(owner, fixture, source).await?;
    let (vault, outputs, acceptance_receipt) =
        prepare_outputs(runtime, fixture, context, kind).await?;
    record_allowed_delivery_policy(runtime, acceptance_receipt, outputs.len()).await?;
    Ok((vault, outputs))
}

async fn prepare_outputs(
    runtime: &PgPool,
    fixture: &dispatch::Fixture,
    context: &RequestContext,
    kind: EmbeddingJobKind,
) -> Result<(Arc<HostMaterialKeyVault>, Vec<DeliveryOutputIdentity>, Uuid), String> {
    let outputs = (0..2)
        .map(|output_ordinal| DeliveryOutputIdentity {
            output_ordinal,
            intent_id: MaterialKeyCreationIntentId::new(),
            material_id: ContentMaterialId::new(),
            key_id: MaterialKeyId::new(),
            nonce: IntentNonce::new(),
        })
        .collect::<Vec<_>>();
    let receipt_id = Uuid::now_v7();
    let at = DateTime::<Utc>::from_timestamp(1_800_000_000, 0)
        .ok_or_else(|| "fault acceptance timestamp is invalid".to_owned())?;
    let acceptance = AcceptEmbeddingJob {
        job_id: EmbeddingJobId::from_uuid(fixture.job_id),
        space_registration_id: EmbeddingSpaceId::from_uuid(fixture.space_registration_id),
        kind,
        model_binding_snapshot_id: fixture.snapshot_id,
        intent: fixture
            .intent
            .clone()
            .ok_or_else(|| "child fixture omitted its exact effect intent".to_owned())?,
        model_request_evidence_id: ModelRequestEvidenceId::from_uuid(fixture.evidence_id),
        retries_unknown_embedding_job_id: None,
        expected_predecessor_version: None,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("result-preparation-{receipt_id}"),
            workspace_id: context.workspace_id,
            request_hash: format!("result-preparation:{receipt_id}"),
            response_payload: None,
            status: "completed".into(),
            created_at: at,
            expires_at: at + Duration::from_secs(24 * 60 * 60),
        }),
        outbox: vec![OutboxMessage {
            id: OutboxId::from_uuid(receipt_id),
            workspace_id: context.workspace_id,
            topic: "embedding.job.delivery_accepted".into(),
            payload: serde_json::json!({"receipt_id": receipt_id}),
            created_at: at,
            attempts: 0,
        }],
        audit: AuditEvent::new(
            AuditEventId::from_uuid(receipt_id),
            context.workspace_id,
            context.principal_id,
            "embedding.job.delivery_accepted",
            "embedding_job",
            fixture.job_id,
            serde_json::json!({"receipt_id": receipt_id}),
            at,
        )
        .map_err(|error| error.to_string())?,
    };
    let repository = Arc::new(PgEmbeddingOutputKeyRepository::new(PgStore::from_pool(
        runtime.clone(),
    )));
    repository
        .accept_delivery_outputs(
            context,
            AcceptDeliveryOutputs {
                receipt_id,
                idempotency_key: format!("result-preparation-{receipt_id}"),
                acceptance,
                outputs: outputs.clone(),
            },
        )
        .await
        .map_err(|error| format!("delivery output acceptance failed: {error}"))?;
    let (vault_root, bootstrap_root) = intent_support::vault_roots()?;
    let reference = KeyReference::new(
        MOUNTED_SECRET_STORE_PROVIDER,
        BOOTSTRAP_KEY_ID,
        "v1",
        KeyPurpose::Storage,
        BOOTSTRAP_SCOPE,
        BOOTSTRAP_ALGORITHM,
    )
    .map_err(|error| error.to_string())?;
    let vault = Arc::new(
        HostMaterialKeyVault::new(
            vault_root,
            bootstrap_root,
            reference,
            SecretResolutionRequest::new(
                context.workspace_id,
                BOOTSTRAP_SCOPE,
                "fault://result-preparation",
            ),
        )
        .map_err(|error| error.to_string())?,
    );
    let service = EmbeddingOutputKeyService::new(repository, vault.clone());
    let mut reconciled = 0usize;
    let mut final_prepared = false;
    for _ in 0..=outputs.len() {
        match service
            .reconcile_one(context)
            .await
            .map_err(|error| format!("delivery output key reconciliation failed: {error}"))?
        {
            Some(EmbeddingOutputKeyProgress::WaitingForResultKeys) => reconciled += 1,
            Some(EmbeddingOutputKeyProgress::Prepared { .. }) => {
                reconciled += 1;
                final_prepared = true;
            }
            None => break,
            other => return Err(format!("result output key was not prepared: {other:?}")),
        }
    }
    if reconciled != outputs.len() || !final_prepared {
        return Err(format!(
            "result output keys were incomplete: reconciled {reconciled} of {}, final prepared={final_prepared}",
            outputs.len()
        ));
    }
    Ok((vault, outputs, receipt_id))
}

async fn record_allowed_delivery_policy(
    runtime: &PgPool,
    cause: Uuid,
    input_count: usize,
) -> Result<(), String> {
    PgEmbeddingDataPolicyDecisionRepository::new(PgStore::from_pool(runtime.clone()))
        .record(&EmbeddingDataPolicyDecisionRecord {
            id: Uuid::now_v7(),
            purpose: vestrace_application::EmbeddingPurpose::Delivery,
            causal_reference_id: cause,
            delivery_attempt: Some(1),
            batch_ordinal: None,
            destination: DataDestination::LocalModel,
            classification: Sensitivity::Internal,
            classification_labels: vec!["fault-scenario".into()],
            unclassified_count: 0,
            input_count: u32::try_from(input_count)
                .map_err(|_| "fault output count does not fit policy input count".to_owned())?,
            classification_allowed: true,
            destination_allowed: true,
            allowed: true,
            reason: "ephemeral fault scenario allow".into(),
            policy_version: "fault-result-preparation-v1".into(),
            mode: EmbeddingDataPolicyMode::Enforce,
            decided_at: Utc::now(),
        })
        .await
        .map_err(|error| error.to_string())
}

/// Shared with the worker-completion scenario for the same reason the stub
/// above is: a second copy of this join would eventually disagree with this one
/// and nothing would say which was right.
pub(super) async fn read_dispatch_authority(
    runtime: &PgPool,
    fixture: &dispatch::Fixture,
) -> Result<ProviderDispatchAuthority, String> {
    let mut transaction = runtime.begin().await.map_err(dispatch::sql)?;
    dispatch::set_workspace(&mut transaction, fixture.workspace_id).await?;
    let row: (Uuid, Uuid, Uuid, DateTime<Utc>) = sqlx::query_as(
        "SELECT effect_authorization.id, lease.id, lifecycle.id, lifecycle.dispatch_expires_at \
           FROM external_effect_authorizations effect_authorization \
           JOIN connection_dispatch_admissions admission \
             ON admission.workspace_id=effect_authorization.workspace_id AND admission.external_effect_id=effect_authorization.effect_id \
           JOIN provider_concurrency_leases lease \
             ON lease.workspace_id=admission.workspace_id AND lease.external_effect_id=admission.external_effect_id \
            AND lease.id=admission.requested_concurrency_lease_id AND lease.released_at IS NULL \
           JOIN external_effect_lifecycle_transitions lifecycle \
             ON lifecycle.workspace_id=effect_authorization.workspace_id AND lifecycle.effect_id=effect_authorization.effect_id \
          WHERE effect_authorization.workspace_id=$1 AND effect_authorization.effect_id=$2 \
            AND admission.decision='admitted' AND lifecycle.status='dispatching' \
            AND lifecycle.cause='dispatch_started' \
          ORDER BY lifecycle.ordinal DESC LIMIT 1",
    )
    .bind(fixture.workspace_id)
    .bind(fixture.effect_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(dispatch::sql)?;
    transaction.commit().await.map_err(dispatch::sql)?;
    Ok(ProviderDispatchAuthority {
        effect_id: ExternalEffectId::from_uuid(fixture.effect_id),
        authorization_id: PolicyDecisionId::from_uuid(row.0),
        connection_id: vestrace_domain::ConnectionId::from_uuid(fixture.connection_id),
        connection_revision_id: vestrace_domain::ConnectionRevisionId::from_uuid(
            fixture.connection_revision_id,
        ),
        concurrency_lease_id: row.1,
        credential_lease_id: None,
        dispatch_transition_id: ExternalEffectLifecycleTransitionId::from_uuid(row.2),
        dispatch_expires_at: row.3,
    })
}

struct Marker {
    workspace: Uuid,
    job: Uuid,
    effect: Uuid,
    preparation: Uuid,
    receipt: Uuid,
}

fn announce(
    fixture: &dispatch::Fixture,
    preparation: EmbeddingResultPreparationId,
    receipt: ExternalEffectReceiptId,
) {
    let mut stderr = std::io::stderr().lock();
    writeln!(
        stderr,
        "{MARKER} workspace={} job={} effect={} preparation={} receipt={}",
        fixture.workspace_id,
        fixture.job_id,
        fixture.effect_id,
        preparation.as_uuid(),
        receipt.as_uuid()
    )
    .expect("marker write");
    stderr.flush().expect("marker flush");
}

fn parse_marker(stderr: &str) -> Result<Marker, String> {
    let line = stderr
        .lines()
        .find(|line| line.starts_with(MARKER))
        .ok_or_else(|| format!("child did not announce result preparation: {stderr}"))?;
    let values = line[MARKER.len()..]
        .split_whitespace()
        .map(|part| {
            part.split_once('=')
                .ok_or_else(|| format!("malformed marker: {line}"))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
    let id = |name| {
        values
            .get(name)
            .ok_or_else(|| format!("marker omitted {name}"))
            .and_then(|value| {
                value
                    .parse::<Uuid>()
                    .map_err(|error| format!("marker {name} is invalid: {error}"))
            })
    };
    Ok(Marker {
        workspace: id("workspace")?,
        job: id("job")?,
        effect: id("effect")?,
        preparation: id("preparation")?,
        receipt: id("receipt")?,
    })
}

async fn read_persisted(owner: &PgPool, marker: &Marker) -> Result<serde_json::Value, String> {
    let row: PersistedResultPreparationCounts = sqlx::query_as(
        "SELECT \
           (SELECT count(*) FROM embedding_job_result_preparations p WHERE p.id=$1 AND p.workspace_id=$2 AND p.job_id=$3 AND p.external_effect_id=$4 AND p.receipt_id=$5), \
           (SELECT count(*) FROM external_effect_receipts r WHERE r.id=$5 AND r.workspace_id=$2 AND r.effect_id=$4), \
           (SELECT count(*) FROM embedding_job_result_prepared_attachments a WHERE a.workspace_id=$2 AND a.preparation_id=$1), \
           (SELECT count(*) FROM prepared_material_attachments a JOIN embedding_job_result_prepared_attachments r ON r.prepared_attachment_id=a.id WHERE r.workspace_id=$2 AND r.preparation_id=$1), \
           (SELECT count(*) FROM embedding_projection_entries p WHERE p.workspace_id=$2 AND p.preparation_id=$1 AND p.state='result_finalizing'), \
           (SELECT count(*) FROM embedding_projection_source_dependencies d JOIN embedding_projection_entries p ON p.workspace_id=d.workspace_id AND p.id=d.projection_id WHERE p.workspace_id=$2 AND p.preparation_id=$1), \
           (SELECT count(*) FROM embedding_index_generation_guards g JOIN embedding_job_result_preparations p ON p.workspace_id=g.workspace_id AND p.space_registration_id=g.space_registration_id WHERE p.id=$1), \
           (SELECT count(*) FROM embedding_space_corpus_states s JOIN embedding_job_result_preparations p ON p.workspace_id=s.workspace_id AND p.space_registration_id=s.space_registration_id WHERE p.id=$1), \
           (SELECT state FROM embedding_jobs WHERE id=$3 AND workspace_id=$2), \
           (SELECT count(*) FROM content_materials m JOIN embedding_projection_entries p ON p.material_id=m.id AND p.workspace_id=m.workspace_id WHERE p.preparation_id=$1 AND m.state='live'), \
           (SELECT count(*) FROM content_material_ordinary_references r JOIN embedding_projection_entries p ON p.material_id=r.material_id AND p.workspace_id=r.workspace_id WHERE p.preparation_id=$1)",
    ).bind(marker.preparation).bind(marker.workspace).bind(marker.job).bind(marker.effect).bind(marker.receipt)
     .fetch_one(owner).await.map_err(dispatch::sql)?;
    if row.0 != 1
        || row.1 != 1
        || row.2 != 2
        || row.3 != 2
        || row.4 != 2
        || row.5 != 2
        || row.6 != 1
        || row.7 != 1
        || row.8.as_deref() != Some("running")
        || row.9 != 0
        || row.10 != 0
    {
        return Err(format!(
            "result-prepared durable state was incomplete, lost a dependency, or became live: {row:?}"
        ));
    }
    Ok(
        serde_json::json!({"marker": row.0, "receipt": row.1, "attachments": row.2,
        "ciphertexts": row.3, "result_finalizing_projections": row.4, "source_dependencies": row.5,
        "generation_guards": row.6, "corpus_states": row.7,
        "job_state": row.8, "live_materials": row.9, "ordinary_references": row.10}),
    )
}

async fn recover(runtime: &PgPool, marker: &Marker) -> Result<String, String> {
    let mut transaction = runtime.begin().await.map_err(dispatch::sql)?;
    dispatch::set_workspace(&mut transaction, marker.workspace).await?;
    let phase: String = sqlx::query_scalar(
        "SELECT phase FROM vestrace_lock_embedding_job_recovery_authority($1,$2)",
    )
    .bind(marker.workspace)
    .bind(marker.job)
    .fetch_one(&mut *transaction)
    .await
    .map_err(dispatch::sql)?;
    transaction.commit().await.map_err(dispatch::sql)?;
    Ok(phase)
}

async fn run_same_child(
    _settings: &ScenarioSettings,
    url: String,
) -> Result<std::process::Output, String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let url_file = crate::argument(std::env::args().skip(1), "--database-url-file")
        .ok_or_else(|| "database URL file is required".to_owned())?;
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .map_err(|_| "VESTRACE_RUNTIME_DATABASE_URL is required for the child".to_owned())?;
    tokio::process::Command::new(program)
        .arg("--database-url-file")
        .arg(url_file)
        .arg("--scenario")
        .arg("embedding_result_preparation_crash")
        .env("VESTRACE_FAULT_CHILD", "1")
        .env("VESTRACE_FAULT_ISOLATION", "ephemeral")
        .env(
            "VESTRACE_FAULT_POINT",
            "after_result_prepared_before_return",
        )
        .env("VESTRACE_RUNTIME_DATABASE_URL", runtime_url)
        .env("VESTRACE_RESULT_PREPARATION_DISPATCH_URL", url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|error| format!("result-preparation child could not start: {error}"))
}

fn die(error: impl std::fmt::Display) -> ! {
    eprintln!("{MARKER} setup failed: {error}");
    std::process::exit(2)
}

struct LoopbackCounter {
    url: String,
    count: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl LoopbackCounter {
    async fn start() -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| error.to_string())?;
        let url = format!(
            "http://{}/embeddings",
            listener.local_addr().map_err(|error| error.to_string())?
        );
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                observed.fetch_add(1, Ordering::SeqCst);
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        });
        Ok(Self { url, count, task })
    }
    fn url(&self) -> String {
        self.url.clone()
    }
    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}
impl Drop for LoopbackCounter {
    fn drop(&mut self) {
        self.task.abort();
    }
}
