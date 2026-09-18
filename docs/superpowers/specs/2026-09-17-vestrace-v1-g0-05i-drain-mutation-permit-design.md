# DrainMutationPermit / Quiescing — Design

**Status:** Approved for implementation planning (design only; no code in this document).
**Owner package:** P05-I, per `docs/superpowers/plans/2026-09-17-vestrace-v1-g0-closure-program.md`.
**Closes:** G0 criteria g0-12, g0-13, and the `DrainMutationPermit`/"no post-freeze write" conjunct of g0-10.
**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md`, lines 919 (browser scenario, out of scope here), 944 (g0-12), 945 (g0-13), part of 942 (g0-10).

## 1. Problem

`DrainMutationPermit` and `Quiescing` do not exist anywhere in the repository, in Rust or SQL — confirmed by `grep -rn "DrainMutationPermit\|Quiescing" crates/` returning zero matches, and independently recorded in `docs/development-evidence/v1-g0-04-embedding-transition-foundation.md:6417-6420`. The spec requires:

- **g0-12:** "DrainMutationPermit explicitly reconciles only exact pre-Quiescing MaterialKeyCreationIntent/CredentialKeyCreationIntent states from Reserved through vault-create/receipt/Prepared/Bound/promotion or guarded abort, plus ResultPrepared, and freeze evidence covers every crash boundary without a new identity/effect/output/dispatch or post-freeze write."
- **g0-13:** "for unbound pre-Prepared drain work, retained reconstructable input may resume only the fixed prepare route while unavailable input must take its existing guarded abort/unbound-erase/receipt route; no synthetic bytes or ContentPrepared-only marker is permitted, ResultPrepared always bind/finalizes, and Bound never abandons."

In plain terms: an operator (or, later, the installer) can request the installation "freeze" — stop admitting new provisional-key work — and the system must wait until every *already in-flight* `MaterialKeyCreationIntent`/`CredentialKeyCreationIntent` reaches a state that can survive the freeze indefinitely (terminal, or safely resumable later), using **only the resume/abort logic those intents already have** (built and evidenced under g0-08/g0-09). `DrainMutationPermit` does not invent new per-intent transitions; it only decides, and durably remembers, which intents it is waiting on and when none remain.

## 2. What already exists and is reused unchanged

- **`InstallationMutationPermit`** (`crates/vestrace-application/src/installation/permit.rs`): a transaction-scoped lock with `PermitMode::{Shared, Exclusive}`. Every governed mutation already acquires `Shared`; restore and other installation-exclusive work acquires `Exclusive`. The drain-request transaction acquires `Exclusive` — this is the *only* concurrency primitive DrainMutationPermit needs; it is not a new lock type.
- **`MaterialKeyCreationIntentState`** (`crates/vestrace-domain/src/material/identity.rs:102`): `Reserved, ProvisionalCreated, ProvisionalReceipted, ContentPrepared, ResultPrepared, ContentAbandonPrepared, PrePreparedAbandonPrepared, Bound, Live, ErasurePrepared, Tombstoned, Abandoned`.
- **`CredentialKeyCreationIntentState`** (same file, line 123): `Reserved, ProvisionalCreated, ProvisionalReceipted, CredentialPrepared, CredentialAbandonPrepared, Bound, Candidate, ErasurePrepared, Destroyed, Abandoned`.
- **Terminal-for-drain-purposes states** (a drain never waits on these): material `Live, ErasurePrepared, Tombstoned, Abandoned`; credential `Candidate, ErasurePrepared, Destroyed, Abandoned`. `Bound` is terminal for drain purposes for *both* — g0-13 states "Bound never abandons," so once an intent reaches Bound it can only move forward, never back into drain's concern.
- **Pre-Quiescing (drain must wait on) states**: material `Reserved, ProvisionalCreated, ProvisionalReceipted, ContentPrepared, ResultPrepared`; credential `Reserved, ProvisionalCreated, ProvisionalReceipted, CredentialPrepared`. (`ContentAbandonPrepared`/`CredentialAbandonPrepared`/`PrePreparedAbandonPrepared` are themselves *part of* the guarded-abort route already, not states drain needs to act on beyond waiting for them to reach `Abandoned`.)
- **Crash-safe resume/abort logic for every one of those states**: already built and evidenced (g0-08, g0-09, this program's P05-F). DrainMutationPermit does not add a single new transition to either state machine.

## 3. A migration-numbering prerequisite, found during design

The next migration is `0216`. `bounded_migrator()` in `crates/vestrace-infrastructure/src/postgres/pool.rs:430` selects migrations with a hard numeric cutoff — `migration.version <= version` — not an exclusion list. `HISTORICAL_MIGRATOR` calls `bounded_migrator(208)`. A migration numbered `0216` can **never** be included by that function no matter how it's called, because `216 <= 208` is false and raising the bound to `216` would also re-admit `0209`–`0215` (the protected P05 assertion migrations P05-E deliberately excluded).

This means DrainMutationPermit's own new table would be invisible to ordinary `#[sqlx::test]` suites — exactly the problem P05-E fixed for `0209`, recurring one migration later — unless `bounded_migrator` is generalized from "everything up to N" to "everything up to N except this fixed exclusion list." This is a small, low-risk change:

