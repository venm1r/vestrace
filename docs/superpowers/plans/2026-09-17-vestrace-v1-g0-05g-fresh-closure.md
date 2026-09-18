# P05-G Fresh G0 Closure (P03 Category A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `unknown` claims for G0 criteria g0-03, g0-04, g0-05, g0-06, g0-07, g0-09, and g0-11 in `docs/development-evidence/v1-g0-05-gate.json` with `pass`, backed by fresh, digest-pinned evidence from P03-scope suites confirmed passing this session — without touching any protected P01–P04 authority.

**Architecture:** Same as P05-F: for each criterion, capture the exact stdout of its already-passing test command(s) to a new file under `docs/development-evidence/v1-g0-05-gate/`, compute the file's SHA-256, and add a `sources` entry to the criterion's manifest object. `scripts/p05-g0-gate.mjs` is not modified.

**Tech Stack:** Rust 1.85, Cargo, PostgreSQL 17 (`vestrace-test-postgres` container, plus the restricted `vestrace` runtime role).

**Spec:** `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md` (parent index), which argues from `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 15, lines 935 (g0-03), 936 (g0-04), 937 (g0-05), 938 (g0-06), 939 (g0-07), 941 (g0-09), 943 (g0-11).

## Global Constraints

- `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`. `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test` — required for suites that exercise the restricted runtime role (named per-task below). Use `127.0.0.1`, never `localhost`.
- **Never trust a combined `cargo test --test A --test B --test C` run's positional output for per-suite counts.** P05-F found Cargo does not run `--test` targets in flag order. Always capture each target with its own separate `cargo test -p <crate> --test <name>` invocation.
- Do not modify migrations `0001`–`0215`, `scripts/verify-dirty-baseline.mjs`, or any path in `protectedAuthorityPaths`.
- Do not weaken, skip, or add a fallback to any test this package cites as evidence.
- A criterion may be claimed `pass` only when `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json` independently confirms it.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | Admits this plan's own path and the seven new evidence files. |
| `tests/p05_scope.test.mjs` | P05-G scope regression. |
| `docs/development-evidence/v1-g0-05-preflight.json` | Captured reviewed scope entry. |
| `docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt` | g0-03 evidence. |
| `docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt` | g0-04 evidence. |
| `docs/development-evidence/v1-g0-05-gate/transport-security-test.txt` | g0-05 evidence. |
| `docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt` | g0-06 evidence. |
| `docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt` | g0-07 evidence. |
| `docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt` | g0-09 evidence. |
| `docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt` | g0-11 evidence. |
| `docs/development-evidence/v1-g0-05-gate.json` | Seven criteria's `claim` moved from `unknown` to `pass`. |
| `docs/development-evidence/v1-g0-05g-fresh-closure.md` | P05-G observations and the resulting aggregate. |

---

### Task 1: Admit the P05-G implementation scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`

- [x] **Step 1: Write the failing assertion**

Append to `tests/p05_scope.test.mjs`:

```javascript
test('P05-G admits its own plan and evidence paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05g-fresh-closure.md',
    'docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt',
    'docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt',
    'docs/development-evidence/v1-g0-05-gate/transport-security-test.txt',
    'docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt',
    'docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt',
    'docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt',
    'docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt',
    'docs/development-evidence/v1-g0-05g-fresh-closure.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});
```

- [x] **Step 2: Run it and watch it fail**

Run: `node --test tests/p05_scope.test.mjs`
Expected: FAIL.

- [x] **Step 3: Add the nine paths to `changeScopePaths`, sorted**

Insert the eight `docs/development-evidence/v1-g0-05-gate/*.txt` entries alphabetically by filename among the existing ones, `docs/development-evidence/v1-g0-05g-fresh-closure.md` immediately after `v1-g0-05f-fresh-closure.md`, and `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05g-fresh-closure.md` immediately after `...-v1-g0-05f-fresh-closure.md` in the plans block.

- [x] **Step 4: Sync the preflight capture to the module, verbatim**

Same disposable `sync-preflight.mjs` script as P05-F used (write at repo root, run, delete). Confirm it prints `169` (160 + 9).

- [x] **Step 5: Record the amendment**

Add one `scope_amendments` entry naming these nine paths and this plan.

- [x] **Step 6: Update the count assertion and run the suite**

Change `assert.equal(changeScopePaths.length, 160);` to `169`.

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS, all tests.

- [x] **Step 7: Verify the baseline is still clean**

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: exit 0.

