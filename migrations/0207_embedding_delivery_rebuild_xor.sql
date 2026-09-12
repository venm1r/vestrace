-- P04 completion: close the delivery/rebuild XOR at both of its ends.
--
-- Task 5 was asked for "a closed delivery/rebuild acceptance XOR" and produced
-- an open one. 0199 lines 138-158 widened sixteen authorities by blanket
-- lexical substitution, turning every `kind='delivery'` into
-- `kind IN ('delivery','rebuild')` and every `kind<>'delivery'` into
-- `kind NOT IN ('delivery','rebuild')`. That is the right rewrite for a check
-- whose subject is "is this one of the two kinds this path serves" and the
-- wrong one for a check whose subject is "is this the kind THIS path serves".
-- The substitution could not tell them apart, and neither reviewer nor test
-- separated them afterwards.
--
-- Two holes came out of it, one at each end of the XOR:
--
--   1. `vestrace_begin_delivery_embedding_outputs` validates that the declared
--      kind is one of the two legal words and nothing else. The declared word
--      then becomes the job: the receipt carries it, and
--      `vestrace_finalize_delivery_embedding_outputs` passes it straight into
--      `vestrace_accept_embedding_job`, which writes it to
--      `embedding_jobs.kind`. So a caller could mint a `rebuild` out of
--      nothing at all.
--
--   2. `vestrace_create_embedding_transition_batch_attempt` (0200 line 754)
--      reads `job_row.kind NOT IN ('delivery','rebuild')`, which refuses a
--      third word and admits both of the two. A transition attempt is the act
--      of declaring "this physical job is the rebuild that answers that
--      recipe", so admitting a delivery lets an ordinary memory write satisfy
--      a transition recipe -- and a satisfied recipe is what activation moves
--      the corpus head on.
--
-- The fact that separates the two kinds already exists and is already relied
-- on: the one production rebuild path, legacy adoption, refuses a target space
-- that has no transition plan naming it
-- (`PgGovernedEmbeddingJobFactory::canonical_binding`, and the suite's
-- `a_target_space_without_a_transition_binding_is_refused`). A rebuild exists
-- to serve a transition; a delivery does not. This migration makes the two
-- authorities say so.
--
-- What it deliberately does NOT refuse: a delivery whose space happens to be a
-- transition target. The target space becomes the head after activation and
-- ordinary writes to it are not obviously wrong, so refusing them would be a
-- rule invented here rather than one the product already holds.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade()')
       IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding delivery/rebuild XOR upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

-- Both replacements are anchored substitutions on the definition as installed,
-- rather than retyped bodies: these two functions are long, and a retyped body
-- can silently drop a line that a substitution cannot. The difference from
-- 0199, whose substitution caused this, is that each anchor here is required
-- to occur exactly once in exactly one named function, and each result is read
-- back before the migration is allowed to finish.
DO $close_the_xor$
DECLARE
    attempt_definition TEXT;
    acceptance_definition TEXT;
    attempt_signature CONSTANT TEXT :=
        'public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)';
    acceptance_signature CONSTANT TEXT :=
        'public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)';
    attempt_anchor CONSTANT TEXT := 'job_row.kind NOT IN (''delivery'',''rebuild'')';
    attempt_closed CONSTANT TEXT := 'job_row.kind<>''rebuild''';
    -- The same two lines appear again inside the insert-race branch at a
    -- deeper indent, and the shallower text is a substring of the deeper one.
    -- The anchor therefore carries its WHERE line, whose indent differs
    -- between the two, so that it matches the first and only the first.
    acceptance_anchor CONSTANT TEXT :=
'    SELECT * INTO prior FROM embedding_delivery_acceptance_receipts
     WHERE workspace_id=target_workspace AND idempotency_key=target_idempotency FOR UPDATE;';
    acceptance_closed CONSTANT TEXT :=
