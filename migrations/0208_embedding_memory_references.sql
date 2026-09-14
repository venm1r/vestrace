-- Canonical memory provenance. Historical terminal results remain version zero.
DO $upgrade$ BEGIN
 IF to_regprocedure('public.vestrace_prepare_embedding_memory_references_upgrade()') IS NOT NULL THEN
  PERFORM vestrace_prepare_embedding_memory_references_upgrade();
 ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
  RAISE EXCEPTION 'embedding memory references upgrade must be provisioned' USING ERRCODE='42501';
 END IF;
END $upgrade$;

ALTER TABLE embedding_retrieval_results ADD COLUMN provenance_version INTEGER NOT NULL DEFAULT 0 CHECK(provenance_version IN (0,1));
ALTER TABLE embedding_retrieval_result_references ADD COLUMN projection_id UUID, ADD COLUMN source_material_id UUID,
 ADD CONSTRAINT embedding_retrieval_reference_provenance_pair CHECK((projection_id IS NULL)=(source_material_id IS NULL));
ALTER TABLE embedding_retrieval_fences ADD COLUMN built_through_projection_ordinal BIGINT;

CREATE FUNCTION vestrace_resolve_embedding_memory_references(
 target_workspace UUID,target_generation UUID,target_projection_ids UUID[] DEFAULT NULL)
RETURNS TABLE(projection_id UUID,source_material_id UUID,memory_id UUID,revision_id UUID)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE g embedding_corpus_generations%ROWTYPE; c embedding_space_corpus_states%ROWTYPE;
 guard_row embedding_index_generation_guards%ROWTYPE; candidate RECORD;
BEGIN
 IF target_workspace IS NULL OR target_generation IS NULL
    OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
  RAISE EXCEPTION 'embedding memory reference workspace context is required' USING ERRCODE='42501';
 END IF;
 IF target_projection_ids IS NOT NULL AND (cardinality(target_projection_ids)>100000
    OR array_ndims(target_projection_ids)>1 OR (cardinality(target_projection_ids)>0 AND array_lower(target_projection_ids,1)<>1)
    OR array_position(target_projection_ids,NULL) IS NOT NULL) THEN
  RAISE EXCEPTION 'embedding memory reference projection bound is malformed' USING ERRCODE='22023';
 END IF;
 SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=target_generation;
 IF NOT FOUND THEN RAISE EXCEPTION 'embedding memory reference generation is absent' USING ERRCODE='23514'; END IF;
 SELECT * INTO c FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=g.space_registration_id FOR SHARE;
 SELECT * INTO guard_row FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=g.space_registration_id FOR SHARE;
 SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=target_generation FOR SHARE;
 IF c.workspace_id IS NULL OR guard_row.workspace_id IS NULL OR g.id IS NULL
    OR g.member_representation IS DISTINCT FROM 'encrypted_projection' OR g.state IS DISTINCT FROM 'ready'
    OR guard_row.current_generation_id IS DISTINCT FROM g.id OR guard_row.generation_epoch IS DISTINCT FROM g.generation_epoch
    OR guard_row.guard_version IS DISTINCT FROM g.captured_guard_version+1
    OR g.built_through_projection_ordinal IS DISTINCT FROM (
      SELECT coalesce(max(p.projection_ordinal),0) FROM embedding_projection_entries p
      JOIN content_materials v ON v.workspace_id=p.workspace_id AND v.id=p.material_id AND v.state='live'
      WHERE p.workspace_id=target_workspace AND p.space_registration_id=g.space_registration_id AND p.state='live')
    OR g.corpus_revision IS DISTINCT FROM c.corpus_revision OR g.member_count IS DISTINCT FROM c.live_member_count
    OR g.member_count IS DISTINCT FROM
      (SELECT count(*) FROM embedding_corpus_generation_members WHERE workspace_id=target_workspace AND corpus_generation_id=target_generation) THEN
  RAISE EXCEPTION 'embedding memory references require exact current canonical generation' USING ERRCODE='23514';
 END IF;
 -- Publication and erasure take corpus/guard before projection and material locks.
 PERFORM p.id FROM embedding_projection_entries p JOIN embedding_corpus_generation_members gm
 ON gm.workspace_id=p.workspace_id AND gm.embedding_projection_entry_id=p.id
 WHERE gm.workspace_id=target_workspace AND gm.corpus_generation_id=target_generation
 AND (target_projection_ids IS NULL OR p.id=ANY(target_projection_ids)) ORDER BY p.id FOR SHARE OF p;
 PERFORM m.id FROM content_materials m WHERE m.workspace_id=target_workspace AND m.id IN (
 SELECT p.material_id FROM embedding_projection_entries p JOIN embedding_corpus_generation_members gm
 ON gm.workspace_id=p.workspace_id AND gm.embedding_projection_entry_id=p.id
 WHERE gm.workspace_id=target_workspace AND gm.corpus_generation_id=target_generation
 AND (target_projection_ids IS NULL OR p.id=ANY(target_projection_ids))
 UNION SELECT d.source_material_id FROM embedding_projection_source_dependencies d JOIN embedding_corpus_generation_members gm
 ON gm.workspace_id=d.workspace_id AND gm.embedding_projection_entry_id=d.projection_id
 WHERE gm.workspace_id=target_workspace AND gm.corpus_generation_id=target_generation
 AND (target_projection_ids IS NULL OR d.projection_id=ANY(target_projection_ids))) ORDER BY m.id FOR SHARE;
 FOR candidate IN
  SELECT p.id AS projection, (array_agg(s.id ORDER BY s.id))[1] AS source,
    (array_agg(r.memory_id ORDER BY s.id))[1] AS memory,
    (array_agg(o.owner_id ORDER BY s.id))[1] AS revision, count(DISTINCT o.owner_id) AS revision_count
  FROM embedding_corpus_generation_members gm
  JOIN embedding_projection_entries p ON p.workspace_id=gm.workspace_id AND p.id=gm.embedding_projection_entry_id
    AND p.space_registration_id=g.space_registration_id AND p.state='live'
  JOIN content_materials v ON v.workspace_id=p.workspace_id AND v.id=p.material_id AND v.state='live'
  JOIN embedding_projection_source_dependencies d ON d.workspace_id=p.workspace_id AND d.projection_id=p.id
  JOIN content_materials s ON s.workspace_id=d.workspace_id AND s.id=d.source_material_id AND s.state='live'
  JOIN material_key_creation_intents i ON i.workspace_id=s.workspace_id AND i.id=s.intent_id AND i.material_id=s.id AND i.state='live'
  JOIN content_material_ordinary_references o ON o.workspace_id=s.workspace_id AND o.material_id=s.id AND o.intent_id=i.id
    AND o.owner_kind=i.owner_kind AND o.owner_id=i.owner_id AND o.output_ordinal=i.output_ordinal AND o.owner_kind='memory_revision'
  LEFT JOIN memory_revisions r ON r.workspace_id=o.workspace_id AND r.id=o.owner_id
  LEFT JOIN memories m ON m.workspace_id=r.workspace_id AND m.id=r.memory_id
  WHERE gm.workspace_id=target_workspace AND gm.corpus_generation_id=target_generation
    AND (target_projection_ids IS NULL OR p.id=ANY(target_projection_ids))
    AND NOT EXISTS(SELECT 1 FROM embedding_projection_source_dependencies dead
      LEFT JOIN content_materials dm ON dm.workspace_id=dead.workspace_id AND dm.id=dead.source_material_id
      WHERE dead.workspace_id=p.workspace_id AND dead.projection_id=p.id AND (dm.id IS NULL OR dm.state<>'live'))
  GROUP BY p.id ORDER BY p.id
 LOOP
  IF candidate.revision_count<>1 THEN RAISE EXCEPTION 'embedding memory reference owner is ambiguous' USING ERRCODE='23514'; END IF;
  PERFORM m.id FROM memories m JOIN memory_revisions r ON r.workspace_id=m.workspace_id AND r.memory_id=m.id
   WHERE m.workspace_id=target_workspace AND m.id=candidate.memory AND m.status<>'deleted'
    AND r.id=candidate.revision FOR SHARE OF m,r;
  IF FOUND THEN
   projection_id:=candidate.projection; source_material_id:=candidate.source; memory_id:=candidate.memory; revision_id:=candidate.revision;
   RETURN NEXT;
  END IF;
 END LOOP;
