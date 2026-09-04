# Vestrace Console Interface Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the approved task-first Vestrace console shell, Home surface, and URL-backed Runs workspace without inventing runtime data or disturbing existing generated/user files.

**Architecture:** Keep routing and API ownership in the existing React application. Add a pure run-workspace view model for deterministic filtering/grouping/selection, compose the `/runs` screen from focused rail/workspace/inspector components, and centralize the visual refresh in the existing token stylesheet. Existing SDK clients remain the only source of runtime truth.

**Tech Stack:** React 18, TypeScript 5.6, React Router 7, Vite 5, Node 22 built-in test runner, CSS custom properties.

**Spec:** `docs/superpowers/specs/2026-08-25-console-interface-redesign-design.md`

## Global Constraints

- Layout follows `C:\Users\venmi\OneDrive\Рабочий стол\vr\Сгенерированное изображение 1.png`; brand follows the official supplied Vestrace guide and SVG assets.
- Use navy surfaces, `#2563eb` blue, and `#22d3ee` cyan; do not add a purple accent system.
- Render only data returned by existing console clients or explicit unavailable/loading/error/empty states.
- Do not change Rust APIs, persistence, identity headers, authorization, Docker/nginx, or generated `apps/console/dist` output.
- Preserve all unrelated dirty files and the existing `apps/console/dist` and `apps/console/nginx.conf.template` status exactly.
- Do not commit, push, or deploy.

---

### Task 1: Deterministic run workspace model

**Files:**
- Create: `apps/console/tests/runWorkspaceModel.test.mjs`
- Create: `apps/console/src/routes/runWorkspaceModel.ts`

**Interfaces:**
- Consumes: `RunItem` and `RunStatus` from `apps/console/src/sdk/client.ts` as type-only imports.
- Produces: `RunGroup`, `RunFilter`, `RunStatusPresentation`, `getRunGroup(status)`, `filterRuns(runs, query, filter)`, `groupRuns(runs)`, `resolveSelectedRun(runs, requestedId)`, and `getRunStatusPresentation(status)`.

- [ ] **Step 1: Write the failing behavior test**

Use Node's test runner outside `src` so the browser TypeScript project does not require Node typings. Cover case-insensitive title/id matching, requested-id retention, unknown-id fallback, empty-list selection, all 14 declared status literals, and an unknown runtime status.

```ts
test('filters by title or id without case sensitivity', () => {
  assert.deepEqual(filterRuns(runs, 'ALPHA', 'all').map(({ id }) => id), ['run-alpha']);
  assert.deepEqual(filterRuns(runs, 'BETA-42', 'all').map(({ id }) => id), ['beta-42']);
});

test('resolves URL selection without inventing a run', () => {
  assert.equal(resolveSelectedRun(runs, 'beta-42')?.id, 'beta-42');
  assert.equal(resolveSelectedRun(runs, 'missing')?.id, 'run-alpha');
  assert.equal(resolveSelectedRun([], 'missing'), null);
});

test('places every declared status and an unknown value deterministically', () => {
  const expected = {
    created: 'active', preparing: 'active', running: 'active',
    waiting_for_input: 'waiting', waiting_for_approval: 'waiting',
    waiting_for_dependency: 'waiting', paused: 'waiting',
    paused_policy_changed: 'waiting', succeeded: 'finished',
    succeeded_with_warnings: 'finished', partial: 'finished',
    failed: 'finished', cancelled: 'finished', expired: 'finished',
  };
  for (const [status, group] of Object.entries(expected)) {
    assert.equal(getRunGroup(status), group, status);
  }
  assert.equal(getRunGroup('future_status'), 'other');
  assert.deepEqual(getRunStatusPresentation('future_status'), {
    label: 'Unknown',
    tone: 'neutral',
  });
});
```

- [ ] **Step 2: Run the test and observe RED**

Run:

```powershell
node --test --experimental-strip-types apps/console/tests/runWorkspaceModel.test.mjs
```

Expected: assertion failure because the new workspace model is unavailable.

- [ ] **Step 3: Implement the smallest pure model**

Use explicit status sets so every declared backend status lands in one stable group. An unrecognised runtime string maps to `other` with a neutral label/tone. Filtering preserves server order, trims the query, and matches only `title`, `id`, or the selected group.

```ts
export type RunFilter = 'all' | 'active' | 'waiting' | 'finished';
export function resolveSelectedRun(runs: readonly RunItem[], requestedId?: string): RunItem | null {
  return runs.find(({ id }) => id === requestedId) ?? runs[0] ?? null;
}
```

- [ ] **Step 4: Run the focused test and observe GREEN**

Run the same Node command and require all cases to pass.

- [ ] **Step 5: Review the scoped diff**

Confirm the module has no runtime SDK import, network access, React dependency, sorting side effect, or fallback record.

### Task 2: Official brand shell and responsive design system

