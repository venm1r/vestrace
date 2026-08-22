# Goal

Give a qualification baseline somewhere to live.

`QualificationBaseline` is a durable concept by design: it is published from a
bundle, carries a state through `Qualified → Stale / Invalidated / Failed`, keeps
an invalidation reason, and answers `matches_bundle`. It is stored nowhere. There
is no table, no repository, and the only mention outside the domain is a comment
in `config.rs` describing what `matches_bundle` refuses.

That blocks the next release-gate producer. `ReleaseApprovalService::evaluate`
takes a baseline, and the only way to obtain one today is
`QualificationBaseline::from_bundle` — **from the very bundle being approved**.
Comparing a bundle against a baseline derived from itself always matches, so the
check would pass by construction and mean nothing. A baseline is only a baseline
because it was published *earlier*, by someone, and can be compared against
*later*.

# Requirements

1. A table for qualification baselines carrying what the domain carries: id,
   profile, target digest, state, published time, invalidation reason, and the
   payload as the authoritative record with the columns beside it as projections
   checked on read — following the stores this subsystem already has.
2. A command that publishes a baseline from a qualification bundle and prints its
   id. Publishing is an operator act with a date attached, not something derived
   on demand.
3. Baselines are read back by id and by target digest, so a later release can ask
   "is there a published baseline for this target" without knowing an id it was
   never told.
4. **A published baseline is immutable in its published facts.** Its state may
   move — that is the lifecycle the domain models — but its profile, target
   digest and published time may not be rewritten. A baseline that can be edited
   after the fact is not a baseline, and this is enforced by the database rather
   than by discipline, as migration 0160 did for fault-suite evidence.
5. Publishing twice for one bundle is refused rather than silently duplicated. A
   target with two baselines has no baseline.

# Non-goals

- **The release approval producer itself.** This is its prerequisite; wiring
  `--release-approval` into the gate is the next slice and needs a signer policy
  and signature evidence besides.
- **Lifecycle transitions.** `mark_stale` and invalidation exist in the domain
  and stay unused here: what *makes* a baseline stale is a policy question — a
  newer release, a schema change, an expiry — and inventing a trigger while
  building the store would be answering it by accident.
- Recovery qualification and capability restoration, the other two missing
  producers. Fault point 3, the `NOT VALID` debt, §18's rank 1.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- A baseline is not workspace-scoped: it describes a release target, not a
  tenant's data. Say so where the table is defined, because every neighbouring
  table in this codebase is scoped and a reader will expect it to be — and
  because "no policy" must be a stated decision rather than an omission.
- `QualificationBaseline::from_bundle` keeps its place: it is how a baseline is
  *published*. What must not happen is deriving one at approval time from the
  bundle under approval.

# Acceptance criteria

1. A baseline published from a bundle round-trips: read back by id, every field
   equal, including the state and the published time at the column's precision.
2. It is readable by target digest, returning the published baseline for that
   target.
3. Publishing a second baseline for a target that already has one is refused, and
   the message names the target.
4. Raw SQL updating a published baseline's profile, target digest or published
   time is refused by the database.
5. A baseline published from bundle A does not match bundle B:
   `matches_bundle` is exercised against a real stored baseline rather than an
   in-memory one, since that is the comparison the next slice will depend on.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
7. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** No change: `failures=2`. This slice adds a
   store and a command and touches nothing the scenario drives.

# Implementation plan

1. Migration `0161`: the baselines table, the uniqueness that makes requirement 5
   a database fact, and the trigger enforcing requirement 4. Header comment in
   the established style — what was missing, and why a baseline derived from the
   bundle under approval would be vacuous.
2. Port and Postgres adapter: publish, find by id, find by target digest, with
   the projection-versus-payload check comparing timestamps at the column's
   precision — this has now been the same defect twice, and a third would be
   carelessness.
3. CLI: `conformance publish-baseline --bundle-file ... --profile ...`.
4. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
