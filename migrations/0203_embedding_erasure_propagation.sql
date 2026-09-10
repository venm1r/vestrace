-- P04 Task 9: propagate source and vector erasure through embedding generations.
--
-- Erasing a source material is not a local act. Every canonical projection
-- computed from it is a vector of content that is about to stop existing, and
-- every generation holding such a projection would keep answering queries from
-- it. So one guarded transaction makes the dependents unavailable, advances the
-- affected corpora, revokes their current generations, stales any transition
-- built on the affected recipes, and only then lets the ordinary two-phase
-- material erasure begin.
--
-- The ordering is forced, not chosen. `vestrace_prepare_content_material_erasure`
-- refuses while a nonterminal blocker exists, and migration 0193 records one
-- blocker per embedding delivery source. Those blockers are exactly what stops
-- a source from being erased out from under a live corpus, so they are
-- terminalized here -- after the corpus they protect has been revoked, never
-- before.
--
-- Nothing here deletes ciphertext or touches a vault. That is phase two, which
-- the existing authority already owns; this migration only makes phase one
-- lawful for a source an embedding corpus depends on.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_erasure_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_erasure_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding erasure propagation upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

-- One record per source material whose erasure was propagated.
--
-- It exists to make the propagation auditable after the fact: which corpora were
-- advanced, how many generations were revoked, how many transitions went stale.
-- The counts are recorded rather than recomputed, because after phase two the
-- projections they counted are gone.
CREATE TABLE embedding_erasure_propagations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    source_material_id UUID NOT NULL,
    material_erasure_preparation_id UUID NOT NULL,
    dependent_projection_count BIGINT NOT NULL CHECK (dependent_projection_count >= 0),
    revoked_generation_count BIGINT NOT NULL CHECK (revoked_generation_count >= 0),
    staled_transition_count BIGINT NOT NULL CHECK (staled_transition_count >= 0),
    propagated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, id),
    CONSTRAINT embedding_erasure_propagations_one_per_source
        UNIQUE (workspace_id, source_material_id),
    CONSTRAINT embedding_erasure_propagations_one_per_preparation
        UNIQUE (workspace_id, material_erasure_preparation_id),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE RESTRICT,
    FOREIGN KEY (source_material_id, workspace_id)
        REFERENCES content_materials(id, workspace_id) ON DELETE RESTRICT
);

