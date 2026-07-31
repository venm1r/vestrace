# ADR-0003: A2A Interoperability Boundary

**Status:** Accepted  
**Date:** 2026-07-31  
**Decision owners:** Vestrace maintainers  
**Related design:** `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`  
**Related roadmap:** `docs/superpowers/plans/2026-07-31-vestrace-harness-roadmap.md`  
**Related decisions:** `docs/superpowers/specs/adr/0001-rig-integration-boundary.md`

> ADR-0002 remains reserved for the future H0-RIG spike outcome. Numbering reflects decision lineage, not file creation order.

## Context

Vestrace needs a standards-based way to delegate work to independent external agents and to expose selected Vestrace agents to other runtimes. The A2A v1 protocol provides agent discovery through Agent Cards, task-oriented messaging, streaming status, input-required and authentication-required states, artifacts, cancellation and multiple transport bindings.

The reviewed `a2a-rs` workspace implements A2A v1 protocol types plus client and server libraries for JSON-RPC, HTTP+JSON, SSE and gRPC, with additional SLIMRPC support. Its Rust crates are currently pre-1.0, so direct use of its types in Vestrace domain, persistence or public application contracts would create unnecessary coupling.

A2A also defines its own `Task`, lifecycle and artifact representations. Treating those objects as authoritative Vestrace `AgentRun`, `SubRun` or `Artifact` records would blur ownership, weaken durable recovery and allow a remote system to influence security-sensitive state.

## Decision

Vestrace adopts A2A as the standard protocol for interoperability with independent external agents. The `a2a-rs` SDK is used only through an anti-corruption adapter as a replaceable client/server wire implementation.

```text
vestrace-domain
vestrace-application
        │
        ▼
vestrace-remote-agent-runtime
        │ Vestrace-owned ports and DTOs
        ▼
vestrace-a2a-adapter
        │ a2a-rs client/server bindings
        ▼
independent A2A agent or client
```

No `a2a-rs` type may appear in:

- `vestrace-domain` aggregates or value objects;
- application service signatures outside the adapter boundary;
- PostgreSQL schemas;
- authoritative Run, SubRun, checkpoint or journal records;
- public Vestrace HTTP, MCP, extension or SDK contracts unless the endpoint is explicitly an A2A protocol endpoint;
- Agent Profile, Skill, Workflow or Package definitions.

## Scope

The first supported A2A scope is:

- core A2A protocol types;
- outbound A2A client;
- inbound A2A server;
- JSON-RPC over HTTP;
- HTTP+JSON/REST;
- SSE streaming and task subscription;
- Agent Card discovery and publication;
- message, task-status and artifact mapping;
- cancellation requests;
- input-required and authentication-required continuation flows.

Deferred until a separate decision:

- gRPC as a required product binding;
- SLIMRPC and collaborative group channels;
- push notification callbacks;
- automatic cross-organization credential delegation;
- automatic trust in Agent Card signatures or claims;
- protocol extensions that broaden authority.

## Ownership model

An A2A Task is not a Vestrace `AgentRun` or internal `SubRun`.

```text
Vestrace owns:
  parent AgentRun
  plan and delegation decision
  policy and budget state
  durable RemoteAgentInvocation record
  accepted artifacts and verification result

Remote service owns:
  external A2A Task execution
  its internal planning and runtime
  remote task history and progress

Vestrace observes:
  external task ID and context ID
  remote status and messages
  streamed artifacts
  reconciliation evidence
```

Vestrace introduces a first-class `RemoteAgentInvocation` aggregate bound to a parent Run and step. It stores the selected remote-agent definition revision, external identifiers, observed state, authorization and budget references, event cursor, output references and reconciliation state.

Required canonical statuses include:

```text
Prepared
Dispatching
Working
WaitingForRemoteInput
WaitingForAuthentication
WaitingForDependency
Succeeded
Failed
Rejected
Cancelled
Unknown
```

`Unknown` is mandatory when the remote service may have accepted or completed work but Vestrace cannot establish the outcome.

## Outbound delegation

Outbound flow:

```text
DelegationRequest
→ remote-agent eligibility and trust checks
→ Policy Engine
→ budget reservation
→ operation-bound credential lease
→ RemoteAgentInvocation
→ A2A client adapter
→ external A2A Task
→ streamed observations
→ artifact quarantine and validation
→ verified HandoffArtifact
→ parent Run continuation
```

