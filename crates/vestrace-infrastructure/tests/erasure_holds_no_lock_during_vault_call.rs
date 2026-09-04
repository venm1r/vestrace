use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use uuid::Uuid;
use vestrace_application::{
    FenceReceipt, MaterialErasureService, MaterialKeyVault, RequestContext, VaultError,
};
use vestrace_domain::{
    ContentMaterialId, CredentialKeyCreationIntentId, ErasureReceipt, IntentNonce, MaterialKeyId,
    PrincipalId, VaultReceipt, WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::{PgMaterialErasureRepository, PgStore};

struct Fixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    intent_id: Uuid,
    material_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
    attachment_id: Uuid,
}

struct CredentialFixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    connection_id: Uuid,
    slot_id: Uuid,
    occupancy_id: Uuid,
    intent_id: Uuid,
    revision_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
    attachment_id: Uuid,
}

#[derive(Default)]
struct VaultGate {
    entered: Mutex<usize>,
    released: Mutex<usize>,
    changed: Condvar,
}

impl VaultGate {
    fn wait_until_entered(&self, call_count: usize) {
        let mut entered = self.entered.lock().unwrap();
        while *entered < call_count {
            entered = self.changed.wait(entered).unwrap();
        }
    }

    fn block_vault_call(&self) {
        let entered_call = {
            let mut entered = self.entered.lock().unwrap();
            *entered += 1;
            *entered
        };
        self.changed.notify_all();
        let mut released = self.released.lock().unwrap();
        while *released < entered_call {
            released = self.changed.wait(released).unwrap();
        }
    }

    fn release(&self) {
        *self.released.lock().unwrap() += 1;
        self.changed.notify_all();
    }
}

struct BlockingVault {
    gate: Arc<VaultGate>,
}

impl MaterialKeyVault for BlockingVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

    fn prepare_erasure(&self, _key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        self.gate.block_vault_call();
        Ok(FenceReceipt::from_uuid(Uuid::from_u128(11)))
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        self.gate.block_vault_call();
        Ok(ErasureReceipt::from_uuid(Uuid::from_u128(12)))
    }
}

fn context(fixture: &Fixture) -> RequestContext {
    RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    )
}

async fn transaction<'a>(pool: &'a PgPool, fixture: &Fixture) -> Transaction<'a, Postgres> {
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

async fn assert_nowait_lock_is_released(
    transaction: &mut Transaction<'_, Postgres>,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
    row_class: &str,
) {
    let result = query.fetch_optional(&mut **transaction).await;
    match result {
        Ok(Some(_)) => {}
        Ok(None) => panic!("the {row_class} row must be visible to the independent session"),
        Err(error) => {
            let sqlstate = error
                .as_database_error()
                .and_then(|database_error| database_error.code())
                .map(|code| code.into_owned());
            panic!(
                "the {row_class} row remained locked during the vault call; expected acquisition, got SQLSTATE {}",
                sqlstate.as_deref().unwrap_or("no SQLSTATE")
            );
        }
    }
}

async fn execute(
    pool: &PgPool,
    fixture: &Fixture,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) {
    let mut transaction = transaction(pool, fixture).await;
    query.execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}

