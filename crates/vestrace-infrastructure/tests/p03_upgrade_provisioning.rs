use std::{borrow::Cow, collections::BTreeSet, str::FromStr};

use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const PROVISIONER: &str = include_str!("../../../docker/postgres/init-runtime-role.sh");
const COMPOSE: &str = include_str!("../../../docker-compose.yml");
const EXPECTED_GUARDED_TABLES: [&str; 69] = [
    "p02_guarded_operation_probe",
    "governed_mutation_audit_marks",
    "installation_fingerprint_continuity",
    "installation_mutation_watermark",
    "installation_mutation_watermark_advances",
    "material_key_creation_intents",
    "material_key_creation_intent_erasure_receipts",
    "content_materials",
    "content_material_bytes",
    "prepared_material_attachments",
    "content_material_ordinary_references",
    "connection_execution_guards",
    "credential_activation_guards",
    "credential_slots",
    "credential_revisions",
    "credential_guard_occupancies",
    "credential_key_creation_intents",
    "credential_prepared_materials",
    "credential_prepared_attachments",
    "credential_association_events",
    "credential_lifecycle_events",
    "credential_key_creation_intent_erasure_receipts",
    "material_erasure_preparations",
    "material_erasure_events",
    "material_erasure_audit_tombstones",
    "material_erasure_blockers",
    "connection_revision_heads",
    "connection_revisions",
    "no_auth_binding_revisions",
    "qualification_jobs",
    "qualification_target_bindings",
    "qualification_probe_results",
    "qualification_q1_mre_sources",
    "connection_qualification_revisions",
    "connection_qualification_heads",
    "model_revision_heads",
    "model_revisions",
    "model_qualification_revisions",
    "model_qualification_heads",
    "workspace_model_defaults",
    "model_binding_snapshots",
    "run_model_binding_snapshots",
    "model_request_evidence_roots",
    "model_request_evidence_nodes",
    "model_request_evidence_checks",
    "model_request_shape_revisions",
    "model_sampling_revisions",
    "model_limits_revisions",
    "model_tool_schema_revisions",
    "model_request_evidence_check_missing_references",
    "connection_admission_policy_heads",
    "connection_admission_policy_revisions",
    "connection_admission_states",
    "connection_dispatch_admissions",
    "provider_admission_waits",
    "provider_concurrency_leases",
    "provider_throttle_observations",
    "credential_dispatch_leases",
    "credential_activation_events",
    "credential_rotation_events",
    "provider_result_preparations",
    "provider_result_publications",
    "artifact_revision_contents",
    "provider_dispatch_causes",
    "run_step_execution_attempts",
    // P04 Task 3. The set is exact on purpose: a table that reaches the
    // guarded owner without appearing here means the bootstrap allowlist
    // grew where nobody was looking, which is the defect P03 shipped once.
    "embedding_space_registrations",
    "embedding_corpus_generations",
    "embedding_jobs",
    "model_data_policy_decisions",
];

fn provisioner_sql_from(marker: &str) -> &'static str {
    let start = PROVISIONER
        .find(marker)
        .unwrap_or_else(|| panic!("missing provisioner marker {marker}"));
    PROVISIONER[start..]
        .rsplit_once("\nSQL\n")
        .map(|(sql, _)| sql)
        .expect("the provisioner must contain the SQL heredoc terminator")
}

fn provisioner_sql_between(start_marker: &str, end_marker: &str) -> &'static str {
    let source = provisioner_sql_from(start_marker);
    source
        .split_once(end_marker)
        .map(|(sql, _)| sql)
        .unwrap_or_else(|| panic!("missing provisioner marker {end_marker}"))
}

fn service_block<'a>(compose: &'a str, service: &str) -> &'a str {
    let marker = format!("  {service}:\n");
    let start = compose
        .find(&marker)
        .unwrap_or_else(|| panic!("Compose service {service} is absent"));
    let tail = &compose[start + marker.len()..];
    let end = service_block_end(tail);
    &tail[..end]
}

fn optional_service_block<'a>(compose: &'a str, service: &str) -> Option<&'a str> {
    let marker = format!("  {service}:\n");
    compose.find(&marker).map(|start| {
        let tail = &compose[start + marker.len()..];
        let end = service_block_end(tail);
        &tail[..end]
    })
}

fn service_block_end(tail: &str) -> usize {
    let mut offset = 0;
    for line in tail.split_inclusive('\n') {
        if offset > 0
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
        {
            return offset;
        }
        offset += line.len();
    }
    tail.len()
}