END $$;

CREATE FUNCTION vestrace_accept_embedding_retrieval_attempt(
 target_workspace UUID,target_job UUID,target_request UUID,target_space UUID,expected_generation UUID,target_deadline TIMESTAMPTZ)
RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE g embedding_corpus_generations%ROWTYPE; c embedding_space_corpus_states%ROWTYPE;
 guard_row embedding_index_generation_guards%ROWTYPE; j embedding_jobs%ROWTYPE; result UUID;
BEGIN
 IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
  RAISE EXCEPTION 'embedding retrieval workspace context is required' USING ERRCODE='42501'; END IF;
 IF target_job IS NULL OR target_request IS NULL OR target_space IS NULL OR expected_generation IS NULL OR target_deadline IS NULL THEN
  RAISE EXCEPTION 'embedding retrieval acceptance arguments are malformed' USING ERRCODE='22023'; END IF;
 SELECT * INTO c FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=target_space FOR SHARE;
 SELECT * INTO guard_row FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=target_space FOR SHARE;
 IF c.workspace_id IS NULL OR guard_row.workspace_id IS NULL OR guard_row.current_generation_id IS DISTINCT FROM expected_generation THEN
  RAISE EXCEPTION 'embedding retrieval acceptance generation changed' USING ERRCODE='23514'; END IF;
 SELECT * INTO g FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=expected_generation FOR SHARE;
 IF NOT FOUND OR g.state IS DISTINCT FROM 'ready' OR g.member_representation IS DISTINCT FROM 'encrypted_projection'
 OR g.space_registration_id IS DISTINCT FROM target_space OR g.generation_epoch IS DISTINCT FROM guard_row.generation_epoch
 OR guard_row.guard_version IS DISTINCT FROM g.captured_guard_version+1
    OR g.built_through_projection_ordinal IS DISTINCT FROM (
      SELECT coalesce(max(p.projection_ordinal),0) FROM embedding_projection_entries p
      JOIN content_materials v ON v.workspace_id=p.workspace_id AND v.id=p.material_id AND v.state='live'
      WHERE p.workspace_id=target_workspace AND p.space_registration_id=g.space_registration_id AND p.state='live')
    OR g.corpus_revision IS DISTINCT FROM c.corpus_revision OR g.member_count IS DISTINCT FROM c.live_member_count THEN
  RAISE EXCEPTION 'embedding retrieval acceptance requires a Ready current generation' USING ERRCODE='23514'; END IF;
 SELECT * INTO j FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
 IF NOT FOUND OR j.kind IS DISTINCT FROM 'retrieval_query' OR j.space_registration_id IS DISTINCT FROM target_space
 OR j.state NOT IN ('requested','running') THEN
  RAISE EXCEPTION 'embedding retrieval acceptance requires its exact retrieval_query job' USING ERRCODE='23514'; END IF;
 result:=gen_random_uuid();
 INSERT INTO embedding_retrieval_fences(id,workspace_id,job_id,request_id,space_registration_id,generation_id,generation_epoch,
 guard_version,corpus_revision,member_count,deadline,built_through_projection_ordinal)
 VALUES(result,target_workspace,target_job,target_request,target_space,g.id,g.generation_epoch,guard_row.guard_version,g.corpus_revision,g.member_count,target_deadline,g.built_through_projection_ordinal);
 RETURN result;
