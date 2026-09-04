-- A governed mutation records a marker in the same transaction as its audit
-- event. The deferred foreign key makes a missing audit event a commit-time
-- refusal, including when the error is hidden until after all writes ran.

CREATE TABLE governed_mutation_audit_marks (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    audit_event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT governed_mutation_audit_marks_audit_event_fkey
        FOREIGN KEY (audit_event_id)
        REFERENCES audit_events(id)
        DEFERRABLE INITIALLY DEFERRED
);

ALTER TABLE governed_mutation_audit_marks ENABLE ROW LEVEL SECURITY;
ALTER TABLE governed_mutation_audit_marks FORCE ROW LEVEL SECURITY;

CREATE POLICY governed_mutation_audit_marks_workspace_isolation
    ON governed_mutation_audit_marks
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
