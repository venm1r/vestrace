# Vestrace H11B Web Application and Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, install frontend dependencies, generate application code, modify schemas, update CI, change release images or implement production behavior until the user explicitly ends the documentation-only phase.

**Goal:** Build the first-party Vestrace web application as a task-first, accessible and secure workspace that combines AG-UI interaction with H11 product APIs while keeping all execution, policy and durable state authoritative on the server.

**Architecture:** The application is a React/Vite TypeScript client split into a reusable design-system layer, a product shell, feature modules and two transport adapters: the H11 TypeScript SDK for ordinary product/administrative commands and H11A AG-UI for interactive Run streaming, interrupts and generative UI. Client reducers maintain bounded projections only. The application never reproduces Run transitions, approval validation, Tool authority, Artifact classification or verification logic.

**Tech Stack:** H11 TypeScript SDK and public schemas; H11A AG-UI TypeScript integration; React/Vite workspace versions pinned by H11; TypeScript strict mode; React Router; CSS custom properties and design tokens; Testing Library; Vitest; Playwright; axe-core; Storybook or the repository-approved isolated component harness; browser-native `fetch`, streaming and ResizeObserver; no server-side rendering requirement for v0.2.

## Global Constraints

- `docs/superpowers/specs/2026-07-31-vestrace-web-ui-design.md` is normative.
- ADR-0004, H11 and H11A remain normative for interaction and API boundaries.
- The product is Task-first, not chat-first.
- Simple is the default presentation. Advanced detail is revealed locally through inline expansion and a contextual inspector; there is no global Simple/Advanced switch.
- The web application uses only the public TypeScript SDK and the explicit AG-UI integration layer.
- The client has no direct database, repository, internal adapter or private service access.
- AG-UI is used for interactive Run chat, activity, state projection, interrupts and schema-pinned generative UI.
- `/v1` and `/admin/v1` are used for product and administrative resources and explicit commands.
- UI state, caches, reducers and AG-UI state are projections only.
- Browser credentials remain memory-only and are never stored in localStorage, sessionStorage or IndexedDB.
- LocalTrusted writes preserve H11 Origin, Host, Sec-Fetch-Site and process-local console nonce requirements.
- Active HTML, SVG, scripts and unsafe remote content never enter the DOM.
- H6 safe previews or controlled downloads are the only Artifact-content surfaces.
- Hidden reasoning, `RAW`, `THINKING_*`, `REASONING_*`, secret-bearing Tool arguments/results and opaque encrypted reasoning are never rendered.
- Effectful actions use explicit H11 commands with authentication, idempotency, expected version and H2 authorization.
- Approval UI must show operation, resource, consequence, risk, expiry and challenge/fingerprint context. Generic yes/no approval is prohibited.
- Unknown is distinct from Failed and must not trigger a new operation automatically.
- H7 cursor and H11A event identity remain authoritative for reconnect and deduplication.
- Dark is the primary visual reference; Light is a complete equivalent; System is the initial default.
- Comfortable density is the default; Compact is available for tables and technical views.
- Desktop and tablet support the complete workspace. Mobile prioritizes Home, Runs, Human Requests, results, Artifacts and safe actions.
- Core release workflows target WCAG 2.2 AA.
- Structured Depth is the visual system: neutral layered surfaces, thin borders, small elevation differences, restrained shadows and rare semantic glow.
- Color is never the only carrier of status, risk or urgency.
- No new domain aggregate, persistence schema or migration is introduced by H11B.
- H11B follows H11A for the complete interactive acceptance path, but design-system and ordinary product-shell tasks may execute once H11 public contracts are stable.
- Future implementation branch: `feat/harness-web-application`.

---

## Locked file structure

