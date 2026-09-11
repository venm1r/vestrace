-- P04 completion: let the transition header reach the states its own
-- authorities write.
--
-- `vestrace_guard_embedding_transition_header` was written by 0200, when the
-- only moves that existed were the two the execution path makes. Two later
-- migrations added writers and neither extended it:
--
--   0201 line 429  UPDATE embedding_transitions SET state='activated'
--   0203 line 362  UPDATE embedding_transitions SET state='stale'
--
-- The guard admits neither, so both writes raise 23514 'embedding transition
-- permits only guarded progress'. `activated` and `stale` are both listed among
-- the legal states by 0188 line 42, so this was never a vocabulary decision --
-- the guard simply was not revisited.
--
-- What that has cost, in plain terms: `vestrace_activate_embedding_transition`
-- has never been able to complete, in any deployment. Every call to it in this
-- repository is an `unwrap_err`, and the reason was taken for a fixture problem
-- until an activation attempt was finally carried far enough to reach the real
-- one. And `vestrace_propagate_embedding_source_erasure` cannot commit whenever
-- an erased source invalidates a planned transition, which is precisely the
-- case its own comment describes as the reason the stale-ing exists.
--
-- This migration adds exactly the two moves those authorities make and nothing
-- else. `failed` stays unreachable because nothing writes it; a guard widened
-- past its writers would stop being evidence of anything.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_transition_header_upgrade()')
       IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_transition_header_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding transition header upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

-- The replacement is unconditional, so the body being replaced is checked
-- first. A CREATE OR REPLACE over a definition this migration did not expect
-- would silently discard whatever else had been done to it, and say nothing.
DO $precondition$
DECLARE definition TEXT;
BEGIN
    SELECT pg_get_functiondef(oid) INTO definition FROM pg_proc
     WHERE oid=to_regprocedure('public.vestrace_guard_embedding_transition_header()');
    IF definition IS NULL THEN
        RAISE EXCEPTION 'embedding transition header guard is absent' USING ERRCODE='42883';
    END IF;
    IF position('(OLD.state=''rebuilding'' AND NEW.state=''ready_to_activate'')' IN definition)=0
       OR position('NEW.state=''activated''' IN definition)<>0 THEN
        RAISE EXCEPTION 'embedding transition header guard is not the 0200 definition this migration replaces'
            USING ERRCODE='23514';
    END IF;
END
$precondition$;

CREATE OR REPLACE FUNCTION vestrace_guard_embedding_transition_header()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='INSERT' THEN
        RETURN NEW;
    END IF;
    IF TG_OP='DELETE'
       OR OLD.workspace_id IS DISTINCT FROM NEW.workspace_id
       OR OLD.id IS DISTINCT FROM NEW.id
       OR OLD.target_space_registration_id IS DISTINCT FROM NEW.target_space_registration_id
       OR OLD.created_at IS DISTINCT FROM NEW.created_at
       OR NEW.version<>OLD.version+1
       OR NOT (
           (OLD.state='planned' AND NEW.state='rebuilding')
           OR (OLD.state='rebuilding' AND NEW.state='ready_to_activate')
           -- 0201's activation, which could not commit before this migration.
           OR (OLD.state='ready_to_activate' AND NEW.state='activated')
           -- 0203's erasure invalidation, from any state that still has
           -- recipes to lose. A transition already activated or already stale
           -- has nothing left for an erased source to invalidate.
           OR (OLD.state IN ('planned','rebuilding','ready_to_activate')
               AND NEW.state='stale')
       ) THEN
        RAISE EXCEPTION 'embedding transition permits only guarded progress' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END
$$;

-- Read back what was installed rather than trusting that the statement above
-- said what it meant.
DO $readback$
DECLARE definition TEXT;
BEGIN
    SELECT pg_get_functiondef(oid) INTO definition FROM pg_proc
     WHERE oid=to_regprocedure('public.vestrace_guard_embedding_transition_header()');
    IF position('(OLD.state=''ready_to_activate'' AND NEW.state=''activated'')' IN definition)=0
       OR position('AND NEW.state=''stale''' IN definition)=0
       OR position('(OLD.state=''planned'' AND NEW.state=''rebuilding'')' IN definition)=0
       OR position('(OLD.state=''rebuilding'' AND NEW.state=''ready_to_activate'')' IN definition)=0 THEN
        RAISE EXCEPTION 'embedding transition header guard did not take its four moves'
            USING ERRCODE='23514';
    END IF;
END
$readback$;

DO $ownership$
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_transition_header_upgrade()')
       IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_transition_header_upgrade();
    ELSE
        -- Fresh install: this migration runs as the superuser, so the function
        -- it replaced is restored to the posture 0200 gave it directly. It is
        -- a trigger function and is never called by name, so it carries no
        -- EXECUTE grant to the runtime role and none is restored.
        ALTER FUNCTION public.vestrace_guard_embedding_transition_header()
            OWNER TO vestrace_guarded_owner;
        REVOKE ALL ON FUNCTION public.vestrace_guard_embedding_transition_header()
            FROM PUBLIC,vestrace;
    END IF;
END
$ownership$;
