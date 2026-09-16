# P05-F Fresh G0 Closure (Category A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `unknown` claims for G0 criteria g0-01, g0-02, g0-08, and g0-18 in `docs/development-evidence/v1-g0-05-gate.json` with `pass`, backed by fresh, digest-pinned evidence from suites that already run clean today — without touching any protected P01–P04 authority.

**Architecture:** For each of the four criteria, capture the exact stdout of its already-passing test command to a new file under `docs/development-evidence/v1-g0-05-gate/`, compute that file's SHA-256, and add a `sources` entry to the criterion's manifest object with that path, command, digest, and `exit_code: 0`. `scripts/p05-g0-gate.mjs` is not modified — it already re-verifies the digest and exit code of anything the manifest names.

**Tech Stack:** Rust 1.85, Cargo, Node 22 test runner, PostgreSQL 17 (`vestrace-test-postgres` container).

**Spec:** `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md` (this package's parent index), which argues from `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 15, lines 933–934 (g0-01, g0-02), 940 (g0-08), and 950 (g0-18).

## Global Constraints

- `DATABASE_URL` for ordinary runs is `postgres://test:test@127.0.0.1:55432/vestrace_test`. `VESTRACE_RUNTIME_DATABASE_URL` for the restricted runtime role is `postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test`. Use `127.0.0.1`, never `localhost` (see `[[vestrace-test-postgres]]`).
- Do not modify migrations `0001`–`0215`, `scripts/verify-dirty-baseline.mjs`, or any path in `protectedAuthorityPaths`.
- Do not weaken, skip, or add a fallback to any test this package cites as evidence. If a cited suite fails on re-run, stop and report it — do not edit the suite to make it pass.
- Do not commit, push, deploy, or request secrets. (The user's own standing instruction to push after each verified task, recorded outside this plan, still applies once the task is independently verified.)
- A criterion may be claimed `pass` only when the collector (`node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`) independently confirms it — never by editing the `claim` field without a source the collector can verify.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | Admits this plan's own path and the four new evidence files. |
| `tests/p05_scope.test.mjs` | P05-F scope regression. |
| `docs/development-evidence/v1-g0-05-preflight.json` | Captured reviewed scope entry. |
| `docs/development-evidence/v1-g0-05-gate/route-authority-test.txt` | g0-01 evidence: `cargo test -p vestrace-http --lib`. |
| `docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt` | g0-02 evidence: `cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic`. |
| `docs/development-evidence/v1-g0-05-gate/material-intent-test.txt` | g0-08 evidence: three material-authority suites. |
| `docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt` | g0-18 evidence: protocol lock and P01 fixture suites. |
| `docs/development-evidence/v1-g0-05-gate.json` | Four criteria's `claim` moved from `unknown` to `pass`, each with its `sources` entry. |
| `docs/development-evidence/v1-g0-05f-fresh-closure.md` | P05-F observations and the resulting aggregate. |

---

### Task 1: Admit the P05-F implementation scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`

**Interfaces:**
- Consumes: nothing.
- Produces: `changeScopePaths` containing every path this plan writes.

- [x] **Step 1: Write the failing assertion**

Append to `tests/p05_scope.test.mjs`:

```javascript
test('P05-F admits its own plan and evidence paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05f-fresh-closure.md',
    'docs/development-evidence/v1-g0-05-gate/route-authority-test.txt',
    'docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt',
    'docs/development-evidence/v1-g0-05-gate/material-intent-test.txt',
    'docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt',
    'docs/development-evidence/v1-g0-05f-fresh-closure.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});
```

- [x] **Step 2: Run it and watch it fail**

Run: `node --test tests/p05_scope.test.mjs`
Expected: FAIL on `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05f-fresh-closure.md`.

- [x] **Step 3: Add the six paths to `changeScopePaths`**

In `scripts/p05-scope.mjs`, insert each path in its sorted position (plain ASCII `.sort()`, `-` before `/`):

- `docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt` and `.../material-intent-test.txt` and `.../protocol-lock-test.txt` and `.../route-authority-test.txt` — alongside the existing `docs/development-evidence/v1-g0-05-gate/*.txt` entries, in alphabetical order by filename.
- `docs/development-evidence/v1-g0-05f-fresh-closure.md` — immediately after `docs/development-evidence/v1-g0-05e-test-migrator.md` (or wherever ASCII order places it among the `v1-g0-05*` entries).
- `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05f-fresh-closure.md` — immediately after `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`.

- [x] **Step 4: Sync the preflight capture to the module, verbatim**

Write a temporary script at the repo root (create it, run it, delete it — do not leave it behind):

```javascript
// sync-preflight.mjs
import fs from 'node:fs';
import { changeScopePaths } from './scripts/p05-scope.mjs';
const path = 'docs/development-evidence/v1-g0-05-preflight.json';
const lines = fs.readFileSync(path, 'utf8').split('\n');
const start = lines.findIndex((l) => l === '  "change_scope_paths": [');
let end = start;
while (lines[end] !== '  ],') end += 1;
const block = ['  "change_scope_paths": [',
  ...changeScopePaths.map((p) => `    ${JSON.stringify(p)},`), '  ],'];
block[block.length - 2] = block[block.length - 2].replace(/,$/, '');
const out = [...lines.slice(0, start), ...block, ...lines.slice(end + 1)].join('\n');
JSON.parse(out);
fs.writeFileSync(path, out);
console.log(`preflight now carries ${changeScopePaths.length} scope paths`);
```

Run: `node sync-preflight.mjs`, confirm it prints `160` (154 + 6), then delete the script.

- [x] **Step 5: Record the amendment**

Add one entry to `scope_amendments` in the preflight, before the existing entries:

```json
{
  "authorized_at_utc": "2026-09-17T00:00:00.000Z",
  "authorized_by": "user standing authorization",
  "reason": "P05-F closes G0 criteria g0-01, g0-02, g0-08, and g0-18 with fresh digest-pinned evidence from suites confirmed passing this session, per docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md.",
  "paths": [
    "docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05f-fresh-closure.md",
    "docs/development-evidence/v1-g0-05-gate/route-authority-test.txt",
    "docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt",
    "docs/development-evidence/v1-g0-05-gate/material-intent-test.txt",
    "docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt",
    "docs/development-evidence/v1-g0-05f-fresh-closure.md"
  ]
}
```

- [x] **Step 6: Update the count assertion and run the suite**

Change `assert.equal(changeScopePaths.length, 154);` to `160` in `tests/p05_scope.test.mjs`.

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS, all tests.

- [x] **Step 7: Verify the baseline is still clean**

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: exit 0, no output.

---

### Task 2: Record g0-01 evidence (default-deny route authority)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/route-authority-test.txt`

**Interfaces:**
- Consumes: nothing new — `vestrace-http`'s existing lib test suite.
- Produces: the g0-01 source Task 6 wires into the manifest.

- [x] **Step 1: Confirm the suite passes before capturing it**

Run: `cargo test -p vestrace-http --lib`
Expected: `test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`, including `auth::tests::public_probe_exemption_matches_the_inventory` (walks every entry of `route_inventory()` and asserts its public/auth-required classification) and `router::tests::unknown_transport_path_is_denied` (asserts any path outside the inventory is denied). If the count or either named test differs, stop — the criterion's evidence would misdescribe what the suite proves.

- [x] **Step 2: Capture it**

```bash
{
  printf '$ cargo test -p vestrace-http --lib\n'
  cargo test -p vestrace-http --lib
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/route-authority-test.txt
```

- [x] **Step 3: Confirm the captured exit line reads 0**

Run: `tail -1 docs/development-evidence/v1-g0-05-gate/route-authority-test.txt`
Expected: `exit: 0`

---

### Task 3: Record g0-02 evidence (mutation plus audit is atomic)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt`

- [x] **Step 1: Confirm the suite passes before capturing it**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic`
Expected: `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. (An earlier combined multi-target run in this session misattributed counts across targets by assuming Cargo runs `--test` binaries in flag order — it does not. Trust only a single-target run's own count.)

- [x] **Step 2: Capture it**

```bash
{
  printf '$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt
```

- [x] **Step 3: Confirm the captured exit line reads 0**

Run: `tail -1 docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt`
Expected: `exit: 0`

---

### Task 4: Record g0-08 evidence (MaterialKeyCreationIntent)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/material-intent-test.txt`

**Interfaces:**
- Consumes: `material_contract` (domain, no database), `material_intent_lifecycle` and `material_vault_contract` (infrastructure, PostgreSQL-backed).

- [x] **Step 1: Confirm all three suites pass before capturing them**

Run: `cargo test -p vestrace-domain --test material_contract`
Expected: `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_intent_lifecycle`
Expected: `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. Without `VESTRACE_RUNTIME_DATABASE_URL` set, `runtime_role_direct_write_is_refused` and two others fail — that is a fixture precondition (the suite exercises the restricted runtime role), not a product defect.

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_vault_contract`
Expected: `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. (Without `DATABASE_URL` set, `unwrap_refuses_after_prepare_erasure_without_consulting_postgres` fails with `DATABASE_URL must be set` — that is a fixture precondition, not a product defect; do not record that run.)

- [x] **Step 2: Capture all three into one file**

```bash
{
  printf '$ cargo test -p vestrace-domain --test material_contract\n'
  cargo test -p vestrace-domain --test material_contract
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_intent_lifecycle\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_intent_lifecycle
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_vault_contract\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_vault_contract
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/material-intent-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/material-intent-test.txt`
Expected: three lines, each `exit: 0`.

---

### Task 5: Record g0-18 evidence (protocol lock and executable contract fixtures)

**Files:**
- Create: `docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt`

**Interfaces:**
- Consumes: `protocol_a2a_lock`, `protocol_q1_manifest` (Cargo, no database), `tests/protocol_q1_marker.test.mjs`, `tests/p01_text_hygiene.test.mjs` (Node).

- [x] **Step 1: Confirm all suites pass before capturing them**

Run: `cargo test --test protocol_a2a_lock --test protocol_q1_manifest`
Expected: two `test result: ok. 1 passed; 0 failed` blocks, one per binary.

Run: `node --test tests/protocol_q1_marker.test.mjs tests/p01_text_hygiene.test.mjs`
Expected: `# pass 2`, `# fail 0`.

- [x] **Step 2: Capture both into one file**

```bash
{
  printf '$ cargo test --test protocol_a2a_lock --test protocol_q1_manifest\n'
  cargo test --test protocol_a2a_lock --test protocol_q1_manifest
  printf 'exit: %s\n' "$?"
  printf '\n$ node --test tests/protocol_q1_marker.test.mjs tests/p01_text_hygiene.test.mjs\n'
  node --test tests/protocol_q1_marker.test.mjs tests/p01_text_hygiene.test.mjs
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt
```

- [x] **Step 3: Confirm every captured exit line reads 0**

Run: `grep '^exit:' docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt`
Expected: two lines, each `exit: 0`.

---

### Task 6: Wire the four sources into the G0 manifest

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-gate.json`

**Interfaces:**
- Consumes: the four evidence files from Tasks 2–5.
- Produces: a manifest the collector reports as `pass` for g0-01, g0-02, g0-08, g0-18.

- [x] **Step 1: Compute each file's digest**

Run: `sha256sum docs/development-evidence/v1-g0-05-gate/route-authority-test.txt docs/development-evidence/v1-g0-05-gate/mutation-audit-test.txt docs/development-evidence/v1-g0-05-gate/material-intent-test.txt docs/development-evidence/v1-g0-05-gate/protocol-lock-test.txt`

Record each printed digest for the matching path in the next step.

- [x] **Step 2: Update the g0-01 entry**

In `docs/development-evidence/v1-g0-05-gate.json`, change the `g0-01` object from:

```json
{
  "id": "g0-01",
  "claim": "unknown",
  "reason": "Route authority is P01 work. P05-D started no server and exercised no route.",
  "sources": []
}
```

to:

```json
{
  "id": "g0-01",
  "claim": "pass",
  "reason": "vestrace-http's lib suite exhaustively walks route_inventory() (public_probe_exemption_matches_the_inventory) and independently proves any path outside it is denied (unknown_transport_path_is_denied), re-run fresh 2026-09-17.",
  "sources": [
    {
      "path": "v1-g0-05-gate/route-authority-test.txt",
      "sha256": "<digest from Step 1>",
      "command": "cargo test -p vestrace-http --lib",
      "exit_code": 0
    }
  ]
}
```

- [x] **Step 3: Update the g0-02, g0-08, and g0-18 entries the same way**

Follow the same pattern for each:

- `g0-02`: `claim: "pass"`, reason names `governed_mutation_is_atomic`, one source at `v1-g0-05-gate/mutation-audit-test.txt` with `command: "DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic"`.
- `g0-08`: `claim: "pass"`, reason names all three suites, one source at `v1-g0-05-gate/material-intent-test.txt` with `command: "cargo test -p vestrace-domain --test material_contract && DATABASE_URL=... cargo test -p vestrace-infrastructure --test material_intent_lifecycle --test material_vault_contract"` (the file records all three invocations; the `command` field documents what was run for a human reader — the collector only re-hashes the file and checks the recorded `exit_code`).
- `g0-18`: `claim: "pass"`, reason names the protocol-lock and P01-fixture suites, one source at `v1-g0-05-gate/protocol-lock-test.txt`.

Use the exact digests from Step 1 for each.

- [x] **Step 4: Run the collector**

Run: `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`
Expected: exit 1 (aggregate still `blocked` because of g0-17), but the JSON now shows `g0-01`, `g0-02`, `g0-08`, and `g0-18` with `"status": "pass"`, and `counts.pass` risen from 1 to 5.

If any of the four still reads `blocked` or `unknown`, the reason names exactly which check failed (missing file, digest mismatch, or nonzero exit) — fix that specific mismatch, do not guess.

---

### Task 7: Qualify P05-F

**Files:**
- Create: `docs/development-evidence/v1-g0-05f-fresh-closure.md`

- [x] **Step 1: Run the full gate set**

```
node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings
git diff --check
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs
node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json
```

Expected: every command exits 0 except the last, which exits 1 with `counts.pass: 5` as established in Task 6.

- [x] **Step 2: Write the evidence doc**

Record in `docs/development-evidence/v1-g0-05f-fresh-closure.md`: which four criteria moved from `unknown` to `pass` and why each suite is sufficient evidence for its criterion's exact spec wording; the full qualification-check table from Step 1; and an explicit statement that the aggregate remains `blocked` (g0-17) with 14 criteria still `unknown` or `blocked`, listing them by ID so the next sub-package (P05-G) knows exactly what remains.

- [x] **Step 3: Mark P05-F complete only if every check in Step 1 passed**

Record actual exits and the collector's `counts` object verbatim.

---

## Self-review

- Every criterion this plan claims (g0-01, g0-02, g0-08, g0-18) traces to a suite this session already confirmed passing, with exact counts — no step asks a future executor to trust an unverified number.
- Task 6's manifest edits follow the exact shape `scripts/p05-g0-gate.mjs`'s `validateSource` requires (`path`, `command`, `sha256`, `exit_code`), matching the g0-17/g0-19 entries already in the file.
- No task touches a protected P01–P04 authority path or any migration file.
- The evidence doc in Task 7 explicitly states what remains open, so P05-G's plan starts from an accurate baseline rather than re-deriving it.

## Execution handoff

Execute Task 1 first. Tasks 2–5 may run in any order once Task 1's scope admission passes. Do not start P05-G until this plan's Task 7 evidence is recorded and pushed.
