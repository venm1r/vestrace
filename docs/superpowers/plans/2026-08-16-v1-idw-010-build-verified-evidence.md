# IDW-010 Build-Verified Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close only `IDW-010` with a compiler-enforced, mutation-sensitive proof that `SharedMemoryRef` cannot substitute for `MemoryId`, while keeping TRUSTED truthfully open on the three remaining skips.

**Architecture:** Extend the conformance evidence model with a distinct build-time origin, enforce its narrow Domain-only hard-gate policy, compile negative trait assertions beside `SharedMemoryRef`, expose one dedicated build-verified domain case, and merge that case into the CLI report. Runtime execution, compiler proof, and human attestation remain separately counted and rendered.

**Tech Stack:** Rust 1.85 / edition 2024, `serde`, `schemars`, `static_assertions`, Clap CLI, Cargo integration tests, PowerShell, Git Bash.

## Global Constraints

- Start from task base commit `7ca671f` (`docs: design IDW-010 build-verified evidence`).
- Treat `docs/superpowers/specs/2026-08-16-v1-idw-010-build-verified-evidence-design.md` as the authority for this slice.
- Implement only `IDW-010`; do not begin `IDW-014`, `QUAL-010`, or `REC-016`.
- Do not claim TRUSTED, production qualification, or Vestrace v1.0 complete.
- Preserve the pre-existing cached rename `apps/console/nginx.conf -> apps/console/nginx.conf.template` and exclude it from every task commit.
- Preserve all existing `apps/console/dist`, `apps/console/node_modules`, `target`, and `graphify-out` state.
- Use `git -c safe.directory=E:/Soft/vestrace` for every Git command.
- Use `apply_patch` for every intentional text edit, including adding and removing the mutation probe.
- Stage and commit only the paths named by the current task. Before and after each commit, verify the cached diff explicitly.
- Each implementation task follows RED -> GREEN -> focused verification -> exact-path commit -> spec-compliance review -> code-quality review. Fix Important or Critical findings before proceeding.
- Do not run Docker for this slice; the invariant and acceptance surface are compile-time/local CLI concerns.

## Required Public Semantics

The finished slice must provide these interfaces:

```rust
pub enum CaseOrigin {
    Executed,
    BuildVerified,
    Attested,
}

pub struct ConformanceSummary {
    // existing fields...
    pub passed_executed: usize,
    #[serde(default)]
    pub passed_build_verified: usize,
}

pub enum EvidenceOrigin {
    LocalExecutable,
    LocalBuildVerified,
    LocalAttested,
    RemoteAttested,
    RemoteSelfAsserted,
}

pub enum HardGateFailure {
    // existing variants...
    BuildVerificationWhereRuntimeIsRequired {
        requirement_id: RequirementId,
    },
}

pub fn build_verified_cases() -> ConformanceRunner;
```

The exact accepted TRUSTED report is:

```text
total=199
passed=196
failed=0
skipped=3
not_applicable=0
remaining=IDW-014,QUAL-010,REC-016
exit=non-zero
```

---

## Task 1: Extend the evidence model and hard-gate policy

**Files:**

- Modify: `crates/vestrace-domain/src/conformance/mod.rs`
- Modify: `crates/vestrace-domain/src/conformance/gate.rs`

### Step 1: Add failing summary and compatibility tests

- [ ] Replace the existing two-origin accounting test in `conformance/mod.rs` with a three-origin test. Keep the test data explicit so a future accidental merge of executed and build-verified counts is visible:

```rust
#[test]
fn pass_origins_are_counted_separately() {
    let result = |case_id, family, number, origin| ConformanceCaseResult {
        case_id: case_id.into(),
        requirement_ids: vec![RequirementId::new(family, number)],
        status: CaseStatus::Pass,
        message: "passed".into(),
        evidence: Some(format!("evidence:{case_id}")),
        origin,
    };
    let report = ConformanceReport::from_results(
        None,
        vec![
            result("executed", RequirementFamily::Tmp, 2, CaseOrigin::Executed),
            result(
                "build-verified",
                RequirementFamily::Idw,
                10,
                CaseOrigin::BuildVerified,
            ),
            result("attested", RequirementFamily::Arc, 5, CaseOrigin::Attested),
        ],
    );

    assert_eq!(report.summary.passed, 3);
    assert_eq!(report.summary.passed_executed, 1);
    assert_eq!(report.summary.passed_build_verified, 1);
}
```

