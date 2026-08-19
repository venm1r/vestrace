# A gate nobody could run

**Date:** 2026-08-19
**Scope:** the v1.0 release evidence gate, and what it turns out to be waiting
for.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim.

```text
conformance gate: 199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
release gate:     failed — 5 evidence sources with no producer
suites:           vestrace-cli 45 → 49, domain+application 365, 0 failed
```

## The question that could not be asked

With the conformance gate at 199/199 the obvious next question is what a v1.0
release still needs, and the roadmap answers it in two halves: no unclosed
applicable MUST for the profile — done — and *TRUSTED passes for a specific
supported deployment configuration*, which nothing had tried.

So the cheapest useful act was to try it. The reachable half worked end to end:
a release build with `VESTRACE_SOURCE_REVISION` compiled in produced a manifest
whose source revision it was never told on the command line, a bundle bound to
that manifest with 199 evidence entries and 9 published limitations, Ed25519
signatures over both, and `conformance verify` returning **five of seven**:

```text
bundle_status  bundle_target_binding  manifest_integrity  signature  signer_trust   passed
migration_history  runtime_database                                                 failed
```

Both failures were the host process having no route to the database, not a
defect: the running deployment's own runtime role is `rolsuper=f`,
`rolbypassrls=f`, with 82 migrations applied through `0152`.

Then the actual release decision, and it could not be reached at all:

```text
ReleaseApprovalService              cli=0 http=0
V1ReleaseEvidenceService            cli=0 http=0
CapabilityRestorationService        cli=0 http=0
CryptoAdapterQualificationService   cli=0 http=0
evaluate_fault_suite                cli=0 http=0
evaluate_runtime_qualification      one call site
```

Five of the six evaluators the v1 gate consumes had **no production call site**
— defined, exported, and exercised only by `tests/v1_release_evidence.rs`. The
gate was not failing. Nothing invoked it, so it had no result to fail with, and
the sentence "v1.0 needs a release decision" was a claim no build could check.

## What the command decides, and what it refuses to decide for you

`conformance release` reads the manifest and the bundle and asks
`V1ReleaseEvidenceService`. Two decisions in that wiring are load-bearing.

**The evidence identity comes from the bundle, never from the manifest a second
time.** The gate compares target identity against evidence identity; sourcing
both from one file would make the comparison a tautology, and every release
would agree with itself about which build it qualified — including one
qualified against a different build entirely. Mutation-proved: substituting the
manifest's identity fails the foreign-build case *and only that case*.

**The profile comes from the bundle.** A bundle earns a profile by passing that
profile's requirements; reading the operator's `--profile` for both sides would
let a `core` bundle release a `trusted` claim. Mutation-proved the same way.

**No evidence is filled in.** Five sources are reported as missing because they
are missing. A gate that supplied a default for evidence nobody collected would
be the self-assertion this project refuses everywhere else, one layer higher up.

## Runtime evidence, and the difference between two kinds of nothing

`--runtime-evidence` collects runtime qualification from the configured
database. Asking for it and not getting it is an **error**, not absent evidence:
"nobody looked" and "the environment could not be reached" are different facts
about a release, and only the first is `missing`. Reporting an unreachable
database as absent would let a release read as merely incomplete when the
environment it claims to qualify could not be reached at all. The failure names
the database with its credential removed.

Verified against an isolated PostgreSQL 17 provisioned by this repository's own
`docker/postgres/init-runtime-role.sh`, migrations run as the restricted role:

```text
no flag                        runtime_qualification_missing
restricted role                (absent — collected and passed)
same database, superuser       runtime_qualification_failed
```

The third line is the one that matters. Without it the check would be satisfied
by any reachable database, and "we collected runtime evidence" would mean only
that something answered on port 5432.

## The blocker this exposes

The gate is now runnable and **cannot pass**, for a reason that is structural
rather than unfinished:

```rust
if !target.custody.is_production() {
    failures.push(CryptoQualificationFailure::ProductionCustodyRequired);
}
```

`CryptoCustody::LocalDevelopment` is the only custody that accepts the provider
`local-file`, and every production custody rejects it. The only `impl
KeyProvider` in the codebase is `LocalFileKeyProvider`, and the only
`CryptoAdapterQualificationProbe` is a test fixture. No argument makes crypto
qualification pass; a second key custody adapter has to exist first.

The compose deployment cannot be the qualified configuration either, and says so
itself: fixed public admin token, fixed public master key, and a comment stating
it "does not pass production crypto qualification, by design". `qualification.*`
is not enabled there, so the deployment never qualifies itself at startup.

## What this does **not** do

It does not make the gate passable, and the four remaining evidence sources —
release approval, recovery qualification, fault suite, capability restoration —
still have no producer; the command names their absence and does not supply it.
The runtime component is hardcoded to `Server`, so a worker cannot be asked.
The live collection path is verified by hand and **not by an automated test**:
`crates/vestrace-cli/tests` has no database harness, and the four cases in
`v1_release_gate_cli.rs` cover the failure path only — the passing path is
evidence in this document, which is weaker than a test and is not claimed to be
one. The artifacts produced while probing (`target/v1.0-dry-run/`) describe the
local-development compose configuration, are unsigned by any custodied key, and
are not a release candidate. No baseline is published and no bundle persisted.

Finally, this log itself is behind: the three slices that closed `IDW-014`,
`REC-016` and `QUAL-010` and took the conformance gate to 199/199 have a report
under `docs/superpowers/reports/` and no entry here. This delta does not
backfill them.
