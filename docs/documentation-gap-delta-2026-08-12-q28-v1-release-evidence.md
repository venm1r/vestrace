# Documentation Gap Delta — Q28 Exact-Environment v1.0 Release Evidence

**Status:** implemented as a bounded additive application gate; not a v1.0 qualification claim

## Authority

- `docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md`, sections 7, 9, and 10
- `docs/adr/0008-v1-trust-is-a-qualification-contract.md`
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`, exact target and release evidence requirements
- Q25 Trusted release approval, Q26 capability restoration, and Q27 crypto/provider qualification deltas

## Implemented contract

`V1ReleaseEvidenceService` evaluates an `ExactEnvironmentReleaseTarget` against
`ExactEnvironmentReleaseEvidence`. The target records the exact release version,
source revision, build digest, configuration digest, environment manifest,
manifest digest, schema versions, and claimed profile.

The evidence must contain:

- matching exact identity and schema values;
- an approved Q25 release decision;
- a passed runtime qualification decision bound to the target manifest;
- a passed Q27 crypto/provider decision;
- passed recovery and deterministic fault-suite decisions;
- at least one allowed Q26 capability-restoration decision;
- unique non-blank evidence references and published non-blank limitations.

Missing decisions and unavailable evidence fail closed. `V1ReleaseEvidenceProbe`
is an adapter seam for collecting real deployment evidence; the pure evaluator
does not claim to implement Docker, KMS, HSM, Vault, or provider backends.

## Verification

- RED test observed before implementation.
- Focused tests cover unavailable evidence, a complete typed-decision pass, and exact build identity drift.
- Formatting, workspace tests, no-run compilation, and scoped diff checks remain required before commit.

## Explicit non-claims

This slice does not create a v1.0 certificate, execute live deployment
qualification, provide production key custody, or turn local fixtures into
environment evidence. The final claim remains deployment-specific and requires
fresh Docker/runtime/provider execution with the exact supported target.
