//! Governed adoption of legacy embeddings.
//!
//! Adoption is how a legacy plaintext vector stops existing. It never copies
//! that vector: for each legacy row it materializes the memory revision's
//! current content as an ordinary governed content material and creates a
//! `rebuild` job that recomputes the vector through the same provider path an
//! ordinary delivery uses.
//!
//! Nothing here decides when a plan is finished. Every state change is a
//! guarded SQL fact, and this module only sequences the steps and reports what
//! the database already committed.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use vestrace_domain::{EmbeddingJobId, embedding::LegacyAdoptionState, id::LegacyAdoptionId};

use crate::{ApplicationError, RequestContext};

/// Why a member cannot be adopted. Closed, and identical to the set the
/// migration's CHECK constraint accepts: an operator reads this to decide what
/// to repair, so a free-text reason would be unusable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LegacyAdoptionBlocker {
    /// The memory the legacy vector was derived from no longer exists.
    SourceMemoryAbsent,
    /// The memory exists but points at no revision.
    SourceRevisionAbsent,
    /// The memory or its content has been erased.
    SourceContentErased,
    /// The content material exists but is not Live, so nothing may embed it.
    SourceMaterialNotLive,
    /// The governed rebuild job reached a terminal failure.
    RebuildFailed,
    /// The named target space is not a canonical registration.
    TargetSpaceNotCanonical,
}

impl LegacyAdoptionBlocker {
    pub const ALL: [Self; 6] = [
        Self::SourceMemoryAbsent,
        Self::SourceRevisionAbsent,
        Self::SourceContentErased,
        Self::SourceMaterialNotLive,
        Self::RebuildFailed,
        Self::TargetSpaceNotCanonical,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceMemoryAbsent => "source_memory_absent",
            Self::SourceRevisionAbsent => "source_revision_absent",
            Self::SourceContentErased => "source_content_erased",
            Self::SourceMaterialNotLive => "source_material_not_live",
            Self::RebuildFailed => "rebuild_failed",
            Self::TargetSpaceNotCanonical => "target_space_not_canonical",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// Where one legacy row has reached.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LegacyAdoptionMemberState {
    Planned,
    Sourced,
    Rebuilding,
    Satisfied,
    Blocked,
}

impl LegacyAdoptionMemberState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Sourced => "sourced",
            Self::Rebuilding => "rebuilding",
            Self::Satisfied => "satisfied",
            Self::Blocked => "blocked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Planned,
            Self::Sourced,
            Self::Rebuilding,
            Self::Satisfied,
            Self::Blocked,
        ]
        .into_iter()
        .find(|state| state.as_str() == value)
    }
}

/// One legacy row and the identities adoption has bound to it so far.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyAdoptionMember {
    pub ordinal: u64,
    pub legacy_embedding_id: Uuid,
    pub memory_id: Uuid,
    pub memory_revision_id: Uuid,
    pub source_material_id: Option<Uuid>,
    pub source_intent_id: Option<Uuid>,
    pub rebuild_job_id: Option<Uuid>,
    pub state: LegacyAdoptionMemberState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyAdoptionBlockerRecord {
    pub ordinal: u64,
    pub reason: LegacyAdoptionBlocker,
}

/// What the database says about the plan right now. Counts are read back, never
/// accumulated in memory, so a resumed process reports the same numbers as the
/// one that started the plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyAdoptionProgress {
    pub plan_id: LegacyAdoptionId,
    pub state: LegacyAdoptionState,
    pub version: u64,
    /// The canonical space this plan adopts into. Carried here because every
    /// rebuild the plan creates has to target it, and a caller that had to look
    /// it up separately could act on a different space than the plan fixed.
    pub target_space_registration_id: Uuid,
    pub total_members: u64,
    pub satisfied_members: u64,
    pub blocked_members: u64,
    pub blockers: Vec<LegacyAdoptionBlockerRecord>,
    /// Present only once the cutover committed.
    pub deleted_legacy_rows: Option<u64>,
}

impl LegacyAdoptionProgress {
    /// True when no member is still waiting on work.
    pub fn is_settled(&self) -> bool {
        self.satisfied_members + self.blocked_members >= self.total_members
    }
}

#[derive(Clone, Debug)]
pub struct StartLegacyAdoption {
    pub plan_id: LegacyAdoptionId,
    pub legacy_space_registration_id: Uuid,
    pub target_space_registration_id: Uuid,
    pub idempotency_key: String,
}

