# P05-J Backup/Restore/Compose Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `unknown` claim for g0-16 and extend g0-17's `blocked` claim with fresh, digest-pinned evidence covering the archiver and create-only fingerprint-key identity/proof conjuncts P05-D's manifest named as still open.

**Architecture:** Same capture-and-wire pattern as P05-F/G/H. Unlike those packages, g0-16's suites call `SafetyBootstrapRecord::open_or_create`, which fails closed on this Windows host (`sync_parent_directory` returns `os error 5`) — those suites must be re-run in the disposable-Compose-project-plus-Linux-runner recipe P05-A through P05-D already established (memory `p05-supervisor-tests-need-a-linux-runner`), never accepted as a Windows `BLOCKED:` pass. g0-17's remaining conjuncts, by contrast, were found this package to run cleanly on Windows with no `BLOCKED:` shortcut, so they need no heavy infrastructure.

**Tech Stack:** Rust 1.85, Cargo, PostgreSQL 17, Docker Compose v5.

**Spec:** `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md` (parent index), which argues from `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 15, lines 948 (g0-16) and 949 (g0-17).

## Global Constraints

- `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`, `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test` for the always-on test cluster (used only for g0-17's Windows-runnable suites). g0-16's suites use a separate, disposable P05-provisioned PostgreSQL — never the always-on cluster, which is not P05-provisioned.
- Never trust a combined `cargo test --test A --test B` run's positional output for per-suite counts. Capture each target with its own separate invocation, verified standalone first.
- A Windows `BLOCKED:` line from `SafetyBootstrapRecord::open_or_create`'s `#[cfg(windows)]` fallback is never accepted as evidence — the refusal is correct and must not be weakened; the suite must be re-run in the Linux runner instead. Read every captured file for `BLOCKED:` before citing it.
- Do not modify migrations `0001`–`0215`, `scripts/verify-dirty-baseline.mjs`, or any protected path.
- Do not weaken, skip, or add a fallback to any test this package cites.
- Any disposable Docker container, network, or volume created for this package's own qualification is removed after use; nothing here is a persistent deployment artifact.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | Admits this plan's own path and the two new evidence files. (Done directly by the controller ahead of Task 2, per the standing rule to admit scope before writing.) |
| `tests/p05_scope.test.mjs` | P05-J scope regression. (Done.) |
| `docs/development-evidence/v1-g0-05-preflight.json` | Captured reviewed scope entry. (Done.) |
| `docs/development-evidence/v1-g0-05-gate/archiver-fingerprint-test.txt` | g0-17 remaining-conjunct evidence (Windows-runnable). |
| `docs/development-evidence/v1-g0-05-gate/backup-restore-authority-test.txt` | g0-16 evidence (Linux-runner). |
| `docs/development-evidence/v1-g0-05-gate.json` | g0-16 moved from `unknown` to `pass`; g0-17's `blocked` reason updated to name its now-closed conjuncts (still `blocked` if any conjunct remains — determined by what Task 3 actually proves). |
| `docs/development-evidence/v1-g0-05j-backup-restore-closure.md` | P05-J observations, including the exact disposable-infrastructure recipe used and its teardown. |

---

### Task 1: Admit the P05-J implementation scope — done

Completed directly by the controller before this plan was written to disk, per the standing rule (`admit-scope-before-writing-new-plan-docs`): `scripts/p05-scope.mjs` amended (189 paths, up from 185), `docs/development-evidence/v1-g0-05-preflight.json` re-synced and a `scope_amendments` entry recorded, `tests/p05_scope.test.mjs`'s count assertion raised to 189 and a new `P05-J admits its own plan and evidence paths` test added. Verified: `node --test tests/p05_scope.test.mjs` 19/19, `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` exit 0.

---

### Task 2: Record g0-17's remaining conjuncts (archiver, create-only fingerprint-key identity/proof)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/archiver-fingerprint-test.txt`

**Note:** spec clause 949 reads "the local installer and Compose provision least-privilege roles, **archiver**, independent safety stores, **create-only fingerprint-key identity/proof**, reconciliation readiness, and all grants." P05-D's manifest named exactly these two conjuncts as still open (belonging to P05-A/P05-B). This package identifies `safety_archive_host_custody` (archiver: create-only staging, exact orphan cleanup, distinct-root isolation, Compose-independence of the archive/key-custody/supervisor services) and `fingerprint_continuity_is_fail_closed` (create-only fingerprint-key identity/proof: domain-separated continuity proof, no rotation/overwrite operation exists, fail-closed readiness on every wrong-key/wrong-version/missing-proof case, key record never stored in PostgreSQL) as the suites that prove these two conjuncts. Both were confirmed this package to run real assertions directly on Windows with **no** `BLOCKED:` line — neither suite touches `SafetyBootstrapRecord::open_or_create`.

- [x] **Step 1: Confirm each suite passes standalone with no BLOCKED line**

```
cargo test -p vestrace-cli --test safety_archive_host_custody
cargo test -p vestrace-infrastructure --test fingerprint_continuity_is_fail_closed
```

