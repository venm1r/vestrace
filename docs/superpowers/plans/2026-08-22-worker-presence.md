# Goal

Let the system know which workers are alive, so a dispatch owned by a dead one
is lost because its owner is gone rather than because a clock ran out.

`external_effect_lifecycle_transitions.dispatch_owner` records the `WorkerId`
that began a dispatch, and nothing anywhere records whether that worker still
exists. `worker_id` appears in exactly one table, `run_leases`, and only per
leased run — so a worker holding no run leases has no presence in this system at
all.

Recovery therefore waits out `dispatch_expires_at` for every lost dispatch, which
after the adapter-stated deadline is twenty seconds for the webhook adapter. If
the owning process is provably gone, twenty seconds is twenty seconds of waiting
for information already available.

It is also the missing piece under startup recovery for effects. An earlier
adversarial review refused "at startup every `Dispatching` is lost" because a
starting replica cannot tell another live replica's in-flight call from an
abandoned one. With a liveness record it can: not by assuming, but by checking.

# What this does not achieve, stated up front

**Fault point 3 stays red, and liveness does not change that.** In a database a
crashed process is indistinguishable from a slow one until its heartbeat lapses,
and a lapse window is necessarily longer than the seconds the fault scenario
waits before sweeping. There is no instant signal to be had here; the parent
knows its child died because it waited on the process, and that knowledge belongs
to the harness, not to the system.

Turning point 3 green needs the scenario to exercise **startup** recovery, where
a process that is gone cannot still be dispatching. That is a decision about what
the harness drives, and it is deliberately not bundled into the slice that builds
the mechanism it would depend on.

# Requirements

1. A worker records its presence: which worker, which workspace, when it started,
   when it last reported. Workspace-scoped, because everything in this schema is,
   and because a worker already serves a configured set of workspaces.
2. The worker refreshes it on the cadence it already uses for run leases
   (`heartbeat_interval`, 20s), and a presence unrefreshed for longer than
   `lease_ttl` (60s) is lapsed. Those two numbers already describe this
   deployment's tolerance for a silent worker; inventing a third pair would mean
   two answers to one question.
3. A dispatch whose owner's presence has **lapsed** is lost, regardless of its
   deadline.
4. A dispatch whose owner has **no presence record at all** is *not* lost on that
   basis, and falls back to the deadline. Absence is not death: it is also what a
   dispatch written before this table existed looks like, and what a worker that
   died before its first heartbeat looks like. Treating absence as evidence would
   repeat the `NOT VALID` mistake — a rule whose meaning silently changes for
   rows that predate it.
5. A worker that shuts down cleanly removes its presence, so an orderly restart
   is not indistinguishable from a crash for a whole lapse window.

# Non-goals

- **Startup recovery for effects.** It becomes safe once this exists, and it is
  its own slice with its own decision about the harness.
- **Liveness for anything but external-effect dispatch.** Run leases already
  carry their own per-run heartbeat and are not touched.
- **Fencing.** Knowing a worker is gone is not the same as stopping it from
  acting if it returns, and this makes no such claim.
- §18's rank 1, a minimum evidence strength, a uniqueness claim on
  reconciliation, the `NOT VALID` exemption debt, release-gate producers.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- The new table is under `ENABLE`/`FORCE ROW LEVEL SECURITY` with the workspace
  isolation policy the rest of the schema carries.
- Reads and writes predicate on workspace explicitly, never on RLS alone.
- Both `ExternalEffectRepository` implementations honour the lapsed-owner rule.
- The candidate query keeps every predicate it has. This **adds** a way to be
  lost; it removes none.

# Acceptance criteria

1. A dispatch whose owner's presence is older than the lapse window is returned
   as a candidate even though its deadline has not passed.
2. A dispatch whose owner refreshed its presence within the window is **not**
   returned before its deadline. Asserted with two dispatches of the same age and
   different owner presence, so the test cannot pass by accident of timing.
3. A dispatch whose owner has no presence row is not returned before its
   deadline, and is returned after it. This is the compatibility case and it must
   be pinned, not assumed.
4. A clean shutdown removes the presence, and a dispatch owned by that worker is
   then lost without waiting out the lapse window.
5. Presence is workspace-scoped: a worker's presence in one workspace is not
   readable from another, asserted under a restricted runtime role.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
7. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** No change. The scenario's child registers no
   presence — it is a short-lived process driving one effect, not a worker — so
   its dispatch falls into requirement 4's compatibility case and waits out its
   deadline exactly as now. All five lines unchanged, `failures=2`.

   If point 3 turns green, something is treating absence of presence as death,
   which requirement 4 forbids and which would be the `NOT VALID` mistake again.
   That would need reporting, not accepting.

# Implementation plan

1. Migration `0159`: the presence table, its RLS, policy and index.
2. Port: record presence, clear presence, and the lapsed-owner term in the
   candidate query.
3. Worker: register at startup, refresh on the existing heartbeat tick, clear on
   clean shutdown.
4. `MemoryEffectRepository` mirrors the lapsed-owner rule.
5. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