END $$;

CREATE FUNCTION vestrace_finalize_embedding_retrieval_result(
 target_workspace UUID,target_job UUID,target_fence UUID,target_projection_ids UUID[],target_source_material_ids UUID[],
 target_memory_ids UUID[],target_revision_ids UUID[],target_ranks BIGINT[],target_scores DOUBLE PRECISION[])
RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE fence_row embedding_retrieval_fences%ROWTYPE; guard_row embedding_index_generation_guards%ROWTYPE;
 generation_row embedding_corpus_generations%ROWTYPE; corpus_row embedding_space_corpus_states%ROWTYPE;
 job_row embedding_jobs%ROWTYPE; result_id UUID; total INTEGER; position INTEGER; resolved RECORD; validated INTEGER:=0;
BEGIN
 IF target_workspace IS NULL OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
  RAISE EXCEPTION 'embedding retrieval workspace context is required' USING ERRCODE='42501'; END IF;
 IF target_job IS NULL OR target_fence IS NULL OR target_projection_ids IS NULL OR target_source_material_ids IS NULL
 OR target_memory_ids IS NULL OR target_revision_ids IS NULL OR target_ranks IS NULL OR target_scores IS NULL THEN
  RAISE EXCEPTION 'embedding retrieval finalization arguments are malformed' USING ERRCODE='22023'; END IF;
 total:=cardinality(target_memory_ids);
 IF total>1024 OR cardinality(target_projection_ids)<>total OR cardinality(target_source_material_ids)<>total
 OR cardinality(target_revision_ids)<>total OR cardinality(target_ranks)<>total OR cardinality(target_scores)<>total
 OR (total>0 AND (array_lower(target_projection_ids,1)<>1 OR array_lower(target_source_material_ids,1)<>1
 OR array_lower(target_memory_ids,1)<>1 OR array_lower(target_revision_ids,1)<>1 OR array_lower(target_ranks,1)<>1 OR array_lower(target_scores,1)<>1))
 OR array_ndims(target_projection_ids)>1 OR array_ndims(target_source_material_ids)>1 OR array_ndims(target_memory_ids)>1
 OR array_ndims(target_revision_ids)>1 OR array_ndims(target_ranks)>1 OR array_ndims(target_scores)>1 THEN
  RAISE EXCEPTION 'embedding retrieval finalization requires bounded parallel reference arrays' USING ERRCODE='22023'; END IF;
 SELECT * INTO fence_row FROM embedding_retrieval_fences WHERE workspace_id=target_workspace AND id=target_fence AND job_id=target_job;
 IF NOT FOUND THEN RAISE EXCEPTION 'embedding retrieval finalization requires its exact fence' USING ERRCODE='23514'; END IF;
 SELECT * INTO corpus_row FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=fence_row.space_registration_id FOR SHARE;
 SELECT * INTO guard_row FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=fence_row.space_registration_id FOR SHARE;
 SELECT * INTO generation_row FROM embedding_corpus_generations WHERE workspace_id=target_workspace AND id=fence_row.generation_id FOR SHARE;
    IF guard_row.current_generation_id IS DISTINCT FROM fence_row.generation_id
       OR guard_row.guard_version <> fence_row.guard_version
       OR generation_row.state <> 'ready'
       OR generation_row.generation_epoch <> fence_row.generation_epoch
       OR generation_row.corpus_revision <> fence_row.corpus_revision
       OR generation_row.member_count <> fence_row.member_count THEN
  RAISE EXCEPTION 'embedding retrieval result requires its exact pinned generation' USING ERRCODE='23514'; END IF;
 IF corpus_row.workspace_id IS NULL OR guard_row.workspace_id IS NULL OR generation_row.id IS NULL
 OR generation_row.member_representation IS DISTINCT FROM 'encrypted_projection'
 OR generation_row.space_registration_id IS DISTINCT FROM fence_row.space_registration_id
 OR generation_row.built_through_projection_ordinal IS DISTINCT FROM fence_row.built_through_projection_ordinal
 OR corpus_row.corpus_revision IS DISTINCT FROM fence_row.corpus_revision OR corpus_row.live_member_count IS DISTINCT FROM fence_row.member_count
 OR guard_row.generation_epoch IS DISTINCT FROM fence_row.generation_epoch THEN
  RAISE EXCEPTION 'embedding retrieval result requires its exact pinned generation' USING ERRCODE='23514'; END IF;
 -- Exact completed query dispatch evidence is mandatory even for an empty answer.
 IF NOT EXISTS(
  SELECT 1 FROM embedding_jobs j
  JOIN provider_dispatch_causes cause ON cause.workspace_id=j.workspace_id AND cause.external_effect_id=j.external_effect_id
    AND cause.cause_kind='embedding_job' AND cause.embedding_job_id=j.id
  JOIN connection_dispatch_admissions admission ON admission.workspace_id=cause.workspace_id AND admission.external_effect_id=cause.external_effect_id
    AND admission.decision='admitted' AND admission.model_binding_snapshot_id=j.model_binding_snapshot_id
  JOIN external_effect_lifecycle_transitions dispatch ON dispatch.workspace_id=j.workspace_id AND dispatch.effect_id=j.external_effect_id AND dispatch.status='dispatching'
  JOIN external_effect_receipts receipt ON receipt.workspace_id=j.workspace_id AND receipt.effect_id=j.external_effect_id
    AND receipt.outcome_status='acknowledged' AND receipt.payload->>'response_class'='retrieval_query_embedded'
  JOIN external_effect_lifecycle_transitions acknowledged ON acknowledged.workspace_id=j.workspace_id AND acknowledged.effect_id=j.external_effect_id
    AND acknowledged.status='acknowledged' AND acknowledged.cause='receipt_recorded' AND acknowledged.cause_ref=receipt.id::text
  JOIN provider_concurrency_leases lease ON lease.workspace_id=j.workspace_id AND lease.external_effect_id=j.external_effect_id
    AND lease.released_receipt_id=receipt.id AND lease.released_at IS NOT NULL
  WHERE j.workspace_id=target_workspace AND j.id=target_job AND j.kind='retrieval_query' AND j.state='running'
    AND j.space_registration_id=fence_row.space_registration_id
 ) THEN RAISE EXCEPTION 'embedding retrieval result requires completed query dispatch authority' USING ERRCODE='23514'; END IF;
 -- Resolve even empty results: stale and historical-empty attempts are not an escape hatch.
 FOR resolved IN SELECT * FROM vestrace_resolve_embedding_memory_references(target_workspace,fence_row.generation_id,target_projection_ids) LOOP
  position:=array_position(target_projection_ids,resolved.projection_id);
  IF target_source_material_ids[position] IS DISTINCT FROM resolved.source_material_id
  OR target_memory_ids[position] IS DISTINCT FROM resolved.memory_id OR target_revision_ids[position] IS DISTINCT FROM resolved.revision_id THEN
   RAISE EXCEPTION 'embedding retrieval result requires exact canonical memory provenance' USING ERRCODE='23514'; END IF;
  validated:=validated+1;
 END LOOP;
 IF validated<>total OR (SELECT count(DISTINCT id) FROM unnest(target_revision_ids) id)<>total THEN
  RAISE EXCEPTION 'embedding retrieval result requires unique canonical memory references' USING ERRCODE='23514'; END IF;
 IF total>0 THEN FOR position IN 1..total LOOP
  IF target_ranks[position] IS NULL OR target_ranks[position]<0 OR target_ranks[position]>2147483647
  OR (position>1 AND target_ranks[position]<=target_ranks[position-1]) OR target_scores[position] IS NULL
  OR target_scores[position] NOT BETWEEN '-1.7976931348623157e308'::float8 AND '1.7976931348623157e308'::float8 THEN
   RAISE EXCEPTION 'embedding retrieval result requires increasing nonnegative ranks and finite scores' USING ERRCODE='22023'; END IF;
 END LOOP; END IF;
 SELECT * INTO fence_row FROM embedding_retrieval_fences WHERE workspace_id=target_workspace AND id=target_fence AND job_id=target_job FOR UPDATE;
 SELECT * INTO job_row FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
 IF job_row.id IS NULL OR fence_row.id IS NULL OR job_row.kind IS DISTINCT FROM 'retrieval_query'
 OR job_row.space_registration_id IS DISTINCT FROM fence_row.space_registration_id
 OR job_row.state IS DISTINCT FROM 'running' THEN
  RAISE EXCEPTION 'embedding retrieval finalization requires its exact unfinished job' USING ERRCODE='23514'; END IF;
 IF EXISTS(SELECT 1 FROM embedding_retrieval_generation_changes WHERE workspace_id=target_workspace AND job_id=target_job) THEN
  RAISE EXCEPTION 'embedding retrieval attempt already closed as generation changed' USING ERRCODE='23514'; END IF;
 result_id:=gen_random_uuid();
 INSERT INTO embedding_retrieval_results(id,workspace_id,job_id,fence_id,reference_count,provenance_version)
 VALUES(result_id,target_workspace,target_job,target_fence,total,1);
 IF total>0 THEN FOR position IN 1..total LOOP
 INSERT INTO embedding_retrieval_result_references(workspace_id,result_id,ordinal,projection_id,source_material_id,memory_id,revision_id,rank,score)
 VALUES(target_workspace,result_id,position-1,target_projection_ids[position],target_source_material_ids[position],target_memory_ids[position],target_revision_ids[position],target_ranks[position],target_scores[position]);
 END LOOP; END IF;
 UPDATE embedding_jobs SET state='succeeded',version=version+1 WHERE workspace_id=target_workspace AND id=target_job;
 RETURN result_id;
