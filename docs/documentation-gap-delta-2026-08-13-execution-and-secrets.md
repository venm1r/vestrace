# Documentation Gap Delta — Execution, Secrets and Content Storage

**Date:** 2026-08-13
**Scope:** administrator authentication fallout, envelope-encrypted secret
storage, artifact content storage, and real model invocation for run steps
**Repository state:** dirty, implementation changes uncommitted

## Two regressions introduced by administrator authentication, and fixed

Enabling the bearer-token layer broke two things that no test covered.

1. **Liveness and readiness probes answered 401.** The container never became
   healthy, and both `console` and `dev-seed` wait on `service_healthy`, so the
   whole stack would have stopped coming up. `/health/live` and `/health/ready`
   now answer before authentication: an orchestrator's probe cannot present a
   credential, and answering it 401 makes "the process is wedged"
   indistinguishable from "the probe is not authorized".

   `/metrics` is deliberately **not** exempt — it reports per-workspace
   activity, unlike the probes.

2. **The console lost every request.** It reaches the API through nginx and knew
   no token. nginx now attaches the token itself, so the browser never receives
   it; a token compiled into the JS bundle is readable by anyone who opens the
   page. `proxy_set_header` replaces whatever the browser sent, so a client
   cannot substitute its own credential either.

   Consequence stated plainly: reaching the console port is now equivalent to
   holding the token. The port is published on `127.0.0.1` only, and that is the
   only thing separating the two.

The exemption is an exact match on the two probe paths, with a test asserting
that `/health`, `/health/livez` and `/health/live/../../v1/runs` stay
authenticated.

## Secret storage (migration 0133)

AES-256-GCM envelope encryption: a master key wraps a per-workspace data key,
which encrypts individual secrets.

`workspace_keks` already existed from migration 0087, declaring
`algorithm DEFAULT 'AeadAes256GcmV1'` and holding no key material, and **no Rust
code referenced it**. It was completed rather than replaced.

Decisions worth recording:

- **Envelope, not direct encryption.** Rotating the master key rewraps one data
  key per workspace instead of rewriting every ciphertext. Rotation that
  requires touching every row is rotation that never happens.
- **A fresh CSPRNG nonce per operation.** A repeated nonce under GCM leaks the
  XOR of the plaintexts and permits forgery. A test performs 256 encryptions of
  identical plaintext and asserts every nonce differs.
- **Associated data binds ciphertext to its row** — workspace, secret id and
  purpose. Relabelling `purpose` in the database makes the secret unopenable
  rather than re-homing it into the new context; a test does exactly that.
- **One failure mode, not three.** Wrong key, tampered ciphertext and moved row
  all report "secret could not be decrypted". Distinguishing them tells an
  attacker how far they have got.
- **No plaintext fallback.** Without a master key there is no secret storage and
  the surface answers 501. A store that silently degrades to unencrypted is
  worse than none, because it will be trusted.

### Custody: what this does not claim

The master key is read from process configuration, which for this deployment
means `.env`. That is `CryptoCustody::LocalDevelopment` with provider
`local-file`, and `CryptoAdapterQualificationService` **refuses** that
combination for production — `ProductionCustodyRequired`. The adapter therefore
fails v1.0 crypto qualification by construction, which is correct.

What it does buy: the database holds nothing usable. A dump, replica, backup or
SQL-injection read discloses names and sizes, not secrets. A test reads the row
straight out of PostgreSQL, bypassing all application code, and asserts the
plaintext is absent.

### HTTP surface

`GET /v1/secrets`, `POST /v1/secrets`, `DELETE /v1/secrets/{id}`.

**There is deliberately no endpoint that returns a secret's value.**
`/v1/secrets/{id}/value` answers `403 secret_value_not_readable` — an explicit
refusal rather than a confusing 405. An operator who has lost a key replaces it;
they do not read it back out of the system that stored it. Writes and deletions
carry `RiskCategory::Critical` and are recorded in the audit trail, naming the
secret and never its value.

## Artifact content storage (migration 0134)

Migration 0042 is named `..._blobs_...` and creates only `artifacts` and
`artifact_revisions`. **No table in the schema could hold bytes** — before this
change the only `BYTEA` columns in the entire schema were the ones migration
0133 had just added for secrets. The registry could pin a digest for content the
system had nowhere to keep.

Model output goes to an artifact rather than into a run event, because a run
event is append-only forever: output written there could never be redacted,
purged, or held to a retention window. An artifact already carries the `purged`
status that makes deletion expressible.

Content is addressed by SHA-256 and deduplicated **per workspace, never
globally** — a shared blob table would let one workspace learn that another
holds a particular document by observing a hash collision. A test stores content
in one workspace and fails to read it from another using the same digest.

`artifact_revisions.content_hash` is deliberately **not** a foreign key to the
blob table: a revision may legitimately pin content held elsewhere, which is the
registry behaviour 0042 already supports. Absence of a blob means "not held
here", not "inconsistent".

## Run steps now actually execute

`ExecuteStepHandler` moved a step to `Running` and immediately to `Succeeded`
without invoking anything. Every run reported success having done no work. It
had **no test coverage at all**, which is how it survived.

