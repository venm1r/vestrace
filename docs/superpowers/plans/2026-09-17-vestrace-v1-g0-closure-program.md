# V1 G0 Closure Program

> **For agentic workers:** This is a scoping and sequencing index, not a detailed task-by-task plan. Per the gate-program's Package Planning and Review Rule ("write the detailed plan only after every predecessor package has accepted interfaces and persisted evidence"), each sub-package below gets its own bite-sized RED-to-GREEN plan written immediately before it executes, following superpowers:writing-plans. Do not skip a sub-package's own plan and start editing code from this index alone.

**Goal:** Move `scripts/p05-g0-gate.mjs`'s aggregate result for `docs/development-evidence/v1-g0-05-gate.json` as far toward `pass` as this environment honestly allows, by replacing each `unknown`/stale-`blocked` criterion with fresh, digest-pinned evidence — never by editing the manifest to point at old files, and never by inheriting a pass from an earlier package's narrative.

**Why now:** P05-E (`e7aa4f0`) removed the migration-0209 barrier that silently prevented every `#[sqlx::test]` suite from reaching its own assertions since P05-A. That barrier, not missing implementation, is why most G0 criteria still read `unknown` — P01 through P04's own suites mostly still pass today, they just haven't been re-run and recorded in the collector's digest-pinned format since the barrier lifted. This was confirmed empirically this session (see "Ground truth gathered this session" below), not assumed.

**Non-goal:** This program does not re-litigate P01-P05's design. It closes evidence gaps and, where a genuine gap in implementation is found (see g0-12/g0-13), it says so and scopes the smallest correct fix — it does not redesign the feature.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md`, section 15 "G0 — security and reproducible baseline" (lines 931–951), frozen SHA-256 `B31B5BE62504E1A65F411CD31B446CD41B3D032B7282F1AA42907706EF9C1473`.

**Parent plan:** `docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md` (P05 exit evidence: "aggregate G0 gate result"). This closure program is P05's remaining internal work, sequenced as further P05 sub-packages (P05-F onward), the same way P05-A through P05-E were.

## Global Constraints

- Do not modify migrations `0001` through `0215`, any P01–P05 authority path already accepted, or `scripts/verify-dirty-baseline.mjs` — same boundary every prior P05 sub-package observed.
- A criterion may be claimed `pass` in `v1-g0-05-gate.json` only when every one of its named sources is a real file with a matching SHA-256 and a recorded `exit_code: 0`, per `scripts/p05-g0-gate.mjs`'s `collect()`. Prose is never proof; an inherited old evidence file is never a source.
- Where the spec bullet names an oracle type this environment cannot produce (browser automation — no Playwright/browser tool is connected this session; a live independent remote provider with real credentials), the honest claim is `blocked` with the missing oracle named exactly, never `pass` on the conjuncts that are available and never silent omission.
- Each sub-package amends `scripts/p05-scope.mjs` and the preflight capture first, exactly as P05-A through P05-E did, before any implementation write.
- No test is deleted, weakened, or given a fallback value to make a criterion read `pass`.
- Record what is *not* claimed as carefully as what is, in both the sub-package's evidence doc and its commit message.

---

## Ground truth gathered this session (2026-09-17)

Before sequencing, the following suites were re-run fresh against the local `vestrace-test-postgres` container (`DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`, `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test`), *after* P05-E's `HISTORICAL_MIGRATOR` fix:

| Suite | Result | Corrects |
| --- | --- | --- |
| `cargo test -p vestrace-infrastructure --test embedding_dispatch_is_atomic` | 22/22 pass | `docs/development-evidence/v1-g0-04-embedding-transition-foundation.md` clauses 935/939 recorded 4 of these as **red**, as of commit `1eb0925`. Two later P04 commits (`b3d4cea`, `26efdd9`, both before P05 began) fixed the dispatch path. The red figure is stale. |
| `cargo test -p vestrace-infrastructure --test embedding_space_isolation` | 4/4 pass | Same doc's clause 942 recorded 3 of 4 **red**. The failure this session reproduced first (before `VESTRACE_RUNTIME_DATABASE_URL` was set) was only a fixture precondition, not a product defect; with the role set, all four pass. |
| `cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic --test material_intent_lifecycle --test credential_intent_lifecycle --test capability_grants` | 5/5, 25/25, 7/7, 9/9 pass | No prior claim; establishes current P01/P02/P03 baseline is healthy. |
| `cargo test --test authorization --test protocol_a2a_lock --test protocol_q1_manifest` | all pass | Establishes P01 protocol-lock and route-authority tests still run clean. |
| `cargo test -p vestrace-http --lib` | 64/64 pass, incl. `unknown_transport_path_is_denied`, `public_probe_exemption_matches_the_inventory` | Direct evidence for the default-deny route claim (g0-01), at unit-test grain. |

**Two genuine gaps were found, not fixed by re-running anything:**

