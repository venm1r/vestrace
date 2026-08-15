ALTER TABLE conflicts
    ADD COLUMN IF NOT EXISTS state_revision INTEGER NOT NULL DEFAULT 0;

ALTER TABLE claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE claim_evidence_links ENABLE ROW LEVEL SECURITY;
ALTER TABLE claim_assessments ENABLE ROW LEVEL SECURITY;
ALTER TABLE conflicts ENABLE ROW LEVEL SECURITY;
ALTER TABLE cognitive_mutations ENABLE ROW LEVEL SECURITY;
ALTER TABLE reconciliation_records ENABLE ROW LEVEL SECURITY;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = 'claims'
          AND policyname = 'claims_workspace_isolation'
    ) THEN
        CREATE POLICY claims_workspace_isolation ON claims
            FOR ALL
            USING (workspace_id = vestrace_current_workspace_id())
            WITH CHECK (workspace_id = vestrace_current_workspace_id());
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = 'claim_evidence_links'
          AND policyname = 'claim_evidence_links_workspace_isolation'
    ) THEN
        CREATE POLICY claim_evidence_links_workspace_isolation ON claim_evidence_links
            FOR ALL
            USING (workspace_id = vestrace_current_workspace_id())
            WITH CHECK (workspace_id = vestrace_current_workspace_id());
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = 'claim_assessments'
          AND policyname = 'claim_assessments_workspace_isolation'
    ) THEN
        CREATE POLICY claim_assessments_workspace_isolation ON claim_assessments
            FOR ALL
            USING (workspace_id = vestrace_current_workspace_id())
            WITH CHECK (workspace_id = vestrace_current_workspace_id());
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = 'conflicts'
          AND policyname = 'conflicts_workspace_isolation'
    ) THEN
        CREATE POLICY conflicts_workspace_isolation ON conflicts
            FOR ALL
            USING (workspace_id = vestrace_current_workspace_id())
            WITH CHECK (workspace_id = vestrace_current_workspace_id());
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = 'cognitive_mutations'
          AND policyname = 'cognitive_mutations_workspace_isolation'
    ) THEN
        CREATE POLICY cognitive_mutations_workspace_isolation ON cognitive_mutations
            FOR ALL
            USING (workspace_id = vestrace_current_workspace_id())
            WITH CHECK (workspace_id = vestrace_current_workspace_id());
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = 'reconciliation_records'
          AND policyname = 'reconciliation_records_workspace_isolation'
    ) THEN
        CREATE POLICY reconciliation_records_workspace_isolation ON reconciliation_records
            FOR ALL
            USING (workspace_id = vestrace_current_workspace_id())
            WITH CHECK (workspace_id = vestrace_current_workspace_id());
    END IF;
END
$$;