async fn live_content(pool: &PgPool) -> Fixture {
    let fixture = Fixture {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        intent_id: Uuid::now_v7(),
        material_id: Uuid::now_v7(),
        material_key_id: Uuid::now_v7(),
        nonce: Uuid::now_v7(),
        attachment_id: Uuid::now_v7(),
    };
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(fixture.workspace_id)
        .bind(format!("erasure-lock-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!("erasure-lock-principal-{}", fixture.principal_id))
        .execute(pool)
        .await
        .unwrap();
    execute(
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
    execute(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await;
    execute(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    execute(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(vec![0x4C_u8; 4096])
            .bind(4096_i64),
    )
    .await;
    execute(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    execute(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_content_material($1)").bind(fixture.intent_id),
    )
    .await;
    fixture
}

fn credential_context(fixture: &CredentialFixture) -> RequestContext {
    RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    )
}

async fn credential_transaction<'a>(
    pool: &'a PgPool,
    fixture: &CredentialFixture,
) -> Transaction<'a, Postgres> {
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

async fn execute_credential(
    pool: &PgPool,
    fixture: &CredentialFixture,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) {
    let mut transaction = credential_transaction(pool, fixture).await;
    query.execute(&mut *transaction).await.unwrap();
    transaction.commit().await.unwrap();
}

async fn candidate_credential(pool: &PgPool) -> CredentialFixture {
    let fixture = CredentialFixture {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        connection_id: Uuid::now_v7(),
        slot_id: Uuid::now_v7(),
        occupancy_id: Uuid::now_v7(),
        intent_id: Uuid::now_v7(),
        revision_id: Uuid::now_v7(),
        material_key_id: Uuid::now_v7(),
        nonce: Uuid::now_v7(),
        attachment_id: Uuid::now_v7(),
    };
    let connector_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(fixture.workspace_id)
        .bind(format!("erasure-lock-credential-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!(
            "erasure-lock-credential-principal-{}",
            fixture.principal_id
        ))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(fixture.workspace_id)
    .bind(format!("erasure-lock-connector-{connector_id}"))
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
    .bind(format!("erasure-lock-connection-{}", fixture.connection_id))
    .execute(pool)
    .await
    .unwrap();

    let mut transaction = credential_transaction(pool, &fixture).await;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
        .bind(fixture.slot_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind("provider")
        .bind("primary")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
        .bind(fixture.occupancy_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    execute_credential(
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
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await;
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(b"erasure-lock-credential-ciphertext".as_slice()),
    )
    .await;
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await;
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(fixture.intent_id),
    )
    .await;
    fixture
}

#[sqlx::test(migrations = "../../migrations")]
async fn nowait_probe_observes_a_lock_held_by_a_third_session(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let third = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut holder = transaction(&third, &fixture).await;
    sqlx::query("SELECT id FROM content_materials WHERE id = $1 FOR UPDATE")
        .bind(fixture.material_id)
        .fetch_one(&mut *holder)
        .await
        .unwrap();

    let probe_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut probe = transaction(&probe_pool, &fixture).await;
    let error = sqlx::query("SELECT id FROM content_materials WHERE id = $1 FOR UPDATE NOWAIT")
        .bind(fixture.material_id)
        .fetch_one(&mut *probe)
        .await
        .expect_err("the independent NOWAIT probe must observe the third session's lock");
    let sqlstate = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned());
    assert_eq!(sqlstate.as_deref(), Some("55P03"));

    holder.rollback().await.unwrap();
    probe.rollback().await.unwrap();
    third.close().await;
    probe_pool.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn erasure_holds_no_lock_during_vault_call(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let gate = Arc::new(VaultGate::default());
    let service = Arc::new(MaterialErasureService::new(
        PgMaterialErasureRepository::new(PgStore::from_pool(pool.clone())),
        BlockingVault {
            gate: Arc::clone(&gate),
        },
    ));
    let service_task = {
        let context = context(&fixture);
        let service = Arc::clone(&service);
        let material_id = ContentMaterialId::from_uuid(fixture.material_id);
        tokio::spawn(async move { service.erase_content(&context, material_id).await })
    };

    let wait_gate = Arc::clone(&gate);
    tokio::task::spawn_blocking(move || wait_gate.wait_until_entered(1))
        .await
        .unwrap();

    // This pool is constructed from the sqlx test pool's actual connect
    // options, so it reaches the same per-test migrated database while using
    // a distinct PostgreSQL session.
    let independent = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut independent_tx = independent.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *independent_tx)
        .await
        .unwrap();
    let preparation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM material_erasure_preparations WHERE content_material_id = $1",
    )
    .bind(fixture.material_id)
    .fetch_one(&mut *independent_tx)
    .await
    .unwrap();
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM content_materials WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(fixture.material_id),
        "content_materials",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM material_key_creation_intents WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(fixture.intent_id),
        "material_key_creation_intents",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM material_erasure_preparations WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(preparation_id),
        "material_erasure_preparations",
    )
    .await;
    independent_tx.commit().await.unwrap();
    independent.close().await;

    gate.release();
    let wait_gate = Arc::clone(&gate);
    tokio::task::spawn_blocking(move || wait_gate.wait_until_entered(2))
        .await
        .unwrap();

    let independent = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut independent_tx = independent.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *independent_tx)
        .await
        .unwrap();
    let preparation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM material_erasure_preparations WHERE content_material_id = $1",
    )
    .bind(fixture.material_id)
    .fetch_one(&mut *independent_tx)
    .await
    .unwrap();
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM content_materials WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(fixture.material_id),
        "content_materials during the irreversible vault erase",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM material_key_creation_intents WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(fixture.intent_id),
        "material_key_creation_intents during the irreversible vault erase",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM material_erasure_preparations WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(preparation_id),
        "material_erasure_preparations during the irreversible vault erase",
    )
    .await;
    independent_tx.commit().await.unwrap();
    independent.close().await;

    gate.release();
    let receipt = tokio::time::timeout(Duration::from_secs(5), service_task)
        .await
        .expect("the gated vault call must resume")
        .expect("the erasure task must not panic")
        .expect("the erasure finalizer must commit");
    assert_eq!(receipt.as_uuid(), Uuid::from_u128(12));
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_erasure_holds_no_lock_during_vault_call(pool: PgPool) {
    let fixture = candidate_credential(&pool).await;
    let gate = Arc::new(VaultGate::default());
    let service = Arc::new(MaterialErasureService::new(
        PgMaterialErasureRepository::new(PgStore::from_pool(pool.clone())),
        BlockingVault {
            gate: Arc::clone(&gate),
        },
    ));
    let service_task = {
        let context = credential_context(&fixture);
        let service = Arc::clone(&service);
        let intent_id = CredentialKeyCreationIntentId::from_uuid(fixture.intent_id);
        tokio::spawn(async move { service.erase_credential(&context, intent_id).await })
    };

    let wait_gate = Arc::clone(&gate);
    tokio::task::spawn_blocking(move || wait_gate.wait_until_entered(1))
        .await
        .unwrap();

    // This is a separate PostgreSQL session derived from the sqlx test pool's
    // actual database. Every row class acquired by credential phase one must
    // be lockable NOWAIT while the host vault is deliberately blocked.
    let independent = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut independent_tx = credential_transaction(&independent, &fixture).await;
    let preparation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM material_erasure_preparations WHERE credential_intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&mut *independent_tx)
    .await
    .unwrap();

    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query(
            "SELECT id FROM connection_execution_guards \
             WHERE workspace_id = $1 AND connection_id = $2 FOR UPDATE NOWAIT",
        )
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id),
        "connection_execution_guards",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query(
            "SELECT id FROM credential_activation_guards \
             WHERE workspace_id = $1 AND connection_id = $2 AND credential_slot_id = $3 FOR UPDATE NOWAIT",
        )
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id),
        "credential_activation_guards",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query(
            "SELECT id FROM credential_slots \
             WHERE workspace_id = $1 AND connection_id = $2 AND id = $3 FOR UPDATE NOWAIT",
        )
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id),
        "credential_slots",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM credential_guard_occupancies WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(fixture.occupancy_id),
        "credential_guard_occupancies",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query(
            "SELECT id FROM credential_key_creation_intents WHERE id = $1 FOR UPDATE NOWAIT",
        )
        .bind(fixture.intent_id),
        "credential_key_creation_intents",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM credential_revisions WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(fixture.revision_id),
        "credential_revisions",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query(
            "SELECT id FROM credential_prepared_materials WHERE intent_id = $1 FOR UPDATE NOWAIT",
        )
        .bind(fixture.intent_id),
        "credential_prepared_materials",
    )
    .await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query("SELECT id FROM material_erasure_preparations WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(preparation_id),
        "material_erasure_preparations",
    )
    .await;
    independent_tx.commit().await.unwrap();
    independent.close().await;

    gate.release();
    let wait_gate = Arc::clone(&gate);
    tokio::task::spawn_blocking(move || wait_gate.wait_until_entered(2))
        .await
        .unwrap();

    let independent = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let mut independent_tx = credential_transaction(&independent, &fixture).await;
    assert_nowait_lock_is_released(
        &mut independent_tx,
        sqlx::query(
            "SELECT intent.id \
             FROM connection_execution_guards AS execution \
             JOIN credential_activation_guards AS activation \
               ON activation.workspace_id = execution.workspace_id \
              AND activation.connection_id = execution.connection_id \
             JOIN credential_slots AS slot \
               ON slot.workspace_id = activation.workspace_id \
              AND slot.connection_id = activation.connection_id \
              AND slot.id = activation.credential_slot_id \
             JOIN credential_guard_occupancies AS occupancy \
               ON occupancy.activation_guard_id = activation.id \
             JOIN credential_key_creation_intents AS intent \
               ON intent.occupancy_id = occupancy.id \
             JOIN credential_revisions AS revision \
               ON revision.id = intent.credential_revision_id \
             JOIN credential_prepared_materials AS prepared \
               ON prepared.intent_id = intent.id \
             JOIN material_erasure_preparations AS erasure \
               ON erasure.credential_intent_id = intent.id \
             WHERE intent.id = $1 \
             FOR UPDATE OF execution, activation, slot, occupancy, intent, revision, prepared, erasure NOWAIT",
        )
        .bind(fixture.intent_id),
        "every credential erasure row during the irreversible vault erase",
    )
    .await;
    independent_tx.commit().await.unwrap();
    independent.close().await;

    gate.release();
    let receipt = tokio::time::timeout(Duration::from_secs(5), service_task)
        .await
        .expect("the gated vault call must resume")
        .expect("the erasure task must not panic")
        .expect("the erasure finalizer must commit");
    assert_eq!(receipt.as_uuid(), Uuid::from_u128(12));
}
