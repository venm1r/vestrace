//! Crash boundaries of the two key-creation lifecycles.
//!
//! Every guarded transition commits its own transaction, so the database state
//! after a crash at boundary N is exactly the committed state after step N.
//! Each test drives the lifecycle to one boundary, stops as the crashed process
//! would have, and then asks the resumption what it can lawfully do with what
//! survived. The real harness contract — an ephemeral database and a process
//! that actually aborts — is exercised separately by the fault-scenario binary.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use vestrace_application::{
    CredentialIntentResumption, CredentialResumptionOutcome, FenceReceipt,
    MaterialIntentResumption, MaterialKeyVault, RequestContext, ResumptionOutcome, VaultError,
};
use vestrace_domain::{
    ContentMaterialId, CredentialKeyCreationIntentId, CredentialKeyCreationIntentState,
    ErasureReceipt, IntentNonce, MaterialKeyCreationIntentId, MaterialKeyCreationIntentState,
    MaterialKeyId, PrincipalId, VaultReceipt, WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::{PgCredentialIntentRepository, PgMaterialIntentRepository, PgStore};

// ─── a vault that refuses to lose track of a key ────────────────────────────

#[derive(Default)]
struct VaultKey {
    nonce: Uuid,
    receipt: Uuid,
    fenced: bool,
    erased: Option<Uuid>,
}

/// Counts every key it mints and every erasure it witnesses.
///
/// "Creates no second vault key" is only checkable against something that
/// remembers, so this double keeps the real adapter's identity rules: creation
/// is idempotent on the exact `(key_id, nonce)` pair, a different nonce for a
/// known key is refused, and erasure returns its original receipt on replay.
#[derive(Default)]
struct CountingVault {
    keys: Mutex<HashMap<Uuid, VaultKey>>,
    creates: Mutex<u32>,
    erasures: Mutex<u32>,
}

impl CountingVault {
    fn distinct_keys(&self) -> usize {
        self.keys.lock().unwrap().len()
    }

    fn create_calls_that_minted(&self) -> u32 {
        *self.creates.lock().unwrap()
    }

    fn erasures_performed(&self) -> u32 {
        *self.erasures.lock().unwrap()
    }

    fn seed(&self, key_id: Uuid, nonce: Uuid) -> Uuid {
        self.create_if_absent(
            MaterialKeyId::from_uuid(key_id),
            IntentNonce::from_uuid(nonce),
        )
        .expect("seeding a vault key must succeed")
        .as_uuid()
    }
}

impl MaterialKeyVault for CountingVault {
    fn create_if_absent(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        let mut keys = self.keys.lock().unwrap();
        if let Some(existing) = keys.get(&key_id.as_uuid()) {
            if existing.nonce != nonce.as_uuid() {
                return Err(VaultError::NonceMismatch);
            }
            return Ok(VaultReceipt::from_uuid(existing.receipt));
        }
        let receipt = Uuid::now_v7();
        keys.insert(
            key_id.as_uuid(),
            VaultKey {
                nonce: nonce.as_uuid(),
                receipt,
                fenced: false,
                erased: None,
            },
        );
        *self.creates.lock().unwrap() += 1;
        Ok(VaultReceipt::from_uuid(receipt))
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

    fn prepare_erasure(&self, key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        let mut keys = self.keys.lock().unwrap();
        let key = keys
            .get_mut(&key_id.as_uuid())
            .ok_or(VaultError::NotFound)?;
        if key.erased.is_some() {
            return Err(VaultError::Erased);
        }
        if key.fenced {
            return Err(VaultError::ErasurePrepared);
        }
        key.fenced = true;
        Ok(FenceReceipt::new())
    }

    fn erase(&self, key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        let mut keys = self.keys.lock().unwrap();
        let key = keys
            .get_mut(&key_id.as_uuid())
            .ok_or(VaultError::NotFound)?;
        if let Some(receipt) = key.erased {
            return Ok(ErasureReceipt::from_uuid(receipt));
        }
        if !key.fenced {
            return Err(VaultError::ErasureNotPrepared);
        }
        let receipt = Uuid::now_v7();
        key.erased = Some(receipt);
        *self.erasures.lock().unwrap() += 1;
        Ok(ErasureReceipt::from_uuid(receipt))
    }
}

// ─── shared plumbing ────────────────────────────────────────────────────────

struct Fixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    intent_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
    // material only
    material_id: Uuid,
    attachment_id: Uuid,
    // credential only
    connection_id: Uuid,
    slot_id: Uuid,
    occupancy_id: Uuid,
    revision_id: Uuid,
}

impl Fixture {
    fn new() -> Self {
        Self {
            workspace_id: Uuid::now_v7(),
            principal_id: Uuid::now_v7(),
            intent_id: Uuid::now_v7(),
            material_key_id: Uuid::now_v7(),
            nonce: Uuid::now_v7(),
            material_id: Uuid::now_v7(),
            attachment_id: Uuid::now_v7(),
            connection_id: Uuid::now_v7(),
            slot_id: Uuid::now_v7(),
            occupancy_id: Uuid::now_v7(),
            revision_id: Uuid::now_v7(),
        }
    }

    fn context(&self) -> RequestContext {
        RequestContext::new(
            WorkspaceId::from_uuid(self.workspace_id),
            PrincipalId::from_uuid(self.principal_id),
        )
    }
}

async fn scoped<'a>(pool: &'a PgPool, fixture: &Fixture) -> Transaction<'a, Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    for (setting, value) in [
        ("vestrace.workspace_id", fixture.workspace_id.to_string()),
        ("vestrace.principal_id", fixture.principal_id.to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(setting)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    transaction
}

