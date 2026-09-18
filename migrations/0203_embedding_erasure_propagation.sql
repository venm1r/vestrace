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
-- lawful for a source an embedding corpus depends on -- and for every vector
-- computed from it, because a vector of erased content is that content in
-- another representation. Each dependent projection is retired and its own
-- ciphertext prepared for erasure, so the caller that finishes phase one for
-- the source finishes it for the vectors too.

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
    -- The projection's own ciphertext. A vector computed from erased content is
    -- that content in another representation, so erasing the source and keeping
    -- the embedding would destroy nothing that mattered. Recorded because after
    -- the projection is retired this row is the only way back to the material.
    vector_material_id UUID NOT NULL,
    PRIMARY KEY (workspace_id, propagation_id, projection_entry_id),
    -- Deferred because the two rows are one fact written in one transaction:
    -- what left the corpus is discovered while the propagation is being
    -- computed, and the propagation row is only complete once the material
    -- erasure preparation it delegates to exists.
    FOREIGN KEY (workspace_id, propagation_id)
        REFERENCES embedding_erasure_propagations(workspace_id, id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

-- A projection may now leave the corpus.
--
-- 0195 gave the entry two phases and no third: finalizing, then live. There was
-- no way to say "this vector is gone", which is why erasing a source could
-- decrement a corpus counter and leave the projections it counted still
-- drawable -- a space in that state can never capture another generation,
-- because capture refuses unless the counter equals what it can draw.
--
-- The retired phase keeps the output commitment. What the projection was is
-- history and history is not erased; what stops existing is the ciphertext it
-- pointed at.
ALTER TABLE embedding_projection_entries
    DROP CONSTRAINT embedding_projection_publication_phase,
    ADD CONSTRAINT embedding_projection_publication_phase CHECK(
      (state='result_finalizing' AND retention_eligibility_state='blocked_result_finalizing'
        AND output_commitment IS NULL)
      OR (state='live' AND retention_eligibility_state='blocked_pending_erasure_propagation'
        AND octet_length(output_commitment)=32 AND output_commitment IS NOT NULL)
      OR (state='erased' AND retention_eligibility_state='erasure_propagated'
        AND octet_length(output_commitment)=32 AND output_commitment IS NOT NULL));

-- Forward-replace 0195's projection guard to admit exactly that transition.
--
-- The guard permits one update and refuses every other: the publication
-- promotion. It has to keep refusing every other, so the retirement is added as
-- a second exact branch rather than by loosening the first, and it is admitted
-- against its authority: the row must already be recorded as revoked by a
-- propagation in this workspace, for this space. Setting the state alone
-- enables nothing.
DO $forward_replace_projection_guard$
DECLARE
    original TEXT;
    replaced TEXT;
    needle CONSTANT TEXT := E'BEGIN\n IF TG_OP<>''UPDATE'' OR OLD.state<>''result_finalizing''';
    replacement CONSTANT TEXT := E'BEGIN\n'
        ' IF TG_OP=''UPDATE'' AND OLD.state=''live'' AND NEW.state=''erased'' THEN\n'
        '  IF NEW.retention_eligibility_state<>''erasure_propagated''\n'
        '    OR (to_jsonb(OLD)-''state''-''retention_eligibility_state'')'
        ' IS DISTINCT FROM (to_jsonb(NEW)-''state''-''retention_eligibility_state'')\n'
        '    OR NOT EXISTS(SELECT 1 FROM embedding_erasure_revoked_members revoked\n'
        '                   WHERE revoked.workspace_id=OLD.workspace_id\n'
        '                     AND revoked.projection_entry_id=OLD.id\n'
        '                     AND revoked.space_registration_id=OLD.space_registration_id) THEN\n'
        '   RAISE EXCEPTION ''embedding projection retirement requires its exact erasure propagation'''
        ' USING ERRCODE=''23514'';\n'
        '  END IF;\n'
        '  RETURN NEW;\n'
        ' END IF;\n'
        ' IF TG_OP<>''UPDATE'' OR OLD.state<>''result_finalizing''';
BEGIN
    original := pg_get_functiondef(
        'public.vestrace_guard_embedding_projection_publication()'::REGPROCEDURE);
    IF position(needle IN original) = 0 THEN
        RAISE EXCEPTION
            'the 0195 projection publication guard was not found; erasure propagation could not '
            'retire a projection'
            USING ERRCODE='23514';
    END IF;
    replaced := replace(original, needle, replacement);
    IF replaced = original THEN
        RAISE EXCEPTION 'the 0195 projection publication guard was not replaced'
            USING ERRCODE='23514';
    END IF;
    EXECUTE replaced;
    IF position('erasure_propagated' IN pg_get_functiondef(
        'public.vestrace_guard_embedding_projection_publication()'::REGPROCEDURE)) = 0
       OR position('result_finalizing' IN pg_get_functiondef(
        'public.vestrace_guard_embedding_projection_publication()'::REGPROCEDURE)) = 0
    THEN
        RAISE EXCEPTION
            'the installed projection guard must carry both the publication promotion and the '
            'retirement branch'
            USING ERRCODE='23514';
    END IF;
END
$forward_replace_projection_guard$;

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
                   entry.material_id AS vector_material_id,
                   entry.state AS projection_state,
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
             GROUP BY entry.id, entry.workspace_id, entry.material_id, entry.state
             ORDER BY entry.id
        LOOP
            INSERT INTO embedding_erasure_revoked_members(
                workspace_id, propagation_id, projection_entry_id,
                space_registration_id, corpus_generation_id,
                vector_material_id
            ) VALUES (
                target_workspace, target_id, dependent.projection_id,
                space.registration, dependent.generation_id,
                dependent.vector_material_id
            ) ON CONFLICT DO NOTHING;
            -- Recorded first, because the guard admitting this retirement reads
            -- that record as its authority.
            IF dependent.projection_state = 'live' THEN
                UPDATE embedding_projection_entries
                   SET state='erased', retention_eligibility_state='erasure_propagated'
                 WHERE workspace_id=target_workspace AND id=dependent.projection_id;
            END IF;
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
        --
        -- Counted with the predicate `vestrace_capture_embedding_generation`
        -- selects members by, not with one of this migration's own devising.
        -- Capture refuses unless the counter equals what it can draw, so a
        -- counter computed any other way would silently make the space unable
        -- to build another generation ever again.
        SELECT count(*) INTO remaining
          FROM embedding_projection_entries AS entry
          JOIN content_materials AS vector
            ON vector.workspace_id=entry.workspace_id AND vector.id=entry.material_id
         WHERE entry.workspace_id=target_workspace
           AND entry.space_registration_id=space.registration
           AND entry.state='live' AND vector.state='live';
        UPDATE embedding_space_corpus_states
           SET corpus_revision=corpus_revision+1,
               live_member_count=remaining
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

-- Forward-replace the 0174 content erasure primitive for retired vectors.
--
-- The primitive refuses any material with no ordinary reference. That rule is
-- right for ordinary content -- an unreferenced Live material is a bookkeeping
-- fault, not an erasure target -- but an embedding projection's ciphertext
-- never had one. It is referenced by the projection entry, which is why nothing
-- in the product could erase a vector: the primitive refused every one of them
-- by construction, so erasing a source destroyed the plaintext and left every
-- embedding computed from it intact.
--
-- The reference test is therefore widened by exactly one alternative: a
-- projection entry that has already been retired names it. Already retired, not
-- merely existing -- a live projection is a member of a live corpus, and
-- admitting those would let any caller erase a vector out from under a
-- generation still answering from it. Retirement happens only inside the
-- propagation above, under the locks that revoke the generation first.
--
-- This only widens. No material that could be prepared before can be refused
-- now, because the added disjunct can only make the refusal condition false.
-- Both halves are widened, never one. A prepare that admits a vector while the
-- finalizer still refuses it would strand the material in erasure_prepared with
-- no lawful way forward, which is worse than refusing it outright.
DO $forward_replace_erasure_primitive$
DECLARE
    original TEXT;
    replaced TEXT;
    target REGPROCEDURE;
    needle CONSTANT TEXT :=
        'SELECT 1 FROM content_material_ordinary_references WHERE material_id = material_row.id';
    replacement CONSTANT TEXT :=
        'SELECT 1 FROM content_material_ordinary_references WHERE material_id = material_row.id'
        ' UNION ALL SELECT 1 FROM embedding_projection_entries retired'
        ' WHERE retired.workspace_id = material_row.workspace_id'
        '   AND retired.material_id = material_row.id'
        '   AND retired.state = ''erased''';
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'public.vestrace_prepare_content_material_erasure(uuid)'::REGPROCEDURE,
        'public.vestrace_finalize_content_material_erasure(uuid,uuid)'::REGPROCEDURE
    ] LOOP
        original := pg_get_functiondef(target);
        IF position(needle IN original) = 0 THEN
            RAISE EXCEPTION
                'the 0174 ordinary reference test was not found in %; no embedding vector could '
                'ever be erased', target
                USING ERRCODE='23514';
        END IF;
        replaced := replace(original, needle, replacement);
        IF replaced = original OR position(replacement IN replaced) = 0 THEN
            RAISE EXCEPTION 'the 0174 ordinary reference test was not replaced in %', target
                USING ERRCODE='23514';
        END IF;
        EXECUTE replaced;
        IF position(replacement IN pg_get_functiondef(target)) = 0 THEN
            RAISE EXCEPTION 'the installed % does not admit a retired vector', target
                USING ERRCODE='23514';
        END IF;
    END LOOP;
