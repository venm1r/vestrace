-- P04 disposable index work. No plaintext vectors or process-local loaded flags.
DO $upgrade$ BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_index_upgrade()') IS NOT NULL THEN
        PERFORM vestrace_prepare_embedding_index_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
            RAISE EXCEPTION 'embedding index upgrade must be provisioned' USING ERRCODE='42501';
        END IF;
-- BEGIN INDEX DDL
ALTER TABLE embedding_index_rebuild_events
    ALTER COLUMN publication_id DROP NOT NULL,
    ADD COLUMN cause TEXT NOT NULL DEFAULT 'result_publication'
        CHECK(cause IN ('result_publication','material_erasure','transition_publication','legacy_cutover','operator_rebuild')),
    ADD COLUMN material_erasure_preparation_id UUID,
    ADD COLUMN transition_activation_receipt_id UUID,
    ADD COLUMN legacy_cutover_receipt_id UUID,
    ADD COLUMN operator_rebuild_request_id UUID,
    ADD UNIQUE(workspace_id,id),
    ADD CONSTRAINT embedding_index_change_cause_xor CHECK(
        num_nonnulls(publication_id,material_erasure_preparation_id,transition_activation_receipt_id,legacy_cutover_receipt_id,operator_rebuild_request_id)=1
        AND ((cause='result_publication' AND publication_id IS NOT NULL)
          OR (cause='material_erasure' AND material_erasure_preparation_id IS NOT NULL)
          OR (cause='transition_publication' AND transition_activation_receipt_id IS NOT NULL)
          OR (cause='legacy_cutover' AND legacy_cutover_receipt_id IS NOT NULL)
          OR (cause='operator_rebuild' AND operator_rebuild_request_id IS NOT NULL)));
CREATE TABLE embedding_index_build_attempts (
    id UUID PRIMARY KEY,workspace_id UUID NOT NULL,space_registration_id UUID NOT NULL,
    generation_id UUID NOT NULL,event_id UUID,
    cause TEXT NOT NULL CHECK(cause IN ('corpus_change','startup','lazy_load')),
    claim_owner TEXT NOT NULL CHECK(length(claim_owner) BETWEEN 1 AND 128),
    claim_deadline TIMESTAMPTZ NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('claimed','building','published','loaded','discarded','failed')),
    safe_reason TEXT CHECK(safe_reason IN ('invalid_vector','memory_limit','material_unavailable','generation_changed','storage_unavailable','claim_expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),terminal_at TIMESTAMPTZ,
    UNIQUE(workspace_id,id),
    FOREIGN KEY(workspace_id,space_registration_id) REFERENCES embedding_space_registrations(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,generation_id) REFERENCES embedding_corpus_generations(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,event_id) REFERENCES embedding_index_rebuild_events(workspace_id,id) ON DELETE RESTRICT,
    CHECK((cause='corpus_change' AND event_id IS NOT NULL) OR (cause IN ('startup','lazy_load') AND event_id IS NULL)),
    CHECK(claim_deadline>created_at),
    CHECK((state IN ('claimed','building') AND terminal_at IS NULL AND safe_reason IS NULL)
       OR (state IN ('published','loaded') AND terminal_at IS NOT NULL AND safe_reason IS NULL)
       OR (state IN ('discarded','failed') AND terminal_at IS NOT NULL AND safe_reason IS NOT NULL))
);
CREATE INDEX embedding_index_attempt_claims ON embedding_index_build_attempts(workspace_id,space_registration_id,claim_deadline) WHERE state IN ('claimed','building');
CREATE UNIQUE INDEX embedding_index_attempt_active_owner ON embedding_index_build_attempts(workspace_id,generation_id,cause,claim_owner) WHERE state IN ('claimed','building');
CREATE TABLE embedding_index_build_observations (
    id UUID PRIMARY KEY,workspace_id UUID NOT NULL,attempt_id UUID NOT NULL,
    safe_reason TEXT NOT NULL CHECK(safe_reason IN ('invalid_vector','memory_limit','material_unavailable','generation_changed','storage_unavailable','claim_expired')),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id,attempt_id,safe_reason),
    FOREIGN KEY(workspace_id,attempt_id) REFERENCES embedding_index_build_attempts(workspace_id,id) ON DELETE RESTRICT
);
-- END INDEX DDL
    END IF;
END $upgrade$;