Every dispatch and cancellation is a protected operation with its own operation fingerprint and H2 `GuardedAction`. Retrying a request after an ambiguous network outcome is forbidden until reconciliation proves that a new dispatch is safe.

A remote agent receives only explicitly delegated context, data classifications, resource references and budget. It never receives Vestrace authorization tickets, approval grants, permanent credentials, unrestricted memory access or the parent Run checkpoint.

## Inbound A2A server

Inbound flow:

```text
A2A request
→ transport authentication
→ A2A protocol validation
→ Vestrace principal/workspace resolution
→ Policy Engine
→ CreateRun or ContinueRun
→ durable AgentRun execution
→ RunEvent projection
→ A2A task/status/message/artifact stream
```

The A2A transport-supplied `tenant`, task ID, context ID, headers or metadata are consistency and routing inputs only. They never establish Vestrace workspace identity, principal identity or authority by themselves.

The protocol-facing A2A Task representation is a projection or binding over Vestrace state. An `a2a-server` task store may cache protocol projections, but it is never the source of truth for Run state, leases, checkpoints or replay.

## Agent Card boundary

An Agent Card is an untrusted remote declaration used for discovery and compatibility negotiation. It is not:

- a capability grant;
- proof of identity by itself;
- permission to transfer classified data;
- a quality guarantee;
- approval to invoke a skill;
- authorization to use a credential.

Vestrace stores a versioned `RemoteAgentDefinitionRevision` containing:

- source URL and retrieval time;
- exact card bytes or normalized snapshot;
- content hash;
- declared interfaces, skills and security schemes;
- verified external identity when available;
- local trust classification;
- locally allowed transports, skills and data classifications;
- lifecycle and health state.

Card changes create a new revision and require compatibility and permission-diff evaluation before activation.

## Authentication and credentials

The convenience in-memory credential store and permanent bearer-token interceptor provided by `a2a-rs` are not production credential storage for Vestrace.

Production authentication uses:

```text
Connection
→ Policy Engine
→ Credential Broker
→ short-lived operation-bound CredentialLease
→ request-scoped A2A auth interceptor
→ transport request
```

Secret material is resolved immediately before dispatch, excluded from durable A2A messages and invocation state, redacted from diagnostics and released after the request. Remote `AuthRequired` becomes a typed Vestrace authentication request; it does not permit the remote agent to choose credentials or scopes.

## Artifact and content boundary

All Agent Cards, messages, metadata, status text and artifacts are external untrusted input.

A2A artifact parts map as follows:

```text
Text
→ untrusted text content

Data/JSON
→ schema-checked external structured data

Raw bytes
→ bounded streaming quarantine and content hashing

URL
→ policy-controlled retrieval with SSRF, scheme, DNS and size checks
```

No A2A artifact becomes an available Vestrace Artifact directly. Required pipeline:

```text
receive
→ size and media validation
→ quarantine
→ content hash
→ malware and secret inspection
→ representation processing
→ provenance attachment
→ policy decision
→ available or rejected
```

Remote `Completed` does not imply Vestrace `Succeeded`. Required output schemas, evidence, artifact validation and verification policies must pass first.

## State mapping

Initial mapping guidance:

| A2A state | Vestrace observation |
| --- | --- |
| `Submitted` | `Dispatching` or `Working` |
| `Working` | `Working` |
| `InputRequired` | `WaitingForRemoteInput` plus typed `HumanRequest` |
| `AuthRequired` | `WaitingForAuthentication` |
| `Completed` | validation pending, then `Succeeded` or `Failed` |
| `Failed` | `Failed` with normalized remote failure |
| `Rejected` | `Rejected` |
| `Canceled` | `Cancelled` observation; no rollback claim |
| lost or ambiguous response | `Unknown` and reconciliation |

Cancellation is a request to the remote service, not proof that already committed remote effects were undone.

## Extension and transport strategy

The first adapter is isolated in:

```text
vestrace-remote-agent-runtime
├── RemoteAgentPort
├── RemoteAgentInvocation
├── RemoteAgentDefinitionRevision
├── canonical events and errors
└── reconciliation contracts

vestrace-a2a-adapter
├── client
├── server
├── Agent Card mapping
├── task and event mapping
├── artifact mapping
├── auth
└── transport error normalization
```

