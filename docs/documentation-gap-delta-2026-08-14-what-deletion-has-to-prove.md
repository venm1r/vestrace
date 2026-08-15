# What deletion has to prove

**Date:** 2026-08-14
**Scope:** executable conformance cases for twenty GOV requirements —
classification, deletion, keys, export, redaction, cross-workspace sharing,
audit, secrets and retention.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

GOV is the largest remaining family: twenty-six requirements over
`trust.rs`, three thousand lines that no case had executed. This is the first
batch — the part that decides where classified data may go and what a deletion
has to demonstrate before it counts as done.

## The six

- **GOV-003** — a data policy refuses by three independent routes: data above its
  sensitivity ceiling, a destination it does not list, and a missing required
  capability. Each refusal names which one failed, and a policy with **no**
  allowed destinations is refused at construction, because a policy that lists
  nothing permits everything by omission.
- **GOV-004** — a derivation takes the **highest** sensitivity among its sources.
  Public mixed with restricted yields restricted, not public and not an average,
  and the lineage keeps every source reference and the policy version that
  produced it. Deriving from no sources at all is refused: derived data could
  otherwise claim whatever sensitivity suited it.
- **GOV-005** — the model boundary allows confidential data to the local model
  its policy lists and refuses it to a remote provider, a log destination and an
  export bundle. The log case is the one worth having: data leaks into logs more
  often than into models.
- **GOV-007** — an active hold makes verification report `BlockedByHold` rather
  than `Verified` **or** `Incomplete`; those are three different facts and only
  one of them means "somebody decided this must not be deleted yet". An expired
  hold stops blocking, so "until the case closes" does not quietly mean "for
  ever" — the same shape as the health disposition expiry two slices ago, checked
  here before it could become the same defect.
- **GOV-008** — verification is refused when it skipped a planned dependency, and
  reports `Incomplete` when copies remain. The dependency case is what turns a
  delete into an orphan: removing a memory while its search document survives
  leaves a reference to content that is gone.
- **GOV-009** — verification cannot be recorded without evidence, without saying
  what it checked, or against a plan belonging to a different request.

All six pass against the current domain. As with the CAP round, nothing broke:
the governance model was built with these guards in place.

## The second batch: keys and export

Four more, in the same pass:

- **GOV-013** — a key rotates only from active, cannot be destroyed before it is
  revoked, and once destroyed accepts no further transition. Destroying an active
  key would let material vanish while something still believed it usable.
- **GOV-014** — access follows the key's lifecycle state: a **rotating** key is
  still usable, and retired, revoked and destroyed keys are not. Rotation that
  interrupted access would make every rotation an outage, which is how keys stop
  being rotated; revocation ending access without re-encrypting anything is the
  requirement itself.
- **GOV-010** — an export is refused when its classification exceeds the policy,
  when the required capability is absent, when the plan has expired, and when the
  **policy version has moved on**. The last is the one worth having: a plan
  reviewed under one version of the rules must not be authorized by another.
- **GOV-022** — an export names its scope, purpose, recipient and the exact
  revisions it carries, refuses a blank selection or a blank redaction reference,
  and pins revisions rather than naming objects that could say something else
  later.

Writing GOV-022 turned up a small gap: `DataExportPlan` stored
`object_revisions` and `exact_scope` with no accessors, so what an export carries
was recorded and unreadable. Both are exposed now.

## The third batch: redaction and immutability

- **GOV-019** — redaction replaces the sensitive span and leaves the rest of the
  record readable, *including its structure* when the payload is JSON. Both
  halves matter: a message erased entirely is one an operator cannot act on, and
  a redacted audit payload that stops being machine-readable does so exactly when
  somebody is investigating it.
- **GOV-023** — restricted content is redacted for **every** destination,
  including the audit store. That is the case worth having, because the audit
  store is the one destination somebody might argue should see everything, and a
  restricted secret written there is a secret in permanent storage. Internal
  content is redacted where it leaves the system and kept where it does not, so
  redaction is a boundary rule rather than a blanket one — asserted in both
  directions, since a service that redacts everything passes any test that only
  looks for absence.