CREATE FUNCTION vestrace_validate_embedding_index_event_cause()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    -- Their exact authority tables/transactions arrive in the owning later tasks.
    -- Merely supplying a UUID never enables an unattached corpus-change cause.
    IF NEW.cause<>'result_publication' THEN
        RAISE EXCEPTION 'embedding index change cause authority is not installed' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE FUNCTION vestrace_embedding_index_snapshot(target_workspace UUID,target_generation UUID)
RETURNS JSONB LANGUAGE sql SECURITY DEFINER SET search_path=public,pg_temp AS $$
    SELECT jsonb_build_object('workspace_id',g.workspace_id,'name',s.name,'dimensions',s.dimensions,
        'model_revision_id',s.model_revision_id,'model_qualification_revision_id',s.model_qualification_revision_id,
        'request_shape_revision_id',s.request_shape_revision_id,'adapter_profile_revision',s.adapter_profile_revision,
        'returned_model',s.returned_model,'encoding_format',s.encoding_format,
        'generation_id',g.id,'generation_epoch',g.generation_epoch,'guard_version',g.captured_guard_version+1,
        'corpus_revision',g.corpus_revision,'built_through_projection_ordinal',g.built_through_projection_ordinal,'member_count',g.member_count)
    FROM embedding_corpus_generations g JOIN embedding_space_registrations s ON s.workspace_id=g.workspace_id AND s.id=g.space_registration_id
    WHERE g.workspace_id=target_workspace AND g.id=target_generation AND g.member_representation='encrypted_projection' AND s.registration_kind='canonical';
$$;
CREATE FUNCTION vestrace_embedding_index_attempt_plan(target_workspace UUID,target_attempt UUID)
RETURNS JSONB LANGUAGE sql SECURITY DEFINER SET search_path=public,pg_temp AS $$
    SELECT jsonb_build_object('attempt_id',a.id,'space_registration_id',a.space_registration_id,'purpose',a.cause,
        'event_id',a.event_id,'owner',a.claim_owner,'snapshot',vestrace_embedding_index_snapshot(a.workspace_id,a.generation_id))
    FROM embedding_index_build_attempts a WHERE a.workspace_id=target_workspace AND a.id=target_attempt;