```text
apps/console/
  src/
    app/
      App.tsx
      router.tsx
      providers.tsx
      routes.ts
      errorBoundary.tsx
    auth/
      authContext.tsx
      bearerSession.ts
      localTrustedSession.ts
      consoleNonce.ts
    api/
      productClient.ts
      agUiClient.ts
      queryKeys.ts
      errors.ts
    design-system/
      tokens/
        color.css
        typography.css
        spacing.css
        radius.css
        elevation.css
        motion.css
        density.css
        breakpoints.ts
      primitives/
        Button.tsx
        IconButton.tsx
        Surface.tsx
        Stack.tsx
        Inline.tsx
        Text.tsx
        Badge.tsx
        StatusIndicator.tsx
        Progress.tsx
        Skeleton.tsx
        Tooltip.tsx
        Dialog.tsx
        Drawer.tsx
        Menu.tsx
        Tabs.tsx
        DataTable.tsx
        VirtualList.tsx
      patterns/
        PageHeader.tsx
        EmptyState.tsx
        ErrorState.tsx
        ConfirmCommandDialog.tsx
        RiskBanner.tsx
        DetailDisclosure.tsx
        ContextInspector.tsx
        ResponsiveShell.tsx
      theme/
        ThemeProvider.tsx
        DensityProvider.tsx
        preference.ts
    shell/
      AppShell.tsx
      PrimaryNavigation.tsx
      MobileNavigation.tsx
      WorkspaceSwitcher.tsx
      GlobalHumanRequestBadge.tsx
      CommandPalette.tsx
    home/
      HomePage.tsx
      SmartComposer.tsx
      ComposerControls.tsx
      ContinueWork.tsx
      ActiveRuns.tsx
      HumanRequestInbox.tsx
      RecentArtifacts.tsx
      SystemSummary.tsx
    runs/
      RunsPage.tsx
      RunWorkspacePage.tsx
      RunHeader.tsx
      RunStatusSummary.tsx
      AdaptivePlan.tsx
      PlanList.tsx
      PlanKanban.tsx
      CurrentWork.tsx
      IntermediateResults.tsx
      CompactChat.tsx
      RunArtifacts.tsx
      FinalResultCard.tsx
      RunInspector.tsx
      runProjection.ts
    human-requests/
      HumanRequestCard.tsx
      ApprovalCard.tsx
      ClarificationForm.tsx
      ReviewForm.tsx
      HumanRequestDetail.tsx
    artifacts/
      ArtifactsPage.tsx
      ArtifactCard.tsx
      ArtifactPreview.tsx
      ArtifactProvenance.tsx
      ArtifactDownloadAction.tsx
    result-report/
      ResultReportPage.tsx
      ResultSummary.tsx
      PlanReport.tsx
      EvidenceReport.tsx
      VerificationReport.tsx
      UsageReport.tsx
      ApprovalHistory.tsx
    automation/
      AgentsPage.tsx
      WorkflowsPage.tsx
      TriggersPage.tsx
    system/
      ConnectionsPage.tsx
      ModelsPage.tsx
      EvaluationsPage.tsx
      AuditPage.tsx
      SettingsPage.tsx
    generative-ui/
      registry.ts
      renderer.tsx
      schemas.ts
      ApprovalChallengeCard.tsx
      OptionComparisonCard.tsx
      ArtifactPreviewCard.tsx
      BudgetWarningCard.tsx
      VerificationFindingCard.tsx
      RemoteStatusCard.tsx
      UnknownOperationCard.tsx
    state/
      routeState.ts
      inspectorState.ts
      eventDedup.ts
      projectionLag.ts
    styles/
      reset.css
      globals.css
      utilities.css
  tests/
    fixtures/
    visual/
    accessibility/
    security/
    contracts/
  stories/
  playwright/
    home.spec.ts
    run-workspace.spec.ts
    human-requests.spec.ts
    result-report.spec.ts
    responsive.spec.ts
    reconnect.spec.ts
    security.spec.ts
    accessibility.spec.ts

packages/sdk-typescript/src/
  public product client from H11
  ag-ui integration from H11A

scripts/
  verify-console-boundary.sh
  verify-console-security.sh
  verify-console-accessibility.sh
  verify-console-visual-baseline.sh
  run-h11b-web-acceptance.sh

docs/
  console.md
  console-security.md
  console-accessibility.md
  console-design-system.md
```

---

## Normative UI contracts

### Route model