Transport selection is configuration and policy data. Domain logic does not branch on `a2a-rs`, JSON-RPC, REST or SSE implementation types.

## Relationship to other Vestrace layers

- H4 Tool Runtime does not model a remote A2A agent as a tool. External delegation is a distinct H5/H9A runtime path.
- H5 defines internal SubRun and external `RemoteAgentInvocation` as separate delegation targets.
- H6 owns ingestion and validation of remote artifacts and context sent to external agents.
- H7 owns channel-independent human continuation and durable streaming semantics used by inbound/outbound A2A flows.
- H8 owns remote-agent Connections, credential leases and authentication continuation.
- H9 owns adapter registration, remote-agent definition revisions and permission-diff activation.
- H9A implements and validates the A2A interoperability gateway.

## Conformance requirements

The A2A adapter requires deterministic conformance tests for:

- Agent Card retrieval, hashing, revision and trust handling;
- JSON-RPC and HTTP+JSON request equivalence;
- SSE reconnect and duplicate-event handling;
- create, get, list, subscribe and cancel task operations;
- `InputRequired` continuation;
- `AuthRequired` continuation;
- artifact text, JSON, raw-byte and URL handling;
- external task ID reconciliation after lost responses;
- cancellation without rollback claims;
- request-scoped credential redaction;
- inbound mapping to CreateRun/ContinueRun;
- outbound mapping from DelegationRequest;
- restart recovery without duplicate remote dispatch;
- absence of `a2a-rs` types in domain/application/persistence contracts.

Tests use local deterministic A2A fixtures and require no external service or public network.

## Acceptance scenario

```text
parent AgentRun
→ protected remote delegation
→ external A2A task
→ streamed Working status
→ InputRequired
→ durable Vestrace pause
→ user response
→ continue the same external task
→ streamed artifact
→ quarantine and validation
→ verified HandoffArtifact
→ parent Run continuation after worker restart
```

A second failure-path scenario drops the response after remote task acceptance. Vestrace must reconcile through the known task/context identifiers or remain `Unknown`; it must not create a duplicate remote task automatically.

## Consequences

### Positive

- Vestrace gains standards-based interoperability without surrendering runtime ownership.
- Independent agents can be delegated to and Vestrace agents can be published through one protocol family.
- Durable Run, policy, budget, credential and artifact controls remain consistent.
- `a2a-rs` upgrades are localized to one adapter and its conformance suite.
- Internal SubRun semantics remain optimized for trusted Vestrace-managed workers.

### Costs

- Vestrace must maintain explicit mapping between A2A Tasks and `RemoteAgentInvocation`.
- Remote and local state can diverge and requires reconciliation.
- Agent Card discovery adds trust, freshness and permission-diff management.
- Artifact URLs and push-style callbacks introduce SSRF and credential risks, so the initial scope is deliberately narrow.
- Supporting both inbound and outbound roles adds protocol conformance work.

## Rejected alternatives

### Treat A2A Task as AgentRun or SubRun

Rejected because the remote service owns execution semantics and Vestrace cannot guarantee its journal, leases, checkpoints, policy or side effects.

### Implement a custom proprietary agent-to-agent protocol first

Rejected because A2A already covers discovery, tasks, streaming, input/auth continuation and artifacts, while a private protocol would reduce interoperability.

### Use remote agents as Tool Runtime adapters

Rejected because a remote agent is a delegated autonomous execution boundary with its own task lifecycle, not a single tool operation.

### Expose `a2a-rs` types throughout Vestrace

Rejected because the crates are pre-1.0 and their wire/domain model does not match Vestrace authority and durability semantics.

## Non-goals

This decision does not approve:

- remote agents as trusted peers by default;
- sharing full Vestrace memory or Run state;
- accepting remote artifacts without quarantine;
- remote mutation of policy, profiles, skills or workflows;
- permanent credentials in A2A client configuration;
- automatic fallback that duplicates an ambiguous external task;
- mandatory gRPC, SLIMRPC or push-notification support in v0.2;
- A2A as the internal worker or extension protocol for all components.