$$;
CREATE FUNCTION vestrace_validate_embedding_index_attempt()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='DELETE' THEN RAISE EXCEPTION 'embedding index attempt history is immutable' USING ERRCODE='23514'; END IF;
    IF TG_OP='UPDATE' THEN
        IF (NEW.id,NEW.workspace_id,NEW.space_registration_id,NEW.generation_id,NEW.event_id,NEW.cause,NEW.claim_owner,NEW.claim_deadline,NEW.created_at)
            IS DISTINCT FROM (OLD.id,OLD.workspace_id,OLD.space_registration_id,OLD.generation_id,OLD.event_id,OLD.cause,OLD.claim_owner,OLD.claim_deadline,OLD.created_at)
            OR OLD.state IN ('published','loaded','discarded','failed') THEN
            RAISE EXCEPTION 'embedding index attempt cannot rewrite captured or terminal history' USING ERRCODE='23514';
        END IF;
        IF NEW.state IS DISTINCT FROM OLD.state AND NOT ((OLD.state='claimed' AND NEW.state IN ('building','discarded','failed'))
          OR (OLD.state='building' AND NEW.state IN ('published','loaded','discarded','failed'))) THEN
            RAISE EXCEPTION 'embedding index attempt transition is invalid' USING ERRCODE='23514';
        END IF;
    END IF;
    IF NOT EXISTS(SELECT 1 FROM embedding_corpus_generations g WHERE g.workspace_id=NEW.workspace_id AND g.id=NEW.generation_id
        AND g.space_registration_id=NEW.space_registration_id AND g.member_representation='encrypted_projection') THEN
        RAISE EXCEPTION 'embedding index attempt requires exact canonical generation' USING ERRCODE='23514';
    END IF;
    IF NEW.cause='startup' AND NOT EXISTS(SELECT 1 FROM embedding_corpus_generations g
        WHERE g.workspace_id=NEW.workspace_id AND g.id=NEW.generation_id
          AND g.corpus_revision=0 AND g.built_through_projection_ordinal=0 AND g.member_count=0) THEN
        RAISE EXCEPTION 'embedding index startup requires initial empty corpus' USING ERRCODE='23514';
    END IF;
    IF NEW.event_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM embedding_index_rebuild_events e JOIN embedding_corpus_generations g
        ON g.workspace_id=e.workspace_id AND g.space_registration_id=e.space_registration_id
        WHERE e.workspace_id=NEW.workspace_id AND e.id=NEW.event_id AND g.id=NEW.generation_id
          AND e.after_corpus_revision=g.corpus_revision AND e.after_live_member_count=g.member_count
          AND e.built_through_projection_ordinal=g.built_through_projection_ordinal) THEN
        RAISE EXCEPTION 'embedding index attempt event must match captured corpus' USING ERRCODE='23514';
    END IF;
    IF NEW.state='loaded' AND NEW.cause<>'lazy_load' OR NEW.state='published' AND NEW.cause='lazy_load' THEN
        RAISE EXCEPTION 'embedding index attempt outcome must match its cause' USING ERRCODE='23514';
    END IF;
    IF NEW.state IN ('published','loaded') AND NOT EXISTS(SELECT 1 FROM embedding_corpus_generations g
        JOIN embedding_index_generation_guards guard USING(workspace_id,space_registration_id)
        JOIN embedding_space_corpus_states c USING(workspace_id,space_registration_id)
        WHERE g.workspace_id=NEW.workspace_id AND g.id=NEW.generation_id AND g.state='ready'
          AND guard.current_generation_id=g.id AND guard.generation_epoch=g.generation_epoch
          AND guard.guard_version=g.captured_guard_version+1 AND c.corpus_revision=g.corpus_revision AND c.live_member_count=g.member_count) THEN
        RAISE EXCEPTION 'successful index attempt requires exact Ready publication authority' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE FUNCTION vestrace_claim_embedding_index_build(target_workspace UUID,target_owner TEXT,member_limit INTEGER)
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE registration UUID;guard_row embedding_index_generation_guards%ROWTYPE;corpus embedding_space_corpus_states%ROWTYPE;
    generation embedding_corpus_generations%ROWTYPE;attempt UUID;event UUID;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
      OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128 OR member_limit IS NULL OR member_limit<=0 THEN
        RAISE EXCEPTION 'embedding index claim arguments malformed' USING ERRCODE='22023';
    END IF;
    SELECT c.* INTO corpus FROM embedding_space_corpus_states c
      JOIN embedding_space_registrations s ON s.workspace_id=c.workspace_id AND s.id=c.space_registration_id
      JOIN embedding_index_generation_guards guard ON guard.workspace_id=c.workspace_id AND guard.space_registration_id=c.space_registration_id
      WHERE c.workspace_id=target_workspace AND s.registration_kind='canonical' AND guard.current_generation_id IS NULL
      ORDER BY c.space_registration_id FOR UPDATE OF c SKIP LOCKED LIMIT 1;
    IF NOT FOUND THEN RETURN NULL; END IF;
    IF corpus.live_member_count>member_limit THEN
        RAISE EXCEPTION 'embedding-index-memory-limit' USING ERRCODE='54000';
    END IF;
    registration:=corpus.space_registration_id;
    SELECT * INTO guard_row FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=registration FOR UPDATE SKIP LOCKED;
    IF NOT FOUND OR guard_row.current_generation_id IS NOT NULL THEN RETURN NULL; END IF;
    SELECT id INTO attempt FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND space_registration_id=registration
      AND claim_owner=target_owner AND claim_deadline>NOW() AND state='building' AND cause<>'lazy_load' ORDER BY created_at,id LIMIT 1;
    IF FOUND THEN RETURN vestrace_embedding_index_attempt_plan(target_workspace,attempt); END IF;
    UPDATE embedding_index_build_attempts SET state='discarded',safe_reason='claim_expired',terminal_at=NOW()
      WHERE workspace_id=target_workspace AND space_registration_id=registration AND state IN ('claimed','building') AND claim_deadline<=NOW();
    SELECT * INTO generation FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND space_registration_id=registration AND state='building' FOR UPDATE;
    IF FOUND AND (generation.captured_guard_version<>guard_row.guard_version OR generation.corpus_revision<>corpus.corpus_revision
      OR generation.generation_epoch<>guard_row.generation_epoch+1 OR generation.member_count<>corpus.live_member_count) THEN
        UPDATE embedding_corpus_generations SET state='stale' WHERE id=generation.id;
        generation.id:=NULL;
    END IF;
    IF generation.id IS NULL THEN
        generation.id:=gen_random_uuid();
        PERFORM vestrace_capture_embedding_generation(generation.id,target_workspace,registration,guard_row.guard_version);
    END IF;
    SELECT id INTO event FROM embedding_index_rebuild_events WHERE workspace_id=target_workspace AND space_registration_id=registration
      AND after_corpus_revision=corpus.corpus_revision AND after_live_member_count=corpus.live_member_count
      AND built_through_projection_ordinal=corpus.next_projection_ordinal-1 ORDER BY id LIMIT 1;
    IF event IS NULL AND NOT (corpus.corpus_revision=0 AND corpus.live_member_count=0 AND corpus.next_projection_ordinal=1) THEN
        RAISE EXCEPTION 'embedding index corpus change authority absent' USING ERRCODE='23514';
    END IF;
    attempt:=gen_random_uuid();
    INSERT INTO embedding_index_build_attempts(id,workspace_id,space_registration_id,generation_id,event_id,cause,claim_owner,claim_deadline,state)
      VALUES(attempt,target_workspace,registration,generation.id,event,CASE WHEN event IS NULL THEN 'startup' ELSE 'corpus_change' END,target_owner,NOW()+INTERVAL '60 seconds','building');
    RETURN vestrace_embedding_index_attempt_plan(target_workspace,attempt);