END
$forward_replace_erasure_primitive$;

-- Forward-replace 0194's projection dependency validator for a retired
-- projection.
--
-- The validator asserts an invariant this migration depends on: a projection
-- may not stand while a source it was computed from has left Live. That is
-- exactly why the propagation retires it -- but the validator never learned
-- there is such a thing as a retired projection, so it refuses the retirement
-- as well as the thing the retirement prevents.
--
-- The source-liveness test is therefore scoped to a live projection. A retired
-- one is the invariant being satisfied, not violated. Every other conjunct --
-- the exact ordered dependency set, one per delivery source -- still applies to
-- a retired projection, because history is not what erasure removes.
DO $forward_replace_dependency_validator$
DECLARE
    original TEXT;
    replaced TEXT;
    needle CONSTANT TEXT :=
        'AND (material.state<>''live'' OR source_intent.state<>''live'')';
    replacement CONSTANT TEXT :=
        'AND (material.state<>''live'' OR source_intent.state<>''live'')'
        ' AND projection.state=''live''';
BEGIN
    original := pg_get_functiondef(
        'public.vestrace_validate_embedding_projection_dependency()'::REGPROCEDURE);
    IF position(needle IN original) = 0 THEN
        RAISE EXCEPTION
            'the 0194 projection source-liveness test was not found; retiring a projection would '
            'be refused by the invariant it satisfies'
            USING ERRCODE='23514';
    END IF;
    replaced := replace(original, needle, replacement);
    IF replaced = original OR position(replacement IN replaced) = 0 THEN
        RAISE EXCEPTION 'the 0194 projection source-liveness test was not replaced'
            USING ERRCODE='23514';
    END IF;
    EXECUTE replaced;
    IF position(replacement IN pg_get_functiondef(
        'public.vestrace_validate_embedding_projection_dependency()'::REGPROCEDURE)) = 0
    THEN
        RAISE EXCEPTION 'the installed dependency validator does not scope liveness to a live '
            'projection'
            USING ERRCODE='23514';
    END IF;