END $$;

CREATE OR REPLACE FUNCTION vestrace_accept_embedding_retrieval_attempt(target_workspace UUID,target_job UUID,target_request UUID,target_space UUID,target_deadline TIMESTAMPTZ)
RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN RAISE EXCEPTION 'unchecked retrieval admission signature is retired' USING ERRCODE='42501'; END $$;
CREATE OR REPLACE FUNCTION vestrace_finalize_embedding_retrieval_result(target_workspace UUID,target_job UUID,target_fence UUID,target_memory_ids UUID[],target_revision_ids UUID[],target_ranks BIGINT[],target_scores DOUBLE PRECISION[])
RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN RAISE EXCEPTION 'unchecked retrieval finalizer signature is retired' USING ERRCODE='42501'; END $$;

-- Bring source erasure into the same corpus/guard/material order as capture.
-- Inventory is checked again after the material lock: a new space is a retry,
-- never a late acquisition of a corpus lock while holding a material lock.
DO $patch$
DECLARE definition TEXT; needle TEXT; replacement TEXT;
BEGIN

 -- Owner locks precede every material/corpus/intent lock in all three entrypoints.
 -- Immutable intent ownership supplies the key; existing guarded reads recheck it.
 definition:=pg_get_functiondef('vestrace_prepare_content_material_erasure(uuid)'::regprocedure);
 needle:='BEGIN';
 replacement:=$body$BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(i.workspace_id::text || ':' || i.owner_id::text,208))
      FROM content_materials m JOIN material_key_creation_intents i ON i.workspace_id=m.workspace_id AND i.id=m.intent_id
      WHERE m.id=target_material_id AND i.owner_kind='memory_revision'
        AND i.workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::uuid;
