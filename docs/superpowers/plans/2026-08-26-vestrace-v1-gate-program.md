# Vestrace v1.0 Gate Program

**Goal:** Deliver the approved local single-workspace Vestrace v1.0 product through a finite, dependency-ordered set of reviewable implementation packages without weakening the release contract or mixing unrelated dirty work.

**Architecture:** The canonical `Run` engine remains the sole execution authority. Provider, AG-UI, and A2A adapters translate at the boundary and reuse the shared authorization, external-effect, evidence, retention, and recovery authorities. PostgreSQL owns canonical product state; the local host supervisor owns the independent safety domains named by the spec; the React console is a truthful projection and command surface over typed HTTP contracts.

**Tech Stack:** Rust 1.85 / edition 2024, Axum 0.8, SQLx 0.8, PostgreSQL, React 18, TypeScript 5.6, Vite 5, Docker Compose, Node 22 test runner, pinned AG-UI 0.0.58, pinned A2A v1.0.1 and official Rust SDK crates.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md`, approved frozen SHA-256 `B31B5BE62504E1A65F411CD31B446CD41B3D032B7282F1AA42907706EF9C1473`.

**External corpus impact:** `compatibility seam` against registered corpus digest `762C25F3781F1BA30783E218DB64AA4DC9536ADD490F61255056BFE8635D8594`. P01 persists the exact twelve-file repo-local manifest and pins it into the protocol baseline. The narrow v1 seam is the frozen spec's separation of canonical Run/effect truth from protocol projections and its reconstructable model-request evidence contract. The post-v1 RFC/roadmap and generalized runtime concepts remain `defer post-v1`; derived JSON/HTML companions remain non-authoritative; missing referenced documents are recorded rather than inferred.

## Program Constraints

- This index does not authorize release or permit an implementation package to skip its own detailed RED-to-GREEN plan and independent review.
- `PLAN.md` is unrelated approved work and remains untouched. Preserve all existing modified and untracked files unless a later package explicitly names one as in scope.
- Do not commit, push, deploy, request secrets, or manufacture qualification evidence.
- A build, typecheck, fixture, or component test is scoped development evidence only. v1.0 requires the fresh supported-environment evidence in package P12.
- Before its first write or dependency install, each package records its source revision and an untracked-aware content digest of the existing dirty-file set. Final evidence compares every out-of-scope baseline path byte-for-byte. P12 alone requires a clean release checkout and reproducible source/image digests.
- Existing domain authorities are reused. No protocol adapter, provider adapter, scheduler, console route, or restore path may create a parallel execution, authorization, idempotency, effect, recovery, or evidence lifecycle.
- Database behavior is proved against PostgreSQL, including raw-SQL refusal where the spec assigns the invariant to the database. In-memory doubles cannot qualify those invariants.
- Ambiguous external outcomes stay explicit. Absence alone never proves death, and work already in `Dispatching` never auto-retries.
- The external corpus at `E:\Junk\AI\vestrace-docss` remains an input corpus, not a second source of product authority. Its accepted 12-file manifest digest is `762C25F3781F1BA30783E218DB64AA4DC9536ADD490F61255056BFE8635D8594`; after P01, later plans consume the repo-local manifest and do not require that machine-local path.

## Fixed Package Count

The v1.0 program contains exactly twelve implementation packages. A package may contain multiple TDD tasks, but implementation must not invent additional architecture packages without an operator-approved program amendment.

| Package | Gate | Scope | Required predecessor | Exit evidence |
| --- | --- | --- | --- | --- |
| P01 | G0 | Preflight dirty baseline, repo-local external-corpus registration, protocol lock, AG-UI runtime-schema extraction, A2A SDK/spec pins, OpenAI-compatible q1 manifest and fixtures | approved spec | deterministic manifest/lock verification, exact lockfiles/provenance, focused conformance tests |
| P02 | G0 | Universal default-deny route inventory, atomic mutation-plus-Audit transaction authority, installation mutation/fingerprint authority, content/credential key-intent lifecycle, material vault, guards, erasure primitives | P01 | route-inventory/raw-negative and Audit-rollback tests plus PostgreSQL mutation/refusal/crash-boundary evidence |
| P03 | G0 | Connection/Model revisions, auth-binding XOR, qualification bindings, provider transport, ModelRequestEvidence, shared external-effect admission/dispatch/recovery; every new route remains inventory-covered and every governed provider mutation uses P02's atomic Audit authority | P02 | loopback semantic-request observation, route-denial/Audit rollback, PostgreSQL concurrency and recovery faults, no-auth and credential branches |
| P04 | G0 | Embedding jobs/spaces/corpus/generations, transition recipes/batches/carry/barriers, retrieval generation fences | P03 | component, PostgreSQL, worker-restart, and fault evidence with no mixed-space or duplicate dispatch |
| P05 | G0 | Backup/WAL archive, restore/activation supervisor, least-privilege Compose roles, installer/readiness, G0 evidence aggregation | P02 and P04 | fresh full route inventory including access tokens, unknown-route/raw-negative denial, atomic mutation/Audit rollback, real base/WAL archive and restore faults, role checks, Compose readiness, aggregate G0 gate result |
| P06 | G1 | Real Connections, Models, Agents, Home, Runs, runtime Settings, LM Studio and remote OpenAI-compatible execution | P05 | browser plus restart evidence for both auth branches and real model-backed Runs |
| P07 | G2 | Interaction/thread/message/state/context kernel, artifacts, retention, protocol bindings and replay low-water evidence | P06 | PostgreSQL, browser, retention/race, and official-client preparation evidence |
| P08 | G3 | Single-endpoint full pinned AG-UI server and console client; remove pre-v1 split routes | P07 | official `HttpAgent`, browser, restart, replay/expiry, interrupt/tool/multimedia evidence |
| P09 | G4 | A2A server, signed Agent Card, six core JSON-RPC/SSE operations, canonical Run projection | P07 | pinned contract/TCK, restart, idempotency, retention, cancellation, and unsupported-operation evidence |
| P10 | G5 | A2A Connection qualification and outbound A2A workflow/agent Step | P09 | external sample-server interop, SSRF/auth, streaming, cancellation, and ambiguity reconciliation evidence |
| P11 | G6 | Workflows, Triggers, Evaluations, Audit, complete Settings, and all eleven menu workflows | P08 and P10 | full browser/restart suite with no enabled stub or inert configuration |
| P12 | G7 | Exact supported-environment qualification and release evidence | P11 | clean checkout, recorded source/image/config/model/protocol digests, real LM Studio and independent Bearer API, all named fault/protocol/browser/accessibility gates |

## Package Planning and Review Rule

1. Write the detailed plan only after every predecessor package has accepted interfaces and persisted evidence.
2. Map every requirement to an exact file, interface, RED test, GREEN command, and scoped diff check.
3. Include an `External corpus impact` row using exactly `adopt now`, `compatibility seam`, `defer post-v1`, or `reject`, with affected manifest entries and rationale.
4. Have an independent reviewer trace the detailed plan against the frozen spec and live repository.
5. Correct all material findings and repeat review.
6. Execute with the same persistent cdx builder; use independent reviewers at task/package gates.
7. Mark a package complete only from fresh observed evidence. Record unavailable Docker, PostgreSQL, LM Studio, remote API, browser, or destructive-fault evidence as blocked, never as pass.

## Dependency Flow

```text
P01 protocol lock
  -> P02 security/material foundation
     -> P03 provider execution foundation
        -> P04 embeddings/retrieval foundation
     -> P05 restore/Compose/G0 aggregation <- P04
        -> P06 real configuration and agents
           -> P07 interaction/artifact kernel
              -> P08 AG-UI
              -> P09 A2A server -> P10 A2A client
                 P08 + P10 -> P11 remaining menu closure
                              -> P12 exact-environment qualification
```

## Current Authorization Boundary

- P01 has a detailed companion plan and is the only implementation package ready for operator approval.
- P02-P12 are fixed program scope, sequencing, and exit criteria. Their implementation is not authorized by this index alone.
- Completing P01 proves only a reproducible protocol baseline. It does not claim AG-UI, A2A, provider execution, any menu workflow, or v1.0 is implemented.
