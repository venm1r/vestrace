-- An embedding data-policy decision is deployment evidence captured at the
-- boundary where one configured embedding client is about to receive content.
-- It stores labels and counts, never the disclosed text or resulting vectors.
-- The label verdict comes from the domain ClassificationPolicy used by
-- retrieval; the destination verdict comes independently from DataPolicy.
-- Keeping both facts prevents a later reader from mistaking a refused revision
-- label for a refused host, or one allowance for the other.
--
-- The table deliberately has no workspace_id and therefore no tenant RLS. A
-- workspace id would say a tenant owns the deployment's proof that its egress
-- boundary ran. What distinguishes disclosures is their cause instead: an
-- outbox message id plus delivery attempt, a retrieval request id, or a rebuild
-- invocation id plus batch ordinal. The shape constraint binds each purpose to
-- exactly the cause fields its purpose-specific method can supply; a caller
-- cannot write a dimension_probe row using a delivery attempt.
--
-- These causal ids deliberately have no foreign keys. Deployment evidence must
-- survive cleanup of tenant outbox, retrieval-journal, and rebuild state. The
-- row commits before the provider is called, and UPDATE or DELETE would turn an
-- observed decision into a later story, so both are trigger-refused following
-- the append-only evidence pattern in 0160, 0162, and 0163.

CREATE TABLE embedding_data_policy_decisions (
    id UUID PRIMARY KEY,
    purpose TEXT NOT NULL,
    causal_reference_id UUID NOT NULL,
    delivery_attempt INTEGER,
    batch_ordinal INTEGER,
    destination TEXT NOT NULL,
    classification TEXT NOT NULL,
    classification_labels TEXT[] NOT NULL,
    unclassified_count INTEGER NOT NULL,
    input_count INTEGER NOT NULL,
    classification_allowed BOOLEAN NOT NULL,
    destination_allowed BOOLEAN NOT NULL,
    verdict TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    policy_version TEXT NOT NULL CHECK (btrim(policy_version) <> ''),
    mode TEXT NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT embedding_data_policy_purpose_known CHECK (
        purpose IN ('delivery', 'retrieval_query', 'backfill', 'dimension_probe')
    ),
    CONSTRAINT embedding_data_policy_cause_matches_purpose CHECK (
        (purpose = 'delivery' AND delivery_attempt > 0 AND batch_ordinal IS NULL)
        OR (purpose = 'retrieval_query' AND delivery_attempt IS NULL AND batch_ordinal IS NULL)
        OR (purpose IN ('backfill', 'dimension_probe')
            AND delivery_attempt IS NULL AND batch_ordinal > 0)
    ),
    CONSTRAINT embedding_data_policy_destination_known CHECK (
        destination IN ('local_model', 'remote_provider')
    ),
    CONSTRAINT embedding_data_policy_classification_known CHECK (
        classification IN ('public', 'internal', 'confidential', 'restricted')
    ),
    CONSTRAINT embedding_data_policy_labels_are_not_blank CHECK (
        array_position(classification_labels, '') IS NULL
    ),
    CONSTRAINT embedding_data_policy_unclassified_count_valid CHECK (
        unclassified_count >= 0 AND unclassified_count <= input_count
    ),
    CONSTRAINT embedding_data_policy_input_count_positive CHECK (input_count > 0),
    CONSTRAINT embedding_data_policy_verdict_known CHECK (
        verdict IN ('allowed', 'denied')
    ),
    CONSTRAINT embedding_data_policy_verdict_matches_checks CHECK (
        (verdict = 'allowed') = (classification_allowed AND destination_allowed)
    ),
    CONSTRAINT embedding_data_policy_mode_known CHECK (
        mode IN ('enforce', 'observe')
    )
);

CREATE INDEX idx_embedding_data_policy_decisions_cause
    ON embedding_data_policy_decisions
    (purpose, causal_reference_id, delivery_attempt, batch_ordinal, decided_at, id);

CREATE OR REPLACE FUNCTION vestrace_prevent_embedding_data_policy_decision_modification()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'embedding data-policy decisions are append-only'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER tr_embedding_data_policy_decisions_no_update
    BEFORE UPDATE ON embedding_data_policy_decisions
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_embedding_data_policy_decision_modification();

CREATE TRIGGER tr_embedding_data_policy_decisions_no_delete
    BEFORE DELETE ON embedding_data_policy_decisions
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_embedding_data_policy_decision_modification();
