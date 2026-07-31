# ADR-0004: AG-UI Interaction Boundary

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Vestrace architecture
- **Related:** ADR-0003 A2A Interoperability Boundary
- **Implementation phase:** H11A AG-UI Interaction Gateway

## Context

Vestrace already defines authoritative durable execution, conversation, human-interaction, artifact, policy and product surfaces:

- H1 owns `AgentRun`, `RunStep`, `RunEvent`, checkpoints and logical completion;
- H2 owns policy, approvals, capabilities and budgets;
- H4 owns backend Tool and Sandbox execution;
- H6 owns Artifact intake, quarantine, representations and provenance;
- H7 owns `Conversation`, `InteractionEvent`, `HumanRequest`, `HumanResponse`, public events and durable cursors;
- H9A owns independent agent-to-agent interoperability through A2A;
- H10 owns verification and the one-use completion gate;
- H11 owns the versioned product API, SDKs and web console.

AG-UI is an open, event-oriented protocol for connecting agents to user-facing applications. Its current public repository describes lifecycle, text, Tool-call, state, activity, custom and reasoning events; `RunAgentInput` carries thread/run identifiers, state, messages, Tools, context and resume entries. The observed source baseline for this decision is:

```text
repository: ag-ui-protocol/ag-ui
source revision: bb1c2afddb4880309879b9564cfb3a635a5da4eb
TypeScript @ag-ui/core: 0.0.57
community Rust ag-ui-core/ag-ui-client: 0.1.0
license: MIT
```

The protocol is a strong fit for interactive agent applications, but its generic state, message, Tool and Run abstractions cannot become Vestrace authority. Vestrace also has stricter requirements for durable replay, exact approval binding, untrusted content, secret isolation, verified completion and hidden-reasoning exclusion.

## Decision

Vestrace adopts AG-UI as the standard optional **agent-to-application interaction protocol** through an anti-corruption gateway.

```text
MCP   → agent-to-tool interoperability
A2A   → independent agent-to-agent interoperability
AG-UI → agent-to-user-application interoperability
HTTP  → product and administrative integration
```

AG-UI complements H7 and H11. It does not replace them.

The authoritative direction is:

```text
Vestrace journals and application services
→ Vestrace AG-UI projection
→ AG-UI event stream
→ interactive application
```

The inbound direction is:

```text
AG-UI RunAgentInput / resume
→ authentication and bounds
→ Vestrace intake and application commands
→ H2 policy
→ H7 interaction or response
→ H1 AgentRun
```

## Ownership model

### Vestrace owns

- `ConversationId`, `InteractionEvent` and participant authorization;
- `AgentRunId`, Run version, plan, steps, status and checkpoint;
- `HumanRequest`, exact approval challenge and `HumanResponse`;
- Tool definitions, Tool invocations and side effects;
- Artifact identities, bytes, quarantine and safe representations;
- policy, capability, credential and budget decisions;
- durable public event ID and workspace cursor;
- verification and terminal Run completion;
- exact UI projection schema and projection version.

### AG-UI owns

- the external interaction event vocabulary;
- the external request and resume envelope;
- client-side event consumption conventions;
- optional client SDK behavior within the adapter boundary.

### The client application owns

- ephemeral presentation state;
- local navigation and rendering;
- safe client-only actions;
- reconnect cursor persistence appropriate to the application.

The client application does not own Run state, approval state, Tool authority, policy, Artifact availability or completion.

## Anti-corruption boundary

AG-UI SDK and schema types are allowed only in:

```text
crates/vestrace-ag-ui-adapter
packages/sdk-typescript AG-UI integration layer
apps/console AG-UI workspace integration
explicit AG-UI wire/schema tests
```

AG-UI types are prohibited from:

- Vestrace domain contracts;
- application service interfaces outside the adapter;
- PostgreSQL schemas and authoritative persistence;
- `RunCheckpoint` payloads;
- H7 canonical public-event records;
- H4 Tool definitions and invocations;
- H9 package/profile/workflow contracts;
- ordinary `/v1` product DTOs except explicit AG-UI endpoint metadata.

Vestrace-owned protocol-neutral DTOs sit between H7/H11 and the adapter.

## External Run identity

AG-UI `threadId`, `runId` and `parentRunId` are external correlation identifiers, not trusted Vestrace IDs.

H11A introduces a durable binding:

