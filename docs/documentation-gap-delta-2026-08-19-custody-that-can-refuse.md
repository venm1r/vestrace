# Custody that can refuse

**Date:** 2026-08-19
**Scope:** a second key custody adapter, and the crypto qualification evidence
the v1.0 release gate had no way to obtain.
**Status:** committed delta on `feat/mounted-secret-store-custody`. It qualifies
no profile and makes no release claim.

```text
conformance gate: 199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
release gate:     failed — 5 evidence sources with no producer, down from 6
suites:           vestrace-cli 49 → 53, mounted store 20 (new file), domain+application 365, 0 failed
```

## The check no argument could make pass

The previous delta made the v1 gate runnable and ended on a blocker it could
name and not move:

```rust
if !target.custody.is_production() {
    failures.push(CryptoQualificationFailure::ProductionCustodyRequired);
}
```

`CryptoCustody::LocalDevelopment` is the only custody that accepts the provider
`local-file`, every production custody rejects it, the only `impl KeyProvider`
in the codebase was `LocalFileKeyProvider`, and the only
`CryptoAdapterQualificationProbe` was a fixture in
`tests/crypto_adapter_qualification.rs`. So `crypto_qualification_missing` was
not an unfinished item in the gate — it was unreachable, and no argument to any
command could change that. A second custody adapter had to exist first.

The key in question is the **release signing key** (`KeyPurpose::Signing`, scope
`release`), the one that signs capability manifests and qualification bundles.
It is a release-machine and CI concern; the running deployment does not hold it
and this slice does not give it one.

## What makes this not a rename

The failure mode this slice existed to avoid is declaring a provider string
`mounted-secret-store`, keeping `local-file` behaviour, and calling the
qualification passed — the self-assertion this project refuses everywhere else,
one layer up.

With `local-file` the caller names a path and receives the bytes. Provider, key
id, version, purpose, scope and algorithm all live in a `KeyReference` the
**caller constructed**, so every check compares the caller's word against the
caller's word:

```rust
if request.purpose() != key.scope() { /* denied */ }
```

Both sides are supplied by whoever is asking; a caller that wants a different
scope writes a different reference. In the mounted store those five facts are
**declared by the store** — `<root>/<key_id>/{scope,purpose,algorithm}` and
`<version>/{state,private.pkcs8,public.bin}` — and read from it, so the request
is checked against a statement the caller cannot edit. The adapter can say no to
the caller, and that is the whole difference. Review confirmed it holds in the
shipped code: every substantive refusal compares a store-read value against a
caller-supplied one.

| `resolve` refuses when | Error |
|---|---|
| `key.provider()` is not `mounted-secret-store` | `Denied` |
| the reference is not usable | `NotUsable` |
| `request.authorization_ref()` is blank | `Denied` |
| `key_id` or `version` is not a single path segment | `Denied` |
| the key id or version directory is absent | `Unavailable` |
| stored `purpose` differs from `key.purpose()` | `Denied` |
| stored `algorithm` differs from `key.algorithm_suite()` | `Denied` |
| stored `scope` differs from `key.scope()` | `Denied` |
| stored `scope` differs from `request.purpose()` | `Denied` |
| stored `state` is not `active` or `rotating` | `NotUsable` |

The scope is compared against **both** the reference and the request because
they are two different claims: what the caller says the key is for, and what
this particular resolution is for. The store must agree with each independently.

## The six checks, obtained by executing them

`MountedStoreCryptoProbe` records a check only downstream of an operation that
could have gone the other way. A probe that always reports six checks is a probe
that checks nothing.

| Check | How it is obtained |
|---|---|
| `Resolution` | the active version resolves and returns material |
| `ScopeIsolation` | a request naming `{scope}-not-this` is attempted and **denied** |
| `CryptographicRoundTrip` | sign `PROBE_PAYLOAD` with the resolved key, verify against the store's own `public.bin` |
| `Lifecycle` | a version the store marks `revoked` is attempted and **refused** |
| `Rotation` | a superseded version exists and its public key **differs** from the active one's |
| `SecretNonDisclosure` | the refusal's `Display` and `Debug` are rendered and **scanned** for the private bytes |

