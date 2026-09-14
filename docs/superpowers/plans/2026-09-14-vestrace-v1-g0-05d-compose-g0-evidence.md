# P05-D Compose and G0 Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Qualify P05's least-privilege Compose topology and host safety readiness, then produce an aggregate G0 result that reports only persisted, independently verified evidence.

**Architecture:** Compose remains responsible only for PostgreSQL and product services plus the one `installation-safety-journal` volume. The supervisor, witness, archive, and protected host-key roots remain outside Compose; a supervisor-owned readiness check validates them against the database's signed safety state. A deterministic gate collector returns `pass`, `blocked`, or `unknown` from declared evidence sources and cannot invent a pass.

**Tech Stack:** Rust, Tokio, SQLx/PostgreSQL 17, Docker Compose, Node.js test runner, JSON evidence manifests.

**Spec:** `docs/superpowers/specs/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation-design.md`; frozen v1 requirements at `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` sections 11.6, 14.4, and G0.

## Global Constraints

- Preserve inherited dirty paths; amend only the P05-D allowlist and its captured preflight scope entry before implementation writes.
- Do not modify protected P01-P04 authority or migrations through 0208; migration 0215 is limited to the supervisor-only read surface below.
- Compose may define the `installation-safety-journal` volume only. The supervisor executable, witness, archive root, and protected host-key roots must never be Compose-managed or mounted into PostgreSQL, runtime, server, worker, or console containers.
- Do not start a product server or worker. Disposable PostgreSQL source/target containers and host-supervisor processes are allowed only for named tests.
- Readiness fails closed on a missing, corrupt, mismatched, or stale witness/fingerprint continuity proof. It must not synthesize a receipt or health result.
- The G0 collector may pass a criterion only from a named persisted source and successful command. Missing, blocked, or non-independent proof remains `unknown` or `blocked`.
- Do not commit, push, deploy, request secrets, or make provider calls.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | P05-D allowlist and protected-authority boundary. |
| `tests/p05_scope.test.mjs` | P05-D scope regression. |
| `docs/development-evidence/v1-g0-05-preflight.json` | Captured reviewed P05-D scope entry. |
| `docker-compose.yml` | Least-privilege service, mount, dependency, and readiness topology. |
| `crates/vestrace-infrastructure/src/postgres/safety_authority_repository.rs` | Guarded supervisor read adapter. |`n| `migrations/0215_managed_safety_readiness.sql` | Forward-only assertion for the least-privilege read function. |`n| `crates/vestrace-cli/src/commands/safety_supervisor.rs` | Host-only safety continuity readiness. |
| `crates/vestrace-cli/src/main.rs` | Readiness subcommand parser registration. |
| `crates/vestrace-cli/tests/safety_supervisor_readiness.rs` | Healthy, absent, corrupt, forged, and stale readiness cases. |
| `crates/vestrace-cli/tests/safety_journal_compose.rs` | Resolved Compose topology proof. |
| `scripts/p05-g0-gate.mjs` | Fail-closed G0 evidence collector. |
| `tests/p05_g0_gate.test.mjs` | Collector mutation tests. |
| `docs/development-evidence/v1-g0-05-backup-restore-foundation.md` | Actual P05-D and G0 observations. |

### Task 1: Admit and freeze P05-D scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`
- Create: `docs/superpowers/plans/2026-09-14-vestrace-v1-g0-05d-compose-g0-evidence.md`

**Interfaces:** extends `changeScopePaths` with exactly the P05-D paths in this plan; retains protected paths and P05-A/B/C entries byte-for-byte.

- [ ] Add a failing assertion for the P05-D plan and enumerate only the topology, readiness, collector, test, and evidence paths below.
- [ ] Run `node --test tests/p05_scope.test.mjs`; observe RED while the P05-D path is omitted.
- [ ] Add sorted entries to `changeScopePaths` and preflight `change_scope_paths` without recapturing the inherited dirty baseline.
- [ ] Assert P05-D adds only `0215_managed_safety_readiness.sql` and cannot include protected authority.
- [ ] Run the scope suite; expect PASS.

### Task 2: Prove least-privilege Compose topology

**Files:**
- Modify: `docker-compose.yml`, `crates/vestrace-cli/tests/safety_journal_compose.rs`

**Interfaces:** `docker compose config` shows only `vestrace-safety-journal-init` mounting `installation-safety-journal`; product services cannot see journal, witness, archive, or host-key roots.

