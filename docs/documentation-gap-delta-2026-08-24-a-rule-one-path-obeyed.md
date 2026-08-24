# A rule one path obeyed

**Date:** 2026-08-24
**Scope:** the data-policy gate on the embedding channel — the policy half of the
slice whose transport half shipped as 0d320f8.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim.

```text
infrastructure suite:  238 passed, 0 failed  (real PostgreSQL 17 + pgvector)
application suite:     green; the gate's own tests 9/9
live model tests:      4 passed against LM Studio
compose smoke:         4 passed, serially, with the model switch on
release gate tests:    24 passed
conformance gate:      199 passed, 0 failed — unchanged
fault suite:           FAILED — 2 failures, unchanged
```

## The gap

`retrieval::hydration::ClassificationPolicy` decides which
`MemoryRevision.classification` labels may be disclosed. It admits by exact
label, treats a `NULL` classification as *unassessed* rather than *harmless*
via a separate explicit flag, has no allow-everything variant that a deployment
can drift into, and lives in the domain deliberately —

> so a future adapter cannot quietly implement a laxer version of it.

The embedding path read `item.content` and `active_text(memory_id)` and sent
them to a provider. It never looked at the label. A revision that retrieval
withheld travelled out of the building by the adjacent route.

Not a missing rule. A rule that existed, was argued, was placed where it could
not be weakened — and one path did not ask it.

## What I got wrong, twice, on the way here

**First:** the plan before this one said the classification field "is
interpreted by nothing" and therefore could not govern anything. That was
wrong, and it was wrong because of how I checked: I grepped for reads and
writes of the column and found only persistence. The rule was not in the
repository layer. It was in the domain, keyed off the value, exactly where the
codebase's conventions say such a rule belongs.

Searching for *who reads a field* finds persistence. Searching for *who decides
on it* finds governance. Only the second question was the one worth asking.

**Second:** this slice's smoke test caught a defect I introduced three slices
earlier. Slice 16 began refusing startup when the model is enabled without a
completion data policy, and `docker-compose.yml` never supplied one. I reported
slice 16 as verified. It was verified by everything I thought to run, and the
compose smoke tests are `#[ignore]`, so I did not run them.

The correction that matters: the checked-in stack was **not** broken, because
compose defaults the model switch to `false`. What was broken is the path that
switch turns on. "We shipped a broken deployment" and "the deployment cannot
serve its own documented option" are different claims, and only the second is
true.

## What the gate does

Two checks stand between memory content and a provider, and a denial says which
one refused, because the row carries `classification_allowed` and
`destination_allowed` separately:

- the label must be admissible — `ClassificationPolicy`, reused, not
  reimplemented;
- the destination must be allowed — `DataPolicy`, against the egress descriptor
  that 0d320f8 built.

`[policy.data.embedding]` is its own declaration. Reusing the completion key
would have turned an existing statement about run objectives into consent for
bulk corpus disclosure. No ordering between the two is enforced: requiring
`embedding >= completion` would presume memory is always at least as sensitive
as an objective, and presuming is what a gate exists to avoid.

## Closed by type, not by wiring

Wrapping the three composition roots would have proved today's wiring.
`EmbeddingBackfillService`, `EmbedMemoryHandler` and `PgVectorRetriever` now
accept only a governed provider, so a raw client cannot be injected past the
gate. Purpose is expressed by method rather than by an argument, so a row
saying `dimension_probe` is one only the probe could have written.

The probe is gated with no exemption. It discloses nothing, which is precisely
why exempting it is tempting — and an ungated route to the same endpoint is how
a gate becomes decorative.

## The column that was not added

The evidence table has no `workspace_id`. The repository enforces
`a_table_holding_a_workspace_id_has_row_level_security`: such a column declares
tenant ownership and demands forced RLS. The earlier plan called one
"attribution, not tenancy" — a contradiction the codebase already refuses — and
renaming it to slip past the invariant would have been the same semantics in a
disguise.

It would also have distinguished nothing. The worker builds one
`RequestContext` per workspace and reuses it across every delivery and retry;
rebuild reuses one across every batch and probe. Causal references do the work
the column was imagined for, and a CHECK binds their shape to the purpose: a
delivery row must carry an attempt, a query row neither, backfill and probe a
batch ordinal.

## Proof by breaking

Replacing `let allowed = classification_allowed && destination_allowed;` with
`let allowed = true;` fails four tests, among them
`a_label_retrieval_would_withhold_is_refused_before_embedding` — the one the
slice exists for. Restored, 9/9 pass.

## What this does not do

**`MemoryRevision.classification` still has no validated vocabulary.** This
slice reuses the admission rule; it does not constrain what may be written into
the field. Until it is constrained, the rule admits by exact string match
against whatever a writer chose.

**The compose policy is permissive in effect** — every label admitted,
unclassified allowed — and now says so in a comment above both blocks, because
an enumerated allow-everything reads as a considered policy while being none.

**`memory_repository`'s `row.try_get("classification").ok()`** turns a decode
failure into an absent classification: a failure manufactured into an absence,
which is the inverse of the distinction this codebase otherwise keeps.

**`EmbeddingConfig::secret_name` is still inert.** Remote embedding
authentication cannot work while the configuration says it can.

**The four compose smoke tests cannot run in parallel** — each runs
`docker compose up` on the same project. Their first collective failure looked
like a verdict and was a race.
