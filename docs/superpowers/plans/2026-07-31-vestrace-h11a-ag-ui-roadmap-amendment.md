# Vestrace Harness v0.2 Roadmap Amendment — H11A AG-UI Interaction Gateway

> **Status:** Approved roadmap amendment. This file is normative together with `2026-07-31-vestrace-harness-roadmap.md` until the consolidated roadmap is regenerated.

**Decision source:** `docs/superpowers/specs/adr/0004-ag-ui-interaction-boundary.md`

## Purpose

Add a dedicated AG-UI phase without changing the ownership of H1–H11 or treating AG-UI as a replacement for H7 public events, H11 APIs or the web console.

## Revised dependency tail

```text
H9A A2A Interoperability Gateway
        ↓
H10 Observability and Evaluation
        ↓
H11 Universal Vertical Slice and Product Surface
        ↓
H11A AG-UI Interaction Gateway
        ↓
Final Harness v0.2 readiness
```

H11A follows H11 because it consumes:

- the H11 HTTP/SSE authentication and public error boundary;
- the H11 TypeScript SDK and console security model;
- H11 public schema publication and release-manifest tooling;
- the H11 reference package and product-profile surfaces.

H11 remains responsible for the ordinary product and administrative UI. H11A adds the standard interactive agent workspace protocol and final interaction conformance gates.

## Global constraints added by this amendment

- AG-UI types remain confined to an explicit adapter, TypeScript integration layer and wire tests.
- AG-UI Run identifiers are external correlation keys mapped to `AgentRun`, never authoritative IDs.
- H7 `PublicEventRecord` and cursor remain the durable stream source.
- AG-UI state snapshots/deltas are UI projections only.
- Client-supplied Tools do not create H4 Tools or capabilities.
- Frontend actions that cause server effects use ordinary H11 application commands and H2 authorization.
- `RUN_FINISHED success` is emitted only after H10 verified completion.
- `RUN_FINISHED interrupt` ends an interaction stream but does not terminate the underlying `AgentRun`.
- `RAW`, `rawEvent`, deprecated `THINKING_*`, all reasoning events and opaque encrypted reasoning are prohibited.
- `CUSTOM` events require a Vestrace namespace, exact schema and viewer-policy filtering.
- Multimedia and URL input pass H6 intake, quarantine and inspection before Run execution.
- AG-UI does not replace `/v1`, `/admin/v1`, MCP or A2A.
- The implementation uses an exact AG-UI source/schema pin and upgrade conformance gate.
- H11A creates only forward migrations after H11 migration `0086` and never edits earlier migrations.

## H11A. AG-UI Interaction Gateway

### Builds

- Vestrace-owned protocol-neutral interaction projection and input DTOs;
- exact-pinned AG-UI adapter and schema validation;
- durable `AGUIRunBinding` from external thread/run keys to `Conversation` and `AgentRun`;
- authenticated `/v1/ag-ui` HTTP POST + SSE surface;
- H7 event-to-AG-UI lifecycle, message, step, activity and state projection;
- Vestrace cursor/event/run-version extension envelope;
- typed `HumanRequest` to AG-UI interrupt and resume mapping;
- exact approval continuation through H2;
- H6 multimedia/document/URL intake;
- safe Tool-call and Artifact projection;
- allowlisted frontend actions and generative UI cards;
- state snapshot/delta schemas with RFC 6902 path and revision validation;
- TypeScript console AG-UI workspace integration;
- optional third-party AG-UI client compatibility;
- conformance, restart, reconnect and security-negative suites;
- release-manifest AG-UI schema/source-pin evidence.

### Initial product binding

```text
Authenticated HTTP POST RunAgentInput
→ durable Vestrace intake/binding
→ SSE AG-UI events
```

WebSocket and binary AG-UI transport are deferred unless the detailed H11A plan proves a release requirement.

### Standard event profile

Enabled:

```text
RUN_STARTED
RUN_FINISHED
RUN_ERROR
STEP_STARTED
STEP_FINISHED
TEXT_MESSAGE_START
TEXT_MESSAGE_CONTENT
TEXT_MESSAGE_END
TOOL_CALL_START
TOOL_CALL_ARGS only when safely publishable
TOOL_CALL_END
TOOL_CALL_RESULT only as safe projection
STATE_SNAPSHOT
STATE_DELTA
MESSAGES_SNAPSHOT
ACTIVITY_SNAPSHOT
ACTIVITY_DELTA
allowlisted vestrace.* CUSTOM
```

