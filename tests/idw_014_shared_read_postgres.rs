mod support;

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use chrono::{TimeZone, Utc};
use sqlx::{postgres::PgPoolOptions, types::Uuid};
use vestrace_application::{
    MemoryRepository, RequestContext, SharedMemoryReadClock, SharedMemoryReadService,
};
use vestrace_domain::{
    Timestamp,
    enterprise::{
        MemoryMount, MemoryMountAcceptance, MemoryShareGrant, MemoryShareGrantRevision,
        MemoryShareGrantRevisionSpec, ShareOperation, ShareTarget, TargetSharePolicy,
    },
    id::{MemoryId, MemoryRevisionId, PrincipalId, WorkspaceId},
};
use vestrace_infrastructure::{PgMemoryRepository, PgSharedMemoryRevisionReader, PgStore};

use support::{
    RestrictedRole, RestrictedRuntimeRoleSetupStep, with_restricted_runtime_role,
    with_restricted_runtime_role_connector,
};

const SOURCE_WORKSPACE: &str = "70000000-0000-0000-0000-000000000001";
const TARGET_WORKSPACE: &str = "70000000-0000-0000-0000-000000000002";
const WRONG_SOURCE_WORKSPACE: &str = "70000000-0000-0000-0000-000000000003";
const TARGET_PRINCIPAL: &str = "70000000-0000-0000-0000-000000000004";
const SOURCE_MEMORY: &str = "70000000-0000-0000-0000-000000000005";
const SOURCE_REVISION_1: &str = "70000000-0000-0000-0000-000000000006";
const SOURCE_REVISION_2: &str = "70000000-0000-0000-0000-000000000007";
const SOURCE_EVENT: &str = "70000000-0000-0000-0000-000000000008";
const SOURCE_LINK: &str = "70000000-0000-0000-0000-000000000009";

#[derive(Clone)]
struct FixedClock(Timestamp);

impl SharedMemoryReadClock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

type ClassSnapshot = (bool, bool, String);
type PolicySnapshot = (String, String, Option<String>, Option<String>);

fn parse_uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

fn workspace(value: &str) -> WorkspaceId {
    WorkspaceId::from_uuid(parse_uuid(value))
}

fn principal(value: &str) -> PrincipalId {
    PrincipalId::from_uuid(parse_uuid(value))
}

fn memory(value: &str) -> MemoryId {
    MemoryId::from_uuid(parse_uuid(value))
}

fn revision(value: &str) -> MemoryRevisionId {
    MemoryRevisionId::from_uuid(parse_uuid(value))
}

fn at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, 0)
        .single()
        .unwrap()
}

fn read_operations() -> BTreeSet<ShareOperation> {
    BTreeSet::from([ShareOperation::ReadContent])
}

fn sharing_fixture(
    source_workspace_id: WorkspaceId,
    target_workspace_id: WorkspaceId,
    target_principal_id: PrincipalId,
    source_memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
) -> (MemoryShareGrant, MemoryMount, TargetSharePolicy) {
    let policy = TargetSharePolicy::new(
        target_workspace_id,
        target_principal_id,
        read_operations(),
        None,
    )
    .unwrap();
    let grant_revision = MemoryShareGrantRevision::issue(
        MemoryShareGrantRevisionSpec {
            source_workspace_id,
            target: ShareTarget::ExactWorkspace(target_workspace_id),
            memory_id: source_memory_id,
            memory_revision_id,
            source_generation: "source-generation-1".to_owned(),
            operations: read_operations(),
            valid_from: at(),
            valid_until: None,
        },
        at(),
    )
    .unwrap();
    let grant = MemoryShareGrant::issue(grant_revision, at()).unwrap();
    let mount = MemoryMount::accept(
        MemoryMountAcceptance {
            grant_id: grant.id(),
            grant_revision_id: grant.revision().id(),
            target_workspace_id,
            target_principal_id,
            operations: read_operations(),
            valid_until: None,
        },
        &grant,
        &policy,
        at(),
    )
    .unwrap();
    (grant, mount, policy)
}

