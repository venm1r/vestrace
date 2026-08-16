# IDW-010 Build-Verified Evidence Design

**Date:** 2026-08-16  
**Status:** approved design  
**Scope:** close only `IDW-010` by adding a truthful compile-time evidence source to the conformance system

## 1. Problem

The TRUSTED conformance profile currently reports 199 requirements: 195 passed, 0 failed, 4 skipped, and 0 not-applicable. `IDW-010` is one of the four skips. Its normative statement is:

> `SharedMemoryRef` must not be substitutable for a local `MemoryId`.

The existing domain type already preserves the intended boundary: `SharedMemoryRef` is a distinct type carrying source workspace, source memory, source revision, grant revision, and source generation. It exposes an explicitly named `source_memory_id()` accessor but implements no implicit or trait-based substitution into `MemoryId`.

The conformance runner cannot prove the absence of a conversion at runtime. Returning an executed PASS from an ordinary runtime case would therefore misstate the evidence source, while leaving the requirement skipped prevents the TRUSTED gate from recording a guarantee enforced by the compiler.

## 2. Goal

Add a first-class `build_verified` evidence origin and use it to close `IDW-010` only when the compiled domain crate contains a mutation-sensitive negative trait assertion proving that `SharedMemoryRef` cannot substitute for `MemoryId`.

The accepted result is:

```text
199 total
196 passed
0 failed
3 skipped
0 not applicable
```

The remaining skips must be exactly `IDW-014`, `QUAL-010`, and `REC-016`. The command must still exit non-zero because TRUSTED remains open.

## 3. Non-goals

This slice does not:

- implement the `IDW-014` cross-workspace sharing adapter or any public sharing surface;
- implement cognitive qualification scenarios for `QUAL-010`;
- implement destructive recovery or forensic capture for `REC-016`;
- add production release-evidence collection, signing, KMS/HSM/Vault/OS-keyring custody, or exact-environment qualification;
- claim that TRUSTED or Vestrace v1.0 is complete;
- remove or rename `source_memory_id()`, which explicitly exposes source identity rather than substituting the shared reference itself as a local identifier;
- modify or commit preserved console `dist`, `node_modules`, nginx, `target`, or `graphify-out` state.

## 4. Evidence model

### 4.1 Case origin

`CaseOrigin` gains `BuildVerified`, serialized as `build_verified`. Its meaning is narrower than `Executed`: compilation evaluated a type-level invariant and the binary could not exist if the invariant were false. It is also stronger and more specific than `Attested`: no human statement stands in for the check.

Old serialized reports remain readable by the new implementation:

- a missing `origin` continues to default to `attested`;
- a missing `passed_build_verified` summary field defaults to zero.

The extension is additive for current consumers. Older strict consumers that model `CaseOrigin` as a closed enum will need an update before reading a new report containing `build_verified`; the design does not mislabel that direction as backward compatible.

### 4.2 Summary semantics

`ConformanceSummary` gains `passed_build_verified`. Existing `passed_executed` keeps its current runtime meaning and must not include build-verified results. The attested count remains derivable as:

```text
passed - passed_executed - passed_build_verified
```

This keeps runtime execution, compiler proof, and human attestation distinguishable in both human-readable and JSON output.

### 4.3 Hard-gate origin

`EvidenceOrigin` gains `LocalBuildVerified`. The CLI maps `CaseOrigin::BuildVerified` to this origin when assembling hard-gate evidence.

The governance/federation hard gate accepts local build verification only for `VerificationClass::Domain`. A build-verified result used for Security, Stateful, Behavioral, Recovery, Evidence, Interop, Fault, or Static requirements is rejected with a dedicated failure. Static requirements retain their existing local-attestation path; runtime-relevant classes still require execution.

This restriction is deliberately narrow. It admits the compiler-enforced domain boundary needed by `IDW-010` without creating a general escape hatch for runtime properties.

## 5. Compile-time invariant

The domain crate adds the small compile-time dependency `static_assertions`. A private assertion beside `SharedMemoryRef` proves the absence of each substitution path:

- `SharedMemoryRef: Into<MemoryId>`;
- `MemoryId: From<SharedMemoryRef>`;
- `MemoryId: From<&SharedMemoryRef>`;
- `SharedMemoryRef: Deref<Target = MemoryId>`;
- `SharedMemoryRef: AsRef<MemoryId>`;
- `SharedMemoryRef: Borrow<MemoryId>`.