```rust
fn bounded_migrator_excluding(
    upper: i64,
    excluded: &[i64],
) -> Result<sqlx::migrate::Migrator, InfrastructureError> {
    if !MIGRATOR.version_exists(upper) {
        return Err(InfrastructureError::configuration(
            "requested bounded migration version is not embedded",
        ));
    }
    Ok(sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|m| m.version <= upper && !excluded.contains(&m.version))
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    })
}
```

`bounded_migrator(version)` stays exactly as it is today (used by `migrate_through_version`, and by `HISTORICAL_MIGRATOR = bounded_migrator(208)`, unchanged — no existing caller or test changes behavior). A new static, `DRAIN_HISTORICAL_MIGRATOR = bounded_migrator_excluding(216, &P05_ASSERTION_VERSIONS)`, is added for the new suites this package writes, where `P05_ASSERTION_VERSIONS` is the existing seven `P05_*_ASSERTION_MIGRATION_VERSION` constants collected into a slice. This does not weaken migration `0209` (still excluded, still reachable only through the fixed three-phase route) and does not touch any P01–P04 authority path.

If a *future* package adds another ordinary migration after `0216`, `DRAIN_HISTORICAL_MIGRATOR`'s upper bound would need to move again — this is an accepted, narrow, documented pattern now, not a one-off hack.

## 4. Chosen approach: snapshot-by-column, not a join table or live recount

Three approaches were considered:

1. **Live recount** — reconciliation just counts current non-terminal rows. Rejected: does not "reconcile only *exact* pre-Quiescing states" — the set is implicit and could in principle include an intent that raced past the guard, masking a bug instead of surfacing it.
2. **Separate snapshot table** (`drain_pending_intents` join table, one row per intent id + kind, populated at drain-request time). Rejected: an extra table and extra consistency obligation (keep it in sync as intents resolve) for no benefit over option 3.
3. **Snapshot-by-column** (chosen): a nullable `drained_by` column on both `material_key_creation_intents` and `credential_key_creation_intents`, stamped with the current drain request's id in the same transaction that flips the request to `Draining`. This is an explicit, fixed, auditable set — exactly the "compute once, never redefine" pattern this codebase already uses for `TransitionBatchId` and `ModelBindingSnapshot` — with no new table and no ongoing bookkeeping: reconciliation is one `COUNT(*) WHERE drained_by = $1 AND state NOT IN (<terminal states>)` per table.

## 5. Schema

One new migration, `0216_installation_drain_request.sql`:

```sql
CREATE TABLE installation_drain_requests (
    id UUID PRIMARY KEY,
    -- Single-workspace product (gate-program.md: "local single-workspace
    -- Vestrace v1.0 product") -- one drain request may be active at a time,
    -- enforced by a partial unique index rather than a fixed singleton id,
    -- so history of prior completed drains is retained.
    requested_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    CONSTRAINT installation_drain_requests_completed_after_requested
        CHECK (completed_at IS NULL OR completed_at >= requested_at)
);

-- Exactly one undrained (completed_at IS NULL) request may exist at a time.
CREATE UNIQUE INDEX installation_drain_requests_one_active
    ON installation_drain_requests ((true))
    WHERE completed_at IS NULL;

ALTER TABLE material_key_creation_intents
    ADD COLUMN drained_by UUID REFERENCES installation_drain_requests(id);

ALTER TABLE credential_key_creation_intents
    ADD COLUMN drained_by UUID REFERENCES installation_drain_requests(id);
```

(Exact existing table names to be confirmed against the live schema in the implementation plan's Task 1 — the names above are inferred from the domain type names and P05-F/G's evidence and must be checked against `crates/vestrace-infrastructure/src/postgres/` and `docs/database-schema.md` before the migration is written.)

## 6. New domain and application surface