'    IF target_kind=''rebuild'' AND NOT EXISTS(
        SELECT 1 FROM embedding_transition_plans
         WHERE workspace_id=target_workspace
           AND target_space_registration_id=target_space
    ) THEN
        RAISE EXCEPTION ''a rebuild acceptance requires a transition plan targeting its space''
            USING ERRCODE=''23514'';
    END IF;
    SELECT * INTO prior FROM embedding_delivery_acceptance_receipts
     WHERE workspace_id=target_workspace AND idempotency_key=target_idempotency FOR UPDATE;';
BEGIN
    SELECT pg_get_functiondef(oid) INTO attempt_definition
      FROM pg_proc WHERE oid=to_regprocedure(attempt_signature);
    SELECT pg_get_functiondef(oid) INTO acceptance_definition
      FROM pg_proc WHERE oid=to_regprocedure(acceptance_signature);
    IF attempt_definition IS NULL OR acceptance_definition IS NULL THEN
        RAISE EXCEPTION 'the delivery/rebuild XOR authorities are not both present'
            USING ERRCODE='42883';
    END IF;

    -- Exactly once, or the anchor is not the thing this migration believes it
    -- is and the substitution would hit something else.
    IF (length(attempt_definition)-length(replace(attempt_definition,attempt_anchor,'')))
       / length(attempt_anchor) <> 1 THEN
        RAISE EXCEPTION 'the transition attempt kind anchor is not present exactly once'
            USING ERRCODE='23514';
    END IF;
    IF (length(acceptance_definition)-length(replace(acceptance_definition,acceptance_anchor,'')))
       / length(acceptance_anchor) <> 1 THEN
        RAISE EXCEPTION 'the delivery acceptance receipt anchor is not present exactly once'
            USING ERRCODE='23514';
    END IF;
    IF position('a rebuild acceptance requires a transition plan' IN acceptance_definition)<>0 THEN
        RAISE EXCEPTION 'the delivery acceptance already carries the rebuild binding check'
            USING ERRCODE='23514';
    END IF;

    EXECUTE replace(attempt_definition,attempt_anchor,attempt_closed);
    EXECUTE replace(acceptance_definition,acceptance_anchor,acceptance_closed);

    -- Read back what was installed rather than trusting that the statements
    -- above said what they meant.
    SELECT pg_get_functiondef(oid) INTO attempt_definition
      FROM pg_proc WHERE oid=to_regprocedure(attempt_signature);
    SELECT pg_get_functiondef(oid) INTO acceptance_definition
      FROM pg_proc WHERE oid=to_regprocedure(acceptance_signature);
    IF position(attempt_closed IN attempt_definition)=0
       OR position(attempt_anchor IN attempt_definition)<>0 THEN
        RAISE EXCEPTION 'the transition attempt did not take the rebuild-only rule'
            USING ERRCODE='23514';
    END IF;
    IF position('a rebuild acceptance requires a transition plan' IN acceptance_definition)=0
       OR position('embedding_transition_plans' IN acceptance_definition)=0 THEN
        RAISE EXCEPTION 'the delivery acceptance did not take the rebuild binding check'
            USING ERRCODE='23514';
    END IF;
END
$close_the_xor$;

DO $ownership$
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade()')
       IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade();
    ELSE
        -- Fresh install: this migration runs as the superuser, so both
        -- functions are restored to the posture their own migrations gave
        -- them. Unlike a trigger function, both are called by name from the
        -- runtime role, so the exact EXECUTE grant is restored with them.
        ALTER FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
            uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)
            OWNER TO vestrace_guarded_owner;
        REVOKE ALL ON FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
            uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) FROM PUBLIC,vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
            uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) TO vestrace;
        ALTER FUNCTION public.vestrace_begin_delivery_embedding_outputs(
            uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
            OWNER TO vestrace_guarded_owner;
        REVOKE ALL ON FUNCTION public.vestrace_begin_delivery_embedding_outputs(
            uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
            FROM PUBLIC,vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_begin_delivery_embedding_outputs(
            uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
            TO vestrace;
    END IF;
END
$ownership$;