/// The Live governed material a rebuild will read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterializedSource {
    pub material_id: Uuid,
    pub intent_id: Uuid,
}

/// Turns one memory revision's current content into a Live governed content
/// material.
///
/// Separate from the repository because it is not a database fact but a
/// governed side effect: it reserves an intent, encrypts the content and
/// publishes the material. A revision that cannot be materialized yields a
/// blocker rather than an error, because "this memory is gone" is an ordinary
/// adoption outcome an operator must be able to see.
#[async_trait]
pub trait LegacyAdoptionSourceMaterializer: Send + Sync {
    async fn materialize(
        &self,
        context: &RequestContext,
        member: &LegacyAdoptionMember,
    ) -> Result<Result<MaterializedSource, LegacyAdoptionBlocker>, ApplicationError>;
}

/// Creates the governed `rebuild` job that recomputes one member's vector.
#[async_trait]
pub trait LegacyAdoptionRebuildFactory: Send + Sync {
    async fn create_rebuild(
        &self,
        context: &RequestContext,
        target_space_registration_id: Uuid,
        member: &LegacyAdoptionMember,
        source: MaterializedSource,
    ) -> Result<EmbeddingJobId, ApplicationError>;
}

#[async_trait]
pub trait EmbeddingLegacyAdoptionRepository: Send + Sync {
    async fn start_or_resume(
        &self,
        context: &RequestContext,
        command: StartLegacyAdoption,
    ) -> Result<LegacyAdoptionProgress, ApplicationError>;

    async fn progress(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
    ) -> Result<LegacyAdoptionProgress, ApplicationError>;

    /// Members that still need work, in ordinal order.
    async fn unfinished_members(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        limit: u32,
    ) -> Result<Vec<LegacyAdoptionMember>, ApplicationError>;

    async fn bind_source(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        source: MaterializedSource,
    ) -> Result<(), ApplicationError>;

    async fn bind_rebuild(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        job_id: EmbeddingJobId,
    ) -> Result<(), ApplicationError>;

    async fn satisfy_member(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        projection_entry_id: Uuid,
    ) -> Result<(), ApplicationError>;

    async fn record_blocker(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        ordinal: u64,
        reason: LegacyAdoptionBlocker,
    ) -> Result<(), ApplicationError>;

    /// Returns the plan version the cutover must present.
    async fn prove_ready(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        generation_id: Uuid,
    ) -> Result<u64, ApplicationError>;

    /// Returns how many legacy plaintext rows the cutover deleted.
    async fn commit_cutover(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
        receipt_id: Uuid,
        expected_version: u64,
        audit_event_id: Uuid,
    ) -> Result<u64, ApplicationError>;

    /// The installation gate. Deliberately takes no workspace: one workspace
    /// still holding a legacy vector must keep it shut.
    async fn commit_plaintext_retirement(
        &self,
        context: &RequestContext,
    ) -> Result<u64, ApplicationError>;
}

pub type SharedEmbeddingLegacyAdoptionRepository = Arc<dyn EmbeddingLegacyAdoptionRepository>;

/// Sequences the adoption steps. It owns no state: every decision it makes is
/// re-read from the repository, so a restart mid-plan resumes rather than
/// duplicating.
pub struct EmbeddingLegacyAdoptionService {
    repository: SharedEmbeddingLegacyAdoptionRepository,
    materializer: Arc<dyn LegacyAdoptionSourceMaterializer>,
    rebuilds: Arc<dyn LegacyAdoptionRebuildFactory>,
    batch: u32,
}

impl EmbeddingLegacyAdoptionService {
    pub fn new(
        repository: SharedEmbeddingLegacyAdoptionRepository,
        materializer: Arc<dyn LegacyAdoptionSourceMaterializer>,
        rebuilds: Arc<dyn LegacyAdoptionRebuildFactory>,
        batch: u32,
    ) -> Result<Self, ApplicationError> {
        if batch == 0 {
            return Err(ApplicationError::Policy(
                "legacy adoption batch must be positive".to_owned(),
            ));
        }
        Ok(Self {
            repository,
            materializer,
            rebuilds,
            batch,
        })
    }

