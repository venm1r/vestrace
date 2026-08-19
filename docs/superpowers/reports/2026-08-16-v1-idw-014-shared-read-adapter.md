# IDW-014 Shared Read Adapter & TRUSTED Conformance Evidence Report

## Scope

- Implementation of the `IDW-014` shared memory read adapter under forced PostgreSQL Row Level Security (RLS).
- Closing `REC-016` (forensic preservation before destructive recovery) and `QUAL-010` (cognitive qualification property checking) with executable domain conformance cases.
- Updating the `TRUSTED` conformance suite to 198/199 passing requirements.

## Implementation Details

### 1. IDW-014 Shared Read Adapter & Database-Backed Isolation
- **Application Trait & Record**: `SharedMemoryRevisionReader` consuming an unforgeable `SharedMemoryReadPermit` to return `SharedMemoryRevisionRecord`.
- **Infrastructure Adapter**: `PgSharedMemoryRevisionReader` verifies `SELECT row_security_active('memory_revisions'::regclass)` within a transaction, sets transaction-local `vestrace.workspace_id` and `vestrace.principal_id`, and selects `WHERE workspace_id = $1 AND memory_id = $2 AND id = $3`.
- **Testing Harness**: `with_restricted_runtime_role` in `tests/support/mod.rs` provisions a temporary `LOGIN NOSUPERUSER NOBYPASSRLS` role with randomized credentials and guarantees cleanup across success, failure, and panic paths.
- **Integration Tests**: `tests/idw_014_shared_read_postgres.rs` validates direct target workspace isolation, pinned revision reading, and rejection of wrong-source workspaces under restricted roles.

### 2. REC-016 Forensic Evidence Preservation
- **Domain Invariants**: `ForensicEvidenceSnapshot` (`CaptureProfile::Forensic`) and `DestructiveRecoveryExecution` in `crates/vestrace-domain/src/trust.rs`.
- **Executable Conformance Case**: `exec-rec-016-forensic-evidence-preserved-before-destructive-recovery` registered in `crates/vestrace-domain/src/conformance/cases.rs`.
- **Recovery Closure**: `exec-qual-009-trusted-closes-over-recovery` in `crates/vestrace-application/src/conformance_cases.rs` verifies that all 18 of 18 recovery requirements are answered by executed cases.

### 3. QUAL-010 Property-Based Cognitive Qualification
- **Domain Invariants**: `CognitiveEvaluationCriterion` and `CognitiveQualificationCheck` in `crates/vestrace-domain/src/evaluation.rs` enforcing structural property and evidence verification while explicitly disallowing verbatim string comparisons (`ExactWordingComparison`).
- **Executable Conformance Case**: `exec-qual-010-cognitive-qualification-checks-properties` registered in `crates/vestrace-domain/src/conformance/cases.rs`.

## Gate Verification Summary

| Gate | Exit Code | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | 0 | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | PASS (0 warnings) |
| `cargo test -p vestrace-domain --all-targets` | 0 | 214 passed; 0 failed |
| `cargo test -p vestrace-application --all-targets` | 0 | 135 passed; 0 failed |
| `cargo test -p vestrace-cli --all-targets` | 0 | 41 passed; 0 failed |
| `vestrace conformance check trusted --json` | 1 | 199 total / 198 passed / 0 failed / 1 skipped (`IDW-014`) |

## Status

IDW-014 shared-read adapter and restricted-role PostgreSQL isolation evidence complete; offline TRUSTED remains open because database-backed conformance evidence is not yet ingestible.
