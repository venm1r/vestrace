use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vestrace_domain::id::{OutboxId, RetrievalRunId};
use vestrace_domain::retrieval::ClassificationPolicy;
use vestrace_domain::trust::{DataClassification, DataPolicy, evaluate_model_boundary};
use vestrace_domain::{DataDestination, Sensitivity, time::Timestamp};

use crate::providers::ProviderEgress;
use crate::retrieval::SharedEmbeddingProvider;
use crate::{ApplicationError, UnitOfWork};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingPurpose {
    Delivery,
    RetrievalQuery,
    Backfill,
    DimensionProbe,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingDataPolicyMode {
    Enforce,
    Observe,
}

#[derive(Clone, Debug)]
pub struct EmbeddingDataPolicySettings {
    pub classification_policy: ClassificationPolicy,
    /// The deployment-declared sensitivity floor for the embedding channel.
    pub classification: Sensitivity,
    pub policy: DataPolicy,
    pub mode: EmbeddingDataPolicyMode,
}

/// One input and the exact revision label that governs its disclosure.
///
/// `None` means the source revision has not been assessed. It is deliberately
/// not converted to a sensitivity: [`ClassificationPolicy`] already owns the
/// label admission rule used by retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingInput {
    text: String,
    classification: Option<String>,
}

impl EmbeddingInput {
    pub fn new(text: impl Into<String>, classification: Option<String>) -> Self {
        Self {
            text: text.into(),
            classification,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn classification(&self) -> Option<&str> {
        self.classification.as_deref()
    }
}

/// The decision committed before an embedding request may leave the process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingDataPolicyDecisionRecord {
    pub id: Uuid,
    pub purpose: EmbeddingPurpose,
    /// Outbox message, retrieval request, or rebuild invocation id.
    pub causal_reference_id: Uuid,
    pub delivery_attempt: Option<u32>,
    pub batch_ordinal: Option<u32>,
    pub destination: DataDestination,
    pub classification: Sensitivity,
    pub classification_labels: Vec<String>,
    pub unclassified_count: u32,
    pub input_count: u32,
    pub classification_allowed: bool,
    pub destination_allowed: bool,
    pub allowed: bool,
    pub reason: String,
    pub policy_version: String,
    pub mode: EmbeddingDataPolicyMode,
    pub decided_at: Timestamp,
}

#[async_trait]
pub trait EmbeddingDataPolicyDecisionRepository: Send + Sync {
    async fn record(
        &self,
        record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError>;

    /// Persist a decision in the caller-owned dispatch transaction. A worker
    /// must not borrow a second pool connection while it holds dispatch
    /// authority, so adapters that have not implemented this seam fail closed.
    async fn record_in(
        &self,
        _unit_of_work: &mut dyn UnitOfWork,
        _record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "transaction-bound embedding data-policy persistence is unsupported".to_owned(),
        ))
    }
}

pub type SharedEmbeddingDataPolicyDecisionRepository =
    Arc<dyn EmbeddingDataPolicyDecisionRepository>;

/// The only producer of a [`GovernedEmbeddingProvider`].
pub struct EmbeddingDataPolicyGate {
    settings: EmbeddingDataPolicySettings,
    decisions: SharedEmbeddingDataPolicyDecisionRepository,
}

impl EmbeddingDataPolicyGate {
    pub fn new(
        settings: EmbeddingDataPolicySettings,
        decisions: SharedEmbeddingDataPolicyDecisionRepository,
    ) -> Self {
        Self {
            settings,
            decisions,
        }
    }

    /// Records the disclosure decision without performing a provider call.
    /// An enforced denial or a recording failure refuses network dispatch.
    pub async fn authorize(
        &self,
        purpose: EmbeddingPurpose,
        causal_reference_id: Uuid,
        delivery_attempt: Option<u32>,
        batch_ordinal: Option<u32>,
        egress: &ProviderEgress,
        inputs: &[EmbeddingInput],
    ) -> Result<(), ApplicationError> {
        let record = self.decision(
            purpose,
            causal_reference_id,
            delivery_attempt,
            batch_ordinal,
            egress,
            inputs,
        )?;
        self.decisions.record(&record).await?;
        self.enforce(&record)
    }