END $$;

CREATE FUNCTION vestrace_validate_local_embedding_generation(target_workspace UUID,target_generation UUID,target_epoch BIGINT,target_guard BIGINT,target_corpus BIGINT,target_watermark BIGINT,target_count BIGINT)
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE registration UUID;result JSONB;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding index validation requires exact workspace' USING ERRCODE='23514'; END IF;
    SELECT space_registration_id INTO registration FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=target_generation;
    IF NOT FOUND THEN RETURN NULL; END IF;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=registration FOR SHARE;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=registration FOR SHARE;
    IF NOT EXISTS(SELECT 1 FROM embedding_corpus_generations g JOIN embedding_index_generation_guards guard USING(workspace_id,space_registration_id)
        JOIN embedding_space_corpus_states c USING(workspace_id,space_registration_id)
        WHERE g.workspace_id=target_workspace AND g.id=target_generation AND g.state='ready' AND g.member_representation='encrypted_projection'
          AND guard.current_generation_id=g.id AND guard.generation_epoch=target_epoch AND g.generation_epoch=target_epoch
          AND guard.guard_version=target_guard AND g.captured_guard_version+1=target_guard
          AND g.corpus_revision=target_corpus AND c.corpus_revision=target_corpus
          AND g.built_through_projection_ordinal=target_watermark AND g.member_count=target_count AND c.live_member_count=target_count) THEN RETURN NULL; END IF;
    result:=vestrace_embedding_index_snapshot(target_workspace,target_generation);
    RETURN jsonb_build_object('space_registration_id',registration,'snapshot',result);
END $$;
CREATE FUNCTION vestrace_claim_embedding_index_load(target_workspace UUID,target_generation UUID,target_epoch BIGINT,target_guard BIGINT,target_corpus BIGINT,target_watermark BIGINT,target_count BIGINT,target_owner TEXT)
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE validated JSONB;registration UUID;attempt UUID;
BEGIN
    IF target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128 THEN RAISE EXCEPTION 'embedding index owner malformed' USING ERRCODE='22023'; END IF;
    SELECT space_registration_id INTO registration FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=target_generation;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=registration FOR UPDATE;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=registration FOR UPDATE;
    validated:=vestrace_validate_local_embedding_generation(target_workspace,target_generation,target_epoch,target_guard,target_corpus,target_watermark,target_count);
    IF validated IS NULL THEN RAISE EXCEPTION 'embedding-generation-changed' USING ERRCODE='23514'; END IF;
    registration:=(validated->>'space_registration_id')::UUID;
    SELECT id INTO attempt FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND generation_id=target_generation
      AND cause='lazy_load' AND claim_owner=target_owner AND state='building' AND claim_deadline>NOW() ORDER BY created_at,id LIMIT 1;
    IF FOUND THEN RETURN vestrace_embedding_index_attempt_plan(target_workspace,attempt); END IF;
    UPDATE embedding_index_build_attempts SET state='discarded',safe_reason='claim_expired',terminal_at=NOW()
      WHERE workspace_id=target_workspace AND generation_id=target_generation AND cause='lazy_load'
        AND claim_owner=target_owner AND state IN ('claimed','building') AND claim_deadline<=NOW();
    attempt:=gen_random_uuid();
    INSERT INTO embedding_index_build_attempts(id,workspace_id,space_registration_id,generation_id,cause,claim_owner,claim_deadline,state)
      VALUES(attempt,target_workspace,registration,target_generation,'lazy_load',target_owner,NOW()+INTERVAL '60 seconds','building');
    RETURN vestrace_embedding_index_attempt_plan(target_workspace,attempt);
