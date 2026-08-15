# Documentation Gap Delta — Remaining HTTP Surfaces

**Date:** 2026-08-13
**Scope:** the last five surfaces that answered 501
**Repository state:** dirty, implementation changes uncommitted

All eight previously unimplemented surfaces now exist. No
`unsupported_handler!` remains in `crates/vestrace-http/src/api/mod.rs`.

## Run approval

Added a canonical run event `run.approval_granted { approver_id }` and a
`RunCommand::Approve`, accepted only from `WaitingForApproval`.

This was the point of the work. Implementing `/approve` on top of the existing
`Resume` would have been far cheaper and would have appeared to function, but
approval and un-pause would then be the same event in the canonical history and
the approver would be recorded nowhere. A test asserts the two event types
differ.

`POST /v1/runs/{id}/approve` uses `If-Match`, names the requesting principal as
approver, and additionally records an audit event.

The development compose risk ceiling moved from `high` to `critical`. It had
been held at `high` only because `/approve` was an unimplemented stub that a
development setting should not wave through; that reason no longer holds.

## Artifacts, triggers, connections

Each is a **registry read over a schema that already existed** — artifacts and
revisions from migration 0042, external triggers from 0048, connectors and
connections from 0054. No new tables were added.

While starting artifacts a duplicate domain type and migration were written
before the existing ones were found. They were removed. The earlier claim that
"artifacts requires content storage" was wrong about metadata: it was already
modelled.

Each surface states its limit rather than implying more:

- artifacts is a registry, not storage — it pins each revision's content hash
  so a consumer can verify bytes retrieved elsewhere, and deliberately offers no
  download link;
- a trigger's `enabled` flag is stored state only — no scheduler or webhook
  receiver consults the registry, so an enabled trigger does not fire;
- connections carry no credential material, because the schema has no credential
  column and exposing one would be the first step toward leaking it.

`GET /v1/artifacts` required `export.read`, which the development capability set
did not include; it was added.

## Profile

`GET /v1/profile` returns the identity the request is acting under, with
`identity_source: "request_header"` and `authenticated: false`. This build has
no authentication and nothing verifies the `x-principal-id` header, so the
response says so instead of presenting an unverified header as an account. It
issues no API keys: a key would grant nothing the headers do not already.

## Recorded decision: interim secret storage

The operator has stated that authentication will arrive later and that the
master key will temporarily live in `.env`.

**Credential storage was not implemented against that decision.** What remains
is a cryptographic adapter, and it needs all of: AEAD encryption, nonce
discipline (a repeated nonce under AES-GCM reveals plaintext), a key rotation
path, and a guarantee the key never reaches logs, responses or error messages.
Getting any one wrong produces a store that is worse than none, because it will
be trusted.

The domain scaffolding is already correct — `SecretRef` and `KeyReference` from
phase T3 keep references opaque and require an authorization context to resolve.
The missing piece is the adapter, tracked as R2 in
[`superpowers/plans/2026-08-12-open-follow-up-gates.md`](superpowers/plans/2026-08-12-open-follow-up-gates.md).

## Evidence

Verified against the running stack, each with real rows rather than an empty
response:

- artifacts returned a seeded artifact with its latest revision, including
  `media_type`, `size_bytes` and `content_sha256`;
- triggers returned a seeded webhook trigger;
- connections returned a seeded connection with its connector name and provider
  type, and no credential field;
- profile returned the request identity with `authenticated: false`;
- approval on a `created` run returned `400 command is not valid while run is
  Created` — the route is live, authorization passed, and the domain guard
  rejected it. The success path is covered by domain tests only: no HTTP route
  puts a run into `WaitingForApproval`.

Seeded rows were removed after verification.

## Remaining honest gaps

Several of these surfaces read registries that nothing writes yet: no component
registers artifacts, and no scheduler dispatches triggers. The read paths are
proven with data, but the pages stay empty in ordinary operation until a
producer exists.