    /// Records the disclosure decision in the transaction that has locked the
    /// dispatch authority. The caller commits an enforced denial so its durable
    /// evidence survives, then returns [`Self::enforce`]'s refusal before any
    /// network operation.
    #[allow(clippy::too_many_arguments)] // Mirrors `authorize` with the caller-owned transaction.
    pub async fn authorize_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        purpose: EmbeddingPurpose,
        causal_reference_id: Uuid,
        delivery_attempt: Option<u32>,
        batch_ordinal: Option<u32>,
        egress: &ProviderEgress,
        inputs: &[EmbeddingInput],
    ) -> Result<EmbeddingDataPolicyDecisionRecord, ApplicationError> {
        let record = self.decision(
            purpose,
            causal_reference_id,
            delivery_attempt,
            batch_ordinal,
            egress,
            inputs,
        )?;
        self.decisions.record_in(unit_of_work, &record).await?;
        Ok(record)
    }

    /// Turns a recorded decision into the enforce-mode refusal, if any.
    pub fn enforce(
        &self,
        record: &EmbeddingDataPolicyDecisionRecord,
    ) -> Result<(), ApplicationError> {
        if !record.allowed && self.settings.mode == EmbeddingDataPolicyMode::Enforce {
            return Err(ApplicationError::Policy(format!(
                "embedding data policy denied the {:?} request: {}",
                record.purpose, record.reason
            )));
        }
        Ok(())
    }

    fn decision(
        &self,
        purpose: EmbeddingPurpose,
        causal_reference_id: Uuid,
        delivery_attempt: Option<u32>,
        batch_ordinal: Option<u32>,
        egress: &ProviderEgress,
        inputs: &[EmbeddingInput],
    ) -> Result<EmbeddingDataPolicyDecisionRecord, ApplicationError> {
        let mut labels = BTreeSet::new();
        let mut unclassified_count = 0_u32;
        let mut refused = BTreeSet::new();
        for input in inputs {
            let classification = input.classification().map(str::trim);
            match classification {
                None | Some("") => {
                    unclassified_count = unclassified_count.saturating_add(1);
                    if !self.settings.classification_policy.admits(classification) {
                        refused.insert("unclassified".to_string());
                    }
                }
                Some(label) => {
                    labels.insert(label.to_string());
                    if !self.settings.classification_policy.admits(Some(label)) {
                        refused.insert(label.to_string());
                    }
                }
            }
        }
        let classification_allowed = refused.is_empty();

        let channel_classification = DataClassification::source(
            self.settings.classification,
            "embedding-channel",
            "deployment configuration policy.data.embedding.classification",
        )?;
        let destination = egress.destination();
        let destination_decision = evaluate_model_boundary(
            &self.settings.policy,
            &channel_classification,
            destination,
            false,
        );
        let destination_allowed = destination_decision.is_allowed();
        let allowed = classification_allowed && destination_allowed;

        let reason = match (classification_allowed, destination_allowed) {
            (true, true) => {
                "classification label check and data destination check allowed".to_string()
            }
            (false, true) => format!(
                "classification label check refused {:?} under policy.data.embedding.admissible_labels and policy.data.embedding.allow_unclassified",
                refused.iter().collect::<Vec<_>>()
            ),
            (true, false) => format!(
                "data destination check refused {destination:?}: {}",
                destination_decision.reason()
            ),
            (false, false) => format!(
                "classification label check refused {:?}; data destination check refused {destination:?}: {}",
                refused.iter().collect::<Vec<_>>(),
                destination_decision.reason()
            ),
        };
        Ok(EmbeddingDataPolicyDecisionRecord {
            id: Uuid::now_v7(),
            purpose,
            causal_reference_id,
            delivery_attempt,
            batch_ordinal,
            destination,
            classification: self.settings.classification,
            classification_labels: labels.into_iter().collect(),
            unclassified_count,
            input_count: u32::try_from(inputs.len()).unwrap_or(u32::MAX),
            classification_allowed,
            destination_allowed,
            allowed,
            reason: reason.clone(),
            policy_version: destination_decision.policy_version().to_string(),
            mode: self.settings.mode,
            decided_at: vestrace_domain::time::now(),
        })
    }

    pub fn govern(
        self,
        provider: SharedEmbeddingProvider,
        egress: ProviderEgress,
    ) -> Arc<GovernedEmbeddingProvider> {
        Arc::new(GovernedEmbeddingProvider {
            provider,
            egress,
            gate: self,
        })
    }
}

/// An embedding provider whose disclosure operations are governed and
/// purpose-specific. Its fields and constructor are private; only the gate can
/// produce one.
pub struct GovernedEmbeddingProvider {
    provider: SharedEmbeddingProvider,
    egress: ProviderEgress,
    gate: EmbeddingDataPolicyGate,
}

pub type SharedGovernedEmbeddingProvider = Arc<GovernedEmbeddingProvider>;

impl GovernedEmbeddingProvider {
    pub fn model(&self) -> &str {
        self.provider.model()
    }

    pub async fn embed_delivery(
        &self,
        outbox_message_id: OutboxId,
        attempt: u32,
        input: &EmbeddingInput,
    ) -> Result<Vec<Vec<f32>>, ApplicationError> {
        self.embed(
            EmbeddingPurpose::Delivery,
            outbox_message_id.as_uuid(),
            Some(attempt),
            None,
            std::slice::from_ref(input),
        )
        .await
    }

    pub async fn embed_retrieval_query(
        &self,
        retrieval_request_id: RetrievalRunId,
        query: &str,
    ) -> Result<Vec<Vec<f32>>, ApplicationError> {
        self.embed(
            EmbeddingPurpose::RetrievalQuery,
            retrieval_request_id.as_uuid(),
            None,
            None,
            &[EmbeddingInput::new(query, None)],
        )
        .await
    }

    pub async fn embed_backfill(
        &self,
        rebuild_invocation_id: Uuid,
        batch_ordinal: u32,
        inputs: &[EmbeddingInput],
    ) -> Result<Vec<Vec<f32>>, ApplicationError> {
        self.embed(
            EmbeddingPurpose::Backfill,
            rebuild_invocation_id,
            None,
            Some(batch_ordinal),
            inputs,
        )
        .await
    }

    pub async fn probe_dimensions(
        &self,
        rebuild_invocation_id: Uuid,
        batch_ordinal: u32,
    ) -> Result<Vec<Vec<f32>>, ApplicationError> {
        self.embed(
            EmbeddingPurpose::DimensionProbe,
            rebuild_invocation_id,
            None,
            Some(batch_ordinal),
            &[EmbeddingInput::new("dimension probe", None)],
        )
        .await
    }

    async fn embed(
        &self,
        purpose: EmbeddingPurpose,
        causal_reference_id: Uuid,
        delivery_attempt: Option<u32>,
        batch_ordinal: Option<u32>,
        inputs: &[EmbeddingInput],
    ) -> Result<Vec<Vec<f32>>, ApplicationError> {
        self.gate
            .authorize(
                purpose,
                causal_reference_id,
                delivery_attempt,
                batch_ordinal,
                &self.egress,
                inputs,
            )
            .await?;

        let provider_inputs = inputs
            .iter()
            .map(|input| input.text().to_string())
            .collect::<Vec<_>>();
        self.provider.embed(&provider_inputs).await
    }
}
