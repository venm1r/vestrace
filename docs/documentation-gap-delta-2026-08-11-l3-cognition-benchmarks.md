# Documentation Gap Delta — L3 COGNITION Benchmarks

Date: 2026-08-11

## Boundary

L3 is the bounded `v0.3 Learn` continuation after L1 typed evaluation facts and L2 advisory projection/proposal storage. The authoritative normative catalog defines LRN-001…008; the executable conformance registry is now aligned to that catalog.

## Implemented

`crates/vestrace-domain/src/evaluation.rs` now exposes a conservative authority relation and validates evaluator/authority compatibility:

- deterministic evaluators require deterministic authority;
- human evaluators require human-authorized authority;
- heuristic/model-judge evaluators remain advisory;
- deterministic and human-authorized signals outrank advisory signals.

`crates/vestrace-domain/src/learning.rs` adds `LearnedProjection::rebuild_from_facts`. The rebuild path is deterministic and workspace-bounded: stable fact IDs define ordering, duplicate facts are rejected, only deterministic generators are used, typed result/metric aggregates are generated, and source evaluation/evidence references are retained.

`crates/vestrace-cli/src/commands/conformance.rs` now provides evidence-backed LRN cases rather than returning `skip` for every COGNITION learning requirement. Cases that still need asset publication/application or PostgreSQL deletion/recovery are explicitly non-passing, and the CLI exits non-zero when a report contains skipped or failed cases.

`tests/fixtures/qualification/l3-cognition.json` defines `cognition-bench-v1` baseline/comparative cases for provenance, authority, contradiction/result aggregation, long-horizon source retention, forgetting, and the explicitly blocked full-runtime closure. `tests/l3_cognition_benchmarks.rs` verifies the manifest mapping and deterministic properties.

## Verification evidence

- `cargo test --test l3_cognition_benchmarks` — 4 passed.
- The existing L1/L2 tests remain the upstream evidence for typed facts and proposal-only learning boundaries.
- `cargo check --workspace` and workspace library/no-run checks are required as the next gate.
- `cargo diff --check` remains required; generated and unrelated dirty work is preserved.

## Remaining gaps

- The fixture does not produce a `QualificationBundle`, capture build/configuration/environment identity, or turn blocked cases into qualification evidence.
- PostgreSQL migration/RLS runtime evidence, projection deletion/recovery, cross-session continuity, and full benchmark execution require an isolated `DATABASE_URL` environment.
- The suite checks deterministic properties and provenance, not nondeterministic model wording or absolute model quality.
