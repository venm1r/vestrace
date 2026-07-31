# Vestrace Web UI Design

- **Status:** Approved design
- **Date:** 2026-07-31
- **Scope:** First-party Vestrace web application and built-in console
- **Related:** H11 Universal Vertical Slice and Product Surface, ADR-0004 AG-UI Interaction Boundary, H11A AG-UI Interaction Gateway

## Purpose

Vestrace needs a complete web product, not only an interoperability protocol. AG-UI provides the interaction transport and event vocabulary for agent-facing applications, but it does not define the product information architecture, navigation, task workspace, operator controls, responsive behavior, accessibility, visual language or administration experience.

The first-party Vestrace Web UI is therefore a task-oriented application built around durable Runs, Human Requests, Artifacts, plans and verified outcomes. It must feel like a professional execution workspace rather than a chat application with extra panels.

## Product principles

1. **Task-first, not chat-first.** The primary object is a durable task and its outcome. Chat is one interaction channel inside the task workspace.
2. **Simple by default.** The default view shows goal, status, plan, required user actions, current work, results and Artifacts.
3. **Advanced locally, not globally.** Technical detail is revealed where needed through inline expansion or a contextual right-side inspector.
4. **Authority remains on the server.** UI state, caches, AG-UI state and client reducers are projections only.
5. **Risk controls are explicit.** Approvals, destructive actions and expensive operations show scope, consequences, expiry and expected version before submission.
6. **Progress is understandable without hidden reasoning.** The UI uses safe activity summaries, plans, Tool projections, verification findings and evidence.
7. **One product, adaptive surfaces.** Desktop and tablet provide the full workspace; mobile focuses on review, continuation, Human Requests, results and safe actions.
8. **Dense when useful, calm by default.** Comfortable density is standard; Compact is available for tables, events, audit and technical work.

## Chosen product approach

Three approaches were considered:

- **Technical operator console only:** efficient for developers, but too dense for ordinary task use.
- **Separate user, operator and administrator products:** clear roles, but excessive duplication and navigation fragmentation for v0.2.
- **Unified product with Simple and locally revealed Advanced detail:** preserves one mental model and one navigation system while allowing deep inspection.

Vestrace adopts the third approach.

There is no global Simple/Advanced mode switch. Simple is the default presentation. Each card may expose a bounded inline detail section, and full technical context may open in the contextual inspector.

## Information architecture

```text
Workspace
├── Home
├── Runs
└── Artifacts

Automation
├── Agents
├── Workflows
└── Triggers

System
├── Connections
├── Models
├── Evaluations
├── Audit
└── Settings
```

The primary sidebar is persistent on desktop, collapsible on tablet and replaced by bottom navigation plus a menu sheet on mobile.

### Home

Home is a combined personal and operational overview:

- Smart Composer for a new task;
- Continue Work for recent interrupted or active Runs;
- Active Runs grouped by current state;
- global Human Request and approval inbox;
- recent Artifacts;
- compact system, budget and Connection health summary.

Home is not an analytics dashboard. It prioritizes the next user action.

### Runs

The Runs area provides:

- saved filters and search;
- status, agent, workflow, owner, time and risk filters;
- Comfortable card view and Compact table view;
- active, waiting, completed, failed, cancelled and Unknown views;
- direct navigation to the Task Workspace.

### Artifacts

The Artifacts area provides safe metadata, version lineage, provenance, preview availability, export status and Run relationships. It never embeds unsafe active content directly.

### Automation

Agents, Workflows and Triggers are ordinary H11 administration surfaces. They use `/v1` commands and queries, not AG-UI state.

### System

Connections, Models, Evaluations, Audit and Settings are operator and administrator surfaces. Capability checks determine visibility and actions. Sensitive configuration remains under `/admin/v1` where defined by H11.

## Smart Composer

The Smart Composer preserves the speed of a chat input while exposing execution controls when necessary.

Always visible:

- task description;
- attachment control;
- submit action.

Expandable controls:

- Agent or package;
- Workflow;
- sources and Connections;
- autonomy level;
- budget and time limits;
- output type;
- scheduling or Trigger options;
- advanced execution constraints.

Selected controls appear as compact chips before launch. Risky, expensive or effectful settings require explicit confirmation. The composer submits an ordinary H11 application command with idempotency and does not create authority through client state.

## Task Workspace

The Task Workspace is the core product screen.

### Desktop layout