END $$;

CREATE FUNCTION vestrace_lock_embedding_index_attempt(target_workspace UUID,target_attempt UUID,target_owner TEXT)
RETURNS embedding_index_build_attempts LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE a embedding_index_build_attempts%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding index attempt requires exact workspace' USING ERRCODE='23514'; END IF;
    SELECT * INTO a FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND id=target_attempt;
    IF NOT FOUND OR a.claim_owner IS DISTINCT FROM target_owner THEN RAISE EXCEPTION 'embedding index claim owner mismatch' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=a.space_registration_id FOR UPDATE;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=a.space_registration_id FOR UPDATE;
    SELECT * INTO a FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND id=target_attempt FOR UPDATE;
    RETURN a;
END $$;

CREATE FUNCTION vestrace_load_embedding_index_chunk(target_workspace UUID,target_attempt UUID,target_owner TEXT,after_ordinal BIGINT,chunk_limit INTEGER)
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE a embedding_index_build_attempts%ROWTYPE;g embedding_corpus_generations%ROWTYPE;rows JSONB;last_ordinal BIGINT;
BEGIN
    IF chunk_limit IS NULL OR chunk_limit NOT BETWEEN 1 AND 64 OR after_ordinal<0 THEN RAISE EXCEPTION 'embedding index chunk bound malformed' USING ERRCODE='22023'; END IF;
    a:=vestrace_lock_embedding_index_attempt(target_workspace,target_attempt,target_owner);
    IF a.state<>'building' OR a.claim_deadline<=NOW() THEN RAISE EXCEPTION 'embedding-generation-changed' USING ERRCODE='23514'; END IF;
    SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=a.generation_id;
    IF a.cause='lazy_load' THEN
        IF vestrace_validate_local_embedding_generation(target_workspace,g.id,g.generation_epoch,g.captured_guard_version+1,g.corpus_revision,g.built_through_projection_ordinal,g.member_count) IS NULL THEN
            RAISE EXCEPTION 'embedding-generation-changed' USING ERRCODE='23514'; END IF;
    ELSIF NOT EXISTS(SELECT 1 FROM embedding_index_generation_guards guard JOIN embedding_space_corpus_states c USING(workspace_id,space_registration_id)
      WHERE guard.workspace_id=target_workspace AND guard.space_registration_id=a.space_registration_id AND g.state='building'
        AND guard.guard_version=g.captured_guard_version AND guard.generation_epoch+1=g.generation_epoch AND c.corpus_revision=g.corpus_revision AND c.live_member_count=g.member_count) THEN
        RAISE EXCEPTION 'embedding-generation-changed' USING ERRCODE='23514';
    END IF;
    -- A short hydration transaction locks corpus/generation before material IDs.
    PERFORM material.id FROM content_materials material WHERE material.workspace_id=target_workspace AND material.id IN (
        SELECT p.material_id FROM embedding_corpus_generation_members member JOIN embedding_projection_entries p
          ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
          WHERE member.workspace_id=target_workspace AND member.corpus_generation_id=g.id AND member.member_ordinal>coalesce(after_ordinal,0)
          ORDER BY member.member_ordinal LIMIT chunk_limit
    ) ORDER BY material.id FOR SHARE;
    IF EXISTS(SELECT 1 FROM embedding_corpus_generation_members member JOIN embedding_projection_entries p ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
      JOIN content_materials material ON material.workspace_id=p.workspace_id AND material.id=p.material_id
      WHERE member.workspace_id=target_workspace AND member.corpus_generation_id=g.id
        AND (p.state<>'live' OR material.state<>'live' OR p.space_registration_id<>a.space_registration_id OR p.projection_ordinal<>member.member_ordinal)) THEN
        RAISE EXCEPTION 'embedding-generation-changed' USING ERRCODE='23514'; END IF;
    IF EXISTS(SELECT 1
      FROM (SELECT * FROM embedding_corpus_generation_members WHERE workspace_id=target_workspace AND corpus_generation_id=g.id
        AND member_ordinal>coalesce(after_ordinal,0) ORDER BY member_ordinal LIMIT chunk_limit) member
      LEFT JOIN embedding_projection_entries p ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
      LEFT JOIN material_key_creation_intents i ON i.workspace_id=p.workspace_id AND i.id=p.intent_id
      LEFT JOIN content_material_bytes bytes ON bytes.workspace_id=p.workspace_id AND bytes.material_id=p.material_id
      LEFT JOIN embedding_result_key_binding_receipts receipt ON receipt.workspace_id=p.workspace_id AND receipt.preparation_id=p.preparation_id
        AND receipt.output_ordinal=p.output_ordinal AND receipt.projection_id=p.id AND receipt.material_id=p.material_id
        AND receipt.material_key_id=p.material_key_id AND receipt.intent_id=p.intent_id AND receipt.intent_nonce=i.nonce
      WHERE p.id IS NULL OR i.id IS NULL OR bytes.ciphertext IS NULL OR receipt.binding_receipt IS NULL
        OR octet_length(bytes.ciphertext)>1048576) THEN
        RAISE EXCEPTION 'embedding-index-material-unavailable' USING ERRCODE='23514';
    END IF;
    SELECT coalesce(jsonb_agg(jsonb_build_object('projection_id',p.id,'projection_ordinal',p.projection_ordinal,
        'job_id',p.job_id,'output_ordinal',p.output_ordinal,'intent_id',p.intent_id,'material_id',p.material_id,'material_key_id',p.material_key_id,
        'intent_nonce',i.nonce,'preparation_id',p.preparation_id,'binding_receipt',receipt.binding_receipt,'ciphertext',encode(bytes.ciphertext,'hex')) ORDER BY p.projection_ordinal),'[]'::jsonb),max(p.projection_ordinal)
      INTO rows,last_ordinal
      FROM (SELECT * FROM embedding_corpus_generation_members WHERE workspace_id=target_workspace AND corpus_generation_id=g.id AND member_ordinal>coalesce(after_ordinal,0) ORDER BY member_ordinal LIMIT chunk_limit) member
      JOIN embedding_projection_entries p ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
      JOIN material_key_creation_intents i ON i.workspace_id=p.workspace_id AND i.id=p.intent_id
      JOIN content_material_bytes bytes ON bytes.workspace_id=p.workspace_id AND bytes.material_id=p.material_id
      JOIN embedding_result_key_binding_receipts receipt ON receipt.workspace_id=p.workspace_id AND receipt.preparation_id=p.preparation_id
        AND receipt.output_ordinal=p.output_ordinal AND receipt.projection_id=p.id AND receipt.material_id=p.material_id AND receipt.material_key_id=p.material_key_id
        AND receipt.intent_id=p.intent_id AND receipt.intent_nonce=i.nonce;
    RETURN jsonb_build_object('plan',vestrace_embedding_index_attempt_plan(target_workspace,target_attempt),'projections',rows,
      'complete',NOT EXISTS(SELECT 1 FROM embedding_corpus_generation_members WHERE workspace_id=target_workspace AND corpus_generation_id=g.id AND member_ordinal>coalesce(last_ordinal,after_ordinal,0)));