The assertion is compiled in normal domain builds, not hidden behind `cfg(test)`. Consequently, a release binary that reports `IDW-010` as build-verified was built under the same invariant.

The proof concerns substitution of the reference itself. Explicitly calling `source_memory_id()` remains allowed and remains visibly source-qualified at the call site. The slice does not claim that a caller can never misuse an explicitly extracted source ID; `IDW-014` and the later sharing adapter must enforce runtime workspace isolation.

## 6. Conformance case and data flow

The domain conformance runner registers one case for `IDW-010`:

1. Rust compiles `SharedMemoryRef` together with the negative trait assertion.
2. Compilation fails if a prohibited conversion or coercion exists.
3. A successfully built runner can return `PASS` with `CaseOrigin::BuildVerified` and a non-empty evidence reference naming the domain type and assertion source.
4. `ConformanceReport` counts the result in `passed` and `passed_build_verified`, but not `passed_executed`.
5. The CLI maps the case to `EvidenceOrigin::LocalBuildVerified` for hard-gate evaluation.
6. The CLI's fallback evaluator no longer emits the historical `IDW-010` skip because a registered case owns the requirement.

No external receipt, source-tree scan, build script, or production evidence collector is introduced. The proof travels with the compiled conformance implementation.

## 7. Failure handling

- Adding any prohibited conversion makes compilation fail at the assertion rather than allowing a false PASS.
- A build-verified result without an evidence reference is rejected by the existing evidence-reference rule.
- A `LocalBuildVerified` item attached to a non-Domain requirement is rejected by the hard gate.
- A duplicated, failed, skipped, inconclusive, or not-applicable item retains the existing hard-gate failures.
- Old reports without the new summary field read as zero build-verified passes.
- TRUSTED remains a failing/open command while any of the three remaining requirements is skipped.

## 8. Test strategy

### 8.1 RED

Before implementation, focused tests require:

- `CaseOrigin::BuildVerified` and `passed_build_verified`;
- a registered passing `IDW-010` case with build-verified origin;
- hard-gate acceptance for the Domain requirement and rejection for a non-Domain requirement;
- TRUSTED counts of 199/196/0/3/0 with the exact remaining skip IDs.

These tests must fail against the current 195/4 baseline.

### 8.2 GREEN

Implement the minimal evidence-model extension, compile-time assertion, case registration, CLI mapping, summary rendering, and historical skip removal. Then run focused tests for:

- old/new origin serialization;
- old summary deserialization defaulting `passed_build_verified` to zero;
- exact summary accounting across executed, build-verified, and attested passes;
- accepted Domain build evidence;
- rejected Security or Stateful build evidence;
- the registered `IDW-010` result and evidence reference;
- CLI JSON and human-readable output.

### 8.3 Mutation proof

Temporarily add `From<SharedMemoryRef> for MemoryId` inside the domain crate. A focused compilation must fail at the negative assertion. Remove the mutation with `apply_patch`, rerun the same command, and require GREEN. No mutation commit is created.

### 8.4 Final gates

The slice must pass:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --no-run
cargo test --test v1_release_evidence -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
```

It must also rerun the CLI truth gate and parse:

```text
199 total / 196 passed / 0 failed / 3 skipped / 0 not-applicable
IDW-014, QUAL-010, REC-016
```

The exact task range must pass `git diff --check`. The cached diff must remain only the pre-existing nginx rename, and preserved generated/cache status and hash must remain unchanged.

## 9. Commit and review boundaries

The design document is committed independently before implementation planning. The later implementation plan must split evidence types/gate policy, compile-time proof/case registration, CLI integration, and final evidence into reviewable TDD tasks where each task can be independently rejected.

Implementation commits may include only explicitly named Cargo manifests/lockfile, conformance/domain/CLI source, focused tests, and the paired design/plan/report amendments. Every task receives independent review, followed by a broad review of the complete `IDW-010` range.

## 10. Truthful completion statement

After every accepted gate passes, the slice may state:

> IDW-010 build-verified evidence complete; TRUSTED now reports 196 passed and remains open on IDW-014, QUAL-010, and REC-016.

It must not state that TRUSTED, production qualification, or Vestrace v1.0 is complete.
