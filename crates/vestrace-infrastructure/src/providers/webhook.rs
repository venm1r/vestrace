use std::time::Duration;

use vestrace_domain::external_effects::{
    AdapterDispatchResult, AdapterError, DeliverySemantics, DryRunMode, EffectReversibility,
    ExternalEffectAdapter, ExternalEffectAdapterDescriptor, ExternalEffectIntent,
    IdempotencyProfile,
};
use vestrace_domain::{Capability, DomainError};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// An external effect that is an HTTP request to a configured endpoint.
///
/// # The first adapter that actually leaves the process
///
/// Eighteen EXT requirements execute against this domain and nothing in this
/// build had ever performed an external effect: `ExternalEffectAdapter` had no
/// implementation outside test fixtures. This is one.
///
/// # Why the destination is configuration and not a request field
///
/// An intent names its adapter by name and carries the arguments; it does not
/// carry a URL. A surface that let a caller name the destination would be an
/// open outbound proxy — the caller could reach anything the process can,
/// including the metadata endpoints of whatever it is deployed on. The endpoint
/// is chosen by whoever configures the deployment; the caller chooses only what
/// to send.
///
/// # Why a read-back endpoint is required rather than optional
///
/// `validate_adapter_descriptor` refuses an adapter that does not declare
/// reconciliation support, and it is right to: an effect whose outcome can
/// become unknown and cannot be re-checked leaves the system permanently unable
/// to say what happened. A plain fire-and-forget webhook is exactly that, so it
/// cannot be built here at all — the constructor refuses without a read-back URL
/// rather than declaring a capability it does not have.
///
/// That is the honest reading of the contract, and it means "just POST it
/// somewhere" is not expressible. It is meant not to be.
pub struct HttpWebhookEffectAdapter {
    client: reqwest::Client,
    descriptor: ExternalEffectAdapterDescriptor,
    dispatch_url: String,
    read_back_url: String,
}

impl std::fmt::Debug for HttpWebhookEffectAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpWebhookEffectAdapter")
            .field("name", &self.descriptor.name())
            .field("dispatch_url", &self.dispatch_url)
            .field("read_back_url", &self.read_back_url)
            .finish()
    }
}