async fn seed_shared_revisions(pool: &sqlx::PgPool) {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, slug) VALUES
         ($1, 'idw014-source'), ($2, 'idw014-target'), ($3, 'idw014-wrong-source')",
    )
    .bind(parse_uuid(SOURCE_WORKSPACE))
    .bind(parse_uuid(TARGET_WORKSPACE))
    .bind(parse_uuid(WRONG_SOURCE_WORKSPACE))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, 'idw014-target')",
    )
    .bind(parse_uuid(TARGET_PRINCIPAL))
    .bind(parse_uuid(TARGET_WORKSPACE))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (id, workspace_id, event_type, actor)
         VALUES ($1, $2, 'idw014-source', '{}'::jsonb)",
    )
    .bind(parse_uuid(SOURCE_EVENT))
    .bind(parse_uuid(SOURCE_WORKSPACE))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, workspace_id, kind, status) VALUES ($1, $2, 'fact', 'active')",
    )
    .bind(parse_uuid(SOURCE_MEMORY))
    .bind(parse_uuid(SOURCE_WORKSPACE))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memory_revisions
           (id, memory_id, workspace_id, revision_number, content, confidence, importance)
         VALUES
           ($1, $3, $4, 1, 'source revision one', 1.0, 1.0),
           ($2, $3, $4, 2, 'source revision two', 1.0, 1.0)",
    )
    .bind(parse_uuid(SOURCE_REVISION_1))
    .bind(parse_uuid(SOURCE_REVISION_2))
    .bind(parse_uuid(SOURCE_MEMORY))
    .bind(parse_uuid(SOURCE_WORKSPACE))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memory_sources
           (id, memory_id, workspace_id, event_id, role)
         VALUES ($1, $2, $3, $4, 'primary')",
    )
    .bind(parse_uuid(SOURCE_LINK))
    .bind(parse_uuid(SOURCE_MEMORY))
    .bind(parse_uuid(SOURCE_WORKSPACE))
    .bind(parse_uuid(SOURCE_EVENT))
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

