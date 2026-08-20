# Mounted Secret Store Crypto Custody Design

**Date:** 2026-08-19
**Status:** approved design, not yet implemented.
**Scope:** a second key custody adapter, so crypto adapter qualification can be
attempted at all.
**Non-claim:** this design qualifies no profile, makes no release claim, and
does not make the compose deployment a supported production configuration.

## 1. Problem

`CryptoAdapterQualificationService::evaluate` refuses any target whose custody
is not production:

```rust
if !target.custody.is_production() {
    failures.push(CryptoQualificationFailure::ProductionCustodyRequired);
}
```

`CryptoCustody::LocalDevelopment` is the only custody that accepts the provider
`local-file`, and every production custody rejects it. The only `impl
KeyProvider` in the codebase is `LocalFileKeyProvider` in
`crates/vestrace-cli/src/commands/conformance.rs`, and the only
`CryptoAdapterQualificationProbe` is a fixture in
`tests/crypto_adapter_qualification.rs`.

So `CryptoQualificationMissing` is not an unfinished task in the v1.0 release
gate — it is unreachable. No argument to any command makes it pass, because
there is no production custody to name.

The key in question is the **release signing key** (`KeyPurpose::Signing`, scope
`release`) — the one that signs capability manifests and qualification bundles.
It is a release-machine and CI concern. The running deployment does not need to
hold it, and this design does not give it one.

## 2. Goal

One production custody adapter — `CryptoCustody::MountedSecretStore` — whose
key material is delivered by the orchestrator (Docker secret, Kubernetes
Secret, tmpfs mount), together with a probe that collects all six qualification
checks by **executing** them.

Success is not "crypto qualification passes". Success is that a passing crypto
qualification means something a reader can check, and that a failing one names
which property failed.

## 3. What makes this not a rename

The risk this design exists to avoid: declaring a provider string
`mounted-secret-store`, keeping `local-file` behaviour, and calling the
qualification passed. That is the self-assertion this project refuses
everywhere else, one layer up.

With `local-file` the caller names a path and receives the bytes. The provider,
key id, version, purpose, scope and algorithm all live in a `KeyReference` the
**caller constructed**, so the adapter's checks compare the caller's word
against the caller's word:

```rust
if request.purpose() != key.scope() { /* denied */ }
```

Both sides of that comparison are supplied by whoever is asking. A caller that
wants a different scope writes a different `KeyReference`.

In the mounted store, identity, scope, purpose, algorithm and lifecycle state
are **declared by the store** and read from it. The request is checked against
what the store says, which is a statement the caller cannot edit, so the
adapter can refuse. That difference — the ability to say no to the caller — is
what distinguishes production custody here, and it is the property the tests
must pin.

## 4. Store contract

```text
<root>/<key_id>/
    scope                 # the scope this key may be resolved for
    purpose               # signing
    algorithm             # ed25519
    <version>/
        state             # active | rotating | retired | revoked | destroyed
        private.pkcs8     # PKCS#8 v2, as ring's Ed25519KeyPair::from_pkcs8 accepts
        public.bin        # 32 raw public key bytes
```

The store is provisioned by the operator. This design adds no key generation
tooling.

### 4.1 Resolution rules

`MountedSecretStoreKeyProvider { root }` implements `KeyProvider` under the
provider name `mounted-secret-store`. `resolve(key, request)` refuses when:

| Condition | Error |
|---|---|
| `key.provider() != "mounted-secret-store"` | `Denied` |
| `key_id` directory absent | `Unavailable` |
| version directory absent | `Unavailable` |
| stored `state` is not `active` or `rotating` | `NotUsable` |
| stored `scope` differs from `key.scope()` | `Denied` |
| stored `scope` differs from `request.purpose()` | `Denied` |
| stored `purpose` differs from `key.purpose()` | `Denied` |
| stored `algorithm` differs from `key.algorithm_suite()` | `Denied` |
| `request.authorization_ref()` is blank | `Denied` |

The scope is compared against **both** the reference and the request, because
they are two different claims: one is what the caller says the key is for, the
other is what this particular resolution is for, and the store must agree with
each independently.

Path handling: `key_id` and `version` are single path segments and are rejected
if they contain a separator or `..`, so a key id cannot walk out of the store.

## 5. Probe

`MountedStoreCryptoProbe` lives in infrastructure (which already depends on
application and on `ring`) and implements
`CryptoAdapterQualificationProbe::collect`.

| Check | How it is obtained |
|---|---|
| `Resolution` | the active version resolves and returns material |
| `ScopeIsolation` | a request naming another scope is **attempted and denied** |
| `CryptographicRoundTrip` | sign a probe payload with the resolved key, verify against `public.bin` |
| `Lifecycle` | a version whose `state` is `revoked` is **attempted and refused** |
| `Rotation` | two versions exist, the retired one is refused by `resolve`, **and the two public keys differ** |
| `SecretNonDisclosure` | provoke each refusal and scan rendered `Display` and `Debug` output for the private key bytes |

