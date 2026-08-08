# Vestrace Persistent Cognition Architecture Design

**Date:** 2026-08-08  
**Status:** Approved architecture direction; implementation specifications follow separately  
**Target:** Vestrace v0.2 through v1.0

## 1. Decision

Vestrace is a **memory-first platform for persistent cognition shared across agents and executions**.

The product is not defined primarily as an agent framework, workflow engine, vector memory service, or temporal knowledge graph. Its core responsibility is to maintain a durable, explainable, temporal, policy-governed model of what a workspace knows, why it knows it, how that knowledge changes, and how that knowledge influences execution.

The canonical product statement is:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

The canonical architectural statement is:

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

State Engine remains essential, but its role is reliability and durable causality rather than product identity.

## 2. Architectural shape

Vestrace will evolve as a **cognition-centered modular monolith in Rust** with strict bounded contexts and PostgreSQL as the authoritative store.

Vestrace will not be split into microservices during the v0.2 foundation work. Internal interfaces must nevertheless be strong enough that specialized services or projections can be separated later without changing domain semantics.

```text
┌──────────────────────────────────────────────────────────────┐
│                         VESTRACE                             │
│                                                              │
│  ┌──────────────── PERSISTENT COGNITION ─────────────────┐  │
│  │ Evidence                                              │  │
│  │    ↓                                                  │  │
│  │ Claims                                                │  │
│  │    ↓                                                  │  │
│  │ Knowledge Slots                                       │  │
│  │    ↓                                                  │  │
│  │ Reconciliation                                        │  │
│  │    ↓                                                  │  │
│  │ Temporal Knowledge                                    │  │
│  │    ↓                                                  │  │
│  │ Consolidation                                         │  │
│  │    ↓                                                  │  │
│  │ Retrieval Planner                                     │  │
│  │    ↓                                                  │  │
│  │ Context Compiler → ContextPack                         │  │
│  └───────────────────────┬───────────────────────────────┘  │
│                          │                                   │
│        ┌─────────────────┼─────────────────┐                 │
│        ▼                 ▼                 ▼                 │
│   GOVERNANCE         STATE ENGINE      PROJECTIONS           │
│   capabilities       runs              FTS                    │
│   policies           events            vectors                │
│   approvals          checkpoints       graph                  │
│   sensitivity        recovery          analytics              │
│   audit              outcomes                                 │
│        │                 │                 │                  │
│        └─────────────────┼─────────────────┘                  │
│                          ▼                                   │
│                     AGENT RUNTIME                            │
│       Agents · Skills · Workflows · Models · Tools           │
│                          │                                   │
│              MCP · A2A · HTTP · CLI · UI                    │
└──────────────────────────────────────────────────────────────┘
```

## 3. Architectural laws

The following rules are invariants of the target architecture.

### 3.1 Cognition is workspace state, not agent-owned memory

Persistent cognition belongs to a workspace and may be scoped to projects, repositories, users, agents, teams, executions, or other domain scopes.

Agents and executions are producers and consumers of cognition. They are not the root owner of all knowledge.

### 3.2 Evidence is not knowledge

Incoming events, messages, artifacts, tool results, execution results, model outputs, imports, and human statements are evidence or sources of evidence.

They do not become canonical knowledge merely because they were observed.

### 3.3 Claims are not automatically knowledge

A Claim represents an assertion derived from, imported from, or directly authored against evidence.

A Claim may be accepted, corroborated, refined, superseded, contested, rejected, or left unresolved. The system must preserve the distinction between what was asserted and what it currently accepts as working knowledge.

### 3.4 No LLM has direct canonical-write authority

LLMs, extractors, agents, consolidators, and verifiers may propose changes to cognition. They do not directly mutate canonical knowledge.

Canonical changes pass through a mutation protocol, validation, reconciliation, policy evaluation, and an atomic commit.

### 3.5 Time is part of knowledge semantics

Vestrace distinguishes:

- **valid time** — when a fact or state applies in the modeled world;
- **system time** — when Vestrace recorded or accepted that knowledge.

Temporal semantics belong to canonical cognition, not merely to retrieval ranking.

### 3.6 PostgreSQL is authoritative; search representations are projections

