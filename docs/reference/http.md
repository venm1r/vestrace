# HTTP conventions

**Scope:** The supplied source snapshot, not every proposed endpoint. The [route catalog](route-catalog.md) describes inventory declarations; the runtime schema and proposed MW OpenAPI are different artifacts. A route descriptor establishes an intended authorization boundary, not a tested handler.

## Identity and authorization

A Bearer token resolves to a workspace/principal. Middleware replaces client-supplied identity headers with those resolved values. Invalid, expired, or revoked credentials must not expose details that help guess a credential. In the reviewed middleware, token-store failure returns `503`, not a misleading `401`.

`/health/live` and `/health/ready` are bounded public probes; `/metrics` requires authorization. Capabilities, risk, and current policy determine access. Permission to read data is distinct from permission to transmit it to a model/provider or export destination.

## Idempotency and concurrent changes

Create a stable `Idempotency-Key` for each logical write. An identical HTTP retry uses the same key; a different command uses another key. The key is not a capability. The legacy memory handler falls back to `x-request-id`; new clients should send the explicit idempotency header instead of depending on that fallback.

The current revision handler parses `If-Match` as a numeric `u32` string, not a general ETag or `W/"..."`. Do not apply MW's proposed precondition rules to the legacy endpoint without explicit compatibility handling. Run versions and memory content revisions are separate values.

## Errors

`ApiError` and the specific handler define exact status/code behavior. Do not treat every 4xx as a missing object or automatically retry every 5xx. An unknown external outcome requires reconciliation rather than blind SDK retry.

| Observation | Client action |
| --- | --- |
| 401 | Check the supplied credential, not workspace-header permutations. |
| 403 / policy refusal | Obtain a lawful scope/grant or stop the operation. |
| 404 | Handle absence within the permitted scope; do not probe hidden namespaces. |
| Revision/idempotency conflict | Read permitted current state and make an explicit new decision. |
| 501 | The route is unimplemented; the UI must not report success. |
| 503 / unavailable | Preserve request identity and establish what committed before retrying a mutation. |

This is handling guidance, not a new uniform error contract for every existing endpoint.

## Compatibility

Additive fields can still break strict generated clients. Check schema and actual responses, deprecation policy, and errors for each change. Proposed MW routes become available only after implementation and acceptance; their schema is not the runtime schema for an existing client.

**Sources:** [authentication](../../crates/vestrace-http/src/auth.rs), [inventory](../../crates/vestrace-http/src/route_inventory.rs), [Memory handlers](../../crates/vestrace-http/src/api/memory.rs), [Console client](../../apps/console/src/sdk/client.ts).