**Files:**
- Create: `apps/console/src/assets/vestrace-mark.svg`
- Modify: `apps/console/src/shell/AppLayout.tsx`
- Modify: `apps/console/src/design-system/tokens/theme.css`

**Interfaces:**
- Consumes: the existing route children, readiness polling, identity diagnostics, and unchanged `NAV_GROUPS` destinations.
- Produces: a compact application shell and reusable classes/tokens consumed by Tasks 3-5.

- [ ] **Step 1: Add the official vector mark**

Copy the supplied mark geometry as text SVG into the source asset path. Preserve an accessible adjacent `Vestrace` wordmark in HTML; the decorative image uses an empty alt value.

- [ ] **Step 2: Refactor shell markup into semantic CSS classes**

Keep `aside`, primary `nav`, top `header`, `main#main-content`, skip link, readiness poll, identity warning, and mobile drawer behavior. Add `useLocation()` so `/runs` routes receive a full-bleed main-content modifier while other routes keep standard page padding.

- [ ] **Step 3: Replace the token and layout layer**

Define the approved navy/blue/cyan palette, compact type scale, focus ring, surface/border tokens, shell dimensions, form/button primitives, status chips, and pane classes. Add responsive rules for `1440`, `1200`, `1024`, and `720` px transitions and retain `prefers-reduced-motion` handling.

- [ ] **Step 4: Typecheck the shell**

```powershell
npm --prefix apps/console run typecheck
```

Expected: exit code 0.

- [ ] **Step 5: Review the scoped diff**

Confirm all existing destinations, kernel readiness states, identity error, skip link, keyboard focus, and mobile navigation remain functional.

### Task 3: Runs rail and URL-backed selection

**Files:**
- Create: `apps/console/src/components/RunsRail.tsx`
- Modify: `apps/console/src/routes/RunsPage.tsx`
- Modify: `apps/console/src/main.tsx`

**Interfaces:**
- Consumes: Task 1 model functions, `useApiResource(vestraceClient.listRuns)`, `vestraceClient.createRun`, `useParams`, and `useNavigate`.
- Produces: `/runs` and `/runs/:runId`, searchable/group-filtered run cards, selected-run state, and the outer operations-workspace grid.

- [ ] **Step 1: Add both route patterns**

Render the same `RunsPage` for `/runs` and `/runs/:runId`. Selecting a real run navigates to `/runs/${encodeURIComponent(id)}`; unknown URL ids resolve through the pure fallback without rendering a fictional run.

- [ ] **Step 2: Build the runs rail**

Use a labelled search input, a labelled native status filter, total/visible counts, reload control, and semantic buttons for run cards. Display only `title`, shortened `id`, mapped status label, and a real timestamp.

- [ ] **Step 3: Preserve truthful resource states and creation**

Retain loading, identity, API error, empty-list, reload, and title-based create-run behavior. After a successful create, insert the returned `RunItem` into local run state before navigating to its id, then trigger background revalidation. Do not depend on the current non-awaitable `reload()` to make selection safe.

- [ ] **Step 4: Connect the selected run to the workspace composition**

Render a selected-run header and pass the real `RunItem` into the central and support panes. When the list is empty, render an explicit empty workspace rather than placeholder content.

- [ ] **Step 5: Run focused behavior tests and typecheck**

```powershell
node --test --experimental-strip-types apps/console/tests/runWorkspaceModel.test.mjs
npm --prefix apps/console run typecheck
```

Expected: both commands exit 0.

### Task 4: Task-first workspace, separately scoped agent console, and truthful support rails

**Files:**
- Create: `apps/console/src/components/RunWorkspace.tsx`
- Create: `apps/console/src/components/RunInspector.tsx`
- Create: `apps/console/tests/agUiClient.test.mjs`
- Modify: `apps/console/src/components/CompactChat.tsx`
- Modify: `apps/console/src/sdk/agUiClient.ts`
- Modify: `apps/console/src/routes/RunsPage.tsx`

**Interfaces:**
- Consumes: selected `RunItem`, real `MetricsSummary`, the existing AG-UI create/stream behavior, and Task 2 pane classes.
- Produces: keyboard-operable `Workspace`/`Agent`/`Record` tabs, a separately identified AG-UI run console, run details, and explicitly workspace-level metrics.

- [ ] **Step 1: Build semantic workspace tabs**

Use buttons with `role="tab"`, `aria-selected`, `aria-controls`, and matching `tabpanel` regions. `Workspace` shows only run status, version, identity, and timestamps; `Record` renders the same `RunItem` as escaped JSON; `Agent` is explicitly separate from the selected run.

- [ ] **Step 2: Test AG-UI event-stream scoping**

Add a pure URL builder and test that a returned run id is URL-encoded into the stream query:

```js
test('scopes the event stream to the created AG-UI run', () => {
  assert.equal(buildAgUiEventStreamUrl('run/with space'), '/api/ag-ui/events/stream?run_id=run%2Fwith%20space');
});
```

