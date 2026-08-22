# Goal

Stop withholding from the provider the evidence we already hold about its own
effect.

§18 of the external-effects contract opens with "Reconciliation использует
**strongest available evidence**" and ranks it:

```text
1. provider idempotency lookup
2. exact external resource ID
3. operation status endpoint
4. ETag/version/revision
5. content hash
6. provider transaction/reference number
7. bounded search by unique Vestrace marker
8. human verification
```

`HttpExternalEffectReadBackAdapter::observe` takes the receipt and ignores it —
the parameter is literally `_receipt` — and asks `{endpoint}/{intent.id()}`,
which is a lookup by *our* identifier and appears nowhere on that ladder. The
receipt it discards carries `external_resource_id` (rank 2), `external_version`
(rank 4) and `response_digest` (rank 5), given to us by the provider being asked.

This is not merely using weak evidence. It is holding the strong evidence and not
passing it on, so the far side cannot perform the stronger lookup even when it
would.

It also has to be fixed before §34's work — confirming an acknowledged effect —
means anything. An acknowledged receipt is exactly the case where the provider
handed us its own identifier, and confirming the outcome by asking with ours
instead would be asking a question the provider is under no obligation to answer
precisely.

# Requirements

1. Read-back sends the identifiers the receipt carries when it carries them:
   `external_resource_id`, `external_version`, `response_digest`.
2. It is **additive**. The path stays `{endpoint}/{effect_id}`, because that is
   the request the deployment's own adapters already answer, and because the
   fault scenario's stub is a protected file that routes on it. A provider that
   ignores the additions behaves exactly as it does today.
3. What is sent is stated, not guessed: each identifier travels under a name that
   says which rung of §18 it is, so a provider implementing read-back can tell
   what it has been given without matching on our internal field names.
4. Nothing is invented. A receipt with no `external_resource_id` sends none — an
   absent identifier is absent, and a placeholder would be a claim.
5. The observation's `evidence_strength` continues to come from the provider's
   answer. This slice changes what the provider can use, not what we assert it
   used. Recording a strength because we sent an identifier would be exactly the
   substitution this project refuses.

# Non-goals

- **A minimum evidence strength for settling.** `reconcile_effect` maps
  `effect_applied: Some(true)` to `Confirmed` regardless of how weak the
  evidence was, and §20.4 says risk policy is strengthened for
  `IRREVERSIBLE`/`UNKNOWN` reversibility — which *suggests* a floor but does not
  state one. Inferring a normative requirement from a suggestion is how two
  earlier claims in these deltas turned out to be wrong. Recorded as a question,
  not answered here.
- **Confirming acknowledged effects** (§34). This is its prerequisite.
- **Changing the read-back response shape.** Only the request changes.
- Worker liveness, **`Authorized`** (fault point 2), the `NOT VALID` exemption
  debt, release-gate producers. Fault points 2, 3 and 4 stay red.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
  The stub must keep answering unchanged — which requirement 2 guarantees by
  construction rather than by hoping.
- `MemoryReadBack` in `tests/effect_recovery.rs` is a double; if it grows the
  ability to record what it was sent, that must not change what it answers.
- The request must not carry anything the receipt does not hold. No defaults, no
  empty strings standing in for absent values.

# Acceptance criteria

1. Given a receipt carrying an external resource id, the read-back request
   carries it. Asserted by observing the request the adapter actually issues,
   against a loopback server — not by inspecting the code that builds it.
2. The same for external version and response digest, each independently.
3. Given a receipt carrying none of them — or no receipt at all, which is the
   crash-derived `UNKNOWN` case — the request carries none of them and is
   otherwise identical to today's.
4. The path is unchanged in every case, pinned by a test, because that is what
   keeps every existing read-back endpoint working.
5. The recorded `evidence_strength` still comes from the response. A test feeds a
   response declaring a weak strength while the request carried a strong
   identifier, and asserts the weak one is recorded.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
7. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** The stub ignores anything beyond the path and
   answers from its dispatch count, so all five lines should be unchanged and
   `failures=4`. This slice is expected to be invisible to every harness in the
   repository, and its value is that a real provider stops being asked a weaker
   question than it could answer. A change would mean requirement 2 was not
   honoured.

# Implementation plan

1. Infrastructure: the request carries the receipt's identifiers under names that
   say which rung they are; absent ones are omitted entirely.
2. A loopback test that observes the issued request, so declaration and behaviour
   cannot drift — the same shape as the timeout test added two slices ago.
3. `MemoryReadBack` records what it was asked with, without changing what it
   answers.
4. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
