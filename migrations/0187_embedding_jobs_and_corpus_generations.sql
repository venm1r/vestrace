-- P04 Task 3: guarded embedding space registrations, corpus generations and jobs.
--
-- What this migration deliberately does NOT do: it does not take migration
-- 0009's `embedding_spaces` under the guarded owner. That table is written today
-- by the lazy `ensure_space(context, name, model, dimensions)` path running as
-- the restricted runtime role, and the replacement writer does not exist until a
-- later task. Guarding it here would break the memory write path in the middle
-- of the package. The registration table below is the authorized key; the legacy
-- row it names keeps holding the vectors until that writer arrives.

CREATE TABLE embedding_space_registrations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    -- The legacy `embedding_spaces` row this key authorizes. Not a foreign key:
    -- that table is unguarded and may be rewritten by the later task that takes
    -- it over, and a constraint pointing into it would make this table's
    -- integrity depend on a table nothing yet guards.
    space_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(btrim(name)) > 0),
    model TEXT NOT NULL CHECK (length(btrim(model)) > 0),
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_space_registrations_space_key
        UNIQUE (workspace_id, space_id),
    -- The whole tuple is the identity. Two spaces differing only in model are
    -- different spaces, so the same name may be registered twice only against a
    -- different model or dimension count.
    CONSTRAINT embedding_space_registrations_tuple_key
        UNIQUE (workspace_id, name, model, dimensions),
    CONSTRAINT embedding_space_registrations_workspace_id_id_key
        UNIQUE (workspace_id, id)
);

ALTER TABLE embedding_space_registrations ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_space_registrations FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_space_registrations_workspace_policy
    ON embedding_space_registrations
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_space_registrations FROM PUBLIC;
CREATE TRIGGER embedding_space_registrations_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_space_registrations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

CREATE TABLE embedding_corpus_generations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    space_registration_id UUID NOT NULL,
    ordinal BIGINT NOT NULL CHECK (ordinal >= 1),
    -- Spec line 207: "The target corpus revision, projection/member count, and
    -- Ready generation must match that same recipe set." The count is carried so
    -- a later reader can check that match without recomputing it.
    member_count BIGINT NOT NULL CHECK (member_count >= 0),
    state TEXT NOT NULL CHECK (state IN ('ready', 'stale')),
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_corpus_generations_registration_fkey
        FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_corpus_generations_ordinal_key
        UNIQUE (workspace_id, space_registration_id, ordinal),
    CONSTRAINT embedding_corpus_generations_workspace_id_id_key
        UNIQUE (workspace_id, id)
);

-- Spec line 219: the finalizer "marks any current Ready generation for that
-- space Stale/not-current" before publishing the next one. At most one Ready
-- generation per space is therefore an invariant of the schema rather than a
-- convention the finalizer is trusted to keep.
CREATE UNIQUE INDEX embedding_corpus_generations_one_ready_per_space
    ON embedding_corpus_generations (workspace_id, space_registration_id)
    WHERE state = 'ready';

ALTER TABLE embedding_corpus_generations ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_corpus_generations FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_corpus_generations_workspace_policy
    ON embedding_corpus_generations
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_corpus_generations FROM PUBLIC;
CREATE TRIGGER embedding_corpus_generations_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_corpus_generations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