- [ ] **Step 3: Implement a truthfully separate Agent console**

Do not pass the selected run id to `runAgent`: the current backend ignores it and creates a new run. State this behavior next to the form. After `runAgent` returns a new `run_id`, close any prior stream and call `connectEventStream(returnedRunId, handlers)` so only that new run's events appear. If no id is returned, show an explicit unavailable state. Retain endpoint/stream failures and do not synthesize assistant responses or event history.

- [ ] **Step 4: Build details and metrics support panes**

Run details may render only `RunItem` fields. Label metrics `Workspace metrics` and render only `live_runs`, `runs_today`, `registered_agents`, `registered_models`, and `run_budget_cap_micros`, with explicit loading/error/unavailable states.

- [ ] **Step 5: Validate keyboard and narrow-width composition**

Verify tabs remain reachable in order, visible focus is not clipped, rail headings remain associated with controls, and support panes stack below the workspace before they can cause horizontal overflow.

- [ ] **Step 6: Run focused behavior tests and typecheck**

Run:

```powershell
node --test --experimental-strip-types apps/console/tests/runWorkspaceModel.test.mjs apps/console/tests/agUiClient.test.mjs
npm --prefix apps/console run typecheck
```

Require both commands to exit 0.

### Task 5: Task-oriented Home surface

**Files:**
- Modify: `apps/console/src/routes/HomePage.tsx`

**Interfaces:**
- Consumes: the existing real metrics query, create-run mutation, identity/error states, and Task 2 design primitives.
- Produces: a branded launch surface with real metric cards and a clear path into Runs.

- [ ] **Step 1: Recompose Home around launch and orientation**

Use one restrained introductory panel, a real create-run form, a link to the runs workspace, and real metric cards. Keep values unavailable until returned; do not derive trends, percentages, costs, or charts.

- [ ] **Step 2: Preserve mutation and failure behavior**

Keep trimmed non-empty title validation, disable submission while creating, show returned errors, refresh real data after success, and navigate or link to the created run without fabricating intermediate state.

- [ ] **Step 3: Typecheck**

```powershell
npm --prefix apps/console run typecheck
```

Expected: exit code 0.

- [ ] **Step 4: Review copy and evidence provenance**

Confirm every number and state is either from the API or explicitly marked unavailable/loading/error.

### Task 6: Verification and preservation gate

**Files:**
- Verify only: all files changed in Tasks 1-5 plus pre-existing generated/user state.

**Interfaces:**
- Consumes: the completed implementation and the pre-change Git status/hashes.
- Produces: fresh scoped verification evidence without changing `apps/console/dist`.

**Pre-change snapshot captured before implementation:**

- `apps/console/dist/index.html`: exists, SHA-256 `618E85F7704D680F4F053E42A4CC13E7FB4747793D5751A14E495D2D55B18FB2`.
- `apps/console/nginx.conf.template`: exists, SHA-256 `14C0D178E121160DD0C3DD9CDA1531372F6545756B3628D3618C6F8462FEF64B`.
- `apps/console/dist/assets/index-CHQN-jlm.js`: already deleted in Git status.
- `apps/console/dist/assets/index-RbxBh8zj.css`: already deleted in Git status.

- [ ] **Step 1: Run behavior and type gates**

```powershell
node --test --experimental-strip-types apps/console/tests/runWorkspaceModel.test.mjs apps/console/tests/agUiClient.test.mjs
npm --prefix apps/console run typecheck
```

- [ ] **Step 2: Build into a validated disposable directory**

Create a unique directory below `[System.IO.Path]::GetTempPath()`, run `npm exec -- vite build --outDir <absolute-temp-path> --emptyOutDir` from `apps/console`, confirm output exists, then delete only that validated child directory.

- [ ] **Step 3: Run source and repository checks**

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- apps/console/src docs/superpowers/specs/2026-08-25-console-interface-redesign-design.md docs/superpowers/plans/2026-08-25-console-interface-redesign.md
git -c safe.directory=E:/Soft/vestrace status --short -- apps/console
```

- [ ] **Step 4: Perform browser acceptance**

Inspect at approximately 1536, 1180, 900, and 390 px widths. Check pane hierarchy, drawer behavior, run selection/deep-link fallback, run creation, tabs, truthful separate-Agent wording, filtered AG-UI stream/unavailable state, focus visibility, keyboard order, color contrast, reduced motion, and absence of page-level horizontal overflow.

- [ ] **Step 5: Prove preservation**

Compare the final status and hashes for `apps/console/dist/index.html` and `apps/console/nginx.conf.template` with the pre-change snapshot; confirm the two already-deleted hashed assets remain deleted and no new `dist` assets were generated.

- [ ] **Step 6: Independent review**

Review the complete relevant diff against every acceptance criterion. Report only verified behavior, scoped passes, unavailable live-backend evidence, and any remaining caveat.
