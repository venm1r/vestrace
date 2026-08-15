ALTER TABLE memory_sources
    ADD COLUMN IF NOT EXISTS evidence_ref JSONB;

UPDATE memory_sources
    SET role = 'primary'
    WHERE role = 'direct_source';

UPDATE memory_sources
    SET role = 'supporting'
    WHERE role = 'supporting_context';

UPDATE memory_sources
    SET role = 'contradicting'
    WHERE role = 'contradicting_evidence';