- [ ] Write failing assertions for resolved service mounts, effective user, privilege flags, ordered dependencies, and health checks.
- [ ] Run the focused test and observe RED before any required configuration repair.
- [ ] Make the minimum Compose change so role provision, history migration, safety bootstrap, P05 assertions, fingerprint initialization, server, worker, console, and seed have explicit prerequisites with no expanded privileged mount.
- [ ] Keep journal initialization create-only and owner-restricted; add neither a supervisor service nor an archive/witness/key volume.
- [ ] Run `docker compose config` and `cargo test -p vestrace-cli --test safety_journal_compose -- --nocapture`.

### Task 3: Add host-supervisor continuity readiness

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/safety_authority_repository.rs`, `crates/vestrace-cli/src/commands/safety_supervisor.rs`, `crates/vestrace-cli/src/main.rs`, `crates/vestrace-cli/tests/command_contract.rs`\n- Create: `migrations/0215_managed_safety_readiness.sql`
- Create: `crates/vestrace-cli/tests/safety_supervisor_readiness.rs`

**Interfaces:** `vestrace safety-supervisor readiness` reads configured host roots and database URL; it validates the bootstrap/fingerprint record, journal chain, witness head/signature, and exact installation, proof, sequence, digest, generation, epoch, and canonical state persisted in PostgreSQL. It exits zero only for that exact match and never mutates storage or prints secrets.

- [ ] Add RED tests for absent roots, corrupt journal, forged witness, mismatched fingerprint, stale sequence/digest, and one exact healthy state.
- [ ] Add the 0215 guarded owner function and supervisor-only EXECUTE grant, then implement read-only composition of existing file journal/witness decoders and guarded repository reads. Do not permit caller-supplied verifier keys, reset/import, or generic root traversal.
- [ ] Cover parser/contract restrictions: supervisor-only, no mutating flags.
- [ ] Run `cargo test -p vestrace-cli --test safety_supervisor_readiness --test command_contract -- --nocapture`.

### Task 4: Aggregate G0 evidence without manufacturing a pass

**Files:**
- Create: `scripts/p05-g0-gate.mjs`, `tests/p05_g0_gate.test.mjs`
- Modify: `docs/development-evidence/v1-g0-05-backup-restore-foundation.md`

**Interfaces:** `node scripts/p05-g0-gate.mjs --evidence <path>` emits canonical ordered `{ criterion, status: 'pass' | 'blocked' | 'unknown', sources, reason }` entries. A pass requires every named source to be present, digest-bound, and independently successful; aggregate G0 passes only when every criterion passes.

- [ ] Write mutations that remove a source, relabel blocked evidence, alter command exit, or add an unknown criterion; each must refuse or emit a non-pass aggregate.
- [ ] Implement schema validation, stable ordering, explicit sources, and nonzero malformed-evidence exit. Never parse prose as proof.
- [ ] Record actual P05-D results and leave browser, provider, clean-release, and later-package criteria `unknown`/`blocked` where unperformed.
- [ ] Run gate tests and collector against recorded evidence.

### Task 5: Qualify P05-D and close only its stated boundary

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-backup-restore-foundation.md`
- Test: `tests/p05_scope.test.mjs`, `crates/vestrace-cli/tests/safety_journal_compose.rs`, `crates/vestrace-cli/tests/safety_supervisor_readiness.rs`, `tests/p05_g0_gate.test.mjs`

- [ ] Run scoped Node/Rust tests, `cargo fmt --all -- --check`, strict touched-crate clippy, `docker compose config`, `git diff --check`, and the dirty-baseline verifier.
- [ ] Run a disposable PostgreSQL readiness harness with all host roots outside Compose volumes. Prove corrupt/mismatched conditions refuse and P05-C source/target evidence is unchanged.
- [ ] Record actual exits, counts, resolved mounts, privilege observations, and baseline result.
- [ ] Mark P05-D complete only if its checks pass; report G0 exactly as emitted and never infer full G0 closure from P05.

## Self-review

- Task 2 preserves the designated Compose boundary; Task 3 joins host witness and fingerprint continuity to database state without product-container custody; Task 4 fails closed.
- Each transition names its inputs, durable comparison, refusal, and command. The aggregate has no prose-only pass source.
- Readiness reuses signed journal/witness and `InstallationSafetyState`; it creates no parallel safety state.

## Execution handoff

Execute Task 1 first. Do not use cdx, do not start a product server or worker, and stop before any package beyond P05-D.