Disabled:

```text
RAW
rawEvent
THINKING_*
REASONING_*
REASONING_ENCRYPTED_VALUE
arbitrary CUSTOM
```

### Mandatory acceptance scenario

```text
authenticated AG-UI client
→ external thread/run key binding
→ user message plus document input
→ H6 quarantine and inspection
→ one Conversation and one AgentRun
→ RUN_STARTED
→ text, step and activity streaming
→ safe state snapshots/deltas
→ HumanRequest interrupt
→ exact authenticated resume
→ same AgentRun continuation
→ safe Tool and Artifact projections
→ complete server/worker restart
→ reconnect from durable H7 cursor
→ H10 verified terminal result
→ RUN_FINISHED success
```

### Mandatory negative gates

1. Repeating the same external run key and canonical input does not create another Run.
2. Reusing the key with changed input returns conflict.
3. Client state cannot mutate Run, plan, policy, budget or approval state.
4. Client Tool declarations cannot create or activate H4 Tools.
5. Generic resume payload cannot create an approval.
6. Inbound developer/system/reasoning messages cannot override runtime instructions.
7. Multimedia and URL content cannot bypass H6.
8. Sensitive Tool arguments and results are not exposed.
9. Reasoning and RAW events are absent from output.
10. Reconnect does not duplicate logical messages, Tool projections or terminal completion.
11. A transport disconnect does not fail or cancel the Run.
12. AG-UI SDK types do not appear in domain, application or persistence contracts.
13. Frontend-generated effects cannot bypass H11 commands, H2 policy or exact approval.
14. `RUN_FINISHED success` cannot be emitted before H10 completion consumption.
15. An interrupt stream finish does not mark `AgentRun` terminal.

### Exit gate

A Vestrace reference agent can be used from the built-in console and an independent deterministic AG-UI client. Both clients can create, stream, interrupt, resume, reconnect and complete one durable verified Run without exposing reasoning, duplicating effects, bypassing Artifact intake or creating authority from client state/Tools.

## H11 integration amendment

The H11 implementation plan remains authoritative for product APIs, SDKs, console administration, deployment, backup and the base vertical slice. Its final release acceptance is extended as follows:

- the console contains an AG-UI interactive workspace in addition to ordinary administration pages;
- `ProductSurface` includes `AgUiHttpSse`;
- the public schema bundle includes AG-UI input/event/extension schemas and exact source-pin metadata;
- the release manifest includes the AG-UI schema digest, adapter revision and conformance report;
- Personal may enable the local AG-UI console surface by safe default;
- Team and Embedded enable it explicitly;
- final Harness completion requires the H11A exit gate.

## Branch policy amendment

Suggested implementation branch:

```text
feat/harness-ag-ui-gateway
```

The branch is not created during the documentation-only phase.

## Verification amendment

H11A additionally requires local, network-isolated checks for:

- input/event schema golden vectors;
- H7 cursor reconnect and event deduplication;
- interrupt/resume idempotency;
- state-patch path/revision enforcement;
- frontend action policy boundaries;
- hidden-reasoning/RAW absence;
- multimedia quarantine;
- cross-workspace binding isolation;
- adapter type-boundary enforcement;
- TypeScript client and built-in console compatibility.

No public AG-UI service is required in CI.

## Scope exclusions added

- AG-UI as the authoritative Run journal;
- AG-UI state as domain state;
- client-defined backend Tools;
- reasoning or encrypted reasoning transport;
- arbitrary custom events;
- unrestricted frontend effects;
- mandatory AG-UI WebSocket or binary transport;
- replacement of H11 product administration APIs;
- direct persistence of AG-UI SDK objects;
- automatic trust of client context, transcript or forwarded properties.

## Revised completion definition

The Harness v0.2 roadmap is complete only when H1–H11, H9A and H11A pass their exit gates, H0-RIG has a conclusive follow-up ADR, and the mandatory vertical slice succeeds through:

- the native OpenAI-compatible provider path;
- internal SubRuns;
- one external A2A delegation;
- one AG-UI interactive client with interrupt/resume and restart reconnect;
- verified Artifact export and H10 completion;
- no duplicate or unauthorized effects.