END $$;

CREATE FUNCTION vestrace_publish_embedding_index_build(target_workspace UUID,target_attempt UUID,target_owner TEXT)
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE a embedding_index_build_attempts%ROWTYPE;g embedding_corpus_generations%ROWTYPE;won BOOLEAN:=false;
BEGIN
    a:=vestrace_lock_embedding_index_attempt(target_workspace,target_attempt,target_owner);
    IF a.state='published' THEN RETURN jsonb_build_object('plan',vestrace_embedding_index_attempt_plan(target_workspace,target_attempt),'outcome','published'); END IF;
    SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=a.generation_id;
    IF a.state='building' AND a.cause<>'lazy_load' AND a.claim_deadline>NOW() AND g.state='building'
      AND EXISTS(SELECT 1 FROM embedding_index_generation_guards guard JOIN embedding_space_corpus_states c USING(workspace_id,space_registration_id)
        WHERE guard.workspace_id=target_workspace AND guard.space_registration_id=a.space_registration_id AND guard.guard_version=g.captured_guard_version
          AND guard.generation_epoch+1=g.generation_epoch AND c.corpus_revision=g.corpus_revision AND c.live_member_count=g.member_count) THEN
        PERFORM vestrace_publish_embedding_generation(target_workspace,a.space_registration_id,g.id,g.captured_guard_version);
        UPDATE embedding_index_build_attempts SET state='published',terminal_at=NOW() WHERE id=a.id;
        won:=true;
    ELSIF a.state IN ('claimed','building') THEN
        UPDATE embedding_index_build_attempts SET state='discarded',terminal_at=NOW(),safe_reason=CASE WHEN a.claim_deadline<=NOW() THEN 'claim_expired' ELSE 'generation_changed' END WHERE id=a.id;
    END IF;
    RETURN jsonb_build_object('plan',vestrace_embedding_index_attempt_plan(target_workspace,target_attempt),'outcome',CASE WHEN won THEN 'published' ELSE 'discarded' END);
