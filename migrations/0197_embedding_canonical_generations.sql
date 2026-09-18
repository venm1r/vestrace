-- P04 completion: canonical encrypted generations and legacy quarantine.
-- The one-shot bridge preserves table ACLs by executing only this exact DDL.
DO $upgrade$ BEGIN
    IF to_regprocedure('public.vestrace_prepare_canonical_generation_upgrade()') IS NOT NULL THEN
        PERFORM vestrace_prepare_canonical_generation_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
            RAISE EXCEPTION 'canonical generation upgrade must be provisioned' USING ERRCODE='42501';
        END IF;
ALTER TABLE embedding_space_registrations
    ALTER COLUMN space_id DROP NOT NULL,
    DROP CONSTRAINT embedding_space_registrations_tuple_key,
    ADD COLUMN registration_kind TEXT NOT NULL DEFAULT 'legacy_upgrade'
        CHECK(registration_kind IN ('legacy_upgrade','canonical')),
    ADD COLUMN model_revision_id UUID,
    ADD COLUMN model_qualification_revision_id UUID,
    ADD COLUMN request_shape_revision_id UUID,
    ADD COLUMN adapter_profile_revision TEXT,
    ADD COLUMN returned_model TEXT,
    ADD COLUMN encoding_format TEXT,
    ADD CONSTRAINT embedding_space_registration_representation CHECK (
        (registration_kind='legacy_upgrade' AND space_id IS NOT NULL
         AND model_revision_id IS NULL AND model_qualification_revision_id IS NULL
         AND request_shape_revision_id IS NULL AND adapter_profile_revision IS NULL
         AND returned_model IS NULL AND encoding_format IS NULL)
        OR (registration_kind='canonical' AND space_id IS NULL
         AND model_revision_id IS NOT NULL AND model_qualification_revision_id IS NOT NULL
         AND request_shape_revision_id IS NOT NULL AND adapter_profile_revision IS NOT NULL
         AND btrim(adapter_profile_revision)<>'' AND returned_model IS NOT NULL
         AND returned_model=model AND encoding_format IS NOT NULL AND encoding_format='float'));
CREATE UNIQUE INDEX embedding_space_registrations_legacy_tuple_key
    ON embedding_space_registrations(workspace_id,name,model,dimensions)
    WHERE registration_kind='legacy_upgrade';
CREATE UNIQUE INDEX embedding_space_registrations_canonical_tuple_key
    ON embedding_space_registrations(workspace_id,name,model_revision_id,
        model_qualification_revision_id,adapter_profile_revision,request_shape_revision_id,
        returned_model,encoding_format,dimensions) WHERE registration_kind='canonical';
ALTER TABLE model_qualification_heads ADD COLUMN active_space_registration_id UUID;
ALTER TABLE embedding_index_generation_guards
    ADD COLUMN guard_version BIGINT NOT NULL DEFAULT 1 CHECK(guard_version>0),
    ADD COLUMN current_generation_id UUID;
ALTER TABLE embedding_corpus_generations
    DROP CONSTRAINT embedding_corpus_generations_state_check,
    ADD CONSTRAINT embedding_corpus_generations_state_check CHECK(state IN ('building','ready','stale','revoked')),
    ADD COLUMN member_representation TEXT NOT NULL DEFAULT 'legacy_upgrade'
        CHECK(member_representation IN ('legacy_upgrade','encrypted_projection')),
    ADD COLUMN generation_epoch BIGINT,
    ADD COLUMN captured_guard_version BIGINT,
    ADD COLUMN corpus_revision BIGINT,
    ADD COLUMN built_through_projection_ordinal BIGINT,
    ADD CONSTRAINT embedding_generation_capture_complete CHECK (
        (member_representation='legacy_upgrade' AND generation_epoch IS NULL
            AND captured_guard_version IS NULL AND corpus_revision IS NULL
            AND built_through_projection_ordinal IS NULL)
        OR (member_representation='encrypted_projection' AND generation_epoch IS NOT NULL
            AND generation_epoch>0 AND captured_guard_version IS NOT NULL AND captured_guard_version>0
            AND corpus_revision IS NOT NULL AND corpus_revision>=0
            AND built_through_projection_ordinal IS NOT NULL AND built_through_projection_ordinal>=0));
ALTER TABLE embedding_corpus_generations
    ALTER COLUMN published_at DROP NOT NULL,
    ALTER COLUMN published_at DROP DEFAULT,
    ADD COLUMN created_at TIMESTAMPTZ,
    ADD COLUMN state_changed_at TIMESTAMPTZ,
    ADD COLUMN lifecycle_reason TEXT NOT NULL DEFAULT 'legacy_upgrade'
        CHECK(lifecycle_reason IN ('legacy_upgrade','captured','published','invalidated','revoked'));
ALTER TABLE embedding_corpus_generations DISABLE TRIGGER embedding_corpus_generations_guarded;
UPDATE embedding_corpus_generations SET created_at=published_at,state_changed_at=published_at;
SET CONSTRAINTS ALL IMMEDIATE;
SET CONSTRAINTS ALL DEFERRED;
ALTER TABLE embedding_corpus_generations ENABLE TRIGGER embedding_corpus_generations_guarded;
ALTER TABLE embedding_corpus_generations
    ALTER COLUMN created_at SET NOT NULL,
    ALTER COLUMN created_at SET DEFAULT NOW(),
    ALTER COLUMN state_changed_at SET NOT NULL,
    ALTER COLUMN state_changed_at SET DEFAULT NOW();