$body$;
 EXECUTE replace(definition,needle,replacement);
 definition:=pg_get_functiondef('vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)'::regprocedure);
 definition:=replace(definition,'BEGIN',$body$BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(i.workspace_id::text || ':' || i.owner_id::text,208))
      FROM content_materials m JOIN material_key_creation_intents i ON i.workspace_id=m.workspace_id AND i.id=m.intent_id
      WHERE m.workspace_id=target_workspace AND m.id=target_material AND i.owner_kind='memory_revision'
        AND i.workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::uuid;
$body$);
 needle:='    SELECT * INTO material FROM content_materials';
 replacement:=$body$
    FOR space IN
      SELECT DISTINCT entry.space_registration_id AS registration
      FROM embedding_projection_source_dependencies dependency JOIN embedding_projection_entries entry
      ON entry.workspace_id=dependency.workspace_id AND entry.id=dependency.projection_id
      WHERE dependency.workspace_id=target_workspace AND dependency.source_material_id=target_material
      ORDER BY 1
    LOOP
      PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=space.registration FOR UPDATE;
      PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=space.registration FOR UPDATE;
      affected_spaces:=affected_spaces || space.registration;
    END LOOP;
    SELECT * INTO material FROM content_materials$body$;
 IF strpos(definition,needle)=0 THEN RAISE EXCEPTION 'source erasure lock-order predecessor differs'; END IF;
 definition:=replace(definition,needle,replacement);
 needle:='    -- Every space holding a projection computed from this source, locked in a';
 replacement:=$body$
    IF EXISTS(SELECT 1 FROM embedding_projection_source_dependencies dependency JOIN embedding_projection_entries entry
      ON entry.workspace_id=dependency.workspace_id AND entry.id=dependency.projection_id
      WHERE dependency.workspace_id=target_workspace AND dependency.source_material_id=target_material
      AND NOT(entry.space_registration_id=ANY(affected_spaces))) THEN
      RAISE EXCEPTION 'embedding erasure affected space inventory changed' USING ERRCODE='40001';
    END IF;
    affected_spaces:=ARRAY[]::UUID[];
    -- Every space holding a projection computed from this source, locked in a$body$;
 IF strpos(definition,needle)=0 THEN RAISE EXCEPTION 'source erasure inventory predecessor differs'; END IF;
 EXECUTE replace(definition,needle,replacement);

 definition:=pg_get_functiondef('vestrace_finalize_bound_content_material(uuid)'::regprocedure);
 definition:=replace(definition,'BEGIN',$body$BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(i.workspace_id::text || ':' || i.owner_id::text,208))
      FROM material_key_creation_intents i WHERE i.id=target_intent_id AND i.owner_kind='memory_revision'
        AND i.workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::uuid;
$body$);
 needle:='    IF intent_row.owner_kind=''embedding_job_output'' THEN';
 replacement:=$body$
    IF intent_row.owner_kind='memory_revision' THEN
      PERFORM memory.id FROM memory_revisions revision JOIN memories memory
        ON memory.workspace_id=revision.workspace_id AND memory.id=revision.memory_id
        WHERE revision.workspace_id=intent_row.workspace_id AND revision.id=intent_row.owner_id AND memory.status<>'deleted'
        FOR SHARE OF memory,revision;
      IF NOT FOUND OR EXISTS(
        SELECT 1 FROM material_key_creation_intents prior JOIN content_materials material
          ON material.workspace_id=prior.workspace_id AND material.intent_id=prior.id
          WHERE prior.workspace_id=intent_row.workspace_id AND prior.owner_kind='memory_revision'
            AND prior.owner_id=intent_row.owner_id AND prior.id<>intent_row.id
            AND material.state IN ('erasure_prepared','tombstoned')
            AND EXISTS(SELECT 1 FROM content_material_ordinary_references published
              WHERE published.workspace_id=prior.workspace_id AND published.intent_id=prior.id
                AND published.material_id=material.id AND published.owner_kind=prior.owner_kind
                AND published.owner_id=prior.owner_id AND published.output_ordinal=prior.output_ordinal)) THEN
        RAISE EXCEPTION 'memory revision material publication requires an eligible revision' USING ERRCODE='23514';
      END IF;
    END IF;
    IF intent_row.owner_kind='embedding_job_output' THEN$body$;
 IF strpos(definition,needle)=0 THEN RAISE EXCEPTION 'ordinary material finalizer predecessor differs'; END IF;
 EXECUTE replace(definition,needle,replacement);

 FOREACH needle IN ARRAY ARRAY[
 'vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)',
 'vestrace_fence_embedding_job_dispatching()'] LOOP
  definition:=pg_get_functiondef(needle::regprocedure);
  IF strpos(definition,'IF output_position=0 THEN')=0 THEN RAISE EXCEPTION 'query dispatch output predecessor differs'; END IF;
  definition:=replace(definition,'IF output_position=0 THEN',
    $body$IF job_row.kind='retrieval_query' AND (output_position<>0 OR NOT EXISTS(
      SELECT 1 FROM embedding_retrieval_fences f
      JOIN embedding_index_generation_guards g ON g.workspace_id=f.workspace_id AND g.space_registration_id=f.space_registration_id
      JOIN embedding_corpus_generations c ON c.workspace_id=f.workspace_id AND c.id=f.generation_id
      WHERE f.workspace_id=job_row.workspace_id AND f.job_id=job_row.id AND f.space_registration_id=job_row.space_registration_id
        AND f.deadline>NOW() AND g.current_generation_id=f.generation_id AND g.guard_version=f.guard_version
        AND c.state='ready' AND c.generation_epoch=f.generation_epoch AND c.corpus_revision=f.corpus_revision
        AND c.built_through_projection_ordinal=f.built_through_projection_ordinal AND c.member_count=f.member_count)) THEN
      RAISE EXCEPTION 'query dispatch requires its exact live fence and no output material' USING ERRCODE='23514';
    END IF;
    IF job_row.kind<>'retrieval_query' AND output_position=0 THEN$body$);
  EXECUTE definition;
 END LOOP;
