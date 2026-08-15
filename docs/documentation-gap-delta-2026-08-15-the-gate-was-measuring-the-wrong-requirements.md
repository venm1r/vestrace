# The gate was measuring the wrong requirements

**Date:** 2026-08-15
**Scope:** the conformance registry against the normative invariants catalogue.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate:   192 passed (184 executed, 8 attested), 0 failed, 7 skipped — unchanged
suite:  909 → 911 passed, 0 failed
registry realigned to the specification: 122 of 199 requirements
```

## What was found

`docs/specs/vestrace-normative-invariants-v0.2.md` is the authoritative
catalogue. `crates/vestrace-domain/src/conformance/registry.rs` is what the gate
evaluates, and it is a hand-maintained copy of it.

They had drifted on **122 of 199 requirements**.

Both drifted fields are load-bearing:

- **Class decides what evidence is admissible.** The hard gate admits
  `EvidenceOrigin::LocalAttested` — somebody writing down that they read the
  code — only where the class is `Static`. Six requirements were `Static` in the
  registry and something else in the catalogue: CAP-001, GOV-002, HLT-001,
  QUAL-008, QUAL-011, QUAL-016. CAP-001 is *"runtime authorization is determined
  by effective capability/policy, not role name"* — MUST/SECURITY in the
  specification, and the gate would have accepted an attestation for it.
- **Level decides whether a profile can close without a requirement.** Five
  MUSTs were recorded as SHOULD, among them **IDW-012** — "federation trust and
  identity recognition must not by itself permit data disclosure" — and
  **QUAL-014**, on invalidating qualification after a material environment,
  provider, policy, crypto or schema change.
- **Seventeen `MUST NOT`s were flattened to `MUST`**, losing the polarity of the
  obligation.

Seven SHOULDs were recorded as MUST, which is the harmless direction and still
not what the specification says.

**No requirement was actually satisfied by evidence the specification forbids.**
All six wrongly-`Static` requirements were passing as `executed` anyway. The
weakening was latent, and that is the only reason this is a near miss rather
than an incident.

## The mistake I made on the way, and what it proves

The earlier record listed CAP-001 as having "spec/registry drift (needs an ADR)"
and left it. Reading the seven remaining skips against the registry, I found
IDW-010's registry statement — *"grant and mount operations must be auditable"* —
had nothing to do with its skip reason, which is about `SharedMemoryRef` not
being substitutable for a local `MemoryId`. I concluded the skip was wrong,
wrote an executed case for auditability, and closed the skip. The gate went to
193 passed, 6 skipped, and the case was mutation-proved.

Then I checked the catalogue:

> **IDW-010 | MUST NOT | DOMAIN | SharedMemoryRef не должен подставляться как
> local MemoryId.**

The skip was right. The **registry statement** was the drifted one, and I had
closed a requirement against a sentence the specification does not contain —
committing precisely the defect that
[`a-skip-that-described-the-wrong-requirement`](documentation-gap-delta-2026-08-15-a-skip-that-described-the-wrong-requirement.md)
was written about, four months of gate work later. Reverted; the gate is back to
192/7 and IDW-010's original skip is restored verbatim.

The lesson is not "check the catalogue". It is that **three sources can disagree
— catalogue, registry statement, and skip reason — and I had been reading two of
them.** That is what turned a single-requirement fix into a systematic one.

## What changed

Level and class are regenerated from the catalogue for all 199 requirements,
correcting 122. Both fields already had the full vocabulary in their enums —
`MustNot`, `ShouldNot`, every class — so the drift was never a representational
limit, only an unchecked copy.

A guard makes it mechanical:
[`the_registry_matches_the_specification.rs`](../crates/vestrace-domain/tests/the_registry_matches_the_specification.rs).
Two assertions, kept apart because their failures mean different things — every
requirement carries the catalogue's level and class, and nothing admits an
attestation the specification does not classify `STATIC`.

Mutation-proved by putting CAP-001 back to `Static`: both fail, the second with
*"CAP-001 is SECURITY in the catalogue"* — the exact drift that had been sitting
in the record as "needs an ADR". It needed a test.

## What this does not do

- **It does not catch statement drift, which is the worse half.** The catalogue
  is in Russian and the registry carries an English paraphrase; no test can tell
  whether a paraphrase is faithful. IDW-010 proves the failure mode is real and
  live, and I found it by reading, not by testing. **Other requirements may
  carry statements that describe something else, and nothing here would say so.**
  Auditing 199 paraphrases against the catalogue needs a reader and is the
  obvious next slice.
- **It does not revisit the skip reasons.** Four of the six remaining skips were
  written against catalogue statements and are fine; the CAP-005 and CAP-012
  skip texts match their *registry* statements, which the corrected classes now
  make questionable. I have not re-read them against the catalogue, and I am not
  going to claim they are right because the gate is green.
- **No skip was closed.** The one I closed I reverted. The seven remain:
  CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010, REC-016, RET-004.
- **The registry is still a copy.** Generating it from the catalogue at build
  time would remove the class of defect entirely rather than detecting it. That
  is a bigger change and the guard is the cheap version of it.
- **Nothing re-derives the profile closures.** Corrected levels change which
  requirements a profile must close over; the gate reports the same numbers, but
  I have not audited whether any profile's MUST-set changed shape.

## Test results

Full workspace suite against a live PostgreSQL 17: **911 passed, 0 failed** (909
before; the increase is this guard's two tests). Conformance gate: **199 total,
192 passed (184 executed, 8 attested), 0 failed, 7 skipped** — unchanged, and
now measured against the requirements the specification actually states.