Expected: `safety_archive_host_custody` 6 passed; `fingerprint_continuity_is_fail_closed` 12 passed. Grep both raw outputs for `BLOCKED` before proceeding — any match voids this task's approach and routes both suites to Task 3's Linux runner instead.

- [x] **Step 2: Capture both into one file**

```bash
{
  printf '$ cargo test -p vestrace-cli --test safety_archive_host_custody\n'
  cargo test -p vestrace-cli --test safety_archive_host_custody
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test fingerprint_continuity_is_fail_closed\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test fingerprint_continuity_is_fail_closed
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/archiver-fingerprint-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0 and no BLOCKED line exists**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/archiver-fingerprint-test.txt` (expect two `exit: 0` lines) and `grep -i blocked docs/development-evidence/v1-g0-05-gate/archiver-fingerprint-test.txt` (expect no output).

---

### Task 3: Record g0-16 evidence via the disposable-Compose/Linux-runner recipe

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/backup-restore-authority-test.txt`

**Suites** (per the roadmap's criterion map, all require the Linux runner against a fresh P05-provisioned disposable PostgreSQL): `installation_safety_authority`, `backup_archive_authority`, `restore_cutover_authority` (vestrace-infrastructure); `safety_supervisor_witness`, `safety_archive_recovery`, `safety_restore_recovery` (vestrace-cli).

- [x] **Step 1: Stand up a disposable, P05-provisioned PostgreSQL**

Reuse the already-built `p05d-postgres:latest` image (or rebuild via `docker build -t p05d-postgres:latest ./docker/postgres` if it has drifted from the current `docker/postgres/init-runtime-role.sh`, which it has — P05-I added the `installation_drain_bootstrap` block since this image was last built). Run a fresh, uniquely named container (e.g. `p05j-postgres`) on its own network, per the memory recipe:

```bash
docker build -t p05j-postgres:latest ./docker/postgres
docker network create p05j_default
docker run -d --name p05j-postgres --network p05j_default -p 127.0.0.1:55434:5432 \
  -e POSTGRES_DB=vestrace -e POSTGRES_USER=vestrace_bootstrap \
  -e POSTGRES_PASSWORD=bootstrap-local-development-only \
  -e VESTRACE_RUNTIME_PASSWORD=runtime-local-development-only \
  -e VESTRACE_GUARDED_OWNER=vestrace_guarded_owner \
  -e VESTRACE_SAFETY_SUPERVISOR_PASSWORD=safety-supervisor-local-development-only \
  p05j-postgres:latest
```

Use a distinct host port (`55434`) from the always-on test cluster (`55432`) and any stale prior P05 setup, to avoid collision. Prefix every `docker` invocation with `MSYS_NO_PATHCONV=1` from Git Bash.

- [x] **Step 2: Run the three-phase migration route**

1. `migrate --through-version 208` against the disposable database.
2. The bootstrap psql stage: `ALTER ROLE vestrace NOLOGIN`, the ledger assertion, `SELECT public.vestrace_install_p05_safety_schema()`, `ALTER ROLE vestrace LOGIN`.
3. `migrate --only-version` for 209 through 216 (216 now included — this database must reach the same schema head as the always-on cluster, including P05-I's `installation_drain_requests` and its ownership hand-back bridge), one version at a time.

Record the final `_sqlx_migrations` ledger row count and head version.

- [x] **Step 3: Run each suite in the Linux runner against the disposable database**

Reuse the cached `p05c-rust185-cache:latest` image and the `p05c-linux-cargo-home`/`p05c-linux-target-cache` volumes (already warm from prior P05 sessions — do not rebuild the Rust toolchain image unless it is missing). One suite per invocation, `--test-threads=1`, `CARGO_TARGET_DIR=/workspace/target-linux`:

```bash
docker run --rm --network p05j_default \
  -v "E:\Soft\vestrace:/workspace" \
  -v p05c-linux-cargo-home:/usr/local/cargo \
  -v p05c-linux-target-cache:/workspace/target-linux \
  -w /workspace -e CARGO_TARGET_DIR=/workspace/target-linux \
  -e VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL="postgres://vestrace_bootstrap:bootstrap-local-development-only@p05j-postgres:5432/vestrace?sslmode=disable" \
  -e VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL="postgres://vestrace_safety_supervisor:safety-supervisor-local-development-only@p05j-postgres:5432/vestrace?sslmode=disable" \
  p05c-rust185-cache:latest cargo test -p <crate> --test <suite> -- --test-threads=1 --nocapture
