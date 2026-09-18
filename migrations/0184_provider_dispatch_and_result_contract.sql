-- Task 10: normalized provider dispatch authority and retained result binding.
-- All externally callable state changes below are fixed-search-path guarded
-- operations. Runtime retains no direct write privilege on P03 authority rows.

DO $$
BEGIN
    IF to_regprocedure(
        'public.vestrace_install_provider_result_live_trigger()'
    ) IS NULL THEN
        RAISE EXCEPTION 'provider-result Live trigger installer was not provisioned'
            USING ERRCODE = '42501';
    END IF;
END
$$;

SELECT vestrace_grant_p03_dependency_references();
SELECT vestrace_prepare_task10_p03_upgrade();

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.model_request_evidence_checks'::regclass
           AND conname = 'model_request_evidence_checks_exact_root_key'
    ) THEN
        ALTER TABLE model_request_evidence_checks
            ADD CONSTRAINT model_request_evidence_checks_exact_root_key
            UNIQUE (workspace_id, evidence_root_id, id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.qualification_target_bindings'::regclass
           AND conname = 'qualification_target_bindings_exact_job_key'
    ) THEN
        ALTER TABLE qualification_target_bindings
            ADD CONSTRAINT qualification_target_bindings_exact_job_key
            UNIQUE (workspace_id, qualification_job_id, id);
    END IF;
END
$$;

