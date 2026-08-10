# Vestrace Future Implementation PR Review Checklist

**Status:** normative planning checklist  
**Implementation changes authorized:** none

Use this checklist for every future implementation PR derived from the v0.2 documentation baseline.

## Architecture

- [ ] PR references exact requirement IDs.
- [ ] PR references applicable ADRs/specifications.
- [ ] authoritative state owner is explicit.
- [ ] no second runtime/event store/policy engine is introduced.
- [ ] derived state remains rebuildable.
- [ ] historical facts are not silently rewritten.
- [ ] milestone label and named qualification-profile claims are stated separately.

## Data / Migration

- [ ] migration is forward-only or explicitly `none`.
- [ ] applied historical migrations are untouched.
- [ ] backfill rules are deterministic.
- [ ] unknown historical values remain unknown rather than invented.
- [ ] workspace/RLS behavior is preserved for new tables.
- [ ] downgrade/rollback claims do not exceed actual guarantees.
- [ ] legacy data is not promoted to broader authority during backfill.

## Temporal / Concurrency

- [ ] expected revision/state preconditions are defined where needed.
- [ ] stale mutation behavior is explicit.
- [ ] `occurred_at`, `recorded_at`, validity and revision time are not conflated.
- [ ] crash/replay semantics are documented where applicable.

## Security / Governance

- [ ] capability requirements are exact.
- [ ] policy/data-governance gates are identified.
- [ ] negative authorization cases exist.
- [ ] child/delegated authority cannot widen parent authority.
- [ ] Memory/object scope cannot widen workspace authority.
- [ ] secrets are referenced, not persisted in plaintext.
- [ ] model/export/federation destination restrictions are preserved.
- [ ] mounted cross-workspace access evaluates source + target policy when applicable.

## External / Repair / Recovery

When applicable:

- [ ] UNKNOWN is represented explicitly.
- [ ] retry safety comes from adapter/operation contract.
- [ ] compensation is not called rollback.
- [ ] RepairPlan is immutable and stale-safe.
- [ ] finding resolution requires verification.
- [ ] AUTONOMY crash safety is not mislabeled as TRUSTED incident/revalidation.
- [ ] recovery does not restore TRUSTED without revalidation.

## Evidence / Conformance

- [ ] every targeted applicable MUST has executable evidence.
- [ ] test PASS is not presented as profile qualification by itself.
- [ ] required applicable MUST cases cannot be silently skipped.
- [ ] `NOT_APPLICABLE` is a separate target-scoped decision with reason/evidence, not an alias for missing implementation or SKIPPED.
- [ ] stable conformance case IDs are not reused for a different observable obligation.
- [ ] hard security/governance gates cannot be averaged away.
- [ ] formal FEDERATION/AUTONOMY/TRUSTED profile claims have complete applicable dependency closure.
- [ ] known limitations are updated.

## Documentation

- [ ] `docs/current-implementation.md` is updated for real behavior changes.
- [ ] schema/security/getting-started docs are updated when affected.
- [ ] target architecture docs are changed only when architecture itself changes.
- [ ] any architecture reversal gets an ADR rather than a silent rewrite.
- [ ] historical date-prefixed plans are not treated as current transition contracts.

## Merge block conditions

A PR must not merge when any of these is true:

```text
unresolved authority ownership ambiguity
missing required migration/backfill rule
required applicable MUST case skipped/inconclusive
NOT_APPLICABLE without applicability evidence
stable conformance case ID reused for different semantics
security hard-gate failure
unknown destructive retry semantics
history rewrite without explicit architecture decision
profile/milestone claim stronger than evidence
implementation claims stronger than evidence
```
