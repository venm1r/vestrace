-- P04 Task 14E: embedding-owned delivery publication.
-- Host key binding precedes this transaction; no command dispatches a provider.
DO $$ BEGIN
    PERFORM vestrace_prepare_p04_result_finalization_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
        RAISE EXCEPTION 'result finalization upgrade must be provisioned' USING ERRCODE='42501';
    END IF;
END $$;

ALTER TABLE embedding_jobs ADD CONSTRAINT embedding_jobs_completion_identity_key
    UNIQUE(workspace_id,id,external_effect_id,model_binding_snapshot_id);
ALTER TABLE embedding_job_result_preparations ADD CONSTRAINT embedding_preparation_completion_identity_key
    UNIQUE(workspace_id,id,job_id,external_effect_id);
ALTER TABLE embedding_job_result_prepared_attachments ADD CONSTRAINT embedding_attachment_completion_identity_key
    UNIQUE(workspace_id,preparation_id,output_ordinal,intent_id,projection_id);

CREATE TABLE embedding_job_credential_completion_blockers (
    workspace_id UUID NOT NULL, job_id UUID NOT NULL, external_effect_id UUID NOT NULL,
    model_binding_snapshot_id UUID NOT NULL, credential_revision_id UUID NOT NULL,
    credential_intent_id UUID NOT NULL, blocker_id UUID NOT NULL,
    PRIMARY KEY(workspace_id,job_id), UNIQUE(blocker_id), UNIQUE(workspace_id,blocker_id),
    UNIQUE(workspace_id,job_id,blocker_id),
    FOREIGN KEY(workspace_id,job_id,external_effect_id,model_binding_snapshot_id)
      REFERENCES embedding_jobs(workspace_id,id,external_effect_id,model_binding_snapshot_id),
    FOREIGN KEY(workspace_id,credential_revision_id) REFERENCES credential_revisions(workspace_id,id),
    FOREIGN KEY(workspace_id,credential_intent_id) REFERENCES credential_key_creation_intents(workspace_id,id),
    FOREIGN KEY(blocker_id,workspace_id) REFERENCES material_erasure_blockers(id,workspace_id)
);
CREATE TABLE embedding_result_credential_blocker_adoptions (
    workspace_id UUID NOT NULL, preparation_id UUID NOT NULL,
    historical_blocker_id UUID NOT NULL, owned_blocker_id UUID NOT NULL,
    PRIMARY KEY(workspace_id,preparation_id),
    FOREIGN KEY(preparation_id,workspace_id) REFERENCES embedding_job_result_preparations(id,workspace_id),
    FOREIGN KEY(historical_blocker_id,workspace_id) REFERENCES material_erasure_blockers(id,workspace_id),
    FOREIGN KEY(workspace_id,owned_blocker_id) REFERENCES embedding_job_credential_completion_blockers(workspace_id,blocker_id),
    CHECK(historical_blocker_id<>owned_blocker_id)
);
CREATE TABLE embedding_result_key_binding_receipts (
    workspace_id UUID NOT NULL, preparation_id UUID NOT NULL, job_id UUID NOT NULL,
    external_effect_id UUID NOT NULL, output_ordinal BIGINT NOT NULL CHECK(output_ordinal>=0),
    projection_id UUID NOT NULL, intent_id UUID NOT NULL, material_id UUID NOT NULL,
    material_key_id UUID NOT NULL, intent_nonce UUID NOT NULL, binding_receipt UUID NOT NULL,
    PRIMARY KEY(workspace_id,preparation_id,output_ordinal),
    UNIQUE(workspace_id,intent_id), UNIQUE(binding_receipt),
    FOREIGN KEY(workspace_id,preparation_id,job_id,external_effect_id)
      REFERENCES embedding_job_result_preparations(workspace_id,id,job_id,external_effect_id),
    FOREIGN KEY(workspace_id,preparation_id,output_ordinal,intent_id,projection_id)
      REFERENCES embedding_job_result_prepared_attachments(workspace_id,preparation_id,output_ordinal,intent_id,projection_id),
    FOREIGN KEY(material_id,workspace_id,intent_id) REFERENCES content_materials(id,workspace_id,intent_id)
);
ALTER TABLE embedding_space_corpus_states ADD COLUMN live_member_count BIGINT NOT NULL DEFAULT 0
    CHECK(live_member_count>=0);
ALTER TABLE embedding_projection_entries
    DROP CONSTRAINT embedding_projection_entries_state_check,
    DROP CONSTRAINT embedding_projection_entries_retention_eligibility_state_check,
    ADD COLUMN output_commitment BYTEA,
    ADD CONSTRAINT embedding_projection_publication_phase CHECK(
      (state='result_finalizing' AND retention_eligibility_state='blocked_result_finalizing' AND output_commitment IS NULL)
      OR (state='live' AND retention_eligibility_state='blocked_pending_erasure_propagation' AND octet_length(output_commitment)=32 AND output_commitment IS NOT NULL));
-- History survives removal of its generic attachment. The deferred validator
-- below checks absence as well as mismatches, including attachment DELETE.
DO $$ DECLARE incompatible_name TEXT; match_count INTEGER; BEGIN
 SELECT min(conname),count(*) INTO incompatible_name,match_count FROM pg_constraint
   WHERE conrelid='embedding_job_result_prepared_attachments'::regclass
     AND confrelid='prepared_material_attachments'::regclass AND contype='f';
 IF match_count<>1 THEN RAISE EXCEPTION 'exact historical attachment FK absent or ambiguous' USING ERRCODE='23514'; END IF;
 EXECUTE format('ALTER TABLE embedding_job_result_prepared_attachments DROP CONSTRAINT %I',incompatible_name);
END $$;

CREATE TABLE embedding_job_result_publications (
    id UUID PRIMARY KEY, workspace_id UUID NOT NULL, preparation_id UUID NOT NULL,
    job_id UUID NOT NULL, external_effect_id UUID NOT NULL, space_registration_id UUID NOT NULL,
    rebuild_event_id UUID NOT NULL, output_count BIGINT NOT NULL CHECK(output_count>0),
    terminal_job_version BIGINT NOT NULL CHECK(terminal_job_version>0),
    resulting_corpus_revision BIGINT NOT NULL CHECK(resulting_corpus_revision>0),
    live_member_count BIGINT NOT NULL CHECK(live_member_count>=output_count),
    generation_epoch BIGINT NOT NULL CHECK(generation_epoch>0),
    UNIQUE(workspace_id,preparation_id), UNIQUE(workspace_id,job_id), UNIQUE(workspace_id,id),
    UNIQUE(workspace_id,id,rebuild_event_id,space_registration_id),
    FOREIGN KEY(workspace_id,preparation_id,job_id,external_effect_id)
      REFERENCES embedding_job_result_preparations(workspace_id,id,job_id,external_effect_id),
    FOREIGN KEY(workspace_id,space_registration_id) REFERENCES embedding_space_registrations(workspace_id,id)
);
CREATE TABLE embedding_index_rebuild_events (
    id UUID PRIMARY KEY, workspace_id UUID NOT NULL, publication_id UUID NOT NULL,
    space_registration_id UUID NOT NULL, before_corpus_revision BIGINT NOT NULL CHECK(before_corpus_revision>=0),
    after_corpus_revision BIGINT NOT NULL, before_generation_epoch BIGINT NOT NULL CHECK(before_generation_epoch>=0),
    after_generation_epoch BIGINT NOT NULL, before_live_member_count BIGINT NOT NULL CHECK(before_live_member_count>=0),
    after_live_member_count BIGINT NOT NULL, built_through_projection_ordinal BIGINT NOT NULL CHECK(built_through_projection_ordinal>0),
    invalidated_generation_ids UUID[] NOT NULL DEFAULT '{}',
    UNIQUE(workspace_id,id,publication_id,space_registration_id),
    UNIQUE(workspace_id,space_registration_id,after_corpus_revision),
    CHECK(after_corpus_revision=before_corpus_revision+1),
    CHECK(after_generation_epoch=before_generation_epoch+1),
    CHECK(after_live_member_count>before_live_member_count),
    FOREIGN KEY(workspace_id,publication_id,id,space_registration_id)
      REFERENCES embedding_job_result_publications(workspace_id,id,rebuild_event_id,space_registration_id)
      DEFERRABLE INITIALLY DEFERRED
);
ALTER TABLE embedding_job_result_publications ADD CONSTRAINT embedding_publication_event_exact_fkey
    FOREIGN KEY(workspace_id,rebuild_event_id,id,space_registration_id)
      REFERENCES embedding_index_rebuild_events(workspace_id,id,publication_id,space_registration_id)
      DEFERRABLE INITIALLY DEFERRED;

DO $$ DECLARE target REGCLASS; BEGIN
    FOREACH target IN ARRAY ARRAY[
      'embedding_job_credential_completion_blockers'::regclass,
      'embedding_result_credential_blocker_adoptions'::regclass,
      'embedding_result_key_binding_receipts'::regclass,
      'embedding_job_result_publications'::regclass,
      'embedding_index_rebuild_events'::regclass
    ] LOOP
      EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY',target);
      EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY',target);
      EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC',target);
      EXECUTE format('CREATE POLICY %I ON %s USING (workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',true),'''')::UUID) WITH CHECK (workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',true),'''')::UUID)',target::text||'_workspace_policy',target);
      EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %s FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation()',target::text||'_immutable',target);
    END LOOP;
END $$;


CREATE OR REPLACE FUNCTION vestrace_validate_embedding_credential_completion_owner()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM embedding_jobs j JOIN model_binding_snapshots s ON s.workspace_id=j.workspace_id AND s.id=j.model_binding_snapshot_id
   JOIN credential_key_creation_intents c ON c.workspace_id=s.workspace_id AND c.credential_revision_id=s.credential_revision_id
   JOIN material_erasure_blockers b ON b.workspace_id=c.workspace_id AND b.credential_intent_id=c.id
   WHERE j.workspace_id=NEW.workspace_id AND j.id=NEW.job_id AND j.external_effect_id=NEW.external_effect_id
     AND s.id=NEW.model_binding_snapshot_id AND s.branch='credential' AND s.credential_revision_id=NEW.credential_revision_id
     AND c.id=NEW.credential_intent_id AND c.connection_id=s.connection_id AND c.credential_slot_id=s.credential_slot_id
     AND c.state='active' AND b.id=NEW.blocker_id AND b.target_kind='credential' AND b.blocker_kind='effect'
     AND b.state='nonterminal' AND b.usable_until IS NULL) THEN
   RAISE EXCEPTION 'credential completion owner tuple incomplete' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER embedding_credential_completion_owner_exact BEFORE INSERT ON embedding_job_credential_completion_blockers
 FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_credential_completion_owner();