ALTER TABLE embedding_corpus_generation_members
    DROP CONSTRAINT embedding_corpus_generation_members_pkey,
    ALTER COLUMN memory_embedding_id DROP NOT NULL,
    ADD COLUMN member_ordinal BIGINT,
    ADD COLUMN legacy_embedding_id UUID,
    ADD COLUMN embedding_projection_entry_id UUID;
ALTER TABLE embedding_corpus_generation_members DISABLE TRIGGER embedding_corpus_generation_members_guarded;
WITH ordered AS (
    SELECT workspace_id,corpus_generation_id,memory_embedding_id,
        row_number() OVER(PARTITION BY workspace_id,corpus_generation_id ORDER BY memory_embedding_id) AS ordinal
    FROM embedding_corpus_generation_members
)
UPDATE embedding_corpus_generation_members m SET legacy_embedding_id=m.memory_embedding_id,member_ordinal=o.ordinal
FROM ordered o WHERE o.workspace_id=m.workspace_id AND o.corpus_generation_id=m.corpus_generation_id
    AND o.memory_embedding_id=m.memory_embedding_id;
ALTER TABLE embedding_corpus_generation_members ENABLE TRIGGER embedding_corpus_generation_members_guarded;
ALTER TABLE embedding_corpus_generation_members
    ALTER COLUMN member_ordinal SET NOT NULL,
    ADD PRIMARY KEY(workspace_id,corpus_generation_id,member_ordinal),
    ADD CHECK(member_ordinal>0),
    ADD CONSTRAINT embedding_generation_member_xor CHECK (
        (legacy_embedding_id IS NOT NULL AND embedding_projection_entry_id IS NULL
          AND memory_embedding_id IS NOT DISTINCT FROM legacy_embedding_id)
        OR (legacy_embedding_id IS NULL AND embedding_projection_entry_id IS NOT NULL AND memory_embedding_id IS NULL));
CREATE UNIQUE INDEX embedding_generation_members_legacy_unique
    ON embedding_corpus_generation_members(workspace_id,corpus_generation_id,legacy_embedding_id)
    WHERE legacy_embedding_id IS NOT NULL;
CREATE UNIQUE INDEX embedding_generation_members_projection_unique
    ON embedding_corpus_generation_members(workspace_id,corpus_generation_id,embedding_projection_entry_id)
    WHERE embedding_projection_entry_id IS NOT NULL;
DROP TRIGGER embedding_corpus_generation_members_consistent ON embedding_corpus_generation_members;
ALTER TABLE model_qualification_heads ADD FOREIGN KEY(workspace_id,active_space_registration_id)
    REFERENCES embedding_space_registrations(workspace_id,id) ON DELETE RESTRICT;
ALTER TABLE embedding_index_generation_guards ADD FOREIGN KEY(workspace_id,current_generation_id)
    REFERENCES embedding_corpus_generations(workspace_id,id) ON DELETE RESTRICT;
ALTER TABLE embedding_corpus_generation_members ADD FOREIGN KEY(workspace_id,embedding_projection_entry_id)
    REFERENCES embedding_projection_entries(workspace_id,id) ON DELETE RESTRICT;
ALTER TABLE embedding_space_registrations
    ADD FOREIGN KEY(workspace_id,model_revision_id) REFERENCES model_revisions(workspace_id,id) ON DELETE RESTRICT,
    ADD FOREIGN KEY(workspace_id,model_qualification_revision_id) REFERENCES model_qualification_revisions(workspace_id,id) ON DELETE RESTRICT,
    ADD FOREIGN KEY(workspace_id,request_shape_revision_id) REFERENCES model_request_shape_revisions(workspace_id,id) ON DELETE RESTRICT;

    END IF;
END $upgrade$;

