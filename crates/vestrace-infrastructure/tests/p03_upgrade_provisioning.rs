mod common;

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
const EXPECTED_GUARDED_TABLES: [&str; 121] = [
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
    "model_binding_snapshot_scopes",
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
    "embedding_corpus_generation_members",
    "memory_embeddings",
    "embedding_jobs",
    "embedding_job_material_intents",
    "embedding_job_termination_receipts",
    "embedding_delivery_acceptance_receipts",
    "embedding_delivery_source_memberships",
    "embedding_job_pre_dispatch_retirement_authorities",
    "embedding_output_key_retirement_requests",
    "embedding_output_key_receipts",
    "embedding_output_key_retirement_receipts",
    "embedding_space_corpus_states",
    "embedding_index_generation_guards",
    "embedding_job_result_preparations",
    "embedding_projection_entries",
    "embedding_job_result_prepared_attachments",
    "embedding_projection_source_dependencies",
    "embedding_job_credential_completion_blockers",
    "embedding_result_credential_blocker_adoptions",
    "embedding_result_key_binding_receipts",
    "embedding_job_result_publications",
    "embedding_index_rebuild_events",
    "embedding_index_build_attempts",
    "embedding_index_build_observations",
    "embedding_transition_plan_recipes",
    "embedding_transition_plans",
    "embedding_transitions",
    "embedding_transition_ambiguity_carries",
    "embedding_transition_ambiguity_carry_recipes",
    "embedding_transition_barriers",
    "embedding_transition_barrier_recipes",
    // P04 Task 5, migration 0199.
    "embedding_job_work_claims",
    // P04 Task 6, migration 0200.
    "embedding_transition_batches",
    "embedding_transition_batch_recipes",
    "embedding_transition_recipe_dependencies",
    "embedding_transition_job_attempts",
    "embedding_transition_recipe_satisfactions",
    "embedding_transition_observations",
    // P04 Task 7, migration 0201.
    "embedding_transition_activation_receipts",
    // P04 Task 8, migration 0202.
    "embedding_retrieval_fences",
    "embedding_retrieval_results",
    "embedding_retrieval_result_references",
    "embedding_retrieval_generation_changes",
    "embedding_retrieval_retry_edges",
    // P04 Task 9, migration 0203.
    "embedding_erasure_propagations",
    "embedding_erasure_revoked_members",
    // P04 Task 10, migration 0204.
    "embedding_legacy_adoptions",
    "embedding_legacy_adoption_members",
    "embedding_legacy_adoption_blockers",
    "embedding_legacy_cutover_receipts",
    "embedding_legacy_identity_tombstones",
    "embedding_legacy_retirement_gate",
    "model_data_policy_decisions",
];

fn provisioner_sql_from(marker: &str) -> &'static str {
    let start = PROVISIONER
        .find(marker)
        .unwrap_or_else(|| panic!("missing provisioner marker {marker}"));
    PROVISIONER[start..]
        .split_once("\nSQL\n")
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

