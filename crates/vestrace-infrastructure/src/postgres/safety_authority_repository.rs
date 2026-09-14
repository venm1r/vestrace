//! PostgreSQL adapter for the two guarded P05 safety-authority functions.
//!
//! This adapter never writes the safety tables directly and deliberately does
//! not use Rust signature verification as a replacement for the database-side
//! verifier. The guarded function is the signature authority.

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, InitializeInstallationSafety, InstallationSafetySnapshot,
    RegisterDatabaseGeneration, SafetyAuthorityRepository, UnitOfWork,
};
use vestrace_domain::{SignedJournalEntry, WitnessReceipt};

use super::{PgStore, transaction::PgScopedTransaction};

#[derive(Clone, Debug)]
pub struct PgSafetyAuthorityRepository {
    store: PgStore,
}

impl PgSafetyAuthorityRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }

    fn transaction(
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<&mut PgScopedTransaction, ApplicationError> {
        unit_of_work
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".to_owned()))
    }
}

#[async_trait]
impl SafetyAuthorityRepository for PgSafetyAuthorityRepository {
    async fn initialize_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        request: InitializeInstallationSafety,
        signed_entry: SignedJournalEntry,
        witness_receipt: WitnessReceipt,
    ) -> Result<InstallationSafetySnapshot, ApplicationError> {
        validate_initialization(&request, &signed_entry, &witness_receipt)?;
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query(
            "SELECT public.vestrace_initialize_installation_safety(\
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(request.bootstrap().digest().as_bytes().as_slice())
        .bind(request.bootstrap().installation_id().as_uuid())
        .bind(request.bootstrap().fingerprint_key_id().as_uuid())
        .bind(request.bootstrap().continuity_proof().as_bytes().as_slice())
        .bind(signed_entry.request_id().as_uuid())
        .bind(
            request
                .bootstrap()
                .journal_public_key()
                .as_bytes()
                .as_slice(),
        )
        .bind(
            request
                .bootstrap()
                .witness_public_key()
                .as_bytes()
                .as_slice(),
        )
        .bind(witness_receipt.sequence() as i64)
        .bind(witness_receipt.journal_digest().as_bytes().as_slice())
        .bind(signed_entry.signature().as_slice())
        .bind(witness_receipt.generation_id().as_uuid())
        .bind(witness_receipt.activation_epoch() as i64)
        .bind(witness_receipt.state().canonical_bytes())
        .bind(witness_receipt.signature().as_slice())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        Ok(snapshot(&signed_entry, &witness_receipt))
    }

    async fn register_generation_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        request: RegisterDatabaseGeneration,
        signed_entry: SignedJournalEntry,
        witness_receipt: WitnessReceipt,
    ) -> Result<InstallationSafetySnapshot, ApplicationError> {
        validate_registration(request, &signed_entry, &witness_receipt)?;
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query(
            "SELECT public.vestrace_register_database_generation(\
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(signed_entry.installation_id().as_uuid())
        .bind(signed_entry.fingerprint_key_id().as_uuid())
        .bind(signed_entry.continuity_proof().as_bytes().as_slice())
        .bind(signed_entry.request_id().as_uuid())
        .bind(witness_receipt.sequence() as i64)
        .bind(witness_receipt.journal_digest().as_bytes().as_slice())
        .bind(signed_entry.signature().as_slice())
        .bind(witness_receipt.generation_id().as_uuid())
        .bind(witness_receipt.activation_epoch() as i64)
        .bind(witness_receipt.state().canonical_bytes())
        .bind(witness_receipt.signature().as_slice())
        .fetch_one(transaction.connection())
        .await
        .map_err(storage_error)?;
        Ok(snapshot(&signed_entry, &witness_receipt))
    }

    async fn current(&self) -> Result<Option<InstallationSafetySnapshot>, ApplicationError> {
        // P05 grants the supervisor only the two mutation functions and no
        // table SELECT privilege. A read surface must be a separately reviewed
        // guarded function, rather than a direct-table exception here.
        let _ = &self.store;
        Err(ApplicationError::Unavailable(
            "guarded installation safety read is not configured".to_owned(),
        ))
    }
}

fn validate_initialization(
    request: &InitializeInstallationSafety,
    entry: &SignedJournalEntry,
    receipt: &WitnessReceipt,
) -> Result<(), ApplicationError> {
    if request.bootstrap().installation_id() != entry.installation_id()
        || request.bootstrap().fingerprint_key_id() != entry.fingerprint_key_id()
        || request.bootstrap().continuity_proof() != entry.continuity_proof()
        || request.bootstrap().journal_public_key() != entry.signer_public_key()
        || request.bootstrap().witness_public_key() != receipt.witness_public_key()
        || request.generation_id() != entry.generation_id()
    {
        return Err(ApplicationError::Conflict(
            "installation safety initialization identities do not match".to_owned(),
        ));
    }
    entry
        .verify()
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
    receipt
        .verify_against(request.bootstrap().witness_public_key())
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
    validate_entry_and_receipt(entry, receipt)
}

fn validate_registration(
    request: RegisterDatabaseGeneration,
    entry: &SignedJournalEntry,
    receipt: &WitnessReceipt,
) -> Result<(), ApplicationError> {
    if request.generation_id() != entry.generation_id()
        || request.activation_epoch() != entry.activation_epoch()
    {
        return Err(ApplicationError::Conflict(
            "database generation request does not match signed entry".to_owned(),
        ));
    }
    entry
        .verify()
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
    // The pinned witness key is held in the guarded singleton and cannot be
    // read by this role. The SQL function verifies this exact receipt against
    // that key before accessing safety rows.
    validate_entry_and_receipt(entry, receipt)
}

fn validate_entry_and_receipt(
    entry: &SignedJournalEntry,
    receipt: &WitnessReceipt,
) -> Result<(), ApplicationError> {
    if receipt.installation_id() != entry.installation_id()
        || receipt.fingerprint_key_id() != entry.fingerprint_key_id()
        || receipt.continuity_proof() != entry.continuity_proof()
        || receipt.sequence() != entry.sequence()
        || receipt.journal_digest() != entry.digest()
        || receipt.generation_id() != entry.generation_id()
        || receipt.activation_epoch() != entry.activation_epoch()
        || receipt.state() != entry.state()
    {
        return Err(ApplicationError::Conflict(
            "witness receipt does not exactly bind the signed entry".to_owned(),
        ));
    }
    Ok(())
}

fn snapshot(entry: &SignedJournalEntry, receipt: &WitnessReceipt) -> InstallationSafetySnapshot {
    InstallationSafetySnapshot::new(
        receipt.installation_id(),
        receipt.sequence(),
        receipt.journal_digest(),
        receipt.generation_id(),
        receipt.activation_epoch(),
        entry.signer_public_key().clone(),
        receipt.witness_public_key().clone(),
    )
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}