-- Which projections a propagation made unavailable, and which generation each
-- was in. Kept because after phase two neither can be reconstructed, and an
-- operator asking "what did erasing this source remove from the corpus" has
-- nothing else to read.
CREATE TABLE embedding_erasure_revoked_members (
    workspace_id UUID NOT NULL,
    propagation_id UUID NOT NULL,
    projection_entry_id UUID NOT NULL,
    space_registration_id UUID NOT NULL,
    -- Null when no generation had yet captured the projection. Membership is a
    -- later fact than the projection itself, and what leaves the corpus is
    -- decided by dependence on the erased source, not by whether a generation
    -- happened to hold it first.
    corpus_generation_id UUID,
    PRIMARY KEY (workspace_id, propagation_id, projection_entry_id),
    -- Deferred because the two rows are one fact written in one transaction:
    -- what left the corpus is discovered while the propagation is being
    -- computed, and the propagation row is only complete once the material
    -- erasure preparation it delegates to exists.
    FOREIGN KEY (workspace_id, propagation_id)
        REFERENCES embedding_erasure_propagations(workspace_id, id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

-- Phase one for a source an embedding corpus depends on.
--
-- Returns the material erasure preparation the ordinary two-phase authority
-- continues from. Every mutation below happens in this one transaction or none
-- of them does: a corpus advanced without its generation revoked would keep
-- answering from vectors of content that is about to stop existing, and a
-- blocker terminalized without the corpus advanced would let phase two delete
-- ciphertext a live generation still points at.
CREATE OR REPLACE FUNCTION vestrace_propagate_embedding_source_erasure(
    target_id UUID,
    target_workspace UUID,
    target_material UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing embedding_erasure_propagations%ROWTYPE;
    material content_materials%ROWTYPE;
    dependent RECORD;
    space RECORD;
    corpus embedding_space_corpus_states%ROWTYPE;
    guard embedding_index_generation_guards%ROWTYPE;
    generation embedding_corpus_generations%ROWTYPE;
    preparation UUID;
    affected_spaces UUID[] := ARRAY[]::UUID[];
    dependent_count BIGINT := 0;
    revoked_count BIGINT := 0;
    staled_count BIGINT := 0;
    remaining BIGINT;
BEGIN
    IF target_id IS NULL OR target_workspace IS NULL OR target_material IS NULL THEN
        RAISE EXCEPTION 'embedding erasure propagation arguments are malformed'
            USING ERRCODE='22023';
    END IF;

    -- A replay returns its one prior preparation. Propagating twice would
    -- advance the corpus a second time for a source already gone from it.
    SELECT * INTO existing FROM embedding_erasure_propagations
     WHERE workspace_id=target_workspace AND source_material_id=target_material
     FOR UPDATE;
    IF FOUND THEN
        RETURN existing.material_erasure_preparation_id;
    END IF;

    SELECT * INTO material FROM content_materials
     WHERE workspace_id=target_workspace AND id=target_material FOR UPDATE;
    IF NOT FOUND OR material.state <> 'live' THEN
        RAISE EXCEPTION 'embedding erasure propagation requires an exact Live source material'
            USING ERRCODE='23514';
    END IF;

    -- Every space holding a projection computed from this source, locked in a
    -- stable order so two concurrent erasures of different sources touching the
    -- same space cannot deadlock.
    FOR space IN
        SELECT DISTINCT entry.space_registration_id AS registration
          FROM embedding_projection_source_dependencies AS dependency
          JOIN embedding_projection_entries AS entry
            ON entry.workspace_id = dependency.workspace_id
           AND entry.id = dependency.projection_id
         WHERE dependency.workspace_id = target_workspace
           AND dependency.source_material_id = target_material
         ORDER BY 1
    LOOP
        SELECT * INTO corpus FROM embedding_space_corpus_states
         WHERE workspace_id=target_workspace AND space_registration_id=space.registration
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding erasure propagation requires the corpus state of every affected space'
                USING ERRCODE='23514';
        END IF;
        SELECT * INTO guard FROM embedding_index_generation_guards
         WHERE workspace_id=target_workspace AND space_registration_id=space.registration
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding erasure propagation requires the generation guard of every affected space'
                USING ERRCODE='23514';
        END IF;

        -- Every projection in this space computed from the erased source, with
        -- the generation holding it when one already does. Recorded before the
        -- revocation so a later reader can still see what left the corpus.
        FOR dependent IN
            SELECT entry.id AS projection_id,
                   (SELECT member.corpus_generation_id
                      FROM embedding_corpus_generation_members AS member
                      JOIN embedding_corpus_generations AS held
                        ON held.workspace_id = member.workspace_id
                       AND held.id = member.corpus_generation_id
                     WHERE member.workspace_id = entry.workspace_id
                       AND member.embedding_projection_entry_id = entry.id
                       AND held.space_registration_id = space.registration
                       AND held.state IN ('building','ready')
                     LIMIT 1) AS generation_id
              FROM embedding_projection_entries AS entry
              JOIN embedding_projection_source_dependencies AS dependency
                ON dependency.workspace_id = entry.workspace_id
               AND dependency.projection_id = entry.id
             WHERE entry.workspace_id = target_workspace
               AND entry.space_registration_id = space.registration
               AND dependency.source_material_id = target_material
             GROUP BY entry.id, entry.workspace_id
        LOOP
            INSERT INTO embedding_erasure_revoked_members(
                workspace_id, propagation_id, projection_entry_id,
                space_registration_id, corpus_generation_id
            ) VALUES (
                target_workspace, target_id, dependent.projection_id,
                space.registration, dependent.generation_id
            ) ON CONFLICT DO NOTHING;
            dependent_count := dependent_count + 1;
        END LOOP;
        affected_spaces := affected_spaces || space.registration;

        FOR generation IN
            SELECT * FROM embedding_corpus_generations
             WHERE workspace_id=target_workspace
               AND space_registration_id=space.registration
               AND state IN ('building','ready')
             ORDER BY id
             FOR UPDATE
        LOOP
            IF EXISTS (
                SELECT 1 FROM embedding_corpus_generation_members AS member
                  JOIN embedding_projection_source_dependencies AS dependency
                    ON dependency.workspace_id = member.workspace_id
                   AND dependency.projection_id = member.embedding_projection_entry_id
                 WHERE member.workspace_id = target_workspace
                   AND member.corpus_generation_id = generation.id
                   AND dependency.source_material_id = target_material
            ) THEN
                UPDATE embedding_corpus_generations SET state='revoked'
                 WHERE workspace_id=target_workspace AND id=generation.id;
                revoked_count := revoked_count + 1;
                -- A revoked generation is no longer current. Leaving the guard
                -- pointing at it would let retrieval acceptance pin a
                -- generation whose members are being erased.
                IF guard.current_generation_id = generation.id THEN
                    UPDATE embedding_index_generation_guards
                       SET current_generation_id=NULL
                     WHERE workspace_id=target_workspace
                       AND space_registration_id=space.registration;
                END IF;
            END IF;
        END LOOP;

        -- The corpus revision advances once per affected space, and its live
        -- member count drops by what this source contributed. A later build
        -- may only draw from what remains.
        SELECT count(*) INTO remaining
          FROM embedding_projection_entries AS entry
         WHERE entry.workspace_id=target_workspace
           AND entry.space_registration_id=space.registration
           AND NOT EXISTS (
               SELECT 1 FROM embedding_projection_source_dependencies AS dependency
                WHERE dependency.workspace_id=entry.workspace_id
                  AND dependency.projection_id=entry.id
                  AND dependency.source_material_id=target_material);
        UPDATE embedding_space_corpus_states
           SET corpus_revision=corpus_revision+1,
               live_member_count=LEAST(live_member_count, remaining)
         WHERE workspace_id=target_workspace AND space_registration_id=space.registration;
        -- The epoch advances once per affected space, whether or not a
        -- generation was current. Every corpus-change event on this stream
        -- carries exactly one epoch step, and this corpus changed.
        UPDATE embedding_index_generation_guards
           SET generation_epoch=generation_epoch+1,
               guard_version=guard_version+1
         WHERE workspace_id=target_workspace AND space_registration_id=space.registration;
    END LOOP;

    -- A transition whose recipes were computed from this source can no longer
    -- prove its bijection, so it is stale rather than merely blocked.
    WITH affected AS (
        UPDATE embedding_transitions AS transition
           SET state='stale', version=version+1
         WHERE transition.workspace_id=target_workspace
           AND transition.state IN ('planned','rebuilding','ready_to_activate')
           AND EXISTS (
               SELECT 1
                 FROM embedding_transition_plans AS plan
                 JOIN embedding_projection_entries AS entry
                   ON entry.workspace_id = plan.workspace_id
                  AND entry.space_registration_id = plan.target_space_registration_id
                 JOIN embedding_projection_source_dependencies AS dependency
                   ON dependency.workspace_id = entry.workspace_id
                  AND dependency.projection_id = entry.id
                WHERE plan.workspace_id = transition.workspace_id
                  AND plan.transition_id = transition.id
                  AND dependency.source_material_id = target_material)
        RETURNING 1
    )
    SELECT count(*) INTO staled_count FROM affected;

    -- Only now may the blockers go terminal. They exist to stop exactly this
    -- source being erased out from under a live corpus, and that corpus has
    -- just been revoked.
    UPDATE material_erasure_blockers
       SET state='terminal'
     WHERE workspace_id=target_workspace
       AND target_kind='content'
       AND content_material_id=target_material
       AND state='nonterminal'
       AND id IN (SELECT blocker_id FROM embedding_delivery_source_memberships
                   WHERE workspace_id=target_workspace
                     AND source_material_id=target_material);

    preparation := (SELECT p.preparation_id
                      FROM vestrace_prepare_content_material_erasure(target_material) AS p);
    IF preparation IS NULL THEN
        RAISE EXCEPTION 'embedding erasure propagation could not prepare its source'
            USING ERRCODE='23514';
    END IF;

    INSERT INTO embedding_erasure_propagations(
        id, workspace_id, source_material_id, material_erasure_preparation_id,
        dependent_projection_count, revoked_generation_count, staled_transition_count
    ) VALUES (
        target_id, target_workspace, target_material, preparation,
        dependent_count, revoked_count, staled_count
    );

    -- One invalidation event per affected space, on the append-only corpus
    -- change stream every index worker already consumes. Driven by the affected
    -- space rather than by revoked membership: a space whose projections were
    -- never captured into a generation is still a corpus that just changed.
    -- Written last, so no consumer can observe an invalidation for a corpus this
    -- transaction has not finished advancing.
    INSERT INTO embedding_index_rebuild_events(
        id, workspace_id, space_registration_id,
        before_corpus_revision, after_corpus_revision,
        before_generation_epoch, after_generation_epoch,
        before_live_member_count, after_live_member_count,
        built_through_projection_ordinal,
        invalidated_generation_ids, cause, material_erasure_preparation_id
    )
    SELECT gen_random_uuid(), target_workspace, corpus_now.space_registration_id,
           corpus_now.corpus_revision-1, corpus_now.corpus_revision,
           guard_now.generation_epoch-1, guard_now.generation_epoch,
           corpus_now.live_member_count, corpus_now.live_member_count,
           -- An erasure removes rather than builds, so it carries the highest
           -- ordinal the corpus has reached: the point in the projection
           -- sequence at which this change was recorded.
           GREATEST(corpus_now.next_projection_ordinal - 1, 1),
           coalesce(
               (SELECT array_agg(DISTINCT revoked.corpus_generation_id)
                  FROM embedding_erasure_revoked_members AS revoked
                 WHERE revoked.workspace_id = target_workspace
                   AND revoked.propagation_id = target_id
                   AND revoked.space_registration_id = corpus_now.space_registration_id
                   AND revoked.corpus_generation_id IS NOT NULL),
               ARRAY[]::UUID[]),
           'material_erasure', preparation
      FROM embedding_space_corpus_states AS corpus_now
      JOIN embedding_index_generation_guards AS guard_now
        ON guard_now.workspace_id = corpus_now.workspace_id
       AND guard_now.space_registration_id = corpus_now.space_registration_id
     WHERE corpus_now.workspace_id = target_workspace
       AND corpus_now.space_registration_id = ANY(affected_spaces);

    RETURN preparation;
END
$$;

-- Forward-replace the 0195 publication validator for the post-erasure branch.
--
-- The validator requires a space's live member count to equal the sum of every
-- publication's output count. That is true until a source is erased: erasure
-- lawfully removes projections from the corpus while the historical
-- publications that produced them stay exactly as they were, because a
-- publication is a record of what happened and erasure does not unmake it.
--
-- So a historical publication becomes valid in exactly one of two branches:
-- its ciphertext is still Live, or the difference is accounted for by an exact
-- erasure lineage. The subtrahend is the distinct projections this space lost
-- to propagated erasures, which `embedding_erasure_revoked_members` records for
-- precisely this purpose.
--
-- Unlike migration 0199's substitutions, this one proves it happened. A lexical
-- replacement that silently matched nothing would reinstall the unchanged
-- function and leave the validator refusing every post-erasure publication,
-- which is a failure that only appears the first time an operator erases
-- something.
-- The corpus-change stream required every event to grow the corpus. That is the
-- shape of a result publication, generalised into a table-level rule before any
-- cause could shrink one. Erasure removes members, so the rule becomes
-- cause-specific: a publication still has to grow, an erasure may only shrink
-- or hold, and the causes with no settled rule keep none rather than inheriting
-- one that was never about them.
--
-- Found by name lookup rather than by the generated name, which is positional
-- and would silently drop a different constraint if the table ever changes.
-- Migration 0198 left every corpus-change cause but `result_publication`
-- refused, with the note that each one's authority arrives in the task that
-- owns it. This is that task for `material_erasure`, so the cause is admitted
-- here -- and admitted against its authority, not against its spelling: the
-- event must name a propagation this workspace actually recorded, for the same
-- space. Supplying a UUID still enables nothing.
DO $admit_erasure_cause$
DECLARE definition TEXT;
    needle CONSTANT TEXT := 'IF NEW.cause<>''result_publication'' THEN';
    replacement CONSTANT TEXT :=
        'IF NEW.cause=''material_erasure'' THEN
        IF NOT EXISTS(SELECT 1 FROM embedding_erasure_propagations propagation
                       WHERE propagation.workspace_id=NEW.workspace_id
                         AND propagation.material_erasure_preparation_id=NEW.material_erasure_preparation_id)
           OR NOT EXISTS(SELECT 1 FROM embedding_erasure_revoked_members revoked
                          JOIN embedding_erasure_propagations propagation
                            ON propagation.workspace_id=revoked.workspace_id
                           AND propagation.id=revoked.propagation_id
                         WHERE revoked.workspace_id=NEW.workspace_id
                           AND revoked.space_registration_id=NEW.space_registration_id
                           AND propagation.material_erasure_preparation_id=NEW.material_erasure_preparation_id) THEN
            RAISE EXCEPTION ''embedding index erasure cause requires its exact propagation'' USING ERRCODE=''23514'';
        END IF;
        RETURN NULL;
    END IF;
    IF NEW.cause<>''result_publication'' THEN';
BEGIN
    definition := pg_get_functiondef(
        'public.vestrace_validate_embedding_index_event_cause()'::REGPROCEDURE);
    IF position(needle IN definition) = 0 THEN
        RAISE EXCEPTION 'the 0198 corpus-change cause guard was not found; erasure events would be refused'
            USING ERRCODE='23514';
    END IF;
    definition := replace(definition, needle, replacement);
    EXECUTE definition;
    IF position('embedding index erasure cause requires its exact propagation' IN
        pg_get_functiondef('public.vestrace_validate_embedding_index_event_cause()'::REGPROCEDURE)) = 0
    THEN
        RAISE EXCEPTION 'the installed corpus-change cause guard does not admit erasure'
            USING ERRCODE='23514';
    END IF;
END
$admit_erasure_cause$;

DO $cause_specific_counts$
DECLARE growth_constraint TEXT;
BEGIN
    SELECT conname INTO growth_constraint
      FROM pg_constraint
     WHERE conrelid='embedding_index_rebuild_events'::REGCLASS AND contype='c'
       AND pg_get_constraintdef(oid) = 'CHECK ((after_live_member_count > before_live_member_count))';
    IF growth_constraint IS NULL THEN
        RAISE EXCEPTION 'the corpus-change growth rule was not found; erasure events would be refused'
            USING ERRCODE='23514';
    END IF;
    EXECUTE format('ALTER TABLE embedding_index_rebuild_events DROP CONSTRAINT %I',
                   growth_constraint);
    ALTER TABLE embedding_index_rebuild_events
        ADD CONSTRAINT embedding_index_rebuild_events_cause_member_counts CHECK (
            (cause = 'result_publication' AND after_live_member_count > before_live_member_count)
            OR (cause = 'material_erasure' AND after_live_member_count <= before_live_member_count)
            OR cause IN ('transition_publication','legacy_cutover','operator_rebuild'));
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid='embedding_index_rebuild_events'::REGCLASS
           AND conname='embedding_index_rebuild_events_cause_member_counts'
    ) THEN
        RAISE EXCEPTION 'the cause-specific corpus-change rule was not installed'
            USING ERRCODE='23514';
    END IF;
END
$cause_specific_counts$;

DO $forward_replace$
DECLARE
    definition TEXT;
    pair RECORD;
    erased CONSTANT TEXT :=
        '(SELECT count(DISTINCT projection_entry_id) FROM embedding_erasure_revoked_members WHERE workspace_id=c.workspace_id AND space_registration_id=c.space_registration_id)';
BEGIN
    definition := pg_get_functiondef(
        'public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)'::REGPROCEDURE);
    -- Both live-count predicates, and both for the same reason: erasure removes
    -- projections from a corpus while the publications that produced them stay
    -- exactly as they were, because a publication records what happened and
    -- erasure does not unmake it.
    FOR pair IN
        SELECT * FROM (VALUES
            ('AND c.live_member_count>=publication.live_member_count',
             'AND c.live_member_count>=publication.live_member_count-' || erased),
            ('AND c.live_member_count=(SELECT sum(output_count) FROM embedding_job_result_publications WHERE workspace_id=c.workspace_id AND space_registration_id=c.space_registration_id)',
             'AND c.live_member_count=(SELECT coalesce(sum(output_count),0) FROM embedding_job_result_publications WHERE workspace_id=c.workspace_id AND space_registration_id=c.space_registration_id)-' || erased)
        ) AS pairs(needle, replacement)
    LOOP
        IF position(pair.needle IN definition) = 0 THEN
            RAISE EXCEPTION
                'the 0195 publication predicate % was not found; erasure propagation would leave every post-erasure publication refused',
                left(pair.needle, 48) USING ERRCODE='23514';
        END IF;
        definition := replace(definition, pair.needle, pair.replacement);
        IF position(pair.replacement IN definition) = 0 THEN
            RAISE EXCEPTION 'the 0195 publication predicate % was not replaced',
                left(pair.needle, 48) USING ERRCODE='23514';
        END IF;
    END LOOP;
    EXECUTE definition;
    -- Read back from the catalogue rather than trusting the string: EXECUTE
    -- could install a function that parses and still lacks the new branch.
    definition := pg_get_functiondef(
        'public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)'::REGPROCEDURE);
    IF position(erased IN definition) = 0 THEN
        RAISE EXCEPTION 'the installed publication validator does not carry the erasure branch'
            USING ERRCODE='23514';
    END IF;
END
$forward_replace$;

DO $rls$
DECLARE target REGCLASS;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'embedding_erasure_propagations'::REGCLASS,
        'embedding_erasure_revoked_members'::REGCLASS
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

DO $handback$
DECLARE target REGCLASS; target_function REGPROCEDURE;
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_erasure_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_erasure_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
            RAISE EXCEPTION 'embedding erasure ownership hand-back is unavailable'
                USING ERRCODE='42501';
        END IF;
        FOREACH target IN ARRAY ARRAY[
            'embedding_erasure_propagations'::REGCLASS,
            'embedding_erasure_revoked_members'::REGCLASS
        ] LOOP
            EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner', target);
            EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner', target);
            EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace', target);
            EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace', target);
        END LOOP;
        FOREACH target_function IN ARRAY ARRAY[
            'vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)'::REGPROCEDURE
        ] LOOP
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function);
            EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace', target_function);
            EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace', target_function);
        END LOOP;
    END IF;
END
$handback$;
