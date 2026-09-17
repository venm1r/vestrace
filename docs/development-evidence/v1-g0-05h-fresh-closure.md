# P05-H fresh G0 closure (P04 non-browser conjuncts): measured result

**Recorded:** 2026-09-17, following `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05h-fresh-closure.md`.
**Acceptance boundary:** P05-H only. Closes G0 criteria g0-10, g0-14, and g0-15 in `docs/development-evidence/v1-g0-05-gate.json` — as `blocked`, not `pass`. This corrects the parent roadmap's original classification of g0-15 as pure "Category A" (fully closable): investigation found it is a conjunction with an unproven conjunct, same as g0-10 and g0-14.

## Why all three became `blocked`, not `pass`

The parent roadmap (`docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`) already flagged g0-10 and g0-14 as "mixed: A + B/C" because their spec bullets explicitly name a browser oracle. Investigating g0-15 during this package found the same pattern was missed there:

| Criterion | Proven conjuncts | Unproven conjunct | Why |
| --- | --- | --- | --- |
| g0-10 | Independently-exact spaces, opaque recipes, immutable batch identity, recipe-granular carry, only-current classification | `DrainMutationPermit`/`Quiescing` reconciliation; browser oracle | Confirmed absent from the repository by `grep -rn "DrainMutationPermit\|Quiescing" crates/` — zero matches, not even under an unrelated name. (An earlier draft of this plan assumed `restore_cutover.rs` used `SourceQuiescing`; it actually uses `SourceFreezePoint`, a different name entirely — corrected before this evidence was recorded.) Browser oracle unavailable this session. |
| g0-14 | Atomic result/Succeeded or provider-success-plus-generation-changed ordering; authorized one-successor retry chain | Browser oracle | Spec clause 946 explicitly lists "PostgreSQL/fault/browser tests." |
| g0-15 | One-way `Live -> ErasurePrepared -> Tombstoned` material lifecycle; vault-call lock discipline | Dictionary-resistant erasure-bound comparison-scope mechanism | `grep -rni "dictionary"` and `grep -rni "comparison.scope"` across `crates/` both return zero matches. Unlike `DrainMutationPermit`, no prior evidence doc records this as a deliberate omission, so this package can only report "not found by search," not "confirmed unbuilt." |

## Suite results

| Criterion | Suites | Result |
| --- | --- | --- |
| g0-10 | `embedding_transition_planning` (7), `embedding_transition_barriers` (10), `embedding_transition_activation` (14), `embedding_space_isolation` (4) | 35/35 pass |
| g0-14 | `retrieval_generation_fence` (4), `embedding_retrieval_results` (21), `embedding_result_finalization` (17), `embedding_result_preparation` (14) | 56/56 pass |
| g0-15 | `erasure_is_one_way` (16), `erasure_holds_no_lock_during_vault_call` (3), `invariant_observer_contract` (2), `material_vault_contract` (8) | 29/29 pass |

Every count is from a standalone single-target run. Two capture mistakes were caught before the evidence file was finalized: the first `material-lifecycle-test.txt` capture omitted `DATABASE_URL` for `erasure_is_one_way`/`erasure_holds_no_lock_during_vault_call` (16 tests failed to connect at all), and the corrected retry still omitted `VESTRACE_RUNTIME_DATABASE_URL` for `erasure_is_one_way` specifically (`raw_sql_cannot_resurrect_a_tombstoned_identity` and one other test need the restricted runtime role). The final capture with both variables set correctly per suite is clean.

## A genuine defect found and not repaired

`cargo test -p vestrace-domain --test evidence_is_readable` fails: `every_recorded_field_can_be_read` reports that `WitnessStateV1.generation_lineage_state`, `WitnessAdvance.event_kind`, and `WitnessAdvance.next_state` (all in `crates/vestrace-domain/src/installation_safety.rs`) are recorded fields with no reader. This is real and pre-existing, unrelated to any P05-F/G/H change. It is not cited as evidence for g0-15 or any other criterion — it does not bear on the claim made — and is recorded here purely as follow-up work for whichever package next touches `installation_safety.rs` (likely P05-J, which closes g0-16/g0-17 via the same safety-authority machinery).

## Manifest change

`docs/development-evidence/v1-g0-05-gate.json`: g0-10, g0-14, and g0-15 moved from `claim: "unknown"` to `claim: "blocked"`, each with one `sources` entry and a reason naming both what is proven and what remains open.

## Qualification checks

| Command | Result |
| --- | --- |
| `node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs` | exit 0, 33/33 passed (the pinned manifest assertion was extended to assert g0-10/g0-14/g0-15 are `blocked` with sources, and a new `blocked.length === 4` assertion added) |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings` | exit 0 |
| `git diff --check` | exit 0 (only LF/CRLF line-ending notices) |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | exit 0 |
| `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json` | exit 1 (expected): `aggregate: "blocked"`, `counts: { pass: 12, blocked: 4, unknown: 3 }` |

## What remains open

Three criteria remain `unknown`: g0-12, g0-13, g0-16. Four are `blocked`: g0-10, g0-14, g0-15, g0-17. Per the parent roadmap:

- **P05-I** implements `DrainMutationPermit`/`Quiescing` to close g0-12, g0-13, and complete g0-10's remaining conjunct (minus its browser oracle, which stays blocked regardless).
- **P05-J** closes g0-16 and the rest of g0-17 via the disposable-Compose/Linux-runner recipe, and is the natural place to also address the `evidence_is_readable` defect named above, since it touches the same `installation_safety.rs` machinery.
- g0-10's and g0-14's browser-automation conjuncts, and g0-15's dictionary-resistance conjunct, remain open regardless of P05-I/J: the first two need a browser-driving tool this session does not have, and the third needs either a targeted design investigation or an operator confirming the mechanism's actual name/location.

P05-H makes no claim beyond `blocked` for its three criteria, and repairs nothing it found.