```text
┌────────────────┬────────────────────────────────────────┬─────────────────────┐
│ Primary nav    │ Task workspace                         │ Context inspector   │
│                │                                        │                     │
│ Workspace      │ Goal and status                        │ Run status          │
│ Automation     │ Current action                         │ Budget and usage    │
│ System         │ Adaptive plan                          │ Models              │
│                │ Human Requests                         │ Verification        │
│                │ Current work                           │ Tool activity       │
│                │ Intermediate results                   │ SubRuns/remotes     │
│                │ Final result                           │ Events/metadata     │
│                │ Artifacts                              │                     │
│                │ Compact chat                           │                     │
└────────────────┴────────────────────────────────────────┴─────────────────────┘
```

The inspector is resizable and dismissible. Closing it expands the central workspace. Opening an inspector does not navigate away from the current task context.

### Default content order

1. Goal and concise status.
2. Current state and nearest expected action.
3. Adaptive plan.
4. Human Requests and approvals.
5. Current work and safe activity.
6. Intermediate results.
7. Final result when available.
8. Artifacts.
9. Compact chat.

### Goal and status header

The header shows:

- task title and editable display label;
- authoritative Run status;
- owner and selected Agent/Workflow;
- elapsed time;
- high-level budget state;
- pause, resume and cancel actions where permitted;
- overflow menu for secondary actions.

Destructive or ambiguous actions are never placed next to ordinary navigation without separation and confirmation.

## Adaptive plan

The plan is displayed as a vertical list by default. Each step shows state, owner, dependencies, short progress summary and bounded evidence or Artifact links.

For complex or parallel Runs, the user may switch to Kanban:

```text
Planned | Running | Waiting | Done
```

The system may recommend Kanban when parallelism is high but never changes the view automatically. List/Kanban is a presentation preference and never modifies the authoritative ExecutionPlan.

Inline Advanced detail may show:

- step identifier and Run version;
- dependencies;
- internal SubRun or remote invocation references;
- Tool summary;
- budget allocation;
- verification state.

## Human Requests and approvals

One authoritative HumanRequest may be projected in three places:

- global inbox;
- Run workspace;
- navigation badge.

Resolving it updates every projection.

### Intensity

- clarification and ordinary review: neutral card;
- medium risk: accented card with deadline;
- high risk: pinned card with consequence summary;
- critical: persistent attention state that cannot auto-dismiss.

Color is reserved for semantic risk, urgency and status. Every state also has text and iconography.

### Approval presentation

An approval card must show:

- requested operation;
- resource and scope;
- expected effect;
- risk classification;
- budget or external-cost impact;
- exact challenge or fingerprint summary;
- expiry;
- requester and eligible responder context;
- approve and deny actions.

A generic yes/no interface is prohibited. Approval submission uses the exact H2/H7 command boundary and expected version.

## Current work and activity

The workspace uses safe progress summaries instead of hidden reasoning. Activity cards may represent:

- planning;
- research;
- retrieval;
- Tool execution;
- remote-agent delegation;
- Artifact processing;
- waiting;
- reconciliation;
- verification;
- export.

Each card has a concise default state. Inline expansion may expose timestamps, safe inputs/outputs, related steps, retries and warnings. Full event and metadata detail opens in the inspector.

## Compact chat

Chat is available inside every Run but does not dominate the layout. It supports:

- user messages;
- assistant messages;
- attachments;
- references to steps, Artifacts and Human Requests;
- continuation after interruption;
- safe generative UI cards.

The chat does not display hidden chain-of-thought, raw Tool secrets, unsafe Artifact content or arbitrary HTML.

AG-UI is the preferred interaction surface for Run chat, activities, interrupts, state projections and generative cards. Ordinary administration uses the TypeScript SDK over `/v1` and `/admin/v1`.

## Final result and Result Report

A completed Run shows a compact result card inside the workspace:

- concise conclusion;
- completion and verification status;
- key decisions or recommendations;
- primary Artifacts;
- warnings and unresolved issues;
- Open Full Report action.

The full Result Report contains:

- goal and outcome;
- completed plan;
- evidence and sources;
- Tools, internal SubRuns and remote agents;
- Artifacts and versions;
- budget, usage and duration;
- verification findings;
- approvals and material user decisions;
- unresolved warnings;
- exportable safe HTML/PDF projection.

The report excludes hidden reasoning and secret-bearing operational detail.

## Context inspector

The right-side inspector provides deep context without replacing the main task flow.

