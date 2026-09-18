# P05-H Fresh G0 Closure (P04 Non-Browser Conjuncts) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `unknown` claims for G0 criteria g0-10, g0-14, and g0-15 in `docs/development-evidence/v1-g0-05-gate.json` with the most honest status the fresh evidence supports — which, after investigation, is `blocked` for all three, not `pass`.

**Architecture:** Same capture-and-wire pattern as P05-F/P05-G. The difference from those two packages: every criterion here is a conjunction with at least one conjunct this environment cannot prove (a browser oracle, or a mechanism that appears unbuilt), so each manifest entry names what the fresh evidence proves and what specifically remains open, following the exact pattern P05-D already established for g0-17.

**Tech Stack:** Rust 1.85, Cargo, PostgreSQL 17.

**Spec:** `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md` (parent index), which argues from `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 15, lines 942 (g0-10), 946 (g0-14), 947 (g0-15).

## Global Constraints

- `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`, `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test`. Use `127.0.0.1`, never `localhost`.
- Never trust a combined `cargo test --test A --test B` run's positional output for per-suite counts (P05-F finding). Capture each target with its own separate invocation, verified standalone first.
- A criterion whose spec wording names a conjunct this environment cannot produce (browser automation, or a mechanism a targeted search finds no implementation of) must be claimed `blocked`, never `pass`, even when every suite this package can run is green. Do not round a partial conjunction up.
- Before claiming any conjunct "not built," search for it the same way P04's own evidence doc did for `DrainMutationPermit` (grep across `crates/`, not just the test directory) — an absent test does not by itself prove an absent mechanism, but an absent mechanism after a real search is worth recording as a finding, not silently working around.
- Do not modify migrations `0001`–`0215`, `scripts/verify-dirty-baseline.mjs`, or any protected path.
- Do not weaken, skip, or add a fallback to any test this package cites.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | Admits this plan's own path and the three new evidence files. |
| `tests/p05_scope.test.mjs` | P05-H scope regression. |
| `docs/development-evidence/v1-g0-05-preflight.json` | Captured reviewed scope entry. |
| `docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt` | g0-10 evidence. |
| `docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt` | g0-14 evidence. |
| `docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt` | g0-15 evidence. |
| `docs/development-evidence/v1-g0-05-gate.json` | Three criteria's `claim` moved from `unknown` to `blocked`, each with sources and a reason naming exactly what remains open. |
| `docs/development-evidence/v1-g0-05h-fresh-closure.md` | P05-H observations, including the `evidence_is_readable` finding and the dictionary-resistance search result. |

---

### Task 1: Admit the P05-H implementation scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`

- [x] **Step 1: Write the failing assertion**

Append to `tests/p05_scope.test.mjs`:

```javascript
test('P05-H admits its own plan and evidence paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05h-fresh-closure.md',
    'docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt',
    'docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt',
    'docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt',
    'docs/development-evidence/v1-g0-05h-fresh-closure.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});
```

- [x] **Step 2: Run it and watch it fail**

Run: `node --test tests/p05_scope.test.mjs`
Expected: FAIL.

- [x] **Step 3: Add the five paths to `changeScopePaths`, sorted**

Insert the three `docs/development-evidence/v1-g0-05-gate/*.txt` entries alphabetically (`embedding-transition-test.txt` before `material-intent-test.txt`, `material-lifecycle-test.txt` immediately after `material-intent-test.txt`, `retrieval-retry-test.txt` between `readiness-test.txt` and `route-authority-test.txt`), and the two `v1-g0-05h-fresh-closure` doc paths immediately after their `05g` counterparts.

- [x] **Step 4: Sync the preflight capture to the module, verbatim**

Same disposable `sync-preflight.mjs` recipe as P05-F/G. Confirm it prints `174` (169 + 5).

- [x] **Step 5: Record the amendment**

Add one `scope_amendments` entry naming these five paths and this plan.

- [x] **Step 6: Update the count assertion and run the suite**

Change `assert.equal(changeScopePaths.length, 169);` to `174`.

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS, all tests.

- [x] **Step 7: Verify the baseline is still clean**

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: exit 0.

---

### Task 2: Record g0-10 evidence (EmbeddingSpaceTransition, non-browser conjuncts)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt`