CREATE FUNCTION vestrace_assert_canonical_embedding_space(target_workspace UUID,target_registration UUID)
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM embedding_space_registrations s
        JOIN model_revisions m ON m.workspace_id=s.workspace_id AND m.id=s.model_revision_id AND m.kind='embedding'
        JOIN model_qualification_revisions q ON q.workspace_id=s.workspace_id AND q.id=s.model_qualification_revision_id
            AND q.model_revision_id=m.id AND q.connection_revision_id=m.connection_revision_id
        JOIN qualification_jobs job ON job.workspace_id=q.workspace_id AND job.id=q.qualification_job_id
            AND job.profile_revision='q1' AND job.state='succeeded'
        JOIN connection_revisions c ON c.workspace_id=m.workspace_id AND c.id=m.connection_revision_id
        JOIN model_request_shape_revisions r ON r.workspace_id=s.workspace_id AND r.id=s.request_shape_revision_id
        JOIN qualification_probe_results p ON p.workspace_id=q.workspace_id AND p.qualification_job_id=q.qualification_job_id
            AND p.probe_ordinal='90' AND p.result='pass'
        JOIN provider_dispatch_causes cause ON cause.workspace_id=p.workspace_id AND cause.external_effect_id=p.external_effect_id
            AND cause.cause_kind='qualification_probe' AND cause.qualification_job_id=q.qualification_job_id
            AND cause.qualification_probe_ordinal='90' AND cause.model_request_evidence_id=p.model_request_evidence_id
        JOIN qualification_target_bindings target ON target.workspace_id=cause.workspace_id AND target.id=cause.qualification_target_binding_id
            AND target.qualification_job_id=q.qualification_job_id AND target.embedding_model_revision_id=m.id
            AND target.connection_revision_id=c.id AND target.connection_id=c.connection_id
        JOIN model_request_evidence_roots root ON root.workspace_id=cause.workspace_id AND root.id=cause.model_request_evidence_id
            AND root.external_effect_id=p.external_effect_id AND root.request_kind='embeddings'
            AND root.cause_kind='qualification_probe' AND root.cause_id=q.qualification_job_id
            AND root.qualification_target_binding_id=target.id
        JOIN qualification_q1_mre_sources source ON source.workspace_id=root.workspace_id AND source.evidence_root_id=root.id AND source.probe_ordinal='90'
        WHERE s.workspace_id=target_workspace AND s.id=target_registration AND s.registration_kind='canonical'
          AND 'embeddings'=ANY(q.capabilities) AND s.returned_model=m.wire_model_id
          AND s.adapter_profile_revision=c.adapter_profile_revision
          AND r.request_kind='embeddings' AND NOT r.stream AND cardinality(r.input_roles)=0
          AND s.encoding_format='float' AND s.dimensions>0
          AND (m.observed_embedding_dimension IS NULL OR m.observed_embedding_dimension=s.dimensions)
          AND (SELECT status='complete' FROM model_request_evidence_checks ch
               WHERE ch.workspace_id=root.workspace_id AND ch.evidence_root_id=root.id
               ORDER BY ch.checked_at DESC,ch.id DESC LIMIT 1)
    ) THEN
        RAISE EXCEPTION 'canonical embedding space requires exact qualified structural evidence' USING ERRCODE='23514';
    END IF;
END $$;

CREATE FUNCTION vestrace_register_canonical_embedding_space(
    target_registration UUID,target_workspace UUID,target_name TEXT,target_model UUID,
    target_qualification UUID,target_shape UUID,target_returned_model TEXT,target_encoding TEXT,target_dimensions INTEGER)
RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE profile TEXT; stored UUID;
BEGIN
    IF target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
       OR target_workspace IS NULL OR target_registration IS NULL OR target_registration='00000000-0000-0000-0000-000000000000'::UUID
       OR target_model IS NULL OR target_qualification IS NULL OR target_shape IS NULL
       OR target_name IS NULL OR btrim(target_name)='' OR target_returned_model IS NULL
       OR target_encoding IS DISTINCT FROM 'float' OR target_dimensions IS NULL OR target_dimensions<=0 THEN
        RAISE EXCEPTION 'canonical embedding registration arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT c.adapter_profile_revision INTO profile FROM model_revisions m JOIN connection_revisions c
      ON c.workspace_id=m.workspace_id AND c.id=m.connection_revision_id
      WHERE m.workspace_id=target_workspace AND m.id=target_model;
    SELECT id INTO stored FROM embedding_space_registrations WHERE workspace_id=target_workspace AND registration_kind='canonical'
      AND name=target_name AND model_revision_id=target_model AND model_qualification_revision_id=target_qualification
      AND request_shape_revision_id=target_shape AND adapter_profile_revision=profile
      AND returned_model=target_returned_model AND encoding_format=target_encoding AND dimensions=target_dimensions;
    IF FOUND THEN PERFORM vestrace_assert_canonical_embedding_space(target_workspace,stored); RETURN stored; END IF;
    INSERT INTO embedding_space_registrations(id,workspace_id,space_id,name,model,dimensions,registration_kind,
        model_revision_id,model_qualification_revision_id,request_shape_revision_id,adapter_profile_revision,returned_model,encoding_format)
    VALUES(target_registration,target_workspace,NULL,target_name,target_returned_model,target_dimensions,'canonical',
        target_model,target_qualification,target_shape,profile,target_returned_model,target_encoding)
    ON CONFLICT(workspace_id,name,model_revision_id,model_qualification_revision_id,
        adapter_profile_revision,request_shape_revision_id,returned_model,encoding_format,dimensions)
        WHERE registration_kind='canonical' DO NOTHING RETURNING id INTO stored;
    IF stored IS NULL THEN
        SELECT id INTO stored FROM embedding_space_registrations WHERE workspace_id=target_workspace AND registration_kind='canonical'
          AND name=target_name AND model_revision_id=target_model AND model_qualification_revision_id=target_qualification
          AND request_shape_revision_id=target_shape AND adapter_profile_revision=profile
          AND returned_model=target_returned_model AND encoding_format=target_encoding AND dimensions=target_dimensions;
        IF stored IS NULL THEN RAISE EXCEPTION 'canonical embedding registration replay conflict' USING ERRCODE='23514'; END IF;
    END IF;
    PERFORM vestrace_assert_canonical_embedding_space(target_workspace,stored);
    RETURN stored;
END $$;

