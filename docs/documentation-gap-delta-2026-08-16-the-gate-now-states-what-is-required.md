# The gate now states what is required

**Date:** 2026-08-16
**Scope:** the conformance registry's requirement *statements*, and the four
remaining skips.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate:   195 passed (187 executed, 8 attested), 0 failed, 4 skipped — unchanged
suite:  911 → 912 passed, 0 failed
```

## The audit that was owed

The previous slice realigned the registry's *level and class* to the catalogue
and said plainly that statement drift was the worse half and uncaught. This is
that audit.

Comparing all 199 registry statements against the catalogue by shared technical
tokens — identifiers and code spans survive the Russian/English boundary
intact:

```text
compared 199 statements
best match is the same number : 61
no catalogue match above 10%  : 43
```

Only **61 of 199** registry statements are even the closest match for their own
catalogue entry, and the offsets are scattered rather than systematic, so this is
not a renumbering. The token scores are low enough (10–27%) that the method
cannot prove correspondence — but it does not need to. Reading the CAP family is
enough:

| id | catalogue | registry gloss |
|---|---|---|
| CAP-002 | Default policy — deny | `CapabilityGrant` must specify resource scope, actions, constraints and expiry |
| CAP-004 | Expired capability must be rejected | `PolicyDecision` must be the single authorization verdict |
| CAP-008 | Risk categories at least `low/medium/high/critical` | Budget consumption must be checked at effect time |

And the source of at least some of them is now identified. Registry CAP-012 read
*"effective authority is the intersection of Brain capability/policy, Face-local
policy and host/organ OS rights"* — that is from the **Brain–Face–Organ System
Model**, which `docs/specs/README.md` lists as an *accepted post-v0.2 extension*,
explicitly not part of the frozen normative set. The registry had absorbed
statements from other documents under catalogue IDs.

The README also settles the authority question I should have checked a slice
earlier: the Normative Invariants Catalog is item 3 of the frozen v0.2 hierarchy
and owns the *stable requirement IDs*. The previous slice's premise held.

## What changed

`Requirement` gains `spec_statement`, carrying the catalogue's text **verbatim**
for all 199, populated mechanically. `statement` remains as an English gloss and
is now documented as not authoritative.

A third guard keeps `spec_statement` byte-identical to the catalogue.
Mutation-proved by restating CAP-002 as *"Default policy is permissive"* — it
fails naming both texts.

`conformance list` now prints the authoritative wording rather than the gloss,
because a listing somebody uses to decide what is required should say what is
required:

```text
RET-004 MUST DOMAIN  `ContextPack.used_budget` не должен превышать hard token/content budget.
CAP-012 MUST SECURITY Material change approved operation/intent должен инвалидировать approval.
IDW-010 MUST NOT DOMAIN SharedMemoryRef не должен подставляться как local MemoryId.
```

## Why the glosses were not rewritten

Replacing 199 English glosses with faithful translations is the obvious next
step and I did not take it. A translation is a paraphrase, and paraphrases are
what produced this; doing 199 of them at the end of a long session would
manufacture exactly the artifact this slice exists to distrust. The authoritative
text is now present and guarded, which makes each gloss checkable by a reader
against the line above it. **Until that reading happens, `statement` should be
treated as commentary — several are known to describe a different requirement.**

## The four remaining skips

All four match their catalogue statement, and none is closable by writing a case:

- **IDW-010** — `SharedMemoryRef` must not be substitutable for a local
  `MemoryId`. The absence of a conversion is not observable at runtime, and the
  conformance runner executes from a deployed binary with no source tree, so the
  source-scanning guard that *could* assert it cannot run there. Its class is
  `DOMAIN`, so an attestation is not admissible either — correctly.
- **IDW-014** — cross-workspace access must not require relaxing isolation.
  Sharing is constructed by no adapter, so no cross-workspace read has ever
  happened; the negative half (every table forces RLS) is separately live-tested.
- **QUAL-010** — a SHOULD over cognitive qualification, which does not exist.
- **REC-016** — a SHOULD over destructive recovery, which nothing performs.

Three need architecture. IDW-010 needs a second evidence source in the runner,
which is the same shape as RET-004's old blocker and would be a real change to
how the gate collects evidence rather than a case.

## What this does not do

- **It does not fix the glosses.** 138 of them are unverified against the
  catalogue and several are known wrong. Nothing depends on them for correctness
  now that the listing shows `spec_statement`, but they are still shipped.
- **It does not re-check the 195 passing cases.** A case citing a requirement id
  now displays the right requirement beside it; whether each case actually
  exercises that requirement is a reading nobody has done. This slice made that
  reading possible and did not perform it.
- **It does not touch the case descriptions.** Those are the third source, and
  they were written against the glosses.
- **Nothing prevents a new requirement being added with a gloss that contradicts
  its `spec_statement`.** The guard checks the authoritative field against the
  catalogue; it cannot check a paraphrase against meaning.

## Test results

Full workspace suite against a live PostgreSQL 17: **912 passed, 0 failed** (911
before). Conformance gate: **199 total, 195 passed (187 executed, 8 attested), 0
failed, 4 skipped** — unchanged.

Remaining skips: IDW-010, IDW-014, QUAL-010, REC-016.
