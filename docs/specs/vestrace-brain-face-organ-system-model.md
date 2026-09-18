# Vestrace Brain–Face–Organ System Model

**Status:** Accepted target architecture extension  
**Date:** 2026-08-11  
**Scope:** post-v0.2 system decomposition; documentation only

> This document extends the frozen v0.2 architecture with a system-level decomposition for a persistent autonomous agent. It does not retroactively change v0.2 qualification claims, implementation availability, or the existing 36-PR transition roadmap.

## 1. Purpose

Vestrace requires a stable boundary between persistent cognition, user/host interaction, and replaceable execution environments.

The target system is decomposed into three primary planes:

1. **Brain** — persistent cognition and autonomous reasoning;
2. **Face** — user experience plus a local host-access broker;
3. **Organ** — replaceable execution capability attached to the Brain.

A fourth cross-cutting concept, the **System Interconnect** (informally, the nervous system), carries authenticated commands, events, observations, capabilities, receipts, artifacts, heartbeats, approvals, and telemetry. It does not own canonical state or authority.

```text
                         USER
                          │
                          ▼
                 ┌────────────────┐
                 │      FACE      │
                 │ Desktop / CLI  │
                 │ Host Broker    │
                 └───────┬────────┘
                         │
                  System Interconnect
                         │
                         ▼
                ┌──────────────────┐
                │      BRAIN       │
                │ Vestrace +       │
                │ Prime-like       │
                │ Runtime          │
                └────────┬─────────┘
                         │
                  System Interconnect
                         │
          ┌──────────────┼──────────────┐
          ▼              ▼              ▼
       Coding          Browser        Compute
       Organ            Organ          Organ
          │              │              │
       Docker / remote / sandboxed execution endpoints
```

## 2. Architectural thesis

The system SHALL follow these conceptual identities:

- **Brain = persistent agent identity and cognition.**
- **Face = replaceable user/host interface.**
- **Organ = replaceable execution capability.**
- **Model = cognitive compute resource, not agent identity.**

The agent MUST NOT be identified with a UI process, a model process, a single container, a single worker, or a single running operating-system process.

A Brain may survive replacement or disconnection of any Face, Organ, model provider, or transient worker while retaining the same durable identity, memory, evidence, run history, authority state, and trust state.

## 3. Brain

### 3.1 Definition

The Brain combines:

- **Vestrace** as persistent cognition, authority, durable execution state, evidence, trust, governance, and recovery substrate;
- a **Prime-like autonomous cognition runtime** as active reasoning, planning, working context, RLM-style computation, subagent coordination, model routing, goal management, and autonomous continuation.

These are two halves of one cognition plane, not two competing durable runtimes.

```text
                         COGNITION
                             │
                ┌────────────┴────────────┐
                │                         │
        Active / working            Persistent / durable
          cognition                    cognition
                │                         │
       Prime-like Runtime              Vestrace
```

### 3.2 Brain-owned authoritative state

The Brain is the authoritative owner of durable agent state, including where applicable:

- `AgentIdentity`;
- `Workspace` identity and authority boundaries;
- `AgentRun` and its authoritative execution history;
- Memory identities and immutable revisions;
- Claims, Evidence, Assessments, conflicts, and provenance;
- temporal validity and recorded history;
- capability grants and delegation chains;
- policy decisions, approvals, budgets, and risk state;
- external-effect intents and effect receipts;
- learned proposals and governed learning state;
- incidents, recovery state, trust state, and revalidation evidence;
- qualification evidence and capability manifests.

A Face or Organ MUST NOT become a second canonical owner of any of these states.

### 3.3 Brain-owned transient cognition

The Prime-like runtime MAY own transient or reconstructible working state such as:

- active reasoning context;
- RLM/Python working memory;
- temporary scratchpads;
- planner queues;
- model-specific context windows;
- speculative task decomposition;
- transient subagent coordination state.

When such state becomes operationally important across failure or restart boundaries, the Brain MUST persist an appropriate durable representation through Vestrace rather than treating the transient runtime object as canonical truth.

### 3.4 One durable execution authority

`AgentRun` remains the single authoritative durable execution lifecycle.

Planners, schedulers, RLM workers, subagents, Face clients, Host Brokers, and Organs may each expose local operational states, but those states MUST NOT establish a competing durable execution truth.

A scheduler may decide that a run should start. An Organ may report that a process is running. A Face may show progress. The authoritative execution lifecycle remains the Brain's `AgentRun` state and evidence.

## 4. Face

