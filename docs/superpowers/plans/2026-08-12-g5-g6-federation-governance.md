# G5/G6 Federation Trust and Governance Hard Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development when executing this plan with independent tasks.

**Goal:** Implement the bounded G5 federation trust boundary and G6 Govern/Federation hard gate described by the v0.2 normative documents, with executable negative evidence and no stronger qualification claim than the evidence supports.

**Architecture:** G5 adds an in-memory, authoritative federation relationship and remote-identity trust evaluator beside the G4 sharing model. Trust decisions are scoped to an exact local workspace, remote workspace, and remote principal; they are never convertible to a local data permission. Federated memory access requires both an active federation trust decision and the existing G4 source-grant/target-mount/local-policy decision. G6 adds a pure hard-gate evaluator to the existing conformance layer. It enforces profile dependency closure, required MUST evidence, and security/governance negative cases without pretending that a fixture is a durable QualificationBundle.

**Tech Stack:** Rust, `vestrace-domain`, serde/schemars, existing typed IDs, chrono timestamps, integration tests, JSON qualification fixtures, Markdown authority/delta documentation.

## Global Constraints

- Preserve all unrelated dirty work and generated evidence; do not reset, rebase, stage, or commit.
- Treat `IDW-006`, `IDW-007`, `IDW-008`, `IDW-013`, `GOV-020`, `GOV-024`, `QUAL-002`, `QUAL-004`, `QUAL-006`, `QUAL-007`, `QUAL-013`, and `QUAL-017` as the minimum traceability boundary for this slice.
- Remote identity, signed/claimed remote status, and federation trust are not local capability, share grant, mount acceptance, DataPolicy approval, or export/provider authority.
- Required `SKIPPED` or `INCONCLUSIVE` evidence is a hard failure; quality metrics cannot compensate for a failed security/governance gate.
- Constructors generate authoritative IDs internally. Unknown, expired, suspended, revoked, mismatched, and stale state fails closed.
- This slice is additive in-memory/domain plus deterministic fixture evidence. It does not claim durable federation persistence, transport wiring, cryptographic signature verification, or a qualified FEDERATION/TRUSTED release profile.

---

## Task 1: Freeze authority map and write RED tests for G5

**Files:** `tests/g5_federation_trust.rs`, `crates/vestrace-domain/src/enterprise/federation.rs` (new), `crates/vestrace-domain/src/enterprise/mod.rs`, `crates/vestrace-domain/src/id.rs`

- Add tests for exact local/remote identity binding, active trust, unknown identity denial, and suspended/revoked/expired trust denial.
- Add a mutation-sensitive test proving active federation trust alone cannot produce a local shared-memory reference or allow data access.
- Add a positive intersection test proving federated data access requires both an active trust relationship and G4 local share/mount/policy access.
- Add tests for mismatched remote principal/workspace, stale policy version, and non-transitive remote authority.
- Run the focused test target and capture the expected compile/test failure before implementation.

## Task 2: Implement G5 federation trust boundary

**Files:** `crates/vestrace-domain/src/enterprise/federation.rs`, `crates/vestrace-domain/src/enterprise/mod.rs`, `crates/vestrace-domain/src/id.rs`, `tests/g5_federation_trust.rs`

- Add generated `FederationRelationshipId` and `FederationTrustDecisionId` typed IDs.
- Add exact `RemoteIdentity`, explicit `FederationRelationshipStatus`, immutable relationship issuance, explicit revoke/suspend transitions, validity and policy-version checks.
- Add `FederationTrustDecision` with a bounded reason enum and `evaluate_federation_trust` that validates identity, relationship, scope, policy version, and time.
- Add `evaluate_federated_memory_access` that requires a passing federation decision and delegates local authorization to `evaluate_share_access`; no trust-only conversion API may return `SharedMemoryRef`.
- Keep remote qualification/claims as input metadata only; no signature or remote self-assertion is accepted as local authority.
- Run focused G5 tests and the domain library tests.

## Task 3: Write RED tests for G6 hard gates

**Files:** `tests/g6_governance_federation_gate.rs`, `crates/vestrace-domain/src/conformance/gate.rs` (new), `crates/vestrace-domain/src/conformance/mod.rs`

- Add a deterministic evidence input covering profile, required requirement IDs, per-requirement results, and hard-gate markers.
- Test that all applicable required MUST results pass, while `SKIPPED`, `INCONCLUSIVE`, failed security/governance results, missing results, and profile dependency gaps deny the gate.
- Test that non-applicable cases are accepted only outside the target closure and cannot hide an applicable requirement.
- Test that a `FEDERATION` claim includes its dependency closure and cannot be widened to `TRUSTED` or `TRUSTED-FEDERATED` by a milestone label or high metric score.
- Run the focused test target and capture the expected failure before implementation.

## Task 4: Implement G6 Govern/Federation hard gate

**Files:** `crates/vestrace-domain/src/conformance/gate.rs`, `crates/vestrace-domain/src/conformance/mod.rs`, `tests/g6_governance_federation_gate.rs`

- Add explicit evidence status `Pass`, `Fail`, `NotApplicable`, `Skipped`, and `Inconclusive` for the gate input without changing existing report compatibility.
- Add `GovernanceFederationGate`, `HardGateDecision`, and deterministic reasons for missing/failed/skipped/inconclusive/unsupported/dependency evidence.
- Derive the target requirement closure from the existing profile dependency semantics, including `FEDERATION` and `TRUSTED` families.
- Require exact policy/evidence references for governance decisions and reject claims based solely on remote self-assertion or an unscoped milestone label.
- Expose a read-only helper for required closure so fixtures/tests can assert scope without duplicating private implementation logic.
- Run focused G6 tests and all domain conformance tests.

## Task 5: Add qualification fixtures and documentation delta

**Files:** `tests/fixtures/qualification/g5-federation.json`, `tests/fixtures/qualification/g6-governance-federation.json`, `docs/documentation-gap-delta-2026-08-12-g5-g6-federation-governance.md`, `docs/current-implementation.md`, `docs/documentation-status-v0.2.md`, `docs/requirement-coverage-v0.2.md`

- Record executable G5/G6 cases against the authoritative requirement IDs and explicitly mark remaining durable/cryptographic/transport qualification work as limitations.
- Document that G5/G6 implement a bounded domain/fixture gate, not a qualified v0.4 or FEDERATION/TRUSTED release.
- Update the implementation map and requirement coverage only for evidence actually present in this checkout.
- Parse both fixtures and verify no new trailing whitespace or diff errors.

## Task 6: Independent review and final verification

- Request an independent review of the combined G5/G6 diff for authority leakage, stale-state bypass, profile-scope widening, and test quality.
- Address only actionable findings in a follow-up patch, then rerun focused G5/G6 tests, domain tests, workspace library/check/no-run checks, scoped formatting, fixture parsing, and `git diff --check`.
- Report exact verified gates, preserved unrelated dirty state, and the remaining nonclaims. Do not commit.