- [ ] Add a legacy-report deserialization test. Construct JSON without `origin` and without `passed_build_verified`; assert the result origin is `Attested` and the new summary field is zero.

```rust
#[test]
fn legacy_reports_default_new_evidence_fields_safely() {
    let json = r#"{
        "profile":null,
        "results":[{
            "case_id":"legacy",
            "requirement_ids":[{"family":"arc","number":5}],
            "status":"pass",
            "message":"legacy claim",
            "evidence":"docs/legacy.md"
        }],
        "summary":{
            "total":1,
            "passed":1,
            "failed":0,
            "skipped":0,
            "not_applicable":0,
            "passed_executed":0
        }
    }"#;

    let report: ConformanceReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.results[0].origin, CaseOrigin::Attested);
    assert_eq!(report.summary.passed_build_verified, 0);
}
```

### Step 2: Add failing Domain-only gate tests

- [ ] In `conformance/gate.rs`, add a helper that supplies passing local-executable evidence for every requirement returned by `GovernanceFederationGate::required_requirements(QualificationProfile::Trusted)`. Supply `Some("policy-v1")` only for `GOV-024` and `GOV-025`.

```rust
fn trusted_evidence() -> Vec<HardGateEvidence> {
    GovernanceFederationGate::required_requirements(QualificationProfile::Trusted)
        .into_iter()
        .map(|id| {
            let policy_version = matches!(
                (id.family, id.number),
                (RequirementFamily::Gov, 24) | (RequirementFamily::Gov, 25)
            )
            .then(|| "policy-v1".to_owned());
            HardGateEvidence::pass(
                id,
                format!("evidence:{id}"),
                policy_version,
                EvidenceOrigin::LocalExecutable,
            )
        })
        .collect()
}
```

- [ ] Add a test that replaces `MEM-001` evidence with `LocalBuildVerified`, asserts the registry class is `VerificationClass::Domain`, and requires the complete TRUSTED decision to pass.

- [ ] Add a test that replaces required `ARC-002` evidence with `LocalBuildVerified`, asserts it is not Domain, and requires exactly the dedicated failure for that requirement:

```rust
assert!(decision.failures().contains(
    &HardGateFailure::BuildVerificationWhereRuntimeIsRequired {
        requirement_id: id,
    }
));
```

`IDW-010` itself is not one of the hard gate's required `Must`/special IDs because it is `MustNot`. Do not add it to `hard_gate_requirements()` merely to exercise this origin policy; its closure is asserted by the report and CLI tasks below.

### Step 3: Run RED

- [ ] Run:

```powershell
cargo test -p vestrace-domain pass_origins_are_counted_separately -- --nocapture
cargo test -p vestrace-domain conformance::gate::tests -- --nocapture
```

Expected: compilation fails because `BuildVerified`, `passed_build_verified`, `LocalBuildVerified`, and the dedicated failure variant do not exist.

### Step 4: Implement the minimal evidence-model extension

- [ ] Add `CaseOrigin::BuildVerified`, update `Display`, and document that compilation established a type-level invariant.
- [ ] Add `#[serde(default)] pub passed_build_verified: usize` to `ConformanceSummary`.
- [ ] Count it only when `status == Pass && origin == CaseOrigin::BuildVerified`.
- [ ] Update comments that currently define all non-executed passes as attestations.
- [ ] Add `EvidenceOrigin::LocalBuildVerified` and `HardGateFailure::BuildVerificationWhereRuntimeIsRequired`.
- [ ] Extend the hard-gate origin match exactly as follows:

```rust
EvidenceOrigin::LocalBuildVerified => {
    let is_domain = registry::find(&requirement_id)
        .map(|requirement| requirement.class == super::VerificationClass::Domain)
        .unwrap_or(false);
    if !is_domain {
        failures.push(HardGateFailure::BuildVerificationWhereRuntimeIsRequired {
            requirement_id,
        });
    }
}
```

Keep `LocalExecutable`, `LocalAttested`, `RemoteAttested`, and `RemoteSelfAsserted` behavior unchanged.

### Step 5: Run GREEN and focused lint

- [ ] Run:

```powershell
cargo test -p vestrace-domain conformance::tests -- --nocapture
cargo test -p vestrace-domain conformance::gate::tests -- --nocapture
cargo fmt --all -- --check
cargo clippy -p vestrace-domain --all-targets --all-features -- -D warnings
```

Expected: all commands exit 0.

### Step 6: Commit and review Task 1

- [ ] Verify and commit only the two files:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- crates/vestrace-domain/src/conformance/mod.rs crates/vestrace-domain/src/conformance/gate.rs
git -c safe.directory=E:/Soft/vestrace add -- crates/vestrace-domain/src/conformance/mod.rs crates/vestrace-domain/src/conformance/gate.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "feat(conformance): add build-verified evidence origin" -- crates/vestrace-domain/src/conformance/mod.rs crates/vestrace-domain/src/conformance/gate.rs
```

Expected cached state after commit: only the preserved nginx rename.

---

## Task 2: Compile the negative invariant and register its domain case

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/vestrace-domain/Cargo.toml`
- Modify: `crates/vestrace-domain/src/enterprise/sharing.rs`
- Modify: `crates/vestrace-domain/src/conformance/cases.rs`

### Step 1: Add the failing conformance-case test

- [ ] In the existing `cases.rs` test module, require a dedicated runner returning exactly one result for `IDW-010`:

```rust
#[test]
fn idw_010_is_reported_only_as_build_verified() {
    let report = build_verified_cases().run_all(None);
    assert_eq!(report.results.len(), 1);

    let result = &report.results[0];
    assert_eq!(
        result.requirement_ids,
        vec![RequirementId::new(RequirementFamily::Idw, 10)]
    );
    assert_eq!(result.status, CaseStatus::Pass);
    assert_eq!(result.origin, CaseOrigin::BuildVerified);
    assert_eq!(
        result.evidence.as_deref(),
        Some("crates/vestrace-domain/src/enterprise/sharing.rs:SharedMemoryRef")
    );
    assert_eq!(report.summary.passed_build_verified, 1);
    assert_eq!(report.summary.passed_executed, 0);
}
```

Add the needed imports explicitly; do not hide this result inside `executable_cases()`.

### Step 2: Run RED

- [ ] Run:

```powershell
cargo test -p vestrace-domain idw_010_is_reported_only_as_build_verified -- --nocapture
```

Expected: compilation fails because `build_verified_cases()` does not exist.

### Step 3: Add the compile-time dependency

- [ ] Add `static_assertions = "1"` to `[workspace.dependencies]` in root `Cargo.toml`.
- [ ] Add `static_assertions.workspace = true` to `crates/vestrace-domain/Cargo.toml`.
- [ ] Run `cargo check -p vestrace-domain` once to update `Cargo.lock`. Review the lockfile delta and require it to contain only the expected dependency resolution.

### Step 4: Add the production-build negative assertions

- [ ] Beside `SharedMemoryRef` in `enterprise/sharing.rs`, add a private, non-`cfg(test)` assertion module:

```rust
mod idw_010_compile_time_boundary {
    use std::{borrow::Borrow, ops::Deref};

    use static_assertions::assert_not_impl_any;

    use super::SharedMemoryRef;
    use crate::id::MemoryId;

    assert_not_impl_any!(SharedMemoryRef:
        Into<MemoryId>,
        AsRef<MemoryId>,
        Borrow<MemoryId>,
        Deref<Target = MemoryId>
    );
    assert_not_impl_any!(MemoryId:
        From<SharedMemoryRef>,
        From<&'static SharedMemoryRef>
    );
}
```

If `static_assertions` rejects the grouped syntax under Rust 1.85, split it into one assertion per trait without weakening any of the six prohibited paths. Do not move the assertions behind tests or features.

### Step 5: Add a separate build-verified case runner

- [ ] In `conformance/cases.rs`, add `pub fn build_verified_cases() -> ConformanceRunner` next to `executable_cases()` and register only `SharedMemoryRefIsNotLocalMemoryId`.
- [ ] Implement the case with `CaseCategory::Static`, `IDW-010`, `Pass`, the exact evidence reference from Step 1, and `CaseOrigin::BuildVerified`.
- [ ] Do not add this case to `executable_cases()`: that name remains reserved for runtime execution.