**Note:** this criterion cannot be claimed `pass`. `DrainMutationPermit` and `Quiescing` — the "completion-only ... reconciliation" and "no post-freeze write" conjuncts — do not exist anywhere in the repository, in Rust or SQL (confirmed by P04's own evidence doc at `docs/development-evidence/v1-g0-04-embedding-transition-foundation.md:6417-6420`, and by a fresh grep in this package). The browser oracle conjunct is also unavailable this session. The claim is `blocked`, naming both gaps.

- [x] **Step 1: Confirm each suite passes standalone**

All with `DATABASE_URL` and `VESTRACE_RUNTIME_DATABASE_URL` set:

- `embedding_transition_planning` → 7 passed
- `embedding_transition_barriers` → 10 passed
- `embedding_transition_activation` → 14 passed
- `embedding_space_isolation` → 4 passed

- [x] **Step 2: Re-confirm `DrainMutationPermit`/`Quiescing` are absent**

Run: `grep -rn "DrainMutationPermit\|Quiescing" crates/`
Expected: no matches at all. (Confirmed: zero matches, not even under an unrelated name — `restore_cutover.rs`'s freeze machinery is named `SourceFreezePoint`, not `SourceQuiescing` as an earlier draft of this plan assumed without checking. `vestrace-application`'s `DrainReport` outbox type is a separate, unrelated concept found by the broader `Drain` search, not by this exact search.)

- [x] **Step 3: Capture the four suites into one file**

```bash
{
  for t in embedding_transition_planning embedding_transition_barriers embedding_transition_activation embedding_space_isolation; do
    printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test %s\n' "$t"
    DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test "$t"
    printf 'exit: %s\n' "$?"
  done
} > docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt
```

- [x] **Step 4: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt`
Expected: four lines, each `exit: 0`.

---

### Task 3: Record g0-14 evidence (duplicate-charge/retry-pinning/retrieval fences, non-browser conjuncts)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt`

**Note:** the spec's clause 946 bullet ends "PostgreSQL/fault/browser tests refuse fallback ..." — the browser conjunct is unavailable this session, so this stays `blocked`, naming the component/PostgreSQL conjuncts as proven and the browser conjunct as the reason it is not `pass`.

- [x] **Step 1: Confirm each suite passes standalone**

- `retrieval_generation_fence` → 4 passed
- `embedding_retrieval_results` → 21 passed (includes `the_retry_queue_holds_only_unspent_confirmed_changes`, the exact "authorized one-successor current-generation retry chain" the clause names)
- `embedding_result_finalization` → 17 passed
- `embedding_result_preparation` → 14 passed

- [x] **Step 2: Capture all four into one file**

```bash
{
  for t in retrieval_generation_fence embedding_retrieval_results embedding_result_finalization embedding_result_preparation; do
    printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test %s\n' "$t"
    DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test "$t"
    printf 'exit: %s\n' "$?"
  done
} > docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt`
Expected: four lines, each `exit: 0`.

---

### Task 4: Investigate and record g0-15 evidence (append-only evidence, material lifecycle)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt`

**Note:** this criterion also cannot be claimed `pass`. Two findings from this package's own investigation:

1. `cargo test -p vestrace-domain --test evidence_is_readable` fails: `every_recorded_field_can_be_read` reports `WitnessStateV1.generation_lineage_state`, `WitnessAdvance.event_kind`, and `WitnessAdvance.next_state` (all in `crates/vestrace-domain/src/installation_safety.rs`) are recorded fields with no reader. This is a genuine, pre-existing failure unrelated to P05-H's own changes — do not fix it here, name it as follow-up.
2. A search for the spec's "dictionary-resistant" comparison-scope mechanism (`grep -rni "dictionary" crates/`, `grep -rni "comparison.?scope" crates/`) returns no matches anywhere in the codebase. Unlike `DrainMutationPermit`, no prior evidence doc records this as a deliberate omission — this package cannot conclude whether it is unbuilt or built under an unexpected name, only that a direct search finds nothing. Record this precisely as "not found by search," not as "not built."

- [x] **Step 1: Confirm each suite passes standalone**

- `erasure_is_one_way` → 16 passed
- `erasure_holds_no_lock_during_vault_call` → 3 passed
- `invariant_observer_contract` → 2 passed
- `material_vault_contract` → 8 passed (with `DATABASE_URL` set)

- [x] **Step 2: Re-run the searches from the note above and record their exact output**

```bash
grep -rn "DictionaryResistant\|dictionary" crates/ || true
grep -rni "comparison.?scope" crates/ || true
```

Record the literal output (or "no matches") in the evidence doc — do not paraphrase.

- [x] **Step 3: Capture the four passing suites into one file**

```bash
{
  printf '$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test erasure_is_one_way\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test erasure_is_one_way
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test erasure_holds_no_lock_during_vault_call\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test erasure_holds_no_lock_during_vault_call
  printf 'exit: %s\n' "$?"
  printf '\n$ cargo test -p vestrace-infrastructure --test invariant_observer_contract\n'
  cargo test -p vestrace-infrastructure --test invariant_observer_contract
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_vault_contract\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_vault_contract
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt
```

- [x] **Step 4: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt`
Expected: four lines, each `exit: 0`. (`evidence_is_readable` is deliberately not in this file — it is not evidence for a `pass` or `blocked` claim here, it is a separately named follow-up defect.)

---

### Task 5: Wire the three sources into the G0 manifest as `blocked`

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-gate.json`

- [x] **Step 1: Compute each file's digest**

Run: `sha256sum docs/development-evidence/v1-g0-05-gate/embedding-transition-test.txt docs/development-evidence/v1-g0-05-gate/retrieval-retry-test.txt docs/development-evidence/v1-g0-05-gate/material-lifecycle-test.txt`

- [x] **Step 2: Update g0-10**

`claim: "blocked"`, reason: names the four passing suites and what they prove, then states `DrainMutationPermit`/`Quiescing` do not exist in the repository and the browser oracle is unavailable this session, so the conjunction is not closed. One `sources` entry.

- [x] **Step 3: Update g0-14**

`claim: "blocked"`, reason: names the four passing suites, then states the browser conjunct named by clause 946 is unavailable this session. One `sources` entry.

- [x] **Step 4: Update g0-15**

`claim: "blocked"`, reason: names the four passing suites and what they prove (one-way material lifecycle, vault-call lock discipline), then states a search for the dictionary-resistant comparison-scope mechanism found no matches, so that conjunct is neither confirmed built nor confirmed absent, and separately names `evidence_is_readable`'s failure as an unrelated pre-existing defect this package does not repair. One `sources` entry (the `evidence_is_readable` failure is not a source here — it is not evidence for this claim, it is follow-up).

- [x] **Step 5: Run the collector**

Run: `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`
Expected: exit 1, aggregate `blocked` (unchanged), `counts.blocked` risen from 1 to 4, `counts.unknown` fallen from 6 to 3 (`g0-12`, `g0-13`, `g0-16` remain), `counts.pass` unchanged at 12.

---

### Task 6: Qualify P05-H

**Files:**
- Create: `docs/development-evidence/v1-g0-05h-fresh-closure.md`
- Modify: `tests/p05_g0_gate.test.mjs` (extend the pinned assertion to name g0-10/g0-14/g0-15 as `blocked` with non-empty sources; `passed.length` stays `12`, add a new assertion for `blocked` count)

- [x] **Step 1: Extend the pinned manifest assertion**

In `tests/p05_g0_gate.test.mjs`, add to `the recorded G0 evidence manifest is well formed and claims exactly the criteria closed so far`:

```javascript
// P05-H moved three more from unknown to blocked -- honest partial-conjunction
// evidence, not a pass. Each still names what it ran.
for (const id of ['g0-10', 'g0-14', 'g0-15']) {
  assert.equal(byId.get(id).status, 'blocked', id);
  assert.ok(byId.get(id).sources.length > 0, `${id} names what it ran`);
}

const blocked = report.criteria.filter((entry) => entry.status === 'blocked');
assert.equal(blocked.length, 4, 'g0-17 plus the three P05-H partial conjunctions');
```

- [x] **Step 2: Run the full gate set**

```
node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings
git diff --check
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs
node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json
```

Expected: every command exits 0 except the last, which exits 1 with `counts: { pass: 12, blocked: 4, unknown: 3 }`.

- [x] **Step 3: Write the evidence doc**

Record: why g0-10, g0-14, and g0-15 became `blocked` rather than `pass` (the roadmap's original Category-A classification for g0-15 was wrong — corrected here); the `evidence_is_readable` failure as named follow-up; the literal grep output for `DrainMutationPermit`/`Quiescing` and the dictionary-resistance search; the qualification-check table; and what remains open (g0-12, g0-13, g0-16 unknown; g0-10, g0-14, g0-15, g0-17 blocked).

---

## Self-review

- Every suite cited traces to a standalone confirmed run, per the discipline P05-F/G established.
- Neither g0-10 nor g0-14 nor g0-15 is claimed `pass` — each names its open conjunct precisely, matching the collector's own design intent (a partial conjunction is `blocked`, not rounded up).
- The g0-15 reclassification (Category A in the parent roadmap turned out to have an unproven conjunct) is recorded explicitly so `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`'s criterion table is understood as superseded on this point by this plan's evidence doc.
- `evidence_is_readable`'s failure is recorded as follow-up, not fixed and not used to block a claim it is not evidence for.

## Execution handoff

Execute Task 1 first. Tasks 2–4 may run in any order once Task 1 passes. Do not start P05-I until this plan's Task 6 evidence is recorded and pushed.