END $patch$;


-- Retired generations preserve structural membership after witnessed erasure.
-- Ready/Building members and every member's space identity remain strict.
CREATE OR REPLACE FUNCTION vestrace_validate_canonical_member_liveness()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM embedding_corpus_generation_members member
  JOIN embedding_corpus_generations g ON g.workspace_id=member.workspace_id AND g.id=member.corpus_generation_id
  JOIN embedding_projection_entries p ON p.workspace_id=member.workspace_id AND p.id=member.embedding_projection_entry_id
  LEFT JOIN content_materials m ON m.workspace_id=p.workspace_id AND m.id=p.material_id
  WHERE member.workspace_id=NEW.workspace_id AND g.member_representation='encrypted_projection'
   AND ((TG_TABLE_NAME='content_materials' AND p.material_id=NEW.id) OR (TG_TABLE_NAME='embedding_projection_entries' AND p.id=NEW.id))
   AND (p.space_registration_id IS DISTINCT FROM g.space_registration_id
    OR ((p.state IS DISTINCT FROM 'live' OR m.state IS DISTINCT FROM 'live') AND NOT (
     g.state IN ('stale','revoked') AND p.state='erased' AND p.retention_eligibility_state='erasure_propagated'
     AND m.id IS NOT NULL AND m.state IN ('live','erasure_prepared','tombstoned')
     AND EXISTS(SELECT 1 FROM embedding_index_generation_guards guard
       WHERE guard.workspace_id=g.workspace_id AND guard.space_registration_id=g.space_registration_id
        AND guard.current_generation_id IS DISTINCT FROM g.id)
     AND EXISTS(SELECT 1 FROM embedding_erasure_revoked_members revoked
       JOIN embedding_erasure_propagations propagation ON propagation.workspace_id=revoked.workspace_id AND propagation.id=revoked.propagation_id
       JOIN material_erasure_preparations preparation ON preparation.workspace_id=propagation.workspace_id
        AND preparation.id=propagation.material_erasure_preparation_id AND preparation.target_kind='content'
        AND preparation.content_material_id=propagation.source_material_id
       JOIN content_materials source ON source.workspace_id=propagation.workspace_id AND source.id=propagation.source_material_id
        AND source.state IN ('erasure_prepared','tombstoned')
       JOIN embedding_projection_source_dependencies dependency ON dependency.workspace_id=p.workspace_id AND dependency.projection_id=p.id
        AND dependency.source_material_id=propagation.source_material_id AND dependency.source_intent_id=source.intent_id
       WHERE revoked.workspace_id=p.workspace_id AND revoked.projection_entry_id=p.id
        AND revoked.space_registration_id=p.space_registration_id AND revoked.vector_material_id=p.material_id)
    )))) THEN
  RAISE EXCEPTION 'canonical generation member must remain Live' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END $$;
REVOKE ALL ON FUNCTION vestrace_validate_canonical_member_liveness() FROM PUBLIC,vestrace;

