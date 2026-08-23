# Slice 16 — the data-policy gate on the model call

**Phase:** Trust / T4 (data governance).
**Status:** plan, revised after one adversarial review. Nothing here qualifies a
profile or makes a release claim.

Named narrowly on purpose. The governance contract's model boundary is a
sequence that includes redaction and minimisation; this slice wires **one leg**
of it, the data-policy destination decision. Calling the slice "the model
boundary" would claim the rest.

## What was found

`DataPolicy::evaluate` and `evaluate_model_boundary` decide whether data of a
given classification may reach a given destination. GOV-003 and GOV-005 have
executable conformance cases. Outside `vestrace-domain`, those cases are the
only references. Tenth occurrence this session of *modelled* and *case-covered*
but never *consulted*.

A second, independent copy of the rule exists:
`security/redaction.rs::should_redact` answers the same question from a hardcoded
matrix, reads no `DataPolicy`, has no callers, and `RedactionService::new()`
starts with zero rules. **Out of scope**, named so nobody wires the wrong one.

## The trap, and what the review corrected

The easy implementation lets the deployment *declare* its destination in
configuration. Then the boundary agrees with configuration rather than reality:
point a "local" deployment at a remote host and confidential data leaves with a
passing decision. That is contract §34's `HTTP success == business outcome` in a
different hat. The destination is **derived**, not declared.

Deriving it honestly is harder than the first draft assumed. Verified directly:

- `provider_for` returns `Arc<dyn TextGenerationProvider>` and nothing else. The
  executor cannot see the URL at all.
- `OpenAiCompatibleClient` is built with `Client::builder().timeout(..)` and **no
  `.redirect(..)`**, so reqwest's default policy follows up to ten redirects, and
  system proxy handling is on. A loopback endpoint can therefore redirect the
  POST carrying the prompt — and the `Authorization` header — to a remote host,
  *after* a `LocalModel` allowance was recorded.
- Private-range addresses are network topology, not provider ownership. A managed
  service behind a VPC endpoint is a remote provider in every sense that matters.
  The first draft's "loopback or private range means local" was wrong.

## Decisions I am making as lead

1. **The adapter owns an egress descriptor.** `provider_for` returns the provider
   together with an immutable descriptor of where the request will actually go.
   The executor never guesses; the layer that holds the URL states it.

2. **`LocalModel` requires all three of:** a loopback host, redirects disabled on
   the client, and no proxy in effect for that request. Everything else —
   including private ranges — is `RemoteProvider`. Redirects are disabled on the
   client unconditionally: following one on an API call with a credential
   attached is a credential leak, not a feature.

3. **Policy source is `[policy.data]`**, following `[policy.capability_restoration]`.

4. **Absence is not consent.** `[policy.data]` missing while `model.enabled =
   true` **refuses startup**, naming the keys to add. This is a deliberate
   compatibility break. Reporting "unenforced" does not prevent disclosure, and
   silent absence is how a governance control ends up never enforced. The refusal
   is loud, one-line-fixable, and cannot be reached accidentally in production
   without an operator seeing it.

5. **Two modes, both explicit: `enforce` and `observe`.** In `observe`, the
   decision is evaluated and recorded and the call proceeds regardless. That is a
   real rollout path — turn on observe, read what would have been denied, then
   enforce — and it cannot be silent, because every observed denial is a row.
   There is no `disabled`.

6. **The required-capability leg is dropped, and configuring it refuses
   startup.** `DataPolicy::evaluate` takes `capability_granted`, but the worker's
   `RequestContext` principal is `PrincipalId::from_uuid(*workspace)` — the
   workspace UUID, not the principal who created the run. Evaluating a
   subject-scoped grant against a synthesised subject authorises the wrong party.
   Refusing to answer beats answering about the wrong subject.

7. **The classification is a channel floor, stated not inferred.**
   `[policy.data]` declares one conservative sensitivity for all run objectives,
   documented as exactly that. It is not a per-run truth and must not be
   described as one. Inferring sensitivity from prompt text would be an
   unauditable classifier nobody asked for.

8. **A dedicated append-only decision record, written before the call.** Not
   `audit_events`: that is JSONB, has no append-only trigger, and would make
   governance evidence unqueryable. A purpose-built table follows the pattern of
   0160 (fault-suite evidence) and 0162 (recovery qualification), with UPDATE and
   DELETE refused by trigger. It carries destination, classification, verdict,
   reason, policy version, mode, and the run and step it belongs to.
   `DataPolicyDecision` is not persisted directly: it is binary and requires a
   policy version, so it cannot express the observe-mode outcome.
   **The record must commit before the provider is called.** A decision written
   afterwards is a decision that a crash erases while the disclosure still
   happened.

9. **`AppConfig::fingerprint` covers every new field.** It already hashes
   `policy.*` and `model.base_url`; without the data-policy fields two
   deployments with different sensitivity ceilings would share a configuration
   digest, and that digest is release identity.

## Acceptance criteria

- `model.enabled = true` with no `[policy.data]`: startup is refused, and the
  message names the keys to add.
- Policy allowing `local_model`, `model.base_url = http://localhost:12345/v1`
  (LM Studio is running there): the step runs and the allowance is recorded with
  the policy version, before the call.
- Same policy, non-loopback `base_url`: the step fails, the reason names the
  destination, the record exists, and **nothing was sent** — proven by a provider
  that records whether it was called.
- **Redirect test against a real HTTP client**: a loopback endpoint that answers
  with a 302 to a remote host. The prompt must not reach the remote host. This
  exercises `OpenAiCompatibleClient`, not a double.
- Private-range `base_url` is classified `RemoteProvider`, not `LocalModel`.
- `mode = "observe"` with a denying policy: the call proceeds and the denial is
  recorded.
- Policy configured with a required capability: startup refused.
- Two configurations differing only in `[policy.data]` produce different
  `AppConfig::fingerprint` values.
- Faking the derivation to a constant `LocalModel` makes a test fail; likewise
  faking the descriptor. Demonstrated by breaking it, per slice 14's precedent.
- `cargo fmt`, clippy clean; conformance 199/199 unchanged; fault suite
  `failures=2` unchanged.

## Explicitly not in this slice

- Redaction and minimisation, the boundary's other legs, and the duplicate rule
  in `should_redact`.
- Embeddings, a second provider path with the same problem.
- Classification lineage, derivation, declassification (GOV-004).
- DNS rebinding between the locality decision and the request. Disabling
  redirects and proxies closes the reachable path; a rebinding defence needs
  connection-level pinning and is its own slice. Stated, not silently omitted.
- Any change to the release gate or its evidence families.