CREATE OR REPLACE FUNCTION vestrace_ensure_embedding_credential_completion_blocker(
    target_workspace UUID,target_job UUID,target_effect UUID
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE job embedding_jobs%ROWTYPE; snapshot model_binding_snapshots%ROWTYPE;
    credential credential_key_creation_intents%ROWTYPE;
    owned embedding_job_credential_completion_blockers%ROWTYPE; blocker UUID;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL
      OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
      RAISE EXCEPTION 'credential completion identity malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO job FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job AND external_effect_id=target_effect AND kind='delivery';
    IF NOT FOUND THEN RAISE EXCEPTION 'credential completion job absent' USING ERRCODE='23514'; END IF;
    SELECT * INTO snapshot FROM model_binding_snapshots WHERE workspace_id=target_workspace AND id=job.model_binding_snapshot_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential completion snapshot absent' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM connection_execution_guards WHERE workspace_id=target_workspace AND connection_id=snapshot.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential completion connection guard absent' USING ERRCODE='23514'; END IF;
    IF snapshot.branch='no_auth' THEN
      IF EXISTS(SELECT 1 FROM embedding_job_credential_completion_blockers WHERE workspace_id=target_workspace AND job_id=target_job)
        OR snapshot.credential_revision_id IS NOT NULL OR snapshot.credential_activation_guard_id IS NOT NULL OR snapshot.credential_slot_id IS NOT NULL THEN
        RAISE EXCEPTION 'no-auth completion has credential identity' USING ERRCODE='23514';
      END IF;
      RETURN NULL;
    END IF;
    IF snapshot.branch<>'credential' THEN RAISE EXCEPTION 'credential completion branch invalid' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM credential_activation_guards WHERE workspace_id=target_workspace AND id=snapshot.credential_activation_guard_id
      AND connection_id=snapshot.connection_id AND credential_slot_id=snapshot.credential_slot_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential completion guard absent' USING ERRCODE='23514'; END IF;
    SELECT * INTO credential FROM credential_key_creation_intents WHERE workspace_id=target_workspace
      AND connection_id=snapshot.connection_id AND credential_slot_id=snapshot.credential_slot_id
      AND credential_revision_id=snapshot.credential_revision_id FOR UPDATE;
    IF NOT FOUND OR credential.state<>'active' THEN RAISE EXCEPTION 'pinned credential is not active' USING ERRCODE='55000'; END IF;
    IF NOT EXISTS(SELECT 1 FROM credential_slots WHERE workspace_id=target_workspace AND id=snapshot.credential_slot_id
      AND connection_id=snapshot.connection_id AND current_revision_id=snapshot.credential_revision_id AND tombstoned_at IS NULL)
      OR EXISTS(SELECT 1 FROM material_erasure_preparations WHERE workspace_id=target_workspace AND credential_intent_id=credential.id) THEN
      RAISE EXCEPTION 'pinned credential is retired, revoked or erasure-prepared' USING ERRCODE='55000';
    END IF;
    SELECT * INTO owned FROM embedding_job_credential_completion_blockers WHERE workspace_id=target_workspace AND job_id=target_job;
    IF FOUND THEN
      IF owned.external_effect_id<>target_effect OR owned.model_binding_snapshot_id<>snapshot.id
        OR owned.credential_revision_id<>credential.credential_revision_id OR owned.credential_intent_id<>credential.id
        OR NOT EXISTS(SELECT 1 FROM material_erasure_blockers WHERE workspace_id=target_workspace AND id=owned.blocker_id
          AND target_kind='credential' AND credential_intent_id=credential.id AND blocker_kind='effect' AND state='nonterminal' AND usable_until IS NULL) THEN
        RAISE EXCEPTION 'credential completion owner conflicts' USING ERRCODE='23514';
      END IF;
      PERFORM 1 FROM material_erasure_blockers WHERE id=owned.blocker_id FOR UPDATE;
      RETURN owned.blocker_id;
    END IF;
    blocker:=gen_random_uuid();
    INSERT INTO material_erasure_blockers(id,workspace_id,target_kind,credential_intent_id,blocker_kind,state,usable_until)
      VALUES(blocker,target_workspace,'credential',credential.id,'effect','nonterminal',NULL);
    INSERT INTO embedding_job_credential_completion_blockers VALUES(target_workspace,target_job,target_effect,snapshot.id,credential.credential_revision_id,credential.id,blocker);
    RETURN blocker;
END $$;

CREATE OR REPLACE FUNCTION vestrace_adopt_embedding_result_credential_blocker(
    target_workspace UUID,target_preparation UUID,target_job UUID,target_effect UUID
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE marker embedding_job_result_preparations%ROWTYPE; owned UUID;
BEGIN
    IF target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
      OR target_workspace IS NULL OR target_preparation IS NULL OR target_job IS NULL OR target_effect IS NULL THEN
      RAISE EXCEPTION 'credential adoption identity malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO marker FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND id=target_preparation AND job_id=target_job AND external_effect_id=target_effect;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential adoption marker absent' USING ERRCODE='23514'; END IF;
    -- Serialize before consulting mutable credential protection. Another actor
    -- may have adopted and published while this caller waited for the guard.
    PERFORM 1 FROM connection_execution_guards g JOIN model_binding_snapshots b
      ON b.workspace_id=g.workspace_id AND b.connection_id=g.connection_id
      WHERE b.workspace_id=target_workspace AND b.id=marker.model_binding_snapshot_id FOR UPDATE OF g;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential adoption connection guard absent' USING ERRCODE='23514'; END IF;
    IF vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect)='published' THEN
      IF marker.auth_branch='no_auth' THEN RETURN NULL; END IF;
      SELECT blocker_id INTO owned FROM embedding_job_credential_completion_blockers
        WHERE workspace_id=target_workspace AND job_id=target_job AND external_effect_id=target_effect;
      RETURN owned;
    END IF;
    -- Same permanent guards as preparation; historical protection is never
    -- released or assigned to the new owner.
    owned:=vestrace_ensure_embedding_credential_completion_blocker(target_workspace,target_job,target_effect);
    IF marker.auth_branch='no_auth' THEN RETURN NULL; END IF;
    IF NOT EXISTS(SELECT 1 FROM embedding_job_credential_completion_blockers WHERE workspace_id=target_workspace AND job_id=target_job
      AND blocker_id=owned AND credential_revision_id=marker.credential_revision_id AND credential_intent_id=marker.credential_intent_id) THEN
      RAISE EXCEPTION 'credential adoption snapshot conflicts' USING ERRCODE='23514';
    END IF;
    IF marker.credential_erasure_blocker_id=owned THEN RETURN owned; END IF;
    IF EXISTS(SELECT 1 FROM embedding_result_credential_blocker_adoptions WHERE workspace_id=target_workspace AND preparation_id=target_preparation
      AND historical_blocker_id=marker.credential_erasure_blocker_id AND owned_blocker_id=owned) THEN RETURN owned; END IF;
    PERFORM 1 FROM material_erasure_blockers WHERE workspace_id=target_workspace AND id=marker.credential_erasure_blocker_id
      AND target_kind='credential' AND credential_intent_id=marker.credential_intent_id AND state='nonterminal'
      AND (usable_until IS NULL OR usable_until>now()) FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'historical credential protection unavailable' USING ERRCODE='55000'; END IF;
    INSERT INTO embedding_result_credential_blocker_adoptions VALUES(target_workspace,target_preparation,marker.credential_erasure_blocker_id,owned)
      ON CONFLICT(workspace_id,preparation_id) DO NOTHING;
    IF NOT EXISTS(SELECT 1 FROM embedding_result_credential_blocker_adoptions WHERE workspace_id=target_workspace AND preparation_id=target_preparation
      AND historical_blocker_id=marker.credential_erasure_blocker_id AND owned_blocker_id=owned) THEN
      RAISE EXCEPTION 'credential adoption conflicts' USING ERRCODE='23514';
    END IF;
    RETURN owned;
END $$;

-- Called by the narrow commands and every deferred phase trigger. This is an
-- evidence check, not a mutable credential/admission gate on published replay.
CREATE OR REPLACE FUNCTION vestrace_assert_embedding_result_phase(
    target_workspace UUID,target_preparation UUID,target_job UUID,target_effect UUID
) RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE marker embedding_job_result_preparations%ROWTYPE;
    publication embedding_job_result_publications%ROWTYPE;
    job embedding_jobs%ROWTYPE; snapshot model_binding_snapshots%ROWTYPE;
    owned embedding_job_credential_completion_blockers%ROWTYPE;
    output RECORD; position BIGINT:=0; bound_count BIGINT:=0;
BEGIN
    IF target_workspace IS NULL OR target_preparation IS NULL OR target_job IS NULL OR target_effect IS NULL
      OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
      RAISE EXCEPTION 'result finalization identity malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO marker FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND id=target_preparation
      AND job_id=target_job AND external_effect_id=target_effect;
    IF NOT FOUND THEN RAISE EXCEPTION 'result finalization preparation absent' USING ERRCODE='23514'; END IF;
    SELECT * INTO job FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job AND external_effect_id=target_effect;
    IF NOT FOUND OR job.kind<>'delivery' OR job.model_binding_snapshot_id<>marker.model_binding_snapshot_id
      OR job.model_request_evidence_id<>marker.model_request_evidence_id OR job.space_registration_id<>marker.space_registration_id THEN
      RAISE EXCEPTION 'result finalization job identity conflicts' USING ERRCODE='23514';
    END IF;
    SELECT * INTO snapshot FROM model_binding_snapshots WHERE workspace_id=target_workspace AND id=marker.model_binding_snapshot_id;
    IF NOT FOUND OR snapshot.branch<>marker.auth_branch OR snapshot.credential_revision_id IS DISTINCT FROM marker.credential_revision_id THEN
      RAISE EXCEPTION 'result finalization snapshot conflicts' USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM external_effect_receipts WHERE workspace_id=target_workspace AND id=marker.receipt_id
      AND effect_id=target_effect AND outcome_status='acknowledged'
      AND payload @> jsonb_build_object('evidence_refs',jsonb_build_array(format('embedding_job_result_preparation:%s',marker.id))))
      OR NOT EXISTS(SELECT 1 FROM external_effect_intents WHERE workspace_id=target_workspace AND id=target_effect AND adapter=marker.adapter)
      OR NOT EXISTS(SELECT 1 FROM embedding_data_policy_decisions d JOIN embedding_delivery_acceptance_receipts a
        ON a.workspace_id=target_workspace AND a.job_id=target_job WHERE d.id=marker.data_policy_decision_id
        AND d.purpose='delivery' AND d.verdict='allowed' AND d.input_count=marker.output_count
        AND d.causal_reference_id=(a.request_tuple #>> '{outbox,0,id}')::UUID
        AND d.delivery_attempt=(a.request_tuple #>> '{outbox,0,attempts}')::INTEGER+1)
      OR EXISTS(SELECT 1 FROM embedding_job_pre_dispatch_retirement_authorities WHERE workspace_id=target_workspace AND job_id=target_job)
      OR EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests WHERE workspace_id=target_workspace AND job_id=target_job) THEN
      RAISE EXCEPTION 'result finalization receipt, policy or retirement conflicts' USING ERRCODE='23514';
    END IF;
    SELECT * INTO publication FROM embedding_job_result_publications WHERE workspace_id=target_workspace AND preparation_id=target_preparation;
    IF publication.id IS NULL THEN
      IF job.state<>'running' OR job.version<>marker.expected_job_version THEN
        RAISE EXCEPTION 'prepared result requires expected Running job' USING ERRCODE='23514';
      END IF;
    ELSE
      IF publication.job_id<>target_job OR publication.external_effect_id<>target_effect OR publication.space_registration_id<>marker.space_registration_id
        OR publication.output_count<>marker.output_count OR publication.terminal_job_version<>marker.expected_job_version+1
        OR job.state<>'succeeded' OR job.version<>publication.terminal_job_version
        OR NOT EXISTS(SELECT 1 FROM embedding_index_rebuild_events e WHERE e.workspace_id=target_workspace AND e.id=publication.rebuild_event_id
          AND e.publication_id=publication.id AND e.space_registration_id=publication.space_registration_id
          AND e.after_corpus_revision=publication.resulting_corpus_revision AND e.after_generation_epoch=publication.generation_epoch
          AND NOT EXISTS(SELECT 1 FROM unnest(e.invalidated_generation_ids) required(id) LEFT JOIN embedding_corpus_generations g ON g.workspace_id=e.workspace_id AND g.id=required.id AND g.space_registration_id=e.space_registration_id WHERE g.id IS NULL OR g.state<>'stale')
          AND e.after_live_member_count=publication.live_member_count AND e.after_live_member_count-e.before_live_member_count=publication.output_count
          AND e.built_through_projection_ordinal=(SELECT max(projection_ordinal) FROM embedding_projection_entries WHERE workspace_id=target_workspace AND preparation_id=target_preparation)
          AND (e.before_corpus_revision=0 AND e.before_live_member_count=0 OR EXISTS(SELECT 1 FROM embedding_index_rebuild_events previous
            WHERE previous.workspace_id=e.workspace_id AND previous.space_registration_id=e.space_registration_id
              AND previous.after_corpus_revision=e.before_corpus_revision AND previous.after_live_member_count=e.before_live_member_count
              AND previous.after_generation_epoch<=e.before_generation_epoch)))
        OR NOT EXISTS(SELECT 1 FROM embedding_space_corpus_states c JOIN embedding_index_generation_guards g USING(workspace_id,space_registration_id)
          WHERE c.workspace_id=target_workspace AND c.space_registration_id=marker.space_registration_id
            AND c.corpus_revision>=publication.resulting_corpus_revision AND c.live_member_count>=publication.live_member_count
            AND c.corpus_revision=(SELECT max(after_corpus_revision) FROM embedding_index_rebuild_events WHERE workspace_id=c.workspace_id AND space_registration_id=c.space_registration_id)
            AND c.live_member_count=(SELECT sum(output_count) FROM embedding_job_result_publications WHERE workspace_id=c.workspace_id AND space_registration_id=c.space_registration_id)
            AND g.generation_epoch>=publication.generation_epoch) THEN
        RAISE EXCEPTION 'publication requires complete immutable terminal event' USING ERRCODE='23514';
      END IF;
    END IF;
    SELECT * INTO owned FROM embedding_job_credential_completion_blockers WHERE workspace_id=target_workspace AND job_id=target_job;
    IF marker.auth_branch='no_auth' THEN
      IF owned.blocker_id IS NOT NULL OR marker.credential_intent_id IS NOT NULL OR marker.credential_erasure_blocker_id IS NOT NULL
        OR EXISTS(SELECT 1 FROM embedding_result_credential_blocker_adoptions WHERE workspace_id=target_workspace AND preparation_id=target_preparation) THEN
        RAISE EXCEPTION 'no-auth preparation contains credential ownership' USING ERRCODE='23514';
      END IF;
    ELSE
      IF owned.blocker_id IS NULL THEN
        IF publication.id IS NOT NULL OR NOT EXISTS(SELECT 1 FROM material_erasure_blockers b JOIN credential_key_creation_intents c
          ON c.workspace_id=b.workspace_id AND c.id=b.credential_intent_id WHERE b.workspace_id=target_workspace
          AND b.id=marker.credential_erasure_blocker_id AND b.target_kind='credential' AND b.credential_intent_id=marker.credential_intent_id
          AND c.credential_revision_id=marker.credential_revision_id AND b.state='nonterminal' AND (b.usable_until IS NULL OR b.usable_until>now())) THEN
          RAISE EXCEPTION 'legacy preparation protection absent' USING ERRCODE='23514';
        END IF;
      ELSIF owned.external_effect_id<>target_effect OR owned.model_binding_snapshot_id<>marker.model_binding_snapshot_id
        OR owned.credential_revision_id<>marker.credential_revision_id OR owned.credential_intent_id<>marker.credential_intent_id
        OR (owned.blocker_id<>marker.credential_erasure_blocker_id AND NOT EXISTS(SELECT 1 FROM embedding_result_credential_blocker_adoptions
          WHERE workspace_id=target_workspace AND preparation_id=target_preparation AND historical_blocker_id=marker.credential_erasure_blocker_id AND owned_blocker_id=owned.blocker_id))
        OR NOT EXISTS(SELECT 1 FROM material_erasure_blockers WHERE workspace_id=target_workspace AND id=owned.blocker_id
          AND target_kind='credential' AND credential_intent_id=owned.credential_intent_id AND blocker_kind='effect' AND usable_until IS NULL
          AND state=CASE WHEN publication.id IS NULL THEN 'nonterminal' ELSE 'terminal' END) THEN
        RAISE EXCEPTION 'credential completion ownership conflicts' USING ERRCODE='23514';
      END IF;
    END IF;
    FOR output IN SELECT m.output_ordinal,m.intent_id,i.material_id,i.material_key_id,i.nonce,i.state AS intent_state,i.bound_receipt,i.owner_kind,i.owner_id,
      p.id AS projection_id,p.state AS projection_state,p.output_commitment,p.retention_eligibility_state,
      h.prepared_attachment_id,b.binding_receipt,c.state AS material_state,
      (SELECT count(*) FROM prepared_material_attachments a WHERE a.id=h.prepared_attachment_id AND a.intent_id=i.id AND a.workspace_id=target_workspace) AS attachments
      FROM embedding_job_material_intents m JOIN material_key_creation_intents i ON i.workspace_id=m.workspace_id AND i.id=m.intent_id
      LEFT JOIN embedding_job_result_prepared_attachments h ON h.workspace_id=m.workspace_id AND h.preparation_id=target_preparation AND h.output_ordinal=m.output_ordinal AND h.intent_id=m.intent_id
      LEFT JOIN embedding_projection_entries p ON p.workspace_id=m.workspace_id AND p.id=h.projection_id AND p.preparation_id=target_preparation AND p.job_id=target_job
        AND p.output_ordinal=m.output_ordinal AND p.input_ordinal=m.output_ordinal AND p.response_index=m.output_ordinal
        AND p.intent_id=i.id AND p.material_id=i.material_id AND p.material_key_id=i.material_key_id
        AND p.space_registration_id=marker.space_registration_id AND p.model_binding_snapshot_id=marker.model_binding_snapshot_id
        AND p.data_policy_decision_id=marker.data_policy_decision_id AND p.response_model=marker.response_model
      LEFT JOIN content_materials c ON c.workspace_id=i.workspace_id AND c.id=i.material_id AND c.intent_id=i.id AND c.material_key_id=i.material_key_id
      LEFT JOIN embedding_result_key_binding_receipts b ON b.workspace_id=m.workspace_id AND b.preparation_id=target_preparation AND b.output_ordinal=m.output_ordinal
        AND b.job_id=target_job AND b.external_effect_id=target_effect AND b.projection_id=p.id AND b.intent_id=i.id
        AND b.material_id=i.material_id AND b.material_key_id=i.material_key_id AND b.intent_nonce=i.nonce
      WHERE m.workspace_id=target_workspace AND m.job_id=target_job ORDER BY m.output_ordinal
    LOOP
      IF output.output_ordinal<>position OR output.projection_id IS NULL OR output.material_state IS NULL
        OR output.owner_kind<>'embedding_job_output' OR output.owner_id<>target_job
        OR NOT EXISTS(SELECT 1 FROM content_material_bytes WHERE workspace_id=target_workspace AND intent_id=output.intent_id)
        OR EXISTS(SELECT 1 FROM content_material_ordinary_references WHERE workspace_id=target_workspace AND intent_id=output.intent_id)
        OR NOT EXISTS(SELECT 1 FROM embedding_projection_entries p JOIN embedding_data_policy_decisions d ON d.id=p.data_policy_decision_id
          JOIN embedding_space_registrations s ON s.workspace_id=p.workspace_id AND s.id=p.space_registration_id
          WHERE p.workspace_id=target_workspace AND p.id=output.projection_id AND p.sensitivity=d.classification
            AND p.classification_labels=d.classification_labels AND p.has_unclassified=(d.unclassified_count>0)
            AND p.dimensions=s.dimensions AND p.response_model=s.model) THEN
        RAISE EXCEPTION 'result output identity or bytes incomplete' USING ERRCODE='23514';
      END IF;
      IF publication.id IS NULL THEN
        IF output.attachments<>1 OR output.material_state<>'prepared' OR output.projection_state<>'result_finalizing'
          OR output.output_commitment IS NOT NULL OR output.retention_eligibility_state<>'blocked_result_finalizing'
          OR (output.intent_state='result_prepared' AND (output.binding_receipt IS NOT NULL OR output.bound_receipt IS NOT NULL))
          OR (output.intent_state='bound' AND (output.binding_receipt IS NULL OR output.bound_receipt IS DISTINCT FROM output.binding_receipt))
          OR output.intent_state NOT IN('result_prepared','bound') THEN
          RAISE EXCEPTION 'prepared output phase incomplete' USING ERRCODE='23514';
        END IF;
      ELSIF output.attachments<>0 OR output.material_state<>'live' OR output.intent_state<>'live' OR output.projection_state<>'live'
        OR output.binding_receipt IS NULL OR output.bound_receipt IS DISTINCT FROM output.binding_receipt
        OR output.output_commitment IS NULL OR octet_length(output.output_commitment)<>32
        OR output.retention_eligibility_state<>'blocked_pending_erasure_propagation' THEN
        RAISE EXCEPTION 'published output phase incomplete' USING ERRCODE='23514';
      END IF;
      IF output.binding_receipt IS NOT NULL THEN bound_count:=bound_count+1; END IF;
      position:=position+1;
    END LOOP;
    IF position<>marker.output_count OR position<>(SELECT count(*) FROM embedding_projection_entries WHERE workspace_id=target_workspace AND preparation_id=target_preparation)
      OR position<>(SELECT count(*) FROM embedding_job_result_prepared_attachments WHERE workspace_id=target_workspace AND preparation_id=target_preparation)
      OR bound_count<>(SELECT count(*) FROM embedding_result_key_binding_receipts WHERE workspace_id=target_workspace AND preparation_id=target_preparation)
      OR EXISTS(SELECT 1 FROM embedding_delivery_source_memberships s LEFT JOIN embedding_projection_entries p
        ON p.workspace_id=s.workspace_id AND p.job_id=s.job_id AND p.preparation_id=target_preparation AND p.output_ordinal=s.output_ordinal
        LEFT JOIN embedding_projection_source_dependencies d ON d.workspace_id=s.workspace_id AND d.projection_id=p.id
          AND d.job_id=s.job_id AND d.output_ordinal=s.output_ordinal AND d.source_ordinal=s.source_ordinal
          AND d.source_material_id=s.source_material_id AND d.source_intent_id=s.source_intent_id AND d.output_intent_id=s.intent_id AND d.erasure_blocker_id=s.blocker_id
        LEFT JOIN material_erasure_blockers b ON b.workspace_id=s.workspace_id AND b.id=s.blocker_id
        WHERE s.workspace_id=target_workspace AND s.job_id=target_job AND (d.projection_id IS NULL OR b.id IS NULL
          OR b.state<>CASE WHEN publication.id IS NULL THEN 'nonterminal' ELSE 'terminal' END)) THEN
      RAISE EXCEPTION 'result finalization set incomplete' USING ERRCODE='23514';
    END IF;
    IF (SELECT count(*) FROM embedding_projection_source_dependencies WHERE workspace_id=target_workspace AND job_id=target_job)
         <> (SELECT count(*) FROM embedding_delivery_source_memberships WHERE workspace_id=target_workspace AND job_id=target_job)
      OR NOT EXISTS(SELECT 1 FROM embedding_delivery_source_memberships WHERE workspace_id=target_workspace AND job_id=target_job)
      OR (publication.id IS NULL AND EXISTS(SELECT 1 FROM embedding_delivery_source_memberships d
        LEFT JOIN content_materials c ON c.workspace_id=d.workspace_id AND c.id=d.source_material_id AND c.intent_id=d.source_intent_id
        LEFT JOIN material_key_creation_intents i ON i.workspace_id=d.workspace_id AND i.id=d.source_intent_id
        WHERE d.workspace_id=target_workspace AND d.job_id=target_job AND (c.id IS NULL OR i.id IS NULL OR c.state<>'live' OR i.state<>'live'))) THEN
      RAISE EXCEPTION 'result source dependency set or Live state conflicts' USING ERRCODE='23514';
    END IF;
    IF publication.id IS NOT NULL THEN RETURN 'published'; END IF;
    IF bound_count=position THEN RETURN 'ready_to_publish'; END IF;
    RETURN 'needs_binding';
END $$;


CREATE OR REPLACE FUNCTION vestrace_embedding_publication_json(target_workspace UUID,target_preparation UUID)
RETURNS JSONB LANGUAGE sql SECURITY DEFINER SET search_path=public,pg_temp AS $$
 SELECT (to_jsonb(p)-'id'-'workspace_id'-'external_effect_id') || jsonb_build_object('publication_id',p.id,'effect_id',p.external_effect_id)
 FROM embedding_job_result_publications p WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation
$$;

CREATE OR REPLACE FUNCTION vestrace_lock_embedding_result_finalization(target_workspace UUID,target_preparation UUID,target_job UUID,target_effect UUID)
RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE marker embedding_job_result_preparations%ROWTYPE; phase TEXT; source RECORD;
BEGIN
 phase:=vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect);
 IF phase='published' THEN RETURN phase; END IF;
 SELECT * INTO marker FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND id=target_preparation;
 PERFORM 1 FROM connection_execution_guards g JOIN model_binding_snapshots b ON b.workspace_id=g.workspace_id AND b.connection_id=g.connection_id
   WHERE b.workspace_id=target_workspace AND b.id=marker.model_binding_snapshot_id FOR UPDATE OF g;
 IF NOT FOUND THEN RAISE EXCEPTION 'finalization connection guard absent' USING ERRCODE='23514'; END IF;
 phase:=vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect);
 IF phase='published' THEN RETURN phase; END IF;
 PERFORM vestrace_adopt_embedding_result_credential_blocker(target_workspace,target_preparation,target_job,target_effect);
 PERFORM 1 FROM embedding_space_registrations WHERE workspace_id=target_workspace AND id=marker.space_registration_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'finalization space absent' USING ERRCODE='23514'; END IF;
 PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'finalization corpus absent' USING ERRCODE='23514'; END IF;
 PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'finalization generation guard absent' USING ERRCODE='23514'; END IF;
 PERFORM 1 FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id AND state='ready' ORDER BY id FOR UPDATE;
 PERFORM 1 FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND id=target_preparation FOR UPDATE;
 PERFORM 1 FROM embedding_data_policy_decisions WHERE id=marker.data_policy_decision_id FOR UPDATE;
 FOR source IN SELECT s.source_material_id,s.source_intent_id,s.blocker_id,m.state AS material_state,i.state AS intent_state,b.state AS blocker_state
   FROM embedding_delivery_source_memberships s JOIN content_materials m ON m.workspace_id=s.workspace_id AND m.id=s.source_material_id AND m.intent_id=s.source_intent_id
   JOIN material_key_creation_intents i ON i.workspace_id=s.workspace_id AND i.id=s.source_intent_id
   JOIN material_erasure_blockers b ON b.workspace_id=s.workspace_id AND b.id=s.blocker_id
   WHERE s.workspace_id=target_workspace AND s.job_id=target_job
   ORDER BY s.output_ordinal,s.source_ordinal,s.source_material_id,s.source_intent_id,s.blocker_id FOR UPDATE OF s,m,i,b
 LOOP
   IF source.material_state<>'live' OR source.intent_state<>'live' OR source.blocker_state<>'nonterminal' THEN
     RAISE EXCEPTION 'finalization source no longer Live and protected' USING ERRCODE='23514';
   END IF;
 END LOOP;
 PERFORM 1 FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
 PERFORM 1 FROM external_effect_intents WHERE workspace_id=target_workspace AND id=target_effect FOR UPDATE;
 PERFORM 1 FROM embedding_result_key_binding_receipts WHERE workspace_id=target_workspace AND preparation_id=target_preparation ORDER BY output_ordinal FOR UPDATE;
 PERFORM 1 FROM material_key_creation_intents i JOIN embedding_job_material_intents m ON m.workspace_id=i.workspace_id AND m.intent_id=i.id
   WHERE m.workspace_id=target_workspace AND m.job_id=target_job ORDER BY m.output_ordinal FOR UPDATE OF i;
 PERFORM 1 FROM content_materials c JOIN embedding_projection_entries p ON p.workspace_id=c.workspace_id AND p.material_id=c.id
   WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation ORDER BY p.output_ordinal FOR UPDATE OF c;
 PERFORM 1 FROM prepared_material_attachments a JOIN embedding_job_result_prepared_attachments h ON h.prepared_attachment_id=a.id
   WHERE h.workspace_id=target_workspace AND h.preparation_id=target_preparation ORDER BY h.output_ordinal FOR UPDATE OF a;
 PERFORM 1 FROM embedding_projection_entries WHERE workspace_id=target_workspace AND preparation_id=target_preparation ORDER BY output_ordinal FOR UPDATE;
 RETURN vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect);