### 4.1 Definition

The Face is the replaceable human-facing and host-facing boundary of the system.

A Face deployment may contain two logical components:

1. **Face Client** — desktop UI, CLI, mobile, web, or another interaction surface;
2. **Host Broker** — a local service that exposes explicitly permitted host-machine capabilities to the Brain.

A desktop application may package both components while preserving their logical separation.

```text
┌─────────────────────────────────────────────┐
│                    FACE                     │
│                                             │
│  Face Client                                │
│  chat · projects · tasks · approvals · UI  │
│                    │                        │
│                    ▼                        │
│  Host Broker                                │
│  files · processes · apps · shell · OS     │
└────────────────────┬────────────────────────┘
                     │
                  Host OS
```

### 4.2 Face Client responsibilities

The Face Client MAY provide:

- conversation and task interaction;
- project/workspace navigation;
- run and subagent observability;
- approvals and user confirmations;
- memory/claim/evidence inspection UX;
- artifact viewing and editing;
- settings and policy management surfaces;
- notifications;
- Organ status and live views;
- local UI preferences.

The Face Client MUST NOT own canonical agent cognition or authoritative run state.

Closing or replacing the Face Client MUST NOT erase the Brain's identity, memory, durable runs, schedules, or trust state.

### 4.3 Host Broker responsibilities

The Host Broker MAY expose typed host capabilities such as:

- filesystem read/write/watch/reveal;
- process list/start/stop/inspect;
- shell execution as a controlled escape hatch;
- application launch/open/focus;
- selected system information and settings;
- clipboard access;
- notifications;
- device access;
- host browser integration;
- explicitly registered local repositories and workspaces.

The preferred contract is typed host operations rather than unrestricted shell access.

### 4.4 Local host policy

The Host Broker maintains a local policy boundary under control of the host user or administrator.

Effective host authority is the intersection of independent constraints:

```text
EffectiveHostAuthority =
    BrainCapability
    ∩ BrainPolicyDecision
    ∩ FaceLocalPolicy
    ∩ HostOSPrincipalRights
```

The Face/Host Broker MAY deny or further restrict an operation already authorized by the Brain.

The Face/Host Broker MUST NOT widen a Brain capability, bypass Brain policy, mint higher authority, or convert local OS access into broader agent authority.

### 4.5 Face persistence modes

Two deployment modes are valid:

- **Interactive-only Host Broker** — host capabilities disappear when the Face application exits;
- **Persistent Host Broker service** — an explicitly enabled local background service remains available after UI exit.

Persistent host access MUST be opt-in, visible to the user, independently revocable, and bound to local policy.

### 4.6 Multiple Faces and hosts

One Brain MAY be connected to multiple registered Faces/Host Brokers, for example a workstation, laptop, and home server.

Host capabilities MUST be scoped to a stable host identity. Authorization for one host MUST NOT imply authorization for another.

Example scope:

```text
host = workstation-01
resource = C:\Projects\Vestrace
operation = filesystem.write
```

## 5. Organ

### 5.1 Definition

An Organ is a replaceable execution capability attached to the Brain.

An Organ is a logical role, not necessarily a one-to-one synonym for a Docker container. It may be implemented by:

- a Docker container;
- a sandbox or VM;
- a remote worker;
- a browser automation service;
- a GPU worker;
- a desktop-automation environment;
- another explicitly bounded execution endpoint.

### 5.2 Example Organs

Typical Organs include:

- **Coding Organ** — repository, compiler, tests, terminal, Git;
- **Browser Organ** — browser, DOM interaction, screenshots, web automation;
- **Desktop Organ** — GUI applications and desktop automation;
- **Compute Organ** — Python, numerical workloads, GPU execution;
- **High-Risk Organ** — aggressively isolated environment for untrusted or risky tasks.

### 5.3 Replaceability

An Organ SHOULD be disposable whenever practical.

The Brain MUST be able to tolerate Organ restart, replacement, or loss without losing canonical cognition.

Deleting and recreating an Organ MUST NOT erase or silently rewrite:

- Agent identity;
- Memory or Claims;
- AgentRun history;
- authorization history;
- effect history;
- trust state.

### 5.4 Organ authority

An Organ executes only within authority delegated by the Brain and further restricted by the Organ's own sandbox/local policy.

```text
EffectiveOrganAuthority =
    BrainCapability
    ∩ BrainPolicyDecision
    ∩ OrganLocalPolicy
    ∩ SandboxOrOSRights
```

An Organ MAY restrict authority more strongly than the Brain requested. It MUST NOT amplify delegated authority.