---

### Task 2: Record g0-03 evidence (typed external-effect authority)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt`

- [x] **Step 1: Confirm each suite passes standalone**

Run each separately (`VESTRACE_RUNTIME_DATABASE_URL` set for all three):

- `cargo test -p vestrace-infrastructure --test embedding_dispatch_is_atomic` → 22 passed
- `cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic` → 44 passed
- `cargo test -p vestrace-infrastructure --test external_effect_repository` → 42 passed

- [x] **Step 2: Capture all three into one file**

```bash
{
  for t in embedding_dispatch_is_atomic provider_dispatch_is_atomic external_effect_repository; do
    printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test %s\n' "$t"
    DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test "$t"
    printf 'exit: %s\n' "$?"
  done
} > docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt`
Expected: three lines, each `exit: 0`.

---

### Task 3: Record g0-04 evidence (ModelRequestEvidence)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt`

- [x] **Step 1: Confirm each suite passes standalone**

- `DATABASE_URL=... VESTRACE_RUNTIME_DATABASE_URL=... cargo test -p vestrace-infrastructure --test model_request_evidence` → 32 passed. (Without `VESTRACE_RUNTIME_DATABASE_URL`, 26 of 32 fail — fixture precondition, not a product defect.)
- `DATABASE_URL=... VESTRACE_RUNTIME_DATABASE_URL=... cargo test --test model_request_semantic_observation` → 5 passed, including `loopback_observes_semantic_equality_with_the_production_adapter` (the spec-named loopback oracle).

- [x] **Step 2: Capture both into one file**

```bash
{
  printf '$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test model_request_evidence\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test model_request_evidence
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test --test model_request_semantic_observation\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test --test model_request_semantic_observation
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt`
Expected: two lines, each `exit: 0`.

---

### Task 4: Record g0-05 evidence (provider transport egress/PKI)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/transport-security-test.txt`

- [x] **Step 1: Confirm the suite passes standalone**