END $$;
CREATE FUNCTION vestrace_finish_embedding_index_attempt(target_workspace UUID,target_attempt UUID,target_owner TEXT,target_outcome TEXT,target_reason TEXT)
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE a embedding_index_build_attempts%ROWTYPE;g embedding_corpus_generations%ROWTYPE;reason TEXT;
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN RAISE EXCEPTION 'index finish workspace mismatch' USING ERRCODE='23514'; END IF;
    SELECT * INTO a FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND id=target_attempt;
    IF NOT FOUND THEN RAISE EXCEPTION 'index attempt absent' USING ERRCODE='23514'; END IF;
    a:=vestrace_lock_embedding_index_attempt(target_workspace,target_attempt,target_owner);
    reason:=CASE WHEN target_outcome='discarded' THEN 'generation_changed' ELSE target_reason END;
    IF a.state IN ('published','loaded','discarded','failed') THEN
        IF a.state=target_outcome AND (a.state IN ('published','loaded','discarded') OR a.safe_reason IS NOT DISTINCT FROM reason) THEN RETURN; END IF;
        RAISE EXCEPTION 'terminal index attempt outcome conflict' USING ERRCODE='23514';
    END IF;
    IF target_outcome NOT IN ('loaded','discarded','failed') THEN RAISE EXCEPTION 'index finish outcome not authorized' USING ERRCODE='23514'; END IF;
    SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=a.generation_id;
    IF target_outcome='loaded' AND (a.cause<>'lazy_load' OR a.claim_deadline<=NOW()
      OR vestrace_validate_local_embedding_generation(target_workspace,g.id,g.generation_epoch,g.captured_guard_version+1,g.corpus_revision,g.built_through_projection_ordinal,g.member_count) IS NULL) THEN
        RAISE EXCEPTION 'embedding-generation-changed' USING ERRCODE='23514'; END IF;
    UPDATE embedding_index_build_attempts SET state=target_outcome,safe_reason=reason,terminal_at=NOW() WHERE id=a.id;
    IF target_outcome IN ('failed','discarded') AND g.state='building' AND NOT EXISTS(SELECT 1 FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND generation_id=g.id AND state='building' AND claim_deadline>NOW()) THEN
        UPDATE embedding_corpus_generations SET state='stale' WHERE workspace_id=target_workspace AND id=g.id;
    END IF;
END $$;
CREATE FUNCTION vestrace_observe_embedding_index_attempt(target_workspace UUID,target_attempt UUID,target_owner TEXT,target_reason TEXT)
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
      OR NOT EXISTS(SELECT 1 FROM embedding_index_build_attempts WHERE workspace_id=target_workspace AND id=target_attempt AND state='published' AND claim_owner=target_owner) THEN
        RAISE EXCEPTION 'index observation requires exact committed publication attempt' USING ERRCODE='23514'; END IF;
    INSERT INTO embedding_index_build_observations(id,workspace_id,attempt_id,safe_reason) VALUES(gen_random_uuid(),target_workspace,target_attempt,target_reason)
      ON CONFLICT(workspace_id,attempt_id,safe_reason) DO NOTHING;
END $$;

DO $upgrade$ BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_index_upgrade()') IS NOT NULL THEN
        PERFORM vestrace_finish_embedding_index_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN RAISE EXCEPTION 'embedding index finish must be provisioned' USING ERRCODE='42501'; END IF;
