# C8 CORE+MEMORY Qualification Fixtures

Date: 2026-08-11

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: C8 is a test-fixtures-only slice for the `v0.2 Correct` gate.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: ARC, MEM, TMP, MUT, RET, and foundational Memory-protecting CAP requirements.
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`: tests, conformance, and qualification are separate layers; a fixture is not a profile result.
- `docs/adr/0010-qualification-profile-scope-follows-evidence-closure.md`: milestone labels do not grant profile claims.

## Scope

This slice adds deterministic evidence fixtures only:

1. a machine-readable mapping for every in-scope ARC/MEM/TMP/MUT/RET requirement and the foundational CAP subset;
2. a Memory evolution scenario covering immutable revisions, lifecycle guards, evidence, and derivation output;
3. an AsOf retrieval and ContextPack scenario covering temporal perspective, historical status, provenance, generation, explanation, and hard budget;
4. structured-memory schema and temporal-range validation fixtures;
5. explicit blocked mappings for semantics that require later runtime work.

No production runtime, database schema, authorization engine, or qualification runner is added in C8.

## Exit criteria

- [x] Every in-scope requirement ID is mapped to a covered or blocked fixture case.
- [x] Covered cases point to deterministic Rust test names.
- [x] Blocked cases carry an explicit reason and cannot be interpreted as PASS.
- [x] Golden fixtures preserve revision identity, status history, temporal perspective, provenance, and budget bounds.
- [x] Fixture manifest explicitly says `evidence_fixture_only` and does not claim `CORE+MEMORY` qualification.
- [x] Focused C8 test suite passes.
- [x] Documentation delta records remaining qualification/runtime limits.

## Verification

```text
cargo test --test c8_core_memory_qualification
4 passed
```

Full workspace verification remains a separate gate. PostgreSQL-backed RLS/migration runtime evidence still requires an isolated environment with `DATABASE_URL`.
