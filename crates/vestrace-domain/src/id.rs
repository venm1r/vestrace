use std::{fmt, str::FromStr};

macro_rules! domain_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Eq,
            Hash,
            PartialEq,
            schemars::JsonSchema,
            serde::Deserialize,
            serde::Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            pub const fn from_uuid(value: uuid::Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

domain_id!(WorkspaceId);
domain_id!(PrincipalId);
domain_id!(OperationId);
domain_id!(RequestId);
domain_id!(CorrelationId);
domain_id!(SessionId);
domain_id!(EventId);
domain_id!(MemoryId);
domain_id!(MemoryRevisionId);
domain_id!(MemorySourceId);
domain_id!(DerivationId);
domain_id!(RelationId);
domain_id!(JobId);
domain_id!(OutboxId);
domain_id!(EmbeddingSpaceId);
domain_id!(EmbeddingJobId);
domain_id!(CorpusGenerationId);
domain_id!(IndexBuildAttemptId);
domain_id!(CorpusChangeEventId);
domain_id!(LegacyAdoptionId);
domain_id!(TransitionBatchId);
// Spec line 205: this is "an identity, never a content hash, fingerprint,
// or digest", which is why it is a v7 UUID like every other identity here
// rather than anything derived from the projection it names.
domain_id!(EmbeddingTransitionProjectionRecipeId);
domain_id!(RetrievalRunId);
domain_id!(ContextPackId);
domain_id!(AccessTokenId);
domain_id!(PolicyId);
domain_id!(CapabilityGrantId);
domain_id!(PolicyDecisionId);
domain_id!(ApprovalRecordId);
domain_id!(AuditEventId);
domain_id!(ProviderId);
domain_id!(ModelId);
domain_id!(RoutingDecisionId);
domain_id!(ModelExecutionId);
domain_id!(AgentId);
domain_id!(SkillId);
domain_id!(AgentRunId);
domain_id!(RunStepId);
domain_id!(RunEventId);
domain_id!(RunCheckpointId);
domain_id!(PolicyBundleId);
domain_id!(AuthorizationTicketId);
domain_id!(BudAccountId);
domain_id!(ModelExecutionAttemptId);
domain_id!(ToolDefinitionId);
domain_id!(ToolInvocationId);
domain_id!(ExecutionPlanId);
domain_id!(ExecutionPlanRevisionId);
domain_id!(ArtifactId);
domain_id!(ArtifactRevisionId);
domain_id!(ConversationThreadId);
domain_id!(TriggerId);
domain_id!(ConnectorId);
domain_id!(ConnectionId);
domain_id!(AgentPackageId);
domain_id!(RemoteAgentInvocationId);
domain_id!(MetricRollupId);
domain_id!(ProductReleaseId);
domain_id!(InteractionSessionId);
domain_id!(ProductApiTransferId);
domain_id!(AgUiEndpointId);
domain_id!(KekId);
domain_id!(MemoryGrantId);
domain_id!(MemoryShareGrantRevisionId);
domain_id!(MemoryMountId);
domain_id!(FederationRelationshipId);
domain_id!(FederationTrustDecisionId);
domain_id!(HealthFindingId);
domain_id!(HealthOccurrenceId);
domain_id!(RepairPlanId);
domain_id!(RepairExecutionId);
domain_id!(VerificationRunId);
domain_id!(ExternalEffectId);
domain_id!(ExternalEffectLifecycleTransitionId);
domain_id!(ExternalEffectReceiptId);
domain_id!(ExternalReconciliationId);
domain_id!(IncidentId);
domain_id!(RevalidationRunId);
domain_id!(RecoveryPointId);
domain_id!(SecretRefId);
domain_id!(KeyReferenceId);
domain_id!(DataPolicyId);
domain_id!(ClassificationLineageId);
domain_id!(DeclassificationDecisionId);
domain_id!(DataHoldId);
domain_id!(DeletionRequestId);
domain_id!(DeletionPlanId);
domain_id!(DeletionVerificationId);
domain_id!(DataExportPlanId);
domain_id!(ExportBundleId);
domain_id!(QualificationBundleId);
domain_id!(QualificationBaselineId);
domain_id!(RunExportId);
domain_id!(WebhookSubscriptionId);
domain_id!(ReleaseManifestId);
domain_id!(AgentRevisionId);
domain_id!(SkillRevisionId);
domain_id!(WorkflowId);
domain_id!(WorkflowRevisionId);
domain_id!(WorkflowNodeId);
domain_id!(WorkflowTransitionId);
domain_id!(WorkflowExecutionId);
domain_id!(StepExecutionId);
domain_id!(ExecutionArtifactId);
domain_id!(ExecutionOutcomeId);
domain_id!(EvaluationId);
domain_id!(PlanRevisionId);
domain_id!(AgentRuntimeSnapshotId);
domain_id!(WorkItemId);
domain_id!(WorkerId);
domain_id!(BudgetSnapshotId);
domain_id!(ResourceUsageSnapshotId);
domain_id!(RunReferenceId);
domain_id!(ClaimId);
domain_id!(ClaimEvidenceLinkId);
domain_id!(ClaimAssessmentId);
domain_id!(ConflictId);
domain_id!(CognitiveMutationId);
domain_id!(SupersessionLinkId);
domain_id!(LearningProjectionId);
domain_id!(LearningProposalId);
domain_id!(LearningChangeId);