Canonical cognition, authoritative execution state, governance records, provenance, and audit remain in PostgreSQL.

FTS, vector, graph, analytics, and cache representations are rebuildable projections. Losing or replacing a projection must not destroy cognition.

### 3.7 Truth, importance, freshness, and utility are different axes

A knowledge item can be true but unimportant, important but stale, frequently useful but uncertain, or reliable but rarely relevant.

The system must not collapse these concepts into a single score.

### 3.8 Context is compiled, not dumped

Agent consumers receive immutable `ContextPack` snapshots produced by an intent-aware retrieval planner and context compiler.

A ContextPack is a traceable representation of what a consumer was allowed and expected to know for a particular reasoning or execution step.

### 3.9 State Engine records causality; Memory Engine records cognition

State Engine answers:

- what happened;
- in what order;
- under which command;
- with which outcome.

Memory Engine answers:

- what is known;
- why it is known;
- when it was valid;
- how certain it is;
- what conflicts with it;
- how it changed.

Neither bounded context subsumes the other.

## 4. Top-level bounded contexts

### 4.1 Persistent Cognition

The primary product domain.

Responsibilities:

- evidence identity and provenance;
- claims and derivations;
- semantic knowledge identity;
- bitemporal state;
- reconciliation and conflict handling;
- consolidation;
- confidence assessment;
- memory/cognitive objects;
- retrieval planning;
- context compilation;
- cognition health and explanation.

### 4.2 State Engine

The durable execution and causality substrate.

Responsibilities:

- run lifecycle;
- event streams;
- optimistic concurrency;
- idempotency;
- steps, waits, retries, and outcomes;
- replay and deterministic reduction;
- checkpoints and recovery;
- durable links from execution steps to immutable ContextPacks.

### 4.3 Governance

The authority and trust boundary.

Responsibilities:

- capability evaluation;
- scope- and resource-aware policies;
- sensitivity;
- risk classification;
- approval workflows;
- audit;
- controlled data movement;
- permissions for cognition read, proposal, resolution, consolidation, verification, and purge.

### 4.4 Projection Layer

Rebuildable representations optimized for access patterns.

Initial projection classes:

- exact/structured indexes;
- PostgreSQL FTS and trigram search;
- vector search;
- graph/traversal projection;
- timeline projection where useful;
- analytics and evaluation projections.

Every projection records a version and source watermark so lag or corruption can be diagnosed.

### 4.5 Agent Runtime

A reference consumer and producer of persistent cognition.

Responsibilities:

- agents;
- skills;
- workflows;
- models and routing;
- tools;
- execution composition.

The Agent Runtime must depend on cognition through application ports rather than database details.

### 4.6 Interfaces

Adapters over application ports:

- HTTP;
- CLI;
- MCP;
- A2A;
- future SDKs;
- Cognition Debugger UI.

Protocol-specific types must not leak into cognition or run domain models.

## 5. Canonical cognition model

### 5.1 Evidence

Evidence is immutable source material or a stable reference to source material.

Conceptual shape:

```text
Evidence
├── id
├── workspace_id
├── source_ref
├── observed_at?
├── recorded_at
├── content_hash?
└── metadata
```

The first required source type for v0.2 is an authoritative Vestrace Event. The model must permit later evidence sources such as artifact revisions, run events, tool invocations, external resources, and imported records without changing Claim semantics.

### 5.2 Claim

A Claim is an assertion about a semantic slot.

```text
Claim
├── id
├── workspace_id
├── slot_id
├── value
├── value_schema
├── canonical_text
├── asserted_valid_from?
├── asserted_valid_until?
├── observed_at?
├── confidence
├── importance
├── derivation_id?
└── recorded_at
```

Claim values use schema-qualified structured values. JSONB may be used as storage representation, but a value is not an untyped arbitrary map.

### 5.3 ClaimEvidence

Claim provenance is revision-/claim-specific rather than attached only to a broad memory object.

```text
ClaimEvidence
├── claim_id
├── evidence_id
├── role
├── support_strength
├── locator?
└── created_at
```

Evidence role and derivation are independent concepts. Direct source evidence may still produce a derived Claim through an LLM extractor or rule.

### 5.4 KnowledgeSlot

