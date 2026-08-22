use serde::{Deserialize, Serialize};

use crate::{
    Capability, DomainError, ExternalEffectId, ExternalEffectReceiptId, ExternalReconciliationId,
    PolicyDecision, PrincipalId, RiskCategory, Timestamp, WorkspaceId,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverySemantics {
    AtMostOnce,
    AtLeastOnce,
    EffectivelyOnce,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyProfile {
    None,
    ProviderKey,
    Conditional,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectReversibility {
    Reversible,
    Compensatable,
    Irreversible,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DryRunMode {
    Native,
    Simulated,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectPrecondition {
    name: String,
    expected_value: String,
}

impl EffectPrecondition {
    pub fn new(
        name: impl Into<String>,
        expected_value: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let expected_value = expected_value.into();
        if name.trim().is_empty() || expected_value.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "external effect precondition requires name and expected value".into(),
            ));
        }
        Ok(Self {
            name,
            expected_value,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn expected_value(&self) -> &str {
        &self.expected_value
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalEffectAdapterDescriptor {
    name: String,
    dispatch_timeout: Option<chrono::Duration>,
    delivery_semantics: DeliverySemantics,
    idempotency_profile: IdempotencyProfile,
    reversibility: EffectReversibility,
    dry_run: DryRunMode,
    supports_reconciliation: bool,
    supports_read_back: bool,
    required_capability: Capability,
}

impl ExternalEffectAdapterDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        dispatch_timeout: Option<chrono::Duration>,
        delivery_semantics: DeliverySemantics,
        idempotency_profile: IdempotencyProfile,
        reversibility: EffectReversibility,
        dry_run: DryRunMode,
        supports_reconciliation: bool,
        supports_read_back: bool,
        required_capability: Capability,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DomainError::InvalidArgument(
                "external effect adapter name must not be empty".into(),
            ));
        }
        if dispatch_timeout.is_some_and(|timeout| timeout <= chrono::Duration::zero()) {
            return Err(DomainError::InvalidArgument(
                "external effect adapter dispatch timeout must be positive".into(),
            ));
        }
        Ok(Self {
            name,
            dispatch_timeout,
            delivery_semantics,
            idempotency_profile,
            reversibility,
            dry_run,
            supports_reconciliation,
            supports_read_back,
            required_capability,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// How long a call may remain in flight before this adapter gives up.
    ///
    /// `None` is retained for adapters that cannot state a bound. Dispatchers
    /// must apply their documented fallback rather than inventing a duration
    /// and presenting it as this adapter's contract.
    pub fn dispatch_timeout(&self) -> Option<chrono::Duration> {
        self.dispatch_timeout
    }

    pub fn delivery_semantics(&self) -> DeliverySemantics {
        self.delivery_semantics
    }

    pub fn idempotency_profile(&self) -> IdempotencyProfile {
        self.idempotency_profile
    }

    pub fn reversibility(&self) -> EffectReversibility {
        self.reversibility
    }

    pub fn dry_run(&self) -> DryRunMode {
        self.dry_run
    }

    pub fn supports_reconciliation(&self) -> bool {
        self.supports_reconciliation
    }

    pub fn supports_read_back(&self) -> bool {
        self.supports_read_back
    }

    pub fn required_capability(&self) -> Capability {
        self.required_capability.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterContractError {
    EffectivelyOnceRequiresProviderIdempotency,
    ReconciliationNotDeclared,
}

pub fn validate_adapter_descriptor(
    descriptor: &ExternalEffectAdapterDescriptor,
) -> Result<(), AdapterContractError> {
    if descriptor.delivery_semantics == DeliverySemantics::EffectivelyOnce
        && !matches!(
            descriptor.idempotency_profile,
            IdempotencyProfile::ProviderKey | IdempotencyProfile::Conditional
        )
    {
        return Err(AdapterContractError::EffectivelyOnceRequiresProviderIdempotency);
    }
    if !descriptor.supports_reconciliation {
        return Err(AdapterContractError::ReconciliationNotDeclared);
    }
    Ok(())
}

/// What an effect may say it belongs to.
///
/// The three things the surface documents — a run, a workflow execution, an
/// operator acting directly — and nothing else. Written in the same vocabulary
/// as a capability grant's resource scope, because this is a reference to the
/// same kind of thing and two vocabularies for one idea is what the last slice
/// was about.
///
/// An operator action names `workspace://` and is deliberately **not**
/// routable: there is no run behind it, and saying so is more honest than
/// inventing a synthetic one.
fn validate_execution_ref(execution_ref: &str) -> Result<(), DomainError> {
    use crate::security::{ResourceKind, ResourceScope};

    let scope = ResourceScope::parse(execution_ref).map_err(|error| {
        DomainError::InvalidArgument(format!(
            "external effect execution reference is not a reference: {error}"
        ))
    })?;
    match scope {
        ResourceScope::One {
            kind: ResourceKind::Run | ResourceKind::Execution,
            ..
        } => Ok(()),
        ResourceScope::EveryOfKind {
            kind: ResourceKind::Workspace,
        } => Ok(()),
        other => Err(DomainError::InvalidArgument(format!(
            "external effect execution reference `{other}` names no execution; it must be \
             `run://<id>`, `execution://<id>`, or `workspace://` for an operator acting directly"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalEffectIntent {
    id: ExternalEffectId,
    execution_ref: String,
    workspace_id: WorkspaceId,
    actor_id: PrincipalId,
    adapter: String,
    operation: String,
    target: String,
    normalized_arguments_digest: String,
    expected_effect: String,
    preconditions: Vec<EffectPrecondition>,
    precondition_digest: String,
    risk: RiskCategory,
    reversibility: EffectReversibility,
    idempotency_profile: IdempotencyProfile,
    delivery_semantics: DeliverySemantics,
    required_capability: Capability,
    budget_reservation_ref: Option<String>,
    policy_decision_ref: Option<String>,
    compensates_effect_id: Option<ExternalEffectId>,
    created_at: Timestamp,
}

impl ExternalEffectIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        execution_ref: impl Into<String>,
        workspace_id: WorkspaceId,
        actor_id: PrincipalId,
        adapter: impl Into<String>,
        operation: impl Into<String>,
        target: impl Into<String>,
        normalized_arguments_digest: impl Into<String>,
        expected_effect: impl Into<String>,
        preconditions: Vec<EffectPrecondition>,
        precondition_digest: impl Into<String>,
        risk: RiskCategory,
        reversibility: EffectReversibility,
        idempotency_profile: IdempotencyProfile,
        delivery_semantics: DeliverySemantics,
        required_capability: Capability,
        budget_reservation_ref: Option<impl Into<String>>,
        policy_decision_ref: Option<impl Into<String>>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        let execution_ref = execution_ref.into();
        let adapter = adapter.into();
        let operation = operation.into();
        let target = target.into();
        let normalized_arguments_digest = normalized_arguments_digest.into();
        let expected_effect = expected_effect.into();
        let precondition_digest = precondition_digest.into();
        if [
            execution_ref.as_str(),
            adapter.as_str(),
            operation.as_str(),
            target.as_str(),
            normalized_arguments_digest.as_str(),
            expected_effect.as_str(),
            precondition_digest.as_str(),
        ]
        .iter()
        .any(|value| value.trim().is_empty())
        {
            return Err(DomainError::InvalidArgument(
                "external effect intent requires execution, adapter, operation, target, digest and expected effect"
                    .into(),
            ));
        }
        // What the intent says it belongs to has to be something that can be
        // followed. `execution_ref` was a free string checked for being
        // non-blank, while the surface that accepts it says "an effect with no
        // execution behind it is untraceable" — and `run-step-1` satisfied it
        // and traced to nothing. So when a reconciliation finally settled the
        // outcome of an effect, there was no way to find the run that had asked
        // for it. See [`Self::execution_run_id`].
        validate_execution_ref(&execution_ref)?;
        // The target *becomes* the resource scope of the authorization this
        // effect is dispatched under, so a target in a vocabulary no grant can
        // be written in is an effect that can never be permitted. It is checked
        // here, where the caller can still fix it, rather than at dispatch —
        // where it would look like a policy denial and send an operator looking
        // for a grant that does not exist. An external endpoint is named by its
        // URL; see [`ResourceScope`](crate::security::ResourceScope).
        crate::security::ResourceScope::parse(&target)?;
        if preconditions.is_empty() {
            return Err(DomainError::InvalidArgument(
                "external effect intent requires at least one precondition".into(),
            ));
        }
        Ok(Self {
            id: ExternalEffectId::new(),
            execution_ref,
            workspace_id,
            actor_id,
            adapter,
            operation,
            target,
            normalized_arguments_digest,
            expected_effect,
            preconditions,
            precondition_digest,
            risk,
            reversibility,
            idempotency_profile,
            delivery_semantics,
            required_capability,
            budget_reservation_ref: budget_reservation_ref.map(Into::into),
            policy_decision_ref: policy_decision_ref.map(Into::into),
            compensates_effect_id: None,
            created_at,
        })
    }

    pub fn id(&self) -> ExternalEffectId {
        self.id
    }

    pub fn execution_ref(&self) -> &str {
        &self.execution_ref
    }

    /// The run this effect was performed for, when there is one.
    ///
    /// `None` for an operator acting directly (`workspace://`) and for a
    /// workflow execution, which is a different identity — a caller that wants
    /// that should ask for it rather than have this quietly return something
    /// else.
    ///
    /// This is the point of validating the reference. When a reconciliation
    /// finally settles what happened, the outcome is only useful to whoever
    /// asked for the effect, and until now nothing could find them.
    pub fn execution_run_id(&self) -> Option<crate::id::AgentRunId> {
        use crate::security::{ResourceKind, ResourceScope};

        match ResourceScope::parse(&self.execution_ref) {
            Ok(ResourceScope::One {
                kind: ResourceKind::Run,
                id,
            }) => uuid::Uuid::parse_str(&id)
                .ok()
                .map(crate::id::AgentRunId::from_uuid),
            _ => None,
        }
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn actor_id(&self) -> PrincipalId {
        self.actor_id
    }

    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn normalized_arguments_digest(&self) -> &str {
        &self.normalized_arguments_digest
    }

    pub fn precondition_digest(&self) -> &str {
        &self.precondition_digest
    }

    pub fn risk(&self) -> RiskCategory {
        self.risk
    }

    pub fn reversibility(&self) -> EffectReversibility {
        self.reversibility
    }

    pub fn idempotency_profile(&self) -> IdempotencyProfile {
        self.idempotency_profile
    }

    pub fn delivery_semantics(&self) -> DeliverySemantics {
        self.delivery_semantics
    }

    pub fn required_capability(&self) -> Capability {
        self.required_capability.clone()
    }

    /// What the caller said this effect would do — the claim the outcome is
    /// eventually measured against.
    pub fn expected_effect(&self) -> &str {
        &self.expected_effect
    }

    pub fn preconditions(&self) -> &[EffectPrecondition] {
        &self.preconditions
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn budget_reservation_ref(&self) -> Option<&str> {
        self.budget_reservation_ref.as_deref()
    }

    pub fn compensates_effect_id(&self) -> Option<ExternalEffectId> {
        self.compensates_effect_id
    }

    pub fn policy_decision_ref(&self) -> Option<&str> {
        self.policy_decision_ref.as_deref()
    }

    pub fn authorize(
        &self,
        authorization: &EffectAuthorization,
    ) -> Result<AuthorizedExternalEffect, DomainError> {
        if !authorization.allowed
            || authorization.workspace_id != self.workspace_id
            || authorization.actor_id != self.actor_id
            || authorization.capability != self.required_capability
            || authorization.operation != self.operation
            || authorization.resource_scope != self.target
            || self
                .policy_decision_ref
                .as_deref()
                .is_some_and(|expected| expected != authorization.decision_ref)
        {
            return Err(DomainError::PolicyViolation(
                "external effect authorization does not match immutable intent".into(),
            ));
        }
        Ok(AuthorizedExternalEffect {
            intent: self.clone(),
            authorization: authorization.clone(),
        })
    }

    pub fn compensation_for(
        &self,
        operation: impl Into<String>,
        normalized_arguments_digest: impl Into<String>,
        expected_effect: impl Into<String>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        if self.reversibility != EffectReversibility::Compensatable {
            return Err(DomainError::PolicyViolation(
                "effect is not declared compensatable".into(),
            ));
        }
        let mut compensation = Self::new(
            self.execution_ref.clone(),
            self.workspace_id,
            self.actor_id,
            self.adapter.clone(),
            operation,
            self.target.clone(),
            normalized_arguments_digest,
            expected_effect,
            self.preconditions.clone(),
            self.precondition_digest.clone(),
            self.risk,
            self.reversibility,
            self.idempotency_profile,
            self.delivery_semantics,
            self.required_capability.clone(),
            None::<String>,
            None::<String>,
            created_at,
        )?;
        compensation.compensates_effect_id = Some(self.id);
        Ok(compensation)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectAuthorization {
    allowed: bool,
    decision_ref: String,
    policy_version: String,
    workspace_id: WorkspaceId,
    actor_id: PrincipalId,
    capability: Capability,
    operation: String,
    resource_scope: String,
}

impl EffectAuthorization {
    pub fn from_policy_decision(decision: &PolicyDecision) -> Self {
        Self {
            allowed: decision.is_allowed(),
            decision_ref: decision.id.to_string(),
            policy_version: decision.policy_version.clone(),
            workspace_id: decision.workspace_id,
            actor_id: decision.subject_id,
            capability: decision.capability.clone(),
            operation: decision.operation.clone(),
            resource_scope: decision.resource_scope.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn allow(
        decision_ref: impl Into<String>,
        policy_version: impl Into<String>,
        workspace_id: WorkspaceId,
        actor_id: PrincipalId,
        capability: Capability,
        operation: impl Into<String>,
        resource_scope: impl Into<String>,
    ) -> Self {
        Self {
            allowed: true,
            decision_ref: decision_ref.into(),
            policy_version: policy_version.into(),
            workspace_id,
            actor_id,
            capability,
            operation: operation.into(),
            resource_scope: resource_scope.into(),
        }
    }

    pub fn deny(
        decision_ref: impl Into<String>,
        policy_version: impl Into<String>,
        workspace_id: WorkspaceId,
        actor_id: PrincipalId,
        capability: Capability,
        operation: impl Into<String>,
        resource_scope: impl Into<String>,
    ) -> Self {
        let mut authorization = Self::allow(
            decision_ref,
            policy_version,
            workspace_id,
            actor_id,
            capability,
            operation,
            resource_scope,
        );
        authorization.allowed = false;
        authorization
    }

    pub fn decision_ref(&self) -> &str {
        &self.decision_ref
    }

    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    pub fn allowed(&self) -> bool {
        self.allowed
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub fn actor_id(&self) -> PrincipalId {
        self.actor_id
    }

    pub fn capability(&self) -> &Capability {
        &self.capability
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn resource_scope(&self) -> &str {
        &self.resource_scope
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedExternalEffect {
    intent: ExternalEffectIntent,
    authorization: EffectAuthorization,
}

impl AuthorizedExternalEffect {
    pub fn intent(&self) -> &ExternalEffectIntent {
        &self.intent
    }

    pub fn authorization(&self) -> &EffectAuthorization {
        &self.authorization
    }

    pub fn validate_dispatch<A: ExternalEffectAdapter + ?Sized>(
        &self,
        adapter: &A,
        current_precondition_digest: &str,
    ) -> Result<(), DispatchError> {
        validate_adapter_descriptor(adapter.descriptor())
            .map_err(DispatchError::AdapterContractViolation)?;
        if adapter.descriptor().name != self.intent.adapter
            || adapter.descriptor().delivery_semantics != self.intent.delivery_semantics
            || adapter.descriptor().idempotency_profile != self.intent.idempotency_profile
            || adapter.descriptor().reversibility != self.intent.reversibility
            || adapter.descriptor().required_capability != self.intent.required_capability
        {
            return Err(DispatchError::AdapterDoesNotMatchIntent);
        }
        if current_precondition_digest != self.intent.precondition_digest {
            return Err(DispatchError::StaleIntent);
        }
        Ok(())
    }

    pub fn dispatch<A: ExternalEffectAdapter + ?Sized>(
        &self,
        adapter: &A,
        current_precondition_digest: &str,
        recorded_at: Timestamp,
    ) -> Result<ExternalEffectReceipt, DispatchError> {
        self.validate_dispatch(adapter, current_precondition_digest)?;
        let result = adapter
            .dispatch(&self.intent)
            .map_err(DispatchError::AdapterFailure)?;
        Ok(ExternalEffectReceipt::from_dispatch(
            self.intent.id,
            self.intent.adapter.clone(),
            result,
            recorded_at,
        ))
    }
}

pub trait ExternalEffectAdapter: Send + Sync {
    fn descriptor(&self) -> &ExternalEffectAdapterDescriptor;
    fn dispatch(
        &self,
        intent: &ExternalEffectIntent,
    ) -> Result<AdapterDispatchResult, AdapterError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    Timeout,
    Unavailable(String),
    Rejected(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterOutcome {
    Acknowledged,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterDispatchResult {
    outcome: AdapterOutcome,
    response_class: String,
    external_resource_id: Option<String>,
    external_version: Option<String>,
    response_digest: Option<String>,
    evidence_refs: Vec<String>,
}

impl AdapterDispatchResult {
    pub fn acknowledged(
        response_class: impl Into<String>,
        external_resource_id: Option<String>,
        response_digest: Option<String>,
        evidence_refs: Vec<String>,
    ) -> Self {
        Self::new(
            AdapterOutcome::Acknowledged,
            response_class,
            external_resource_id,
            response_digest,
            evidence_refs,
        )
    }

    pub fn failed(response_class: impl Into<String>, evidence_refs: Vec<String>) -> Self {
        Self::new(
            AdapterOutcome::Failed,
            response_class,
            None,
            None,
            evidence_refs,
        )
    }

    pub fn unknown(response_class: impl Into<String>, evidence_refs: Vec<String>) -> Self {
        Self::new(
            AdapterOutcome::Unknown,
            response_class,
            None,
            None,
            evidence_refs,
        )
    }

    fn new(
        outcome: AdapterOutcome,
        response_class: impl Into<String>,
        external_resource_id: Option<String>,
        response_digest: Option<String>,
        evidence_refs: Vec<String>,
    ) -> Self {
        Self {
            outcome,
            response_class: response_class.into(),
            external_resource_id,
            external_version: None,
            response_digest,
            evidence_refs,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectLifecycleStatus {
    Prepared,
    Authorized,
    Dispatching,
    Acknowledged,
    Failed,
    Unknown,
    Confirmed,
    Reconciling,
}

impl EffectLifecycleStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Authorized => "authorized",
            Self::Dispatching => "dispatching",
            Self::Acknowledged => "acknowledged",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Confirmed => "confirmed",
            Self::Reconciling => "reconciling",
        }
    }

    pub const fn all_names() -> [&'static str; 8] {
        [
            Self::Prepared.as_str(),
            Self::Authorized.as_str(),
            Self::Dispatching.as_str(),
            Self::Acknowledged.as_str(),
            Self::Failed.as_str(),
            Self::Unknown.as_str(),
            Self::Confirmed.as_str(),
            Self::Reconciling.as_str(),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryDecision {
    Allowed,
    DeniedUnknown,
    DeniedNotRetrySafe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchError {
    StaleIntent,
    AdapterDoesNotMatchIntent,
    AdapterContractViolation(AdapterContractError),
    AdapterFailure(AdapterError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalEffectReceipt {
    id: ExternalEffectReceiptId,
    effect_id: ExternalEffectId,
    adapter: String,
    dispatched_at: Timestamp,
    acknowledgement_at: Option<Timestamp>,
    response_class: String,
    external_resource_id: Option<String>,
    external_version: Option<String>,
    response_digest: Option<String>,
    outcome_status: EffectLifecycleStatus,
    evidence_refs: Vec<String>,
    recorded_at: Timestamp,
}

impl ExternalEffectReceipt {
    fn from_dispatch(
        effect_id: ExternalEffectId,
        adapter: String,
        result: AdapterDispatchResult,
        recorded_at: Timestamp,
    ) -> Self {
        let outcome_status = match result.outcome {
            AdapterOutcome::Acknowledged => EffectLifecycleStatus::Acknowledged,
            AdapterOutcome::Failed => EffectLifecycleStatus::Failed,
            AdapterOutcome::Unknown => EffectLifecycleStatus::Unknown,
        };
        Self {
            id: ExternalEffectReceiptId::new(),
            effect_id,
            adapter,
            dispatched_at: recorded_at,
            acknowledgement_at: (result.outcome == AdapterOutcome::Acknowledged)
                .then_some(recorded_at),
            response_class: result.response_class,
            external_resource_id: result.external_resource_id,
            external_version: result.external_version,
            response_digest: result.response_digest,
            outcome_status,
            evidence_refs: result.evidence_refs,
            recorded_at,
        }
    }

    pub fn synthetic_unknown(
        effect_id: ExternalEffectId,
        adapter: impl Into<String>,
        recorded_at: Timestamp,
        evidence_refs: Vec<String>,
    ) -> Result<Self, DomainError> {
        if evidence_refs.is_empty() {
            return Err(DomainError::InvalidArgument(
                "external effect receipt requires evidence".into(),
            ));
        }
        Ok(Self::from_dispatch(
            effect_id,
            adapter.into(),
            AdapterDispatchResult::unknown("synthetic-unknown", evidence_refs),
            recorded_at,
        ))
    }

    pub fn id(&self) -> ExternalEffectReceiptId {
        self.id
    }

    pub fn effect_id(&self) -> ExternalEffectId {
        self.effect_id
    }

    pub fn outcome_status(&self) -> EffectLifecycleStatus {
        self.outcome_status
    }

    pub fn requires_reconciliation(&self) -> bool {
        matches!(
            self.outcome_status,
            EffectLifecycleStatus::Acknowledged | EffectLifecycleStatus::Unknown
        )
    }

    pub fn is_business_confirmation(&self) -> bool {
        false
    }

    pub fn retry_decision(&self) -> RetryDecision {
        match self.outcome_status {
            EffectLifecycleStatus::Unknown => RetryDecision::DeniedUnknown,
            EffectLifecycleStatus::Failed => RetryDecision::DeniedNotRetrySafe,
            _ => RetryDecision::Allowed,
        }
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    pub fn dispatched_at(&self) -> Timestamp {
        self.dispatched_at
    }

    /// When the far side said something back, if it ever did.
    pub fn acknowledgement_at(&self) -> Option<Timestamp> {
        self.acknowledgement_at
    }

    /// How the far side answered, as a class rather than a body.
    pub fn response_class(&self) -> &str {
        &self.response_class
    }

    /// What the far side called the thing it created, when it named one.
    ///
    /// This and the two below were stored and unreadable. EXT-016 requires a
    /// dispatched effect to have a receipt, and a receipt whose contents cannot
    /// be read is a row, not a record — the fourth field of this shape found in
    /// four slices, and found the same way: by trying to write a case about
    /// what the record is supposed to hold.
    pub fn external_resource_id(&self) -> Option<&str> {
        self.external_resource_id.as_deref()
    }

    pub fn external_version(&self) -> Option<&str> {
        self.external_version.as_deref()
    }

    /// A digest of the response. Deliberately not the response: a receipt has
    /// nowhere to put a body, so a provider's answer cannot arrive in one
    /// carrying whatever the caller sent it.
    pub fn response_digest(&self) -> Option<&str> {
        self.response_digest.as_deref()
    }

    pub fn recorded_at(&self) -> Timestamp {
        self.recorded_at
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStrength {
    ResponseDigest,
    MarkerSearch,
    ContentHash,
    EtagVersion,
    OperationStatus,
    ExternalResourceReadBack,
    ProviderIdempotencyLookup,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedEffectState {
    evidence_strength: EvidenceStrength,
    effect_applied: Option<bool>,
    state_ref: String,
    evidence_refs: Vec<String>,
}

impl ObservedEffectState {
    pub fn new(
        evidence_strength: EvidenceStrength,
        effect_applied: Option<bool>,
        state_ref: impl Into<String>,
        evidence_refs: Vec<String>,
    ) -> Self {
        Self {
            evidence_strength,
            effect_applied,
            state_ref: state_ref.into(),
            evidence_refs,
        }
    }

    pub fn evidence_strength(&self) -> EvidenceStrength {
        self.evidence_strength
    }

    /// `None` means the provider could not determine whether the effect
    /// applied. It is not the same as `Some(false)`.
    pub fn effect_applied(&self) -> Option<bool> {
        self.effect_applied
    }

    pub fn state_ref(&self) -> &str {
        &self.state_ref
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationOutcome {
    Confirmed,
    NotApplied,
    Inconclusive,
    HumanRequired,
}

impl ReconciliationOutcome {
    /// Whether this outcome ends the question.
    ///
    /// # Why this distinction has to exist
    ///
    /// The reconciliation sweep asked about every effect whose outcome nobody
    /// knew, and dropped an effect from that set as soon as *any* reconciliation
    /// existed for it. `Inconclusive` means "we asked the provider and it could
    /// not tell us" — nothing was settled — and it retired the effect from the
    /// sweep permanently. The receipt stayed `unknown` forever and the system
    /// had quietly stopped trying to find out.
    ///
    /// That contradicted a distinction this module already drew: a candidate the
    /// sweep *could not ask about* stays a candidate, "because nothing about it
    /// has been settled". The same is true of one it asked and got no answer
    /// from; only the storage query disagreed.
    ///
    /// `Confirmed` and `NotApplied` are answers. `Inconclusive` and
    /// `HumanRequired` are not, and an effect carrying one of them is still an
    /// effect whose outcome nobody knows.
    pub const fn is_settled(&self) -> bool {
        matches!(self, Self::Confirmed | Self::NotApplied)
    }

    /// The stored names of the outcomes that settle, for storage that has to ask
    /// the question in SQL.
    ///
    /// Here rather than as literals in a query, so a renamed variant cannot
    /// leave a query silently matching nothing — which would put every settled
    /// effect back in the sweep.
    pub fn settled_names() -> Vec<String> {
        [Self::Confirmed, Self::NotApplied]
            .iter()
            .map(|outcome| {
                serde_json::to_value(outcome)
                    .ok()
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .expect("a reconciliation outcome serializes as a string")
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalReconciliation {
    id: ExternalReconciliationId,
    effect_id: ExternalEffectId,
    receipt_id: Option<ExternalEffectReceiptId>,
    evidence_strength: EvidenceStrength,
    observed_state_ref: String,
    outcome: ReconciliationOutcome,
    evidence_refs: Vec<String>,
    reconciled_at: Timestamp,
}

impl ExternalReconciliation {
    pub fn id(&self) -> ExternalReconciliationId {
        self.id
    }

    pub fn outcome(&self) -> ReconciliationOutcome {
        self.outcome
    }

    pub fn evidence_strength(&self) -> EvidenceStrength {
        self.evidence_strength
    }

    pub fn evidence_refs(&self) -> &[String] {
        &self.evidence_refs
    }

    pub fn effect_id(&self) -> ExternalEffectId {
        self.effect_id
    }

    /// The receipt this reconciliation is about, when the dispatch returned
    /// one before the process stopped.
    pub fn receipt_id(&self) -> Option<ExternalEffectReceiptId> {
        self.receipt_id
    }

    pub fn observed_state_ref(&self) -> &str {
        &self.observed_state_ref
    }

    pub fn reconciled_at(&self) -> Timestamp {
        self.reconciled_at
    }
}

pub fn reconcile_effect<'a>(
    intent: &ExternalEffectIntent,
    receipt: impl Into<Option<&'a ExternalEffectReceipt>>,
    observations: Vec<ObservedEffectState>,
    reconciled_at: Timestamp,
) -> Result<ExternalReconciliation, DomainError> {
    let receipt = receipt.into();
    if receipt.is_some_and(|receipt| receipt.effect_id != intent.id) {
        return Err(DomainError::InvalidArgument(
            "receipt does not belong to external effect intent".into(),
        ));
    }
    let Some(observation) = observations
        .into_iter()
        .max_by_key(|item| item.evidence_strength)
    else {
        return Err(DomainError::InvalidArgument(
            "reconciliation requires at least one observation".into(),
        ));
    };
    let outcome = match observation.effect_applied {
        Some(true) => ReconciliationOutcome::Confirmed,
        Some(false) => ReconciliationOutcome::NotApplied,
        None => ReconciliationOutcome::Inconclusive,
    };
    Ok(ExternalReconciliation {
        id: ExternalReconciliationId::new(),
        effect_id: intent.id,
        receipt_id: receipt.map(ExternalEffectReceipt::id),
        evidence_strength: observation.evidence_strength,
        observed_state_ref: observation.state_ref,
        outcome,
        evidence_refs: observation.evidence_refs,
        reconciled_at,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectFaultPoint {
    AfterIntentPersistence,
    AfterAuthorizationBeforeDispatch,
    AfterDispatchBeforeReceipt,
    AfterReceiptBeforeOutcomeConfirmation,
    AfterOutcomeBeforeRunCommit,
}

impl EffectFaultPoint {
    pub fn required_points() -> [Self; 5] {
        [
            Self::AfterIntentPersistence,
            Self::AfterAuthorizationBeforeDispatch,
            Self::AfterDispatchBeforeReceipt,
            Self::AfterReceiptBeforeOutcomeConfirmation,
            Self::AfterOutcomeBeforeRunCommit,
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultObservation {
    pub point: EffectFaultPoint,
    pub status: EffectLifecycleStatus,
    pub retry_attempted: bool,
    pub reconciliation_started: bool,
    pub receipt_persisted: bool,
}

impl FaultObservation {
    pub fn expected(point: EffectFaultPoint) -> Self {
        match point {
            EffectFaultPoint::AfterIntentPersistence => Self {
                point,
                status: EffectLifecycleStatus::Prepared,
                retry_attempted: false,
                reconciliation_started: false,
                receipt_persisted: false,
            },
            EffectFaultPoint::AfterAuthorizationBeforeDispatch => Self {
                point,
                status: EffectLifecycleStatus::Authorized,
                retry_attempted: false,
                reconciliation_started: false,
                receipt_persisted: false,
            },
            EffectFaultPoint::AfterDispatchBeforeReceipt => Self {
                point,
                status: EffectLifecycleStatus::Unknown,
                retry_attempted: false,
                reconciliation_started: true,
                receipt_persisted: false,
            },
            EffectFaultPoint::AfterReceiptBeforeOutcomeConfirmation
            | EffectFaultPoint::AfterOutcomeBeforeRunCommit => Self {
                point,
                status: EffectLifecycleStatus::Reconciling,
                retry_attempted: false,
                reconciliation_started: true,
                receipt_persisted: true,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultSuiteDecision {
    failures: Vec<String>,
}

impl FaultSuiteDecision {
    pub fn is_passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failures(&self) -> &[String] {
        &self.failures
    }
}

pub fn evaluate_fault_suite(observations: &[FaultObservation]) -> FaultSuiteDecision {
    let mut failures = Vec::new();
    for point in EffectFaultPoint::required_points() {
        let matches = observations
            .iter()
            .filter(|item| item.point == point)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            failures.push(format!(
                "fault point {point:?} must have exactly one observation"
            ));
            continue;
        }
        let observed = matches[0];
        let expected = FaultObservation::expected(point);
        if point == EffectFaultPoint::AfterDispatchBeforeReceipt
            && observed.status != EffectLifecycleStatus::Unknown
        {
            failures.push("after dispatch before receipt must become UNKNOWN".into());
        }
        if observed.retry_attempted {
            failures.push(format!("fault point {point:?} attempted unsafe retry"));
        }
        if point == EffectFaultPoint::AfterDispatchBeforeReceipt && !observed.reconciliation_started
        {
            failures.push("UNKNOWN fault point must start reconciliation".into());
        }
        if point != EffectFaultPoint::AfterDispatchBeforeReceipt
            && observed.status != expected.status
        {
            failures.push(format!(
                "fault point {point:?} has incorrect lifecycle status"
            ));
        }
    }
    FaultSuiteDecision { failures }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effectively_once_requires_provider_idempotency() {
        let descriptor = ExternalEffectAdapterDescriptor::new(
            "adapter",
            Some(chrono::Duration::seconds(1)),
            DeliverySemantics::EffectivelyOnce,
            IdempotencyProfile::None,
            EffectReversibility::Unknown,
            DryRunMode::Unsupported,
            true,
            true,
            Capability::ExportRead,
        )
        .unwrap();
        assert_eq!(
            validate_adapter_descriptor(&descriptor),
            Err(AdapterContractError::EffectivelyOnceRequiresProviderIdempotency)
        );
    }

    #[test]
    fn adapter_descriptor_refuses_a_non_positive_dispatch_timeout() {
        for dispatch_timeout in [chrono::Duration::zero(), chrono::Duration::seconds(-1)] {
            let error = ExternalEffectAdapterDescriptor::new(
                "adapter",
                Some(dispatch_timeout),
                DeliverySemantics::AtLeastOnce,
                IdempotencyProfile::ProviderKey,
                EffectReversibility::Unknown,
                DryRunMode::Unsupported,
                true,
                true,
                Capability::ExportRead,
            )
            .unwrap_err();

            assert!(
                matches!(error, DomainError::InvalidArgument(message) if message.contains("dispatch timeout")),
                "the invalid field was not named"
            );
        }
    }

    #[test]
    fn adapter_descriptor_states_its_dispatch_timeout() {
        let dispatch_timeout = chrono::Duration::seconds(7);
        let descriptor = ExternalEffectAdapterDescriptor::new(
            "adapter",
            Some(dispatch_timeout),
            DeliverySemantics::AtLeastOnce,
            IdempotencyProfile::ProviderKey,
            EffectReversibility::Unknown,
            DryRunMode::Unsupported,
            true,
            true,
            Capability::ExportRead,
        )
        .unwrap();

        assert_eq!(descriptor.dispatch_timeout(), Some(dispatch_timeout));
    }

    #[test]
    fn fault_expected_set_is_self_consistent() {
        let cases = EffectFaultPoint::required_points()
            .into_iter()
            .map(FaultObservation::expected)
            .collect::<Vec<_>>();
        assert!(evaluate_fault_suite(&cases).is_passed());
    }

    /// An intent differing only in what it says it belongs to.
    fn intent_belonging_to(execution_ref: &str) -> Result<ExternalEffectIntent, DomainError> {
        ExternalEffectIntent::new(
            execution_ref,
            WorkspaceId::new(),
            PrincipalId::new(),
            "webhook-v1",
            "send",
            "https://alpha.effects.test/hook",
            "sha256:arguments",
            "deliver notification",
            vec![EffectPrecondition::new("resource-version", "v1").unwrap()],
            "sha256:preconditions-v1",
            RiskCategory::Medium,
            EffectReversibility::Compensatable,
            IdempotencyProfile::ProviderKey,
            DeliverySemantics::AtLeastOnce,
            Capability::ExportRead,
            None::<String>,
            None::<String>,
            crate::now(),
        )
    }

    /// The property, through the constructor rather than past it.
    ///
    /// An earlier version of these tests called `validate_execution_ref`
    /// directly, so deleting the call from `ExternalEffectIntent::new` left them
    /// all passing. A check that cannot tell whether the thing it checks is
    /// wired up is not a check.
    #[test]
    fn an_intent_cannot_be_built_with_an_execution_reference_that_names_nothing() {
        assert!(
            intent_belonging_to("run-step-1").is_err(),
            "an intent was built saying it belongs to `run-step-1`, which names nothing"
        );

        let run = crate::id::AgentRunId::new();
        let intent = intent_belonging_to(&format!("run://{run}"))
            .expect("an intent belonging to a real run");
        assert_eq!(
            intent.execution_run_id(),
            Some(run),
            "the effect could not say which run asked for it"
        );

        // An operator acting directly is honestly attributed to nobody rather
        // than to a run that does not exist.
        let operator = intent_belonging_to("workspace://").expect("an operator action");
        assert_eq!(operator.execution_run_id(), None);
    }

    #[test]
    fn an_execution_reference_names_something_that_can_be_found() {
        // The surface accepting these says "an effect with no execution behind
        // it is untraceable". `run-step-1` satisfied the old check and traced to
        // nothing, so a settled outcome could never be attributed to whatever
        // asked for the effect.
        for label in ["run-step-1", "step 4", "the nightly job", "run:0198"] {
            assert!(
                validate_execution_ref(label).is_err(),
                "`{label}` was accepted as an execution reference and names nothing"
            );
        }
    }

    #[test]
    fn a_run_reference_resolves_to_the_run() {
        let run = crate::id::AgentRunId::new();
        assert!(validate_execution_ref(&format!("run://{run}")).is_ok());

        // A reference to something other than a run is accepted and is honestly
        // not a run, rather than being coerced into one.
        assert!(validate_execution_ref("workspace://").is_ok());
        assert!(validate_execution_ref(&format!("execution://{run}")).is_ok());

        // And a scope that is well-formed but names no execution is refused:
        // `run://` is every run, which is not something an effect belongs to.
        assert!(validate_execution_ref("run://").is_err());
        assert!(validate_execution_ref("memory://0198").is_err());
        assert!(validate_execution_ref("/v1/runs").is_err());
    }
}
