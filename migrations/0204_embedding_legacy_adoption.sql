-- P04 Task 10: governed adoption of legacy embeddings, and the retirement gate.
--
-- Adoption is the one path by which a legacy plaintext vector stops existing.
-- It never copies that vector.  For each legacy `memory_embeddings` row it
-- fixes the memory revision the row was derived from, materializes that
-- revision's current content as an ordinary governed content material, and
-- creates a rebuild job that recomputes the vector through the same provider
-- path an ordinary delivery uses.  The legacy bytes are read by nothing.
--
-- That materialization is also what closes a gap the canonical transition left
-- open.  A canonical generation member points at an embedding_projection_entry,
-- whose source is a content_material; memory content lived only in
-- memory_revisions.content, so nothing connected a canonical member back to a
-- memory.  The adoption member row, and the content_material_ordinary_references
-- row it produces with owner_kind 'memory_revision', are that connection.
--
-- Cutover is a single guarded transaction: it proves the target canonical
-- generation is Ready and current under its own guard, stales the legacy
-- generation, deletes the legacy plaintext rows, and records both an exact
-- receipt and one opaque tombstone per retired legacy identity.  After that
-- the identity survives as history and as nothing else.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_legacy_adoption_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_legacy_adoption_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding legacy adoption upgrade must be provisioned'
            USING ERRCODE='42501';
    ELSE
        -- `memories` is an ordinary application table, not a guarded one, so the
        -- guarded owner these functions run as holds no privilege on it.
        -- Planning reads exactly one thing there -- which revision each legacy
        -- vector's memory currently points at -- so the grant is read-only and
        -- stops at that table.  On the upgrade path the migration runs as the
        -- restricted runtime role, which cannot grant on a table it does not
        -- own; there the provisioned prepare function performs the same grant.
        GRANT SELECT ON TABLE memories TO vestrace_guarded_owner;
    END IF;
END
$upgrade$;

-- One upgrade-only plan per legacy space registration.  `version` is the CAS
-- token every advance and the cutover check against, so two operators racing
-- the same plan cannot both believe they moved it.
CREATE TABLE embedding_legacy_adoptions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    legacy_space_registration_id UUID NOT NULL,
    legacy_space_id UUID NOT NULL,
    legacy_generation_id UUID,
    -- How many legacy rows the plan was fixed against.  A row appearing later
    -- is not silently adopted: the watermark is what the cutover re-proves.
    legacy_watermark BIGINT NOT NULL CHECK (legacy_watermark >= 0),
    target_space_registration_id UUID NOT NULL,
    target_generation_id UUID,
    state TEXT NOT NULL
        CHECK (state IN ('planned','rebuilding','ready_to_cutover','completed','failed')),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    state_changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, id),
    CONSTRAINT embedding_legacy_adoptions_one_per_space
        UNIQUE (workspace_id, legacy_space_registration_id),
    CONSTRAINT embedding_legacy_adoptions_idempotency
        UNIQUE (workspace_id, idempotency_key),
    CONSTRAINT embedding_legacy_adoptions_terminal_generation
        CHECK (state <> 'completed' OR target_generation_id IS NOT NULL),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, legacy_space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, target_space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT
);