A KnowledgeSlot provides stable semantic identity for a question or property.

```text
KnowledgeSlot
├── id
├── workspace_id
├── scope
├── subject
├── predicate
├── cardinality
├── value_schema
├── resolution_policy
├── temporal_policy
└── created_at
```

The semantic key is conceptually `(scope, subject, predicate)` and is unique within a workspace for a given slot definition.

Examples:

```text
project:vestrace / project:vestrace / database.primary
project:vestrace / project:vestrace / core.language
user:ivan       / user:ivan       / preference.response_language
service:api     / service:api     / deployment.status
```

Slots prevent two competing values for the same semantic property from being treated as unrelated memories.

### 5.5 SlotResolution

A SlotResolution records when and why a Claim is accepted as working knowledge for a slot.

```text
SlotResolution
├── id
├── workspace_id
├── slot_id
├── claim_id
├── disposition
├── valid_from?
├── valid_until?
├── recorded_from
├── recorded_until?
└── resolution_reason
```

`Accepted` and `Contested` are required initial dispositions. Rejected claims remain Claims but do not become accepted knowledge.

For single-cardinality slots, overlapping accepted valid-time intervals are prohibited within the same system-time view.

### 5.6 Cognitive objects

Not all durable cognition should be reduced to atomic subject/predicate/value claims.

Vestrace retains typed cognitive objects such as:

- Decision;
- Procedure;
- Task;
- Outcome;
- Summary;
- Preference profile;
- Constraint sets.

These objects may reference Claims, Evidence, KnowledgeSlots, executions, and other objects.

The existing `Memory` and `MemoryRevision` model evolves toward this higher-level cognitive-object role and compatibility facade rather than remaining the sole unit of knowledge truth.

## 6. Bitemporal semantics

All knowledge APIs use half-open validity intervals `[from, until)`.

Vestrace distinguishes two questions:

1. **What was valid at time T?**
2. **What did Vestrace believe about time T as of system time K?**

This enables knowledge-time travel and execution forensics.

Required query semantics include:

- current knowledge;
- valid-at time;
- known-at/system-time view;
- valid-at plus known-at;
- timeline/history.

Example:

```text
Aug 1: system accepts database = SQLite
Aug 5: real migration to PostgreSQL occurs
Aug 8: Vestrace learns that migration happened Aug 5
```

After reconciliation, current knowledge says PostgreSQL is valid from Aug 5, but Vestrace can still reconstruct that on Aug 7 the system believed SQLite remained current.

## 7. Safe cognition mutation protocol

Every autonomous cognition change uses `MemoryMutationSet` as the first-class proposal aggregate.

```text
MemoryMutationSet
├── id
├── workspace_id
├── proposer
├── reason
├── input_evidence
├── proposed_claims
├── proposed_resolutions
├── proposed_invalidations
├── proposed_relations
├── expected_versions
├── policy_context
└── status
```

Lifecycle:

```text
Draft
  ↓
Validated
  ↓
Reconciled
  ↓
Approved
  ↓
Committed
```

Terminal or alternate outcomes include:

```text
Rejected
Conflict
NeedsReview
Expired
```

A proposal is not canonical cognition until the commit succeeds.

## 8. Reconciliation Engine

Reconciliation is a core domain service, not a retrieval heuristic.

For each proposed Claim, it:

1. identifies or creates the target KnowledgeSlot;
2. loads current and historical accepted knowledge for that slot;
3. loads competing Claims and Evidence;
4. evaluates temporal relationships;
5. evaluates source and derivation quality;
6. detects duplicate, support, contradiction, refinement, and supersession relationships;
7. applies slot resolution policy;
8. applies governance policy and review requirements;
9. produces an explainable ReconciliationDecision.

Required decisions:

```text
Accept
Corroborate
Refine
Supersede
Coexist
Contest
Reject
NeedsReview
```

Reconciliation decisions themselves are durable and explainable.

## 9. Confidence model

A scalar confidence score remains useful for APIs and ranking, but canonical confidence becomes an assessment.

```text
ConfidenceAssessment
├── source_reliability
├── extraction_certainty
├── corroboration
├── contradiction_penalty
├── temporal_consistency
├── verification_level
└── effective
```