END $$;

CREATE OR REPLACE FUNCTION vestrace_embedding_output_binding_identities(target_workspace UUID,target_preparation UUID)
RETURNS JSONB LANGUAGE sql SECURITY DEFINER SET search_path=public,pg_temp AS $$
 SELECT jsonb_agg(jsonb_build_object('workspace_id',p.workspace_id,'job_id',p.job_id,'output_ordinal',p.output_ordinal,
   'intent_id',p.intent_id,'material_id',p.material_id,'material_key_id',p.material_key_id,'intent_nonce',i.nonce,
   'projection_id',p.id,'binding_receipt',b.binding_receipt) ORDER BY p.output_ordinal)
 FROM embedding_projection_entries p JOIN material_key_creation_intents i ON i.workspace_id=p.workspace_id AND i.id=p.intent_id
 LEFT JOIN embedding_result_key_binding_receipts b ON b.workspace_id=p.workspace_id AND b.preparation_id=p.preparation_id AND b.output_ordinal=p.output_ordinal
 WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation
$$;

CREATE OR REPLACE FUNCTION vestrace_load_embedding_result_finalization(target_workspace UUID,target_preparation UUID,target_job UUID,target_effect UUID)
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE phase TEXT; outputs JSONB; adoption BOOLEAN;
BEGIN
 phase:=vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect);
 IF phase='published' THEN RETURN jsonb_build_object('phase',phase,'publication',vestrace_embedding_publication_json(target_workspace,target_preparation),'all_output_bindings',vestrace_embedding_output_binding_identities(target_workspace,target_preparation)); END IF;
 SELECT marker.auth_branch='credential' AND NOT EXISTS(SELECT 1 FROM embedding_job_credential_completion_blockers owned
   WHERE owned.workspace_id=marker.workspace_id AND owned.job_id=marker.job_id AND
     (owned.blocker_id=marker.credential_erasure_blocker_id OR EXISTS(SELECT 1 FROM embedding_result_credential_blocker_adoptions a
       WHERE a.workspace_id=marker.workspace_id AND a.preparation_id=marker.id AND a.owned_blocker_id=owned.blocker_id)))
 INTO adoption FROM embedding_job_result_preparations marker WHERE marker.workspace_id=target_workspace AND marker.id=target_preparation;
 SELECT jsonb_agg(jsonb_build_object('workspace_id',p.workspace_id,'job_id',p.job_id,'output_ordinal',p.output_ordinal,
   'intent_id',p.intent_id,'material_id',p.material_id,'material_key_id',p.material_key_id,'intent_nonce',i.nonce,
   'projection_id',p.id,'binding_receipt',b.binding_receipt,'ciphertext',encode(c.ciphertext,'hex')) ORDER BY p.output_ordinal)
 INTO outputs FROM embedding_projection_entries p JOIN material_key_creation_intents i ON i.workspace_id=p.workspace_id AND i.id=p.intent_id
 JOIN content_material_bytes c ON c.workspace_id=p.workspace_id AND c.intent_id=p.intent_id
 LEFT JOIN embedding_result_key_binding_receipts b ON b.workspace_id=p.workspace_id AND b.preparation_id=p.preparation_id AND b.output_ordinal=p.output_ordinal
 WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation AND (phase='ready_to_publish' OR b.binding_receipt IS NULL);
 RETURN jsonb_build_object('phase',phase,'outputs',outputs,'credential_adoption_required',adoption,'all_output_bindings',vestrace_embedding_output_binding_identities(target_workspace,target_preparation));