```ts
export type AppRoute =
  | { kind: "home" }
  | { kind: "runs"; filters?: RunFilterState }
  | { kind: "run"; runId: string; inspector?: InspectorRouteState }
  | { kind: "result-report"; runId: string }
  | { kind: "artifacts"; filters?: ArtifactFilterState }
  | { kind: "artifact"; artifactId: string; revisionId?: string }
  | { kind: "agents" }
  | { kind: "workflows" }
  | { kind: "triggers" }
  | { kind: "connections" }
  | { kind: "models" }
  | { kind: "evaluations" }
  | { kind: "audit" }
  | { kind: "settings" };
```

Inspector route state is secondary navigation. Closing the inspector restores the same primary Run route.

### Presentation preferences

```ts
export type ThemePreference = "system" | "dark" | "light";
export type DensityPreference = "comfortable" | "compact";
export type PlanViewPreference = "list" | "kanban";

export interface PresentationPreferences {
  theme: ThemePreference;
  density: DensityPreference;
  inspectorWidthPx: number;
  lastInspectorSection?: string;
  runPlanViews: Readonly<Record<string, PlanViewPreference>>;
}
```

These preferences are non-authoritative presentation data. H11B first keeps them in an in-memory provider and, where permitted by the existing H11 user-settings API, synchronizes only validated presentation values. No credential, Run state, approval payload or sensitive content is stored with them.

### Command boundary

```ts
export interface CommandDescriptor<TPayload> {
  operation: string;
  idempotencyKey: string;
  expectedVersion?: string;
  payload: TPayload;
  confirmation: {
    title: string;
    consequence: string;
    risk: "low" | "medium" | "high" | "critical";
    resourceLabel: string;
  };
}
```

Every effectful UI action is constructed as a `CommandDescriptor`, reviewed in `ConfirmCommandDialog` when required and submitted through the generated H11 client. AG-UI events do not directly execute it.

### Inspector contract

```ts
export type InspectorSection =
  | "summary"
  | "step"
  | "human-request"
  | "tools"
  | "subruns"
  | "remotes"
  | "models"
  | "budget"
  | "artifacts"
  | "verification"
  | "events"
  | "metadata";

export interface InspectorSelection {
  section: InspectorSection;
  resourceId?: string;
  runVersion?: number;
}
```

Inspector content is loaded through public viewer-filtered queries. It never accepts arbitrary repository/table identifiers.

### Generative UI registry

```ts
export interface GenerativeUiDefinition<TPayload> {
  stableId: string;
  schemaVersion: number;
  validate(payload: unknown): TPayload;
  render(payload: TPayload, context: GenerativeUiContext): React.ReactNode;
  allowedActions: readonly string[];
}
```

Only compile-time registered, schema-pinned definitions render. Unknown definitions produce a safe unsupported-card state. Arbitrary HTML, scripts, styles and component code are rejected.

---

## Task 1: Establish the console boundary, test harness and application shell

**Files:** create `app`, `auth`, `api` foundations; configure Vitest, Testing Library and Playwright; add boundary verification script.

**Interfaces:**
- Consumes: H11 generated TypeScript client factory and H11A AG-UI client factory.
- Produces: `ProductClientContext`, `AgUiClientContext`, `AuthSession`, `AppRoute`, `AppShell`.

- [ ] Write failing tests proving the console imports only public SDK entry points, starts in an unauthenticated shell, and routes errors through `AppErrorBoundary`.
- [ ] Add `scripts/verify-console-boundary.sh` that fails when console source imports `vestrace-domain`, infrastructure paths, SQL modules, internal HTTP handlers or server adapter crates.
- [ ] Implement `AuthSession` variants `LocalTrustedSession` and `BearerSession`; bearer tokens remain closure-held memory values and `toJSON`/Debug helpers redact them.
- [ ] Implement `ProductClientProvider` and `AgUiClientProvider` from explicit authenticated factories; no global singleton.
- [ ] Implement the route table and an application error boundary with safe correlation ID display.
- [ ] Run:

```bash
npm --prefix apps/console test -- app auth api
bash scripts/verify-console-boundary.sh
npm --prefix apps/console run build
```

- [ ] Commit:

```bash
git add apps/console scripts/verify-console-boundary.sh
git commit -m "feat(console): establish the public application boundary"
```

## Task 2: Build Structured Depth tokens, themes and density

**Files:** create design tokens, `ThemeProvider`, `DensityProvider`, primitive surfaces and isolated component stories/tests.