```text
AGUIRunBinding
├── workspace and principal
├── endpoint/agent revision
├── external thread key hash
├── external run key hash
├── ConversationId
├── AgentRunId
├── projection revision
├── last durable H7 cursor
└── lifecycle
```

A repeated authenticated external run key with the same canonical request returns or resumes the same binding. Changed canonical content conflicts. An AG-UI identifier cannot select another workspace, principal, Run or parent relationship.

## Inbound input rules

### Messages

Ordinary clients may submit user content. Inbound assistant, Tool, reasoning, developer or system messages never become trusted conversation history or runtime instructions.

Historical client transcript may be accepted only as explicitly untrusted import material under a bounded compatibility mode. It passes H6/H7 validation and cannot override server-owned conversation records.

### Context

AG-UI context entries are untrusted user-supplied context candidates. They are not system instructions, memory, policy or verified evidence.

They pass classification, size, provenance and transfer checks before inclusion in an H6 context source or H7 interaction.

### State

Inbound AG-UI state is an untrusted UI projection or form payload. It cannot mutate domain entities directly.

The gateway accepts only a versioned, schema-validated allowlist of UI fields and expected projection revision. All domain changes use explicit application commands.

### Tools

Client-supplied AG-UI Tool definitions do not create H4 Tools and do not expand an Agent runtime catalogue.

They may refer only to server-published safe frontend-action definitions. Unknown or changed definitions are rejected.

### Multimedia and URLs

Image, audio, video, document, data, binary and URL inputs pass through H6 bounded intake before they can create or continue a Run.

```text
AG-UI part
→ size/type validation
→ streaming intake or secure URL fetch
→ quarantine and inspection
→ ArtifactRevision reference
→ InteractionEvent
```

Inline bytes and URLs never enter model context directly.

### Forwarded properties

Arbitrary `forwardedProps` are ignored by default. Only versioned `vestrace.*` properties declared in the endpoint schema are accepted.

## Event projection

The gateway projects canonical Vestrace state into AG-UI events.

### Lifecycle

```text
AG-UI RUN_STARTED
← durable AGUIRunBinding and visible AgentRun

AG-UI STEP_STARTED / STEP_FINISHED
← viewer-authorized public RunStep projection

AG-UI RUN_FINISHED success
← consumed H10 VerifiedRunCompletion and authoritative terminal Run

AG-UI RUN_FINISHED interrupt
← an open typed H7 HumanRequest that ends the current interaction stream

AG-UI RUN_ERROR
← safe stream/application error projection
```

`RUN_FINISHED` with `interrupt` ends an AG-UI interaction stream. It does not mark the underlying `AgentRun` terminal. Resume continues the same `AGUIRunBinding` and `AgentRun`.

A transport disconnect is not `RUN_ERROR` and does not alter Run state.

### Messages

Assistant text may use `TEXT_MESSAGE_START`, `TEXT_MESSAGE_CONTENT` and `TEXT_MESSAGE_END` after viewer-policy filtering.

`MESSAGES_SNAPSHOT` is a projection of H7 conversation data, not an alternate conversation store.

### Tool calls

Backend Tool activity is display-only:

```text
H4 ToolInvocation
→ safe public summary
→ AG-UI TOOL_CALL events
```

Raw normalized arguments, credentials, authorization tickets, secret-bearing headers and unsafe Tool results are never emitted. Sensitive calls use a bounded activity summary instead.

An AG-UI Tool result cannot mark a Vestrace Tool invocation successful or provide external-effect authority.

### Activity

`ACTIVITY_SNAPSHOT` and `ACTIVITY_DELTA` are the preferred channel for safe progress:

- planning;
- research;
- waiting;
- Artifact processing;
- sandbox rendering;
- verification;
- reconciliation;
- budget warnings.

Activity content uses closed, versioned schemas and bounded fields.

### State snapshots and deltas

`STATE_SNAPSHOT` represents a non-authoritative UI projection.

`STATE_DELTA` uses RFC 6902 only against that projection and requires:

- exact base projection revision;
- schema validation after patch;
- allowed JSON Pointer paths;
- bounded operation count and byte size;
- rejection of invalid pointer escaping and structural abuse;
- deterministic fallback to a full snapshot on mismatch.

No JSON Patch is applied to `AgentRun`, plan, policy, budget, Tool, Artifact or approval aggregates.

## Durable event and reconnect model

AG-UI does not replace the H7 durable public event stream.

