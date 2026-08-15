# The first effect that left the process

**Date:** 2026-08-15
**Scope:** an external effect adapter, the service that records an intent before
dispatch, and the surface that performs one.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Eighteen EXT requirements execute against this domain. Every delta that closed
one ended with the same sentence: **nothing in this build had ever performed an
external effect.** `ExternalEffectAdapter` had no implementation outside test
fixtures, and `PgExternalEffectRepository` was constructed nowhere.

It has now performed several.

```text
outcome      | class       | count
acknowledged | http-200    | 3
failed       | unreachable | 1
unknown      | timeout     | 1
```

## What the contract demanded before anything could be built

`validate_adapter_descriptor` refuses an adapter that does not declare
reconciliation support. That single line decided the shape of the whole adapter:
**a fire-and-forget webhook cannot exist here.** An effect whose outcome can
become unknown and cannot be re-checked leaves the system permanently unable to
say what happened.

So `HttpWebhookEffectAdapter` requires a read-back URL and refuses to be
constructed without one, rather than declaring a capability it does not have.
"Just POST it somewhere" is not expressible, and it is meant not to be. A
misconfigured deployment fails at startup instead of discovering the gap after
its first timeout.

The rest of the descriptor is written to be true rather than flattering:

- `AtLeastOnce` — we may retry and the far side may already have acted.
- `IdempotencyProfile::Unknown` — an `Idempotency-Key` header is sent, and
  whether it is honoured is not ours to know. `ProviderKey` would be a guess
  about somebody else's system.
- `Irreversible` — a delivered request cannot be recalled.
- `DryRunMode::Simulated` — we can decline to send; we cannot ask most endpoints
  to pretend.

## The destination is configuration

An intent names its adapter; it never carries a URL. A surface that let a request
choose the destination would be an outbound proxy for everything the process can
reach, including whatever metadata endpoint it is deployed beside. The caller
chooses what to send; the deployment chooses where.

The request arguments are stored as a digest, not verbatim — the same reason a
receipt has nowhere to put a response body.

## The intent is written before the dispatch

EXT-001 asks for this and the reason is the crash in between: a process that dies
after the request and before any record has caused something in the world and
holds no evidence that it tried.

This was demonstrated by accident. The first attempt panicked inside the adapter
— a `reqwest::Client` belongs to the runtime that built it, and I drove it from
another one — and the table now holds **two intents with no receipt**. That is
precisely the state the ordering exists to produce: not a silent act, but an
effect with an unknown outcome, which the reconciliation path knows how to
handle. The bug is fixed (each dispatch builds its client inside its own
runtime); the evidence it left is left where it is.

## Three outcomes, live

Against a receiver on the host, through `host.docker.internal`:

```text
receiver answers 200        → acknowledged | http-200    | reconcile: false
receiver stopped            → failed       | unreachable | reconcile: false
receiver accepts, never answers → unknown  | timeout     | reconcile: true
```

The middle one is a judgement I changed while building it. A refused connection
means nothing was sent, so reporting it as `unknown` would **manufacture doubt**
— and an effect stuck in unknown can never be retried. A timeout is different:
the request went out and may have been received, parsed and acted on. `unknown`
has to be earned.

The receiver logged the delivery with the idempotency key the adapter sent:

```text
APPLIED 01a003af-4b92-7b50-95bd-f8837e3ba9e6 key=01a003af-… op=notify
```

## What this does not do

- **No reconciliation is wired.** The adapter can read back — that is why it is
  allowed to exist — and nothing calls it. The one `unknown` receipt in the
  table is a genuine open question the deployment cannot yet answer, and closing
  that loop is the obvious next slice.
- **Nothing retries.** The receipt carries `RetryDecision`, the surface reports
  `requires_reconciliation`, and no component acts on either.
- **Effects are triggered by an operator, not by a run.** A run step that needed
  to act outside would still have no way to; the surface is a person with a
  token.
- **One adapter kind, one destination.** The environment configures a single
  webhook; more than one needs a configuration file, which nothing in the
  deployment uses.
- **Nothing verifies the far side is what it claims.** No signature, no mutual
  TLS, no allowlist beyond "the configured URL". For a local webhook that is
  fine; for anything else it is not.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**. The conformance gate is unchanged at **188 passed (178 executed, 10
attested), 11 skipped** — EXT was already green, which is the point: the family
was fully verified and entirely unreachable, and it is the reachability that
changed.
