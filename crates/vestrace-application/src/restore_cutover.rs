//! Application boundary for preparing a host-only P05-C restore target.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use vestrace_domain::{
    RestoreAttemptProgress, RestoreTargetRoots, RestoreTerminalReceipt, SafetyEventKind,
    SignedJournalEntry, SourceFreezePoint, TargetActivationPlan, WitnessError, WitnessReceipt,
};

use crate::{
    ApplicationError, InstallationMutationPermit, InstallationSafetyWitness,
    InstallationSupervisorContext, PermitMode, SafetyJournal, UnitOfWork,
};

#[async_trait]
pub trait FreshRestoreTargetCustody: Send + Sync {
    /// Creates one new target directory. Implementations must not accept an
    /// existing directory and must never touch a protected source root.
    async fn prepare_fresh(&self, target: PathBuf) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait RestoreAttemptAuthority: Send + Sync {
    async fn prepare_attempt_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
    ) -> Result<(), ApplicationError>;

    async fn record_source_freeze_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        freeze: SourceFreezePoint,
    ) -> Result<(), ApplicationError>;

    async fn record_target_initialized_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        receipt: vestrace_domain::SafetyJournalDigest,
    ) -> Result<(), ApplicationError>;

    async fn record_source_resume_prepared_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        receipt: vestrace_domain::SafetyJournalDigest,
    ) -> Result<(), ApplicationError>;

    async fn record_restore_safety_event_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<(), ApplicationError>;

    async fn release_hold_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        terminal: &RestoreTerminalReceipt,
    ) -> Result<(), ApplicationError>;
}

/// Makes a restore attempt durable before any target filesystem operation.
pub struct RestoreAttemptAuthorityService {
    permit: Arc<dyn InstallationMutationPermit>,
    journal: Arc<dyn SafetyJournal>,
    witness: Arc<dyn InstallationSafetyWitness>,
    repository: Arc<dyn RestoreAttemptAuthority>,
}

impl RestoreAttemptAuthorityService {
    pub fn new(
        permit: Arc<dyn InstallationMutationPermit>,
        journal: Arc<dyn SafetyJournal>,
        witness: Arc<dyn InstallationSafetyWitness>,
        repository: Arc<dyn RestoreAttemptAuthority>,
    ) -> Self {
        Self {
            permit,
            journal,
            witness,
            repository,
        }
    }

