# Slice 18 — a writer for the classification nothing has ever written

**Phase:** Trust / T4 (data governance).
**Status:** plan, revised after one adversarial review that rejected the first
draft's central mechanism. Nothing here qualifies a profile or makes a release
claim.

## What was found

`MemoryRevision.classification` has existed since migration 0116 as
unconstrained `TEXT`. Two policies read it. **Nothing has ever written it:** the
two revision construction sites in `memory/services.rs` both write
`classification: None`, and neither HTTP request carries a field for it.

## Two corrections this plan is built on

**The label leg has never fired, on either path, for two independent reasons.**
No revision carries a label — and `HydrationOutcome::apply_policy` is called only
from conformance and domain tests. `PgRevisionHydrator` is constructed only in
infrastructure tests. `RetrievalCandidate` has no classification field at all,
`PgTextRetriever` reads content straight past it, and `ContextPackBuilder`
renders that content without consulting any policy while filling
`source_classification` with the **channel name**.

So slice 17b did not close a divergence between two live paths. It made the
embedding channel the *first* production consumer of `ClassificationPolicy`,
while the retrieval path it was written for still does not ask it. The delta for
17b says otherwise and is corrected separately.

**And `DeclassificationDecision` is not merely unwired — it is inapplicable.**
It operates on `Sensitivity` and `DataClassification`, not on string labels. It
cannot express a transition between two vocabulary labels, so it is not the
escape hatch the first draft assumed. (It is also not unreferenced:
`tests/t1_t8_trust.rs` constructs and applies it. The true narrower claim is
that nothing in production or conformance wires it.)

## The mechanism the review destroyed

The first draft declared an **ordered** vocabulary so that raising a label could
be allowed and lowering refused. That is unsound. `ClassificationPolicy` admits
by exact label set, not by rank threshold, and nothing requires an admission set
to be monotone under any order. With vocabulary `[internal, pii]` and admissible
`{pii}`, "raising" `internal` to `pii` turns a refusal into a disclosure. Rank
and admission are independent, and inventing an order over labels like `pii` and
`internal` — which are not nested dimensions — would have made the policy's
meaning depend on a list's sort position.

Dropping the order also dissolves the integrity problem it created: nothing
needs to bind a revision to the ordering that authorised it, because no
ordering authorises anything.

## Decisions I am making as lead

1. **The vocabulary is an unordered declared set**, `[policy.data.memory_labels]`.
   Labels are trimmed and non-blank; blanks and duplicates are refused at load.
   Absent or empty means no memory may carry a label, and stating one is refused.

2. **The classification is caller-stated, never derived.** Inferring it from
   content would be the unauditable classifier already refused in slices 16 and
   17b.

3. **A label is set at creation and cannot be changed afterwards.** Revise
   inherits; stating the identical label is accepted; stating a different label,
   or clearing it, is refused. There is no correct transition rule available:
   ranks are unsound, and the domain's declassification machinery cannot express
   a label change. Refusing every change is the only honest position until a
   label-native transition mechanism exists, and the error says exactly that
   rather than pointing at a mechanism that would not work.

4. **Absent stays absent and means unassessed** — not the lowest anything.
   `ClassificationPolicy` already keeps that distinction behind its own flag.

5. **The fail-open read is fixed here, not later.**
   `memory_repository`'s `row.try_get("classification").ok()` turns a decode
   failure into an absent label. Once revise inherits, a manufactured absence
   would silently clear a label without passing any check. It becomes a typed
   read whose failure is an error. This was "not in this slice" in the first
   draft and the review was right that it cannot be.

6. **The read surface must show the label.** `MemoryResponse` carries no active
   revision and no classification, so a caller cannot see what they would
   inherit. It gains the active revision's classification. A caller must be able
   to discover a label before revising against it.

7. **Idempotency fingerprints include the classification.** They are maintained
   by hand; without it, the same key with a different stated label replays the
   wrong write.

8. **`AppConfig::fingerprint` includes the vocabulary.** It enumerates policy
   fields manually and will not pick a new one up on its own.

9. **No migration and no database CHECK.** The column and type are right, and a
   CHECK against deployment configuration would freeze policy into the schema.
   Configuration-independent constraints — non-blank, trimmed — are enforced in
   the domain instead.

## Acceptance criteria

- Creating with a declared label persists it, and `MemoryResponse` returns it.
- Creating with an undeclared label is refused, naming the configured set.
- Creating with no label persists absent, distinguishable on read from any label.
- With no vocabulary configured, stating any label is refused; an unlabelled
  write still succeeds.
- Revising without stating a classification carries the previous label forward.
  **Proven on a labelled memory.**
- Revising with the identical label succeeds.
- **Revising with any different label is refused**, and the message says no
  label transition mechanism exists rather than naming one that cannot express it.
- Revising to clear a label is refused on the same grounds.
- **A decode failure on the classification column fails the read**, and does not
  surface as an absent label. Proven by a corrupted or type-mismatched value.
- **The embedding gate actually refuses a labelled memory whose label is not
  admissible.** This is the headline: the first test in this repository to
  exercise that leg with real data, end to end through the governed provider.
- Two identical idempotency keys with different stated classifications do not
  replay each other's write.
- Two configurations differing only in the vocabulary produce different
  `AppConfig::fingerprint` values.
- A concurrent labelled revise commits exactly one revision, leaves no partial
  label change, and gives the loser a revision conflict rather than a raw
  storage uniqueness error.
- Faking the inheritance to always write `None` makes a test fail.
- `cargo fmt`, clippy clean; conformance 199/199 and fault suite `failures=2`
  unchanged; full `vestrace-infrastructure` suite passing on real PostgreSQL;
  compose smoke passing **serially** with the model switch on.

## Explicitly not in this slice

- **Wiring retrieval.** `apply_policy` has no production caller,
  `RetrievalCandidate` carries no label, and `ContextPackBuilder` consults no
  policy. Making labelled content real while that path still discloses it is a
  genuine consequence of this slice, stated here rather than discovered later:
  **this slice makes labels real for the embedding boundary and not for
  retrieval.** Wiring retrieval is the next slice and should follow immediately.
- `ContextPackBuilder` filling `source_classification` with a channel name. A
  field named for classification carrying something else, found in review.
- A label-native transition or correction path, without which a mislabelled
  memory cannot be relabelled. This is a real trap for a user and it is named:
  the only remedy today is a new memory.
- Backfilling labels onto existing revisions.
- Redaction and minimisation, and the duplicate rule in `should_redact`.