- **GOV-024** — a classification is a value: deriving from one yields a new one
  naming its sources and never alters the original. If a derivation could edit
  its source, a revision's classification would change under readers who had
  already been shown it.

## The fourth batch: two-sided sharing and audit

- **GOV-018** — a cross-workspace mount carries only the operations **both** the
  source grant and the target policy allow. Asserted from both directions: an
  operation the source offered but the target withholds is refused, and so is one
  the target permits but the source never offered. Either check alone would let
  one side's consent stand in for both. Validity is the earlier of the two
  expiries, or one side could extend the other's consent by outliving it, and a
  target policy authorizing onward re-sharing is refused at construction — the
  source consented to this target, not to whoever the target later chooses.
- **GOV-020** — a policy decision survives into an audit event carrying the
  actor, action, resource, verdict, reason, policy version and the input it
  judged. The check is on the serialized form, since an audit trail holds what
  serialization produced rather than the value in memory.
- **GOV-026** — two policy versions produce decisions naming different versions,
  and a blank version is refused: a decision that cannot say what it was decided
  against cannot be re-evaluated when the rules change.

## The fifth batch: secrets, retention and rotation

- **GOV-002** — a secret reference names a provider and an opaque `secret://`
  location, and **its serialized form is checked**, because that is where a value
  would leak if one were held: an audit record, a log line and a debug dump all
  go through it. Resolving it requires the right workspace, the right purpose and
  a recorded authorization reference, so holding the reference is not enough to
  read the secret.
- **GOV-006** — retention requires a boundary to exist, refuses a minimum longer
  than its maximum, and reports expiry as a **state** rather than performing a
  deletion. A policy that deleted on evaluation would delete during a read.
- **GOV-021** — data whose retention has expired is still not deleted while a
  hold is active, and the verification reports it as *blocked* rather than
  *incomplete*. This is the case that would otherwise be found by a retention
  sweep destroying something an operator had ordered preserved.
- **GOV-025** — a key stays usable throughout rotation and its successor is
  usable before the predecessor retires, so there is never a window with no
  usable key. A rotation that retired the old key first would leave data
  encrypted under a key nothing may use.

## Where the profiles stand

```text
trusted    122 passed (113 executed, 9 attested), 77 skipped, 0 N/A
```

TRUSTED was `102 passed (93 executed, 9 attested), 97 skipped` before this slice.
Remaining: REC 18, EXT 18, QUAL 15, IDW 14, GOV 6, RET 2, LRN 2, CAP 2 — GOV has
gone from the largest family to the fifth, and twenty of its twenty-six
requirements are verified executably.

## What is left in GOV, and which parts are not cases

The sixteen remaining split into three kinds, and separating them is what keeps
the next batch from mixing evidence with assertion:

**Executable against existing domain code** — the federated-data policy rule
(GOV-015) and auditable policy enforcement (GOV-016), both of which need the
federation module read properly rather than skimmed.

**Claims about the deployment**, which will need honest skips or live evidence
rather than domain cases: secrets not stored as memory (GOV-001) and `SecretRef`
referencing a provider (GOV-002) are true of this build — envelope encryption
landed earlier — but the custody is `local-file`, which the crypto qualification
service refuses for production. Audit tamper-evidence (GOV-012) and "sensitive
data must not appear in logs" (GOV-023) are properties of the running system that
a domain case cannot reach.

**Requirements whose mechanism does not exist**: retention enforcement as a
running process (GOV-006, GOV-021 in the deployment sense) has domain types and
no scheduler, exactly as health had a domain and no producer two slices ago.

## Test results

Full workspace suite against a live PostgreSQL 17: **855 passed, 0 failed, 132
suites, exit=0** — unchanged, since the new cases run inside the existing
case-list test.