```rust
pub fn build_verified_cases() -> ConformanceRunner {
    let mut runner = ConformanceRunner::new();
    runner.register(Box::new(SharedMemoryRefIsNotLocalMemoryId));
    runner
}

struct SharedMemoryRefIsNotLocalMemoryId;

impl ConformanceCase for SharedMemoryRefIsNotLocalMemoryId {
    fn case_id(&self) -> &str {
        "build-idw-010-shared-memory-ref-is-not-local-memory-id"
    }

    fn requirement_ids(&self) -> &[RequirementId] {
        &[RequirementId {
            family: RequirementFamily::Idw,
            number: 10,
        }]
    }

    fn category(&self) -> CaseCategory {
        CaseCategory::Static
    }

    fn description(&self) -> &str {
        "SharedMemoryRef cannot substitute for a local MemoryId"
    }

    fn run(&self) -> ConformanceCaseResult {
        ConformanceCaseResult {
            case_id: self.case_id().to_owned(),
            requirement_ids: self.requirement_ids().to_vec(),
            status: CaseStatus::Pass,
            message: "SharedMemoryRef has no local MemoryId substitution traits".to_owned(),
            evidence: Some(
                "crates/vestrace-domain/src/enterprise/sharing.rs:SharedMemoryRef".to_owned(),
            ),
            origin: CaseOrigin::BuildVerified,
        }
    }
}
```

### Step 6: Run GREEN

- [ ] Run:

```powershell
cargo test -p vestrace-domain idw_010_is_reported_only_as_build_verified -- --nocapture
cargo test -p vestrace-domain conformance::cases::tests -- --nocapture
cargo check -p vestrace-domain
cargo fmt --all -- --check
cargo clippy -p vestrace-domain --all-targets --all-features -- -D warnings
```

Expected: all commands exit 0.

### Step 7: Prove mutation sensitivity

- [ ] With `apply_patch`, temporarily add this prohibited implementation beside `SharedMemoryRef`:

```rust
impl From<SharedMemoryRef> for MemoryId {
    fn from(value: SharedMemoryRef) -> Self {
        value.source_memory_id()
    }
}
```

- [ ] Run `cargo check -p vestrace-domain` and require a non-zero exit whose diagnostic points to the negative trait assertion.
- [ ] Remove only the temporary implementation with `apply_patch`.
- [ ] Rerun `cargo check -p vestrace-domain` and the focused `IDW-010` test; both must exit 0.
- [ ] Record both RED and restored-GREEN command outputs for the final report. Never stage or commit the mutation.

### Step 8: Commit and review Task 2

- [ ] Verify and commit only the five files:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- Cargo.toml Cargo.lock crates/vestrace-domain/Cargo.toml crates/vestrace-domain/src/enterprise/sharing.rs crates/vestrace-domain/src/conformance/cases.rs
git -c safe.directory=E:/Soft/vestrace add -- Cargo.toml Cargo.lock crates/vestrace-domain/Cargo.toml crates/vestrace-domain/src/enterprise/sharing.rs crates/vestrace-domain/src/conformance/cases.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "feat(domain): prove shared refs are not local ids" -- Cargo.toml Cargo.lock crates/vestrace-domain/Cargo.toml crates/vestrace-domain/src/enterprise/sharing.rs crates/vestrace-domain/src/conformance/cases.rs
```

Expected cached state after commit: only the preserved nginx rename.

---

## Task 3: Integrate build verification into CLI JSON and human output

**Files:**

- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Create: `crates/vestrace-cli/tests/idw_010_build_verified.rs`

### Step 1: Add failing end-to-end CLI tests

- [ ] Create a helper that invokes the built binary and deliberately accepts the expected non-zero exit:

```rust
use std::process::{Command, Output};