async fn guarded(
    pool: &PgPool,
    fixture: &Fixture,
    query: sqlx::query::Query<'_, Postgres, sqlx::postgres::PgArguments>,
) {
    let mut transaction = scoped(pool, fixture).await;
    query.execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}

async fn tenancy(pool: &PgPool, fixture: &Fixture, label: &str) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(fixture.workspace_id)
        .bind(format!("{label}-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!("{label}-principal-{}", fixture.principal_id))
        .execute(pool)
        .await
        .unwrap();
}

async fn material_state(pool: &PgPool, fixture: &Fixture) -> String {
    sqlx::query_scalar("SELECT state FROM material_key_creation_intents WHERE id = $1")
        .bind(fixture.intent_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn credential_state(pool: &PgPool, fixture: &Fixture) -> String {
    sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
        .bind(fixture.intent_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// One intent identity, and never a second one under the same reserved key.
async fn intents_for_key(pool: &PgPool, fixture: &Fixture, table: &str) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {table} WHERE material_key_id = $1"
    ))
    .bind(fixture.material_key_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ─── material lifecycle ─────────────────────────────────────────────────────

/// Drives the material lifecycle to one boundary and stops there, exactly as a
/// process that died at that point would have left it.
async fn material_at(pool: &PgPool, boundary: &str, vault: &CountingVault) -> Fixture {
    let fixture = Fixture::new();
    tenancy(pool, &fixture, "material-crash").await;

    guarded(
        pool,
        &fixture,
        sqlx::query(
            "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(fixture.intent_id)
        .bind(fixture.workspace_id)
        .bind(fixture.material_id)
        .bind(fixture.material_key_id)
        .bind(fixture.nonce)
        .bind("content")
        .bind(fixture.principal_id)
        .bind(0_i64),
    )
    .await;
    if boundary == "after_reserved" {
        return fixture;
    }

    // The vault write lands before the database row: that asymmetry is the
    // whole point of this boundary.
    vault.seed(fixture.material_key_id, fixture.nonce);
    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await;
    if boundary == "after_vault_create_before_receipt" {
        return fixture;
    }

    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    if boundary == "after_receipt_before_prepared" {
        return fixture;
    }

    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(vec![0x41_u8; 4096])
            .bind(4096_i64),
    )
    .await;
    if boundary == "after_prepared_before_bound" {
        return fixture;
    }

    if boundary == "after_abort_before_witnessed_erase"
        || boundary == "after_erase_receipt_before_terminal_append"
    {
        guarded(
            pool,
            &fixture,
            sqlx::query("SELECT vestrace_prepare_content_abandon($1)").bind(fixture.intent_id),
        )
        .await;
        if boundary == "after_abort_before_witnessed_erase" {
            return fixture;
        }
        // The key was erased and its receipt committed; only the terminal
        // append is missing.
        let key = MaterialKeyId::from_uuid(fixture.material_key_id);
        vault.prepare_erasure(key).unwrap();
        let receipt = vault.erase(key).unwrap();
        guarded(
            pool,
            &fixture,
            sqlx::query("SELECT vestrace_record_unbound_material_key_erasure($1, $2)")
                .bind(fixture.intent_id)
                .bind(receipt.as_uuid()),
        )
        .await;
        return fixture;
    }

    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    fixture
}

/// Drives the owner-only ResultPrepared transition at the same persisted
/// boundary as `after_prepared_before_bound`. Runtime must not be able to
/// enter this state, but an owner-issued transition remains observable and is
/// deliberately parked because it needs a bind receipt.
async fn material_result_prepared_at_boundary(pool: &PgPool, vault: &CountingVault) -> Fixture {
    let fixture = Fixture::new();
    tenancy(pool, &fixture, "material-result-prepared-crash").await;

    guarded(
        pool,
        &fixture,
        sqlx::query(
            "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(fixture.intent_id)
        .bind(fixture.workspace_id)
        .bind(fixture.material_id)
        .bind(fixture.material_key_id)
        .bind(fixture.nonce)
        .bind("result")
        .bind(fixture.principal_id)
        .bind(0_i64),
    )
    .await;
    vault.seed(fixture.material_key_id, fixture.nonce);
    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await;
    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_prepare_result_material($1, $2, $3, $4)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(vec![0x52_u8; 4096])
            .bind(4096_i64),
    )
    .await;
    fixture
}

async fn resume_material(
    pool: &PgPool,
    fixture: &Fixture,
    vault: Arc<CountingVault>,
) -> ResumptionOutcome {
    MaterialIntentResumption::new(
        PgMaterialIntentRepository::new(PgStore::from_pool(pool.clone())),
        vault,
    )
    .resume(
        &fixture.context(),
        MaterialKeyCreationIntentId::from_uuid(fixture.intent_id),
    )
    .await
    .expect("resumption must not fail")
}

/// Asserts the shape every material boundary shares: the surviving state is the
/// expected one, and resumption is replayable without a second identity or key.
async fn material_boundary(
    pool: &PgPool,
    boundary: &str,
    expected_survivor: &str,
) -> (ResumptionOutcome, Fixture, Arc<CountingVault>) {
    let vault = Arc::new(CountingVault::default());
    let fixture = material_at(pool, boundary, &vault).await;
    assert_eq!(
        material_state(pool, &fixture).await,
        expected_survivor,
        "{boundary} must leave the intent in {expected_survivor}"
    );

    let keys_before = vault.distinct_keys();
    let outcome = resume_material(pool, &fixture, Arc::clone(&vault)).await;
    let replayed = resume_material(pool, &fixture, Arc::clone(&vault)).await;

    assert_eq!(
        intents_for_key(pool, &fixture, "material_key_creation_intents").await,
        1,
        "{boundary} resumption must not create a second intent identity"
    );
    assert!(
        vault.distinct_keys() <= keys_before.max(1),
        "{boundary} resumption must not mint a second vault key"
    );
    assert!(
        vault.erasures_performed() <= 1,
        "{boundary} must never erase the same key twice"
    );
    // Replay is the crash-safety property: a reconciler that runs twice must
    // reach the same place, not push the intent further.
    match (&outcome, &replayed) {
        (
            ResumptionOutcome::Parked { state: first, .. },
            ResumptionOutcome::Parked { state: second, .. },
        ) => {
            assert_eq!(first, second, "{boundary} replay changed a parked state")
        }
        (ResumptionOutcome::Terminal(first), ResumptionOutcome::AlreadyTerminal(second)) => {
            assert_eq!(first, second, "{boundary} replay moved past its terminal")
        }
        (first, second) => {
            panic!("{boundary} replay was not idempotent: {first:?} then {second:?}")
        }
    }
    (outcome, fixture, vault)
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_reserved_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) = material_boundary(&pool, "after_reserved", "reserved").await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Abandoned)
    );
    assert_eq!(material_state(&pool, &fixture).await, "abandoned");
    // The crash landed before the vault was reached, so the resumption resolves
    // that ambiguity by creating the reserved identity and erasing it, rather
    // than recording a receipt nobody witnessed.
    assert_eq!(vault.create_calls_that_minted(), 1);
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_vault_create_before_receipt_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) = material_boundary(
        &pool,
        "after_vault_create_before_receipt",
        "provisional_created",
    )
    .await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Abandoned)
    );
    assert_eq!(material_state(&pool, &fixture).await, "abandoned");
    // The key the crashed process created is reused and erased, not replaced:
    // this is the boundary where the host holds a key the database never
    // received a receipt for.
    assert_eq!(vault.create_calls_that_minted(), 1);
    assert_eq!(vault.distinct_keys(), 1);
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_receipt_before_prepared_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) = material_boundary(
        &pool,
        "after_receipt_before_prepared",
        "provisional_receipted",
    )
    .await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Abandoned)
    );
    assert_eq!(material_state(&pool, &fixture).await, "abandoned");
    assert_eq!(vault.erasures_performed(), 1);
    // The pre-prepared branch is distinct: it never carries a ContentPrepared
    // marker, and the spec refuses to let one stand in for it.
    let marker: Option<String> = sqlx::query_scalar(
        "SELECT prepared_marker FROM material_key_creation_intents WHERE id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(marker, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_prepared_before_bound_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) =
        material_boundary(&pool, "after_prepared_before_bound", "content_prepared").await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Abandoned)
    );
    assert_eq!(material_state(&pool, &fixture).await, "abandoned");
    assert_eq!(vault.erasures_performed(), 1);
    let ciphertext: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_material_bytes WHERE intent_id = $1")
            .bind(fixture.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ciphertext, 0, "abandonment removes the prepared ciphertext");
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_result_prepared_crash_after_prepared_before_bound_is_parked(pool: PgPool) {
    let vault = Arc::new(CountingVault::default());
    let fixture = material_result_prepared_at_boundary(&pool, &vault).await;
    assert_eq!(material_state(&pool, &fixture).await, "result_prepared");

    let outcome = resume_material(&pool, &fixture, Arc::clone(&vault)).await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Parked {
            state: MaterialKeyCreationIntentState::ResultPrepared,
            reason: "ResultPrepared always binds and never abandons; its bind receipt is owned by the job that crashed",
        }
    );
    assert_eq!(material_state(&pool, &fixture).await, "result_prepared");
    assert_eq!(vault.distinct_keys(), 1);
    assert_eq!(vault.erasures_performed(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_bound_before_promotion_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) =
        material_boundary(&pool, "after_bound_before_promotion", "bound").await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Live)
    );
    assert_eq!(material_state(&pool, &fixture).await, "live");
    // Completing a committed bind must not erase the key it just published.
    assert_eq!(vault.erasures_performed(), 0);

    let mut transaction = scoped(&pool, &fixture).await;
    let live: bool = sqlx::query_scalar("SELECT vestrace_content_material_is_live($1)")
        .bind(fixture.material_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert!(live, "the promoted material must be Live");
    let _ = ContentMaterialId::from_uuid(fixture.material_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_abort_before_witnessed_erase_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) = material_boundary(
        &pool,
        "after_abort_before_witnessed_erase",
        "content_abandon_prepared",
    )
    .await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Abandoned)
    );
    assert_eq!(material_state(&pool, &fixture).await, "abandoned");
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn material_crash_after_erase_receipt_before_terminal_append_is_resumable(pool: PgPool) {
    let (outcome, fixture, vault) = material_boundary(
        &pool,
        "after_erase_receipt_before_terminal_append",
        "content_abandon_prepared",
    )
    .await;
    assert_eq!(
        outcome,
        ResumptionOutcome::Terminal(MaterialKeyCreationIntentState::Abandoned)
    );
    assert_eq!(material_state(&pool, &fixture).await, "abandoned");
    // The receipt was already witnessed before the crash. Resumption appends
    // the terminal and must not start a second erasure.
    assert_eq!(
        vault.erasures_performed(),
        1,
        "a committed erase receipt must not be re-earned"
    );
}