- A step assigned to `RunActorRef::AgentSnapshot` invokes a model. The actor is
  the field that says who is responsible for a step; inferring this from a
  free-text label would let a rename change what executes.
- **With no executor configured, an agent step now fails** with
  `model_executor_unconfigured`, not retryable. A step that was supposed to call
  a model and did not has not been performed, and reporting success made the
  whole run a false record. This is a visible behaviour change.
- Steps assigned to a principal, a worker or the system are untouched — they
  were never meant to call a model.
- A completed step carries two output references: the artifact (what was
  produced) and the model invocation (what it cost).
- A failed step still schedules the run to advance, or the run stalls in
  `Running` with nothing queued.
- Provider outages are retryable; an unreadable provider response is not,
  because retrying an adapter that cannot parse replies produces the same reply.

### Scope of the execution path

Single-shot: one prompt, one completion, no tool use, no multi-turn loop. The
prompt is the run's objective. **This is not an agent loop**, and the code says
so where it could otherwise be assumed.

### Defects fixed in the pre-existing provider adapter

`OpenAiCompatibleClient` existed, was exported, and was called by nothing. Four
silent failure modes:

1. `Client::builder().build().unwrap_or_default()` fell back to a client with
   **no timeout** — the single property the builder existed to set. A wedged
   provider would hold a worker slot forever.
2. Missing `choices[0].message.content` became an empty string, so a response
   the adapter could not read was recorded as the model having said nothing.
3. Missing `usage` token counts became zero, understating consumption in a
   system that enforces budgets — the budget would be spent without appearing
   spent.
4. Every non-2xx status collapsed to one error, so a retryable rate limit was
   indistinguishable from a permanent 4xx.

Error bodies are still not included in failures: they echo request content back,
which may be the data a caller is careful about.

### Credential handling

The provider key is resolved from the secret store **per invocation**, scoped to
the executing workspace. A process-wide key read at start-up would mean one
key for every workspace the process serves, no rotation without a restart, and a
plaintext credential resident for the process's lifetime.

The model row is looked up by name per workspace rather than configured as a
UUID: `models` is workspace-scoped, so one configured id could only ever be
correct for one workspace. An absent row is a configuration error, never
invented — the accounting record has a foreign key to `models`, and a fabricated
id would attribute cost to another model.

## Evidence

- crypto unit tests — 14 passed;
- `secret_store` against live PostgreSQL 17 — 11 passed;
- `artifact_content` against live PostgreSQL 17 — 7 passed;
- `execute_step` — 7 passed;
- provider adapter unit tests — 6 passed;
- configuration tests — 17 passed;
- secret storage verified end-to-end against the running stack: stored, listed,
  audit event recorded, 49 bytes of ciphertext with a 12-byte nonce in the
  table, zero matches searching the table for the plaintext, value read refused
  with 403, deletion returned 204. The verification secret was removed.

## The execution path is not reachable from the API

Found while verifying the above against the running stack, and **unresolved**.

There are two run mechanisms, and they do not meet:

|  | `/v1/runs` | worker |
| --- | --- | --- |
| path | `RunCommandService` → `PgRunCommandCommitter` | `RunCoordinator` → `PostgresRunStore` |
| writes | `agent_runs`, `run_events`, `run_streams` | `agent_runs`, `run_events`, `run_steps`, `run_work_items` |
| step model | `RunCommand::StartStep { step_id, kind, label }` | `NewRunStep { assigned_actor, input_references }` |

No HTTP route creates a step, and nothing in the repository calls
`AddRunSteps`. After four runs created through the API the live database held:

```text
agent_runs     | 4
run_events     | 4
run_steps      | 0
run_work_items | 0
run_leases     | 0
```

So the worker has nothing to claim, and the model invocation described above —
correct and covered by tests — cannot currently be triggered from outside.

They cannot simply be joined: **both write `agent_runs` and `run_events`** with
independent optimistic version counters, so a naive connection produces version
conflicts and interleaved sequences in the canonical log. Which model is
authoritative is a design decision, not a patch, and it is left open here rather
than settled by whichever change happened to be easiest.

## The worker was silent

`init_tracing` was private to `server.rs` and only the server called it, so the
worker installed no subscriber. Every `tracing::info!` and `tracing::warn!` it
emitted was discarded — including the warning added by the startup-recovery work
saying it served no workspaces. A misconfigured worker was indistinguishable
from a healthy idle one. Fixed by sharing the initialiser; verified by observing
the warning appear in a running container.

## Remaining gaps

- **No production crypto adapter.** KMS, HSM, Vault and OS-keyring custody are
  all unimplemented; only `local-file` exists, and it does not qualify.
- **No `V1ReleaseEvidenceProbe` implementation** outside a test fixture. The
  v1.0 gate can judge evidence but nothing collects it.
- **Conformance coverage is 40 of 199 statements** (0 failed, 124 with no
  registered case, 35 not applicable to the Trusted profile). GOV, EXT, REC,
  CAP, IDW, RET and HLT are at zero.
- **AG-UI answers 501** on all three routes, while the console's chat component
  calls them.
- **No agent loop**: single-shot invocation only, no tools, no planning.
- **Nothing registers artifacts other than step output**, and no scheduler
  dispatches triggers.
- **Authentication is one shared token** with no per-user attribution and no
  revocation.