Every projected event includes a Vestrace extension envelope:

```json
{
  "vestrace": {
    "schemaVersion": 1,
    "eventId": "...",
    "cursor": "...",
    "runVersion": 18,
    "projectionVersion": 9,
    "causationId": "..."
  }
}
```

For SSE:

- SSE `id` is the H7 cursor;
- reconnect uses `Last-Event-ID` or the explicit Vestrace cursor parameter;
- disagreement between cursors is rejected;
- delivery is at-least-once;
- clients deduplicate by Vestrace event ID;
- heartbeats do not advance the cursor;
- a projection rebuild may emit a full state snapshot without changing Run state.

## Human requests and resume

AG-UI interrupts map to typed H7 `HumanRequest` records.

```text
HumanRequest
→ AG-UI interrupt with response schema
→ authenticated resume entry
→ exact request and responder validation
→ H7 SubmitHumanResponse
```

For approvals:

```text
resume payload
→ exact HumanRequest
→ exact ApprovalChallenge
→ responder eligibility
→ H2 approval.grant authorization
→ operation fingerprint validation
→ ApprovalGrant
```

A boolean, free-form “yes” or generic resolved interrupt is never sufficient to create approval authority.

Resume entries are idempotent and cannot be replayed against another request or Run.

## Frontend actions

H11A distinguishes two classes.

### Client-only actions

Examples:

- navigate to Run or Artifact;
- focus an approval panel;
- copy a reference;
- expand a safe progress card;
- select a value for a typed form before submission.

These do not call authoritative application services.

### Server commands initiated from UI

Examples:

- submit HumanResponse;
- grant approval;
- cancel or resume Run;
- create download grant;
- export Artifact;
- enable or fire Trigger;
- modify Connection or package activation.

These map to explicit H11 application commands with authentication, idempotency, expected version and H2 policy. They are never executed merely because an AG-UI Tool call was emitted.

## Prohibited and restricted events

### Prohibited in the standard Vestrace profile

- `RAW`;
- `rawEvent` payloads;
- all deprecated `THINKING_*` events;
- `REASONING_START`;
- `REASONING_MESSAGE_*`;
- `REASONING_END`;
- `REASONING_ENCRYPTED_VALUE`;
- arbitrary model- or extension-defined `CUSTOM` events.

Vestrace does not publish hidden chain-of-thought, reasoning-token content or encrypted opaque reasoning payloads.

### Restricted `CUSTOM`

Only namespaced, schema-pinned events are allowed, initially:

```text
vestrace.approval.challenge
vestrace.artifact.preview
vestrace.artifact.available
vestrace.budget.warning
vestrace.verification.summary
vestrace.remote.status
vestrace.operation.unknown
```

A custom event is a display projection. It cannot create authority or bypass an ordinary command.

## Security and privacy

- Authentication and workspace/principal resolution happen before AG-UI input binding.
- All inbound strings, state, context, messages, metadata and parts are untrusted.
- Viewer policy filters every outbound event.
- Secrets, credentials, operation tickets, backend paths and hidden reasoning never enter AG-UI payloads.
- Tool and Artifact content follows classification and export policy.
- Active content is represented only by safe H6 previews or download references.
- Event and state sizes are bounded.
- Client-provided schemas cannot relax server validation.
- The browser token and H11 local-console nonce rules remain unchanged.
- AG-UI cannot introduce a second notification, audit or telemetry authority.

## SDK strategy

The protocol is pre-1.0 and its TypeScript and community Rust packages have different maturity levels. Therefore:

1. Vestrace pins an exact AG-UI source revision and schema digest in H11A.
2. Vestrace-owned protocol DTOs and mapping tests are authoritative inside the server.
3. The TypeScript AG-UI core/client may be used behind a console integration adapter with an exact lockfile pin.
4. The community Rust crates are optional conformance/test dependencies, not domain or application dependencies.
5. Cross-language golden vectors verify event, input, interrupt, state-patch and extension compatibility.
6. An AG-UI dependency upgrade requires adapter conformance, schema diff, security review and H10 regression evidence.

The observed revision `bb1c2afddb4880309879b9564cfb3a635a5da4eb` is the design baseline, not an unchangeable production pin. The H11A implementation plan must select and record the exact implementation pin.

## Deployment and product surface

AG-UI is exposed as an optional H11 product surface:

```text
ProductSurface::AgUiHttpSse
```

Initial binding:

```text
authenticated HTTP POST input
→ SSE AG-UI event response
```

The exact endpoint is versioned under `/v1/ag-ui` and publishes its input/event/extension schemas in the H11 schema bundle.

Personal may enable AG-UI for the local console by default. Team and Embedded require explicit surface configuration. AG-UI never enables A2A, Tools, Connections or Triggers implicitly.

## Interaction console boundary

The H11 web console is split conceptually:

```text
Interactive Agent Workspace
→ AG-UI

Product Administration
→ ordinary H11 /v1 and /admin/v1 APIs
```

The interactive workspace includes chat, streaming, activity, HumanRequests, approvals, safe Artifact previews and generative UI cards.

Administration for policies, models, Connections, packages, extensions, Triggers, audit, backup and upgrade remains on explicit product APIs.

## Sequencing

H11A follows H11 and reuses its HTTP/SSE authentication, TypeScript SDK, console security, schema publication and release tooling.

```text
H10 Observability and Evaluation
→ H11 Universal Product Surface
→ H11A AG-UI Interaction Gateway
→ final Harness v0.2 readiness
```

H11A may add forward-only migrations after H11 migration `0086`. It must not edit H1–H11 migrations.

The final release manifest and acceptance matrix must include the AG-UI schema digest, adapter/source pin, console integration and conformance evidence.

## Required H11A acceptance

```text
authenticated AG-UI client
→ user message and inspected document
→ one Conversation and one AgentRun binding
→ durable RUN_STARTED
→ streamed text/activity/step projections
→ typed interrupt
→ exact resume of same AgentRun
→ safe Tool and Artifact projections
→ full runtime restart and cursor reconnect
→ H10 verified terminal completion
→ RUN_FINISHED success
```

Negative gates prove:

- duplicate input does not create a second Run;
- changed input under the same external run key conflicts;
- client state cannot change Run state;
- client Tools cannot create backend Tools or permissions;
- approval payload cannot bypass H2;
- reasoning and RAW events are absent;
- reconnect does not duplicate logical events;
- multimedia input cannot bypass H6 quarantine;
- sensitive Tool arguments/results are not disclosed;
- frontend actions cannot execute server effects without ordinary commands;
- SDK types do not escape the adapter boundary.

## Consequences

### Positive

- Vestrace gains a standard interactive agent-application protocol.
- The console can use ecosystem-compatible streaming, interrupts, shared state and generative UI.
- MCP, A2A and AG-UI receive distinct, comprehensible responsibilities.
- Existing durable and security guarantees remain authoritative.
- Third-party AG-UI applications can integrate without adopting Vestrace internals.

### Costs

- A translation/projection layer and schema compatibility suite are required.
- AG-UI pre-1.0 evolution must be contained by exact pins and conformance tests.
- Some generic AG-UI capabilities are intentionally unavailable or narrowed.
- The UI must combine AG-UI interaction with ordinary product APIs.

## Rejected alternatives

### Replace H7 public events with AG-UI events

Rejected because AG-UI does not provide Vestrace’s authoritative journal, durable workspace cursor, viewer policy, replay and product-wide event semantics.

### Make AG-UI Run equal to AgentRun

Rejected because client identifiers and protocol lifecycle cannot own Vestrace execution state or verified completion.

### Accept frontend Tool definitions as H4 Tools

Rejected because client declarations cannot create capabilities, Tool trust or side-effect authority.

### Publish reasoning events

Rejected because Vestrace does not retain or expose hidden chain-of-thought and opaque encrypted reasoning creates an unnecessary sensitive channel.

### Use only AG-UI for the entire web console

Rejected because product administration requires explicit stable APIs and resource-oriented workflows beyond an interaction protocol.

### Depend directly on the community Rust SDK throughout the server

Rejected because the protocol is pre-1.0, SDK maturity differs by language and SDK types must not become domain/application contracts.

## Review triggers

Revisit this ADR if:

- AG-UI reaches a stable 1.x specification with materially different identity, cursor or interrupt semantics;
- a standard durable resume/cursor contract becomes mandatory;
- reasoning events become unavoidable for conformance;
- the protocol introduces authority-bearing frontend Tools;
- H7/H11 public-event or authentication contracts change materially;
- a production AG-UI Rust server SDK demonstrates a stable boundary superior to Vestrace-owned DTOs.