// ─── credential lifecycle ───────────────────────────────────────────────────

async fn credential_at(pool: &PgPool, boundary: &str, vault: &CountingVault) -> Fixture {
    let fixture = Fixture::new();
    tenancy(pool, &fixture, "credential-crash").await;
    let connector_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(fixture.workspace_id)
    .bind(format!("credential-crash-connector-{connector_id}"))
    .bind("openai")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(fixture.connection_id)
    .bind(connector_id)
    .bind(fixture.workspace_id)
    .bind(fixture.principal_id)
    .bind(format!(
        "credential-crash-connection-{}",
        fixture.connection_id
    ))
    .execute(pool)
    .await
    .unwrap();

    let mut transaction = scoped(pool, &fixture).await;
    for query in [
        sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
            .bind(Uuid::now_v7())
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id),
        sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
            .bind(fixture.slot_id)
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind("provider")
            .bind("primary"),
        sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
            .bind(Uuid::now_v7())
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(fixture.slot_id),
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
            .bind(fixture.occupancy_id)
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(fixture.slot_id),
    ] {
        query.execute(&mut *transaction).await.unwrap();
    }
    transaction.commit().await.unwrap();

    guarded(
        pool,
        &fixture,
        sqlx::query(
            "SELECT vestrace_reserve_credential_key_creation_intent(\
             $1, $2, $3, $4, $5, $6, $7, $8, 'credential_v2')",
        )
        .bind(fixture.intent_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id)
        .bind(fixture.occupancy_id)
        .bind(fixture.revision_id)
        .bind(fixture.material_key_id)
        .bind(fixture.nonce),
    )
    .await;
    if boundary == "after_reserved" {
        return fixture;
    }

    vault.seed(fixture.material_key_id, fixture.nonce);
    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await;
    if boundary == "after_vault_create_before_receipt" {
        return fixture;
    }

    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    if boundary == "after_receipt_before_prepared" {
        return fixture;
    }

    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(b"encrypted credential material".as_slice()),
    )
    .await;
    if boundary == "after_prepared_before_bound" {
        return fixture;
    }

    if boundary == "after_abort_before_witnessed_erase"
        || boundary == "after_erase_receipt_before_terminal_append"
    {
        guarded(
            pool,
            &fixture,
            sqlx::query("SELECT vestrace_prepare_credential_pre_live_abandon($1, $2)")
                .bind(fixture.intent_id)
                .bind(1_i64),
        )
        .await;
        if boundary == "after_abort_before_witnessed_erase" {
            return fixture;
        }
        let key = MaterialKeyId::from_uuid(fixture.material_key_id);
        vault.prepare_erasure(key).unwrap();
        let receipt = vault.erase(key).unwrap();
        guarded(
            pool,
            &fixture,
            sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
                .bind(fixture.intent_id)
                .bind(receipt.as_uuid()),
        )
        .await;
        return fixture;
    }

    guarded(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    fixture
}