Inspector sections:

- Run summary and versions;
- plan and selected step detail;
- Human Request detail;
- Tool activity;
- internal SubRuns;
- A2A remote invocations;
- model selection and usage;
- budgets and reservations;
- Artifacts and provenance;
- verification;
- public events;
- safe structured metadata.

The inspector remembers its width and last selected section per user. It remains a projection and cannot act as an alternate editor of domain state.

## Generative UI

Generative UI is limited to server-published, schema-pinned component types.

Initial component classes:

- approval challenge;
- typed clarification form;
- option comparison;
- Artifact preview card;
- budget warning;
- verification finding;
- remote-agent status;
- operation Unknown/reconciliation state;
- structured result summary.

Model-produced arbitrary component code, scripts, HTML or styles are prohibited. Effectful actions inside a component map to ordinary application commands with authentication, idempotency, expected version and policy checks.

## Visual system

### Character

Vestrace uses a hybrid visual identity:

- enterprise clarity and restraint as the base;
- futuristic accents for active execution, graphs, activities, verification and remote-agent states;
- no decorative effect that interferes with reading or risk interpretation.

### Structured Depth

The chosen visual system is **Structured Depth**:

- neutral layered surfaces;
- thin borders;
- small elevation differences;
- restrained shadows;
- rare glow accents for active execution, live connections and high-priority states;
- minimal decorative gradients;
- status communicated by color, icon, label and shape.

### Themes

- Dark is the primary visual reference.
- Light is a complete equivalent theme.
- System theme is selected on first launch.
- User settings support Light, Dark and System.
- Risk and status semantics remain identical across themes.

### Density

- Comfortable is default.
- Compact is user-selectable.
- Events, Audit and large tables may default to a denser layout.
- Density changes presentation only.

### Typography and spacing

The interface uses a highly legible sans-serif family with a separate monospaced family for identifiers and structured data. Headings remain compact. Body copy and form labels meet readability requirements at common desktop and mobile sizes.

Spacing follows a consistent token scale. Dense technical tables may reduce row height but not hit targets or accessible labels.

## Responsive behavior

### Desktop

Full navigation, Task Workspace, adaptive plan, inspector, Kanban, event views and administration are available.

### Tablet

Primary navigation collapses. The inspector becomes a drawer or tabbed detail surface. Core task and administration functionality remains available.

### Mobile

Mobile prioritizes:

- Home;
- Runs;
- Human Requests and approvals;
- result summaries;
- Artifact metadata and safe previews;
- compact chat;
- pause, resume, cancel and other explicitly safe actions.

Deep event inspection, complex workflow editing, Model administration and large Audit views are desktop/tablet-first. Mobile never hides destructive actions behind swipe or long-press gestures.

Critical approvals on mobile require the full consequence summary and explicit confirmation.

## Navigation and state

- URLs are stable and deep-linkable for Home, Runs, Run workspace, Artifacts and administration pages.
- Selecting an inspector object may update a secondary route state without destroying the primary Run route.
- Back/forward navigation restores the prior workspace and inspector context.
- Unsaved form state is explicit and never confused with server state.
- Reconnect or projection lag is shown clearly.
- The client cache never overrides an authoritative server response.

## Error and recovery UX

The UI distinguishes:

- validation error;
- permission denied;
- approval required;
- version conflict;
- budget exceeded;
- transient transport failure;
- projection lag;
- operation Unknown;
- terminal Run failure;
- partial verified result.

Unknown is never rendered as Failed or automatically retried as a new operation. The user receives reconciliation status and safe available actions.

A disconnected AG-UI or SSE stream shows reconnect state without changing the Run status. Cursor recovery resumes from H7 durable events.

## Accessibility

The release target is WCAG 2.2 AA for core workflows.

Required behavior:

- complete keyboard navigation;
- visible focus;
- semantic headings and landmarks;
- accessible names and descriptions;
- sufficient contrast in both themes;
- no state communicated by color alone;
- screen-reader announcements for Run status and Human Requests;
- reduced-motion support;
- minimum touch targets on mobile/tablet;
- accessible table, Kanban and drawer patterns;
- focus containment and restoration for dialogs and inspector drawers.

## Security and privacy