CREATE FUNCTION vestrace_set_initial_embedding_active_space(
    target_workspace UUID,target_model UUID,expected_version BIGINT,target_registration UUID)
RETURNS BIGINT LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE next_version BIGINT;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'canonical active space requires exact workspace' USING ERRCODE='23514';
    END IF;
    PERFORM vestrace_assert_canonical_embedding_space(target_workspace,target_registration);
    UPDATE model_qualification_heads h SET active_space_registration_id=target_registration,version=h.version+1,updated_at=NOW()
    FROM embedding_space_registrations s JOIN model_qualification_revisions q
      ON q.workspace_id=s.workspace_id AND q.id=s.model_qualification_revision_id
    WHERE q.valid_until>NOW() AND 'embeddings'=ANY(q.capabilities) AND h.workspace_id=target_workspace AND h.model_revision_id=target_model AND h.version=expected_version
      AND h.active_space_registration_id IS NULL AND s.workspace_id=h.workspace_id AND s.id=target_registration
      AND s.model_revision_id=h.model_revision_id AND s.model_qualification_revision_id=h.current_qualification_revision_id
    RETURNING h.version INTO next_version;
    IF NOT FOUND THEN RAISE EXCEPTION 'initial embedding active space compare-and-swap conflict' USING ERRCODE='23514'; END IF;
    RETURN next_version;
END $$;

CREATE FUNCTION vestrace_validate_embedding_active_space()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF EXISTS(SELECT 1 FROM model_qualification_heads h LEFT JOIN embedding_space_registrations s
        ON s.workspace_id=h.workspace_id AND s.id=h.active_space_registration_id
        LEFT JOIN model_revisions m ON m.workspace_id=h.workspace_id AND m.id=h.model_revision_id
        WHERE h.workspace_id=NEW.workspace_id AND h.model_revision_id=NEW.model_revision_id
          AND h.active_space_registration_id IS NOT NULL
          AND (s.id IS NULL OR m.kind IS DISTINCT FROM 'embedding' OR s.registration_kind IS DISTINCT FROM 'canonical'
            OR s.model_revision_id IS DISTINCT FROM h.model_revision_id
            OR s.model_qualification_revision_id IS DISTINCT FROM h.current_qualification_revision_id)) THEN
        RAISE EXCEPTION 'embedding active space must match the exact qualification head' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE FUNCTION vestrace_invalidate_canonical_generation_guard()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    -- 0195 changes only epoch. A canonical publication changes the version and
    -- pointer explicitly; corpus invalidation must never leave that pointer current.
    IF NEW.generation_epoch IS DISTINCT FROM OLD.generation_epoch
       AND NEW.guard_version=OLD.guard_version AND NEW.current_generation_id IS NOT DISTINCT FROM OLD.current_generation_id THEN
        NEW.current_generation_id:=NULL;
        NEW.guard_version:=OLD.guard_version+1;
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION vestrace_capture_embedding_generation(
    target_generation UUID,target_workspace UUID,target_registration UUID,expected_guard_version BIGINT)
RETURNS TABLE(generation_id UUID,generation_epoch BIGINT,guard_version BIGINT,corpus_revision BIGINT,built_through_projection_ordinal BIGINT,member_count BIGINT)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE c embedding_space_corpus_states%ROWTYPE; g embedding_index_generation_guards%ROWTYPE;
    count_live BIGINT; watermark BIGINT; next_ordinal BIGINT;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
       OR target_generation IS NULL OR target_generation='00000000-0000-0000-0000-000000000000'::UUID THEN
        RAISE EXCEPTION 'canonical generation capture arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM vestrace_assert_canonical_embedding_space(target_workspace,target_registration);
    SELECT * INTO STRICT c FROM embedding_space_corpus_states s WHERE s.workspace_id=target_workspace AND s.space_registration_id=target_registration FOR UPDATE;
    SELECT * INTO STRICT g FROM embedding_index_generation_guards s WHERE s.workspace_id=target_workspace AND s.space_registration_id=target_registration FOR UPDATE;
    IF g.guard_version IS DISTINCT FROM expected_guard_version THEN
        RAISE EXCEPTION 'canonical generation capture guard conflict' USING ERRCODE='23514';
    END IF;
    PERFORM p.id FROM embedding_projection_entries p JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
      WHERE p.workspace_id=target_workspace AND p.space_registration_id=target_registration AND p.state='live' AND m.state='live'
      ORDER BY p.projection_ordinal,p.id FOR SHARE OF p,m;
    SELECT count(*),coalesce(max(p.projection_ordinal),0) INTO count_live,watermark
      FROM embedding_projection_entries p JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
      WHERE p.workspace_id=target_workspace AND p.space_registration_id=target_registration AND p.state='live' AND m.state='live';
    IF count_live<>c.live_member_count THEN RAISE EXCEPTION 'canonical corpus live count mismatch' USING ERRCODE='23514'; END IF;
    SELECT coalesce(max(s.ordinal),0)+1 INTO next_ordinal FROM embedding_corpus_generations s
      WHERE s.workspace_id=target_workspace AND s.space_registration_id=target_registration;
    INSERT INTO embedding_corpus_generations(id,workspace_id,space_registration_id,ordinal,member_count,state,
        member_representation,generation_epoch,captured_guard_version,corpus_revision,built_through_projection_ordinal)
      VALUES(target_generation,target_workspace,target_registration,next_ordinal,count_live,'building','encrypted_projection',g.generation_epoch+1,g.guard_version,c.corpus_revision,watermark);
    INSERT INTO embedding_corpus_generation_members(workspace_id,corpus_generation_id,member_ordinal,embedding_projection_entry_id)
      SELECT target_workspace,target_generation,p.projection_ordinal,p.id FROM embedding_projection_entries p
        JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
        WHERE p.workspace_id=target_workspace AND p.space_registration_id=target_registration AND p.state='live' AND m.state='live';
    RETURN QUERY SELECT target_generation,g.generation_epoch+1,g.guard_version,c.corpus_revision,watermark,count_live;
