# Documentation Gap Delta — Retrieval Conformance, and Reaching Above the Domain

**Date:** 2026-08-14
**Scope:** RET requirements, `ContextPack` invariants, and extending the conformance runner past the domain crate
**Repository state:** dirty, implementation changes uncommitted

## The structural limit that had to go first

`vestrace_domain::conformance::cases` can only reach domain types, because the
domain crate depends on nothing above it. That was adequate for ARC, TMP, MEM
and MUT, which are statements about the domain model. It is a dead end for
almost everything left.

RET, CAP, EXT, GOV and REC are statements about **behaviour**: how channels
fuse, what a boundary refuses, what a journal records. None of that is visible
from the domain. Left as it was, the conformance mechanism would have topped out
at the requirements it already covered no matter how much code was written.

The runner is now assembled from both layers —
`vestrace_application::conformance_cases` alongside the domain's — and the CLI
merges them, being the first place that can see both.

## Nine RET requirements are now verified

| ID | Class | What runs |
| --- | --- | --- |
| RET-003 | Security | A pack cannot be constructed unless an authorization check happened, so there is no moment at which one exists having skipped it |
| RET-005 | Evidence | An item with no provenance reference, or no revision, cannot be packed |
| RET-006 | Domain | The four representation levels have distinct wire forms and each round-trips |
| RET-007 | Domain | Over budget refused, exactly-at-budget allowed, and a declared total that disagrees with the items caught |
| RET-008 | Stateful | Superseded and expired items keep their status *and* their closing validity bound through packing |
| RET-009 | Domain | Fused ranking is a total order over the data |
| RET-010 | Stateful | The degraded flag is derived from the named channels, so the two cannot come apart; a blank channel name is refused |
| RET-012 | Domain | An as-of pack is distinguishable from a current one and names the instant it reconstructs |
| RET-014 | Security | A retrieval result from another workspace is refused outright, not filtered |

MEMORY moved from 15 unregistered to **6**, at `57 passed (50 executed, 7
attested)`. Every remaining attestation across CORE and MEMORY is `Static`.

## Four defects the work exposed

### 1. Channel fusion was not deterministic

`reciprocal_rank_fusion` sorted on score alone, over a `HashMap` whose iteration
order Rust randomises **per process**. Tied candidates therefore ranked by hash
order, and two runs of the same server over the same data could rank them
differently.

Ties are not rare here: reciprocal rank fusion assigns identical contributions
to identical ranks, so any two candidates at mirrored positions across channels
tie exactly.

The sort now breaks ties on memory id, making the order a total function of the
data — which is what "deterministic and auditable" requires: the same inputs
justify the same ranking, and a ranking can be re-derived from a journal.

**A note on how this was nearly missed.** The first attempt at a check repeated
the fusion call and compared results. That test passes whether or not the bug
exists, because the hasher seed is fixed for a process's lifetime. The case
asserts the *property* instead — that the output is in a total order derived
from the data — which fails if ties are left to iteration order.

### 2. A context pack could carry text with no provenance

Nothing required an included item to reference the revision it was rendered
from. That is the difference between context a reader can check and context they
have to take on faith, and a model given unattributable context produces
unattributable output. `ContextPack::new` now refuses it.

**Two existing tests were asserting the gap.** `v01_acceptance` built its context
items with `provenance_refs: Vec::new()` — an acceptance test stating that
unattributable context is acceptable. Both had the candidate's memory and
revision ids in hand and simply were not using them.

### 3. Declared token usage did not have to match the payload

`ContextPack::new` checked `used_tokens <= token_budget` and nothing else, so a
pack could declare 800 tokens while carrying items summing to 400 — or to 4000.
The budget was honoured in the summary and unconstrained in the payload. The
per-item counts must now add up to the declared total.

Two fixtures were building packs that claimed a non-zero usage while carrying no
sections at all. Both are corrected rather than exempted, and
`retrieval_e2e.rs` gained the complementary case: staying under the ceiling is
not enough if the declaration and the payload disagree.

### 4. Degradation was set by field assignment after construction

`RetrievalService::build_context` assigned `degraded`, `degraded_channels` and
`warnings` directly onto the pack after building it. It worked, but forgetting
any one line would have produced a pack that looked complete while some of its
channels had failed — the silent substitution RET-010 forbids.
`ContextPack::with_degradation` now derives the flag from the named channels, so
they cannot disagree, and refuses a channel recorded without a name.

## RET-013: the journal now records what it is for

The requirement is that the journal records "the intent, parameters, channels
and resulting pack". It recorded the intent, the query text, a candidate count
and the pack. The rest of the parameters were absent and the channels were
absent entirely.

Those omissions turn a journal from evidence into a log line. Two retrievals
with the same query and intent can legitimately return different results because
their admissible statuses differed, or because a channel was down; without those
fields the journal cannot tell a correct ranking from a degraded one, and cannot
be replayed at all.

Migration 0138 adds `parameters` and `channels`. The port takes a
`RetrievalRunRecord` instead of six positional arguments — it was already
`#[allow(clippy::too_many_arguments)]`, and every missing field would have made
it worse, which is part of why they were never added. `channels` records **every
configured channel and its outcome**, successes included: a journal listing only
what worked cannot answer "was the vector channel consulted?", and the two
reasons for an absence — not configured, and failed — are the two an
investigation must separate. A channel that was never asked is skipped rather
than recorded as a failure.

Two more things fell out.

**The journal was writing unscoped.** `PgRetrievalJournal` held a bare `PgPool`,
so `vestrace.workspace_id` was never set for its connection. Both its tables had
RLS **enabled but not forced**, and the runtime role owns them — so the policy
was inert for precisely the writer that mattered, and the journal recorded
whatever workspace id it was handed with nothing checking it. Migration 0138
forces the policy, which turned the latent hole into an immediate failure: the
retrieval endpoint started answering 500. The adapter now writes inside a scoped
transaction like every other store, and `record_context_pack` refuses a
workspace that is not the caller's rather than taking the parameter on trust.

**The recorded intent did not match the wire form.** `format!("{:?}").to_lowercase()`
produced `semanticrecall`, which matches neither the variant name nor the
`semantic_recall` the API accepts, so an entry could not be correlated with the
request that produced it without knowing about the mangling. It is serialised
now. Verified live: the endpoint answers 200 and the row reads

```text
intent     | semantic_recall
channels   | [{"channel": "text", "outcome": "succeeded", "candidates": 0}]
parameters | {"time_perspective": "Current", "allowed_statuses": ["Active"],
              "allowed_kinds": [], "channel_limit": 5, "token_budget": null,
              "include_explanation": false}
```

## Where MEMORY stands

Five requirements remain, and none is a case waiting to be written:

| ID | What it needs |
| --- | --- |
| RET-001, RET-002 | A hydration path that fetches an exact revision. Retrievers return candidates with content already attached; there is no "resolve this revision reference" step to test |
| RET-004 | A retriever plus RLS under the runtime role — a database test, not a unit case |
| RET-011 | An invalidation mechanism. Nothing propagates supersession into retrieval results |
| RET-015 | Classification filtering at the hydration boundary. `ContextItem::source_classification` exists; nothing filters on it |

## Test results

Full workspace suite against a live PostgreSQL 17: **819 passed, 0 failed, 127
suites, exit=0** (817 before this work).

Profiles:

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      58 passed (51 executed,  7 attested),   5 skipped
trusted     67 passed (51 executed, 16 attested), 132 skipped, 0 N/A
```