    pub async fn prepare_attempt(
        &self,
        context: &InstallationSupervisorContext,
        attempt: &RestoreAttemptProgress,
        entry: SignedJournalEntry,
    ) -> Result<WitnessReceipt, ApplicationError> {
        if entry.event_kind() != SafetyEventKind::RestoreAttemptPrepared
            || entry.state().restore_attempt_progress() != Some(attempt)
        {
            return Err(ApplicationError::Policy(
                "restore preparation requires its exact signed attempt successor".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(&entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        self.repository
            .prepare_attempt_in(permit.unit_of_work_mut(), attempt)
            .await?;
        self.repository
            .record_restore_safety_event_in(permit.unit_of_work_mut(), entry, receipt.clone())
            .await?;
        permit.commit().await?;
        Ok(receipt)
    }

    pub async fn record_source_freeze(
        &self,
        context: &InstallationSupervisorContext,
        attempt: &RestoreAttemptProgress,
        freeze: SourceFreezePoint,
        entry: SignedJournalEntry,
    ) -> Result<WitnessReceipt, ApplicationError> {
        let expected = attempt
            .record_source_freeze(freeze)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        if entry.event_kind() != SafetyEventKind::SourceFreezeRecorded
            || entry.state().restore_attempt_progress() != Some(&expected)
        {
            return Err(ApplicationError::Policy(
                "source freeze requires its exact signed frozen successor".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(&entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        self.repository
            .record_source_freeze_in(permit.unit_of_work_mut(), attempt, freeze)
            .await?;
        self.repository
            .record_restore_safety_event_in(permit.unit_of_work_mut(), entry, receipt.clone())
            .await?;
        permit.commit().await?;
        Ok(receipt)
    }

    pub async fn record_activation_plan(
        &self,
        context: &InstallationSupervisorContext,
        plan: &TargetActivationPlan,
        entry: SignedJournalEntry,
    ) -> Result<WitnessReceipt, ApplicationError> {
        if entry.event_kind() != SafetyEventKind::TargetActivationPlanned
            || entry.state().target_activation_plan() != Some(plan)
        {
            return Err(ApplicationError::Policy(
                "activation plan requires its exact signed successor".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(&entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        self.repository
            .record_restore_safety_event_in(permit.unit_of_work_mut(), entry, receipt.clone())
            .await?;
        permit.commit().await?;
        Ok(receipt)
    }

    /// Persists the exact witnessed activation-start marker before the
    /// guarded generation CAS. No restore-table mutation accompanies this
    /// step: it is the durable recovery boundary for a CAS that has not yet
    /// committed.
    pub async fn record_activation_started(
        &self,
        context: &InstallationSupervisorContext,
        plan: &TargetActivationPlan,
        entry: SignedJournalEntry,
    ) -> Result<WitnessReceipt, ApplicationError> {
        if entry.event_kind() != SafetyEventKind::TargetActivating
            || entry.state().target_activation_plan() != Some(plan)
            || entry.state().target_activation_started() != Some(plan.digest())
        {
            return Err(ApplicationError::Policy(
                "activation start requires its exact plan-pinned signed successor".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(&entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        self.repository
            .record_restore_safety_event_in(permit.unit_of_work_mut(), entry, receipt.clone())
            .await?;
        permit.commit().await?;
        Ok(receipt)
    }

    pub async fn record_target_initialized(
        &self,
        context: &InstallationSupervisorContext,
        attempt: &RestoreAttemptProgress,
        terminal: &RestoreTerminalReceipt,
        entry: SignedJournalEntry,
    ) -> Result<WitnessReceipt, ApplicationError> {
        let expected = attempt
            .record_terminal_receipt(terminal.clone())
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        if entry.event_kind() != SafetyEventKind::RestoreTerminalRecorded
            || entry.state().restore_attempt_progress() != Some(&expected)
        {
            return Err(ApplicationError::Policy(
                "target initialization requires its exact signed terminal successor".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(&entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        self.repository
            .record_target_initialized_in(
                permit.unit_of_work_mut(),
                attempt,
                terminal.release_reason().receipt(),
            )
            .await?;
        self.repository
            .record_restore_safety_event_in(permit.unit_of_work_mut(), entry, receipt.clone())
            .await?;
        permit.commit().await?;
        Ok(receipt)
    }

    /// Records a terminal source-resume receipt only after target refusal
    /// cleanup has completed outside PostgreSQL.
    pub async fn record_source_resume_prepared(
        &self,
        context: &InstallationSupervisorContext,
        attempt: &RestoreAttemptProgress,
        terminal: &RestoreTerminalReceipt,
        entry: SignedJournalEntry,
    ) -> Result<WitnessReceipt, ApplicationError> {
        let expected = attempt
            .record_terminal_receipt(terminal.clone())
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        if !matches!(
            terminal.release_reason(),
            vestrace_domain::RestoreHoldReleaseReason::SourceResumePrepared { .. }
        ) || entry.event_kind() != SafetyEventKind::RestoreTerminalRecorded
            || entry.state().restore_attempt_progress() != Some(&expected)
        {
            return Err(ApplicationError::Policy(
                "source resume requires its exact signed terminal successor".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(&entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        self.repository
            .record_source_resume_prepared_in(
                permit.unit_of_work_mut(),
                attempt,
                terminal.release_reason().receipt(),
            )
            .await?;
        self.repository
            .record_restore_safety_event_in(permit.unit_of_work_mut(), entry, receipt.clone())
            .await?;
        permit.commit().await?;
        Ok(receipt)
    }

    pub async fn release_hold(
        &self,
        context: &InstallationSupervisorContext,
        terminal: &RestoreTerminalReceipt,
    ) -> Result<(), ApplicationError> {
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        self.repository
            .release_hold_in(permit.unit_of_work_mut(), terminal)
            .await?;
        permit.commit().await
    }

    /// Replays only the already witnessed target-initialization side effect.
    /// This closes the crash window after the durable terminal receipt but
    /// before the guarded SQL transition commits.
    pub async fn ensure_target_initialized(
        &self,
        context: &InstallationSupervisorContext,
        attempt: &RestoreAttemptProgress,
        terminal: &RestoreTerminalReceipt,
    ) -> Result<(), ApplicationError> {
        if attempt.attempt_id() != terminal.attempt_id()
            || attempt.target_id() != terminal.target_id()
            || !matches!(
                terminal.release_reason(),
                vestrace_domain::RestoreHoldReleaseReason::TargetInitializationComplete { .. }
            )
        {
            return Err(ApplicationError::Policy(
                "target initialization replay does not match its restore attempt".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        self.repository
            .record_target_initialized_in(
                permit.unit_of_work_mut(),
                attempt,
                terminal.release_reason().receipt(),
            )
            .await?;
        permit.commit().await
    }
}

fn witness_error(error: WitnessError) -> ApplicationError {
    match error {
        WitnessError::Conflict => ApplicationError::Conflict("witness head changed".to_owned()),
        WitnessError::Unavailable(message) => ApplicationError::Unavailable(message),
        WitnessError::Invalid(error) => ApplicationError::Policy(error.to_string()),
    }
}

/// The controller deliberately stops before materializing archive objects or
/// activation. Its only effect is creation of a new target after the caller
/// has durably prepared the guarded restore attempt and hold.
pub struct RestoreCutoverController {
    custody: Arc<dyn FreshRestoreTargetCustody>,
}

impl RestoreCutoverController {
    pub fn new(custody: Arc<dyn FreshRestoreTargetCustody>) -> Self {
        Self { custody }
    }

    pub async fn restore_to_target(
        &self,
        attempt: RestoreAttemptProgress,
        source_root: PathBuf,
        target_root: PathBuf,
    ) -> Result<RestoreAttemptProgress, ApplicationError> {
        if attempt.source_freeze().is_some() || attempt.terminal_receipt().is_some() {
            return Err(ApplicationError::Policy(
                "restore target may be prepared only for an exact prepared attempt".to_owned(),
            ));
        }
        RestoreTargetRoots::new(source_root, target_root.clone())
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.custody.prepare_fresh(target_root).await?;
        Ok(attempt)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        sync::{Arc, Mutex},
    };

    use vestrace_domain::{
        BackupSetId, DatabaseGenerationId, RestoreAttemptId, RestoreHoldId, RestoreTargetId,
        SourceFreezePoint,
    };

    use super::*;

    #[derive(Default)]
    struct RecordingCustody(Mutex<Vec<PathBuf>>);

    #[async_trait]
    impl FreshRestoreTargetCustody for RecordingCustody {
        async fn prepare_fresh(&self, target: PathBuf) -> Result<(), ApplicationError> {
            self.0.lock().unwrap().push(target);
            Ok(())
        }
    }

    #[tokio::test]
    async fn controller_refuses_overlap_and_nonprepared_attempt_before_host_custody() {
        let custody = Arc::new(RecordingCustody::default());
        let controller = RestoreCutoverController::new(custody.clone());
        let generation = DatabaseGenerationId::new();
        let attempt = RestoreAttemptProgress::prepare(
            RestoreAttemptId::new(),
            RestoreTargetId::new(),
            BackupSetId::new(),
            RestoreHoldId::new(),
            generation,
            DatabaseGenerationId::new(),
        )
        .unwrap();
        let source = std::env::temp_dir().join("vestrace-controller-source");
        assert!(
            controller
                .restore_to_target(attempt.clone(), source.clone(), source.join("target"))
                .await
                .is_err()
        );
        assert!(custody.0.lock().unwrap().is_empty());
        let frozen = attempt
            .record_source_freeze(SourceFreezePoint::new(generation, 1, 1, 0).unwrap())
            .unwrap();
        assert!(
            controller
                .restore_to_target(
                    frozen,
                    source.clone(),
                    std::env::temp_dir().join("vestrace-controller-target")
                )
                .await
                .is_err()
        );
        assert!(custody.0.lock().unwrap().is_empty());
    }

    struct FakeUnit;

    #[async_trait]
    impl UnitOfWork for FakeUnit {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    struct FakePermit;

    #[async_trait]
    impl InstallationMutationPermit for FakePermit {
        async fn acquire(
            &self,
            mode: PermitMode,
            _: &crate::RequestContext,
        ) -> Result<crate::PermitHandle, ApplicationError> {
            assert_eq!(mode, PermitMode::Exclusive);
            Ok(crate::PermitHandle::new(Box::new(FakeUnit)))
        }
    }

    struct UnusedJournal;

    #[async_trait]
    impl SafetyJournal for UnusedJournal {
        async fn append(&self, _: &SignedJournalEntry) -> Result<(), ApplicationError> {
            panic!("target initialization replay does not append a journal entry")
        }
    }

    struct UnusedWitness;

    #[async_trait]
    impl InstallationSafetyWitness for UnusedWitness {
        async fn read_head(&self) -> Result<vestrace_domain::WitnessHead, WitnessError> {
            panic!("target initialization replay does not read the witness")
        }

        async fn compare_and_advance(
            &self,
            _: vestrace_domain::WitnessHead,
            _: vestrace_domain::WitnessAdvance,
        ) -> Result<WitnessReceipt, WitnessError> {
            panic!("target initialization replay does not advance the witness")
        }
    }

    #[derive(Default)]
    struct RecordingAuthority(Mutex<Vec<vestrace_domain::SafetyJournalDigest>>);

    #[async_trait]
    impl RestoreAttemptAuthority for RecordingAuthority {
        async fn prepare_attempt_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: &RestoreAttemptProgress,
        ) -> Result<(), ApplicationError> {
            unreachable!()
        }

        async fn record_source_freeze_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: &RestoreAttemptProgress,
            _: SourceFreezePoint,
        ) -> Result<(), ApplicationError> {
            unreachable!()
        }

        async fn record_target_initialized_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: &RestoreAttemptProgress,
            receipt: vestrace_domain::SafetyJournalDigest,
        ) -> Result<(), ApplicationError> {
            self.0.lock().unwrap().push(receipt);
            Ok(())
        }

        async fn record_source_resume_prepared_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: &RestoreAttemptProgress,
            _: vestrace_domain::SafetyJournalDigest,
        ) -> Result<(), ApplicationError> {
            unreachable!()
        }

        async fn record_restore_safety_event_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: SignedJournalEntry,
            _: WitnessReceipt,
        ) -> Result<(), ApplicationError> {
            unreachable!()
        }

        async fn release_hold_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: &RestoreTerminalReceipt,
        ) -> Result<(), ApplicationError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn terminal_receipt_replay_restores_only_its_exact_sql_effect() {
        let source = DatabaseGenerationId::new();
        let frozen = RestoreAttemptProgress::prepare(
            RestoreAttemptId::new(),
            RestoreTargetId::new(),
            BackupSetId::new(),
            RestoreHoldId::new(),
            source,
            DatabaseGenerationId::new(),
        )
        .unwrap()
        .record_source_freeze(SourceFreezePoint::new(source, 1, 256, 9).unwrap())
        .unwrap();
        let terminal = RestoreTerminalReceipt::new(
            frozen.attempt_id(),
            frozen.target_id(),
            vestrace_domain::RestoreHoldReleaseReason::TargetInitializationComplete {
                receipt: vestrace_domain::SafetyJournalDigest::of(b"target-ready"),
            },
        );
        let persisted_attempt = frozen.record_terminal_receipt(terminal.clone()).unwrap();
        let authority = Arc::new(RecordingAuthority::default());
        let service = RestoreAttemptAuthorityService::new(
            Arc::new(FakePermit),
            Arc::new(UnusedJournal),
            Arc::new(UnusedWitness),
            authority.clone(),
        );

        service
            .ensure_target_initialized(
                &InstallationSupervisorContext::host_supervisor(),
                &persisted_attempt,
                &terminal,
            )
            .await
            .unwrap();
        assert_eq!(
            authority.0.lock().unwrap().as_slice(),
            &[terminal.release_reason().receipt()]
        );
    }
}