END $$;

CREATE FUNCTION vestrace_publish_embedding_generation(
    target_workspace UUID,target_registration UUID,target_generation UUID,expected_guard_version BIGINT)
RETURNS TABLE(generation_epoch BIGINT,guard_version BIGINT)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE c embedding_space_corpus_states%ROWTYPE; guard_row embedding_index_generation_guards%ROWTYPE;
    target embedding_corpus_generations%ROWTYPE; live_count BIGINT; watermark BIGINT;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'canonical publication requires exact workspace' USING ERRCODE='23514';
    END IF;
    PERFORM vestrace_assert_canonical_embedding_space(target_workspace,target_registration);
    SELECT * INTO STRICT c FROM embedding_space_corpus_states s WHERE s.workspace_id=target_workspace AND s.space_registration_id=target_registration FOR UPDATE;
    SELECT * INTO STRICT guard_row FROM embedding_index_generation_guards s WHERE s.workspace_id=target_workspace AND s.space_registration_id=target_registration FOR UPDATE;
    SELECT * INTO target FROM embedding_corpus_generations s WHERE s.workspace_id=target_workspace AND s.space_registration_id=target_registration AND s.id=target_generation FOR UPDATE;
    IF target.id IS NULL OR target.state<>'building' OR target.member_representation<>'encrypted_projection'
      OR target.captured_guard_version IS DISTINCT FROM expected_guard_version OR guard_row.guard_version IS DISTINCT FROM expected_guard_version
      OR target.generation_epoch<>guard_row.generation_epoch+1 OR target.corpus_revision<>c.corpus_revision THEN
        RAISE EXCEPTION 'canonical generation publication capture conflict' USING ERRCODE='23514';
    END IF;
    PERFORM p.id FROM embedding_projection_entries p JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
      WHERE p.workspace_id=target_workspace AND p.space_registration_id=target_registration AND p.state='live' AND m.state='live'
      ORDER BY p.projection_ordinal,p.id FOR SHARE OF p,m;
    SELECT count(*),coalesce(max(p.projection_ordinal),0) INTO live_count,watermark
      FROM embedding_projection_entries p JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
      WHERE p.workspace_id=target_workspace AND p.space_registration_id=target_registration AND p.state='live' AND m.state='live';
    IF live_count<>target.member_count OR live_count<>c.live_member_count OR watermark<>target.built_through_projection_ordinal
      OR live_count<>(SELECT count(*) FROM embedding_corpus_generation_members s WHERE s.workspace_id=target_workspace AND s.corpus_generation_id=target_generation)
      OR EXISTS(SELECT 1 FROM embedding_projection_entries p JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
        WHERE p.workspace_id=target_workspace AND p.space_registration_id=target_registration AND p.state='live' AND m.state='live'
          AND NOT EXISTS(SELECT 1 FROM embedding_corpus_generation_members s WHERE s.workspace_id=p.workspace_id AND s.corpus_generation_id=target_generation AND s.embedding_projection_entry_id=p.id)) THEN
        RAISE EXCEPTION 'canonical generation publication requires its exact Live member set' USING ERRCODE='23514';
    END IF;
    UPDATE embedding_corpus_generations SET state='stale' WHERE workspace_id=target_workspace AND space_registration_id=target_registration AND state='ready';
    UPDATE embedding_corpus_generations SET state='ready',published_at=NOW() WHERE workspace_id=target_workspace AND id=target_generation;
    UPDATE embedding_index_generation_guards SET generation_epoch=target.generation_epoch,current_generation_id=target_generation,guard_version=guard_row.guard_version+1
      WHERE workspace_id=target_workspace AND space_registration_id=target_registration;
    RETURN QUERY SELECT target.generation_epoch,guard_row.guard_version+1;
END $$;