impl HttpWebhookEffectAdapter {
    pub fn new(
        name: impl Into<String>,
        dispatch_url: impl Into<String>,
        read_back_url: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let dispatch_url = dispatch_url.into();
        let read_back_url = read_back_url.into();
        if dispatch_url.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "webhook effect adapter requires a dispatch URL".into(),
            ));
        }
        if read_back_url.trim().is_empty() {
            return Err(DomainError::PolicyViolation(
                "webhook effect adapter requires a read-back URL: an effect whose outcome can \
                 become unknown and cannot be re-checked leaves the system unable to say what \
                 happened, and the adapter contract refuses to let it claim otherwise"
                    .into(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            // Redirects are refused rather than followed: the destination is
            // configuration, and a 302 would let the far side choose a
            // different one.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                DomainError::InvalidArgument(format!("webhook client could not be built: {error}"))
            })?;

        let descriptor = ExternalEffectAdapterDescriptor::new(
            name,
            // We may retry, and the far side may already have acted. Anything
            // stronger would be a claim about somebody else's system.
            DeliverySemantics::AtLeastOnce,
            // An `Idempotency-Key` header is sent, and whether it is honoured is
            // not ours to know. `Unknown` says exactly that; `ProviderKey` would
            // be a guess about the receiver.
            IdempotencyProfile::Unknown,
            // A delivered request cannot be recalled. Whether the *effect* can
            // be compensated is a property of what the endpoint does, which
            // this adapter cannot see.
            EffectReversibility::Irreversible,
            // A dry run validates and sends nothing; it does not ask the far
            // side to simulate, because most endpoints cannot.
            DryRunMode::Simulated,
            true,
            true,
            Capability::ExecutionWrite,
        )?;

        Ok(Self {
            client,
            descriptor,
            dispatch_url,
            read_back_url,
        })
    }

    pub fn read_back_url(&self) -> &str {
        &self.read_back_url
    }

    /// Ask the far side what it knows about an effect.
    ///
    /// Used by reconciliation after an unknown outcome. It reports what it saw
    /// and decides nothing: `None` means the endpoint answered without saying
    /// whether the effect applied, which is an inconclusive reconciliation and
    /// not a denial.
    pub async fn read_back(
        &self,
        intent: &ExternalEffectIntent,
    ) -> Result<Option<bool>, AdapterError> {
        let response = self
            .client
            .get(format!("{}/{}", self.read_back_url.trim_end_matches('/'), intent.id()))
            .send()
            .await
            .map_err(|error| AdapterError::Unavailable(error.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // The endpoint is reachable and has no record of it. That is a
            // statement, and it is the one that matters most.
            return Ok(Some(false));
        }
        if !response.status().is_success() {
            return Ok(None);
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| AdapterError::Rejected(error.to_string()))?;
        Ok(body.get("applied").and_then(serde_json::Value::as_bool))
    }
}

#[async_trait::async_trait]
impl ExternalEffectAdapter for HttpWebhookEffectAdapter {
    fn descriptor(&self) -> &ExternalEffectAdapterDescriptor {
        &self.descriptor
    }

    fn dispatch(
        &self,
        intent: &ExternalEffectIntent,
    ) -> Result<AdapterDispatchResult, AdapterError> {
        // The domain's `dispatch` is synchronous, so the request is driven on a
        // blocking handle rather than in the caller's async context. Doing it
        // this way keeps the decision — what an outcome means — inside the
        // domain, where every EXT case can reach it.
        let payload = serde_json::json!({
            "effect_id": intent.id(),
            "operation": intent.operation(),
            "target": intent.target(),
            "expected_effect": intent.expected_effect(),
            "arguments_digest": intent.normalized_arguments_digest(),
        });

        // A `reqwest::Client` belongs to the runtime it was built in: its
        // connection pool holds handles to that reactor, and using it from
        // another one panics. So the thread that runs this request builds its
        // own client inside its own runtime — a fresh connection per effect,
        // which is the right trade for an operation that leaves the process and
        // cannot be recalled.
        let dispatch_url = self.dispatch_url.clone();
        let idempotency_key = intent.id().to_string();
        let evidence = vec![format!("effect://{}/dispatch", intent.id())];

        let outcome = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
                    runtime.block_on(async {
                        let client = reqwest::Client::builder()
                            .timeout(DEFAULT_TIMEOUT)
                            .redirect(reqwest::redirect::Policy::none())
                            .build()
                            .map_err(|error| AdapterError::Unavailable(error.to_string()))?;
                        Ok(client
                            .post(&dispatch_url)
                            .header("content-type", "application/json")
                            .header("idempotency-key", idempotency_key)
                            .json(&payload)
                            .send()
                            .await)
                    })
                })
                .join()
                .map_err(|_| AdapterError::Unavailable("dispatch thread panicked".to_string()))?
        })?;

        match outcome {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    Ok(AdapterDispatchResult::acknowledged(
                        format!("http-{}", status.as_u16()),
                        Some(intent.id().to_string()),
                        None,
                        evidence,
                    ))
                } else if status.is_server_error() || status == reqwest::StatusCode::REQUEST_TIMEOUT
                {
                    // The far side may or may not have acted before it failed.
                    // Reporting this as a failure would be a verdict nobody
                    // has the evidence for.
                    Ok(AdapterDispatchResult::unknown(
                        format!("http-{}", status.as_u16()),
                        evidence,
                    ))
                } else {
                    // A 4xx is the far side saying it did not act.
                    Ok(AdapterDispatchResult::failed(
                        format!("http-{}", status.as_u16()),
                        evidence,
                    ))
                }
            }
            Err(error) if error.is_connect() => {
                // The connection was never established, so nothing was sent and
                // nothing can have happened. Reporting this as unknown would
                // manufacture doubt: `unknown` has to be earned, and an effect
                // stuck in it can never be retried.
                Ok(AdapterDispatchResult::failed("unreachable", evidence))
            }
            Err(error) if error.is_timeout() => {
                // The request went out and no answer came back. It may have
                // been received, parsed and acted on. Nobody knows, and saying
                // so is the only honest option.
                Ok(AdapterDispatchResult::unknown("timeout", evidence))
            }
            Err(error) => Err(AdapterError::Unavailable(error.to_string())),
        }
    }
}
