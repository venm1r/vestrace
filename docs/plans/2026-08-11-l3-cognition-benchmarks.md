# L3 COGNITION Benchmark / Conformance Boundary

Date: 2026-08-11

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: L3 is the benchmark-fixture and COGNITION property-suite gate after L2.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: normative `LRN-001` through `LRN-008` semantics.
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`: property-oriented cognitive evaluation and benchmark families.
- `docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md`: v0.3 Learn exit criteria and Cognition Bench baseline.

## Scope implemented

1. Align the executable LRN registry with the normative invariant IDs; the previous registry text had drifted to a different numbering scheme.
2. Add explicit evaluation-authority ordering: deterministic and human-authorized signals outrank advisory/model-judge signals, without inventing an ordering between deterministic and human authority.
3. Add a deterministic `LearnedProjection::rebuild_from_facts` path. It sorts canonical facts by stable identity, rejects duplicate or cross-workspace input, aggregates only typed raw measurements, and retains exact source fact/evidence references.
4. Add `tests/fixtures/qualification/l3-cognition.json` and `tests/l3_cognition_benchmarks.rs` covering baseline/comparative modes, provenance, authority, contradiction/result aggregation, long-horizon source retention, and the forgetting boundary.
5. Wire LRN-001…008 into the CLI conformance registry with source evidence while preserving the distinction between code/property evidence and full runtime qualification.

## Exit criteria

- [x] LRN registry text matches the normative invariant IDs.
- [x] Raw evaluation facts remain independent from advisory learned projections.
- [x] A deterministic projection rebuild is reproducible from the same canonical facts regardless of input order.
- [x] Rebuild output retains underlying measurement and evidence references.
- [x] Deterministic/human authority is explicitly stronger than advisory authority.
- [x] The COGNITION fixture maps all LRN requirements and uses explicit baseline/comparative suite metadata.
- [x] Focused L3 property tests pass.
- [x] CLI conformance emits evidence for the LRN family and marks partial/runtime-gated cases as non-passing instead of silently claiming qualification.

## Explicit non-claims

This slice is an evidence fixture/property suite, not a `QualificationBundle` and does not qualify the `COGNITION` profile. It does not implement model-quality scoring, exact LLM wording comparison, projection deletion/recovery in PostgreSQL, cross-session runtime benchmarks, environment/build identity capture, or governance approval/application of learning proposals. PostgreSQL runtime evidence remains dependent on an isolated configured database.