CREATE FUNCTION vestrace_set_canonical_generation_lifecycle()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF NEW.member_representation='legacy_upgrade' THEN
        IF TG_OP='UPDATE' THEN
            NEW.created_at:=OLD.created_at; NEW.published_at:=OLD.published_at;
            NEW.state_changed_at:=OLD.state_changed_at; NEW.lifecycle_reason:=OLD.lifecycle_reason;
            IF NEW.state IS DISTINCT FROM OLD.state THEN
                NEW.state_changed_at:=NOW();
                NEW.lifecycle_reason:=CASE NEW.state WHEN 'stale' THEN 'invalidated' WHEN 'revoked' THEN 'revoked' ELSE 'legacy_upgrade' END;
            END IF;
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP='UPDATE' THEN
        NEW.created_at:=OLD.created_at;
        NEW.published_at:=OLD.published_at;
        NEW.state_changed_at:=CASE WHEN NEW.state IS DISTINCT FROM OLD.state THEN NOW() ELSE OLD.state_changed_at END;
    ELSE
        NEW.created_at:=NOW(); NEW.state_changed_at:=NOW(); NEW.published_at:=NULL;
    END IF;
    NEW.lifecycle_reason:=CASE NEW.state WHEN 'building' THEN 'captured' WHEN 'ready' THEN 'published'
        WHEN 'stale' THEN 'invalidated' WHEN 'revoked' THEN 'revoked' END;
    IF NEW.state='building' THEN NEW.published_at:=NULL;
    ELSIF NEW.state='ready' THEN NEW.published_at:=coalesce(NEW.published_at,NOW()); END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION vestrace_normalize_legacy_generation_member()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF NEW.embedding_projection_entry_id IS NULL THEN
        RAISE EXCEPTION 'embedding-legacy-adoption-required' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_corpus_generation_member()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE target_workspace UUID; target_generation UUID; generation embedding_corpus_generations%ROWTYPE;
BEGIN
    target_workspace:=coalesce(NEW.workspace_id,OLD.workspace_id);
    target_generation:=coalesce(NEW.corpus_generation_id,OLD.corpus_generation_id);
    SELECT * INTO generation FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=target_generation;
    IF generation.id IS NULL THEN RAISE EXCEPTION 'generation member requires exact generation' USING ERRCODE='23514'; END IF;
    IF TG_OP='UPDATE' THEN RAISE EXCEPTION 'generation membership is immutable' USING ERRCODE='23514'; END IF;
    IF generation.member_representation='encrypted_projection' AND generation.member_count<>(
        SELECT count(*) FROM embedding_corpus_generation_members member
          WHERE member.workspace_id=target_workspace AND member.corpus_generation_id=target_generation) THEN
        RAISE EXCEPTION 'canonical generation membership must preserve captured count' USING ERRCODE='23514';
    END IF;

    IF TG_OP='DELETE' AND generation.state='ready' THEN
        RAISE EXCEPTION 'embedding corpus members may not be removed from a ready generation' USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_corpus_generation_members member
      JOIN embedding_space_registrations registration ON registration.workspace_id=generation.workspace_id AND registration.id=generation.space_registration_id
      LEFT JOIN memory_embeddings legacy ON legacy.workspace_id=member.workspace_id AND legacy.id=member.legacy_embedding_id
      LEFT JOIN embedding_projection_entries p ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
      LEFT JOIN content_materials material ON material.workspace_id=p.workspace_id AND material.id=p.material_id
      WHERE member.workspace_id=target_workspace AND member.corpus_generation_id=target_generation
        AND ((generation.member_representation='legacy_upgrade' AND (registration.registration_kind<>'legacy_upgrade'
            OR legacy.id IS NULL OR legacy.space_id IS DISTINCT FROM registration.space_id OR member.embedding_projection_entry_id IS NOT NULL))
          OR (generation.member_representation='encrypted_projection' AND (registration.registration_kind<>'canonical'
            OR member.legacy_embedding_id IS NOT NULL OR p.id IS NULL OR p.state<>'live' OR material.state IS DISTINCT FROM 'live'
            OR member.member_ordinal IS DISTINCT FROM p.projection_ordinal
            OR p.space_registration_id IS DISTINCT FROM generation.space_registration_id)))) THEN
        RAISE EXCEPTION 'canonical generation requires exact Live projection members or exact legacy members' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE FUNCTION vestrace_validate_canonical_generation()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE g embedding_corpus_generations%ROWTYPE;
