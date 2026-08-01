# Vestrace H11B Memory Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, install dependencies, generate application code, modify schemas, update CI or implement production behavior until the user explicitly ends the documentation-only phase.

**Goal:** Add the complete Memory Console to the H11B web application: searchable library, immutable detail and provenance, candidate review, correction and supersession, conflict resolution, lifecycle controls and cross-workspace sharing views.

**Architecture:** The console is a projection-and-command client over the public H11 TypeScript SDK. Memory Core and Cross-Workspace Memory Sharing remain the only owners of Memory lifecycle and authorization. The UI maps server projections into Library, Detail and Inspector regions and maps each mutation into one typed, idempotent, version-checked command. It never infers a successful transition from local state.

**Tech Stack:** Existing H11B React/Vite TypeScript application; H11 generated TypeScript SDK and public schemas; React Router; existing Structured Depth components; Testing Library; Vitest; Playwright; axe-core.

## Global Constraints

- `docs/superpowers/specs/2026-08-01-vestrace-v0.2-full-product-release-design.md` is normative.
- This addendum starts only after H11, all six State Engine follow-up extensions and H11A are merged, and after H11B Tasks 1-4 provide the shell, route model, public clients and design system.
- Memory Core owns Memory identity, revisions, provenance, relations, candidates, conflicts, correction, supersession, retention and purge.
- Cross-Workspace Memory Sharing owns grants, acceptance, mounts, source reads, quarantine and purge acknowledgement.
- The console imports only public H11 SDK entry points. Direct database, internal service, repository or private adapter access is prohibited.
- Search, result display, Context Pack use and mounted reads do not create local Memory.
- Candidate approval, correction, supersession, conflict resolution, soft deletion, hard purge and shared-memory import are distinct commands and distinct confirmations.
- A correction creates a new immutable revision. The client never edits committed content in place.
- Hard purge is protected, capability-gated and consequence-specific; soft delete is never presented as cryptographic erasure.
- Mounted content is visually and semantically foreign and untrusted. It never acquires local trust, execution authority or a local Memory identity through display or model use.
- Cross-workspace exact import requires its own permission and creates a new target-owned object through the authoritative API.
- Every effectful command carries an idempotency key and the latest expected version or revision token required by the public contract.
- `OutcomeUnknown`, stale version, source unavailable, source erased and downstream cleanup pending are separate recoverable states.
- Browser credentials and secret-bearing payloads remain memory-only and never enter URLs, browser storage, logs, telemetry or test snapshots.
- Desktop uses Library + Detail + Inspector; tablet uses a detail page with Inspector drawer; mobile uses separate list, detail and inspector routes.
- Keyboard operation, visible focus, live-region discipline, reduced motion and WCAG 2.2 AA are release requirements.
- This addendum introduces no aggregate, persistence schema, migration, private endpoint or second memory runtime.
- Future implementation branch: `feat/h11b-memory-console`, created from the then-current merged v0.2 base.

---

## Locked file additions

```text
apps/console/src/memory/
  types.ts
  routeState.ts
  memoryClient.ts
  memoryProjection.ts
  MemoryPage.tsx
  MemoryLibrary.tsx
  MemorySearch.tsx
  MemoryFilters.tsx
  MemoryResultRow.tsx
  MemoryDetail.tsx
  MemoryContentView.tsx
  MemoryStatusActions.tsx
  MemoryInspector.tsx
  MemoryProvenance.tsx
  MemoryRevisions.tsx
  MemoryRelations.tsx
  MemoryConflicts.tsx
  MemoryUsage.tsx
  MemoryAudit.tsx
  CandidateReviewForm.tsx
  CorrectionForm.tsx
  SupersedeDialog.tsx
  ConflictResolutionForm.tsx
  DeleteMemoryDialog.tsx
  HardPurgeDialog.tsx
  SharedMemoryPanel.tsx
  MemoryImportProposal.tsx
apps/console/src/app/routes.ts
apps/console/src/app/router.tsx
apps/console/src/api/queryKeys.ts
apps/console/src/shell/PrimaryNavigation.tsx
apps/console/src/shell/MobileNavigation.tsx
apps/console/playwright/memory-console.spec.ts
apps/console/playwright/memory-task-handoff.spec.ts
scripts/verify-console-memory-boundary.sh
docs/testing/memory-console-acceptance.md
```

## Normative client types