**Interfaces:**
- Consumes: `ThemePreference`, `DensityPreference`.
- Produces: CSS token contract, `useThemePreference`, `useDensityPreference`, `Surface`, `Text`, `Stack`, `Inline`, `StatusIndicator`.

- [ ] Write token tests asserting complete dark/light semantic token sets for background, surface, border, text, focus, status and risk roles.
- [ ] Define CSS custom properties for color, typography, spacing, radius, elevation, motion and density; prohibit feature components from hard-coding semantic colors.
- [ ] Implement system-theme initialization without a flash of incorrect theme using a non-secret inline class bootstrap compatible with H11 CSP.
- [ ] Implement Comfortable and Compact density through token overrides; minimum interactive target remains accessible in both modes.
- [ ] Build primitives with keyboard/focus semantics and reduced-motion support.
- [ ] Add visual baselines for dark/light and Comfortable/Compact combinations.
- [ ] Run:

```bash
npm --prefix apps/console test -- design-system
npm --prefix apps/console run storybook:test
bash scripts/verify-console-visual-baseline.sh
```

- [ ] Commit:

```bash
git add apps/console/src/design-system apps/console/stories apps/console/tests/visual
git commit -m "feat(console): add the Structured Depth design system"
```

## Task 3: Implement responsive shell and hybrid navigation

**Files:** create `AppShell`, desktop/tablet/mobile navigation, workspace switcher, badges and route restoration.

**Interfaces:**
- Consumes: `AppRoute`, viewer capabilities, unresolved Human Request count.
- Produces: stable shell regions and navigation links for Workspace, Automation and System.

- [ ] Write tests for desktop persistent sidebar, tablet collapse and mobile bottom navigation/menu sheet.
- [ ] Implement navigation groups exactly as Workspace: Home/Runs/Artifacts; Automation: Agents/Workflows/Triggers; System: Connections/Models/Evaluations/Audit/Settings.
- [ ] Capability-filter System actions without hiding the existence of a resource when the API contract requires a disabled/read-only state.
- [ ] Implement the global Human Request badge as a projection count linked to the inbox.
- [ ] Preserve primary route and inspector route state across back/forward navigation.
- [ ] Test keyboard navigation, focus restoration and small viewport overflow.
- [ ] Run:

```bash
npm --prefix apps/console test -- shell router
npm --prefix apps/console exec playwright test playwright/responsive.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/src/shell apps/console/src/app apps/console/playwright/responsive.spec.ts
git commit -m "feat(console): add adaptive product navigation"
```

## Task 4: Build Home and the Smart Composer

**Files:** create Home sections, composer controls and launch command mapping.

**Interfaces:**
- Consumes: H11 queries for recent Runs, Human Requests, Artifacts, health and composer option catalogues.
- Produces: `CreateRunCommandInput`, launch receipt navigation and presentation-only composer chips.

- [ ] Write tests for empty, loading, partial-failure and populated Home states.
- [ ] Implement Smart Composer with always-visible task text, attachments and submit; expandable Agent, Workflow, sources, autonomy, budget, output and schedule controls.
- [ ] Render selected constraints as removable chips and require confirmation for expensive/effectful selections.
- [ ] Construct one idempotency key per launch attempt and preserve it across an allowed transport retry.
- [ ] Prevent submission until attachment references are accepted by H6/H11 upload surfaces; never embed full binary content in composer state snapshots.
- [ ] Implement Continue Work, Active Runs, Human Request inbox summary, Recent Artifacts and compact System Summary.
- [ ] Run:

```bash
npm --prefix apps/console test -- home
npm --prefix apps/console exec playwright test playwright/home.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/src/home apps/console/playwright/home.spec.ts
git commit -m "feat(console): add Home and the Smart Composer"
```

## Task 5: Build Runs discovery and Task Workspace skeleton

**Files:** create Runs list/table, Task Workspace page, Run header, status summary and layout skeleton.

**Interfaces:**
- Consumes: H11 Run list/detail queries and viewer capabilities.
- Produces: `RunWorkspaceContext`, page regions and command descriptors for pause/resume/cancel.