async fn resume_credential(
    pool: &PgPool,
    fixture: &Fixture,
    vault: Arc<CountingVault>,
) -> CredentialResumptionOutcome {
    CredentialIntentResumption::new(
        PgCredentialIntentRepository::new(PgStore::from_pool(pool.clone())),
        vault,
    )
    .resume(
        &fixture.context(),
        CredentialKeyCreationIntentId::from_uuid(fixture.intent_id),
    )
    .await
    .expect("resumption must not fail")
}

async fn credential_boundary(
    pool: &PgPool,
    boundary: &str,
    expected_survivor: &str,
    expected_terminal: CredentialKeyCreationIntentState,
) -> (Fixture, Arc<CountingVault>) {
    let vault = Arc::new(CountingVault::default());
    let fixture = credential_at(pool, boundary, &vault).await;
    assert_eq!(
        credential_state(pool, &fixture).await,
        expected_survivor,
        "{boundary} must leave the intent in {expected_survivor}"
    );

    let outcome = resume_credential(pool, &fixture, Arc::clone(&vault)).await;
    assert_eq!(
        outcome,
        CredentialResumptionOutcome::Terminal(expected_terminal),
        "{boundary} must reach its lawful terminal"
    );

    let replayed = resume_credential(pool, &fixture, Arc::clone(&vault)).await;
    assert_eq!(
        replayed,
        CredentialResumptionOutcome::AlreadyTerminal(expected_terminal),
        "{boundary} replay must not move past its terminal"
    );
    assert_eq!(
        intents_for_key(pool, &fixture, "credential_key_creation_intents").await,
        1,
        "{boundary} resumption must not create a second intent identity"
    );
    assert_eq!(
        vault.distinct_keys(),
        1,
        "{boundary} resumption must reuse the reserved key and mint no other"
    );
    assert!(
        vault.erasures_performed() <= 1,
        "{boundary} must never erase the same key twice"
    );
    (fixture, vault)
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_reserved_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_reserved",
        "reserved",
        CredentialKeyCreationIntentState::Abandoned,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "abandoned");
    // The crash landed before the vault was reached, so resumption resolves the
    // ambiguity by creating the reserved identity and then erasing it, rather
    // than recording a receipt nobody witnessed.
    assert_eq!(vault.create_calls_that_minted(), 1);
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_vault_create_before_receipt_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_vault_create_before_receipt",
        "provisional_created",
        CredentialKeyCreationIntentState::Abandoned,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "abandoned");
    assert_eq!(
        vault.create_calls_that_minted(),
        1,
        "the key the crashed process created must be reused, not replaced"
    );
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_receipt_before_prepared_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_receipt_before_prepared",
        "provisional_receipted",
        CredentialKeyCreationIntentState::Abandoned,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "abandoned");
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_prepared_before_bound_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_prepared_before_bound",
        "credential_prepared",
        CredentialKeyCreationIntentState::Abandoned,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "abandoned");
    let ciphertext: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ciphertext, 0, "abandonment removes the prepared ciphertext");
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_bound_before_promotion_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_bound_before_promotion",
        "bound",
        CredentialKeyCreationIntentState::Candidate,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "candidate");
    assert_eq!(
        vault.erasures_performed(),
        0,
        "completing a committed bind must not erase the key it publishes"
    );
    let occupancy: String =
        sqlx::query_scalar("SELECT state FROM credential_guard_occupancies WHERE id = $1")
            .bind(fixture.occupancy_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(occupancy, "candidate");
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_abort_before_witnessed_erase_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_abort_before_witnessed_erase",
        "credential_abandon_prepared",
        CredentialKeyCreationIntentState::Abandoned,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "abandoned");
    assert_eq!(vault.erasures_performed(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_crash_after_erase_receipt_before_terminal_append_is_resumable(pool: PgPool) {
    let (fixture, vault) = credential_boundary(
        &pool,
        "after_erase_receipt_before_terminal_append",
        "credential_abandon_prepared",
        CredentialKeyCreationIntentState::Abandoned,
    )
    .await;
    assert_eq!(credential_state(&pool, &fixture).await, "abandoned");
    assert_eq!(
        vault.erasures_performed(),
        1,
        "a committed erase receipt must not be re-earned"
    );
}