```ts
export type MemoryLifecycleStatus =
  | "candidate"
  | "active"
  | "superseded"
  | "soft_deleted"
  | "purge_pending"
  | "purged"
  | "outcome_unknown";

export interface MemoryFilterState {
  readonly query: string;
  readonly statuses: readonly MemoryLifecycleStatus[];
  readonly kinds: readonly string[];
  readonly origins: readonly ("local" | "mounted")[];
  readonly createdAfter?: string;
  readonly createdBefore?: string;
}

export type MemoryInspectorSection =
  | "provenance"
  | "revisions"
  | "relations"
  | "conflicts"
  | "usage"
  | "audit";

export interface MemoryRouteState {
  readonly filters: MemoryFilterState;
  readonly selectedMemoryId?: string;
  readonly inspector?: MemoryInspectorSection;
}

export interface MemoryCommandDescriptor<TPayload> {
  readonly command: string;
  readonly payload: TPayload;
  readonly expectedVersion: string;
  readonly idempotencyKey: string;
  readonly consequence: string;
}
```

The generated SDK may use different wire names. `memoryClient.ts` is the only translation boundary; feature components consume the types above and immutable display projections.

---

## Task 1: Establish route, public client and projection boundaries

**Files:** create `types.ts`, `routeState.ts`, `memoryClient.ts`, `memoryProjection.ts`; update route, query-key and navigation files; add the boundary script.

**Interfaces:** consumes H11 Memory and sharing schemas; produces typed queries, exact command descriptors and URL-safe route state.

- [ ] Write failing tests for `/memory`, `/memory/:memoryId`, Inspector deep links, filter round-tripping and invalid query normalization.
- [ ] Write a failing boundary test proving Memory code imports no server crate, SQL, repository, private HTTP handler or credential helper.
- [ ] Add `memory` to Workspace navigation and preserve selection and Inspector state through browser back/forward.
- [ ] Implement `memoryClient.ts` as a thin adapter around generated SDK methods; forbid generic `mutate(name, payload)` entry points.
- [ ] Normalize server data into immutable projections without changing lifecycle status or inventing missing provenance.
- [ ] Make query keys workspace-, viewer-, filter- and cursor-aware to prevent cross-workspace cache reuse.
- [ ] Run:

```bash
npm --prefix apps/console test -- memory routeState memoryClient memoryProjection
bash scripts/verify-console-boundary.sh
bash scripts/verify-console-memory-boundary.sh
npm --prefix apps/console run build
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/src/app apps/console/src/api/queryKeys.ts apps/console/src/shell scripts/verify-console-memory-boundary.sh
git commit -m "feat(console): establish the Memory Console boundary"
```

## Task 2: Build Memory Library, search and filters

**Files:** create `MemoryPage.tsx`, `MemoryLibrary.tsx`, `MemorySearch.tsx`, `MemoryFilters.tsx`, `MemoryResultRow.tsx` and focused tests.

**Interfaces:** consumes paginated Memory search projections; produces URL-backed filters and selected Memory identity.

- [ ] Write failing tests for empty, loading, partial-error, disconnected, stale-index and populated states.
- [ ] Implement debounced search with cancellation and generation guards so late responses cannot replace a newer workspace, query or cursor.
- [ ] Implement status, kind, origin and date filters with removable chips and an explicit clear action.
- [ ] Distinguish local, mounted, candidate, superseded, deleted and purge-pending rows using text and iconography, never color alone.
- [ ] Preserve active row, scroll position and filters when returning from Detail; virtualize long lists without losing table/list semantics.
- [ ] Announce result counts only after settled requests and avoid per-keystroke live-region noise.
- [ ] Run:

```bash
npm --prefix apps/console test -- MemoryLibrary MemorySearch MemoryFilters
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts --grep "library"
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/playwright/memory-console.spec.ts
git commit -m "feat(console): add the Memory Library"
```

## Task 3: Build immutable Detail and contextual Inspector

**Files:** create Detail, content, status, Inspector, provenance, revisions, relations, conflicts, usage and audit components.

**Interfaces:** consumes Memory detail and cursor-paged evidence projections; produces read-only Detail and route-addressable Inspector sections.

- [ ] Write failing tests for every lifecycle status, redacted fields, missing evidence, cursor continuation and independently failed Inspector sections.
- [ ] Render canonical content, current revision, status, kind, timestamps and origin without unsafe HTML or unbounded payload expansion.
- [ ] Render provenance as source references and evidence links; never imply confidence or trust absent from the response.
- [ ] Render revision lineage and supersession direction without offering in-place editing.
- [ ] Render relation, conflict, usage and audit sections through bounded pagination and safe correlation identifiers.
- [ ] Implement desktop Inspector, tablet drawer and mobile route with deterministic focus entry and restoration.
- [ ] Run:

```bash
npm --prefix apps/console test -- MemoryDetail MemoryInspector MemoryProvenance MemoryRevisions MemoryRelations MemoryConflicts MemoryUsage MemoryAudit
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts --grep "detail|inspector"
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/playwright/memory-console.spec.ts
git commit -m "feat(console): add Memory detail and evidence"
```

## Task 4: Implement candidate review and approval

**Files:** create `CandidateReviewForm.tsx`, extend status actions, client descriptors and tests.

**Interfaces:** consumes candidate projection and allowed actions; produces approve, edit-and-approve or reject commands.

- [ ] Write failing tests proving proposed content, provenance, capture profile, confidence and conflicts are visible before a decision.
- [ ] Implement editable approval as an explicit new accepted payload; do not mutate the candidate projection locally.
- [ ] Require a reason for rejection when the public contract requires it and preserve user input across recoverable failures.
- [ ] Disable actions when server capabilities do not authorize them; a hidden button is not an authorization boundary.
- [ ] Reconcile success from the returned authoritative revision and handle stale version by refetching before resubmission.
- [ ] Run:

```bash
npm --prefix apps/console test -- CandidateReviewForm MemoryStatusActions
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts --grep "candidate"
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/playwright/memory-console.spec.ts
git commit -m "feat(console): add Memory candidate review"
```

## Task 5: Implement correction, supersession and conflict resolution

**Files:** create `CorrectionForm.tsx`, `SupersedeDialog.tsx`, `ConflictResolutionForm.tsx`; extend command adapters and tests.

**Interfaces:** consumes exact versions, conflicts and allowed resolutions; produces separate correction, supersede and resolve-conflict commands.

- [ ] Write failing tests proving correction creates a new revision and never issues an update-in-place request.
- [ ] Require the user to review changed content, rationale and provenance before correction submission.
- [ ] Show both source and target identities, direction and consequence before supersession.
- [ ] Present conflict alternatives and evidence without preselecting a destructive resolution.
- [ ] On version conflict, keep the draft, show the authoritative change and require explicit rebase or discard.
- [ ] Treat transport ambiguity as `OutcomeUnknown`; query by idempotency key before enabling another command.
- [ ] Run:

```bash
npm --prefix apps/console test -- CorrectionForm SupersedeDialog ConflictResolutionForm
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts --grep "correction|supersede|conflict"
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/playwright/memory-console.spec.ts
git commit -m "feat(console): add governed Memory revisions"
```

## Task 6: Add soft deletion and protected hard purge

**Files:** create `DeleteMemoryDialog.tsx`, `HardPurgeDialog.tsx`; extend status actions, client descriptors and tests.

**Interfaces:** consumes retention, backup and purge-precondition projections; produces soft-delete, restore and hard-purge commands.

- [ ] Write failing tests that soft delete and hard purge cannot share a dialog, label, command or confirmation text.
- [ ] Implement soft delete with consequence, retention state and restore availability.
- [ ] Show purge scope across primary data, indexes, summaries, caches, encrypted representations and known backups exactly as returned by the server.
- [ ] Require capability, fresh challenge and exact confirmation phrase for hard purge; never cache the challenge.
- [ ] Keep `purge_pending`, `OutcomeUnknown` and downstream-cleanup-pending visible until authoritative completion.
- [ ] Verify restored pages do not resurrect a purged object from browser cache.
- [ ] Run:

```bash
npm --prefix apps/console test -- DeleteMemoryDialog HardPurgeDialog
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts --grep "delete|purge"
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/playwright/memory-console.spec.ts
git commit -m "feat(console): add protected Memory lifecycle controls"
```

## Task 7: Expose mounted Memory and governed target import

**Files:** create `SharedMemoryPanel.tsx`, `MemoryImportProposal.tsx`; extend library/detail projections and tests.

**Interfaces:** consumes mount, source-policy and availability projections; produces reviewable exact-import proposal and command.

- [ ] Write failing tests for active, review-required, revoked, quarantined, source-unavailable, source-erased and cleanup-pending mounts.
- [ ] Label source workspace, origin, content mode, granted permissions and live-read status on every mounted detail.
- [ ] Prevent correction, local supersession and local purge actions from appearing as valid operations on a mounted source object.
- [ ] Implement exact import only when separately authorized; show that the result is a new target-owned object with fresh target encryption.
- [ ] Prevent mount-of-mount and transitive-share affordances.
- [ ] Invalidate affected search/detail caches immediately on revoke or quarantine events without claiming remote purge completion.
- [ ] Run:

```bash
npm --prefix apps/console test -- SharedMemoryPanel MemoryImportProposal
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts --grep "mounted|shared|import"
```

