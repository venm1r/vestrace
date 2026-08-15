# Documentation Gap Delta — C8 CORE+MEMORY Fixtures

Date: 2026-08-11

## Authority and boundary

C8 is the `v0.2 Correct` test-fixtures-only slice in the execution matrix. It does not implement a qualification engine and does not qualify the `CORE` or `MEMORY` profile. The qualification specification requires separate tests, conformance evidence, and qualification results; the fixture artifact therefore uses the explicit status `evidence_fixture_only`.

## Implemented fixture evidence

`tests/fixtures/qualification/c8-core-memory.json` now provides:

- complete mapping of the in-scope ARC/MEM/TMP/MUT/RET MUST IDs plus the foundational Memory-protecting CAP subset;
- `covered` mappings for the behavior currently represented by the domain/application surfaces;
- `blocked` mappings with reasons for policy/audit boundaries, claims/conflicts, concurrency preconditions, reconciliation, classification, cache invalidation, mounts, and capability enforcement that are not yet runtime contracts;
- stable test names and scenario categories for future conformance/qualification promotion.

`tests/c8_core_memory_qualification.rs` provides four deterministic fixtures:

1. Memory evolution preserves immutable revision identity, status history, direct evidence, and derivation output.
2. AsOf retrieval preserves historical status defaults, temporal perspective, exact revision provenance, source generation, explanation, and ContextPack hard budget.
3. Structured memory carries schema identity and bounded confidence/importance.
4. Invalid temporal ranges are rejected.

## Verification evidence

```text
cargo test --test c8_core_memory_qualification
4 passed
```

The focused suite is fixture evidence only. It does not produce a `QualificationBundle`, target identity, signed result, profile dependency closure, or PASS for blocked MUST requirements.

## Remaining gaps

- `QUAL-001..018` qualification lifecycle and runner remain future work.
- CORE+MEMORY profile qualification remains open because blocked MUSTs are not executable passing evidence.
- PostgreSQL/RLS/migration runtime evidence remains environment-dependent and was not claimed by this slice.
- Conflict/Claim semantics, capability authorization, classification/model-destination enforcement, mounted retrieval, and cache invalidation remain outside C8.
