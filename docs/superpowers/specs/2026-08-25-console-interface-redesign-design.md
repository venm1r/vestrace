# Vestrace Console Interface Redesign

## Goal

Rework the Vestrace console into a dense, task-first operations workspace whose spatial hierarchy follows the supplied desktop reference while its visual language follows the official Vestrace brand assets.

## Authorities

- Layout authority: `C:\Users\venmi\OneDrive\Рабочий стол\vr\Сгенерированное изображение 1.png`.
- Brand authority: `C:\Users\venmi\OneDrive\Рабочий стол\vr\Тёмный гайдлайн Vestrace с неоновыми акцентами.png` and the supplied Vestrace SVG marks.
- Runtime truth authority: the existing console clients in `apps/console/src/sdk/` and the data they actually return.
- Current repository behavior and accessibility remain authoritative where the visual references are silent.

## Visual direction

- Preserve the reference's compact desktop composition: primary navigation, runs rail, central work area, details rail, and workspace metrics rail.
- Use the official navy surfaces with blue (`#2563eb`) and cyan (`#22d3ee`) accents. Do not introduce a separate purple brand accent from the generated layout example.
- Use Space Grotesk for interface typography, a monospace face for identifiers, and a compact 12/14/16/18 px working scale.
- Prefer structured tonal depth, one-pixel low-contrast borders, restrained 6-8 px radii, and almost no shadows.
- Use the supplied official mark/wordmark rather than recreating the generic symbol shown in the generated layout example.

## Information architecture

The primary navigation remains:

- Workspace: Home, Runs, Artifacts.
- Automation: Agents, Workflows, Triggers.
- System: Connections, Models, Evaluations, Audit, Settings.

`/runs` becomes the operations workspace:

- A searchable runs rail uses only records returned by `listRuns`.
- A URL-backed selected run supports both `/runs` and `/runs/:runId`.
- The central workspace exposes `Workspace`, `Agent`, and `Record` views.
- `Workspace` and `Record` show fields present on `RunItem` only.
- `Agent` states that sending a message starts a separate AG-UI run and does not mutate or continue the selected run. Once AG-UI returns that new run id, its event stream is filtered to that id and reconnects when another AG-UI run is started.
- The details rail shows the selected run's id, status, version, and timestamps.
- The metrics rail is explicitly labelled as workspace-level and shows only `MetricsSummary` fields.

Home remains a task-oriented launch surface. It may show real workspace metrics and the existing create-run action, but it must not become an invented analytics dashboard.

## Responsive behavior

- At wide desktop sizes, all five spatial zones are visible.
- At medium desktop/tablet sizes, secondary details and metrics share a stacked support rail.
- Below 1024 px, the primary navigation is a drawer and the runs rail moves above the workspace.
- Below 720 px, content becomes a single column, controls wrap, and no page-level horizontal overflow is allowed.
- Navigation, tab selection, forms, and dismissible overlays remain keyboard accessible; reduced-motion preferences are respected.

## Truthfulness and non-goals

- Do not fabricate plans, activity, cost, token usage, approvals, run-artifact relationships, model attribution, or progress percentages.
- Do not add pause, resume, cancel, share, export, or other actions that are not already exposed through the console client in this slice.
- Do not change Rust APIs, persistence, authorization, identity handling, deployment, or generated console output.
- Do not modify or rebuild `apps/console/dist` and do not modify `apps/console/nginx.conf.template`.
- Do not commit, push, or deploy.

## Acceptance criteria

1. The shell and `/runs` visibly follow the reference's density and pane hierarchy while using the official blue/cyan Vestrace palette and mark.
2. `/runs/:runId` selects an existing returned run; an unknown id falls back safely without fabricating a record.
3. Run filtering matches title or id case-insensitively; all 14 declared `RunStatus` values are grouped deterministically; an unrecognised runtime status receives a neutral presentation and appears only in the all/other group.
4. The Agent view never claims to continue the selected run. It starts a separate AG-UI run, filters events by the id returned for that run, and still reports unavailable AG-UI behavior as unavailable.
5. Home and the runs workspace render only real API values or explicit empty/error/loading states.
6. The interface is usable at wide desktop, tablet, and mobile widths without page-level horizontal overflow.
7. Focus states, semantic controls, labels, landmarks, reduced motion, and contrast-sensitive states remain present.
8. Focused behavior tests, TypeScript typecheck, and a disposable Vite build pass.
9. The pre-existing `dist` deletions/modification and untracked nginx template remain byte-for-byte/status-for-status untouched.