CREATE TABLE provider_dispatch_causes (
    external_effect_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    model_request_evidence_id UUID NOT NULL,
    model_request_evidence_check_id UUID NOT NULL,
    cause_kind TEXT NOT NULL CHECK (cause_kind IN ('run_step', 'qualification_probe')),
    run_id UUID,
    step_id UUID,
    model_binding_snapshot_id UUID,
    qualification_job_id UUID,
    qualification_target_binding_id UUID,
    qualification_probe_ordinal TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT provider_dispatch_causes_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_dispatch_causes_exact_evidence_check_fkey
        FOREIGN KEY (
            workspace_id, model_request_evidence_id, model_request_evidence_check_id
        ) REFERENCES model_request_evidence_checks(
            workspace_id, evidence_root_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT provider_dispatch_causes_run_step_fkey
        FOREIGN KEY (workspace_id, run_id, step_id)
        REFERENCES run_steps(workspace_id, run_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_dispatch_causes_snapshot_fkey
        FOREIGN KEY (workspace_id, model_binding_snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_dispatch_causes_qualification_job_fkey
        FOREIGN KEY (workspace_id, qualification_job_id)
        REFERENCES qualification_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_dispatch_causes_exact_qualification_target_fkey
        FOREIGN KEY (
            workspace_id, qualification_job_id, qualification_target_binding_id
        ) REFERENCES qualification_target_bindings(
            workspace_id, qualification_job_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT provider_dispatch_causes_closed_matrix CHECK (
        (cause_kind = 'run_step'
         AND run_id IS NOT NULL
         AND step_id IS NOT NULL
         AND model_binding_snapshot_id IS NOT NULL
         AND qualification_job_id IS NULL
         AND qualification_target_binding_id IS NULL
         AND qualification_probe_ordinal IS NULL)
        OR
        (cause_kind = 'qualification_probe'
         AND run_id IS NULL
         AND step_id IS NULL
         AND model_binding_snapshot_id IS NULL
         AND qualification_job_id IS NOT NULL
         AND qualification_target_binding_id IS NOT NULL
         AND qualification_probe_ordinal IN (
             '00','10','15','20','30','35','40','50','60','70','80','90'
         ))
    ),
    CONSTRAINT provider_dispatch_causes_workspace_id_effect_key
        UNIQUE (workspace_id, external_effect_id),
    CONSTRAINT provider_dispatch_causes_exact_result_key
        UNIQUE (
            workspace_id, external_effect_id, run_id, step_id,
            model_request_evidence_id, model_request_evidence_check_id
        )
);

CREATE TRIGGER provider_dispatch_causes_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON provider_dispatch_causes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
ALTER TABLE provider_dispatch_causes ENABLE ROW LEVEL SECURITY;
ALTER TABLE provider_dispatch_causes FORCE ROW LEVEL SECURITY;
CREATE POLICY provider_dispatch_causes_workspace_policy
    ON provider_dispatch_causes
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

ALTER TABLE artifact_revisions
    ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'legacy_digest';
ALTER TABLE artifact_revisions
    ADD CONSTRAINT artifact_revisions_storage_kind_check
    CHECK (storage_kind IN ('legacy_digest', 'governed_material'));
ALTER TABLE artifact_revisions ALTER COLUMN content_hash DROP NOT NULL;
ALTER TABLE artifact_revisions ALTER COLUMN byte_size DROP NOT NULL;
ALTER TABLE artifact_revisions
    ADD CONSTRAINT artifact_revisions_storage_shape_check CHECK (
        (storage_kind = 'legacy_digest' AND content_hash IS NOT NULL AND byte_size IS NOT NULL)
        OR
        (storage_kind = 'governed_material' AND content_hash IS NULL AND byte_size IS NULL)
    );

ALTER TABLE provider_admission_waits
    DROP CONSTRAINT provider_admission_waits_terminal_reason_check;
ALTER TABLE provider_admission_waits
    ADD CONSTRAINT provider_admission_waits_terminal_reason_check
    CHECK (terminal_reason IN ('admitted', 'timeout', 'cancelled', 'throttled'));
ALTER TABLE provider_admission_waits
    ADD COLUMN requested_admission_id UUID,
    ADD COLUMN requested_concurrency_lease_id UUID,
    ADD COLUMN requested_dispatch_ttl_seconds INTEGER;
ALTER TABLE provider_admission_waits
    ALTER COLUMN requested_admission_id SET NOT NULL,
    ALTER COLUMN requested_concurrency_lease_id SET NOT NULL,
    ALTER COLUMN requested_dispatch_ttl_seconds SET NOT NULL;
ALTER TABLE provider_admission_waits
    ADD CONSTRAINT provider_admission_waits_requested_ttl_check
    CHECK (requested_dispatch_ttl_seconds BETWEEN 1 AND 900);

ALTER TABLE connection_dispatch_admissions
    ADD COLUMN requested_wait_id UUID,
    ADD COLUMN requested_concurrency_lease_id UUID,
    ADD COLUMN requested_dispatch_ttl_seconds INTEGER,
    ADD COLUMN retry_after_seconds INTEGER,
    ADD COLUMN dispatch_expires_at TIMESTAMPTZ;
ALTER TABLE connection_dispatch_admissions
    ALTER COLUMN requested_wait_id SET NOT NULL,
    ALTER COLUMN requested_concurrency_lease_id SET NOT NULL,
    ALTER COLUMN requested_dispatch_ttl_seconds SET NOT NULL;
ALTER TABLE connection_dispatch_admissions
    ADD CONSTRAINT connection_dispatch_admissions_requested_ttl_check
    CHECK (requested_dispatch_ttl_seconds BETWEEN 1 AND 900);
ALTER TABLE connection_dispatch_admissions
    ADD CONSTRAINT connection_dispatch_admissions_result_shape CHECK (
        (decision = 'admitted'
         AND retry_after_seconds IS NULL
         AND dispatch_expires_at IS NOT NULL)
        OR
        (decision = 'throttled'
         AND retry_after_seconds BETWEEN 1 AND 900
         AND dispatch_expires_at IS NULL)
        OR
        (decision IN ('conflict', 'denied')
         AND retry_after_seconds IS NULL
         AND dispatch_expires_at IS NULL)
    );

ALTER TABLE provider_concurrency_leases
    ADD COLUMN released_receipt_id UUID;
ALTER TABLE provider_concurrency_leases
    ADD CONSTRAINT provider_concurrency_leases_release_receipt_pair CHECK (
        (released_at IS NULL AND released_receipt_id IS NULL)
        OR (released_at IS NOT NULL AND (
            released_receipt_id IS NOT NULL OR expires_at <= released_at
        ))
    );
ALTER TABLE provider_concurrency_leases
    ADD CONSTRAINT provider_concurrency_leases_release_receipt_fkey
    FOREIGN KEY (released_receipt_id, workspace_id)
    REFERENCES external_effect_receipts(id, workspace_id) ON DELETE RESTRICT;

ALTER TABLE provider_throttle_observations
    ADD COLUMN external_effect_receipt_id UUID,
    ADD COLUMN policy_revision_id UUID,
    ADD COLUMN requested_retry_after_seconds INTEGER;
ALTER TABLE provider_throttle_observations
    ALTER COLUMN external_effect_receipt_id SET NOT NULL,
    ALTER COLUMN policy_revision_id SET NOT NULL,
    ALTER COLUMN requested_retry_after_seconds SET NOT NULL;
ALTER TABLE provider_throttle_observations
    ADD CONSTRAINT provider_throttle_observations_receipt_fkey
    FOREIGN KEY (external_effect_receipt_id, workspace_id)
    REFERENCES external_effect_receipts(id, workspace_id) ON DELETE RESTRICT;
ALTER TABLE provider_throttle_observations
    ADD CONSTRAINT provider_throttle_observations_exact_policy_fkey
    FOREIGN KEY (workspace_id, connection_id, policy_revision_id)
    REFERENCES connection_admission_policy_revisions(
        workspace_id, connection_id, id
    ) ON DELETE RESTRICT;
ALTER TABLE provider_throttle_observations
    ADD CONSTRAINT provider_throttle_observations_requested_retry_check
    CHECK (requested_retry_after_seconds BETWEEN 1 AND 900);

ALTER TABLE provider_result_preparations
    ADD COLUMN model_request_evidence_id UUID,
    ADD COLUMN model_request_evidence_check_id UUID,
    ADD COLUMN external_effect_receipt_id UUID,
    ADD COLUMN receipt_witnessed_at TIMESTAMPTZ,
    ADD COLUMN advance_work_item_id UUID,
    ADD COLUMN finish_reason TEXT,
    ADD COLUMN usage_known BOOLEAN,
    ADD COLUMN prompt_tokens INTEGER,
    ADD COLUMN completion_tokens INTEGER;
ALTER TABLE model_executions
    ADD COLUMN usage_known BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE provider_result_preparations
    ALTER COLUMN model_request_evidence_id SET NOT NULL,
    ALTER COLUMN model_request_evidence_check_id SET NOT NULL;
ALTER TABLE provider_result_preparations
    ADD CONSTRAINT provider_result_preparations_exact_evidence_check_fkey
    FOREIGN KEY (
        workspace_id, model_request_evidence_id, model_request_evidence_check_id
    ) REFERENCES model_request_evidence_checks(
        workspace_id, evidence_root_id, id
    ) ON DELETE RESTRICT;
ALTER TABLE provider_result_preparations
    ADD CONSTRAINT provider_result_preparations_receipt_fkey
    FOREIGN KEY (external_effect_receipt_id, workspace_id)
    REFERENCES external_effect_receipts(id, workspace_id) ON DELETE RESTRICT;
ALTER TABLE provider_result_preparations
    ADD CONSTRAINT provider_result_preparations_receipt_pair CHECK (
        (external_effect_receipt_id IS NULL
         AND receipt_witnessed_at IS NULL
         AND advance_work_item_id IS NULL
         AND finish_reason IS NULL
         AND usage_known IS NULL
         AND prompt_tokens IS NULL
         AND completion_tokens IS NULL)
        OR
        (external_effect_receipt_id IS NOT NULL
         AND receipt_witnessed_at IS NOT NULL
         AND advance_work_item_id IS NOT NULL
         AND finish_reason IN ('stop', 'length', 'tool_calls', 'content_filter')
         AND (
             (usage_known = TRUE
              AND prompt_tokens >= 0 AND completion_tokens >= 0)
             OR
             (usage_known = FALSE
              AND prompt_tokens IS NULL AND completion_tokens IS NULL)
         ))
    );
ALTER TABLE provider_result_preparations
    ADD CONSTRAINT provider_result_preparations_advance_work_item_key
    UNIQUE (advance_work_item_id);
ALTER TABLE provider_result_preparations
    ADD CONSTRAINT provider_result_preparations_exact_dispatch_cause_fkey
    FOREIGN KEY (
        workspace_id, external_effect_id, run_id, step_id,
        model_request_evidence_id, model_request_evidence_check_id
    ) REFERENCES provider_dispatch_causes(
        workspace_id, external_effect_id, run_id, step_id,
        model_request_evidence_id, model_request_evidence_check_id
    ) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE provider_result_preparations
    DROP CONSTRAINT provider_result_preparations_fixed_output_key;
ALTER TABLE provider_result_preparations
    ADD CONSTRAINT provider_result_preparations_fixed_output_key UNIQUE (
        external_effect_id, run_id, step_id, model_request_evidence_id,
        artifact_id, artifact_revision_id, model_execution_id
    );
GRANT SELECT ON TABLE external_effect_receipts TO vestrace_guarded_owner;

SELECT vestrace_assign_p03_table_owner('provider_dispatch_causes'::REGCLASS);

CREATE OR REPLACE FUNCTION vestrace_try_admit_provider_dispatch(
    target_admission_id UUID,
    target_wait_id UUID,
    target_concurrency_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_effect_id UUID,
    target_model_request_evidence_id UUID,
    target_cause_kind TEXT,
    target_run_id UUID,
    target_step_id UUID,
    target_snapshot_id UUID,
    target_qualification_job_id UUID,
    target_qualification_target_id UUID,
    target_qualification_probe_ordinal TEXT,
    target_dispatch_ttl_seconds INTEGER
)
RETURNS TABLE(
    decision TEXT,
    retry_after_seconds INTEGER,
    concurrency_lease_id UUID,
    wait_deadline_at TIMESTAMPTZ,
    dispatch_expires_at TIMESTAMPTZ
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_now TIMESTAMPTZ := NOW();
    target_dispatch_expiry TIMESTAMPTZ;
    evidence_root model_request_evidence_roots%ROWTYPE;
    latest_check_id UUID;
    policy_row connection_admission_policy_revisions%ROWTYPE;
    state_row connection_admission_states%ROWTYPE;
    existing_cause provider_dispatch_causes%ROWTYPE;
    existing_admission connection_dispatch_admissions%ROWTYPE;
    existing_wait provider_admission_waits%ROWTYPE;
    existing_lease provider_concurrency_leases%ROWTYPE;
    active_throttle provider_throttle_observations%ROWTYPE;
    target_slot SMALLINT;
    target_retry INTEGER;
    evidence_node_count BIGINT;
    governed_input_count BIGINT;
    governed_framed_bytes BIGINT;
BEGIN
    IF target_admission_id IS NULL OR target_wait_id IS NULL
       OR target_concurrency_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_effect_id IS NULL OR target_model_request_evidence_id IS NULL
       OR target_dispatch_ttl_seconds IS NULL
       OR target_dispatch_ttl_seconds NOT BETWEEN 1 AND 900
       OR target_cause_kind IS NULL
       OR target_cause_kind NOT IN ('run_step', 'qualification_probe') THEN
        RAISE EXCEPTION 'provider dispatch admission arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF NOT (
        (target_cause_kind = 'run_step'
         AND target_run_id IS NOT NULL
         AND target_step_id IS NOT NULL
         AND target_snapshot_id IS NOT NULL
         AND target_qualification_job_id IS NULL
         AND target_qualification_target_id IS NULL
         AND target_qualification_probe_ordinal IS NULL)
        OR
        (target_cause_kind = 'qualification_probe'
         AND target_run_id IS NULL
         AND target_step_id IS NULL
         AND target_snapshot_id IS NULL
         AND target_qualification_job_id IS NOT NULL
         AND target_qualification_target_id IS NOT NULL
         AND target_qualification_probe_ordinal IN (
             '00','10','15','20','30','35','40','50','60','70','80','90'
         ))
    ) THEN
        RAISE EXCEPTION 'provider dispatch cause arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider dispatch workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    target_dispatch_expiry := target_now + make_interval(secs => target_dispatch_ttl_seconds);

    IF target_cause_kind = 'run_step' AND NOT EXISTS (
        SELECT 1 FROM run_model_binding_snapshots AS run_snapshot
         WHERE run_snapshot.workspace_id = target_workspace_id
           AND run_snapshot.run_id = target_run_id
           AND run_snapshot.snapshot_id = target_snapshot_id
    ) THEN
        RAISE EXCEPTION 'provider dispatch run and snapshot mapping is not exact'
            USING ERRCODE = '23514';
    END IF;

    -- Permanent ConnectionExecutionGuard is the outer admission serializer.
    PERFORM guard.id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM connection_revisions AS revision
         WHERE revision.workspace_id = target_workspace_id
           AND revision.connection_id = target_connection_id
           AND revision.id = target_connection_revision_id
    ) THEN
        RAISE EXCEPTION 'provider dispatch requires its permanent connection guard and revision'
            USING ERRCODE = '23514';
    END IF;

    -- Revalidate the named evidence while every lower source is held in the
    -- same canonical order used by Task 9 reconstruction.
    SELECT * INTO evidence_root
      FROM model_request_evidence_roots AS root
     WHERE root.id = target_model_request_evidence_id
       AND root.workspace_id = target_workspace_id
     FOR UPDATE OF root;
    IF NOT FOUND OR evidence_root.external_effect_id IS DISTINCT FROM target_effect_id
       OR evidence_root.cause_kind IS DISTINCT FROM target_cause_kind
       OR (
           target_cause_kind = 'run_step'
           AND (
               evidence_root.cause_id IS DISTINCT FROM target_step_id
               OR evidence_root.binding_snapshot_id IS DISTINCT FROM target_snapshot_id
               OR evidence_root.qualification_target_binding_id IS NOT NULL
               OR NOT EXISTS (
                   SELECT 1 FROM run_steps
                    WHERE workspace_id = target_workspace_id
                      AND run_id = target_run_id AND id = target_step_id
               )
               OR NOT EXISTS (
                   SELECT 1 FROM model_request_evidence_nodes
                    WHERE workspace_id = target_workspace_id
                      AND evidence_root_id = target_model_request_evidence_id
                      AND reference_kind = 'binding_snapshot'
                      AND reference_id = target_snapshot_id
               )
           )
       )
       OR (
           target_cause_kind = 'qualification_probe'
           AND (
               evidence_root.cause_id IS DISTINCT FROM target_qualification_job_id
               OR evidence_root.binding_snapshot_id IS NOT NULL
               OR evidence_root.qualification_target_binding_id
                    IS DISTINCT FROM target_qualification_target_id
               OR NOT EXISTS (
                   SELECT 1 FROM qualification_target_bindings
                    WHERE workspace_id = target_workspace_id
                      AND qualification_job_id = target_qualification_job_id
                      AND id = target_qualification_target_id
                      AND connection_id = target_connection_id
                      AND connection_revision_id = target_connection_revision_id
               )
               OR NOT EXISTS (
                   SELECT 1 FROM model_request_evidence_nodes
                    WHERE workspace_id = target_workspace_id
                      AND evidence_root_id = target_model_request_evidence_id
                      AND reference_kind = 'qualification_probe'
                      AND reference_id = target_qualification_job_id
                      AND safe_ordinal = target_qualification_probe_ordinal
               )
           )
       )
       OR NOT EXISTS (
           SELECT 1 FROM model_request_evidence_nodes
            WHERE workspace_id = target_workspace_id
              AND evidence_root_id = target_model_request_evidence_id
              AND reference_kind = 'external_effect'
              AND reference_id = target_effect_id
       )
       OR NOT EXISTS (
           SELECT 1 FROM model_request_evidence_nodes
            WHERE workspace_id = target_workspace_id
              AND evidence_root_id = target_model_request_evidence_id
              AND reference_kind = 'connection_revision'
              AND reference_id = target_connection_revision_id
       ) THEN
        RAISE EXCEPTION 'provider dispatch evidence cause tuple is not current and exact'
            USING ERRCODE = '23514';
    END IF;

    PERFORM node.id
      FROM model_request_evidence_nodes AS node
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
     ORDER BY node.ordinal
     FOR UPDATE OF node;
    SELECT COUNT(*),
           COUNT(*) FILTER (WHERE node.reference_kind = 'governed_input_material')
      INTO evidence_node_count, governed_input_count
      FROM model_request_evidence_nodes AS node
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id;
    IF evidence_node_count NOT BETWEEN 1 AND 8200
       OR governed_input_count > 4096 THEN
        RAISE EXCEPTION 'provider dispatch evidence node cardinality exceeds its bound'
            USING ERRCODE = '23514';
    END IF;

    -- Lock every retained source in a fixed table/id order, then apply the
    -- same type-dispatched existence and exact-version test as Task 9.
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN external_effect_intents AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'external_effect'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN connection_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'connection_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN connection_qualification_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'connection_qualification_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'model_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_qualification_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'model_qualification_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_binding_snapshots AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'binding_snapshot'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN qualification_target_bindings AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'qualification_target'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_request_shape_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'request_shape_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_sampling_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'sampling_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_limits_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'limits_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_tool_schema_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'tool_schema_revision'
     ORDER BY source.id
     FOR SHARE OF source;

    IF EXISTS (
        SELECT 1
          FROM model_request_evidence_nodes AS node
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_model_request_evidence_id
           AND (
               (node.reference_kind IN (
                   'request_shape_revision', 'sampling_revision',
                   'limits_revision', 'tool_schema_revision'
                ) AND (node.reference_version IS NULL OR node.reference_version < 1))
               OR (node.reference_kind NOT IN (
                   'request_shape_revision', 'sampling_revision',
                   'limits_revision', 'tool_schema_revision'
               ) AND node.reference_version IS NOT NULL)
               OR (node.reference_kind = 'external_effect' AND NOT EXISTS (
                   SELECT 1 FROM external_effect_intents AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'connection_revision' AND NOT EXISTS (
                   SELECT 1 FROM connection_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'connection_qualification_revision' AND NOT EXISTS (
                   SELECT 1 FROM connection_qualification_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'model_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'model_qualification_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_qualification_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'binding_snapshot' AND NOT EXISTS (
                   SELECT 1 FROM model_binding_snapshots AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'qualification_target' AND NOT EXISTS (
                   SELECT 1 FROM qualification_target_bindings AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'request_shape_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_request_shape_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
               OR (node.reference_kind = 'sampling_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_sampling_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
               OR (node.reference_kind = 'limits_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_limits_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
               OR (node.reference_kind = 'tool_schema_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_tool_schema_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
           )
    ) THEN
        RAISE EXCEPTION 'provider dispatch evidence retained source is absent or wrong-version'
            USING ERRCODE = '23514';
    END IF;
    PERFORM material.id
      FROM model_request_evidence_nodes AS node
      JOIN content_materials AS material
        ON material.workspace_id = node.workspace_id
       AND material.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY material.id
     FOR UPDATE OF material;
    PERFORM intent.id
      FROM model_request_evidence_nodes AS node
      JOIN content_materials AS material
        ON material.workspace_id = node.workspace_id
       AND material.id = node.reference_id
      JOIN material_key_creation_intents AS intent
        ON intent.id = material.intent_id
       AND intent.workspace_id = material.workspace_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY intent.id
     FOR UPDATE OF intent;
    PERFORM bytes.material_id
      FROM model_request_evidence_nodes AS node
      JOIN content_material_bytes AS bytes
        ON bytes.workspace_id = node.workspace_id
       AND bytes.material_id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY bytes.material_id
     FOR UPDATE OF bytes;
    IF EXISTS (
        SELECT 1
          FROM model_request_evidence_nodes AS node
          LEFT JOIN content_materials AS material
            ON material.workspace_id = node.workspace_id
           AND material.id = node.reference_id
          LEFT JOIN material_key_creation_intents AS intent
            ON intent.id = material.intent_id
           AND intent.workspace_id = material.workspace_id
          LEFT JOIN content_material_bytes AS bytes
            ON bytes.workspace_id = node.workspace_id
           AND bytes.material_id = node.reference_id
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_model_request_evidence_id
           AND node.reference_kind = 'governed_input_material'
           AND (
               material.id IS NULL OR material.state <> 'live'
               OR intent.id IS NULL OR intent.state <> 'live'
               OR bytes.material_id IS NULL
               OR octet_length(bytes.ciphertext) NOT BETWEEN 4096 AND 1048576
               OR (octet_length(bytes.ciphertext)
                   & (octet_length(bytes.ciphertext) - 1)) <> 0
               OR substring(bytes.ciphertext FROM 1 FOR 4) <> decode('564d5246','hex')
               OR substring(bytes.ciphertext FROM 5 FOR 1) <> decode('01','hex')
               OR EXISTS (
                   SELECT 1 FROM material_erasure_preparations AS erasure
                    WHERE erasure.workspace_id = target_workspace_id
                      AND erasure.content_material_id = node.reference_id
                      AND erasure.finalized_at IS NOT NULL
               )
           )
    ) THEN
        RAISE EXCEPTION 'provider dispatch evidence input is not live and reconstructible'
            USING ERRCODE = '23514';
    END IF;
    SELECT COUNT(*), COALESCE(SUM(octet_length(bytes.ciphertext)::BIGINT), 0)
      INTO governed_input_count, governed_framed_bytes
      FROM model_request_evidence_nodes AS node
      JOIN content_material_bytes AS bytes
        ON bytes.workspace_id = node.workspace_id
       AND bytes.material_id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material';
    IF governed_framed_bytes
       > governed_input_count * 4096::BIGINT + 8388608::BIGINT THEN
        RAISE EXCEPTION 'provider dispatch governed input aggregate exceeds derived framed ceiling'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO existing_cause FROM provider_dispatch_causes
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND THEN
        latest_check_id := existing_cause.model_request_evidence_check_id;
        PERFORM 1 FROM model_request_evidence_checks AS check_row
         WHERE check_row.id = latest_check_id
           AND check_row.workspace_id = target_workspace_id
           AND check_row.evidence_root_id = target_model_request_evidence_id
         FOR UPDATE OF check_row;
    ELSE
        SELECT check_row.id INTO latest_check_id
          FROM model_request_evidence_checks AS check_row
         WHERE check_row.workspace_id = target_workspace_id
           AND check_row.evidence_root_id = target_model_request_evidence_id
         ORDER BY check_row.checked_at DESC, check_row.id DESC
         LIMIT 1
         FOR UPDATE OF check_row;
    END IF;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM model_request_evidence_checks
         WHERE id = latest_check_id
           AND workspace_id = target_workspace_id
           AND evidence_root_id = target_model_request_evidence_id
           AND status = 'complete'
    ) THEN
        RAISE EXCEPTION 'provider dispatch requires the latest complete evidence check'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO provider_dispatch_causes (
        external_effect_id, workspace_id, model_request_evidence_id,
        model_request_evidence_check_id, cause_kind, run_id, step_id,
        model_binding_snapshot_id, qualification_job_id,
        qualification_target_binding_id, qualification_probe_ordinal
    ) VALUES (
        target_effect_id, target_workspace_id, target_model_request_evidence_id,
        latest_check_id, target_cause_kind, target_run_id, target_step_id,
        target_snapshot_id, target_qualification_job_id,
        target_qualification_target_id, target_qualification_probe_ordinal
    ) ON CONFLICT (external_effect_id) DO NOTHING;
    SELECT * INTO existing_cause FROM provider_dispatch_causes
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF existing_cause.workspace_id IS DISTINCT FROM target_workspace_id
       OR existing_cause.model_request_evidence_id IS DISTINCT FROM target_model_request_evidence_id
       OR existing_cause.model_request_evidence_check_id IS DISTINCT FROM latest_check_id
       OR existing_cause.cause_kind IS DISTINCT FROM target_cause_kind
       OR existing_cause.run_id IS DISTINCT FROM target_run_id
       OR existing_cause.step_id IS DISTINCT FROM target_step_id
       OR existing_cause.model_binding_snapshot_id IS DISTINCT FROM target_snapshot_id
       OR existing_cause.qualification_job_id IS DISTINCT FROM target_qualification_job_id
       OR existing_cause.qualification_target_binding_id IS DISTINCT FROM target_qualification_target_id
       OR existing_cause.qualification_probe_ordinal IS DISTINCT FROM target_qualification_probe_ordinal THEN
        RAISE EXCEPTION 'provider dispatch cause replay tuple mismatch'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO existing_admission FROM connection_dispatch_admissions
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND THEN
        IF existing_admission.id IS DISTINCT FROM target_admission_id
           OR existing_admission.workspace_id IS DISTINCT FROM target_workspace_id
           OR existing_admission.connection_id IS DISTINCT FROM target_connection_id
           OR existing_admission.connection_revision_id IS DISTINCT FROM target_connection_revision_id
           OR existing_admission.requested_wait_id IS DISTINCT FROM target_wait_id
           OR existing_admission.requested_concurrency_lease_id
                IS DISTINCT FROM target_concurrency_lease_id
           OR existing_admission.requested_dispatch_ttl_seconds
                IS DISTINCT FROM target_dispatch_ttl_seconds
           OR existing_admission.model_binding_snapshot_id IS DISTINCT FROM target_snapshot_id
           OR existing_admission.qualification_target_binding_id
                IS DISTINCT FROM target_qualification_target_id THEN
            RAISE EXCEPTION 'provider dispatch admission replay tuple mismatch'
                USING ERRCODE = '23514';
        END IF;
        IF existing_admission.decision = 'admitted' THEN
            SELECT * INTO existing_lease FROM provider_concurrency_leases
             WHERE id = target_concurrency_lease_id
               AND external_effect_id = target_effect_id;
            IF NOT FOUND OR existing_lease.expires_at
                IS DISTINCT FROM existing_admission.dispatch_expires_at THEN
                RAISE EXCEPTION 'provider dispatch admitted replay lacks its exact lease'
                    USING ERRCODE = '23514';
            END IF;
            decision := 'admitted';
            retry_after_seconds := NULL;
            concurrency_lease_id := existing_lease.id;
            wait_deadline_at := NULL;
            dispatch_expires_at := existing_lease.expires_at;
        ELSIF existing_admission.decision = 'throttled' THEN
            decision := 'throttled';
            retry_after_seconds := existing_admission.retry_after_seconds;
            concurrency_lease_id := NULL;
            SELECT deadline_at INTO wait_deadline_at FROM provider_admission_waits
             WHERE external_effect_id = target_effect_id;
            dispatch_expires_at := NULL;
        ELSE
            decision := existing_admission.decision;
            retry_after_seconds := NULL;
            concurrency_lease_id := NULL;
            SELECT deadline_at INTO wait_deadline_at FROM provider_admission_waits
             WHERE external_effect_id = target_effect_id;
            dispatch_expires_at := NULL;
        END IF;
        RETURN NEXT;
        RETURN;
    END IF;

    SELECT policy.* INTO policy_row
      FROM connection_admission_policy_heads AS head
      JOIN connection_admission_policy_revisions AS policy
        ON policy.workspace_id = head.workspace_id
       AND policy.connection_id = head.connection_id
       AND policy.id = head.current_policy_revision_id
     WHERE head.workspace_id = target_workspace_id
       AND head.connection_id = target_connection_id
     FOR UPDATE OF head, policy;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch admission policy is absent'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO connection_admission_states (
        workspace_id, connection_id, window_started_at,
        admitted_in_window, active_lease_count
    ) VALUES (target_workspace_id, target_connection_id, target_now, 0, 0)
    ON CONFLICT (workspace_id, connection_id) DO NOTHING;
    SELECT * INTO state_row FROM connection_admission_states
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id FOR UPDATE;

    UPDATE provider_concurrency_leases
       SET released_at = target_now,
           released_receipt_id = NULL
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND released_at IS NULL
       AND expires_at <= target_now;
    IF target_now >= state_row.window_started_at + INTERVAL '60 seconds' THEN
        UPDATE connection_admission_states
           SET window_started_at = target_now, admitted_in_window = 0,
               active_lease_count = 0, version = version + 1, updated_at = target_now
         WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id;
        state_row.window_started_at := target_now;
        state_row.admitted_in_window := 0;
    END IF;
    SELECT COUNT(*)::INTEGER INTO state_row.active_lease_count
      FROM provider_concurrency_leases
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND released_at IS NULL AND expires_at > target_now;
    UPDATE connection_admission_states
       SET active_lease_count = state_row.active_lease_count,
           updated_at = target_now
     WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id;

    SELECT * INTO active_throttle FROM provider_throttle_observations
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND effective_until > target_now
     ORDER BY effective_until DESC, id DESC LIMIT 1 FOR UPDATE;
    SELECT * INTO existing_wait FROM provider_admission_waits
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND AND (
        existing_wait.id IS DISTINCT FROM target_wait_id
        OR existing_wait.workspace_id IS DISTINCT FROM target_workspace_id
        OR existing_wait.connection_id IS DISTINCT FROM target_connection_id
        OR existing_wait.requested_admission_id IS DISTINCT FROM target_admission_id
        OR existing_wait.requested_concurrency_lease_id IS DISTINCT FROM target_concurrency_lease_id
        OR existing_wait.requested_dispatch_ttl_seconds IS DISTINCT FROM target_dispatch_ttl_seconds
    ) THEN
        RAISE EXCEPTION 'provider dispatch wait replay tuple mismatch'
            USING ERRCODE = '23514';
    END IF;

    IF active_throttle.id IS NOT NULL THEN
        target_retry := LEAST(
            policy_row.provider_throttle_cap_seconds,
            GREATEST(1, CEIL(EXTRACT(EPOCH FROM active_throttle.effective_until - target_now))::INTEGER)
        );
        IF existing_wait.id IS NOT NULL AND existing_wait.terminal_reason IS NULL THEN
            UPDATE provider_admission_waits
               SET terminal_reason = 'throttled', terminal_at = target_now
             WHERE id = existing_wait.id;
        END IF;
        INSERT INTO connection_dispatch_admissions (
            id, workspace_id, connection_id, connection_revision_id,
            policy_revision_id, external_effect_id, model_binding_snapshot_id,
            qualification_target_binding_id, decision, admitted_at,
            requested_wait_id, requested_concurrency_lease_id,
            requested_dispatch_ttl_seconds, retry_after_seconds, dispatch_expires_at
        ) VALUES (
            target_admission_id, target_workspace_id, target_connection_id,
            target_connection_revision_id, policy_row.id, target_effect_id,
            target_snapshot_id, target_qualification_target_id, 'throttled', target_now,
            target_wait_id, target_concurrency_lease_id, target_dispatch_ttl_seconds,
            target_retry, NULL
        );
        decision := 'throttled';
        retry_after_seconds := target_retry;
        concurrency_lease_id := NULL;
        wait_deadline_at := existing_wait.deadline_at;
        dispatch_expires_at := NULL;
        RETURN NEXT;
        RETURN;
    END IF;

    IF existing_wait.id IS NOT NULL
       AND existing_wait.terminal_reason IS NULL
       AND existing_wait.deadline_at <= target_now THEN
        UPDATE provider_admission_waits
           SET terminal_reason = 'timeout', terminal_at = target_now
         WHERE id = existing_wait.id;
        INSERT INTO connection_dispatch_admissions (
            id, workspace_id, connection_id, connection_revision_id,
            policy_revision_id, external_effect_id, model_binding_snapshot_id,
            qualification_target_binding_id, decision, admitted_at,
            requested_wait_id, requested_concurrency_lease_id,
            requested_dispatch_ttl_seconds, retry_after_seconds, dispatch_expires_at
        ) VALUES (
            target_admission_id, target_workspace_id, target_connection_id,
            target_connection_revision_id, policy_row.id, target_effect_id,
            target_snapshot_id, target_qualification_target_id, 'conflict', target_now,
            target_wait_id, target_concurrency_lease_id, target_dispatch_ttl_seconds,
            NULL, NULL
        );
        decision := 'conflict';
        retry_after_seconds := NULL;
        concurrency_lease_id := NULL;
        wait_deadline_at := existing_wait.deadline_at;
        dispatch_expires_at := NULL;
        RETURN NEXT;
        RETURN;
    END IF;

    IF state_row.active_lease_count >= policy_row.max_in_flight
       OR state_row.admitted_in_window >= policy_row.requests_per_60_seconds THEN
        IF existing_wait.id IS NULL THEN
            INSERT INTO provider_admission_waits (
                id, workspace_id, connection_id, external_effect_id,
                requested_at, deadline_at, requested_admission_id,
                requested_concurrency_lease_id, requested_dispatch_ttl_seconds
            ) VALUES (
                target_wait_id, target_workspace_id, target_connection_id,
                target_effect_id, target_now,
                target_now + make_interval(secs => policy_row.queue_wait_timeout_seconds),
                target_admission_id, target_concurrency_lease_id,
                target_dispatch_ttl_seconds
            ) RETURNING * INTO existing_wait;
        ELSIF existing_wait.terminal_reason IS NOT NULL THEN
            RAISE EXCEPTION 'provider dispatch wait is already terminal'
                USING ERRCODE = '23514';
        END IF;
        decision := 'conflict';
        retry_after_seconds := NULL;
        concurrency_lease_id := NULL;
        wait_deadline_at := existing_wait.deadline_at;
        dispatch_expires_at := NULL;
        RETURN NEXT;
        RETURN;
    END IF;

    SELECT slot::SMALLINT INTO target_slot
      FROM generate_series(0, policy_row.max_in_flight - 1) AS slot
     WHERE NOT EXISTS (
         SELECT 1 FROM provider_concurrency_leases AS lease
          WHERE lease.workspace_id = target_workspace_id
            AND lease.connection_id = target_connection_id
            AND lease.slot_ordinal = slot
            AND lease.released_at IS NULL
     )
     ORDER BY slot LIMIT 1;
    IF target_slot IS NULL THEN
        RAISE EXCEPTION 'provider dispatch slot accounting is inconsistent'
            USING ERRCODE = '23514';
    END IF;
    IF existing_wait.id IS NOT NULL THEN
        IF existing_wait.terminal_reason IS NOT NULL THEN
            RAISE EXCEPTION 'provider dispatch wait is already terminal'
                USING ERRCODE = '23514';
        END IF;
        UPDATE provider_admission_waits
           SET terminal_reason = 'admitted', terminal_at = target_now
         WHERE id = existing_wait.id;
    END IF;
    INSERT INTO connection_dispatch_admissions (
        id, workspace_id, connection_id, connection_revision_id,
        policy_revision_id, external_effect_id, model_binding_snapshot_id,
        qualification_target_binding_id, decision, admitted_at,
        requested_wait_id, requested_concurrency_lease_id,
        requested_dispatch_ttl_seconds, retry_after_seconds, dispatch_expires_at
    ) VALUES (
        target_admission_id, target_workspace_id, target_connection_id,
        target_connection_revision_id, policy_row.id, target_effect_id,
        target_snapshot_id, target_qualification_target_id, 'admitted', target_now,
        target_wait_id, target_concurrency_lease_id, target_dispatch_ttl_seconds,
        NULL, target_dispatch_expiry
    );
    INSERT INTO provider_concurrency_leases (
        id, workspace_id, connection_id, external_effect_id, slot_ordinal,
        issued_at, expires_at
    ) VALUES (
        target_concurrency_lease_id, target_workspace_id, target_connection_id,
        target_effect_id, target_slot, target_now, target_dispatch_expiry
    );
    UPDATE connection_admission_states
       SET admitted_in_window = admitted_in_window + 1,
           active_lease_count = active_lease_count + 1,
           version = version + 1, updated_at = target_now
     WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id;
    decision := 'admitted';
    retry_after_seconds := NULL;
    concurrency_lease_id := target_concurrency_lease_id;
    wait_deadline_at := NULL;
    dispatch_expires_at := target_dispatch_expiry;
    RETURN NEXT;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_release_provider_dispatch(
    target_workspace_id UUID,
    target_effect_id UUID,
    target_receipt_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_now TIMESTAMPTZ := NOW();
    target_connection_id UUID;
    lease_row provider_concurrency_leases%ROWTYPE;
BEGIN
    IF target_workspace_id IS NULL OR target_effect_id IS NULL OR target_receipt_id IS NULL THEN
        RAISE EXCEPTION 'provider dispatch release arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider dispatch release workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    SELECT connection_id INTO target_connection_id
      FROM provider_concurrency_leases
     WHERE workspace_id = target_workspace_id
       AND external_effect_id = target_effect_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch release lease is absent'
            USING ERRCODE = '23514';
    END IF;
    PERFORM guard.id FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch release guard is absent'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO lease_row FROM provider_concurrency_leases
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND external_effect_id = target_effect_id
     FOR UPDATE;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1
          FROM external_effect_receipts AS receipt
          JOIN external_effect_lifecycle_transitions AS transition
            ON transition.effect_id = receipt.effect_id
           AND transition.workspace_id = receipt.workspace_id
           AND transition.status = receipt.outcome_status
           AND transition.cause = 'receipt_recorded'
           AND transition.cause_ref = receipt.id::TEXT
         WHERE receipt.id = target_receipt_id
           AND receipt.workspace_id = target_workspace_id
           AND receipt.effect_id = target_effect_id
           AND receipt.outcome_status IN ('acknowledged', 'failed', 'unknown')
    ) THEN
        RAISE EXCEPTION 'provider dispatch release requires the exact effect receipt pair'
            USING ERRCODE = '23514';
    END IF;
    IF lease_row.released_receipt_id IS NOT NULL THEN
        IF lease_row.released_receipt_id IS DISTINCT FROM target_receipt_id THEN
            RAISE EXCEPTION 'provider dispatch release receipt replay mismatch'
                USING ERRCODE = '23514';
        END IF;
        RETURN lease_row.id;
    END IF;
    UPDATE provider_concurrency_leases
       SET released_at = target_now, released_receipt_id = target_receipt_id
     WHERE id = lease_row.id;
    UPDATE connection_admission_states
       SET active_lease_count = (
               SELECT COUNT(*) FROM provider_concurrency_leases
                WHERE workspace_id = target_workspace_id
                  AND connection_id = target_connection_id
                  AND released_at IS NULL AND expires_at > target_now
           ),
           version = version + 1,
           updated_at = target_now
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id;
    RETURN lease_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_provider_throttle(
    target_observation_id UUID,
    target_workspace_id UUID,
    target_effect_id UUID,
    target_receipt_id UUID,
    target_retry_after_seconds INTEGER
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_now TIMESTAMPTZ := NOW();
    admission_row connection_dispatch_admissions%ROWTYPE;
    target_policy_cap INTEGER;
    effective_retry INTEGER;
    existing_observation provider_throttle_observations%ROWTYPE;
BEGIN
    IF target_observation_id IS NULL OR target_workspace_id IS NULL
       OR target_effect_id IS NULL OR target_receipt_id IS NULL
       OR target_retry_after_seconds IS NULL
       OR target_retry_after_seconds NOT BETWEEN 1 AND 900 THEN
        RAISE EXCEPTION 'provider throttle arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider throttle workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    SELECT * INTO admission_row
      FROM connection_dispatch_admissions
     WHERE workspace_id = target_workspace_id
       AND external_effect_id = target_effect_id
       AND decision = 'admitted';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider throttle requires an admitted effect'
            USING ERRCODE = '23514';
    END IF;
    PERFORM guard.id FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = admission_row.connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider throttle guard is absent'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO admission_row
      FROM connection_dispatch_admissions
     WHERE workspace_id = target_workspace_id
       AND external_effect_id = target_effect_id
       AND decision = 'admitted'
     FOR UPDATE;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1
          FROM external_effect_receipts AS receipt
          JOIN external_effect_lifecycle_transitions AS transition
            ON transition.effect_id = receipt.effect_id
           AND transition.workspace_id = receipt.workspace_id
           AND transition.status = receipt.outcome_status
           AND transition.cause = 'receipt_recorded'
           AND transition.cause_ref = receipt.id::TEXT
         WHERE receipt.id = target_receipt_id
           AND receipt.workspace_id = target_workspace_id
           AND receipt.effect_id = target_effect_id
           AND receipt.outcome_status IN ('acknowledged', 'failed')
           AND receipt.payload->>'response_class' = 'http_429'
    ) THEN
        RAISE EXCEPTION 'provider throttle requires the exact http_429 effect receipt pair'
            USING ERRCODE = '23514';
    END IF;
    SELECT policy.provider_throttle_cap_seconds INTO target_policy_cap
      FROM connection_admission_policy_revisions AS policy
     WHERE policy.workspace_id = target_workspace_id
       AND policy.connection_id = admission_row.connection_id
       AND policy.id = admission_row.policy_revision_id
     FOR UPDATE OF policy;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider throttle policy is absent'
            USING ERRCODE = '23514';
    END IF;
    effective_retry := LEAST(target_retry_after_seconds, target_policy_cap);
    SELECT * INTO existing_observation FROM provider_throttle_observations
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND THEN
        IF existing_observation.id IS DISTINCT FROM target_observation_id
           OR existing_observation.workspace_id IS DISTINCT FROM target_workspace_id
           OR existing_observation.connection_id IS DISTINCT FROM admission_row.connection_id
           OR existing_observation.external_effect_receipt_id IS DISTINCT FROM target_receipt_id
           OR existing_observation.policy_revision_id IS DISTINCT FROM admission_row.policy_revision_id
           OR existing_observation.requested_retry_after_seconds
                IS DISTINCT FROM target_retry_after_seconds
           OR existing_observation.retry_after_seconds IS DISTINCT FROM effective_retry THEN
            RAISE EXCEPTION 'provider throttle replay tuple mismatch'
                USING ERRCODE = '23514';
        END IF;
        RETURN existing_observation.id;
    END IF;
    IF EXISTS (SELECT 1 FROM provider_throttle_observations WHERE id = target_observation_id) THEN
        RAISE EXCEPTION 'provider throttle identity is already used'
            USING ERRCODE = '23514';
    END IF;
    INSERT INTO provider_throttle_observations (
        id, workspace_id, connection_id, external_effect_id,
        external_effect_receipt_id, policy_revision_id,
        requested_retry_after_seconds, retry_after_seconds,
        observed_at, effective_until
    ) VALUES (
        target_observation_id, target_workspace_id, admission_row.connection_id,
        target_effect_id, target_receipt_id, admission_row.policy_revision_id,
        target_retry_after_seconds, effective_retry, target_now,
        target_now + make_interval(secs => effective_retry)
    );
    RETURN target_observation_id;
END
$$;

-- Guarded provider-result entrypoints may not receive direct Run DML. This
-- deliberately non-runtime-executable bridge is owned by the existing Run
-- table owner and exposes only row locks plus the safe state/version tuple.
CREATE OR REPLACE FUNCTION vestrace_lock_provider_result_run_step(
    target_workspace_id UUID,
    target_run_id UUID,
    target_step_id UUID
)
RETURNS TABLE(run_version BIGINT, run_status TEXT, step_status TEXT)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    SELECT run.run_version, run.status
      INTO run_version, run_status
      FROM agent_runs AS run
     WHERE run.workspace_id = target_workspace_id AND run.id = target_run_id
     FOR UPDATE OF run;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result Run is absent' USING ERRCODE = '23514';
    END IF;
    SELECT step.status INTO step_status
      FROM run_steps AS step
     WHERE step.workspace_id = target_workspace_id
       AND step.run_id = target_run_id AND step.id = target_step_id
     FOR UPDATE OF step;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result step is absent from its Run'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEXT;
END
$$;
ALTER FUNCTION vestrace_lock_provider_result_run_step(UUID, UUID, UUID)
    OWNER TO vestrace;
REVOKE ALL ON FUNCTION vestrace_lock_provider_result_run_step(UUID, UUID, UUID)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_lock_provider_result_run_step(UUID, UUID, UUID)
    FROM vestrace;
GRANT EXECUTE ON FUNCTION vestrace_lock_provider_result_run_step(UUID, UUID, UUID)
    TO vestrace_guarded_owner;

DROP FUNCTION vestrace_prepare_provider_result(
    UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT
);

CREATE OR REPLACE FUNCTION vestrace_prepare_provider_result(
    target_effect_id UUID,
    target_intent_id UUID,
    target_attachment_id UUID,
    target_artifact_id UUID,
    target_artifact_revision_id UUID,
    target_model_execution_id UUID,
    target_ciphertext BYTEA,
    target_size_class BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    cause_row provider_dispatch_causes%ROWTYPE;
    run_step_lock RECORD;
    intent_row material_key_creation_intents%ROWTYPE;
    existing_preparation provider_result_preparations%ROWTYPE;
    target_preparation_id UUID;
BEGIN
    IF target_effect_id IS NULL OR target_intent_id IS NULL
       OR target_attachment_id IS NULL OR target_artifact_id IS NULL
       OR target_artifact_revision_id IS NULL OR target_model_execution_id IS NULL
       OR target_ciphertext IS NULL OR target_size_class < 4096
       OR target_size_class > 1048576
       OR (target_size_class & (target_size_class - 1)) <> 0
       OR octet_length(target_ciphertext) <> target_size_class
       OR substring(target_ciphertext FROM 1 FOR 4) <> decode('564d5246','hex')
       OR substring(target_ciphertext FROM 5 FOR 1) <> decode('01','hex') THEN
        RAISE EXCEPTION 'provider result preparation arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    SELECT * INTO cause_row FROM provider_dispatch_causes
     WHERE external_effect_id = target_effect_id;
    IF NOT FOUND OR cause_row.cause_kind <> 'run_step'
       OR cause_row.run_id IS NULL OR cause_row.step_id IS NULL THEN
        RAISE EXCEPTION 'provider result requires a normalized run-step dispatch cause'
            USING ERRCODE = '23514';
    END IF;
    IF cause_row.workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider result workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    IF NOT EXISTS (
        SELECT 1
          FROM model_request_evidence_roots AS root
          JOIN model_binding_snapshots AS snapshot
            ON snapshot.workspace_id = root.workspace_id
           AND snapshot.id = root.binding_snapshot_id
          JOIN model_revisions AS revision
            ON revision.workspace_id = snapshot.workspace_id
           AND revision.id = snapshot.model_revision_id
         WHERE root.workspace_id = cause_row.workspace_id
           AND root.id = cause_row.model_request_evidence_id
           AND root.request_kind = 'chat_completions'
           AND root.binding_snapshot_id = cause_row.model_binding_snapshot_id
           AND revision.kind = 'chat'
    ) THEN
        RAISE EXCEPTION 'provider result requires chat completions evidence and chat model'
            USING ERRCODE = '23514';
    END IF;

    -- Common retained-result lock order: Run, step, effect, preparation, intent.
    SELECT * INTO run_step_lock
      FROM vestrace_lock_provider_result_run_step(
          cause_row.workspace_id, cause_row.run_id, cause_row.step_id
      );
    IF run_step_lock.run_status <> 'running' THEN
        RAISE EXCEPTION 'provider result requires its active running Run'
            USING ERRCODE = '23514';
    END IF;
    IF run_step_lock.step_status <> 'running' THEN
        RAISE EXCEPTION 'provider result requires its active running step'
            USING ERRCODE = '23514';
    END IF;
    PERFORM 1 FROM external_effect_intents
     WHERE id = target_effect_id AND workspace_id = cause_row.workspace_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result effect is absent'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO existing_preparation FROM provider_result_preparations
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    SELECT * INTO intent_row FROM material_key_creation_intents
     WHERE id = target_intent_id AND workspace_id = cause_row.workspace_id
     FOR UPDATE;
    IF NOT FOUND OR intent_row.owner_kind <> 'provider_result'
       OR intent_row.owner_id <> target_effect_id
       OR intent_row.output_ordinal <> 0 THEN
        RAISE EXCEPTION 'provider result requires its exact output-zero material intent'
            USING ERRCODE = '23514';
    END IF;
    IF existing_preparation.id IS NOT NULL THEN
        IF existing_preparation.run_id <> cause_row.run_id
           OR existing_preparation.step_id <> cause_row.step_id
           OR existing_preparation.model_request_evidence_id
                <> cause_row.model_request_evidence_id
           OR existing_preparation.model_request_evidence_check_id
                <> cause_row.model_request_evidence_check_id
           OR existing_preparation.material_intent_id <> target_intent_id
           OR existing_preparation.prepared_attachment_id <> target_attachment_id
           OR existing_preparation.artifact_id <> target_artifact_id
           OR existing_preparation.artifact_revision_id <> target_artifact_revision_id
           OR existing_preparation.model_execution_id <> target_model_execution_id
           OR existing_preparation.size_class <> target_size_class
           OR intent_row.state NOT IN ('result_prepared', 'bound', 'live')
           OR NOT EXISTS (
               SELECT 1
                 FROM prepared_material_attachments AS attachment
                 JOIN content_material_bytes AS bytes
                   ON bytes.intent_id = attachment.intent_id
                  AND bytes.material_id = attachment.material_id
                WHERE attachment.id = target_attachment_id
                  AND attachment.intent_id = target_intent_id
                  AND attachment.marker = 'result_prepared'
                  AND bytes.ciphertext = target_ciphertext
                  AND octet_length(bytes.ciphertext) = target_size_class
           ) THEN
            RAISE EXCEPTION 'provider result replay tuple mismatch'
                USING ERRCODE = '23514';
        END IF;
        RETURN existing_preparation.id;
    END IF;
    IF intent_row.state <> 'provisional_receipted' OR intent_row.vault_receipt IS NULL THEN
        RAISE EXCEPTION 'provider result requires its exact receipted material intent'
            USING ERRCODE = '23514';
    END IF;
    IF EXISTS (
        SELECT 1 FROM prepared_material_attachments
         WHERE id = target_attachment_id
    ) THEN
        RAISE EXCEPTION 'provider result identity collision'
            USING ERRCODE = '23514';
    END IF;
    BEGIN
        PERFORM vestrace_prepare_result_material(
            target_intent_id, target_attachment_id, target_ciphertext, target_size_class
        );
    EXCEPTION WHEN unique_violation THEN
        -- This sub-block contains only the result-material preparation call.
        -- The exact intent is locked and has no material yet, so the sole
        -- caller-owned unique identity in this operation is the attachment.
        RAISE EXCEPTION 'provider result identity collision'
            USING ERRCODE = '23514';
    END;
    target_preparation_id := gen_random_uuid();
    INSERT INTO provider_result_preparations (
        id, workspace_id, external_effect_id, run_id, step_id,
        model_request_evidence_id, model_request_evidence_check_id,
        material_intent_id, prepared_attachment_id, artifact_id,
        artifact_revision_id, model_execution_id, size_class,
        expected_run_version
    ) VALUES (
        target_preparation_id, cause_row.workspace_id, target_effect_id,
        cause_row.run_id, cause_row.step_id, cause_row.model_request_evidence_id,
        cause_row.model_request_evidence_check_id, target_intent_id,
        target_attachment_id, target_artifact_id, target_artifact_revision_id,
        target_model_execution_id, target_size_class, run_step_lock.run_version
    );
    RETURN target_preparation_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_witness_provider_result_receipt(
    target_provider_result_preparation_id UUID,
    target_external_effect_receipt_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preparation_identity RECORD;
    preparation_row provider_result_preparations%ROWTYPE;
    canonical_marker TEXT;
    recovery_payload JSONB;
    recovery_advance_work_item_id UUID;
    recovery_finish_reason TEXT;
    recovery_usage_known BOOLEAN;
    recovery_prompt_tokens INTEGER;
    recovery_completion_tokens INTEGER;
    collision_constraint TEXT;
BEGIN
    IF target_provider_result_preparation_id IS NULL
       OR target_external_effect_receipt_id IS NULL THEN
        RAISE EXCEPTION 'provider result receipt witness arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id, run_id, step_id, external_effect_id, material_intent_id
      INTO preparation_identity
      FROM provider_result_preparations
     WHERE id = target_provider_result_preparation_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result preparation is absent'
            USING ERRCODE = '23514';
    END IF;
    IF preparation_identity.workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider result workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    PERFORM * FROM vestrace_lock_provider_result_run_step(
        preparation_identity.workspace_id,
        preparation_identity.run_id,
        preparation_identity.step_id
    );
    PERFORM 1 FROM external_effect_intents
     WHERE workspace_id = preparation_identity.workspace_id
       AND id = preparation_identity.external_effect_id FOR UPDATE;
    SELECT * INTO preparation_row FROM provider_result_preparations
     WHERE id = target_provider_result_preparation_id FOR UPDATE;
    PERFORM 1 FROM material_key_creation_intents
     WHERE workspace_id = preparation_identity.workspace_id
       AND id = preparation_identity.material_intent_id FOR UPDATE;
    IF preparation_row.id IS NULL THEN
        RAISE EXCEPTION 'provider result preparation disappeared'
            USING ERRCODE = '23514';
    END IF;
    canonical_marker := 'provider_result_preparation:'
        || lower(preparation_row.id::TEXT);
    SELECT receipt.payload->'provider_result_recovery'
      INTO recovery_payload
          FROM external_effect_receipts AS receipt
          JOIN external_effect_lifecycle_transitions AS transition
            ON transition.effect_id = receipt.effect_id
           AND transition.workspace_id = receipt.workspace_id
           AND transition.status = 'acknowledged'
           AND transition.cause = 'receipt_recorded'
           AND transition.cause_ref = receipt.id::TEXT
         WHERE receipt.id = target_external_effect_receipt_id
           AND receipt.workspace_id = preparation_row.workspace_id
           AND receipt.effect_id = preparation_row.external_effect_id
           AND receipt.outcome_status = 'acknowledged'
           AND jsonb_typeof(receipt.payload->'evidence_refs') = 'array'
           AND jsonb_array_length(receipt.payload->'evidence_refs') = 1
           AND receipt.payload->'evidence_refs'->>0 = canonical_marker
    ;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result receipt witness is not the exact canonical receipt'
            USING ERRCODE = '23514';
    END IF;
    IF jsonb_typeof(recovery_payload) IS DISTINCT FROM 'object'
       OR recovery_payload
            - 'advance_work_item_id'
            - 'finish_reason'
            - 'usage_known'
            - 'prompt_tokens'
            - 'completion_tokens' <> '{}'::JSONB
       OR jsonb_typeof(recovery_payload->'advance_work_item_id') IS DISTINCT FROM 'string'
       OR NOT (recovery_payload->>'advance_work_item_id' ~
            '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$')
       OR recovery_payload->>'finish_reason'
            NOT IN ('stop', 'length', 'tool_calls', 'content_filter')
       OR jsonb_typeof(recovery_payload->'usage_known') IS DISTINCT FROM 'boolean' THEN
        RAISE EXCEPTION 'provider result receipt recovery evidence is malformed'
            USING ERRCODE = '23514';
    END IF;
    recovery_advance_work_item_id :=
        (recovery_payload->>'advance_work_item_id')::UUID;
    recovery_finish_reason := recovery_payload->>'finish_reason';
    recovery_usage_known := (recovery_payload->>'usage_known')::BOOLEAN;
    IF recovery_usage_known THEN
        IF jsonb_typeof(recovery_payload->'prompt_tokens') IS DISTINCT FROM 'number'
           OR jsonb_typeof(recovery_payload->'completion_tokens') IS DISTINCT FROM 'number'
           OR NOT (recovery_payload->>'prompt_tokens' ~ '^[0-9]+$')
           OR NOT (recovery_payload->>'completion_tokens' ~ '^[0-9]+$')
           OR (recovery_payload->>'prompt_tokens')::NUMERIC > 2147483647
           OR (recovery_payload->>'completion_tokens')::NUMERIC > 2147483647 THEN
            RAISE EXCEPTION 'provider result receipt recovery evidence is malformed'
                USING ERRCODE = '23514';
        END IF;
        recovery_prompt_tokens := (recovery_payload->>'prompt_tokens')::INTEGER;
        recovery_completion_tokens :=
            (recovery_payload->>'completion_tokens')::INTEGER;
    ELSE
        IF jsonb_typeof(recovery_payload->'prompt_tokens') IS DISTINCT FROM 'null'
           OR jsonb_typeof(recovery_payload->'completion_tokens') IS DISTINCT FROM 'null' THEN
            RAISE EXCEPTION 'provider result receipt recovery evidence is malformed'
                USING ERRCODE = '23514';
        END IF;
        recovery_prompt_tokens := NULL;
        recovery_completion_tokens := NULL;
    END IF;
    IF preparation_row.external_effect_receipt_id IS NOT NULL THEN
        IF preparation_row.external_effect_receipt_id
                IS DISTINCT FROM target_external_effect_receipt_id
           OR preparation_row.advance_work_item_id
                IS DISTINCT FROM recovery_advance_work_item_id
           OR preparation_row.finish_reason IS DISTINCT FROM recovery_finish_reason
           OR preparation_row.usage_known IS DISTINCT FROM recovery_usage_known
           OR preparation_row.prompt_tokens IS DISTINCT FROM recovery_prompt_tokens
           OR preparation_row.completion_tokens
                IS DISTINCT FROM recovery_completion_tokens THEN
            RAISE EXCEPTION 'provider result receipt witness replay mismatch'
                USING ERRCODE = '23514';
        END IF;
        RETURN target_external_effect_receipt_id;
    END IF;
    IF EXISTS (
        SELECT 1 FROM provider_result_preparations AS other
         WHERE other.advance_work_item_id = recovery_advance_work_item_id
           AND other.id <> preparation_row.id
    ) THEN
        RAISE EXCEPTION 'provider result continuation identity collision'
            USING ERRCODE = '23514';
    END IF;
    BEGIN
        UPDATE provider_result_preparations
           SET external_effect_receipt_id = target_external_effect_receipt_id,
               receipt_witnessed_at = NOW(),
               advance_work_item_id = recovery_advance_work_item_id,
               finish_reason = recovery_finish_reason,
               usage_known = recovery_usage_known,
               prompt_tokens = recovery_prompt_tokens,
               completion_tokens = recovery_completion_tokens
         WHERE id = preparation_row.id;
    EXCEPTION WHEN unique_violation THEN
        GET STACKED DIAGNOSTICS collision_constraint = CONSTRAINT_NAME;
        IF collision_constraint = 'provider_result_preparations_advance_work_item_key' THEN
            RAISE EXCEPTION 'provider result continuation identity collision'
                USING ERRCODE = '23514';
        END IF;
        RAISE;
    END;
    RETURN target_external_effect_receipt_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_provider_result(
    target_provider_result_preparation_id UUID,
    target_erasure_bound_commitment BYTEA
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preparation_identity RECORD;
    preparation_row provider_result_preparations%ROWTYPE;
    intent_row material_key_creation_intents%ROWTYPE;
    material_row content_materials%ROWTYPE;
    existing_commitment BYTEA;
BEGIN
    IF target_erasure_bound_commitment IS NULL
       OR octet_length(target_erasure_bound_commitment) <> 32 THEN
        RAISE EXCEPTION 'provider result erasure-bound commitment must be exactly 32 bytes'
            USING ERRCODE = '23514';
    END IF;
    SELECT workspace_id, run_id, step_id, external_effect_id, material_intent_id
      INTO preparation_identity
      FROM provider_result_preparations
     WHERE id = target_provider_result_preparation_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result preparation is absent'
            USING ERRCODE = '23514';
    END IF;
    IF preparation_identity.workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider result workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    PERFORM * FROM vestrace_lock_provider_result_run_step(
        preparation_identity.workspace_id,
        preparation_identity.run_id,
        preparation_identity.step_id
    );
    PERFORM 1 FROM external_effect_intents
     WHERE workspace_id = preparation_identity.workspace_id
       AND id = preparation_identity.external_effect_id FOR UPDATE;
    SELECT * INTO preparation_row FROM provider_result_preparations
     WHERE id = target_provider_result_preparation_id FOR UPDATE;
    SELECT * INTO intent_row FROM material_key_creation_intents
     WHERE workspace_id = preparation_identity.workspace_id
       AND id = preparation_identity.material_intent_id FOR UPDATE;
    IF preparation_row.state = 'published' THEN
        SELECT erasure_bound_commitment INTO existing_commitment
          FROM artifact_revision_contents
         WHERE provider_result_preparation_id = preparation_row.id;
        IF NOT FOUND OR existing_commitment <> target_erasure_bound_commitment THEN
            RAISE EXCEPTION 'provider result finalization replay commitment mismatch'
                USING ERRCODE = '23514';
        END IF;
    ELSIF preparation_row.external_effect_receipt_id IS NULL
       OR preparation_row.receipt_witnessed_at IS NULL
       OR intent_row.state <> 'bound'
       OR intent_row.bound_receipt IS NULL
       OR intent_row.prepared_marker <> 'result_prepared' THEN
        RAISE EXCEPTION 'provider result finalization requires receipt and Bound witnesses'
            USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM provider_dispatch_causes AS cause
         WHERE cause.external_effect_id = preparation_row.external_effect_id
           AND cause.workspace_id = preparation_row.workspace_id
           AND cause.cause_kind = 'run_step'
           AND cause.run_id = preparation_row.run_id
           AND cause.step_id = preparation_row.step_id
           AND cause.model_request_evidence_id = preparation_row.model_request_evidence_id
           AND cause.model_request_evidence_check_id
                = preparation_row.model_request_evidence_check_id
    ) THEN
        RAISE EXCEPTION 'provider result normalized dispatch tuple changed'
            USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM artifacts
         WHERE workspace_id = preparation_row.workspace_id
           AND id = preparation_row.artifact_id
           AND name = 'provider-result-' || preparation_row.artifact_id::TEXT
    ) OR NOT EXISTS (
        SELECT 1 FROM artifact_revisions
         WHERE workspace_id = preparation_row.workspace_id
           AND artifact_id = preparation_row.artifact_id
           AND id = preparation_row.artifact_revision_id
           AND revision_number = 1
           AND media_type = 'text/plain'
           AND storage_kind = 'governed_material'
           AND content_hash IS NULL AND byte_size IS NULL
    ) THEN
        RAISE EXCEPTION 'provider result runtime envelopes are absent or not governed'
            USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1
          FROM model_executions AS execution
          JOIN provider_dispatch_causes AS cause
            ON cause.workspace_id = execution.workspace_id
           AND cause.external_effect_id = preparation_row.external_effect_id
          JOIN model_binding_snapshots AS snapshot
            ON snapshot.workspace_id = cause.workspace_id
           AND snapshot.id = cause.model_binding_snapshot_id
          JOIN model_revisions AS revision
            ON revision.workspace_id = snapshot.workspace_id
           AND revision.id = snapshot.model_revision_id
         WHERE execution.workspace_id = preparation_row.workspace_id
           AND execution.id = preparation_row.model_execution_id
           AND execution.model_id = revision.model_id
           AND execution.usage_known = preparation_row.usage_known
           AND execution.prompt_tokens = COALESCE(preparation_row.prompt_tokens, 0)
           AND execution.completion_tokens = COALESCE(preparation_row.completion_tokens, 0)
           AND execution.latency_ms = 0
           AND execution.status = 'succeeded'
    ) THEN
        RAISE EXCEPTION 'provider result model execution is not the exact safe succeeded snapshot model'
            USING ERRCODE = '23514';
    END IF;
    IF preparation_row.state = 'published' THEN
        RETURN;
    END IF;

    PERFORM vestrace_finalize_bound_content_material(intent_row.id);
    SELECT * INTO material_row FROM content_materials
     WHERE intent_id = intent_row.id;
    INSERT INTO artifact_revision_contents (
        id, workspace_id, artifact_id, artifact_revision_id, content_material_id,
        provider_result_preparation_id, external_effect_id, run_id, step_id,
        material_intent_id, prepared_attachment_id, model_execution_id,
        erasure_bound_commitment, size_class, media_class
    ) VALUES (
        gen_random_uuid(), preparation_row.workspace_id, preparation_row.artifact_id,
        preparation_row.artifact_revision_id, material_row.id, preparation_row.id,
        preparation_row.external_effect_id, preparation_row.run_id, preparation_row.step_id,
        preparation_row.material_intent_id, preparation_row.prepared_attachment_id,
        preparation_row.model_execution_id, target_erasure_bound_commitment,
        preparation_row.size_class, 'text'
    );
    INSERT INTO provider_result_publications (
        id, workspace_id, provider_result_preparation_id, external_effect_id,
        run_id, step_id, artifact_id, artifact_revision_id, model_execution_id,
        material_intent_id, prepared_attachment_id, size_class
    ) VALUES (
        gen_random_uuid(), preparation_row.workspace_id, preparation_row.id,
        preparation_row.external_effect_id, preparation_row.run_id, preparation_row.step_id,
        preparation_row.artifact_id, preparation_row.artifact_revision_id,
        preparation_row.model_execution_id, preparation_row.material_intent_id,
        preparation_row.prepared_attachment_id, preparation_row.size_class
    );
    UPDATE provider_result_preparations
       SET state = 'published', published_at = NOW()
     WHERE id = preparation_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_task10_deferred_contract()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_workspace_id UUID;
    target_revision_id UUID;
    target_run_id UUID;
    target_step_id UUID;
    revision_storage_kind TEXT;
    content_count INTEGER;
    current_preparation provider_result_preparations%ROWTYPE;
    canonical_marker TEXT;
BEGIN
    IF TG_TABLE_NAME IN ('artifact_revisions', 'artifact_revision_contents') THEN
        target_workspace_id := COALESCE(NEW.workspace_id, OLD.workspace_id);
        PERFORM set_config('vestrace.workspace_id', target_workspace_id::TEXT, true);
        IF TG_TABLE_NAME = 'artifact_revisions' THEN
            target_revision_id := COALESCE(NEW.id, OLD.id);
        ELSE
            target_revision_id := COALESCE(NEW.artifact_revision_id, OLD.artifact_revision_id);
        END IF;
        SELECT storage_kind INTO revision_storage_kind
          FROM artifact_revisions
         WHERE workspace_id = target_workspace_id AND id = target_revision_id;
        IF FOUND THEN
            SELECT COUNT(*) INTO content_count
              FROM artifact_revision_contents
             WHERE workspace_id = target_workspace_id
               AND artifact_revision_id = target_revision_id;
            IF revision_storage_kind = 'governed_material' AND content_count <> 1 THEN
                RAISE EXCEPTION 'governed Artifact revision requires exactly one typed content map'
                    USING ERRCODE = '23514';
            ELSIF revision_storage_kind = 'legacy_digest' AND content_count <> 0 THEN
                RAISE EXCEPTION 'legacy Artifact revision cannot have a governed content map'
                    USING ERRCODE = '23514';
            END IF;
        ELSIF TG_TABLE_NAME = 'artifact_revision_contents' THEN
            RAISE EXCEPTION 'Artifact revision content requires its governed revision'
                USING ERRCODE = '23514';
        END IF;
        RETURN NULL;
    END IF;

    IF TG_TABLE_NAME IN (
        'external_effect_receipts',
        'external_effect_lifecycle_transitions',
        'provider_concurrency_leases',
        'provider_throttle_observations'
    ) THEN
        target_workspace_id := COALESCE(NEW.workspace_id, OLD.workspace_id);
        IF TG_TABLE_NAME IN (
            'external_effect_receipts', 'external_effect_lifecycle_transitions'
        ) THEN
            target_revision_id := COALESCE(NEW.effect_id, OLD.effect_id);
        ELSE
            target_revision_id := COALESCE(
                NEW.external_effect_id, OLD.external_effect_id
            );
        END IF;
        PERFORM set_config('vestrace.workspace_id', target_workspace_id::TEXT, true);
        IF TG_TABLE_NAME = 'external_effect_lifecycle_transitions' THEN
            IF NEW.status = 'dispatching'
               AND EXISTS (
                   SELECT 1 FROM model_request_evidence_roots AS root
                    WHERE root.workspace_id = NEW.workspace_id
                      AND root.external_effect_id = NEW.effect_id
               )
               AND NOT EXISTS (
                SELECT 1 FROM connection_dispatch_admissions AS admission
                 WHERE admission.workspace_id = NEW.workspace_id
                   AND admission.external_effect_id = NEW.effect_id
                   AND admission.decision = 'admitted'
                   AND admission.dispatch_expires_at = NEW.dispatch_expires_at
            ) THEN
                RAISE EXCEPTION 'Dispatching deadline must equal its admitted lease expiry'
                    USING ERRCODE = '23514';
            END IF;
        END IF;
        IF EXISTS (
            SELECT 1 FROM connection_dispatch_admissions AS admission
             WHERE admission.workspace_id = target_workspace_id
               AND admission.external_effect_id = target_revision_id
               AND admission.decision = 'admitted'
        ) AND EXISTS (
            SELECT 1 FROM provider_concurrency_leases AS lease
             WHERE lease.workspace_id = target_workspace_id
               AND lease.external_effect_id = target_revision_id
        ) THEN
            IF EXISTS (
                SELECT 1
                  FROM external_effect_receipts AS receipt
                 WHERE receipt.workspace_id = target_workspace_id
                   AND receipt.effect_id = target_revision_id
                   AND NOT EXISTS (
                       SELECT 1
                         FROM provider_concurrency_leases AS lease
                         JOIN external_effect_lifecycle_transitions AS transition
                           ON transition.workspace_id = receipt.workspace_id
                          AND transition.effect_id = receipt.effect_id
                          AND transition.status = receipt.outcome_status
                          AND transition.cause = 'receipt_recorded'
                          AND transition.cause_ref = receipt.id::TEXT
                        WHERE lease.workspace_id = receipt.workspace_id
                          AND lease.external_effect_id = receipt.effect_id
                          AND lease.released_at IS NOT NULL
                          AND lease.released_receipt_id = receipt.id
                   )
            ) OR EXISTS (
                SELECT 1
                  FROM provider_concurrency_leases AS lease
                 WHERE lease.workspace_id = target_workspace_id
                   AND lease.external_effect_id = target_revision_id
                   AND lease.released_receipt_id IS NOT NULL
                   AND NOT EXISTS (
                       SELECT 1
                         FROM external_effect_receipts AS receipt
                         JOIN external_effect_lifecycle_transitions AS transition
                           ON transition.workspace_id = receipt.workspace_id
                          AND transition.effect_id = receipt.effect_id
                          AND transition.status = receipt.outcome_status
                          AND transition.cause = 'receipt_recorded'
                          AND transition.cause_ref = receipt.id::TEXT
                        WHERE receipt.id = lease.released_receipt_id
                          AND receipt.workspace_id = lease.workspace_id
                          AND receipt.effect_id = lease.external_effect_id
                   )
            ) THEN
                RAISE EXCEPTION 'provider dispatch receipt and lease release must be committed together'
                    USING ERRCODE = '23514';
            END IF;
            IF EXISTS (
                SELECT 1
                  FROM external_effect_receipts AS receipt
                  JOIN provider_concurrency_leases AS lease
                    ON lease.workspace_id = receipt.workspace_id
                   AND lease.external_effect_id = receipt.effect_id
                   AND lease.released_receipt_id = receipt.id
                 WHERE receipt.workspace_id = target_workspace_id
                   AND receipt.effect_id = target_revision_id
                   AND receipt.payload->>'response_class' = 'http_429'
                   AND 1 <> (
                       SELECT COUNT(*)
                         FROM provider_throttle_observations AS observation
                        WHERE observation.workspace_id = receipt.workspace_id
                          AND observation.external_effect_id = receipt.effect_id
                          AND observation.external_effect_receipt_id = receipt.id
                   )
            ) OR EXISTS (
                SELECT 1
                  FROM provider_throttle_observations AS observation
                 WHERE observation.workspace_id = target_workspace_id
                   AND observation.external_effect_id = target_revision_id
                   AND NOT EXISTS (
                       SELECT 1
                         FROM external_effect_receipts AS receipt
                         JOIN provider_concurrency_leases AS lease
                           ON lease.workspace_id = receipt.workspace_id
                          AND lease.external_effect_id = receipt.effect_id
                          AND lease.released_receipt_id = receipt.id
                        WHERE receipt.id = observation.external_effect_receipt_id
                          AND receipt.workspace_id = observation.workspace_id
                          AND receipt.effect_id = observation.external_effect_id
                          AND receipt.payload->>'response_class' = 'http_429'
                   )
            ) THEN
                RAISE EXCEPTION 'http_429 provider receipt requires its exact throttle observation'
                    USING ERRCODE = '23514';
            END IF;
        END IF;
        RETURN NULL;
    END IF;

    IF TG_TABLE_NAME = 'agent_runs' THEN
        target_workspace_id := NEW.workspace_id;
        PERFORM set_config('vestrace.workspace_id', target_workspace_id::TEXT, true);
        target_run_id := NEW.id;
        IF NEW.status <> 'running' AND EXISTS (
            SELECT 1 FROM provider_result_preparations
             WHERE workspace_id = target_workspace_id
               AND run_id = target_run_id
               AND state = 'result_prepared'
        ) THEN
            RAISE EXCEPTION 'ResultPrepared blocks incompatible Run terminalization'
                USING ERRCODE = '23514';
        END IF;
        RETURN NULL;
    END IF;

    IF TG_TABLE_NAME = 'run_steps' THEN
        target_workspace_id := NEW.workspace_id;
        PERFORM set_config('vestrace.workspace_id', target_workspace_id::TEXT, true);
        target_run_id := NEW.run_id;
        target_step_id := NEW.id;
        IF NEW.status <> 'running' AND EXISTS (
            SELECT 1 FROM provider_result_preparations
             WHERE workspace_id = target_workspace_id
               AND run_id = target_run_id
               AND step_id = target_step_id
               AND state = 'result_prepared'
        ) THEN
            RAISE EXCEPTION 'ResultPrepared blocks incompatible step terminalization'
                USING ERRCODE = '23514';
        END IF;
        RETURN NULL;
    END IF;

    IF TG_TABLE_NAME = 'provider_result_preparations' THEN
        SELECT * INTO current_preparation
          FROM provider_result_preparations
         WHERE id = COALESCE(NEW.id, OLD.id);
        IF NOT FOUND OR current_preparation.state <> 'result_prepared' THEN
            RETURN NULL;
        END IF;
        PERFORM set_config(
            'vestrace.workspace_id', current_preparation.workspace_id::TEXT, true
        );
        IF NOT EXISTS (
            SELECT 1 FROM agent_runs
             WHERE workspace_id = current_preparation.workspace_id
               AND id = current_preparation.run_id AND status = 'running'
        ) OR NOT EXISTS (
            SELECT 1 FROM run_steps
             WHERE workspace_id = current_preparation.workspace_id
               AND run_id = current_preparation.run_id
               AND id = current_preparation.step_id
               AND status = 'running'
        ) THEN
            RAISE EXCEPTION 'ResultPrepared requires its active Run and running step'
                USING ERRCODE = '23514';
        END IF;
        canonical_marker := 'provider_result_preparation:'
            || lower(current_preparation.id::TEXT);
        IF current_preparation.external_effect_receipt_id IS NULL
           OR current_preparation.receipt_witnessed_at IS NULL
           OR NOT EXISTS (
               SELECT 1
                 FROM external_effect_receipts AS receipt
                 JOIN external_effect_lifecycle_transitions AS transition
                   ON transition.effect_id = receipt.effect_id
                  AND transition.workspace_id = receipt.workspace_id
                  AND transition.status = 'acknowledged'
                  AND transition.cause = 'receipt_recorded'
                  AND transition.cause_ref = receipt.id::TEXT
                WHERE receipt.id = current_preparation.external_effect_receipt_id
                  AND receipt.workspace_id = current_preparation.workspace_id
                  AND receipt.effect_id = current_preparation.external_effect_id
                  AND receipt.outcome_status = 'acknowledged'
                  AND jsonb_typeof(receipt.payload->'evidence_refs') = 'array'
                  AND jsonb_array_length(receipt.payload->'evidence_refs') = 1
                  AND receipt.payload->'evidence_refs'->>0 = canonical_marker
           ) THEN
            RAISE EXCEPTION 'ResultPrepared requires its exact witnessed provider receipt'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER artifact_revisions_task10_content_contract
    AFTER INSERT OR UPDATE OR DELETE ON artifact_revisions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER artifact_revision_contents_task10_content_contract
    AFTER INSERT OR UPDATE OR DELETE ON artifact_revision_contents
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER external_effect_dispatch_deadline_contract
    AFTER INSERT OR UPDATE ON external_effect_lifecycle_transitions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER external_effect_receipts_task10_dispatch_contract
    AFTER INSERT OR UPDATE OR DELETE ON external_effect_receipts
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER provider_concurrency_leases_task10_receipt_contract
    AFTER INSERT OR UPDATE OR DELETE ON provider_concurrency_leases
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER provider_throttle_observations_task10_receipt_contract
    AFTER INSERT OR UPDATE OR DELETE ON provider_throttle_observations
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER agent_runs_result_prepared_contract
    AFTER INSERT OR UPDATE ON agent_runs
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER run_steps_result_prepared_contract
    AFTER INSERT OR UPDATE ON run_steps
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();
CREATE CONSTRAINT TRIGGER provider_result_preparations_task10_pending_contract
    AFTER INSERT OR UPDATE ON provider_result_preparations
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_task10_deferred_contract();

CREATE OR REPLACE FUNCTION public.vestrace_record_model_data_policy_decision(
    target_id UUID,
    target_run_id UUID,
    target_step_id UUID,
    target_destination TEXT,
    target_classification TEXT,
    target_verdict TEXT,
    target_reason TEXT,
    target_policy_version TEXT,
    target_mode TEXT,
    target_decided_at TIMESTAMPTZ
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    existing public.model_data_policy_decisions%ROWTYPE;
BEGIN
    INSERT INTO public.model_data_policy_decisions (
        id, run_id, step_id, destination, classification, verdict,
        reason, policy_version, mode, decided_at
    ) VALUES (
        target_id, target_run_id, target_step_id, target_destination,
        target_classification, target_verdict, target_reason,
        target_policy_version, target_mode, target_decided_at
    )
    ON CONFLICT (id) DO NOTHING
    RETURNING * INTO existing;

    IF FOUND THEN
        RETURN existing.id;
    END IF;

    SELECT * INTO existing
      FROM public.model_data_policy_decisions AS decision
     WHERE decision.id = target_id
     FOR UPDATE OF decision;
    IF NOT FOUND
       OR existing.run_id IS DISTINCT FROM target_run_id
       OR existing.step_id IS DISTINCT FROM target_step_id
       OR existing.destination IS DISTINCT FROM target_destination
       OR existing.classification IS DISTINCT FROM target_classification
       OR existing.verdict IS DISTINCT FROM target_verdict
       OR existing.reason IS DISTINCT FROM target_reason
       OR existing.policy_version IS DISTINCT FROM target_policy_version
       OR existing.mode IS DISTINCT FROM target_mode
       OR existing.decided_at IS DISTINCT FROM target_decided_at THEN
        RAISE EXCEPTION 'model data-policy decision replay tuple mismatch'
            USING ERRCODE = '23514';
    END IF;
    RETURN existing.id;
END
$$;

-- Forward replacement: immutable lease discovery is allowed to learn the
-- credential slot, but the canonical connection/activation/slot chain must be
-- held before the lease, material intent, or effect row is locked.
CREATE OR REPLACE FUNCTION public.vestrace_consume_credential_dispatch_lease(
    target_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_external_effect_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preliminary credential_dispatch_leases%ROWTYPE;
    lease_row credential_dispatch_leases%ROWTYPE;
    latest_effect_state TEXT;
    latest_effect_cause_ref TEXT;
BEGIN
    IF target_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_external_effect_id IS NULL THEN
        RAISE EXCEPTION 'credential dispatch lease consumption arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT * INTO preliminary
      FROM credential_dispatch_leases AS lease
     WHERE lease.id = target_lease_id
       AND lease.workspace_id = target_workspace_id
       AND lease.connection_id = target_connection_id
       AND lease.external_effect_id = target_external_effect_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential dispatch lease is already terminal or absent'
            USING ERRCODE = '23514';
    END IF;

    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, preliminary.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard',
              'credential_slot', 'revision_material']::TEXT[]
    );

    SELECT * INTO lease_row
      FROM credential_dispatch_leases AS lease
     WHERE lease.id = target_lease_id
       AND lease.workspace_id = target_workspace_id
       AND lease.connection_id = target_connection_id
       AND lease.external_effect_id = target_external_effect_id
     FOR UPDATE OF lease;
    IF NOT FOUND OR lease_row.terminal_state IS NOT NULL
       OR lease_row.credential_slot_id IS DISTINCT FROM preliminary.credential_slot_id
       OR lease_row.credential_revision_id IS DISTINCT FROM preliminary.credential_revision_id
       OR lease_row.credential_activation_guard_id IS DISTINCT FROM preliminary.credential_activation_guard_id
       OR lease_row.authorization_id IS DISTINCT FROM preliminary.authorization_id
       OR lease_row.destination_authority IS DISTINCT FROM preliminary.destination_authority
       OR lease_row.auth_mode IS DISTINCT FROM preliminary.auth_mode
       OR lease_row.expires_at IS DISTINCT FROM preliminary.expires_at THEN
        RAISE EXCEPTION 'credential dispatch lease is already terminal or absent'
            USING ERRCODE = '23514';
    END IF;
    IF lease_row.expires_at <= NOW() THEN
        RAISE EXCEPTION 'credential dispatch lease is expired'
            USING ERRCODE = '23514';
    END IF;

    PERFORM 1
      FROM credential_activation_guards AS activation
      JOIN credential_slots AS slot
        ON slot.workspace_id = activation.workspace_id
       AND slot.connection_id = activation.connection_id
       AND slot.id = activation.credential_slot_id
      JOIN credential_key_creation_intents AS intent
        ON intent.workspace_id = slot.workspace_id
       AND intent.connection_id = slot.connection_id
       AND intent.credential_slot_id = slot.id
       AND intent.credential_revision_id = slot.current_revision_id
      JOIN credential_revisions AS revision
        ON revision.workspace_id = intent.workspace_id
       AND revision.id = intent.credential_revision_id
      JOIN credential_prepared_materials AS material
        ON material.workspace_id = intent.workspace_id
       AND material.intent_id = intent.id
       AND material.credential_revision_id = intent.credential_revision_id
     WHERE activation.id = lease_row.credential_activation_guard_id
       AND activation.workspace_id = target_workspace_id
       AND activation.connection_id = target_connection_id
       AND activation.credential_slot_id = lease_row.credential_slot_id
       AND slot.current_revision_id = lease_row.credential_revision_id
       AND slot.tombstone_version IS NULL
       AND intent.state = 'active'
       AND revision.associated_data_profile = 'credential_v2'
     FOR UPDATE OF intent;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential dispatch lease no longer names the current Active credential'
            USING ERRCODE = '23514';
    END IF;

    SELECT transition.status, transition.cause_ref
      INTO latest_effect_state, latest_effect_cause_ref
      FROM external_effect_lifecycle_transitions AS transition
     WHERE transition.effect_id = target_external_effect_id
       AND transition.workspace_id = target_workspace_id
     ORDER BY transition.ordinal DESC
     LIMIT 1
     FOR KEY SHARE OF transition;
    IF NOT FOUND OR latest_effect_state <> 'authorized'
       OR latest_effect_cause_ref <> lease_row.authorization_id::TEXT THEN
        RAISE EXCEPTION 'credential dispatch lease effect is no longer authorized for dispatch'
            USING ERRCODE = '23514';
    END IF;

    UPDATE credential_dispatch_leases
       SET terminal_state = 'consumed_for_dispatch', consumed_at = NOW()
     WHERE id = lease_row.id;
    RETURN lease_row.id;
END
$$;

-- Provider dispatch must acquire the permanent ConnectionExecutionGuard before
-- reconstruction takes any lower evidence/material locks. Runtime cannot read
-- guarded routing tables directly, so this narrow entrypoint returns only the
-- non-secret tuple needed to validate caller-supplied credential data.
CREATE OR REPLACE FUNCTION public.vestrace_lock_provider_dispatch_routing(
    target_workspace_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_effect_id UUID,
    target_evidence_id UUID,
    target_cause_kind TEXT,
    target_run_id UUID,
    target_step_id UUID,
    target_snapshot_id UUID,
    target_qualification_job_id UUID,
    target_qualification_target_id UUID
)
RETURNS TABLE(
    branch TEXT,
    credential_revision_id UUID,
    credential_slot_id UUID,
    credential_activation_guard_id UUID,
    expected_slot_version BIGINT,
    no_auth_binding_revision_id UUID,
    runtime_base_url TEXT,
    auth_mode TEXT,
    revision_credential_slot_id UUID
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_guard_id UUID;
    preliminary RECORD;
    routing RECORD;
BEGIN
    IF target_workspace_id IS NULL OR target_connection_id IS NULL
       OR target_connection_revision_id IS NULL OR target_effect_id IS NULL
       OR target_evidence_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR NOT (
          (target_cause_kind = 'run_step'
           AND target_run_id IS NOT NULL AND target_step_id IS NOT NULL
           AND target_snapshot_id IS NOT NULL
           AND target_qualification_job_id IS NULL
           AND target_qualification_target_id IS NULL)
          OR
          (target_cause_kind = 'qualification_probe'
           AND target_run_id IS NULL AND target_step_id IS NULL
           AND target_snapshot_id IS NULL
           AND target_qualification_job_id IS NOT NULL
           AND target_qualification_target_id IS NOT NULL)
       ) THEN
        RAISE EXCEPTION 'provider dispatch routing arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT guard.id INTO existing_guard_id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch requires its existing permanent connection guard'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_ensure_connection_execution_guard(
        existing_guard_id, target_workspace_id, target_connection_id
    );

    -- Immutable discovery only: no snapshot/qualification/evidence row is
    -- touched before the optional credential chain and MRE root authority.
    SELECT revision.runtime_base_url, revision.auth_mode,
           revision.credential_slot_id
      INTO preliminary
      FROM connection_revisions AS revision
     WHERE revision.workspace_id = target_workspace_id
       AND revision.connection_id = target_connection_id
       AND revision.id = target_connection_revision_id;
    IF NOT FOUND OR NOT (
        (preliminary.auth_mode = 'none' AND preliminary.credential_slot_id IS NULL)
        OR
        (preliminary.auth_mode <> 'none' AND preliminary.credential_slot_id IS NOT NULL)
    ) THEN
        RAISE EXCEPTION 'provider dispatch connection revision is not routable'
            USING ERRCODE = '23514';
    END IF;

    IF preliminary.credential_slot_id IS NOT NULL THEN
        PERFORM vestrace_acquire_credential_lock_chain(
            target_workspace_id, target_connection_id,
            preliminary.credential_slot_id,
            ARRAY['connection_execution_guard', 'credential_activation_guard',
                  'credential_slot', 'revision_material']::TEXT[]
        );
        PERFORM intent.id
          FROM credential_slots AS slot
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = slot.current_revision_id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = preliminary.credential_slot_id
           AND slot.current_revision_id IS NOT NULL
           AND slot.tombstone_version IS NULL
           AND intent.state = 'active'
         FOR UPDATE OF intent;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'provider dispatch credential routing is no longer usable'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    PERFORM vestrace_lock_model_request_evidence_for_reconstruction(
        target_workspace_id, target_evidence_id
    );

    IF target_cause_kind = 'run_step' THEN
        SELECT snapshot.branch, snapshot.credential_revision_id,
               snapshot.credential_slot_id,
               snapshot.credential_activation_guard_id,
               snapshot.expected_slot_version,
               snapshot.no_auth_binding_revision_id,
               revision.runtime_base_url, revision.auth_mode,
               revision.credential_slot_id AS revision_credential_slot_id
          INTO routing
          FROM run_model_binding_snapshots AS run_snapshot
          JOIN run_steps AS step
            ON step.workspace_id = run_snapshot.workspace_id
           AND step.run_id = run_snapshot.run_id
           AND step.id = target_step_id
          JOIN model_binding_snapshots AS snapshot
            ON snapshot.workspace_id = run_snapshot.workspace_id
           AND snapshot.id = run_snapshot.snapshot_id
          JOIN connection_revisions AS revision
            ON revision.workspace_id = snapshot.workspace_id
           AND revision.connection_id = snapshot.connection_id
           AND revision.id = snapshot.connection_revision_id
          JOIN model_request_evidence_roots AS evidence
            ON evidence.workspace_id = snapshot.workspace_id
           AND evidence.id = target_evidence_id
           AND evidence.external_effect_id = target_effect_id
           AND evidence.cause_kind = 'run_step'
           AND evidence.cause_id = target_step_id
           AND evidence.binding_snapshot_id = snapshot.id
         WHERE run_snapshot.workspace_id = target_workspace_id
           AND run_snapshot.run_id = target_run_id
           AND run_snapshot.snapshot_id = target_snapshot_id
           AND snapshot.connection_id = target_connection_id
           AND snapshot.connection_revision_id = target_connection_revision_id
         ;
    ELSE
        SELECT binding.branch, binding.credential_revision_id,
               binding.credential_slot_id,
               binding.credential_activation_guard_id,
               binding.expected_slot_version,
               binding.no_auth_binding_revision_id,
               revision.runtime_base_url, revision.auth_mode,
               revision.credential_slot_id AS revision_credential_slot_id
          INTO routing
          FROM qualification_target_bindings AS binding
          JOIN connection_revisions AS revision
            ON revision.workspace_id = binding.workspace_id
           AND revision.connection_id = binding.connection_id
           AND revision.id = binding.connection_revision_id
          JOIN model_request_evidence_roots AS evidence
            ON evidence.workspace_id = binding.workspace_id
           AND evidence.id = target_evidence_id
           AND evidence.external_effect_id = target_effect_id
           AND evidence.cause_kind = 'qualification_probe'
           AND evidence.cause_id = binding.qualification_job_id
           AND evidence.qualification_target_binding_id = binding.id
         WHERE binding.workspace_id = target_workspace_id
           AND binding.qualification_job_id = target_qualification_job_id
           AND binding.id = target_qualification_target_id
           AND binding.connection_id = target_connection_id
           AND binding.connection_revision_id = target_connection_revision_id
         ;
    END IF;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch routing tuple is not current and exact'
            USING ERRCODE = '23514';
    END IF;

    IF routing.branch = 'credential' THEN
        IF routing.auth_mode = 'none'
           OR routing.revision_credential_slot_id IS DISTINCT FROM routing.credential_slot_id THEN
            RAISE EXCEPTION 'provider dispatch credential routing is no longer current and usable'
                USING ERRCODE = '23514';
        END IF;
        PERFORM 1
          FROM credential_slots AS slot
          JOIN credential_activation_guards AS activation
            ON activation.workspace_id = slot.workspace_id
           AND activation.connection_id = slot.connection_id
           AND activation.credential_slot_id = slot.id
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = slot.current_revision_id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = routing.credential_slot_id
           AND slot.id = preliminary.credential_slot_id
           AND slot.current_revision_id = routing.credential_revision_id
           AND slot.current_revision_version = routing.expected_slot_version
           AND slot.tombstone_version IS NULL
           AND activation.id = routing.credential_activation_guard_id
           AND intent.state = 'active';
        IF NOT FOUND THEN
            RAISE EXCEPTION 'provider dispatch credential routing is no longer current and usable'
                USING ERRCODE = '23514';
        END IF;
    ELSIF routing.branch <> 'no_auth' OR routing.auth_mode <> 'none'
          OR routing.revision_credential_slot_id IS NOT NULL
          OR routing.no_auth_binding_revision_id IS NULL
          OR preliminary.credential_slot_id IS NOT NULL THEN
        RAISE EXCEPTION 'provider dispatch no-auth routing is not exact'
            USING ERRCODE = '23514';
    END IF;

    RETURN QUERY SELECT
        routing.branch::TEXT,
        routing.credential_revision_id::UUID,
        routing.credential_slot_id::UUID,
        routing.credential_activation_guard_id::UUID,
        routing.expected_slot_version::BIGINT,
        routing.no_auth_binding_revision_id::UUID,
        routing.runtime_base_url::TEXT,
        routing.auth_mode::TEXT,
        routing.revision_credential_slot_id::UUID;
END
$$;

-- The legacy effect-intent table is runtime-owned. Its rows were already
-- append-only by application contract; make that boundary database-visible
-- before granting the guarded reconstruction owner the column privilege that
-- PostgreSQL requires for SELECT ... FOR SHARE.
CREATE OR REPLACE FUNCTION public.vestrace_reject_external_effect_intent_rewrite()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    RAISE EXCEPTION 'external effect intents are immutable'
        USING ERRCODE = '23514';
END
$$;

CREATE TRIGGER external_effect_intents_immutable
    BEFORE UPDATE OR DELETE ON public.external_effect_intents
    FOR EACH ROW EXECUTE FUNCTION public.vestrace_reject_external_effect_intent_rewrite();

CREATE OR REPLACE FUNCTION public.vestrace_lock_model_request_evidence_for_reconstruction(
    target_workspace_id UUID,
    target_evidence_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    evidence_root model_request_evidence_roots%ROWTYPE;
BEGIN
    IF target_workspace_id IS NULL OR target_evidence_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model request evidence reconstruction workspace is required'
            USING ERRCODE = '42501';
    END IF;

    SELECT * INTO evidence_root
      FROM model_request_evidence_roots AS root
     WHERE root.workspace_id = target_workspace_id
       AND root.id = target_evidence_id
     FOR UPDATE OF root;
    IF NOT FOUND OR NOT (
        (evidence_root.cause_kind = 'run_step'
         AND evidence_root.binding_snapshot_id IS NOT NULL
         AND evidence_root.qualification_target_binding_id IS NULL
         AND EXISTS (
             SELECT 1 FROM model_request_evidence_nodes AS node
              WHERE node.workspace_id = target_workspace_id
                AND node.evidence_root_id = target_evidence_id
                AND node.reference_kind = 'binding_snapshot'
                AND node.reference_id = evidence_root.binding_snapshot_id
         ))
        OR
        (evidence_root.cause_kind = 'qualification_probe'
         AND evidence_root.binding_snapshot_id IS NULL
         AND evidence_root.qualification_target_binding_id IS NOT NULL
         AND EXISTS (
             SELECT 1 FROM qualification_target_bindings AS binding
              WHERE binding.workspace_id = target_workspace_id
                AND binding.id = evidence_root.qualification_target_binding_id
                AND binding.qualification_job_id = evidence_root.cause_id
         )
         AND EXISTS (
             SELECT 1 FROM model_request_evidence_nodes AS node
              WHERE node.workspace_id = target_workspace_id
                AND node.evidence_root_id = target_evidence_id
                AND node.reference_kind = 'qualification_target'
                AND node.reference_id = evidence_root.qualification_target_binding_id
         )
         AND EXISTS (
             SELECT 1 FROM model_request_evidence_nodes AS node
              WHERE node.workspace_id = target_workspace_id
                AND node.evidence_root_id = target_evidence_id
                AND node.reference_kind = 'qualification_probe'
                AND node.reference_id = evidence_root.cause_id
                AND node.safe_ordinal IS NOT NULL
         ))
    ) OR NOT EXISTS (
        SELECT 1 FROM model_request_evidence_nodes AS node
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_evidence_id
           AND node.reference_kind = 'external_effect'
           AND node.reference_id = evidence_root.external_effect_id
    ) THEN
        RAISE EXCEPTION 'model request evidence reconstruction cause is malformed'
            USING ERRCODE = '23514';
    END IF;

    IF evidence_root.binding_snapshot_id IS NOT NULL THEN
        PERFORM snapshot.id
          FROM model_binding_snapshots AS snapshot
         WHERE snapshot.workspace_id = target_workspace_id
           AND snapshot.id = evidence_root.binding_snapshot_id
         FOR SHARE OF snapshot;
    ELSE
        PERFORM binding.id
          FROM qualification_target_bindings AS binding
         WHERE binding.workspace_id = target_workspace_id
           AND binding.id = evidence_root.qualification_target_binding_id
         FOR SHARE OF binding;
    END IF;

    PERFORM node.id
      FROM model_request_evidence_nodes AS node
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
     ORDER BY node.ordinal
     FOR SHARE OF node;

    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN external_effect_intents AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'external_effect'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN connection_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'connection_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN connection_qualification_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'connection_qualification_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'model_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_qualification_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'model_qualification_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_binding_snapshots AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'binding_snapshot'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN qualification_target_bindings AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'qualification_target'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_request_shape_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'request_shape_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_sampling_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'sampling_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_limits_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'limits_revision'
     ORDER BY source.id FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_tool_schema_revisions AS source
        ON source.workspace_id = node.workspace_id AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'tool_schema_revision'
     ORDER BY source.id FOR SHARE OF source;

    PERFORM material.id
      FROM model_request_evidence_nodes AS node
      JOIN content_materials AS material
        ON material.workspace_id = node.workspace_id AND material.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY material.id FOR SHARE OF material;
    PERFORM intent.id
      FROM model_request_evidence_nodes AS node
      JOIN content_materials AS material
        ON material.workspace_id = node.workspace_id AND material.id = node.reference_id
      JOIN material_key_creation_intents AS intent
        ON intent.workspace_id = material.workspace_id AND intent.id = material.intent_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY intent.id FOR SHARE OF intent;
    PERFORM bytes.material_id
      FROM model_request_evidence_nodes AS node
      JOIN content_material_bytes AS bytes
        ON bytes.workspace_id = node.workspace_id AND bytes.material_id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY bytes.material_id FOR SHARE OF bytes;
END
$$;

-- Provider-result material may become Live only inside the provider-owned
-- publication transaction. This is intentionally an internal deferred guard:
-- generic P02 Bound resumption has no execute path around it, while the exact
-- provider finalizer can satisfy it before commit.
CREATE OR REPLACE FUNCTION public.vestrace_validate_provider_result_material_live()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    exact_publication_exists BOOLEAN;
BEGIN
    SELECT EXISTS (
        SELECT 1
          FROM public.provider_result_preparations AS preparation
         WHERE preparation.workspace_id = NEW.workspace_id
           AND preparation.material_intent_id = NEW.id
           AND preparation.state = 'published'
    ) INTO exact_publication_exists;
    IF NEW.state = 'live'
       AND OLD.state IS DISTINCT FROM NEW.state
       AND NEW.owner_kind = 'provider_result'
       AND NOT exact_publication_exists THEN
        RAISE EXCEPTION 'provider result material may become Live only beside its exact publication'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END
$$;

SELECT public.vestrace_install_provider_result_live_trigger();
SELECT vestrace_grant_p03_dependency_references();

SELECT vestrace_assign_p03_table_owner('model_request_evidence_checks'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('qualification_target_bindings'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_admission_waits'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_dispatch_admissions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_concurrency_leases'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_throttle_observations'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_result_preparations'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('artifact_revision_contents'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_data_policy_decisions'::REGCLASS);

SELECT vestrace_assign_p03_function_owner(
    'vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_release_provider_dispatch(UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_record_provider_throttle(UUID, UUID, UUID, UUID, INTEGER)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_witness_provider_result_receipt(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_finalize_provider_result(UUID, BYTEA)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_validate_task10_deferred_contract()'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_validate_provider_result_material_live()'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_record_model_data_policy_decision(UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TIMESTAMPTZ)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_consume_credential_dispatch_lease(UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_lock_model_request_evidence_for_reconstruction(UUID, UUID)'::REGPROCEDURE
);