fn trusted(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["conformance", "check", "trusted"])
        .args(args)
        .output()
        .expect("run vestrace conformance check trusted")
}
```

- [ ] Add a JSON test that parses `stdout` even though the command fails open. Assert:

```rust
assert!(!output.status.success());
assert_eq!(report["summary"]["total"], 199);
assert_eq!(report["summary"]["passed"], 196);
assert_eq!(report["summary"]["failed"], 0);
assert_eq!(report["summary"]["skipped"], 3);
assert_eq!(report["summary"]["not_applicable"], 0);
assert_eq!(report["summary"]["passed_build_verified"], 1);
```

- [ ] Find the result whose requirement list contains `{ "family": "idw", "number": 10 }`. Assert `status == "pass"`, `origin == "build_verified"`, and the exact non-empty evidence reference.
- [ ] Format every skipped result from its serialized family/number and assert the sorted list is exactly:

```rust
vec!["IDW-014", "QUAL-010", "REC-016"]
```

- [ ] Add a human-output test that asserts non-zero exit, `IDW-010 [build_verified]`, and the summary's explicit executed/build-verified/attested split.

### Step 2: Run RED

- [ ] Run:

```powershell
cargo test -p vestrace-cli --test idw_010_build_verified -- --nocapture
```

Expected: tests fail at the 195/4 baseline and/or because the CLI cannot map/render `BuildVerified` yet.

### Step 3: Merge build-verified cases into the report

- [ ] In `build_report()`, rename the local `executed` map to `verified` (or another origin-neutral name).
- [ ] Chain `cases::build_verified_cases().run_all(None).results` with the existing domain and application executable case results.
- [ ] Keep registered case precedence over `evaluate_requirement()` fallback.
- [ ] Delete the stale `IDW-010` fallback branch so no second source can silently reintroduce a skip.

The assembly shape should be:

```rust
let verified: HashMap<_, _> = cases::executable_cases()
    .run_all(None)
    .results
    .into_iter()
    .chain(cases::build_verified_cases().run_all(None).results)
    .chain(
        vestrace_application::conformance_cases::executable_cases()
            .run_all(None)
            .results,
    )
    .filter_map(|result| {
        result
            .requirement_ids
            .first()
            .copied()
            .map(|id| (id, result))
    })
    .collect();
```

### Step 4: Map and render the new origin

- [ ] In `hard_gate_evidence()`, map `CaseOrigin::BuildVerified` to `EvidenceOrigin::LocalBuildVerified`.
- [ ] In human output, render the origin through its `Display` implementation or add the explicit `build_verified` match arm.
- [ ] Print summary counts so the three categories cannot be confused:

```text
Passed: 196 (executed: ..., build-verified: 1, attested: ...)
```

Derive attested as `passed - passed_executed - passed_build_verified` with checked subtraction or an equivalent non-panicking expression.

### Step 5: Run GREEN and focused regression checks

- [ ] Run:

```powershell
cargo test -p vestrace-cli --test idw_010_build_verified -- --nocapture
cargo test -p vestrace-cli --all-targets --all-features --no-run
cargo fmt --all -- --check
cargo clippy -p vestrace-cli --all-targets --all-features -- -D warnings
```

Expected: all commands exit 0. The test process itself succeeds even though each child TRUSTED command exits non-zero as asserted.

### Step 6: Commit and review Task 3

- [ ] Verify and commit only the CLI implementation and its integration test:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- crates/vestrace-cli/src/commands/conformance.rs crates/vestrace-cli/tests/idw_010_build_verified.rs
git -c safe.directory=E:/Soft/vestrace add -- crates/vestrace-cli/src/commands/conformance.rs crates/vestrace-cli/tests/idw_010_build_verified.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "feat(cli): report IDW-010 build verification" -- crates/vestrace-cli/src/commands/conformance.rs crates/vestrace-cli/tests/idw_010_build_verified.rs
```

Expected cached state after commit: only the preserved nginx rename.

---

## Task 4: Run final gates and publish durable evidence

**Files:**

- Create: `docs/superpowers/reports/2026-08-16-v1-idw-010-build-verified-evidence.md`
- Modify only if execution facts require clarification: `docs/superpowers/specs/2026-08-16-v1-idw-010-build-verified-evidence-design.md`
- Modify only if execution diverged and the plan must remain truthful: `docs/superpowers/plans/2026-08-16-v1-idw-010-build-verified-evidence.md`

### Step 1: Capture preservation baselines

- [ ] Record:

```powershell
git -c safe.directory=E:/Soft/vestrace status --short
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace diff -- apps/console/dist apps/console/node_modules apps/console/nginx.conf apps/console/nginx.conf.template
git -c safe.directory=E:/Soft/vestrace diff --cached -- crates/vestrace-domain/src/generated crates/vestrace-domain/src/conformance/generated
```