- `crates/vestrace-domain/src/installation_drain.rs` (new file, paralleling `installation_safety.rs`): `InstallationDrainRequest` with states `Draining { requested_at }` / `Frozen { requested_at, completed_at }`, and pure logic for "is this material/credential state pre-Quiescing" (a total function over each enum, so a new state added to either enum later fails to compile here rather than silently being ignored — the same defensive pattern `INTERNAL_STATE`/`evidence_is_readable` uses elsewhere in this codebase).
- `crates/vestrace-application/src/installation_drain.rs` (new file): the `DrainMutationPermitRepository` port —
  - `request_drain(&self, context: &RequestContext) -> Result<InstallationDrainRequest, ApplicationError>` — acquires `InstallationMutationPermit::Exclusive`, refuses if a drain is already active (the partial unique index makes this a real database constraint, not just an application check), stamps `drained_by` on every current non-terminal row in both tables, commits.
  - `current_state(&self) -> Result<InstallationDrainRequest, ApplicationError>` — reads the latest request row (there is always at most one active, and the most recent completed one is the last relevant history).
  - `reconcile(&self) -> Result<InstallationDrainRequest, ApplicationError>` — if active and the count of `drained_by = <id> AND state NOT IN (<terminal>)` across both tables is zero, sets `completed_at` and returns `Frozen`; otherwise returns `Draining` unchanged. Idempotent and safe to call repeatedly or after a crash (it only reads current durable state; it performs no per-intent action).
- **The guard**, added at each existing `reserve()` call site for both intent types (`crates/vestrace-infrastructure/src/postgres/material_intent.rs` and the analogous credential repository — exact files confirmed in the implementation plan): before inserting a new `Reserved` row, check for an active drain request in the same transaction; if one exists, refuse with a new, named error variant (not a generic conflict) so the "visible drain blocker names the exact fixed intent/job/effect/state" requirement from the browser-scenario text has a concrete typed answer to report, even though the browser surface itself is out of scope here.

## 7. Test plan shape

Mirrors the existing crash-boundary test suites for both intent types (`credential_intent_lifecycle.rs`, `material_intent_lifecycle.rs`, `intent_crash_boundaries.rs`) rather than inventing a new testing style:

- A PostgreSQL contract suite proving: `request_drain` refuses a second concurrent request (constraint violation surfaces as a typed refusal, not a raw SQL error leaking upward); stamps exactly the pre-existing non-terminal rows and no others; a `reserve()` attempted after `Draining` is refused with the named error, in the same transaction race as the drain request (no window where both can succeed).
- A fault-injection suite (paralleling `intent_crash_boundaries.rs`) exercising `reconcile()` at each pre-Quiescing boundary named by g0-12 for both intent types, crashing before and after each existing resume/abort step, proving: the retained-input case resumes only the precommitted route, the lost-input case takes only the guarded pre-prepared abort/unbound-erase route (never synthetic bytes or a bare `ContentPrepared` marker — this is g0-13's own wording, already enforced by the *existing* per-intent logic; the test here proves drain doesn't disturb it), `ResultPrepared` always finalizes, and `Bound` never abandons.
- A test proving `reconcile()` transitions `Draining -> Frozen` only when the count reaches exactly zero, and that a `Frozen` request refuses `reserve()` identically to `Draining` (freezing is permanent for that request; a new drain would be a new request row).

## 8. Explicitly out of scope for P05-I

- Any CLI surface (`vestrace drain ...`). Confirmed with the operator: domain + application + infrastructure only.
- The full browser/supervisor scenario at spec line 919 (release-level section 14.4 evidence, not this G0 delivery-gate bullet).
- Extending `DRAIN_HISTORICAL_MIGRATOR`'s bound for any future package's migrations — each later package that needs this repeats the same small, documented adjustment.
- Repairing `evidence_is_readable`'s pre-existing failure (named in P05-H's evidence doc as follow-up for whichever package touches `installation_safety.rs` next — not this one, since `installation_drain.rs` is a sibling file, not a change to `installation_safety.rs` itself).

## 9. Self-review

- **Placeholder scan:** no TBD/TODO; the one open item (exact existing table names) is explicitly flagged as an implementation-plan Task 1 confirmation step, not a gap in the design's logic.
- **Internal consistency:** the terminal-state lists in §2 and the guard behavior in §6 agree; the schema in §5 matches the repository operations described in §6.
- **Scope check:** single implementation plan's worth of work — one migration, two new small modules, guard additions at existing call sites, tests mirroring an existing pattern. Not decomposed further.
- **Ambiguity check:** "pre-Quiescing" is given an exact, enumerated definition per intent type in §2 rather than left to interpretation.
