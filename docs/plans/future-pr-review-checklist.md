# Vestrace Future Implementation PR Review Checklist

## Architecture

- [ ] exact requirement IDs referenced;
- [ ] applicable ADRs/specifications referenced;
- [ ] authoritative state owner explicit;
- [ ] no second runtime/event store/policy engine;
- [ ] derived state remains rebuildable;
- [ ] history not silently rewritten.

## Data / Migration

- [ ] migration forward-only or explicitly `none`;
- [ ] applied historical migrations untouched;
- [ ] backfill deterministic;
- [ ] unknown historical values remain unknown;
- [ ] workspace/RLS behavior preserved;
- [ ] compatibility/rollback claims match reality.

## Temporal / Concurrency

- [ ] expected state/revision preconditions defined;
- [ ] stale mutation behavior explicit;
- [ ] occurred/recorded/validity/revision time not conflated;
- [ ] crash/replay semantics documented where applicable.

## Security / Governance

- [ ] exact capabilities identified;
- [ ] policy/data-governance gates identified;
- [ ] negative authorization cases exist;
- [ ] delegation cannot widen authority;
- [ ] secrets referenced, not stored plaintext;
- [ ] provider/export/federation destination rules preserved.

## External / Repair / Recovery

When applicable:

- [ ] UNKNOWN explicit;
- [ ] retry safety comes from operation/adapter contract;
- [ ] compensation is not rollback;
- [ ] RepairPlan immutable/stale-safe;
- [ ] finding resolution requires verification;
- [ ] recovery cannot restore TRUSTED without revalidation.

## Evidence / Conformance

- [ ] every targeted MUST has executable evidence;
- [ ] test PASS is not presented as qualification by itself;
- [ ] required MUST cannot be silently skipped;
- [ ] hard security/governance gates cannot be averaged away;
- [ ] known limitations updated.

## Merge blockers

```text
unresolved authority ownership ambiguity
missing required migration/backfill rule
required MUST skipped/inconclusive
security hard-gate failure
unknown destructive retry semantics
history rewrite without explicit architecture decision
implementation claims stronger than evidence
```