#[test]
fn provisioner_sql_from_stops_at_its_first_heredoc_terminator() {
    let sql = provisioner_sql_from("-- P02 migrations run as the runtime role");

    assert!(!sql.contains("\nSQL\n"));
    assert!(!sql.contains("VESTRACE_SAFETY_SUPERVISOR_PASSWORD"));
    assert!(sql.contains("vestrace_finish_embedding_memory_references_upgrade"));
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

async fn assert_final_p03_schema(pool: &PgPool, task10_installer_exists: bool) {
    let versions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE version BETWEEN 176 AND 184 AND success",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(versions, 9);

    let output_key_migration_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=193 AND success)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let result_preparation_migration_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=194 AND success)",
    )
    .fetch_one(pool)
    .await
    .unwrap();

    let result_finalization_migration_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=195 AND success)",
    )
    .fetch_one(pool)
    .await
    .unwrap();

    // Every later completion migration is read the same way, so a database
    // built to an earlier point still compares against an exact set rather
    // than a set that happens to be a superset of what it has.
    let mut applied_since = std::collections::BTreeMap::new();
    for version in [199_i64, 200, 201, 202, 203, 204] {
        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=$1 AND success)",
        )
        .bind(version)
        .fetch_one(pool)
        .await
        .unwrap();
        applied_since.insert(version, applied);
    }

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
    let mut expected_guarded: BTreeSet<String> = EXPECTED_GUARDED_TABLES.map(str::to_owned).into();
    let canonical_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=197 AND success)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    if !canonical_applied {
        expected_guarded.remove("memory_embeddings");
    }
    let index_builds_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=198 AND success)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    if !index_builds_applied {
        expected_guarded.remove("embedding_index_build_attempts");
        expected_guarded.remove("embedding_index_build_observations");
    }

    if !output_key_migration_applied {
        for table in [
            "embedding_delivery_acceptance_receipts",
            "embedding_delivery_source_memberships",
            "embedding_job_pre_dispatch_retirement_authorities",
            "embedding_output_key_retirement_requests",
            "embedding_output_key_receipts",
            "embedding_output_key_retirement_receipts",
        ] {
            expected_guarded.remove(table);
        }
    }
    if !result_preparation_migration_applied {
        for table in [
            "embedding_space_corpus_states",
            "embedding_index_generation_guards",
            "embedding_job_result_preparations",
            "embedding_projection_entries",
            "embedding_job_result_prepared_attachments",
            "embedding_projection_source_dependencies",
        ] {
            expected_guarded.remove(table);
        }
    }
    if !result_finalization_migration_applied {
        for table in [
            "embedding_job_credential_completion_blockers",
            "embedding_result_credential_blocker_adoptions",
            "embedding_result_key_binding_receipts",
            "embedding_job_result_publications",
            "embedding_index_rebuild_events",
        ] {
            expected_guarded.remove(table);
        }
    }
    for (version, tables) in [
        (199_i64, &["embedding_job_work_claims"][..]),
        (
            200,
            &[
                "embedding_transition_batches",
                "embedding_transition_batch_recipes",
                "embedding_transition_recipe_dependencies",
                "embedding_transition_job_attempts",
                "embedding_transition_recipe_satisfactions",
                "embedding_transition_observations",
            ][..],
        ),
        (201, &["embedding_transition_activation_receipts"][..]),
        (
            202,
            &[
                "embedding_retrieval_fences",
                "embedding_retrieval_results",
                "embedding_retrieval_result_references",
                "embedding_retrieval_generation_changes",
                "embedding_retrieval_retry_edges",
            ][..],
        ),
        (
            203,
            &[
                "embedding_erasure_propagations",
                "embedding_erasure_revoked_members",
            ][..],
        ),
        (
            204,
            &[
                "embedding_legacy_adoptions",
                "embedding_legacy_adoption_members",
                "embedding_legacy_adoption_blockers",
                "embedding_legacy_cutover_receipts",
                "embedding_legacy_identity_tombstones",
                "embedding_legacy_retirement_gate",
            ][..],
        ),
    ] {
        if !applied_since.get(&version).copied().unwrap_or(false) {
            for table in tables {
                expected_guarded.remove(*table);
            }
        }
    }
    assert_eq!(
        guarded.into_iter().collect::<BTreeSet<_>>(),
        expected_guarded,
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
        (task10_installer_exists, false, false),
        "the closed 0184 installer must reflect whether the real bootstrap has refreshed the volume"
    );

    let termination_function_acl: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT procedure.proname, pg_get_userbyid(procedure.proowner), \
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'), \
                has_function_privilege('public',procedure.oid,'EXECUTE') \
           FROM pg_proc AS procedure \
          WHERE procedure.oid IN ( \
              'public.vestrace_validate_embedding_job_output_membership()'::regprocedure, \
              'public.vestrace_reserve_embedding_job_output_intent(uuid,uuid,uuid,bigint,uuid,uuid,uuid)'::regprocedure, \
              'public.vestrace_terminate_embedding_job_pre_dispatch(uuid,uuid,uuid,uuid,bigint,text,text,text,uuid,text,text,text,text,text)'::regprocedure, \
              'public.vestrace_fence_embedding_job_dispatching()'::regprocedure, \
              'public.vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)'::regprocedure \
          ) ORDER BY procedure.proname",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        termination_function_acl,
        vec![
            (
                "vestrace_fence_embedding_job_dispatching".into(),
                "vestrace_guarded_owner".into(),
                false,
                false,
            ),
            (
                "vestrace_lock_embedding_job_pre_dispatch_gate".into(),
                "vestrace_guarded_owner".into(),
                false,
                false,
            ),
            (
                "vestrace_reserve_embedding_job_output_intent".into(),
                "vestrace_guarded_owner".into(),
                true,
                false,
            ),
            (
                "vestrace_terminate_embedding_job_pre_dispatch".into(),
                "vestrace_guarded_owner".into(),
                true,
                false,
            ),
            (
                "vestrace_validate_embedding_job_output_membership".into(),
                "vestrace_guarded_owner".into(),
                false,
                false,
            ),
        ],
        "0192 exposes only its reservation and termination commands to runtime"
    );

    let lifecycle_owner: String = sqlx::query_scalar(
        "SELECT pg_get_userbyid(relowner) \
           FROM pg_class WHERE oid='public.external_effect_lifecycle_transitions'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        lifecycle_owner, "vestrace",
        "0192 must not re-own the lifecycle table"
    );

    let material_intent_trigger_acl: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('vestrace', \
            'public.material_key_creation_intents','TRIGGER')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        !material_intent_trigger_acl,
        "0192 must revoke its narrow material-intent trigger installation grant"
    );

    let termination_upgrade_acl: (bool, bool, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT to_regprocedure('public.vestrace_prepare_p04_termination_upgrade()') IS NOT NULL, \
                COALESCE(has_function_privilege('vestrace', \
                    to_regprocedure('public.vestrace_prepare_p04_termination_upgrade()'),'EXECUTE'),FALSE), \
                COALESCE(has_function_privilege('public', \
                    to_regprocedure('public.vestrace_prepare_p04_termination_upgrade()'),'EXECUTE'),FALSE), \
                to_regprocedure('public.vestrace_finish_p04_termination_upgrade()') IS NOT NULL, \
                COALESCE(has_function_privilege('vestrace', \
                    to_regprocedure('public.vestrace_finish_p04_termination_upgrade()'),'EXECUTE'),FALSE), \
                COALESCE(has_function_privilege('public', \
                    to_regprocedure('public.vestrace_finish_p04_termination_upgrade()'),'EXECUTE'),FALSE)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        termination_upgrade_acl,
        (true, false, false, true, false, false),
        "0192 must close both one-shot ownership hand-backs"
    );

    if output_key_migration_applied {
        let output_key_function_acl: Vec<(String, String, bool, bool)> = sqlx::query_as(
        "SELECT procedure.proname,pg_get_userbyid(procedure.proowner), \
                has_function_privilege('vestrace',procedure.oid,'EXECUTE'), \
                has_function_privilege('public',procedure.oid,'EXECUTE') \
           FROM pg_proc procedure WHERE procedure.oid IN ( \
             'public.vestrace_validate_delivery_source_membership()'::regprocedure, \
             'public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)'::regprocedure, \
             'public.vestrace_finalize_delivery_embedding_outputs(uuid,uuid,uuid)'::regprocedure, \
             'public.vestrace_request_embedding_output_retirement(uuid,uuid,uuid,uuid,bigint,text,text,text,uuid,text,text,text,text,text)'::regprocedure, \
             'public.vestrace_claim_embedding_output_key(uuid)'::regprocedure, \
             'public.vestrace_record_embedding_output_key_receipt(uuid,uuid,uuid)'::regprocedure, \
             'public.vestrace_embedding_output_key_progress(uuid,uuid)'::regprocedure, \
             'public.vestrace_record_embedding_output_key_retirement(uuid,uuid,uuid)'::regprocedure, \
             'public.vestrace_validate_embedding_output_termination_authority()'::regprocedure \
           ) ORDER BY procedure.proname",
    )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(output_key_function_acl.len(), 9);
        for (name, owner, runtime_execute, public_execute) in output_key_function_acl {
            assert_eq!(owner, "vestrace_guarded_owner", "{name}");
            assert!(!public_execute, "{name}");
            assert_eq!(
                runtime_execute,
                !matches!(
                    name.as_str(),
                    "vestrace_validate_delivery_source_membership"
                        | "vestrace_validate_embedding_output_termination_authority"
                ),
                "{name}"
            );
        }
        for table in [
            "embedding_delivery_acceptance_receipts",
            "embedding_delivery_source_memberships",
            "embedding_job_pre_dispatch_retirement_authorities",
            "embedding_output_key_retirement_requests",
            "embedding_output_key_receipts",
            "embedding_output_key_retirement_receipts",
        ] {
            let authority: (String, bool, bool) = sqlx::query_as(
                "SELECT pg_get_userbyid(relowner),relrowsecurity,relforcerowsecurity \
               FROM pg_class WHERE oid=$1::regclass",
            )
            .bind(format!("public.{table}"))
            .fetch_one(pool)
            .await
            .unwrap();
            assert_eq!(
                authority,
                ("vestrace_guarded_owner".into(), true, true),
                "{table}"
            );
        }
        let blocker_authority: (String, bool) = sqlx::query_as(
            "SELECT pg_get_userbyid(relowner),EXISTS(SELECT 1 FROM pg_constraint \
           WHERE conrelid='public.material_erasure_blockers'::regclass \
             AND conname='material_erasure_blockers_id_workspace_key') \
           FROM pg_class WHERE oid='public.material_erasure_blockers'::regclass",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(blocker_authority, ("vestrace_guarded_owner".into(), true));

        let output_upgrade_acl: (bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT to_regprocedure('public.vestrace_prepare_p04_output_key_upgrade()') IS NOT NULL, \
          COALESCE(has_function_privilege('vestrace',to_regprocedure('public.vestrace_prepare_p04_output_key_upgrade()'),'EXECUTE'),FALSE), \
          COALESCE(has_function_privilege('public',to_regprocedure('public.vestrace_prepare_p04_output_key_upgrade()'),'EXECUTE'),FALSE), \
          to_regprocedure('public.vestrace_finish_p04_output_key_upgrade()') IS NOT NULL, \
          COALESCE(has_function_privilege('vestrace',to_regprocedure('public.vestrace_finish_p04_output_key_upgrade()'),'EXECUTE'),FALSE), \
          COALESCE(has_function_privilege('public',to_regprocedure('public.vestrace_finish_p04_output_key_upgrade()'),'EXECUTE'),FALSE)",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            output_upgrade_acl,
            (
                task10_installer_exists,
                false,
                false,
                task10_installer_exists,
                false,
                false
            ),
            "0193 must close both one-shot output-key ownership hand-backs"
        );
    }

    if result_preparation_migration_applied {
        let result_preparation_functions: Vec<(String, String, bool, bool)> = sqlx::query_as(
            "SELECT procedure.proname,pg_get_userbyid(procedure.proowner), \
                    has_function_privilege('vestrace',procedure.oid,'EXECUTE'), \
                    has_function_privilege('public',procedure.oid,'EXECUTE') \
               FROM pg_proc AS procedure WHERE procedure.oid IN ( \
                 'public.vestrace_validate_embedding_result_preparation()'::regprocedure, \
                 'public.vestrace_validate_embedding_projection_dependency()'::regprocedure, \
                 'public.vestrace_create_embedding_result_space_guards()'::regprocedure, \
                 'public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'::regprocedure, \
                 'public.vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)'::regprocedure, \
                 'public.vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])'::regprocedure, \
                 'public.vestrace_reject_result_prepared_pre_dispatch_terminalization()'::regprocedure, \
                 'public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)'::regprocedure \
               ) ORDER BY procedure.proname",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(result_preparation_functions.len(), 8);
        for (name, owner, runtime_execute, public_execute) in result_preparation_functions {
            assert_eq!(owner, "vestrace_guarded_owner", "{name}");
            assert!(!public_execute, "{name}");
            assert_eq!(
                runtime_execute,
                matches!(
                    name.as_str(),
                    "vestrace_commit_embedding_result_preparation"
                        | "vestrace_load_embedding_result_eligibility"
                        | "vestrace_lock_embedding_job_recovery_authority"
                        | "vestrace_lock_embedding_result_completion_authority"
                ),
                "{name}"
            );
        }
    }
}

#[test]
fn compose_provisioning_precedes_runtime_migrate_without_leaking_bootstrap_credentials() {
    let provision = service_block(COMPOSE, "vestrace-role-provision");
    let migrate = service_block(COMPOSE, "vestrace-migrate-history");
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
    vestrace_infrastructure::HISTORICAL_MIGRATOR
        .run(&runtime)
        .await
        .unwrap();
    assert_final_p03_schema(&runtime, true).await;
    assert_retired_credential_erasure_function_inventory(&runtime).await;
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn existing_0191_volume_hands_termination_objects_to_runtime_once(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the real provisioner must install the 0192 ownership bridge");

    let runtime = runtime_pool(&pool).await;
    let through_0191 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 191)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_0191.run(&runtime).await.unwrap();
    runtime.close().await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the refreshed real provisioner must preserve the sealed 0191 volume");
    let runtime = runtime_pool(&pool).await;

    type TerminationUpgradePreflight = (
        String,
        bool,
        bool,
        bool,
        Vec<(String, String)>,
        String,
        bool,
        bool,
    );

    let before: TerminationUpgradePreflight = (
        sqlx::query_scalar(
            "SELECT pg_get_userbyid(relowner) \
               FROM pg_class WHERE oid='public.provider_concurrency_leases'::regclass",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT has_function_privilege('vestrace', \
                'public.vestrace_finish_p04_termination_upgrade()','EXECUTE')",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT has_function_privilege('public', \
                'public.vestrace_finish_p04_termination_upgrade()','EXECUTE')",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT has_table_privilege('vestrace', \
                'public.material_key_creation_intents','TRIGGER')",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
        sqlx::query_as(
            "SELECT procedure.proname, pg_get_userbyid(procedure.proowner) \
               FROM pg_proc AS procedure \
              WHERE procedure.oid IN ( \
                  'public.vestrace_try_admit_provider_dispatch(uuid,uuid,uuid,uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid,text,integer)'::regprocedure, \
                  'public.vestrace_lock_provider_dispatch_routing(uuid,uuid,uuid,uuid,uuid,text,uuid,uuid,uuid,uuid,uuid)'::regprocedure, \
                  'public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)'::regprocedure \
              ) ORDER BY procedure.proname",
        )
        .fetch_all(&runtime)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT pg_get_userbyid(relowner) \
               FROM pg_class WHERE oid='public.external_effect_lifecycle_transitions'::regclass",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT has_function_privilege('vestrace', \
                'public.vestrace_prepare_p04_termination_upgrade()','EXECUTE')",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT has_function_privilege('public', \
                'public.vestrace_prepare_p04_termination_upgrade()','EXECUTE')",
        )
        .fetch_one(&runtime)
        .await
        .unwrap(),
    );
    assert_eq!(before.0, "vestrace_guarded_owner");
    assert_eq!(
        before.4,
        vec![
            (
                "vestrace_lock_embedding_job_recovery_authority".into(),
                "vestrace_guarded_owner".into(),
            ),
            (
                "vestrace_lock_provider_dispatch_routing".into(),
                "vestrace_guarded_owner".into(),
            ),
            (
                "vestrace_try_admit_provider_dispatch".into(),
                "vestrace_guarded_owner".into(),
            ),
        ]
    );
    assert_eq!(before.5, "vestrace");
    assert_eq!((before.6, before.7), (true, false));
    assert!(
        !before.3,
        "the 0192 trigger grant must remain sealed until the migration calls its prepare helper"
    );
    assert_eq!((before.1, before.2), (true, false));

    let only_0192 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version == 192)
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    only_0192.run(&runtime).await.unwrap();
    assert_final_p03_schema(&runtime, false).await;
    runtime.close().await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("the post-0192 provisioner must remove closed one-shot bridges");
    let helpers_after_refresh: (bool, bool) = sqlx::query_as(
        "SELECT to_regprocedure('public.vestrace_prepare_p04_termination_upgrade()') IS NOT NULL, \
                to_regprocedure('public.vestrace_finish_p04_termination_upgrade()') IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(helpers_after_refresh, (false, false));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
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
                .filter(|migration| (176..=208).contains(&migration.version))
                .cloned()
                .collect(),
        ),
        ignore_missing: true,
        ..Migrator::DEFAULT
    };
    p03.run(&runtime).await.unwrap();
    assert_final_p03_schema(&runtime, true).await;
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

