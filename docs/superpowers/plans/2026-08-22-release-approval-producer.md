# Goal

Give the release gate its second producer: release approval.

`run_release` passes `None` for release approval, so the gate reports
`ReleaseApprovalMissing`. Everything `ReleaseApprovalService::evaluate` needs is
now obtainable:

- the manifest and the bundle — already loaded by the gate;
- a **published** baseline — the previous slice gave it a store, so it can be
  looked up rather than derived from the bundle under approval;
- the trust state — `find_latest_trust_state(scope)`, persisted;
- a signer trust policy — the CLI already builds one for the crypto path;
- signature evidence — two booleans saying whether the manifest and the bundle
  were cryptographically verified.

# The one thing that can quietly destroy this slice

`ReleaseSignatureEvidence` is two `bool`s. Passing `true, true` compiles, reads
naturally, and turns the whole approval into a formality. It is the easiest
possible place in this codebase to write an attestation wearing an execution's
clothes, and the slice is worth nothing unless both booleans come from actually
verifying the signatures against the mounted key store — the same verification
`run_verify_signature` already performs.

Nothing in the type system distinguishes a verified `true` from an asserted one.
Only a test that fails when verification is skipped can.

# Requirements

1. `conformance release --release-approval` collects the decision; without the
   flag the gate still reports `ReleaseApprovalMissing`, because when nobody
   asked that is the honest answer.
2. Signature evidence is produced by verifying the manifest's and the bundle's
   signatures against the mounted key store. `--release-approval` therefore
   requires the key-store arguments, and says so rather than defaulting to
   unverified.
3. **An absent baseline is a failure, not a missing evidence family.** If
   approval was requested and no baseline exists for this target and profile, the
   gate reports `ReleaseApprovalFailed` with a reason naming what was looked for.
   "Nobody looked" and "we looked and there is nothing" are different answers and
   the gate already has two words for them.
4. The same for an absent trust state.
5. The baseline is fetched by the release's target digest **and** profile, which
   is what the store is keyed on. A baseline published for another profile of the
   same build does not approve this release.
6. Nothing derives a baseline from the bundle under approval. The store is the
   only source.

# Non-goals

- **Making the gate pass.** Two producers remain absent after this — recovery
  qualification and capability restoration — and the fault suite reports two
  failures.
- **Publishing baselines automatically**, or driving their lifecycle.
- Recovery qualification's harness, the capability restoration policy, fault
  point 3, the `NOT VALID` debt.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- Do not weaken `ReleaseApprovalService`. If a check it performs cannot be fed
  honestly, report that rather than feeding it something that passes.
- The scope used for the trust-state lookup must be stated and justified, not
  picked. If the release does not determine one, say so rather than inventing a
  default that quietly makes every release read the same record.

# Acceptance criteria

1. With `--release-approval` and a published baseline matching the release, the
   gate reports the service's decision — and reports
   `ReleaseApprovalFailed` rather than `ReleaseApprovalMissing` when that
   decision has failures.
2. Without the flag, `ReleaseApprovalMissing`, unchanged.
3. With the flag and **no** baseline for this target and profile,
   `ReleaseApprovalFailed`, and the message names the target and profile it
   looked for.
4. A baseline published for a different profile of the same build does not
   satisfy the lookup.
5. **Signature evidence is not asserted.** A test tampers with a signature — or
   supplies a key store that cannot verify it — and the resulting evidence is
   `false`, reaching the gate as a failure. This test must fail if the
   implementation hardcodes the booleans, and saying so is the point of the
   criterion.
6. `--release-approval` without the key-store arguments is refused, rather than
   proceeding with unverified evidence.
7. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
8. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
9. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** No change: `failures=2`. This slice touches
   the release gate, not the scenario.

# Implementation plan

1. CLI: `--release-approval`, requiring the key-store arguments; collect the
   baseline and trust state from the database, verify both signatures, and
   evaluate.
2. Map an absent baseline or trust state to a failed decision with a reason,
   never to a missing evidence family.
3. Tests for each criterion, with criterion 5 written so it fails against
   hardcoded evidence.
4. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the release gate run with and without `--release-approval`, both recorded