- [ ] Write tests for Run filters, card/table density, pagination, deep-linking and all authoritative Run statuses including Unknown.
- [ ] Implement Run Workspace order: Goal/Status, nearest action, Plan, Human Requests, Current Work, Intermediate Results, Final Result, Artifacts and Compact Chat.
- [ ] Implement desktop three-region layout with resizable/dismissible inspector; tablet drawer and mobile detail route.
- [ ] Build pause/resume/cancel descriptors with expected version and separated destructive confirmation.
- [ ] Display projection lag and disconnected stream separately from authoritative Run status.
- [ ] Virtualize long Run lists without dropping accessible row semantics.
- [ ] Run:

```bash
npm --prefix apps/console test -- runs
npm --prefix apps/console exec playwright test playwright/run-workspace.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/src/runs apps/console/playwright/run-workspace.spec.ts
git commit -m "feat(console): add the task-first Run workspace"
```

## Task 6: Implement Adaptive Plan list, Kanban and local Advanced detail

**Files:** create Plan list/Kanban, step cards and inline `DetailDisclosure` integration.

**Interfaces:**
- Consumes: viewer-filtered ExecutionPlan projection and step details.
- Produces: presentation-only `PlanViewPreference` and inspector selections.

- [ ] Write tests proving list is default, Kanban changes presentation only and the system recommendation never auto-switches the user.
- [ ] Implement Planned/Running/Waiting/Done Kanban columns from projection state; Failed/Cancelled/Unknown remain semantically labelled within their appropriate non-success column or explicit terminal lane per schema.
- [ ] Add inline Advanced disclosure for step ID, dependencies, owner, SubRun/remote reference, Tool summary, budget allocation and verification state.
- [ ] Route full detail to the inspector without mutating plan or step state.
- [ ] Implement keyboard-accessible list and Kanban traversal with visible focus.
- [ ] Run:

```bash
npm --prefix apps/console test -- AdaptivePlan PlanList PlanKanban
npm --prefix apps/console exec playwright test --grep "adaptive plan"
```

- [ ] Commit:

```bash
git add apps/console/src/runs/AdaptivePlan.tsx apps/console/src/runs/PlanList.tsx \
  apps/console/src/runs/PlanKanban.tsx apps/console/src/design-system/patterns/DetailDisclosure.tsx
git commit -m "feat(console): add adaptive Run plan views"
```

## Task 7: Integrate AG-UI streaming, Compact Chat and safe activity

**Files:** create AG-UI session/reducer integration, Compact Chat, Current Work and event dedup state.

**Interfaces:**
- Consumes: H11A `AgUiProjectedEvent`, H7 cursor extension and reconnect contract.
- Produces: bounded `RunInteractionProjection`, deduplicated messages/activities and reconnect state.

- [ ] Write reducer golden tests for text start/content/end, messages snapshot, steps, activity snapshot/delta, safe Tool projection, interrupt and terminal success.
- [ ] Implement deduplication by Vestrace event ID plus deterministic projection key.
- [ ] Reconnect with H7 cursor; reject disagreement between stored cursor and server-provided resume state.
- [ ] Batch streaming updates to avoid main-thread starvation and cap retained transient fragments.
- [ ] Render Compact Chat below task content, preserving references to steps, Artifacts and Human Requests.
- [ ] Render safe activity summaries and never expose forbidden reasoning/RAW events; forbidden input produces a security-visible unsupported-event fault, not content rendering.
- [ ] Test disconnect does not alter Run status and terminal success appears only after the server event.
- [ ] Run:

```bash
npm --prefix apps/console test -- ag-ui CompactChat CurrentWork eventDedup
npm --prefix apps/console exec playwright test playwright/reconnect.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/src/api/agUiClient.ts apps/console/src/runs/CompactChat.tsx \
  apps/console/src/runs/CurrentWork.tsx apps/console/src/state apps/console/tests/contracts
git commit -m "feat(console): integrate durable AG-UI interaction"
```

## Task 8: Build Human Request inbox, forms and exact approval UX

**Files:** create Human Request cards/forms/detail, global inbox and command mappings.

**Interfaces:**
- Consumes: H7/H11A Human Request projection, response schema, approval challenge and viewer eligibility.
- Produces: exact `SubmitHumanResponse` or `GrantApproval` command descriptors.