CREATE TABLE embedding_jobs (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    space_registration_id UUID NOT NULL,
    -- Spec line 219: "Its closed kinds are `retrieval_query`, `delivery`, and
    -- `rebuild` (backfill is `rebuild` mode)." All three, and the same three
    -- crates/vestrace-domain/src/embedding/job.rs declares.
    kind TEXT NOT NULL CHECK (kind IN ('retrieval_query', 'delivery', 'rebuild')),
    -- Spec line 219: the job "records append-only `Requested -> Running ->
    -- Succeeded | FailedDefinite | InconclusiveUnknown | Cancelled`
    -- transitions". Six states, and the same six
    -- crates/vestrace-domain/src/embedding/job.rs declares. A state in one list
    -- and not the other is a defect the schema contract test is required to
    -- catch.
    --
    -- `dispatching`, `waiting` and `result_prepared` are deliberately absent:
    -- they belong to the external effect, the attempt's pre-dispatch phase and
    -- the EmbeddingJobResultPrepared marker respectively. A job column carrying
    -- them would be a second answer to whether the provider was reached.
    state TEXT NOT NULL DEFAULT 'requested' CHECK (state IN (
        'requested', 'running', 'succeeded', 'failed_definite',
        'inconclusive_unknown', 'cancelled'
    )),
    model_binding_snapshot_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    model_request_evidence_id UUID NOT NULL,
    -- The handle an authorized acknowledgement compares against. Line 257
    -- requires that command to take an expected job version, and append-only
    -- states alone cannot express "the job I read is still the job I am acting
    -- on".
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    -- Spec line 257: the acknowledged successor links to the predecessor it
    -- retries. Nullable because an ordinary job retries nothing.
    retries_unknown_embedding_job_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_jobs_registration_fkey
        FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_jobs_snapshot_fkey
        FOREIGN KEY (workspace_id, model_binding_snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_jobs_predecessor_fkey
        FOREIGN KEY (workspace_id, retries_unknown_embedding_job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    -- One adapter invocation per external-effect identity (line 245) begins with
    -- one job per effect.
    CONSTRAINT embedding_jobs_unique_external_effect UNIQUE (external_effect_id),
    CONSTRAINT embedding_jobs_unique_model_request_evidence
        UNIQUE (model_request_evidence_id),
    CONSTRAINT embedding_jobs_workspace_id_id_key UNIQUE (workspace_id, id),
    -- A job cannot retry itself.
    CONSTRAINT embedding_jobs_predecessor_is_not_self
        CHECK (retries_unknown_embedding_job_id IS DISTINCT FROM id)
);

-- Spec line 251: "A predecessor has at most one direct successor across this
-- same-version edge or the cross-version carry edge below." The same-version
-- edge is enforced here; the carry edge is enforced by the carry mapping table
-- a later task adds.
CREATE UNIQUE INDEX embedding_jobs_one_successor_per_predecessor
    ON embedding_jobs (workspace_id, retries_unknown_embedding_job_id)
    WHERE retries_unknown_embedding_job_id IS NOT NULL;

ALTER TABLE embedding_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_jobs_workspace_policy
    ON embedding_jobs
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_jobs FROM PUBLIC;
CREATE TRIGGER embedding_jobs_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_jobs
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

-- The authorized way to bring a space key into existence.
--
-- It replaces nothing yet: the lazy path in retrieval/embedding.rs still runs.
-- What it establishes is that from here on there is a moment at which a space
-- was authorized, with the whole tuple named, rather than a row that appeared
-- because something needed to write a vector.
CREATE OR REPLACE FUNCTION vestrace_register_embedding_space(
    target_registration_id UUID,
    target_workspace_id UUID,
    target_space_id UUID,
    target_name TEXT,
    target_model TEXT,
    target_dimensions INTEGER
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_id UUID;
BEGIN
    IF target_registration_id IS NULL OR target_workspace_id IS NULL
       OR target_space_id IS NULL
       OR target_name IS NULL OR length(btrim(target_name)) = 0
       OR target_model IS NULL OR length(btrim(target_model)) = 0
       OR target_dimensions IS NULL OR target_dimensions <= 0 THEN
        RAISE EXCEPTION 'embedding space registration arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    -- Registration is idempotent on the exact tuple: the same key registered
    -- twice returns the first registration rather than conflicting, because the
    -- caller that retries after a crash must converge on one identity.
    SELECT id INTO existing_id
      FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id
       AND name = target_name
       AND model = target_model
       AND dimensions = target_dimensions;
    IF FOUND THEN
        IF (SELECT space_id FROM embedding_space_registrations WHERE id = existing_id)
           IS DISTINCT FROM target_space_id THEN
            RAISE EXCEPTION 'embedding space key already names a different space'
                USING ERRCODE = '23514';
        END IF;
        RETURN existing_id;
    END IF;

    INSERT INTO embedding_space_registrations (
        id, workspace_id, space_id, name, model, dimensions
    ) VALUES (
        target_registration_id, target_workspace_id, target_space_id,
        target_name, target_model, target_dimensions
    );
    RETURN target_registration_id;
END
$$;

-- Publish a Ready generation and supersede the one it replaces, in one
-- transaction.
--
-- Spec line 219 requires the finalizer to mark "any current Ready generation for
-- that space Stale/not-current" as it advances the corpus. Doing both here means
-- the partial unique index can never see two Ready rows, rather than relying on
-- two callers ordering themselves correctly.
CREATE OR REPLACE FUNCTION vestrace_publish_embedding_corpus_generation(
    target_generation_id UUID,
    target_workspace_id UUID,
    target_registration_id UUID,
    target_member_count BIGINT
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    next_ordinal BIGINT;
BEGIN
    IF target_generation_id IS NULL OR target_workspace_id IS NULL
       OR target_registration_id IS NULL
       OR target_member_count IS NULL OR target_member_count < 0 THEN
        RAISE EXCEPTION 'embedding corpus generation arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    PERFORM 1 FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id AND id = target_registration_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding corpus generation requires its registered space'
            USING ERRCODE = '23514';
    END IF;

    UPDATE embedding_corpus_generations
       SET state = 'stale'
     WHERE workspace_id = target_workspace_id
       AND space_registration_id = target_registration_id
       AND state = 'ready';

    SELECT COALESCE(MAX(ordinal), 0) + 1 INTO next_ordinal
      FROM embedding_corpus_generations
     WHERE workspace_id = target_workspace_id
       AND space_registration_id = target_registration_id;

    INSERT INTO embedding_corpus_generations (
        id, workspace_id, space_registration_id, ordinal, member_count, state
    ) VALUES (
        target_generation_id, target_workspace_id, target_registration_id,
        next_ordinal, target_member_count, 'ready'
    );
    RETURN next_ordinal;
END
$$;

-- The connection admission policy's missing producer.
--
-- P03 built this policy's vocabulary and its consumer:
-- `ConnectionAdmissionPolicy` is declared in the domain and
-- `vestrace_try_admit_provider_dispatch` refuses without a current revision.
-- Nothing wrote one. No route, no service, no repository, and no guarded
-- function existed, so six test suites insert the rows with raw SQL as the
-- guarded owner because there is no other way, and both Run-step and embedding
-- dispatch are unreachable in production.
--
-- This is P03 subject matter completed inside P04 under an explicit operator
-- decision, recorded in the third scope amendment.
CREATE OR REPLACE FUNCTION vestrace_publish_connection_admission_policy(
    target_revision_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_expected_version BIGINT,
    target_max_in_flight SMALLINT,
    target_requests_per_60_seconds INTEGER,
    target_queue_wait_timeout_seconds INTEGER,
    target_provider_throttle_cap_seconds INTEGER
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    current_version BIGINT;
    next_version BIGINT;
BEGIN
    IF target_revision_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL
       OR target_expected_version IS NULL OR target_expected_version < 0 THEN
        RAISE EXCEPTION 'connection admission policy arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    -- Canonical lock order: the permanent connection execution guard first, as
    -- every Connection-scoped mutation must. This publishes a policy for a
    -- Connection; it is not a routing, qualification, or credential authority.
    PERFORM 1 FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'connection admission policy requires its connection execution guard'
            USING ERRCODE = '23514';
    END IF;

    SELECT version INTO current_version
      FROM connection_admission_policy_heads
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;

    IF NOT FOUND THEN
        -- Expected version zero means "there is no policy yet", the same
        -- convention the connection revision head uses.
        IF target_expected_version <> 0 THEN
            RAISE EXCEPTION 'connection admission policy head version conflict'
                USING ERRCODE = '40001';
        END IF;
        next_version := 1;
        INSERT INTO connection_admission_policy_revisions (
            id, workspace_id, connection_id, version, max_in_flight,
            requests_per_60_seconds, queue_wait_timeout_seconds,
            provider_throttle_cap_seconds
        ) VALUES (
            target_revision_id, target_workspace_id, target_connection_id, next_version,
            target_max_in_flight, target_requests_per_60_seconds,
            target_queue_wait_timeout_seconds, target_provider_throttle_cap_seconds
        );
        INSERT INTO connection_admission_policy_heads (
            workspace_id, connection_id, current_policy_revision_id, version
        ) VALUES (
            target_workspace_id, target_connection_id, target_revision_id, next_version
        );
        RETURN next_version;
    END IF;

    IF current_version <> target_expected_version THEN
        RAISE EXCEPTION 'connection admission policy head version conflict'
            USING ERRCODE = '40001';
    END IF;

    next_version := current_version + 1;
    INSERT INTO connection_admission_policy_revisions (
        id, workspace_id, connection_id, version, max_in_flight,
        requests_per_60_seconds, queue_wait_timeout_seconds,
        provider_throttle_cap_seconds
    ) VALUES (
        target_revision_id, target_workspace_id, target_connection_id, next_version,
        target_max_in_flight, target_requests_per_60_seconds,
        target_queue_wait_timeout_seconds, target_provider_throttle_cap_seconds
    );
    UPDATE connection_admission_policy_heads
       SET current_policy_revision_id = target_revision_id,
           version = next_version,
           updated_at = NOW()
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id;
    RETURN next_version;
END
$$;

-- Production upgrades run as the restricted runtime role and use the exact
-- bootstrap allowlist, which docker/postgres/init-runtime-role.sh extends for
-- this migration. The provisioner re-runs before the migrator on every start, so
-- an existing deployment acquires the extended allowlist before this executes.
-- SQLx fresh databases are provisioned by a superuser and never run the Compose
-- bootstrap, which is what the narrow fallback below is for.
DO $$
BEGIN
    PERFORM vestrace_assign_p03_table_owner('embedding_space_registrations'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('embedding_corpus_generations'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('embedding_jobs'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER TABLE embedding_space_registrations OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_corpus_generations OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_jobs OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON TABLE embedding_space_registrations FROM PUBLIC;
    REVOKE ALL ON TABLE embedding_corpus_generations FROM PUBLIC;
    REVOKE ALL ON TABLE embedding_jobs FROM PUBLIC;
    REVOKE ALL ON TABLE embedding_space_registrations FROM vestrace;
    REVOKE ALL ON TABLE embedding_corpus_generations FROM vestrace;
    REVOKE ALL ON TABLE embedding_jobs FROM vestrace;
    GRANT SELECT ON TABLE embedding_space_registrations TO vestrace;
    GRANT SELECT ON TABLE embedding_corpus_generations TO vestrace;
    GRANT SELECT ON TABLE embedding_jobs TO vestrace;
END
$$;

-- The two guarded tables this migration widens belong to
-- vestrace_guarded_owner, and `ALTER TABLE` requires ownership. The deployment
-- bootstrap hands them to the runtime migrator for exactly this migration; the
-- superuser branch is what lets `#[sqlx::test]` run the same file unchanged.
-- Ownership goes back to the guarded owner at the end of this file.
DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_p04_dispatch_cause_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_p04_dispatch_cause_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P04 dispatch-cause ownership hand-back must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
    END IF;
END
$$;

-- The third dispatch cause.
--
-- P03 closed both of these tables over `run_step` and `qualification_probe`.
-- Spec line 219 makes an embedding job a third owner of an external effect and
-- a `ModelRequestEvidence` identity, so the cause vocabulary has to admit it or
-- embedding dispatch would need its own admission and evidence tables -- which
-- is what "one dispatch authority, two callers" exists to prevent.
--
-- These are widenings, not rewrites: every existing row still satisfies the
-- constraints below, and the closed matrix stays closed. The embedding branch
-- names its job and its pinned snapshot and nothing else, exactly as the Run
-- branch names its run, step and snapshot.

ALTER TABLE provider_dispatch_causes
    ADD COLUMN embedding_job_id UUID;

ALTER TABLE provider_dispatch_causes
    DROP CONSTRAINT provider_dispatch_causes_cause_kind_check;
ALTER TABLE provider_dispatch_causes
    ADD CONSTRAINT provider_dispatch_causes_cause_kind_check
    CHECK (cause_kind IN ('run_step', 'qualification_probe', 'embedding_job'));

ALTER TABLE provider_dispatch_causes
    ADD CONSTRAINT provider_dispatch_causes_embedding_job_fkey
    FOREIGN KEY (workspace_id, embedding_job_id)
    REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT;

ALTER TABLE provider_dispatch_causes
    DROP CONSTRAINT provider_dispatch_causes_closed_matrix;
ALTER TABLE provider_dispatch_causes
    ADD CONSTRAINT provider_dispatch_causes_closed_matrix CHECK (
        (cause_kind = 'run_step'
         AND run_id IS NOT NULL
         AND step_id IS NOT NULL
         AND model_binding_snapshot_id IS NOT NULL
         AND embedding_job_id IS NULL
         AND qualification_job_id IS NULL
         AND qualification_target_binding_id IS NULL
         AND qualification_probe_ordinal IS NULL)
        OR
        (cause_kind = 'qualification_probe'
         AND run_id IS NULL
         AND step_id IS NULL
         AND model_binding_snapshot_id IS NULL
         AND embedding_job_id IS NULL
         AND qualification_job_id IS NOT NULL
         AND qualification_target_binding_id IS NOT NULL
         AND qualification_probe_ordinal IN (
             '00','10','15','20','30','35','40','50','60','70','80','90'
         ))
        OR
        -- An embedding job's cause is the job and the snapshot it pinned at
        -- acceptance. It has no run, no step and no probe: line 219 calls it
        -- "the durable non-Run owner for production Embeddings".
        (cause_kind = 'embedding_job'
         AND embedding_job_id IS NOT NULL
         AND model_binding_snapshot_id IS NOT NULL
         AND run_id IS NULL
         AND step_id IS NULL
         AND qualification_job_id IS NULL
         AND qualification_target_binding_id IS NULL
         AND qualification_probe_ordinal IS NULL)
    );

ALTER TABLE model_request_evidence_roots
    DROP CONSTRAINT model_request_evidence_roots_cause_kind_check;
ALTER TABLE model_request_evidence_roots
    ADD CONSTRAINT model_request_evidence_roots_cause_kind_check
    CHECK (cause_kind IN ('run_step', 'qualification_probe', 'embedding_job'));

ALTER TABLE model_request_evidence_roots
    DROP CONSTRAINT model_request_evidence_roots_cause_xor;
ALTER TABLE model_request_evidence_roots
    ADD CONSTRAINT model_request_evidence_roots_cause_xor CHECK (
        (cause_kind = 'run_step'
         AND binding_snapshot_id IS NOT NULL
         AND qualification_target_binding_id IS NULL)
        OR
        (cause_kind = 'qualification_probe'
         AND binding_snapshot_id IS NULL
         AND qualification_target_binding_id IS NOT NULL)
        OR
        -- Same shape as the Run branch: an embedding job's evidence is rooted
        -- in the immutable snapshot it pinned, and `cause_id` is the job.
        (cause_kind = 'embedding_job'
         AND binding_snapshot_id IS NOT NULL
         AND qualification_target_binding_id IS NULL)
    );

-- Hand the two widened tables straight back. The migrator owned them for this
-- file only, and nothing after this point may write them as anything but the
-- guarded owner.
DO $$
BEGIN
    PERFORM vestrace_assign_p03_table_owner('provider_dispatch_causes'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('model_request_evidence_roots'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER TABLE provider_dispatch_causes OWNER TO vestrace_guarded_owner;
    ALTER TABLE model_request_evidence_roots OWNER TO vestrace_guarded_owner;
END
$$;

-- Accept one embedding job under the canonical lock order.
--
-- Spec line 219: an ordinary job "owns its snapshot resolved from the current
-- tuple at acceptance and never changed thereafter", and every pre-dispatch
-- transaction shares "ConnectionExecutionGuard-first ordering". Line 189's
-- canonical order continues: ConnectionExecutionGuard, the optional exact
-- CredentialActivationGuard, then the exact (workspace, EmbeddingSpaceKey)
-- guard, then the job. This function takes them in that order and no other,
-- because a second lock order between two callers of one dispatch authority is
-- a deadlock waiting for load.
--
-- It resolves nothing. The caller states the snapshot, the effect identity and
-- the evidence identity; this function proves they are consistent and makes the
-- row durable. A function that looked up "the current snapshot" would be
-- routing, which the worker is not allowed to do.
CREATE OR REPLACE FUNCTION vestrace_accept_embedding_job(
    target_job_id UUID,
    target_workspace_id UUID,
    target_space_registration_id UUID,
    target_kind TEXT,
    target_snapshot_id UUID,
    target_external_effect_id UUID,
    target_model_request_evidence_id UUID,
    target_retries_unknown_embedding_job_id UUID,
    target_expected_predecessor_version BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    snapshot RECORD;
    guard_id UUID;
    existing embedding_jobs%ROWTYPE;
BEGIN
    IF target_job_id IS NULL OR target_workspace_id IS NULL
       OR target_space_registration_id IS NULL OR target_kind IS NULL
       OR target_snapshot_id IS NULL OR target_external_effect_id IS NULL
       OR target_model_request_evidence_id IS NULL
       OR target_kind NOT IN ('retrieval_query', 'delivery', 'rebuild')
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding job acceptance arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    -- The pinned snapshot names the Connection. Nothing else may.
    SELECT id, connection_id, branch, credential_slot_id,
           credential_activation_guard_id
      INTO snapshot
      FROM model_binding_snapshots
     WHERE workspace_id = target_workspace_id AND id = target_snapshot_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job requires its exact pinned binding snapshot'
            USING ERRCODE = '23514';
    END IF;

    -- 1. ConnectionExecutionGuard.
    SELECT id INTO guard_id
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = snapshot.connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job requires its existing permanent connection guard'
            USING ERRCODE = '23514';
    END IF;

    -- 2. The optional exact CredentialActivationGuard. Line 219: the snapshot
    -- "structurally pins exactly one auth branch ... never both or neither", so
    -- a credential branch missing its slot or guard is a refusal rather than a
    -- quiet fall through to the no-auth path.
    IF snapshot.branch = 'credential' THEN
        IF snapshot.credential_slot_id IS NULL
           OR snapshot.credential_activation_guard_id IS NULL THEN
            RAISE EXCEPTION 'embedding job credential branch is not exact'
                USING ERRCODE = '23514';
        END IF;
        PERFORM 1
          FROM credential_activation_guards
         WHERE workspace_id = target_workspace_id
           AND connection_id = snapshot.connection_id
           AND credential_slot_id = snapshot.credential_slot_id
           AND id = snapshot.credential_activation_guard_id
           AND execution_guard_id = guard_id
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job credential branch requires its exact activation guard'
                USING ERRCODE = '23514';
        END IF;
    ELSIF snapshot.credential_slot_id IS NOT NULL
          OR snapshot.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding job no-auth branch cannot reference a credential'
            USING ERRCODE = '23514';
    END IF;

    -- 3. The exact (workspace, EmbeddingSpaceKey) guard, which is the
    -- registration row: the key is unique on the whole tuple, so locking the
    -- registration locks the key.
    PERFORM 1 FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id AND id = target_space_registration_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job requires its authorized embedding space'
            USING ERRCODE = '23514';
    END IF;

    -- 4. The job itself. Convergence is by identity: a replay that names the
    -- same job with the same tuple gets the same row, and one that names it with
    -- a different tuple is a conflict rather than a silent second acceptance.
    SELECT * INTO existing FROM embedding_jobs
     WHERE workspace_id = target_workspace_id AND id = target_job_id
     FOR UPDATE;
    IF FOUND THEN
        IF existing.space_registration_id = target_space_registration_id
           AND existing.kind = target_kind
           AND existing.model_binding_snapshot_id = target_snapshot_id
           AND existing.external_effect_id = target_external_effect_id
           AND existing.model_request_evidence_id = target_model_request_evidence_id
           AND existing.retries_unknown_embedding_job_id
               IS NOT DISTINCT FROM target_retries_unknown_embedding_job_id
           AND (
                target_retries_unknown_embedding_job_id IS NULL
                OR EXISTS (
                    SELECT 1 FROM embedding_jobs AS predecessor
                     WHERE predecessor.workspace_id = target_workspace_id
                       AND predecessor.id = target_retries_unknown_embedding_job_id
                       AND predecessor.version = target_expected_predecessor_version
                )
           ) THEN
            RETURN existing.id;
        END IF;
        RAISE EXCEPTION 'embedding job identity conflicts with its existing tuple'
            USING ERRCODE = '23514';
    END IF;

    -- Line 257: a successor may only retry a terminal InconclusiveUnknown
    -- predecessor. Checked here rather than by the caller, because the caller
    -- that wanted to retry a running job is exactly the caller that would skip
    -- the check.
    IF target_retries_unknown_embedding_job_id IS NOT NULL THEN
        IF target_expected_predecessor_version IS NULL THEN
            RAISE EXCEPTION 'embedding job successor must name the predecessor version it read'
                USING ERRCODE = '22023';
        END IF;
        PERFORM 1 FROM embedding_jobs
         WHERE workspace_id = target_workspace_id
           AND id = target_retries_unknown_embedding_job_id
           AND kind = target_kind
           AND state = 'inconclusive_unknown'
           AND version = target_expected_predecessor_version
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job successor requires a terminal inconclusive predecessor at the version it read'
                USING ERRCODE = '23514';
        END IF;
    ELSIF target_expected_predecessor_version IS NOT NULL THEN
        RAISE EXCEPTION 'embedding job without a predecessor cannot name a predecessor version'
            USING ERRCODE = '22023';
    END IF;

    INSERT INTO embedding_jobs (
        id, workspace_id, space_registration_id, kind, model_binding_snapshot_id,
        external_effect_id, model_request_evidence_id,
        retries_unknown_embedding_job_id
    ) VALUES (
        target_job_id, target_workspace_id, target_space_registration_id, target_kind,
        target_snapshot_id, target_external_effect_id, target_model_request_evidence_id,
        target_retries_unknown_embedding_job_id
    );
    RETURN target_job_id;
END
$$;

-- Finalize an embedding job whose effect ended ambiguously.
--
-- Spec line 245: "Loss after `Dispatching` remains deadline-gated; one recovery
-- winner leaves the effect `Unknown` and finalizes the job
-- `InconclusiveUnknown`." The shared recovery winner appends the effect
-- outcome; this appends the job's terminal state and nothing else. It publishes
-- no result, releases no admission, and never calls an adapter -- there is
-- nothing to call, because whether the provider ran is precisely what is
-- unknown.
--
-- It is idempotent by design. A crash between the effect outcome and the job
-- outcome is recovered by running this again, which is what makes "recovered
-- from the immutable effect outcome without another adapter call" true rather
-- than hoped for.
CREATE OR REPLACE FUNCTION vestrace_finalize_embedding_job_unknown(
    target_workspace_id UUID,
    target_job_id UUID
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    initial embedding_jobs%ROWTYPE;
    job embedding_jobs%ROWTYPE;
    root_connection_id UUID;
BEGIN
    IF target_workspace_id IS NULL OR target_job_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding job unknown finalization arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT stored.* INTO initial FROM embedding_jobs AS stored
     WHERE stored.workspace_id = target_workspace_id AND stored.id = target_job_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job is absent' USING ERRCODE = '23514';
    END IF;

    -- Canonical order: the permanent connection guard, reached through the
    -- admission this job was dispatched under, before the job row.
    SELECT admission.connection_id INTO root_connection_id
      FROM connection_dispatch_admissions AS admission
     WHERE admission.workspace_id = target_workspace_id
       AND admission.external_effect_id = initial.external_effect_id
       AND admission.decision = 'admitted'
       AND admission.model_binding_snapshot_id = initial.model_binding_snapshot_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job unknown finalization requires its admitted dispatch authority'
            USING ERRCODE = '23514';
    END IF;
    PERFORM guard.id FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = root_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job unknown finalization requires its permanent connection guard'
            USING ERRCODE = '23514';
    END IF;

    SELECT stored.* INTO job FROM embedding_jobs AS stored
     WHERE stored.workspace_id = target_workspace_id
       AND stored.id = initial.id
       AND stored.external_effect_id = initial.external_effect_id
     FOR UPDATE OF stored;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job identity changed before its unknown finalization'
            USING ERRCODE = '23514';
    END IF;

    -- The effect's own outcome is the authority. Without it this function would
    -- be deciding that a provider call was ambiguous, which is the recovery
    -- winner's decision and not this one's.
    IF NOT EXISTS (
        SELECT 1 FROM external_effect_lifecycle_transitions AS lifecycle
         WHERE lifecycle.workspace_id = target_workspace_id
           AND lifecycle.effect_id = job.external_effect_id
           AND lifecycle.status = 'unknown'
    ) THEN
        RAISE EXCEPTION 'embedding job unknown finalization requires its unknown effect outcome'
            USING ERRCODE = '23514';
    END IF;

    IF job.state = 'inconclusive_unknown' THEN
        RETURN job.version;
    END IF;
    IF job.state NOT IN ('requested', 'running') THEN
        RAISE EXCEPTION 'embedding job is already terminal and cannot become inconclusive'
            USING ERRCODE = '23514';
    END IF;

    UPDATE embedding_jobs
       SET state = 'inconclusive_unknown', version = version + 1
     WHERE workspace_id = target_workspace_id AND id = job.id;
    RETURN job.version + 1;
END
$$;

-- Lock one embedding job and report the phase its own evidence proves.
--
-- The Run-step twin reads a persisted `phase` column on
-- `run_step_execution_attempts`. This one derives the phase instead, and the
-- difference is deliberate. Spec line 219 gives an embedding job six append-only
-- states, `Requested -> Running -> Succeeded | FailedDefinite |
-- InconclusiveUnknown | Cancelled`; `dispatching`, `waiting` and
-- `result_prepared` are the external effect's, the attempt's pre-dispatch
-- phase's, and the result marker's respectively. A `phase` column on
-- `embedding_jobs` would be a second, independently writable answer to whether
-- the provider has been reached, and the job would then be able to disagree
-- with the effect about a possible charge.
--
-- So the answer comes from the evidence: the admitted admission, the dispatch
-- transition, the receipt, the result preparation, and the job's own terminal
-- state. Nothing here writes.
CREATE OR REPLACE FUNCTION vestrace_lock_embedding_job_recovery_authority(
    target_workspace_id UUID,
    target_job_id UUID
)
RETURNS TABLE (
    job_id UUID,
    workspace_id UUID,
    space_registration_id UUID,
    kind TEXT,
    state TEXT,
    model_binding_snapshot_id UUID,
    external_effect_id UUID,
    model_request_evidence_id UUID,
    phase TEXT,
    connection_id UUID,
    connection_revision_id UUID,
    dispatch_transition_id UUID,
    dispatch_expires_at TIMESTAMPTZ,
    has_receipt BOOLEAN,
    has_result_preparation BOOLEAN
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    initial embedding_jobs%ROWTYPE;
    job embedding_jobs%ROWTYPE;
    admission RECORD;
    root_connection_id UUID := NULL;
    canonical_connection_id UUID := NULL;
    canonical_connection_revision_id UUID := NULL;
    canonical_dispatch_transition_id UUID := NULL;
    canonical_dispatch_expires_at TIMESTAMPTZ := NULL;
    receipt_exists BOOLEAN;
    preparation_exists BOOLEAN;
    derived_phase TEXT;
BEGIN
    IF target_workspace_id IS NULL OR target_job_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding job recovery authority arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT stored.* INTO initial
      FROM embedding_jobs AS stored
     WHERE stored.workspace_id = target_workspace_id AND stored.id = target_job_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job is absent' USING ERRCODE = '23514';
    END IF;

    -- The admitted admission is what makes a job post-admission. Its absence is
    -- the pre-dispatch case and is locked directly, exactly as the Run-step twin
    -- locks a `reserved` attempt without first waiting on a connection guard it
    -- has no admission for.
    SELECT stored.connection_id, stored.connection_revision_id
      INTO admission
      FROM connection_dispatch_admissions AS stored
     WHERE stored.workspace_id = target_workspace_id
       AND stored.external_effect_id = initial.external_effect_id
       AND stored.decision = 'admitted'
       AND stored.model_binding_snapshot_id = initial.model_binding_snapshot_id;
    IF FOUND THEN
        root_connection_id := admission.connection_id;
        canonical_connection_id := admission.connection_id;
        canonical_connection_revision_id := admission.connection_revision_id;
        -- Canonical lock order: the permanent connection guard before the job.
        PERFORM guard.id
          FROM connection_execution_guards AS guard
         WHERE guard.workspace_id = target_workspace_id
           AND guard.connection_id = root_connection_id
         FOR UPDATE OF guard;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job recovery requires its permanent connection guard'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    SELECT stored.* INTO job
      FROM embedding_jobs AS stored
     WHERE stored.workspace_id = target_workspace_id
       AND stored.id = initial.id
       AND stored.model_binding_snapshot_id = initial.model_binding_snapshot_id
       AND stored.external_effect_id = initial.external_effect_id
       AND stored.model_request_evidence_id = initial.model_request_evidence_id
     FOR UPDATE OF stored;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job identity changed before its recovery lock'
            USING ERRCODE = '23514';
    END IF;

    SELECT lifecycle.id, lifecycle.dispatch_expires_at
      INTO canonical_dispatch_transition_id, canonical_dispatch_expires_at
      FROM external_effect_lifecycle_transitions AS lifecycle
     WHERE lifecycle.workspace_id = target_workspace_id
       AND lifecycle.effect_id = job.external_effect_id
       AND lifecycle.status = 'dispatching'
       AND lifecycle.cause = 'dispatch_started'
       AND lifecycle.dispatch_expires_at IS NOT NULL
     ORDER BY lifecycle.ordinal DESC
     LIMIT 1
     FOR KEY SHARE OF lifecycle;

    receipt_exists := EXISTS (
        SELECT 1 FROM external_effect_receipts AS receipt
         WHERE receipt.workspace_id = job.workspace_id
           AND receipt.effect_id = job.external_effect_id
    );
    preparation_exists := EXISTS (
        SELECT 1 FROM provider_result_preparations AS preparation
         WHERE preparation.workspace_id = job.workspace_id
           AND preparation.external_effect_id = job.external_effect_id
    );

    -- The order of these tests is the order of the evidence, most durable
    -- first. A job that is terminal says so before anything else, because no
    -- scheduler may advance it whatever the effect looks like.
    IF job.state = 'succeeded' THEN
        derived_phase := 'succeeded';
    ELSIF job.state = 'inconclusive_unknown' THEN
        derived_phase := 'unknown';
    ELSIF receipt_exists AND preparation_exists THEN
        derived_phase := 'result_prepared';
    ELSIF canonical_dispatch_transition_id IS NOT NULL THEN
        derived_phase := 'dispatching';
    ELSIF root_connection_id IS NOT NULL THEN
        derived_phase := 'admitted';
    ELSE
        derived_phase := 'reserved';
    END IF;

    RETURN QUERY
    SELECT job.id,
           job.workspace_id,
           job.space_registration_id,
           job.kind,
           job.state,
           job.model_binding_snapshot_id,
           job.external_effect_id,
           job.model_request_evidence_id,
           derived_phase,
           canonical_connection_id,
           canonical_connection_revision_id,
           canonical_dispatch_transition_id,
           canonical_dispatch_expires_at,
           receipt_exists,
           preparation_exists;
END
$$;

-- Routing for a third dispatch cause.
--
-- This is a whole-body replacement of the function 0185 last replaced, with the
-- same signature and one branch added. 0184 and 0185 are historical and
-- immutable, so a forward migration cannot patch them; P03 did exactly this in
-- 0185 when qualification probes became a second cause, and eleven guarded
-- functions in this schema are already defined in more than one migration.
--
-- Everything outside the new branch is 0185's body unchanged, including the two
-- places that let a `candidate` credential intent route. Those key on
-- `target_cause_kind = 'qualification_probe'`, so an embedding job -- a
-- production path -- reaches the provider only through an `active` intent, and
-- gains no leniency by being added here.
--
-- The embedding branch takes no new parameter. `embedding_jobs.external_effect_
-- id` is unique, so the job is identified by the effect id and pinned snapshot
-- the caller already states; deriving it from its own effect proves consistency
-- rather than resolving a choice the worker is not allowed to make.
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
          OR
          (target_cause_kind = 'embedding_job'
           AND target_run_id IS NULL AND target_step_id IS NULL
           AND target_snapshot_id IS NOT NULL
           AND target_qualification_job_id IS NULL
           AND target_qualification_target_id IS NULL)
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
           AND (
               intent.state = 'active'
               OR (target_cause_kind = 'qualification_probe'
                   AND intent.state = 'candidate')
           )
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
           AND snapshot.connection_revision_id = target_connection_revision_id;
    ELSIF target_cause_kind = 'embedding_job' THEN
        -- The same shape as the Run branch, with the job standing where the run
        -- and step stand. The job's own effect identifies it, its pinned
        -- snapshot must be the one the caller named, and the evidence root must
        -- name the job as its cause. Nothing here can select a Connection the
        -- job did not pin at acceptance.
        SELECT snapshot.branch, snapshot.credential_revision_id,
               snapshot.credential_slot_id,
               snapshot.credential_activation_guard_id,
               snapshot.expected_slot_version,
               snapshot.no_auth_binding_revision_id,
               revision.runtime_base_url, revision.auth_mode,
               revision.credential_slot_id AS revision_credential_slot_id
          INTO routing
          FROM embedding_jobs AS job
          JOIN model_binding_snapshots AS snapshot
            ON snapshot.workspace_id = job.workspace_id
           AND snapshot.id = job.model_binding_snapshot_id
          JOIN connection_revisions AS revision
            ON revision.workspace_id = snapshot.workspace_id
           AND revision.connection_id = snapshot.connection_id
           AND revision.id = snapshot.connection_revision_id
          JOIN model_request_evidence_roots AS evidence
            ON evidence.workspace_id = snapshot.workspace_id
           AND evidence.id = target_evidence_id
           AND evidence.external_effect_id = target_effect_id
           AND evidence.cause_kind = 'embedding_job'
           AND evidence.cause_id = job.id
           AND evidence.binding_snapshot_id = snapshot.id
         WHERE job.workspace_id = target_workspace_id
           AND job.external_effect_id = target_effect_id
           AND job.model_binding_snapshot_id = target_snapshot_id
           AND job.model_request_evidence_id = target_evidence_id
           AND snapshot.connection_id = target_connection_id
           AND snapshot.connection_revision_id = target_connection_revision_id;
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
           AND binding.connection_revision_id = target_connection_revision_id;
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
           AND (
               intent.state = 'active'
               OR (target_cause_kind = 'qualification_probe'
                   AND intent.state = 'candidate')
           );
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

-- Admission for a third dispatch cause.
--
-- A whole-body replacement of the function 0184 defines, with the same
-- signature. 0184 is historical and immutable, so a forward migration cannot
-- patch it; this follows the pattern P03 used in 0185 for
-- `vestrace_lock_provider_dispatch_routing` when qualification probes became a
-- second cause. It is emphatically not a second admission function -- the plan
-- names that as a defect -- and the five guarded functions an embedding
-- dispatch calls remain exactly the five a Run step calls.
--
-- This body was derived from 0184's by seven anchored substitutions rather than
-- retyped, and the diff against the original is the review surface. Every line
-- outside those seven is byte-identical.
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
    -- Derived, never supplied. `embedding_jobs.external_effect_id` is unique, so
    -- the job is a consequence of the effect the caller already named rather
    -- than a second thing the caller gets to choose.
    resolved_embedding_job_id UUID := NULL;
BEGIN
    IF target_admission_id IS NULL OR target_wait_id IS NULL
       OR target_concurrency_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_effect_id IS NULL OR target_model_request_evidence_id IS NULL
       OR target_dispatch_ttl_seconds IS NULL
       OR target_dispatch_ttl_seconds NOT BETWEEN 1 AND 900
       OR target_cause_kind IS NULL
       OR target_cause_kind NOT IN ('run_step', 'qualification_probe', 'embedding_job') THEN
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
        OR
        (target_cause_kind = 'embedding_job'
         AND target_run_id IS NULL
         AND target_step_id IS NULL
         AND target_snapshot_id IS NOT NULL
         AND target_qualification_job_id IS NULL
         AND target_qualification_target_id IS NULL
         AND target_qualification_probe_ordinal IS NULL)
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

    -- The embedding equivalent of the run/snapshot mapping check above. The job
    -- is found by its own effect, and the snapshot it pinned at acceptance must
    -- be the one the caller named: line 219 says an ordinary job "owns its
    -- snapshot resolved from the current tuple at acceptance and never changed
    -- thereafter", so a dispatch naming a different snapshot is not a late
    -- resolution but a contradiction.
    IF target_cause_kind = 'embedding_job' THEN
        SELECT job.id INTO resolved_embedding_job_id
          FROM embedding_jobs AS job
         WHERE job.workspace_id = target_workspace_id
           AND job.external_effect_id = target_effect_id
           AND job.model_binding_snapshot_id = target_snapshot_id
           AND job.model_request_evidence_id = target_model_request_evidence_id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'provider dispatch embedding job and snapshot mapping is not exact'
                USING ERRCODE = '23514';
        END IF;
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
           target_cause_kind = 'embedding_job'
           AND (
               evidence_root.cause_id IS DISTINCT FROM resolved_embedding_job_id
               OR evidence_root.binding_snapshot_id IS DISTINCT FROM target_snapshot_id
               OR evidence_root.qualification_target_binding_id IS NOT NULL
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
        qualification_target_binding_id, qualification_probe_ordinal,
        embedding_job_id
    ) VALUES (
        target_effect_id, target_workspace_id, target_model_request_evidence_id,
        latest_check_id, target_cause_kind, target_run_id, target_step_id,
        target_snapshot_id, target_qualification_job_id,
        target_qualification_target_id, target_qualification_probe_ordinal,
        resolved_embedding_job_id
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
       OR existing_cause.qualification_probe_ordinal IS DISTINCT FROM target_qualification_probe_ordinal
       OR existing_cause.embedding_job_id IS DISTINCT FROM resolved_embedding_job_id THEN
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

-- Reconstruction for a third dispatch cause.
--
-- Whole-body replacement of 0184's function with the same signature, found by
-- scanning every migration for a function body that closes over both
-- `'run_step'` and `'qualification_probe'`. That search returned four; the other
-- three are the admission, the routing lock, and the evidence constructor.
-- Widening them one failing test at a time would have been three more rounds of
-- the same diagnosis.
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
        -- The same shape as the Run branch: an embedding job's evidence is
        -- rooted in the immutable snapshot it pinned at acceptance, and its
        -- cause is the job. Nothing about reconstruction differs by owner.
        (evidence_root.cause_kind = 'embedding_job'
         AND evidence_root.binding_snapshot_id IS NOT NULL
         AND evidence_root.qualification_target_binding_id IS NULL
         AND EXISTS (
             SELECT 1 FROM model_request_evidence_nodes AS node
              WHERE node.workspace_id = target_workspace_id
                AND node.evidence_root_id = target_evidence_id
                AND node.reference_kind = 'binding_snapshot'
                AND node.reference_id = evidence_root.binding_snapshot_id
         )
         AND EXISTS (
             SELECT 1 FROM embedding_jobs AS job
              WHERE job.workspace_id = target_workspace_id
                AND job.id = evidence_root.cause_id
                AND job.external_effect_id = evidence_root.external_effect_id
                AND job.model_binding_snapshot_id = evidence_root.binding_snapshot_id
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

-- Evidence creation for a third dispatch cause.
--
-- Whole-body replacement of 0183's function with the same signature. The Run
-- branch already required a snapshot-rooted tuple with no qualification nodes,
-- which is exactly what an embedding job's evidence is, so the branch is shared
-- rather than copied and its two refusal messages now say `snapshot-rooted`
-- instead of `run-step`. The embedding-specific claim -- that the cause is the
-- job owning this effect and this snapshot -- is checked separately, because it
-- is the one thing the Run branch cannot state for it.
CREATE OR REPLACE FUNCTION vestrace_create_model_request_evidence(
    target_root_id UUID,
    target_workspace_id UUID,
    target_external_effect_id UUID,
    target_request_kind TEXT,
    target_binding_snapshot_id UUID,
    target_qualification_binding_id UUID,
    target_cause_kind TEXT,
    target_cause_id UUID,
    node_kinds TEXT[],
    node_ids UUID[],
    node_versions BIGINT[],
    node_safe_ordinals TEXT[]
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_root model_request_evidence_roots%ROWTYPE;
    node_count INTEGER;
    node_index INTEGER;
    snapshot_row model_binding_snapshots%ROWTYPE;
    target_row qualification_target_bindings%ROWTYPE;
    model_kind TEXT;
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model request evidence workspace mismatch' USING ERRCODE = '42501';
    END IF;
    node_count := cardinality(node_kinds);
    IF node_count IS NULL OR node_count = 0 OR node_count > 8200
       OR cardinality(node_ids) <> node_count
       OR cardinality(node_versions) <> node_count
       OR cardinality(node_safe_ordinals) <> node_count THEN
        RAISE EXCEPTION 'model request evidence node arrays are malformed' USING ERRCODE = '22023';
    END IF;
    IF target_request_kind NOT IN ('models_list', 'chat_completions', 'embeddings')
       OR target_cause_kind NOT IN ('run_step', 'qualification_probe', 'embedding_job') THEN
        RAISE EXCEPTION 'model request evidence kind is malformed' USING ERRCODE = '22023';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended(target_external_effect_id::TEXT, 0));
    SELECT * INTO existing_root FROM model_request_evidence_roots
     WHERE external_effect_id = target_external_effect_id;
    IF FOUND THEN
        IF existing_root.workspace_id = target_workspace_id
           AND existing_root.request_kind = target_request_kind
           AND existing_root.binding_snapshot_id IS NOT DISTINCT FROM target_binding_snapshot_id
           AND existing_root.qualification_target_binding_id IS NOT DISTINCT FROM target_qualification_binding_id
           AND existing_root.cause_kind = target_cause_kind
           AND existing_root.cause_id = target_cause_id
           AND (SELECT array_agg(reference_kind ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) = node_kinds
           AND (SELECT array_agg(reference_id ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) = node_ids
           AND (SELECT array_agg(reference_version ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) IS NOT DISTINCT FROM node_versions
           AND (SELECT array_agg(safe_ordinal ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) IS NOT DISTINCT FROM node_safe_ordinals THEN
            RETURN existing_root.id;
        END IF;
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    IF EXISTS (SELECT 1 FROM model_request_evidence_roots WHERE id = target_root_id) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM external_effect_intents
         WHERE id = target_external_effect_id AND workspace_id = target_workspace_id
    ) THEN
        RAISE EXCEPTION 'model request evidence effect is absent or cross-workspace' USING ERRCODE = '23514';
    END IF;
    IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'external_effect') <> 1
       OR node_ids[array_position(node_kinds, 'external_effect')] <> target_external_effect_id
       OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'connection_revision') <> 1
       OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'request_shape_revision') <> 1
       OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'limits_revision') <> 1 THEN
        RAISE EXCEPTION 'model request evidence is missing an exact root node' USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM connection_revisions
         WHERE id = node_ids[array_position(node_kinds, 'connection_revision')]
           AND workspace_id = target_workspace_id
    ) THEN
        RAISE EXCEPTION 'model request evidence connection revision is absent or cross-workspace' USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM model_request_shape_revisions
         WHERE id = node_ids[array_position(node_kinds, 'request_shape_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'request_shape_revision')]
           AND request_kind = target_request_kind
    ) OR NOT EXISTS (
        SELECT 1 FROM model_limits_revisions
         WHERE id = node_ids[array_position(node_kinds, 'limits_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'limits_revision')]
    ) THEN
        RAISE EXCEPTION 'model request evidence canonical source version is absent or wrong' USING ERRCODE = '23514';
    END IF;
    IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') >
       (SELECT max_inputs FROM model_limits_revisions
         WHERE id = node_ids[array_position(node_kinds, 'limits_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'limits_revision')]) THEN
        RAISE EXCEPTION 'model request evidence input count exceeds its pinned limit' USING ERRCODE = '23514';
    END IF;

    IF target_request_kind = 'models_list' THEN
        IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN (
            'model_revision', 'sampling_revision', 'tool_schema_revision', 'governed_input_material'
        )) <> 0 THEN
            RAISE EXCEPTION 'models-list evidence contains a forbidden node' USING ERRCODE = '23514';
        END IF;
    ELSIF target_request_kind = 'chat_completions' THEN
        IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'sampling_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') < 1 THEN
            RAISE EXCEPTION 'chat evidence node matrix is incomplete' USING ERRCODE = '23514';
        END IF;
    ELSE
        IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') < 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN ('sampling_revision', 'tool_schema_revision')) <> 0 THEN
            RAISE EXCEPTION 'embeddings evidence node matrix is incomplete or contains a forbidden node' USING ERRCODE = '23514';
        END IF;
    END IF;
    IF target_request_kind = 'chat_completions' AND (
        SELECT cardinality(input_roles)
          FROM model_request_shape_revisions
         WHERE id = node_ids[array_position(node_kinds, 'request_shape_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'request_shape_revision')]
    ) <> (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') THEN
        RAISE EXCEPTION 'chat evidence roles do not match its ordered governed inputs'
            USING ERRCODE = '23514';
    END IF;

    IF target_request_kind <> 'models_list' THEN
        SELECT kind INTO model_kind FROM model_revisions
         WHERE id = node_ids[array_position(node_kinds, 'model_revision')]
           AND workspace_id = target_workspace_id
           AND connection_revision_id = node_ids[array_position(node_kinds, 'connection_revision')];
        IF model_kind IS NULL
           OR (target_request_kind = 'chat_completions' AND model_kind <> 'chat')
           OR (target_request_kind = 'embeddings' AND model_kind <> 'embedding') THEN
            RAISE EXCEPTION 'model request evidence model kind is incompatible' USING ERRCODE = '23514';
        END IF;
    END IF;
    IF target_request_kind = 'chat_completions' AND NOT EXISTS (
        SELECT 1 FROM model_sampling_revisions
         WHERE id = node_ids[array_position(node_kinds, 'sampling_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'sampling_revision')]
    ) THEN
        RAISE EXCEPTION 'model request evidence sampling version is absent or wrong' USING ERRCODE = '23514';
    END IF;
    FOR node_index IN 1..node_count LOOP
        IF node_kinds[node_index] IN (
            'request_shape_revision', 'sampling_revision', 'limits_revision', 'tool_schema_revision'
        ) AND (node_versions[node_index] IS NULL OR node_versions[node_index] < 1) THEN
            RAISE EXCEPTION 'model request evidence source node lacks an exact version' USING ERRCODE = '23514';
        ELSIF node_kinds[node_index] NOT IN (
            'request_shape_revision', 'sampling_revision', 'limits_revision', 'tool_schema_revision'
        ) AND node_versions[node_index] IS NOT NULL THEN
            RAISE EXCEPTION 'model request evidence nonversioned node carries a version' USING ERRCODE = '23514';
        END IF;
        IF node_kinds[node_index] IN ('governed_input_material', 'tool_schema_revision') THEN
            IF node_safe_ordinals[node_index] IS NULL
               OR node_safe_ordinals[node_index] !~ '^(0|[1-9][0-9]*)$'
               OR node_safe_ordinals[node_index]::INTEGER <> (
                   SELECT count(*) - 1 FROM generate_subscripts(node_kinds, 1) AS prior
                    WHERE prior <= node_index AND node_kinds[prior] = node_kinds[node_index]
               ) THEN
                RAISE EXCEPTION 'model request evidence ordered node ordinal is non-contiguous' USING ERRCODE = '23514';
            END IF;
        ELSIF node_kinds[node_index] <> 'qualification_probe'
              AND node_safe_ordinals[node_index] IS NOT NULL THEN
            RAISE EXCEPTION 'model request evidence unordered node carries a safe ordinal' USING ERRCODE = '23514';
        END IF;
        IF node_kinds[node_index] = 'tool_schema_revision' AND NOT EXISTS (
            SELECT 1 FROM model_tool_schema_revisions
             WHERE id = node_ids[node_index] AND workspace_id = target_workspace_id
               AND version = node_versions[node_index]
        ) THEN
            RAISE EXCEPTION 'model request evidence tool version is absent or wrong' USING ERRCODE = '23514';
        ELSIF node_kinds[node_index] = 'governed_input_material' AND NOT EXISTS (
            SELECT 1 FROM content_materials
             WHERE id = node_ids[node_index] AND workspace_id = target_workspace_id
        ) THEN
            RAISE EXCEPTION 'model request evidence input is absent or cross-workspace' USING ERRCODE = '23514';
        END IF;
    END LOOP;

    IF target_cause_kind IN ('run_step', 'embedding_job') THEN
        IF target_request_kind = 'models_list'
           OR target_binding_snapshot_id IS NULL OR target_qualification_binding_id IS NOT NULL
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'binding_snapshot') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'connection_qualification_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_qualification_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN ('qualification_target', 'qualification_probe')) <> 0 THEN
            RAISE EXCEPTION 'snapshot-rooted evidence cause matrix is invalid'
                USING ERRCODE = '23514';
        END IF;
        SELECT * INTO snapshot_row FROM model_binding_snapshots
         WHERE id = target_binding_snapshot_id AND workspace_id = target_workspace_id;
        IF NOT FOUND
           OR snapshot_row.connection_revision_id <> node_ids[array_position(node_kinds, 'connection_revision')]
           OR snapshot_row.connection_qualification_revision_id <> node_ids[array_position(node_kinds, 'connection_qualification_revision')]
           OR snapshot_row.model_revision_id <> node_ids[array_position(node_kinds, 'model_revision')]
           OR snapshot_row.model_qualification_revision_id <> node_ids[array_position(node_kinds, 'model_qualification_revision')]
           OR node_ids[array_position(node_kinds, 'binding_snapshot')] <> target_binding_snapshot_id THEN
            RAISE EXCEPTION 'snapshot-rooted evidence tuple is incompatible'
                USING ERRCODE = '23514';
        END IF;
        -- The Run branch proves its cause is a step of the named run. The
        -- embedding branch proves its cause is the job that owns this exact
        -- effect and pinned this exact snapshot, which is the same claim for a
        -- non-Run owner.
        IF target_cause_kind = 'embedding_job' AND NOT EXISTS (
            SELECT 1 FROM embedding_jobs AS job
             WHERE job.workspace_id = target_workspace_id
               AND job.id = target_cause_id
               AND job.external_effect_id = target_external_effect_id
               AND job.model_binding_snapshot_id = target_binding_snapshot_id
        ) THEN
            RAISE EXCEPTION 'embedding-job evidence tuple is incompatible'
                USING ERRCODE = '23514';
        END IF;
    ELSE
        IF target_binding_snapshot_id IS NOT NULL OR target_qualification_binding_id IS NULL
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'qualification_target') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'qualification_probe') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN (
               'binding_snapshot', 'connection_qualification_revision', 'model_qualification_revision'
           )) <> 0 THEN
            RAISE EXCEPTION 'qualification-probe evidence cause matrix is invalid' USING ERRCODE = '23514';
        END IF;
        SELECT * INTO target_row FROM qualification_target_bindings
         WHERE id = target_qualification_binding_id AND workspace_id = target_workspace_id;
        IF NOT FOUND
           OR target_row.connection_revision_id <> node_ids[array_position(node_kinds, 'connection_revision')]
           OR target_row.qualification_job_id <> target_cause_id
           OR node_ids[array_position(node_kinds, 'qualification_target')] <> target_qualification_binding_id
           OR node_ids[array_position(node_kinds, 'qualification_probe')] <> target_cause_id
           OR node_safe_ordinals[array_position(node_kinds, 'qualification_probe')] NOT IN (
               '00','10','15','20','30','35','40','50','60','70','80','90'
           ) THEN
            RAISE EXCEPTION 'qualification-probe evidence tuple is incompatible' USING ERRCODE = '23514';
        END IF;
    END IF;

    IF EXISTS (
        SELECT 1 FROM generate_subscripts(node_kinds, 1) AS left_index
        JOIN generate_subscripts(node_kinds, 1) AS right_index
          ON left_index < right_index
         AND node_kinds[left_index] = node_kinds[right_index]
         AND node_ids[left_index] = node_ids[right_index]
    ) THEN
        RAISE EXCEPTION 'model request evidence contains a duplicate node' USING ERRCODE = '23514';
    END IF;

    INSERT INTO model_request_evidence_roots (
        id, workspace_id, external_effect_id, request_kind, binding_snapshot_id,
        qualification_target_binding_id, cause_kind, cause_id
    ) VALUES (
        target_root_id, target_workspace_id, target_external_effect_id, target_request_kind,
        target_binding_snapshot_id, target_qualification_binding_id, target_cause_kind, target_cause_id
    );
    FOR node_index IN 1..node_count LOOP
        INSERT INTO model_request_evidence_nodes (
            id, workspace_id, evidence_root_id, ordinal, reference_kind,
            reference_id, reference_version, safe_ordinal
        ) VALUES (
            gen_random_uuid(), target_workspace_id, target_root_id, node_index - 1,
            node_kinds[node_index], node_ids[node_index], node_versions[node_index],
            node_safe_ordinals[node_index]
        );
    END LOOP;
    RETURN target_root_id;
END
$$;

DO $$
BEGIN
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_register_embedding_space(UUID, UUID, UUID, TEXT, TEXT, INTEGER)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_publish_embedding_corpus_generation(UUID, UUID, UUID, BIGINT)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_publish_connection_admission_policy(UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_lock_embedding_job_recovery_authority(UUID, UUID)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_finalize_embedding_job_unknown(UUID, UUID)'::REGPROCEDURE
    );
    -- Re-assert ownership after the whole-body replacement, exactly as 0185 did
    -- when it replaced this same function for the second cause.
    PERFORM vestrace_assign_p03_function_owner(
        'public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'public.vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'public.vestrace_lock_model_request_evidence_for_reconstruction(UUID, UUID)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])'::REGPROCEDURE
    );
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER FUNCTION vestrace_register_embedding_space(
        UUID, UUID, UUID, TEXT, TEXT, INTEGER
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_publish_embedding_corpus_generation(
        UUID, UUID, UUID, BIGINT
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_publish_connection_admission_policy(
        UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_accept_embedding_job(
        UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_lock_embedding_job_recovery_authority(
        UUID, UUID
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_finalize_embedding_job_unknown(
        UUID, UUID
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION public.vestrace_lock_provider_dispatch_routing(
        UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION public.vestrace_try_admit_provider_dispatch(
        UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID,
        UUID, UUID, TEXT, INTEGER
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION public.vestrace_lock_model_request_evidence_for_reconstruction(
        UUID, UUID
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_create_model_request_evidence(
        UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[]
    ) OWNER TO vestrace_guarded_owner;
END
$$;

REVOKE ALL ON FUNCTION vestrace_register_embedding_space(
    UUID, UUID, UUID, TEXT, TEXT, INTEGER
) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_publish_embedding_corpus_generation(
    UUID, UUID, UUID, BIGINT
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_register_embedding_space(
    UUID, UUID, UUID, TEXT, TEXT, INTEGER
) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_publish_embedding_corpus_generation(
    UUID, UUID, UUID, BIGINT
) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_publish_connection_admission_policy(
    UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_publish_connection_admission_policy(
    UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER
) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_accept_embedding_job(
    UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_accept_embedding_job(
    UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_lock_embedding_job_recovery_authority(
    UUID, UUID
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_lock_embedding_job_recovery_authority(
    UUID, UUID
) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_finalize_embedding_job_unknown(
    UUID, UUID
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_finalize_embedding_job_unknown(
    UUID, UUID
) TO vestrace;