- [ ] Commit:

```bash
git add apps/console/src/memory apps/console/playwright/memory-console.spec.ts
git commit -m "feat(console): add governed shared Memory views"
```

## Task 8: Qualify responsive, accessibility, security and visual behavior

**Files:** extend Playwright coverage, visual fixtures, accessibility tests and boundary scripts; add acceptance documentation.

**Interfaces:** consumes the complete Memory Console; produces reproducible qualification evidence.

- [ ] Cover 1440 px desktop, 1024 px tablet and 390 px mobile layouts, including long content and 200% zoom.
- [ ] Verify full keyboard operation, focus restoration, modal focus traps, skip navigation, semantic headings and reduced motion.
- [ ] Run axe-core on Library, Detail, each command dialog, mounted Memory and all error states in dark and light themes.
- [ ] Test hostile content, oversized provenance, bidi text, script-like strings and redacted secrets; assert no active content reaches the DOM.
- [ ] Test late responses after workspace, identity and route changes; assert no cross-workspace cache or announcement leak.
- [ ] Record visual baselines for Library + Detail + Inspector, tablet drawer and mobile routes.
- [ ] Document each acceptance scenario and evidence path in `docs/testing/memory-console-acceptance.md`.
- [ ] Run:

```bash
npm --prefix apps/console test -- memory
npm --prefix apps/console exec playwright test playwright/memory-console.spec.ts
bash scripts/verify-console-boundary.sh
bash scripts/verify-console-memory-boundary.sh
npm --prefix apps/console run build
```

- [ ] Commit:

```bash
git add apps/console docs/testing/memory-console-acceptance.md scripts/verify-console-memory-boundary.sh
git commit -m "test(console): qualify the Memory Console"
```

## Task 9: Prove Task-to-Memory vertical acceptance

**Files:** create `memory-task-handoff.spec.ts`; extend Memory acceptance documentation and release evidence index.

**Interfaces:** consumes a completed Run with a proposed Memory candidate; produces an accepted immutable Memory visible in search, Detail and provenance.

- [ ] Start a Run through the H11B Workbench and complete it through the authoritative H11/H11A path.
- [ ] Verify the agent proposes, but does not silently commit, a Memory candidate.
- [ ] Edit and approve the candidate; verify the returned revision, provenance and originating Run evidence.
- [ ] Search for the accepted Memory and open it through a deep link after a full page reload.
- [ ] Correct it into a new revision, verify lineage, then soft-delete and restore it.
- [ ] Exercise a mounted Memory read and verify it remains foreign until an independently authorized exact import.
- [ ] Repeat the critical path after service restart and prove idempotency for a retried approval response.
- [ ] Run the full H11B and Memory Console suites, then attach command output, screenshots and server evidence to the release evidence index.
- [ ] Run:

```bash
npm --prefix apps/console test
npm --prefix apps/console exec playwright test
bash scripts/verify-console-boundary.sh
bash scripts/verify-console-memory-boundary.sh
npm --prefix apps/console run build
```

- [ ] Commit:

```bash
git add apps/console/playwright/memory-task-handoff.spec.ts docs/testing/memory-console-acceptance.md docs/release
git commit -m "feat(console): complete Task-to-Memory acceptance"
```

---

## Completion gate

The Memory Console addendum is complete only when all of the following are true:

- [ ] No new database migration, domain aggregate, private API or browser-side authority was introduced.
- [ ] Library filters and selection survive navigation and reload without leaking workspace-scoped cache data.
- [ ] Detail and Inspector expose provenance, revisions, relations, conflicts, usage and audit with bounded pagination.
- [ ] Candidate approval remains explicit and editable; no candidate is committed by display or model use.
- [ ] Correction creates an immutable revision and supersession preserves direction and lineage.
- [ ] Soft delete, restore and hard purge use distinct commands and consequences.
- [ ] Ambiguous command outcomes reconcile through authoritative lookup before retry.
- [ ] Mounted content remains labeled foreign and untrusted across Library, Detail, search and Task use.
- [ ] Cross-workspace revoke and erase states immediately block new reads and remain honest about pending cleanup.
- [ ] No secret, hidden reasoning, active content or unsafe preview is rendered or persisted in the browser.
- [ ] Desktop, tablet and mobile layouts meet their locked interaction models.
- [ ] Keyboard, axe-core, 200% zoom, reduced-motion and visual-regression gates pass.
- [ ] The Task-to-Memory scenario passes after restart with attached server and UI evidence.
- [ ] H11B's complete acceptance suite still passes unchanged.
- [ ] A reviewer signs off the Memory Core, sharing, security and accessibility boundaries.