async fn assert_retired_credential_erasure_function_inventory(pool: &PgPool) {
    for (signature, executable) in [
        ("vestrace_validate_material_erasure()", false),
        (
            "vestrace_finalize_credential_material_erasure(uuid,uuid)",
            true,
        ),
    ] {
        let actual: (String, bool, bool, bool) = sqlx::query_as(
            "SELECT pg_get_userbyid(proowner),prosecdef,has_function_privilege('vestrace',oid,'EXECUTE'),EXISTS(SELECT 1 FROM aclexplode(coalesce(proacl,acldefault('f',proowner))) a WHERE a.grantee=0 AND a.privilege_type='EXECUTE') FROM pg_proc WHERE oid=$1::regprocedure",
        ).bind(signature).fetch_one(pool).await.unwrap();
        assert_eq!(
            actual,
            ("vestrace_guarded_owner".into(), true, executable, false),
            "{signature}"
        );
    }
}

#[sqlx::test(migrations = false)]
async fn existing_0195_runtime_erasure_upgrade_changes_only_two_function_owners(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    hand_database_to_runtime(&pool).await;
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .unwrap();
    let runtime = runtime_pool(&pool).await;
    let through_0195 = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 195)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    through_0195.run(&runtime).await.unwrap();
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .unwrap();
    let inventory_query = "SELECT relname,pg_get_userbyid(relowner),coalesce(relacl::text,''),relrowsecurity,relforcerowsecurity FROM pg_class WHERE relnamespace='public'::regnamespace AND relkind='r' ORDER BY relname";
    let before: Vec<(String, String, String, bool, bool)> = sqlx::query_as(inventory_query)
        .fetch_all(&pool)
        .await
        .unwrap();
    Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 196)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&runtime)
    .await
    .unwrap();
    let after: Vec<(String, String, String, bool, bool)> = sqlx::query_as(inventory_query)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        after, before,
        "0196 must not transfer table ownership or alter table ACL/RLS"
    );
    assert_retired_credential_erasure_function_inventory(&runtime).await;
    for name in [
        "vestrace_prepare_retired_credential_erasure_upgrade()",
        "vestrace_finish_retired_credential_erasure_upgrade()",
    ] {
        let executable: bool =
            sqlx::query_scalar("SELECT has_function_privilege('vestrace',$1,'EXECUTE')")
                .bind(name)
                .fetch_one(&runtime)
                .await
                .unwrap();
        assert!(!executable, "one-shot bridge must close: {name}");
    }
    Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 196)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&runtime)
    .await
    .unwrap();
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert_retired_credential_erasure_function_inventory(&runtime).await;
    let helpers:i64=sqlx::query_scalar("SELECT count(*) FROM pg_proc WHERE pronamespace='public'::regnamespace AND proname IN('vestrace_prepare_retired_credential_erasure_upgrade','vestrace_finish_retired_credential_erasure_upgrade')").fetch_one(&pool).await.unwrap();
    assert_eq!(helpers, 0);
}