    /// Creates the plan, or returns the existing one. A replay under the same
    /// key is the same plan; a different key against the same legacy space is
    /// a second plan for one space and the database refuses it.
    pub async fn start_or_resume(
        &self,
        context: &RequestContext,
        command: StartLegacyAdoption,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        if command.idempotency_key.trim().is_empty() {
            return Err(ApplicationError::Policy(
                "legacy adoption requires an idempotency key".to_owned(),
            ));
        }
        self.repository.start_or_resume(context, command).await
    }

    /// Advances one bounded batch. Each member is carried as far as it can go
    /// in this pass; a blocker is recorded and the pass continues, because one
    /// unadoptable memory must not stall the rest of the plan.
    pub async fn advance(
        &self,
        context: &RequestContext,
        plan_id: LegacyAdoptionId,
    ) -> Result<LegacyAdoptionProgress, ApplicationError> {
        // Read the plan before acting on it: the target is the plan's, not the
        // caller's, and a pass that inferred it could rebuild into a space this
        // plan never adopted.
        let plan = self.repository.progress(context, plan_id).await?;
        let members = self
            .repository
            .unfinished_members(context, plan_id, self.batch)
            .await?;
        for member in members {
            match member.state {
                LegacyAdoptionMemberState::Planned => {
                    match self.materializer.materialize(context, &member).await? {
                        Ok(source) => {
                            self.repository
                                .bind_source(context, plan_id, member.ordinal, source)
                                .await?;
                            self.create_rebuild(
                                context,
                                plan.target_space_registration_id,
                                plan_id,
                                &member,
                                source,
                            )
                            .await?;
                        }
                        Err(blocker) => {
                            self.repository
                                .record_blocker(context, plan_id, member.ordinal, blocker)
                                .await?;
                        }
                    }
                }
                LegacyAdoptionMemberState::Sourced => {
                    // Resumption after a crash between binding the source and
                    // creating the job.  Both identities come from the row the
                    // database already committed, never reconstructed here.
                    let (Some(material_id), Some(intent_id)) =
                        (member.source_material_id, member.source_intent_id)
                    else {
                        return Err(ApplicationError::Internal(
                            "a sourced legacy adoption member must name its material and intent"
                                .to_owned(),
                        ));
                    };
                    let source = MaterializedSource {
                        material_id,
                        intent_id,
                    };
                    self.create_rebuild(
                        context,
                        plan.target_space_registration_id,
                        plan_id,
                        &member,
                        source,
                    )
                    .await?;
                }
                // Rebuilding members are satisfied by the result-finalization
                // path, not by this loop; terminal members need nothing.
                LegacyAdoptionMemberState::Rebuilding
                | LegacyAdoptionMemberState::Satisfied
                | LegacyAdoptionMemberState::Blocked => {}
            }
        }
        self.repository.progress(context, plan_id).await
    }

    async fn create_rebuild(
        &self,
        context: &RequestContext,
        target_space_registration_id: Uuid,
        plan_id: LegacyAdoptionId,
        member: &LegacyAdoptionMember,
        source: MaterializedSource,
    ) -> Result<(), ApplicationError> {
        let job = self
            .rebuilds
            .create_rebuild(context, target_space_registration_id, member, source)
            .await?;
        self.repository
            .bind_rebuild(context, plan_id, member.ordinal, job)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blocker_vocabulary_round_trips_and_is_closed() {
        for blocker in LegacyAdoptionBlocker::ALL {
            assert_eq!(
                LegacyAdoptionBlocker::parse(blocker.as_str()),
                Some(blocker)
            );
        }
        assert_eq!(LegacyAdoptionBlocker::parse("something_else"), None);
        let names = LegacyAdoptionBlocker::ALL.map(LegacyAdoptionBlocker::as_str);
        assert_eq!(
            names.iter().collect::<std::collections::HashSet<_>>().len(),
            names.len(),
            "each blocker must be distinguishable to an operator"
        );
    }

    #[test]
    fn a_settled_plan_is_one_where_no_member_is_still_waiting() {
        let progress = |total, satisfied, blocked| LegacyAdoptionProgress {
            plan_id: LegacyAdoptionId::new(),
            state: LegacyAdoptionState::Rebuilding,
            version: 1,
            target_space_registration_id: Uuid::now_v7(),
            total_members: total,
            satisfied_members: satisfied,
            blocked_members: blocked,
            blockers: Vec::new(),
            deleted_legacy_rows: None,
        };
        assert!(!progress(3, 1, 1).is_settled());
        assert!(progress(3, 2, 1).is_settled());
        assert!(progress(0, 0, 0).is_settled());
    }
}