async fn snapshot_class(pool: &sqlx::PgPool) -> ClassSnapshot {
    sqlx::query_as(
        "SELECT c.relrowsecurity,
                c.relforcerowsecurity,
                pg_get_userbyid(c.relowner)::text
         FROM pg_class c
         WHERE c.oid = 'memory_revisions'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn snapshot_policies(pool: &sqlx::PgPool) -> Vec<PolicySnapshot> {
    sqlx::query_as(
        "SELECT polname::text,
                polcmd::text,
                pg_get_expr(polqual, polrelid),
                pg_get_expr(polwithcheck, polrelid)
         FROM pg_policy
         WHERE polrelid = 'memory_revisions'::regclass
         ORDER BY polname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn role_exists(pool: &sqlx::PgPool, role_name: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
        .bind(role_name)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn panic_message(error: tokio::task::JoinError) -> String {
    let payload = error.into_panic();
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_owned(),
            Err(_) => panic!("panic payload was not text"),
        },
    }
}

fn role_name_from_setup_panic(message: &str) -> String {
    let prefix = "restricted runtime role setup failed for ";
    let remainder = message
        .strip_prefix(prefix)
        .unwrap_or_else(|| panic!("unexpected setup panic: {message}"));
    remainder
        .split_once(':')
        .map(|(role_name, _)| role_name.to_owned())
        .unwrap_or_else(|| panic!("setup panic omitted role name: {message}"))
}

#[sqlx::test(migrations = "./migrations")]
async fn authorized_shared_reader_uses_exact_revision_under_forced_rls(pool: sqlx::PgPool) {
    seed_shared_revisions(&pool).await;
    let class_before = snapshot_class(&pool).await;
    let policies_before = snapshot_policies(&pool).await;
    assert!(class_before.0);
    assert!(class_before.1);

    let source_workspace_id = workspace(SOURCE_WORKSPACE);
    let target_workspace_id = workspace(TARGET_WORKSPACE);
    let wrong_source_workspace_id = workspace(WRONG_SOURCE_WORKSPACE);
    let target_principal_id = principal(TARGET_PRINCIPAL);
    let source_memory_id = memory(SOURCE_MEMORY);
    let source_revision_1 = revision(SOURCE_REVISION_1);
    let source_revision_2 = revision(SOURCE_REVISION_2);
    let target_context = RequestContext::new(target_workspace_id, target_principal_id);

    let evidence = with_restricted_runtime_role(&pool, move |runtime_pool, role| async move {
        let identity: (String, String, bool, bool, bool) = sqlx::query_as(
            "SELECT current_user::text,
                    session_user::text,
                    r.rolsuper,
                    r.rolbypassrls,
                    EXISTS (
                        SELECT 1
                        FROM pg_roles privileged
                        WHERE (privileged.rolsuper OR privileged.rolbypassrls)
                          AND pg_has_role(current_user, privileged.oid, 'MEMBER')
                    ) AS has_privileged_membership
             FROM pg_roles r
             WHERE r.rolname = current_user",
        )
        .fetch_one(&runtime_pool)
        .await
        .unwrap();
        assert_eq!(identity.0, role.name());
        assert_eq!(identity.1, role.name());
        assert!(!identity.2);
        assert!(!identity.3);
        assert!(!identity.4);

        let store = PgStore::from_pool(runtime_pool.clone());
        let ordinary = PgMemoryRepository::new(store.clone())
            .find_revision_by_id(&target_context, source_revision_1)
            .await
            .unwrap();
        assert!(ordinary.is_none());

        let reader = Arc::new(PgSharedMemoryRevisionReader::new(store));
        let service = SharedMemoryReadService::new(reader, Arc::new(FixedClock(at())));

        let (grant_1, mut mount_1, policy_1) = sharing_fixture(
            source_workspace_id,
            target_workspace_id,
            target_principal_id,
            source_memory_id,
            source_revision_1,
        );
        let result_1 = service
            .read_shared(&target_context, &grant_1, &mut mount_1, &policy_1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result_1.content(), "source revision one");
        assert_eq!(result_1.shared_ref().memory_revision_id(), source_revision_1);

        let (grant_2, mut mount_2, policy_2) = sharing_fixture(
            source_workspace_id,
            target_workspace_id,
            target_principal_id,
            source_memory_id,
            source_revision_2,
        );
        let result_2 = service
            .read_shared(&target_context, &grant_2, &mut mount_2, &policy_2)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result_2.content(), "source revision two");
        assert_eq!(result_2.shared_ref().memory_revision_id(), source_revision_2);

        let (wrong_grant, mut wrong_mount, wrong_policy) = sharing_fixture(
            wrong_source_workspace_id,
            target_workspace_id,
            target_principal_id,
            source_memory_id,
            source_revision_1,
        );
        let wrong_result = service
            .read_shared(
                &target_context,
                &wrong_grant,
                &mut wrong_mount,
                &wrong_policy,
            )
            .await
            .unwrap();
        assert!(wrong_result.is_none());
        assert!(wrong_mount.disclosures().is_empty());

        let guc_is_clear: bool = sqlx::query_scalar(
            "SELECT NULLIF(current_setting('vestrace.workspace_id', true), '') IS NULL",
        )
        .fetch_one(&runtime_pool)
        .await
        .unwrap();
        assert!(guc_is_clear);

        println!(
            "IDW014_ROLE current_user={} session_user={} rolsuper={} rolbypassrls={} privileged_membership={} direct_target_visible={} revision_1={} revision_2={} wrong_source_visible={} workspace_guc_leaked={}",
            identity.0,
            identity.1,
            identity.2,
            identity.3,
            identity.4,
            ordinary.is_some(),
            result_1.content(),
            result_2.content(),
            wrong_result.is_some(),
            !guc_is_clear,
        );
        identity.0
    })
    .await;

    assert!(!role_exists(&pool, &evidence).await);
    let class_after = snapshot_class(&pool).await;
    let policies_after = snapshot_policies(&pool).await;
    assert_eq!(class_after, class_before);
    assert_eq!(policies_after, policies_before);
    println!("IDW014_CATALOG class={class_after:?} policies={policies_after:?} role_dropped=true");
}

#[sqlx::test(migrations = "./migrations")]
async fn restricted_runtime_role_is_dropped_when_body_panics(pool: sqlx::PgPool) {
    let captured = Arc::new(Mutex::new(None::<String>));
    let task_pool = pool.clone();
    let task_captured = captured.clone();
    let outcome = tokio::spawn(async move {
        with_restricted_runtime_role(&task_pool, move |_runtime_pool, role| {
            *task_captured.lock().unwrap() = Some(role.name().to_owned());
            async move { panic!("injected callback panic") }
        })
        .await
    })
    .await;

    let join_error = match outcome {
        Ok(_) => panic!("callback panic was not propagated"),
        Err(error) => error,
    };
    assert!(join_error.is_panic());
    assert_eq!(panic_message(join_error), "injected callback panic");
    let role_name = captured.lock().unwrap().clone().unwrap();
    assert!(!role_exists(&pool, &role_name).await);
    println!("IDW014_CALLBACK_CLEANUP role={role_name} dropped=true");
}

#[sqlx::test(migrations = "./migrations")]
async fn restricted_runtime_role_is_dropped_after_each_partial_setup_failure(pool: sqlx::PgPool) {
    for step in [
        RestrictedRuntimeRoleSetupStep::GrantConnect,
        RestrictedRuntimeRoleSetupStep::GrantSchemaUsage,
        RestrictedRuntimeRoleSetupStep::GrantTableSelect,
    ] {
        let task_pool = pool.clone();
        let outcome = tokio::spawn(async move {
            with_restricted_runtime_role_connector(
                &task_pool,
                Some(step),
                |_options, _role| async move {
                    panic!("connector ran after an injected setup failure")
                },
                |_runtime_pool, _role| async move {
                    panic!("callback ran after an injected setup failure")
                },
            )
            .await
        })
        .await;

        let join_error = match outcome {
            Ok(_) => panic!("setup failpoint {step:?} did not fail"),
            Err(error) => error,
        };
        assert!(join_error.is_panic());
        let message = panic_message(join_error);
        let role_name = role_name_from_setup_panic(&message);
        assert!(message.contains(&format!("before {step:?}")));
        assert!(!role_exists(&pool, &role_name).await);
        println!("IDW014_SETUP_CLEANUP step={step:?} role={role_name} dropped=true");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn restricted_runtime_role_is_dropped_when_pool_creation_fails(pool: sqlx::PgPool) {
    let captured = Arc::new(Mutex::new(None::<String>));
    let task_pool = pool.clone();
    let task_captured = captured.clone();
    let outcome = tokio::spawn(async move {
        with_restricted_runtime_role_connector(
            &task_pool,
            None,
            move |options, role: RestrictedRole| {
                *task_captured.lock().unwrap() = Some(role.name().to_owned());
                async move {
                    let runtime_pool = PgPoolOptions::new()
                        .max_connections(1)
                        .connect_with(options.password("known-wrong-password"))
                        .await;
                    match runtime_pool {
                        Ok(pool) => {
                            pool.close().await;
                            panic!("wrong-password pool unexpectedly connected")
                        }
                        Err(error) => Err(error),
                    }
                }
            },
            |_runtime_pool, _role| async move { panic!("callback ran after pool creation failed") },
        )
        .await
    })
    .await;

    let join_error = match outcome {
        Ok(_) => panic!("pool connection failure was not propagated"),
        Err(error) => error,
    };
    assert!(join_error.is_panic());
    let message = panic_message(join_error);
    assert!(message.contains("restricted runtime role pool connection failed"));
    let role_name = captured.lock().unwrap().clone().unwrap();
    assert!(!role_exists(&pool, &role_name).await);
    println!("IDW014_POOL_CLEANUP role={role_name} dropped=true");
}
