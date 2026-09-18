use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, ConnectionAuth, CredentialDispatchLeaseRepository,
    CredentialDispatchLeaseRequest, RequestContext,
};
use vestrace_domain::{
    ConnectionId, CredentialKeyCreationIntentId, CredentialRevisionId, CredentialSlotId,
    IntentNonce, MaterialKeyId, PrincipalId, WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::crypto::{CredentialMaterialCodec, CredentialMaterialContext};
use vestrace_infrastructure::{PgCredentialDispatchLeaseRepository, PgStore};

#[test]
fn credential_v2_codec_is_bounded_and_binds_every_identity() {
    let codec = CredentialMaterialCodec::new();
    let context = CredentialMaterialContext {
        profile: "credential_v2",
        workspace_id: WorkspaceId::new(),
        connection_id: ConnectionId::new(),
        credential_slot_id: CredentialSlotId::new(),
        credential_revision_id: CredentialRevisionId::new(),
        material_key_id: MaterialKeyId::new(),
        intent_id: CredentialKeyCreationIntentId::new(),
        intent_nonce: IntentNonce::new(),
    };
    let dek = ZeroizingDek::new([0x39; 32]);
    let frame = codec
        .seal(&context, &dek, b"operator-credential-sentinel")
        .expect("credential_v2 must seal");
    assert_eq!(frame.len(), 4096);
    assert_eq!(&frame[..4], b"VCRF");
    assert_eq!(frame[4], 2);
    assert_eq!(
        codec.open(&context, &dek, &frame).unwrap().as_slice(),
        b"operator-credential-sentinel"
    );

    for wrong_context in [
        CredentialMaterialContext {
            profile: "legacy_v1",
            ..context
        },
        CredentialMaterialContext {
            workspace_id: WorkspaceId::new(),
            ..context
        },
        CredentialMaterialContext {
            connection_id: ConnectionId::new(),
            ..context
        },
        CredentialMaterialContext {
            credential_slot_id: CredentialSlotId::new(),
            ..context
        },
        CredentialMaterialContext {
            credential_revision_id: CredentialRevisionId::new(),
            ..context
        },
        CredentialMaterialContext {
            material_key_id: MaterialKeyId::new(),
            ..context
        },
        CredentialMaterialContext {
            intent_id: CredentialKeyCreationIntentId::new(),
            ..context
        },
        CredentialMaterialContext {
            intent_nonce: IntentNonce::new(),
            ..context
        },
    ] {
        assert!(codec.open(&wrong_context, &dek, &frame).is_err());
    }
    let mut tampered = frame.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(codec.open(&context, &dek, &tampered).is_err());
    let mut wrong_magic = frame.clone();
    wrong_magic[0] ^= 1;
    assert!(codec.validate_frame(&wrong_magic).is_err());
    let mut wrong_version = frame.clone();
    wrong_version[4] = 1;
    assert!(codec.validate_frame(&wrong_version).is_err());
    assert!(codec.validate_frame(&frame[..frame.len() - 1]).is_err());
    assert!(codec.seal(&context, &dek, b"").is_err());
    assert!(codec.seal(&context, &dek, &[0xff]).is_err());
    let largest = vec![b'x'; 65_495];
    assert_eq!(codec.seal(&context, &dek, &largest).unwrap().len(), 65_536);
    assert!(codec.seal(&context, &dek, &vec![b'x'; 65_496]).is_err());
    assert!(codec.validate_frame(&vec![0_u8; 64 * 1024 + 1]).is_err());
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn credential_dispatch_lease_entrypoints_are_present(pool: PgPool) {
    let entrypoints: Vec<String> = sqlx::query_scalar(
        "SELECT p.oid::regprocedure::text \
           FROM pg_proc AS p \
           JOIN pg_namespace AS n ON n.oid = p.pronamespace \
          WHERE n.nspname = 'public' \
            AND p.oid::regprocedure::text = ANY($1) \
          ORDER BY p.oid::regprocedure::text",
    )
    .bind([
        "vestrace_consume_credential_dispatch_lease(uuid,uuid,uuid,uuid)",
        "vestrace_issue_credential_dispatch_lease(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,text,timestamp with time zone)",
    ])
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(entrypoints.len(), 2);
}

#[test]
fn dispatch_lease_request_keeps_the_exact_lease_tuple() {
    let request = CredentialDispatchLeaseRequest {
        lease_id: Uuid::now_v7(),
        connection_id: ConnectionId::new(),
        external_effect_id: Uuid::now_v7(),
        authorization_id: Uuid::now_v7(),
        credential_slot_id: CredentialSlotId::new(),
        credential_revision_id: Uuid::now_v7(),
        credential_activation_guard_id: Uuid::now_v7(),
        destination_authority: "api.example.test".to_owned(),
        auth_mode: "bearer".to_owned(),
        expires_at: chrono::Utc::now() + ChronoDuration::minutes(5),
    };
    assert_ne!(request.lease_id, Uuid::nil());
    assert_ne!(WorkspaceId::new().as_uuid(), Uuid::nil());
}

#[test]
fn dispatch_lease_repository_is_constructible_from_the_postgres_store() {
    fn takes_repository<V: vestrace_application::MaterialKeyVault>(
        _repository: PgCredentialDispatchLeaseRepository<V>,
    ) {
    }
    let _ = takes_repository::<UnimplementedVault>;
    let _ = std::any::TypeId::of::<PgStore>();
}

struct UnimplementedVault;

impl vestrace_application::MaterialKeyVault for UnimplementedVault {
    fn create_if_absent(
        &self,
        _: vestrace_domain::MaterialKeyId,
        _: vestrace_domain::IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn unwrap(
        &self,
        _: vestrace_domain::MaterialKeyId,
        _: &mut dyn FnMut(&vestrace_domain::ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        unreachable!()
    }

    fn prepare_erasure(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn erase(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
}

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

struct LeaseFixture {
    workspace_id: Uuid,
    connection_id: Uuid,
    slot_id: Uuid,
    revision_id: Uuid,
    activation_guard_id: Uuid,
}

#[derive(sqlx::FromRow)]
struct PersistedLease {
    id: Uuid,
    workspace_id: Uuid,
    connection_id: Uuid,
    external_effect_id: Uuid,
    authorization_id: Uuid,
    credential_slot_id: Uuid,
    credential_revision_id: Uuid,
    destination_authority: String,
    auth_mode: String,
    issued_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
    consumed_at: Option<chrono::DateTime<Utc>>,
    terminal_state: Option<String>,
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let url = std::env::var(RUNTIME_DATABASE_URL_ENV).unwrap();
    let parsed = PgConnectOptions::from_str(&url).unwrap();
    let password = url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password)
        .unwrap();
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(parsed.get_username())
                .password(password),
        )
        .await
        .unwrap();
    let role = sqlx::query(
        "SELECT current_user::text AS role, rolsuper, rolbypassrls \
           FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(role.get::<String, _>("role"), "vestrace");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
    runtime
}

async fn set_workspace(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    workspace_id: Uuid,
) {
    sqlx::query("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(workspace_id.to_string())
        .execute(executor)
        .await
        .unwrap();
}

async fn active_lease_fixture(pool: &PgPool) -> LeaseFixture {
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let execution_guard_id = Uuid::now_v7();
    let activation_guard_id = Uuid::now_v7();
    let slot_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let occupancy_id = Uuid::now_v7();
    let intent_id = Uuid::now_v7();
    let material_key_id = Uuid::now_v7();
    let nonce = Uuid::now_v7();

    sqlx::query("INSERT INTO workspaces (id,slug) VALUES ($1,$2)")
        .bind(workspace_id)
        .bind(format!("lease-{workspace_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id,workspace_id,identifier) VALUES ($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("lease-{principal_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id,workspace_id,name,provider_type) VALUES ($1,$2,$3,'local')",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("lease-{connector_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connections (id,connector_id,workspace_id,principal_id,name) VALUES ($1,$2,$3,$4,$5)")
        .bind(connection_id).bind(connector_id).bind(workspace_id).bind(principal_id).bind(format!("lease-{connection_id}")).execute(pool).await.unwrap();

    let mut setup = pool.begin().await.unwrap();
    set_workspace(&mut *setup, workspace_id).await;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1,$2,$3)")
        .bind(execution_guard_id)
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1,$2,$3,'provider','primary')")
        .bind(slot_id)
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1,$2,$3,$4)")
        .bind(activation_guard_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind(slot_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("INSERT INTO credential_revisions (id,workspace_id,credential_slot_id,material_key_id,associated_data_profile) VALUES ($1,$2,$3,$4,'credential_v2')")
        .bind(revision_id).bind(workspace_id).bind(slot_id).bind(material_key_id).execute(&mut *setup).await.unwrap();
    sqlx::query("INSERT INTO credential_guard_occupancies (id,workspace_id,connection_id,credential_slot_id,activation_guard_id,state) VALUES ($1,$2,$3,$4,$5,'activated')")
        .bind(occupancy_id).bind(workspace_id).bind(connection_id).bind(slot_id).bind(activation_guard_id).execute(&mut *setup).await.unwrap();
    sqlx::query("INSERT INTO credential_key_creation_intents (id,workspace_id,connection_id,credential_slot_id,occupancy_id,credential_revision_id,material_key_id,nonce,state,vault_receipt,bound_receipt) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'active',$9,$10)")
        .bind(intent_id).bind(workspace_id).bind(connection_id).bind(slot_id).bind(occupancy_id).bind(revision_id).bind(material_key_id).bind(nonce).bind(Uuid::now_v7()).bind(Uuid::now_v7()).execute(&mut *setup).await.unwrap();
    sqlx::query("UPDATE credential_guard_occupancies SET intent_id=$2 WHERE id=$1")
        .bind(occupancy_id)
        .bind(intent_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    let frame = CredentialMaterialCodec::new()
        .seal(
            &CredentialMaterialContext {
                profile: "credential_v2",
                workspace_id: WorkspaceId::from_uuid(workspace_id),
                connection_id: ConnectionId::from_uuid(connection_id),
                credential_slot_id: CredentialSlotId::from_uuid(slot_id),
                credential_revision_id: CredentialRevisionId::from_uuid(revision_id),
                material_key_id: MaterialKeyId::from_uuid(material_key_id),
                intent_id: CredentialKeyCreationIntentId::from_uuid(intent_id),
                intent_nonce: IntentNonce::from_uuid(nonce),
            },
            &ZeroizingDek::new([7; 32]),
            b"credential-secret",
        )
        .unwrap();
    sqlx::query("INSERT INTO credential_prepared_materials (id,workspace_id,intent_id,credential_revision_id,ciphertext) VALUES ($1,$2,$3,$4,$5)")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(intent_id).bind(revision_id).bind(frame).execute(&mut *setup).await.unwrap();
    sqlx::query("INSERT INTO credential_lifecycle_events (id,workspace_id,intent_id,credential_revision_id,event_kind) VALUES ($1,$2,$3,$4,'candidate')")
        .bind(Uuid::now_v7()).bind(workspace_id).bind(intent_id).bind(revision_id).execute(&mut *setup).await.unwrap();
    sqlx::query(
        "UPDATE credential_slots SET current_revision_id=$2,current_revision_version=1 WHERE id=$1",
    )
    .bind(slot_id)
    .bind(revision_id)
    .execute(&mut *setup)
    .await
    .unwrap();
    setup.commit().await.unwrap();

    LeaseFixture {
        workspace_id,
        connection_id,
        slot_id,
        revision_id,
        activation_guard_id,
    }
}

async fn authorized_effect(pool: &PgPool, workspace_id: Uuid) -> (Uuid, Uuid) {
    let effect_id = Uuid::now_v7();
    let authorization_id = Uuid::now_v7();
    sqlx::query("INSERT INTO external_effect_intents (id,workspace_id,adapter,payload) VALUES ($1,$2,'provider','{}'::jsonb)")
        .bind(effect_id).bind(workspace_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO external_effect_authorizations (id,effect_id,workspace_id,policy_version,subject_id,capability,operation,resource_scope,result,reason,input_state,decided_at,payload) VALUES ($1,$2,$3,'v1',$4,'provider.dispatch','dispatch','effect','allow','configured_allowance','{}'::jsonb,NOW(),'{}'::jsonb)")
        .bind(authorization_id).bind(effect_id).bind(workspace_id).bind(Uuid::now_v7()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO external_effect_lifecycle_transitions (effect_id,workspace_id,status,cause,cause_ref,recorded_at) VALUES ($1,$2,'authorized','authorization_recorded',$3,NOW())")
        .bind(effect_id).bind(workspace_id).bind(authorization_id.to_string()).execute(pool).await.unwrap();
    (effect_id, authorization_id)
}

async fn assert_sqlstate(
    result: Result<sqlx::postgres::PgQueryResult, sqlx::Error>,
    expected: &str,
) {
    let error = result.unwrap_err();
    assert_eq!(
        error
            .as_database_error()
            .and_then(|value| value.code())
            .as_deref(),
        Some(expected)
    );
}

async fn issue_lease(
    runtime: &PgPool,
    fixture: &LeaseFixture,
    effect_id: Uuid,
    authorization_id: Uuid,
    expires_at: chrono::DateTime<Utc>,
) -> Uuid {
    let lease_id = Uuid::now_v7();
    let mut issue = runtime.begin().await.unwrap();
    set_workspace(&mut *issue, fixture.workspace_id).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_issue_credential_dispatch_lease($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(lease_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .bind(effect_id)
    .bind(authorization_id)
    .bind(fixture.slot_id)
    .bind(fixture.revision_id)
    .bind(fixture.activation_guard_id)
    .bind("api.example.test")
    .bind("bearer")
    .bind(expires_at)
    .fetch_one(&mut *issue)
    .await
    .unwrap();
    issue.commit().await.unwrap();
    lease_id
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_lease_issue_consume_and_refusals_are_structural(pool: PgPool) {
    let fixture = active_lease_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let (effect_id, authorization_id) = authorized_effect(&pool, fixture.workspace_id).await;
    let lease_id = Uuid::now_v7();
    let expires_at = Utc::now() + ChronoDuration::minutes(5);
    let mut issue = runtime.begin().await.unwrap();
    set_workspace(&mut *issue, fixture.workspace_id).await;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_issue_credential_dispatch_lease($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(lease_id)
    .bind(fixture.workspace_id)
    .bind(fixture.connection_id)
    .bind(effect_id)
    .bind(authorization_id)
    .bind(fixture.slot_id)
    .bind(fixture.revision_id)
    .bind(fixture.activation_guard_id)
    .bind("api.example.test")
    .bind("bearer")
    .bind(expires_at)
    .fetch_one(&mut *issue)
    .await
    .unwrap();
    issue.commit().await.unwrap();
    let issued: (Option<String>, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT terminal_state,consumed_at FROM credential_dispatch_leases WHERE id=$1",
    )
    .bind(lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(issued, (None, None));

    let mut duplicate = runtime.begin().await.unwrap();
    set_workspace(&mut *duplicate, fixture.workspace_id).await;
    assert_sqlstate(
        sqlx::query(
            "SELECT vestrace_issue_credential_dispatch_lease($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(effect_id)
        .bind(authorization_id)
        .bind(fixture.slot_id)
        .bind(fixture.revision_id)
        .bind(fixture.activation_guard_id)
        .bind("api.example.test")
        .bind("bearer")
        .bind(expires_at)
        .execute(&mut *duplicate)
        .await,
        "23505",
    )
    .await;
    duplicate.rollback().await.unwrap();

    let mut consume = runtime.begin().await.unwrap();
    set_workspace(&mut *consume, fixture.workspace_id).await;
    sqlx::query_scalar::<_, Uuid>("SELECT vestrace_consume_credential_dispatch_lease($1,$2,$3,$4)")
        .bind(lease_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(effect_id)
        .fetch_one(&mut *consume)
        .await
        .unwrap();
    consume.commit().await.unwrap();
    let consumed: (Option<String>, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT terminal_state,consumed_at FROM credential_dispatch_leases WHERE id=$1",
    )
    .bind(lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(consumed.0.as_deref(), Some("consumed_for_dispatch"));
    assert!(consumed.1.is_some());
    let mut second = runtime.begin().await.unwrap();
    set_workspace(&mut *second, fixture.workspace_id).await;
    assert_sqlstate(
        sqlx::query("SELECT vestrace_consume_credential_dispatch_lease($1,$2,$3,$4)")
            .bind(lease_id)
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(effect_id)
            .execute(&mut *second)
            .await,
        "23514",
    )
    .await;
    second.rollback().await.unwrap();

    let mut rewrite = pool.begin().await.unwrap();
    set_workspace(&mut *rewrite, fixture.workspace_id).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *rewrite)
        .await
        .unwrap();
    assert_sqlstate(sqlx::query("UPDATE credential_dispatch_leases SET terminal_state='revoked',consumed_at=NULL WHERE id=$1").bind(lease_id).execute(&mut *rewrite).await, "23514").await;
    rewrite.rollback().await.unwrap();

    let mut raw = runtime.begin().await.unwrap();
    set_workspace(&mut *raw, fixture.workspace_id).await;
    assert_sqlstate(sqlx::query("INSERT INTO credential_dispatch_leases (id,workspace_id,connection_id,external_effect_id,authorization_id,credential_revision_id,credential_slot_id,credential_activation_guard_id,destination_authority,auth_mode,issued_at,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'api.example.test','bearer',NOW(),NOW()+INTERVAL '1 hour')")
        .bind(Uuid::now_v7()).bind(fixture.workspace_id).bind(fixture.connection_id).bind(Uuid::now_v7()).bind(Uuid::now_v7()).bind(fixture.revision_id).bind(fixture.slot_id).bind(fixture.activation_guard_id).execute(&mut *raw).await, "42501").await;
    raw.rollback().await.unwrap();
    let mut raw_update = runtime.begin().await.unwrap();
    set_workspace(&mut *raw_update, fixture.workspace_id).await;
    assert_sqlstate(sqlx::query("UPDATE credential_dispatch_leases SET expires_at=expires_at+INTERVAL '1 minute' WHERE id=$1").bind(lease_id).execute(&mut *raw_update).await, "42501").await;
    raw_update.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn expired_and_no_longer_current_leases_cannot_be_consumed(pool: PgPool) {
    let fixture = active_lease_fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    let (expired_effect_id, expired_authorization_id) =
        authorized_effect(&pool, fixture.workspace_id).await;
    let expired_lease_id = issue_lease(
        &runtime,
        &fixture,
        expired_effect_id,
        expired_authorization_id,
        Utc::now() + ChronoDuration::milliseconds(250),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut expired = runtime.begin().await.unwrap();
    set_workspace(&mut *expired, fixture.workspace_id).await;
    assert_sqlstate(
        sqlx::query("SELECT vestrace_consume_credential_dispatch_lease($1,$2,$3,$4)")
            .bind(expired_lease_id)
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(expired_effect_id)
            .execute(&mut *expired)
            .await,
        "23514",
    )
    .await;
    expired.rollback().await.unwrap();

    let (stale_effect_id, stale_authorization_id) =
        authorized_effect(&pool, fixture.workspace_id).await;
    let stale_lease_id = issue_lease(
        &runtime,
        &fixture,
        stale_effect_id,
        stale_authorization_id,
        Utc::now() + ChronoDuration::minutes(5),
    )
    .await;
    let mut stale = pool.begin().await.unwrap();
    set_workspace(&mut *stale, fixture.workspace_id).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *stale)
        .await
        .unwrap();
    sqlx::query("UPDATE credential_slots SET current_revision_id=NULL WHERE id=$1")
        .bind(fixture.slot_id)
        .execute(&mut *stale)
        .await
        .unwrap();
    sqlx::query("RESET ROLE")
        .execute(&mut *stale)
        .await
        .unwrap();
    assert_sqlstate(
        sqlx::query("SELECT vestrace_consume_credential_dispatch_lease($1,$2,$3,$4)")
            .bind(stale_lease_id)
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(stale_effect_id)
            .execute(&mut *stale)
            .await,
        "23514",
    )
    .await;
    stale.rollback().await.unwrap();
    runtime.close().await;
}

struct CountingVault(AtomicUsize);

impl CountingVault {
    fn calls(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl vestrace_application::MaterialKeyVault for CountingVault {
    fn create_if_absent(
        &self,
        _: vestrace_domain::MaterialKeyId,
        _: vestrace_domain::IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
    fn unwrap(
        &self,
        _: vestrace_domain::MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([7; 32]));
        Ok(())
    }
    fn prepare_erasure(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
    fn erase(
        &self,
        _: vestrace_domain::MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn vault_unwrap_follows_successful_consumption_and_never_a_refusal(pool: PgPool) {
    let fixture = active_lease_fixture(&pool).await;
    let (effect_id, authorization_id) = authorized_effect(&pool, fixture.workspace_id).await;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::new(),
    );
    let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let store = PgStore::from_pool(pool.clone());
    let repository = PgCredentialDispatchLeaseRepository::new(store.clone(), Arc::clone(&vault));
    let lease = repository
        .issue(
            &context,
            CredentialDispatchLeaseRequest {
                lease_id: Uuid::now_v7(),
                connection_id: ConnectionId::from_uuid(fixture.connection_id),
                external_effect_id: effect_id,
                authorization_id,
                credential_slot_id: CredentialSlotId::from_uuid(fixture.slot_id),
                credential_revision_id: fixture.revision_id,
                credential_activation_guard_id: fixture.activation_guard_id,
                destination_authority: "api.example.test".to_owned(),
                auth_mode: "bearer".to_owned(),
                expires_at: Utc::now() + ChronoDuration::minutes(5),
            },
        )
        .await
        .unwrap();
    let mut dispatch = store.begin_scoped(&context).await.unwrap();
    let auth = repository
        .consume_for_dispatch(&context, &mut dispatch, &lease)
        .await
        .unwrap();
    dispatch.commit().await.unwrap();
    let ConnectionAuth::Bearer(credential) = auth else {
        panic!("bearer lease did not return bearer auth")
    };
    assert_eq!(credential.expose(), "credential-secret");
    assert_eq!(vault.calls(), 1);

    let mut refused = store.begin_scoped(&context).await.unwrap();
    assert!(
        repository
            .consume_for_dispatch(&context, &mut refused, &lease)
            .await
            .is_err()
    );
    refused.rollback().await.unwrap();
    assert_eq!(
        vault.calls(),
        1,
        "a terminal lease must refuse before vault unwrap"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn caller_mutations_never_override_the_persisted_lease_authority(pool: PgPool) {
    let fixture = active_lease_fixture(&pool).await;
    let (effect_id, authorization_id) = authorized_effect(&pool, fixture.workspace_id).await;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::new(),
    );
    let vault = Arc::new(CountingVault(AtomicUsize::new(0)));
    let store = PgStore::from_pool(pool.clone());
    let repository = PgCredentialDispatchLeaseRepository::new(store.clone(), Arc::clone(&vault));
    let lease = repository
        .issue(
            &context,
            CredentialDispatchLeaseRequest {
                lease_id: Uuid::now_v7(),
                connection_id: ConnectionId::from_uuid(fixture.connection_id),
                external_effect_id: effect_id,
                authorization_id,
                credential_slot_id: CredentialSlotId::from_uuid(fixture.slot_id),
                credential_revision_id: fixture.revision_id,
                credential_activation_guard_id: fixture.activation_guard_id,
                destination_authority: "api.example.test".to_owned(),
                auth_mode: "bearer".to_owned(),
                expires_at: Utc::now() + ChronoDuration::minutes(5),
            },
        )
        .await
        .unwrap();

    let mut mutations = Vec::new();
    let mut changed = lease.clone();
    changed.id = Uuid::now_v7();
    mutations.push(("lease id", changed));
    let mut changed = lease.clone();
    changed.workspace_id = WorkspaceId::new();
    mutations.push(("workspace", changed));
    let mut changed = lease.clone();
    changed.connection_id = ConnectionId::new();
    mutations.push(("connection", changed));
    let mut changed = lease.clone();
    changed.external_effect_id = Uuid::now_v7();
    mutations.push(("effect", changed));
    let mut changed = lease.clone();
    changed.authorization_id = Uuid::now_v7();
    mutations.push(("authorization", changed));
    let mut changed = lease.clone();
    changed.credential_slot_id = CredentialSlotId::new();
    mutations.push(("slot", changed));
    let mut changed = lease.clone();
    changed.credential_revision_id = Uuid::now_v7();
    mutations.push(("revision", changed));
    let mut changed = lease.clone();
    changed.credential_activation_guard_id = Uuid::now_v7();
    mutations.push(("guard", changed));
    let mut changed = lease.clone();
    changed.destination_authority = "other.example.test".to_owned();
    mutations.push(("destination", changed));
    let mut changed = lease.clone();
    changed.auth_mode = "api_key".to_owned();
    mutations.push(("valid auth mode", changed));
    let mut changed = lease.clone();
    changed.auth_mode = "unsupported".to_owned();
    mutations.push(("unsupported auth mode", changed));
    let mut changed = lease.clone();
    changed.issued_at += ChronoDuration::seconds(1);
    mutations.push(("issued at", changed));
    let mut changed = lease.clone();
    changed.expires_at += ChronoDuration::seconds(1);
    mutations.push(("expires at", changed));

    for (name, changed) in mutations {
        let mut transaction = store.begin_scoped(&context).await.unwrap();
        let result = repository
            .consume_for_dispatch(&context, &mut transaction, &changed)
            .await;
        assert!(
            matches!(result, Err(ApplicationError::Policy(_))),
            "caller-mutated {name} unexpectedly consumed the persisted lease"
        );
        transaction.rollback().await.unwrap();
        assert_eq!(
            vault.calls(),
            0,
            "caller-mutated {name} reached vault unwrap"
        );
        let consumed_at: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT consumed_at FROM credential_dispatch_leases WHERE id=$1")
                .bind(lease.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            consumed_at.is_none(),
            "caller-mutated {name} consumed the lease"
        );
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn transaction_bound_issue_rolls_back_and_reuses_the_exact_identity(pool: PgPool) {
    let fixture = active_lease_fixture(&pool).await;
    let (effect_id, authorization_id) = authorized_effect(&pool, fixture.workspace_id).await;
    let context = RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::new(),
    );
    let store = PgStore::from_pool(pool.clone());
    let repository = PgCredentialDispatchLeaseRepository::new(
        store.clone(),
        Arc::new(CountingVault(AtomicUsize::new(0))),
    );
    let request = CredentialDispatchLeaseRequest {
        lease_id: Uuid::now_v7(),
        connection_id: ConnectionId::from_uuid(fixture.connection_id),
        external_effect_id: effect_id,
        authorization_id,
        credential_slot_id: CredentialSlotId::from_uuid(fixture.slot_id),
        credential_revision_id: fixture.revision_id,
        credential_activation_guard_id: fixture.activation_guard_id,
        destination_authority: "api.example.test".to_owned(),
        auth_mode: "bearer".to_owned(),
        expires_at: Utc::now() + ChronoDuration::minutes(5),
    };

    let mut rolled_back = store.begin_scoped(&context).await.unwrap();
    let rolled_back_output = repository
        .issue_in(&context, &mut rolled_back, request.clone())
        .await
        .unwrap();
    assert_eq!(rolled_back_output.id, request.lease_id);
    rolled_back.rollback().await.unwrap();

    let count_after_rollback: i64 =
        sqlx::query_scalar("SELECT count(*) FROM credential_dispatch_leases WHERE id = $1")
            .bind(request.lease_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_after_rollback, 0);

    let mut committed = store.begin_scoped(&context).await.unwrap();
    let committed_output = repository
        .issue_in(&context, &mut committed, request.clone())
        .await
        .unwrap();
    committed.commit().await.unwrap();

    assert_eq!(committed_output.id, rolled_back_output.id);
    assert_eq!(
        committed_output.workspace_id,
        rolled_back_output.workspace_id
    );
    assert_eq!(
        committed_output.connection_id,
        rolled_back_output.connection_id
    );
    assert_eq!(
        committed_output.external_effect_id,
        rolled_back_output.external_effect_id
    );
    assert_eq!(
        committed_output.authorization_id,
        rolled_back_output.authorization_id
    );
    assert_eq!(
        committed_output.credential_slot_id,
        rolled_back_output.credential_slot_id
    );
    assert_eq!(
        committed_output.credential_revision_id,
        rolled_back_output.credential_revision_id
    );
    assert_eq!(
        committed_output.credential_activation_guard_id,
        rolled_back_output.credential_activation_guard_id
    );
    assert_eq!(
        committed_output.destination_authority,
        rolled_back_output.destination_authority
    );
    assert_eq!(committed_output.auth_mode, rolled_back_output.auth_mode);
    assert_eq!(committed_output.expires_at, rolled_back_output.expires_at);
    assert_eq!(committed_output.consumed_at, None);
    assert_eq!(committed_output.terminal_state, None);

    let persisted: PersistedLease = sqlx::query_as(
        "SELECT id, workspace_id, connection_id, external_effect_id, authorization_id, \
                credential_slot_id, credential_revision_id, destination_authority, auth_mode, \
                issued_at, expires_at, consumed_at, terminal_state \
           FROM credential_dispatch_leases WHERE id = $1",
    )
    .bind(request.lease_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted.id, committed_output.id);
    assert_eq!(
        persisted.workspace_id,
        committed_output.workspace_id.as_uuid()
    );
    assert_eq!(
        persisted.connection_id,
        committed_output.connection_id.as_uuid()
    );
    assert_eq!(
        persisted.external_effect_id,
        committed_output.external_effect_id
    );
    assert_eq!(
        persisted.authorization_id,
        committed_output.authorization_id
    );
    assert_eq!(
        persisted.credential_slot_id,
        committed_output.credential_slot_id.as_uuid()
    );
    assert_eq!(
        persisted.credential_revision_id,
        committed_output.credential_revision_id
    );
    assert_eq!(
        persisted.destination_authority,
        committed_output.destination_authority
    );
    assert_eq!(persisted.auth_mode, committed_output.auth_mode);
    assert_eq!(persisted.issued_at, committed_output.issued_at);
    assert_eq!(persisted.expires_at, committed_output.expires_at);
    assert_eq!(persisted.consumed_at, committed_output.consumed_at);
    assert_eq!(persisted.terminal_state, committed_output.terminal_state);
}
