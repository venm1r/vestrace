# Documentation Gap Delta — Authentication With Attribution

**Date:** 2026-08-13
**Scope:** replacing one shared administrator token with per-principal credentials
**Decision:** approved by the operator, reversing the standing "one administrator account during development"
**Repository state:** dirty, implementation changes uncommitted

## What was there

A single bearer token, read from the process environment, compared inside the
middleware against the presented value, and mapped to one hardcoded
`(workspace, principal)` pair. It authenticated, but it did not identify: every
authenticated request in the system was the same administrator. Audit rows, run
events and approvals all recorded a principal that said nothing about who acted.

Fifty-four requirements across CAP, IDW and GOV are unprovable in that state,
not because the code is missing but because there is nothing to attribute.

The configured token also **did not exist** as an object. It could not be
listed, could not be revoked without restarting the process, and left no record
of having been used.

## What it is now

`access_tokens` was declared by migration 0012 and never read or written by any
code — the same pattern `workspace_keks` had before migration 0133. It already
carried a workspace, a principal, a hash and a label. Migrations 0135–0137
complete it and the code now uses it.

A credential is `vst_` followed by 64 hex characters — 256 bits from the OS
CSPRNG. The database stores its SHA-256 and nothing else, so a dump of the table
authenticates nobody: a 64-character hex string does not carry the prefix and
cannot be presented.

### Why SHA-256 and not Argon2

Argon2 and bcrypt make *guessable* secrets expensive to guess. Against a
uniformly random 256-bit value there is nothing to guess, so a slow KDF buys no
security and costs latency on every request. It would also make the lookup
impossible: a per-row salt means the database cannot find the row from the
presented value without verifying every credential in the table.

The reasoning is recorded next to the code, because it stops holding the moment
the token format becomes something a human picks.

### The bootstrap credential is a row, not a special case

Minting the first credential requires authenticating, which requires a
credential. `VESTRACE_AUTH__ADMIN_TOKEN` still exists, but it is now **seeded
into `access_tokens` at startup** rather than compared inside the middleware.
It is therefore listable, attributable, revocable and shows a `last_used_at`,
like any other credential — and there is exactly one code path that
authenticates, instead of a general one plus an exception.

The seed is idempotent by hash and does **not** resurrect a credential an
operator revoked.

## Three defects found while building it

### 1. `SECURITY DEFINER` cannot escape `FORCE ROW LEVEL SECURITY`

Authentication cannot be workspace scoped: the workspace is what the credential
tells us, so the lookup runs before any workspace is known, and RLS forbids
exactly that. Migration 0135 used a `SECURITY DEFINER` function for it.

That does not work, and the reason is worth keeping. `SECURITY DEFINER` executes
as the function's **owner**; the owner is the table owner; and `FORCE ROW LEVEL
SECURITY` applies to the table owner too. The function was filtered by the very
policy it was meant to step around and returned no rows to anybody — including
a superuser caller, since the executing identity is the owner regardless of who
calls.

The remaining options were to grant `BYPASSRLS` to the runtime role, which
defeats RLS everywhere, or to introduce a privileged role owning those two
functions, which needs superuser at deploy time and so cannot live in a
migration.

Neither is necessary, because the lookup does not need a **privilege**
exception. It needs a **knowledge** one:

```sql
USING (token_hash = current_setting('vestrace.authenticating_token_hash', true))
```

A caller presenting a credential already knows the value that hashes to the row
they are asking about. The policy discloses exactly one row, only to a caller
who already held it, and states the rule declaratively next to the isolation
policy rather than hiding it in a function body. `current_setting(..., true)`
yields NULL when unset and `token_hash = NULL` is not true, so an ordinary
connection sees nothing extra.

This is strictly narrower than a role that bypasses RLS.

### 2. The usage timestamp silently never wrote

Migration 0136's UPDATE policy keyed on the token id; the read policy keyed on
the hash. An `UPDATE ... WHERE` does not run on the UPDATE policy alone —
Postgres also applies SELECT policies to the rows the statement scans, because
the WHERE clause reads them. No SELECT policy matched, and the statement
updated nothing while reporting success.

It failed in the way that is hardest to notice: authentication worked, so
nothing looked broken, and `last_used_at` simply stayed NULL — the one signal an
operator uses to decide whether a credential is still in use. Migration 0137
extends the read policy to the token id, which is learned only by having already
resolved the credential.

### 3. The canonical event log said the system created every run

`RunCoordinator::create_run` hardcoded `RunActorRef::System` on the
`run.created` event, while `agent_runs.principal_id` carried the real caller.
The authoritative record and the projection disagreed about who acted.

Invisible while one shared token mapped to one identity — and precisely what
per-principal credentials exist to fix. The actor is now taken from the request
context, not from the command: the context is the identity authentication
resolved and the middleware overwrote the caller's headers with, so it cannot be
asserted. A command field could be.

Verified live: a run created with a second principal's credential records

```json
run.created  ->  {"principal": "10000000-0000-0000-0000-0000000000a1"}
```

while the two subsequent `run.status_changed` events remain `"system"`, which is
correct — the worker advanced them.

## Verified on the live stack

| check | result |
|---|---|
| previous shared token | 401 |
| no credential | 401 |
| a provider key sent by mistake | 401, and never reaches the store |
| bootstrap credential | 200 |
| `/health/ready` | 200 — probes stay open |
| minting for a second principal | 201, token returned once |
| that principal's run | attributed to them in `agent_runs` **and** `run_events` |
| revoking it | 204, and the credential is 401 immediately after |
| revoking it twice | 409, and the first timestamp is not moved |
| `GET /access-tokens/{id}/value` | 403, explicitly refused |
| the revoked row | still listed, with its revocation and last-use times |
| console through nginx | 200 |

Full workspace suite against a live PostgreSQL 17: **817 passed, 0 failed, 127
suites, exit=0** (796 before this work).

## What this does not do

- **No login UI.** The console still authenticates with a token nginx injects,
  so the browser never receives it. Per-user sessions are a separate
  deliverable, and until they exist the console acts as the bootstrap principal.
- **No roles or capabilities per principal.** Authentication now says *who*;
  authorization still uses the configured static capability set for everyone.
  CAP and GOV need the grant store, which this unblocks but does not build.
- **No credential rotation workflow.** Mint, list, revoke. Rotating means
  minting the replacement and revoking the old one by hand.
- **The bootstrap credential is still in the environment.** Anything that can
  read the server's environment can authenticate as that principal. It is now
  visible and revocable, which it was not, but it is not custody.