CREATE CONSTRAINT TRIGGER embedding_index_rebuild_event_cause_consistent AFTER INSERT OR UPDATE ON embedding_index_rebuild_events
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_index_event_cause();
CREATE CONSTRAINT TRIGGER embedding_index_attempt_consistent AFTER INSERT OR UPDATE OR DELETE ON embedding_index_build_attempts
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_index_attempt();
ALTER TABLE public.embedding_index_build_attempts OWNER TO vestrace_guarded_owner;
ALTER TABLE public.embedding_index_build_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.embedding_index_build_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_index_build_attempts_workspace_policy ON public.embedding_index_build_attempts
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
REVOKE ALL ON TABLE public.embedding_index_build_attempts FROM PUBLIC,vestrace;
GRANT SELECT ON TABLE public.embedding_index_build_attempts TO vestrace;
CREATE TRIGGER embedding_index_build_attempts_guarded BEFORE INSERT OR UPDATE OR DELETE ON public.embedding_index_build_attempts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
ALTER TABLE public.embedding_index_build_observations OWNER TO vestrace_guarded_owner;
ALTER TABLE public.embedding_index_build_observations ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.embedding_index_build_observations FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_index_build_observations_workspace_policy ON public.embedding_index_build_observations
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
REVOKE ALL ON TABLE public.embedding_index_build_observations FROM PUBLIC,vestrace;
GRANT SELECT ON TABLE public.embedding_index_build_observations TO vestrace;
CREATE TRIGGER embedding_index_build_observations_guarded BEFORE INSERT OR UPDATE OR DELETE ON public.embedding_index_build_observations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER embedding_index_build_observations_immutable BEFORE UPDATE OR DELETE ON embedding_index_build_observations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
ALTER FUNCTION public.vestrace_validate_embedding_index_event_cause() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_embedding_index_event_cause() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_embedding_index_snapshot(uuid,uuid) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_embedding_index_snapshot(uuid,uuid) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_embedding_index_attempt_plan(uuid,uuid) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_embedding_index_attempt_plan(uuid,uuid) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_validate_embedding_index_attempt() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_embedding_index_attempt() FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_claim_embedding_index_build(uuid,text,integer) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_claim_embedding_index_build(uuid,text,integer) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_claim_embedding_index_build(uuid,text,integer) TO vestrace;
ALTER FUNCTION public.vestrace_validate_local_embedding_generation(uuid,uuid,bigint,bigint,bigint,bigint,bigint) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_validate_local_embedding_generation(uuid,uuid,bigint,bigint,bigint,bigint,bigint) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_validate_local_embedding_generation(uuid,uuid,bigint,bigint,bigint,bigint,bigint) TO vestrace;
ALTER FUNCTION public.vestrace_claim_embedding_index_load(uuid,uuid,bigint,bigint,bigint,bigint,bigint,text) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_claim_embedding_index_load(uuid,uuid,bigint,bigint,bigint,bigint,bigint,text) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_claim_embedding_index_load(uuid,uuid,bigint,bigint,bigint,bigint,bigint,text) TO vestrace;
ALTER FUNCTION public.vestrace_lock_embedding_index_attempt(uuid,uuid,text) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_lock_embedding_index_attempt(uuid,uuid,text) FROM PUBLIC,vestrace;
ALTER FUNCTION public.vestrace_load_embedding_index_chunk(uuid,uuid,text,bigint,integer) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_load_embedding_index_chunk(uuid,uuid,text,bigint,integer) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_load_embedding_index_chunk(uuid,uuid,text,bigint,integer) TO vestrace;
ALTER FUNCTION public.vestrace_publish_embedding_index_build(uuid,uuid,text) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_publish_embedding_index_build(uuid,uuid,text) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_publish_embedding_index_build(uuid,uuid,text) TO vestrace;
ALTER FUNCTION public.vestrace_finish_embedding_index_attempt(uuid,uuid,text,text,text) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_index_attempt(uuid,uuid,text,text,text) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_index_attempt(uuid,uuid,text,text,text) TO vestrace;
ALTER FUNCTION public.vestrace_observe_embedding_index_attempt(uuid,uuid,text,text) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_observe_embedding_index_attempt(uuid,uuid,text,text) FROM PUBLIC,vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_observe_embedding_index_attempt(uuid,uuid,text,text) TO vestrace;

    END IF;
END $upgrade$;
