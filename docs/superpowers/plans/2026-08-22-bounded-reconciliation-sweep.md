# Goal

Bound the reconciliation sweep.

`find_reconciliation_candidates` has no `LIMIT`, uses `fetch_all`, and the worker
calls it once per workspace on every tick of a 500 ms loop
(`crates/vestrace-cli/src/commands/worker.rs:212`, sleep at `:219`). Each
candidate then costs a sequential provider read-back.

A workspace holding a thousand effects whose outcome nobody knows therefore loads
a thousand rows and makes a thousand sequential HTTP calls every half second, in
the same loop that claims run work and drains the outbox. The tick does not
finish, and the work it was also supposed to do does not happen.

The neighbouring `find_undelivered_outcomes` in the same file takes a limit and
is driven with a batch of 32. The same job, the same file, two different
attitudes to how much of it to do at once.

# Requirements

1. `find_reconciliation_candidates` takes a batch limit and applies it in SQL.
   Bounding in the service would still load every row.
2. The existing order — oldest candidate first — is kept and is load-bearing once
   the set is bounded: it is what stops a large backlog from starving the effects
   that have waited longest. Say so where the ordering is written, because a
   later change to it would silently become a fairness change.
3. The sweep reports whether the batch came back full. An operator watching a
   saturated sweep needs to be able to tell "there was nothing more to do" from
   "we ran out of budget", and those look identical in a count.
4. The batch size is a named constant with a doc comment explaining what it
   trades: a tick's worst-case duration and provider load against how quickly a
   backlog drains. It is not silently equal to `DELIVERY_BATCH`; if the same
   number is right, say why rather than sharing the constant.
5. Nothing about *which* effects are candidates changes. This slice changes only
   how many are taken at once.

# Non-goals

- **Parallel read-back.** Sequential is bounded and predictable once the batch
  is; making it concurrent adds provider-side load questions this slice has no
  answer for.
- **Per-provider rate limits or backoff.** Related, larger, and only worth doing
  with a stated policy rather than a number.
- **Confirming acknowledged effects** (§34). This is a prerequisite for it — that
  change would grow the candidate population by orders of magnitude — but it is
  not it.
- **Worker liveness**, and with it an effects sweep at startup. Fault point 3
  stays red.
- **`Authorized`** (fault point 2), **§18's evidence ladder**, the `NOT VALID`
  exemption debt, release-gate producers.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
  The scenario sweeps one effect; a batch limit must not change what it observes.
- Both `ExternalEffectRepository` implementations honour the limit, including
  `MemoryEffectRepository` — a double that ignores it would make every test of
  bounded behaviour vacuous.
- The workspace predicate, the deadline comparison, and the crash-derived
  `Unknown` branch keep their current meaning. This is a `LIMIT`, not a rewrite.

# Acceptance criteria

1. With more candidates than the batch size, exactly the batch size is returned,
   and they are the oldest ones. Asserted against PostgreSQL with a population
   larger than the limit.
2. Successive sweeps drain the backlog rather than re-reading the same head: the
   second sweep returns the next oldest, because the first sweep settled or
   enrolled what it took. A test that only checks the first batch would pass
   against a sweep that makes no progress at all.
3. A sweep that fills its batch reports saturation; one that does not, does not.
4. `MemoryEffectRepository` applies the same limit and ordering, pinned by a test
   that would fail if it returned everything.
5. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
6. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
7. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** The scenario has one effect per point, far
   below any sensible batch size, so all five lines should be unchanged and
   `failures=4`. A change would mean the limit altered which effects are
   candidates, which requirement 5 forbids.

# Implementation plan

1. Port: the limit parameter, and a shape for the sweep's report that can say
   the batch was full.
2. Postgres adapter: `LIMIT` in the query, ordering unchanged and commented.
3. `MemoryEffectRepository`: the same limit and ordering.
4. Application: the batch constant with its explanation; `sweep` passes it and
   surfaces saturation in `ExternalEffectRecoveryReport`.
5. Worker: log saturation when it happens, so a backlog that never drains is
   visible rather than inferred from an unchanging count.
6. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