BEGIN
    IF TG_OP='DELETE' THEN
        IF OLD.member_representation='encrypted_projection' THEN
            RAISE EXCEPTION 'canonical generation history cannot be deleted' USING ERRCODE='23514';
        END IF;
        RETURN NULL;
    END IF;
    SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=NEW.workspace_id AND id=NEW.id;
    IF g.id IS NULL THEN RETURN NULL; END IF;
    IF TG_OP='UPDATE' AND (OLD.member_representation='encrypted_projection' OR NEW.member_representation='encrypted_projection')
      AND (NEW.id,NEW.workspace_id,NEW.space_registration_id,NEW.ordinal,NEW.member_count,NEW.member_representation,
        NEW.generation_epoch,NEW.captured_guard_version,NEW.corpus_revision,NEW.built_through_projection_ordinal)
      IS DISTINCT FROM (OLD.id,OLD.workspace_id,OLD.space_registration_id,OLD.ordinal,OLD.member_count,OLD.member_representation,
        OLD.generation_epoch,OLD.captured_guard_version,OLD.corpus_revision,OLD.built_through_projection_ordinal) THEN
        RAISE EXCEPTION 'canonical generation snapshot is immutable' USING ERRCODE='23514';
    END IF;
    IF g.member_representation='legacy_upgrade' THEN
        IF g.state='ready' AND (TG_OP='INSERT' OR OLD.state IS DISTINCT FROM 'ready') THEN
            RAISE EXCEPTION 'embedding-legacy-adoption-required' USING ERRCODE='23514';
        END IF;
        RETURN NULL;
    END IF;
    PERFORM vestrace_assert_canonical_embedding_space(g.workspace_id,g.space_registration_id);
    IF TG_OP='UPDATE' AND NEW.state IS DISTINCT FROM OLD.state AND NOT (
        (OLD.state='building' AND NEW.state IN ('ready','stale','revoked'))
        OR (OLD.state='ready' AND NEW.state IN ('stale','revoked'))
        OR (OLD.state='stale' AND NEW.state='revoked')) THEN
        RAISE EXCEPTION 'canonical generation transition cannot reopen history' USING ERRCODE='23514';
    END IF;
    IF g.state<>'ready' AND EXISTS(SELECT 1 FROM embedding_index_generation_guards guard
        WHERE guard.workspace_id=g.workspace_id AND guard.current_generation_id=g.id) THEN
        RAISE EXCEPTION 'only Ready canonical generation can remain current' USING ERRCODE='23514';
    END IF;

    IF g.member_count<>(SELECT count(*) FROM embedding_corpus_generation_members m WHERE m.workspace_id=g.workspace_id AND m.corpus_generation_id=g.id) THEN
        RAISE EXCEPTION 'canonical generation member count must be exact' USING ERRCODE='23514';
    END IF;
    IF g.state='ready' AND NOT EXISTS(SELECT 1 FROM embedding_index_generation_guards guard
        JOIN embedding_space_corpus_states c USING(workspace_id,space_registration_id)
        WHERE guard.workspace_id=g.workspace_id AND guard.space_registration_id=g.space_registration_id
          AND guard.current_generation_id=g.id AND guard.generation_epoch=g.generation_epoch
          AND guard.guard_version=g.captured_guard_version+1 AND c.corpus_revision=g.corpus_revision AND c.live_member_count=g.member_count) THEN
        RAISE EXCEPTION 'Ready canonical generation requires its exact current guard and corpus' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE FUNCTION vestrace_validate_canonical_generation_guard()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'canonical generation guard is permanent' USING ERRCODE='23514';
    END IF;
    IF TG_OP='UPDATE' AND (NEW.workspace_id,NEW.space_registration_id)
        IS DISTINCT FROM (OLD.workspace_id,OLD.space_registration_id) THEN
        RAISE EXCEPTION 'canonical generation guard identity is immutable' USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_index_generation_guards guard LEFT JOIN embedding_corpus_generations g
        ON g.workspace_id=guard.workspace_id AND g.id=guard.current_generation_id AND g.space_registration_id=guard.space_registration_id
        JOIN embedding_space_corpus_states c ON c.workspace_id=guard.workspace_id AND c.space_registration_id=guard.space_registration_id
        WHERE guard.workspace_id=NEW.workspace_id AND guard.space_registration_id=NEW.space_registration_id AND guard.current_generation_id IS NOT NULL
          AND (g.id IS NULL OR g.state<>'ready' OR g.member_representation<>'encrypted_projection'
            OR g.generation_epoch IS DISTINCT FROM guard.generation_epoch OR g.captured_guard_version+1 IS DISTINCT FROM guard.guard_version
            OR g.corpus_revision IS DISTINCT FROM c.corpus_revision OR g.member_count IS DISTINCT FROM c.live_member_count)) THEN
        RAISE EXCEPTION 'canonical generation guard requires exact Ready snapshot' USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_corpus_generations g
        JOIN embedding_index_generation_guards guard ON guard.workspace_id=g.workspace_id
          AND guard.space_registration_id=g.space_registration_id
        WHERE g.workspace_id=NEW.workspace_id AND g.space_registration_id=NEW.space_registration_id
          AND g.member_representation='encrypted_projection' AND g.state='ready'
          AND g.id IS DISTINCT FROM guard.current_generation_id) THEN
        RAISE EXCEPTION 'Ready canonical generation cannot be orphaned from its guard' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

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

    PERFORM 1
      FROM embedding_spaces AS legacy_space
     WHERE legacy_space.id = target_space_id
       AND legacy_space.workspace_id = target_workspace_id
       AND legacy_space.name = target_name
       AND legacy_space.model = target_model
       AND legacy_space.dimensions = target_dimensions;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding space registration requires a matching legacy embedding space'
            USING ERRCODE = '23514';
    END IF;

    -- Registration is idempotent on the exact tuple: the same key registered
    -- twice returns the first registration rather than conflicting, because the
    -- caller that retries after a crash must converge on one identity.
    SELECT id INTO existing_id
      FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id AND registration_kind = 'legacy_upgrade'
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
        id, workspace_id, space_id, name, model, dimensions, registration_kind
    ) VALUES (
        target_registration_id, target_workspace_id, target_space_id,
        target_name, target_model, target_dimensions, 'legacy_upgrade'
    );
    RETURN target_registration_id;
END
$$;

CREATE FUNCTION vestrace_validate_canonical_space_registration()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='UPDATE' AND NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'embedding registration identity is immutable' USING ERRCODE='23514';
    END IF;
    IF NEW.registration_kind='canonical' THEN
        PERFORM vestrace_assert_canonical_embedding_space(NEW.workspace_id,NEW.id);
    END IF;
    RETURN NULL;
END $$;