#[sqlx::test(migrations = false)]
async fn retired_credential_erasure_sqlx_fallback_preserves_exact_function_acl(pool: PgPool) {
    install_extensions_from_real_provisioner(&pool).await;
    Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= 196)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    assert_retired_credential_erasure_function_inventory(&pool).await;
}

#[sqlx::test(migrations = false)]
async fn retrieval_0207_upgrade_preserves_unverified_empty_and_nonempty_results(pool: PgPool) {
    common::result_preparation_fixture::provision_result_behavior_database_through(&pool, 207)
        .await;
    let runtime = runtime_pool(&pool).await;
    let checksums: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version,checksum FROM _sqlx_migrations WHERE version<=207 ORDER BY version",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut history = Vec::new();
    for count in [0_i32, 1] {
        let base = common::prepare_delivery_embedding_job(&pool, &runtime).await;
        let workspace = base.context.workspace_id.as_uuid();
        let (space, _) = common::canonical_memory_fixture::register_space(
            &pool,
            &runtime,
            &base,
            "historical-results",
        )
        .await;
        let generation = Uuid::now_v7();
        let mut tx = runtime.begin().await.unwrap();
        common::result_preparation_fixture::scoped(&mut tx, workspace).await;
        sqlx::query("SELECT vestrace_capture_embedding_generation($1,$2,$3,1::bigint)")
            .bind(generation)
            .bind(workspace)
            .bind(space)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT vestrace_publish_embedding_generation($1,$2,$3,1::bigint)")
            .bind(workspace)
            .bind(space)
            .bind(generation)
            .execute(&mut *tx)
            .await
            .unwrap();
        // This deliberately reproduces the unchecked 0207 authority: a query
        // job in the old registration could fence a different canonical space
        // and persist unproved UUIDs. Upgrade must preserve and label that fact.
        sqlx::query("SELECT vestrace_accept_embedding_job($1,$2,$3,'retrieval_query',$4,$5,$6,NULL,NULL::bigint)")
            .bind(base.job_id.as_uuid()).bind(workspace).bind(base.space_registration_id).bind(base.snapshot_id)
            .bind(base.external_effect_id).bind(base.evidence_id).execute(&mut *tx).await.unwrap();
        let fence: Uuid = sqlx::query_scalar("SELECT vestrace_accept_embedding_retrieval_attempt($1,$2,$3,$4,NOW()+interval '30 seconds')")
            .bind(workspace).bind(base.job_id.as_uuid()).bind(Uuid::now_v7()).bind(space).fetch_one(&mut *tx).await.unwrap();
        let memories: Vec<Uuid> = (0..count).map(|_| Uuid::now_v7()).collect();
        let revisions: Vec<Uuid> = (0..count).map(|_| Uuid::now_v7()).collect();
        let ranks: Vec<i64> = (0..i64::from(count)).collect();
        let scores = vec![0.5_f64; count as usize];
        let result: Uuid = sqlx::query_scalar(
            "SELECT vestrace_finalize_embedding_retrieval_result($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(workspace)
        .bind(base.job_id.as_uuid())
        .bind(fence)
        .bind(&memories)
        .bind(&revisions)
        .bind(ranks)
        .bind(scores)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        history.push((
            workspace,
            base.job_id.as_uuid(),
            result,
            count,
            memories,
            revisions,
        ));
    }
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .unwrap();
    vestrace_infrastructure::HISTORICAL_MIGRATOR
        .run(&runtime)
        .await
        .expect("restricted runtime upgrades existing terminal results");
    let after: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version,checksum FROM _sqlx_migrations WHERE version<=207 ORDER BY version",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(after, checksums, "no historical migration checksum changed");
    for (workspace, job, result, count, memories, revisions) in history {
        let header: (i32, i32, String) = sqlx::query_as("SELECT r.provenance_version,r.reference_count,j.state FROM embedding_retrieval_results r JOIN embedding_jobs j ON j.workspace_id=r.workspace_id AND j.id=r.job_id WHERE r.workspace_id=$1 AND r.id=$2 AND j.id=$3")
            .bind(workspace).bind(result).bind(job).fetch_one(&pool).await.unwrap();
        assert_eq!(
            header,
            (0, count, "succeeded".to_owned()),
            "empty results are historical too; job stays terminal"
        );
        let references: Vec<(Uuid,Uuid,Option<Uuid>,Option<Uuid>)> = sqlx::query_as("SELECT memory_id,revision_id,projection_id,source_material_id FROM embedding_retrieval_result_references WHERE workspace_id=$1 AND result_id=$2 ORDER BY ordinal")
            .bind(workspace).bind(result).fetch_all(&pool).await.unwrap();
        assert_eq!(references.len(), count as usize);
        for (index, reference) in references.iter().enumerate() {
            assert_eq!(
                reference,
                &(memories[index], revisions[index], None, None),
                "upgrade must never fabricate provenance"
            );
        }
    }
    for (signature, executable) in [
        (
            "vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)",
            false,
        ),
        (
            "vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])",
            false,
        ),
        (
            "vestrace_resolve_embedding_memory_references(uuid,uuid,uuid[])",
            true,
        ),
        (
            "vestrace_issue_canonical_retrieval_snapshot(uuid,uuid,uuid,uuid)",
            true,
        ),
        (
            "vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,uuid,timestamptz)",
            true,
        ),
        (
            "vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],uuid[],uuid[],bigint[],double precision[])",
            true,
        ),
    ] {
        let posture: (String,bool,bool,bool) = sqlx::query_as("SELECT pg_get_userbyid(proowner),prosecdef,has_function_privilege('vestrace',oid,'EXECUTE'),has_function_privilege('public',oid,'EXECUTE') FROM pg_proc WHERE oid=$1::regprocedure")
            .bind(signature).fetch_one(&pool).await.unwrap();
        assert_eq!(
            posture,
            ("vestrace_guarded_owner".to_owned(), true, executable, false),
            "{signature}"
        );
    }
    for statement in [
        "SELECT vestrace_accept_embedding_retrieval_attempt(NULL::uuid,NULL::uuid,NULL::uuid,NULL::uuid,NULL::timestamptz)",
        "SELECT vestrace_finalize_embedding_retrieval_result(NULL::uuid,NULL::uuid,NULL::uuid,NULL::uuid[],NULL::uuid[],NULL::bigint[],NULL::float8[])",
    ] {
        let error = sqlx::query(statement).execute(&pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
        assert!(
            error
                .as_database_error()
                .unwrap()
                .message()
                .contains("retired"),
            "body itself refuses privileged callers"
        );
        let error = sqlx::query(statement).execute(&runtime).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
    }
    sqlx::raw_sql(provisioner_sql_from(
        "-- P02 migrations run as the runtime role",
    ))
    .execute(&pool)
    .await
    .expect("reprovisioning audits retired and current overloads consistently");
    runtime.close().await;
}