- The web application consumes only public TypeScript SDK and AG-UI integration contracts.
- It has no direct database, repository or internal adapter access.
- Bearer credentials remain memory-only and are never placed in localStorage, sessionStorage or IndexedDB.
- LocalTrusted writes follow H11 Origin, Host, Sec-Fetch-Site and console nonce rules.
- Active HTML, SVG, scripts and unsafe remote content never enter the DOM.
- Artifact content uses H6 safe previews or controlled download.
- Text is escaped by default.
- CSP prohibits unsafe-eval and arbitrary inline script.
- Sensitive data is not placed in URLs, analytics, error messages or browser persistence.
- Viewer policy filters all projected Run, Tool, Artifact, budget, audit and remote-agent detail.

## Application boundary

```text
Interactive Task Workspace
→ AG-UI HTTP/SSE
→ H11A gateway
→ H7/H1/H2/H4/H6/H10 authority

Product and administration UI
→ TypeScript SDK
→ /v1 and /admin/v1
→ H11 application facades
```

The web application must not reproduce orchestration logic, Tool authorization, approval validation, Run transitions, Artifact classification or verification locally.

## Performance expectations

- Initial authenticated shell is usable before non-critical dashboards finish loading.
- Long Run histories, events, audit rows and Artifact lists use pagination or virtualization.
- Streaming reducers apply bounded batches to avoid UI starvation.
- Large Artifact content is never loaded automatically.
- Inspector panels load detail on demand.
- Repeated event delivery is deduplicated without rebuilding the entire workspace.

Exact budgets are defined in the implementation plan and release performance tests.

## Testing strategy

### Component and visual tests

- navigation and layout states;
- Simple default and local Advanced expansion;
- plan list and Kanban;
- Human Request intensity states;
- approval detail and confirmation;
- Result Report;
- inspector sections;
- dark/light themes;
- Comfortable/Compact density;
- desktop/tablet/mobile layouts.

### Contract tests

- TypeScript SDK and `/v1` commands;
- AG-UI event reduction;
- reconnect and deduplication;
- interrupt/resume;
- stale expected versions;
- projection lag;
- Unknown outcomes;
- safe Artifact previews;
- frontend-action policy boundaries.

### Security tests

- token and nonce non-persistence;
- XSS and active content rejection;
- Origin/nonce CSRF boundaries;
- permission-filtered inspector content;
- no hidden reasoning or forbidden AG-UI events;
- no direct internal API use.

### Accessibility tests

- automated accessibility checks;
- keyboard-only core workflows;
- screen-reader acceptance for task launch, Human Request, approval and result review;
- reduced motion;
- contrast and focus checks.

## Release acceptance scenario

```text
Home
→ create a task through Smart Composer
→ open Task-first Run workspace
→ inspect adaptive plan
→ receive a medium-risk clarification
→ answer from the global inbox
→ reveal inline Advanced Tool detail
→ open full context in the inspector
→ switch plan from list to Kanban
→ review safe Artifact preview
→ receive a high-risk approval with full consequences
→ approve through exact command boundary
→ disconnect and reconnect from H7 cursor
→ complete after H10 verification
→ read compact result
→ open and export full Result Report
→ repeat essential review and approval steps on mobile
```

The scenario must prove that no UI projection, AG-UI state, frontend Tool declaration or cached value can mutate authoritative Run, Tool, approval, budget, Artifact or verification state directly.

## Scope exclusions

- arbitrary user-authored dashboard layouts;
- unrestricted plugin UI code;
- model-generated executable UI;
- global Advanced mode that replaces the normal product;
- mobile parity for deep administration and Audit analysis;
- raw hidden reasoning display;
- unsafe inline Artifact rendering;
- direct database administration;
- replacing `/v1` administration with AG-UI;
- a separate operator product in v0.2.

## Consequences

### Benefits

- Vestrace presents a coherent task execution product rather than a protocol demo.
- Ordinary users see a calm, understandable workflow.
- Technical users can inspect deep context without leaving the Run.
- AG-UI and H11 APIs have distinct responsibilities.
- Approvals, verification and Artifacts remain prominent and safe.
- The product scales from desktop execution to mobile review and continuation.

### Costs

- The console is larger than the minimal H11 console originally envisioned.
- Responsive and accessibility acceptance requires substantial design-system and test investment.
- Both AG-UI and ordinary H11 API client paths must be maintained.
- Local Advanced expansion and inspector state require careful projection and routing design.

## Required next document

After user review and approval of this specification, create a dedicated implementation plan for the first-party Web UI. The plan must integrate with H11 and H11A but remain a separate, testable product phase, tentatively named **H11B Vestrace Web Application and Design System**.