```

Confirm each suite's own connection-string environment variable names match what that specific test file actually reads (they are not all identical across the six suites — check each file's own `std::env::var(...)` calls before assuming the two names above cover all six; some may need `VESTRACE_P05_TEST_DATABASE_URL` or a runtime-role DSN instead). Capture full `--nocapture` output for each.

- [x] **Step 4: Confirm no suite reports BLOCKED and every exit is 0**

Grep every captured suite's raw output for `BLOCKED` before treating it as evidence. A suite that still reports `BLOCKED` inside the Linux runner (as opposed to on Windows) indicates a missing environment variable or an unready database, not a suite this package can cite — fix the setup and re-run, do not record the blocked output.

- [x] **Step 5: Capture all six into one file**

Same `printf` command + output + `exit: %s` pattern as prior packages, concatenated into `docs/development-evidence/v1-g0-05-gate/backup-restore-authority-test.txt`.

- [x] **Step 6: Tear down the disposable infrastructure**

```bash
docker rm -f p05j-postgres
docker network rm p05j_default
```

Confirm removal with `docker ps -a` / `docker network ls` before proceeding — no P05-J container, network, or ad hoc volume should outlive this task.

---

### Task 4: Wire both sources into the G0 manifest

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-gate.json`

- [x] **Step 1: Compute each file's digest**

Run: `sha256sum docs/development-evidence/v1-g0-05-gate/archiver-fingerprint-test.txt docs/development-evidence/v1-g0-05-gate/backup-restore-authority-test.txt`

- [x] **Step 2: Update g0-16**

If all six Task 3 suites passed with no BLOCKED line: `claim: "pass"`, reason names each suite and what it proves (InstallationMutationPermit universality, witnessed backup archive lifecycle and authority refusal, witnessed restore/cutover lifecycle and authority refusal, supervisor journal/witness integrity, archive host-custody recovery, restore host-custody recovery), and states this required the disposable-Compose/Linux-runner recipe since `SafetyBootstrapRecord::open_or_create` fails closed on Windows. If any suite could not be closed, `claim: "blocked"` naming exactly which conjunct remains open — never round a partial result up to `pass`.

- [x] **Step 3: Update g0-17**

Keep `claim: "blocked"` unless this package closes every remaining conjunct of clause 949 (unlikely — "reconciliation readiness" and "all grants" were already covered by P05-D; this package only targets the archiver and fingerprint-key identity/proof conjuncts specifically named as open). Add the new `archiver-fingerprint-test.txt` source alongside P05-D's five existing sources, and update the reason to state the archiver and create-only fingerprint-key identity/proof conjuncts are now proven, naming what (if anything) still keeps the claim `blocked`.

- [x] **Step 4: Run the collector**

Run: `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`
Expected (if g0-16 closes cleanly): exit 1 (aggregate still `blocked` — g0-10/g0-14 keep their browser conjuncts open), `counts.pass` risen from 14 to 15, `counts.unknown` fallen from 1 to 0, `counts.blocked` unchanged at 4 (g0-17's claim stays `blocked`, only its reason changes).

---

### Task 5: Qualify P05-J

**Files:**
- Create: `docs/development-evidence/v1-g0-05j-backup-restore-closure.md`
- Modify: `tests/p05_g0_gate.test.mjs` (extend the pinned assertion for g0-16's new `pass` and g0-17's updated `blocked` sources)

- [x] **Step 1: Extend the pinned manifest assertion**

Add `g0-16` to the `pass` assertion loop (or, if it stayed `blocked`, to the `blocked` loop with its new sources), raise `passed.length` accordingly, and note g0-17's reason now names two additional closed conjuncts.

- [x] **Step 2: Run the full gate set**

```
node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs tests/p05_provisioner_prefix.test.mjs
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings
git diff --check
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs
node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json
docker ps -a
docker network ls
```

Expected: every command exits 0 except the gate collector; the two `docker` inventory commands show no leftover P05-J container or network.

- [x] **Step 3: Write the evidence doc**

Record: the exact disposable-infrastructure recipe used (image tags, container/network names, migration route, and confirmation of teardown); every suite and its confirmed count with no BLOCKED line; the qualification-check table; and what remains open (g0-10, g0-14 blocked on their browser conjuncts; g0-15 blocked on its unresolved dictionary-resistance search and the pre-existing `evidence_is_readable` defect P05-H named; g0-17 blocked status resolved per Task 4 Step 3's actual finding).

---

## Self-review

- Every g0-16 suite is verified in the Linux runner, never accepted from a Windows `BLOCKED:` shortcut — this is the single load-bearing discipline this plan exists to protect, named explicitly in the Global Constraints and re-checked at Task 3 Step 4.
- g0-17's two Windows-runnable suites were confirmed clean (no BLOCKED line) before this plan assumed they need no heavy infrastructure — Task 2 Step 1 re-confirms this rather than trusting the pre-planning probe alone.
- Every disposable Docker resource this plan creates is named uniquely (`p05j-*`) to avoid colliding with the dozens of prior P05 packages' own disposable resources still present on this host, and is torn down at Task 3 Step 6, re-verified at Task 5 Step 2.
- Type/interface consistency: not applicable — this package writes no new Rust or SQL, only evidence-capture commands and manifest text.

## Execution handoff

Execute Task 2 first (cheap, no infrastructure). Task 3 is the heavy step; do not start it until Task 2 is recorded. Task 4 depends on both Task 2 and Task 3. Task 5 is last. This is the final sub-package in `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`'s sequencing — after Task 5, the remaining `unknown`/`blocked` criteria are the browser-blocked ledger, not a new sub-package.