**Rotation compares key material, not directory counts.** Two directories named
`v1` and `v2` holding the same bytes would satisfy a check that counted
versions, so the probe reads both public halves and records `Rotation` only if
they differ. The superseded version's public half is read from `public.bin`
directly rather than through `resolve`, which refuses it — the point is that a
key the store will no longer hand out is nevertheless a *different* key.

## The plan predicted three mutations; the evidence corrected it

Mutation testing was meant to confirm the plan. It repeatedly did not, and each
time the evidence was right and the plan was wrong.

**Two checks were mutually redundant, so neither could be killed.** Commenting
out `resolve`'s outer `segment("key id", …)` failed no test: `declaration()`
runs its own identical check two lines later. Commenting out the inner one
failed no test either, because the outer one fires first. Whichever remains
catches the case, so the pair is unkillable while both stand — and the first
ruling on this (that the inner check was the load-bearing one) was corrected by
the implementer reporting the second result rather than adjusting the
expectation. The resolution was neither deletion nor a shrug: the redundancy
stays as defence in depth, and a case now calls `provider.declaration("../../etc")`
**directly**, which is the path the probe actually takes and on which the inner
check is the only guard. Mutating it then failed exactly that one case out of
13. The same round found the `key version` segment check pinned by nothing at
all, and added `a_key_version_cannot_walk_out_of_the_store` for it.

**A fixture made two independent checks indistinguishable.** Disabling
`declaration.scope != key.scope()` killed nothing, because the case set the
reference's scope and the request's purpose to the same value — so the *other*
scope check caught it. The two checks exist precisely because they are two
claims; that fixture could not tell them apart. Holding the request purpose at
the declared scope while the reference disagrees isolated it, and the mutation
then failed exactly one case.

**One check's recording was pinned by nothing until it was extracted.**
Recording `SecretNonDisclosure` unconditionally — dropping the `!leaked` clause
— killed no test, because every fixture refuses cleanly and nothing in the suite
leaks. Task 1's `no_refusal_discloses_key_material` does not help: it scans the
adapter's errors and never enters the probe. The scan is now a pure function,

```rust
pub fn discloses(rendered: &str, secret: &[u8]) -> bool
```

unit-tested against a string with the secret spliced into it and against a clean
refusal, with the function stubbed to `false` failing exactly that one case. The
trait-object alternative was rejected: `KeyProvider` exposes only `resolve()`
while the probe also needs `declaration()`, `versions()` and `public_key()`, so
a test double would mean widening a domain trait with store-specific concepts.

Two further corrections came from review rather than from mutation. `versions()`
and `public_key()` shipped with **zero coverage** — the plan's own test code
never called either — and were covered before the probe was built, because the
probe depends on the exact sort order and path join of both; reversing the sort
and dropping the version segment each failed exactly one case. And the
`Rotation` comment claimed the superseded version "is refused" when the code
never calls `resolve` on it and only reads its `state` label. It was corrected
in the same round: in this project a comment that overclaims what was proven is
the specific defect the slice exists to avoid.

Two probe mutations **killed more than predicted** — disabling the adapter's
scope checks, or admitting `revoked` as a usable state, also broke the adapter
cases that pin those behaviours directly — and were recorded as information
rather than adjusted for. One mutation's diagnostic was recorded and not acted
on: with the version segment check removed, `../../etc` fails as `Unavailable`
rather than `Denied` because nothing happens to exist at that path, so the
fallback is filesystem-topology-dependent and the segment check is the only
guarantee that is not.

## Against the real candidate, not a fixture

Verified by hand against the artifacts in `target/v1.0-dry-run/` (manifest
`sha256:7fe91d51…`), with a store provisioned at a scratch path declaring key
`release-signing`, scope `release`, purpose `signing`, algorithm `ed25519`, v1
active and v2 revoked with independently generated material:

```text
no flag                    6 missing — release_approval, runtime_qualification, crypto_qualification,
                           recovery_qualification, fault_suite, capability_restoration
--crypto-evidence <store>  5 missing — crypto_qualification_missing gone, and
                           crypto_qualification_failed did not replace it
absent store root          Error: crypto evidence could not be collected: … the store holds no key
                           release-signing   (non-zero exit, no release report on stdout)
conformance sign           target/v1.0-dry-run/manifest.store-signed.json, verify-signature passed
```

The third line follows `--runtime-evidence`'s rule: evidence asked for and not
obtained is an **error**, not absent evidence. Structure enforces it rather than
convention — `collect_crypto_qualification`'s `?` propagates before
`ExactEnvironmentReleaseEvidence` is constructed, so a store that cannot be read
can never reach the `None` branch that would print `crypto_qualification_missing`.
`conformance sign` takes `--key-store-root`, `--private-key-file` remains the
`local-file` path, and exactly one must be given for the chosen provider; every
other combination bails without signing. There is no custody flag: this build
implements one production custody, and a menu of variants it cannot honour would
be a menu of claims rather than of behaviours.

## What this does **not** do

**Nothing about the filesystem is checked.** Not permission bits, not
ephemerality, not who else can read the mount. A store on an ordinary disk
readable by every user on the host satisfies this adapter completely. Those
checks fork per platform and would go unverified on the machine that develops
them, so the custody guarantee remains the **orchestrator's** to make; this
adapter reads what it was given and reports what it finds.

**The mounted store is the weakest of the production custodies.** It is honest
about being a delivered file rather than a key that never leaves a boundary. A
KMS where signing happens inside the service and the private half never reaches
this process is strictly stronger, and is not what this built; `CryptoCustody`
keeps those variants and this slice supplies none of them. No key generation, no
rotation orchestration, no distribution — the operator provisions and rotates
the store, and this reads it.

**The workspace secrets master key is untouched.** `VESTRACE_SECRETS__MASTER_KEY`
is `KeyPurpose::Storage`, a different key for a different purpose, and its
custody is still `local-file`. Nothing here upgrades it.

**The release gate still cannot pass.** Four evidence sources — release
approval, recovery qualification, fault suite, capability restoration — remain
without a producer; the gate names their absence and does not supply it. The
compose deployment is still not a supported production configuration, and the
`target/v1.0-dry-run/` artifacts still describe the local-development compose
configuration: signing one of them from a mounted store does not make it a
release candidate.

**The real-candidate table above is evidence, not a test.** The committed CLI
cases prove the three outcomes — missing, collected, failed — against temporary
fixture stores; the run against the actual manifest was performed by hand and is
weaker than a test and not claimed to be one. Likewise the committed suite pins
the disclosure *scan*, not the probe's use of it: no fixture leaks, so a probe
that dropped the `!leaked` clause would still fail no committed case. The case
that proved otherwise required an adapter deliberately mutated to splice private
bytes into a refusal message, and was reverted with it — keeping it would mean
shipping either a code path capable of leaking key bytes or the `unsafe` that
`#![forbid(unsafe_code)]` denies.

**The conformance figure above is carried forward, not re-measured.** This slice
registers no requirement, closes no skip and touches no registry file — the diff
is ten files across infrastructure, the CLI, one domain type and the test crates
— but the 199 line is the previous delta's measurement, restated here because
nothing in this work could move it.

Minor findings recorded and deliberately deferred: the catch-all message in
`resolve_signing_key` is reused for an unknown `--key-provider`, so a typo'd
provider name is told it needs one of two flags, neither of which could help it;
the `"crypto evidence could not be collected: "` prefix is repeated across four
call sites rather than wrapped once; the release-gate fixture uses v1-active
with v2-revoked per a ruling made during execution without restating that
rationale inline, so a reader diffing it against the plan could suspect a typo;
and one implementer report cites a step number its own brief does not contain.
One accepted deviation from the plan's file list: `ResolvedKeyMaterial` gained a
redacted `Debug` in the domain crate, because `.expect_err()` requires one and a
redacted rendering directly serves the constraint that nothing may print private
key bytes.