END $$;

CREATE OR REPLACE FUNCTION vestrace_record_embedding_result_key_binding(target_workspace UUID,target_preparation UUID,target_job UUID,target_effect UUID,target_ordinal BIGINT,target_receipt UUID)
RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE phase TEXT; existing UUID; output RECORD;
BEGIN
 IF target_ordinal IS NULL OR target_ordinal<0 OR target_receipt IS NULL THEN RAISE EXCEPTION 'binding arguments malformed' USING ERRCODE='22023'; END IF;
 phase:=vestrace_lock_embedding_result_finalization(target_workspace,target_preparation,target_job,target_effect);
 SELECT binding_receipt INTO existing FROM embedding_result_key_binding_receipts WHERE workspace_id=target_workspace AND preparation_id=target_preparation AND output_ordinal=target_ordinal;
 IF FOUND THEN
   IF existing<>target_receipt THEN RAISE EXCEPTION 'binding receipt conflicts' USING ERRCODE='23514'; END IF;
   RETURN existing;
 END IF;
 IF phase='published' THEN RAISE EXCEPTION 'published binding absent' USING ERRCODE='23514'; END IF;
 SELECT p.*,i.nonce INTO output FROM embedding_projection_entries p JOIN material_key_creation_intents i ON i.workspace_id=p.workspace_id AND i.id=p.intent_id
   WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation AND p.job_id=target_job AND p.output_ordinal=target_ordinal AND i.state='result_prepared';
 IF NOT FOUND THEN RAISE EXCEPTION 'binding output absent or not prepared' USING ERRCODE='23514'; END IF;
 INSERT INTO embedding_result_key_binding_receipts VALUES(target_workspace,target_preparation,target_job,target_effect,target_ordinal,output.id,output.intent_id,output.material_id,output.material_key_id,output.nonce,target_receipt);
 PERFORM vestrace_bind_material_key_creation_intent(output.intent_id,target_receipt);
 RETURN target_receipt;
END $$;

CREATE OR REPLACE FUNCTION vestrace_publish_embedding_job_result(target_workspace UUID,target_preparation UUID,target_job UUID,target_effect UUID,
 target_publication UUID,target_event UUID,target_projections UUID[],target_ordinals BIGINT[],target_commitments BYTEA[])
RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE phase TEXT; marker embedding_job_result_preparations%ROWTYPE; output RECORD; position BIGINT:=0;
 old_revision BIGINT; old_count BIGINT; old_epoch BIGINT; projection_max BIGINT;
BEGIN
 phase:=vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect);
 -- Exact immutable identity precedes proposed ids, current admission and vault.
 IF phase='published' THEN
   IF target_projections IS NULL OR target_ordinals IS NULL OR target_commitments IS NULL
     OR cardinality(target_projections) IS DISTINCT FROM (SELECT output_count FROM embedding_job_result_publications WHERE workspace_id=target_workspace AND preparation_id=target_preparation)
     OR cardinality(target_ordinals) IS DISTINCT FROM cardinality(target_projections)
     OR cardinality(target_commitments) IS DISTINCT FROM cardinality(target_projections)
     OR array_lower(target_projections,1)<>1 OR array_lower(target_ordinals,1)<>1 OR array_lower(target_commitments,1)<>1
     OR EXISTS(SELECT 1 FROM embedding_projection_entries p WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation
       AND (target_projections[p.output_ordinal+1] IS DISTINCT FROM p.id OR target_ordinals[p.output_ordinal+1] IS DISTINCT FROM p.output_ordinal
         OR target_commitments[p.output_ordinal+1] IS DISTINCT FROM p.output_commitment)) THEN
     RAISE EXCEPTION 'publication replay commitment tuple conflicts' USING ERRCODE='23514';
   END IF;
   RETURN vestrace_embedding_publication_json(target_workspace,target_preparation);
 END IF;
 IF target_publication IS NULL OR target_event IS NULL OR target_projections IS NULL OR target_ordinals IS NULL OR target_commitments IS NULL
   OR cardinality(target_projections)=0 OR cardinality(target_projections)<>cardinality(target_ordinals) OR cardinality(target_projections)<>cardinality(target_commitments)
   OR array_lower(target_projections,1)<>1 OR array_lower(target_ordinals,1)<>1 OR array_lower(target_commitments,1)<>1 THEN
   RAISE EXCEPTION 'publication arguments malformed' USING ERRCODE='22023';
 END IF;
 phase:=vestrace_lock_embedding_result_finalization(target_workspace,target_preparation,target_job,target_effect);
 IF phase='published' THEN
   IF target_projections IS NULL OR target_ordinals IS NULL OR target_commitments IS NULL
     OR cardinality(target_projections) IS DISTINCT FROM (SELECT output_count FROM embedding_job_result_publications WHERE workspace_id=target_workspace AND preparation_id=target_preparation)
     OR cardinality(target_ordinals) IS DISTINCT FROM cardinality(target_projections)
     OR cardinality(target_commitments) IS DISTINCT FROM cardinality(target_projections)
     OR array_lower(target_projections,1)<>1 OR array_lower(target_ordinals,1)<>1 OR array_lower(target_commitments,1)<>1
     OR EXISTS(SELECT 1 FROM embedding_projection_entries p WHERE p.workspace_id=target_workspace AND p.preparation_id=target_preparation
       AND (target_projections[p.output_ordinal+1] IS DISTINCT FROM p.id OR target_ordinals[p.output_ordinal+1] IS DISTINCT FROM p.output_ordinal
         OR target_commitments[p.output_ordinal+1] IS DISTINCT FROM p.output_commitment)) THEN
     RAISE EXCEPTION 'publication replay commitment tuple conflicts' USING ERRCODE='23514';
   END IF;
   RETURN vestrace_embedding_publication_json(target_workspace,target_preparation);
 END IF;
 IF phase<>'ready_to_publish' THEN RAISE EXCEPTION 'publication requires all exact bindings' USING ERRCODE='23514'; END IF;
 SELECT * INTO marker FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND id=target_preparation;
 IF marker.output_count<>cardinality(target_projections) THEN RAISE EXCEPTION 'publication output count conflicts' USING ERRCODE='23514'; END IF;
 FOR output IN SELECT * FROM embedding_projection_entries WHERE workspace_id=target_workspace AND preparation_id=target_preparation ORDER BY output_ordinal LOOP
   IF target_ordinals[position+1] IS DISTINCT FROM position OR target_projections[position+1] IS DISTINCT FROM output.id
     OR target_commitments[position+1] IS NULL OR octet_length(target_commitments[position+1])<>32 THEN
     RAISE EXCEPTION 'publication output order or commitment malformed' USING ERRCODE='22023';
   END IF;
   position:=position+1;
 END LOOP;
 IF position<>marker.output_count THEN RAISE EXCEPTION 'publication projection set incomplete' USING ERRCODE='23514'; END IF;
 SELECT corpus_revision,live_member_count INTO old_revision,old_count FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id;
 SELECT generation_epoch INTO old_epoch FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id;
 SELECT max(projection_ordinal) INTO projection_max FROM embedding_projection_entries WHERE workspace_id=target_workspace AND preparation_id=target_preparation;
 INSERT INTO embedding_job_result_publications VALUES(target_publication,target_workspace,target_preparation,target_job,target_effect,marker.space_registration_id,target_event,marker.output_count,marker.expected_job_version+1,old_revision+1,old_count+marker.output_count,old_epoch+1);
 INSERT INTO embedding_index_rebuild_events VALUES(target_event,target_workspace,target_publication,marker.space_registration_id,old_revision,old_revision+1,old_epoch,old_epoch+1,old_count,old_count+marker.output_count,projection_max,ARRAY(SELECT id FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id AND state='ready' ORDER BY id));
 FOR output IN SELECT * FROM embedding_projection_entries WHERE workspace_id=target_workspace AND preparation_id=target_preparation ORDER BY output_ordinal LOOP
   UPDATE content_materials SET state='live',updated_at=now() WHERE workspace_id=target_workspace AND id=output.material_id AND state='prepared';
   IF NOT FOUND THEN RAISE EXCEPTION 'publication material changed' USING ERRCODE='23514'; END IF;
   DELETE FROM prepared_material_attachments WHERE workspace_id=target_workspace AND intent_id=output.intent_id;
   IF NOT FOUND THEN RAISE EXCEPTION 'publication attachment absent' USING ERRCODE='23514'; END IF;
   UPDATE material_key_creation_intents SET state='live',updated_at=now() WHERE workspace_id=target_workspace AND id=output.intent_id AND state='bound';
   IF NOT FOUND THEN RAISE EXCEPTION 'publication intent changed' USING ERRCODE='23514'; END IF;
   UPDATE embedding_projection_entries SET state='live',retention_eligibility_state='blocked_pending_erasure_propagation',output_commitment=target_commitments[output.output_ordinal+1]
     WHERE workspace_id=target_workspace AND id=output.id;
 END LOOP;
 UPDATE embedding_space_corpus_states SET corpus_revision=old_revision+1,live_member_count=old_count+marker.output_count
   WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id;
 UPDATE embedding_index_generation_guards SET generation_epoch=old_epoch+1 WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id;
 UPDATE embedding_corpus_generations SET state='stale' WHERE workspace_id=target_workspace AND space_registration_id=marker.space_registration_id AND state='ready';
 UPDATE material_erasure_blockers SET state='terminal' WHERE workspace_id=target_workspace AND id IN(
   SELECT blocker_id FROM embedding_delivery_source_memberships WHERE workspace_id=target_workspace AND job_id=target_job
   UNION ALL SELECT blocker_id FROM embedding_job_credential_completion_blockers WHERE workspace_id=target_workspace AND job_id=target_job AND external_effect_id=target_effect);
 UPDATE embedding_jobs SET state='succeeded',version=marker.expected_job_version+1 WHERE workspace_id=target_workspace AND id=target_job AND state='running' AND version=marker.expected_job_version;
 IF NOT FOUND THEN RAISE EXCEPTION 'publication job changed' USING ERRCODE='23514'; END IF;
 PERFORM vestrace_assert_embedding_result_phase(target_workspace,target_preparation,target_job,target_effect);
 RETURN vestrace_embedding_publication_json(target_workspace,target_preparation);
