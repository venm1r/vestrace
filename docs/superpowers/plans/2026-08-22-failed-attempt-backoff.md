# Goal

Stop an effect nobody can ask about from consuming the sweep's whole budget
forever.

This is a regression introduced by the slice immediately before it, and saying so
is the point of writing it down.

`UnreconciledEffect` exists only in the sweep's in-memory report. Nothing durable
records that an effect could not be asked, so an effect whose adapter has no
registered route — or whose provider is persistently unreachable — produces no
reconciliation, is therefore never excluded by the `retry_unsettled_before`
cutoff, and comes back as a candidate on every single sweep. Oldest-first
ordering keeps such effects at the head of the queue.

Before the sweep was bounded this was waste: the same effects were asked about
uselessly every tick, and everything else still got processed. With
`RECONCILIATION_BATCH = 8`, eight unaskable effects consume the entire budget
every 500 ms, permanently, and nothing behind them is ever reached.

Bounding without backoff turned waste into starvation.

# Requirements

1. A recovery attempt that **could not be made** is recorded durably: the effect,
   the workspace, when it was attempted, and why it failed.
2. It is recorded as an *attempt*, not as a reconciliation. `ReconciliationOutcome::Inconclusive`
   means "we asked the provider and it could not tell us" and is a finding about
   the world; "we could not ask" is a finding about us. Migration 0151 exists
   because those two were once conflated, and the domain already keeps
   `UnreconciledEffect` apart from a reconciliation for exactly this reason. A
   new record must not undo that.
3. The candidate query excludes an effect whose most recent failed attempt is
   newer than a cutoff, in the same way it already excludes one whose most recent
   reconciliation settled nothing. Same shape, different reason, and the two
   cutoffs stay separate arguments so a caller cannot make one imply the other.
4. A successful reconciliation clears the effect from the failed-attempt path by
   settling it — no separate cleanup. An effect that becomes askable again must
   not have to wait out a cutoff earned while it was not.
5. The cutoff is a named constant explaining what it trades: how quickly a
   transiently unreachable provider is retried against how much budget a
   permanently unroutable effect is allowed to consume.
6. Nothing about which effects are *eligible* changes beyond this exclusion.

# Non-goals

- **Exponential backoff.** A flat cutoff matches `RECONCILIATION_RETRY_AFTER`'s
  precedent and fixes the starvation. Escalating delays are a policy worth having
  and worth arguing separately.
- **Escalation to human verification.** §18's step 8 exists and nothing
  implements it. An effect that can never be asked eventually needs a person, and
  that is not this slice.
- **Confirming acknowledged effects** (§34). This is its prerequisite: an
  adapter that cannot be read back would otherwise starve the sweep on a much
  larger population.
- Worker liveness, **`Authorized`** (fault point 2), §18's evidence ladder, the
  `NOT VALID` exemption debt, release-gate producers. Fault point 3 stays red.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- The new table carries a workspace and is under `ENABLE`/`FORCE ROW LEVEL
  SECURITY` with the isolation policy every other table in this subsystem has,
  and a composite foreign key onto `(id, workspace_id)` of the intent.
- Both `ExternalEffectRepository` implementations honour the exclusion. A double
  that ignores it makes the starvation test vacuous.
- Reads and writes predicate on workspace explicitly, never on RLS alone.

# Acceptance criteria

1. An effect whose adapter has no registered route is returned by one sweep,
   recorded as a failed attempt, and **not** returned by the next sweep before
   the cutoff. Asserted against PostgreSQL.
2. It is returned again once the cutoff has passed.
3. Eight unaskable effects do not prevent a ninth, askable one from being
   reconciled. This is the starvation test and it must fail against the current
   code — confirm that it does before the fix, and say so.
4. The durable record is not a reconciliation: a test asserts no
   `external_reconciliations` row appears for an effect that could only fail.
5. An effect whose provider fails and then succeeds is reconciled on the retry
   without waiting out a further cutoff.
6. `MemoryEffectRepository` applies the same exclusion, pinned by a test that
   would fail if it ignored it.
7. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
8. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
9. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** The scenario's adapter is registered and its
   stub answers, so no attempt fails and no exclusion applies. All five lines
   unchanged, `failures=4`.

# Implementation plan

1. Migration `0156`: the attempts table, its foreign key, RLS, policy, and an
   index supporting "most recent failed attempt per effect". Header comment in
   the established style — what was wrong, and why an attempt record rather than
   an inconclusive reconciliation.
2. Port: record a failed attempt; the candidate query takes the new cutoff and
   excludes on it.
3. Service: the `unreachable` path records before reporting, and the report is
   unchanged in shape so the worker's logging still works.
4. `MemoryEffectRepository` mirrors it.
5. The cutoff constant with its explanation.
6. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the starvation test from criterion 3 confirmed to fail against `HEAD` before
  the fix