-- One row per legacy vector.  `legacy_embedding_id` stays after cutover as the
-- opaque historical identity; `memory_embedding_id` is the live foreign key and
-- is dropped when the plaintext row is deleted, so no function can hydrate a
-- retired identity through this table.
CREATE TABLE embedding_legacy_adoption_members (
    workspace_id UUID NOT NULL,
    adoption_id UUID NOT NULL,
    member_ordinal BIGINT NOT NULL CHECK (member_ordinal > 0),
    legacy_embedding_id UUID NOT NULL,
    memory_id UUID NOT NULL,
    memory_revision_id UUID NOT NULL,
    source_material_id UUID,
    source_intent_id UUID,
    rebuild_job_id UUID,
    projection_entry_id UUID,
    state TEXT NOT NULL
        CHECK (state IN ('planned','sourced','rebuilding','satisfied','blocked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    state_changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, adoption_id, member_ordinal),
    CONSTRAINT embedding_legacy_adoption_members_identity
        UNIQUE (workspace_id, adoption_id, legacy_embedding_id),
    -- A satisfied member has recomputed its vector: it must name both the
    -- source it was rebuilt from and the canonical projection that resulted.
    CONSTRAINT embedding_legacy_adoption_members_satisfied_is_complete
        CHECK (state <> 'satisfied'
               OR (source_material_id IS NOT NULL AND rebuild_job_id IS NOT NULL
                   AND projection_entry_id IS NOT NULL)),
    CONSTRAINT embedding_legacy_adoption_members_sourced_has_material
        CHECK (state IN ('planned','blocked') OR source_material_id IS NOT NULL),
    FOREIGN KEY (workspace_id, adoption_id)
        REFERENCES embedding_legacy_adoptions(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, source_material_id)
        REFERENCES content_materials(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, projection_entry_id)
        REFERENCES embedding_projection_entries(workspace_id, id) ON DELETE RESTRICT
);

-- A blocker is a visible typed reason, never a free-text message: an operator
-- reads it to decide what to repair, so the set has to be closed.
CREATE TABLE embedding_legacy_adoption_blockers (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    adoption_id UUID NOT NULL,
    member_ordinal BIGINT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN (
        'source_memory_absent',
        'source_revision_absent',
        'source_content_erased',
        'source_material_not_live',
        'rebuild_failed',
        'target_space_not_canonical'
    )),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_legacy_adoption_blockers_exact
        UNIQUE (workspace_id, adoption_id, member_ordinal, reason),
    FOREIGN KEY (workspace_id, adoption_id, member_ordinal)
        REFERENCES embedding_legacy_adoption_members(workspace_id, adoption_id, member_ordinal)
        ON DELETE RESTRICT
);

-- The cutover fact.  One per adoption, immutable once written.
CREATE TABLE embedding_legacy_cutover_receipts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    adoption_id UUID NOT NULL,
    canonical_generation_id UUID NOT NULL,
    deleted_legacy_row_count BIGINT NOT NULL CHECK (deleted_legacy_row_count >= 0),
    qualification_head_version BIGINT NOT NULL CHECK (qualification_head_version > 0),
    audit_event_id UUID NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_legacy_cutover_receipts_one_per_adoption
        UNIQUE (workspace_id, adoption_id),
    FOREIGN KEY (workspace_id, adoption_id)
        REFERENCES embedding_legacy_adoptions(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, canonical_generation_id)
        REFERENCES embedding_corpus_generations(workspace_id, id) ON DELETE RESTRICT
);

-- What remains of a legacy vector after its plaintext is gone: an identity and
-- the fact that it was retired.  No vector, no digest, no dimension.
CREATE TABLE embedding_legacy_identity_tombstones (
    workspace_id UUID NOT NULL,
    legacy_embedding_id UUID NOT NULL,
    adoption_id UUID NOT NULL,
    legacy_space_registration_id UUID NOT NULL,
    retired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, legacy_embedding_id),
    FOREIGN KEY (workspace_id, adoption_id)
        REFERENCES embedding_legacy_adoptions(workspace_id, id) ON DELETE RESTRICT
);