### 5.5 Computer leases

A Brain MAY allocate a temporary Organ or computer environment to a run or subagent through a bounded lease.

A lease SHOULD bind at least:

- Brain/agent identity;
- workspace;
- AgentRun or child run;
- Organ identity/type;
- capability scope;
- resource scope;
- lifetime/expiry;
- budgets;
- risk conditions;
- network policy where applicable.

The lease is not itself permission to exceed the underlying capability grant.

## 6. Models are not the Brain

Model providers are cognitive compute resources used by the Brain.

The system MAY route among cloud or local models without changing agent identity.

Changing GPT, Claude, Gemini, a local model, or another provider MUST NOT by itself create a new agent identity or discard persistent cognition.

Model output is untrusted reasoning material until it passes the relevant Vestrace evidence, authority, policy, and mutation boundaries.

## 7. System Interconnect (Nervous System)

### 7.1 Role

The System Interconnect transports communication among Brain, Faces, and Organs.

It MAY carry:

- commands;
- event streams;
- observations;
- artifacts and artifact references;
- capability tokens/references;
- approvals;
- heartbeats;
- leases;
- effect intents;
- effect receipts;
- reconciliation requests;
- telemetry and health signals.

The Interconnect MUST NOT become an independent state engine, policy authority, scheduler of record, or canonical event store.

### 7.2 Identity and authentication

Every connected Face, Host Broker, and Organ MUST have a distinguishable endpoint identity.

Connections MUST be authenticated before privileged operations are accepted.

Authorization MUST be evaluated against the intended operation and resource rather than inferred from the mere existence of an authenticated connection.

### 7.3 Reconnection

Reconnect MUST NOT imply replay of every previously requested operation.

After disconnect or crash, pending actions MUST be classified according to their durable state and external-effect semantics. Ambiguous external effects remain `UNKNOWN` until reconciled.

## 8. Brain → Organ / Host action protocol

The Brain MUST NOT treat unrestricted remote execution as the primary architectural contract.

The preferred flow is:

```text
Prime reasoning
      ↓
Execution / Host Action Request
      ↓
Vestrace capability + policy evaluation
      ↓
ExternalEffectIntent when externally consequential
      ↓
Face Host Broker or Organ
      ↓
local policy / sandbox enforcement
      ↓
execution
      ↓
Observation / Receipt / Artifact
      ↓
Vestrace durable state
```

A generic shell execution endpoint may exist as an escape hatch, but its use SHOULD carry higher risk classification and narrower scope than typed operations.

## 9. Observation is not cognition

Output from a Face Host Broker or Organ is an observation, artifact, or execution result. It is not automatically canonical memory or truth.

Example:

```text
Organ observes: "port=5432" in a file
        ↓
Observation / Artifact
        ↓
Prime interpretation
        ↓
Candidate Claim
        ↓
Evidence binding / Assessment
        ↓
Durable cognition when justified
```

The system MUST preserve the distinction between:

- raw observation;
- model interpretation;
- candidate claim;
- assessed claim;
- durable memory projection.

## 10. Subagents

A Prime-like runtime MAY recursively delegate tasks to subagents.

A durable subagent execution that matters beyond transient model reasoning SHOULD be represented as a child `AgentRun` or otherwise linked to the authoritative run tree.

Delegated authority MUST be attenuated:

```text
Authority(child) ⊆ Authority(parent)
```

A child MAY receive additional restrictions for tools, resources, operations, time, budgets, risk, network access, workspace, or execution environment.

A child MUST NOT inherit unrestricted host or Organ access merely because it was created by a more privileged parent.

## 11. External effects

For externally consequential operations, tool or transport success is not equivalent to confirmed world state.

The existing Vestrace external-effect contract remains authoritative:

```text
reason
  ↓
intent
  ↓
authorization
  ↓
dispatch
  ↓
receipt
  ↓
ACKNOWLEDGED / CONFIRMED / FAILED / UNKNOWN
  ↓
reconciliation when required
```

A Face or Organ MUST NOT silently retry an `UNKNOWN` effect in a way that can duplicate an irreversible or non-idempotent external action.

## 12. Secrets

Secrets are not ordinary memory.

Where possible, the Brain SHOULD pass a `SecretRef` or bounded credential request rather than durable plaintext secret material.

Resolution SHOULD occur as close as practical to the execution boundary, with secret values kept ephemeral and excluded from ordinary logs, prompts, memory, artifacts, and telemetry.