- [ ] Reuse the established generated/cache inventory procedure and require 43 entries, SHA-1 `771c271c6fcc611a3405860235cc2ecdac411fb5`, and zero cached generated paths. If the inventory differs, stop and investigate rather than normalizing it.

### Step 2: Run the complete Rust gates

- [ ] Run each command independently and record exit code and test counts:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --no-run
cargo test --test v1_release_evidence -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
```

Expected: all exit 0; release evidence remains 4/4 and registry conformance remains 3/3.

### Step 3: Run CLI truth and exact TRUSTED acceptance

- [ ] Run the existing CLI truth gate through Git Bash:

```powershell
& 'C:\Program Files\Git\bin\bash.exe' ./scripts/foundation-cli-truth.sh
```

Expected: exit 0.

- [ ] Run the built CLI with `conformance check trusted --json`, capture stdout and stderr separately, and require the command itself to exit 1.
- [ ] Parse JSON rather than grepping prose. Assert exactly 199/196/0/3/0, `passed_build_verified == 1`, `IDW-010 == pass/build_verified`, and skips exactly `IDW-014`, `QUAL-010`, `REC-016`.

### Step 4: Verify exact range and preserved state

- [ ] Run:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check 7ca671f..HEAD
git -c safe.directory=E:/Soft/vestrace log --oneline 7ca671f..HEAD
git -c safe.directory=E:/Soft/vestrace diff --name-status 7ca671f..HEAD
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace status --short
```

Expected: the task range contains only approved files; cached diff is still only the nginx rename; preserved generated/cache inventory is unchanged.

### Step 5: Write the durable report

- [ ] Create the report with:

  - base and final commit IDs;
  - each task commit and exact changed paths;
  - RED/GREEN evidence, including the mutation failure and restored compilation;
  - final Rust gate results;
  - CLI truth result;
  - exact TRUSTED JSON counts and remaining IDs;
  - generated/cache count, hash, and cached count;
  - explicit statement that Docker was not run because this slice has no runtime database surface;
  - explicit statement that TRUSTED and v1.0 remain open.

- [ ] End with exactly this completion wording:

```text
IDW-010 build-verified evidence complete; TRUSTED now reports 196 passed and remains open on IDW-014, QUAL-010, and REC-016.
```

### Step 6: Commit the report and rerun post-commit checks

- [ ] Commit only the report and any necessary truthful design/plan amendments:

```powershell
git -c safe.directory=E:/Soft/vestrace add -- docs/superpowers/reports/2026-08-16-v1-idw-010-build-verified-evidence.md
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "docs: record IDW-010 build verification" -- docs/superpowers/reports/2026-08-16-v1-idw-010-build-verified-evidence.md
git -c safe.directory=E:/Soft/vestrace diff --check 7ca671f..HEAD
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
```

If design or plan amendments were necessary, add them explicitly to both the staging and `commit --only` path lists and explain why in the report.

---

## Task 5: Independent broad review

**Files:** No planned tracked edits. Any accepted fix must return to the relevant task's RED/GREEN/commit discipline.

### Step 1: Review the complete range

- [ ] Give an independent reviewer the approved design, this plan, the durable report, and exact range `7ca671f..HEAD`.
- [ ] Require review of:

  - compiler-proof completeness across all six prohibited substitution traits;
  - mutation sensitivity and absence of the temporary mutation from HEAD;
  - serde compatibility defaults;
  - summary accounting and human/JSON truthfulness;
  - Domain-only hard-gate origin policy;
  - absence of stale `IDW-010` fallback evidence;
  - exact 199/196/0/3/0 TRUSTED outcome with non-zero exit;
  - scope exclusions and preserved dirty/cached state.

### Step 2: Resolve findings by severity

- [ ] Fix all Critical and Important findings with focused RED/GREEN tests and exact-path commits, then rerun Task 4 in full.
- [ ] Record Minor findings in the durable report only if intentionally deferred with a concrete reason.
- [ ] Do not declare completion until the reviewer reports no open Critical or Important findings.

### Step 3: Final handoff

- [ ] Report the final HEAD, task commits, gate results, preserved cached rename, and exact remaining TRUSTED skips.
- [ ] Use only the approved truthful completion statement; do not expand it into a v1.0 completion claim.
