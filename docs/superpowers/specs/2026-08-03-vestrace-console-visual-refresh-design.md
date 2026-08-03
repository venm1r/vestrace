# Vestrace Console Visual Refresh Design

- **Status:** Proposed design
- **Date:** 2026-08-03
- **Scope:** Restyle the existing `apps/console` prototype using the Stitch-generated "Neon" mockups and the `execution_kernel` design tokens.
- **Related:** `docs/superpowers/specs/2026-07-31-vestrace-web-ui-design.md` (normative product spec), `docs/superpowers/plans/2026-07-31-vestrace-h11b-web-application-design-system.md` (H11B implementation plan)

## Purpose

`apps/console` currently contains a small, unofficial React/Vite prototype (11 stub pages, inline-style components, a rough approximation of the brand palette) built before H11B execution began. A set of high-fidelity Stitch mockups now exists outside this repo at `E:\Soft\stitch_vestrace_neon_design_system\` — one HTML/PNG pair per major surface, plus an `execution_kernel\DESIGN.md` token manifest at that same location. Neither is checked into this repository; this design references them by that external path.

This document scopes a **visual refresh of the existing prototype**: adopt the Stitch visual language (Tailwind build, Space Grotesk/Geist/JetBrains Mono, Structured Depth surfaces, Material Symbols) and convert the mockups into React components for the prototype's 11 pages. It explicitly does **not** end H11B's documentation-only gate, does not wire the H11 TypeScript SDK or AG-UI, and does not introduce the H11B locked file structure. It is a throwaway-safe styling pass on the existing prototype that a future H11B execution can draw visual reference from, not a replacement for it.

## Non-negotiable constraint: reconcile with the approved product spec

The Stitch mockups were generated independently of `2026-07-31-vestrace-web-ui-design.md` and disagree with it in one material way: the `operational_control_dashboard` mockup is a metrics/heatmap/live-feed analytics dashboard, while the approved spec is explicit that **"Home is not an analytics dashboard. It prioritizes the next user action."** Where a mockup's content conflicts with the approved information architecture, the approved spec wins; the mockup is used for tokens, chrome and component styling only, not page content or navigation structure.

Concretely:

- The primary navigation stays exactly `Workspace{Home, Runs, Artifacts} / Automation{Agents, Workflows, Triggers} / System{Connections, Models, Evaluations, Audit, Settings}` — no new top-level nav item is added.
- **Home** keeps its existing task-first composition (Smart Composer, active run focus, Human Request inbox, compact chat), restyled with the new tokens/primitives — it is not replaced by the Stitch Cluster Overview dashboard.
- A **compact** system-health strip (Live Runs / Active Agents / Resource Health / Avg Latency metric cards only, restyled from the mockup) is added to Home as its "compact system, budget and Connection health summary," per the approved spec's own Home bullet list.
- The mockup's richer System Live Feed, Cluster Heatmap and High-Risk Runs table have no home in the approved information architecture and are **out of scope** for this pass — porting them would mean inventing a nav destination the approved spec doesn't define. They're left as a future candidate once H11B's real AG-UI/audit data model exists to back them honestly.
- `user_profile_access` has its own inconsistent, unrelated nav taxonomy in its mockup (Dashboard/Workspaces/Executions/Kernel Logs/Infrastructure/Security instead of the standard 11 items) — it's clearly an orphan template. Its content (identity, MFA, API keys, active sessions) becomes a new `/profile` route reached from the top-bar avatar, not a primary nav item, so it doesn't expand the approved IA.
- `Triggers` has no corresponding mockup; it is restyled with the same shared primitives/patterns used on the other System-section pages (no bespoke layout to port).

## Design system foundation

- Add Tailwind to the Vite build (not the CDN script the mockups use). Generate `tailwind.config` colors/typography/spacing/radius from `execution_kernel/DESIGN.md`'s front matter — that file, not any individual mockup's inline config, is the single source of truth, since the mockups disagree with each other on token names (compare `operational_control_dashboard`'s MD3-style `surface-container-lowest`/`on-primary` naming against `kernel_settings`'s simpler `brand-navy`/`brand-blue` naming for the same colors).
- Add Space Grotesk, Geist, JetBrains Mono and Material Symbols Outlined, self-hosted via `@font-face`/link tags, replacing the current Inter-only `theme.css`.
- Rebuild the shell as one canonical `SideNavBar` + `TopNavBar`, based on the `operational_control_dashboard` chrome (cross-checked against `kernel_settings`, which uses the same 11 nav items), replacing today's inline-style `PrimaryNavigation`/`AppLayout`.
- Replace the `Button`/`Surface` primitives with Tailwind-class versions matching the mockups, and add the shared pieces the mockups repeat across pages: `StatCard`, `StatusBadge`, `DataTable`, `ToggleSwitch`.
- Keep `TaskWorkbench`, `ApprovalChallenge` and `CompactChat` (no Stitch counterpart) — restyle them in place on Home/Runs with the new primitives.

## Routing

Add `react-router-dom` with one route per nav item (`/`, `/runs`, `/artifacts`, `/agents`, `/workflows`, `/triggers`, `/connections`, `/models`, `/evaluations`, `/audit`, `/settings`) plus `/profile`, mounted inside the shell. This is a prototype-scoped routing table, not the H11B `AppRoute` discriminated-union contract (no `run`/`result-report`/inspector route state) — those require the H11 API layer this pass doesn't touch.

## Page mapping

| Stitch mockup | Route | File | Content source |
| --- | --- | --- | --- |
| *(kept as-is, restyled)* | `/` | `HomePage.tsx` | existing task-first composition + compact system-health strip |
| execution_history | `/runs` | `RunsPage.tsx` | mockup layout/table |
| artifacts_repository | `/artifacts` | `ArtifactsPage.tsx` | mockup layout |
| agent_fleet_dashboard | `/agents` | `AgentsPage.tsx` | mockup layout |
| workflow_management | `/workflows` | `WorkflowsPage.tsx` | mockup layout |
| *(none)* | `/triggers` | `TriggersPage.tsx` | shared primitives, no bespoke mockup |
| external_connections | `/connections` | `ConnectionsPage.tsx` | mockup layout |
| models_ai_providers_registry | `/models` | `ModelsPage.tsx` | mockup layout |
| evaluations_registry | `/evaluations` | `EvaluationsPage.tsx` | mockup layout |
| audit_log_trace_registry | `/audit` | `AuditPage.tsx` | mockup layout |
| kernel_settings | `/settings` | `SettingsPage.tsx` | mockup layout |
| user_profile_access | `/profile` (new, avatar-triggered) | `ProfilePage.tsx` | mockup layout, minus its orphan nav |

## Data

Every page keeps static/mocked data in the component, matching the current prototype pattern. No calls to the Rust HTTP API and no H11 SDK usage — the README already states product workflows and the web UI aren't implemented in this slice, and wiring real data is H11B's job, not this pass's.

## Verification

- `npm run build` in `apps/console` for a TypeScript/Tailwind compile check.
- `npm run dev`, then drive each route with Playwright to screenshot it against the mockup's `screen.png` for a visual sanity check — a manual side-by-side, not pixel-diff CI.
- No new automated test suite is added; this pass doesn't touch the H11B `tests/`, `playwright/`, security, or accessibility scaffolding, since those are scoped to real H11B execution against real data.

## Scope exclusions

- H11B's locked file structure, TypeScript SDK/AG-UI wiring, command-descriptor boundary, generative UI registry, contextual inspector, adaptive plan (list/Kanban), Result Report, and all security/accessibility/responsive acceptance work — all out of scope; this pass produces visual reference, not H11B implementation.
- The Stitch Cluster Overview dashboard's Live Feed, Heatmap and High-Risk Runs table content — no home in the approved IA (see above).
- Mobile/tablet responsive behavior beyond what naturally falls out of the Tailwind layout — no dedicated breakpoint work.
- Any change to `docs/superpowers/specs/2026-07-31-vestrace-web-ui-design.md` or the H11B plan itself.

## Consequences

**Benefits:** the prototype gets a coherent, on-brand look derived from real design tokens instead of ad hoc inline styles, without committing to or contradicting the much larger H11B backend-integration effort. It gives a concrete visual reference for whoever executes H11B later.

**Costs:** work done here (Tailwind setup, shell, primitives) will likely be partially redone under H11B's locked file structure and design-token contract (Task 2), since that structure's token files (`color.css`, `typography.css`, etc.) and primitive set (`IconButton`, `Stack`, `Inline`, `Text`, `Badge`, `StatusIndicator`, `Progress`, `Skeleton`, `Tooltip`, `Dialog`, `Drawer`, `Menu`, `Tabs`, `VirtualList`) are broader than what this prototype needs. That duplication is accepted as the cost of having a visual reference now rather than waiting for full H11B execution.