A Face Host Broker MAY own host-local credential integrations, but those integrations MUST remain subject to both Brain authorization and local host policy.

## 13. Failure and recovery

### 13.1 Face failure

Loss of a Face Client does not imply Brain failure.

If the Host Broker is interactive-only, host capabilities become unavailable until the Face reconnects. If a persistent Host Broker is enabled, its locally authorized capabilities may remain available.

### 13.2 Organ failure

Loss of an Organ invalidates its active connection and lease execution context.

The Brain determines whether affected work is:

- safe to resume;
- safe to retry;
- must reconcile;
- must abort;
- requires human intervention.

### 13.3 Brain worker failure

A Prime-like worker may be recreated from durable Brain state and reconstructible checkpoints.

Recovery MUST NOT infer successful external effects or restored trust solely from process restart.

### 13.4 Trust

Replacing an Organ, Face, model, or runtime process MUST NOT automatically raise trust.

Trust increases continue to require evidence and, where applicable, revalidation under the existing Vestrace trust model.

## 14. Local state ownership

The following local state is valid without becoming Brain canonical cognition:

### Face-local state

- window/layout state;
- theme;
- cached views;
- local notification preferences;
- connection settings;
- local host policy;
- OS-specific integration state.

### Organ-local state

- container filesystem caches;
- build caches;
- browser session caches;
- temporary process state;
- ephemeral scratch files;
- model/runtime caches.

Any local state that becomes required for durable reasoning, recovery, audit, or authorization MUST be projected into an explicit Brain-owned representation rather than relied upon implicitly.

## 15. Non-goals

This architecture does not require Vestrace to implement every product surface itself.

Vestrace SHOULD NOT become a monolithic collection of bespoke browser, desktop, office, messaging, or device implementations merely to own the entire stack.

The system is intentionally compositional:

```text
Hermes-like UX concepts
        ↓
Face
        ↓
Vestrace + Prime-like cognition
        ↓
Organs / Agent-Computer concepts
        ↓
World
```

Concrete products may replace any Face or Organ implementation while preserving the Brain contracts.

## 16. Normative laws

The Brain–Face–Organ architecture is governed by the following laws:

1. **Brain continuity:** persistent agent identity and cognition live in the Brain, not in a Face, Organ, model, or transient process.
2. **Single durable execution authority:** `AgentRun` remains the authoritative durable execution lifecycle.
3. **Replaceable periphery:** Faces and Organs are replaceable endpoints and MUST NOT own canonical cognition.
4. **No authority amplification:** a Face, Host Broker, Organ, subagent, or Interconnect component may only preserve or reduce delegated authority, never increase it.
5. **Host dual control:** host actions require both Brain authorization and local Host Broker/OS permission.
6. **Organ dual control:** Organ actions require Brain authorization and Organ sandbox/local permission.
7. **Observation before belief:** execution output becomes evidence/observation before it may become durable cognition.
8. **Effects remain governed:** transport/tool success is not automatically confirmed external outcome.
9. **Models are resources:** replacing a model does not replace agent identity.
10. **Recovery is evidence-based:** reconnect/restart/recreate does not silently imply success or restored trust.
11. **Interconnect is not authority:** communication infrastructure must not become a second policy engine, execution store, or state engine.
12. **Local state is explicit:** Face/Organ local state may exist, but durable system truth must have an explicit Brain-owned representation.

## 17. Relationship to the frozen v0.2 baseline

This model is intentionally compatible with the v0.2 architectural laws:

- authoritative state remains distinct from projections;
- the single execution/state boundary is preserved;
- capability delegation remains attenuating;
- external `UNKNOWN` outcomes still require reconciliation;
- recovery does not restore trust without evidence;
- secrets remain outside ordinary memory.

This document adds system-level placement and ownership boundaries for a future autonomous-agent product around that core.

It does not declare the Brain, Face, Host Broker, Prime-like runtime, or Organ implementation complete in the current repository.

## 18. Implementation sequencing

Implementation sequencing is intentionally not added to the frozen 36-PR v0.2→v1.0 roadmap by this document.

Before implementation, the architecture requires a separate transition plan covering at minimum:

- Brain runtime boundary and Prime-like integration;
- Face/Host Broker protocol;
- endpoint identity and registration;
- Organ leases and lifecycle;
- capability transport/enforcement;
- observation/artifact ingestion;
- reconnection and `UNKNOWN` recovery semantics;
- local policy and host permission UX;
- multi-host and multi-Organ behavior;
- conformance and fault-injection scenarios.
