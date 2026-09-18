# P05-F fresh G0 closure (Category A): measured result

**Recorded:** 2026-09-17, following `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05f-fresh-closure.md`.
**Acceptance boundary:** P05-F only. Closes G0 criteria g0-01, g0-02, g0-08, and g0-18 in `docs/development-evidence/v1-g0-05-gate.json` with fresh, digest-pinned evidence. It does not touch g0-17 or any other criterion, and it does not repair any failure — every suite it cites was already passing before this package ran.

## What changed and why each suite is sufficient

| Criterion | Spec wording (section 15) | Suite | Result | Why it is sufficient |
| --- | --- | --- | --- | --- |
| g0-01 | "default-deny route authority covers every current route, including access tokens" | `cargo test -p vestrace-http --lib` | 64/64 pass | `auth::tests::public_probe_exemption_matches_the_inventory` walks every entry of `route_inventory()` and asserts its public/auth-required classification, so it is exhaustive over the current route set, not a sample. `router::tests::unknown_transport_path_is_denied` independently proves a path outside the inventory is denied. |
| g0-02 | "mutation plus audit is atomic" | `DATABASE_URL=... cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic` | 7/7 pass | Exercises injected write-boundary failure across the shared mutation-plus-audit transaction authority and proves rollback. |
| g0-08 | "MaterialKeyCreationIntent state/receipt/attachment enforcement permits only Live ordinary/executable references" | `material_contract` (4/4), `material_intent_lifecycle` (9/9, with `VESTRACE_RUNTIME_DATABASE_URL` set), `material_vault_contract` (8/8) | 21/21 pass total | Together prove `ContentPrepared` stays internal until Bound promotion, guarded pre-live abort never touches a `ResultPrepared`, and raw SQL cannot publish or resurrect either path. |
| g0-18 | "pinned protocol lock and executable contract fixtures exist" | `protocol_a2a_lock` (1/1), `protocol_q1_manifest` (1/1), `protocol_q1_marker.test.mjs` + `p01_text_hygiene.test.mjs` (2/2) | 4/4 pass total | Proves the pinned A2A and q1 protocol locks are intact and the P01 executable fixtures (marker generation, text hygiene) still hold. |

## A correction made during this package

An early combined run (`cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic --test material_intent_lifecycle --test credential_intent_lifecycle --test capability_grants`) was read positionally, assuming Cargo runs `--test` targets in the order given on the command line. It does not — it appears to run them in an unspecified (observed: roughly alphabetical) order. This produced two wrong counts in an earlier draft of this package's plan (`governed_mutation_is_atomic` recorded as 5, `material_intent_lifecycle` recorded as 25). Both were caught before any evidence file was captured, by re-running each suite standalone: the true counts are 7 and 9 respectively, and `material_intent_lifecycle` additionally requires `VESTRACE_RUNTIME_DATABASE_URL` (three of its nine tests exercise the restricted runtime role and fail with a fixture-precondition error otherwise, not a product defect). Every count in the table above is from a standalone single-target run, captured into the evidence file at the moment it was verified.

## A process defect found and fixed

The G0 closure roadmap doc (`docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`) was written and pushed one commit before its path was admitted into `scripts/p05-scope.mjs`, which `verify-dirty-baseline.mjs` correctly caught as "new dirty path outside P05 scope." Fixed in a follow-up commit (`e0e5ffc`) that admitted the path retroactively. This package's own new paths were admitted in Task 1 before any of them were written, avoiding a repeat.

## Manifest change

`docs/development-evidence/v1-g0-05-gate.json`: g0-01, g0-02, g0-08, and g0-18 moved from `claim: "unknown"` to `claim: "pass"`, each with one `sources` entry pointing at its captured evidence file under `docs/development-evidence/v1-g0-05-gate/`.

## Qualification checks

| Command | Result |
| --- | --- |
| `node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs` | exit 0, 31/31 passed (one pre-existing assertion, `the recorded P05-D evidence manifest is well formed and claims no G0 pass`, pinned the manifest's pass count at exactly 1 and was updated to assert the new true count of 5 and the four newly-passing criteria by name; renamed to `the recorded G0 evidence manifest is well formed and claims exactly the criteria closed so far`) |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings` | exit 0 |
| `git diff --check` | exit 0 (only LF/CRLF line-ending notices, no whitespace error) |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | exit 0 |
| `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json` | exit 1 (expected — aggregate is not `pass`): `aggregate: "blocked"`, `counts: { pass: 5, blocked: 1, unknown: 13 }` |

## What remains open

Aggregate is `blocked`, not `pass`, because g0-17 is a partially-closed conjunction (see P05-D's own recorded reason, unchanged by this package). Thirteen criteria remain `unknown`: g0-03, g0-04, g0-05, g0-06, g0-07, g0-09, g0-10, g0-11, g0-12, g0-13, g0-14, g0-15, g0-16. Per `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`, these are sequenced as P05-G (P03 criteria: g0-03 through g0-07, g0-09, g0-11), P05-H (P04 Category-A conjuncts of g0-10/g0-14, plus g0-15), P05-I (DrainMutationPermit implementation for g0-12/g0-13 and the remaining g0-10 conjunct), and P05-J (g0-16 and the rest of g0-17, via the disposable-Compose/Linux-runner recipe). Browser-automation conjuncts of g0-10, g0-11, and g0-14 remain permanently blocked pending a browser-driving tool this session does not have.

P05-F makes no claim beyond the four criteria it closes: it does not claim G0, it does not claim P05 is complete, and it does not repair or reinterpret any criterion outside its own four.
