# A hundred facts nobody could read

**Date:** 2026-08-15
**Scope:** the recurring unreadable-evidence defect, swept across the domain, and
the test that stops it coming back.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Four consecutive slices found the same shape and fixed one instance each:
`DataExportPlan::object_revisions`, the recovery types' evidence,
`QualificationBaseline`'s invalidation reason, the external effect receipt's
response digest. Each time I wrote that it was a pattern rather than an accident,
and each time I patched the instance in front of me.

A sweep of the domain found **100 such fields across 26 types**.

```text
before:  26 types with unreadable fields, 100 fields
after:    4 types,                          7 fields — all of them machinery, each named
```

## Why this is a specific kind of dishonesty

A private field that is validated on construction and never exposed makes a type
*look* like it records something. The author sees the field, the constructor
checks it, the requirement is satisfied to their eye. Everyone else has a type
that took the value and cannot be asked for it.

It hurts EVIDENCE-class requirements most, because the whole requirement is that
the record survives. The worst case here was `AuditIntegrityEntry`: a
hash-chained audit record with nine private fields — sequence, actor, action,
resource, content digest, previous digest, entry digest, timestamp — and **no
`impl` block at all**. Verification lived entirely inside `AuditIntegrityChain`,
so nothing outside one file could check the chain, rebuild it, or say what an
entry was about. An audit trail is precisely the thing whose value is that
somebody else can read it.

Others worth naming:

- `ExportBundle` — the object revisions, provenance references and object hashes
  an export carries. What left, unreadable.
- `DeletionVerification` — the references checked and the evidence gathered.
  Proof of deletion, unreadable.
- `DeclassificationDecision` — who decided, who approved, on what evidence, when.
- `QualificationBundle::suite_version` — QUAL-013 requires a bundle to *contain*
  the suite version. The case I wrote for it two slices ago checked six other
  fields and could not check that one.

Eighty-four accessors were generated, three impl blocks written by hand, and
QUAL-013 now checks the field it was always about.

## The check

`crates/vestrace-domain/tests/evidence_is_readable.rs` walks every `pub struct`
in the domain, finds private named fields, and asserts each has a public method
whose name contains it. A field that is genuinely machinery goes on an exception
list **with a reason**, and a second test asserts the reasons are not one-word
placeholders.

Four exceptions, all state rather than record: the conformance runner's case
list, the repair budget's configured limits, the classification policy's
unclassified switch.

Verified failable by deleting `AuditIntegrityEntry::previous_digest`:

```text
these fields are recorded and cannot be read, so anything that depends on them
being kept is unverifiable from outside the type:
  AuditIntegrityEntry.previous_digest (src/trust.rs)

Add a reader, or add the field to INTERNAL_STATE with the reason it is machinery
rather than a record.
```

## What the scan says about the other crates

The same scan, run outside the domain:

```text
vestrace-application     45 types, 119 fields
vestrace-infrastructure  57 types,  69 fields
vestrace-http             4 types,  44 fields
```

Those numbers are mostly **not** this defect. Application and infrastructure
types are overwhelmingly services holding injected dependencies —
`memory_repo`, `store`, `pool`, `authorization` — which are machinery by
definition, and a rule requiring readers for them would produce noise rather
than signal. That is why the guard is scoped to the domain, where the records
live.

But not entirely noise: `RestorationEvidence`,
`CryptoAdapterQualificationTarget` and `CryptoAdapterQualificationEvidence` are
genuine evidence types with the same defect — a decision to hand capabilities
back after a trust incident whose grounds could not be inspected, and a crypto
qualification that could not say which provider, key, version or algorithm it
examined. Those are fixed here by hand. The rest of the application layer is not
covered by the guard, and that is a judgement rather than a proof.

## What this does not do

- **A reader is not an invariant.** This asserts that what a type records can be
  asked for. It says nothing about whether the value is meaningful, whether
  anything reads it, or whether the record is complete.
- **The guard is a source scanner, not a compiler plugin.** It matches text: a
  method whose name merely contains the field's counts as a reader, which is
  what lets `recovery_provenance` read `provenance`, and is also how a
  sufficiently odd naming choice could slip past it.
- **Eighty-four of the accessors were generated.** They are uniform and
  unexciting on purpose; a generated getter is exactly as good as a hand-written
  one and I have not pretended each was a considered decision. The curation was
  in choosing *which* types are records, and that list is in the script's
  history rather than in the code.
- **Nothing here closes a conformance requirement.** The gate is unchanged. What
  changed is that several EVIDENCE-class requirements can now be checked by
  someone other than the type that satisfies them, and that the fifth instance of
  this defect will fail a test instead of taking a slice to find.

## Test results

Full workspace suite against a live PostgreSQL 17: **877 passed, 0 failed,
exit=0** — the two new tests are the guard and its exception check. Conformance
gate unchanged at **182 passed (172 executed, 10 attested), 17 skipped**.