The target model maintains separate measures for:

- truth confidence;
- importance;
- freshness;
- utility.

Execution success may update utility but must not automatically increase truth confidence.

## 10. Authoritative storage and transaction boundary

PostgreSQL remains authoritative.

The target write boundary is a transaction-scoped `CognitionUnitOfWork` containing the repositories needed for one authoritative cognition commit:

```text
CognitionUnitOfWork
├── evidence
├── claims
├── slots
├── resolutions
├── mutations
├── provenance
├── cognitive_objects
├── outbox
├── idempotency
└── commit()
```

No authoritative cognition write path may consist of independent autocommitted repository calls that rely on a later write to satisfy a constraint.

The first prerequisite implementation before M0.2-A is therefore **Memory Write Integrity**: create/revise/source/policy/outbox/idempotency changes must share one database transaction.

## 11. Immutable, temporal, and rebuildable layers

### Immutable or append-oriented

- Evidence;
- Claims;
- committed MutationSets;
- ReconciliationDecisions;
- ContextPacks;
- execution events;
- audit events.

### Temporal state

- SlotResolutions;
- current accepted knowledge;
- cognitive-object lifecycle state;
- procedure/decision state.

### Rebuildable projections

- FTS;
- vector indexes;
- graph indexes/projections;
- analytics;
- caches.

## 12. Projection model

Each projection has explicit operational metadata:

```text
projection_kind
projection_version
source_watermark
build_status
last_rebuilt_at
```

Projection failures degrade capabilities but do not change canonical cognition.

Examples:

- changing embedding model rebuilds only vector projection;
- changing graph backend rebuilds only graph projection;
- losing a search projection does not lose Evidence, Claims, Slots, or Resolutions.

## 13. Retrieval Planner

Retrieval is an intent-aware query planner rather than a single semantic search function.

Conceptual pipeline:

```text
CognitionQuery
     ↓
Intent
     ↓
Authorization
     ↓
Retrieval Planner
 ├── slot/exact lookup
 ├── structured query
 ├── temporal query
 ├── graph traversal
 ├── FTS
 └── vector similarity
       ↓
Fusion / Rerank
       ↓
Context Compiler
       ↓
ContextPack
```

Representative recipes:

### CurrentState

Prefer slot/exact and temporal-current queries. Semantic search is fallback, not authority.

### Timeline

Prefer temporal history and evidence/event ordering.

### DecisionRecall

Prefer typed Decision objects, rationale, supporting knowledge, rejected alternatives, and evidence.

### ProcedureLookup

Prefer typed procedures, applicable constraints, historical outcomes, semantic similarity, and utility.

### ErrorRecovery

Prefer related failures, outcomes, stale or contested knowledge, prior attempts, and applicable procedures.

### Exploration

Use broad FTS/vector/graph retrieval with diversity-aware reranking.

## 14. ContextPack as immutable cognition snapshot

`ContextPack` becomes a first-class immutable snapshot of compiled cognition for a consumer and task.

Target contents:

```text
ContextPack
├── id
├── workspace_id
├── consumer
├── intent
├── query
├── valid_at
├── known_at
├── scope_set
├── policy_snapshot
├── knowledge_items
├── cognitive_objects
├── conflicts
├── evidence_refs
├── token_budget
├── used_tokens
├── retrieval_trace
├── projection_versions
└── created_at
```

State Engine records the ContextPack used by a reasoning or execution step. This enables reconstruction of what an agent was allowed to know and what cognition it actually received at that moment.

## 15. State Engine integration

State Engine and Persistent Cognition form a durable loop:

```text
State Engine ── execution events ──► Evidence
Memory Engine ── ContextPack ──────► State Engine / Agent Runtime
State Engine ── outcomes ──────────► Evidence
Memory Engine ── reconcile ────────► Knowledge
```

State Engine remains event-sourced for run lifecycle and deterministic recovery.

Memory Engine is not required to event-source every knowledge aggregate. It uses relational canonical state plus immutable/append-oriented Evidence, Claims, mutations, resolution history, and audit.

## 16. Execution feedback

Vestrace must eventually connect cognition to outcomes without confusing usefulness with truth.

A future first-class execution feedback record links:

```text
ExecutionOutcome
├── context_pack_id
├── actions
├── outcome
├── evaluation
└── observed_effects
```

Derived operational signals include:

- knowledge retrieval/use count;
- whether a memory item materially contributed to an execution;
- procedure success/failure history;
- last useful time;
- utility score;
- stale-knowledge failure association.

The resulting closed loop is:

```text
remember → reason → act → observe → evaluate → update cognition
```

## 17. Governance model

Governance operates inside cognition read and write paths, not only at HTTP middleware.

Read path:

```text
Principal
   ↓
CognitionQuery
   ↓
Capability / Policy / Sensitivity
   ↓
AuthorizedQuery
   ↓
Retrieval Planner
```

Write path:

```text
MutationSet
   ↓
Capability
   ↓
Policy
   ↓
Risk
   ↓
Approval if required
   ↓
Commit
```

Target capability vocabulary distinguishes operations such as:

```text
evidence.read
claim.read
claim.propose
knowledge.read
knowledge.resolve
procedure.propose
procedure.activate
cognition.consolidate
cognition.verify
evidence.purge
```

Roles remain templates. Effective authority is represented by attenuable capability grants/tokens. A subagent receives only an explicitly delegated subset of parent authority with additional restrictions.

## 18. Scope and visibility

Cognition may be scoped to entities such as:

```text
workspace
team
project
repository
deployment
user
agent
conversation
execution
```

Scope expresses semantic ownership/context. It does not by itself grant access.

**Ownership is not visibility.**

A project-owned Claim may be readable by one agent and invisible to another depending on capabilities, policy, sensitivity, and current purpose.

## 19. Failure semantics

Cognition queries do not collapse all absence into `None`.

The application model must be able to represent at least:

```text
Known
Contested
Unknown
InsufficientEvidence
PolicyRestricted
ProjectionDegraded
```

Agents must not receive fabricated certainty when the underlying cognition is unresolved or inaccessible.

## 20. Cognition health and observability

A dedicated cognition-health surface is part of the target architecture.

Health checks include:

- orphan Claims;
- unsupported accepted knowledge;
- stale knowledge;
- unresolved conflicts;
- low-confidence accepted knowledge;
- malformed provenance;
- projection lag;
- projection corruption/version mismatch;
- unreachable Evidence references.

Expected operational interfaces include commands such as:

```text
vestrace cognition doctor
vestrace cognition verify
vestrace cognition rebuild
vestrace cognition explain
vestrace cognition diff
```

These names describe the intended capability, not a frozen CLI contract for v0.2.

## 21. Compatibility with the current v0.1 model

The architecture must evolve without a big-bang rewrite.

The existing `Memory`, `MemoryRevision`, `StructuredMemory`, relations, provenance, retrieval intents, ContextPack, run state engine, and PostgreSQL infrastructure remain useful.

Migration direction:

```text
Current Memory API
      │
      ▼
Compatibility / cognitive-object facade
      │
      ▼
Evidence → Claim → KnowledgeSlot → SlotResolution
```

Rules:

1. Existing public behavior is preserved where possible during v0.2 foundation work.
2. New canonical cognition types are added before old types are renamed or removed.
3. `MemorySource(memory_id, event_id)` evolves toward claim-specific provenance.
4. `Memory` evolves toward typed cognitive objects and compatibility representation.
5. `MemoryRevision` remains useful for non-atomic objects and historical object versions.
6. `RetrievalIntent` and `ContextPack` are retained and strengthened rather than replaced.
7. Existing migrations remain immutable; new behavior is introduced through forward-only migrations.

## 22. Delivery sequence

The architecture is implemented through separate reviewable specifications and vertical slices.

### P0 — Memory Write Integrity

Purpose: establish correct transaction boundaries before extending semantics.

Required outcome:

- one transaction for memory/revision/provenance/policy/outbox/idempotency changes;
- no active canonical object can reference not-yet-persisted provenance or revisions;
- existing write policy inputs are either enforced or explicitly removed from the path.

### M0.2-A — Temporal Claims

Purpose: introduce the canonical cognition foundation.

Required capabilities:

- Evidence;
- Claim;
- ClaimEvidence;
- KnowledgeSlot;
- SlotResolution;
- bitemporal query semantics;
- current, valid-at, known-at, timeline queries;
- provenance explanation;
- legacy Memory compatibility.

Canonical acceptance scenario:

```text
SQLite is accepted as current database.
A later Evidence item states that migration to PostgreSQL completed on Aug 5.
The system learns this on Aug 8.

Current                      → PostgreSQL
ValidAt(Aug 3)               → SQLite
ValidAt(Aug 6)               → PostgreSQL
ValidAt(Aug 6), KnownAt(Aug7)→ SQLite
ValidAt(Aug 6), KnownAt(Aug8)→ PostgreSQL
Explain(PostgreSQL)          → Claim → Evidence → source Event
```

### M0.2-B — Safe Mutation and Reconciliation

Purpose: ensure autonomous memory changes are policy-governed and explainable.

Required capabilities:

- MemoryMutationSet;
- reconciliation decisions;
- conflict detection;
- duplicate/corroboration/refinement/supersession logic;
- review path;
- atomic commit;
- mutation audit.

### M0.2-C — Retrieval 2.0 and Cognition Bench

Purpose: turn retrieval into an intent-aware planner and measure cognition quality.

Required capabilities:

- retrieval recipes;
- structured/exact/temporal/FTS/vector/graph channels;
- fusion and reranking;
- ContextPack enrichment;
- benchmark harness.

Benchmark categories include:

- current-state recall;
- knowledge update;
- temporal reasoning;
- contradiction resolution;
- source attribution;
- abstention;
- cross-session continuity;
- cross-agent continuity;
- decision recall;
- procedure reuse;
- error recovery;
- context efficiency.

### Post-M0.2 — Execution Feedback

Purpose: connect cognition to outcome quality.

### Post-M0.2 — Cognition Debugger

Purpose: make temporal knowledge, provenance, conflicts, and execution influence inspectable.

### Post-M0.2 — Fine-grained Governance

Purpose: replace coarse memory permissions with cognition-aware capabilities and attenuated delegation.

### Post-M0.2 — Interoperability conformance

Purpose: bring MCP and A2A adapters to the current specifications without changing core cognition semantics.

## 23. Non-goals for the cognition foundation

The following are explicitly not required to prove the core architecture:

- microservices;
- Kafka or another external event bus;
- dedicated graph database;
- custom vector database;
- own embedding model/runtime;
- own foundation model runtime;
- new workflow language;
- marketplace;
- broad connector expansion;
- rich multi-agent orchestration;
- distributed consensus.

They may be added later only when justified by measured product or operational needs.

## 24. Success criteria for the architecture

The architecture direction is considered successfully realized when Vestrace can demonstrate all of the following properties:

### Continuity

Knowledge survives agent, model, process, session, and execution changes and can be safely reused by authorized future consumers.

### Correctness

The system can distinguish current, historical, superseded, contested, uncertain, expired, and derived cognition.

### Explainability

For accepted knowledge, Vestrace can explain the Claim, Evidence, derivation, reconciliation decision, confidence, and relevant temporal state.

### Reproducibility

For a historical execution step, Vestrace can reconstruct the immutable ContextPack and therefore the cognition supplied to the agent at that time.

### Safe evolution

Autonomous components propose cognition changes through governed mutations rather than directly overwriting canonical state.

### Measurability

Cognition quality is benchmarked for correctness, temporal reasoning, provenance, continuity, abstention, latency, and context/token efficiency.

### Replaceability

Agents, models, vector representations, graph projections, and protocol adapters can change without discarding canonical cognition.

## 25. Product consequence

This architecture intentionally positions Vestrace away from three crowded categories:

- “memory API for chatbots”;
- “temporal graph database for agents”;
- “another agent framework”.

The target category is:

> **Persistent cognition infrastructure for reliable agent systems.**

The differentiating product promise is:

> Vestrace maintains what an agent system knows, why it knows it, what it knew at any point in time, and how that cognition affected execution.

This specification is the macroarchitecture contract. Detailed implementation contracts for Memory Write Integrity, M0.2-A, M0.2-B, M0.2-C, execution feedback, governance, and the Cognition Debugger are authored and reviewed independently against this document.
