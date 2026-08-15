# What a written claim is worth

**Date:** 2026-08-15
**Scope:** the evidence origin an attestation is filed under, and what the v1.0
gate refuses because of it.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Asked what v1.0 still needs, I stopped guessing and produced a TRUSTED bundle.
It failed, and the failure list was not the one I would have written from
memory:

```text
evidence by (status, origin)
  (pass,    local_executable)      178
  (pass,    remote_self_asserted)   10   ← every attestation, failing the gate
  (skipped, remote_self_asserted)   11
```

Ten requirements *passed* and failed the gate anyway, because
`hard_gate_evidence` mapped `CaseOrigin::Attested` onto
`EvidenceOrigin::RemoteSelfAsserted`.

## Two statements in one build, contradicting each other

- `ConformanceReport::attested_but_should_execute` **exempts**
  `VerificationClass::Static`: a claim about architectural shape is the one kind
  of requirement no runtime assertion observes, so an attestation is the correct
  evidence for it.
- `GovernanceFederationGate` **refuses** every attestation, because they arrived
  labelled as a remote party vouching for itself.

Both are in the codebase. They cannot both be right, and the consequence was
that **v1.0 was unreachable for a reason nobody had decided**: not eleven open
requirements, but eleven plus ten that the gate would never accept in any form.

## The distinction is the trust boundary, not the word

A remote self-assertion is worthless because the subject and the source are the
same party and the reader cannot check. A local attestation is published *with
the thing it describes*: the source is in the bundle, and a reader who doubts it
can read the code it points at. Those are different failures of trust, and
`EvidenceOrigin` had a name for only one of them.

There are now four origins, and the gate's rule is:

- `RemoteSelfAsserted` — always refused.
- `LocalAttested` — admissible **only** where the requirement's class is
  `Static`; anything else gets `AttestationWhereExecutionIsRequired`.
- `LocalExecutable`, `RemoteAttested` — accepted.

The class comes from the registry, never from the evidence, so a bundle cannot
admit an attestation by relabelling what the requirement is. A new conformance
case checks all three arms against the real catalogue: a Static requirement
accepts a written claim, a behavioural one refuses it, and a peer's word about
itself is refused even where an attestation would have been admissible.

## CAP-001, and a drift worth naming

Under the new rule one attestation still failed: CAP-001, which I told the user
was class `SECURITY`. It is — **in the specification table**. In the registry the
code actually gates on, CAP-001 is `Static`, and its statement is different too:

| Source | Class | Statement |
|---|---|---|
| `vestrace-normative-invariants-v0.2.md` | SECURITY | Runtime authorization is determined by effective capability/policy, not a role name |
| `conformance/registry.rs` | STATIC | Capability must be a durable, constrained grant, not a coarse enum |

One identifier, two requirements. The gate reads the second; every human reads
the first. That is worse than a missing requirement, because both look
authoritative and nothing compares them.

Rather than pick, I wrote the case the *specification's* CAP-001 asks for, which
is executable and needed no reclassification: one principal is refused and
permitted according to which grant is held, two principals holding grants of the
same shape receive the same answer, and a grant refuses a scope it does not name.
Authority is what was granted, not who holds it. CAP-001 is now executed, so the
class question no longer blocks anything — but the drift is still there and is
recorded here rather than quietly resolved.

## Where v1.0 stands now

```text
188 passed (180 executed, 8 attested), 0 failed, 11 skipped
evidence: 180 local_executable, 8 local_attested (all Static), 11 skipped
```

The eight remaining attestations are ARC-001, ARC-004, ARC-005, ARC-008,
ARC-009, ARC-010, MEM-020 and QUAL-001 — every one `Static`, every one now
admissible.

**The v1.0 blocker list is exactly the eleven skips**, down from twenty-one. Each
names its blocker and none is closable by writing a case:

| Blocker | Requirements |
|---|---|
| Missing implementation | GOV-017, IDW-011, REC-016, QUAL-010, LRN-005, LRN-008 |
| Missing architecture | CAP-012, IDW-014 |
| Not a runtime property | IDW-010, CAP-005 |
| Needs a third evidence source | RET-004 |

## What this does not do

- **It weakens a gate.** That deserves saying plainly. The safeguard is that
  admissibility is decided by the requirement's class in the registry, and
  `Static` is exactly the class for which the report already declared execution
  inapplicable. If a requirement is misclassified, this rule inherits the
  mistake — which is precisely the CAP-001 drift above.
- **The spec/registry drift is unresolved.** Reconciling them is a change to a
  normative document under its own change-control rules, not something to slip
  into a delta.
- **No baseline is published and no bundle is signed.** The manifest I used to
  produce these numbers is a scratch file; the repository still contains no
  capability manifest.
- **I reported a stale result mid-slice.** After adding the CAP-001 case I read
  the gate from a container whose image predated it, and said CAP-001 was still
  attested. I caught it by grepping the deployed binary for the case id — the
  same check that found the outbox strings weeks ago — and the numbers above are
  from an image verified to contain the case.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**. Two new conformance cases (QUAL-011's attestation boundary, CAP-001's
granted authority), one new gate failure kind, one new evidence origin.