- [ ] Write tests for clarification, review, authentication and approval requests across low/medium/high/critical intensity.
- [ ] Implement one authoritative request projected in global inbox, Run workspace and navigation badge.
- [ ] Build schema-controlled clarification/review forms; reject unknown form controls and stale request versions.
- [ ] Approval cards show operation, resource, effect, risk, cost/budget impact, challenge/fingerprint summary, expiry and responder context.
- [ ] Require explicit confirmation for high/critical approval and show the complete consequence text on mobile.
- [ ] Remove generic yes/no submission paths; deny and approve remain distinct commands.
- [ ] Test resolved requests disappear consistently after authoritative event/query refresh.
- [ ] Run:

```bash
npm --prefix apps/console test -- human-requests ApprovalCard
npm --prefix apps/console exec playwright test playwright/human-requests.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/src/human-requests apps/console/src/home/HumanRequestInbox.tsx \
  apps/console/playwright/human-requests.spec.ts
git commit -m "feat(console): add exact Human Request and approval flows"
```

## Task 9: Implement Artifacts, safe previews and provenance

**Files:** create Artifact pages/cards/preview/provenance/download actions.

**Interfaces:**
- Consumes: H11 Artifact metadata, safe representation, download-grant and provenance APIs.
- Produces: preview-only components and explicit export/download command descriptors.

- [ ] Write tests for metadata-only, quarantined, validated, preview-ready, export-pending, rejected and purged states.
- [ ] Render only exact H6 safe preview representations; disallow arbitrary iframe, object, active SVG and raw HTML injection.
- [ ] Force attachment download unless the response is an explicitly safe preview representation.
- [ ] Show revision lineage, provenance edges, source Run, verification and export status.
- [ ] Create one-use download/export commands with expected version and consequence confirmation where required.
- [ ] Test range download UI, expired grants, cross-workspace denial and preview sanitization.
- [ ] Run:

```bash
npm --prefix apps/console test -- artifacts
npm --prefix apps/console exec playwright test --grep "artifact"
```

- [ ] Commit:

```bash
git add apps/console/src/artifacts apps/console/src/runs/RunArtifacts.tsx
git commit -m "feat(console): add safe Artifact workflows"
```

## Task 10: Build Final Result and full Result Report

**Files:** create compact final card and complete report sections with print/export stylesheet.

**Interfaces:**
- Consumes: H11 verified Run result, plan/evidence/usage/approval/Artifact projections.
- Produces: compact summary and safe report route.

- [ ] Write tests for verified success, partial verified result, unresolved warnings and non-terminal Runs.
- [ ] Show conclusion, verification status, key decisions, primary Artifacts, warnings and Open Full Report action in the workspace.
- [ ] Implement report sections for goal/outcome, completed plan, evidence, Tools/SubRuns/remotes, Artifacts, usage, verification, approvals and unresolved warnings.
- [ ] Exclude hidden reasoning, secrets, unsafe Tool payloads and deployment-local internal paths.
- [ ] Add print-safe HTML styling and request server-generated PDF/export through an ordinary H11 command rather than client-side privileged rendering.
- [ ] Test stable deep links, print layout and viewer filtering.
- [ ] Run:

```bash
npm --prefix apps/console test -- result-report FinalResultCard
npm --prefix apps/console exec playwright test playwright/result-report.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/src/result-report apps/console/src/runs/FinalResultCard.tsx \
  apps/console/playwright/result-report.spec.ts
git commit -m "feat(console): add verified Run result reports"
```

## Task 11: Build contextual inspector and technical projections

**Files:** create `RunInspector`, `ContextInspector`, inspector state and lazy detail sections.

**Interfaces:**
- Consumes: `InspectorSelection` and viewer-filtered H11 queries.
- Produces: resizable desktop panel, tablet drawer and mobile detail route.

- [ ] Write tests for summary, step, Human Request, Tools, SubRuns, remotes, models, budget, Artifacts, verification, events and metadata sections.
- [ ] Load each section lazily and cancel stale requests when selection changes.
- [ ] Persist only validated width and last section presentation preferences; never cache sensitive payloads in browser persistence.
- [ ] Render bounded structured metadata through safe key/value or JSON viewers with copy controls and redaction markers.
- [ ] Show model usage, budget and Tool activity as projections; no client-side recalculation overrides server totals.
- [ ] Implement focus containment/restoration for tablet drawer and mobile route.
- [ ] Run:

```bash
npm --prefix apps/console test -- RunInspector ContextInspector inspectorState
npm --prefix apps/console exec playwright test --grep "inspector"
```

- [ ] Commit:

```bash
git add apps/console/src/runs/RunInspector.tsx apps/console/src/design-system/patterns/ContextInspector.tsx \
  apps/console/src/state/inspectorState.ts
git commit -m "feat(console): add the contextual Run inspector"
```

## Task 12: Implement schema-pinned generative UI

**Files:** create registry, validator, renderer and initial approved components.

**Interfaces:**
- Consumes: H11A schema-pinned `vestrace.*` custom events and frontend-action catalogue references.
- Produces: safe React nodes and explicit allowed action callbacks.

- [ ] Write tests proving unknown component IDs, wrong schema versions, extra effectful actions, scripts, HTML and style payloads are rejected.
- [ ] Implement compile-time registry entries for approval challenge, clarification form, option comparison, Artifact preview, budget warning, verification finding, remote status, Unknown operation and structured result summary.
- [ ] Validate payloads before rendering and use only design-system primitives.
- [ ] Map client-only actions to navigation/focus/copy; map server effects to ordinary command descriptors.
- [ ] Never evaluate code, templates, expressions, arbitrary CSS or remote component URLs.
- [ ] Add component stories and accessibility checks for every registry entry.
- [ ] Run:

```bash
npm --prefix apps/console test -- generative-ui
npm --prefix apps/console run storybook:test -- --grep "Generative UI"
```

- [ ] Commit:

```bash
git add apps/console/src/generative-ui apps/console/stories
git commit -m "feat(console): add schema-pinned generative UI"
```

## Task 13: Complete Automation and System administration surfaces

**Files:** create Agents, Workflows, Triggers, Connections, Models, Evaluations, Audit and Settings pages.

**Interfaces:**
- Consumes: H11 `/v1` and `/admin/v1` resource/query/command contracts and viewer capabilities.
- Produces: explicit administration forms and command descriptors; no AG-UI state dependency.

- [ ] Write route and capability tests for every administration page.
- [ ] Build list/detail/revision views for Agents, Workflows and Triggers with explicit activate/deactivate/fire commands.
- [ ] Build Connection status and authorization-start flows without exposing credential material.
- [ ] Build Models and Evaluations views with revision, readiness and regression evidence.
- [ ] Build virtualized Audit with signed cursor pagination and redacted event detail.
- [ ] Build Settings for theme, density and permitted presentation preferences plus deployment-safe read-only configuration summaries.
- [ ] Keep secret/provider/hard-purge/backup/upgrade operations under exact `/admin/v1` capabilities and explicit confirmations.
- [ ] Run:

```bash
npm --prefix apps/console test -- automation system
npm --prefix apps/console exec playwright test --grep "administration"
```

- [ ] Commit:

```bash
git add apps/console/src/automation apps/console/src/system
git commit -m "feat(console): add product administration surfaces"
```

## Task 14: Enforce security, accessibility and responsive acceptance

**Files:** add security/accessibility scripts, CSP checks, Playwright suites and documentation.

**Interfaces:**
- Consumes: completed shell/features and H11 auth rules.
- Produces: release evidence for browser security, WCAG 2.2 AA core workflows and responsive surfaces.

- [ ] Test that bearer values never enter Web Storage, IndexedDB, URLs, analytics, snapshots, errors or console logs.
- [ ] Test LocalTrusted writes require expected nonce/header behavior and fail on invalid Origin/Host/Sec-Fetch-Site fixtures.
- [ ] Test XSS payloads through chat, Tool projections, Artifact metadata, custom events and structured metadata.
- [ ] Enforce CSP without unsafe-eval/arbitrary inline scripts; verify no third-party remote script dependency in the release shell.
- [ ] Run axe-core and keyboard-only acceptance for task launch, plan, Human Request, approval, result review, inspector and administration navigation.
- [ ] Verify dark/light contrast, visible focus, reduced motion, semantic announcements and touch targets.
- [ ] Verify desktop/tablet/mobile behavior and prohibit destructive swipe/long-press actions.
- [ ] Run:

```bash
bash scripts/verify-console-security.sh
bash scripts/verify-console-accessibility.sh
npm --prefix apps/console exec playwright test playwright/security.spec.ts \
  playwright/accessibility.spec.ts playwright/responsive.spec.ts
```

- [ ] Commit:

```bash
git add apps/console/tests/security apps/console/tests/accessibility apps/console/playwright \
  scripts/verify-console-security.sh scripts/verify-console-accessibility.sh \
  docs/console-security.md docs/console-accessibility.md
git commit -m "test(console): qualify security accessibility and responsiveness"
```

## Task 15: Run the complete H11B product acceptance and release qualification

**Files:** create full acceptance script, release evidence, final docs and visual baseline manifest.

**Interfaces:**
- Consumes: H11 product APIs, H11A AG-UI gateway, reference package and all H11B features.
- Produces: deterministic H11B qualification report included by the H11/H11A release manifest process.

- [ ] Implement deterministic acceptance fixture:

```text
Home
→ Smart Composer with Agent, budget and document
→ one durable Run
→ Task-first workspace
→ list plan, user-selected Kanban
→ streamed activity and compact chat
→ high-risk approval in inbox and Run
→ exact approval response
→ Artifact safe preview
→ inspector Tool/budget/verification detail
→ runtime restart
→ cursor reconnect without duplicate UI records
→ verified result card
→ full Result Report
```

- [ ] Run the same scenario at desktop, tablet and mobile review/approval breakpoints; mobile creation may use the simplified composer but must preserve constraints.
- [ ] Inject failures for stale version, denied capability, upload rejection, stream disconnect, projection lag, operation Unknown and partial verified result.
- [ ] Verify no duplicate command, Run, message, activity, Human Request or terminal result projection.
- [ ] Record bundle size, initial shell readiness, large-list virtualization, streaming batch latency and memory ceilings using fixed release fixtures.
- [ ] Generate a visual-baseline manifest for dark/light and Comfortable/Compact critical screens.
- [ ] Publish `docs/console.md` and `docs/console-design-system.md` covering navigation, command safety, AG-UI/API split, responsive behavior and extension rules.
- [ ] Run:

```bash
npm --prefix apps/console test
npm --prefix apps/console run build
npm --prefix apps/console exec playwright test
bash scripts/verify-console-boundary.sh
bash scripts/verify-console-security.sh
bash scripts/verify-console-accessibility.sh
bash scripts/verify-console-visual-baseline.sh
bash scripts/run-h11b-web-acceptance.sh
```

- [ ] Commit:

```bash
git add apps/console scripts/run-h11b-web-acceptance.sh \
  scripts/verify-console-visual-baseline.sh docs/console.md docs/console-design-system.md
git commit -m "feat(console): qualify the Vestrace web application"
```

---

## Cross-task acceptance matrix

| Requirement | Owning task |
|---|---:|
| Public-only client boundary | 1 |
| Structured Depth tokens/themes/density | 2 |
| Hybrid navigation and adaptive shell | 3 |
| Home and Smart Composer | 4 |
| Task-first workspace | 5 |
| Adaptive List/Kanban plan | 6 |
| AG-UI chat/activity/reconnect | 7 |
| Human Requests and exact approvals | 8 |
| Artifact preview/provenance/export | 9 |
| Compact result and Result Report | 10 |
| Local Advanced inspector | 11 |
| Schema-pinned generative UI | 12 |
| Automation/System administration | 13 |
| Security, WCAG and responsive surfaces | 14 |
| Full restart-safe product acceptance | 15 |

## Exit gate

H11B passes only when a first-time user can launch and understand a durable task through the Simple Task-first workspace, respond safely to a typed Human Request, inspect Advanced detail locally, reconnect after a complete runtime restart, review a verified result and use the same product across desktop, tablet and mobile review surfaces. The acceptance must prove that the browser never becomes authoritative, never stores credentials persistently, never renders forbidden reasoning or unsafe active content, and never executes an effect without an ordinary H11 command and policy boundary.
