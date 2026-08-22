# A baseline with nowhere to live

**Date:** 2026-08-22
**Scope:** where a qualification baseline is kept.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. It is the prerequisite for the release gate's second producer.

```text
fault suite (real ephemeral deployment): FAILED — 2 failures, unchanged, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
```

## The gap

`QualificationBaseline` is durable by design: published from a bundle, carrying a
state through `Qualified → Stale / Invalidated / Failed`, keeping an invalidation
reason, and answering `matches_bundle`. It was stored nowhere — no table, no
repository, and outside the domain the only mention was a comment in `config.rs`
describing what `matches_bundle` refuses.

That blocks the next release-gate producer, and blocks it in a way worth naming.
`ReleaseApprovalService::evaluate` takes a baseline, and the only way to obtain
one was `QualificationBaseline::from_bundle` — from the very bundle being
approved. A bundle compared against a baseline derived from itself always
matches, so an approval producer written today would have passed by construction
and meant nothing.

A baseline is only a baseline because it was published *earlier* and can be
compared against *later*. Without a store, that "earlier" does not exist.

## What changed

`qualification_baselines` holds the published record, with the payload
authoritative and the columns beside it as projections checked on read.
`vestrace conformance publish-baseline` publishes one from a bundle and prints
its id — an operator act with a date attached, not something derived on demand.
`from_bundle` keeps its place as the publication constructor; nothing derives a
baseline at approval time.

Two properties are database facts rather than conventions. A published
baseline's profile, target digest and published time cannot be rewritten — the
trigger guards them in **both** the projected columns and the authoritative
payload, because guarding only the columns would leave a route to rewrite the
record and have the integrity check call it corrupt afterwards. State and
invalidation reason may still move together, which is the lifecycle the domain
models.

The table is deliberately **not** workspace-scoped, and the migration header says
so: a baseline describes a release target, not a tenant's data. Every
neighbouring table here is scoped, so a reader will expect it to be, and "no
policy" has to be a stated decision rather than an omission.

## A requirement of mine that was wrong

The plan asked for uniqueness on the target digest alone — "a target with two
baselines has no baseline" — and the implementation did exactly that. It is
wrong, and the reason is visible in the domain: a baseline carries a `profile`.

The profiles are a ladder of increasingly demanding gates. A build qualified as
`core` and later as `trusted` has two true and different facts about itself.
Under target-only uniqueness the profile column carries nothing the target does
not already determine, and publishing the later baseline would require
invalidating the earlier one — losing history for no reason at all.

Uniqueness is now on `(target_digest, profile)`. The guarantee is unchanged at
the granularity that matters: a target *and profile* with two baselines has no
baseline. The test that asserted cross-profile refusal was inverted into one
asserting cross-profile publication, with the reasoning written beside it.

This is the third requirement of mine this session that turned out to be wrong —
after asking for a clean shutdown to delete its presence row, and for pre-0155
rows to be given an insertion order the schema never stored. In two of those
three the implementing agent caught it; this one I caught while reading the
result. The plans have been the least reliable artifact in this process, which is
worth saying plainly given how much weight the workflow puts on them.

## A dependency that renaming would have broken silently

The repository recognises a duplicate publication by matching the **constraint
name** in the database error. Renaming the constraint in the migration therefore
turned a refusal into a generic storage failure, and nothing in the type system
would have said so. The test caught it — which is the argument for testing a
refusal rather than assuming it: a check that identifies itself by a string is
only as good as the test that pins the string.

## Verification

Run here against a PostgreSQL 17 with pgvector. Eight of eight baseline
repository tests pass, including round-tripping at the column's precision,
refusal of a duplicate, publication of a second profile beside the first, and the
trigger refusing each published fact in both the column and the payload.

The fault suite is unchanged at `failures=2`, as predicted — this slice adds a
store and a command and touches nothing the scenario drives. `cargo fmt` and
`clippy` clean; `cargo test --workspace --no-fail-fast` green apart from the
pre-existing `authorized_shared_reader_uses_exact_revision_under_forced_rls`;
conformance gate unchanged.

## What this does not do

**Nothing publishes a baseline automatically**, and nothing consumes one yet. The
release approval producer is the next slice, and it needs a signer trust policy
and signature evidence besides — both of which the CLI already builds for the
crypto path.

**The lifecycle is stored but not driven.** `mark_stale` and invalidation exist
in the domain and stay unused: what *makes* a baseline stale is a policy question
— a newer release, a schema change, an expiry — and inventing a trigger for it
while building the store would have answered that question by accident.

**Two producers remain absent besides release approval**: recovery qualification,
which needs observations from eight recovery targets and therefore a harness of
its own, and capability restoration, which needs a restoration policy with no
source today. The release gate still cannot pass.