CREATE OR REPLACE FUNCTION vestrace_issue_canonical_retrieval_snapshot(
    target_snapshot_id UUID,
    target_workspace_id UUID,
    target_registration UUID,
    expected_generation UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    registration embedding_space_registrations%ROWTYPE;
    model_row model_revisions%ROWTYPE;
    connection_row connection_revisions%ROWTYPE;
    connection_qualification_row connection_qualification_revisions%ROWTYPE;
    model_qualification_row model_qualification_revisions%ROWTYPE;
    target_binding_row qualification_target_bindings%ROWTYPE;
    credential_slot_row credential_slots%ROWTYPE;
    connection_guard_id UUID;
    activation_guard_id UUID;
BEGIN
    IF target_workspace_id IS NULL OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::uuid THEN
        RAISE EXCEPTION 'canonical retrieval snapshot workspace context is required' USING ERRCODE='42501';
    END IF;
    IF target_snapshot_id IS NULL OR target_registration IS NULL OR expected_generation IS NULL THEN
        RAISE EXCEPTION 'canonical retrieval snapshot arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO registration FROM embedding_space_registrations WHERE workspace_id=target_workspace_id AND id=target_registration;
    IF NOT FOUND OR registration.registration_kind IS DISTINCT FROM 'canonical' THEN
        RAISE EXCEPTION 'canonical retrieval snapshot requires exact canonical registration' USING ERRCODE='23514';
    END IF;
    -- This lookup identifies the connection whose permanent guard must be the
    -- first lock. It is rechecked after the lock, below, before anything is
    -- persisted.
    SELECT revision.* INTO model_row
      FROM model_revision_heads AS model_head
      JOIN model_revisions AS revision
        ON revision.workspace_id = model_head.workspace_id
       AND revision.model_id = model_head.model_id
       AND revision.id = model_head.current_revision_id
     WHERE model_head.workspace_id = target_workspace_id
       AND revision.id = registration.model_revision_id
       AND revision.kind = 'embedding';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'canonical retrieval registration has no current embedding model revision'
            USING ERRCODE = '23514', CONSTRAINT = 'canonical_retrieval_model_not_current_embedding';
    END IF;

    SELECT * INTO connection_row
      FROM connection_revisions
     WHERE workspace_id = target_workspace_id
       AND id = model_row.connection_revision_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding references a missing connection revision'
            USING ERRCODE = '23514';
    END IF;

    -- Canonical lock order: connection execution guard first; credential
    -- activation guard second only for the credential branch; all predicates
    -- follow those locks.
    SELECT id INTO connection_guard_id
      FROM connection_execution_guards
     WHERE id = connection_row.execution_guard_id
       AND workspace_id = target_workspace_id
       AND connection_id = connection_row.connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires the exact connection execution guard'
            USING ERRCODE = '23514';
    END IF;

    -- A head advance may have waited on the guard. Check the exact registered model
    -- again before taking the optional branch lock so acceptance observes a
    -- complete old tuple or a complete new tuple, never their mixture.
    SELECT revision.* INTO model_row
      FROM model_revision_heads AS model_head
      JOIN model_revisions AS revision
        ON revision.workspace_id = model_head.workspace_id
       AND revision.model_id = model_head.model_id
       AND revision.id = model_head.current_revision_id
     WHERE model_head.workspace_id = target_workspace_id
       AND revision.id = registration.model_revision_id
       AND revision.kind = 'embedding';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'canonical retrieval registration has no current embedding model revision'
            USING ERRCODE = '23514', CONSTRAINT = 'canonical_retrieval_model_not_current_embedding';
    END IF;
    SELECT * INTO connection_row
      FROM connection_revisions
     WHERE workspace_id = target_workspace_id
       AND id = model_row.connection_revision_id
       AND execution_guard_id = connection_guard_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'canonical retrieval registration changed outside its locked connection guard'
            USING ERRCODE = '23514';
    END IF;

    IF connection_row.auth_mode <> 'none' THEN
        SELECT id INTO activation_guard_id
          FROM credential_activation_guards
         WHERE workspace_id = target_workspace_id
           AND connection_id = connection_row.connection_id
           AND credential_slot_id = connection_row.credential_slot_id
           AND execution_guard_id = connection_guard_id
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'model binding requires the exact credential activation guard'
                USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
        END IF;
    END IF;

    IF connection_row.auth_mode <> 'none' THEN
        PERFORM 1 FROM credential_slots WHERE workspace_id=target_workspace_id AND id=connection_row.credential_slot_id
            AND connection_id=connection_row.connection_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'canonical retrieval snapshot credential slot is absent' USING ERRCODE='23514'; END IF;
    END IF;
    PERFORM vestrace_assert_canonical_embedding_space(target_workspace_id,target_registration);
    IF NOT EXISTS(SELECT 1 FROM embedding_corpus_generations WHERE workspace_id=target_workspace_id AND id=expected_generation AND space_registration_id=target_registration) THEN
        RAISE EXCEPTION 'canonical retrieval snapshot generation belongs to another space' USING ERRCODE='23514';
    END IF;
    PERFORM * FROM vestrace_resolve_embedding_memory_references(target_workspace_id,expected_generation,ARRAY[]::uuid[]);
    SELECT revision.* INTO connection_row
      FROM connection_revision_heads AS connection_head
      JOIN connection_revisions AS revision
        ON revision.workspace_id = connection_head.workspace_id
       AND revision.connection_id = connection_head.connection_id
       AND revision.id = connection_head.current_revision_id
     WHERE connection_head.workspace_id = target_workspace_id
       AND connection_head.connection_id = connection_row.connection_id
       AND connection_head.current_revision_id = model_row.connection_revision_id
       AND connection_head.state = 'enabled'
       AND revision.execution_guard_id = connection_guard_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires the current enabled connection revision'
            USING ERRCODE = '23514';
    END IF;

    SELECT revision.* INTO model_row
      FROM model_revision_heads AS model_head
      JOIN model_revisions AS revision
        ON revision.workspace_id = model_head.workspace_id
       AND revision.model_id = model_head.model_id
       AND revision.id = model_head.current_revision_id
     WHERE model_head.workspace_id = target_workspace_id
       AND revision.id = registration.model_revision_id
       AND revision.id = model_row.id
       AND revision.connection_revision_id = connection_row.id
       AND revision.kind = 'embedding';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'canonical retrieval requires the current embedding model revision'
            USING ERRCODE = '23514', CONSTRAINT = 'canonical_retrieval_model_not_current_embedding';
    END IF;

    SELECT qualification_revision.* INTO connection_qualification_row
      FROM connection_qualification_heads AS qualification_head
      JOIN connection_qualification_revisions AS qualification_revision
        ON qualification_revision.workspace_id = qualification_head.workspace_id
       AND qualification_revision.connection_revision_id = qualification_head.connection_revision_id
       AND qualification_revision.id = qualification_head.current_qualification_revision_id
     WHERE qualification_head.workspace_id = target_workspace_id
       AND qualification_head.connection_revision_id = connection_row.id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires a current connection qualification'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;
    IF connection_qualification_row.valid_until <= NOW() THEN
        RAISE EXCEPTION 'model binding connection qualification has expired'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_expired';
    END IF;
    IF NOT connection_qualification_row.capabilities @> ARRAY['embeddings']::text[] THEN
        RAISE EXCEPTION 'model binding connection qualification is incompatible'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;

    SELECT qualification_revision.* INTO model_qualification_row
      FROM model_qualification_heads AS qualification_head
      JOIN model_qualification_revisions AS qualification_revision
        ON qualification_revision.workspace_id = qualification_head.workspace_id
       AND qualification_revision.model_revision_id = qualification_head.model_revision_id
       AND qualification_revision.id = qualification_head.current_qualification_revision_id
     WHERE qualification_head.workspace_id = target_workspace_id
       AND qualification_head.model_revision_id = model_row.id
       AND qualification_revision.id = registration.model_qualification_revision_id
       AND qualification_revision.connection_revision_id = connection_row.id
       AND qualification_revision.connection_qualification_revision_id = connection_qualification_row.id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires a current compatible model qualification'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;
    IF model_qualification_row.valid_until <= NOW() THEN
        RAISE EXCEPTION 'model binding model qualification has expired'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_expired';
    END IF;
    IF NOT model_qualification_row.capabilities @> ARRAY['embeddings']::text[] THEN
        RAISE EXCEPTION 'model binding model qualification is incompatible'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;

    SELECT binding.* INTO target_binding_row
      FROM qualification_target_bindings AS binding
     WHERE binding.workspace_id = target_workspace_id
       AND binding.qualification_job_id = connection_qualification_row.qualification_job_id
       AND binding.connection_id = connection_row.connection_id
       AND binding.connection_revision_id = connection_row.id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires an exact qualification target branch'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
    END IF;

    IF connection_row.auth_mode = 'none' THEN
        IF target_binding_row.branch <> 'no_auth'
           OR target_binding_row.credential_revision_id IS NOT NULL
           OR target_binding_row.credential_slot_id IS NOT NULL
           OR target_binding_row.credential_activation_guard_id IS NOT NULL
           OR target_binding_row.expected_slot_version IS NOT NULL
           OR target_binding_row.no_auth_binding_revision_id IS NULL
           OR NOT EXISTS (
                SELECT 1 FROM no_auth_binding_revisions AS no_auth
                 WHERE no_auth.workspace_id = target_workspace_id
                   AND no_auth.connection_id = connection_row.connection_id
                   AND no_auth.connection_revision_id = connection_row.id
                   AND no_auth.id = target_binding_row.no_auth_binding_revision_id
           ) THEN
            RAISE EXCEPTION 'no-auth model binding must carry no credential reference'
                USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
        END IF;

        INSERT INTO model_binding_snapshots (
            id, workspace_id, connection_id, connection_revision_id,
            connection_qualification_revision_id, model_revision_id,
            model_qualification_revision_id, branch, no_auth_binding_revision_id
        ) VALUES (
            target_snapshot_id, target_workspace_id, connection_row.connection_id,
            connection_row.id, connection_qualification_row.id, model_row.id,
            model_qualification_row.id, 'no_auth', target_binding_row.no_auth_binding_revision_id
        );
    ELSE
        SELECT * INTO credential_slot_row
          FROM credential_slots
         WHERE workspace_id = target_workspace_id
           AND connection_id = connection_row.connection_id
           AND id = connection_row.credential_slot_id;
        IF NOT FOUND
           OR credential_slot_row.current_revision_id IS NULL
           OR credential_slot_row.tombstone_version IS NOT NULL
           OR target_binding_row.branch <> 'credential'
           OR target_binding_row.credential_revision_id IS DISTINCT FROM credential_slot_row.current_revision_id
           OR target_binding_row.credential_slot_id IS DISTINCT FROM credential_slot_row.id
           OR target_binding_row.credential_activation_guard_id IS DISTINCT FROM activation_guard_id
           OR target_binding_row.expected_slot_version IS DISTINCT FROM credential_slot_row.current_revision_version
           OR target_binding_row.no_auth_binding_revision_id IS NOT NULL
           OR NOT EXISTS (
                SELECT 1 FROM credential_key_creation_intents AS intent
                 WHERE intent.workspace_id = target_workspace_id
                   AND intent.connection_id = connection_row.connection_id
                   AND intent.credential_slot_id = credential_slot_row.id
                   AND intent.credential_revision_id = credential_slot_row.current_revision_id
                   AND intent.state = 'active'
           ) THEN
            RAISE EXCEPTION 'credential model binding requires the exact active guarded credential'
                USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
        END IF;

        INSERT INTO model_binding_snapshots (
            id, workspace_id, connection_id, connection_revision_id,
            connection_qualification_revision_id, model_revision_id,
            model_qualification_revision_id, branch, credential_revision_id,
            credential_slot_id, credential_activation_guard_id, expected_slot_version
        ) VALUES (
            target_snapshot_id, target_workspace_id, connection_row.connection_id,
            connection_row.id, connection_qualification_row.id, model_row.id,
            model_qualification_row.id, 'credential', credential_slot_row.current_revision_id,
            credential_slot_row.id, activation_guard_id,
            credential_slot_row.current_revision_version
        );
    END IF;

    INSERT INTO model_binding_snapshot_scopes (
        workspace_id, snapshot_id, scope, transition_plan_id
    ) VALUES (
        target_workspace_id, target_snapshot_id, 'ordinary', NULL
    );

    RETURN target_snapshot_id;
END
$$;

DO $finish$
DECLARE item REGPROCEDURE;
BEGIN
 IF to_regprocedure('public.vestrace_finish_embedding_memory_references_upgrade()') IS NOT NULL THEN
  PERFORM vestrace_finish_embedding_memory_references_upgrade();
 ELSE
  IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
   RAISE EXCEPTION 'embedding memory references ownership hand-back unavailable' USING ERRCODE='42501'; END IF;
  GRANT SELECT, UPDATE ON memories,memory_revisions TO vestrace_guarded_owner;
  FOREACH item IN ARRAY ARRAY[
   'vestrace_issue_canonical_retrieval_snapshot(uuid,uuid,uuid,uuid)'::regprocedure,
   'vestrace_resolve_embedding_memory_references(uuid,uuid,uuid[])'::regprocedure,
   'vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,uuid,timestamptz)'::regprocedure,
   'vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],uuid[],uuid[],bigint[],double precision[])'::regprocedure
  ] LOOP
   EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',item);
   EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',item);
   EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',item);
  END LOOP;
  REVOKE ALL ON FUNCTION vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz),
   vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[]) FROM PUBLIC,vestrace;
 END IF;
END $finish$;

