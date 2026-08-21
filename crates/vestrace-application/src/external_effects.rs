use vestrace_domain::external_effects::{
    AuthorizedExternalEffect, DispatchError, EffectAuthorization, ExternalEffectAdapter,
    ExternalEffectIntent, ExternalEffectReceipt,
};
use vestrace_domain::{AuthorizationRequest, Timestamp, WorkerId};

use crate::effect_recovery::DEFAULT_DISPATCH_ALLOWANCE;
use crate::{
    ApplicationError, AuthorizationBoundary, RequestContext, SharedExternalEffectRepository,
};

/// Grace after the adapter returns for committing the receipt that makes the
/// dispatch visible to the ordinary recovery sweep.
///
/// This is deliberately separate from the adapter timeout. The timeout bounds
/// time spent waiting on the external call; this margin bounds the local
/// persistence window in which a process can die after learning the result but
/// before recording it.
const DISPATCH_RECEIPT_COMMIT_MARGIN: chrono::Duration = chrono::Duration::seconds(5);

pub struct ExternalEffectService {
    effects: SharedExternalEffectRepository,
    authorization: AuthorizationBoundary,
    dispatch_owner: WorkerId,
}

impl ExternalEffectService {
    pub fn new(
        effects: SharedExternalEffectRepository,
        authorization: AuthorizationBoundary,
        dispatch_owner: WorkerId,
    ) -> Self {
        Self {
            effects,
            authorization,
            dispatch_owner,
        }
    }

    pub async fn authorize(
        &self,
        context: &RequestContext,
        intent: &ExternalEffectIntent,
        request: AuthorizationRequest,
    ) -> Result<AuthorizedExternalEffect, ApplicationError> {
        let decision = self.authorization.require(context, request).await?;
        intent
            .authorize(&EffectAuthorization::from_policy_decision(&decision))
            .map_err(ApplicationError::from)
    }

    /// Cross the durable boundary shared by composed and granular dispatches.
    ///
    /// Validation stays before the write because a rejected dispatch has not
    /// touched the world. The write stays before the adapter because a process
    /// that dies during the call must leave enough evidence for recovery to
    /// distinguish a lost dispatch from an intent that was merely prepared.
    pub async fn dispatch<A: ExternalEffectAdapter + ?Sized>(
        &self,
        context: &RequestContext,
        authorized: &AuthorizedExternalEffect,
        adapter: &A,
        current_precondition_digest: &str,
        recorded_at: Timestamp,
    ) -> Result<ExternalEffectReceipt, ApplicationError> {
        authorized
            .validate_dispatch(adapter, current_precondition_digest)
            .map_err(map_dispatch_error)?;
        let dispatch_timeout = adapter
            .descriptor()
            .dispatch_timeout()
            .unwrap_or(DEFAULT_DISPATCH_ALLOWANCE);
        let dispatch_expires_at = recorded_at + dispatch_timeout + DISPATCH_RECEIPT_COMMIT_MARGIN;
        self.effects
            .record_dispatch_started(
                context,
                authorized.intent().id(),
                self.dispatch_owner,
                dispatch_expires_at,
                recorded_at,
            )
            .await?;
        authorized
            .dispatch(adapter, current_precondition_digest, recorded_at)
            .map_err(map_dispatch_error)
    }
}

/// Perform an external effect: record the intent, authorize it, dispatch it,
/// record the receipt.
///
/// # Why the intent is written before the dispatch
///
/// EXT-001 requires an immutable intent to be **stored** before anything leaves
/// the process, and the reason is the crash in between. If the process dies
/// after the request and before any record of it, the system has caused
/// something in the world and holds no evidence that it tried — nothing to
/// reconcile against, nothing to find during startup recovery, nothing to tell
/// an operator. Writing the intent first turns an invisible act into an effect
/// with an unknown outcome, which is a state the rest of this machinery knows
/// how to handle.
///
/// The receipt is written after, in a second transaction. That ordering is the
/// honest one: an effect whose receipt is missing is exactly the unknown the
/// reconciliation path exists for, whereas an intent written after a dispatch
/// would be a record fabricated to match what already happened.
pub struct PerformExternalEffectService {
    effects: SharedExternalEffectRepository,
    granular: ExternalEffectService,
}

impl PerformExternalEffectService {
    pub fn new(
        effects: SharedExternalEffectRepository,
        authorization: AuthorizationBoundary,
        dispatch_owner: WorkerId,
    ) -> Self {
        let granular = ExternalEffectService::new(effects.clone(), authorization, dispatch_owner);
        Self { effects, granular }
    }

    pub async fn perform<A: ExternalEffectAdapter + ?Sized>(
        &self,
        context: &RequestContext,
        intent: ExternalEffectIntent,
        adapter: &A,
        at: Timestamp,
    ) -> Result<ExternalEffectReceipt, ApplicationError> {
        // 1. The record, before anything can happen in the world.
        self.effects.insert_intent(context, &intent).await?;

        // 2. Authority, checked against the intent it will act on.
        let request = AuthorizationRequest::new(
            intent.required_capability(),
            intent.operation().to_owned(),
            intent.target().to_owned(),
            intent.risk(),
        );
        let authorized = self.granular.authorize(context, &intent, request).await?;

        // 3. The preconditions as they are now, not as they were when the
        //    intent was written.
        let precondition_digest = authorized.intent().precondition_digest().to_owned();

        // 4. Cross the same validated, durable boundary the granular fault
        //    path uses, so neither path can dispatch without recording it.
        let receipt = self
            .granular
            .dispatch(context, &authorized, adapter, &precondition_digest, at)
            .await?;

        // 5. What came back, including "nothing came back".
        self.effects.insert_receipt(context, &receipt).await?;
        Ok(receipt)
    }
}

fn map_dispatch_error(error: DispatchError) -> ApplicationError {
    match error {
        DispatchError::StaleIntent => ApplicationError::Conflict("STALE_INTENT".into()),
        DispatchError::AdapterDoesNotMatchIntent | DispatchError::AdapterContractViolation(_) => {
            ApplicationError::Policy(format!("external adapter contract rejected: {error:?}"))
        }
        DispatchError::AdapterFailure(error) => {
            ApplicationError::Unavailable(format!("external adapter failed: {error:?}"))
        }
    }
}
