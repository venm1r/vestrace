# Documentation Gap Delta — Live Provider Verification

**Date:** 2026-08-13
**Scope:** executing a run step against a real model provider
**Provider:** NVIDIA NIM (`https://integrate.api.nvidia.com/v1`), `meta/llama-3.1-8b-instruct`
**Repository state:** dirty, implementation changes uncommitted

## Why this was worth doing

Every previous run in this repository failed at the same place, with
`model_executor_unconfigured`. That is the honest outcome for a system with no
provider key, and it was deliberately introduced — the build before it reported
success for agent steps that invoked nothing. But it also meant the entire
execution path past `ExecuteStepHandler` had never run outside a test double:
no provider adapter had spoken to a provider, no completion had been stored,
and no cost had been recorded from a real invocation.

The 787-test suite says nothing about this. It cannot: the provider is a port,
and every test supplies a fake.

## What was verified

The full chain, on the live Docker stack:

```text
POST /v1/secrets            key stored, AES-256-GCM, never returned
POST /v1/runs               run created,  version 1
POST /v1/runs/{id}/steps    agent step added
worker                      leases → authorizes → resolves secret → calls NVIDIA
                            → stores completion as artifact → records cost
                            → step succeeded → run succeeded, version 7
```

Evidence from the live database, not from logs:

| check | result |
|---|---|
| `model_executions` | `succeeded`, 55 prompt / 36 completion tokens, 1039 ms |
| `run_steps.output_references` | one `artifact`, one `model_invocation` |
| artifact digest | `sha256(bytes) = content_hash` verified in SQL |
| `agent_runs.run_version` vs `run_streams.current_version` | both 7 |
| canonical events | 7, all `event_version = 1`, all carrying a correlation id |
| `run_work_items` | 6, all `completed` |

### The secret is stored as ciphertext

```text
name    purpose            kek_version    ct_bytes  nonce  'nvapi' in ciphertext
nvidia  provider-api-key   local-dev-v1   86        12     0
```

86 bytes for a 70-byte key is the plaintext length plus the 16-byte GCM tag. No
log line and no `run_events` payload anywhere in the stack contains the string
`nvapi`.

### Model output does not enter the event log

`run.step_status_changed` carries the step id and the transition and nothing
else. The completion text lives only in `artifact_blobs`, which is purgeable;
an append-only event holding model output would not be.

## Defects this exposed

### 1. A rejected credential was reported as an unusable response

**Found by:** deliberately storing an invalid key and running a step.

`map_status` had no case for 401/403, so they fell through to the catch-all and
became `ProviderError::InvalidResponse` → `ApplicationError::Internal`. The
operator saw:

```text
model_invocation_failed: internal failure: model provider returned an
unusable response: provider returned 403 Forbidden
```

Correct in outcome — the step failed, not succeeded, and `retryable` was
`false`, which is right, because a wrong key does not become right on retry.
Wrong in diagnosis: this reads like an adapter bug, and it is the one provider
failure the operator can fix unaided.

`ProviderError::CredentialRejected` now exists and maps to
`ApplicationError::InvalidConfiguration`:

```text
model_invocation_failed: invalid configuration: the model provider rejected
the configured credential (provider returned 403 Forbidden); replace the
workspace secret holding the provider API key
```

Both 401 and 403 map to it: NVIDIA answers 403 to a bad key, OpenAI 401.
Verified against the live provider after redeploying, not only in unit tests.

### 2. The console showed artifacts with no digest

**Found by:** being the first person to look at the artifacts table with an
artifact in it.

`ArtifactItem` declared `kind`, `size` and `checksum`. The adapter sends
`media_type`, `size_bytes` and `content_sha256`. Three columns rendered blank,
including the checksum — the field that makes an artifact listing a provenance
record rather than a filename list.

The mismatch survived because the response is cast rather than validated, so
the compiler could not see it, and because until this run no artifact had ever
existed: an empty table hides a field mismatch perfectly. The type was also
still sitting under the comment block describing surfaces that answer 501,
which this one no longer does.

### 3. The console and server read different environment variables

`docker-compose.yml` gave the server `${VESTRACE_ADMIN_TOKEN}` and the console
`${VESTRACE_AUTH__ADMIN_TOKEN}`. Overriding either one alone gave the console a
token the server would not accept, and every proxied request returned 401. Both
now read `VESTRACE_ADMIN_TOKEN`. Latent, because the defaults happen to agree.

## Findings that are not defects

### Reasoning models need a larger token budget than instruct models

NVIDIA's reasoning models (`nvidia/nvidia-nemotron-nano-9b-v2`,
`openai/gpt-oss-20b`) return `content: null` with the text in
`reasoning_content` when `max_tokens` is exhausted by the reasoning trace. At
64 tokens both returned nothing usable; at 1024 the same model returned
`content` normally.

The adapter rejects a null `content` as `InvalidResponse`, which is the right
call — recording an empty completion as success is how a system produces false
records. But an operator who sets `VESTRACE_MODEL__MAX_TOKENS` low and picks a
reasoning model will get "provider response has no choices[0].message.content"
and no hint that the budget is the cause. `.env` currently sets 512, which is
adequate for instruct models and marginal for reasoning ones.

This is not fixed here. Fixing it properly means reading `finish_reason` and
distinguishing `length` from `stop`, which is a change to the adapter's
contract, not a message tweak.

## Configuration used

`.env` (gitignored, no credential in it):

```env
VESTRACE_MODEL_ENABLED=true
VESTRACE_MODEL_BASE_URL=https://integrate.api.nvidia.com/v1
VESTRACE_MODEL_NAME=meta/llama-3.1-8b-instruct
VESTRACE_MODEL_SECRET_NAME=nvidia
VESTRACE_MODEL_MAX_TOKENS=512
```

Three things must line up or the step fails at a different place each time:

1. `VESTRACE_MODEL_NAME` must match a row in `models` **in the executing
   workspace** — `ProviderStepModelExecutor::model_id` looks it up by name and
   refuses to invent one, because `model_executions` has a foreign key to it.
2. `VESTRACE_MODEL_SECRET_NAME` must match a secret whose purpose is exactly
   `provider-api-key`; the resolution lease is refused otherwise.
3. Server and worker must carry the same `VESTRACE_SECRETS__MASTER_KEY`, or the
   worker cannot decrypt what the server stored.

## What still has not been exercised

- **Multi-turn or tool use.** `model_step.rs` states its scope: one prompt, one
  completion. The prompt is the run's objective and nothing else. This is a
  real model invocation; it is not an agent loop.
- **Prompt construction.** The run title *is* the prompt. There is no system
  prompt, no context injection and no retrieval, so the model answers with no
  knowledge of the workspace. The first live run answered its question wrongly
  for exactly this reason.
- **Rate limiting and timeouts.** `RateLimited` and `Timeout` map to
  `Unavailable` and are therefore retryable, but no live run has hit either.
- **RLS under the runtime role.** Unchanged from the earlier delta: `sqlx::test`
  connects as superuser, and these live checks queried as the bootstrap role.
- **Cost figures.** The registered model carries `0.0` for both token prices, so
  `model_executions` records tokens but no money.