Run: `cargo test -p vestrace-infrastructure --test openai_transport_security`
Expected: 14 passed. No `DATABASE_URL` needed — the suite drives a local `std::net::TcpListener`, not a live remote endpoint (contrary to P05-D's original manifest reason for this criterion).

- [x] **Step 2: Capture it**

```bash
{
  printf '$ cargo test -p vestrace-infrastructure --test openai_transport_security\n'
  cargo test -p vestrace-infrastructure --test openai_transport_security
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/transport-security-test.txt
```

- [x] **Step 3: Confirm the captured exit line reads 0**

Run: `tail -1 docs/development-evidence/v1-g0-05-gate/transport-security-test.txt`
Expected: `exit: 0`

---

### Task 5: Record g0-06 evidence (connection/credential guards, occupancy, serialization)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt`

- [x] **Step 1: Confirm each suite passes standalone**

All with `DATABASE_URL` and `VESTRACE_RUNTIME_DATABASE_URL` set:

- `credential_intent_lifecycle` → 25 passed
- `credential_guards` → 6 passed
- `credential_activation` → 17 passed
- `credential_dispatch_lease` → 9 passed
- `connection_revision_lifecycle` → 6 passed
- `connection_mutation_is_atomic` → 5 passed
- `capability_grants` → 5 passed

- [x] **Step 2: Capture all seven into one file**

```bash
{
  for t in credential_intent_lifecycle credential_guards credential_activation credential_dispatch_lease connection_revision_lifecycle connection_mutation_is_atomic capability_grants; do
    printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test %s\n' "$t"
    DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test "$t"
    printf 'exit: %s\n' "$?"
  done
} > docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt`
Expected: seven lines, each `exit: 0`.

---

### Task 6: Record g0-07 evidence (admission, leases, throttle, no-retry-after-dispatch)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt`

- [x] **Step 1: Confirm each suite passes standalone**

- `provider_admission` → 12 passed
- `effect_outcome_delivery` → 4 passed
- `intent_crash_boundaries` → 15 passed

- [x] **Step 2: Capture all three into one file**

```bash
{
  for t in provider_admission effect_outcome_delivery intent_crash_boundaries; do
    printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test %s\n' "$t"
    DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test "$t"
    printf 'exit: %s\n' "$?"
  done
} > docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt`
Expected: three lines, each `exit: 0`.

---

### Task 7: Record g0-09 evidence (CredentialKeyCreationIntent lifecycle)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt`

**Note:** this is the same suite as g0-06's first entry, captured into its own dedicated file so each criterion's manifest entry stays self-contained (matching the one-file-per-criterion pattern P05-D through P05-F established), rather than one evidence file being cross-referenced by two criteria.

- [x] **Step 1: Confirm the suite passes standalone**

Run: `DATABASE_URL=... VESTRACE_RUNTIME_DATABASE_URL=... cargo test -p vestrace-infrastructure --test credential_intent_lifecycle`
Expected: 25 passed.

- [x] **Step 2: Capture it**

```bash
{
  printf '$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test credential_intent_lifecycle\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test credential_intent_lifecycle
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt
```

- [x] **Step 3: Confirm the captured exit line reads 0**

Run: `tail -1 docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt`
Expected: `exit: 0`

---

### Task 8: Record g0-11 evidence (pre-live credential abort + bound-Candidate abandon guard order)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt`

- [x] **Step 1: Confirm each suite passes standalone**

- `credential_activation` → 17 passed (covers pre-live credential abort paths)
- `embedding_transition_barriers` → 10 passed (covers bound-Candidate abandon and barrier supersession)

- [x] **Step 2: Capture both into one file**

```bash
{
  printf '$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test credential_activation\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test credential_activation
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test embedding_transition_barriers\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test embedding_transition_barriers
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt`
Expected: two lines, each `exit: 0`.

---

### Task 9: Wire the seven sources into the G0 manifest

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-gate.json`

- [x] **Step 1: Compute each file's digest**

Run: `sha256sum docs/development-evidence/v1-g0-05-gate/effect-authority-test.txt docs/development-evidence/v1-g0-05-gate/model-request-evidence-test.txt docs/development-evidence/v1-g0-05-gate/transport-security-test.txt docs/development-evidence/v1-g0-05-gate/connection-credential-guards-test.txt docs/development-evidence/v1-g0-05-gate/admission-leases-test.txt docs/development-evidence/v1-g0-05-gate/credential-intent-lifecycle-test.txt docs/development-evidence/v1-g0-05-gate/candidate-abandon-test.txt`

- [x] **Step 2: Update each of the seven entries**

Change `claim` from `"unknown"` to `"pass"`, write a reason naming the suites and what they prove, and add one `sources` entry per criterion with its digest, command, and `exit_code: 0` — following the exact shape P05-F used for g0-01/g0-02/g0-08/g0-18.

- [x] **Step 3: Run the collector**

Run: `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`
Expected: exit 1 (aggregate still `blocked`, g0-17), `counts.pass` risen from 5 to 12.

---

### Task 10: Qualify P05-G

**Files:**
- Create: `docs/development-evidence/v1-g0-05g-fresh-closure.md`
- Modify: `tests/p05_g0_gate.test.mjs` (the pinned pass-count assertion from P05-F needs updating to 12 and naming the seven new criteria, the same way P05-F updated it from P05-D's pinned 1)

- [x] **Step 1: Update the pinned pass-count assertion**

In `tests/p05_g0_gate.test.mjs`, extend `the recorded G0 evidence manifest is well formed and claims exactly the criteria closed so far` to assert `g0-03`, `g0-04`, `g0-05`, `g0-06`, `g0-07`, `g0-09`, and `g0-11` are `pass` with non-empty sources, and change `passed.length` from `5` to `12`.

- [x] **Step 2: Run the full gate set**

```
node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings
git diff --check
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs
node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json
```

Expected: every command exits 0 except the last, which exits 1 with `counts.pass: 12`.

- [x] **Step 3: Write the evidence doc**

Record which seven criteria moved to `pass`, the full suite/count table, the qualification-check results, and what remains open (g0-10, g0-12 through g0-16 unknown; g0-17 blocked — 7 criteria remaining before P05-H/I/J).

---

## Self-review

- Every criterion this plan claims traces to a suite confirmed passing standalone in this session, with the exact env vars each needs named per task — no step trusts the earlier combined-run numbers this session already found unreliable.
- g0-09 and part of g0-11 deliberately re-capture `credential_intent_lifecycle`/`credential_activation` into their own dedicated files rather than cross-referencing g0-06's file, keeping every criterion's evidence self-contained.
- No task touches a protected P01–P04 authority path or any migration file.
- Task 10 updates the same pinned-count assertion pattern P05-F established, so the next package (P05-H) inherits an accurate, currently-passing baseline rather than a stale one.

## Execution handoff

Execute Task 1 first. Tasks 2–8 may run in any order once Task 1 passes. Do not start P05-H until this plan's Task 10 evidence is recorded and pushed.