- `DrainMutationPermit` and `Quiescing` (g0-12, g0-13, and the "no post-freeze write" / "readiness oracles" rows of g0-10) **do not exist anywhere in the repository**, in Rust or SQL — confirmed by grep across `crates/`. This was already recorded honestly in `docs/development-evidence/v1-g0-04-embedding-transition-foundation.md:6417-6420`: *"They belong to the backup/freeze machinery that G0 also names, and P04 does not build it."* Closing g0-12/g0-13 is real implementation work, not evidence recovery.
- `crates/vestrace-domain/src/embedding/readiness.rs` defines `EmbeddingReadinessReason`, which is a **different** readiness concept (why a Models listing can't answer readiness — used by `vestrace-http/src/api/models.rs`) from the "readiness oracles" the P04 evidence doc marked **not built** for the freeze/drain conjunct. Do not conflate the two when scoping P05-H below.

`crates/vestrace-infrastructure/tests/openai_transport_security.rs` was inspected and uses a local `TcpListener`, not a live remote endpoint — so g0-05's PKI/redirect/proxy-refusal conjuncts are testable headlessly, contrary to P05-D's recorded reason ("needs a remote endpoint").

---

## Criterion map

Categories: **A** — fresh re-run should suffice, no new code; **B** — genuine implementation gap; **C** — needs an oracle this environment cannot produce (browser automation, or a real independent remote endpoint with live credentials); **D** — achievable but needs the heavier disposable-PostgreSQL/Linux-runner recipe P05-A through P05-D already used.

| Criterion | Owner | Category | Candidate suites (this session confirmed passing unless noted) |
| --- | --- | --- | --- |
| g0-01 route authority | P01 | A | `vestrace-http --lib` (64/64incl. inventory-denial tests), `tests/authorization.rs` |
| g0-02 mutation+audit atomic | P01 | A | `governed_mutation_is_atomic` (5/5), `connection_mutation_is_atomic` |
| g0-03 typed external-effect authority | P03/P04 | A | `embedding_dispatch_is_atomic` (22/22), `provider_dispatch_is_atomic`, `external_effect_repository` |
| g0-04 ModelRequestEvidence | P03/P04 | A | `model_request_evidence` (32/32 per prior doc), `model_request_semantic_observation` (loopback, not live-remote) |
| g0-05 provider transport egress/PKI | P03 | A | `openai_transport_security` (confirmed: local `TcpListener`, not live-remote — P05-D's "needs a remote endpoint" was wrong) |
| g0-06 connection/credential guards | P03 | A | `credential_intent_lifecycle` (7/7), `credential_guards`, `credential_activation`, `credential_dispatch_lease`, `capability_grants` (9/9), `connection_revision_lifecycle` |
| g0-07 admission/leases/throttle/no-retry | P03 | A | `provider_admission`, `effect_outcome_delivery`, `intent_crash_boundaries` — not re-run this session, needs verification in P05-G |
| g0-08 MaterialKeyCreationIntent | P02 | A | `material_intent_lifecycle` (25/25), `material_vault_contract` |
| g0-09 CredentialKeyCreationIntent | P03 | A | `credential_intent_lifecycle` (7/7, shared with g0-06) |
| g0-10 EmbeddingSpaceTransition | P04 | **mixed: A + B + C** | Component/PostgreSQL/fault conjuncts (A): `embedding_transition_planning`, `embedding_transition_barriers`, `embedding_transition_activation`, `embedding_space_isolation` (4/4, confirmed this session). DrainMutationPermit/no-post-freeze-write (B): not built. Browser oracle (C): unavailable. |
| g0-11 credential abort + candidate abandon | P03/P04 | A | `credential_activation`, `embedding_transition_barriers` (both reference candidate-abandon paths) — needs a dedicated verification pass, not yet run this session |
| g0-12 DrainMutationPermit reconciliation | P04 (per spec) / undecided (per P04's own record) | **B** | Nothing exists. Needs a design decision first: is this a new authority, or does it reuse P05's `SourceQuiescing`/`Lfreeze` machinery in `restore_cutover.rs`? The two are conceptually similar (freeze existing mutations, reconcile pre-freeze identities) but currently serve different domains (installation-wide restore vs. per-embedding-job intents). |
| g0-13 unbound pre-Prepared drain routing | same as g0-12 | **B** | Same gap, same design decision. |
| g0-14 duplicate-charge/retry pinning/retrieval fences | P04 | mixed: A + C | Component conjuncts (A): `retrieval_generation_fence`, `embedding_result_finalization`, `embedding_result_preparation` (per prior doc, "proven"). Browser conjunct (C): unavailable. |
| g0-15 append-only evidence + one-way material lifecycle | P02/P04 | A | `erasure_is_one_way`, `erasure_holds_no_lock_during_vault_call`, `material_vault_contract`, `invariant_observer_contract` — not re-run this session |
| g0-16 InstallationMutationPermit + witnessed backup/restore + crypto-erasure | P05-A/B/C | **D** | `installation_safety_authority`, `backup_archive_authority`, `restore_cutover_authority`, `safety_supervisor_witness`, `safety_archive_recovery`, `safety_restore_recovery` — all require the disposable-Compose-project + Linux-runner recipe `docs/development-evidence/v1-g0-05-backup-restore-foundation.md` already used |
| g0-17 installer/Compose least-privilege (remaining conjuncts) | P05-A/B | **D** | Same recipe; specifically the archiver and create-only fingerprint-key identity/proof conjuncts P05-D's manifest says are still open |
| g0-18 protocol lock + executable contract fixtures | P01 | A | `protocol_a2a_lock`, `protocol_q1_manifest` (confirmed passing this session), plus the `qN_*` CLI contract suites and `tests/p01_text_hygiene.test.mjs` |
| g0-19 dirty work preserved | P05 (all) | done | Already `pass` in the manifest. |

---

## Sequencing

Each row becomes its own sub-package with its own detailed plan, written and reviewed immediately before it executes (never all at once, per the Package Planning and Review Rule). Order follows the gate-program's own P01→P02→P03→P04 dependency flow, cheapest categories first:

1. **P05-F — P01/P02 fresh closure (Category A).** g0-01, g0-02, g0-08, g0-18. Lowest risk: these suites already ran clean this session or in the immediately preceding commit history, with no live-provider or browser dependency. Produces the first digest-pinned `pass` entries in `v1-g0-05-gate.json` beyond g0-19.
2. **P05-G — P03 fresh closure (Category A).** g0-03, g0-04, g0-05, g0-06, g0-07, g0-09, g0-11. Requires re-running `provider_admission`, `effect_outcome_delivery`, `intent_crash_boundaries`, and the credential-abort/candidate-abandon paths this session did not reach, and correcting P05-D's wrong "needs a remote endpoint" reason for g0-05.
3. **P05-H — P04 fresh closure, Category A slice only.** The component/PostgreSQL/fault conjuncts of g0-10 and g0-14, and g0-15. Explicitly does **not** claim the browser or DrainMutationPermit conjuncts — those stay named as open in the same manifest entries via partial-conjunction `blocked`, the same pattern P05-D already established for g0-17.
4. **P05-I — DrainMutationPermit design and implementation (Category B).** g0-12, g0-13, and the remaining conjuncts of g0-10. Starts with a short design note resolving the reuse-vs-new-authority question above, reviewed before any code — this is new product surface, not evidence recovery, and deserves the same rigor as P04's original transition work. Sized and planned only once P05-F/G/H are accepted, since its design decision may be informed by how the fresh P04 evidence reads.
5. **P05-J — Backup/restore/Compose closure (Category D).** g0-16 and the remaining conjuncts of g0-17. Reuses the disposable-Compose-project-plus-Linux-runner recipe already documented and exercised by P05-A through P05-D; no new design, but real setup cost each run. See `[[p05-supervisor-tests-need-a-linux-runner]]` memory for the exact recipe.
6. **Browser-blocked ledger (not a sub-package).** The browser conjuncts of g0-10, g0-11, g0-14 (and any found in P05-G/H) stay recorded as `blocked` with the exact missing oracle named, in whichever sub-package's manifest entry they belong to. They are not silently dropped, and they are not retried until a browser-driving tool is available to this environment — revisit explicitly if/when Playwright reconnects or an equivalent tool is added.

**Expected final aggregate:** not `pass`. Even after P05-F through P05-J, g0-10/g0-11/g0-14's browser conjuncts keep those three criteria at `blocked`, so the honest ceiling for this environment is `blocked` with 14–16 of 19 criteria passing (g0-01 through g0-09, g0-12/13, g0-15, g0-16, g0-17, g0-18, g0-19) and 3 permanently blocked pending browser tooling. State this explicitly in every sub-package's evidence doc so nobody mistakes a rising pass-count for a rising aggregate.

---

## Self-review

- Every one of the 17 `unknown` plus the 1 partial `blocked` (g0-17) criteria from the current manifest is accounted for above, either as closable (A/D) or as a named gap (B/C).
- The DrainMutationPermit finding is load-bearing for sequencing P05-I after, not before, P05-F/G/H — recorded so a future session does not schedule implementation work ahead of the cheap evidence wins.
- The transport-security and loopback-evidence corrections (g0-04, g0-05) are recorded with their exact contradicting evidence so a future session does not re-trust the stale P05-D reasons.
- No task here invents a new top-level program package — every item is a P05 sub-package, consistent with the gate-program's "exactly twelve implementation packages" constraint.

## Execution handoff

Write P05-F's detailed bite-sized plan next, following `superpowers:writing-plans`, scoped to exactly: admitting its own paths into `scripts/p05-scope.mjs`, re-running the four named suites, capturing stdout to `docs/development-evidence/v1-g0-05-gate/` with computed SHA-256, and updating `v1-g0-05-gate.json`'s g0-01/g0-02/g0-08/g0-18 entries from `unknown` to `pass` with those sources. Do not start P05-G until P05-F's evidence is recorded and the gate collector re-run confirms the new aggregate.
