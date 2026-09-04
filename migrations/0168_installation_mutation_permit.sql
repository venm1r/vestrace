-- Installation-wide mutation progression is owned by a single guarded row.
-- The accompanying advisory lock is transaction-scoped in PostgreSQL, while
-- this watermark records only committed governed mutations.
CREATE TABLE installation_mutation_watermark (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    watermark BIGINT NOT NULL DEFAULT 0 CHECK (watermark >= 0),
    advanced_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO installation_mutation_watermark (singleton, watermark)
VALUES (TRUE, 0)
ON CONFLICT (singleton) DO NOTHING;

CREATE TABLE installation_mutation_watermark_advances (
    governed_mutation_mark_id UUID PRIMARY KEY,
    watermark BIGINT NOT NULL UNIQUE CHECK (watermark > 0),
    advanced_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT installation_mutation_watermark_advances_mark_fkey
        FOREIGN KEY (governed_mutation_mark_id)
        REFERENCES governed_mutation_audit_marks(id)
        DEFERRABLE INITIALLY DEFERRED
);

-- A marker is a governed mutation claim. It cannot commit unless the same
-- transaction supplied its advancing installation watermark row.
ALTER TABLE governed_mutation_audit_marks
    ADD CONSTRAINT governed_mutation_audit_marks_watermark_advance_fkey
    FOREIGN KEY (id)
    REFERENCES installation_mutation_watermark_advances(governed_mutation_mark_id)
    DEFERRABLE INITIALLY DEFERRED;


CREATE OR REPLACE FUNCTION vestrace_record_governed_mutation_audit_mark_and_advance(
    marker_id UUID,
    marker_workspace_id UUID,
    marker_audit_event_id UUID,
    marker_created_at TIMESTAMPTZ
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    advanced_watermark BIGINT;
BEGIN
    INSERT INTO governed_mutation_audit_marks
        (id, workspace_id, audit_event_id, created_at)
    VALUES
        (marker_id, marker_workspace_id, marker_audit_event_id, marker_created_at);

    UPDATE installation_mutation_watermark
    SET watermark = watermark + 1,
        advanced_at = marker_created_at
    WHERE singleton
    RETURNING watermark INTO advanced_watermark;

    INSERT INTO installation_mutation_watermark_advances
        (governed_mutation_mark_id, watermark, advanced_at)
    VALUES
        (marker_id, advanced_watermark, marker_created_at);

    RETURN advanced_watermark;
END
$$;

-- Preserve the existing guarded-operation entry point for any caller that
-- still uses it, while making its marker advance the watermark as well.
CREATE OR REPLACE FUNCTION vestrace_record_governed_mutation_audit_mark(
    marker_id UUID,
    marker_workspace_id UUID,
    marker_audit_event_id UUID,
    marker_created_at TIMESTAMPTZ
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM vestrace_record_governed_mutation_audit_mark_and_advance(
        marker_id,
        marker_workspace_id,
        marker_audit_event_id,
        marker_created_at
    );
END
$$;
