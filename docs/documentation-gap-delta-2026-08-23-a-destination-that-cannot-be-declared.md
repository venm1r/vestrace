# A destination that cannot be declared

**Date:** 2026-08-23
**Scope:** one leg of the governance contract's model boundary — the data-policy
destination decision, wired at the place a run objective reaches a model.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim.

```text
infrastructure suite:  28 targets, 227 passed, 0 failed  (real PostgreSQL 17 + pgvector)
release gate tests:    24 passed, 0 failed
live model:            2 passed against prism-ml/bonsai-27b via LM Studio
conformance gate:      199 passed, 0 failed, 0 skipped — unchanged
fault suite:           FAILED — 2 failures, unchanged
```

## The tenth time

`DataPolicy::evaluate` and `evaluate_model_boundary` decide whether data of a
given classification may reach a given destination. GOV-003 and GOV-005 have had
executable conformance cases since 14 August. Outside `vestrace-domain`, those
cases were the only callers. Nothing on an execution path had ever asked.

That is the tenth occurrence in this session of the same shape: a thing is
*modelled*, it is even *case-covered*, and it is not *used*. The survey of
22 August named data governance as one of three Trust families with no
references outside the domain crate. This closes part of one of them.

A second copy of the rule also exists and is still unwired.
`security/redaction.rs::should_redact` answers the same question from a hardcoded
matrix that reads no `DataPolicy` and cannot be configured, has no callers, and
`RedactionService::new()` starts with zero rules — so if it were called today it
would redact nothing. Two rules for one question, free to disagree. Left alone
deliberately, and recorded here so that nobody wires the wrong one.

## The shortcut refused

The easy implementation lets the deployment declare its destination:
`destination = "local_model"` in configuration. Then the boundary agrees with
configuration rather than with reality. Point a "local" deployment at a remote
host and confidential data leaves the building with a passing policy decision.
That is contract §34's `HTTP success == business outcome` wearing a different
hat.

**The destination is derived from where the request will actually go.** The
adapter that holds the URL states it, in an immutable descriptor returned
alongside the provider. Before this, `provider_for` returned
`Arc<dyn TextGenerationProvider>` and nothing else: the executor could not have
seen the endpoint even if it had wanted to.

## What the plan got wrong, and how

The first draft said a loopback **or private-range** host means `LocalModel`. An
adversarial review of the plan rejected it, and was right on three counts that
the plan had not considered:

- `OpenAiCompatibleClient` was built with `Client::builder().timeout(..)` and no
  `.redirect(..)`. reqwest's default policy follows up to ten redirects. A
  loopback endpoint could therefore answer `302` and forward the POST — the
  prompt, and the `Authorization` header with it — to a remote host, *after* the
  `LocalModel` allowance had been recorded. Verified by reading the client, not
  taken on report.
- System proxy handling was on, which is a second egress path the descriptor
  knew nothing about.
- Private address space is network topology, not provider ownership. A managed
  service behind a VPC endpoint is a remote provider in every sense that matters.

`LocalModel` now requires all three: a loopback host, redirects disabled, and no
proxy in effect. Private ranges are `RemoteProvider`.

## And the name was trusted too

Even after that, `localhost` was compared as a string. A hosts-file entry
defeats it. The anticipated objection — that anyone who can edit the hosts file
already owns the machine — applies equally to anyone who can set `HTTP_PROXY`,
and the proxy was defended against. So the name is resolved at client
construction: the resolution must be non-empty and **every** address must be
loopback, and a failed or empty resolution is not local. All three unknowns fail
closed. DNS rebinding after that check remains a stated non-goal rather than a
silent omission, and the code says so where the check lives.

## Proof by breaking

Three of this slice's claims are the kind that a passing test cannot establish,
because nothing in the type system distinguishes a derived value from an
asserted one. Each was proven by breaking it:

| faked | what failed |
|---|---|
| destination derivation pinned to `LocalModel` | private-network classification test |
| descriptor replaced with a fabricated local endpoint | actual-client descriptor test |
| `.no_proxy()` deleted from the builder chain | proxy test, with `Timeout` |

The third exists because `redirects_disabled` and `proxy_disabled` are literals
written beside the builder chain — reqwest does not expose its own policy, so
they cannot be derived, and the only guard against drift is a test that fails
when the chain changes.

## Decisions, and what they refuse

**Absence is not consent.** `model.enabled` with no `[policy.data]` is refused at
startup, naming the four keys to add. This is a deliberate compatibility break.
Reporting the boundary as "unenforced" would not have prevented a single
disclosure, and silent absence is how a control of this kind ends up never
enforced anywhere. There are two explicit modes, `enforce` and `observe`, and no
third state: in `observe` the call proceeds and every denial that would have
happened is a row.

**A configured `required_capability` is refused, not evaluated.**
`DataPolicy::evaluate` takes a `capability_granted` boolean, but the worker's
request context carries `PrincipalId::from_uuid(*workspace)` — the workspace, not
whoever created the run. Answering about the wrong subject is worse than
declining to answer. `false` is passed, which is inert when no capability is
configured, and the configuration that would make it load-bearing cannot start.

**The classification is a stated channel floor.** One conservative sensitivity
for all run objectives, documented as exactly that. Inferring sensitivity from
prompt text would be an unauditable classifier nobody asked for.

**The decision commits before the provider is called.** A decision written
afterwards is one a crash erases while the disclosure still happened. The table
is append-only by trigger, following 0160 and 0162.

## What this does not do

**It is one leg of the boundary, not the boundary.** The governance contract's
sequence includes redaction and minimisation. Neither is wired. The slice is
named for what it is.

**Embeddings are a second path to a provider with the same problem**, untouched.

**Nothing classifies data.** The floor is configuration. Lineage, derivation and
declassification (GOV-004) remain modelled only.

**The gate is unaffected.** No evidence family changed, and the fault suite still
answers `failures=2`.

## Found in passing

`migrations/0135_access_token_authentication.sql` issues `GRANT ... TO vestrace`
unconditionally, while `0020` guards the same kind of statement with
`IF EXISTS`. Migrations therefore do not apply to a fresh server until that role
is created by hand — encountered while standing up a database to verify this
slice. Unrelated to this work, and a real barrier to anyone starting from
nothing.

Two release-gate CLI tests were reported as failing in a parallel run and
passing individually. Re-run here in parallel, they passed. That is not evidence
against a race; it is a failure to reproduce one, and it is recorded as such.