fn runtime_credentials() -> (String, String) {
    let runtime_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL is required");
    let parsed = PgConnectOptions::from_str(&runtime_url).expect("runtime URL must be PostgreSQL");
    let username = parsed.get_username().to_owned();
    let password = runtime_url
        .split_once("://")
        .and_then(|(_, authority)| authority.rsplit_once('@'))
        .and_then(|(credentials, _)| credentials.split_once(':'))
        .map(|(_, password)| password.to_owned())
        .expect("runtime URL must contain a password");
    (username, password)
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let (username, password) = runtime_credentials();
    assert_eq!(username, "vestrace");
    PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(&username)
                .password(&password),
        )
        .await
        .expect("runtime must connect to the SQLx database")
}

async fn hand_database_to_runtime(pool: &PgPool) {
    sqlx::query("ALTER SCHEMA public OWNER TO vestrace")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "DO $$ BEGIN EXECUTE format('ALTER DATABASE %I OWNER TO vestrace', current_database()); END $$",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn install_extensions_from_real_provisioner(pool: &PgPool) {
    let statements = PROVISIONER
        .lines()
        .filter(|line| line.starts_with("CREATE EXTENSION IF NOT EXISTS "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!statements.is_empty());
    sqlx::raw_sql(&statements)
        .execute(pool)
        .await
        .expect("extensions must be installed from the real provisioner statements");
}

async fn assert_final_p03_schema(pool: &PgPool) {
    let versions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE version BETWEEN 176 AND 184 AND success",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(versions, 9);

    let guarded: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class AS c \
         JOIN pg_namespace AS n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relkind = 'r' \
           AND pg_get_userbyid(c.relowner) = 'vestrace_guarded_owner' \
         ORDER BY c.relname",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        guarded.into_iter().collect::<BTreeSet<_>>(),
        EXPECTED_GUARDED_TABLES.map(str::to_owned).into(),
        "only the exact P02 and P03 guarded tables may be re-owned"
    );

    let legacy_owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid = 'public.agent_runs'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(legacy_owner, "vestrace");

    for table in [
        "connection_execution_guards",
        "credential_activation_guards",
        "credential_slots",
        "credential_revisions",
        "credential_guard_occupancies",
        "credential_key_creation_intents",
        "material_key_creation_intents",
        "content_materials",
        "material_erasure_preparations",
    ] {
        let privileges: (bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT has_table_privilege('vestrace', $1, 'SELECT'), \
                    has_table_privilege('vestrace', $1, 'REFERENCES'), \
                    has_table_privilege('vestrace', $1, 'INSERT'), \
                    has_table_privilege('vestrace', $1, 'UPDATE'), \
                    has_table_privilege('vestrace', $1, 'DELETE')",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(privileges, (true, true, false, false, false), "{table}");
    }

    let deployment_policy_acl: (bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT has_table_privilege('vestrace','public.model_data_policy_decisions','SELECT'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','REFERENCES'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','INSERT'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','UPDATE'),
                has_table_privilege('vestrace','public.model_data_policy_decisions','DELETE')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(deployment_policy_acl, (true, false, false, false, false));

    let policy_function_acl: (bool, bool) = sqlx::query_as(
        "SELECT has_function_privilege('vestrace',
             'public.vestrace_record_model_data_policy_decision(uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone)',
             'EXECUTE'),
             has_function_privilege('public',
             'public.vestrace_record_model_data_policy_decision(uuid,uuid,uuid,text,text,text,text,text,text,timestamp with time zone)',
             'EXECUTE')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(policy_function_acl, (true, false));

    for helper in [
        "public.vestrace_assign_p03_table_owner(regclass)",
        "public.vestrace_assign_p03_function_owner(regprocedure)",
        "public.vestrace_grant_p03_dependency_references()",
    ] {
        let acl: (bool, bool) = sqlx::query_as(
            "SELECT has_function_privilege('vestrace', $1, 'EXECUTE'), \
                    has_function_privilege('public', $1, 'EXECUTE')",
        )
        .bind(helper)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(acl, (true, false), "{helper}");
    }

    let task10_upgrade_acl: (bool, bool) = sqlx::query_as(
        "SELECT has_function_privilege('vestrace', 'public.vestrace_prepare_task10_p03_upgrade()', 'EXECUTE'), \
                has_function_privilege('public', 'public.vestrace_prepare_task10_p03_upgrade()', 'EXECUTE')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        task10_upgrade_acl,
        (false, false),
        "the one-shot Task 10 ownership hand-back must close after migration 0184"
    );
    let trigger_installer_acl: (bool, bool, bool) = sqlx::query_as(
        "SELECT to_regprocedure(
                    'public.vestrace_install_provider_result_live_trigger()'
                ) IS NOT NULL,
                COALESCE(has_function_privilege(
                    'vestrace',
                    to_regprocedure('public.vestrace_install_provider_result_live_trigger()'),
                    'EXECUTE'
                ),FALSE),
                COALESCE(has_function_privilege(
                    'public',
                    to_regprocedure('public.vestrace_install_provider_result_live_trigger()'),
                    'EXECUTE'
                ),FALSE)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        trigger_installer_acl,
        (true, false, false),
        "0184 must leave a closed installer until the real bootstrap removes it"
    );
}

#[test]
fn compose_provisioning_precedes_runtime_migrate_without_leaking_bootstrap_credentials() {
    let provision = service_block(COMPOSE, "vestrace-role-provision");
    let migrate = service_block(COMPOSE, "vestrace-migrate");
    assert!(provision.contains("vestrace_bootstrap"));
    assert!(provision.contains("init-runtime-role.sh"));
    assert!(migrate.contains("vestrace-role-provision:"));
    assert!(migrate.contains("condition: service_completed_successfully"));
    assert!(migrate.contains("postgres://vestrace:"));
    assert!(!migrate.contains("vestrace_bootstrap"));
    assert!(!migrate.contains("bootstrap-local-development-only"));

    for service in ["vestrace-server", "vestrace-worker"] {
        let block = service_block(COMPOSE, service);
        assert!(!block.contains("vestrace_bootstrap"), "{service}");
        assert!(
            !block.contains("bootstrap-local-development-only"),
            "{service}"
        );
    }
    if let Some(block) = optional_service_block(COMPOSE, "vestrace-mcp") {
        assert!(!block.contains("vestrace_bootstrap"), "vestrace-mcp");
        assert!(
            !block.contains("bootstrap-local-development-only"),
            "vestrace-mcp"
        );
    }
}

#[sqlx::test(migrations = false)]
async fn fresh_database_runs_real_provisioner_before_runtime_migrator(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the embedded real provisioner SQL must run before migration");

    let runtime = runtime_pool(&pool).await;
    MIGRATOR.run(&runtime).await.unwrap();
    assert_final_p03_schema(&runtime).await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_cannot_replace_run_step_publication_evidence_predicate(pool: PgPool) {
    const PUBLICATION_EVIDENCE: &str = "public.vestrace_run_step_publication_evidence_is_exact(uuid,uuid,uuid,bigint,uuid,uuid,uuid,uuid)";
    let original: String = sqlx::query_scalar("SELECT pg_get_functiondef($1::regprocedure)")
        .bind(PUBLICATION_EVIDENCE)
        .fetch_one(&pool)
        .await
        .unwrap();
    let runtime = runtime_pool(&pool).await;
    let mut replacement = runtime.begin().await.unwrap();
    let attempt = sqlx::query(
        "CREATE OR REPLACE FUNCTION public.vestrace_run_step_publication_evidence_is_exact(
             target_workspace_id UUID,
             target_run_id UUID,
             target_step_id UUID,
             target_expected_run_version BIGINT,
             target_artifact_id UUID,
             target_artifact_revision_id UUID,
             target_model_execution_id UUID,
             target_advance_work_item_id UUID
         ) RETURNS BOOLEAN LANGUAGE sql AS 'SELECT FALSE'",
    )
    .execute(&mut *replacement)
    .await;
    let error = match attempt {
        Ok(_) => {
            replacement.rollback().await.unwrap();
            panic!("the runtime role replaced the publication-evidence predicate")
        }
        Err(error) => error,
    };
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501")
    );
    replacement.rollback().await.unwrap();
    let after: String = sqlx::query_scalar("SELECT pg_get_functiondef($1::regprocedure)")
        .bind(PUBLICATION_EVIDENCE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        after, original,
        "the rejected runtime replacement changed the helper"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn restricted_runtime_task11_candidate_abandon_upgrade_returns_guarded_owner(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the real provisioner must install the Task 11 ownership bridge");

    let runtime = runtime_pool(&pool).await;
    let through_0185 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 185)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_0185.run(&runtime).await.unwrap();
    let before: (String, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(proowner), \
                has_function_privilege('vestrace', \
                    'public.vestrace_prepare_task11_candidate_abandon_upgrade()', 'EXECUTE'), \
                has_function_privilege('public', \
                    'public.vestrace_prepare_task11_candidate_abandon_upgrade()', 'EXECUTE') \
           FROM pg_proc \
          WHERE oid='public.vestrace_prepare_candidate_abandon_and_erasure(uuid,bigint)'::regprocedure",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(before, ("vestrace_guarded_owner".into(), true, false));
    let recovery_relation_acl_before: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT relation.relname,
                COALESCE(relation.relacl::TEXT, ''),
                COALESCE(
                    string_agg(
                        attribute.attname || '=' || COALESCE(attribute.attacl::TEXT, ''),
                        E'\\n' ORDER BY attribute.attname
                    ),
                    ''
                )
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
           LEFT JOIN pg_attribute AS attribute
             ON attribute.attrelid=relation.oid
            AND attribute.attnum > 0
            AND NOT attribute.attisdropped
          WHERE namespace.nspname='public'
            AND relation.relname IN (
                'provider_dispatch_causes',
                'connection_dispatch_admissions',
                'provider_concurrency_leases',
                'external_effect_authorizations',
                'external_effect_lifecycle_transitions'
            )
          GROUP BY relation.relname, relation.relacl::TEXT
          ORDER BY relation.relname",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();
    let legacy_catalog_table_acl_before: Vec<(String, String)> = sqlx::query_as(
        "SELECT relation.relname, COALESCE(relation.relacl::TEXT, '')
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
          WHERE namespace.nspname='public'
            AND relation.relname IN ('connections','models','providers')
          ORDER BY relation.relname",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();

    let only_0186 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version == 186)
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    only_0186.run(&runtime).await.unwrap();
    let after: (String, bool, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(proowner), \
                to_regprocedure('public.vestrace_prepare_task11_candidate_abandon_upgrade()') IS NOT NULL, \
                COALESCE(has_function_privilege('vestrace', \
                    to_regprocedure('public.vestrace_prepare_task11_candidate_abandon_upgrade()'), 'EXECUTE'), FALSE), \
                COALESCE(has_function_privilege('public', \
                    to_regprocedure('public.vestrace_prepare_task11_candidate_abandon_upgrade()'), 'EXECUTE'), FALSE) \
           FROM pg_proc \
          WHERE oid='public.vestrace_prepare_candidate_abandon_and_erasure(uuid,bigint)'::regprocedure",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(
        after,
        ("vestrace_guarded_owner".into(), true, false, false),
        "0186 must return its exact guarded function and leave the one-shot bridge inert"
    );
    let recovery_authority_acl: (String, bool, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(proowner),
                has_function_privilege('vestrace',
                    'public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)',
                    'EXECUTE'),
                has_function_privilege('public',
                    'public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)',
                    'EXECUTE'),
                prosecdef
           FROM pg_proc
          WHERE oid='public.vestrace_lock_run_step_attempt_recovery_authority(uuid,uuid,uuid)'::regprocedure",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(
        recovery_authority_acl,
        ("vestrace_guarded_owner".into(), true, false, true),
        "0185→0186 must hand the recovery authority back to the guarded owner and only runtime"
    );
    let publication_evidence_acl: (String, bool, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(proowner),
                has_function_privilege('vestrace_guarded_owner',
                    'public.vestrace_run_step_publication_evidence_is_exact(uuid,uuid,uuid,bigint,uuid,uuid,uuid,uuid)',
                    'EXECUTE'),
                has_function_privilege('public',
                    'public.vestrace_run_step_publication_evidence_is_exact(uuid,uuid,uuid,bigint,uuid,uuid,uuid,uuid)',
                    'EXECUTE'),
                prosecdef
           FROM pg_proc
          WHERE oid='public.vestrace_run_step_publication_evidence_is_exact(uuid,uuid,uuid,bigint,uuid,uuid,uuid,uuid)'::regprocedure",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(
        publication_evidence_acl,
        ("vestrace".into(), true, false, true),
        "0185-to-0186 must leave publication evidence runtime-owned and guarded-only"
    );
    let recovery_relation_acl_after: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT relation.relname,
                COALESCE(relation.relacl::TEXT, ''),
                COALESCE(
                    string_agg(
                        attribute.attname || '=' || COALESCE(attribute.attacl::TEXT, ''),
                        E'\\n' ORDER BY attribute.attname
                    ),
                    ''
                )
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
           LEFT JOIN pg_attribute AS attribute
             ON attribute.attrelid=relation.oid
            AND attribute.attnum > 0
            AND NOT attribute.attisdropped
          WHERE namespace.nspname='public'
            AND relation.relname IN (
                'provider_dispatch_causes',
                'connection_dispatch_admissions',
                'provider_concurrency_leases',
                'external_effect_authorizations',
                'external_effect_lifecycle_transitions'
            )
          GROUP BY relation.relname, relation.relacl::TEXT
          ORDER BY relation.relname",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(
        recovery_relation_acl_after, recovery_relation_acl_before,
        "0186 recovery authority must not mutate historical canonical-table or column ACLs"
    );

    // The 0185 upgrade runner is the runtime role and owns legacy catalog
    // tables.  `has_*_privilege` therefore cannot distinguish ownership from
    // the narrowly-scoped 0186 grants.  Inspect the explicit column ACLs
    // written by the forward migration instead.
    let explicit_catalog_column_acl: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT relation.relname, attribute.attname, privilege.privilege_type
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
           JOIN pg_attribute AS attribute ON attribute.attrelid=relation.oid
           CROSS JOIN LATERAL aclexplode(attribute.attacl) AS privilege
           JOIN pg_roles AS grantee ON grantee.oid=privilege.grantee
          WHERE namespace.nspname='public'
            AND relation.relname IN ('connections','models','providers')
            AND attribute.attnum > 0
            AND NOT attribute.attisdropped
            AND grantee.rolname='vestrace'
            AND privilege.privilege_type IN ('SELECT','INSERT','UPDATE','REFERENCES')
          ORDER BY relation.relname, attribute.attname, privilege.privilege_type",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(
        explicit_catalog_column_acl,
        vec![
            ("connections".into(), "created_at".into(), "SELECT".into()),
            ("connections".into(), "id".into(), "SELECT".into()),
            ("connections".into(), "workspace_id".into(), "SELECT".into()),
            ("models".into(), "created_at".into(), "SELECT".into()),
            ("models".into(), "id".into(), "SELECT".into()),
            ("models".into(), "provider_id".into(), "SELECT".into()),
            ("models".into(), "workspace_id".into(), "SELECT".into()),
            ("providers".into(), "id".into(), "SELECT".into()),
            ("providers".into(), "workspace_id".into(), "SELECT".into()),
        ],
        "0186 may grant runtime only stable identity columns, never names, costs, or DML"
    );
    let legacy_catalog_table_acl_after: Vec<(String, String)> = sqlx::query_as(
        "SELECT relation.relname, COALESCE(relation.relacl::TEXT, '')
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
          WHERE namespace.nspname='public'
            AND relation.relname IN ('connections','models','providers')
          ORDER BY relation.relname",
    )
    .fetch_all(&runtime)
    .await
    .unwrap();
    assert_eq!(
        legacy_catalog_table_acl_after, legacy_catalog_table_acl_before,
        "0186 must not change historical legacy-catalog table ACLs"
    );
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn accepted_p02_database_acquires_p03_helpers_before_runtime_upgrade(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    let pre_p02 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version < 166)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    pre_p02.run(&pool).await.unwrap();
    hand_database_to_runtime(&pool).await;
    sqlx::query(
        "DO $$ DECLARE target RECORD; BEGIN FOR target IN \
         SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relkind = 'r' LOOP \
         EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace', target.relname); END LOOP; END $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(provisioner_sql_between(
        "-- P02 migrations run as the runtime role",
        "-- P03 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the real P02 portion of the provisioner must model the accepted volume");

    let runtime = runtime_pool(&pool).await;
    let p02 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| (166..=175).contains(&migration.version))
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    p02.run(&runtime).await.unwrap();
    let p03_helpers_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_proc WHERE oid IN ( \
         to_regprocedure('public.vestrace_assign_p03_table_owner(regclass)'), \
         to_regprocedure('public.vestrace_assign_p03_function_owner(regprocedure)') )",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(p03_helpers_before, 0);
    runtime.close().await;

    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("rerunning the real provisioner must install the P03 bridge");
    let runtime = runtime_pool(&pool).await;
    let p03 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version >= 176)
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    p03.run(&runtime).await.unwrap();
    assert_final_p03_schema(&runtime).await;
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn existing_0183_volume_refreshes_and_invokes_dependency_grant_before_dispatch(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the real provisioner must install the pre-migration bridge");

    let through_0183 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 183)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_0183.run(&pool).await.unwrap();
    sqlx::query("DROP FUNCTION public.vestrace_install_provider_result_live_trigger()")
        .execute(&pool)
        .await
        .unwrap();
    let runtime_without_installer = runtime_pool(&pool).await;
    let migration_0184 = MIGRATOR
        .iter()
        .find(|migration| migration.version == 184)
        .expect("migration 0184");
    let mut refused = runtime_without_installer.begin().await.unwrap();
    let missing_installer = sqlx::raw_sql(migration_0184.sql.as_ref())
        .execute(&mut *refused)
        .await
        .unwrap_err();
    assert_eq!(
        missing_installer
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501")
    );
    refused.rollback().await.unwrap();
    runtime_without_installer.close().await;
    sqlx::query(
        "DO $$ DECLARE target RECORD; BEGIN FOR target IN
         SELECT relation.relname
           FROM pg_class AS relation
           JOIN pg_namespace AS namespace ON namespace.oid=relation.relnamespace
          WHERE namespace.nspname='public' AND relation.relkind='r'
            AND pg_get_userbyid(relation.relowner)=current_user
            AND relation.relname NOT IN (
                'external_effect_authorizations','audit_events','idempotency_keys','outbox'
            )
         LOOP EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace',target.relname);
         END LOOP; END $$",
    )
    .execute(&pool)
    .await
    .expect("the fixture must retain only the observed admin-owned authorization exception");

    sqlx::raw_sql(
        "REVOKE SELECT, INSERT ON TABLE
             public.external_effect_authorizations,
             public.audit_events,
             public.idempotency_keys
         FROM vestrace;
         REVOKE SELECT, INSERT, UPDATE ON TABLE public.outbox FROM vestrace;
         REVOKE UPDATE(id) ON TABLE public.external_effect_intents
             FROM vestrace_guarded_owner;
         REVOKE SELECT ON TABLE public.content_material_bytes FROM vestrace;
         REVOKE SELECT (workspace_id,intent_id,credential_revision_id,ciphertext)
              ON TABLE public.credential_prepared_materials FROM vestrace;
         REVOKE SELECT ON TABLE public.credential_prepared_attachments FROM vestrace;
         GRANT INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
              ON TABLE public.credential_prepared_materials,
                       public.credential_prepared_attachments TO vestrace;
         GRANT INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
              ON TABLE public.credential_prepared_materials,
                       public.credential_prepared_attachments TO PUBLIC;
         GRANT SELECT (id, created_at)
              ON TABLE public.credential_prepared_materials TO vestrace, PUBLIC;
         CREATE OR REPLACE FUNCTION public.vestrace_grant_p03_dependency_references()
         RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER
         SET search_path = public, pg_temp AS $$ BEGIN NULL; END $$;",
    )
    .execute(&pool)
    .await
    .expect("the fixture must model the stale 0183 helper and missing grant");
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("rerunning the real provisioner must refresh the dependency helper");
    let material_intent_owner_before_0184: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(relowner) FROM pg_class
          WHERE oid='public.material_key_creation_intents'::regclass",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let installer_before_0184: (bool, bool) = sqlx::query_as(
        "SELECT has_function_privilege(
                    'vestrace','public.vestrace_install_provider_result_live_trigger()','EXECUTE'
                ),
                has_function_privilege(
                    'public','public.vestrace_install_provider_result_live_trigger()','EXECUTE'
                )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(installer_before_0184, (true, false));

    let prepared_before_0184: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_column_privilege('vestrace','public.credential_prepared_materials','workspace_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','intent_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','credential_revision_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','ciphertext','SELECT')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        prepared_before_0184,
        (false, false, false, false),
        "the refreshed helper must remain unapplied until migration 0184 invokes it"
    );
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT has_table_privilege('vestrace','public.credential_prepared_attachments','SELECT')"
    )
    .fetch_one(&pool)
    .await
    .unwrap());

    for table in [
        "external_effect_authorizations",
        "audit_events",
        "idempotency_keys",
        "outbox",
    ] {
        let before_0184: (bool, bool, bool) = sqlx::query_as(
            "SELECT has_table_privilege('vestrace',$1,'SELECT'),
                    has_table_privilege('vestrace',$1,'INSERT'),
                    has_table_privilege('vestrace',$1,'UPDATE')",
        )
        .bind(format!("public.{table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let owner_before_0184: bool = sqlx::query_scalar(
            "SELECT pg_get_userbyid(relowner) = current_user FROM pg_class
              WHERE oid=$1::regclass",
        )
        .bind(format!("public.{table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            owner_before_0184,
            "this regression deliberately models the admin-owned {table} exception"
        );
        assert_eq!(
            before_0184,
            (false, false, false),
            "the provisioner refreshes the helper; migration 0184 must invoke it for {table}"
        );
    }

    let runtime = runtime_pool(&pool).await;
    let only_0184 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version == 184)
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    only_0184.run(&runtime).await.unwrap();

    let material_intent_owner_after_0184: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(relowner) FROM pg_class
          WHERE oid='public.material_key_creation_intents'::regclass",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        material_intent_owner_after_0184, material_intent_owner_before_0184,
        "0184 must never re-own the pre-P03 material intent table"
    );
    let installer_after_0184: (bool, bool) = sqlx::query_as(
        "SELECT has_function_privilege(
                    'vestrace','public.vestrace_install_provider_result_live_trigger()','EXECUTE'
                ),
                has_function_privilege(
                    'public','public.vestrace_install_provider_result_live_trigger()','EXECUTE'
                )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(installer_after_0184, (false, false));

    let reconstruction_upgrade_acl: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_table_privilege('vestrace_guarded_owner','public.external_effect_intents','SELECT'),
            has_table_privilege('vestrace_guarded_owner','public.external_effect_intents','UPDATE'),
            has_column_privilege('vestrace_guarded_owner','public.external_effect_intents','id','UPDATE'),
            has_column_privilege('vestrace_guarded_owner','public.external_effect_intents','payload','UPDATE'),
            has_table_privilege('vestrace','public.content_material_bytes','SELECT'),
            has_table_privilege('vestrace','public.content_material_bytes','REFERENCES'),
            has_function_privilege('vestrace','public.vestrace_lock_model_request_evidence_for_reconstruction(uuid,uuid)','EXECUTE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        reconstruction_upgrade_acl,
        (true, false, true, false, true, false, true),
        "only 0184 must invoke the refreshed reconstruction-lock dependency grants"
    );
    let prepared_after_0184: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_column_privilege('vestrace','public.credential_prepared_materials','workspace_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','intent_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','credential_revision_id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','ciphertext','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','id','SELECT'),
            has_column_privilege('vestrace','public.credential_prepared_materials','created_at','SELECT'),
            has_table_privilege('vestrace','public.credential_prepared_materials','UPDATE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        prepared_after_0184,
        (true, true, true, true, false, false, false),
        "only 0184 must invoke the refreshed exact ciphertext-read grant"
    );
    let material_table_after_0184: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_table_privilege('vestrace','public.credential_prepared_materials','SELECT'),
            has_table_privilege('vestrace','public.credential_prepared_materials','INSERT'),
            has_table_privilege('vestrace','public.credential_prepared_materials','UPDATE'),
            has_table_privilege('vestrace','public.credential_prepared_materials','DELETE'),
            has_table_privilege('vestrace','public.credential_prepared_materials','TRUNCATE'),
            has_table_privilege('vestrace','public.credential_prepared_materials','REFERENCES'),
            has_table_privilege('vestrace','public.credential_prepared_materials','TRIGGER')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        material_table_after_0184,
        (false, false, false, false, false, false, false)
    );
    let attachment_after_0184: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT
            has_table_privilege('vestrace','public.credential_prepared_attachments','SELECT'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','INSERT'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','UPDATE'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','DELETE'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','TRUNCATE'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','REFERENCES'),
            has_table_privilege('vestrace','public.credential_prepared_attachments','TRIGGER')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        attachment_after_0184,
        (true, false, false, false, false, false, false)
    );
    for relation in [
        "public.credential_prepared_materials",
        "public.credential_prepared_attachments",
    ] {
        let runtime_column_mutation_acl_is_empty: bool = sqlx::query_scalar(
            "SELECT bool_and(
                 NOT has_column_privilege('vestrace',$1,column_name,'INSERT')
                 AND NOT has_column_privilege('vestrace',$1,column_name,'UPDATE')
                 AND NOT has_column_privilege('vestrace',$1,column_name,'REFERENCES'))
               FROM information_schema.columns
              WHERE table_schema='public' AND table_name=split_part($1,'.',2)",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(runtime_column_mutation_acl_is_empty, "{relation}");
        let public_after_0184: (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT
                has_table_privilege('public',$1,'SELECT'),
                has_table_privilege('public',$1,'INSERT'),
                has_table_privilege('public',$1,'UPDATE'),
                has_table_privilege('public',$1,'DELETE'),
                has_table_privilege('public',$1,'TRUNCATE'),
                has_table_privilege('public',$1,'REFERENCES'),
                has_table_privilege('public',$1,'TRIGGER')",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            public_after_0184,
            (false, false, false, false, false, false, false),
            "0184 did not clear stale PUBLIC privilege on {relation}"
        );
        let public_column_acl_is_empty: bool = sqlx::query_scalar(
            "SELECT bool_and(
                 NOT has_column_privilege('public',$1,column_name,'SELECT')
                 AND NOT has_column_privilege('public',$1,column_name,'INSERT')
                 AND NOT has_column_privilege('public',$1,column_name,'UPDATE')
                 AND NOT has_column_privilege('public',$1,column_name,'REFERENCES'))
               FROM information_schema.columns
              WHERE table_schema='public' AND table_name=split_part($1,'.',2)",
        )
        .bind(relation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(public_column_acl_is_empty, "{relation}");
    }
    let reconstruction_function_owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(proowner) FROM pg_proc
          WHERE oid='public.vestrace_lock_model_request_evidence_for_reconstruction(uuid,uuid)'::regprocedure",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reconstruction_function_owner, "vestrace_guarded_owner");
    let provider_result_live_guard: (String, String, bool, bool, bool) = sqlx::query_as(
        "SELECT pg_get_userbyid(procedure.proowner), \
                COALESCE(array_to_string(procedure.proconfig,','),''), \
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'), \
                trigger.tgdeferrable, trigger.tginitdeferred \
           FROM pg_proc AS procedure \
           JOIN pg_trigger AS trigger ON trigger.tgfoid=procedure.oid \
          WHERE procedure.oid='public.vestrace_validate_provider_result_material_live()'::regprocedure \
            AND trigger.tgrelid='public.material_key_creation_intents'::regclass \
            AND trigger.tgname='material_key_creation_intents_provider_result_live'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        provider_result_live_guard,
        (
            "vestrace_guarded_owner".into(),
            "search_path=pg_catalog".into(),
            false,
            true,
            true,
        )
    );

    for (table, expected_update) in [
        ("external_effect_authorizations", false),
        ("audit_events", false),
        ("idempotency_keys", false),
        ("outbox", true),
    ] {
        let owner_unchanged: bool = sqlx::query_scalar(
            "SELECT pg_get_userbyid(relowner) = current_user FROM pg_class
              WHERE oid=$1::regclass",
        )
        .bind(format!("public.{table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(owner_unchanged, "0184 must not re-own {table}");
        let after_0184: (bool, bool, bool, bool) = sqlx::query_as(
            "SELECT COALESCE(bool_or(privilege_type='SELECT'),false),
                    COALESCE(bool_or(privilege_type='INSERT'),false),
                    COALESCE(bool_or(privilege_type='UPDATE'),false),
                    COALESCE(bool_or(privilege_type='DELETE'),false)
               FROM pg_class AS relation
               CROSS JOIN LATERAL aclexplode(COALESCE(relation.relacl, acldefault('r',relation.relowner)))
              WHERE relation.oid=$1::regclass
                AND grantee=(SELECT oid FROM pg_roles WHERE rolname='vestrace')",
        )
        .bind(format!("public.{table}"))
        .fetch_one(&runtime)
        .await
        .unwrap();
        assert_eq!(
            after_0184,
            (true, true, expected_update, false),
            "0184 must invoke the refreshed exact grant for {table}"
        );
    }

    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let effect_id = Uuid::now_v7();
    let authorization_id = Uuid::now_v7();
    let audit_id = Uuid::now_v7();
    let outbox_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces(id,slug) VALUES($1,$2)")
        .bind(workspace_id)
        .bind(format!("upgrade-dispatch-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals(id,workspace_id,identifier) VALUES($1,$2,$3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("upgrade-dispatch-{principal_id}"))
        .execute(&pool)
        .await
        .unwrap();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id',$1,true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_intents(id,workspace_id,adapter,payload)
         VALUES($1,$2,'provider','{}'::jsonb)",
    )
    .bind(effect_id)
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO external_effect_authorizations(
            id,effect_id,workspace_id,policy_version,subject_id,capability,operation,
            resource_scope,result,reason,input_state,decided_at,payload
         ) VALUES($1,$2,$3,'v1',$4,'model.use','dispatch','provider','allow',
                  'configured_allowance','{}'::jsonb,NOW(),'{}'::jsonb)",
    )
    .bind(authorization_id)
    .bind(effect_id)
    .bind(workspace_id)
    .bind(Uuid::now_v7())
    .execute(&mut *transaction)
    .await
    .expect("the provider-dispatch authorization prerequisite must be writable");
    sqlx::query(
        "INSERT INTO audit_events(id,workspace_id,principal_id,action,resource_type,resource_id,payload)
         VALUES($1,$2,$3,'provider.dispatch','external_effect',$4,'{}'::jsonb)",
    )
    .bind(audit_id)
    .bind(workspace_id)
    .bind(principal_id)
    .bind(effect_id)
    .execute(&mut *transaction)
    .await
    .expect("the provider-dispatch audit prerequisite must be writable");
    sqlx::query(
        "INSERT INTO idempotency_keys(
            idempotency_key,workspace_id,request_hash,response_payload,status,expires_at
         ) VALUES('upgrade-dispatch',$1,'hash','{}'::jsonb,'completed',NOW()+INTERVAL '1 day')",
    )
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await
    .expect("the provider-dispatch idempotency prerequisite must be writable");
    sqlx::query(
        "INSERT INTO outbox(id,workspace_id,topic,payload)
         VALUES($1,$2,'provider.dispatch.prepared','{}'::jsonb)",
    )
    .bind(outbox_id)
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await
    .expect("the provider-dispatch outbox prerequisite must be writable");
    let updated = sqlx::query("UPDATE outbox SET processed_at=NOW() WHERE id=$1")
        .bind(outbox_id)
        .execute(&mut *transaction)
        .await
        .expect("the delivery-state update must remain available");
    assert_eq!(updated.rows_affected(), 1);
    transaction.commit().await.unwrap();
    let rows: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM external_effect_authorizations WHERE id=$1),
                (SELECT COUNT(*) FROM audit_events WHERE id=$2),
                (SELECT COUNT(*) FROM idempotency_keys WHERE workspace_id=$3 AND idempotency_key='upgrade-dispatch'),
                (SELECT COUNT(*) FROM outbox WHERE id=$4 AND processed_at IS NOT NULL)",
    )
    .bind(authorization_id)
    .bind(audit_id)
    .bind(workspace_id)
    .bind(outbox_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, (1, 1, 1, 1));
    runtime.close().await;
    for _ in 0..2 {
        sqlx::raw_sql(provisioner_sql_from(
            "-- P02 migrations run as the runtime role",
        ))
        .execute(&pool)
        .await
        .expect("post-0184 bootstrap reruns must remain idempotent");
        let installer_exists: bool = sqlx::query_scalar(
            "SELECT to_regprocedure(
                'public.vestrace_install_provider_result_live_trigger()'
             ) IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            !installer_exists,
            "post-0184 bootstrap must remove the installer"
        );
    }
}