A check is recorded only when its attempt produced the expected outcome. Two of
these deserve their reasoning stated:

**Rotation compares public keys.** Two directories named `v1` and `v2` holding
the same key material would satisfy a rotation check that only counted
versions. Rotation means the key changed, so the probe compares the material's
public halves and records the check only if they differ. The retired version's
public half is read from its `public.bin` directly rather than through
`resolve`, which refuses it — the point of the check is that a key the store
will no longer hand out is nevertheless a *different* key from the active one.

**Non-disclosure is scanned, not asserted.** The private key bytes are searched
for in the rendered error text, because "the adapter does not leak the key" is
a claim about formatting code that no type prevents from regressing.

Every check contributes an evidence reference naming what was exercised, since
`CryptoAdapterQualificationService` refuses evidence with no references.

## 6. Wiring

`conformance sign` gains `--key-store-root <path>`, used when `--key-provider`
is `mounted-secret-store`; `--private-key-file` remains the `local-file` path.
Exactly one of the two must be given for the chosen provider.

`conformance release` gains `--crypto-evidence`, symmetric with
`--runtime-evidence` and under the same rule: evidence that was asked for and
could not be collected is an **error**, not absent evidence. Collecting it
requires `--key-store-root`, `--key-id`, `--key-version` and `--key-scope`,
which name the key being qualified. There is no custody flag: this build
implements one production custody, and offering a choice among variants it
cannot honour would be a menu of claims rather than of behaviours.

No configuration file changes. The store root is an argument, not a config
section, because the release machine passes it per invocation and a deployment
does not hold this key at all.

## 7. Test strategy

All tests are pure filesystem work over a temporary store, so they run on
Windows and in a Linux container alike, with no external service.

**RED/GREEN per refusal.** One case per row of the resolution table: unknown key
id, unknown version, retired state, revoked state, scope mismatch against the
reference, scope mismatch against the request, purpose mismatch, algorithm
mismatch, blank authorization reference, and a traversal attempt in `key_id`.

**Probe cases.** One case per qualification check, each asserting the check is
present when the store supports it and absent when the store does not — a probe
that always reports six checks is a probe that checks nothing. Specifically:
a store with one version must not yield `Rotation`; a store whose two versions
hold identical material must not yield `Rotation`; a store whose revoked
version still resolves must not yield `Lifecycle`.

**Mutation proof.** For each of the six checks, break the property in the
adapter and confirm the corresponding probe case fails and the others do not.

**End-to-end.** `conformance release --crypto-evidence` against a temporary
store shows `crypto_qualification_missing` gone from the failure list, and a
store with a revoked-only key shows `crypto_qualification_failed`. This mirrors
the three-outcome proof already used for runtime evidence.

## 8. Non-goals and limitations

- The workspace secrets master key (`VESTRACE_SECRETS__MASTER_KEY`,
  `KeyPurpose::Storage`) is untouched. It is a different key with a different
  purpose, and its custody remains local-file.
- No KMS, HSM or Vault adapter. `CryptoCustody` keeps those variants and this
  design supplies none of them.
- No key generation, rotation orchestration or distribution. The operator
  provisions and rotates the store; this reads it and reports what it finds.
- Filesystem properties are deliberately **not** checked — no permission bits,
  no refusal of non-ephemeral paths. Those checks fork per platform, and a
  check that cannot run on the machine that develops it is a check nobody
  verifies. This is a real limitation: a store on an ordinary disk readable by
  other users satisfies this adapter, and the custody guarantee is the
  orchestrator's to make.
- The mounted store is the weakest of the production custodies. It is honest
  about being a delivered file rather than a key that never leaves a boundary;
  a KMS where signing happens inside the service is strictly stronger and is
  not what this builds.
- Passing crypto qualification closes one of five missing evidence sources in
  the v1.0 release gate. Release approval, recovery qualification, fault suite
  and capability restoration remain without producers, so the gate still cannot
  pass after this work.
- Crypto evidence is not bound to the build being released. Runtime
  qualification is bound — `collect_runtime_qualification` compares the
  deployment's own account against `target_manifest` — but crypto
  qualification performs no equivalent comparison: `--crypto-evidence` with
  any well-formed store clears `crypto_qualification_missing` for any
  manifest. Nothing compares the qualified key's declared providers against
  the manifest's `crypto_providers`, and nothing compares the qualified key
  against the key that actually signed the manifest and the bundle. This is a
  real gap, not a footnote.