END $$;

CREATE OR REPLACE FUNCTION vestrace_bind_material_key_creation_intent(
    target_intent_id UUID,
    target_bound_receipt UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row material_key_creation_intents%ROWTYPE;
BEGIN
    SELECT *
      INTO intent_row
      FROM material_key_creation_intents
     WHERE id = target_intent_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(intent_row.workspace_id);
    IF intent_row.owner_kind='embedding_job_output' AND NOT EXISTS(
        SELECT 1 FROM embedding_result_key_binding_receipts b WHERE b.workspace_id=intent_row.workspace_id
          AND b.intent_id=intent_row.id AND b.job_id=intent_row.owner_id AND b.output_ordinal=intent_row.output_ordinal
          AND b.material_id=intent_row.material_id AND b.material_key_id=intent_row.material_key_id
          AND b.intent_nonce=intent_row.nonce AND b.binding_receipt=target_bound_receipt) THEN
        RAISE EXCEPTION 'embedding bind requires exact specialized receipt' USING ERRCODE='23514';
    END IF;
    IF intent_row.state NOT IN ('content_prepared', 'result_prepared') THEN
        RAISE EXCEPTION 'only prepared content may bind a material key'
            USING ERRCODE = '23514';
    END IF;

    UPDATE material_key_creation_intents
       SET state = 'bound', bound_receipt = target_bound_receipt, updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;
CREATE OR REPLACE FUNCTION vestrace_finalize_bound_content_material(
    target_intent_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row material_key_creation_intents%ROWTYPE;
    material_state TEXT;
    attachment_marker TEXT;
BEGIN
    SELECT *
      INTO intent_row
      FROM material_key_creation_intents
     WHERE id = target_intent_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(intent_row.workspace_id);
    IF intent_row.owner_kind='embedding_job_output' THEN RAISE EXCEPTION 'embedding output requires specialized publication' USING ERRCODE='23514'; END IF;
    IF intent_row.state <> 'bound' OR intent_row.bound_receipt IS NULL THEN
        RAISE EXCEPTION 'only Bound may finalize content material publication'
            USING ERRCODE = '23514';
    END IF;

    SELECT state INTO material_state
      FROM content_materials
     WHERE intent_id = intent_row.id
     FOR UPDATE;
    SELECT marker INTO attachment_marker
      FROM prepared_material_attachments
     WHERE intent_id = intent_row.id
     FOR UPDATE;
    IF material_state <> 'prepared'
       OR attachment_marker <> intent_row.prepared_marker
       OR NOT EXISTS (SELECT 1 FROM content_material_bytes WHERE intent_id = intent_row.id) THEN
        RAISE EXCEPTION 'Bound finalizer requires the exact prepared attachment and ciphertext'
            USING ERRCODE = '23514';
    END IF;

    UPDATE content_materials
       SET state = 'live', updated_at = NOW()
     WHERE intent_id = intent_row.id;
    DELETE FROM prepared_material_attachments WHERE intent_id = intent_row.id;
    INSERT INTO content_material_ordinary_references (
        id, workspace_id, material_id, intent_id, owner_kind, owner_id, output_ordinal
    )
    VALUES (
        gen_random_uuid(),
        intent_row.workspace_id,
        intent_row.material_id,
        intent_row.id,
        intent_row.owner_kind,
        intent_row.owner_id,
        intent_row.output_ordinal
    );
    UPDATE material_key_creation_intents
       SET state = 'live', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;
CREATE OR REPLACE FUNCTION vestrace_validate_material_key_creation_intent()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_intent_id UUID;
    intent_row material_key_creation_intents%ROWTYPE;
    material_row content_materials%ROWTYPE;
    attachment_marker TEXT;
    byte_count BIGINT;
    attachment_count BIGINT;
    ordinary_reference_count BIGINT;
    erasure_receipt_count BIGINT;
    ciphertext_size BIGINT;
BEGIN
    IF TG_TABLE_NAME = 'material_key_creation_intents' THEN
        target_intent_id := COALESCE(NEW.id, OLD.id);
    ELSE
        target_intent_id := COALESCE(NEW.intent_id, OLD.intent_id);
    END IF;

    SELECT * INTO intent_row
      FROM material_key_creation_intents
     WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;

    SELECT * INTO material_row
      FROM content_materials
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*), MAX(octet_length(ciphertext))
      INTO byte_count, ciphertext_size
      FROM content_material_bytes
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*), MAX(marker)
      INTO attachment_count, attachment_marker
      FROM prepared_material_attachments
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO ordinary_reference_count
      FROM content_material_ordinary_references
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO erasure_receipt_count
      FROM material_key_creation_intent_erasure_receipts
     WHERE intent_id = target_intent_id;

    IF intent_row.state IN ('reserved', 'provisional_created')
       AND (intent_row.vault_receipt IS NOT NULL OR intent_row.bound_receipt IS NOT NULL
            OR material_row.id IS NOT NULL OR byte_count <> 0 OR attachment_count <> 0
            OR ordinary_reference_count <> 0 OR erasure_receipt_count <> 0
            OR intent_row.prepared_marker IS NOT NULL) THEN
        RAISE EXCEPTION 'pre-provisional-receipt intent has dependent material state'
            USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'provisional_receipted'
       AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NOT NULL
            OR material_row.id IS NOT NULL OR byte_count <> 0 OR attachment_count <> 0
            OR ordinary_reference_count <> 0 OR erasure_receipt_count <> 0
            OR intent_row.prepared_marker IS NOT NULL) THEN
        RAISE EXCEPTION 'ProvisionalReceipted intent is inconsistent' USING ERRCODE = '23514';
    END IF;

    IF intent_row.state IN ('content_prepared', 'result_prepared', 'bound')
       AND (
           intent_row.vault_receipt IS NULL
           OR material_row.id IS NULL
           OR material_row.id <> intent_row.material_id
           OR material_row.workspace_id <> intent_row.workspace_id
           OR material_row.material_key_id <> intent_row.material_key_id
           OR material_row.state <> 'prepared'
           OR byte_count <> 1
           OR attachment_count <> 1
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count <> 0
           OR ciphertext_size <> material_row.size_class
           OR attachment_marker <> intent_row.prepared_marker
           OR (intent_row.state = 'bound' AND intent_row.bound_receipt IS NULL)
           OR (intent_row.state <> 'bound' AND intent_row.bound_receipt IS NOT NULL)
       ) THEN
        RAISE EXCEPTION 'prepared or Bound material-key creation intent is inconsistent'
            USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'content_prepared'
       AND intent_row.prepared_marker <> 'content_prepared' THEN
        RAISE EXCEPTION 'ContentPrepared must retain its content marker' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'result_prepared'
       AND intent_row.prepared_marker <> 'result_prepared' THEN
        RAISE EXCEPTION 'ResultPrepared must retain its result marker' USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'content_abandon_prepared'
       AND (
           intent_row.prepared_marker <> 'content_prepared'
           OR intent_row.bound_receipt IS NOT NULL
           OR material_row.id IS NULL
           OR material_row.state <> 'abandon_prepared'
           OR byte_count <> 0
           OR attachment_count <> 0
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count > 1
       ) THEN
        RAISE EXCEPTION 'ContentAbandonPrepared is inconsistent' USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'live'
       AND (
           intent_row.vault_receipt IS NULL
           OR intent_row.bound_receipt IS NULL
           OR material_row.id IS NULL
           OR material_row.state <> 'live'
           OR byte_count <> 1
           OR attachment_count <> 0
           OR ordinary_reference_count <> CASE WHEN intent_row.owner_kind='embedding_job_output' THEN 0 ELSE 1 END
           OR (intent_row.owner_kind='embedding_job_output' AND NOT EXISTS(SELECT 1 FROM embedding_projection_entries p JOIN embedding_job_result_publications u ON u.workspace_id=p.workspace_id AND u.preparation_id=p.preparation_id WHERE p.workspace_id=intent_row.workspace_id AND p.intent_id=intent_row.id AND p.material_id=intent_row.material_id AND p.material_key_id=intent_row.material_key_id AND p.job_id=intent_row.owner_id AND p.output_ordinal=intent_row.output_ordinal AND p.state='live'))
           OR erasure_receipt_count <> 0
           OR ciphertext_size <> material_row.size_class
       ) THEN
        RAISE EXCEPTION 'Live material requires its exact Bound promotion' USING ERRCODE = '23514';
    END IF;

    -- The pre-prepared abort is taken before any marker exists, so it carries
    -- no prepared_marker and no material row. It is a distinct branch, not a
    -- ContentPrepared abort with pieces missing, and the spec refuses to let a
    -- ContentPrepared-only marker stand in for it.
    IF intent_row.state = 'pre_prepared_abandon_prepared'
       AND (
           intent_row.prepared_marker IS NOT NULL
           OR intent_row.bound_receipt IS NOT NULL
           OR material_row.id IS NOT NULL
           OR byte_count <> 0
           OR attachment_count <> 0
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count > 1
       ) THEN
        RAISE EXCEPTION 'a pre-prepared abort must carry no prepared, result or live material'
            USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'abandoned'
       AND (
           intent_row.bound_receipt IS NOT NULL
           OR byte_count <> 0
           OR attachment_count <> 0
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count <> 1
       ) THEN
        RAISE EXCEPTION 'Abandoned material requires a committed abort branch and its receipt'
            USING ERRCODE = '23514';
    END IF;

    -- Which branch produced the Abandoned decides what must remain: the
    -- ordinary one retires its material row, the pre-prepared one never had one.
    IF intent_row.state = 'abandoned'
       AND intent_row.prepared_marker IS NOT NULL
       AND (
           intent_row.prepared_marker <> 'content_prepared'
           OR material_row.id IS NULL
           OR material_row.state <> 'abandoned'
       ) THEN
        RAISE EXCEPTION 'an ordinary Abandoned requires its retired ContentPrepared material'
            USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'abandoned'
       AND intent_row.prepared_marker IS NULL
       AND material_row.id IS NOT NULL THEN
        RAISE EXCEPTION 'a pre-prepared Abandoned must have no material row'
            USING ERRCODE = '23514';
    END IF;

    RETURN NULL;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_load_embedding_result_eligibility(
    target_workspace UUID,target_job UUID,target_effect UUID
) RETURNS TABLE(
    preparation_id UUID, preparation_output_count BIGINT, expected_job_version BIGINT, response_model TEXT, adapter TEXT,
    output_ordinal BIGINT, intent_id UUID, material_id UUID, material_key_id UUID,
    intent_nonce UUID, dimensions INTEGER, preparation_input_ordinal BIGINT,
    preparation_response_index BIGINT
) LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE job embedding_jobs%ROWTYPE; registration embedding_space_registrations%ROWTYPE;
        existing embedding_job_result_preparations%ROWTYPE;
        acceptance embedding_delivery_acceptance_receipts%ROWTYPE;
        policy_cause UUID; policy_attempt INTEGER; marker_complete BOOLEAN;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding result eligibility arguments are malformed' USING ERRCODE='22023';
    END IF;
    -- This loader is marker-first and performs no row lock.  Its caller has
    -- already taken the shared completion prefix; the commit command owns the
    -- canonical embedding-specific lock sequence if preparation is needed.
    SELECT * INTO job FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job;
    IF NOT FOUND OR job.kind <> 'delivery' OR job.external_effect_id <> target_effect THEN
        RAISE EXCEPTION 'embedding result requires its exact delivery job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO registration FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=job.space_registration_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding result eligibility registration is absent' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=job.space_registration_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding result eligibility corpus guard is absent' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=job.space_registration_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding result eligibility generation guard is absent' USING ERRCODE='23514';
    END IF;
    -- An existing marker is authority only when all of the facts it promises
    -- are still present and exact.  A row that was only partially persisted
    -- (or later found corrupted) must not suppress the normal guarded path or
    -- let a replay skip vault work.  Caller-generated ids and ciphertext are
    -- deliberately absent from this semantic check.
    SELECT * INTO existing FROM embedding_job_result_preparations marker
     WHERE marker.workspace_id=target_workspace AND marker.job_id=target_job;
    IF FOUND THEN
        PERFORM vestrace_assert_embedding_result_phase(target_workspace,existing.id,target_job,target_effect);
    END IF;
    RETURN QUERY
    SELECT prepared_marker.id, prepared_marker.output_count,
           COALESCE(prepared_marker.expected_job_version,job.version),
           COALESCE(prepared_marker.response_model,registration.model),COALESCE(prepared_marker.adapter,effect.adapter),
           member.output_ordinal, member.intent_id, intent.material_id, intent.material_key_id,
           intent.nonce, COALESCE(projection.dimensions,registration.dimensions),
           projection.input_ordinal,projection.response_index
      FROM embedding_job_material_intents member
      JOIN material_key_creation_intents intent ON intent.workspace_id=member.workspace_id AND intent.id=member.intent_id
      JOIN external_effect_intents effect ON effect.workspace_id=member.workspace_id AND effect.id=target_effect
      LEFT JOIN embedding_job_result_preparations prepared_marker
        ON prepared_marker.workspace_id=member.workspace_id AND prepared_marker.job_id=member.job_id
      LEFT JOIN embedding_projection_entries projection
        ON projection.workspace_id=member.workspace_id AND projection.preparation_id=prepared_marker.id
       AND projection.output_ordinal=member.output_ordinal
     WHERE member.workspace_id=target_workspace AND member.job_id=target_job
     ORDER BY member.output_ordinal;
