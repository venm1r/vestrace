use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use vestrace_application::{ApplicationError, ExternalEffectReadBackAdapter};
use vestrace_domain::DomainError;
use vestrace_domain::external_effects::{
    EvidenceStrength, ExternalEffectIntent, ExternalEffectReceipt, ObservedEffectState,
};

/// Reads the current state of a dispatched effect back over HTTP.
///
/// This is the observation half of reconciliation, not a second dispatch path:
/// it only ever issues `GET`, and it never converts a transport failure into
/// evidence that the effect did not happen. An unreachable provider is
/// unavailable evidence, which leaves the reconciliation inconclusive.
#[derive(Clone, Debug)]
pub struct HttpExternalEffectReadBackAdapter {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpExternalEffectReadBackAdapter {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, ApplicationError> {
        let endpoint = endpoint.into().trim().trim_end_matches('/').to_owned();
        if endpoint.is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "external effect read-back endpoint must not be blank".to_owned(),
            ));
        }
        let client = reqwest::Client::builder().build().map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "external effect read-back client could not be built: {error}"
            ))
        })?;
        Ok(Self { endpoint, client })
    }

    pub async fn observe<'a>(
        &self,
        intent: &ExternalEffectIntent,
        receipt: impl Into<Option<&'a ExternalEffectReceipt>>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        <Self as ExternalEffectReadBackAdapter>::observe(self, intent, receipt.into()).await
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadBackResponse {
    effect_applied: Option<bool>,
    state_ref: String,
    evidence_refs: Vec<String>,
    evidence_strength: String,
}

/// Provider evidence already present in the receipt, named by the matching
/// rung of the reconciliation ladder rather than by Vestrace's storage fields.
/// An absent value is omitted: sending a default would claim evidence the
/// provider never gave us.
#[derive(Serialize)]
struct ReadBackEvidence<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    exact_external_resource_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag_version_revision: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<&'a str>,
}

fn parse_evidence_strength(value: &str) -> Result<EvidenceStrength, ApplicationError> {
    match value {
        "response_digest" => Ok(EvidenceStrength::ResponseDigest),
        "marker_search" => Ok(EvidenceStrength::MarkerSearch),
        "content_hash" => Ok(EvidenceStrength::ContentHash),
        "etag_version" => Ok(EvidenceStrength::EtagVersion),
        "operation_status" => Ok(EvidenceStrength::OperationStatus),
        "external_resource_read_back" => Ok(EvidenceStrength::ExternalResourceReadBack),
        "provider_idempotency_lookup" => Ok(EvidenceStrength::ProviderIdempotencyLookup),
        other => Err(DomainError::InvalidArgument(format!(
            "read-back returned unknown evidence strength {other:?}"
        ))
        .into()),
    }
}

#[async_trait]
impl ExternalEffectReadBackAdapter for HttpExternalEffectReadBackAdapter {
    async fn observe(
        &self,
        intent: &ExternalEffectIntent,
        receipt: Option<&ExternalEffectReceipt>,
    ) -> Result<Vec<ObservedEffectState>, ApplicationError> {
        let url = format!("{}/{}", self.endpoint, intent.id());
        let evidence = ReadBackEvidence {
            exact_external_resource_id: receipt
                .and_then(ExternalEffectReceipt::external_resource_id),
            etag_version_revision: receipt.and_then(ExternalEffectReceipt::external_version),
            content_hash: receipt.and_then(ExternalEffectReceipt::response_digest),
        };
        let response = self
            .client
            .get(&url)
            .query(&evidence)
            .header("x-vestrace-adapter", intent.adapter())
            .send()
            .await
            .map_err(|error| {
                ApplicationError::Unavailable(format!("effect read-back request failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApplicationError::Unavailable(format!(
                "effect read-back returned {status}"
            )));
        }

        let body: ReadBackResponse = response.json().await.map_err(|error| {
            DomainError::InvalidArgument(format!(
                "effect read-back body is not valid JSON: {error}"
            ))
        })?;

        if body.state_ref.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "effect read-back returned a blank state reference".into(),
            )
            .into());
        }
        if body.evidence_refs.is_empty() {
            return Err(DomainError::InvalidArgument(
                "effect read-back returned no evidence references".into(),
            )
            .into());
        }

        // The outbound identifiers are lookup hints, not proof of which lookup
        // the provider performed. Only the provider's answer can state the
        // strength of the observation we record.
        let evidence_strength = parse_evidence_strength(&body.evidence_strength)?;
        Ok(vec![ObservedEffectState::new(
            evidence_strength,
            body.effect_applied,
            body.state_ref,
            body.evidence_refs,
        )])
    }
}