-- The installation-level gate.  One row for the whole database, and it commits
-- only from a guarded scan that found nothing left to retire.
CREATE TABLE embedding_legacy_retirement_gate (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    retired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_adoption_count BIGINT NOT NULL CHECK (completed_adoption_count >= 0),
    scanned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Creates the plan, or returns the existing one for an exact replay.  The plan
-- is fixed here and nowhere else: which legacy rows it covers, which memory
-- revision each was derived from, and how many rows the cutover must find.
CREATE OR REPLACE FUNCTION vestrace_start_or_resume_legacy_adoption(
    target_id UUID,
    target_workspace UUID,
    target_legacy_registration UUID,
    target_canonical_registration UUID,
    target_key TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing embedding_legacy_adoptions%ROWTYPE;
    legacy_registration embedding_space_registrations%ROWTYPE;
    canonical_registration embedding_space_registrations%ROWTYPE;
    legacy_generation UUID;
    watermark BIGINT;
    legacy_row RECORD;
    memory_row RECORD;
    ordinal BIGINT := 0;
    member_state TEXT;
    blocker TEXT;
BEGIN
    IF target_id IS NULL OR target_workspace IS NULL OR target_legacy_registration IS NULL
       OR target_canonical_registration IS NULL OR btrim(coalesce(target_key,'')) = '' THEN
        RAISE EXCEPTION 'legacy adoption arguments are malformed' USING ERRCODE='22023';
    END IF;

    -- A replay returns its one prior plan; a different key against the same
    -- legacy space is a second plan for one space and is refused.
    SELECT * INTO existing FROM embedding_legacy_adoptions
     WHERE workspace_id=target_workspace
       AND legacy_space_registration_id=target_legacy_registration
     FOR UPDATE;
    IF FOUND THEN
        IF existing.idempotency_key <> target_key THEN
            RAISE EXCEPTION 'legacy adoption for this space already exists under another key'
                USING ERRCODE='23505';
        END IF;
        RETURN existing.id;
    END IF;

    SELECT * INTO legacy_registration FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=target_legacy_registration FOR SHARE;
    IF NOT FOUND OR legacy_registration.registration_kind <> 'legacy_upgrade'
       OR legacy_registration.space_id IS NULL THEN
        RAISE EXCEPTION 'legacy adoption requires its exact legacy space registration'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO canonical_registration FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=target_canonical_registration FOR SHARE;
    IF NOT FOUND OR canonical_registration.registration_kind <> 'canonical' THEN
        RAISE EXCEPTION 'legacy adoption requires a canonical target space'
            USING ERRCODE='23514';
    END IF;

    SELECT id INTO legacy_generation FROM embedding_corpus_generations
     WHERE workspace_id=target_workspace
       AND space_registration_id=target_legacy_registration
       AND member_representation='legacy_upgrade'
     ORDER BY created_at DESC LIMIT 1;

    SELECT count(*) INTO watermark FROM memory_embeddings
     WHERE workspace_id=target_workspace AND space_id=legacy_registration.space_id;

    INSERT INTO embedding_legacy_adoptions(
        id,workspace_id,legacy_space_registration_id,legacy_space_id,legacy_generation_id,
        legacy_watermark,target_space_registration_id,state,idempotency_key
    ) VALUES (
        target_id,target_workspace,target_legacy_registration,legacy_registration.space_id,
        legacy_generation,watermark,target_canonical_registration,'planned',target_key
    );

    -- One member per legacy row, in a stable order so a resume enumerates the
    -- same ordinals.  A row whose memory or revision is gone becomes a visible
    -- blocker here rather than a silent omission at cutover.
    FOR legacy_row IN
        SELECT id, memory_id FROM memory_embeddings
         WHERE workspace_id=target_workspace AND space_id=legacy_registration.space_id
         ORDER BY id
    LOOP
        ordinal := ordinal + 1;
        blocker := NULL;
        SELECT m.id AS memory_id, m.active_revision_id, m.status
          INTO memory_row
          FROM memories m
         WHERE m.workspace_id=target_workspace AND m.id=legacy_row.memory_id;
        IF NOT FOUND THEN
            blocker := 'source_memory_absent';
        ELSIF memory_row.status = 'deleted' THEN
            blocker := 'source_content_erased';
        ELSIF memory_row.active_revision_id IS NULL THEN
            blocker := 'source_revision_absent';
        END IF;

        member_state := CASE WHEN blocker IS NULL THEN 'planned' ELSE 'blocked' END;
        INSERT INTO embedding_legacy_adoption_members(
            workspace_id,adoption_id,member_ordinal,legacy_embedding_id,
            memory_id,memory_revision_id,state
        ) VALUES (
            target_workspace,target_id,ordinal,legacy_row.id,
            legacy_row.memory_id,
            coalesce(memory_row.active_revision_id,legacy_row.id),
            member_state
        );
        IF blocker IS NOT NULL THEN
            INSERT INTO embedding_legacy_adoption_blockers(
                id,workspace_id,adoption_id,member_ordinal,reason
            ) VALUES (gen_random_uuid(),target_workspace,target_id,ordinal,blocker);
        END IF;
    END LOOP;

    RETURN target_id;
END
$$;

-- Binds the member to the governed content material its rebuild reads.  The
-- material must be Live and must actually be this revision's content: an
-- ordinary reference naming another owner is a different memory's text, and
-- adopting it would silently re-embed the wrong source.
CREATE OR REPLACE FUNCTION vestrace_bind_legacy_adoption_source(
    target_workspace UUID,
    target_adoption UUID,
    target_ordinal BIGINT,
    target_material UUID,
    target_intent UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    member embedding_legacy_adoption_members%ROWTYPE;
    material content_materials%ROWTYPE;
    reference content_material_ordinary_references%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_adoption IS NULL OR target_ordinal IS NULL
       OR target_material IS NULL OR target_intent IS NULL THEN
        RAISE EXCEPTION 'legacy adoption source arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO member FROM embedding_legacy_adoption_members
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy adoption source requires its exact member' USING ERRCODE='23514';
    END IF;
    -- An exact replay of the same binding is the same fact, not a conflict.
    IF member.state='sourced' AND member.source_material_id=target_material
       AND member.source_intent_id=target_intent THEN
        RETURN;
    END IF;
    IF member.state <> 'planned' THEN
        RAISE EXCEPTION 'only a planned legacy adoption member may take a source'
            USING ERRCODE='23514';
    END IF;

    SELECT * INTO material FROM content_materials
     WHERE workspace_id=target_workspace AND id=target_material FOR SHARE;
    IF NOT FOUND OR material.state <> 'live' THEN
        RAISE EXCEPTION 'legacy adoption source must be a Live content material'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO reference FROM content_material_ordinary_references
     WHERE workspace_id=target_workspace AND material_id=target_material;
    IF NOT FOUND OR reference.intent_id <> target_intent
       OR reference.owner_kind <> 'memory_revision'
       OR reference.owner_id <> member.memory_revision_id THEN
        RAISE EXCEPTION 'legacy adoption source must be the exact memory revision content'
            USING ERRCODE='23514';
    END IF;

    UPDATE embedding_legacy_adoption_members
       SET source_material_id=target_material, source_intent_id=target_intent,
           state='sourced', state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal;
END
$$;

-- Attaches the governed rebuild job that will recompute this member's vector.
CREATE OR REPLACE FUNCTION vestrace_bind_legacy_adoption_rebuild(
    target_workspace UUID,
    target_adoption UUID,
    target_ordinal BIGINT,
    target_job UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    member embedding_legacy_adoption_members%ROWTYPE;
    job embedding_jobs%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_adoption IS NULL OR target_ordinal IS NULL
       OR target_job IS NULL THEN
        RAISE EXCEPTION 'legacy adoption rebuild arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO member FROM embedding_legacy_adoption_members
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy adoption rebuild requires its exact member' USING ERRCODE='23514';
    END IF;
    IF member.state='rebuilding' AND member.rebuild_job_id=target_job THEN
        RETURN;
    END IF;
    IF member.state <> 'sourced' THEN
        RAISE EXCEPTION 'only a sourced legacy adoption member may take a rebuild job'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO job FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job FOR SHARE;
    -- Adoption recomputes; it never re-delivers.  A delivery job here would mean
    -- the vector came from somewhere other than a fresh provider computation.
    IF NOT FOUND OR job.kind <> 'rebuild' THEN
        RAISE EXCEPTION 'legacy adoption requires an exact rebuild job' USING ERRCODE='23514';
    END IF;

    UPDATE embedding_legacy_adoption_members
       SET rebuild_job_id=target_job, state='rebuilding', state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal;
    UPDATE embedding_legacy_adoptions
       SET state='rebuilding', version=version+1, state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND id=target_adoption AND state='planned';
END
$$;

-- The member is satisfied only by a canonical projection its own rebuild job
-- produced.  Any other projection would be another job vector wearing this
-- member identity.
CREATE OR REPLACE FUNCTION vestrace_satisfy_legacy_adoption_member(
    target_workspace UUID,
    target_adoption UUID,
    target_ordinal BIGINT,
    target_projection UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    member embedding_legacy_adoption_members%ROWTYPE;
    projection embedding_projection_entries%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_adoption IS NULL OR target_ordinal IS NULL
       OR target_projection IS NULL THEN
        RAISE EXCEPTION 'legacy adoption satisfaction arguments are malformed'
            USING ERRCODE='22023';
    END IF;
    SELECT * INTO member FROM embedding_legacy_adoption_members
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy adoption satisfaction requires its exact member'
            USING ERRCODE='23514';
    END IF;
    IF member.state='satisfied' AND member.projection_entry_id=target_projection THEN
        RETURN;
    END IF;
    IF member.state <> 'rebuilding' THEN
        RAISE EXCEPTION 'only a rebuilding legacy adoption member may be satisfied'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO projection FROM embedding_projection_entries
     WHERE workspace_id=target_workspace AND id=target_projection FOR SHARE;
    IF NOT FOUND OR projection.job_id IS DISTINCT FROM member.rebuild_job_id THEN
        RAISE EXCEPTION 'legacy adoption satisfaction requires the own rebuild output'
            USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM embedding_projection_source_dependencies dependency
         WHERE dependency.workspace_id=target_workspace
           AND dependency.projection_id=target_projection
           AND dependency.source_material_id=member.source_material_id
    ) THEN
        RAISE EXCEPTION 'legacy adoption satisfaction requires the exact bound source'
            USING ERRCODE='23514';
    END IF;

    UPDATE embedding_legacy_adoption_members
       SET projection_entry_id=target_projection, state='satisfied', state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal;
END
$$;

-- Records a visible typed blocker and fails the member it belongs to.
CREATE OR REPLACE FUNCTION vestrace_record_legacy_adoption_blocker(
    target_workspace UUID,
    target_adoption UUID,
    target_ordinal BIGINT,
    target_reason TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    member embedding_legacy_adoption_members%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_adoption IS NULL OR target_ordinal IS NULL
       OR target_reason IS NULL THEN
        RAISE EXCEPTION 'legacy adoption blocker arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO member FROM embedding_legacy_adoption_members
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy adoption blocker requires its exact member' USING ERRCODE='23514';
    END IF;
    INSERT INTO embedding_legacy_adoption_blockers(
        id,workspace_id,adoption_id,member_ordinal,reason
    ) VALUES (gen_random_uuid(),target_workspace,target_adoption,target_ordinal,target_reason)
    ON CONFLICT ON CONSTRAINT embedding_legacy_adoption_blockers_exact DO NOTHING;
    UPDATE embedding_legacy_adoption_members
       SET state='blocked', state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption
       AND member_ordinal=target_ordinal;
END
$$;

-- Proves the plan may cut over: every member is terminal, at least one was
-- actually rebuilt, and the named canonical generation is Ready and current
-- under its own guard.  Blocked members are permitted here and refused at
-- cutover, so an operator can see a complete picture before committing.
CREATE OR REPLACE FUNCTION vestrace_prove_legacy_adoption_ready(
    target_workspace UUID,
    target_adoption UUID,
    target_generation UUID
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    adoption embedding_legacy_adoptions%ROWTYPE;
    generation embedding_corpus_generations%ROWTYPE;
    guard embedding_index_generation_guards%ROWTYPE;
    unfinished BIGINT;
    satisfied BIGINT;
BEGIN
    IF target_workspace IS NULL OR target_adoption IS NULL OR target_generation IS NULL THEN
        RAISE EXCEPTION 'legacy adoption readiness arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO adoption FROM embedding_legacy_adoptions
     WHERE workspace_id=target_workspace AND id=target_adoption FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy adoption readiness requires its exact plan' USING ERRCODE='23514';
    END IF;
    IF adoption.state='ready_to_cutover' AND adoption.target_generation_id=target_generation THEN
        RETURN adoption.version;
    END IF;
    IF adoption.state <> 'rebuilding' THEN
        RAISE EXCEPTION 'only a rebuilding legacy adoption may become ready to cut over'
            USING ERRCODE='23514';
    END IF;

    SELECT count(*) FILTER (WHERE state NOT IN ('satisfied','blocked')),
           count(*) FILTER (WHERE state='satisfied')
      INTO unfinished, satisfied
      FROM embedding_legacy_adoption_members
     WHERE workspace_id=target_workspace AND adoption_id=target_adoption;
    IF unfinished > 0 THEN
        RAISE EXCEPTION 'legacy adoption readiness requires every member terminal'
            USING ERRCODE='23514';
    END IF;

    SELECT * INTO generation FROM embedding_corpus_generations
     WHERE workspace_id=target_workspace AND id=target_generation FOR SHARE;
    IF NOT FOUND OR generation.state <> 'ready'
       OR generation.member_representation <> 'encrypted_projection'
       OR generation.space_registration_id <> adoption.target_space_registration_id THEN
        RAISE EXCEPTION 'legacy adoption readiness requires a Ready canonical target generation'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO guard FROM embedding_index_generation_guards
     WHERE workspace_id=target_workspace
       AND space_registration_id=adoption.target_space_registration_id FOR SHARE;
    IF NOT FOUND OR guard.current_generation_id IS DISTINCT FROM target_generation
       OR guard.generation_epoch IS DISTINCT FROM generation.generation_epoch THEN
        RAISE EXCEPTION 'legacy adoption readiness requires the target generation to be current'
            USING ERRCODE='23514';
    END IF;
    -- A generation carrying none of this plan's rebuilt vectors is not this
    -- plan's target, however Ready it happens to be.
    IF satisfied > 0 AND NOT EXISTS (
        SELECT 1 FROM embedding_corpus_generation_members member
          JOIN embedding_legacy_adoption_members adopted
            ON adopted.workspace_id=member.workspace_id
           AND adopted.projection_entry_id=member.embedding_projection_entry_id
         WHERE member.workspace_id=target_workspace
           AND member.corpus_generation_id=target_generation
           AND adopted.adoption_id=target_adoption
    ) THEN
        RAISE EXCEPTION 'legacy adoption readiness requires the target generation to hold its rebuilt members'
            USING ERRCODE='23514';
    END IF;

    UPDATE embedding_legacy_adoptions
       SET state='ready_to_cutover', target_generation_id=target_generation,
           version=version+1, state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND id=target_adoption;
    RETURN adoption.version + 1;
END
$$;

-- The one transaction in which a legacy plaintext vector stops existing.
--
-- Everything it proves is proven again here under its own locks, because the
-- readiness proof happened in an earlier transaction and the world may have
-- moved since.  A refusal on any branch performs none of the mutations.
CREATE OR REPLACE FUNCTION vestrace_commit_legacy_adoption_cutover(
    target_receipt UUID,
    target_workspace UUID,
    target_adoption UUID,
    expected_version BIGINT,
    target_audit_event UUID
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    adoption embedding_legacy_adoptions%ROWTYPE;
    generation embedding_corpus_generations%ROWTYPE;
    guard embedding_index_generation_guards%ROWTYPE;
    canonical embedding_space_registrations%ROWTYPE;
    head_version BIGINT;
    remaining BIGINT;
    deleted BIGINT;
    existing embedding_legacy_cutover_receipts%ROWTYPE;
BEGIN
    IF target_receipt IS NULL OR target_workspace IS NULL OR target_adoption IS NULL
       OR expected_version IS NULL OR target_audit_event IS NULL THEN
        RAISE EXCEPTION 'legacy adoption cutover arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO adoption FROM embedding_legacy_adoptions
     WHERE workspace_id=target_workspace AND id=target_adoption FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy adoption cutover requires its exact plan' USING ERRCODE='23514';
    END IF;
    -- A replay of the committed cutover returns its one prior count.
    IF adoption.state='completed' THEN
        SELECT * INTO existing FROM embedding_legacy_cutover_receipts
         WHERE workspace_id=target_workspace AND adoption_id=target_adoption;
        IF FOUND AND existing.id=target_receipt THEN
            RETURN existing.deleted_legacy_row_count;
        END IF;
        RAISE EXCEPTION 'legacy adoption cutover already committed under another receipt'
            USING ERRCODE='23505';
    END IF;
    IF adoption.state <> 'ready_to_cutover' THEN
        RAISE EXCEPTION 'only a ready legacy adoption may cut over' USING ERRCODE='23514';
    END IF;
    IF adoption.version <> expected_version THEN
        RAISE EXCEPTION 'legacy adoption cutover expected version does not match'
            USING ERRCODE='40001';
    END IF;

    -- No member may still be blocked: a partial cutover would delete plaintext
    -- whose vector was never recomputed.
    IF EXISTS (
        SELECT 1 FROM embedding_legacy_adoption_members
         WHERE workspace_id=target_workspace AND adoption_id=target_adoption
           AND state <> 'satisfied'
    ) THEN
        RAISE EXCEPTION 'legacy adoption cutover requires every member satisfied'
            USING ERRCODE='23514';
    END IF;

    SELECT * INTO generation FROM embedding_corpus_generations
     WHERE workspace_id=target_workspace AND id=adoption.target_generation_id FOR SHARE;
    IF NOT FOUND OR generation.state <> 'ready'
       OR generation.member_representation <> 'encrypted_projection' THEN
        RAISE EXCEPTION 'legacy adoption cutover requires a Ready canonical generation'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO guard FROM embedding_index_generation_guards
     WHERE workspace_id=target_workspace
       AND space_registration_id=adoption.target_space_registration_id FOR UPDATE;
    IF NOT FOUND OR guard.current_generation_id IS DISTINCT FROM adoption.target_generation_id
       OR guard.generation_epoch IS DISTINCT FROM generation.generation_epoch THEN
        RAISE EXCEPTION 'legacy adoption cutover requires the target generation to be current'
            USING ERRCODE='23514';
    END IF;

    SELECT * INTO canonical FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=adoption.target_space_registration_id FOR SHARE;
    IF NOT FOUND OR canonical.registration_kind <> 'canonical' THEN
        RAISE EXCEPTION 'legacy adoption cutover requires a canonical target space'
            USING ERRCODE='23514';
    END IF;
    SELECT version INTO head_version FROM model_qualification_heads
     WHERE workspace_id=target_workspace AND model_revision_id=canonical.model_revision_id
     FOR SHARE;
    IF head_version IS NULL THEN
        RAISE EXCEPTION 'legacy adoption cutover requires the qualification head of its target'
            USING ERRCODE='23514';
    END IF;

    -- Nothing may have been written to the legacy space since planning: a row
    -- that appeared later was never adopted, and deleting it here would destroy
    -- a vector no canonical member replaces.
    SELECT count(*) INTO remaining FROM memory_embeddings
     WHERE workspace_id=target_workspace AND space_id=adoption.legacy_space_id;
    IF remaining <> adoption.legacy_watermark THEN
        RAISE EXCEPTION 'legacy adoption cutover requires the exact planned legacy row count'
            USING ERRCODE='23514';
    END IF;

    IF adoption.legacy_generation_id IS NOT NULL THEN
        UPDATE embedding_corpus_generations
           SET state='stale'
         WHERE workspace_id=target_workspace AND id=adoption.legacy_generation_id
           AND state IN ('building','ready');
    END IF;

    -- The identity survives; the vector does not.  The tombstone is written
    -- before the delete so no window exists in which the row is gone and its
    -- retirement unrecorded.
    INSERT INTO embedding_legacy_identity_tombstones(
        workspace_id,legacy_embedding_id,adoption_id,legacy_space_registration_id
    )
    SELECT target_workspace, member.legacy_embedding_id, target_adoption,
           adoption.legacy_space_registration_id
      FROM embedding_legacy_adoption_members member
     WHERE member.workspace_id=target_workspace AND member.adoption_id=target_adoption
    ON CONFLICT DO NOTHING;

    DELETE FROM embedding_corpus_generation_members
     WHERE workspace_id=target_workspace
       AND legacy_embedding_id IN (
           SELECT legacy_embedding_id FROM embedding_legacy_adoption_members
            WHERE workspace_id=target_workspace AND adoption_id=target_adoption);
    DELETE FROM memory_embeddings
     WHERE workspace_id=target_workspace AND space_id=adoption.legacy_space_id;
    GET DIAGNOSTICS deleted = ROW_COUNT;

    INSERT INTO embedding_legacy_cutover_receipts(
        id,workspace_id,adoption_id,canonical_generation_id,deleted_legacy_row_count,
        qualification_head_version,audit_event_id
    ) VALUES (
        target_receipt,target_workspace,target_adoption,adoption.target_generation_id,
        deleted,head_version,target_audit_event
    );
    UPDATE embedding_legacy_adoptions
       SET state='completed', version=version+1, state_changed_at=NOW()
     WHERE workspace_id=target_workspace AND id=target_adoption;
    RETURN deleted;
END
$$;

-- The installation gate.  It is deliberately database-wide and workspace-blind:
-- one workspace still holding a legacy vector is enough to keep it shut, and a
-- caller cannot narrow the scan to the workspace it happens to have finished.
CREATE OR REPLACE FUNCTION vestrace_commit_legacy_plaintext_retirement()
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    completed BIGINT;
    outstanding BIGINT;
    surviving BIGINT;
    existing embedding_legacy_retirement_gate%ROWTYPE;
BEGIN
    SELECT * INTO existing FROM embedding_legacy_retirement_gate WHERE singleton;
    IF FOUND THEN
        RETURN existing.completed_adoption_count;
    END IF;

    SELECT count(*) FILTER (WHERE state='completed'),
           count(*) FILTER (WHERE state <> 'completed')
      INTO completed, outstanding
      FROM embedding_legacy_adoptions;
    IF outstanding > 0 THEN
        RAISE EXCEPTION 'legacy plaintext retirement requires every adoption completed'
            USING ERRCODE='23514';
    END IF;

    -- A legacy space that no adoption ever covered would otherwise pass by
    -- being invisible, so registrations are counted, not just plans.
    IF EXISTS (
        SELECT 1 FROM embedding_space_registrations registration
         WHERE registration.registration_kind='legacy_upgrade'
           AND NOT EXISTS (
               SELECT 1 FROM embedding_legacy_adoptions adoption
                WHERE adoption.workspace_id=registration.workspace_id
                  AND adoption.legacy_space_registration_id=registration.id
                  AND adoption.state='completed')
    ) THEN
        RAISE EXCEPTION 'legacy plaintext retirement requires every legacy space adopted'
            USING ERRCODE='23514';
    END IF;

    SELECT count(*) INTO surviving FROM memory_embeddings;
    IF surviving > 0 THEN
        RAISE EXCEPTION 'legacy plaintext retirement requires zero surviving legacy vectors'
            USING ERRCODE='23514';
    END IF;

    INSERT INTO embedding_legacy_retirement_gate(singleton,completed_adoption_count)
    VALUES (TRUE,completed);
    RETURN completed;
END
$$;

DO $rls$
DECLARE target REGCLASS;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'embedding_legacy_adoptions'::REGCLASS,
        'embedding_legacy_adoption_members'::REGCLASS,
        'embedding_legacy_adoption_blockers'::REGCLASS,
        'embedding_legacy_cutover_receipts'::REGCLASS,
        'embedding_legacy_identity_tombstones'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', target);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', target);
        EXECUTE format(
            'CREATE POLICY %s_workspace_policy ON %s '
            'USING(workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',TRUE),'''')::UUID) '
            'WITH CHECK(workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',TRUE),'''')::UUID)',
            target::TEXT, target);
        EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC', target);
        EXECUTE format(
            'CREATE TRIGGER %s_guarded BEFORE INSERT OR UPDATE OR DELETE ON %s '
            'FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation()',
            target::TEXT, target);
    END LOOP;
END
$rls$;

-- The retirement gate is installation-level and carries no workspace column, so
-- it takes the guarded mutation trigger without a workspace policy.  Enabling
-- FORCE row security with no policy would hide the row from the SECURITY
-- DEFINER owner too, and the gate would try to commit itself twice.
REVOKE ALL ON TABLE embedding_legacy_retirement_gate FROM PUBLIC;
CREATE TRIGGER embedding_legacy_retirement_gate_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_legacy_retirement_gate
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

DO $handback$
DECLARE target REGCLASS; target_function REGPROCEDURE;
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_legacy_adoption_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_legacy_adoption_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
            RAISE EXCEPTION 'embedding legacy adoption ownership hand-back is unavailable'
                USING ERRCODE='42501';
        END IF;
        FOREACH target IN ARRAY ARRAY[
            'embedding_legacy_adoptions'::REGCLASS,
            'embedding_legacy_adoption_members'::REGCLASS,
            'embedding_legacy_adoption_blockers'::REGCLASS,
            'embedding_legacy_cutover_receipts'::REGCLASS,
            'embedding_legacy_identity_tombstones'::REGCLASS,
            'embedding_legacy_retirement_gate'::REGCLASS
        ] LOOP
            EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner', target);
            EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner', target);
            EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace', target);
            EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace', target);
        END LOOP;
        FOREACH target_function IN ARRAY ARRAY[
            'vestrace_start_or_resume_legacy_adoption(uuid,uuid,uuid,uuid,text)'::REGPROCEDURE,
            'vestrace_bind_legacy_adoption_source(uuid,uuid,bigint,uuid,uuid)'::REGPROCEDURE,
            'vestrace_bind_legacy_adoption_rebuild(uuid,uuid,bigint,uuid)'::REGPROCEDURE,
            'vestrace_satisfy_legacy_adoption_member(uuid,uuid,bigint,uuid)'::REGPROCEDURE,
            'vestrace_record_legacy_adoption_blocker(uuid,uuid,bigint,text)'::REGPROCEDURE,
            'vestrace_prove_legacy_adoption_ready(uuid,uuid,uuid)'::REGPROCEDURE,
            'vestrace_commit_legacy_adoption_cutover(uuid,uuid,uuid,bigint,uuid)'::REGPROCEDURE,
            'vestrace_commit_legacy_plaintext_retirement()'::REGPROCEDURE
        ] LOOP
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function);
            EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace', target_function);
            EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace', target_function);
        END LOOP;
    END IF;
END
$handback$;