CREATE FUNCTION vestrace_validate_canonical_member_liveness()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF EXISTS(SELECT 1 FROM embedding_corpus_generation_members member
        JOIN embedding_corpus_generations g ON g.workspace_id=member.workspace_id AND g.id=member.corpus_generation_id
        JOIN embedding_projection_entries p ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
        LEFT JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
        WHERE member.workspace_id=NEW.workspace_id AND g.member_representation='encrypted_projection'
          AND ((TG_TABLE_NAME='content_materials' AND p.material_id=NEW.id) OR (TG_TABLE_NAME='embedding_projection_entries' AND p.id=NEW.id))
          AND (p.state<>'live' OR m.state IS DISTINCT FROM 'live' OR p.space_registration_id<>g.space_registration_id)) THEN
        RAISE EXCEPTION 'canonical generation member must remain Live' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

DO $upgrade$ BEGIN
    IF to_regprocedure('public.vestrace_finish_canonical_generation_upgrade()') IS NOT NULL THEN
        PERFORM vestrace_finish_canonical_generation_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
            RAISE EXCEPTION 'canonical generation finish must be provisioned' USING ERRCODE='42501';
        END IF;
CREATE CONSTRAINT TRIGGER model_qualification_heads_active_space_consistent
    AFTER INSERT OR UPDATE ON model_qualification_heads DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_active_space();
CREATE TRIGGER embedding_index_generation_guards_corpus_invalidation
    BEFORE UPDATE ON embedding_index_generation_guards FOR EACH ROW
    EXECUTE FUNCTION vestrace_invalidate_canonical_generation_guard();
CREATE TRIGGER embedding_corpus_generations_lifecycle
    BEFORE INSERT OR UPDATE ON embedding_corpus_generations FOR EACH ROW
    EXECUTE FUNCTION vestrace_set_canonical_generation_lifecycle();
CREATE TRIGGER embedding_corpus_generation_members_legacy_alias
    BEFORE INSERT ON embedding_corpus_generation_members FOR EACH ROW
    EXECUTE FUNCTION vestrace_normalize_legacy_generation_member();
CREATE CONSTRAINT TRIGGER embedding_corpus_generation_members_consistent
    AFTER INSERT OR UPDATE OR DELETE ON embedding_corpus_generation_members DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_corpus_generation_member();
CREATE CONSTRAINT TRIGGER embedding_corpus_generations_canonical_consistent
    AFTER INSERT OR UPDATE OR DELETE ON embedding_corpus_generations DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_generation();
CREATE CONSTRAINT TRIGGER embedding_index_generation_guards_current_consistent
    AFTER INSERT OR UPDATE OR DELETE ON embedding_index_generation_guards DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_generation_guard();
CREATE CONSTRAINT TRIGGER embedding_space_registrations_canonical_consistent
    AFTER INSERT OR UPDATE ON embedding_space_registrations DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_space_registration();
CREATE CONSTRAINT TRIGGER content_materials_canonical_generation_live
    AFTER UPDATE ON content_materials DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_member_liveness();
CREATE CONSTRAINT TRIGGER embedding_projections_canonical_generation_live
    AFTER UPDATE ON embedding_projection_entries DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_member_liveness();
ALTER TABLE public.memory_embeddings OWNER TO vestrace_guarded_owner;
ALTER TABLE public.memory_embeddings ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.memory_embeddings FORCE ROW LEVEL SECURITY;
REVOKE ALL ON TABLE public.memory_embeddings FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_register_canonical_embedding_space(uuid,uuid,text,uuid,uuid,uuid,text,text,integer) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_register_canonical_embedding_space(uuid,uuid,text,uuid,uuid,uuid,text,text,integer) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_set_initial_embedding_active_space(uuid,uuid,bigint,uuid) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_set_initial_embedding_active_space(uuid,uuid,bigint,uuid) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_capture_embedding_generation(uuid,uuid,uuid,bigint) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_capture_embedding_generation(uuid,uuid,uuid,bigint) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_publish_embedding_generation(uuid,uuid,uuid,bigint) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_publish_embedding_generation(uuid,uuid,uuid,bigint) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_assert_canonical_embedding_space(uuid,uuid) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_assert_canonical_embedding_space(uuid,uuid) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_embedding_active_space() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_embedding_active_space() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_invalidate_canonical_generation_guard() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_invalidate_canonical_generation_guard() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_set_canonical_generation_lifecycle() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_set_canonical_generation_lifecycle() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_normalize_legacy_generation_member() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_normalize_legacy_generation_member() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_embedding_corpus_generation_member() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_embedding_corpus_generation_member() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_canonical_generation() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_canonical_generation() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_canonical_generation_guard() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_canonical_generation_guard() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_canonical_space_registration() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_canonical_space_registration() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_canonical_member_liveness() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_canonical_member_liveness() FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_register_canonical_embedding_space(uuid,uuid,text,uuid,uuid,uuid,text,text,integer) TO vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_set_initial_embedding_active_space(uuid,uuid,bigint,uuid) TO vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_capture_embedding_generation(uuid,uuid,uuid,bigint) TO vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_publish_embedding_generation(uuid,uuid,uuid,bigint) TO vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer) TO vestrace;

    END IF;
END $upgrade$;