END
$forward_replace_dependency_validator$;

-- Forward-replace 0195's published-output phase for a retired projection.
--
-- The validator requires every published output to still be exactly live: live
-- material, live intent, live projection, retention blocked pending erasure
-- propagation. That is the phase a published output stays in until this
-- propagation runs, and the retention state names what it is waiting for.
--
-- Once the propagation runs, that phase is over for the outputs it retired, and
-- the validator has to say so rather than refuse them. It is not loosened: the
-- retired phase is admitted only when it is exactly the retired phase and only
-- for a projection this workspace recorded as revoked. Every output the
-- propagation did not touch is judged exactly as before.
DO $forward_replace_published_phase$
DECLARE
    original TEXT;
    replaced TEXT;
    retired CONSTANT TEXT :=
        '(EXISTS(SELECT 1 FROM embedding_erasure_revoked_members revoked'
        ' WHERE revoked.workspace_id=target_workspace'
        '   AND revoked.projection_entry_id=output.projection_id)'
        ' AND output.projection_state=''erased'''
        ' AND output.retention_eligibility_state=''erasure_propagated'')';
    live_needle CONSTANT TEXT :=
        'output.attachments<>0 OR output.material_state<>''live'' OR output.intent_state<>''live'' OR output.projection_state<>''live''';
    retention_needle CONSTANT TEXT :=
        'output.retention_eligibility_state<>''blocked_pending_erasure_propagation'' THEN';
    -- Phase two deletes the output's ciphertext, and the identity check
    -- requires it to be there. That requirement is about a published output
    -- that still exists; for a retired one the bytes being gone is the point.
    bytes_needle CONSTANT TEXT :=
        'NOT EXISTS(SELECT 1 FROM content_material_bytes WHERE workspace_id=target_workspace AND intent_id=output.intent_id)';
BEGIN
    original := pg_get_functiondef(
        'public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)'::REGPROCEDURE);
    IF position(live_needle IN original) = 0 OR position(retention_needle IN original) = 0
       OR position(bytes_needle IN original) = 0 THEN
        RAISE EXCEPTION
            'the 0195 published output phase was not found; a retired projection would refuse '
            'every later publication check'
            USING ERRCODE='23514';
    END IF;
    replaced := replace(original, live_needle,
        '((' || live_needle || ') AND NOT ' || retired || ')');
    replaced := replace(replaced, retention_needle,
        '(output.retention_eligibility_state<>''blocked_pending_erasure_propagation'''
        ' AND NOT ' || retired || ') THEN');
    replaced := replace(replaced, bytes_needle,
        '(' || bytes_needle || ' AND NOT ' || retired || ')');
    IF replaced = original OR position(retired IN replaced) = 0 THEN
        RAISE EXCEPTION 'the 0195 published output phase was not replaced'
            USING ERRCODE='23514';
    END IF;
    EXECUTE replaced;
    IF position(retired IN pg_get_functiondef(
        'public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)'::REGPROCEDURE)) = 0
    THEN
        RAISE EXCEPTION 'the installed publication validator does not admit a retired projection'
            USING ERRCODE='23514';
    END IF;
END
$forward_replace_published_phase$;

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