END $$;

CREATE OR REPLACE FUNCTION vestrace_lock_embedding_result_completion_authority(
    target_workspace UUID,target_job UUID,target_effect UUID,target_connection UUID,
    target_connection_revision UUID,target_dispatch_transition UUID,target_lease UUID
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE cause provider_dispatch_causes%ROWTYPE; job embedding_jobs%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding result completion authority arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM * FROM vestrace_lock_provider_dispatch_completion_authority(
        target_workspace,target_effect,target_connection,target_connection_revision,target_dispatch_transition,target_lease
    );
    -- The shared authority already owns the permanent connection guard and
    -- holds its immutable dispatch cause/admission/lease/transition tuple.
    -- Read the embedding identity without taking the job/effect lock: the
    -- guarded commit takes those only after credential, space, policy and
    -- source locks in its canonical embedding-specific order.
    SELECT * INTO cause FROM provider_dispatch_causes
     WHERE workspace_id=target_workspace AND external_effect_id=target_effect;
    SELECT * INTO job FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job;
    IF job.id IS NULL OR cause.cause_kind <> 'embedding_job' OR cause.embedding_job_id <> target_job
       OR job.external_effect_id <> target_effect OR job.kind <> 'delivery' OR job.state NOT IN ('running','succeeded')
       OR EXISTS (SELECT 1 FROM embedding_job_pre_dispatch_retirement_authorities termination
                   WHERE termination.workspace_id=target_workspace AND termination.job_id=target_job) THEN
        RAISE EXCEPTION 'embedding result completion authority is not its active delivery job'
            USING ERRCODE='23514';
    END IF;
    IF job.state='succeeded' THEN
      PERFORM vestrace_assert_embedding_result_phase(target_workspace,(SELECT id FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND job_id=target_job),target_job,target_effect);
    END IF;
    RETURN job.id;
END $$;

CREATE OR REPLACE FUNCTION vestrace_commit_embedding_result_preparation(
    target_workspace UUID,target_job UUID,target_effect UUID,target_preparation UUID,target_receipt UUID,
    target_expected_version BIGINT,target_response_model TEXT,target_attachment_ids UUID[],
    target_ciphertexts BYTEA[],target_dimensions INTEGER[]
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE initial_job embedding_jobs%ROWTYPE; job embedding_jobs%ROWTYPE; registration embedding_space_registrations%ROWTYPE;
        acceptance embedding_delivery_acceptance_receipts%ROWTYPE; existing embedding_job_result_preparations%ROWTYPE;
        decision embedding_data_policy_decisions%ROWTYPE; member RECORD; position INTEGER; projection_id UUID;
        projection_ordinal BIGINT; policy_cause UUID; policy_attempt INTEGER;
        snapshot model_binding_snapshots%ROWTYPE; credential_intent UUID := NULL; credential_blocker UUID := NULL;
        effect external_effect_intents%ROWTYPE; source RECORD; source_count BIGINT:=0;
        receipt_recorded_at TIMESTAMPTZ;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL OR target_preparation IS NULL
       OR target_receipt IS NULL OR target_expected_version IS NULL OR target_response_model IS NULL
       OR cardinality(target_attachment_ids) IS NULL OR cardinality(target_attachment_ids)=0
       OR cardinality(target_attachment_ids) <> cardinality(target_ciphertexts)
       OR cardinality(target_attachment_ids) <> cardinality(target_dimensions)
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding result preparation arguments are malformed' USING ERRCODE='22023';
    END IF;
    -- The job/snapshot identities are append-only inputs only.  They are read
    -- without a row lock so the first mutable embedding row follows the
    -- shared completion prefix's permanent connection guard.
    SELECT * INTO initial_job FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job;
    IF NOT FOUND OR initial_job.kind <> 'delivery' OR initial_job.external_effect_id <> target_effect THEN
        RAISE EXCEPTION 'embedding result requires its exact delivery job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_job_result_preparations WHERE workspace_id=target_workspace AND job_id=target_job;
    IF FOUND THEN
      PERFORM vestrace_assert_embedding_result_phase(target_workspace,existing.id,target_job,target_effect);
      IF existing.expected_job_version<>target_expected_version OR existing.response_model<>target_response_model
        OR existing.output_count<>cardinality(target_attachment_ids) OR existing.output_count<>cardinality(target_dimensions)
        OR EXISTS(SELECT 1 FROM embedding_projection_entries p WHERE p.workspace_id=target_workspace AND p.preparation_id=existing.id
          AND p.dimensions IS DISTINCT FROM target_dimensions[p.output_ordinal+1]) THEN
        RAISE EXCEPTION 'prepared semantic replay conflicts' USING ERRCODE='23514';
      END IF;
      RETURN existing.id;
    END IF;
    SELECT * INTO snapshot FROM model_binding_snapshots
     WHERE workspace_id=target_workspace AND id=initial_job.model_binding_snapshot_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result requires its model-binding snapshot' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM connection_execution_guards guard
     WHERE guard.workspace_id=target_workspace AND guard.connection_id=snapshot.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result connection guard is absent' USING ERRCODE='23514'; END IF;
    IF snapshot.branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards guard
         WHERE guard.id=snapshot.credential_activation_guard_id AND guard.workspace_id=target_workspace
           AND guard.connection_id=snapshot.connection_id AND guard.credential_slot_id=snapshot.credential_slot_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding result credential guard is absent' USING ERRCODE='23514'; END IF;
        SELECT intent.id INTO credential_intent FROM credential_key_creation_intents intent
         WHERE intent.workspace_id=target_workspace AND intent.connection_id=snapshot.connection_id
           AND intent.credential_slot_id=snapshot.credential_slot_id
           AND intent.credential_revision_id=snapshot.credential_revision_id AND intent.state='active' FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding result credential intent is not active' USING ERRCODE='23514'; END IF;
        credential_blocker:=vestrace_ensure_embedding_credential_completion_blocker(target_workspace,target_job,target_effect);
    ELSIF snapshot.branch<>'no_auth' OR snapshot.credential_revision_id IS NOT NULL
       OR snapshot.credential_slot_id IS NOT NULL OR snapshot.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding result snapshot has an invalid no-auth branch' USING ERRCODE='23514';
    END IF;
    SELECT * INTO registration FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=initial_job.space_registration_id FOR UPDATE;
    IF NOT FOUND OR registration.model <> target_response_model THEN
        RAISE EXCEPTION 'embedding result response model is not its registered space model' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=initial_job.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result corpus guard is absent' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=initial_job.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result generation guard is absent' USING ERRCODE='23514'; END IF;
    SELECT * INTO acceptance FROM embedding_delivery_acceptance_receipts
     WHERE workspace_id=target_workspace AND job_id=target_job FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result requires delivery acceptance output authority' USING ERRCODE='23514'; END IF;
    IF jsonb_typeof(acceptance.request_tuple->'outbox')<>'array'
       OR jsonb_array_length(acceptance.request_tuple->'outbox')<1
       OR jsonb_typeof(acceptance.request_tuple #> '{outbox,0,id}')<>'string'
       OR jsonb_typeof(acceptance.request_tuple #> '{outbox,0,attempts}')<>'number'
       OR (acceptance.request_tuple #>> '{outbox,0,attempts}') !~ '^(0|[1-9][0-9]*)$'
       OR (acceptance.request_tuple #>> '{outbox,0,attempts}')::NUMERIC > 2147483646 THEN
        RAISE EXCEPTION 'embedding result delivery policy attempt is absent or invalid' USING ERRCODE='23514';
    END IF;
    policy_cause := (acceptance.request_tuple #>> '{outbox,0,id}')::UUID;
    policy_attempt := (acceptance.request_tuple #>> '{outbox,0,attempts}')::INTEGER+1;
    SELECT * INTO decision FROM embedding_data_policy_decisions
     WHERE purpose='delivery' AND causal_reference_id=policy_cause AND delivery_attempt=policy_attempt FOR UPDATE;
    IF NOT FOUND OR decision.verdict <> 'allowed' OR decision.input_count <> cardinality(target_attachment_ids) THEN
        RAISE EXCEPTION 'embedding result requires its exact allowed delivery data-policy decision' USING ERRCODE='23514';
    END IF;
    -- Lock every persisted source fact before authoring any output.  The
    -- tuple is the 0193 membership identity, not a current corpus lookup.
    FOR source IN
        SELECT membership.job_id,membership.output_ordinal,membership.source_ordinal,
               membership.source_material_id,membership.intent_id AS output_intent_id,
               membership.source_intent_id,membership.blocker_id,
               material.state AS material_state,source_intent.state AS source_intent_state,
               blocker.state AS blocker_state
          FROM embedding_delivery_source_memberships membership
          JOIN content_materials material
            ON material.workspace_id=membership.workspace_id AND material.id=membership.source_material_id
          -- 0193's membership intent is the exact output intent. The source
          -- intent is independently persisted and bound to the source material.
          JOIN material_key_creation_intents source_intent
            ON source_intent.workspace_id=material.workspace_id
           AND source_intent.id=membership.source_intent_id
           AND source_intent.id=material.intent_id
          JOIN material_erasure_blockers blocker
            ON blocker.workspace_id=membership.workspace_id AND blocker.id=membership.blocker_id
         WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
         ORDER BY membership.output_ordinal,membership.source_ordinal,
                  membership.source_material_id,membership.intent_id,membership.blocker_id
         FOR UPDATE OF membership,material,source_intent,blocker
    LOOP
        source_count:=source_count+1;
        IF source.material_state<>'live' OR source.source_intent_state<>'live'
           OR source.blocker_state<>'nonterminal' THEN
            RAISE EXCEPTION 'embedding result source membership is no longer live and blocked'
                USING ERRCODE='23514';
        END IF;
    END LOOP;
    -- The source chain is now held.  Re-lock the initially read job and its
    -- effect only after the connection/auth/space/policy/source prefix, then
    -- reject any identity change before touching output material.
    SELECT * INTO job FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
    SELECT * INTO effect FROM external_effect_intents
     WHERE workspace_id=target_workspace AND id=target_effect FOR UPDATE;
    IF job.id IS NULL OR effect.id IS NULL OR job.kind <> 'delivery' OR job.state <> 'running'
       OR job.external_effect_id <> target_effect OR job.version <> target_expected_version
       OR job.space_registration_id <> initial_job.space_registration_id
       OR job.model_binding_snapshot_id <> initial_job.model_binding_snapshot_id
       OR job.model_request_evidence_id <> initial_job.model_request_evidence_id THEN
        RAISE EXCEPTION 'embedding result requires its active expected-version delivery job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_job_result_preparations
     WHERE workspace_id=target_workspace AND job_id=target_job FOR KEY SHARE;
    IF FOUND THEN
        IF existing.external_effect_id=target_effect AND existing.expected_job_version=target_expected_version
           AND existing.response_model=target_response_model AND existing.output_count=cardinality(target_attachment_ids)
           AND (SELECT count(*) FROM embedding_job_result_prepared_attachments attachment
                 WHERE attachment.workspace_id=target_workspace AND attachment.preparation_id=existing.id)
               = cardinality(target_attachment_ids)
           AND (SELECT count(*) FROM embedding_projection_entries projection
                 WHERE projection.workspace_id=target_workspace AND projection.preparation_id=existing.id)
               = cardinality(target_attachment_ids)
           AND NOT EXISTS (
               SELECT 1 FROM embedding_projection_entries projection
                WHERE projection.workspace_id=target_workspace AND projection.preparation_id=existing.id
                  AND (projection.output_ordinal<0
                    OR projection.response_index<>projection.output_ordinal
                    OR projection.output_ordinal>=cardinality(target_dimensions)
                    OR projection.response_model<>target_response_model
                    OR projection.dimensions<>target_dimensions[projection.output_ordinal+1])
           )
           AND EXISTS (
               SELECT 1 FROM external_effect_receipts receipt
                WHERE receipt.workspace_id=target_workspace AND receipt.id=existing.receipt_id
                  AND receipt.effect_id=target_effect AND receipt.outcome_status='acknowledged'
                  AND receipt.payload @> jsonb_build_object(
                      'evidence_refs',jsonb_build_array(
                          format('embedding_job_result_preparation:%s',existing.id)
                      )
                  )
           )
        THEN RETURN existing.id; END IF;
        RAISE EXCEPTION 'embedding result preparation conflicts with its marker semantic tuple' USING ERRCODE='23514';
    END IF;
    -- Lock and validate the complete exact output set before authoring any
    -- receipt, marker, attachment, projection or ciphertext.  The later write
    -- loop only re-locks these already-held rows.
    position := 0;
    FOR member IN SELECT membership.output_ordinal,membership.intent_id,intent.material_id,intent.material_key_id,intent.state,intent.vault_receipt,
                         receipt.intent_id AS output_key_receipt_intent
                    FROM embedding_job_material_intents membership
                    JOIN material_key_creation_intents intent ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
                    JOIN embedding_output_key_receipts receipt ON receipt.workspace_id=membership.workspace_id AND receipt.intent_id=membership.intent_id
                    LEFT JOIN embedding_output_key_retirement_requests retired ON retired.workspace_id=membership.workspace_id AND retired.intent_id=membership.intent_id
                   WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
                   ORDER BY membership.output_ordinal,intent.id FOR UPDATE OF membership,intent,receipt
    LOOP
        IF member.output_ordinal <> position OR member.state <> 'provisional_receipted' OR member.vault_receipt IS NULL
           OR member.output_key_receipt_intent IS NULL
           OR EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests retired WHERE retired.workspace_id=target_workspace AND retired.intent_id=member.intent_id)
           OR target_attachment_ids[position+1] IS NULL
           OR target_dimensions[position+1] <> registration.dimensions
           OR target_ciphertexts[position+1] IS NULL THEN
            RAISE EXCEPTION 'embedding result output set is incomplete or not receipted' USING ERRCODE='23514';
        END IF;
        position := position+1;
    END LOOP;
    IF source_count=0 OR position <> cardinality(target_attachment_ids) OR EXISTS (
        SELECT 1 FROM embedding_job_material_intents output
         WHERE output.workspace_id=target_workspace AND output.job_id=target_job
           AND NOT EXISTS (
               SELECT 1 FROM embedding_delivery_source_memberships membership
                WHERE membership.workspace_id=output.workspace_id AND membership.job_id=output.job_id
                  AND membership.output_ordinal=output.output_ordinal
           )
    ) THEN
        RAISE EXCEPTION 'embedding result requires every complete exact source membership'
            USING ERRCODE='23514';
    END IF;
    -- The guarded command creates the receipt and its lifecycle transition;
    -- no runtime caller can leave an embedding-result receipt without the
    -- marker transaction that owns it.
    receipt_recorded_at:=clock_timestamp();
    INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload,created_at)
    VALUES(
        target_receipt,target_effect,target_workspace,'acknowledged',
        jsonb_build_object(
            'id',target_receipt,'effect_id',target_effect,'adapter',effect.adapter,
            'dispatched_at',receipt_recorded_at,'acknowledgement_at',receipt_recorded_at,
            'response_class','http_200','external_resource_id',NULL,
            'external_version',NULL,'response_digest',NULL,'outcome_status','acknowledged',
            'evidence_refs',jsonb_build_array(format('embedding_job_result_preparation:%s',target_preparation)),
            'recorded_at',receipt_recorded_at
        ),receipt_recorded_at
    );
    INSERT INTO external_effect_lifecycle_transitions(
        effect_id,workspace_id,status,cause,cause_ref,recorded_at
    ) VALUES(
        target_effect,target_workspace,'acknowledged','receipt_recorded',target_receipt::TEXT,receipt_recorded_at
    );
    -- Insert the marker before children. Its completeness trigger is deferred,
    -- while the children use ordinary foreign keys to the marker. Nothing is
    -- visible until this transaction also records every child and receipt.
    INSERT INTO embedding_job_result_preparations(
        id,workspace_id,job_id,external_effect_id,space_registration_id,model_binding_snapshot_id,model_request_evidence_id,
        expected_job_version,adapter,auth_branch,credential_revision_id,credential_intent_id,credential_erasure_blocker_id,
        response_model,output_count,data_policy_decision_id,receipt_id
    ) VALUES(
        target_preparation,target_workspace,job.id,job.external_effect_id,job.space_registration_id,
        job.model_binding_snapshot_id,job.model_request_evidence_id,job.version,effect.adapter,
        snapshot.branch,snapshot.credential_revision_id,credential_intent,credential_blocker,
        target_response_model,cardinality(target_attachment_ids),decision.id,target_receipt
    );
    position := 0;
    FOR member IN SELECT membership.output_ordinal,membership.intent_id,intent.material_id,intent.material_key_id,intent.state,intent.vault_receipt,
                         receipt.intent_id AS output_key_receipt_intent
                    FROM embedding_job_material_intents membership
                    JOIN material_key_creation_intents intent ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
                    JOIN embedding_output_key_receipts receipt ON receipt.workspace_id=membership.workspace_id AND receipt.intent_id=membership.intent_id
                    LEFT JOIN embedding_output_key_retirement_requests retired ON retired.workspace_id=membership.workspace_id AND retired.intent_id=membership.intent_id
                   WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
                   ORDER BY membership.output_ordinal,intent.id FOR UPDATE OF membership,intent,receipt
    LOOP
        IF member.output_ordinal <> position OR member.state <> 'provisional_receipted' OR member.vault_receipt IS NULL
           OR member.output_key_receipt_intent IS NULL
           OR EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests retired WHERE retired.workspace_id=target_workspace AND retired.intent_id=member.intent_id)
           OR target_dimensions[position+1] <> registration.dimensions
           OR target_ciphertexts[position+1] IS NULL THEN
            RAISE EXCEPTION 'embedding result output set is incomplete or not receipted' USING ERRCODE='23514';
        END IF;
        PERFORM vestrace_prepare_result_material(member.intent_id,target_attachment_ids[position+1],target_ciphertexts[position+1],octet_length(target_ciphertexts[position+1]));
        UPDATE embedding_space_corpus_states SET next_projection_ordinal=next_projection_ordinal+1
         WHERE workspace_id=target_workspace AND space_registration_id=job.space_registration_id
         RETURNING next_projection_ordinal-1 INTO projection_ordinal;
        projection_id := gen_random_uuid();
        INSERT INTO embedding_projection_entries(
            id,workspace_id,preparation_id,job_id,input_ordinal,output_ordinal,space_registration_id,projection_ordinal,
            material_id,intent_id,material_key_id,response_index,response_model,dimensions,model_binding_snapshot_id,
            data_policy_decision_id,sensitivity,classification_labels,has_unclassified,retention_eligibility_state,state
        ) VALUES (
            projection_id,target_workspace,target_preparation,target_job,member.output_ordinal,member.output_ordinal,job.space_registration_id,projection_ordinal,
            member.material_id,member.intent_id,member.material_key_id,member.output_ordinal,target_response_model,registration.dimensions,job.model_binding_snapshot_id,
            decision.id,decision.classification,decision.classification_labels,(decision.unclassified_count>0),'blocked_result_finalizing','result_finalizing'
        );
        INSERT INTO embedding_job_result_prepared_attachments(workspace_id,preparation_id,output_ordinal,intent_id,prepared_attachment_id,projection_id)
        VALUES(target_workspace,target_preparation,member.output_ordinal,member.intent_id,target_attachment_ids[position+1],projection_id);
        INSERT INTO embedding_projection_source_dependencies(
            workspace_id,projection_id,job_id,output_ordinal,source_ordinal,
            source_material_id,output_intent_id,source_intent_id,erasure_blocker_id
        )
        SELECT membership.workspace_id,projection_id,membership.job_id,membership.output_ordinal,membership.source_ordinal,
               membership.source_material_id,membership.intent_id,membership.source_intent_id,membership.blocker_id
          FROM embedding_delivery_source_memberships membership
         WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
           AND membership.output_ordinal=member.output_ordinal
         ORDER BY membership.source_ordinal;
        position := position+1;
    END LOOP;
    IF position <> cardinality(target_attachment_ids) THEN
        RAISE EXCEPTION 'embedding result output count does not match accepted outputs' USING ERRCODE='23514';
    END IF;
    PERFORM vestrace_release_provider_dispatch(target_workspace,target_effect,target_receipt);
    RETURN target_preparation;
END $$;

CREATE OR REPLACE FUNCTION vestrace_lock_embedding_job_recovery_authority(
    target_workspace_id UUID,
    target_job_id UUID
)
RETURNS TABLE (
    job_id UUID, workspace_id UUID, space_registration_id UUID, kind TEXT, state TEXT,
    model_binding_snapshot_id UUID, external_effect_id UUID, model_request_evidence_id UUID,
    phase TEXT, connection_id UUID, connection_revision_id UUID, dispatch_transition_id UUID,
    dispatch_expires_at TIMESTAMPTZ, has_receipt BOOLEAN, has_result_preparation BOOLEAN
)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE initial embedding_jobs%ROWTYPE; job embedding_jobs%ROWTYPE; admission RECORD;
        root_connection_id UUID := NULL; canonical_connection_id UUID := NULL;
        canonical_connection_revision_id UUID := NULL; canonical_dispatch_transition_id UUID := NULL;
        canonical_dispatch_expires_at TIMESTAMPTZ := NULL; prepared embedding_job_result_preparations%ROWTYPE;
        receipt_exists BOOLEAN; preparation_exists BOOLEAN := FALSE; attachment_count BIGINT;
        member_count BIGINT; complete_result_prepared BOOLEAN := FALSE; derived_phase TEXT;
BEGIN
    IF target_workspace_id IS NULL OR target_job_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding job recovery authority arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT stored.* INTO initial FROM embedding_jobs stored
     WHERE stored.workspace_id=target_workspace_id AND stored.id=target_job_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job is absent' USING ERRCODE='23514'; END IF;
    SELECT * INTO prepared FROM embedding_job_result_preparations WHERE embedding_job_result_preparations.workspace_id=target_workspace_id AND embedding_job_result_preparations.job_id=target_job_id;
    IF FOUND THEN
      PERFORM vestrace_assert_embedding_result_phase(target_workspace_id,prepared.id,target_job_id,initial.external_effect_id);
      complete_result_prepared:=true;
    ELSE
      IF initial.kind='delivery' AND initial.state='succeeded' THEN RAISE EXCEPTION 'succeeded delivery lacks publication' USING ERRCODE='23514'; END IF;
      PERFORM vestrace_lock_embedding_job_pre_dispatch_gate(target_workspace_id,initial.external_effect_id,TRUE);
    END IF;
    SELECT stored.connection_id,stored.connection_revision_id INTO admission
      FROM connection_dispatch_admissions stored
     WHERE stored.workspace_id=target_workspace_id AND stored.external_effect_id=initial.external_effect_id
       AND stored.decision='admitted' AND stored.model_binding_snapshot_id=initial.model_binding_snapshot_id;
    IF FOUND THEN
        root_connection_id:=admission.connection_id; canonical_connection_id:=admission.connection_id;
        canonical_connection_revision_id:=admission.connection_revision_id;
        PERFORM guard.id FROM connection_execution_guards guard
         WHERE guard.workspace_id=target_workspace_id AND guard.connection_id=root_connection_id FOR UPDATE OF guard;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding job recovery requires its permanent connection guard' USING ERRCODE='23514'; END IF;
    END IF;
    SELECT stored.* INTO job FROM embedding_jobs stored
     WHERE stored.workspace_id=target_workspace_id AND stored.id=initial.id
       AND stored.model_binding_snapshot_id=initial.model_binding_snapshot_id
       AND stored.external_effect_id=initial.external_effect_id
       AND stored.model_request_evidence_id=initial.model_request_evidence_id FOR UPDATE OF stored;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job identity changed before its recovery lock' USING ERRCODE='23514'; END IF;
    SELECT lifecycle.id,lifecycle.dispatch_expires_at INTO canonical_dispatch_transition_id,canonical_dispatch_expires_at
      FROM external_effect_lifecycle_transitions lifecycle
     WHERE lifecycle.workspace_id=target_workspace_id AND lifecycle.effect_id=job.external_effect_id
       AND lifecycle.status='dispatching' AND lifecycle.cause='dispatch_started'
       AND lifecycle.dispatch_expires_at IS NOT NULL
     ORDER BY lifecycle.ordinal DESC LIMIT 1 FOR KEY SHARE OF lifecycle;
    receipt_exists:=EXISTS(SELECT 1 FROM external_effect_receipts receipt
      WHERE receipt.workspace_id=job.workspace_id AND receipt.effect_id=job.external_effect_id);
    SELECT * INTO prepared FROM embedding_job_result_preparations marker
     WHERE marker.workspace_id=job.workspace_id AND marker.job_id=job.id
       AND marker.external_effect_id=job.external_effect_id FOR KEY SHARE OF marker;
    IF FOUND THEN
        receipt_exists:=EXISTS(SELECT 1 FROM external_effect_receipts receipt
          WHERE receipt.workspace_id=job.workspace_id AND receipt.effect_id=job.external_effect_id
            AND receipt.id=prepared.receipt_id AND receipt.outcome_status='acknowledged'
            AND receipt.payload @> jsonb_build_object(
                'evidence_refs', jsonb_build_array(
                    format('embedding_job_result_preparation:%s', prepared.id)
                )
            ));
        SELECT count(*) INTO attachment_count FROM embedding_job_result_prepared_attachments attachment
         WHERE attachment.workspace_id=job.workspace_id AND attachment.preparation_id=prepared.id;
        SELECT count(*) INTO member_count FROM embedding_job_material_intents member
         WHERE member.workspace_id=job.workspace_id AND member.job_id=job.id;
        preparation_exists:=receipt_exists AND prepared.output_count=attachment_count
          AND prepared.output_count=member_count
          AND NOT EXISTS(
             SELECT 1 FROM embedding_job_material_intents member
              LEFT JOIN embedding_job_result_prepared_attachments attachment
                ON attachment.workspace_id=member.workspace_id AND attachment.preparation_id=prepared.id
               AND attachment.output_ordinal=member.output_ordinal AND attachment.intent_id=member.intent_id
             WHERE member.workspace_id=job.workspace_id AND member.job_id=job.id AND attachment.intent_id IS NULL
          )
          AND NOT EXISTS(
             SELECT 1 FROM embedding_delivery_source_memberships source
              LEFT JOIN embedding_projection_entries projection
                ON projection.workspace_id=source.workspace_id AND projection.preparation_id=prepared.id
               AND projection.job_id=source.job_id AND projection.output_ordinal=source.output_ordinal
              LEFT JOIN embedding_projection_source_dependencies dependency
                ON dependency.workspace_id=source.workspace_id AND dependency.projection_id=projection.id
               AND dependency.job_id=source.job_id AND dependency.output_ordinal=source.output_ordinal
               AND dependency.source_ordinal=source.source_ordinal
               AND dependency.source_material_id=source.source_material_id
               AND dependency.output_intent_id=source.intent_id
               AND dependency.source_intent_id=source.source_intent_id
               AND dependency.erasure_blocker_id=source.blocker_id
             WHERE source.workspace_id=job.workspace_id AND source.job_id=job.id
               AND dependency.projection_id IS NULL
          );
    END IF;
    IF job.state='succeeded' THEN derived_phase:='succeeded';
    ELSIF job.state='inconclusive_unknown' THEN derived_phase:='unknown';
    ELSIF job.state IN ('cancelled','failed_definite') THEN derived_phase:=job.state;
    ELSIF receipt_exists AND preparation_exists THEN derived_phase:='result_prepared';
    ELSIF canonical_dispatch_transition_id IS NOT NULL THEN derived_phase:='dispatching';
    ELSIF root_connection_id IS NOT NULL THEN derived_phase:='admitted';
    ELSE derived_phase:='reserved'; END IF;
    RETURN QUERY SELECT job.id,job.workspace_id,job.space_registration_id,job.kind,job.state,
      job.model_binding_snapshot_id,job.external_effect_id,job.model_request_evidence_id,derived_phase,
      canonical_connection_id,canonical_connection_revision_id,canonical_dispatch_transition_id,
      canonical_dispatch_expires_at,receipt_exists,preparation_exists;
END $$;

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_projection_dependency()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE projection embedding_projection_entries%ROWTYPE; dependency_count BIGINT; source_count BIGINT;
        target_projection_id UUID; target_workspace UUID;
BEGIN
    IF TG_TABLE_NAME='embedding_projection_entries' THEN
        target_projection_id:=COALESCE(NEW.id,OLD.id);
        target_workspace:=COALESCE(NEW.workspace_id,OLD.workspace_id);
    ELSE
        target_projection_id:=COALESCE(NEW.projection_id,OLD.projection_id);
        target_workspace:=COALESCE(NEW.workspace_id,OLD.workspace_id);
    END IF;
    SELECT * INTO projection FROM embedding_projection_entries
     WHERE id=target_projection_id AND workspace_id=target_workspace;
    IF NOT FOUND THEN RETURN NULL; END IF;
    SELECT count(*) INTO dependency_count FROM embedding_projection_source_dependencies
     WHERE workspace_id=projection.workspace_id AND projection_id=projection.id;
    SELECT count(*) INTO source_count FROM embedding_delivery_source_memberships
     WHERE workspace_id=projection.workspace_id AND job_id=projection.job_id
       AND output_ordinal=projection.output_ordinal;
    IF dependency_count <> source_count OR EXISTS (
        SELECT 1 FROM embedding_delivery_source_memberships source
         LEFT JOIN embedding_projection_source_dependencies dependency
          ON dependency.workspace_id=source.workspace_id AND dependency.projection_id=projection.id
          AND dependency.source_ordinal=source.source_ordinal
          AND dependency.source_material_id=source.source_material_id
          AND dependency.output_intent_id=source.intent_id
          AND dependency.source_intent_id=source.source_intent_id
          AND dependency.erasure_blocker_id=source.blocker_id
        WHERE source.workspace_id=projection.workspace_id AND source.job_id=projection.job_id
          AND source.output_ordinal=projection.output_ordinal AND dependency.projection_id IS NULL
    ) OR EXISTS (
        SELECT 1 FROM embedding_projection_source_dependencies dependency
        JOIN content_materials material
          ON material.workspace_id=dependency.workspace_id AND material.id=dependency.source_material_id
         AND material.intent_id=dependency.source_intent_id
        JOIN material_key_creation_intents source_intent
          ON source_intent.workspace_id=dependency.workspace_id AND source_intent.id=dependency.source_intent_id
        WHERE dependency.workspace_id=projection.workspace_id AND dependency.projection_id=projection.id
          AND projection.state<>'live' AND (material.state<>'live' OR source_intent.state<>'live')
    ) THEN
        RAISE EXCEPTION 'embedding projection requires every exact ordered delivery source dependency'
            USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_result_preparation()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE marker RECORD; target_workspace UUID;
BEGIN
 target_workspace:=CASE WHEN TG_OP='DELETE' THEN OLD.workspace_id ELSE NEW.workspace_id END;
 -- The OLD workspace is deliberate: a removed generic attachment must not
 -- disappear from an inner-joined validation set.
 FOR marker IN SELECT id,job_id,external_effect_id FROM embedding_job_result_preparations WHERE workspace_id=target_workspace LOOP
   PERFORM vestrace_assert_embedding_result_phase(target_workspace,marker.id,marker.job_id,marker.external_effect_id);
 END LOOP;
 RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION vestrace_guard_embedding_projection_publication()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF TG_OP<>'UPDATE' OR OLD.state<>'result_finalizing' OR NEW.state<>'live'
   OR (to_jsonb(OLD)-'state'-'retention_eligibility_state'-'output_commitment') IS DISTINCT FROM (to_jsonb(NEW)-'state'-'retention_eligibility_state'-'output_commitment')
   OR NOT EXISTS(SELECT 1 FROM embedding_job_result_publications WHERE workspace_id=OLD.workspace_id AND preparation_id=OLD.preparation_id AND job_id=OLD.job_id)
   OR NOT EXISTS(SELECT 1 FROM embedding_result_key_binding_receipts WHERE workspace_id=OLD.workspace_id AND preparation_id=OLD.preparation_id AND projection_id=OLD.id AND intent_id=OLD.intent_id) THEN
   RAISE EXCEPTION 'embedding projection permits only exact publication promotion' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
DROP TRIGGER embedding_projection_entries_immutable ON embedding_projection_entries;
CREATE TRIGGER embedding_projection_entries_immutable BEFORE UPDATE OR DELETE ON embedding_projection_entries
 FOR EACH ROW EXECUTE FUNCTION vestrace_guard_embedding_projection_publication();

CREATE OR REPLACE FUNCTION vestrace_guard_embedding_credential_completion_blocker()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE owned embedding_job_credential_completion_blockers%ROWTYPE;
BEGIN
 SELECT * INTO owned FROM embedding_job_credential_completion_blockers WHERE workspace_id=OLD.workspace_id AND blocker_id=OLD.id;
 IF NOT FOUND THEN IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF; END IF;
 IF TG_OP<>'UPDATE' OR OLD.state<>'nonterminal' OR NEW.state<>'terminal'
   OR (to_jsonb(OLD)-'state'-'updated_at') IS DISTINCT FROM (to_jsonb(NEW)-'state'-'updated_at')
   OR NOT EXISTS(SELECT 1 FROM embedding_job_result_publications WHERE workspace_id=owned.workspace_id AND job_id=owned.job_id AND external_effect_id=owned.external_effect_id) THEN
   RAISE EXCEPTION 'credential completion blocker requires its exact publication' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER embedding_credential_completion_blocker_guard BEFORE UPDATE OR DELETE ON material_erasure_blockers
 FOR EACH ROW EXECUTE FUNCTION vestrace_guard_embedding_credential_completion_blocker();

DO $$ DECLARE target REGCLASS; BEGIN
 FOREACH target IN ARRAY ARRAY[
   'prepared_material_attachments'::regclass,'embedding_job_result_prepared_attachments'::regclass,
   'embedding_projection_entries'::regclass,'embedding_projection_source_dependencies'::regclass,
   'embedding_result_key_binding_receipts'::regclass,'material_key_creation_intents'::regclass,
   'content_materials'::regclass,'content_material_bytes'::regclass,'content_material_ordinary_references'::regclass,
   'embedding_job_result_publications'::regclass,'embedding_index_rebuild_events'::regclass,
   'embedding_jobs'::regclass,'material_erasure_blockers'::regclass,
   'embedding_space_corpus_states'::regclass,'embedding_index_generation_guards'::regclass,
   'embedding_corpus_generations'::regclass,'embedding_job_credential_completion_blockers'::regclass,
   'embedding_result_credential_blocker_adoptions'::regclass
 ] LOOP
   EXECUTE format('CREATE CONSTRAINT TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %s DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_result_preparation()',target::text||'_finalization_complete',target);
 END LOOP;
END $$;

DO $$ DECLARE item TEXT; target REGPROCEDURE; BEGIN
 PERFORM vestrace_finish_p04_result_finalization_upgrade();
EXCEPTION WHEN undefined_function THEN
 IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
   RAISE EXCEPTION 'finalization ownership hand-back unavailable' USING ERRCODE='42501';
 END IF;

 FOREACH item IN ARRAY ARRAY['embedding_jobs','embedding_job_result_preparations','embedding_job_result_prepared_attachments','embedding_space_corpus_states','embedding_projection_entries','prepared_material_attachments','embedding_projection_source_dependencies','material_key_creation_intents','content_materials','content_material_bytes','content_material_ordinary_references','material_erasure_blockers','embedding_index_generation_guards','embedding_corpus_generations','embedding_job_credential_completion_blockers','embedding_result_credential_blocker_adoptions','embedding_result_key_binding_receipts','embedding_job_result_publications','embedding_index_rebuild_events'] LOOP
   EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace_guarded_owner',item);
   EXECUTE format('REVOKE INSERT,UPDATE,DELETE,TRUNCATE,TRIGGER,REFERENCES ON TABLE public.%I FROM vestrace',item);
   -- Preserve only preexisting reads:0187:419-421,0194:1447-1448, provisioner922-934.
   IF item=ANY(ARRAY['embedding_jobs','embedding_corpus_generations','embedding_space_corpus_states','embedding_index_generation_guards','embedding_job_result_preparations','embedding_projection_entries','embedding_job_result_prepared_attachments','embedding_projection_source_dependencies','material_key_creation_intents','content_materials','content_material_bytes']) THEN EXECUTE format('GRANT SELECT ON TABLE public.%I TO vestrace',item); END IF;
   IF item=ANY(ARRAY['embedding_jobs','embedding_corpus_generations','material_key_creation_intents','content_materials']) THEN EXECUTE format('GRANT REFERENCES ON TABLE public.%I TO vestrace',item); END IF;
 END LOOP;
 FOREACH item IN ARRAY ARRAY['vestrace_validate_embedding_credential_completion_owner()','vestrace_ensure_embedding_credential_completion_blocker(uuid,uuid,uuid)','vestrace_adopt_embedding_result_credential_blocker(uuid,uuid,uuid,uuid)','vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)','vestrace_embedding_output_binding_identities(uuid,uuid)','vestrace_embedding_publication_json(uuid,uuid)','vestrace_lock_embedding_result_finalization(uuid,uuid,uuid,uuid)','vestrace_load_embedding_result_finalization(uuid,uuid,uuid,uuid)','vestrace_record_embedding_result_key_binding(uuid,uuid,uuid,uuid,bigint,uuid)','vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])','vestrace_bind_material_key_creation_intent(uuid,uuid)','vestrace_finalize_bound_content_material(uuid)','vestrace_validate_material_key_creation_intent()','vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)','vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)','vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])','vestrace_lock_embedding_job_recovery_authority(uuid,uuid)','vestrace_validate_embedding_projection_dependency()','vestrace_validate_embedding_result_preparation()','vestrace_guard_embedding_projection_publication()','vestrace_guard_embedding_credential_completion_blocker()'] LOOP
   target:=to_regprocedure('public.'||item);
   IF target IS NULL THEN RAISE EXCEPTION 'finalization function absent: %',item USING ERRCODE='42501'; END IF;
   EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target);
   EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target);
   IF split_part(item,'(',1)=ANY(ARRAY['vestrace_adopt_embedding_result_credential_blocker','vestrace_load_embedding_result_finalization','vestrace_record_embedding_result_key_binding','vestrace_publish_embedding_job_result','vestrace_bind_material_key_creation_intent','vestrace_finalize_bound_content_material','vestrace_lock_embedding_result_completion_authority','vestrace_load_embedding_result_eligibility','vestrace_commit_embedding_result_preparation','vestrace_lock_embedding_job_recovery_authority']) THEN EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target); END IF;
 END LOOP;
 -- Restore exact documented0191fallback grant lost in its two-phase owner bridge.
 GRANT EXECUTE ON FUNCTION public.vestrace_publish_embedding_corpus_generation(UUID,UUID,UUID,BIGINT) TO vestrace;
 GRANT REFERENCES ON TABLE public.credential_revisions,public.credential_key_creation_intents,public.embedding_space_registrations TO vestrace;

END $$;
