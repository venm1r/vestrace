-- Migration: 0090_release_orchestration_and_manifests.sql

CREATE TABLE IF NOT EXISTS release_manifests (
    id UUID PRIMARY KEY,
    release_version TEXT NOT NULL UNIQUE,
    components JSONB NOT NULL DEFAULT '[]'::jsonb,
    checksum TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
